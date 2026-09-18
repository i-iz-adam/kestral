use serde::Serialize;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};

/// A file touched in the workspace, as reported by `git status`.
#[derive(Debug, Clone, Serialize)]
pub struct GitFile {
    pub path: String,
    /// Porcelain status code, for example `M`, `A`, `D`, `??`, or `R`.
    pub status: String,
    pub staged: bool,
    pub additions: usize,
    pub deletions: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct GitDiff {
    pub files: Vec<GitFile>,
    pub patch: String,
}

fn ensure_workspace(workspace: &str) -> Result<PathBuf, String> {
    let path = Path::new(workspace);
    if !path.is_dir() {
        return Err("Workspace directory does not exist".to_string());
    }
    Ok(path.to_path_buf())
}

/// Git paths are always relative to the selected workspace. Rejecting path
/// traversal here is important because these commands are also exposed to
/// the webview, not just called by trusted Rust code.
fn validate_paths(workspace: &Path, paths: &[String]) -> Result<Vec<String>, String> {
    let root = workspace
        .canonicalize()
        .map_err(|e| format!("Could not resolve workspace: {e}"))?;
    paths
        .iter()
        .map(|raw| {
            let path = Path::new(raw);
            if path.is_absolute()
                || path.has_root()
                || path
                    .components()
                    .any(|component| matches!(component, Component::ParentDir))
            {
                return Err(format!("Invalid workspace-relative path: {raw}"));
            }
            let candidate = root.join(path);
            // Existing files must remain beneath the workspace after resolving
            // symlinks. Nonexistent paths are safe because git treats them as
            // pathspecs and never follows them outside the repository.
            if candidate.exists() {
                let resolved = candidate
                    .canonicalize()
                    .map_err(|e| format!("Could not resolve {raw}: {e}"))?;
                if !resolved.starts_with(&root) {
                    return Err(format!("Path is outside the workspace: {raw}"));
                }
            }
            Ok(raw.replace('\\', "/"))
        })
        .collect()
}

fn git_output(workspace: &Path, args: &[&str]) -> Result<Output, String> {
    Command::new("git")
        .args(args)
        .current_dir(workspace)
        .output()
        .map_err(|e| format!("Could not run git: {e}"))
}

fn check_output(output: Output, operation: &str) -> Result<Output, String> {
    if output.status.success() {
        Ok(output)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            format!("git {operation} failed")
        } else {
            format!("git {operation} failed: {stderr}")
        })
    }
}

fn status_files(workspace: &Path, paths: &[String]) -> Result<Vec<GitFile>, String> {
    let mut args = vec!["status", "--porcelain=v1", "-z", "--untracked-files=all"];
    let path_refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    if !path_refs.is_empty() {
        args.push("--");
        args.extend(path_refs.iter().copied());
    }
    let output = check_output(git_output(workspace, &args)?, "status")?;
    let raw = String::from_utf8_lossy(&output.stdout);
    let mut entries = raw.split('\0');
    let mut files = Vec::new();
    while let Some(entry) = entries.next() {
        if entry.is_empty() || entry.len() < 3 {
            continue;
        }
        let index = entry.as_bytes()[0] as char;
        let worktree = entry.as_bytes()[1] as char;
        let path = entry[3..].to_string();
        // Porcelain -z emits the old path as a second NUL-delimited value for
        // renames/copies. Keep only the destination, which is the path that
        // stage/unstage/revert should operate on.
        if matches!(index, 'R' | 'C') || matches!(worktree, 'R' | 'C') {
            let _ = entries.next();
        }
        let status = if index == '?' && worktree == '?' {
            "??".to_string()
        } else if index == 'R' || worktree == 'R' {
            "R".to_string()
        } else if index != ' ' && worktree != ' ' {
            "M".to_string()
        } else if index != ' ' {
            index.to_string()
        } else {
            worktree.to_string()
        };
        files.push(GitFile {
            path,
            status,
            staged: index != ' ' && index != '?',
            additions: 0,
            deletions: 0,
        });
    }
    Ok(files)
}

fn add_line_stats(files: &mut [GitFile], patch: &str) {
    let mut current: Option<usize> = None;
    for line in patch.lines() {
        if let Some(path) = line.strip_prefix("+++ b/") {
            current = files.iter().position(|file| file.path == path);
            continue;
        }
        if let Some(path) = line.strip_prefix("--- a/") {
            if current.is_none() {
                current = files.iter().position(|file| file.path == path);
            }
            continue;
        }
        if line.starts_with("@@") || line.starts_with("--- ") || line.starts_with("diff ") {
            continue;
        }
        if let Some(index) = current {
            if line.starts_with('+') {
                files[index].additions += 1;
            } else if line.starts_with('-') {
                files[index].deletions += 1;
            }
        }
    }
}

pub fn empty_diff() -> GitDiff {
    GitDiff {
        files: Vec::new(),
        patch: String::new(),
    }
}

