import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import type { StepProps } from "../types";
import type { SessionDefaults } from "../../types";

export default function Stop({ onComplete }: StepProps) {
  const [gracefulStop, setGracefulStop] = useState(true);
  const [currentDefaults, setCurrentDefaults] = useState<SessionDefaults | null>(null);

  useEffect(() => {
    invoke<SessionDefaults>("get_session_defaults")
      .then((d) => {
        if (d) {
          setCurrentDefaults(d);
          setGracefulStop(d.graceful_stop ?? true);
        }
      })
      .catch((e) => {
        console.error("Failed to load session defaults:", e);
      });
  }, []);

  const handleContinue = async () => {
    await invoke("save_session_defaults", {
      defaults: {
        ...(currentDefaults || { planning_enabled: true, subagents_enabled: true }),
        graceful_stop: gracefulStop,
      },
    });
    onComplete();
  };

  return (
    <div className="step-card">
      <h2>Stop Button</h2>
      <p>
        You can now stop a turn in progress. While the agent is working, the "Send" button will turn into a "Stop" button.
      </p>
      <div className="field-group">
        <label>
          <input
            type="checkbox"
            checked={gracefulStop}
            onChange={(e) => setGracefulStop(e.target.checked)}
          />{" "}
          Graceful stop (sub-agents return an overview when you stop)
        </label>
      </div>
      <button className="primary" onClick={handleContinue}>
        Continue
      </button>
    </div>
  );
}
