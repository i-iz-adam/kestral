import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { stepComponents } from "./stepRegistry";
import type { SetupStepDef } from "./types";

export default function Wizard({ onFinished }: { onFinished: () => void }) {
  const [steps, setSteps] = useState<SetupStepDef[] | null>(null);
  const [index, setIndex] = useState(0);

  useEffect(() => {
    invoke<SetupStepDef[]>("get_pending_setup_steps").then(setSteps);
  }, []);

  if (steps === null) {
    return <div className="wizard-loading">Loading setup...</div>;
  }

  if (steps.length === 0) {
    onFinished();
    return null;
  }

  const current = steps[index];
  const StepComponent = stepComponents[current.id];

  const handleComplete = async () => {
    await invoke("complete_setup_step", { stepId: current.id });
    if (index + 1 < steps.length) {
      setIndex(index + 1);
    } else {
      onFinished();
    }
  };

  // A step id the registry knows about but this frontend build doesn't
  // (can happen briefly during an upgrade) — skip rather than crash.
  if (!StepComponent) {
    handleComplete();
    return null;
  }

  return (
    <div className="wizard">
      <div className="wizard-progress">
        {index + 1} / {steps.length}
      </div>
      <StepComponent onComplete={handleComplete} />
    </div>
  );
}
