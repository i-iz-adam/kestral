import type { ToolCallEventPayload } from "../types";

interface Props {
  event: ToolCallEventPayload;
  calls: ToolCallEventPayload[];
  onOpen: () => void;
}

export default function SubagentCard({ event, calls, onOpen }: Props) {
  const task = (() => {
    const args = event.args as { task?: string } | undefined;
    return args?.task ?? "subagent task";
  })();

  const hasPendingQuestion = calls.some(
    (c) => c.name === "ask_question" && c.status === "awaiting-approval"
  );

  const stateClass =
    event.status === "done"
      ? "done"
      : event.status === "error"
      ? "error"
      : hasPendingQuestion || event.status === "awaiting-approval"
      ? "awaiting"
      : "running";

  const skillCount = calls.filter((c) => c.name === "__skill_loaded__").length;
  const stepCount = calls.filter(
    (c) => c.name !== "__skill_loaded__" && c.name !== "__subagent_thought__"
  ).length;

  return (
    <div
      id={`subagent-card-${event.call_id}`}
      className={`subagent-card-animated ${stateClass}`}
      onClick={onOpen}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onOpen();
        }
      }}
    >
      <div className="subagent-card-glow-bg" />
      <div className="subagent-card-border-glow" />

      <div className="subagent-card-header">
        <div className="subagent-card-icon-wrapper">
          <span className="subagent-animated-orb" />
          <span className="subagent-icon font-mono">🤖</span>
        </div>

        <div className="subagent-card-title-block">
          <div className="subagent-card-label font-mono">SUB-AGENT DELEGATION</div>
          <div className="subagent-card-task">{task}</div>
        </div>

        <div className={`subagent-card-status-badge ${stateClass}`}>
          <span className="status-badge-dot" />
          <span className="status-badge-text font-mono">
            {event.status === "done"
              ? "Completed"
              : event.status === "error"
              ? "Failed"
              : hasPendingQuestion
              ? "Question"
              : event.status === "awaiting-approval"
              ? "Awaiting"
              : "Active"}
          </span>
        </div>
      </div>

      <div className="subagent-card-metrics font-mono">
        {stepCount > 0 && (
          <span className="metric-tag">
            ⚡ {stepCount} step{stepCount === 1 ? "" : "s"}
          </span>
        )}
        {skillCount > 0 && (
          <span className="metric-tag">
            🧠 {skillCount} skill{skillCount === 1 ? "" : "s"} loaded
          </span>
        )}
        {stepCount === 0 && skillCount === 0 && event.status !== "done" && (
          <span className="metric-tag pulsing">Initializing...</span>
        )}
      </div>

      {event.status === "done" && event.result && (
        <div className="subagent-card-summary-preview">
          <span className="summary-quote">"{event.result}"</span>
        </div>
      )}

      {event.status === "error" && event.result && (
        <div className="subagent-card-summary-preview error">
          <span>{event.result}</span>
        </div>
      )}

      <div className="subagent-card-action-bar">
        <span className="action-text font-mono">Click to view sub-agent chat</span>
        <span className="action-arrow">→</span>
      </div>
    </div>
  );
}
