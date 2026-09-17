use crate::config::OmniRouteConfig;
use crate::omniroute::{self, ChatMessage};

/// Rough token estimate (~4 chars/token for English/code). Not exact, but
/// cheap and good enough to drive a budget check — we don't have access to
/// the actual tokenizer OmniRoute's backend model uses, and being off by
/// 20-30% doesn't matter when the trigger threshold already leaves a wide
/// margin under typical 128k-200k context windows.
fn estimate_tokens(s: &str) -> usize {
    (s.chars().count() / 4).max(1)
}

fn message_tokens(m: &ChatMessage) -> usize {
    let mut n = estimate_tokens(m.content.as_deref().unwrap_or(""));
    if let Some(calls) = &m.tool_calls {
        for c in calls {
            n += estimate_tokens(&c.function.name) + estimate_tokens(&c.function.arguments);
        }
    }
    n + 4 // per-message role/framing overhead
}

/// Estimated token size of a raw message history alone (not counting the
/// system prompt or auto-loaded skill content, which are rebuilt fresh
/// each step and are comparatively small and bounded).
pub fn messages_tokens(messages: &[ChatMessage]) -> usize {
    messages.iter().map(message_tokens).sum()
}

/// Compaction kicks in once the session's own messages are estimated to
/// exceed this many tokens. Deliberately conservative — this is a budget
/// for the *history* alone, leaving headroom for the system prompt,
/// auto-loaded skill content, and the model's own response on top of it.
const COMPACT_TRIGGER_TOKENS: usize = 60_000;

/// Always keep at least this many of the most recent messages verbatim —
/// recent context (including anything mid-edit) is never summarized away,
/// only whatever's older than that once the budget is blown.
const KEEP_RECENT_MESSAGES: usize = 40;

const SUMMARY_MARKER: &str = "[conversation summary — earlier turns compacted to save context]";

fn is_summary_message(m: &ChatMessage) -> bool {
    m.role == "system"
        && m.content
            .as_deref()
            .is_some_and(|c| c.starts_with(SUMMARY_MARKER))
}

/// Finds the first message at or after `from` whose role is "user" — the
/// only place it's safe to cut the history. Everything between one user
/// message and the next belongs to a single turn (an assistant tool_calls
/// message and the tool-result messages answering it can never be
/// separated, or the next request to the model is malformed), so the cut
/// point has to land exactly on a turn boundary.
fn safe_split_point(messages: &[ChatMessage], from: usize) -> Option<usize> {
    (from..messages.len()).find(|&i| messages[i].role == "user")
}

fn truncate_for_summary(s: &str) -> String {
    const MAX_CHARS: usize = 4000;
    if s.chars().count() <= MAX_CHARS {
        s.to_string()
    } else {
        let head: String = s.chars().take(MAX_CHARS).collect();
        format!("{}\n...[truncated for summary]", head)
    }
}

/// Flattens a chunk of chat messages into plain text for the summarizer
/// prompt, dropping streaming/schema noise and capping any single giant
/// tool result (a huge build log, a huge file read) rather than feeding it
/// to the summarizer verbatim.
fn render_transcript(chunk: &[ChatMessage]) -> String {
    let mut out = String::new();
    for m in chunk {
        if is_summary_message(m) {
            out.push_str("PREVIOUS SUMMARY:\n");
            out.push_str(m.content.as_deref().unwrap_or(""));
            out.push_str("\n\n");
            continue;
        }
        match m.role.as_str() {
            "user" => {
                out.push_str("USER: ");
                out.push_str(m.content.as_deref().unwrap_or(""));
                out.push_str("\n\n");
            }
            "assistant" => {
                if let Some(c) = &m.content {
                    if !c.is_empty() {
                        out.push_str("ASSISTANT: ");
                        out.push_str(c);
                        out.push_str("\n\n");
                    }
                }
                if let Some(calls) = &m.tool_calls {
                    for call in calls {
                        out.push_str(&format!(
                            "ASSISTANT CALLED: {}({})\n\n",
                            call.function.name,
                            truncate_for_summary(&call.function.arguments)
                        ));
                    }
                }
            }
            "tool" => {
                out.push_str(&format!(
                    "TOOL RESULT [{}]: {}\n\n",
                    m.name.as_deref().unwrap_or("tool"),
                    truncate_for_summary(m.content.as_deref().unwrap_or(""))
                ));
            }
            _ => {}
        }
    }
    out
}

