import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { open as openDialog } from "@tauri-apps/api/dialog";
import type { Workspace } from "../types";
import { getActiveWorkspace, setActiveWorkspace, subscribeActiveWorkspace } from "./agentStore";

export default function WorkspacePanel() {
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [activeWorkspacePath, setActiveWorkspacePathState] = useState<string | null>(
    getActiveWorkspace()
  );

  const loadWorkspaces = () => {
    invoke<Workspace[]>("list_workspaces").then((list) => {
      setWorkspaces(list);
      const currentActive = getActiveWorkspace();
      if (!currentActive && list.length > 0) {
        setActiveWorkspace(list[0].path);
      }
    });
  };

  useEffect(() => {
    loadWorkspaces();
    const unsub = subscribeActiveWorkspace(() => {
      setActiveWorkspacePathState(getActiveWorkspace());
    });
    return () => unsub();
  }, []);

  const addWorkspace = async () => {
    const selected = await openDialog({ directory: true, multiple: false });
    if (typeof selected !== "string") return;
    const workspace = await invoke<Workspace>("add_workspace", {
      name: null,
      path: selected,
    });
    setWorkspaces((list) => {
      const nextList = list.some((w) => w.id === workspace.id) ? list : [...list, workspace];
      return nextList;
    });
    // Set as active if no active workspace set
    if (!getActiveWorkspace()) {
      setActiveWorkspace(workspace.path);
    }
  };

  const removeWorkspace = async (id: string, path: string) => {
    await invoke("remove_workspace", { id });
    const remaining = workspaces.filter((w) => w.id !== id);
    setWorkspaces(remaining);
    if (activeWorkspacePath === path) {
      const nextActive = remaining.length > 0 ? remaining[0].path : null;
      setActiveWorkspace(nextActive);
    }
  };

  return (
    <div className="workspace-panel">
      <h2>Workspaces</h2>
      <p className="hint small">
        Folders the agent works in. Pick an active workspace to filter your session list and set the project context for new chats.
      </p>

      <div className="workspace-card-list">
        {workspaces.map((w) => {
          const isActive = activeWorkspacePath === w.path;
          return (
            <div
              key={w.id}
              className={`workspace-card ${isActive ? "active" : ""}`}
              onClick={() => setActiveWorkspace(w.path)}
            >
              <div className="workspace-card-info">
                <div className="workspace-card-header">
                  <span className="workspace-card-name">{w.name}</span>
                  {isActive && (
                    <span className="workspace-active-badge">
                      <span className="pulsing-dot" style={{ width: 8, height: 8 }} />
                      Active Workspace
                    </span>
                  )}
                </div>
                <span className="workspace-card-path">{w.path}</span>
              </div>
              <div className="workspace-card-actions">
                {!isActive && (
                  <button
                    className="small"
                    onClick={(e) => {
                      e.stopPropagation();
                      setActiveWorkspace(w.path);
                    }}
                  >
                    Set Active
                  </button>
                )}
                <button
                  className="small danger-btn"
                  onClick={(e) => {
                    e.stopPropagation();
                    removeWorkspace(w.id, w.path);
                  }}
                  title="Remove Workspace"
                >
                  Remove
                </button>
              </div>
            </div>
          );
        })}

        {workspaces.length === 0 && (
          <div className="workspace-empty-state">
            <p className="hint small">No workspaces added yet. Add a project folder to start working with the agent.</p>
          </div>
        )}
      </div>

      <button className="primary" onClick={addWorkspace} style={{ marginTop: 16 }}>
        Add folder
      </button>
    </div>
  );
}
