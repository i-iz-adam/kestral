use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::Manager;
use tokio::sync::oneshot;

use crate::config;
use crate::github;
use crate::omniroute::{self, ChatMessage, ToolCall};
use crate::prompts;
use crate::sessions::{self, Session};
use crate::skills;
use crate::subagent;
use crate::tools;

/// Tool calls waiting on user approval (planning mode), keyed by the
/// model's tool_call id. A pending call blocks its turn's loop on the
/// receiving end of a oneshot channel; `approve_tool_call` resolves it.
/// Shared globally (not per-session) since call ids are unique regardless
/// of whether they came from the top-level loop or a sub-agent's loop.
#[derive(Default)]
pub struct PendingApprovals(pub Mutex<HashMap<String, oneshot::Sender<bool>>>);

#[derive(Clone, Serialize)]
pub(crate) struct ToolEvent<'a> {
    session_id: &'a str,
    call_id: &'a str,
    name: &'a str,
    /// "start" | "awaiting-approval" | "done" | "error"
    status: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    args: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<String>,
    /// Set when this call happened inside a sub-agent spawned by a
    /// delegate_to_subagent call — the value is that call's own id, so the
    /// frontend can nest this event under the right card instead of
    /// showing it as a top-level step.
    #[serde(skip_serializing_if = "Option::is_none")]
    parent_call_id: Option<&'a str>,
}

#[derive(Clone, Serialize)]
struct MessageEvent<'a> {
    session_id: &'a str,
    role: &'a str,
    content: &'a str,
}

pub(crate) const MAX_STEPS: u32 = 12;
pub(crate) const MODEL: &str = "auto/coding";

pub(crate) fn is_mutating(name: &str) -> bool {
    tools::is_mutating(name) || github::is_mutating(name)
}

pub(crate) fn emit_tool_event(
    app_handle: &tauri::AppHandle,
    session_id: &str,
    call_id: &str,
    name: &str,
    status: &str,
    args: Option<Value>,
    result: Option<String>,
    parent_call_id: Option<&str>,
) {
    let _ = app_handle.emit_all(
        "agent://tool-call",
        ToolEvent { session_id, call_id, name, status, args, result, parent_call_id },
    );
}

/// Routes one tool call to whichever module handles it: delegate_to_subagent
/// spawns a nested agent loop with its own isolated context, skills tools
/// are synchronous and local, github_* tools hit the GitHub API, everything
/// else is a workspace file/shell tool.
pub(crate) async fn execute_tool(
    app_handle: &tauri::AppHandle,
    approvals: &PendingApprovals,
    session: &Session,
    call_id: &str,
    name: &str,
    args: &Value,
) -> Result<String, String> {
    if name == "delegate_to_subagent" {
        let task = args.get("task").and_then(|v| v.as_str()).ok_or("missing task")?;
        // Boxed to break the async recursion cycle:
        // execute_tool -> subagent::run -> handle_tool_call -> execute_tool.
        return Box::pin(subagent::run(app_handle, approvals, session, call_id, task)).await;
    }
    if let Some(result) = skills::maybe_execute(app_handle, name, args) {
        return result;
    }
    if name.starts_with("github_") {
        let token = github::load_token(app_handle)
            .ok_or("GitHub is not connected — add a token in the GitHub tab first")?;
        return github::execute(&token, session.linked_repo.as_deref(), name, args).await;
    }
    tools::execute(&session.workspace, name, args)
}

