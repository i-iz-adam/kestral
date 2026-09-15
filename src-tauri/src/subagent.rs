use serde_json::{json, Value};
use std::sync::Arc;

use crate::agent::{self, PendingApprovals, StopRequests, SessionStop, LoopDetector};
use crate::config;
use crate::context;
use crate::github;
use crate::omniroute::{self, ChatMessage};
use crate::plan;
use crate::prompts;
use crate::sessions::Session;
use crate::skills;
use crate::tools;

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
                "description": "Delegate a bounded task to a fresh sub-agent with its own context window — including making edits, running commands, or committing, not just reading and reporting back. You get only its final summary, not its full working transcript, which is the point: this is the default way substantial work gets done in this session, keeping your own context focused on decisions rather than filled with intermediate file contents and command output you don't need verbatim. The sub-agent starts with no conversation history, only the task text you give it, so make that task self-contained — include exact file paths, conventions, or context it would otherwise have to rediscover.",
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
    stops: &StopRequests,
    stop_flag: Arc<SessionStop>,
    session: &Session,
    parent_call_id: &str,
    task: &str,
) -> Result<String, String> {
    let cfg = config::load_omniroute_config(app_handle)
        .ok_or("No OmniRoute config saved yet — finish setup first")?;
    let model = agent::effective_model(&cfg);

    let mut messages = vec![
        ChatMessage {
            role: "system".into(),
            content: Some(prompts::SUBAGENT_SYSTEM_PROMPT.to_string()),
            ..Default::default()
        },
    ];

    // Same auto-loading the top-level turn does (see agent.rs::run_turn_inner)
    // keyed off the sub-agent's own task text, since that's this loop's
    // equivalent of a user message — a sub-agent asked to "add tests for
    // the parser" should get the testing skill without needing to
    // remember list_skills/read_skill exist any more than the parent does.
    // Each unique skill is loaded into the sub-agent context at most once.
    let workspace = if session.workspace.trim().is_empty() { None } else { Some(session.workspace.as_str()) };
    let mut subagent_loaded_skills = std::collections::HashSet::new();
    let mut subagent_matched = skills::find_relevant(app_handle, task, workspace);
    let already: std::collections::HashSet<String> = subagent_matched.iter().map(|s| s.id.clone()).collect();
    subagent_matched.extend(skills::find_relevant_ai(app_handle, &cfg, workspace, task, &already).await);
    for skill in subagent_matched {
        if subagent_loaded_skills.insert(skill.id.clone()) {
            if let Some(content) = skills::get_content(app_handle, &skill.id, workspace) {
                agent::emit_skill_loaded(app_handle, &session.id, &skill, Some(parent_call_id));
                messages.push(ChatMessage {
                    role: "system".into(),
                    content: Some(format!("Relevant skill — {}:\n\n{}", skill.name, content)),
                    ..Default::default()
                });
            }
        }
    }
    if let Some(ws) = workspace {
        if let Some(agents_md) = skills::read_agents_md(ws) {
            messages.push(ChatMessage {
                role: "system".into(),
                content: Some(format!("This project's AGENTS.md:\n\n{}", agents_md)),
                ..Default::default()
            });
        }
    }

    // Same durable plan the parent (and any sibling sub-agent) sees — see
    // plan.rs — so a sub-agent picking up mid-task on a long-running job
    // knows what's already been marked done rather than re-deriving it
    // from scratch, and can check steps off as it completes them too.
    if let Some(plan_text) = plan::render(&plan::load(app_handle, &session.id)) {
        messages.push(ChatMessage { role: "system".into(), content: Some(plan_text), ..Default::default() });
    }

    messages.push(ChatMessage {
        role: "user".into(),
        content: Some(task.to_string()),
        ..Default::default()
    });

    // Same tool surface as the parent, minus delegate_to_subagent itself —
    // sub-agents don't spawn further sub-agents. One level of nesting only.
    let mut tool_list: Vec<Value> = tools::tool_definitions().as_array().cloned().unwrap_or_default();
    tool_list.extend(skills::tool_definitions().as_array().cloned().unwrap_or_default());
    tool_list.extend(plan::tool_definitions().as_array().cloned().unwrap_or_default());
    if github::load_token(app_handle).is_some() {
        tool_list.extend(github::tool_definitions().as_array().cloned().unwrap_or_default());
    }
    let tools_value = Value::Array(tool_list);

    let graceful = session.graceful_stop;

    // Use the same loop detection as the main agent to catch infinite loops
    let mut loop_detector = LoopDetector::new(20, 5);
    let mut step_count: u64 = 0;

    loop {
        if stop_flag.is_requested() {
            if !graceful {
                return Ok("Stopped: interrupted before completing the task".to_string());
            }
            if messages.iter().any(|m| m.role == "user") && messages.iter().any(|m| m.role == "assistant") {
                let overview_prompt = "The parent agent's chat was stopped while you were working. \
                    Write a concise overview of what you've done so far on this task and what remains, \
                    so the work isn't lost when the chat is continued. Don't run any tools — just \
                    summarize from the conversation above, in plain text.";
                messages.push(ChatMessage {
                    role: "user".into(),
                    content: Some(overview_prompt.to_string()),
                    ..Default::default()
                });
                let resp = omniroute::chat_completion(&cfg, model, &messages, None).await;
                if let Ok(resp) = resp {
                    if let Some(text) = resp.content {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            return Ok(trimmed.to_string());
                        }
                    }
                }
            }
            return Ok("Stopped: interrupted before completing the task".to_string());
        }

        step_count += 1;

        // A delegated task can itself run for hundreds of steps (see
        // context.rs) — same per-step budget check the top-level loop
        // does, just against this sub-agent's own local `messages`.
        context::maybe_compact(&cfg, &mut messages, false).await;

        let assistant_msg = match omniroute::chat_completion(&cfg, model, &messages, Some(&tools_value)).await {
            Ok(m) => m,
            Err(e) if context::is_context_length_error(&e) => {
                if context::maybe_compact(&cfg, &mut messages, true).await {
                    omniroute::chat_completion(&cfg, model, &messages, Some(&tools_value)).await?
                } else {
                    return Err(e);
                }
            }
            Err(e) => return Err(e),
        };
        messages.push(assistant_msg.clone());

        if let Some(ref content) = assistant_msg.content {
            let trimmed = content.trim();
            if !trimmed.is_empty() {
                let thought_id = format!("thought-{}", uuid::Uuid::new_v4());
                agent::emit_tool_event(
                    app_handle,
                    &session.id,
                    &thought_id,
                    "__subagent_thought__",
                    "done",
                    None,
                    Some(trimmed.to_string()),
                    Some(parent_call_id),
                );
            }
        }

        let tool_calls = assistant_msg.tool_calls.clone().unwrap_or_default();
        if tool_calls.is_empty() {
            return Ok(assistant_msg.content.unwrap_or_default());
        }

        let mut call_signatures: Vec<(String, String)> = Vec::with_capacity(tool_calls.len());
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
                stops,
                stop_flag.clone(),
                session,
                &session.id,
                call,
                Some(parent_call_id),
            )
            .await;

            call_signatures.push((call.function.name.clone(), call.function.arguments.clone()));
            messages.push(tool_msg);

            if stop_flag.is_requested() {
                break;
            }
        }

        // One signature per turn (reasoning text + every call it made this
        // turn), same approach as the top-level agent loop.
        let thought = assistant_msg.content.as_deref().unwrap_or("");
        loop_detector.record_step(thought, &call_signatures);

        // Detect infinite loops: the exact same reasoning and the exact
        // same tool call(s) repeating verbatim, several turns running.
        if loop_detector.is_looping() {
            return Err(format!(
                "Sub-agent stopped after {} steps: infinite loop detected (same thinking and tool call repeating with no progress).",
                step_count
            ));
        }

        // Safety net: absolute maximum step limit
        if step_count >= 500 {
            return Err(format!(
                "Sub-agent stopped after {} steps: maximum step limit reached.",
                step_count
            ));
        }
    }
}
