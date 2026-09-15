import { useEffect, useRef } from "react";
import type { ToolCallEventPayload, PlanItem } from "../types";
import PlanDrawer from "./PlanDrawer";
import GithubToolCard from "./GithubToolCard";
import ToolCallRow from "./ToolCallRow";
import DiffToolCard from "./DiffToolCard";
import SkillLoadedCard from "./SkillLoadedCard";
import MessageContent from "./MessageContent";

interface Props {
  event: ToolCallEventPayload;
  subagentCalls: ToolCallEventPayload[];
  workspace: string;
  onBack: () => void;
  onPromptFix: (text: string) => void;
  onApprove: (callId: string, approved: boolean) => void;
  plan?: PlanItem[];
}

export default function SubagentView({
  event,
  subagentCalls,
  workspace,
  onBack,
  onPromptFix,
  onApprove,
  plan,
}: Props) {
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [subagentCalls.length, event.status]);

  const task = (() => {
    const args = event.args as { task?: string } | undefined;
    return args?.task ?? "Sub-agent Task";
  })();

  const isRunning = event.status === "start" || event.status === "awaiting-approval";

  return (
    <div className="subagent-view-container" style={{ position: "relative", flex: 1, display: "flex", flexDirection: "column", height: "100%", overflow: "hidden" }}>
      {plan && plan.length > 0 && <PlanDrawer plan={plan} title="Sub-agent Task Plan" />}
      <div className="subagent-view-header">
        <button type="button" className="subagent-back-btn" onClick={onBack}>
          <span className="back-icon">←</span> Back to Main Chat
        </button>
        <div className="subagent-title-area">
          <span className="subagent-badge-icon font-mono">🤖</span>
          <h2 className="subagent-view-title" title={task}>
            {task}
          </h2>
        </div>
        <div className={`subagent-status-pill ${event.status}`}>
          <span className="status-dot" />
          <span className="status-label font-mono">
            {event.status === "done"
              ? "Completed"
              : event.status === "error"
              ? "Failed"
              : event.status === "awaiting-approval"
              ? "Awaiting Approval"
              : "Running"}
          </span>
        </div>
      </div>

      <div className="subagent-view-body">
        {/* Subagent initial prompt / task */}
        <div className="message user subagent-task-message">
          <span className="role-label font-mono">Sub-agent Task</span>
          <div className="bubble">
            <p>{task}</p>
          </div>
        </div>

        {/* Steps, skills, and thoughts */}
        {subagentCalls.length === 0 && isRunning && (
          <div className="subagent-loading-banner">
            <div className="subagent-spinner" />
            <span>Initializing sub-agent environment and executing task...</span>
          </div>
        )}

        {subagentCalls.map((c) => {
          if (c.name === "__skill_loaded__") {
            return <SkillLoadedCard key={c.call_id} event={c} />;
          }
          if (c.name === "__subagent_thought__") {
            return (
              <div key={c.call_id} className="message assistant subagent-thought">
                <span className="role-label font-mono">Sub-agent Reasoning</span>
                <div className="bubble">
                  <MessageContent role="assistant" content={c.result ?? ""} />
                </div>
              </div>
            );
          }
          if (c.name.startsWith("github_")) {
            return (
              <GithubToolCard
                key={c.call_id}
                event={c}
                workspace={workspace}
                onPromptFix={onPromptFix}
              />
            );
          }
          if (c.name === "edit_file" || c.name === "apply_patch") {
            return <DiffToolCard key={c.call_id} event={c} onApprove={onApprove} />;
          }
          return <ToolCallRow key={c.call_id} event={c} onApprove={onApprove} />;
        })}

        {/* Final output / summary */}
        {event.status === "done" && event.result && (
          <div className="message assistant subagent-final-output">
            <span className="role-label font-mono">Final Summary Output</span>
            <div className="bubble">
              <MessageContent role="assistant" content={event.result} />
            </div>
          </div>
        )}

        {event.status === "error" && event.result && (
          <div className="subagent-error-banner">
            <span className="error-icon">⚠️</span>
            <div className="error-text">
              <strong>Sub-agent execution failed:</strong>
              <p>{event.result}</p>
            </div>
          </div>
        )}

        <div ref={bottomRef} />
      </div>

      {/* Subagents missing the bottom part used to send a message */}
      <div className="subagent-view-footer">
        <div className="subagent-readonly-notice font-mono">
          <span className="lock-icon">🔒</span>
          <span>Sub-agents execute tasks autonomously. Direct messaging is disabled.</span>
        </div>
      </div>
    </div>
  );
}
