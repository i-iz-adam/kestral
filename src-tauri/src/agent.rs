use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};
use tauri::Manager;
use tokio::sync::{oneshot, Notify};

use crate::config;
use crate::context;
use crate::github;
use crate::omniroute::{self, ChatMessage, ToolCall};
use crate::plan;
use crate::prompts;
use crate::reflect;
use crate::sessions::{self, Session};
use crate::skills;
use crate::subagent;
use crate::tools;

/// Infinite-loop detector: tracks a signature of each full step — the
/// model's reasoning/answer text for that turn (its "thoughts", including
/// any inline <think> block) plus exactly which tool calls it issued —
/// and flags a loop once that signature repeats verbatim for several
/// consecutive steps in a row.
///
/// This deliberately does NOT key on tool name alone: an agent calling
/// `read_file` ten times in a row on ten different paths is normal work,
/// not a loop, and the old `is_stuck_on_same_tool` check was flagging
/// exactly that as a false positive. A real loop is the model calling the
/// *exact same thing* — same reasoning, same tool(s), same arguments —
/// over and over with no variation, which is what `record_step` captures
/// as a single hash per step rather than one entry per individual tool
/// call.
#[derive(Default)]
pub(crate) struct LoopDetector {
    signatures: Vec<u64>,
    max_history: usize,
    threshold: usize,
}

impl LoopDetector {
    pub(crate) fn new(max_history: usize, threshold: usize) -> Self {
        Self { signatures: Vec::with_capacity(max_history), max_history, threshold }
    }

    fn hash_str(s: &str) -> u64 {
        // Simple non-cryptographic hash for comparison purposes
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        s.hash(&mut hasher);
        hasher.finish()
    }

    /// Records one full step: the assistant's reasoning/answer text for
    /// this turn plus every tool call it issued this turn (name + raw
    /// arguments, in order). Call once per turn — after all of that
    /// turn's tool calls are known — not once per individual tool call,
    /// so a turn that issues three calls is one signature, not three.
    /// Whether the calls actually *succeeded* deliberately isn't part of
    /// the signature: a flaky command whose output differs slightly each
    /// time is still a loop if the model keeps reasoning and calling
    /// identically regardless of what comes back.
    pub(crate) fn record_step(&mut self, thought: &str, calls: &[(String, String)]) {
        let mut combined = String::from(thought.trim());
        for (name, args) in calls {
            combined.push('\u{1}');
            combined.push_str(name);
            combined.push('\u{1}');
            combined.push_str(args);
        }
        self.signatures.push(Self::hash_str(&combined));
        if self.signatures.len() > self.max_history {
            self.signatures.remove(0);
        }
    }

    /// True once the last `threshold` steps all produced the exact same
    /// signature — the model repeating the same thought and the same
    /// call(s) verbatim, with no variation at all.
    pub(crate) fn is_looping(&self) -> bool {
        if self.signatures.len() < self.threshold {
            return false;
        }
        let start = self.signatures.len() - self.threshold;
        let reference = self.signatures[start];
        self.signatures[start..].iter().all(|s| *s == reference)
    }

    #[allow(dead_code)]
    fn reset(&mut self) {
        self.signatures.clear();
    }
}

/// Tool calls waiting on user approval (planning mode), keyed by the
/// model's tool_call id, each holding the session it belongs to alongside
/// the oneshot sender so a whole session's queue can be drained at once
/// (see approve_all_pending) — e.g. when planning mode gets turned off
/// mid-turn and shouldn't leave whatever's already stuck waiting behind.
/// A pending call blocks its turn's loop on the receiving end of the
/// channel; `resolve_approval`/`approve_all_pending` resolve it. Shared
/// globally (not per-session) since call ids are unique regardless of
/// whether they came from the top-level loop or a sub-agent's loop.
#[derive(Default)]
pub struct PendingApprovals(pub Mutex<HashMap<String, (String, oneshot::Sender<bool>)>>);

/// Per-session stop state, shared between the top-level turn loop and any
/// sub-agent loops that turn spawned (a sub-agent is told to check the
/// SAME session's flag, not one of its own — stopping the chat stops its
/// sub-agents too, which is the whole feature). The flag is set the moment
/// the Stop button lands and is never unset during that turn; the turn
/// loop picks it up at its next step boundary, and in-flight approval
/// waits are woken immediately via the `Notify`.
pub struct SessionStop {
    requested: AtomicBool,
    notify: Notify,
}

