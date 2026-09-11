import type { ToolCallEventPayload } from "../types";

export default function ToolCallRow({
  event,
  onApprove,
}: {
  event: ToolCallEventPayload;
  onApprove: (callId: string, approved: boolean) => void;
}) {
  return (
    <div className={"tool-row " + event.status}>
      <span className="tool-name">{event.name}</span>
      <span className="tool-status">{event.status}</span>
      {event.status === "awaiting-approval" && (
        <div className="tool-approve">
          <button onClick={() => onApprove(event.call_id, true)}>
            Approve
          </button>
          <button onClick={() => onApprove(event.call_id, false)}>
            Reject
          </button>
        </div>
      )}
      {event.result && (
        <pre className="tool-result">{event.result.slice(0, 400)}</pre>
      )}
    </div>
  );
}
