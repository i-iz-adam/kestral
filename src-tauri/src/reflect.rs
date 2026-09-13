//! The unattended half of the self-improvement loop.
//!
//! agent.rs's SKILL_AUTHORING_ADDENDUM covers the *attended* half: the
//! agent noticing something worth keeping mid-task and calling
//! create_skill/edit_skill/propose_skill itself. This module is the
//! fallback for everything that doesn't happen — the agent doesn't always
//! recognize a pattern as reusable while it's heads-down doing the work,
//! any more than a person mid-task always stops to write the runbook.
//!
//! After a turn that did real work (mutated files, ran commands, or
//! delegated to a sub-agent), and no more often than once every
//! REFLECT_EVERY_N_TURNS such turns, this runs a small separate model call
//! — deliberately using the fast/cheap model, the same one session-title
//! generation uses, since this never needs to be as capable as the agent
//! doing the actual work — over a condensed transcript of what just
//! happened, asking one question: does this reveal something durable and
//! reusable that isn't already covered by an existing skill?
//!
//! Whatever it decides never touches an active skill directly. It can only
//! ever call skills::propose(), which lands in the review queue —
//! SkillProposal — where a human has to explicitly accept it before it
//! affects any session, including this one. That boundary is the whole
//! safety property of this module: an agent's unsupervised, automatic
//! self-assessment of its own work is exactly the kind of judgment that
//! should require a second opinion before becoming part of its own future
//! instructions.

use serde::Deserialize;
use tauri::Manager;

use crate::agent;
use crate::config::OmniRouteConfig;
use crate::omniroute::{self, ChatMessage};
use crate::sessions::{self, Session};
use crate::skills::{self, SkillProposal};

/// How many qualifying turns (see maybe_reflect) to let pass between
/// reflection runs. Low enough that a genuinely long session still gets
/// reflected on a few times, high enough that a chatty back-and-forth
/// doesn't spend a model call on every single turn. Tune freely — there's
/// no correctness reason this needs to be exactly 3, just a cost/coverage
/// tradeoff.
const REFLECT_EVERY_N_TURNS: u32 = 3;

/// Keeps the transcript sent to the reflection model small: this is a
/// judgment call over what already happened, not a task that needs full
/// file contents or command output verbatim. Each included message is
/// truncated to this many characters.
const MAX_MESSAGE_CHARS: usize = 400;
/// How many of the most recent messages to consider at all — bounds both
/// cost and the chance of the reflection drifting to something from much
/// earlier in a long session rather than what this turn actually did.
const MAX_MESSAGES: usize = 30;