/// Registry of stop requests, keyed by session id. Entries are created
/// lazily on the first request_stop call and garbage-collected once no
/// turn (or sub-agent) for that session is still running, so a stop
/// request the user made is never forgotten mid-turn no matter how many
/// loops are checking it. Shared globally (not per-session) so any thread
/// can request a stop without needing to know which loop is running.
#[derive(Default)]
pub struct StopRequests(Mutex<HashMap<String, Weak<SessionStop>>>);

const STOP_POLL_SECS: f64 = 0.1;

impl SessionStop {
    /// Whether a stop was requested for this session.
    pub(crate) fn is_requested(&self) -> bool {
        self.requested.load(Ordering::Relaxed)
    }

    /// Waits until either `duration` elapses or a stop is requested,
    /// returning early the moment a stop lands. Used to keep a
    /// long-running, non-cancellable chunk of work (a sub-agent winding
    /// down, a blocking tool) interruptible without propping a full stop
    /// through every layer of the call stack.
    pub(crate) async fn sleep_till_stop_or(&self, duration: Duration) {
        let until = Instant::now() + duration;
        loop {
            if self.requested.load(Ordering::Relaxed) {
                return;
            }
            let remaining = until.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return;
            }
            let sleep = remaining.min(Duration::from_secs_f64(STOP_POLL_SECS));
            tokio::time::sleep(sleep).await;
        }
    }
}

impl StopRequests {
    /// Creates (or returns the existing) stop flag for a session, keeping
    /// it alive for as long as the returned Arc — which the turn loop and
    /// every sub-agent it spawns holds. The registry itself only holds a
    /// Weak reference so it can't leak a session's stop state forever.
    pub(crate) fn entry(&self, session_id: &str) -> Arc<SessionStop> {
        let mut map = self.0.lock().unwrap();
        if let Some(weak) = map.get(session_id) {
            if let Some(arc) = weak.upgrade() {
                return arc;
            }
        }
        let arc = Arc::new(SessionStop {
            requested: AtomicBool::new(false),
            notify: Notify::new(),
        });
        map.insert(session_id.to_string(), Arc::downgrade(&arc));
        arc
    }

    /// True if a stop has been requested for this session. Takes the Arc
    /// so callers don't have to, and so this works even from code that
    /// only has a session id (not a loop holding its own handle).
    pub(crate) fn is_requested(&self, session_id: &str) -> bool {
        let map = self.0.lock().unwrap();
        map.get(session_id).and_then(|w| w.upgrade()).is_some_and(|s| s.requested.load(Ordering::Relaxed))
    }

    /// Marks a session as stop-requested and wakes any in-flight waiters
    /// (approval prompts, tool calls) so they settle immediately instead
    /// of waiting out their normal completion.
    pub(crate) fn request(&self, session_id: &str) {
        let arc = self.entry(session_id);
        arc.requested.store(true, Ordering::Relaxed);
        arc.notify.notify_waiters();
    }

    /// Clears a previously-requested stop. Only used at the very start of
    /// a brand-new turn, so a stop that was requested (but that the turn
    /// ran to completion on) doesn't leak into the next turn.
    pub(crate) fn reset(&self, session_id: &str) {
        let map = self.0.lock().unwrap();
        if let Some(weak) = map.get(session_id) {
            if let Some(arc) = weak.upgrade() {
                arc.requested.store(false, Ordering::Relaxed);
            }
        }
    }

    /// Drops any registry entry whose stop flag no loop is holding
    /// anymore (its last Arc went away when the turn ended). Called once
    /// at the end of a turn so the registry can't accumulate a stale
    /// entry per session forever.
    pub(crate) fn cleanup(&self, session_id: &str) {
        let map = self.0.lock().unwrap();
        if map.get(session_id).is_some_and(|w| w.strong_count() == 0) {
            drop(map);
            self.0.lock().unwrap().remove(session_id);
        }
    }
}

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
    #[serde(skip_serializing_if = "Option::is_none")]
    images: Option<&'a [String]>,
    /// Ties this final message to the "start"/"delta" events for the same
    /// assistant turn (see MessageStartEvent) so the frontend can finalize
    /// the streaming bubble it already built rather than appending a
    /// second, duplicate one. Absent for the user's own message, which is
    /// never streamed.
    #[serde(skip_serializing_if = "Option::is_none")]
    request_id: Option<&'a str>,
}

#[derive(Clone, Serialize)]
struct MessageStartEvent<'a> {
    session_id: &'a str,
    request_id: &'a str,
    role: &'a str,
}

#[derive(Clone, Serialize)]
struct MessageDeltaEvent<'a> {
    session_id: &'a str,
    request_id: &'a str,
    delta: &'a str,
}

