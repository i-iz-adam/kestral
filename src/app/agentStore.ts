import { invoke } from "@tauri-apps/api/tauri";
import { listen } from "@tauri-apps/api/event";
import type {
  Session,
  ToolCallEventPayload,
  MessageEventPayload,
  MessageStartEventPayload,
  MessageDeltaEventPayload,
  MessageCancelEventPayload,
  TurnEndEventPayload,
  TimelineItem,
} from "../types";

// A turn on the backend runs on Tauri's own async runtime the moment
// send_message is invoked — it was never actually tied to any webview
// page being open. What broke that illusion before was purely on the
// frontend: SessionView owned its live event state locally, so navigating
// away unmounted it, tore down its event listeners, and lost whatever
// arrived in between; coming back reset everything to empty and the turn
// looked "stopped" even though it kept running. This module fixes that by
// moving live turn state out of any component and into a plain singleton
// that starts listening once, for every session, and never unmounts.

export interface SessionRecord {
  session: Session | null;
  /** Only the current, in-progress turn's events — cleared the moment
   * agent://turn-end confirms everything is safely in `session.messages`
   * on disk, at which point buildHistoryTimeline(session.messages) is the
   * sole source of truth again. */
  timeline: TimelineItem[];
  liveCalls: ToolCallEventPayload[];
  /** Sub-agent calls grouped by parent_call_id, persisted in memory across turn ends */
  subagentCalls: Record<string, ToolCallEventPayload[]>;
  sending: boolean;
  /** True once a turn finishes for this session while it wasn't the one
   * on screen — cleared by setActiveSession the moment the person opens
   * it. Drives the sidebar's "something happened while you were away"
   * glow, distinct from `sending` (which is about right now, not what
   * was missed). */
  unseenActivity: boolean;
}

function emptyRecord(): SessionRecord {
  return { session: null, timeline: [], liveCalls: [], subagentCalls: {}, sending: false, unseenActivity: false };
}

// Stable snapshot for unknown sessions. useSyncExternalStore requires
// getSnapshot to return the SAME reference while data is unchanged —
// returning a fresh emptyRecord() per call makes React re-render forever
// (blank/frozen app on startup, one subscriber per sidebar row).
const EMPTY_RECORD: SessionRecord = emptyRecord();

const records = new Map<string, SessionRecord>();
const subscribers = new Map<string, Set<() => void>>();

let keySeq = 0;
const nextKey = (prefix: string) => `live-${prefix}-${++keySeq}`;

/** The currently selected workspace filter — drives which sessions appear in
 * the sidebar. When null, shows all sessions; when set, shows only sessions
 * from that workspace. Separate from the workspace selected for new sessions. */
let currentWorkspaceFilter: string | null = null;
const workspaceSubscribers = new Set<() => void>();

export function getCurrentWorkspaceFilter(): string | null {
  return currentWorkspaceFilter;
}

export function setCurrentWorkspaceFilter(path: string | null) {
  currentWorkspaceFilter = path;
  workspaceSubscribers.forEach((cb) => cb());
  notifyAny();
}

export function subscribeWorkspaceFilter(cb: () => void): () => void {
  workspaceSubscribers.add(cb);
  return () => workspaceSubscribers.delete(cb);
}

function notify(sessionId: string) {
  subscribers.get(sessionId)?.forEach((cb) => cb());
}

function patch(sessionId: string, changes: Partial<SessionRecord>) {
  const cur = records.get(sessionId) ?? emptyRecord();
  records.set(sessionId, { ...cur, ...changes });
  notify(sessionId);
}

export function getRecord(sessionId: string): SessionRecord {
  return records.get(sessionId) ?? EMPTY_RECORD;
}

export function subscribeSession(sessionId: string, cb: () => void): () => void {
  if (!subscribers.has(sessionId)) subscribers.set(sessionId, new Set());
  subscribers.get(sessionId)!.add(cb);
  return () => {
    subscribers.get(sessionId)?.delete(cb);
  };
}

/** True if any session currently has a turn in flight — drives the
 * sidebar's "something is working" indicator without needing to know
 * which session. */
export function isAnySessionSending(): boolean {
  for (const rec of records.values()) {
    if (rec.sending) return true;
  }
  return false;
}

/** Returns sessions from OTHER workspaces that are currently sending.
 * Used to show "busy" sessions at the top of the sidebar even when
 * filtered to a different workspace. */
