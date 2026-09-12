import { useSyncExternalStore } from "react";
import { getRecord, subscribeSession, subscribeAny, isAnySessionSending, type SessionRecord } from "./agentStore";

export function useAgentSession(sessionId: string): SessionRecord {
  return useSyncExternalStore(
    (cb) => subscribeSession(sessionId, cb),
    () => getRecord(sessionId)
  );
}

/** For the sidebar: true whenever any session (not just the one currently
 * open) has a turn in flight, so switching away from a working session
 * doesn't make it look like nothing is happening anymore. */
export function useAnySessionSending(): boolean {
  return useSyncExternalStore(subscribeAny, isAnySessionSending);
}
