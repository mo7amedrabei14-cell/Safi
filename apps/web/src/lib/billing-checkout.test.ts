import { describe, expect, it, vi } from "vitest";
import {
  BillingCheckoutController,
  CheckoutLifecycleError,
  type CheckoutApiResponse,
  type CheckoutIntent,
  type StorageLike
} from "./billing-checkout";

class MemoryStorage implements StorageLike {
  private readonly map = new Map<string, string>();

  getItem(key: string): string | null {
    return this.map.get(key) ?? null;
  }

  setItem(key: string, value: string): void {
    this.map.set(key, value);
  }

  removeItem(key: string): void {
    this.map.delete(key);
  }
}

const MONTHLY_INTENT: CheckoutIntent = {
  product: "individual_monthly",
  billing_country_iso2: "US"
};

function makeCheckoutOkResponse(idempotencyKey: string, reused = false): CheckoutApiResponse {
  return {
    order_id: "019a0000-0000-7000-8000-000000000001",
    idempotency_key: idempotencyKey,
    order_state: "checkout_pending",
    reused_existing_order: reused,
    provider: "paddle",
    currency: "USD",
    original_amount_minor: 2000,
    discount_amount_minor: 0,
    amount_minor: 2000,
    applied_coupon_code: null,
    provider_status: "CONFIGURED",
    provider_order_ref: "txn_01j_example",
    checkout_session_id: "txn_01j_example",
    checkout_mode: "redirect_url",
    checkout_url: "https://checkout.paddle.com/txn_01j_example"
  };
}