export function getSendingSessionsOutsideWorkspace(workspace: string | null): SessionRecord[] {
  const results: SessionRecord[] = [];
  for (const [, rec] of records) {
    if (!rec.sending) continue;
    const sessionWorkspace = rec.session?.workspace ?? null;
    if (sessionWorkspace !== workspace) {
      results.push(rec);
    }
  }
  return results;
}

export function subscribeAny(cb: () => void): () => void {
  const set = subscribers.get("*") ?? new Set();
  subscribers.set("*", set);
  set.add(cb);
  return () => set.delete(cb);
}

function notifyAny() {
  subscribers.get("*")?.forEach((cb) => cb());
}

/** Which session (if any) is currently the one on screen — set by
 * AppShell on every navigation. Used only to decide whether a completed
 * turn counts as "unseen" for the sidebar glow; it's deliberately not
 * part of any component's render state. */
let activeSessionId: string | null = null;

export function setActiveSession(sessionId: string | null) {
  activeSessionId = sessionId;
  if (sessionId && getRecord(sessionId).unseenActivity) {
    patch(sessionId, { unseenActivity: false });
  }
}

/** Fetches (or refetches) a session's persisted record from disk. Called
 * on every SessionView mount, same as before — the difference now is that
 * the live buffer alongside it survives independently of that mount. */
export async function loadSession(sessionId: string): Promise<void> {
  const session = await invoke<Session>("get_session", { id: sessionId });
  if (session) {
    patch(sessionId, { session });
  }
}

/** Optimistic local edits (repo link, planning/sub-agent toggles) so the
 * UI updates immediately without waiting on a full reload. */
export function mutateSessionLocally(sessionId: string, fn: (s: Session) => Session) {
  const cur = records.get(sessionId);
  if (cur?.session) patch(sessionId, { session: fn(cur.session) });
}

export function markSendingStart(sessionId: string) {
  patch(sessionId, { sending: true });
  notifyAny();
}

/** Clears a stuck `sending` state when the `send_message` invoke itself
 * rejects (IPC/serialization failure, backend panic before any
 * `agent://turn-end` fires). Without this the composer blocks every
 * follow-up send forever and the failure is invisible — the previous
 * `.catch(() => {})` swallowed it. */
export function markSendingFailed(sessionId: string, error: unknown) {
  const rec = getRecord(sessionId);
  const detail = error instanceof Error ? error.message : String(error);
  patch(sessionId, {
    sending: false,
    timeline: [
      ...rec.timeline,
      { kind: "message", key: nextKey("err"), role: "system", content: `Error: ${detail}`, streaming: false },
    ],
  });
  notifyAny();
}

export function pushSystemNote(sessionId: string, text: string) {
  const rec = getRecord(sessionId);
  patch(sessionId, {
    timeline: [
      ...rec.timeline,
      { kind: "message", key: nextKey("sys"), role: "system", content: text, streaming: false },
    ],
  });
}

let started = false;

/** Registers the global Tauri event listeners exactly once for the life
 * of the app (idempotent — safe to call from every SessionView mount).
 * This is the piece that used to live inside SessionView's own effect and
 * get torn down with it. */