/// Handles one tool call end to end: emits the "start" event, gates behind
/// planning-mode approval if the tool mutates something, executes it, and
/// emits the "done"/"error" event. Shared between the top-level loop and
/// sub-agent loops (see subagent.rs) so the approval flow — and what the
/// UI sees — can't drift between the two.
pub(crate) async fn handle_tool_call(
    app_handle: &tauri::AppHandle,
    approvals: &PendingApprovals,
    session: &Session,
    session_id: &str,
    call: &ToolCall,
    parent_call_id: Option<&str>,
) -> ChatMessage {
    let args: Value = serde_json::from_str(&call.function.arguments).unwrap_or(Value::Null);

    emit_tool_event(
        app_handle, session_id, &call.id, &call.function.name, "start",
        Some(args.clone()), None, parent_call_id,
    );

    if session.planning_enabled && is_mutating(&call.function.name) {
        let (tx, rx) = oneshot::channel::<bool>();
        approvals.0.lock().unwrap().insert(call.id.clone(), tx);

        emit_tool_event(
            app_handle, session_id, &call.id, &call.function.name, "awaiting-approval",
            Some(args.clone()), None, parent_call_id,
        );

        let approved = rx.await.unwrap_or(false);
        if !approved {
            let result = "Rejected by user.".to_string();
            emit_tool_event(
                app_handle, session_id, &call.id, &call.function.name, "error",
                None, Some(result.clone()), parent_call_id,
            );
            return ChatMessage {
                role: "tool".into(),
                content: Some(result),
                tool_call_id: Some(call.id.clone()),
                name: Some(call.function.name.clone()),
                ..Default::default()
            };
        }
    }

    let result = execute_tool(app_handle, approvals, session, &call.id, &call.function.name, &args)
        .await
        .unwrap_or_else(|e| format!("error: {}", e));

    emit_tool_event(
        app_handle, session_id, &call.id, &call.function.name, "done",
        None, Some(result.clone()), parent_call_id,
    );

    ChatMessage {
        role: "tool".into(),
        content: Some(result),
        tool_call_id: Some(call.id.clone()),
        name: Some(call.function.name.clone()),
        ..Default::default()
    }
}

pub async fn run_turn(
    app_handle: tauri::AppHandle,
    approvals: tauri::State<'_, PendingApprovals>,
    session_id: String,
    user_message: String,
) -> Result<(), String> {
    let cfg = config::load_omniroute_config(&app_handle)
        .ok_or("No OmniRoute config saved yet — finish setup first")?;

    let mut session =
        sessions::load(&app_handle, &session_id).ok_or("Session not found")?;

    session.messages.push(ChatMessage {
        role: "user".into(),
        content: Some(user_message.clone()),
        ..Default::default()
    });
    let _ = app_handle.emit_all(
        "agent://message",
        MessageEvent { session_id: &session_id, role: "user", content: &user_message },
    );

    // System prompt is built fresh each turn rather than persisted into
    // session.messages, so improving it later doesn't require migrating
    // old sessions. The delegation addendum is only appended when the
    // subagent tool is actually in this session's tool list — no point
    // telling the model about a tool it can't see.
    let system_prompt = if session.mode == "general" {
        prompts::GENERAL_SYSTEM_PROMPT.to_string()
    } else if session.subagents_enabled {
        format!("{}\n\n{}", prompts::CODING_SYSTEM_PROMPT, prompts::SUBAGENT_DELEGATION_ADDENDUM)
    } else {
        prompts::CODING_SYSTEM_PROMPT.to_string()
    };

    let tools_schema = if session.mode == "general" {
        None
    } else {
        let mut all: Vec<Value> = tools::tool_definitions()
            .as_array()
            .cloned()
            .unwrap_or_default();
        all.extend(skills::tool_definitions().as_array().cloned().unwrap_or_default());
        if github::load_token(&app_handle).is_some() {
            all.extend(github::tool_definitions().as_array().cloned().unwrap_or_default());
        }
        if session.subagents_enabled {
            all.extend(subagent::tool_definitions().as_array().cloned().unwrap_or_default());
        }
        Some(Value::Array(all))
    };

    for _ in 0..MAX_STEPS {
        let mut request_messages = vec![ChatMessage {
            role: "system".into(),
            content: Some(system_prompt.clone()),
            ..Default::default()
        }];
        request_messages.extend(session.messages.clone());

        let assistant_msg = omniroute::chat_completion(
            &cfg,
            MODEL,
            &request_messages,
            tools_schema.as_ref(),
        )
        .await?;

        session.messages.push(assistant_msg.clone());

        let tool_calls = assistant_msg.tool_calls.clone().unwrap_or_default();

        if tool_calls.is_empty() {
            let text = assistant_msg.content.clone().unwrap_or_default();
            let _ = app_handle.emit_all(
                "agent://message",
                MessageEvent { session_id: &session_id, role: "assistant", content: &text },
            );
            sessions::save(&app_handle, &session);
            return Ok(());
        }

        for call in &tool_calls {
            let tool_msg = handle_tool_call(
                &app_handle,
                approvals.inner(),
                &session,
                &session_id,
                call,
                None,
            )
            .await;
            session.messages.push(tool_msg);
        }

        sessions::save(&app_handle, &session);
    }

    Err("Hit the step limit for this turn (12 tool rounds) without a final answer".into())
}

pub fn resolve_approval(approvals: &PendingApprovals, call_id: &str, approved: bool) {
    if let Some(tx) = approvals.0.lock().unwrap().remove(call_id) {
        let _ = tx.send(approved);
    }
}
