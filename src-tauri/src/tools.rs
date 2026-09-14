use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncReadExt;

use crate::agent::SessionStop;

/// Hard ceiling on how much text a single read_file or run_shell result
/// carries back into the conversation. Decompiled sources and build logs
/// can be enormous — left uncapped, one tool result can single-handedly
/// blow the whole context budget (compounding the context-compaction
/// problem) or just be too large for the backend to accept at all. Kept
/// generous enough that this virtually never fires on normal-sized files/
/// output, but bounded all the same.
const MAX_TOOL_OUTPUT_CHARS: usize = 40_000;

/// Default and maximum timeout for run_shell. A build/decompile/test
/// command can legitimately run for minutes; an unbounded wait is what
/// actually breaks a long session (see run_shell's doc comment below), so
/// the default is generous but finite, and the model can ask for more, up
/// to a hard ceiling, for a command it expects to be slow.
pub(crate) const DEFAULT_SHELL_TIMEOUT_SECS: u64 = 300;
const MAX_SHELL_TIMEOUT_SECS: u64 = 1800;

/// Directory names skipped entirely by search_code/find_files — build
/// output, dependency trees, and VCS internals that are almost never what
/// a "search the codebase" call is actually looking for, and that would
/// otherwise dominate the scan budget on any real project.
const IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".nuxt",
    "__pycache__",
    ".venv",
    "venv",
    ".cache",
    ".turbo",
    "vendor",
];

/// Hard ceiling on how many files a single search_code/find_files call will
/// open, independent of max_results — so a query that matches nothing
/// still terminates quickly on a huge workspace instead of walking every
/// file in it.
const MAX_SCAN_FILES: usize = 8000;

