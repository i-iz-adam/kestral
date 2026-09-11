use serde_json::{json, Value};

use crate::agent::{self, PendingApprovals};
use crate::config;
use crate::github;
use crate::omniroute::{self, ChatMessage};
use crate::prompts;
use crate::sessions::Session;
use crate::skills;
use crate::tools;

/// Sub-agents get fewer steps than the top-level loop (agent::MAX_STEPS) —
/// they're meant for bounded, focused tasks, not open-ended work.
const SUBAGENT_MAX_STEPS: u32 = 10;

/// The tool schema exposed to the parent agent. Deliberately just one tool:
/// a task description in, a summary out. What the sub-agent does with that
/// task — which files it reads, what it runs — stays inside its own
/// isolated context and never touches the parent's.
pub fn tool_definitions() -> Value {
    json!([
        {
            "type": "function",
            "function": {
                "name": "delegate_to_subagent",
                "description": "Delegate a bounded, self-contained task to a fresh sub-agent with its own context window. You get back only its final summary — not its full working transcript — which is exactly the point: use this to keep your own context focused on decisions rather than filled with intermediate file contents and command output you don't need verbatim. The sub-agent starts with no conversation history, only the task text you give it, so make that task self-contained.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "task": {
                            "type": "string",
                            "description": "A clear, self-contained description of what the sub-agent should do and what it should report back."
                        }
                    },
                    "required": ["task"]
                }
            }
        }
    ])
}

/// Runs a sub-agent to completion and returns its final summary text as
/// the tool result the parent loop will see. The sub-agent's own message
/// history (`messages` below) is entirely local to this function — it is
/// never merged into or persisted onto the parent session, which is the
/// whole mechanism by which this keeps the parent's context lean.
pub(crate) async fn run(
    app_handle: &tauri::AppHandle,
    approvals: &PendingApprovals,
    session: &Session,
    parent_call_id: &str,
    task: &str,
) -> Result<String, String> {
    let cfg = config::load_omniroute_config(app_handle)
        .ok_or("No OmniRoute config saved yet — finish setup first")?;

    let mut messages = vec![
        ChatMessage {
            role: "system".into(),
            content: Some(prompts::SUBAGENT_SYSTEM_PROMPT.to_string()),
            ..Default::default()
        },
        ChatMessage {
            role: "user".into(),
            content: Some(task.to_string()),
            ..Default::default()
        },
    ];

    // Same tool surface as the parent, minus delegate_to_subagent itself —
    // sub-agents don't spawn further sub-agents. One level of nesting only.
    let mut tool_list: Vec<Value> = tools::tool_definitions().as_array().cloned().unwrap_or_default();
    tool_list.extend(skills::tool_definitions().as_array().cloned().unwrap_or_default());
    if github::load_token(app_handle).is_some() {
        tool_list.extend(github::tool_definitions().as_array().cloned().unwrap_or_default());
    }
    let tools_value = Value::Array(tool_list);

    for _ in 0..SUBAGENT_MAX_STEPS {
        let assistant_msg =
            omniroute::chat_completion(&cfg, agent::MODEL, &messages, Some(&tools_value)).await?;
        messages.push(assistant_msg.clone());

        let tool_calls = assistant_msg.tool_calls.clone().unwrap_or_default();
        if tool_calls.is_empty() {
            return Ok(assistant_msg.content.unwrap_or_default());
        }

        for call in &tool_calls {
            if call.function.name == "delegate_to_subagent" {
                // Defensive only — this tool is never in tools_value above,
                // so a well-behaved model won't request it. Refuse cleanly
                // rather than recursing if one somehow does.
                messages.push(ChatMessage {
                    role: "tool".into(),
                    content: Some("Sub-agents cannot delegate further.".into()),
                    tool_call_id: Some(call.id.clone()),
                    name: Some(call.function.name.clone()),
                    ..Default::default()
                });
                continue;
            }

            let tool_msg = agent::handle_tool_call(
                app_handle,
                approvals,
                session,
                &session.id,
                call,
                Some(parent_call_id),
            )
            .await;
            messages.push(tool_msg);
        }
    }

    Err("Sub-agent hit its step limit without finishing".into())
}
