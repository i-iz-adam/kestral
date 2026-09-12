import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { listen } from "@tauri-apps/api/event";
import type {
  Session,
  ToolCallEventPayload,
  MessageEventPayload,
  MessageStartEventPayload,
  MessageDeltaEventPayload,
  MessageCancelEventPayload,
  TimelineItem,
  HistoryItem,
} from "../types";
import GithubToolCard from "./GithubToolCard";
import ToolCallRow from "./ToolCallRow";
import SubagentCard from "./SubagentCard";
import MessageContent from "./MessageContent";
import { buildHistoryTimeline } from "./historyTimeline";
import { looksLikeSlashCommand, parseSlashCommand, SLASH_HELP } from "./slashCommands";

let timelineKeySeq = 0;
const nextKey = (prefix: string) => `${prefix}-${++timelineKeySeq}`;

export default function SessionView({ sessionId }: { sessionId: string }) {
  const [session, setSession] = useState<Session | null>(null);
  const [input, setInput] = useState("");
  const [sending, setSending] = useState(false);
  // Every live tool call by id, regardless of nesting — the source of
  // truth for each call's current status/result. Render order for
  // top-level calls comes from `timeline` instead, so this can be a plain
  // lookup map in spirit (kept as an array for the existing nested-lookup
  // code in child cards).
  const [liveCalls, setLiveCalls] = useState<ToolCallEventPayload[]>([]);
  // The single chronological feed of messages + top-level tool calls, in
  // the exact order the backend emitted them — this is what actually
  // fixes ordering: a bubble and a tool row are just two kinds of entry in
  // one list instead of two lists rendered one after the other.
  const [timeline, setTimeline] = useState<TimelineItem[]>([]);
  const [editingRepo, setEditingRepo] = useState(false);
  const [repoInput, setRepoInput] = useState("");
  const bottomRef = useRef<HTMLDivElement>(null);
  const composerRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    setLiveCalls([]);
    setTimeline([]);
    setSending(false);
    invoke<Session>("get_session", { id: sessionId }).then((s) => {
      setSession(s);
      setRepoInput(s.linked_repo ?? "");
    });
  }, [sessionId]);

  useEffect(() => {
    const unlistenTool = listen<ToolCallEventPayload>(
      "agent://tool-call",
      (evt) => {
        if (evt.payload.session_id !== sessionId) return;
        setLiveCalls((prev) => {
          const idx = prev.findIndex((c) => c.call_id === evt.payload.call_id);
          if (idx === -1) return [...prev, evt.payload];
          const copy = [...prev];
          copy[idx] = evt.payload;
          return copy;
        });
        if (!evt.payload.parent_call_id) {
          setTimeline((prev) =>
            prev.some(
              (item) => item.kind === "tool" && item.callId === evt.payload.call_id
            )
              ? prev
              : [
                  ...prev,
                  { kind: "tool", key: nextKey("tool"), callId: evt.payload.call_id },
                ]
          );
        }
      }
    );

    // A turn's assistant text arrives as: message-start (placeholder),
    // any number of message-delta chunks, then either a final message
    // (finalize) or a message-cancel (nothing was said, it went straight
    // to tool calls) — see agent.rs::run_turn.
    const unlistenStart = listen<MessageStartEventPayload>(
      "agent://message-start",
      (evt) => {
        if (evt.payload.session_id !== sessionId) return;
        setTimeline((prev) => [
          ...prev,
          {
            kind: "message",
            key: nextKey("msg"),
            requestId: evt.payload.request_id,
            role: evt.payload.role,
            content: "",
            streaming: true,
          },
        ]);
      }
    );
    const unlistenDelta = listen<MessageDeltaEventPayload>(
      "agent://message-delta",
      (evt) => {
        if (evt.payload.session_id !== sessionId) return;
        setTimeline((prev) => {
          const idx = prev.findIndex(
            (item) => item.kind === "message" && item.requestId === evt.payload.request_id
          );
          if (idx === -1) return prev;
          const copy = [...prev];
          const item = copy[idx] as Extract<TimelineItem, { kind: "message" }>;
          copy[idx] = { ...item, content: item.content + evt.payload.delta };
          return copy;
        });
      }
    );
    const unlistenCancel = listen<MessageCancelEventPayload>(
      "agent://message-cancel",
      (evt) => {
        if (evt.payload.session_id !== sessionId) return;
        setTimeline((prev) =>
          prev.filter(
            (item) => !(item.kind === "message" && item.requestId === evt.payload.request_id)
          )
        );
      }
    );
    const unlistenMsg = listen<MessageEventPayload>(
      "agent://message",
      (evt) => {
        if (evt.payload.session_id !== sessionId) return;
        setTimeline((prev) => {
          if (evt.payload.request_id) {
            const idx = prev.findIndex(
              (item) => item.kind === "message" && item.requestId === evt.payload.request_id
            );
            if (idx !== -1) {
              const copy = [...prev];
              copy[idx] = {
                kind: "message",
                key: copy[idx].key,
                requestId: evt.payload.request_id,
                role: evt.payload.role,
                content: evt.payload.content,
                streaming: false,
              };
              return copy;
            }
          }
          // No matching placeholder (the user's own message, which is
          // never streamed) — just append it.
          return [
            ...prev,
            {
              kind: "message",
              key: nextKey("msg"),
              role: evt.payload.role,
              content: evt.payload.content,
              streaming: false,
            },
          ];
        });
        if (evt.payload.role === "assistant") setSending(false);
      }
    );
    return () => {
      unlistenTool.then((f) => f());
      unlistenStart.then((f) => f());
      unlistenDelta.then((f) => f());
      unlistenCancel.then((f) => f());
      unlistenMsg.then((f) => f());
    };
  }, [sessionId]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [timeline]);

  // Reconstructed once per session load (not on every incidental local
  // state tweak, like toggling sub-agents) — this is what makes tool call
  // cards and past replies survive a session switch or app restart,
  // instead of only existing for the lifetime of the live `timeline`
  // above, which starts empty every time this component mounts.
  const historyItems = useMemo(
    () => (session ? buildHistoryTimeline(session.messages) : []),
    [session?.messages]
  );

  // Local-only feedback for a slash command — never touches session.messages
  // or the model, so it doesn't cost a turn and disappears like any other
  // ephemeral UI state if you switch sessions and back.
  const pushSystemNote = (text: string) => {
    setTimeline((prev) => [
      ...prev,
      { kind: "message", key: nextKey("sys"), role: "system", content: text, streaming: false },
    ]);
  };

  // Shared by the "/auto"/"/plan off" slash commands and the header's
  // Planning pill — draining pending approvals here (rather than only
  // flipping the setting) is what makes turning planning off actually
  // unstick a turn that's already sitting on an approval prompt, instead
  // of only affecting calls made from this point on.
  const turnPlanningOff = async () => {
    await invoke("set_session_planning", { id: sessionId, enabled: false });
    const resolved = await invoke<number>("approve_all_pending", { sessionId, approved: true });
    setSession((s) => (s ? { ...s, planning_enabled: false } : s));
    pushSystemNote(
      resolved > 0
        ? `Planning mode off — approved ${resolved} pending call${resolved === 1 ? "" : "s"}; new tool calls will run without asking.`
        : "Planning mode off — tool calls will run without asking for approval."
    );
  };

  const turnPlanningOn = async () => {
    await invoke("set_session_planning", { id: sessionId, enabled: true });
    setSession((s) => (s ? { ...s, planning_enabled: true } : s));
    pushSystemNote("Planning mode on — mutating tool calls will need approval again.");
  };

  const togglePlanning = () => {
    if (!session) return;
    if (session.planning_enabled) turnPlanningOff();
    else turnPlanningOn();
  };

  const setSubagents = async (enabled: boolean, note = true) => {
    await invoke("set_session_subagents", { id: sessionId, enabled });
    setSession((s) => (s ? { ...s, subagents_enabled: enabled } : s));
    if (note) pushSystemNote(`Sub-agents turned ${enabled ? "on" : "off"}.`);
  };

  const toggleSubagents = () => {
    if (session) setSubagents(!session.subagents_enabled, false);
  };

  const runSlashCommand = (raw: string) => {
    const { cmd, arg } = parseSlashCommand(raw);
    switch (cmd) {
      case "auto":
        turnPlanningOff();
        return;
      case "plan":
        if (arg === "on") turnPlanningOn();
        else if (arg === "off") turnPlanningOff();
        else pushSystemNote("Usage: /plan on  or  /plan off");
        return;
      case "subagents":
        if (arg === "on" || arg === "off") setSubagents(arg === "on");
        else pushSystemNote("Usage: /subagents on  or  /subagents off");
        return;
      case "help":
        pushSystemNote(SLASH_HELP);
        return;
      default:
        pushSystemNote(`Unknown command "/${cmd}".\n\n${SLASH_HELP}`);
    }
  };

  const send = async () => {
    if (!input.trim() || sending) return;
    const text = input;
    setInput("");

    if (looksLikeSlashCommand(text)) {
      runSlashCommand(text);
      return;
    }

    setSending(true);
    try {
      await invoke("send_message", { sessionId, message: text });
    } catch (e) {
      setTimeline((prev) => [
        ...prev,
        {
          kind: "message",
          key: nextKey("msg"),
          role: "assistant",
          content: `Error: ${String(e)}`,
          streaming: false,
        },
      ]);
      setSending(false);
    }
  };

  const approve = (callId: string, approved: boolean) => {
    invoke("approve_tool_call", { callId, approved });
  };

  const promptFix = (text: string) => {
    setInput(text);
    composerRef.current?.focus();
  };

  const saveRepo = async () => {
    const repo = repoInput.trim() || null;
    await invoke("set_session_repo", { id: sessionId, repo });
    setSession((s) => (s ? { ...s, linked_repo: repo } : s));
    setEditingRepo(false);
  };

  if (!session) return <div className="loading-screen">Loading session...</div>;

  // Shared by both the live timeline and the reconstructed history below —
  // a tool call renders the same way regardless of whether it just
  // happened or is being replayed from disk.
  const renderToolCard = (call: ToolCallEventPayload, nested: ToolCallEventPayload[]) => {
    if (call.name === "delegate_to_subagent") {
      return (
        <SubagentCard
          key={call.call_id}
          event={call}
          calls={nested}
          linkedRepo={session.linked_repo}
          onPromptFix={promptFix}
          onApprove={approve}
        />
      );
    }
    if (call.name.startsWith("github_")) {
      return (
        <GithubToolCard
          key={call.call_id}
          event={call}
          linkedRepo={session.linked_repo}
          onPromptFix={promptFix}
        />
      );
    }
    return <ToolCallRow key={call.call_id} event={call} onApprove={approve} />;
  };

  // Persisted turns from earlier in this session (or from before the app
  // was last closed) — reconstructed once above via buildHistoryTimeline.
  // Grouped the same way the live timeline is, just with no nested calls
  // to look up (sub-agent steps aren't persisted, only their summary).
  const historyNodes: JSX.Element[] = [];
  for (let i = 0; i < historyItems.length; ) {
    const item = historyItems[i];
    if (item.kind === "message") {
      historyNodes.push(
        <div key={item.key} className={"message " + item.role}>
          <span className="role-label">{item.role}</span>
          <div className="bubble">
            <MessageContent role={item.role} content={item.content} />
          </div>
        </div>
      );
      i++;
    } else {
      const group: Extract<HistoryItem, { kind: "tool" }>[] = [];
      while (i < historyItems.length && historyItems[i].kind === "tool") {
        group.push(historyItems[i] as Extract<HistoryItem, { kind: "tool" }>);
        i++;
      }
      historyNodes.push(
        <div className="tool-stream" key={"hgroup-" + group[0].key}>
          {group.map((g) => renderToolCard(g.call, []))}
        </div>
      );
    }
  }

  // Consecutive tool-call entries render grouped inside one .tool-stream
  // wrapper (tight spacing, like a mini timeline of its own); message
  // entries render standalone. The grouping is purely visual — the order
  // itself already comes straight from `timeline`, which is what keeps a
  // final reply from ever jumping above the tool calls that produced it.
  const timelineNodes: JSX.Element[] = [];
  for (let i = 0; i < timeline.length; ) {
    const item = timeline[i];
    if (item.kind === "message") {
      timelineNodes.push(
        <div
          key={item.key}
          className={"message " + item.role + (item.streaming ? " streaming" : "")}
        >
          <span className="role-label">{item.role}</span>
          <div className="bubble">
            <MessageContent role={item.role} content={item.content} streaming={item.streaming} />
          </div>
        </div>
      );
      i++;
    } else {
      const group: string[] = [];
      while (i < timeline.length && timeline[i].kind === "tool") {
        group.push((timeline[i] as Extract<TimelineItem, { kind: "tool" }>).callId);
        i++;
      }
      timelineNodes.push(
        <div className="tool-stream" key={"group-" + group[0]}>
          {group.map((callId) => {
            const call = liveCalls.find((c) => c.call_id === callId);
            if (!call) return null;
            const nested =
              call.name === "delegate_to_subagent"
                ? liveCalls.filter((n) => n.parent_call_id === call.call_id)
                : [];
            return renderToolCard(call, nested);
          })}
        </div>
      );
    }
  }

  // Covers the brief gap between hitting Send and the backend's first
  // message-start event — after that, the streaming placeholder bubble
  // itself (with its blinking cursor) is the "thinking" indicator.
  const showThinking =
    sending && !timeline.some((item) => item.kind === "message" && item.streaming);

  return (
    <div className="session-view">
      <div className="session-header">
        <h2>{session.title}</h2>
        <span className="hint">{session.mode}</span>
        <div className="repo-link">
          {editingRepo ? (
            <>
              <input
                value={repoInput}
                onChange={(e) => setRepoInput(e.target.value)}
                placeholder="owner/repo"
                style={{ width: 160 }}
              />
              <button onClick={saveRepo}>Save</button>
            </>
          ) : (
            <button className="repo-badge" onClick={() => setEditingRepo(true)}>
              {session.linked_repo ? session.linked_repo : "Link a repo"}
            </button>
          )}
          {session.mode === "coding" && (
            <>
              <button
                className={"repo-badge" + (session.planning_enabled ? "" : " off")}
                onClick={togglePlanning}
                title="Mutating tool calls pause for approval while this is on"
              >
                {session.planning_enabled ? "Planning: on" : "Planning: off"}
              </button>
              <button className="repo-badge" onClick={toggleSubagents}>
                {session.subagents_enabled ? "Sub-agents: on" : "Sub-agents: off"}
              </button>
            </>
          )}
        </div>
      </div>

      <div className="message-list">
        <div className="timeline-history">{historyNodes}</div>

        {timelineNodes}

        {showThinking && (
          <div className="thinking-indicator" aria-label="Assistant is thinking">
            <span />
            <span />
            <span />
          </div>
        )}

        <div ref={bottomRef} />
      </div>

      <div className="composer-area">
        <div className="composer">
          <textarea
            ref={composerRef}
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                send();
              }
            }}
            placeholder={
              session.mode === "coding"
                ? "Ask the agent to do something, or type / for commands..."
                : "Ask anything..."
            }
          />
          <button className={"primary" + (sending ? " sending" : "")} onClick={send} disabled={sending}>
            {sending ? "Working" : "Send"}
          </button>
        </div>
        {session.mode === "coding" && looksLikeSlashCommand(input) && (
          <div className="composer-hint">{SLASH_HELP}</div>
        )}
      </div>
    </div>
  );
}