/// The tool schema sent to the model, OpenAI function-calling format.
pub fn tool_definitions() -> Value {
    json!([
        {
            "type": "function",
            "function": {
                "name": "read_file",
                "description": "Read the contents of a text file, relative to the workspace root. Large files are truncated (with a note telling you the total line count) — pass start_line/num_lines to page through the rest instead of re-reading from the top.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "start_line": { "type": "integer", "description": "1-based line to start from. Defaults to 1." },
                        "num_lines": { "type": "integer", "description": "Max lines to return from start_line. Defaults to enough to fill the output cap." }
                    },
                    "required": ["path"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "write_file",
                "description": "Create a new file, or fully overwrite an existing one, relative to the workspace root. Creates parent directories if needed. For changes to an existing file, prefer edit_file (or apply_patch for a multi-hunk diff) instead — rewriting the whole file is wasteful and risks silently dropping unrelated content you didn't mean to touch.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "content": { "type": "string" }
                    },
                    "required": ["path", "content"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "edit_file",
                "description": "Patch an existing file by exact search-and-replace, without rewriting the parts you're not changing. Give one or more edits; each old_string must match the file's current content exactly (including whitespace/indentation) and, by default, exactly once — include enough surrounding context in old_string to make it unique if the text repeats. Edits apply in order, as if to the same in-memory copy. This is the preferred way to change part of an existing file — reach for it instead of write_file whenever the file already exists.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string" },
                        "edits": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "old_string": {
                                        "type": "string",
                                        "description": "Exact text to find, including whitespace. Must be non-empty."
                                    },
                                    "new_string": {
                                        "type": "string",
                                        "description": "Text to replace it with. Empty string deletes the matched text."
                                    },
                                    "replace_all": {
                                        "type": "boolean",
                                        "description": "Replace every occurrence instead of requiring exactly one match. Defaults to false."
                                    }
                                },
                                "required": ["old_string", "new_string"]
                            }
                        }
                    },
                    "required": ["path", "edits"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "apply_patch",
                "description": "Apply a unified diff (the format `diff -u` or `git diff` produce: '--- a/path', '+++ b/path', '@@ ... @@' hunks) to one or more files in the workspace in one call. Each hunk is matched against the target file by its context/removed lines — exact line numbers in the '@@' header don't need to be perfectly accurate, but the context and removed lines must match the file's current content exactly. A hunk with only added lines against a '--- /dev/null' header creates a new file; a '+++ /dev/null' header deletes the file. Prefer this over write_file when you already have (or are generating) a diff, especially across several files or several separate hunks in one file; prefer edit_file for a single, simple search-and-replace.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "patch": {
                            "type": "string",
                            "description": "The full unified diff text, one or more files, standard '--- '/'+++ '/'@@' format."
                        }
                    },
                    "required": ["patch"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "search_code",
                "description": "Search file contents across the workspace (or a subdirectory of it) for a literal substring, returning matching lines as 'path:line: text'. Skips .git, node_modules, target, dist, build, and similar directories automatically. Prefer this over run_shell + grep/find for exploring the codebase — it's sandboxed to the workspace and its output is easier to work with.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Literal text to search for (not a regex)." },
                        "path": { "type": "string", "description": "Subdirectory to scope the search to, relative to the workspace root. Defaults to the whole workspace." },
                        "case_sensitive": { "type": "boolean", "description": "Defaults to false." },
                        "file_glob": { "type": "string", "description": "Only search files whose name matches this glob, e.g. '*.rs' or '*.ts'. Supports '*' and '?' wildcards." },
                        "max_results": { "type": "integer", "description": "Cap on matching lines returned. Defaults to 200, capped at 2000." }
                    },
                    "required": ["query"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "find_files",
                "description": "Find files in the workspace (or a subdirectory of it) by name, using a glob pattern like '*.test.ts' or 'Cargo.*'. Matches against both the filename and the workspace-relative path. Skips .git, node_modules, target, dist, build, and similar directories automatically. Use this to locate files before reading or editing them, rather than list_dir-ing your way down a tree by hand.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "pattern": { "type": "string", "description": "Glob pattern, e.g. '*.rs', 'test_*.py', 'src/**/index.ts' (matched loosely — '*' matches across path separators too)." },
                        "path": { "type": "string", "description": "Subdirectory to scope the search to, relative to the workspace root. Defaults to the whole workspace." },
                        "max_results": { "type": "integer", "description": "Cap on files returned. Defaults to 200, capped at 2000." }
                    },
                    "required": ["pattern"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "list_dir",
                "description": "List files and folders at a path relative to the workspace root.",
                "parameters": {
                    "type": "object",
                    "properties": { "path": { "type": "string" } },
                    "required": ["path"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "run_shell",
                "description": "Run a shell command inside the workspace root and return its stdout/stderr. On macOS/Linux this runs via 'sh -c'; on Windows it runs via PowerShell (not cmd.exe), which does have 'ls', 'cat', 'cp', 'mv', 'rm', 'pwd', and 'echo' as built-in aliases, but not 'grep' or Unix-style 'find' — use search_code/find_files instead of piping through grep/find, and prefer read_file/edit_file/apply_patch over cat/redirection for reading or changing files, since those work identically on every platform. Quote arguments the way the target shell expects (e.g. a git commit message must be one quoted argument to -m — an unquoted multi-word message gets split into extra pathspec arguments and fails). The command is killed if it doesn't finish within the timeout (default 5 minutes) — pass a larger timeout_seconds for something you expect to be slow (a full build, a test suite, a decompile pass), up to 30 minutes; very large stdout/stderr is truncated (head and tail kept) rather than returned in full.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": { "type": "string" },
                        "timeout_seconds": { "type": "integer", "description": "Max time to let the command run before it's killed. Defaults to 300, capped at 1800." }
                    },
                    "required": ["command"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "web_search",
                "description": "Perform a web search query via OmniRoute (supports Tavily, Brave, Exa, Serper, etc.) to find current information, documentation, news, or articles on the internet.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "The search query string." },
                        "provider": { "type": "string", "description": "Optional explicit search provider (e.g. 'tavily', 'brave', 'exa', 'serper'). If omitted, OmniRoute uses quota-aware fallback across configured search providers." },
                        "limit": { "type": "integer", "description": "Optional maximum number of search results to return." }
                    },
                    "required": ["query"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "web_fetch",
                "description": "Fetch and extract text/markdown content from a web page URL via OmniRoute (supports Firecrawl, Jina Reader, Tavily Extract, TinyFish Fetch, etc.).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "url": { "type": "string", "description": "The complete URL of the web page to fetch and scrape." },
                        "provider": { "type": "string", "description": "Optional explicit fetch provider (e.g. 'firecrawl', 'jina-reader', 'tavily-search', 'tinyfish'). If omitted, OmniRoute uses quota-aware fallback." }
                    },
                    "required": ["url"]
                }
            }
        }
    ])
}