/// Emitted instead of a final `agent://message` when an assistant turn
/// produced no visible text at all (the common case: it went straight to
/// tool_calls) — tells the frontend to drop the empty streaming placeholder
/// rather than leaving an empty bubble in the timeline.
#[derive(Clone, Serialize)]
struct MessageCancelEvent<'a> {
    session_id: &'a str,
    request_id: &'a str,
}

/// Emitted exactly once, no matter which path a turn exits through
/// (finished normally, hit an error, hit the step limit, or was stopped
/// by the user) — the frontend's single source of truth for "this turn is
/// over," used to know when it's safe to stop treating a session as live
/// and fall back entirely to its persisted history, and to distinguish a
/// deliberate stop (progress saved, no error) from a failure. Session-
/// scoped rather than tied to any particular view being open, since the
/// turn itself runs independently of whether anyone is looking at it (see
/// run_turn's wrapper below). `reason` is "normal" when the turn finished
/// on its own and "stopped" when the Stop button (or a stop_session call)
/// interrupted it — the frontend surfaces the latter as a quiet system
/// note rather than an error.
#[derive(Clone, Serialize)]
struct TurnEndEvent<'a> {
    session_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'a str>,
    /// "normal" | "stopped" — absent on events from older backends so the
    /// frontend treats it as "normal" (plain end) whenever it's missing.
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<&'a str>,
}

#[derive(Clone, Serialize)]
struct SessionTitleUpdatedEvent<'a> {
    session_id: &'a str,
    title: &'a str,
}

fn is_default_title(title: &str) -> bool {
    let t = title.trim();
    t.is_empty() || t == "New coding session" || t == "New chat"
}

fn clean_title(raw: &str) -> String {
    let mut s = raw.trim();
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        s = s[1..s.len() - 1].trim();
    }
    if s.to_lowercase().starts_with("title:") {
        s = s[6..].trim();
    }
    s = s.trim_end_matches('.');

    let mut result = s.to_string();
    if result.len() > 60 {
        result.truncate(60);
        if let Some(pos) = result.rfind(' ') {
            result.truncate(pos);
        }
    }

    result.trim().to_string()
}

pub(crate) async fn maybe_auto_generate_title(
    app_handle: &tauri::AppHandle,
    cfg: &config::OmniRouteConfig,
    session: &mut Session,
    user_message: &str,
) {
    if !is_default_title(&session.title) {
        return;
    }

    let system_msg = ChatMessage {
        role: "system".into(),
        content: Some(
            "You generate short, concise, descriptive session titles for a coding/chat app. \
             Generate a brief title (3 to 6 words maximum) summarizing the user's request. \
             Do NOT wrap in quotes. Do NOT add prefixes like 'Title:'. Respond ONLY with the title text."
                .to_string(),
        ),
        ..Default::default()
    };

    let user_msg = ChatMessage {
        role: "user".into(),
        content: Some(user_message.to_string()),
        ..Default::default()
    };

    let messages = vec![system_msg, user_msg];

    if let Ok(resp) = omniroute::chat_completion(cfg, "auto/fast", &messages, None).await {
        if let Some(content) = resp.content {
            let cleaned = clean_title(&content);
            if !cleaned.is_empty() {
                session.title = cleaned;
                sessions::save(app_handle, session);
                let _ = app_handle.emit_all(
                    "agent://session-title-updated",
                    SessionTitleUpdatedEvent {
                        session_id: &session.id,
                        title: &session.title,
                    },
                );
            }
        }
    }
}

pub(crate) const MODEL: &str = "auto/coding";

pub(crate) fn is_mutating(name: &str) -> bool {
    tools::is_mutating(name) || github::is_mutating(name) || skills::is_mutating(name)
}

