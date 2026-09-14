import { useEffect, useState } from "react";
import type { ToolCallEventPayload } from "../types";

/// A short, tool-specific summary of the call's arguments shown inline
/// next to the tool name — e.g. the path being edited or the query being
/// searched for — so the stream reads as "what's it doing" at a glance
/// instead of just a bare tool name and a status word.
function summarizeArgs(name: string, args: unknown): string | null {
  if (!args || typeof args !== "object") return null;
  const a = args as Record<string, unknown>;
  const str = (v: unknown) => (typeof v === "string" ? v : undefined);

  switch (name) {
    case "read_file":
    case "write_file":
      return str(a.path) ?? null;
    case "edit_file": {
      const path = str(a.path);
      const n = Array.isArray(a.edits) ? a.edits.length : undefined;
      if (path && n) return `${path} (${n} edit${n === 1 ? "" : "s"})`;
      return path ?? null;
    }
    case "apply_patch": {
      const patch = str(a.patch);
      if (!patch) return null;
      const fileCount = (patch.match(/^--- /gm) || []).length;
      return fileCount ? `${fileCount} file${fileCount === 1 ? "" : "s"}` : null;
    }
    case "list_dir":
      return str(a.path) ?? ".";
    case "search_code":
    case "web_search":
      return str(a.query) ? `"${a.query}"` : null;
    case "web_fetch":
      return str(a.url) ?? null;
    case "find_files":
      return str(a.pattern) ?? null;
    case "run_shell":
      return str(a.command) ?? null;
    case "run_python": {
      const code = str(a.code);
      if (!code) return null;
      const firstLine = code.trim().split("\n")[0];
      return firstLine.length > 50 ? firstLine.slice(0, 50) + "..." : firstLine;
    }
    default:
      return null;
  }
}

export default function ToolCallRow({
  event,
  onApprove,
}: {
  event: ToolCallEventPayload;
  onApprove: (callId: string, approved: boolean) => void;
}) {
  const argSummary = summarizeArgs(event.name, event.args);

  // This component mounts exactly once per call_id, right as the real
  // "start" event arrives (see the stable key in SessionView) — so
  // checking status at mount time, once, is enough to know "this row is
  // igniting because a tool call actually just began" rather than
  // replaying the burst on every subsequent status update.
  const [igniting, setIgniting] = useState(() => event.status === "start");
  useEffect(() => {
    if (!igniting) return;
    const t = setTimeout(() => setIgniting(false), 550);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className={"tool-row " + event.status + (igniting ? " ignite" : "")}>
      <span className="tool-name">{event.name}</span>
      {argSummary && <span className="tool-arg">{argSummary}</span>}
      <span className="tool-status">{event.status}</span>
      {event.status === "awaiting-approval" && (
        <div className="tool-approve">
          <button onClick={() => onApprove(event.call_id, true)}>
            Approve
          </button>
          <button onClick={() => onApprove(event.call_id, false)}>
            Reject
          </button>
        </div>
      )}
      {event.result && (
        <pre className="tool-result">{event.result.slice(0, 400)}</pre>
      )}
    </div>
  );
}
