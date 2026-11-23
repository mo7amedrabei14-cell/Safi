/**
 * Frontend Checkout Lifecycle & Duplicate-Charge Prevention Controller.
 *
 * Guarantees (Section 4 — Payment Integrity & Zero Duplicate Charges):
 * 1. Generates ONE stable `idempotency_key` per intentional purchase attempt and persists it
 *    in `sessionStorage` keyed by purchase intent `(product, billing_country_iso2, seats, coupon_code)`.
 * 2. Deduplicates simultaneous clicks / double-submits via an in-flight lock so double-clicking
 *    "Pay" issues EXACTLY ONE `POST /v1/billing/checkout` request.
 * 3. Preserves the exact same `idempotency_key` across page reloads, browser back/forward
 *    navigation, and transient network timeouts/retries so the server returns the same order
 *    instead of creating a second provider charge.
 * 4. Blocks starting a second charge whenever an attempt is `reconciliation_required` or
 *    `checkout_pending` (returns the existing pending session or blocks until reconciled).
 * 5. NEVER treats a provider redirect URL (`?checkout=success`) as paid — always requires
 *    authoritative server-side verification (`/v1/entitlements/me` / `/v1/billing/invoices`).
 * 6. Rotates the `idempotency_key` ONLY when starting a brand-new purchase after a terminal
 *    outcome (`paid`, `failed`, `canceled`, `expired`).
 */

export type CheckoutProduct =
  | "individual_monthly"
  | "individual_annual"
  | "organization_seat_annual"
  | "pay_per_file"
  | "credit_pack_25";

export type PaymentOrderState =
  | "created"
  | "checkout_pending"
  | "payment_pending"
  | "paid"
  | "failed"
  | "expired"
  | "canceled"
  | "refund_pending"
  | "refunded"
  | "partially_refunded"
  | "chargeback"
  | "reconciliation_required";

export interface CustomerBillingData {
  readonly first_name: string;
  readonly last_name: string;
  readonly email: string;
  readonly phone_number: string;
  readonly country?: string;
  readonly city?: string;
  readonly street?: string;
  readonly building?: string;
  readonly floor?: string;
  readonly apartment?: string;
}

export interface CheckoutIntent {
  readonly product: CheckoutProduct;
  readonly billing_country_iso2: string;
  readonly seats?: number;
  readonly coupon_code?: string;
  readonly billing_data?: CustomerBillingData;
}

export interface CheckoutApiResponse {
  readonly order_id: string;
  readonly idempotency_key: string;
  readonly order_state: PaymentOrderState;
  readonly reused_existing_order: boolean;
  readonly provider: "paddle" | "paymob" | "manual";
  readonly currency: "USD" | "EGP";
  readonly original_amount_minor: number;
  readonly discount_amount_minor: number;
  readonly amount_minor: number;
  readonly applied_coupon_code: string | null;
  readonly provider_status: string;
  readonly provider_order_ref: string | null;
  readonly checkout_session_id: string | null;
  readonly checkout_mode: string;
  readonly checkout_url: string | null;
}

export interface PersistedCheckoutAttempt {
  readonly idempotency_key: string;
  readonly intent_key: string;
  readonly state:
    | "in_flight"
    | "checkout_pending"
    | "reconciliation_required"
    | "paid"
    | "failed"
    | "canceled"
    | "expired";
  readonly order_id: string | null;
  readonly checkout_url: string | null;
  readonly updated_at_ms: number;
}

export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

export class CheckoutLifecycleError extends Error {
  readonly code: string;
  readonly status: number;
  readonly requiresReconciliation: boolean;

  constructor(message: string, code: string, status: number, requiresReconciliation = false) {
    super(message);
    this.name = "CheckoutLifecycleError";
    this.code = code;
    this.status = status;
    this.requiresReconciliation = requiresReconciliation;
  }
}

export interface ServerEntitlementVerification {
  readonly confirmed_paid: boolean;
  readonly effective_plan: string;
  readonly state: string;
  readonly watermark_required?: boolean;
  readonly operations_limit_per_month?: number | null;
  readonly operations_used_this_month?: number;
  readonly max_rows_per_file?: number | null;
  readonly max_columns_per_file?: number | null;
  readonly ai_enabled?: boolean;
  readonly ai_calls_limit?: number;
  readonly ai_calls_used?: number;
  readonly teams_enabled?: boolean;
  readonly seats?: number;
  readonly operation_credits?: number;
  readonly pay_per_file_tokens?: number;
}

