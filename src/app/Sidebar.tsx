import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import type { Session } from "../types";
import EngineStatusBadge from "./EngineStatusBadge";
import { useAgentSession } from "./useAgentSession";

type Mode = "coding" | "general";

interface Props {
  activeSessionId: string | null;
  onSelectSession: (id: string) => void;
  onOpenSettings: () => void;
  onOpenGithub: () => void;
  onOpenSkills: () => void;
  onOpenAbout: () => void;
  onOpenProviders: () => void;
  onSessionCreated: (id: string) => void;
  refreshKey: number;
}

export default function Sidebar({
  activeSessionId,
  onSelectSession,
  onOpenSettings,
  onOpenGithub,
  onOpenSkills,
  onOpenAbout,
  onOpenProviders,
  onSessionCreated,
  refreshKey,
}: Props) {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [mode, setMode] = useState<Mode>("coding");
  const [planning, setPlanning] = useState(true);
  const [subagents, setSubagents] = useState(true);
  const [repo, setRepo] = useState("");
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<Session[]>("list_sessions").then(setSessions);
  }, [refreshKey]);

  const createSession = async () => {
    setError(null);
    setCreating(true);
    try {
      const title = mode === "coding" ? "New coding session" : "New chat";
      const session = await invoke<Session>("create_session", {
        title,
        mode,
        planningEnabled: planning,
        repo: repo.trim() || null,
        subagentsEnabled: subagents,
      });
      onSessionCreated(session.id);
    } catch (e) {
      setError(String(e));
    } finally {
      setCreating(false);
    }
  };

  return (
    <div className="sidebar">
      <div className="mode-toggle small">
        <button
          className={mode === "coding" ? "active" : ""}
          onClick={() => setMode("coding")}
        >
          Coding
        </button>
        <button
          className={mode === "general" ? "active" : ""}
          onClick={() => setMode("general")}
        >
          General AI
        </button>
      </div>

      {mode === "coding" && (
        <>
          <label className="checkbox-row">
            <input
              type="checkbox"
              checked={planning}
              onChange={(e) => setPlanning(e.target.checked)}
            />
            Planning mode (approve writes/commands)
          </label>
          <label className="checkbox-row">
            <input
              type="checkbox"
              checked={subagents}
              onChange={(e) => setSubagents(e.target.checked)}
            />
            Use sub-agents to keep context clean (recommended)
          </label>
          <input
            className="repo-input"
            value={repo}
            onChange={(e) => setRepo(e.target.value)}
            placeholder="owner/repo (optional)"
          />
        </>
      )}

      <button className="primary" onClick={createSession} disabled={creating}>
        {creating ? "Creating..." : "New session"}
      </button>
      {error && <p className="fail small">{error}</p>}

      <div className="session-list">
        {sessions.map((s) => (
          <SessionListItem
            key={s.id}
            session={s}
            active={s.id === activeSessionId}
            onSelect={() => onSelectSession(s.id)}
          />
        ))}
        {sessions.length === 0 && (
          <p className="hint small">No sessions yet.</p>
        )}
      </div>

      <EngineStatusBadge />

      <div className="sidebar-nav">
        <div className="sidebar-footer-row">
          <button onClick={onOpenProviders}>Providers</button>
          <button onClick={onOpenSkills}>Skills</button>
        </div>
        <div className="sidebar-footer-row">
          <button onClick={onOpenGithub}>GitHub</button>
          <button onClick={onOpenSettings}>Settings</button>
        </div>
        <div className="sidebar-footer-row">
          <button onClick={onOpenAbout}>About</button>
        </div>
      </div>
    </div>
  );
}

/** A session's own row subscribes to the global agent store directly, so
 * "this session is working right now" stays visible in the list even
 * while you're looking at a different one — the whole point of turns now
 * running independently of whichever view happens to be open. */
function SessionListItem({
  session,
  active,
  onSelect,
}: {
  session: Session;
  active: boolean;
  onSelect: () => void;
}) {
  const { sending } = useAgentSession(session.id);
  return (
    <button
      className={"session-item" + (active ? " active" : "")}
      onClick={onSelect}
    >
      <span className="session-title">{session.title}</span>
      <span className="session-item-right">
        {sending && <span className="session-working-dot" title="Working..." />}
        <span className="session-mode">{session.mode}</span>
      </span>
    </button>
  );
}
