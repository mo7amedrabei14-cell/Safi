/**
 * `apps/web/src/lib/desktop-pkce.ts`
 *
 * OAuth-style PKCE (RFC 7636 S256) client for SafiData Desktop system-browser authentication:
 * - Generates cryptographically random `code_verifier` (43+ chars) and `state` (32+ chars)
 *   using WebCrypto (`crypto.getRandomValues` + `SHA-256` base64url).
 * - Starts the PKCE authorization request via `POST /v1/auth/desktop/start`.
 * - Launches the user's system browser via the tightly allowlisted native Tauri command
 *   `desktop_open_external_auth_url`.
 * - Verifies `state` strictly upon receiving `safi://auth?code=...&state=...` and exchanges
 *   `(code, code_verifier, state, installation_id)` via `POST /v1/auth/desktop/exchange`.
 * - Never exposes session tokens, passwords, or secrets in URLs or persistent disk storage.
 */
import { getSafiDeviceIdentity } from "./device-identity";
import { ownerApi, type DesktopPkceExchangeResponse } from "./owner-api";
import { openDesktopExternalAuthUrl } from "../desktop/studio-bridge";

export interface PendingDesktopPkceFlow {
  readonly requestId: string;
  readonly codeVerifier: string;
  readonly state: string;
  readonly installationId: string;
  readonly authUrl: string;
  readonly flow: "signin" | "signup";
  readonly expiresAt: number;
}

let activePkceFlow: PendingDesktopPkceFlow | null = null;

function toBase64Url(bytes: Uint8Array): string {
  let binary = "";
  for (let i = 0; i < bytes.length; i += 1) {
    binary += String.fromCharCode(bytes[i] ?? 0);
  }
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

export async function generatePkceChallenge(): Promise<{
  readonly codeVerifier: string;
  readonly codeChallenge: string;
  readonly state: string;
}> {
  const verifierBytes = new Uint8Array(32);
  const stateBytes = new Uint8Array(24);
  crypto.getRandomValues(verifierBytes);
  crypto.getRandomValues(stateBytes);

  const codeVerifier = toBase64Url(verifierBytes);
  const state = toBase64Url(stateBytes);
  const encoded = new TextEncoder().encode(codeVerifier);
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", encoded));
  const codeChallenge = toBase64Url(digest);

  return { codeVerifier, codeChallenge, state };
}

export function getPendingDesktopPkceFlow(): PendingDesktopPkceFlow | null {
  return activePkceFlow;
}

export function clearPendingDesktopPkceFlow(): void {
  activePkceFlow = null;
}

/**
 * Initiates a Desktop browser PKCE flow (`signin` or `signup`), stores the ephemeral
 * `code_verifier` + `state` in memory only, and launches the OS default browser via the
 * allowlisted Tauri IPC command `desktop_open_external_auth_url`.
 */
export async function startDesktopBrowserPkce(
  flow: "signin" | "signup"
): Promise<PendingDesktopPkceFlow> {
  const device = await getSafiDeviceIdentity();
  const { codeVerifier, codeChallenge, state } = await generatePkceChallenge();

  const started = await ownerApi.startDesktopPkceAuth({
    code_challenge: codeChallenge,
    code_challenge_method: "S256",
    state,
    installation_id: device.installationId,
    platform: device.platform === "web" ? "windows" : device.platform,
    device_name: `SafiData Desktop (${device.platform})`,
    flow
  });

  let parsedUrl: URL;
  try {
    parsedUrl = new URL(started.auth_url);
  } catch {
    throw new Error("SF-AUTH-023");
  }
  if (
    parsedUrl.pathname !== "/auth/desktop" ||
    parsedUrl.searchParams.get("state") !== state ||
    parsedUrl.searchParams.get("request_id") !== started.request_id
  ) {
    throw new Error("SF-AUTH-023");
  }

  const pending: PendingDesktopPkceFlow = {
    requestId: started.request_id,
    codeVerifier,
    state,
    installationId: device.installationId,
    authUrl: started.auth_url,
    flow,
    expiresAt: started.expires_at
  };
  activePkceFlow = pending;

  try {
    await openDesktopExternalAuthUrl(started.auth_url);
  } catch (err) {
    activePkceFlow = null;
    throw err;
  }

  return pending;
}

/**
 * Completes a Desktop browser PKCE callback (`safi://auth?code=...&state=...`).
 * Fails closed (`SF-AUTH-023`) on missing flow, state mismatch, or rejected exchange.
 */
export async function completeDesktopBrowserPkce(
  code: string,
  returnedState: string
): Promise<DesktopPkceExchangeResponse> {
  const pending = activePkceFlow;
  if (pending === null) {
    throw new Error("SF-AUTH-023");
  }
  const cleanCode = code.trim();
  const cleanState = returnedState.trim();
  if (cleanCode.length < 12 || cleanState !== pending.state) {
    activePkceFlow = null;
    throw new Error("SF-AUTH-023");
  }

  // Consume in-memory verifier single-use before network call so replays always fail.
  activePkceFlow = null;

  return ownerApi.exchangeDesktopPkceAuth({
    code: cleanCode,
    code_verifier: pending.codeVerifier,
    state: cleanState,
    installation_id: pending.installationId
  });
}
