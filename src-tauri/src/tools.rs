use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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
                "description": "Create or overwrite a text file, relative to the workspace root. Creates parent directories if needed.",
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
                "description": "Run a shell command inside the workspace root and return its stdout/stderr.",
                "parameters": {
                    "type": "object",
                    "properties": { "command": { "type": "string" } },
                    "required": ["command"]
                }
            }
        }
    ])
}

/// Tools that mutate state and should be gated behind approval when
/// planning mode is on. Read-only tools always execute immediately.
pub fn is_mutating(tool_name: &str) -> bool {
    matches!(tool_name, "write_file" | "run_shell")
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
            let (shell, flag) = if cfg!(target_os = "windows") {
                ("cmd", "/C")
            } else {
                ("sh", "-c")
            };
            let output = Command::new(shell)
                .arg(flag)
                .arg(command)
                .current_dir(workspace)
                .output()
                .map_err(|e| e.to_string())?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            Ok(format!("stdout:\n{}\nstderr:\n{}", stdout, stderr))
        }
        _ => Err(format!("unknown tool: {}", name)),
    }
}
