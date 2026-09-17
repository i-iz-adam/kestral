use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::Manager;
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
    /// Whether the agent has delegate_to_subagent available and is told to
    /// prefer it for bulky/exploratory work. Defaults to true (including
    /// for sessions saved before this field existed) — sub-agents are the
    /// default behavior, not an opt-in.
    #[serde(default = "default_true")]
    pub subagents_enabled: bool,
    /// Whether this session's Stop button does a graceful stop (live
    /// sub-agents wind down and hand back an overview) vs. an instant one
    /// (no overview). Defaults to true, matching the app-wide session
    /// default — a session from before this field existed stays on the
    /// graceful path.
    #[serde(default = "default_true")]
    pub graceful_stop: bool,
    pub messages: Vec<ChatMessage>,
    pub created_at: u64,
    /// Timestamp (UNIX epoch milliseconds) when this session was last updated.
    /// Defaults to None for older sessions saved before this field existed,
    /// in which case `effective_updated_at()` falls back to `created_at`.
    #[serde(default)]
    pub updated_at: Option<u64>,
    /// Counts turns since the last skill-distillation reflection pass (see
    /// reflect.rs) — debounces it to roughly once every REFLECT_EVERY_N_TURNS
    /// turns that did real (mutating/delegated) work, rather than running an
    /// extra model call after every single turn. Defaults to 0 for sessions
    /// saved before this field existed, which just means their next
    /// qualifying turn or two count toward the first reflection as normal.
    #[serde(default)]
    pub turns_since_reflection: u32,
    /// Run run_shell inside a locked-down Docker container (no host
    /// filesystem access outside the workspace, no network unless
    /// sandbox_network is also on, dropped capabilities) instead of
    /// directly on the host with the user's own permissions. Off by
    /// default — most coding tasks are fine running directly and this
    /// requires Docker to be installed — but worth turning on for a task
    /// that involves running code of unknown origin (an obfuscated jar
    /// someone's decompiling and rebuilding, for instance), where run_shell
    /// executing arbitrary commands with the user's real permissions is a
    /// meaningfully worse tradeoff than usual.
    #[serde(default)]
    pub sandbox_shell: bool,
    /// Whether the sandbox (when sandbox_shell is on) has network access.
    /// Off by default, matching sandbox_shell's own default posture —
    /// enable it only for a sandboxed task that genuinely needs to fetch
    /// something (a build tool downloading its dependencies).
    #[serde(default)]
    pub sandbox_network: bool,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub archived: bool,
}

impl Session {
    pub fn effective_updated_at(&self) -> u64 {
        self.updated_at.unwrap_or(self.created_at)
    }
}

fn sessions_dir(app_handle: &tauri::AppHandle) -> PathBuf {
    let dir = app_handle
        .path()
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
    subagents_enabled: bool,
    graceful_stop: bool,
) -> Session {
    let now = now_ms();
    let session = Session {
        id: Uuid::new_v4().to_string(),
        title,
        mode,
        planning_enabled,
        workspace,
        subagents_enabled,
        graceful_stop,
        messages: vec![],
        created_at: now,
        updated_at: Some(now),
        turns_since_reflection: 0,
        sandbox_shell: false,
        sandbox_network: false,
        pinned: false,
        archived: false,
    };
    save(app_handle, &session);
    session
}

pub fn set_sandbox_shell(
    app_handle: &tauri::AppHandle,
    id: &str,
    enabled: bool,
) -> Result<(), String> {
    let mut session = load(app_handle, id).ok_or("Session not found")?;
    session.sandbox_shell = enabled;
    save(app_handle, &session);
    Ok(())
}

pub fn set_sandbox_network(
    app_handle: &tauri::AppHandle,
    id: &str,
    enabled: bool,
) -> Result<(), String> {
    let mut session = load(app_handle, id).ok_or("Session not found")?;
    session.sandbox_network = enabled;
    save(app_handle, &session);
    Ok(())
}

pub fn set_workspace(
    app_handle: &tauri::AppHandle,
    id: &str,
    workspace: String,
) -> Result<(), String> {
    let mut session = load(app_handle, id).ok_or("Session not found")?;
    session.workspace = workspace;
    save(app_handle, &session);
    Ok(())
}

pub fn set_subagents_enabled(
    app_handle: &tauri::AppHandle,
    id: &str,
    enabled: bool,
) -> Result<(), String> {
    let mut session = load(app_handle, id).ok_or("Session not found")?;
    session.subagents_enabled = enabled;
    save(app_handle, &session);
    Ok(())
}

pub fn set_graceful_stop(
    app_handle: &tauri::AppHandle,
    id: &str,
    enabled: bool,
) -> Result<(), String> {
    let mut session = load(app_handle, id).ok_or("Session not found")?;
    session.graceful_stop = enabled;
    save(app_handle, &session);
    Ok(())
}

pub fn set_title(app_handle: &tauri::AppHandle, id: &str, title: String) -> Result<(), String> {
    let mut session = load(app_handle, id).ok_or("Session not found")?;
    session.title = title;
    save(app_handle, &session);
    Ok(())
}