/// Tools that mutate state and should be gated behind approval when
/// planning mode is on. Read-only tools always execute immediately.
pub fn is_mutating(tool_name: &str) -> bool {
    matches!(tool_name, "write_file" | "edit_file" | "apply_patch" | "run_shell")
}

/// Resolves a relative path against the workspace root and does a
/// best-effort containment check so the agent can't be steered outside its
/// workspace via "../../" in a tool argument. Not a hard security boundary
/// (the model/user can still ask it to run_shell arbitrary commands), just
/// a guard against the common accidental case.
fn resolve_path(workspace: &str, rel: &str) -> Result<PathBuf, String> {
    let root = Path::new(workspace);
    let joined = root.join(rel);
    let canonical_root = root.canonicalize().map_err(|e| e.to_string())?;

    let check_base = if joined.exists() {
        joined.clone()
    } else {
        joined.parent().unwrap_or(root).to_path_buf()
    };

    if let Ok(canonical_check) = check_base.canonicalize() {
        if !canonical_check.starts_with(&canonical_root) {
            return Err("Path escapes the workspace root".into());
        }
    }

    Ok(joined)
}

/// Walks `dir` recursively, calling `visit` for every file (not directory)
/// found, skipping IGNORED_DIRS along the way. `visit` returns false to
/// stop the walk early (e.g. once a result cap is hit) — propagated all
/// the way back up through the recursion.
fn walk_files<F: FnMut(&Path) -> bool>(dir: &Path, visit: &mut F) -> bool {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return true,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            if IGNORED_DIRS.contains(&name.as_str()) {
                continue;
            }
            if !walk_files(&path, visit) {
                return false;
            }
        } else if !visit(&path) {
            return false;
        }
    }
    true
}

/// Simple shell-style glob matcher supporting `*` (any run of characters,
/// including none) and `?` (exactly one character). Case-insensitive.
/// Deliberately not a regex engine — file-name globs are the only thing
/// this needs to handle.
fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern = pattern.to_lowercase();
    let text = text.to_lowercase();
    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = text.chars().collect();

    let (mut p, mut t) = (0usize, 0usize);
    let mut star_p: Option<usize> = None;
    let mut star_t = 0usize;

    while t < txt.len() {
        if p < pat.len() && (pat[p] == '?' || pat[p] == txt[t]) {
            p += 1;
            t += 1;
        } else if p < pat.len() && pat[p] == '*' {
            star_p = Some(p);
            star_t = t;
            p += 1;
        } else if let Some(sp) = star_p {
            p = sp + 1;
            star_t += 1;
            t = star_t;
        } else {
            return false;
        }
    }
    while p < pat.len() && pat[p] == '*' {
        p += 1;
    }
    p == pat.len()
}

/// Caps `s` to at most `max_chars`, keeping the head and appending a note
/// with the omitted byte count — used for run_shell output, where the
/// most recent lines (often where a build error actually shows up) are at
/// least as important as the first ones, so this keeps head *and* tail
/// rather than just truncating from the end.
fn cap_head_tail(s: &str, max_chars: usize) -> String {
    let total = s.chars().count();
    if total <= max_chars {
        return s.to_string();
    }
    let half = max_chars / 2;
    let head: String = s.chars().take(half).collect();
    let tail: String = s.chars().skip(total - half).collect();
    format!(
        "{}\n\n... [{} characters omitted — output was {} characters total] ...\n\n{}",
        head,
        total - max_chars,
        total,
        tail
    )
}

