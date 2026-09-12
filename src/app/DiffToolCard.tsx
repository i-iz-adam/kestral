import { useMemo, useState } from "react";
import type { ToolCallEventPayload } from "../types";
import Modal from "./Modal";
import { diffLines, parsePatchDiff, type DiffLine } from "./diffUtils";

interface DiffSection {
  label?: string;
  lines: DiffLine[];
}

interface DiffInfo {
  label: string;
  sections: DiffSection[];
}

function asRecord(args: unknown): Record<string, unknown> {
  return args && typeof args === "object" ? (args as Record<string, unknown>) : {};
}

function buildDiff(event: ToolCallEventPayload): DiffInfo | null {
  const args = asRecord(event.args);

  if (event.name === "edit_file") {
    const path = typeof args.path === "string" ? args.path : "file";
    const edits = Array.isArray(args.edits) ? args.edits : [];
    const sections: DiffSection[] = edits.map((raw, i) => {
      const e = asRecord(raw);
      const oldStr = typeof e.old_string === "string" ? e.old_string : "";
      const newStr = typeof e.new_string === "string" ? e.new_string : "";
      return {
        label: edits.length > 1 ? `Edit ${i + 1} of ${edits.length}` : undefined,
        lines: diffLines(oldStr, newStr),
      };
    });
    return { label: path, sections };
  }

  if (event.name === "apply_patch") {
    const patch = typeof args.patch === "string" ? args.patch : "";
    return { label: "patch", sections: [{ lines: parsePatchDiff(patch) }] };
  }

  return null;
}

function countByType(diff: DiffInfo, type: "add" | "del"): number {
  return diff.sections.reduce(
    (n, s) => n + s.lines.filter((l) => l.type === type).length,
    0
  );
}

export default function DiffToolCard({
  event,
  onApprove,
}: {
  event: ToolCallEventPayload;
  onApprove: (callId: string, approved: boolean) => void;
}) {
  const [open, setOpen] = useState(false);
  const diff = useMemo(() => buildDiff(event), [event]);

  if (!diff) {
    // Shouldn't happen (caller only routes edit_file/apply_patch here),
    // but fail gracefully into something usable rather than nothing.
    return null;
  }

  const added = countByType(diff, "add");
  const removed = countByType(diff, "del");

  return (
    <>
      <div className={"tool-row diff-row " + event.status}>
        <span className="tool-name">{event.name}</span>
        <span className="tool-arg">{diff.label}</span>
        {(added > 0 || removed > 0) && (
          <span className="diff-stat">
            {added > 0 && <span className="diff-stat-add">+{added}</span>}
            {removed > 0 && <span className="diff-stat-del">-{removed}</span>}
          </span>
        )}
        <span className="tool-status">{event.status}</span>
        {event.status === "awaiting-approval" && (
          <div className="tool-approve">
            <button onClick={() => onApprove(event.call_id, true)}>Approve</button>
            <button onClick={() => onApprove(event.call_id, false)}>Reject</button>
          </div>
        )}
        <button type="button" className="diff-view-btn" onClick={() => setOpen(true)}>
          View changes
        </button>
      </div>

      {open && (
        <Modal title={diff.label} onClose={() => setOpen(false)}>
          {diff.sections.map((section, i) => (
            <div className="diff-section" key={i}>
              {section.label && <div className="diff-section-label">{section.label}</div>}
              <pre className="diff-view">
                {section.lines.map((l, j) => (
                  <div key={j} className={"diff-line diff-" + l.type}>
                    <span className="diff-marker">
                      {l.type === "add" ? "+" : l.type === "del" ? "-" : ""}
                    </span>
                    <span className="diff-text">{l.text}</span>
                  </div>
                ))}
              </pre>
            </div>
          ))}
          {event.result && event.status !== "start" && (
            <div className="diff-result">{event.result}</div>
          )}
        </Modal>
      )}
    </>
  );
}
