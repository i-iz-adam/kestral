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
    if (m.role === "tool") continue; // consumed below, via the call that produced it
    if (m.role !== "user" && m.role !== "assistant") continue;

    if (m.content && m.content.trim()) {
      items.push({ kind: "message", key: nextKey("msg"), role: m.role, content: m.content });
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
