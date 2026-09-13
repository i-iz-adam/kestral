import { useEffect, useState, type ChangeEvent } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import { open } from "@tauri-apps/api/dialog";
import type { Workspace } from "../types";

const ADD_NEW = "__add_workspace__";

/** A themed folder picker used both when creating a new session and when
 * switching an existing one's workspace. Keeps its own list of known
 * workspaces (fetched once, refreshed whenever a new one is added) so it
 * can be dropped in wherever a folder needs picking without each call
 * site re-implementing the "browse or pick from recent" flow. */
export default function WorkspacePicker({
  value,
  onChange,
  className,
  autoSelectFirst = false,
}: {
  value: string | null;
  onChange: (path: string) => void;
  className?: string;
  /** If nothing is selected yet, default to the most recently added
   * workspace instead of leaving the control on a blank/disabled option —
   * used for the "new session" picker, not when editing an existing
   * session (which always has an explicit workspace already). */
  autoSelectFirst?: boolean;
}) {
  const [workspaces, setWorkspaces] = useState<Workspace[]>([]);
  const [busy, setBusy] = useState(false);

  const refresh = () =>
    invoke<Workspace[]>("list_workspaces").then(setWorkspaces).catch(() => {});

  useEffect(() => {
    refresh();
  }, []);

  useEffect(() => {
    if (autoSelectFirst && !value && workspaces.length > 0) {
      onChange(workspaces[workspaces.length - 1].path);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workspaces, autoSelectFirst]);

  const addFolder = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected !== "string") return;
    setBusy(true);
    try {
      const workspace = await invoke<Workspace>("add_workspace", {
        name: null,
        path: selected,
      });
      setWorkspaces((list) =>
        list.some((w) => w.id === workspace.id) ? list : [...list, workspace]
      );
      onChange(workspace.path);
    } finally {
      setBusy(false);
    }
  };

  const handleChange = (e: ChangeEvent<HTMLSelectElement>) => {
    if (e.target.value === ADD_NEW) {
      addFolder();
      return;
    }
    onChange(e.target.value);
  };

  return (
    <select
      className={"workspace-picker" + (className ? " " + className : "")}
      value={value ?? ""}
      onChange={handleChange}
      disabled={busy}
      title={value ?? undefined}
    >
      {!value && (
        <option value="" disabled>
          Choose a folder...
        </option>
      )}
      {workspaces.map((w) => (
        <option key={w.id} value={w.path}>
          {w.name}
        </option>
      ))}
      <option value={ADD_NEW}>+ Add folder...</option>
    </select>
  );
}
