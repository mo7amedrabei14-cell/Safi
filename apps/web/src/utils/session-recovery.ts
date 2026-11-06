/**
 * `apps/web/src/utils/session-recovery.ts` — Auto-save and session recovery after interruption (N21 / Section 4.10).
 */

export interface SessionSnapshot {
  recipeId: string;
  timestamp: number;
  stepsCount: number;
  dataHash: string;
}

const SESSION_KEY = "safi_session_snapshot_v1";

// In-memory fallback used when Web Storage is unavailable (SSR, private mode, tests).
let memorySnapshot: string | null = null;

function webStorage(): Storage | null {
  return typeof localStorage !== "undefined" ? localStorage : null;
}

export function saveSessionSnapshot(snapshot: SessionSnapshot): void {
  const serialized = JSON.stringify(snapshot);
  const store = webStorage();
  if (!store) {
    memorySnapshot = serialized;
    return;
  }
  try {
    store.setItem(SESSION_KEY, serialized);
  } catch {
    memorySnapshot = serialized;
  }
}

export function restoreSessionSnapshot(): SessionSnapshot | null {
  const store = webStorage();
  let raw = memorySnapshot;
  if (store) {
    try {
      raw = store.getItem(SESSION_KEY) ?? memorySnapshot;
    } catch {
      // Keep the in-memory snapshot when Web Storage is blocked.
    }
  }
  if (!raw) {
    return null;
  }
  try {
    return JSON.parse(raw) as SessionSnapshot;
  } catch {
    return null;
  }
}
