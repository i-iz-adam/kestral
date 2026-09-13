import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import type { HistoryItem, ToolCallEventPayload, TimelineItem } from "../types";
import GithubToolCard from "./GithubToolCard";
import ToolCallRow from "./ToolCallRow";
import SubagentCard from "./SubagentCard";
import DiffToolCard from "./DiffToolCard";
import MessageContent from "./MessageContent";
import WorkspacePicker from "./WorkspacePicker";
import { buildHistoryTimeline } from "./historyTimeline";
import { looksLikeSlashCommand, parseSlashCommand, filterSlashCommands, SLASH_HELP, type SlashCommandDef } from "./slashCommands";
import SlashCommandMenu from "./SlashCommandMenu";
import SkillLoadedCard from "./SkillLoadedCard";
import {
  ensureAgentEventsStarted,
  loadSession,
  markSendingStart,
  mutateSessionLocally,
  pushSystemNote as storePushSystemNote,
} from "./agentStore";
import { useAgentSession } from "./useAgentSession";

/** Last path segment for display — "/Users/adam/projects/kestrel" reads
 * as "kestrel" in the header badge, with the full path still available
 * via the title tooltip. Falls back to the whole string for a bare
 * drive root or an unexpected empty value. */
function folderName(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const parts = trimmed.split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

export default function SessionView({ sessionId }: { sessionId: string }) {
  // Live turn state (timeline/liveCalls/sending) and the persisted session
  // record both come from a global store that keeps running regardless of
  // whether this component is mounted — see agentStore.ts. Switching to
  // another session and back (or opening Providers/Settings, which used
  // to unmount this entirely) no longer loses a turn in progress.
  const { session, timeline, liveCalls, sending } = useAgentSession(sessionId);
  const [input, setInput] = useState("");
  const [editingWorkspace, setEditingWorkspace] = useState(false);
  const [workspaceDraft, setWorkspaceDraft] = useState<string | null>(null);
  const [slashIndex, setSlashIndex] = useState(0);
  const bottomRef = useRef<HTMLDivElement>(null);
  const composerRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    ensureAgentEventsStarted();
    loadSession(sessionId);
  }, [sessionId]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [timeline]);

  // Command menu's filtered list is recomputed from `input` on every
  // render (cheap — a handful of string comparisons over ~4 commands);
  // the highlighted row resets to the top whenever the match set changes
  // so it can't point past the end of a shorter list after a keystroke.
  const slashMatches = input.startsWith("/") ? filterSlashCommands(input) : [];
  useEffect(() => {
    setSlashIndex(0);
  }, [input]);

  const selectSlashCommand = (cmd: SlashCommandDef) => {
    if (cmd.args) {
      setInput(`/${cmd.name} `);
      composerRef.current?.focus();
    } else {
      setInput("");
      runSlashCommand(`/${cmd.name}`);
      composerRef.current?.focus();
    }
  };

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
  const pushSystemNote = (text: string) => storePushSystemNote(sessionId, text);

  // Shared by the "/auto"/"/plan off" slash commands and the header's
  // Planning pill — draining pending approvals here (rather than only
  // flipping the setting) is what makes turning planning off actually
  // unstick a turn that's already sitting on an approval prompt, instead
  // of only affecting calls made from this point on.
  const turnPlanningOff = async () => {
    await invoke("set_session_planning", { id: sessionId, enabled: false });
    const resolved = await invoke<number>("approve_all_pending", { sessionId, approved: true });
    mutateSessionLocally(sessionId, (s) => ({ ...s, planning_enabled: false }));
    pushSystemNote(
      resolved > 0
        ? `Planning mode off — approved ${resolved} pending call${resolved === 1 ? "" : "s"}; new tool calls will run without asking.`
        : "Planning mode off — tool calls will run without asking for approval."
    );
  };

  const turnPlanningOn = async () => {
    await invoke("set_session_planning", { id: sessionId, enabled: true });
    mutateSessionLocally(sessionId, (s) => ({ ...s, planning_enabled: true }));
    pushSystemNote("Planning mode on — mutating tool calls will need approval again.");
  };

  const togglePlanning = () => {
    if (!session) return;
    if (session.planning_enabled) turnPlanningOff();
    else turnPlanningOn();
  };

  const setSubagents = async (enabled: boolean, note = true) => {
    await invoke("set_session_subagents", { id: sessionId, enabled });
    mutateSessionLocally(sessionId, (s) => ({ ...s, subagents_enabled: enabled }));
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

    markSendingStart(sessionId);
    // Errors surface via the global agent://turn-end listener (agentStore.ts)
    // as a system note in this session's timeline, regardless of whether
    // this component is still mounted when they arrive — so there's
    // nothing left to do here on rejection except avoid an unhandled
    // promise rejection warning.
    invoke("send_message", { sessionId, message: text }).catch(() => {});
  };

  const approve = (callId: string, approved: boolean) => {
    invoke("approve_tool_call", { callId, approved });
  };

  const promptFix = (text: string) => {
    setInput(text);
    composerRef.current?.focus();
  };

  const saveWorkspace = async (path: string) => {
    await invoke("set_session_workspace", { id: sessionId, workspace: path });
    mutateSessionLocally(sessionId, (s) => ({ ...s, workspace: path }));
    setEditingWorkspace(false);
    pushSystemNote(`Workspace switched to ${path}.`);
  };

  if (!session) return <div className="loading-screen">Loading session...</div>;

  // Shared by both the live timeline and the reconstructed history below —
  // a tool call renders the same way regardless of whether it just
  // happened or is being replayed from disk.
  const renderToolCard = (call: ToolCallEventPayload, nested: ToolCallEventPayload[]) => {
    if (call.name === "__skill_loaded__") {
      return <SkillLoadedCard key={call.call_id} event={call} />;
    }
    if (call.name === "delegate_to_subagent") {
      return (
        <SubagentCard
          key={call.call_id}
          event={call}
          calls={nested}
          workspace={session.workspace}
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
          workspace={session.workspace}
          onPromptFix={promptFix}
        />
      );
    }
    if (call.name === "edit_file" || call.name === "apply_patch") {
      return <DiffToolCard key={call.call_id} event={call} onApprove={approve} />;
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
        <span
          className={"life-orb" + (sending ? " active" : "")}
          aria-hidden="true"
          title={sending ? "The agent is working" : "Idle"}
        />
        <h2>{session.title}</h2>
        <span className="hint">{session.mode}</span>
        <div className="repo-link">
          {editingWorkspace ? (
            <span onBlur={() => setTimeout(() => setEditingWorkspace(false), 150)}>
              <WorkspacePicker
                className="inline"
                value={workspaceDraft ?? session.workspace}
                onChange={(path) => {
                  setWorkspaceDraft(path);
                  saveWorkspace(path);
                }}
              />
            </span>
          ) : (
            <button
              className="repo-badge workspace-badge"
              onClick={() => {
                setWorkspaceDraft(session.workspace);
                setEditingWorkspace(true);
              }}
              title={session.workspace}
            >
              {folderName(session.workspace)}
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
        {session.mode === "coding" && slashMatches.length > 0 && (
          <SlashCommandMenu
            commands={slashMatches}
            activeIndex={Math.min(slashIndex, slashMatches.length - 1)}
            onSelect={selectSlashCommand}
          />
        )}
        <div className="composer">
          <textarea
            ref={composerRef}
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => {
              if (slashMatches.length > 0) {
                if (e.key === "ArrowDown") {
                  e.preventDefault();
                  setSlashIndex((i) => (i + 1) % slashMatches.length);
                  return;
                }
                if (e.key === "ArrowUp") {
                  e.preventDefault();
                  setSlashIndex((i) => (i - 1 + slashMatches.length) % slashMatches.length);
                  return;
                }
                if ((e.key === "Tab" || e.key === "Enter") && !e.shiftKey) {
                  e.preventDefault();
                  selectSlashCommand(slashMatches[Math.min(slashIndex, slashMatches.length - 1)]);
                  return;
                }
              }
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
      </div>
    </div>
  );
}