describe("BillingCheckoutController — Frontend Duplicate-Charge Prevention", () => {
  it("deduplicates double-click Pay into ONE request with ONE stable idempotency key", async () => {
    const storage = new MemoryStorage();
    let resolveFetch!: (value: Response) => void;
    const fetchMock = vi.fn(
      (_input: RequestInfo | URL, init?: RequestInit) =>
        new Promise<Response>((resolve) => {
          const parsed = JSON.parse(String(init?.body)) as { idempotency_key: string };
          resolveFetch = resolve;
          setTimeout(() => {
            resolveFetch(
              new Response(JSON.stringify(makeCheckoutOkResponse(parsed.idempotency_key)), {
                status: 200,
                headers: { "Content-Type": "application/json" }
              })
            );
          }, 10);
        })
    );

    const ctrl = new BillingCheckoutController({ storage, fetchImpl: fetchMock as typeof fetch });
    const p1 = ctrl.startCheckout(MONTHLY_INTENT, "sfs_test_token");
    expect(ctrl.isSubmitting(MONTHLY_INTENT)).toBe(true);
    const p2 = ctrl.startCheckout(MONTHLY_INTENT, "sfs_test_token");

    const [r1, r2] = await Promise.all([p1, p2]);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(r1.order_id).toBe(r2.order_id);
    expect(r1.idempotency_key).toBe(r2.idempotency_key);
    expect(ctrl.isSubmitting(MONTHLY_INTENT)).toBe(false);
  });

  it("reuses the exact same idempotency key across page refresh / browser back-forward", async () => {
    const storage = new MemoryStorage();
    const seenKeys: string[] = [];
    const fetchMock = vi.fn(async (_input: RequestInfo | URL, init?: RequestInit) => {
      const parsed = JSON.parse(String(init?.body)) as { idempotency_key: string };
      seenKeys.push(parsed.idempotency_key);
      return new Response(
        JSON.stringify(makeCheckoutOkResponse(parsed.idempotency_key, seenKeys.length > 1)),
        { status: 200, headers: { "Content-Type": "application/json" } }
      );
    });

    const firstPageLoad = new BillingCheckoutController({
      storage,
      fetchImpl: fetchMock as typeof fetch
    });
    const res1 = await firstPageLoad.startCheckout(MONTHLY_INTENT, "sfs_test_token");

    // Simulate full browser reload / back button: brand-new controller sharing sessionStorage.
    const afterRefresh = new BillingCheckoutController({
      storage,
      fetchImpl: fetchMock as typeof fetch
    });
    const res2 = await afterRefresh.startCheckout(MONTHLY_INTENT, "sfs_test_token");

    expect(seenKeys).toHaveLength(2);
    expect(seenKeys[0]).toBe(seenKeys[1]);
    expect(res1.idempotency_key).toBe(res2.idempotency_key);
    expect(res2.reused_existing_order).toBe(true);
  });

  it("blocks starting a second charge when prior attempt timed out into reconciliation_required", async () => {
    const storage = new MemoryStorage();
    const seenKeys: string[] = [];
    let attempt = 0;

    const fetchMock = vi.fn(async (_input: RequestInfo | URL, init?: RequestInit) => {
      const parsed = JSON.parse(String(init?.body)) as { idempotency_key: string };
      seenKeys.push(parsed.idempotency_key);
      attempt += 1;
      if (attempt === 1) {
        throw new Error("Network timeout");
      }
      return new Response(JSON.stringify(makeCheckoutOkResponse(parsed.idempotency_key, true)), {
        status: 200,
        headers: { "Content-Type": "application/json" }
      });
    });

    const ctrl = new BillingCheckoutController({ storage, fetchImpl: fetchMock as typeof fetch });
    await expect(ctrl.startCheckout(MONTHLY_INTENT, "sfs_test_token")).rejects.toMatchObject({
      code: "SF-PAY-008",
      requiresReconciliation: true
    });

    // Refuses to clear unresolved attempt or generate a new key!
    expect(ctrl.resetForNewPurchase(MONTHLY_INTENT)).toBe(false);

    // Blind second click without explicit reconcile flag is blocked locally with ZERO second HTTP call!
    await expect(ctrl.startCheckout(MONTHLY_INTENT, "sfs_test_token")).rejects.toBeInstanceOf(
      CheckoutLifecycleError
    );
    expect(fetchMock).toHaveBeenCalledTimes(1);

    // Explicit reconciliation retry uses the EXACT SAME original idempotency key!
    const reconciled = await ctrl.startCheckout(MONTHLY_INTENT, "sfs_test_token", {
      allowReconcileRetry: true
    });
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(seenKeys[0]).toBe(seenKeys[1]);
    expect(reconciled.reused_existing_order).toBe(true);
  });

  it("never treats redirect return as paid without server-side confirmation", async () => {
    const storage = new MemoryStorage();
    const fetchMock = vi.fn(async () => {
      // Server still reports Free plan (webhook not yet confirmed).
      return new Response(
        JSON.stringify({ effective_plan: "free", state: "free", operation_credits: 0 }),
        { status: 200, headers: { "Content-Type": "application/json" } }
      );
    });

    const ctrl = new BillingCheckoutController({ storage, fetchImpl: fetchMock as typeof fetch });
    const verification = await ctrl.verifyServerPaymentCompletion("sfs_test_token", MONTHLY_INTENT);
    expect(verification.confirmed_paid).toBe(false);
    expect(verification.effective_plan).toBe("free");
  });

  it("rotates idempotency key ONLY after a terminal failure/paid state for a brand-new purchase", async () => {
    const storage = new MemoryStorage();
    const seenKeys: string[] = [];
    let call = 0;
    const fetchMock = vi.fn(async (_input: RequestInfo | URL, init?: RequestInit) => {
      const parsed = JSON.parse(String(init?.body)) as { idempotency_key: string };
      seenKeys.push(parsed.idempotency_key);
      call += 1;
      if (call === 1) {
        return new Response(JSON.stringify({ code: "SF-PAY-004", detail: "Card declined" }), {
          status: 400,
          headers: { "Content-Type": "application/json" }
        });
      }
      return new Response(JSON.stringify(makeCheckoutOkResponse(parsed.idempotency_key)), {
        status: 200,
        headers: { "Content-Type": "application/json" }
      });
    });

    const ctrl = new BillingCheckoutController({ storage, fetchImpl: fetchMock as typeof fetch });
    await expect(ctrl.startCheckout(MONTHLY_INTENT, "sfs_test_token")).rejects.toMatchObject({
      code: "SF-PAY-004",
      requiresReconciliation: false
    });

    // Previous attempt definitively failed -> new purchase safely mints a fresh idempotency key!
    expect(ctrl.resetForNewPurchase(MONTHLY_INTENT)).toBe(true);
    const second = await ctrl.startCheckout(MONTHLY_INTENT, "sfs_test_token");
    expect(seenKeys).toHaveLength(2);
    expect(seenKeys[0]).not.toBe(seenKeys[1]);
    expect(second.order_state).toBe("checkout_pending");
  });
});