pub fn set_planning_enabled(
    app_handle: &tauri::AppHandle,
    id: &str,
    enabled: bool,
) -> Result<(), String> {
    let mut session = load(app_handle, id).ok_or("Session not found")?;
    session.planning_enabled = enabled;
    save(app_handle, &session);
    Ok(())
}

pub fn set_pinned(app_handle: &tauri::AppHandle, id: &str, pinned: bool) -> Result<(), String> {
    let mut session = load(app_handle, id).ok_or("Session not found")?;
    session.pinned = pinned;
    save(app_handle, &session);
    Ok(())
}

pub fn set_archived(app_handle: &tauri::AppHandle, id: &str, archived: bool) -> Result<(), String> {
    let mut session = load(app_handle, id).ok_or("Session not found")?;
    session.archived = archived;
    save(app_handle, &session);
    Ok(())
}

pub fn save(app_handle: &tauri::AppHandle, session: &Session) {
    let mut session_to_save = session.clone();
    session_to_save.updated_at = Some(now_ms());
    let dir = sessions_dir(app_handle);
    let path = dir.join(format!("{}.json", session_to_save.id));
    let temp_path = dir.join(format!(
        "{}.json.tmp.{}",
        session_to_save.id,
        Uuid::new_v4()
    ));
    if let Ok(data) = serde_json::to_string_pretty(&session_to_save) {
        if fs::write(&temp_path, &data).is_ok() {
            if fs::rename(&temp_path, &path).is_err() {
                let _ = fs::remove_file(&temp_path);
            }
        }
    }
}

pub fn save_async(app_handle: &tauri::AppHandle, session: &Session) {
    let app_handle = app_handle.clone();
    let session = session.clone();
    tokio::task::spawn_blocking(move || {
        save(&app_handle, &session);
    });
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
    sessions.sort_by(|a, b| {
        b.effective_updated_at()
            .cmp(&a.effective_updated_at())
            .then_with(|| b.created_at.cmp(&a.created_at))
    });
    sessions
}

pub fn delete(app_handle: &tauri::AppHandle, id: &str) {
    let path = sessions_dir(app_handle).join(format!("{}.json", id));
    let _ = fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_effective_updated_at_fallback() {
        let session = Session {
            id: "1".into(),
            title: "Test".into(),
            mode: "coding".into(),
            planning_enabled: false,
            workspace: "".into(),
            subagents_enabled: true,
            graceful_stop: true,
            messages: vec![],
            created_at: 1000,
            updated_at: None,
            turns_since_reflection: 0,
            sandbox_shell: false,
            sandbox_network: false,
            pinned: false,
            archived: false,
        };
        assert_eq!(session.effective_updated_at(), 1000);

        let session_with_updated = Session {
            updated_at: Some(2000),
            ..session
        };
        assert_eq!(session_with_updated.effective_updated_at(), 2000);
    }

    #[test]
    fn test_legacy_session_json_deserialization() {
        let legacy_json = r#"{
            "id": "test-legacy",
            "title": "Legacy Session",
            "mode": "coding",
            "planning_enabled": false,
            "workspace": "/tmp",
            "messages": [],
            "created_at": 1700000000000
        }"#;

        let session: Session = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(session.created_at, 1700000000000);
        assert_eq!(session.updated_at, None);
        assert_eq!(session.effective_updated_at(), 1700000000000);
    }

    #[test]
    fn test_session_sorting_by_updated_at() {
        let mut sessions = vec![
            Session {
                id: "old-created-recently-updated".into(),
                title: "Old Chat".into(),
                mode: "coding".into(),
                planning_enabled: false,
                workspace: "".into(),
                subagents_enabled: true,
                graceful_stop: true,
                messages: vec![],
                created_at: 1000,
                updated_at: Some(5000),
                turns_since_reflection: 0,
                sandbox_shell: false,
                sandbox_network: false,
                pinned: false,
                archived: false,
            },
            Session {
                id: "newly-created".into(),
                title: "New Chat".into(),
                mode: "coding".into(),
                planning_enabled: false,
                workspace: "".into(),
                subagents_enabled: true,
                graceful_stop: true,
                messages: vec![],
                created_at: 3000,
                updated_at: Some(3000),
                turns_since_reflection: 0,
                sandbox_shell: false,
                sandbox_network: false,
                pinned: false,
                archived: false,
            },
            Session {
                id: "legacy-session".into(),
                title: "Legacy Chat".into(),
                mode: "coding".into(),
                planning_enabled: false,
                workspace: "".into(),
                subagents_enabled: true,
                graceful_stop: true,
                messages: vec![],
                created_at: 2000,
                updated_at: None,
                turns_since_reflection: 0,
                sandbox_shell: false,
                sandbox_network: false,
                pinned: false,
                archived: false,
            },
        ];

        sessions.sort_by(|a, b| {
            b.effective_updated_at()
                .cmp(&a.effective_updated_at())
                .then_with(|| b.created_at.cmp(&a.created_at))
        });

        assert_eq!(sessions[0].id, "old-created-recently-updated");
        assert_eq!(sessions[1].id, "newly-created");
        assert_eq!(sessions[2].id, "legacy-session");
    }
}
