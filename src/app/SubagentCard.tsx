import { useEffect, useState } from "react";
import type { ToolCallEventPayload } from "../types";
import GithubToolCard from "./GithubToolCard";
import ToolCallRow from "./ToolCallRow";

interface Props {
  event: ToolCallEventPayload; // the delegate_to_subagent call itself
  calls: ToolCallEventPayload[]; // its nested tool calls, arrival order
  workspace: string;
  onPromptFix: (text: string) => void;
  onApprove: (callId: string, approved: boolean) => void;
}

export default function SubagentCard({
  event,
  calls,
  workspace,
  onPromptFix,
  onApprove,
}: Props) {
  const [expanded, setExpanded] = useState(true);
  const [autoCollapsed, setAutoCollapsed] = useState(false);

  // Watch it work while it's running, then get out of the way once it's
  // done — the summary line still shows, the step-by-step detail doesn't
  // have to stay open to read it. Only auto-collapses once, so it doesn't
  // fight a manual toggle.
  useEffect(() => {
    if (event.status === "done" && !autoCollapsed) {
      setExpanded(false);
      setAutoCollapsed(true);
    }
  }, [event.status, autoCollapsed]);

  const task = (() => {
    const args = event.args as { task?: string } | undefined;
    return args?.task ?? "subagent task";
  })();

  const stateClass =
    event.status === "done"
      ? "done"
      : event.status === "error"
      ? "error"
      : event.status === "awaiting-approval"
      ? "awaiting"
      : "running";

  return (
    <div className={"subagent-card " + stateClass + (expanded ? " expanded" : "")}>
      <div className="subagent-header" onClick={() => setExpanded((e) => !e)}>
        <span className="subagent-dot" />
        <span className="subagent-task">{task}</span>
        <span className="subagent-meta">
          {calls.length > 0
            ? `${calls.length} step${calls.length === 1 ? "" : "s"}`
            : ""}
        </span>
        <span className="subagent-chevron">▸</span>
      </div>

      {expanded && (
        <div className="subagent-body">
          {calls.length === 0 && (
            <p className="subagent-empty">Starting up...</p>
          )}
          {calls.map((c) =>
            c.name.startsWith("github_") ? (
              <GithubToolCard
                key={c.call_id}
                event={c}
                workspace={workspace}
                onPromptFix={onPromptFix}
              />
            ) : (
              <ToolCallRow key={c.call_id} event={c} onApprove={onApprove} />
            )
          )}
        </div>
      )}

      {event.status === "done" && event.result && (
        <div className="subagent-summary">{event.result}</div>
      )}
      {event.status === "error" && event.result && (
        <div className="subagent-summary fail">{event.result}</div>
      )}
    </div>
  );
}
