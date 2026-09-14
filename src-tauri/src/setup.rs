use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// One entry in the setup wizard.
///
/// `id` must be unique and stable forever once shipped — it's how a user's
/// completed-steps list is matched against the current registry. `order`
/// controls display order among steps that are still pending for that user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupStepDef {
    pub id: String,
    pub order: u32,
}

/// The full list of setup steps this build knows about.
///
/// To add a step in a future release: append an entry here with a new id
/// and the next order value, and add a matching component in the frontend's
/// `stepRegistry.tsx` keyed by the same id. Never reuse or reorder existing
/// ids — that would either replay a step for existing users or skip it for
/// new ones.
pub fn step_registry() -> Vec<SetupStepDef> {
    vec![
        SetupStepDef { id: "welcome".into(), order: 0 },
        SetupStepDef { id: "omniroute".into(), order: 1 },
        SetupStepDef { id: "workspace".into(), order: 2 },
        SetupStepDef { id: "defaults".into(), order: 3 },
        SetupStepDef { id: "python".into(), order: 4 },
        SetupStepDef { id: "stop".into(), order: 5 },
        SetupStepDef { id: "installer".into(), order: 6 },
        SetupStepDef { id: "finish".into(), order: 7 },
    ]
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SetupState {
    pub completed_steps: Vec<String>,
}

fn state_path(app_handle: &tauri::AppHandle) -> PathBuf {
    let dir = app_handle
        .path_resolver()
        .app_config_dir()
        .expect("could not resolve app config dir");
    fs::create_dir_all(&dir).ok();
    dir.join("setup_state.json")
}

pub fn load_state(app_handle: &tauri::AppHandle) -> SetupState {
    let path = state_path(app_handle);
    match fs::read_to_string(&path) {
        Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
        Err(_) => SetupState::default(),
    }
}

pub fn save_state(app_handle: &tauri::AppHandle, state: &SetupState) {
    let path = state_path(app_handle);
    if let Ok(data) = serde_json::to_string_pretty(state) {
        let _ = fs::write(path, data);
    }
}

/// Steps this user hasn't completed yet, in display order.
/// Empty means setup is fully done — the app should show the main shell.
pub fn pending_steps(app_handle: &tauri::AppHandle) -> Vec<SetupStepDef> {
    let state = load_state(app_handle);
    let mut steps: Vec<SetupStepDef> = step_registry()
        .into_iter()
        .filter(|s| !state.completed_steps.contains(&s.id))
        .filter(|s| {
            if s.id == "defaults" && crate::config::load_session_defaults(app_handle).is_some() {
                false
            } else {
                true
            }
        })
        .collect();
    steps.sort_by_key(|s| s.order);
    steps
}

pub fn mark_complete(app_handle: &tauri::AppHandle, step_id: &str) {
    let mut state = load_state(app_handle);
    if !state.completed_steps.iter().any(|s| s == step_id) {
        state.completed_steps.push(step_id.to_string());
    }
    save_state(app_handle, &state);
}