fn relative_display_path(workspace: &str, path: &Path) -> String {
    path.strip_prefix(workspace)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

// ---- apply_patch: minimal unified-diff support ----

/// One file's worth of hunks parsed out of a unified diff: a target path
/// plus, for each hunk, the exact block of text to find (context + removed
/// lines) and the block to replace it with (context + added lines).
struct FileHunks {
    path: String,
    is_create: bool,
    is_delete: bool,
    hunks: Vec<(String, String)>,
}

/// Normalizes one side of a `--- `/`+++ ` diff header: strips a trailing
/// tab-separated timestamp, and the conventional `a/`/`b/` prefix git
/// diffs use. Returns None for `/dev/null`, which marks file creation or
/// deletion depending on which side it's on.
fn norm_diff_path(raw: &str) -> Option<String> {
    let raw = raw.split('\t').next().unwrap_or(raw).trim();
    if raw == "/dev/null" {
        return None;
    }
    let stripped = raw.strip_prefix("a/").or_else(|| raw.strip_prefix("b/")).unwrap_or(raw);
    Some(stripped.to_string())
}

fn parse_unified_diff(patch: &str) -> Result<Vec<FileHunks>, String> {
    let lines: Vec<&str> = patch.lines().collect();
    let mut files = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        if !lines[i].starts_with("--- ") {
            i += 1;
            continue;
        }
        let old_raw = &lines[i][4..];
        i += 1;
        if i >= lines.len() || !lines[i].starts_with("+++ ") {
            return Err(format!(
                "malformed patch: expected a '+++ ' line right after '--- {}'",
                old_raw.trim()
            ));
        }
        let new_raw = &lines[i][4..];
        i += 1;

        let old_path = norm_diff_path(old_raw);
        let new_path = norm_diff_path(new_raw);
        let is_create = old_path.is_none();
        let is_delete = new_path.is_none();
        let path = new_path
            .or(old_path)
            .ok_or("malformed patch: both '---' and '+++' sides are /dev/null")?;

        let mut hunks = Vec::new();
        while i < lines.len() && lines[i].starts_with("@@") {
            i += 1; // the "@@ -l,c +l,c @@" line itself carries no text we trust
            let mut search = String::new();
            let mut replace = String::new();
            while i < lines.len() && !lines[i].starts_with("@@") && !lines[i].starts_with("--- ") {
                let l = lines[i];
                if l.starts_with("\\ No newline") {
                    i += 1;
                    continue;
                }
                if let Some(rest) = l.strip_prefix(' ') {
                    search.push_str(rest);
                    search.push('\n');
                    replace.push_str(rest);
                    replace.push('\n');
                } else if let Some(rest) = l.strip_prefix('-') {
                    search.push_str(rest);
                    search.push('\n');
                } else if let Some(rest) = l.strip_prefix('+') {
                    replace.push_str(rest);
                    replace.push('\n');
                } else if l.is_empty() {
                    // A genuinely blank context line often loses its
                    // leading space in transit — treat it as context.
                    search.push('\n');
                    replace.push('\n');
                } else {
                    // Tolerate anything else (e.g. a missing leading
                    // space) as context rather than failing the parse.
                    search.push_str(l);
                    search.push('\n');
                    replace.push_str(l);
                    replace.push('\n');
                }
                i += 1;
            }
            hunks.push((search, replace));
        }

        if hunks.is_empty() {
            return Err(format!("malformed patch: no '@@' hunks found for {}", path));
        }
        files.push(FileHunks { path, is_create, is_delete, hunks });
    }

    if files.is_empty() {
        return Err("no '--- '/'+++ ' file headers found — this doesn't look like a unified diff".into());
    }
    Ok(files)
}

