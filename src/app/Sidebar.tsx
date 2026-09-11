import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import type { Session } from "../types";
import EngineStatusBadge from "./EngineStatusBadge";

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
          <button
            key={s.id}
            className={
              "session-item" + (s.id === activeSessionId ? " active" : "")
            }
            onClick={() => onSelectSession(s.id)}
          >
            <span className="session-title">{s.title}</span>
            <span className="session-mode">{s.mode}</span>
          </button>
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
