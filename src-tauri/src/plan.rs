use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use tauri::{Emitter, Manager};

/// One entry in a session's plan. Status is a plain string rather than an
/// enum so a future status value from a newer app version doesn't fail to
/// deserialize on an older one — anything other than "completed"/
/// "in_progress" just renders as "pending".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanItem {
    pub content: String,
    /// "pending" | "in_progress" | "completed"
    #[serde(default = "default_status")]
    pub status: String,
}

fn default_status() -> String {
    "pending".to_string()
}

/// The tool schema exposed to both the top-level agent and sub-agents.
/// Deliberately a single full-replace call (same shape as Claude Code's
/// TodoWrite) rather than add/complete/remove verbs — a full replace is
/// trivial for the model to get right every time and trivial for this file
/// to persist, and the whole list is always small.
pub fn tool_definitions() -> Value {
    json!([
        {
            "type": "function",
            "function": {
                "name": "update_plan",
                "description": "Write (replacing entirely) the durable task list for this session — a short checklist of concrete steps toward the user's current goal. Unlike the conversation history, this list is never summarized or dropped, so it's the one thing you can rely on staying intact across a very long, multi-hour task: use it for anything with more than a handful of steps (e.g. 'unpack the jar, decompile each package, fix compile errors, verify against the original, rerun tests'), and keep it updated as you make progress — mark a step in_progress before you start it and completed the moment it's done, rather than batching updates. Not needed for short, single-step requests.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "items": {
                            "type": "array",
                            "description": "The full plan, in order. Replaces whatever was there before, so include every step (not just the ones that changed).",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "content": { "type": "string", "description": "Short imperative description of the step, e.g. 'Decompile com.example.util package'." },
                                    "status": { "type": "string", "enum": ["pending", "in_progress", "completed"] }
                                },
                                "required": ["content", "status"]
                            }
                        }
                    },
                    "required": ["items"]
                }
            }
        }
    ])
}

fn plans_dir(app_handle: &tauri::AppHandle) -> PathBuf {
    let dir = app_handle
        .path()
        .app_config_dir()
        .expect("could not resolve app config dir")
        .join("plans");
    fs::create_dir_all(&dir).ok();
    dir
}

pub fn load(app_handle: &tauri::AppHandle, session_id: &str) -> Vec<PlanItem> {
    let path = plans_dir(app_handle).join(format!("{}.json", session_id));
    fs::read_to_string(path)
        .ok()
        .and_then(|d| serde_json::from_str(&d).ok())
        .unwrap_or_default()
}

pub fn save(app_handle: &tauri::AppHandle, session_id: &str, items: &[PlanItem]) -> Result<(), String> {
    let path = plans_dir(app_handle).join(format!("{}.json", session_id));
    let data = serde_json::to_string_pretty(items).map_err(|e| e.to_string())?;
    fs::write(path, data).map_err(|e| e.to_string())
}

/// Renders the current plan as a compact status block for injection into
/// the model's context each step (see agent::run_turn_inner) — re-read
/// fresh from disk every step rather than carried in `session.messages`,
/// which is exactly what makes it survive both context compaction (it was
/// never part of the compacted history to begin with) and, incidentally,
/// a crash mid-turn. Returns None for an empty plan so turns that never
/// touch it don't pay for an empty system message.
pub fn render(items: &[PlanItem]) -> Option<String> {
    if items.is_empty() {
        return None;
    }
    let mut out = String::from("Current task list for this session (kept outside the conversation history, so it's unaffected by any summarization above):\n");
    for item in items {
        let mark = match item.status.as_str() {
            "completed" => "[x]",
            "in_progress" => "[~]",
            _ => "[ ]",
        };
        out.push_str(&format!("{} {}\n", mark, item.content));
    }
    Some(out)
}

/// Handles update_plan if `name` matches, else returns None so the caller
/// can fall through to the next handler — same convention as
/// skills::maybe_execute.
pub fn maybe_execute(app_handle: &tauri::AppHandle, session_id: &str, name: &str, args: &Value) -> Option<Result<String, String>> {
    if name != "update_plan" {
        return None;
    }
    let items_val = match args.get("items").and_then(|v| v.as_array()) {
        Some(v) => v,
        None => return Some(Err("missing items array".to_string())),
    };
    let mut items = Vec::with_capacity(items_val.len());
    for (i, item) in items_val.iter().enumerate() {
        let content = match item.get("content").and_then(|v| v.as_str()) {
            Some(s) if !s.trim().is_empty() => s.to_string(),
            _ => return Some(Err(format!("item {} is missing non-empty content", i + 1))),
        };
        let status = item
            .get("status")
            .and_then(|v| v.as_str())
            .filter(|s| matches!(*s, "pending" | "in_progress" | "completed"))
            .unwrap_or("pending")
            .to_string();
        items.push(PlanItem { content, status });
    }
    if let Err(e) = save(app_handle, session_id, &items) {
        return Some(Err(e));
    }
    let _ = app_handle.emit(
        "agent://plan-updated",
        json!({ "session_id": session_id, "items": items }),
    );
    let done = items.iter().filter(|i| i.status == "completed").count();
    Some(Ok(format!("Plan updated: {} step(s), {} completed.", items.len(), done)))
}

#[tauri::command]
pub fn get_session_plan(app_handle: tauri::AppHandle, id: String) -> Vec<PlanItem> {
    load(&app_handle, &id)
}
