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
      return str(a.query) ? `"${a.query}"` : null;
    case "find_files":
      return str(a.pattern) ?? null;
    case "run_shell":
      return str(a.command) ?? null;
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
  return (
    <div className={"tool-row " + event.status}>
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
