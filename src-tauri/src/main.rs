#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agent;
mod config;
mod context;
mod engine;
mod github;
mod omniroute;
mod plan;
mod prompts;
mod reflect;
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
fn check_python_installed() -> tools::PythonStatus {
    tools::check_python_status()
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
fn get_session_defaults(app_handle: tauri::AppHandle) -> config::SessionDefaults {
    config::get_session_defaults_or_default(&app_handle)
}

#[tauri::command]
fn save_session_defaults(
    app_handle: tauri::AppHandle,
    defaults: config::SessionDefaults,
) -> Result<(), String> {
    config::save_session_defaults(&app_handle, &defaults).map_err(|e| e.to_string())?;
    setup::mark_complete(&app_handle, "defaults");
    Ok(())
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
fn list_workspaces(app_handle: tauri::AppHandle) -> Vec<config::Workspace> {
    config::list_workspaces(&app_handle)
}

#[tauri::command]
fn add_workspace(
    app_handle: tauri::AppHandle,
    name: Option<String>,
    path: String,
) -> Result<config::Workspace, String> {
    config::add_workspace(&app_handle, name, path)
}

#[tauri::command]
fn remove_workspace(app_handle: tauri::AppHandle, id: String) -> Result<(), String> {
    config::remove_workspace(&app_handle, &id)
}

#[tauri::command]
async fn fetch_omniroute_endpoint(
    app_handle: tauri::AppHandle,
    endpoint: String,
    method: Option<String>,
    body: Option<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let cfg = config::load_omniroute_config(&app_handle)
        .ok_or("No OmniRoute config saved yet — finish setup first")?;
    omniroute::fetch_endpoint(&cfg, &endpoint, method.as_deref(), body.as_ref()).await
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
    planning_enabled: Option<bool>,
    subagents_enabled: Option<bool>,
    graceful_stop: Option<bool>,
    workspace: Option<String>,
) -> Result<sessions::Session, String> {
    let defaults = config::get_session_defaults_or_default(&app_handle);
    let planning_enabled = planning_enabled.unwrap_or(defaults.planning_enabled);
    let subagents_enabled = subagents_enabled.unwrap_or(defaults.subagents_enabled);
    let graceful_stop = graceful_stop.unwrap_or(defaults.graceful_stop);

    // The picker in the sidebar always sends a workspace now; the fallback
    // chain here only matters for a stale frontend build or a very first
    // session created before any workspace has explicitly been chosen.
    let workspace = workspace
        .or_else(|| config::list_workspaces(&app_handle).first().map(|w| w.path.clone()))
        .or_else(|| config::load_workspace_path(&app_handle))
        .ok_or("No workspace configured yet")?;
    Ok(sessions::create(
        &app_handle,
        title,
        mode,
        workspace,
        planning_enabled,
        subagents_enabled,
        graceful_stop,
    ))
}

#[tauri::command]
fn set_session_workspace(
    app_handle: tauri::AppHandle,
    id: String,
    workspace: String,
) -> Result<(), String> {
    sessions::set_workspace(&app_handle, &id, workspace)
}

#[tauri::command]
fn set_session_title(
    app_handle: tauri::AppHandle,
    id: String,
    title: String,
) -> Result<(), String> {
    sessions::set_title(&app_handle, &id, title)
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
fn set_session_graceful_stop(
    app_handle: tauri::AppHandle,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    sessions::set_graceful_stop(&app_handle, &id, enabled)
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
fn set_session_sandbox_shell(
    app_handle: tauri::AppHandle,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    sessions::set_sandbox_shell(&app_handle, &id, enabled)
}

#[tauri::command]
fn set_session_sandbox_network(
    app_handle: tauri::AppHandle,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    sessions::set_sandbox_network(&app_handle, &id, enabled)
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
    stops: tauri::State<'_, agent::StopRequests>,
    session_id: String,
    message: String,
    images: Option<Vec<String>>,
) -> Result<(), String> {
    agent::run_turn(app_handle, approvals, stops, session_id, message, images).await
}

#[tauri::command]
async fn stop_session(
    _app_handle: tauri::AppHandle,
    approvals: tauri::State<'_, agent::PendingApprovals>,
    stops: tauri::State<'_, agent::StopRequests>,
    session_id: String,
) -> Result<(), String> {
    // Interrupt the running turn the same way the /auto slash command
    // clears pending approvals: resolve everything waiting so no loop is
    // stuck, then mark the session stop-requested so the loop's next
    // step boundary (and every live sub-agent's own loop) winds down and
    // hands back whatever it already got through.
    agent::approve_all_pending(&approvals, &session_id, false);
    stops.request(&session_id);
    Ok(())
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
    workspace: String,
) -> Result<serde_json::Value, String> {
    let token = github::load_token(&app_handle).ok_or("GitHub is not connected")?;
    let raw = github::execute(&token, &workspace, &name, &args).await?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}

// ---- skills ----

#[tauri::command]
fn list_skills(app_handle: tauri::AppHandle, workspace: Option<String>) -> Vec<skills::Skill> {
    skills::list(&app_handle, workspace.as_deref())
}

#[tauri::command]
fn get_skill_content(app_handle: tauri::AppHandle, id: String, workspace: Option<String>) -> Option<String> {
    skills::get_content(&app_handle, &id, workspace.as_deref())
}

#[tauri::command]
fn toggle_skill(app_handle: tauri::AppHandle, id: String, enabled: bool) {
    skills::toggle(&app_handle, &id, enabled);
}

#[tauri::command]
fn delete_skill(app_handle: tauri::AppHandle, id: String, workspace: Option<String>) -> Result<(), String> {
    skills::delete(&app_handle, &id, workspace.as_deref())
}

/// Whether the given workspace has an AGENTS.md at its root, and its
/// content if so — backs the Skills panel's AGENTS.md card.
#[tauri::command]
fn get_agents_md(workspace: String) -> Option<String> {
    skills::read_agents_md(&workspace)
}

#[tauri::command]
async fn install_skill_from_url(
    app_handle: tauri::AppHandle,
    url: String,
) -> Result<skills::Skill, String> {
    skills::install_from_url(&app_handle, &url).await
}

/// Direct skill authoring from the UI (the SkillsPanel's own "new skill" /
/// "edit skill" forms) — the exact same write_skill the agent's
/// create_skill/edit_skill tools call, so a skill a human writes by hand
/// and one the agent writes are indistinguishable once saved.
#[tauri::command]
fn create_skill(
    app_handle: tauri::AppHandle,
    id: Option<String>,
    name: String,
    description: String,
    content: String,
    triggers: Vec<String>,
    workspace: Option<String>,
    project: Option<bool>,
) -> Result<skills::Skill, String> {
    skills::write_skill(
        &app_handle,
        id.as_deref(),
        &name,
        &description,
        &content,
        &triggers,
        workspace.as_deref(),
        project.unwrap_or(false),
    )
}

#[derive(serde::Deserialize)]
struct SkillEditInput {
    old_string: String,
    new_string: String,
    #[serde(default)]
    replace_all: bool,
}

#[tauri::command]
fn edit_skill(
    app_handle: tauri::AppHandle,
    id: String,
    edits: Vec<SkillEditInput>,
    workspace: Option<String>,
) -> Result<String, String> {
    let parsed: Vec<(String, String, bool)> = edits
        .into_iter()
        .map(|e| (e.old_string, e.new_string, e.replace_all))
        .collect();
    skills::edit_skill(&app_handle, &id, &parsed, workspace.as_deref())
}

// ---- skill proposals (the reviewed half of the self-improvement loop) ----

#[tauri::command]
fn list_skill_proposals(app_handle: tauri::AppHandle) -> Vec<skills::SkillProposal> {
    skills::list_proposals(&app_handle)
}

#[tauri::command]
fn accept_skill_proposal(app_handle: tauri::AppHandle, id: String) -> Result<skills::Skill, String> {
    skills::accept_proposal(&app_handle, &id)
}

#[tauri::command]
fn reject_skill_proposal(app_handle: tauri::AppHandle, id: String) -> Result<(), String> {
    skills::reject_proposal(&app_handle, &id)
}

/// Lets the review UI tweak a proposal's content/name/description before
/// accepting it, without round-tripping through reject-then-recreate.
#[tauri::command]
fn update_skill_proposal(
    app_handle: tauri::AppHandle,
    id: String,
    name: String,
    description: String,
    content: String,
    triggers: Vec<String>,
) -> Result<(), String> {
    let mut proposal = skills::get_proposal(&app_handle, &id).ok_or("proposal not found")?;
    proposal.name = name;
    proposal.description = description;
    proposal.content = content;
    proposal.triggers = triggers;
    skills::propose(&app_handle, proposal).map(|_| ())
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
        .manage(agent::StopRequests::default())
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
            check_python_installed,
            get_session_defaults,
            save_session_defaults,
            save_omniroute_config,
            get_omniroute_config,
            save_workspace_path,
            get_workspace_path,
            list_workspaces,
            add_workspace,
            remove_workspace,
            fetch_omniroute_endpoint,
            test_omniroute_connection,
            create_session,
            list_sessions,
            get_session,
            delete_session,
            set_session_workspace,
            set_session_title,
            set_session_subagents,
            set_session_graceful_stop,
            set_session_planning,
            set_session_sandbox_shell,
            set_session_sandbox_network,
            approve_all_pending,
            send_message,
            stop_session,
            approve_tool_call,
            save_github_token,
            get_github_token,
            test_github_token,
            github_action,
            list_skills,
            get_skill_content,
            get_agents_md,
            toggle_skill,
            delete_skill,
            install_skill_from_url,
            create_skill,
            edit_skill,
            list_skill_proposals,
            accept_skill_proposal,
            reject_skill_proposal,
            update_skill_proposal,
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
