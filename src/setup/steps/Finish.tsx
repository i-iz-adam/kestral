import type { StepProps } from "../types";

export default function Finish({ onComplete }: StepProps) {
  return (
    <div className="step-card">
      <h2>You're set up</h2>
      <p>
        That's everything for now. Future updates may add a short new step
        here occasionally — you'll only ever see what's new, not this whole
        flow again.
      </p>
      <button className="primary" onClick={onComplete}>
        Get started
      </button>
    </div>
  );
}