async fn summarize(cfg: &OmniRouteConfig, chunk: &[ChatMessage]) -> Option<String> {
    let transcript = render_transcript(chunk);
    if transcript.trim().is_empty() {
        return None;
    }
    let system = ChatMessage {
        role: "system".into(),
        content: Some(
            "You compress an in-progress coding agent's work log into a compact but complete \
             summary for that SAME agent to keep working from. Preserve: the original goal(s) \
             and any constraints or decisions the user stated, every file created or modified \
             and what changed in each, commands run and their outcomes (especially failures and \
             whether/how they were resolved), and the current state of the work plus what's left \
             to do. Drop restated file contents, verbose command output, and anything already \
             superseded by a later step. Write dense prose and bullet points, not a narration of \
             the process. Do not invent anything not present in the log."
                .to_string(),
        ),
        ..Default::default()
    };
    let user = ChatMessage {
        role: "user".into(),
        content: Some(transcript),
        ..Default::default()
    };
    match omniroute::chat_completion(cfg, "auto/fast", &[system, user], None).await {
        Ok(resp) => resp.content.filter(|s| !s.trim().is_empty()),
        Err(_) => None,
    }
}

/// Compacts `messages` in place when they've grown past budget: everything
/// before a safe cut point (keeping the most recent messages verbatim) is
/// folded into a single system-role summary message via a cheap/fast model
/// call, replacing the originals. A pre-existing summary at the front gets
/// folded into the new one rather than re-summarized from scratch, so this
/// stays cheap even as a session grows into the hundreds of steps. Returns
/// true if it actually compacted anything.
///
/// Takes the raw message vector (a session's own history, or a sub-agent's
/// local, never-persisted one — both are just `Vec<ChatMessage>`) rather
/// than a whole `Session`, since a sub-agent needs exactly this same
/// budget-keeping without carrying along a full Session's unrelated fields.
///
/// With `force: false` this only fires once the estimated size crosses
/// COMPACT_TRIGGER_TOKENS — called once per step in the turn loop so a
/// single very long turn stays within budget throughout, not just at the
/// start. With `force: true` it compacts regardless of the estimate — the
/// backstop path used right after the API itself reports a context-length
/// error, where the estimate turned out to be wrong (a different tokenizer,
/// a smaller configured context window) and something needs to happen
/// immediately, not just next step.
pub async fn maybe_compact(
    cfg: &OmniRouteConfig,
    messages: &mut Vec<ChatMessage>,
    force: bool,
) -> bool {
    if !force && messages_tokens(messages) < COMPACT_TRIGGER_TOKENS {
        return false;
    }
    if messages.len() <= KEEP_RECENT_MESSAGES {
        return false; // nothing safe/worthwhile to cut yet
    }

    let target_keep_from = messages.len().saturating_sub(KEEP_RECENT_MESSAGES);
    let Some(split) = safe_split_point(messages, target_keep_from) else {
        // No user-message boundary in the recent window (a single turn
        // with a very long tool round) — nothing safe to cut this step.
        // The per-step call next iteration will retry.
        return false;
    };
    if split == 0 {
        return false; // recent window already starts at message 0
    }

    let to_summarize = &messages[..split];
    if to_summarize.len() == 1 && is_summary_message(&to_summarize[0]) {
        return false; // nothing new since the last compaction
    }

    let summary_text = match summarize(cfg, to_summarize).await {
        Some(s) => s,
        None => return false, // summarization failed — leave history alone, retry next step
    };

    let mut new_messages = vec![ChatMessage {
        role: "system".into(),
        content: Some(format!("{}\n\n{}", SUMMARY_MARKER, summary_text)),
        ..Default::default()
    }];
    new_messages.extend_from_slice(&messages[split..]);
    *messages = new_messages;
    true
}

/// Whether an error string returned from the model call looks like the
/// backend rejecting the request for being too long, across the handful of
/// phrasings different OpenAI-compatible providers use. There's no
/// standard error code guaranteed to come through OmniRoute's proxying, so
/// this is necessarily a substring heuristic rather than a status check.
pub fn is_context_length_error(err: &str) -> bool {
    let e = err.to_lowercase();
    e.contains("context_length_exceeded")
        || e.contains("context length")
        || e.contains("maximum context")
        || e.contains("too many tokens")
        || e.contains("reduce the length")
        || (e.contains("token") && e.contains("exceed"))
}
