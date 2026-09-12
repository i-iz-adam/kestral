#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agent;
mod config;
mod engine;
mod github;
mod omniroute;
mod prompts;
mod sessions;
mod setup;
mod skills;
mod subagent;
mod tools;

// ---- setup wizard ----

#[tauri::command]
fn get_pending_setup_steps(app_handle: tauri::AppHandle) -> Vec<setup::SetupStepDef> {
    setup::pending_steps(&app_handle)
}

#[tauri::command]
fn complete_setup_step(app_handle: tauri::AppHandle, step_id: String) {
    setup::mark_complete(&app_handle, &step_id);
}

// ---- config ----

#[tauri::command]
fn save_omniroute_config(
    app_handle: tauri::AppHandle,
    config: config::OmniRouteConfig,
) -> Result<(), String> {
    config::save_omniroute_config(&app_handle, &config).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_omniroute_config(app_handle: tauri::AppHandle) -> Option<config::OmniRouteConfig> {
    config::load_omniroute_config(&app_handle)
}

#[tauri::command]
fn save_workspace_path(app_handle: tauri::AppHandle, path: String) -> Result<(), String> {
    config::save_workspace_path(&app_handle, &path).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_workspace_path(app_handle: tauri::AppHandle) -> Option<String> {
    config::load_workspace_path(&app_handle)
}

#[tauri::command]
async fn test_omniroute_connection(config: config::OmniRouteConfig) -> Result<bool, String> {
    let base = match config.mode.as_str() {
        "local" => "http://127.0.0.1:20128".to_string(),
        _ => config
            .remote_url
            .clone()
            .unwrap_or_default()
            .trim_end_matches('/')
            .to_string(),
    };
    if base.is_empty() {
        return Err("No URL configured".into());
    }
    let url = format!("{}/v1/models", base);
    let client = reqwest::Client::new();
    let mut req = client.get(&url);
    if let Some(key) = &config.api_key {
        if !key.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", key));
        }
    }
    match req.send().await {
        Ok(resp) => Ok(resp.status().is_success()),
        Err(e) => Err(e.to_string()),
    }
}

// ---- sessions + agent loop ----

#[tauri::command]
fn create_session(
    app_handle: tauri::AppHandle,
    title: String,
    mode: String,
    planning_enabled: bool,
    repo: Option<String>,
    subagents_enabled: bool,
) -> Result<sessions::Session, String> {
    let workspace =
        config::load_workspace_path(&app_handle).ok_or("No workspace configured yet")?;
    Ok(sessions::create(
        &app_handle,
        title,
        mode,
        workspace,
        planning_enabled,
        repo,
        subagents_enabled,
    ))
}

#[tauri::command]
fn set_session_repo(
    app_handle: tauri::AppHandle,
    id: String,
    repo: Option<String>,
) -> Result<(), String> {
    sessions::set_linked_repo(&app_handle, &id, repo)
}

#[tauri::command]
fn set_session_subagents(
    app_handle: tauri::AppHandle,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    sessions::set_subagents_enabled(&app_handle, &id, enabled)
}

#[tauri::command]
fn set_session_planning(
    app_handle: tauri::AppHandle,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    sessions::set_planning_enabled(&app_handle, &id, enabled)
}

#[tauri::command]
fn approve_all_pending(
    approvals: tauri::State<'_, agent::PendingApprovals>,
    session_id: String,
    approved: bool,
) -> usize {
    agent::approve_all_pending(&approvals, &session_id, approved)
}

#[tauri::command]
fn list_sessions(app_handle: tauri::AppHandle) -> Vec<sessions::Session> {
    sessions::list(&app_handle)
}

#[tauri::command]
fn get_session(app_handle: tauri::AppHandle, id: String) -> Option<sessions::Session> {
    sessions::load(&app_handle, &id)
}

#[tauri::command]
fn delete_session(app_handle: tauri::AppHandle, id: String) {
    sessions::delete(&app_handle, &id);
}

#[tauri::command]
async fn send_message(
    app_handle: tauri::AppHandle,
    approvals: tauri::State<'_, agent::PendingApprovals>,
    session_id: String,
    message: String,
) -> Result<(), String> {
    agent::run_turn(app_handle, approvals, session_id, message).await
}

#[tauri::command]
fn approve_tool_call(
    approvals: tauri::State<'_, agent::PendingApprovals>,
    call_id: String,
    approved: bool,
) {
    agent::resolve_approval(&approvals, &call_id, approved);
}

// ---- github ----

#[tauri::command]
fn save_github_token(app_handle: tauri::AppHandle, token: String) -> Result<(), String> {
    github::save_token(&app_handle, &token).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_github_token(app_handle: tauri::AppHandle) -> Option<String> {
    github::load_token(&app_handle)
}

#[tauri::command]
async fn test_github_token(token: String) -> Result<String, String> {
    github::test_token(&token).await
}

/// Single entry point for any github_* tool, callable directly from the UI
/// (tool cards) using the exact same dispatch the agent loop uses — one
/// source of truth for what each GitHub action does.
#[tauri::command]
async fn github_action(
    app_handle: tauri::AppHandle,
    name: String,
    args: serde_json::Value,
    linked_repo: Option<String>,
) -> Result<serde_json::Value, String> {
    let token = github::load_token(&app_handle).ok_or("GitHub is not connected")?;
    let raw = github::execute(&token, linked_repo.as_deref(), &name, &args).await?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

// ---- skills ----

#[tauri::command]
fn list_skills(app_handle: tauri::AppHandle) -> Vec<skills::Skill> {
    skills::list(&app_handle)
}

#[tauri::command]
fn get_skill_content(app_handle: tauri::AppHandle, id: String) -> Option<String> {
    skills::get_content(&app_handle, &id)
}

#[tauri::command]
fn toggle_skill(app_handle: tauri::AppHandle, id: String, enabled: bool) {
    skills::toggle(&app_handle, &id, enabled);
}

#[tauri::command]
fn delete_skill(app_handle: tauri::AppHandle, id: String) -> Result<(), String> {
    skills::delete(&app_handle, &id)
}

#[tauri::command]
async fn install_skill_from_url(
    app_handle: tauri::AppHandle,
    url: String,
) -> Result<skills::Skill, String> {
    skills::install_from_url(&app_handle, &url).await
}

// ---- engine (managed OmniRoute process) ----

#[tauri::command]
fn get_engine_status(app_handle: tauri::AppHandle) -> serde_json::Value {
    let (status, error) = engine::status(&app_handle);
    serde_json::json!({ "status": status, "error": error })
}

#[tauri::command]
fn is_engine_installed(app_handle: tauri::AppHandle) -> bool {
    engine::is_installed(&app_handle)
}

#[tauri::command]
async fn install_engine(app_handle: tauri::AppHandle) {
    engine::install(app_handle).await;
}

#[tauri::command]
fn start_engine(app_handle: tauri::AppHandle) {
    engine::start(app_handle);
}

#[tauri::command]
fn stop_engine(app_handle: tauri::AppHandle) {
    engine::stop(&app_handle);
}

#[tauri::command]
fn confirm_engine_running(app_handle: tauri::AppHandle) {
    engine::confirm_running(&app_handle);
}

#[tauri::command]
fn get_engine_config(app_handle: tauri::AppHandle) -> config::EngineConfig {
    config::load_engine_config(&app_handle)
}

#[tauri::command]
fn save_engine_config(
    app_handle: tauri::AppHandle,
    command: String,
    args: Vec<String>,
    auto_start: bool,
) -> Result<(), String> {
    config::save_engine_config(
        &app_handle,
        &config::EngineConfig {
            command,
            args,
            auto_start,
            use_local_install: config::load_engine_config(&app_handle).use_local_install,
        },
    )
    .map_err(|e| e.to_string())
}

fn main() {
    tauri::Builder::default()
        .manage(agent::PendingApprovals::default())
        .manage(engine::EngineState::default())
        .setup(|app| {
            let app_handle = app.handle();
            if config::should_auto_start_engine(&app_handle) {
                engine::start(app_handle);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_pending_setup_steps,
            complete_setup_step,
            save_omniroute_config,
            get_omniroute_config,
            save_workspace_path,
            get_workspace_path,
            test_omniroute_connection,
            create_session,
            list_sessions,
            get_session,
            delete_session,
            set_session_repo,
            set_session_subagents,
            set_session_planning,
            approve_all_pending,
            send_message,
            approve_tool_call,
            save_github_token,
            get_github_token,
            test_github_token,
            github_action,
            list_skills,
            get_skill_content,
            toggle_skill,
            delete_skill,
            install_skill_from_url,
            get_engine_status,
            is_engine_installed,
            install_engine,
            start_engine,
            stop_engine,
            confirm_engine_running,
            get_engine_config,
            save_engine_config
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            // Make sure the managed OmniRoute process doesn't outlive the
            // app — without this, closing the window would leave an orphan
            // npx/node process running.
            if let tauri::RunEvent::ExitRequested { .. } = event {
                engine::stop(app_handle);
            }
        });
}