fn apply_file_hunks(workspace: &str, fh: &FileHunks) -> Result<String, String> {
    let full = resolve_path(workspace, &fh.path)?;

    if fh.is_delete {
        fs::remove_file(&full).map_err(|e| format!("deleting {}: {}", fh.path, e))?;
        return Ok(format!("deleted {}", fh.path));
    }

    let mut content = if fh.is_create {
        String::new()
    } else {
        fs::read_to_string(&full).map_err(|e| format!("reading {}: {}", fh.path, e))?
    };

    for (idx, (search, replace)) in fh.hunks.iter().enumerate() {
        let trimmed_search = search.trim_end_matches('\n');
        let trimmed_replace = replace.trim_end_matches('\n');

        if trimmed_search.is_empty() {
            if content.is_empty() {
                content = trimmed_replace.to_string();
                continue;
            }
            return Err(format!(
                "hunk {} in {} has no context or removed lines to anchor it against the existing content",
                idx + 1,
                fh.path
            ));
        }

        match content.find(trimmed_search) {
            Some(pos) => {
                let end = pos + trimmed_search.len();
                content.replace_range(pos..end, trimmed_replace);
            }
            None => {
                return Err(format!(
                    "hunk {} in {} didn't match the file's current content — it may have changed since you read it, or context/removed lines don't align exactly (whitespace matters). Re-read the file and try again.",
                    idx + 1,
                    fh.path
                ));
            }
        }
    }

    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&full, &content).map_err(|e| e.to_string())?;
    Ok(format!(
        "patched {} ({} hunk{})",
        fh.path,
        fh.hunks.len(),
        if fh.hunks.len() == 1 { "" } else { "s" }
    ))
}