export interface BillingOrderStatusResponse {
  readonly order_id: string;
  readonly order_state: PaymentOrderState;
  readonly provider: "paddle" | "paymob" | "manual";
  readonly product: CheckoutProduct;
  readonly seats: number;
  readonly currency: "USD" | "EGP";
  readonly original_amount_minor: number;
  readonly discount_amount_minor: number;
  readonly expected_amount_minor: number;
  readonly checkout_url: string | null;
  readonly paid_at: number | null;
  readonly updated_at: number;
}

export interface BillingWebLoginResponse {
  readonly token: string;
  readonly session_id: string;
  readonly user_id: string;
  readonly workspace_id: string;
  readonly email: string;
  readonly session_type: "billing_web";
  readonly expires_at: number;
}

export interface RedeemedBillingHandoffResponse {
  readonly handoff_id: string;
  readonly user_id: string;
  readonly workspace_id: string;
  readonly product: CheckoutProduct;
  readonly seats: number;
  readonly billing_country_iso2: string;
  readonly coupon_code: string | null;
  readonly provider: "paddle" | "paymob" | "manual";
  readonly currency: "USD" | "EGP";
  readonly original_amount_minor: number;
  readonly billing_period: "monthly" | "annual" | "one_time";
}

export interface CouponPreviewApiResponse {
  readonly code: string;
  readonly original_amount_minor: number;
  readonly discount_amount_minor: number;
  readonly final_amount_minor: number;
  readonly currency: "USD" | "EGP";
}

const STORAGE_PREFIX = "safi.checkout.attempt.v1:";
const ATTEMPT_TTL_MS = 30 * 60 * 1000;

function isTerminalState(state: PersistedCheckoutAttempt["state"]): boolean {
  return state === "paid" || state === "failed" || state === "canceled" || state === "expired";
}

export function buildIntentKey(intent: CheckoutIntent): string {
  const country = intent.billing_country_iso2.trim().toUpperCase();
  const seats = intent.seats ?? 1;
  const coupon = (intent.coupon_code ?? "").trim().toUpperCase();
  return `${intent.product}:${country}:${seats}:${coupon}`;
}

function generateIdempotencyKey(): string {
  if (
    typeof globalThis.crypto !== "undefined" &&
    typeof globalThis.crypto.randomUUID === "function"
  ) {
    return `chk_${globalThis.crypto.randomUUID()}`;
  }
  const bytes = new Uint8Array(16);
  if (
    typeof globalThis.crypto !== "undefined" &&
    typeof globalThis.crypto.getRandomValues === "function"
  ) {
    globalThis.crypto.getRandomValues(bytes);
  } else {
    for (let i = 0; i < bytes.length; i += 1) {
      bytes[i] = Math.floor(Math.random() * 256);
    }
  }
  const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
  return `chk_${hex}`;
}

function getDefaultStorage(): StorageLike | null {
  try {
    if (typeof window !== "undefined" && window.sessionStorage) {
      return window.sessionStorage;
    }
  } catch {
    return null;
  }
  return null;
}

export class BillingCheckoutController {
  private readonly storage: StorageLike | null;
  private readonly fetchImpl: typeof fetch;
  private readonly apiBaseUrl: string;
  private readonly nowMs: () => number;
  private readonly inFlightByIntent = new Map<string, Promise<CheckoutApiResponse>>();

  constructor(options?: {
    apiBaseUrl?: string;
    storage?: StorageLike | null;
    fetchImpl?: typeof fetch;
    nowMs?: () => number;
  }) {
    this.apiBaseUrl = (options?.apiBaseUrl ?? "").replace(/\/+$/, "");
    this.storage = options?.storage !== undefined ? options.storage : getDefaultStorage();
    this.fetchImpl = options?.fetchImpl ?? globalThis.fetch.bind(globalThis);
    this.nowMs = options?.nowMs ?? (() => Date.now());
  }

  /** Whether a checkout request for this purchase intent is currently in flight (disable Pay button). */
  isSubmitting(intent: CheckoutIntent): boolean {
    return this.inFlightByIntent.has(buildIntentKey(intent));
  }

  /** Read the current persisted checkout attempt for this purchase intent (survives refresh/back-forward). */
  getAttempt(intent: CheckoutIntent): PersistedCheckoutAttempt | null {
    if (!this.storage) return null;
    const raw = this.storage.getItem(STORAGE_PREFIX + buildIntentKey(intent));
    if (!raw) return null;
    try {
      const parsed = JSON.parse(raw) as PersistedCheckoutAttempt;
      if (!parsed || typeof parsed.idempotency_key !== "string") return null;
      if (this.nowMs() - parsed.updated_at_ms > ATTEMPT_TTL_MS && isTerminalState(parsed.state)) {
        this.storage.removeItem(STORAGE_PREFIX + buildIntentKey(intent));
        return null;
      }
      return parsed;
    } catch {
      return null;
    }
  }

