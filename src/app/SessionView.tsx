import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { listen } from "@tauri-apps/api/event";
import type {
  Session,
  ToolCallEventPayload,
  MessageEventPayload,
} from "../types";
import GithubToolCard from "./GithubToolCard";
import ToolCallRow from "./ToolCallRow";
import SubagentCard from "./SubagentCard";

export default function SessionView({ sessionId }: { sessionId: string }) {
  const [session, setSession] = useState<Session | null>(null);
  const [input, setInput] = useState("");
  const [sending, setSending] = useState(false);
  const [liveCalls, setLiveCalls] = useState<ToolCallEventPayload[]>([]);
  const [liveMessages, setLiveMessages] = useState<MessageEventPayload[]>([]);
  const [editingRepo, setEditingRepo] = useState(false);
  const [repoInput, setRepoInput] = useState("");
  const bottomRef = useRef<HTMLDivElement>(null);
  const composerRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    setLiveCalls([]);
    setLiveMessages([]);
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
      }
    );
    const unlistenMsg = listen<MessageEventPayload>(
      "agent://message",
      (evt) => {
        if (evt.payload.session_id !== sessionId) return;
        setLiveMessages((prev) => [...prev, evt.payload]);
        if (evt.payload.role === "assistant") setSending(false);
      }
    );
    return () => {
      unlistenTool.then((f) => f());
      unlistenMsg.then((f) => f());
    };
  }, [sessionId]);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [liveMessages, liveCalls]);

  const send = async () => {
    if (!input.trim() || sending) return;
    setSending(true);
    const text = input;
    setInput("");
    setLiveCalls([]);
    try {
      await invoke("send_message", { sessionId, message: text });
    } catch (e) {
      setLiveMessages((prev) => [
        ...prev,
        { session_id: sessionId, role: "assistant", content: `Error: ${String(e)}` },
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

  const toggleSubagents = async () => {
    if (!session) return;
    const next = !session.subagents_enabled;
    await invoke("set_session_subagents", { id: sessionId, enabled: next });
    setSession((s) => (s ? { ...s, subagents_enabled: next } : s));
  };

  if (!session) return <div className="loading-screen">Loading session...</div>;

  // Prior turns come from the persisted session; the current turn streams
  // in live via events. Once a turn finishes it's saved into `messages` on
  // the backend, so reselecting this session later reloads it from there.
  const historyMessages = session.messages.filter(
    (m) => m.role === "user" || m.role === "assistant"
  );

  return (
    <div className="session-view">
      <div className="session-header">
        <h2>{session.title}</h2>
        <span className="hint">
          {session.mode}
          {session.planning_enabled ? " · planning on" : ""}
        </span>
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
            <button className="repo-badge" onClick={toggleSubagents}>
              {session.subagents_enabled ? "Sub-agents: on" : "Sub-agents: off"}
            </button>
          )}
        </div>
      </div>

      <div className="message-list">
        {historyMessages.map((m, i) => (
          <div key={"h" + i} className={"message " + m.role}>
            <span className="role-label">{m.role}</span>
            <p>{m.content}</p>
          </div>
        ))}

        {liveMessages.map((m, i) => (
          <div key={"l" + i} className={"message " + m.role}>
            <span className="role-label">{m.role}</span>
            <p>{m.content}</p>
          </div>
        ))}

        {liveCalls.length > 0 && (
          <div className="tool-stream">
            {liveCalls
              .filter((c) => !c.parent_call_id)
              .map((c) => {
                if (c.name === "delegate_to_subagent") {
                  const nested = liveCalls.filter(
                    (n) => n.parent_call_id === c.call_id
                  );
                  return (
                    <SubagentCard
                      key={c.call_id}
                      event={c}
                      calls={nested}
                      linkedRepo={session.linked_repo}
                      onPromptFix={promptFix}
                      onApprove={approve}
                    />
                  );
                }
                if (c.name.startsWith("github_")) {
                  return (
                    <GithubToolCard
                      key={c.call_id}
                      event={c}
                      linkedRepo={session.linked_repo}
                      onPromptFix={promptFix}
                    />
                  );
                }
                return (
                  <ToolCallRow key={c.call_id} event={c} onApprove={approve} />
                );
              })}
          </div>
        )}
        <div ref={bottomRef} />
      </div>

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
              ? "Ask the agent to do something..."
              : "Ask anything..."
          }
        />
        <button className="primary" onClick={send} disabled={sending}>
          {sending ? "Working..." : "Send"}
        </button>
      </div>
    </div>
  );
}