pub fn execute(workspace: &str, name: &str, args: &Value) -> Result<String, String> {
    match name {
        "read_file" => {
            let path = args
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or("missing path")?;
            let full = resolve_path(workspace, path)?;
            let content = fs::read_to_string(&full).map_err(|e| e.to_string())?;

            let start_line = args.get("start_line").and_then(|v| v.as_u64()).unwrap_or(1).max(1) as usize;
            let explicit_range = args.get("start_line").is_some() || args.get("num_lines").is_some();

            if explicit_range {
                let total_lines = content.lines().count();
                let num_lines = args
                    .get("num_lines")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as usize)
                    .unwrap_or(usize::MAX);
                let selected: String = content
                    .lines()
                    .skip(start_line - 1)
                    .take(num_lines)
                    .collect::<Vec<_>>()
                    .join("\n");
                let end_line = (start_line + selected.lines().count()).saturating_sub(1);
                let body = cap_head_tail(&selected, MAX_TOOL_OUTPUT_CHARS);
                Ok(format!("[lines {}-{} of {} total]\n{}", start_line, end_line, total_lines, body))
            } else if content.chars().count() > MAX_TOOL_OUTPUT_CHARS {
                let total_lines = content.lines().count();
                let shown: String = content.chars().take(MAX_TOOL_OUTPUT_CHARS).collect();
                let shown_lines = shown.lines().count();
                Ok(format!(
                    "{}\n\n... [truncated after {} of {} lines ({} characters total) — pass start_line: {} to continue reading]",
                    shown, shown_lines, total_lines, content.chars().count(), shown_lines + 1
                ))
            } else {
                Ok(content)
            }
        }
        "write_file" => {
            let path = args
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or("missing path")?;
            let content = args
                .get("content")
                .and_then(|v| v.as_str())
                .ok_or("missing content")?;
            let full = resolve_path(workspace, path)?;
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::write(&full, content).map_err(|e| e.to_string())?;
            Ok(format!("wrote {} bytes to {}", content.len(), path))
        }
        "edit_file" => {
            let path = args
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or("missing path")?;
            let edits = args
                .get("edits")
                .and_then(|v| v.as_array())
                .ok_or("missing edits array")?;
            if edits.is_empty() {
                return Err("edits array is empty — give at least one edit".into());
            }

            let full = resolve_path(workspace, path)?;
            let mut content = fs::read_to_string(&full)
                .map_err(|e| format!("reading {}: {} — use write_file to create a new file", path, e))?;

            for (i, edit) in edits.iter().enumerate() {
                let old = edit
                    .get("old_string")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| format!("edit {} is missing old_string", i + 1))?;
                let new = edit.get("new_string").and_then(|v| v.as_str()).unwrap_or("");
                let replace_all = edit.get("replace_all").and_then(|v| v.as_bool()).unwrap_or(false);

                if old.is_empty() {
                    return Err(format!(
                        "edit {} has an empty old_string — every edit needs something to find",
                        i + 1
                    ));
                }

                let count = content.matches(old).count();
                if count == 0 {
                    return Err(format!(
                        "edit {}: old_string not found in {} — re-read the file, the content may not match exactly (including whitespace/indentation)",
                        i + 1,
                        path
                    ));
                }
                if count > 1 && !replace_all {
                    return Err(format!(
                        "edit {}: old_string matches {} places in {} — include more surrounding context to make it unique, or set replace_all: true if you mean to change all of them",
                        i + 1,
                        count,
                        path
                    ));
                }

                content = if replace_all {
                    content.replace(old, new)
                } else {
                    content.replacen(old, new, 1)
                };
            }

            fs::write(&full, &content).map_err(|e| e.to_string())?;
            Ok(format!(
                "applied {} edit{} to {}",
                edits.len(),
                if edits.len() == 1 { "" } else { "s" },
                path
            ))
        }
        "apply_patch" => {
            let patch = args
                .get("patch")
                .and_then(|v| v.as_str())
                .ok_or("missing patch")?;
            let files = parse_unified_diff(patch)?;
            let mut results = Vec::with_capacity(files.len());
            for fh in &files {
                results.push(apply_file_hunks(workspace, fh)?);
            }
            Ok(results.join("\n"))
        }
        "search_code" => {
            let query = args
                .get("query")
                .and_then(|v| v.as_str())
                .ok_or("missing query")?;
            let scope = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let case_sensitive = args.get("case_sensitive").and_then(|v| v.as_bool()).unwrap_or(false);
            let file_glob = args.get("file_glob").and_then(|v| v.as_str());
            let max_results = args
                .get("max_results")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize)
                .unwrap_or(200)
                .min(2000);

            let root = resolve_path(workspace, scope)?;
            let needle = if case_sensitive { query.to_string() } else { query.to_lowercase() };

            let mut matches: Vec<String> = Vec::new();
            let mut files_scanned = 0usize;
            walk_files(&root, &mut |file_path: &Path| {
                if matches.len() >= max_results {
                    return false;
                }
                files_scanned += 1;
                if files_scanned > MAX_SCAN_FILES {
                    return false;
                }
                if let Some(glob) = file_glob {
                    let file_name = file_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    if !glob_match(glob, &file_name) {
                        return true;
                    }
                }
                let text = match fs::read_to_string(file_path) {
                    Ok(t) => t,
                    Err(_) => return true, // binary or unreadable — skip, don't fail the whole search
                };
                let rel = relative_display_path(workspace, file_path);
                for (line_no, line) in text.lines().enumerate() {
                    if matches.len() >= max_results {
                        break;
                    }
                    let hay = if case_sensitive { line.to_string() } else { line.to_lowercase() };
                    if hay.contains(&needle) {
                        matches.push(format!("{}:{}: {}", rel, line_no + 1, line.trim()));
                    }
                }
                true
            });

            if matches.is_empty() {
                Ok(format!("No matches for {:?} under {}", query, scope))
            } else {
                let truncated = matches.len() >= max_results;
                let mut out = matches.join("\n");
                if truncated {
                    out.push_str(&format!(
                        "\n... stopped at {} results — narrow the query or file_glob for more",
                        max_results
                    ));
                }
                Ok(out)
            }
        }
        "find_files" => {
            let pattern = args
                .get("pattern")
                .and_then(|v| v.as_str())
                .ok_or("missing pattern")?;
            let scope = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let max_results = args
                .get("max_results")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize)
                .unwrap_or(200)
                .min(2000);

            let root = resolve_path(workspace, scope)?;
            let mut found: Vec<String> = Vec::new();
            let mut files_scanned = 0usize;
            walk_files(&root, &mut |file_path: &Path| {
                if found.len() >= max_results {
                    return false;
                }
                files_scanned += 1;
                if files_scanned > MAX_SCAN_FILES {
                    return false;
                }
                let rel = relative_display_path(workspace, file_path);
                let name = file_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                if glob_match(pattern, &name) || glob_match(pattern, &rel) {
                    found.push(rel);
                }
                true
            });

            if found.is_empty() {
                Ok(format!("No files matching {:?} under {}", pattern, scope))
            } else {
                Ok(found.join("\n"))
            }
        }
        "list_dir" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let full = resolve_path(workspace, path)?;
            let mut entries = vec![];
            for entry in fs::read_dir(&full).map_err(|e| e.to_string())? {
                let entry = entry.map_err(|e| e.to_string())?;
                let name = entry.file_name().to_string_lossy().to_string();
                let kind = if entry.path().is_dir() { "dir" } else { "file" };
                entries.push(format!("{} ({})", name, kind));
            }
            Ok(entries.join("\n"))
        }
        // Handled separately by run_shell_async — see agent::execute_tool,
        // which intercepts "run_shell" before reaching this synchronous
        // dispatcher, since it needs real async cancellation (a timeout and
        // a kill path) that this function's synchronous, blocking-friendly
        // callers (see spawn_blocking in agent.rs) can't provide.
        "run_shell" => Err("run_shell must be dispatched via run_shell_async".to_string()),
        _ => Err(format!("unknown tool: {}", name)),
    }
}

