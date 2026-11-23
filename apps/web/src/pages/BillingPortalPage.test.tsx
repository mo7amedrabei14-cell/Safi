import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ApplicationShell } from "../ApplicationShell";
import {
  BillingCheckoutController,
  CheckoutLifecycleError,
  type StorageLike
} from "../lib/billing-checkout";
import { BillingPortalPage } from "./BillingPortalPage";

class MemoryStorage implements StorageLike {
  private readonly store = new Map<string, string>();
  getItem(key: string): string | null {
    return this.store.get(key) ?? null;
  }
  setItem(key: string, value: string): void {
    this.store.set(key, value);
  }
  removeItem(key: string): void {
    this.store.delete(key);
  }
}

describe("Website Billing Portal (/billing) & BillingCheckoutController UI integration", () => {
  afterEach(() => {
    delete (globalThis as { window?: unknown }).window;
  });

  it("mounts BillingPortalPage on /billing without exposing Desktop Studio or Owner controls", () => {
    (globalThis as { window?: unknown }).window = {
      location: {
        href: "https://safidata.app/billing",
        pathname: "/billing",
        search: "",
        origin: "https://safidata.app"
      },
      history: { replaceState: () => {}, pushState: () => {} },
      sessionStorage: new MemoryStorage()
    };

    const html = renderToString(createElement(ApplicationShell));
    expect(html).toContain('data-billing-portal="true"');
    expect(html).toContain('data-billing-auth-gate="true"');
    expect(html).not.toContain('data-tab-id="owner"');
    expect(html).not.toContain('data-tab-id="studio"');
    expect(html).not.toContain("Quad-Studio");
  });

  it("shows all 5 authoritative products from config/business.json when authenticated, with zero raw card inputs", () => {
    const html = renderToString(
      createElement(BillingPortalPage, {
        lang: "en",
        initialSessionToken: "sfb_test_session_token_value_1234567890",
        initialEmail: "buyer@example.test"
      })
    );

    expect(html).toContain('data-billing-plan-option="individual_monthly"');
    expect(html).toContain('data-billing-plan-option="individual_annual"');
    expect(html).toContain('data-billing-plan-option="organization_seat_annual"');
    expect(html).toContain('data-billing-plan-option="credit_pack_25"');
    expect(html).toContain('data-billing-plan-option="pay_per_file"');
    expect(html).toContain("$20");
    expect(html).toContain("$168");
    expect(html).toContain("$204");
    expect(html).toContain("$5");
    expect(html).toContain("$2");
    expect(html).toContain('data-billing-pay-submit="true"');
    expect(html).not.toContain('autocomplete="cc-number"');
    expect(html).not.toContain('autocomplete="cc-csc"');
  });

  it("strips handoff & state immediately from URL bar via history.replaceState on mount", () => {
    const replaceState = vi.fn();
    const sessionStorage = new MemoryStorage();
    (globalThis as { window?: unknown }).window = {
      location: {
        href: "https://safidata.app/billing?handoff=sfbh_secret_once_token_1234567890&state=csrf_state_token_1234567890",
        pathname: "/billing",
        search: "?handoff=sfbh_secret_once_token_1234567890&state=csrf_state_token_1234567890",
        origin: "https://safidata.app"
      },
      history: { state: null, replaceState },
      sessionStorage
    };

    const html = renderToString(createElement(BillingPortalPage, { lang: "en" }));
    expect(replaceState).toHaveBeenCalledWith(null, "", "/billing");
    expect(html).toContain('data-billing-auth-gate="true"');
    expect(html).not.toContain("sfbh_secret_once_token_1234567890");
    expect(html).not.toContain("csrf_state_token_1234567890");
  });

  it("executes full website login -> handoff redeem -> BillingCheckoutController checkout -> order status -> server-only entitlement verification", async () => {
    const seenCheckoutKeys: string[] = [];
    let webhookConfirmed = false;

    const mockFetch: typeof fetch = vi.fn(async (input, init) => {
      const url = typeof input === "string" ? input : input.toString();
      const headers = new Headers(init?.headers);

      if (url.endsWith("/v1/billing/web/login")) {
        return new Response(
          JSON.stringify({
            token: "sfb_scoped_website_billing_token_001",
            session_id: "01950000-0000-7000-8000-000000000000",
            user_id: "01950000-0000-7000-8000-000000000001",
            workspace_id: "01950000-0000-7000-8000-000000000002",
            email: "buyer@example.test",
            session_type: "billing_web",
            expires_at: 2_000_000_000
          }),
          { status: 200, headers: { "Content-Type": "application/json" } }
        );
      }

      if (url.endsWith("/v1/billing/handoff/redeem")) {
        expect(headers.get("Authorization")).toBe("Bearer sfb_scoped_website_billing_token_001");
        return new Response(
          JSON.stringify({
            handoff_id: "01950000-0000-7000-8000-000000000010",
            user_id: "01950000-0000-7000-8000-000000000001",
            workspace_id: "01950000-0000-7000-8000-000000000002",
            product: "organization_seat_annual",
            seats: 5,
            billing_country_iso2: "US",
            coupon_code: null,
            provider: "paddle",
            currency: "USD",
            original_amount_minor: 102000,
            billing_period: "annual"
          }),
          { status: 200, headers: { "Content-Type": "application/json" } }
        );
      }

      if (url.endsWith("/v1/billing/checkout")) {
        const body = JSON.parse(String(init?.body ?? "{}")) as { idempotency_key: string };
        seenCheckoutKeys.push(body.idempotency_key);
        return new Response(
          JSON.stringify({
            order_id: "01950000-0000-7000-8000-000000000099",
            idempotency_key: body.idempotency_key,
            order_state: "checkout_pending",
            reused_existing_order: seenCheckoutKeys.length > 1,
            provider: "paddle",
            currency: "USD",
            original_amount_minor: 102000,
            discount_amount_minor: 0,
            amount_minor: 102000,
            applied_coupon_code: null,
            provider_status: "ready",
            provider_order_ref: "txn_org_5_seats",
            checkout_session_id: "txn_org_5_seats",
            checkout_mode: "hosted_redirect",
            checkout_url: "https://sandbox-checkout.paddle.com/txn_org_5_seats"
          }),
          { status: 200, headers: { "Content-Type": "application/json" } }
        );
      }

      if (url.includes("/v1/billing/orders/")) {
        return new Response(
          JSON.stringify({
            order_id: "01950000-0000-7000-8000-000000000099",
            order_state: webhookConfirmed ? "paid" : "checkout_pending",
            provider: "paddle",
            product: "organization_seat_annual",
            seats: 5,
            currency: "USD",
            original_amount_minor: 102000,
            discount_amount_minor: 0,
            expected_amount_minor: 102000,
            checkout_url: "https://sandbox-checkout.paddle.com/txn_org_5_seats",
            paid_at: webhookConfirmed ? 1_900_000_000 : null,
            updated_at: 1_900_000_000
          }),
          { status: 200, headers: { "Content-Type": "application/json" } }
        );
      }

      if (url.endsWith("/v1/entitlements/me")) {
        return new Response(
          JSON.stringify({
            effective_plan: webhookConfirmed ? "organization" : "free",
            state: "active",
            operation_credits: 0,
            pay_per_file_tokens: 0
          }),
          { status: 200, headers: { "Content-Type": "application/json" } }
        );
      }

      throw new Error(`Unexpected URL: ${url}`);
    });

    const controller = new BillingCheckoutController({
      apiBaseUrl: "https://api.safidata.app",
      storage: new MemoryStorage(),
      fetchImpl: mockFetch
    });

    const login = await controller.loginBillingWeb("buyer@example.test", "SafiStrongPassword!1");
    expect(login.token).toBe("sfb_scoped_website_billing_token_001");
    expect(login.session_type).toBe("billing_web");

    const handoff = await controller.redeemDesktopHandoff(
      "sfbh_once_code_1234567890",
      "csrf_state_1234567890",
      login.token
    );
    expect(handoff.product).toBe("organization_seat_annual");
    expect(handoff.seats).toBe(5);
    expect(handoff.original_amount_minor).toBe(102000);

    const intent = {
      product: handoff.product,
      seats: handoff.seats,
      billing_country_iso2: handoff.billing_country_iso2
    };

    const checkout = await controller.startCheckout(intent, login.token);
    expect(checkout.order_state).toBe("checkout_pending");
    expect(checkout.checkout_url).toBe("https://sandbox-checkout.paddle.com/txn_org_5_seats");
    expect(seenCheckoutKeys).toHaveLength(1);

    // Return before webhook: NEVER treated as paid!
    const beforeWebhook = await controller.verifyServerPaymentCompletion(login.token, intent);
    expect(beforeWebhook.confirmed_paid).toBe(false);
    expect(beforeWebhook.effective_plan).toBe("free");

    // Duplicate retry after redirect reuses the EXACT same idempotency key.
    const retry = await controller.startCheckout(intent, login.token);
    expect(retry.order_id).toBe(checkout.order_id);
    expect(seenCheckoutKeys).toEqual([seenCheckoutKeys[0], seenCheckoutKeys[0]]);

    // After verified webhook, server confirms paid!
    webhookConfirmed = true;
    const status = await controller.fetchOrderStatus(checkout.order_id, login.token);
    expect(status.order_state).toBe("paid");
    const afterWebhook = await controller.verifyServerPaymentCompletion(login.token, intent);
    expect(afterWebhook.confirmed_paid).toBe(true);
    expect(afterWebhook.effective_plan).toBe("organization");
  });

  it("blocks blind double-charge retry when provider timeout leaves reconciliation_required", async () => {
    const mockFetch: typeof fetch = vi.fn(async () => {
      throw new Error("network timeout");
    });

    const controller = new BillingCheckoutController({
      apiBaseUrl: "https://api.safidata.app",
      storage: new MemoryStorage(),
      fetchImpl: mockFetch
    });

    const intent = { product: "credit_pack_25" as const, billing_country_iso2: "US" };

    await expect(
      controller.startCheckout(intent, "sfb_scoped_website_billing_token_001")
    ).rejects.toMatchObject({ code: "SF-PAY-008", requiresReconciliation: true });

    // Second blind attempt MUST fail closed with CheckoutLifecycleError(SF-PAY-008) without calling fetch!
    await expect(
      controller.startCheckout(intent, "sfb_scoped_website_billing_token_001")
    ).rejects.toBeInstanceOf(CheckoutLifecycleError);
    expect(mockFetch).toHaveBeenCalledTimes(1);
  });
});