#[derive(Debug, Default, Deserialize)]
struct ReflectionResult {
    /// "none" | "create" | "update". Missing/unrecognized is treated as
    /// "none" — a malformed response should never accidentally propose
    /// something empty rather than just being silently ignored.
    #[serde(default)]
    action: String,
    #[serde(default)]
    target_id: Option<String>,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    triggers: Vec<String>,
    #[serde(default)]
    rationale: String,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// One line per message, role-tagged and truncated — enough for a
/// reviewer (human or model) to see the shape of what happened without
/// reproducing it verbatim. Tool call arguments and results are the most
/// likely to be huge (a whole file's contents, a long command output), so
/// they get the same truncation as everything else rather than a special
/// exemption.
fn condensed_transcript(session: &Session) -> String {
    let start = session.messages.len().saturating_sub(MAX_MESSAGES);
    let mut lines = Vec::new();

    for msg in &session.messages[start..] {
        let mut body = String::new();
        if let Some(content) = &msg.content {
            body.push_str(content);
        }
        if let Some(calls) = &msg.tool_calls {
            for call in calls {
                body.push_str(&format!(
                    " [called {}({})]",
                    call.function.name, call.function.arguments
                ));
            }
        }
        let truncated: String = body.chars().take(MAX_MESSAGE_CHARS).collect();
        if truncated.trim().is_empty() {
            continue;
        }
        lines.push(format!("{}: {}", msg.role, truncated));
    }

    lines.join("\n")
}

fn strip_code_fence(raw: &str) -> &str {
    let s = raw.trim();
    let s = s.strip_prefix("```json").or_else(|| s.strip_prefix("```")).unwrap_or(s);
    s.strip_suffix("```").unwrap_or(s).trim()
}

/// Called at the end of a successful, non-empty turn (see
/// agent::run_turn_inner). `tool_names_this_turn` is every tool name
/// called across every tool-round of the turn that just finished — used
/// only for the "did this turn do anything worth reflecting on" gate, not
/// sent to the model. Saves `session` itself (to persist the debounce
/// counter) exactly once, whether or not a reflection actually runs.
pub(crate) async fn maybe_reflect(
    app_handle: &tauri::AppHandle,
    cfg: &OmniRouteConfig,
    session: &mut Session,
    tool_names_this_turn: &[String],
) {
    let did_real_work = tool_names_this_turn
        .iter()
        .any(|n| agent::is_mutating(n) || n == "delegate_to_subagent");

    if !did_real_work {
        return;
    }

    session.turns_since_reflection += 1;
    if session.turns_since_reflection < REFLECT_EVERY_N_TURNS {
        sessions::save(app_handle, session);
        return;
    }
    session.turns_since_reflection = 0;
    sessions::save(app_handle, session);

    let transcript = condensed_transcript(session);
    if transcript.trim().is_empty() {
        return;
    }

    let skill_list = skills::list(app_handle)
        .into_iter()
        .filter(|s| s.enabled)
        .map(|s| format!("- {} ({}): {}", s.id, s.source, s.description))
        .collect::<Vec<_>>()
        .join("\n");

    let system = format!(
        "You review a snippet of a coding agent's recent work (a condensed transcript, roles and truncated content/tool-calls) and decide whether it revealed a durable, reusable pattern worth keeping as a skill for future sessions — a convention it had to dig for, a non-obvious gotcha, a repeatable multi-step procedure. Do NOT propose anything specific to this one file, this one bug, or this one user's one-off request; that's just the task, not a reusable skill. Do NOT propose anything already well covered by an existing skill below — if an existing skill is just slightly incomplete or wrong based on what you see, propose \"update\" against its id instead of \"create\".\n\nExisting skills:\n{}\n\nRespond with ONLY a JSON object, no prose, no markdown code fence, matching exactly this shape:\n{{\"action\": \"none\" | \"create\" | \"update\", \"target_id\": string or null (required, and must be one of the ids above, when action is \"update\"; always null for \"create\"), \"name\": string, \"description\": string (one line, what it covers and when it's relevant), \"content\": string (the full skill body in markdown — actual generalized instructions, not a transcript of this task), \"triggers\": string array (a few specific words/phrases that should auto-load this; can be empty), \"rationale\": string (why this is worth keeping)}}.\n\nUse action \"none\" — and leave the other fields empty — for anything not clearly reusable, which should be most of the time. Only propose something you're genuinely confident generalizes.",
        if skill_list.is_empty() { "(none yet)".to_string() } else { skill_list }
    );

    let messages = vec![
        ChatMessage { role: "system".into(), content: Some(system), ..Default::default() },
        ChatMessage { role: "user".into(), content: Some(transcript), ..Default::default() },
    ];

    let resp = match omniroute::chat_completion(cfg, "auto/fast", &messages, None).await {
        Ok(r) => r,
        // Reflection is best-effort background housekeeping, not part of
        // the user-facing turn — a failure here (rate limit, transient
        // network error, whatever) should never surface as a turn error.
        Err(_) => return,
    };

    let raw = match resp.content {
        Some(c) if !c.trim().is_empty() => c,
        _ => return,
    };

    let parsed: ReflectionResult = match serde_json::from_str(strip_code_fence(&raw)) {
        Ok(p) => p,
        Err(_) => return, // malformed output — skip silently, try again next debounce window
    };

    if parsed.action != "create" && parsed.action != "update" {
        return;
    }
    if parsed.name.trim().is_empty() || parsed.content.trim().is_empty() || parsed.rationale.trim().is_empty() {
        return;
    }
    if parsed.action == "update" && parsed.target_id.as_deref().unwrap_or("").is_empty() {
        return; // an "update" with nothing to update against isn't actionable
    }

    let previous_content = parsed
        .target_id
        .as_deref()
        .and_then(|id| skills::get_content(app_handle, id));

    let proposal = SkillProposal {
        id: uuid::Uuid::new_v4().to_string(),
        kind: parsed.action,
        target_id: parsed.target_id,
        name: parsed.name,
        description: parsed.description,
        content: parsed.content,
        triggers: parsed.triggers,
        rationale: parsed.rationale,
        previous_content,
        based_on_session: Some(session.id.clone()),
        created_at: now_ms(),
    };

    if let Ok(proposal_id) = skills::propose(app_handle, proposal) {
        let _ = app_handle.emit_all(
            "agent://skill-proposed",
            serde_json::json!({ "session_id": session.id, "proposal_id": proposal_id }),
        );
    }
}
