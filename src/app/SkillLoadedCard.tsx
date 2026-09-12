import type { ToolCallEventPayload } from "../types";

/** Skills are auto-loaded server-side (see skills::find_relevant in
 * agent.rs) rather than requiring the model to remember to call
 * list_skills/read_skill itself — this is the notice that one just got
 * pulled into context, styled distinctly from a regular tool call since
 * nothing was actually "run", something was "read". */
export default function SkillLoadedCard({ event }: { event: ToolCallEventPayload }) {
  const args = (event.args ?? {}) as { skill_id?: string; skill_name?: string };
  const name = args.skill_name || args.skill_id || "skill";

  return (
    <div className="skill-card">
      <span className="skill-card-icon" aria-hidden="true">
        <svg viewBox="0 0 24 24" width="16" height="16" fill="none">
          <path
            d="M4 4.5C4 3.67 4.67 3 5.5 3H12v18H5.5c-.83 0-1.5-.67-1.5-1.5v-15Z"
            fill="currentColor"
            opacity="0.35"
          />
          <path
            d="M20 4.5c0-.83-.67-1.5-1.5-1.5H12v18h6.5c.83 0 1.5-.67 1.5-1.5v-15Z"
            fill="currentColor"
          />
        </svg>
      </span>
      <div className="skill-card-body">
        <span className="skill-card-label">Skill loaded</span>
        <span className="skill-card-name">{name}</span>
      </div>
    </div>
  );
}
