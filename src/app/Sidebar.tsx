import { useEffect, useState, type ReactNode, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Session, Workspace } from "../types";
import EngineStatusBadge from "./EngineStatusBadge";
import { useAgentSession } from "./useAgentSession";
import {
  subscribeAny,
  getActiveWorkspace,
  subscribeActiveWorkspace,
  getRecord,
} from "./agentStore";

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
  const [activeWorkspacePath, setActiveWorkspacePathState] = useState<string | null>(
    getActiveWorkspace()
  );
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [, setTick] = useState(0);

  // Issue #35, #36, #37, #38 state
  const [selectedSessionIds, setSelectedSessionIds] = useState<string[]>([]);
  const [isBulkMode, setIsBulkMode] = useState(false);
  const [showArchived, setShowArchived] = useState(false);
  const [otherWorkspacesOpen, setOtherWorkspacesOpen] = useState(false);

  // Context menu state
  const [contextMenuSessionId, setContextMenuSessionId] = useState<string | null>(null);
  const [contextMenuPos, setContextMenuPos] = useState<{ x: number; y: number } | null>(null);

  // Renaming state
  const [renamingSessionId, setRenamingSessionId] = useState<string | null>(null);
  const [renameInput, setRenameInput] = useState("");

  const menuRef = useRef<HTMLDivElement>(null);

  // Close context menu on outside click
  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setContextMenuSessionId(null);
        setContextMenuPos(null);
      }
    };
    window.addEventListener("mousedown", handleClickOutside);
    return () => window.removeEventListener("mousedown", handleClickOutside);
  }, []);

  // Refresh workspaces list
  const refreshWorkspaces = useCallback(() => {
    invoke<Workspace[]>("list_workspaces")
      .then((list) => {
        setWorkspaces(list);
      })
      .catch(() => {});
  }, []);

  const fetchSessions = useCallback(() => {
    invoke<Session[]>("list_sessions").then(setSessions);
  }, []);

  // Initial load
  useEffect(() => {
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
  }, [refreshKey, refreshWorkspaces, fetchSessions]);

  // Get workspace name from path
  const getWorkspaceName = useCallback(
    (path: string | null): string => {
      if (!path) return "Unknown Workspace";
      const ws = workspaces.find((w) => w.path === path);
      return ws?.name ?? path.split("/").pop() ?? path;
    },
    [workspaces]
  );

  // Filter archived / unarchived
  const activeOrArchivedSessions = sessions.filter((s) => {
    if (showArchived) return s.archived;
    return !s.archived;
  });

  // Split sessions into current workspace vs other workspaces
  const currentWorkspaceSessions = activeOrArchivedSessions.filter((s) => {
    if (!activeWorkspacePath) return true;
    if (s.workspace === activeWorkspacePath) return true;
    const rec = getRecord(s.id);
    return rec.sending || rec.unseenActivity;
  });

  const otherWorkspaceSessions = activeOrArchivedSessions.filter((s) => {
    if (!activeWorkspacePath) return false;
    if (s.workspace === activeWorkspacePath) return false;
    const rec = getRecord(s.id);
    return !(rec.sending || rec.unseenActivity);
  });

  // Sort sessions: Pinned first (#36), active running next, then updated_at descending
  const sortSessions = (list: Session[]) => {
    return [...list].sort((a, b) => {
      const aPinned = a.pinned ? 1 : 0;
      const bPinned = b.pinned ? 1 : 0;
      if (aPinned !== bPinned) return bPinned - aPinned;

      const aActive = getRecord(a.id).sending ? 1 : 0;
      const bActive = getRecord(b.id).sending ? 1 : 0;
      if (aActive !== bActive) return bActive - aActive;

      const aTime = a.updated_at ?? a.created_at;
      const bTime = b.updated_at ?? b.created_at;
      return bTime - aTime;
    });
  };

  const sortedCurrentSessions = sortSessions(currentWorkspaceSessions);
  const sortedOtherSessions = sortSessions(otherWorkspaceSessions);

  const createSession = async () => {
    setError(null);
    setCreating(true);
    try {
      const title = "New session";
      const session = await invoke<Session>("create_session", {
        title,
        mode: "coding",
        workspace: getActiveWorkspace(),
      });
      fetchSessions();
      onSessionCreated(session.id);
    } catch (e) {
      setError(String(e));
    } finally {
      setCreating(false);
    }
  };

  // Session actions (#35)
  const handlePin = async (id: string, pinned: boolean) => {
    await invoke("set_session_pinned", { id, pinned }).catch(() => {});
    fetchSessions();
    setContextMenuSessionId(null);
  };

  const handleArchive = async (id: string, archived: boolean) => {
    await invoke("set_session_archived", { id, archived }).catch(() => {});
    fetchSessions();
    setContextMenuSessionId(null);
  };

  const handleDelete = async (id: string) => {
    if (!confirm("Are you sure you want to delete this session?")) return;
    await invoke("delete_session", { id }).catch(() => {});
    fetchSessions();
    setContextMenuSessionId(null);
  };

  const handleStartRename = (session: Session) => {
    setRenamingSessionId(session.id);
    setRenameInput(session.title);
    setContextMenuSessionId(null);
  };

  const handleSaveRename = async (id: string) => {
    if (!renameInput.trim()) return;
    await invoke("set_session_title", { id, title: renameInput.trim() }).catch(() => {});
    setRenamingSessionId(null);
    fetchSessions();
  };

  // Bulk actions (#38)
  const toggleSelectSession = (id: string) => {
    setSelectedSessionIds((prev) =>
      prev.includes(id) ? prev.filter((i) => i !== id) : [...prev, id]
    );
  };

  const handleBulkDelete = async () => {
    if (!confirm(`Delete ${selectedSessionIds.length} selected session(s)?`)) return;
    for (const id of selectedSessionIds) {
      await invoke("delete_session", { id }).catch(() => {});
    }
    setSelectedSessionIds([]);
    setIsBulkMode(false);
    fetchSessions();
  };

  const handleBulkExportMarkdown = () => {
    const selected = sessions.filter((s) => selectedSessionIds.includes(s.id));
    let md = `# Kestrel Sessions Export\nExported on ${new Date().toLocaleString()}\n\n---`;
    for (const s of selected) {
      md += `\n\n## Session: ${s.title}\n- **ID**: ${s.id}\n- **Mode**: ${s.mode}\n- **Workspace**: ${s.workspace}\n- **Created**: ${new Date(s.created_at).toLocaleString()}\n\n### Messages:\n`;
      for (const m of s.messages) {
        md += `\n**${m.role.toUpperCase()}**:\n${m.content ?? (m.tool_calls ? JSON.stringify(m.tool_calls) : "")}\n`;
      }
      md += `\n---`;
    }
    const blob = new Blob([md], { type: "text/markdown;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `kestrel-sessions-export-${Date.now()}.md`;
    a.click();
    URL.revokeObjectURL(url);
  };

  const handleBulkExportJson = () => {
    const selected = sessions.filter((s) => selectedSessionIds.includes(s.id));
    const jsonStr = JSON.stringify(selected, null, 2);
    const blob = new Blob([jsonStr], { type: "application/json;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `kestrel-sessions-export-${Date.now()}.json`;
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <div className="sidebar">
      <div style={{ display: "flex", gap: 6 }}>
        <button className="primary" onClick={createSession} disabled={creating} style={{ flex: 1, marginTop: 8 }}>
          {creating ? "Creating..." : "New session"}
        </button>
        <button
          className={isBulkMode ? "active" : ""}
          onClick={() => {
            setIsBulkMode(!isBulkMode);
            setSelectedSessionIds([]);
          }}
          title="Select multiple sessions for bulk actions"
          style={{ marginTop: 8, padding: "0 10px", background: isBulkMode ? "var(--surface-2)" : "transparent", border: "1px solid var(--border)", borderRadius: 8, color: "var(--text-dim)", cursor: "pointer" }}
        >
          ☑️
        </button>
      </div>
      {error && <p className="fail small">{error}</p>}

      {/* Bulk action bar (#38) */}
      {isBulkMode && (
        <div className="bulk-action-bar">
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
            <span style={{ fontWeight: 600 }}>{selectedSessionIds.length} selected</span>
            <button
              onClick={() => {
                if (selectedSessionIds.length === sessions.length) {
                  setSelectedSessionIds([]);
                } else {
                  setSelectedSessionIds(sessions.map((s) => s.id));
                }
              }}
              style={{ background: "none", border: "none", color: "var(--accent)", cursor: "pointer", fontSize: "11px" }}
            >
              {selectedSessionIds.length === sessions.length ? "Deselect All" : "Select All"}
            </button>
          </div>
          <div className="bulk-action-buttons">
            <button onClick={handleBulkExportMarkdown} disabled={selectedSessionIds.length === 0}>
              Export MD
            </button>
            <button onClick={handleBulkExportJson} disabled={selectedSessionIds.length === 0}>
              Export JSON
            </button>
            <button className="danger" onClick={handleBulkDelete} disabled={selectedSessionIds.length === 0}>
              Delete
            </button>
          </div>
        </div>
      )}

      <div className="session-list" style={{ marginTop: 12 }}>
        {/* Toggle between Active and Archived sessions */}
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 6, padding: "0 4px" }}>
          <span className="small" style={{ color: "var(--text-dim)", textTransform: "uppercase", letterSpacing: "0.04em" }}>
            {showArchived ? "Archived Sessions" : "Sessions"}
          </span>
          <button
            onClick={() => setShowArchived(!showArchived)}
            style={{ background: "none", border: "none", color: "var(--accent)", cursor: "pointer", fontSize: "11px" }}
          >
            {showArchived ? "Active" : `Archive (${sessions.filter((s) => s.archived).length})`}
          </button>
        </div>

        {/* Current Workspace Sessions */}
        {sortedCurrentSessions.map((s) => (
          <SessionItemRow
            key={s.id}
            session={s}
            active={s.id === activeSessionId}
            onSelect={() => onSelectSession(s.id)}
            showWorkspace={activeWorkspacePath !== null && s.workspace !== activeWorkspacePath}
            getWorkspaceName={getWorkspaceName}
            isBulkMode={isBulkMode}
            isSelected={selectedSessionIds.includes(s.id)}
            onToggleSelect={() => toggleSelectSession(s.id)}
            isRenaming={renamingSessionId === s.id}
            renameInput={renameInput}
            onRenameChange={setRenameInput}
            onSaveRename={() => handleSaveRename(s.id)}
            onCancelRename={() => setRenamingSessionId(null)}
            onContextMenu={(e) => {
              e.preventDefault();
              setContextMenuSessionId(s.id);
              setContextMenuPos({ x: e.clientX, y: e.clientY });
            }}
            onOpenMenu={(e) => {
              const rect = e.currentTarget.getBoundingClientRect();
              setContextMenuSessionId(s.id);
              setContextMenuPos({ x: rect.left, y: rect.bottom + 4 });
            }}
          />
        ))}

        {sortedCurrentSessions.length === 0 && !showArchived && (
          <p className="hint small">No sessions yet for this workspace.</p>
        )}

        {/* Other Workspaces Section (#37) */}
        {!showArchived && sortedOtherSessions.length > 0 && (
          <>
            <div
              className="workspace-section-header"
              onClick={() => setOtherWorkspacesOpen(!otherWorkspacesOpen)}
            >
              <span>Other workspaces ({sortedOtherSessions.length})</span>
              <span>{otherWorkspacesOpen ? "▼" : "▶"}</span>
            </div>
            {otherWorkspacesOpen &&
              sortedOtherSessions.map((s) => (
                <SessionItemRow
                  key={s.id}
                  session={s}
                  active={s.id === activeSessionId}
                  onSelect={() => onSelectSession(s.id)}
                  showWorkspace={true}
                  getWorkspaceName={getWorkspaceName}
                  isBulkMode={isBulkMode}
                  isSelected={selectedSessionIds.includes(s.id)}
                  onToggleSelect={() => toggleSelectSession(s.id)}
                  isRenaming={renamingSessionId === s.id}
                  renameInput={renameInput}
                  onRenameChange={setRenameInput}
                  onSaveRename={() => handleSaveRename(s.id)}
                  onCancelRename={() => setRenamingSessionId(null)}
                  onContextMenu={(e) => {
                    e.preventDefault();
                    setContextMenuSessionId(s.id);
                    setContextMenuPos({ x: e.clientX, y: e.clientY });
                  }}
                  onOpenMenu={(e) => {
                    const rect = e.currentTarget.getBoundingClientRect();
                    setContextMenuSessionId(s.id);
                    setContextMenuPos({ x: rect.left, y: rect.bottom + 4 });
                  }}
                />
              ))}
          </>
        )}
      </div>

      {/* Context Menu Popup (#35) */}
      {contextMenuSessionId && contextMenuPos && (
        <div
          ref={menuRef}
          className="session-context-menu"
          style={{ top: contextMenuPos.y, left: contextMenuPos.x }}
        >
          {(() => {
            const targetSession = sessions.find((s) => s.id === contextMenuSessionId);
            if (!targetSession) return null;
            return (
              <>
                <button onClick={() => handlePin(targetSession.id, !targetSession.pinned)}>
                  {targetSession.pinned ? "📌 Unpin" : "📌 Pin to top"}
                </button>
                <button onClick={() => handleStartRename(targetSession)}>✏️ Rename</button>
                <button onClick={() => handleArchive(targetSession.id, !targetSession.archived)}>
                  {targetSession.archived ? "📂 Unarchive" : "📦 Archive"}
                </button>
                <button
                  onClick={() => handleDelete(targetSession.id)}
                  style={{ color: "var(--danger)" }}
                >
                  🗑️ Delete
                </button>
              </>
            );
          })()}
        </div>
      )}

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

function SessionItemRow({
  session,
  active,
  onSelect,
  showWorkspace,
  getWorkspaceName,
  isBulkMode,
  isSelected,
  onToggleSelect,
  isRenaming,
  renameInput,
  onRenameChange,
  onSaveRename,
  onCancelRename,
  onContextMenu,
  onOpenMenu,
}: {
  session: Session;
  active: boolean;
  onSelect: () => void;
  showWorkspace?: boolean;
  getWorkspaceName?: (path: string | null) => string;
  isBulkMode: boolean;
  isSelected: boolean;
  onToggleSelect: () => void;
  isRenaming: boolean;
  renameInput: string;
  onRenameChange: (val: string) => void;
  onSaveRename: () => void;
  onCancelRename: () => void;
  onContextMenu: (e: React.MouseEvent) => void;
  onOpenMenu: (e: React.MouseEvent) => void;
}) {
  const { sending, unseenActivity } = useAgentSession(session.id);
  const displayTitle = session.title.length > 20 ? `${session.title.slice(0, 20)}...` : session.title;

  if (isRenaming) {
    return (
      <div style={{ display: "flex", gap: 4, padding: "6px", background: "var(--surface-2)", borderRadius: 8 }}>
        <input
          type="text"
          value={renameInput}
          onChange={(e) => onRenameChange(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") onSaveRename();
            if (e.key === "Escape") onCancelRename();
          }}
          autoFocus
          style={{ flex: 1, background: "var(--bg)", border: "1px solid var(--border)", borderRadius: 4, color: "var(--text)", padding: "4px 6px", fontSize: "13px" }}
        />
        <button onClick={onSaveRename} style={{ background: "var(--accent)", border: "none", color: "#fff", borderRadius: 4, padding: "2px 8px", cursor: "pointer", fontSize: "11px" }}>Save</button>
      </div>
    );
  }

  return (
    <div
      className={
        "session-item" +
        (session.mode === "general" ? " mode-general" : "") +
        (active ? " active" : "") +
        (unseenActivity ? " has-activity" : "")
      }
      onClick={onSelect}
      onContextMenu={onContextMenu}
    >
      <div className="session-item-row">
        {isBulkMode && (
          <input
            type="checkbox"
            className="session-checkbox"
            checked={isSelected}
            onChange={onToggleSelect}
            onClick={(e) => e.stopPropagation()}
          />
        )}
        {session.pinned && <span className="pinned-badge" title="Pinned">📌</span>}
        <span className="session-title" title={session.title}>{displayTitle}</span>
      </div>

      <span className="session-item-right">
        {sending && <span className="session-working-dot" title="Working..." />}
        {!sending && unseenActivity && (
          <span className="session-ready-dot" title="Finished while you were away" />
        )}
        <button
          className="session-action-btn"
          onClick={(e) => {
            e.stopPropagation();
            onOpenMenu(e);
          }}
          title="Session options"
        >
          ⋮
        </button>
      </span>
      {showWorkspace && session.workspace && getWorkspaceName && (
        <span className="session-workspace external" title={session.workspace}>
          {getWorkspaceName(session.workspace)}
        </span>
      )}
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
