import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import type { StepProps } from "../types";
import type { SessionDefaults } from "../../types";

export default function Defaults({ onComplete }: StepProps) {
  const [planningEnabled, setPlanningEnabled] = useState(true);
  const [subagentsEnabled, setSubagentsEnabled] = useState(true);

  useEffect(() => {
    invoke<SessionDefaults>("get_session_defaults")
      .then((d) => {
        if (d) {
          setPlanningEnabled(d.planning_enabled ?? true);
          setSubagentsEnabled(d.subagents_enabled ?? true);
        }
      })
      .catch((e) => {
        console.error("Failed to load session defaults:", e);
      });
  }, []);

  const handleContinue = async () => {
    await invoke("save_session_defaults", {
      defaults: {
        planning_enabled: planningEnabled,
        subagents_enabled: subagentsEnabled,
      },
    });
    onComplete();
  };

  return (
    <div className="step-card">
      <h2>Session defaults</h2>
      <p>
        Configure the default settings applied to all newly created sessions.
      </p>
      <div className="field-group">
        <label>
          <input
            type="checkbox"
            checked={planningEnabled}
            onChange={(e) => setPlanningEnabled(e.target.checked)}
          />{" "}
          Planning mode by default (approve writes/commands)
        </label>
        <label>
          <input
            type="checkbox"
            checked={subagentsEnabled}
            onChange={(e) => setSubagentsEnabled(e.target.checked)}
          />{" "}
          Use sub-agents by default to keep context clean
        </label>
      </div>
      <button className="primary" onClick={handleContinue}>
        Continue
      </button>
    </div>
  );
}
