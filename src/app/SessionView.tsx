import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { HistoryItem, ToolCallEventPayload, TimelineItem } from "../types";
import GithubToolCard from "./GithubToolCard";
import ToolCallRow from "./ToolCallRow";
import SubagentCard from "./SubagentCard";
import SubagentView from "./SubagentView";
import DiffToolCard from "./DiffToolCard";
import MessageContent from "./MessageContent";
import { buildHistoryTimeline } from "./historyTimeline";
import { looksLikeSlashCommand, parseSlashCommand, filterSlashCommands, SLASH_HELP, type SlashCommandDef } from "./slashCommands";
import SlashCommandMenu from "./SlashCommandMenu";
import SkillLoadedCard from "./SkillLoadedCard";
import {
  ensureAgentEventsStarted,
  loadSession,
  markSendingStart,
  markSendingFailed,
  mutateSessionLocally,
  pushSystemNote as storePushSystemNote,
} from "./agentStore";
import { useAgentSession } from "./useAgentSession";
import PlanDrawer from "./PlanDrawer";

export default function SessionView({ sessionId }: { sessionId: string }) {
  // Live turn state (timeline/liveCalls/sending) and the persisted session
  // record both come from a global store that keeps running regardless of
  // whether this component is mounted — see agentStore.ts.
  const { session, timeline, liveCalls, subagentCalls, plan, sending } = useAgentSession(sessionId);
  const [input, setInput] = useState("");
  const [attachedImages, setAttachedImages] = useState<string[]>([]);
  const [slashIndex, setSlashIndex] = useState(0);
  const [activeSubagentId, setActiveSubagentId] = useState<string | null>(null);
  const bottomRef = useRef<HTMLDivElement>(null);
  const composerRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    ensureAgentEventsStarted();
    loadSession(sessionId);
    setActiveSubagentId(null);
  }, [sessionId]);

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

  const historyItems = useMemo(
    () => (session ? buildHistoryTimeline(session.messages) : []),
    [session?.messages]
  );

  // Scroll instantly to bottom on session switch or when history loads
  useEffect(() => {
    if (session) {
      bottomRef.current?.scrollIntoView({ behavior: "auto" });
      const timer = setTimeout(() => {
        bottomRef.current?.scrollIntoView({ behavior: "auto" });
      }, 50);
      return () => clearTimeout(timer);
    }
  }, [sessionId, session?.id, historyItems.length]);

  // Smooth scroll during live streaming updates
  useEffect(() => {
    if (timeline.length > 0) {
      bottomRef.current?.scrollIntoView({ behavior: "smooth" });
    }
  }, [timeline]);

  const pushSystemNote = (text: string) => storePushSystemNote(sessionId, text);

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

  const setSubagents = async (enabled: boolean, note = true) => {
    await invoke("set_session_subagents", { id: sessionId, enabled });
    mutateSessionLocally(sessionId, (s) => ({ ...s, subagents_enabled: enabled }));
    if (note) pushSystemNote(`Sub-agents turned ${enabled ? "on" : "off"}.`);
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

  const MAX_IMAGE_DIM = 1568;

  const downscaleDataUrl = (dataUrl: string): Promise<string> =>
    new Promise((resolve) => {
      const img = new Image();
      img.onload = () => {
        const scale = Math.min(1, MAX_IMAGE_DIM / Math.max(img.width, img.height));
        if (scale >= 1) {
          resolve(dataUrl);
          return;
        }
        const canvas = document.createElement("canvas");
        canvas.width = Math.round(img.width * scale);
        canvas.height = Math.round(img.height * scale);
        const ctx = canvas.getContext("2d");
        if (!ctx) {
          resolve(dataUrl);
          return;
        }
        ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
        const outMime = dataUrl.startsWith("data:image/png") ? "image/png" : "image/jpeg";
        try {
          resolve(canvas.toDataURL(outMime, 0.85));
        } catch {
          resolve(dataUrl);
        }
      };
      img.onerror = () => resolve(dataUrl);
      img.src = dataUrl;
    });

  const processFiles = (files: FileList | File[]) => {
    Array.from(files).forEach((file) => {
      if (file.type.startsWith("image/")) {
        const reader = new FileReader();
        reader.onload = (e) => {
          if (e.target?.result) {
            void downscaleDataUrl(e.target.result as string).then((dataUrl) => {
              setAttachedImages((prev) => [...prev, dataUrl]);
            });
          }
        };
        reader.readAsDataURL(file);
      }
    });
  };

  const handlePaste = (e: React.ClipboardEvent<HTMLTextAreaElement>) => {
    if (e.clipboardData && e.clipboardData.files.length > 0) {
      const imageFiles = Array.from(e.clipboardData.files).filter((f) => f.type.startsWith("image/"));
      if (imageFiles.length > 0) {
        processFiles(imageFiles);
      }
    }
  };

  const handleFileSelect = (e: React.ChangeEvent<HTMLInputElement>) => {
    if (e.target.files) {
      processFiles(e.target.files);
      e.target.value = "";
    }
  };

  const send = async () => {
    if ((!input.trim() && attachedImages.length === 0) || sending) return;
    const text = input;
    const imgs = [...attachedImages];
    setInput("");
    setAttachedImages([]);

    if (looksLikeSlashCommand(text)) {
      runSlashCommand(text);
      return;
    }

    markSendingStart(sessionId);
    invoke("send_message", { sessionId, message: text, images: imgs.length > 0 ? imgs : null }).catch((e) => {
      markSendingFailed(sessionId, e);
    });
  };

  const stop = () => {
    invoke("stop_session", { sessionId }).catch(() => {});
  };

  const approve = (callId: string, approved: boolean) => {
    invoke("approve_tool_call", { callId, approved });
  };

  const promptFix = (text: string) => {
    setInput(text);
    composerRef.current?.focus();
  };

  if (!session) return <div className="loading-screen">Loading session...</div>;

  const renderToolCard = (call: ToolCallEventPayload, nested: ToolCallEventPayload[]) => {
    if (call.name === "__skill_loaded__") {
      return <SkillLoadedCard key={call.call_id} event={call} />;
    }
    if (call.name === "delegate_to_subagent") {
      const calls = (subagentCalls && subagentCalls[call.call_id]) ? subagentCalls[call.call_id] : nested;
      return (
        <SubagentCard
          key={call.call_id}
          event={call}
          calls={calls}
          onOpen={() => setActiveSubagentId(call.call_id)}
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

  const historyNodes: JSX.Element[] = [];
  for (let i = 0; i < historyItems.length; ) {
    const item = historyItems[i];
    if (item.kind === "message") {
      historyNodes.push(
        <div key={item.key} className={"message " + item.role}>
          <span className="role-label">{item.role}</span>
          <div className="bubble">
            <MessageContent role={item.role} content={item.content} images={item.images} />
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
            <MessageContent role={item.role} content={item.content} images={item.images} streaming={item.streaming} />
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

  const showThinking =
    sending && !timeline.some((item) => item.kind === "message" && item.streaming);

  if (activeSubagentId) {
    let subagentEvent = liveCalls.find((c) => c.call_id === activeSubagentId);
    if (!subagentEvent) {
      const histItem = historyItems.find((h) => h.kind === "tool" && h.call.call_id === activeSubagentId);
      if (histItem && histItem.kind === "tool") {
        subagentEvent = histItem.call;
      }
    }
    if (subagentEvent) {
      const childCalls = (subagentCalls && subagentCalls[activeSubagentId])
        ? subagentCalls[activeSubagentId]
        : liveCalls.filter((n) => n.parent_call_id === activeSubagentId);
      return (
        <SubagentView
          event={subagentEvent}
          subagentCalls={childCalls}
          workspace={session.workspace}
          onBack={() => setActiveSubagentId(null)}
          onPromptFix={promptFix}
          onApprove={approve}
          plan={plan}
        />
      );
    }
  }

  return (
    <div className="session-view" style={{ position: "relative", flex: 1, display: "flex", flexDirection: "column", height: "100%", overflow: "hidden" }}>
      {plan && plan.length > 0 && <PlanDrawer plan={plan} />}
      <div className="session-header">
        <span
          className={"life-orb" + (sending ? " active" : "")}
          aria-hidden="true"
          title={sending ? "The agent is working" : "Idle"}
        />
        <h2>{session.title}</h2>
        <span className="session-usage-badge">⚡ {session.messages.length} msgs</span> <span className="hint">{session.mode}</span>
      </div>

      <div className="message-list">
        <div className="timeline-history">{historyNodes}</div>

        {timelineNodes}

        {showThinking && (
          <div className="thinking-indicator" aria-label="Kestrel is scanning">
            <div className="kestrel-hover-icon">
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
                <path d="M12 2c-3 3-7 5-9 7l4 2 5-4 5 4 4-2c-2-2-6-4-9-7z" />
                <path d="M12 11v6" />
                <path d="M9 21l3-4 3 4" />
              </svg>
            </div>
            <div className="kestrel-scan-content">
              <span className="kestrel-thinking-text">Scanning…</span>
              <div className="kestrel-scan-beam" />
            </div>
          </div>
        )}

        <div ref={bottomRef} />
      </div>

      <div className="composer-area">
        {attachedImages.length > 0 && (
          <div className="attached-images-preview">
            {attachedImages.map((img, idx) => (
              <div key={idx} className="preview-thumbnail">
                <img src={img} alt={`preview-${idx}`} />
                <button
                  type="button"
                  className="remove-img-btn"
                  onClick={() => setAttachedImages((prev) => prev.filter((_, i) => i !== idx))}
                  title="Remove image"
                >
                  &times;
                </button>
              </div>
            ))}
          </div>
        )}
        {session.mode === "coding" && slashMatches.length > 0 && (
          <SlashCommandMenu
            commands={slashMatches}
            activeIndex={Math.min(slashIndex, slashMatches.length - 1)}
            onSelect={selectSlashCommand}
          />
        )}
        <div className="composer">
          <label className="attach-btn" title="Attach image">
            <input
              type="file"
              accept="image/*"
              multiple
              onChange={handleFileSelect}
              style={{ display: "none" }}
            />
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 4 0 0 1-2.83-2.83l8.49-8.48" />
            </svg>
          </label>
          <textarea
            ref={composerRef}
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onPaste={handlePaste}
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
          <button
            className={"primary send-btn" + (sending ? " sending" : "")}
            onClick={sending ? stop : send}
            title={sending ? "Click to stop agent" : "Send message"}
            aria-label={sending ? "Stop the agent" : "Send message"}
          >
            {!sending ? (
              <span className="face face-send">
                <svg className="send-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round">
                  <line x1="22" y1="2" x2="11" y2="13" />
                  <polygon points="22 2 15 22 11 13 2 9 22 2" />
                </svg>
                <span>Send</span>
              </span>
            ) : (
              <>
                <span className="face face-running">
                  <span className="running-pulse-orb" />
                  <span>Working</span>
                </span>
                <span className="face face-stop">
                  <svg className="stop-icon" viewBox="0 0 24 24" fill="currentColor">
                    <rect x="6" y="6" width="12" height="12" rx="2" />
                  </svg>
                  <span>Stop</span>
                </span>
              </>
            )}
          </button>
        </div>
      </div>
    </div>
  );
}
