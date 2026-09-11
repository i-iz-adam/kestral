import type { StepProps } from "../types";

export default function Welcome({ onComplete }: StepProps) {
  return (
    <div className="step-card">
      <h2>Welcome</h2>
      <p>
        This short setup connects the app to an LLM backend and picks a
        workspace folder. It takes about a minute, and you won't see it
        again after this.
      </p>
      <button className="primary" onClick={onComplete}>
        Continue
      </button>
    </div>
  );
}
