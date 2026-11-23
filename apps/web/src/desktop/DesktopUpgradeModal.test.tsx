import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { launchDesktopWebsiteBilling } from "../lib/desktop-billing-handoff";
import { DesktopUpgradeModal } from "./DesktopUpgradeModal";

describe("DesktopUpgradeModal & Desktop-to-Website handoff integration", () => {
  it("renders all 5 purchase upgrade triggers with ZERO card payment inputs inside Desktop UI", () => {
    const html = renderToString(
      createElement(DesktopUpgradeModal, {
        open: true,
        lang: "en",
        sessionToken: "sfs_desktop_session_token_1234567890",
        onClose: () => {}
      })
    );

    expect(html).toContain('data-desktop-upgrade-modal="true"');
    expect(html).toContain('data-desktop-buy-product="individual_monthly"');
    expect(html).toContain('data-desktop-buy-product="individual_annual"');
    expect(html).toContain('data-desktop-buy-product="organization_seat_annual"');
    expect(html).toContain('data-desktop-buy-product="credit_pack_25"');
    expect(html).toContain('data-desktop-buy-product="pay_per_file"');
    expect(html).toContain('data-desktop-upgrade-seats="true"');
    expect(html).toContain('data-desktop-refresh-entitlements="true"');

    // Desktop MUST NEVER render credit-card number/CVV inputs.
    expect(html).not.toContain('autocomplete="cc-number"');
    expect(html).not.toContain('autocomplete="cc-csc"');
    expect(html).not.toContain("Card Number");
  });

  it("initiates single-use handoff for all 5 products and launches official website /billing in external browser with ZERO tokens in URL", async () => {
    const products = [
      { product: "individual_monthly" as const, country: "US", seats: undefined },
      { product: "individual_annual" as const, country: "EG", seats: undefined },
      { product: "organization_seat_annual" as const, country: "US", seats: 5 },
      { product: "credit_pack_25" as const, country: "US", seats: undefined },
      { product: "pay_per_file" as const, country: "EG", seats: undefined }
    ];

    for (const item of products) {
      const openedUrls: string[] = [];
      let capturedState = "";

      const mockFetch: typeof fetch = vi.fn(async (input, init) => {
        const url = typeof input === "string" ? input : input.toString();
        expect(url).toBe("https://api.safidata.app/v1/billing/handoff/start");
        const headers = new Headers(init?.headers);
        expect(headers.get("Authorization")).toBe("Bearer sfs_desktop_session_secret_001");
        const body = JSON.parse(String(init?.body ?? "{}")) as {
          product: string;
          seats?: number;
          billing_country_iso2: string;
          state: string;
        };
        expect(body.product).toBe(item.product);
        expect(body.billing_country_iso2).toBe(item.country);
        if (item.seats !== undefined) {
          expect(body.seats).toBe(item.seats);
        }
        capturedState = body.state;
        expect(capturedState.length).toBeGreaterThanOrEqual(16);

        return new Response(
          JSON.stringify({
            handoff_id: "01950000-0000-7000-8000-000000000055",
            billing_url: `https://safidata.app/billing?handoff=sfbh_once_for_${item.product}&state=${capturedState}`,
            product: item.product,
            seats: item.seats ?? 1,
            billing_country_iso2: item.country,
            provider: item.country === "EG" ? "paymob" : "paddle",
            currency: item.country === "EG" ? "EGP" : "USD",
            original_amount_minor: 2000,
            billing_period: item.product.includes("annual")
              ? "annual"
              : item.product.includes("monthly")
                ? "monthly"
                : "one_time",
            expires_at: 2_000_000_000
          }),
          { status: 200, headers: { "Content-Type": "application/json" } }
        );
      });

      const started = await launchDesktopWebsiteBilling(
        "sfs_desktop_session_secret_001",
        {
          product: item.product,
          ...(item.seats !== undefined ? { seats: item.seats } : {}),
          billing_country_iso2: item.country
        },
        {
          apiBaseUrl: "https://api.safidata.app",
          fetchImpl: mockFetch,
          openExternalUrl: async (url) => {
            openedUrls.push(url);
            return { opened: true };
          }
        }
      );

      expect(started.product).toBe(item.product);
      expect(openedUrls).toHaveLength(1);
      const opened = openedUrls[0] ?? "";
      expect(opened).toContain("https://safidata.app/billing?handoff=sfbh_");
      expect(opened).not.toContain("sfs_desktop_session_secret_001");
      expect(opened).not.toContain("token=");
      expect(opened).not.toContain("session=");
    }
  });

  it("rejects tampered handoff URLs (wrong path, mismatched state, or smuggled access token) before opening browser", async () => {
    const opened = vi.fn(async () => ({ opened: true }));
    const tamperedFetch: typeof fetch = vi.fn(async () => {
      return new Response(
        JSON.stringify({
          handoff_id: "01950000-0000-7000-8000-000000000056",
          billing_url:
            "https://safidata.app/billing?handoff=sfbh_abc1234567890&state=wrong_state&token=sfs_leaked",
          product: "individual_monthly",
          seats: 1,
          billing_country_iso2: "US",
          provider: "paddle",
          currency: "USD",
          original_amount_minor: 2000,
          billing_period: "monthly",
          expires_at: 2_000_000_000
        }),
        { status: 200, headers: { "Content-Type": "application/json" } }
      );
    });

    await expect(
      launchDesktopWebsiteBilling(
        "sfs_desktop_session_secret_001",
        { product: "individual_monthly", billing_country_iso2: "US" },
        {
          apiBaseUrl: "https://api.safidata.app",
          fetchImpl: tamperedFetch,
          openExternalUrl: opened
        }
      )
    ).rejects.toThrow("SF-SYS-009");
    expect(opened).not.toHaveBeenCalled();
  });
});
