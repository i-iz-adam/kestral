use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDefaults {
    pub planning_enabled: bool,
    pub subagents_enabled: bool,
}

impl Default for SessionDefaults {
    fn default() -> Self {
        Self {
            planning_enabled: true,
            subagents_enabled: true,
        }
    }
}

pub fn save_session_defaults(
    app_handle: &tauri::AppHandle,
    defaults: &SessionDefaults,
) -> std::io::Result<()> {
    let path = app_config_dir(app_handle).join("session_defaults.json");
    let data = serde_json::to_string_pretty(defaults)?;
    fs::write(path, data)
}

pub fn load_session_defaults(app_handle: &tauri::AppHandle) -> Option<SessionDefaults> {
    let path = app_config_dir(app_handle).join("session_defaults.json");
    fs::read_to_string(path)
        .ok()
        .and_then(|d| serde_json::from_str(&d).ok())
}

pub fn get_session_defaults_or_default(app_handle: &tauri::AppHandle) -> SessionDefaults {
    load_session_defaults(app_handle).unwrap_or_default()
}

/// How this install talks to OmniRoute for LLM calls.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OmniRouteConfig {
    /// "local" or "remote"
    pub mode: String,
    pub remote_url: Option<String>,
    pub api_key: Option<String>,
    /// The hosted-tool type string OmniRoute expects to turn on web search
    /// for a request (e.g. "web_search") — sent as an extra `{"type": ...}`
    /// entry in the `tools` array alongside our own function tools when
    /// set. Left blank by default: this app doesn't have OmniRoute's own
    /// tool-calling contract on hand, so rather than guess at (and risk
    /// silently misusing) a hard-coded value, this is exposed as a plain
    /// field in Providers for the person to fill in from OmniRoute's own
    /// docs for whatever they have it routing to.
    #[serde(default)]
    pub web_search_tool: Option<String>,
}

fn app_config_dir(app_handle: &tauri::AppHandle) -> PathBuf {
    let dir = app_handle
        .path_resolver()
        .app_config_dir()
        .expect("could not resolve app config dir");
    fs::create_dir_all(&dir).ok();
    dir
}

pub fn save_omniroute_config(
    app_handle: &tauri::AppHandle,
    config: &OmniRouteConfig,
) -> std::io::Result<()> {
    let path = app_config_dir(app_handle).join("omniroute_config.json");
    let data = serde_json::to_string_pretty(config)?;
    fs::write(path, data)
}

pub fn load_omniroute_config(app_handle: &tauri::AppHandle) -> Option<OmniRouteConfig> {
    let path = app_config_dir(app_handle).join("omniroute_config.json");
    fs::read_to_string(path)
        .ok()
        .and_then(|d| serde_json::from_str(&d).ok())
}

pub fn save_workspace_path(app_handle: &tauri::AppHandle, path: &str) -> std::io::Result<()> {
    let dir = app_config_dir(app_handle);
    let data = serde_json::json!({ "path": path }).to_string();
    fs::write(dir.join("workspace.json"), data)
}

pub fn load_workspace_path(app_handle: &tauri::AppHandle) -> Option<String> {
    let dir = app_config_dir(app_handle);
    let data = fs::read_to_string(dir.join("workspace.json")).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&data).ok()?;
    parsed.get("path")?.as_str().map(|s| s.to_string())
}

/// A folder the agent can work from. Sessions each pick one at creation
/// time (and can switch later) instead of the whole app being pinned to
/// a single directory chosen once during setup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub path: String,
}

fn workspaces_path(app_handle: &tauri::AppHandle) -> PathBuf {
    app_config_dir(app_handle).join("workspaces.json")
}

fn folder_name(path: &str) -> String {
    PathBuf::from(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

fn write_workspaces(app_handle: &tauri::AppHandle, list: &[Workspace]) -> std::io::Result<()> {
    let data = serde_json::to_string_pretty(list)?;
    fs::write(workspaces_path(app_handle), data)
}

/// All known workspaces, oldest first. The very first time this runs for
/// an install that already went through the old single-folder setup step,
/// it transparently migrates that one path into a one-entry list and
/// persists it — nothing for existing users to redo.
pub fn list_workspaces(app_handle: &tauri::AppHandle) -> Vec<Workspace> {
    if let Ok(data) = fs::read_to_string(workspaces_path(app_handle)) {
        if let Ok(list) = serde_json::from_str::<Vec<Workspace>>(&data) {
            return list;
        }
    }
    if let Some(path) = load_workspace_path(app_handle) {
        let migrated = vec![Workspace {
            id: uuid::Uuid::new_v4().to_string(),
            name: folder_name(&path),
            path,
        }];
        let _ = write_workspaces(app_handle, &migrated);
        return migrated;
    }
    vec![]
}

/// Adds a folder to the list (or returns the existing entry unchanged if
/// that exact path is already known, so re-browsing to the same folder
/// from two different sessions doesn't create duplicate entries).
pub fn add_workspace(
    app_handle: &tauri::AppHandle,
    name: Option<String>,
    path: String,
) -> Result<Workspace, String> {
    let mut list = list_workspaces(app_handle);
    if let Some(existing) = list.iter().find(|w| w.path == path) {
        return Ok(existing.clone());
    }
    let workspace = Workspace {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| folder_name(&path)),
        path,
    };
    list.push(workspace.clone());
    write_workspaces(app_handle, &list).map_err(|e| e.to_string())?;
    Ok(workspace)
}

pub fn remove_workspace(app_handle: &tauri::AppHandle, id: &str) -> Result<(), String> {
    let mut list = list_workspaces(app_handle);
    list.retain(|w| w.id != id);
    write_workspaces(app_handle, &list).map_err(|e| e.to_string())
}

/// How the app launches OmniRoute as a managed child process. The default
/// (`npx -y omniroute`) is a fallback/override path — once a local install
/// exists (see engine::install), `use_local_install` makes the app prefer
/// spawning that directly instead, which is both faster (no npx resolution
/// on every launch) and version-pinned (npx -y always re-resolves latest).
/// `--no-open`/`--no-tray` matter here specifically because this app embeds
/// OmniRoute's dashboard itself (see the Providers page) and has no use for
/// OmniRoute popping its own browser tab or tray icon on every launch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    pub command: String,
    pub args: Vec<String>,
    pub auto_start: bool,
    #[serde(default)]
    pub use_local_install: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        EngineConfig {
            command: "npx".to_string(),
            args: vec![
                "-y".to_string(),
                "omniroute".to_string(),
                "--no-open".to_string(),
                "--no-tray".to_string(),
            ],
            auto_start: true,
            use_local_install: false,
        }
    }
}

pub fn load_engine_config(app_handle: &tauri::AppHandle) -> EngineConfig {
    let path = app_config_dir(app_handle).join("engine_config.json");
    fs::read_to_string(path)
        .ok()
        .and_then(|d| serde_json::from_str(&d).ok())
        .unwrap_or_default()
}

pub fn save_engine_config(app_handle: &tauri::AppHandle, cfg: &EngineConfig) -> std::io::Result<()> {
    let path = app_config_dir(app_handle).join("engine_config.json");
    fs::write(path, serde_json::to_string_pretty(cfg)?)
}

/// Whether the app should launch OmniRoute itself on startup — true only
/// when the connection mode is "local" (a "remote" install has nothing
/// local to launch) and the user hasn't turned auto-start off.
pub fn should_auto_start_engine(app_handle: &tauri::AppHandle) -> bool {
    let is_local = load_omniroute_config(app_handle)
        .map(|c| c.mode == "local")
        .unwrap_or(false);
    is_local && load_engine_config(app_handle).auto_start
}
