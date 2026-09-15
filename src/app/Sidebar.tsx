import { useEffect, useState, type ReactNode, useCallback } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import type { Session, Workspace } from "../types";
import EngineStatusBadge from "./EngineStatusBadge";
import { useAgentSession } from "./useAgentSession";
import {
  subscribeAny,
  getActiveWorkspace,
  subscribeActiveWorkspace,
  getRecord,
} from "./agentStore";

type Mode = "coding" | "general";

interface Props {
  activeSessionId: string | null;
  onSelectSession: (id: string) => void;
  onOpenSettings: () => void;
  onOpenWorkspace: () => void;
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
  onOpenWorkspace,
  onOpenSkills,
  onOpenAbout,
  onOpenProviders,
  onSessionCreated,
  refreshKey,
}: Props) {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [mode, setMode] = useState<Mode>("coding");
  const [activeWorkspacePath, setActiveWorkspacePathState] = useState<string | null>(
    getActiveWorkspace()
  );
  const [creating, setCreating] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [, setTick] = useState(0);

  // Refresh workspaces list
  const refreshWorkspaces = useCallback(() => {
    invoke<Workspace[]>("list_workspaces")
      .then((list) => {
        setWorkspaces(list);
        if (!getActiveWorkspace() && list.length > 0) {
          // If no active workspace is set, set default to first workspace
        }
      })
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
    const unsubSessions = subscribeAny(() => {
      fetchSessions();
      setTick((t) => t + 1);
    });
    // Subscribe to active workspace changes
    const unsubWorkspace = subscribeActiveWorkspace(() => {
      setActiveWorkspacePathState(getActiveWorkspace());
    });

    return () => {
      unsubSessions();
      unsubWorkspace();
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

  // Filter sessions: show chats for active workspace + any chats from other workspaces with running agents/unread completions
  const filteredSessions = sessions.filter((s) => {
    if (!activeWorkspacePath) return true;
    if (s.workspace === activeWorkspacePath) return true;
    const rec = getRecord(s.id);
    return rec.sending || rec.unseenActivity;
  });

  // Sort sessions: active runs float to the top; completed sessions follow ordered by updated_at (or created_at) descending.
  const sortedSessions = [...filteredSessions].sort((a, b) => {
    const aActive = getRecord(a.id).sending;
    const bActive = getRecord(b.id).sending;
    if (aActive !== bActive) {
      return aActive ? -1 : 1;
    }
    const aTime = a.updated_at ?? a.created_at;
    const bTime = b.updated_at ?? b.created_at;
    return bTime - aTime;
  });

  const searchedSessions = sortedSessions.filter((s) =>
    searchQuery.trim() === "" || s.title.toLowerCase().includes(searchQuery.toLowerCase())
  );

  const createSession = async () => {
    setError(null);
    setCreating(true);
    try {
      const title = mode === "coding" ? "New coding session" : "New chat";
      const session = await invoke<Session>("create_session", {
        title,
        mode,
        workspace: getActiveWorkspace(),
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

      <button className="primary" onClick={createSession} disabled={creating} style={{ marginTop: 8 }}>
        {creating ? "Creating..." : "New session"}
      </button>
      {error && <p className="fail small">{error}</p>}

      <div className="session-search-container" style={{ marginTop: 10 }}>
        <input
          type="text"
          placeholder="Search sessions..."
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          className="session-search-input"
        />
      </div>

      <div className="session-list" style={{ marginTop: 8 }}>
        {searchedSessions.map((s) => (
          <SessionListItem
            key={s.id}
            session={s}
            active={s.id === activeSessionId}
            onSelect={() => onSelectSession(s.id)}
            showWorkspace={activeWorkspacePath !== null && s.workspace !== activeWorkspacePath}
            getWorkspaceName={getWorkspaceName}
          />
        ))}
        {sortedSessions.length === 0 && (
          <p className="hint small">No sessions yet for this workspace.</p>
        )}
      </div>

      <EngineStatusBadge />

      <div className="sidebar-nav">
        <IconRailButton label="OmniRoute" onClick={onOpenProviders}>
          <PlugIcon />
        </IconRailButton>
        <IconRailButton label="Skills" onClick={onOpenSkills}>
          <BookIcon />
        </IconRailButton>
        <IconRailButton label="Workspace" onClick={onOpenWorkspace}>
          <FolderIcon />
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
 * label only shown as a tooltip and a small caption underneath. */
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

function FolderIcon() {
  return (
    <svg {...ICON_PROPS}>
      <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
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
        <span className="session-workspace external" title={session.workspace}>
          {getWorkspaceName(session.workspace)}
        </span>
      )}
    </button>
  );
}