/// Builds the child process for run_shell, platform-appropriate, tokio
/// variant (mirrors `shell_command` above but for tokio::process::Command,
/// which is what lets run_shell_async actually kill a hung child instead
/// of blocking the async runtime with no way to cancel it).
#[cfg(target_os = "windows")]
fn tokio_shell_command(command: &str) -> tokio::process::Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut cmd = tokio::process::Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", command]);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[cfg(not(target_os = "windows"))]
fn tokio_shell_command(command: &str) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new("sh");
    cmd.args(["-c", command]);
    cmd
}

/// Reads `pipe` to completion (so the child process is never left blocked
/// on a full pipe buffer), keeping only the first `cap` bytes' worth of
/// text — draining the rest without holding onto it, so a runaway build
/// log doesn't balloon memory even though we still read all of it.
async fn drain_capped<R: tokio::io::AsyncRead + Unpin>(mut pipe: R, cap: usize) -> (String, usize) {
    let mut kept = Vec::with_capacity(cap.min(1 << 16));
    let mut total = 0usize;
    let mut buf = [0u8; 8192];
    loop {
        match pipe.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => {
                total += n;
                if kept.len() < cap {
                    let take = (cap - kept.len()).min(n);
                    kept.extend_from_slice(&buf[..take]);
                }
            }
            Err(_) => break,
        }
    }
    (String::from_utf8_lossy(&kept).to_string(), total)
}

/// The image used for sandbox_shell. Deliberately a stock, widely-cached
/// Debian base rather than a bespoke Kestrel-maintained image — it has a
/// real shell and coreutils, which covers plenty of workspace inspection
/// and scripting, but nothing else preinstalled. A task whose sandboxed
/// commands need a toolchain (a JDK, Gradle, a decompiler) should install
/// it inside the sandbox first (with sandbox_network on) — see the
/// decompilation skill, which spells this out for that specific workflow —
/// rather than this being baked into the image itself, which would mean
/// silently maintaining and trusting a much larger attack surface for
/// every sandboxed session whether it needs a JDK or not.
const SANDBOX_IMAGE: &str = "debian:stable-slim";