pub(crate) fn is_mutating_call(name: &str, args: &Value) -> bool {
    tools::is_mutating_with_args(name, args) || github::is_mutating(name) || skills::is_mutating(name)
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

/// Announces an auto-loaded skill through the same event channel as a
/// real tool call (name "__skill_loaded__", always "done" — nothing was
/// actually invoked, so there's no start/approval phase) rather than a
/// bespoke event type. The frontend already renders arbitrary tool calls
/// generically, so this gets ordering, grouping, and a place in the
/// timeline for free — it just special-cases this one name into its own
/// animated card instead of a plain tool row (see SkillLoadedCard.tsx).
pub(crate) fn emit_skill_loaded(
    app_handle: &tauri::AppHandle,
    session_id: &str,
    skill: &skills::Skill,
    parent_call_id: Option<&str>,
) {
    let call_id = format!("skill-{}-{}", skill.id, uuid::Uuid::new_v4());
    let args = serde_json::json!({ "skill_id": skill.id, "skill_name": skill.name });
    emit_tool_event(
        app_handle, session_id, &call_id, "__skill_loaded__", "done",
        Some(args), Some(skill.description.clone()), parent_call_id,
    );
}

/// Routes one tool call to whichever module handles it: delegate_to_subagent
/// spawns a nested agent loop with its own isolated context, skills tools
/// are synchronous and local, github_* tools hit the GitHub API, everything
/// else is a workspace file/shell tool.
pub(crate) async fn execute_tool(
    app_handle: &tauri::AppHandle,
    approvals: &PendingApprovals,
    stops: &StopRequests,
    stop_flag: Arc<SessionStop>,
    session: &Session,
    call_id: &str,
    name: &str,
    args: &Value,
) -> Result<String, String> {
    if name == "delegate_to_subagent" {
        let task = args.get("task").and_then(|v| v.as_str()).ok_or("missing task")?;
        // Boxed to break the async recursion cycle:
        // execute_tool -> subagent::run -> handle_tool_call -> execute_tool.
        return Box::pin(subagent::run(app_handle, approvals, stops, stop_flag, session, call_id, task)).await;
    }
    if name == "update_plan" {
        // Persisted straight to disk, independent of `session` and of the
        // in-flight turn's message history — see plan.rs for why (it's
        // what lets a plan survive context compaction).
        return plan::maybe_execute(app_handle, &session.id, name, args)
            .unwrap_or_else(|| Err("update_plan handler missing".to_string()));
    }
    if name == "run_shell" {
        // Dispatched separately from the rest of tools::execute — this one
        // needs real async cancellation (a timeout, and a kill path the
        // Stop button can actually reach), which running it synchronously
        // on the async runtime (the old behavior) couldn't provide at all.
        let command = args.get("command").and_then(|v| v.as_str()).ok_or("missing command")?.to_string();
        let timeout_secs = args
            .get("timeout_seconds")
            .and_then(|v| v.as_u64())
            .unwrap_or(tools::DEFAULT_SHELL_TIMEOUT_SECS);
        let workspace = session.workspace.clone();
        return tools::run_shell_async(
            &workspace,
            &command,
            timeout_secs,
            session.sandbox_shell,
            session.sandbox_network,
            stop_flag,
        )
        .await;
    }
    let workspace = if session.workspace.trim().is_empty() { None } else { Some(session.workspace.as_str()) };
    if let Some(result) = skills::maybe_execute(app_handle, name, args, workspace) {
        return result;
    }
    if name.starts_with("github_") {
        let token = github::load_token(app_handle)
            .ok_or("GitHub is not connected — add a token in the GitHub tab first")?;
        return github::execute(&token, &session.workspace, name, args).await;
    }
    if name == "web_search" || name == "web_fetch" {
        let cfg = config::load_omniroute_config(app_handle)
            .ok_or("No OmniRoute config saved yet — finish setup first")?;
        if name == "web_search" {
            let query = args.get("query").and_then(|v| v.as_str()).ok_or("missing query parameter")?;
            let provider = args.get("provider").and_then(|v| v.as_str());
            let limit = args.get("limit").and_then(|v| v.as_u64()).map(|n| n as usize);
            return omniroute::web_search(&cfg, query, provider, limit).await;
        } else {
            let url = args.get("url").and_then(|v| v.as_str()).ok_or("missing url parameter")?;
            let provider = args.get("provider").and_then(|v| v.as_str());
            return omniroute::web_fetch(&cfg, url, provider).await;
        }
    }
    // Everything left (read_file/write_file/edit_file/apply_patch/
    // search_code/find_files/list_dir) is synchronous, blocking I/O —
    // moved off the async runtime's worker threads via spawn_blocking so a
    // slow directory walk or a big file write can't stall every other
    // session's turn loop (and every sub-agent's) running on the same
    // runtime alongside it.
    let workspace = session.workspace.clone();
    let name = name.to_string();
    let args = args.clone();
    tokio::task::spawn_blocking(move || tools::execute(&workspace, &name, &args))
        .await
        .unwrap_or_else(|e| Err(format!("tool task panicked: {}", e)))
}

/// Handles one tool call end to end: emits the "start" event, gates behind
/// planning-mode approval if the tool mutates something, executes it, and
/// emits the "done"/"error" event. Shared between the top-level loop and
/// sub-agent loops (see subagent.rs) so the approval flow — and what the
/// UI sees — can't drift between the two.
pub(crate) async fn handle_tool_call(
    app_handle: &tauri::AppHandle,
    approvals: &PendingApprovals,
    stops: &StopRequests,
    stop_flag: Arc<SessionStop>,
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

    // Re-read planning_enabled fresh rather than trusting the `session`
    // snapshot this whole turn started with — a session-level toggle (the
    // planning pill, or the /auto slash command) issued while a turn is
    // mid-flight should take effect on the very next tool call in that
    // same turn, not only on the next message. Falls back to the
    // in-memory value if the reload fails for some reason.
    let planning_enabled = sessions::load(app_handle, session_id)
        .map(|s| s.planning_enabled)
        .unwrap_or(session.planning_enabled);

    if planning_enabled && is_mutating_call(&call.function.name, &args) {
        let (tx, rx) = oneshot::channel::<bool>();
        approvals.0.lock().unwrap().insert(call.id.clone(), (session_id.to_string(), tx));

        emit_tool_event(
            app_handle, session_id, &call.id, &call.function.name, "awaiting-approval",
            Some(args.clone()), None, parent_call_id,
        );

        let approved = tokio::select! {
            decision = rx => decision.unwrap_or(false),
            _ = stop_flag.notify.notified() => {
                false
            }
        };
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

    let result = execute_tool(app_handle, approvals, stops, stop_flag, session, &call.id, &call.function.name, &args)
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

/// Thin wrapper: the actual work happens in run_turn_inner, but no matter
/// which of its several exit points is taken — success, an early `?`
/// failure, or the step-limit error — this makes sure `agent://turn-end`
/// still fires exactly once. Tauri already runs this on its async runtime
/// independent of any particular webview page, so the turn itself was
/// never actually tied to a view being open; what was missing was a
/// reliable signal for the frontend to know it finished, since navigating
/// away and back used to mean local state (and its event listeners) got
/// torn down and rebuilt, silently dropping whatever happened in between.
pub async fn run_turn(
    app_handle: tauri::AppHandle,
    approvals: tauri::State<'_, PendingApprovals>,
    stops: tauri::State<'_, StopRequests>,
    session_id: String,
    user_message: String,
    images: Option<Vec<String>>,
) -> Result<(), String> {
    run_turn_with_stop(app_handle, approvals, stops, session_id, user_message, images).await
}

pub async fn run_turn_with_stop(
    app_handle: tauri::AppHandle,
    approvals: tauri::State<'_, PendingApprovals>,
    stops: tauri::State<'_, StopRequests>,
    session_id: String,
    user_message: String,
    images: Option<Vec<String>>,
) -> Result<(), String> {
    stops.reset(&session_id);
    let stop_flag = stops.entry(&session_id);
    let result = run_turn_inner(&app_handle, approvals, &stops, stop_flag.clone(), &session_id, user_message, images).await;
    let reason = if stop_flag.requested.load(Ordering::Relaxed) { "stopped" } else { "normal" };
    let _ = app_handle.emit_all(
        "agent://turn-end",
        TurnEndEvent {
            session_id: &session_id,
            error: result.as_ref().err().map(String::as_str),
            reason: if reason == "normal" { None } else { Some(reason) },
        },
    );
    drop(stop_flag);
    stops.cleanup(&session_id);
    result
}

/// Streams one assistant turn (one model call) and forwards deltas to the
/// frontend as they arrive, same as before — pulled out into its own
/// function so run_turn_inner can call it a second time after a forced
/// compaction (see context::is_context_length_error) without duplicating
/// the event-emission wiring. On error, also emits the message-cancel event
/// for the streaming bubble it started, since nothing else will finalize it
/// for a request that never got a chance to produce a message. Returns the
/// request_id alongside the result because the caller needs it either way:
/// to finalize the bubble on success, or to know which id it already
/// cancelled on failure.
async fn stream_assistant_turn(
    app_handle: &tauri::AppHandle,
    cfg: &config::OmniRouteConfig,
    session_id: &str,
    request_messages: &[ChatMessage],
    tools_schema: Option<&Value>,
) -> (String, Result<ChatMessage, String>) {
    let request_id = uuid::Uuid::new_v4().to_string();
    let _ = app_handle.emit_all(
        "agent://message-start",
        MessageStartEvent { session_id, request_id: &request_id, role: "assistant" },
    );

    let delta_app_handle = app_handle.clone();
    let delta_session_id = session_id.to_string();
    let delta_request_id = request_id.clone();
    let result = omniroute::chat_completion_stream(
        cfg,
        MODEL,
        request_messages,
        tools_schema,
        move |delta: &str| {
            let _ = delta_app_handle.emit_all(
                "agent://message-delta",
                MessageDeltaEvent { session_id: &delta_session_id, request_id: &delta_request_id, delta },
            );
        },
    )
    .await;

    if result.is_err() {
        let _ = app_handle.emit_all(
            "agent://message-cancel",
            MessageCancelEvent { session_id, request_id: &request_id },
        );
    }

    (request_id, result)
}

async fn run_turn_inner(
    app_handle: &tauri::AppHandle,
    approvals: tauri::State<'_, PendingApprovals>,
    stops: &StopRequests,
    stop_flag: Arc<SessionStop>,
    session_id: &str,
    user_message: String,
    images: Option<Vec<String>>,
) -> Result<(), String> {
    let cfg = config::load_omniroute_config(app_handle)
        .ok_or("No OmniRoute config saved yet — finish setup first")?;

    let mut session =
        sessions::load(app_handle, session_id).ok_or("Session not found")?;

    maybe_auto_generate_title(app_handle, &cfg, &mut session, &user_message).await;

    session.messages.push(ChatMessage {
        role: "user".into(),
        content: Some(user_message.clone()),
        images: images.clone(),
        ..Default::default()
    });
    let _ = app_handle.emit_all(
        "agent://message",
        MessageEvent {
            session_id,
            role: "user",
            content: &user_message,
            images: images.as_deref(),
            request_id: None,
        },
    );

    // System prompt is built fresh each turn rather than persisted into
    // session.messages, so improving it later doesn't require migrating
    // old sessions. The delegation addendum is only appended when the
    // subagent tool is actually in this session's tool list — no point
    // telling the model about a tool it can't see.
        let system_prompt = if session.mode == "general" {
        prompts::GENERAL_SYSTEM_PROMPT.to_string()
    } else {
        // Skill authoring tools (create_skill/edit_skill/propose_skill) are
        // unconditionally part of skills::tool_definitions(), so they're in
        // this turn's tool list whenever coding mode is — the addendum
        // explaining how to use them belongs here for the same reason
        // SUBAGENT_DELEGATION_ADDENDUM is conditional on subagents_enabled.
        let mut prompt = format!("{}

{}", prompts::CODING_SYSTEM_PROMPT, prompts::SKILL_AUTHORING_ADDENDUM);
        if session.subagents_enabled {
            prompt = format!("{}

{}", prompt, prompts::SUBAGENT_DELEGATION_ADDENDUM);
        }
        prompt
    };

    let tools_schema = if session.mode == "general" {
        None
    } else {
        let mut all: Vec<Value> = tools::tool_definitions()
            .as_array()
            .cloned()
            .unwrap_or_default();
        all.extend(skills::tool_definitions().as_array().cloned().unwrap_or_default());
        all.extend(plan::tool_definitions().as_array().cloned().unwrap_or_default());
        if github::load_token(app_handle).is_some() {
            all.extend(github::tool_definitions().as_array().cloned().unwrap_or_default());
        }
        if session.subagents_enabled {
            all.extend(subagent::tool_definitions().as_array().cloned().unwrap_or_default());
        }

        if !tools::check_python_status().installed {
            all.retain(|t| {
                t.get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|n| n.as_str())
                    != Some("run_python")
            });
        }

        Some(Value::Array(all))
    };

    // Auto-load whatever skills this message's content suggests are
    // relevant, rather than leaving it entirely up to the model to
    // remember list_skills/read_skill exist and choose to call them.
    // Announced once per turn (not per tool-call round) and injected as
    // extra system context on every step of this turn, so the guidance
    // stays present through however many tool rounds the turn takes.
    // Deduplicated per session context so each unique skill is auto-loaded
    // into context only once.
    let workspace = if session.workspace.trim().is_empty() { None } else { Some(session.workspace.as_str()) };
    let mut skill_messages: Vec<ChatMessage> = Vec::new();
    if session.mode != "general" {
        let mut loaded_skill_ids = skills::get_loaded_skill_ids(&session);

        // Keyword matching is the free, instant fast-path (see
        // skills::find_relevant); anything it doesn't catch — a
        // paraphrase, a synonym, a skill whose triggers just don't happen
        // to overlap with how this was phrased — falls to a judgment call
        // by the fast/cheap model instead, the same one session-title
        // generation and reflection use. This is the actual fix for
        // "keyword-only auto-loading misses real matches": the model
        // decides relevance the way a person skimming list_skills would,
        // rather than a fixed word list having to predict every phrasing
        // in advance.
        let mut matched: Vec<skills::Skill> = skills::find_relevant(app_handle, &user_message, workspace);
        let already: std::collections::HashSet<String> =
            matched.iter().map(|s| s.id.clone()).chain(loaded_skill_ids.iter().cloned()).collect();
        matched.extend(skills::find_relevant_ai(app_handle, &cfg, workspace, &user_message, &already).await);

        for skill in matched {
            if !loaded_skill_ids.contains(&skill.id) {
                if skills::get_content(app_handle, &skill.id, workspace).is_some() {
                    emit_skill_loaded(app_handle, session_id, &skill, None);
                    let call_id = format!("skill-{}-{}", skill.id, uuid::Uuid::new_v4());
                    let args = serde_json::json!({ "skill_id": skill.id, "skill_name": skill.name });
                    session.messages.push(ChatMessage {
                        role: "skill-loaded".into(),
                        content: Some(serde_json::json!({
                            "call_id": call_id,
                            "name": "__skill_loaded__",
                            "args": args,
                            "result": skill.description.clone(),
                        }).to_string()),
                        ..Default::default()
                    });
                    loaded_skill_ids.insert(skill.id.clone());
                }
            }
        }

        let all_skills = skills::list(app_handle, workspace);
        for id in &loaded_skill_ids {
            if let Some(skill_info) = all_skills.iter().find(|s| &s.id == id) {
                if let Some(content) = skills::get_content(app_handle, id, workspace) {
                    skill_messages.push(ChatMessage {
                        role: "system".into(),
                        content: Some(format!("Relevant skill — {}:\n\n{}", skill_info.name, content)),
                        ..Default::default()
                    });
                }
            }
        }

        // AGENTS.md is standing, unconditional project context — unlike a
        // skill, it isn't keyword- or AI-matched into relevance, it's just
        // always there for a workspace that has one, the same convention
        // other coding agents follow (and the built-in "agents-md" skill
        // covers how to write a good one).
        if let Some(ws) = workspace {
            if let Some(agents_md) = skills::read_agents_md(ws) {
                skill_messages.push(ChatMessage {
                    role: "system".into(),
                    content: Some(format!("This project's AGENTS.md:\n\n{}", agents_md)),
                    ..Default::default()
                });
            }
        }
    }

    let mut loop_detector = LoopDetector::new(20, 5);
    let mut step_count: u64 = 0;
    // Every tool name called across every tool-round of this whole turn —
    // separate from loop_detector's per-iteration call_signatures — so the
    // end-of-turn reflection gate (see reflect::maybe_reflect) can tell
    // whether *this turn as a whole* did anything worth reflecting on, not
    // just its final tool-round.
    let mut all_tool_names_this_turn: Vec<String> = Vec::new();

    loop {
        step_count += 1;

        if stop_flag.requested.load(Ordering::Relaxed) {
            sessions::save(app_handle, &session);
            return Ok(());
        }

        // A single long turn (hundreds of steps working through one big
        // task) needs its own history kept in budget throughout, not just
        // checked once at the top of run_turn — so this runs every step.
        // Cheap no-op once the session is well under budget.
        if context::maybe_compact(&cfg, &mut session.messages, false).await {
            sessions::save(app_handle, &session);
        }

        let plan_items = plan::load(app_handle, session_id);
        let plan_message = plan::render(&plan_items).map(|text| ChatMessage {
            role: "system".into(),
            content: Some(text),
            ..Default::default()
        });

        let build_messages = |session: &Session| {
            let mut msgs = vec![ChatMessage {
                role: "system".into(),
                content: Some(system_prompt.clone()),
                ..Default::default()
            }];
            msgs.extend(skill_messages.clone());
            msgs.extend(plan_message.clone());
            msgs.extend(
                session
                    .messages
                    .iter()
                    .filter(|m| omniroute::is_valid_llm_role(&m.role))
                    .cloned(),
            );
            msgs
        };
        let request_messages = build_messages(&session);

        let (mut request_id, mut stream_result) =
            stream_assistant_turn(app_handle, &cfg, session_id, &request_messages, tools_schema.as_ref()).await;

        if let Err(e) = &stream_result {
            if context::is_context_length_error(e) {
                // Backstop: the per-step compaction above should normally
                // keep this from happening at all, but the token estimate
                // is just that — an estimate — so on an actual
                // context-length rejection from the backend, force a
                // compaction regardless of the estimate and retry exactly
                // once before giving up.
                if context::maybe_compact(&cfg, &mut session.messages, true).await {
                    sessions::save(app_handle, &session);
                    let retry_messages = build_messages(&session);
                    let (rid2, res2) =
                        stream_assistant_turn(app_handle, &cfg, session_id, &retry_messages, tools_schema.as_ref()).await;
                    request_id = rid2;
                    stream_result = res2;
                }
            }
        }

        let assistant_msg = stream_result?;

        let tool_calls = assistant_msg.tool_calls.clone().unwrap_or_default();
        let text = assistant_msg.content.clone().unwrap_or_default();

        if text.trim().is_empty() && tool_calls.is_empty() {
            let _ = app_handle.emit_all(
                "agent://message-cancel",
                MessageCancelEvent { session_id, request_id: &request_id },
            );
            return Err("Model returned an empty response (no text or tool calls). Please check model/provider configuration and try again.".to_string());
        }

        session.messages.push(assistant_msg.clone());

        if text.is_empty() {
            // Nothing to show for this turn (it went straight to tools) — drop the
            // placeholder instead of finalizing an empty bubble.
            let _ = app_handle.emit_all(
                "agent://message-cancel",
                MessageCancelEvent { session_id, request_id: &request_id },
            );
        } else {
            let _ = app_handle.emit_all(
                "agent://message",
                MessageEvent {
                    session_id,
                    role: "assistant",
                    content: &text,
                    images: None,
                    request_id: Some(&request_id),
                },
            );
        }

        if tool_calls.is_empty() {
            sessions::save(app_handle, &session);
            // Reflection is a background housekeeping pass, not part of
            // what the user is waiting on — spawn it detached so it can't
            // add its own latency (a whole extra model round-trip) onto
            // this turn's completion. It only ever writes to the proposal
            // queue (see reflect::maybe_reflect), never anything the user
            // is currently looking at, so there's nothing time-sensitive
            // about it running a moment after agent://turn-end fires.
            let reflect_app = app_handle.clone();
            let reflect_cfg = cfg.clone();
            let mut reflect_session = session.clone();
            let reflect_tools = all_tool_names_this_turn.clone();
            tokio::spawn(async move {
                reflect::maybe_reflect(&reflect_app, &reflect_cfg, &mut reflect_session, &reflect_tools).await;
            });
            return Ok(());
        }

        let mut call_signatures: Vec<(String, String)> = Vec::with_capacity(tool_calls.len());
        for call in &tool_calls {
            let tool_msg = handle_tool_call(
                app_handle,
                approvals.inner(),
                stops,
                stop_flag.clone(),
                &session,
                session_id,
                call,
                None,
            )
            .await;

            call_signatures.push((call.function.name.clone(), call.function.arguments.clone()));
            all_tool_names_this_turn.push(call.function.name.clone());
            session.messages.push(tool_msg);

            if stop_flag.requested.load(Ordering::Relaxed) {
                break;
            }
        }

        // One signature per turn (reasoning text + every call it made this
        // turn) — not one per individual tool call — so calling several
        // different tools in one turn doesn't look like several steps.
        loop_detector.record_step(&text, &call_signatures);

        sessions::save(app_handle, &session);

        // Detect infinite loops: the exact same reasoning and the exact
        // same tool call(s) repeating verbatim, several turns running.
        if loop_detector.is_looping() {
            let msg = format!(
                "Stopped after {} steps: infinite loop detected (same thinking and tool call repeating with no progress).",
                step_count
            );
            session.messages.push(ChatMessage {
                role: "system".into(),
                content: Some(msg.clone()),
                ..Default::default()
            });
            sessions::save(app_handle, &session);
            return Err(msg);
        }

        // Safety net: absolute maximum step limit
        if step_count >= 1000 {
            let msg = "Stopped after 1000 steps: maximum step limit reached.".to_string();
            session.messages.push(ChatMessage {
                role: "system".into(),
                content: Some(msg.clone()),
                ..Default::default()
            });
            sessions::save(app_handle, &session);
            return Err(msg);
        }
    }
}
/// Resolves a single pending approval by tool-call id.
pub fn resolve_approval(approvals: &PendingApprovals, call_id: &str, approved: bool) {
    if let Some((_, tx)) = approvals.0.lock().unwrap().remove(call_id) {
        let _ = tx.send(approved);
    }
}

/// Resolves every call currently waiting on approval for one session in
/// one shot — what the "/auto" slash command and the planning-mode pill's
/// off-switch use, so disabling planning mode also clears whatever's
/// already stuck waiting instead of leaving it for a separate manual
/// approve click. Returns how many calls were resolved.
pub fn approve_all_pending(approvals: &PendingApprovals, session_id: &str, approved: bool) -> usize {
    let mut map = approvals.0.lock().unwrap();
    let ids: Vec<String> = map
        .iter()
        .filter(|(_, (sid, _))| sid == session_id)
        .map(|(id, _)| id.clone())
        .collect();
    for id in &ids {
        if let Some((_, tx)) = map.remove(id) {
            let _ = tx.send(approved);
        }
    }
    ids.len()
}
