import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import Modal from "./Modal";
import { parsePatchDiff, type DiffLine } from "./diffUtils";
import type { GitDiff } from "../types";

interface Props {
  workspace: string;
  sessionId?: string;
  onClose: () => void;
}

function patchForPath(patch: string, path: string): string {
  const sections = patch.split(/(?=^diff --git )/m);
  const wanted = sections.filter((section) =>
    section.startsWith(`diff --git a/${path} b/${path}`) ||
    section.includes(`+++ b/${path}\n`) ||
    section.includes(`+++ b/${path}\r\n`) ||
    section.includes(`--- a/${path}\n`) ||
    section.includes(`--- a/${path}\r\n`)
  );
  return wanted.length > 0 ? wanted.join("") : patch;
}

function DiffCode({ patch }: { patch: string }) {
  const lines = useMemo(() => parsePatchDiff(patch), [patch]);
  return (
    <pre className="diff-view session-diff-code">
      {lines.map((line: DiffLine, index) => (
        <div key={`${index}-${line.text}`} className={`diff-line diff-${line.type}`}>
          <span className="diff-marker">{line.type === "add" ? "+" : line.type === "del" ? "−" : " "}</span>
          <span className="diff-text">{line.text || " "}</span>
        </div>
      ))}
    </pre>
  );
}

export default function SessionDiffViewer({ workspace, sessionId, onClose }: Props) {
  const [scope, setScope] = useState<"workspace" | "session">(sessionId ? "session" : "workspace");
  const [diff, setDiff] = useState<GitDiff | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const [activeFile, setActiveFile] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const next = await invoke<GitDiff>(scope === "session" ? "get_session_diff" : "get_git_diff", {
        workspace,
        ...(scope === "session" ? { sessionId } : {}),
      });
      setDiff(next);
      setSelected((current) => current.filter((path) => next.files.some((file) => file.path === path)));
      setActiveFile((current) => current && next.files.some((file) => file.path === current) ? current : next.files[0]?.path ?? null);
    } catch (e) {
      setError(String(e));
      setDiff(null);
    } finally {
      setLoading(false);
    }
  }, [scope, sessionId, workspace]);

  useEffect(() => { void refresh(); }, [refresh]);

  const runAction = async (command: "stage_git_files" | "unstage_git_files" | "revert_git_files") => {
    if (selected.length === 0) return;
    if (command === "revert_git_files" && !confirm(`Revert ${selected.length} selected file(s)? This cannot be undone.`)) return;
    setBusy(true);
    setError(null);
    try {
      await invoke(command, { workspace, paths: selected });
      await refresh();
      setSelected([]);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const toggle = (path: string) => {
    setSelected((current) => current.includes(path) ? current.filter((item) => item !== path) : [...current, path]);
  };
  const files = diff?.files ?? [];
  const activePatch = activeFile && diff ? patchForPath(diff.patch, activeFile) : diff?.patch ?? "";

  return (
    <Modal title="Session changes" onClose={onClose} className="session-diff-modal">
      <div className="session-diff-toolbar">
        <div className="session-diff-scope" role="group" aria-label="Diff scope">
          <button type="button" className={scope === "session" ? "active" : ""} disabled={!sessionId} onClick={() => setScope("session")}>Session files</button>
          <button type="button" className={scope === "workspace" ? "active" : ""} onClick={() => setScope("workspace")}>Whole workspace</button>
        </div>
        <button type="button" className="secondary" onClick={() => void refresh()} disabled={loading || busy}>Refresh</button>
      </div>
      {error && <div className="session-diff-error">{error}</div>}
      {loading ? <div className="loading-screen">Reading git diff…</div> : files.length === 0 ? (
        <div className="session-diff-empty">No uncommitted changes in this {scope}.</div>
      ) : (
        <div className="session-diff-layout">
          <aside className="session-diff-files">
            <div className="session-diff-selection">
              <button type="button" className="link-button" onClick={() => setSelected(selected.length === files.length ? [] : files.map((file) => file.path))}>
                {selected.length === files.length ? "Clear all" : "Select all"}
              </button>
              <span>{files.length} file{files.length === 1 ? "" : "s"}</span>
            </div>
            {files.map((file) => (
              <label key={file.path} className={`session-diff-file ${activeFile === file.path ? "active" : ""}`}>
                <input type="checkbox" checked={selected.includes(file.path)} onChange={() => toggle(file.path)} />
                <button type="button" className="session-diff-file-name" onClick={() => setActiveFile(file.path)} title={file.path}>
                  <span className="session-diff-status">{file.status}</span>{file.path}
                  <small>+{file.additions} −{file.deletions}{file.staged ? " · staged" : ""}</small>
                </button>
              </label>
            ))}
          </aside>
          <section className="session-diff-preview" aria-label="Diff preview">
            {activeFile ? <><div className="session-diff-preview-title">{activeFile}</div><DiffCode patch={activePatch} /></> : <div className="session-diff-empty">Select a file to review.</div>}
          </section>
        </div>
      )}
      {files.length > 0 && (
        <div className="session-diff-actions">
          <span>{selected.length ? `${selected.length} selected` : "Select files to act on"}</span>
          <div>
            <button type="button" className="secondary" disabled={busy || selected.length === 0} onClick={() => void runAction("stage_git_files")}>Stage</button>
            <button type="button" className="secondary" disabled={busy || selected.length === 0} onClick={() => void runAction("unstage_git_files")}>Unstage</button>
            <button type="button" className="danger" disabled={busy || selected.length === 0} onClick={() => void runAction("revert_git_files")}>Revert</button>
          </div>
        </div>
      )}
    </Modal>
  );
}
