use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

use crate::omniroute::ChatMessage;

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub title: String,
    /// "coding" (tools enabled) or "general" (plain chat, no tools)
    pub mode: String,
    pub planning_enabled: bool,
    pub workspace: String,
    /// "owner/repo" — lets GitHub tool calls omit owner/repo when this
    /// session is about a specific repo. Optional, settable after creation.
    #[serde(default)]
    pub linked_repo: Option<String>,
    /// Whether the agent has delegate_to_subagent available and is told to
    /// prefer it for bulky/exploratory work. Defaults to true (including
    /// for sessions saved before this field existed) — sub-agents are the
    /// default behavior, not an opt-in.
    #[serde(default = "default_true")]
    pub subagents_enabled: bool,
    pub messages: Vec<ChatMessage>,
    pub created_at: u64,
}

fn sessions_dir(app_handle: &tauri::AppHandle) -> PathBuf {
    let dir = app_handle
        .path_resolver()
        .app_config_dir()
        .expect("could not resolve app config dir")
        .join("sessions");
    fs::create_dir_all(&dir).ok();
    dir
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn create(
    app_handle: &tauri::AppHandle,
    title: String,
    mode: String,
    workspace: String,
    planning_enabled: bool,
    linked_repo: Option<String>,
    subagents_enabled: bool,
) -> Session {
    let session = Session {
        id: Uuid::new_v4().to_string(),
        title,
        mode,
        planning_enabled,
        workspace,
        linked_repo,
        subagents_enabled,
        messages: vec![],
        created_at: now_ms(),
    };
    save(app_handle, &session);
    session
}

pub fn set_linked_repo(app_handle: &tauri::AppHandle, id: &str, repo: Option<String>) -> Result<(), String> {
    let mut session = load(app_handle, id).ok_or("Session not found")?;
    session.linked_repo = repo;
    save(app_handle, &session);
    Ok(())
}

pub fn set_subagents_enabled(app_handle: &tauri::AppHandle, id: &str, enabled: bool) -> Result<(), String> {
    let mut session = load(app_handle, id).ok_or("Session not found")?;
    session.subagents_enabled = enabled;
    save(app_handle, &session);
    Ok(())
}

pub fn save(app_handle: &tauri::AppHandle, session: &Session) {
    let path = sessions_dir(app_handle).join(format!("{}.json", session.id));
    if let Ok(data) = serde_json::to_string_pretty(session) {
        let _ = fs::write(path, data);
    }
}

pub fn load(app_handle: &tauri::AppHandle, id: &str) -> Option<Session> {
    let path = sessions_dir(app_handle).join(format!("{}.json", id));
    let data = fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

pub fn list(app_handle: &tauri::AppHandle) -> Vec<Session> {
    let dir = sessions_dir(app_handle);
    let mut sessions = vec![];
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(data) = fs::read_to_string(entry.path()) {
                if let Ok(session) = serde_json::from_str::<Session>(&data) {
                    sessions.push(session);
                }
            }
        }
    }
    sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    sessions
}

pub fn delete(app_handle: &tauri::AppHandle, id: &str) {
    let path = sessions_dir(app_handle).join(format!("{}.json", id));
    let _ = fs::remove_file(path);
}
