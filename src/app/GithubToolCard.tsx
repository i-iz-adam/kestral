import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import Modal from "./Modal";
import type { ToolCallEventPayload, GithubComment } from "../types";

interface Props {
  event: ToolCallEventPayload;
  workspace: string;
  onPromptFix: (text: string) => void;
}

function tryParse(json?: string): any {
  if (!json) return null;
  try {
    return JSON.parse(json);
  } catch {
    return null;
  }
}

export default function GithubToolCard({ event, workspace, onPromptFix }: Props) {
  const [open, setOpen] = useState(false);
  const [comments, setComments] = useState<GithubComment[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  const parsed = event.status === "done" ? tryParse(event.result) : null;
  const argsObj = (event.args ?? {}) as Record<string, unknown>;

  const callAction = async (name: string, extraArgs: Record<string, unknown> = {}) => {
    setBusy(true);
    setActionError(null);
    try {
      await invoke("github_action", {
        name,
        args: { owner: argsObj.owner, repo: argsObj.repo, ...extraArgs },
        workspace,
      });
      return true;
    } catch (e) {
      setActionError(String(e));
      return false;
    } finally {
      setBusy(false);
    }
  };

  const openModal = async () => {
    setOpen(true);
    if (event.name === "github_get_issue" || event.name === "github_get_pr") {
      const number = (parsed?.number ?? argsObj.number) as number | undefined;
      if (number != null) {
        try {
          const result = await invoke<GithubComment[]>("github_action", {
            name: "github_list_issue_comments",
            args: { owner: argsObj.owner, repo: argsObj.repo, number },
            workspace,
          });
          setComments(result as unknown as GithubComment[]);
        } catch {
          setComments([]);
        }
      }
    }
  };

  if (event.status === "start" || event.status === "awaiting-approval") {
    return (
      <div className="tool-row github-card pending">
        <span className="tool-name">{event.name.replace("github_", "github: ")}</span>
        <span className="tool-status">{event.status}</span>
      </div>
    );
  }
  if (event.status === "error") {
    return (
      <div className="tool-row github-card error">
        <span className="tool-name">{event.name.replace("github_", "github: ")}</span>
        <span className="tool-status">{event.result?.slice(0, 200)}</span>
      </div>
    );
  }

  if (event.name === "github_get_issue" && parsed) {
    return (
      <>
        <button className="github-card issue" onClick={openModal}>
          <span className="github-icon">◆</span>
          <span className="github-title">
            Issue #{parsed.number}: {parsed.title}
          </span>
          <span className={"github-state " + parsed.state}>{parsed.state}</span>
        </button>
        {open && (
          <Modal title={`Issue #${parsed.number}`} onClose={() => setOpen(false)}>
            <h4>{parsed.title}</h4>
            <p className="modal-meta">
              {parsed.state} · opened by {parsed.user?.login ?? "unknown"}
            </p>
            <p className="modal-text">{parsed.body || "No description."}</p>

            {comments && comments.length > 0 && (
              <div className="modal-comments">
                <h5>Comments</h5>
                {comments.map((c) => (
                  <div key={c.id} className="modal-comment">
                    <span className="comment-author">{c.user?.login ?? "unknown"}</span>
                    <p>{c.body}</p>
                  </div>
                ))}
              </div>
            )}

            {actionError && <p className="fail">{actionError}</p>}

            <div className="modal-actions">
              <button
                disabled={busy || parsed.state === "closed"}
                onClick={() => callAction("github_close_issue", { number: parsed.number })}
              >
                Close issue
              </button>
              <button
                className="primary"
                onClick={() => {
                  onPromptFix(
                    `Fix issue #${parsed.number}: ${parsed.title}\n\n${parsed.body ?? ""}`
                  );
                  setOpen(false);
                }}
              >
                Prompt agent to fix
              </button>
            </div>
          </Modal>
        )}
      </>
    );
  }

  if (event.name === "github_get_pr" && parsed) {
    return (
      <>
        <button className="github-card pr" onClick={openModal}>
          <span className="github-icon">⇄</span>
          <span className="github-title">
            PR #{parsed.number}: {parsed.title}
          </span>
          <span className={"github-state " + parsed.state}>
            {parsed.merged ? "merged" : parsed.state}
          </span>
        </button>
        {open && (
          <Modal title={`PR #${parsed.number}`} onClose={() => setOpen(false)}>
            <h4>{parsed.title}</h4>
            <p className="modal-meta">
              {parsed.merged ? "merged" : parsed.state} · opened by{" "}
              {parsed.user?.login ?? "unknown"}
            </p>
            <p className="modal-text">{parsed.body || "No description."}</p>

            {comments && comments.length > 0 && (
              <div className="modal-comments">
                <h5>Comments</h5>
                {comments.map((c) => (
                  <div key={c.id} className="modal-comment">
                    <span className="comment-author">{c.user?.login ?? "unknown"}</span>
                    <p>{c.body}</p>
                  </div>
                ))}
              </div>
            )}

            {actionError && <p className="fail">{actionError}</p>}

            <div className="modal-actions">
              <button
                disabled={busy || !!parsed.merged}
                onClick={() => callAction("github_merge_pr", { number: parsed.number })}
              >
                Merge
              </button>
              <button
                className="primary"
                onClick={() => {
                  onPromptFix(
                    `Review PR #${parsed.number}: ${parsed.title}\n\n${parsed.body ?? ""}`
                  );
                  setOpen(false);
                }}
              >
                Prompt agent to review
              </button>
            </div>
            <p className="hint small">
              Inline diff comments aren't wired up yet — merge, plus a
              summary review via chat, are what's here for now.
            </p>
          </Modal>
        )}
      </>
    );
  }

  if (
    (event.name === "github_list_issues" || event.name === "github_list_open_prs") &&
    Array.isArray(parsed)
  ) {
    const isIssues = event.name === "github_list_issues";
    return (
      <div className="tool-row github-card list">
        <span className="tool-name">
          {isIssues ? "Open issues" : "Open PRs"} ({parsed.length})
        </span>
        <div className="github-list">
          {parsed.slice(0, 8).map((item: any) => (
            <button
              key={item.number}
              className="github-list-item"
              onClick={() =>
                onPromptFix(
                  `${isIssues ? "Fix issue" : "Review PR"} #${item.number}: ${item.title}`
                )
              }
            >
              #{item.number} {item.title}
            </button>
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className="tool-row github-card done">
      <span className="tool-name">{event.name.replace("github_", "github: ")}</span>
      <span className="tool-status">done</span>
    </div>
  );
}
