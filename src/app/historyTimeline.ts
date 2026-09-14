import type { ChatMessage, HistoryItem, ToolCallEventPayload } from "../types";

let seq = 0;
const nextKey = (prefix: string) => `hist-${prefix}-${++seq}`;

function safeParseArgs(argumentsJson: string | undefined): unknown {
  if (!argumentsJson) return {};
  try {
    return JSON.parse(argumentsJson);
  } catch {
    return {};
  }
}

/** Rebuilds the same message-bubble/tool-call-card timeline the live view
 * shows, but from a session's saved ChatMessage[] instead of live Tauri
 * events — this is what makes tool call cards survive a session switch or
 * app restart instead of only existing for the duration of one turn.
 *
 * Sub-agent delegations replay as a SubagentCard with no nested steps
 * (only the top-level call and its final summary are persisted; a
 * sub-agent's own intermediate tool calls never are, by design — see
 * subagent.rs), which SubagentCard already renders sensibly on its own. */
export function buildHistoryTimeline(messages: ChatMessage[]): HistoryItem[] {
  const items: HistoryItem[] = [];

  for (const m of messages) {
    // Tool results are consumed via the matching tool_call on the assistant
    // message that triggered them — never rendered as standalone bubbles.
    if (m.role === "tool") continue;

    // Skill-loaded cards: convert to a tool item so SessionView.renderToolCard
    // can show them as SkillLoadedCard in the persisted history (they survive
    // session switches and app restarts this way). The serialization format
    // must match what agent.rs writes when emitting the event.
    if (m.role === "skill-loaded") {
      const parsed = JSON.parse(m.content ?? "{}");
      items.push({
        kind: "tool" as const,
        key: nextKey("tool"),
        call: {
          session_id: "",
          call_id: parsed.call_id ?? "",
          name: parsed.name ?? "__skill_loaded__",
          status: "done" as const,
          args: parsed.args ?? {},
          result: parsed.result ?? "",
        },
      });
      continue;
    }

    if (m.role !== "user" && m.role !== "assistant") continue;

    if ((m.content && m.content.trim()) || (m.images && m.images.length > 0)) {
      items.push({
        kind: "message",
        key: nextKey("msg"),
        role: m.role,
        content: m.content ?? "",
        images: m.images ?? undefined,
      });
    }

    if (m.role === "assistant" && m.tool_calls) {
      for (const call of m.tool_calls) {
        const resultMsg = messages.find(
          (mm) => mm.role === "tool" && mm.tool_call_id === call.id
        );
        const result = resultMsg?.content ?? undefined;
        const status: ToolCallEventPayload["status"] =
          result === undefined
            ? "error"
            : result === "Rejected by user."
            ? "error"
            : "done";

        items.push({
          kind: "tool",
          key: nextKey("tool"),
          call: {
            session_id: "",
            call_id: call.id,
            name: call.function.name,
            status,
            args: safeParseArgs(call.function.arguments),
            result: result ?? "No result recorded.",
          },
        });
      }
    }
  }

  return items;
}
