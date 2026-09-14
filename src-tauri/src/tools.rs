use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

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
                "description": "Read the contents of a text file, relative to the workspace root.",
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
                "description": "Run a shell command inside the workspace root and return its stdout/stderr. On macOS/Linux this runs via 'sh -c'; on Windows it runs via PowerShell (not cmd.exe), which does have 'ls', 'cat', 'cp', 'mv', 'rm', 'pwd', and 'echo' as built-in aliases, but not 'grep' or Unix-style 'find' — use search_code/find_files instead of piping through grep/find, and prefer read_file/edit_file/apply_patch over cat/redirection for reading or changing files, since those work identically on every platform. Quote arguments the way the target shell expects (e.g. a git commit message must be one quoted argument to -m — an unquoted multi-word message gets split into extra pathspec arguments and fails).",
                "parameters": {
                    "type": "object",
                    "properties": { "command": { "type": "string" } },
                    "required": ["command"]
                }
            }
        },
        {
            "type": "function",
            "function": {
                "name": "run_python",
                "description": "Execute Python code in a sandboxed subprocess. By default, runs in an isolated temporary directory with network disabled and strict execution bounds (wall-clock timeout & output size limits). Set workspace_access: true to allow reading/writing files in the workspace root.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "code": { "type": "string", "description": "The Python code snippet or script to execute." },
                        "workspace_access": { "type": "boolean", "description": "Optional. Defaults to false (isolated temp directory sandbox). Set true if code must read or write files directly in the workspace root." }
                    },
                    "required": ["code"]
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

pub fn is_mutating_with_args(tool_name: &str, args: &Value) -> bool {
    if tool_name == "run_python" {
        return args.get("workspace_access").and_then(|v| v.as_bool()).unwrap_or(false);
    }
    is_mutating(tool_name)
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

/// Builds the child process for run_shell, platform-appropriate.
///
/// On Windows this runs via PowerShell rather than cmd.exe — PowerShell
/// has 'ls'/'cat'/'cp'/'mv'/'rm'/'pwd'/'echo' as built-in aliases, which
/// cmd.exe has none of, so a model reaching for those (as it naturally
/// will, having learned on Unix shells) doesn't just fail outright. It
/// also sets CREATE_NO_WINDOW so approving a shell call doesn't flash a
/// console window on screen for a moment, which — with no console
/// attached at all — is otherwise the default behavior for any child
/// console process spawned from a GUI app on Windows.
#[cfg(target_os = "windows")]
fn shell_command(command: &str) -> Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", command]);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

#[cfg(not(target_os = "windows"))]
fn shell_command(command: &str) -> Command {
    let mut cmd = Command::new("sh");
    cmd.args(["-c", command]);
    cmd
}

pub fn execute(workspace: &str, name: &str, args: &Value) -> Result<String, String> {
    match name {
        "read_file" => {
            let path = args
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or("missing path")?;
            let full = resolve_path(workspace, path)?;
            fs::read_to_string(&full).map_err(|e| e.to_string())
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
        "run_shell" => {
            let command = args
                .get("command")
                .and_then(|v| v.as_str())
                .ok_or("missing command")?;
            let output = shell_command(command)
                .current_dir(workspace)
                .output()
                .map_err(|e| e.to_string())?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            Ok(format!("stdout:\n{}\nstderr:\n{}", stdout, stderr))
        }
        "run_python" => {
            let code = args
                .get("code")
                .and_then(|v| v.as_str())
                .ok_or("missing code")?;
            let workspace_access = args
                .get("workspace_access")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            let (working_dir, _is_temp) = if workspace_access {
                (resolve_path(workspace, ".")?, false)
            } else {
                let temp_dir = std::env::temp_dir().join(format!("kestrel_python_{}", uuid::Uuid::new_v4()));
                fs::create_dir_all(&temp_dir).map_err(|e| format!("failed to create temp dir: {}", e))?;
                (temp_dir, true)
            };

            run_python_execution(&working_dir, code)
        }
        _ => Err(format!("unknown tool: {}", name)),
    }
}

fn run_python_execution(working_dir: &Path, code: &str) -> Result<String, String> {
    let script_path = working_dir.join("script.py");
    fs::write(&script_path, code).map_err(|e| format!("failed to write python script: {}", e))?;

    let python_bin = if Command::new("python3").arg("--version").output().is_ok() {
        "python3"
    } else if Command::new("python").arg("--version").output().is_ok() {
        "python"
    } else {
        return Err("Python interpreter ('python3' or 'python') not found on system PATH.".to_string());
    };

    let mut cmd = Command::new(python_bin);
    cmd.arg("script.py")
        .current_dir(working_dir)
        .env("HTTP_PROXY", "")
        .env("HTTPS_PROXY", "")
        .env("ALL_PROXY", "")
        .env("NO_PROXY", "*");

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd.spawn().map_err(|e| format!("failed to spawn python process: {}", e))?;

    let timeout = Duration::from_secs(30);
    let start = Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = child.wait_with_output().map_err(|e| format!("failed to read python output: {}", e))?;
                let mut stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let mut stderr = String::from_utf8_lossy(&output.stderr).to_string();

                const MAX_OUTPUT_BYTES: usize = 100_000;
                if stdout.len() > MAX_OUTPUT_BYTES {
                    stdout.truncate(MAX_OUTPUT_BYTES);
                    stdout.push_str("\n... [stdout truncated at 100KB]");
                }
                if stderr.len() > MAX_OUTPUT_BYTES {
                    stderr.truncate(MAX_OUTPUT_BYTES);
                    stderr.push_str("\n... [stderr truncated at 100KB]");
                }

                let mut generated_artifacts = Vec::new();
                if let Ok(entries) = fs::read_dir(working_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() {
                            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                                if matches!(ext.to_lowercase().as_str(), "png" | "jpg" | "jpeg" | "webp" | "svg") {
                                    if path.file_name().and_then(|n| n.to_str()) != Some("script.py") {
                                        generated_artifacts.push(path.to_string_lossy().to_string());
                                    }
                                }
                            }
                        }
                    }
                }

                let mut res = format!("Exit status: {}\nstdout:\n{}\nstderr:\n{}", status, stdout, stderr);
                if !generated_artifacts.is_empty() {
                    res.push_str("\nGenerated image artifacts:\n");
                    for artifact in generated_artifacts {
                        res.push_str(&format!("- {}\n", artifact));
                    }
                }
                return Ok(res);
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    return Err("Python execution timed out after 30 seconds.".to_string());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("error waiting for python process: {}", e)),
        }
    }
}
