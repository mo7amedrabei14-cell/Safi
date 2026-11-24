/**
 * `apps/web/src/lib/desktop-billing-handoff.ts`
 *
 * Secure Desktop-to-Website Billing Handoff Client:
 * - Desktop NEVER collects or processes card payments inside the Desktop UI.
 * - Authenticated Desktop calls `POST /v1/billing/handoff/start` with the chosen product,
 *   seat count (for Organization), billing country, and a cryptographically random `state`.
 * - Validates that the returned URL targets `/billing?handoff=sfbh_...&state=...` with ZERO
 *   access tokens, refresh tokens, or reusable session credentials in the URL.
 * - Launches the system's default external browser via the allowlisted Tauri IPC command
 *   `desktop_open_external_billing_url`.
 */
import { getDefaultAuthApiBaseUrl } from "./auth-api";
import type { CheckoutProduct } from "./billing-checkout";
import { openDesktopExternalBillingUrl } from "../desktop/studio-bridge";

export interface DesktopBillingHandoffRequest {
  readonly product: CheckoutProduct;
  readonly seats?: number;
  readonly billing_country_iso2: string;
  readonly coupon_code?: string;
}

export interface DesktopBillingHandoffResponse {
  readonly handoff_id: string;
  readonly billing_url: string;
  readonly product: CheckoutProduct;
  readonly seats: number;
  readonly billing_country_iso2: string;
  readonly provider: "paddle" | "paymob" | "manual";
  readonly currency: "USD" | "EGP";
  readonly original_amount_minor: number;
  readonly billing_period: "monthly" | "annual" | "one_time";
  readonly expires_at: number;
}

function generateHandoffState(): string {
  const bytes = new Uint8Array(24);
  crypto.getRandomValues(bytes);
  let binary = "";
  for (let i = 0; i < bytes.length; i += 1) {
    binary += String.fromCharCode(bytes[i] ?? 0);
  }
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

/**
 * Initiates a short-lived single-use Desktop-to-Website billing handoff and opens the official
 * SafiData website `/billing` route in the system's default external browser.
 */
export async function launchDesktopWebsiteBilling(
  sessionToken: string,
  intent: DesktopBillingHandoffRequest,
  options?: {
    readonly apiBaseUrl?: string;
    readonly expectedPublicWebOrigin?: string;
    readonly fetchImpl?: typeof fetch;
    readonly openExternalUrl?: (url: string) => Promise<{ readonly opened: boolean }>;
  }
): Promise<DesktopBillingHandoffResponse> {
  const base = (options?.apiBaseUrl ?? getDefaultAuthApiBaseUrl()).replace(/\/+$/, "");
  const fetchImpl = options?.fetchImpl ?? globalThis.fetch.bind(globalThis);
  const openExternal = options?.openExternalUrl ?? openDesktopExternalBillingUrl;
  const state = generateHandoffState();

  const response = await fetchImpl(`${base}/v1/billing/handoff/start`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Accept: "application/json",
      Authorization: `Bearer ${sessionToken}`
    },
    body: JSON.stringify({
      product: intent.product,
      seats: intent.seats,
      billing_country_iso2: intent.billing_country_iso2.trim().toUpperCase(),
      coupon_code: intent.coupon_code?.trim() || undefined,
      state
    })
  });

  if (!response.ok) {
    let code = "SF-PAY-004";
    try {
      const parsed = (await response.json()) as { code?: string };
      if (typeof parsed.code === "string") code = parsed.code;
    } catch {
      // Keep default error code.
    }
    throw new Error(code);
  }

  const started = (await response.json()) as DesktopBillingHandoffResponse;

  // Fail closed if the returned billing URL is malformed, points outside the approved public web
  // origin, has mismatched state, or smuggles tokens!
  let parsedUrl: URL;
  try {
    parsedUrl = new URL(started.billing_url);
  } catch {
    throw new Error("SF-SYS-009");
  }
  const expectedPublicOrigin = (
    options?.expectedPublicWebOrigin ??
    (typeof __SAFI_PUBLIC_WEB_ORIGIN__ === "string" ? __SAFI_PUBLIC_WEB_ORIGIN__ : "")
  )
    .trim()
    .replace(/\/+$/, "");
  if (expectedPublicOrigin.length > 0 && parsedUrl.origin !== expectedPublicOrigin) {
    throw new Error("SF-SYS-009");
  }
  const returnedHandoff = parsedUrl.searchParams.get("handoff") ?? "";
  const returnedState = parsedUrl.searchParams.get("state") ?? "";
  const paramKeys = Array.from(parsedUrl.searchParams.keys()).sort();
  if (
    parsedUrl.pathname !== "/billing" ||
    !returnedHandoff.startsWith("sfbh_") ||
    returnedState !== state ||
    paramKeys.length !== 2 ||
    paramKeys[0] !== "handoff" ||
    paramKeys[1] !== "state" ||
    parsedUrl.hash.length > 0 ||
    started.billing_url.includes(sessionToken)
  ) {
    throw new Error("SF-SYS-009");
  }

  await openExternal(started.billing_url);
  return started;
}