/// True if the `docker` binary is on PATH and the daemon actually responds
/// — checked fresh each call rather than cached, since whether Docker is
/// installed/running can change between one run_shell call and the next
/// far more plausibly than it changes mid-call.
async fn docker_available() -> bool {
    tokio::process::Command::new("docker")
        .args(["info"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Builds the sandboxed variant of the command: the workspace bind-mounted
/// read-write at /workspace (and nowhere else on the host reachable at
/// all), network disabled unless `network` is set, capabilities dropped,
/// and a conservative resource cap so a runaway process inside the sandbox
/// can't take down the host. `--rm` so nothing lingers after the command
/// (or the timeout-kill) ends.
fn sandboxed_shell_command(command: &str, workspace: &str, network: bool) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new("docker");
    cmd.args(["run", "--rm", "-i"]);
    if !network {
        cmd.args(["--network", "none"]);
    }
    cmd.args(["--cap-drop", "ALL", "--memory", "2g", "--cpus", "2", "--pids-limit", "512"]);
    cmd.args(["-v", &format!("{}:/workspace", workspace), "-w", "/workspace"]);
    cmd.arg(SANDBOX_IMAGE);
    cmd.args(["sh", "-c", command]);
    cmd
}

/// Runs a shell command with a real timeout and a real kill path: unlike a
/// synchronous `Command::output()` call, this can be cancelled — either
/// because it ran past `timeout` or because the user hit Stop — without
/// leaving the child process (and whatever it spawned) running forever in
/// the background with nothing able to reach it. When `sandbox` is set
/// (see Session::sandbox_shell), runs inside a locked-down container
/// instead of directly on the host — see sandboxed_shell_command — falling
/// back to a clear error (not a silent unsandboxed run) if Docker isn't
/// actually available, since a task that specifically asked for isolation
/// should never quietly run unisolated instead.
pub async fn run_shell_async(
    workspace: &str,
    command: &str,
    timeout_secs: u64,
    sandbox: bool,
    sandbox_network: bool,
    stop_flag: Arc<SessionStop>,
) -> Result<String, String> {
    let timeout_secs = timeout_secs.clamp(1, MAX_SHELL_TIMEOUT_SECS);

    let mut cmd = if sandbox {
        if !docker_available().await {
            return Err(
                "Sandboxed shell is on for this session but Docker isn't available (not \
                 installed, or the daemon isn't running) — install/start Docker, or turn off \
                 the sandbox for this session to run commands directly on the host instead."
                    .to_string(),
            );
        }
        sandboxed_shell_command(command, workspace, sandbox_network)
    } else {
        tokio_shell_command(command)
    };
    if !sandbox {
        cmd.current_dir(workspace);
    }
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.kill_on_drop(true);

    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let stdout_pipe = child.stdout.take().expect("stdout was piped");
    let stderr_pipe = child.stderr.take().expect("stderr was piped");

    let stdout_task = tokio::spawn(drain_capped(stdout_pipe, MAX_TOOL_OUTPUT_CHARS / 2));
    let stderr_task = tokio::spawn(drain_capped(stderr_pipe, MAX_TOOL_OUTPUT_CHARS / 2));

    // A single wait that resolves on whichever comes first: the child
    // exiting, the timeout elapsing, or the user hitting Stop (checked in
    // small increments by sleep_till_stop_or — see agent::SessionStop) —
    // so a hung build both times out on its own AND can be killed on
    // demand, instead of the Stop button being unable to reach it at all.
    let outcome = tokio::select! {
        status = child.wait() => {
            if let Err(e) = status {
                return Err(format!("failed to wait on child process: {}", e));
            }
            Outcome::Finished
        }
        _ = stop_flag.sleep_till_stop_or(Duration::from_secs(timeout_secs)) => {
            if stop_flag.is_requested() { Outcome::Stopped } else { Outcome::TimedOut }
        }
    };

    let killed_for = match &outcome {
        Outcome::TimedOut => Some("timed out"),
        Outcome::Stopped => Some("stopped by user"),
        Outcome::Finished => None,
    };
    if killed_for.is_some() {
        let _ = child.start_kill();
        let _ = child.wait().await;
    }

    let (stdout, stdout_total) = stdout_task.await.unwrap_or_default();
    let (stderr, stderr_total) = stderr_task.await.unwrap_or_default();
    let stdout = if stdout_total > stdout.len() {
        format!("{}\n... [{} more characters omitted]", stdout, stdout_total - stdout.len())
    } else {
        stdout
    };
    let stderr = if stderr_total > stderr.len() {
        format!("{}\n... [{} more characters omitted]", stderr, stderr_total - stderr.len())
    } else {
        stderr
    };

    match killed_for {
        Some(reason) => Ok(format!(
            "[process {reason} after {timeout_secs}s and was killed]\nstdout so far:\n{stdout}\nstderr so far:\n{stderr}"
        )),
        None => Ok(format!("stdout:\n{}\nstderr:\n{}", stdout, stderr)),
    }
}

enum Outcome {
    Finished,
    TimedOut,
    Stopped,
}