export function ensureAgentEventsStarted() {
  if (started) return;
  started = true;

  listen<ToolCallEventPayload>("agent://tool-call", (evt) => {
    const { session_id } = evt.payload;
    const rec = getRecord(session_id);
    const idx = rec.liveCalls.findIndex((c) => c.call_id === evt.payload.call_id);
    const liveCalls =
      idx === -1
        ? [...rec.liveCalls, evt.payload]
        : rec.liveCalls.map((c, i) => (i === idx ? evt.payload : c));

    let timeline = rec.timeline;
    if (
      !evt.payload.parent_call_id &&
      !timeline.some((t) => t.kind === "tool" && t.callId === evt.payload.call_id)
    ) {
      timeline = [...timeline, { kind: "tool", key: nextKey("tool"), callId: evt.payload.call_id }];
    }
    const parentId = evt.payload.parent_call_id;
    let subagentCalls = rec.subagentCalls ?? {};
    if (parentId) {
      const existing = subagentCalls[parentId] ?? [];
      const cIdx = existing.findIndex((c) => c.call_id === evt.payload.call_id);
      const updatedList =
        cIdx === -1
          ? [...existing, evt.payload]
          : existing.map((c, i) => (i === cIdx ? evt.payload : c));
      subagentCalls = { ...subagentCalls, [parentId]: updatedList };
    }
    patch(session_id, { liveCalls, timeline, subagentCalls });
  });

  listen<MessageStartEventPayload>("agent://message-start", (evt) => {
    const { session_id } = evt.payload;
    const rec = getRecord(session_id);
    patch(session_id, {
      timeline: [
        ...rec.timeline,
        {
          kind: "message",
          key: nextKey("msg"),
          requestId: evt.payload.request_id,
          role: evt.payload.role,
          content: "",
          streaming: true,
        },
      ],
    });
  });

  listen<MessageDeltaEventPayload>("agent://message-delta", (evt) => {
    const { session_id } = evt.payload;
    const rec = getRecord(session_id);
    const idx = rec.timeline.findIndex(
      (t) => t.kind === "message" && t.requestId === evt.payload.request_id
    );
    if (idx === -1) return;
    const item = rec.timeline[idx] as Extract<TimelineItem, { kind: "message" }>;
    const timeline = [...rec.timeline];
    timeline[idx] = { ...item, content: item.content + evt.payload.delta };
    patch(session_id, { timeline });
  });

  listen<MessageCancelEventPayload>("agent://message-cancel", (evt) => {
    const { session_id } = evt.payload;
    const rec = getRecord(session_id);
    patch(session_id, {
      timeline: rec.timeline.filter(
        (t) => !(t.kind === "message" && t.requestId === evt.payload.request_id)
      ),
    });
  });

  listen<MessageEventPayload>("agent://message", (evt) => {
    const { session_id } = evt.payload;
    const rec = getRecord(session_id);
    if (evt.payload.request_id) {
      const idx = rec.timeline.findIndex(
        (t) => t.kind === "message" && t.requestId === evt.payload.request_id
      );
      if (idx !== -1) {
        const timeline = [...rec.timeline];
        timeline[idx] = {
          kind: "message",
          key: timeline[idx].key,
          requestId: evt.payload.request_id,
          role: evt.payload.role,
          content: evt.payload.content,
          images: evt.payload.images ?? undefined,
          streaming: false,
        };
        patch(session_id, { timeline });
        return;
      }
    }
    patch(session_id, {
      timeline: [
        ...rec.timeline,
        {
          kind: "message",
          key: nextKey("msg"),
          role: evt.payload.role,
          content: evt.payload.content,
          images: evt.payload.images ?? undefined,
          streaming: false,
        },
      ],
    });
  });

  listen<{ session_id: string; title: string }>("agent://session-title-updated", (evt) => {
    const { session_id, title } = evt.payload;
    mutateSessionLocally(session_id, (s) => ({ ...s, title }));
    notifyAny();
  });

  listen<TurnEndEventPayload>("agent://turn-end", async (evt) => {
    const { session_id, error, reason } = evt.payload;
    // Refresh the persisted record first, then clear the live buffer in
    // the same patch — so a subscribed view swaps from "live" to
    // "history" atomically and never flashes an empty gap in between.
    let session: Session | null = null;
    for (let attempt = 0; attempt < 3; attempt++) {
      try {
        session = await invoke<Session>("get_session", { id: session_id });
        if (session) break;
      } catch {
        // Session may have been deleted while the turn was running — keep
        // whatever record we already had rather than wiping it to null.
      }
      await new Promise((r) => setTimeout(r, 50));
    }
    const rec = getRecord(session_id);
    const targetSession = session ?? rec.session;
    const timeline: TimelineItem[] = session
      ? (error
          ? [{ kind: "message", key: nextKey("err"), role: "system", content: `Error: ${error}`, streaming: false }]
          : !error && reason === "stopped"
          ? [{ kind: "message", key: nextKey("stop"), role: "system", content: "Turn stopped — progress up to this point is saved. Send a follow-up to continue.", streaming: false }]
          : [])
      : rec.timeline;

    patch(session_id, {
      session: targetSession,
      timeline,
      liveCalls: session ? [] : rec.liveCalls,
      subagentCalls: rec.subagentCalls ?? {},
      sending: false,
      unseenActivity: session_id !== activeSessionId,
    });
    notifyAny();
  });
}
