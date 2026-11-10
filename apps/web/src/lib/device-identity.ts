/**
 * Stable installation identity for one-active-device account sessions.
 *
 * The random UUID is stored in IndexedDB, separate from the session token.
 * If IndexedDB or the secure random UUID API is unavailable, sign-in fails
 * closed instead of silently creating a new identity on each login.
 */

export type SafiDevicePlatform = "windows" | "linux" | "macos" | "web";

export interface SafiDeviceIdentity {
  readonly installationId: string;
  readonly platform: SafiDevicePlatform;
}

export class SafiDeviceIdentityError extends Error {
  constructor() {
    super("A stable Safi installation identity could not be read or created.");
    this.name = "SafiDeviceIdentityError";
  }
}

const DATABASE_NAME = "safidata-installation";
const DATABASE_VERSION = 1;
const OBJECT_STORE = "identity";
const INSTALLATION_KEY = "installation_id";
const UUID_V4 =
  /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

function createInstallationUuid(): string {
  if (typeof globalThis.crypto?.randomUUID !== "function") {
    throw new SafiDeviceIdentityError();
  }

  const installationId = globalThis.crypto.randomUUID();
  if (!UUID_V4.test(installationId)) throw new SafiDeviceIdentityError();
  return installationId;
}

function openIdentityDatabase(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    if (typeof globalThis.indexedDB === "undefined") {
      reject(new SafiDeviceIdentityError());
      return;
    }

    let request: IDBOpenDBRequest;
    try {
      request = globalThis.indexedDB.open(DATABASE_NAME, DATABASE_VERSION);
    } catch {
      reject(new SafiDeviceIdentityError());
      return;
    }

    request.onupgradeneeded = () => {
      const database = request.result;
      if (!database.objectStoreNames.contains(OBJECT_STORE)) {
        database.createObjectStore(OBJECT_STORE);
      }
    };
    request.onblocked = () => reject(new SafiDeviceIdentityError());
    request.onerror = () => reject(new SafiDeviceIdentityError());
    request.onsuccess = () => {
      const database = request.result;
      database.onversionchange = () => database.close();
      resolve(database);
    };
  });
}

function readOrCreateInstallationUuid(database: IDBDatabase): Promise<string> {
  return new Promise((resolve, reject) => {
    let settled = false;
    let installationId: string | null = null;

    const fail = (): void => {
      if (settled) return;
      settled = true;
      reject(new SafiDeviceIdentityError());
    };

    let transaction: IDBTransaction;
    try {
      transaction = database.transaction(OBJECT_STORE, "readwrite");
    } catch {
      fail();
      return;
    }

    transaction.oncomplete = () => {
      if (settled) return;
      if (installationId === null) {
        fail();
        return;
      }
      settled = true;
      resolve(installationId);
    };
    transaction.onerror = fail;
    transaction.onabort = fail;

    let store: IDBObjectStore;
    let request: IDBRequest<unknown>;
    try {
      store = transaction.objectStore(OBJECT_STORE);
      request = store.get(INSTALLATION_KEY);
    } catch {
      transaction.abort();
      fail();
      return;
    }

    request.onsuccess = () => {
      const stored = request.result;
      if (typeof stored === "string" && UUID_V4.test(stored)) {
        installationId = stored;
        return;
      }

      try {
        installationId = createInstallationUuid();
        store.put(installationId, INSTALLATION_KEY);
      } catch {
        transaction.abort();
        fail();
      }
    };
    request.onerror = () => {
      transaction.abort();
      fail();
    };
  });
}

function detectPlatform(userAgent: string, isTauri: boolean): SafiDevicePlatform {
  if (!isTauri) return "web";

  const agent = userAgent.toLowerCase();
  if (agent.includes("windows")) return "windows";
  if (agent.includes("macintosh") || agent.includes("mac os")) return "macos";
  if (agent.includes("linux")) return "linux";
  throw new SafiDeviceIdentityError();
}

/** Return the stable IndexedDB installation id and an allow-listed platform. */
export async function getSafiDeviceIdentity(): Promise<SafiDeviceIdentity> {
  if (typeof window === "undefined" || typeof navigator === "undefined") {
    throw new SafiDeviceIdentityError();
  }

  const isTauri = "__TAURI_INTERNALS__" in window;
  const platform = detectPlatform(navigator.userAgent, isTauri);
  const database = await openIdentityDatabase();

  try {
    const installationId = await readOrCreateInstallationUuid(database);
    return { installationId, platform };
  } finally {
    database.close();
  }
}
