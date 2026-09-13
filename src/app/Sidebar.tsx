import { useEffect, useState, type ReactNode, useCallback } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import type { Session, Workspace } from "../types";
import EngineStatusBadge from "./EngineStatusBadge";
import WorkspacePicker from "./WorkspacePicker";
import { useAgentSession } from "./useAgentSession";
import {
  subscribeAny,
  getCurrentWorkspaceFilter,
  setCurrentWorkspaceFilter,
  subscribeWorkspaceFilter,
  getSendingSessionsOutsideWorkspace,
} from "./agentStore";

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
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [mode, setMode] = useState<Mode>("coding");
  const [workspacePath, setWorkspacePath] = useState<string | null>(null);
  const [currentWorkspaceFilter, setCurrentWorkspaceFilterState] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Refresh workspaces list
  const refreshWorkspaces = useCallback(() => {
    invoke<Workspace[]>("list_workspaces")
      .then(setWorkspaces)
      .catch(() => {});
  }, []);

  // Initial load
  useEffect(() => {
    const fetchSessions = () => {
      invoke<Session[]>("list_sessions").then(setSessions);
    };
    fetchSessions();
    refreshWorkspaces();

    // Subscribe to session changes
    const unsubSessions = subscribeAny(fetchSessions);
    // Subscribe to workspace filter changes
    const unsubFilter = subscribeWorkspaceFilter(() => {
      setCurrentWorkspaceFilterState(getCurrentWorkspaceFilter());
    });

    return () => {
      unsubSessions();
      unsubFilter();
    };
  }, [refreshKey, refreshWorkspaces]);

  // Get workspace name from path
  const getWorkspaceName = useCallback(
    (path: string | null): string => {
      if (!path) return "Unknown";
      const ws = workspaces.find((w) => w.path === path);
      return ws?.name ?? path.split("/").pop() ?? path;
    },
    [workspaces]
  );

  // Filter sessions by current workspace filter
  const filteredSessions = sessions.filter((s) => {
    if (currentWorkspaceFilter === null) return true;
    return s.workspace === currentWorkspaceFilter;
  });

  // Sessions from other workspaces with active agents
  const externalActiveSessions = currentWorkspaceFilter
    ? getSendingSessionsOutsideWorkspace(currentWorkspaceFilter).filter(
        (rec) => rec.session
      )
    : [];

  const handleWorkspaceFilterChange = (path: string | null) => {
    setCurrentWorkspaceFilter(path);
  };

  const createSession = async () => {
    setError(null);
    setCreating(true);
    try {
      const title = mode === "coding" ? "New coding session" : "New chat";
      const session = await invoke<Session>("create_session", {
        title,
        mode,
        workspace: workspacePath,
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

      <div className="field-group compact">
        <label>Workspace</label>
        <WorkspacePicker value={workspacePath} onChange={setWorkspacePath} autoSelectFirst />
      </div>

      <button className="primary" onClick={createSession} disabled={creating}>
        {creating ? "Creating..." : "New session"}
      </button>
      {error && <p className="fail small">{error}</p>}

      {/* Workspace filter dropdown */}
      <div className="field-group compact workspace-filter">
        <label>Filter by workspace</label>
        <select
          className="workspace-picker"
          value={currentWorkspaceFilter ?? ""}
          onChange={(e) => handleWorkspaceFilterChange(e.target.value || null)}
        >
          <option value="">All workspaces</option>
          {workspaces.map((w) => (
            <option key={w.id} value={w.path}>
              {w.name}
            </option>
          ))}
        </select>
      </div>

      {/* External active sessions (from other workspaces) */}
      {externalActiveSessions.length > 0 && (
        <div className="external-active-section">
          <p className="hint small">Running in other workspaces:</p>
          {externalActiveSessions.map((rec) =>
            rec.session ? (
              <ExternalSessionItem
                key={rec.session.id}
                session={rec.session}
                onSelect={() => {
                  setCurrentWorkspaceFilter(rec.session!.workspace);
                  onSelectSession(rec.session!.id);
                }}
                getWorkspaceName={getWorkspaceName}
              />
            ) : null
          )}
        </div>
      )}

      <div className="session-list">
        {filteredSessions.map((s) => (
          <SessionListItem
            key={s.id}
            session={s}
            active={s.id === activeSessionId}
            onSelect={() => onSelectSession(s.id)}
            showWorkspace={currentWorkspaceFilter === null}
            getWorkspaceName={getWorkspaceName}
          />
        ))}
        {filteredSessions.length === 0 && externalActiveSessions.length === 0 && (
          <p className="hint small">No sessions yet.</p>
        )}
      </div>

      <EngineStatusBadge />

      <div className="sidebar-nav">
        <IconRailButton label="Providers" onClick={onOpenProviders}>
          <PlugIcon />
        </IconRailButton>
        <IconRailButton label="Skills" onClick={onOpenSkills}>
          <BookIcon />
        </IconRailButton>
        <IconRailButton label="GitHub" onClick={onOpenGithub}>
          <GithubMarkIcon />
        </IconRailButton>
        <IconRailButton label="Settings" onClick={onOpenSettings}>
          <GearIcon />
        </IconRailButton>
        <IconRailButton label="About" onClick={onOpenAbout}>
          <InfoIcon />
        </IconRailButton>
      </div>
    </div>
  );
}

/** One slot in the footer icon rail — a square icon button with the
 * label only shown as a tooltip and a small caption underneath, so five
 * destinations that used to be five full-width text buttons (three rows)
 * now sit in a single compact row. */
function IconRailButton({
  label,
  onClick,
  children,
}: {
  label: string;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button className="icon-rail-btn" onClick={onClick} title={label} aria-label={label}>
      {children}
      <span className="icon-rail-caption">{label}</span>
    </button>
  );
}

const ICON_PROPS = {
  width: 16,
  height: 16,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.6,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
};

function PlugIcon() {
  return (
    <svg {...ICON_PROPS}>
      <path d="M9 2v5M15 2v5M7 8h10l-1 5a4 4 0 0 1-4 3.2A4 4 0 0 1 8 13z" />
      <path d="M12 16.2V20" />
    </svg>
  );
}

function BookIcon() {
  return (
    <svg {...ICON_PROPS}>
      <path d="M4 5.5A2.5 2.5 0 0 1 6.5 3H20v15H6.5A2.5 2.5 0 0 0 4 20.5z" />
      <path d="M4 5.5v15A2.5 2.5 0 0 0 6.5 23H20" />
    </svg>
  );
}

function GithubMarkIcon() {
  return (
    <svg {...ICON_PROPS} strokeWidth={1.4}>
      <path d="M12 2a10 10 0 0 0-3.16 19.5c.5.1.68-.22.68-.48v-1.7c-2.78.6-3.37-1.34-3.37-1.34-.46-1.15-1.11-1.46-1.11-1.46-.9-.62.07-.6.07-.6 1 .07 1.53 1.03 1.53 1.03.9 1.52 2.34 1.08 2.91.83.09-.65.35-1.08.63-1.33-2.22-.25-4.56-1.11-4.56-4.94 0-1.09.39-1.98 1.03-2.68-.1-.25-.45-1.27.1-2.65 0 0 .84-.27 2.75 1.02a9.6 9.6 0 0 1 5 0c1.91-1.29 2.75-1.02 2.75-1.02.55 1.38.2 2.4.1 2.65.64.7 1.03 1.59 1.03 2.68 0 3.84-2.34 4.68-4.57 4.93.36.31.68.92.68 1.85v2.74c0 .27.18.58.69.48A10 10 0 0 0 12 2" />
    </svg>
  );
}

function GearIcon() {
  return (
    <svg {...ICON_PROPS}>
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.7 1.7 0 0 0 .34 1.87l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.87-.34 1.7 1.7 0 0 0-1.04 1.56V21a2 2 0 0 1-4 0v-.09A1.7 1.7 0 0 0 9 19.35a1.7 1.7 0 0 0-1.87.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.7 1.7 0 0 0 4.65 15a1.7 1.7 0 0 0-1.56-1.04H3a2 2 0 0 1 0-4h.09A1.7 1.7 0 0 0 4.65 9a1.7 1.7 0 0 0-.34-1.87l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.7 1.7 0 0 0 9 4.65a1.7 1.7 0 0 0 1.04-1.56V3a2 2 0 0 1 4 0v.09A1.7 1.7 0 0 0 15 4.65a1.7 1.7 0 0 0 1.87-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.7 1.7 0 0 0 19.35 9a1.7 1.7 0 0 0 1.56 1.04H21a2 2 0 0 1 0 4h-.09A1.7 1.7 0 0 0 19.4 15" />
    </svg>
  );
}

function InfoIcon() {
  return (
    <svg {...ICON_PROPS}>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 11v6M12 7.5h.01" />
    </svg>
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
  showWorkspace,
  getWorkspaceName,
}: {
  session: Session;
  active: boolean;
  onSelect: () => void;
  showWorkspace?: boolean;
  getWorkspaceName?: (path: string | null) => string;
}) {
  const { sending, unseenActivity } = useAgentSession(session.id);
  return (
    <button
      className={
        "session-item" +
        (session.mode === "general" ? " mode-general" : "") +
        (active ? " active" : "") +
        (unseenActivity ? " has-activity" : "")
      }
      onClick={onSelect}
    >
      <span className="session-title">{session.title}</span>
      <span className="session-item-right">
        {sending && <span className="session-working-dot" title="Working..." />}
        {!sending && unseenActivity && (
          <span className="session-ready-dot" title="Finished while you were away" />
        )}
        <span className="session-mode">{session.mode}</span>
      </span>
      {showWorkspace && session.workspace && getWorkspaceName && (
        <span className="session-workspace" title={session.workspace}>
          {getWorkspaceName(session.workspace)}
        </span>
      )}
    </button>
  );
}

/** A session running in another workspace — shown at the top of the
 * session list with a workspace indicator so users know where to look
 * if they want to check on it. */
function ExternalSessionItem({
  session,
  onSelect,
  getWorkspaceName,
}: {
  session: Session;
  onSelect: () => void;
  getWorkspaceName: (path: string | null) => string;
}) {
  const { sending } = useAgentSession(session.id);
  return (
    <button className="session-item external-active" onClick={onSelect}>
      <span className="session-title">{session.title}</span>
      <span className="session-item-right">
        {sending && <span className="session-working-dot" title="Working..." />}
        <span className="session-mode">{session.mode}</span>
      </span>
      <span className="session-workspace external" title={session.workspace}>
        <WorkspaceIcon />
        {getWorkspaceName(session.workspace)}
      </span>
    </button>
  );
}

function WorkspaceIcon() {
  return (
    <svg
      width={12}
      height={12}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.6}
      strokeLinecap="round"
      strokeLinejoin="round"
      style={{ marginRight: 4, opacity: 0.7 }}
    >
      <path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
      <polyline points="9,22 9,12 15,12 15,22" />
    </svg>
  );
}
