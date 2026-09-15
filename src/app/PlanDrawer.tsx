import { useState, useEffect } from "react";
import type { PlanItem } from "../types";

interface Props {
  plan: PlanItem[];
  title?: string;
}

export default function PlanDrawer({ plan, title = "Grimoire of Tasks" }: Props) {
  const [isOpen, setIsOpen] = useState(false);
  const [hasPulse, setHasPulse] = useState(false);
  const [prevPlanLength, setPrevPlanLength] = useState(plan.length);
  const [prevDoneCount, setPrevDoneCount] = useState(
    plan.filter((i) => i.status === "completed").length
  );

  const completedCount = plan.filter((i) => i.status === "completed").length;
  const inProgressCount = plan.filter((i) => i.status === "in_progress").length;
  const totalCount = plan.length;
  const progressPercent = totalCount > 0 ? Math.round((completedCount / totalCount) * 100) : 0;

  // Pulse effect when plan items change while drawer is closed
  useEffect(() => {
    if (!isOpen && totalCount > 0) {
      if (totalCount !== prevPlanLength || completedCount !== prevDoneCount) {
        setHasPulse(true);
      }
    }
    setPrevPlanLength(totalCount);
    setPrevDoneCount(completedCount);
  }, [plan, isOpen, totalCount, completedCount, prevPlanLength, prevDoneCount]);

  if (totalCount === 0) {
    return null;
  }

  const toggleDrawer = () => {
    setIsOpen((prev) => !prev);
    if (!isOpen) {
      setHasPulse(false);
    }
  };

  return (
    <div className={`grimoire-drawer-container ${isOpen ? "open" : "closed"}`}>
      {/* Animated Pull-Out Bookmark / Tab */}
      <button
        type="button"
        className={`grimoire-toggle-tab ${isOpen ? "open" : ""} ${hasPulse ? "pulse-gold" : ""}`}
        onClick={toggleDrawer}
        title={isOpen ? "Close Quest Grimoire" : "Open Quest Grimoire"}
        aria-label="Toggle Grimoire Plan List"
      >
        <div className="grimoire-tab-spine" />
        <span className="grimoire-tab-icon font-mono">{isOpen ? "📖" : "📜"}</span>
        <span className="grimoire-tab-badge font-mono">
          {completedCount}/{totalCount}
        </span>
        {inProgressCount > 0 && <span className="grimoire-active-spark" title="Task in progress" />}
      </button>

      {/* Slide-Out Parchment / Grimoire Drawer Panel */}
      <div className="grimoire-drawer-panel">
        <div className="grimoire-header">
          <div className="grimoire-title-row">
            <div className="grimoire-title-block">
              <span className="grimoire-sigil font-mono">📜</span>
              <div>
                <h3 className="grimoire-title">{title}</h3>
                <span className="grimoire-subtitle font-mono">
                  {completedCount} of {totalCount} Steps Completed
                </span>
              </div>
            </div>
            <button
              type="button"
              className="grimoire-close-btn"
              onClick={() => setIsOpen(false)}
              title="Close Grimoire"
            >
              &times;
            </button>
          </div>

          {/* Illuminated Progress Bar */}
          <div className="grimoire-progress-container">
            <div className="grimoire-progress-track">
              <div
                className="grimoire-progress-fill"
                style={{ width: `${progressPercent}%` }}
              />
            </div>
            <span className="grimoire-progress-percent font-mono">{progressPercent}%</span>
          </div>
        </div>

        {/* Step Items List */}
        <div className="grimoire-body">
          <div className="grimoire-step-list">
            {plan.map((item, idx) => {
              const isDone = item.status === "completed";
              const isInProgress = item.status === "in_progress";
              const statusClass = isDone ? "completed" : isInProgress ? "in-progress" : "pending";

              return (
                <div
                  key={idx}
                  className={`grimoire-step-card ${statusClass}`}
                >
                  <div className="grimoire-step-status-icon font-mono">
                    {isDone ? (
                      <span className="step-icon-done" title="Completed">
                        ✨
                      </span>
                    ) : isInProgress ? (
                      <span className="step-icon-progress" title="In Progress">
                        ⚡
                      </span>
                    ) : (
                      <span className="step-icon-pending" title="Pending">
                        ○
                      </span>
                    )}
                  </div>
                  <div className="grimoire-step-content">
                    <span className="grimoire-step-number font-mono">Step {idx + 1}</span>
                    <p className="grimoire-step-text">{item.content}</p>
                  </div>
                  {isInProgress && <div className="grimoire-pulse-border" />}
                </div>
              );
            })}
          </div>
        </div>

        <div className="grimoire-footer font-mono">
          <span className="grimoire-footer-icon">🔮</span>
          <span>Durable Task Checklist • Survives Context Compaction</span>
        </div>
      </div>
    </div>
  );
}
