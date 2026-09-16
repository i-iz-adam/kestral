import { useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";

export interface UpdateCheckResult {
  has_update: boolean;
  current_version: string;
  latest_version: string;
  release_name: string;
  release_notes: string;
  published_at: string;
  download_url: string;
}

export interface UpdaterState {
  checking: boolean;
  result: UpdateCheckResult | null;
  error: string | null;
  lastCheckedAt: number | null;
}

let currentState: UpdaterState = {
  checking: false,
  result: null,
  error: null,
  lastCheckedAt: null,
};

const listeners = new Set<() => void>();

function notify() {
  for (const listener of listeners) {
    listener();
  }
}

export function getUpdaterSnapshot(): UpdaterState {
  return currentState;
}

export function subscribeUpdater(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function useUpdaterStore(): UpdaterState {
  return useSyncExternalStore(subscribeUpdater, getUpdaterSnapshot);
}

let checkPromise: Promise<UpdateCheckResult | null> | null = null;

export async function checkAppUpdates(force = false): Promise<UpdateCheckResult | null> {
  if (currentState.checking && checkPromise) {
    return checkPromise;
  }
  if (!force && currentState.lastCheckedAt && Date.now() - currentState.lastCheckedAt < 60000 && currentState.result) {
    return currentState.result;
  }

  currentState = {
    ...currentState,
    checking: true,
    error: null,
  };
  notify();

  checkPromise = (async () => {
    try {
      const res = await invoke<UpdateCheckResult>("check_app_update");
      currentState = {
        checking: false,
        result: res,
        error: null,
        lastCheckedAt: Date.now(),
      };
      notify();
      return res;
    } catch (err: any) {
      const errorMsg = typeof err === "string" ? err : err?.message || "Failed to check for updates";
      currentState = {
        ...currentState,
        checking: false,
        error: errorMsg,
        lastCheckedAt: Date.now(),
      };
      notify();
      return null;
    } finally {
      checkPromise = null;
    }
  })();

  return checkPromise;
}