  private saveAttempt(attempt: PersistedCheckoutAttempt): void {
    if (!this.storage) return;
    this.storage.setItem(STORAGE_PREFIX + attempt.intent_key, JSON.stringify(attempt));
  }

  /**
   * Explicitly clear a terminal checkout attempt so the user can start a genuinely NEW purchase
   * with a fresh idempotency key. Refuses to clear unresolved `reconciliation_required` or `in_flight` attempts!
   */
  resetForNewPurchase(intent: CheckoutIntent): boolean {
    const intentKey = buildIntentKey(intent);
    if (this.inFlightByIntent.has(intentKey)) {
      return false;
    }
    const current = this.getAttempt(intent);
    if (current && (current.state === "reconciliation_required" || current.state === "in_flight")) {
      return false;
    }
    this.storage?.removeItem(STORAGE_PREFIX + intentKey);
    return true;
  }

  /**
   * Initiate or safely resume/reconcile a protected checkout request.
   *
   * - Concurrent double-clicks share the exact same in-flight Promise (1 HTTP request total).
   * - Page refresh / back-forward / network retry reuses the EXACT SAME persisted `idempotency_key`.
   * - Unresolved `reconciliation_required` retries ONLY with the original `idempotency_key` so
   *   the backend reconciles instead of creating a duplicate charge.
   */
  async startCheckout(
    intent: CheckoutIntent,
    bearerToken: string,
    options?: { readonly allowReconcileRetry?: boolean }
  ): Promise<CheckoutApiResponse> {
    const intentKey = buildIntentKey(intent);
    const existingInFlight = this.inFlightByIntent.get(intentKey);
    if (existingInFlight) {
      return existingInFlight;
    }

    const existing = this.getAttempt(intent);
    if (existing?.state === "reconciliation_required" && !options?.allowReconcileRetry) {
      throw new CheckoutLifecycleError(
        "Previous payment attempt is awaiting reconciliation; refusing to start a second charge.",
        "SF-PAY-008",
        503,
        true
      );
    }

    const idempotencyKey =
      existing && !isTerminalState(existing.state)
        ? existing.idempotency_key
        : generateIdempotencyKey();

    this.saveAttempt({
      idempotency_key: idempotencyKey,
      intent_key: intentKey,
      state: "in_flight",
      order_id: existing?.order_id ?? null,
      checkout_url: existing?.checkout_url ?? null,
      updated_at_ms: this.nowMs()
    });

    const promise = this.executeCheckoutRequest(
      intent,
      intentKey,
      idempotencyKey,
      bearerToken
    ).finally(() => {
      this.inFlightByIntent.delete(intentKey);
    });

    this.inFlightByIntent.set(intentKey, promise);
    return promise;
  }