/// Computes the working tree state (staged and unstaged changes together).
/// Untracked files are included in the file list and represented in the patch
/// too, using git's no-index mode.
pub fn diff(workspace: &str, paths: Option<Vec<String>>) -> Result<GitDiff, String> {
    let workspace = ensure_workspace(workspace)?;
    let paths = validate_paths(&workspace, &paths.unwrap_or_default())?;
    let mut files = status_files(&workspace, &paths)?;
    let path_refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let mut args = vec!["diff", "HEAD", "--no-ext-diff", "--binary"];
    if !path_refs.is_empty() {
        args.push("--");
        args.extend(path_refs.iter().copied());
    }
    let tracked = match git_output(&workspace, &args)? {
        output if output.status.success() => output,
        output => {
            // A newly initialized repository has no HEAD yet. In that case
            // combine the worktree and index diffs instead of treating the
            // missing revision as a repository error.
            let unstaged = check_output(
                git_output(
                    &workspace,
                    &["diff", "--no-ext-diff", "--binary", "--"]
                        .iter()
                        .copied()
                        .chain(path_refs.iter().copied())
                        .collect::<Vec<_>>(),
                )?,
                "diff",
            )?;
            let staged = check_output(
                git_output(
                    &workspace,
                    &["diff", "--cached", "--no-ext-diff", "--binary", "--"]
                        .iter()
                        .copied()
                        .chain(path_refs.iter().copied())
                        .collect::<Vec<_>>(),
                )?,
                "diff --cached",
            )?;
            let mut combined = unstaged.stdout;
            combined.extend(staged.stdout);
            Output {
                status: output.status,
                stdout: combined,
                stderr: Vec::new(),
            }
        }
    };
    let mut patch = String::from_utf8_lossy(&tracked.stdout).into_owned();

    // `git diff HEAD` cannot show untracked content. Ask no-index for each
    // untracked file, accepting its documented exit code 1 (differences).
    for file in files.iter().filter(|file| file.status == "??") {
        let output = git_output(
            &workspace,
            &[
                "diff",
                "--no-index",
                "--binary",
                "/dev/null",
                "--",
                &file.path,
            ],
        )?;
        if !output.status.success() && output.status.code() != Some(1) {
            continue;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        if !text.is_empty() {
            patch.push_str(&text);
            if !patch.ends_with('\n') {
                patch.push('\n');
            }
        }
    }
    add_line_stats(&mut files, &patch);
    Ok(GitDiff { files, patch })
}

pub fn stage(workspace: &str, paths: Vec<String>) -> Result<(), String> {
    let workspace = ensure_workspace(workspace)?;
    let paths = validate_paths(&workspace, &paths)?;
    if paths.is_empty() {
        return Err("Select at least one file".to_string());
    }
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let mut args = vec!["add", "--"];
    args.extend(refs);
    check_output(git_output(&workspace, &args)?, "add").map(|_| ())
}

pub fn unstage(workspace: &str, paths: Vec<String>) -> Result<(), String> {
    let workspace = ensure_workspace(workspace)?;
    let paths = validate_paths(&workspace, &paths)?;
    if paths.is_empty() {
        return Err("Select at least one file".to_string());
    }
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let mut args = vec!["restore", "--staged", "--"];
    args.extend(refs);
    check_output(git_output(&workspace, &args)?, "restore --staged").map(|_| ())
}

/// Discards selected tracked changes and removes selected untracked files.
/// The caller should confirm this destructive operation before invoking it.
pub fn revert(workspace: &str, paths: Vec<String>) -> Result<(), String> {
    let workspace = ensure_workspace(workspace)?;
    let paths = validate_paths(&workspace, &paths)?;
    if paths.is_empty() {
        return Err("Select at least one file".to_string());
    }
    let files = status_files(&workspace, &paths)?;
    let untracked: Vec<String> = files
        .iter()
        .filter(|file| file.status == "??")
        .map(|file| file.path.clone())
        .collect();
    let tracked: Vec<String> = files
        .iter()
        .filter(|file| file.status != "??")
        .map(|file| file.path.clone())
        .collect();

    if !tracked.is_empty() {
        let refs: Vec<&str> = tracked.iter().map(String::as_str).collect();
        let mut restore = vec!["restore", "--source=HEAD", "--worktree", "--staged", "--"];
        restore.extend(refs);
        check_output(git_output(&workspace, &restore)?, "restore")?;
    }
    if !untracked.is_empty() {
        let refs: Vec<&str> = untracked.iter().map(String::as_str).collect();
        let mut clean = vec!["clean", "-f", "--"];
        clean.extend(refs);
        check_output(git_output(&workspace, &clean)?, "clean")?;
    }
    Ok(())
}

/// Extracts paths touched by mutating file tools in a persisted session.
pub fn session_paths(session: &crate::sessions::Session) -> Vec<String> {
    let mut paths = Vec::new();
    for message in &session.messages {
        let Some(calls) = &message.tool_calls else {
            continue;
        };
        for call in calls {
            if !matches!(
                call.function.name.as_str(),
                "write_file" | "edit_file" | "apply_patch"
            ) {
                continue;
            }
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&call.function.arguments) {
                if let Some(path) = value.get("path").and_then(serde_json::Value::as_str) {
                    if !paths.iter().any(|existing| existing == path) {
                        paths.push(path.to_string());
                    }
                }
                if call.function.name == "apply_patch" {
                    if let Some(patch) = value.get("patch").and_then(serde_json::Value::as_str) {
                        for line in patch.lines() {
                            let raw = line
                                .strip_prefix("+++ b/")
                                .or_else(|| line.strip_prefix("--- a/"));
                            if let Some(path) = raw {
                                if path != "/dev/null"
                                    && !paths.iter().any(|existing| existing == path)
                                {
                                    paths.push(path.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    paths
}