  private async executeCheckoutRequest(
    intent: CheckoutIntent,
    intentKey: string,
    idempotencyKey: string,
    bearerToken: string
  ): Promise<CheckoutApiResponse> {
    let response: Response;
    try {
      response = await this.fetchImpl(`${this.apiBaseUrl}/v1/billing/checkout`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Accept: "application/json",
          Authorization: `Bearer ${bearerToken}`
        },
        body: JSON.stringify({
          product: intent.product,
          billing_country_iso2: intent.billing_country_iso2.trim().toUpperCase(),
          seats: intent.seats,
          coupon_code: intent.coupon_code,
          billing_data: intent.billing_data,
          idempotency_key: idempotencyKey
        })
      });
    } catch {
      // Network timeout/drop: provider outcome is UNKNOWN -> mark reconciliation_required & KEEP key!
      this.saveAttempt({
        idempotency_key: idempotencyKey,
        intent_key: intentKey,
        state: "reconciliation_required",
        order_id: null,
        checkout_url: null,
        updated_at_ms: this.nowMs()
      });
      throw new CheckoutLifecycleError(
        "Network failure during checkout; payment status requires reconciliation before retrying.",
        "SF-PAY-008",
        503,
        true
      );
    }

    if (!response.ok) {
      let code = "SF-PAY-004";
      let detail = `Checkout failed with status ${response.status}`;
      try {
        const problem = (await response.json()) as {
          code?: string;
          detail?: string;
          title?: string;
        };
        if (typeof problem.code === "string" && problem.code.length > 0) code = problem.code;
        if (typeof problem.detail === "string" && problem.detail.length > 0)
          detail = problem.detail;
        else if (typeof problem.title === "string" && problem.title.length > 0)
          detail = problem.title;
      } catch {
        // Keep default error details.
      }

      const needsReconcile = code === "SF-PAY-008" || response.status >= 500;
      this.saveAttempt({
        idempotency_key: idempotencyKey,
        intent_key: intentKey,
        state: needsReconcile ? "reconciliation_required" : "failed",
        order_id: null,
        checkout_url: null,
        updated_at_ms: this.nowMs()
      });
      throw new CheckoutLifecycleError(detail, code, response.status, needsReconcile);
    }

    const data = (await response.json()) as CheckoutApiResponse;
    const mappedState: PersistedCheckoutAttempt["state"] =
      data.order_state === "paid"
        ? "paid"
        : data.order_state === "reconciliation_required"
          ? "reconciliation_required"
          : data.order_state === "failed"
            ? "failed"
            : data.order_state === "canceled"
              ? "canceled"
              : data.order_state === "expired"
                ? "expired"
                : "checkout_pending";

    this.saveAttempt({
      idempotency_key: data.idempotency_key || idempotencyKey,
      intent_key: intentKey,
      state: mappedState,
      order_id: data.order_id,
      checkout_url: data.checkout_url,
      updated_at_ms: this.nowMs()
    });
    return data;
  }

  /**
   * Verify payment completion strictly from the authoritative backend (`/v1/entitlements/me`).
   * NEVER trusts redirect query parameters (like `?checkout=success` or `?status=paid`).
   */
  async verifyServerPaymentCompletion(
    bearerToken: string,
    expectedIntent?: CheckoutIntent
  ): Promise<ServerEntitlementVerification> {
    const response = await this.fetchImpl(`${this.apiBaseUrl}/v1/entitlements/me`, {
      method: "GET",
      headers: {
        Accept: "application/json",
        Authorization: `Bearer ${bearerToken}`
      }
    });
    if (!response.ok) {
      throw new CheckoutLifecycleError(
        "Unable to verify payment status from server.",
        "SF-SYS-001",
        response.status,
        false
      );
    }
    const ent = (await response.json()) as {
      effective_plan: string;
      state: string;
      operation_credits?: number;
      pay_per_file_tokens?: number;
    };

    const confirmedPaid =
      expectedIntent?.product === "credit_pack_25"
        ? (ent.operation_credits ?? 0) > 0
        : expectedIntent?.product === "pay_per_file"
          ? (ent.pay_per_file_tokens ?? 0) > 0
          : ent.effective_plan !== "free" && ent.state === "active";

    if (confirmedPaid && expectedIntent) {
      const existing = this.getAttempt(expectedIntent);
      if (existing) {
        this.saveAttempt({
          ...existing,
          state: "paid",
          updated_at_ms: this.nowMs()
        });
      }
    }

    return {
      confirmed_paid: confirmedPaid,
      effective_plan: ent.effective_plan,
      state: ent.state,
      ...(ent.operation_credits !== undefined ? { operation_credits: ent.operation_credits } : {}),
      ...(ent.pay_per_file_tokens !== undefined
        ? { pay_per_file_tokens: ent.pay_per_file_tokens }
        : {})
    };
  }

  /** Fetch authoritative server order status (`GET /v1/billing/orders/{orderId}`). */
  async fetchOrderStatus(
    orderId: string,
    bearerToken: string
  ): Promise<BillingOrderStatusResponse> {
    const response = await this.fetchImpl(
      `${this.apiBaseUrl}/v1/billing/orders/${encodeURIComponent(orderId)}`,
      {
        method: "GET",
        headers: {
          Accept: "application/json",
          Authorization: `Bearer ${bearerToken}`
        }
      }
    );
    if (!response.ok) {
      let code = "SF-PAY-004";
      try {
        const parsed = (await response.json()) as { code?: string };
        if (typeof parsed.code === "string") code = parsed.code;
      } catch {
        // Keep default code.
      }
      throw new CheckoutLifecycleError(
        "Could not read order status from server.",
        code,
        response.status
      );
    }
    return (await response.json()) as BillingOrderStatusResponse;
  }

  /** Fetch full authoritative entitlements (`GET /v1/entitlements/me`). */
  async fetchEntitlements(bearerToken: string): Promise<ServerEntitlementVerification> {
    const response = await this.fetchImpl(`${this.apiBaseUrl}/v1/entitlements/me`, {
      method: "GET",
      headers: {
        Accept: "application/json",
        Authorization: `Bearer ${bearerToken}`
      }
    });
    if (!response.ok) {
      throw new CheckoutLifecycleError(
        "Unable to fetch entitlements from server.",
        "SF-AUTH-005",
        response.status
      );
    }
    const ent = (await response.json()) as Record<string, unknown>;
    const effectivePlan = String(ent.effective_plan ?? "free");
    const state = String(ent.state ?? "free");
    return {
      confirmed_paid: effectivePlan !== "free" && state === "active",
      effective_plan: effectivePlan,
      state,
      watermark_required: Boolean(ent.watermark_required),
      operations_limit_per_month:
        typeof ent.operations_limit_per_month === "number" ? ent.operations_limit_per_month : null,
      operations_used_this_month:
        typeof ent.operations_used_this_month === "number" ? ent.operations_used_this_month : 0,
      max_rows_per_file: typeof ent.max_rows_per_file === "number" ? ent.max_rows_per_file : null,
      max_columns_per_file:
        typeof ent.max_columns_per_file === "number" ? ent.max_columns_per_file : null,
      ai_enabled: Boolean(ent.ai_enabled),
      ai_calls_limit: typeof ent.ai_calls_limit === "number" ? ent.ai_calls_limit : 0,
      ai_calls_used: typeof ent.ai_calls_used === "number" ? ent.ai_calls_used : 0,
      teams_enabled: Boolean(ent.teams_enabled),
      seats: typeof ent.seats === "number" ? ent.seats : 1,
      operation_credits: typeof ent.operation_credits === "number" ? ent.operation_credits : 0,
      pay_per_file_tokens: typeof ent.pay_per_file_tokens === "number" ? ent.pay_per_file_tokens : 0
    };
  }

  /** Authenticate on the Website Billing Portal without revoking the user's active Desktop device. */
  async loginBillingWeb(email: string, password: string): Promise<BillingWebLoginResponse> {
    const response = await this.fetchImpl(`${this.apiBaseUrl}/v1/billing/web/login`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Accept: "application/json"
      },
      body: JSON.stringify({
        email: email.trim(),
        password,
        user_agent_hint: "safidata-web-billing"
      })
    });
    if (!response.ok) {
      let code = "SF-AUTH-001";
      try {
        const parsed = (await response.json()) as { code?: string };
        if (typeof parsed.code === "string") code = parsed.code;
      } catch {
        // Keep default code.
      }
      throw new CheckoutLifecycleError(
        "Website billing authentication failed.",
        code,
        response.status
      );
    }
    return (await response.json()) as BillingWebLoginResponse;
  }

  /** Redeem a single-use Desktop-to-Website handoff code bound strictly to the authenticated user. */
  async redeemDesktopHandoff(
    handoffCode: string,
    state: string,
    bearerToken: string
  ): Promise<RedeemedBillingHandoffResponse> {
    const response = await this.fetchImpl(`${this.apiBaseUrl}/v1/billing/handoff/redeem`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Accept: "application/json",
        Authorization: `Bearer ${bearerToken}`
      },
      body: JSON.stringify({
        handoff_code: handoffCode.trim(),
        state: state.trim()
      })
    });
    if (!response.ok) {
      let code = "SF-AUTH-023";
      try {
        const parsed = (await response.json()) as { code?: string };
        if (typeof parsed.code === "string") code = parsed.code;
      } catch {
        // Keep default code.
      }
      throw new CheckoutLifecycleError(
        "Could not redeem Desktop billing handoff.",
        code,
        response.status
      );
    }
    return (await response.json()) as RedeemedBillingHandoffResponse;
  }

  /** Preview a coupon code against the server's authoritative pricing engine. */
  async previewCoupon(
    intent: CheckoutIntent,
    bearerToken: string
  ): Promise<CouponPreviewApiResponse> {
    const response = await this.fetchImpl(`${this.apiBaseUrl}/v1/billing/coupons/preview`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Accept: "application/json",
        Authorization: `Bearer ${bearerToken}`
      },
      body: JSON.stringify({
        code: (intent.coupon_code ?? "").trim(),
        product: intent.product,
        billing_country_iso2: intent.billing_country_iso2.trim().toUpperCase(),
        seats: intent.seats
      })
    });
    if (!response.ok) {
      let code = "SF-PAY-005";
      try {
        const parsed = (await response.json()) as { code?: string };
        if (typeof parsed.code === "string") code = parsed.code;
      } catch {
        // Keep default code.
      }
      throw new CheckoutLifecycleError("Coupon is invalid or not eligible.", code, response.status);
    }
    return (await response.json()) as CouponPreviewApiResponse;
  }
}
