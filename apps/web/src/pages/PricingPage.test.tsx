import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { afterEach, describe, expect, it } from "vitest";
import { ApplicationShell } from "../ApplicationShell";
import { FloatingGlassDock } from "../components/layout/FloatingGlassDock";
import { BentoCommandCenter } from "../components/home/BentoCommandCenter";
import {
  buildBillingUrlFromIntent,
  loadPendingWebPurchaseIntent,
  normalizeOrganizationSeats,
  parseBusinessPricingConfig,
  parsePurchaseIntentFromSearch,
  savePendingWebPurchaseIntent
} from "../lib/business-pricing";
import { PricingPage } from "./PricingPage";

class MemoryWebStorage implements Storage {
  private readonly map = new Map<string, string>();
  get length(): number {
    return this.map.size;
  }
  clear(): void {
    this.map.clear();
  }
  getItem(key: string): string | null {
    return this.map.get(key) ?? null;
  }
  key(index: number): string | null {
    return Array.from(this.map.keys())[index] ?? null;
  }
  removeItem(key: string): void {
    this.map.delete(key);
  }
  setItem(key: string, value: string): void {
    this.map.set(key, value);
  }
}

describe("Public Pricing Experience (/pricing), strict config/business.json loader & direct purchase intent", () => {
  afterEach(() => {
    delete (globalThis as { window?: unknown }).window;
  });

  it("lets an anonymous visitor open /pricing without signing in and hides Desktop/Owner UI", () => {
    (globalThis as { window?: unknown }).window = {
      location: {
        href: "https://safidata.app/pricing",
        pathname: "/pricing",
        search: "",
        origin: "https://safidata.app"
      },
      history: { replaceState: () => {}, pushState: () => {} },
      localStorage: new MemoryWebStorage(),
      sessionStorage: new MemoryWebStorage()
    };

    const html = renderToString(createElement(ApplicationShell));
    expect(html).toContain('data-runtime="pricing-public"');
    expect(html).toContain('data-pricing-page="true"');
    expect(html).not.toContain('data-billing-auth-gate="true"');
    expect(html).not.toContain('data-tab-id="studio"');
    expect(html).not.toContain('data-tab-id="owner"');
  });

  it("displays all 5 approved products, both USD & EGP prices, recurring vs one-time separation, and honest Free limits", () => {
    const html = renderToString(
      createElement(PricingPage, {
        lang: "en",
        onSelectPurchase: () => {}
      })
    );

    // All 5 paid purchase cards
    expect(html).toContain('data-pricing-card="individual_monthly"');
    expect(html).toContain('data-pricing-card="individual_annual"');
    expect(html).toContain('data-pricing-card="organization_seat_annual"');
    expect(html).toContain('data-pricing-card="credit_pack_25"');
    expect(html).toContain('data-pricing-card="pay_per_file"');

    // Exact approved USD and EGP values from config/business.json
    expect(html).toContain("$20");
    expect(html).toContain("1,000 EGP");
    expect(html).toContain("$168");
    expect(html).toContain("8,400 EGP");
    expect(html).toContain("$17");
    expect(html).toContain("850 EGP");
    expect(html).toContain("$612");
    expect(html).toContain("30,600 EGP");
    expect(html).toContain("$5");
    expect(html).toContain("250 EGP");
    expect(html).toContain("$2");
    expect(html).toContain("50 EGP");

    // Organization minimum 3 seats
    expect(html).toContain('min="3"');
    expect(html).toContain("minimum 3 seats");

    // Micro-purchase limitations disclosed clearly
    expect(html).toContain("25 additional data operations only");
    expect(html).toContain("100,000 rows");
    expect(html).toContain("Does NOT unlock AI, teams, or premium recipe packs");

    // Free plan disclosed separately with all 8 actual limits
    expect(html).toContain('data-pricing-card="free"');
    expect(html).toContain("5 operations per month");
    expect(html).toContain("Maximum 5,000 rows");
    expect(html).toContain("Maximum 20 columns");
    expect(html).toContain("Mandatory Free watermark");
    expect(html).toContain("No AI");
    expect(html).toContain("No teams");
    expect(html).toContain("No premium recipe packs");
    expect(html).toContain("No saved recipe writing");
  });

  it("supports Arabic RTL and English LTR across PricingPage, public navigation, and Hero CTA", () => {
    const arPricing = renderToString(
      createElement(PricingPage, { lang: "ar", onSelectPurchase: () => {} })
    );
    expect(arPricing).toContain('dir="rtl"');
    expect(arPricing).toContain("الأسعار والباقات المعتمدة");
    expect(arPricing).toContain("الحد الأدنى 3 مقاعد");

    const enDock = renderToString(
      createElement(FloatingGlassDock, { activeTab: "home", onSelect: () => {}, lang: "en" })
    );
    const arDock = renderToString(
      createElement(FloatingGlassDock, { activeTab: "home", onSelect: () => {}, lang: "ar" })
    );
    expect(enDock).toContain('data-nav-pricing="true"');
    expect(enDock).toContain("Plans &amp; Pricing");
    expect(arDock).toContain('data-nav-pricing="true"');
    expect(arDock).toContain("الأسعار والاشتراك");

    const enHero = renderToString(createElement(BentoCommandCenter, { lang: "en" }));
    const arHero = renderToString(createElement(BentoCommandCenter, { lang: "ar" }));
    expect(enHero).toContain('data-hero-pricing-cta="true"');
    expect(enHero).toContain("Plans &amp; Pricing");
    expect(arHero).toContain('data-hero-pricing-cta="true"');
    expect(arHero).toContain("الأسعار والاشتراك");
  });

  it("fails closed with SF-SYS-032 when business configuration is missing or tampered (zero hardcoded fallback prices)", () => {
    expect(() => parseBusinessPricingConfig({})).toThrow("SF-SYS-032");
    expect(() =>
      parseBusinessPricingConfig({
        free_plan: {
          operations_per_month: 5,
          max_rows: 5000,
          max_columns: 20,
          watermark: "on_everything",
          watermark_text: "Free",
          saved_recipes: false,
          recipes_after_downgrade: "read_only_locked",
          premium_recipe_packs: false,
          ai: true, // Illegal: Free must never enable AI
          teams: false
        },
        trial: {
          days: 7,
          requires_card: false,
          requires_verified_email: true,
          requires_verified_phone: true,
          ai_calls_cap: 25
        },
        plans: [
          {
            id: "individual_monthly",
            price: { USD: 20, EGP: 1000 },
            ai_calls_per_month: 400,
            includes_premium_recipe_packs: true
          },
          {
            id: "individual_annual",
            price: { USD: 168, EGP: 8400 },
            ai_calls_per_month: 400,
            includes_premium_recipe_packs: true
          },
          {
            id: "organization_seat",
            min_seats: 3,
            price_per_seat_per_month: { USD: 17, EGP: 850 },
            price_per_seat_per_year: { USD: 204, EGP: 10200 },
            ai_calls_per_seat_per_month: 400,
            includes_premium_recipe_packs: true
          }
        ],
        micro_purchases: {
          credit_pack_25: {
            operations: 25,
            price: { USD: 5, EGP: 250 },
            expires: false,
            removes_watermark: true,
            includes_ai: false,
            includes_premium_recipe_packs: false
          },
          pay_per_file: {
            max_rows: 100000,
            price: { USD: 2, EGP: 50 },
            removes_watermark: true,
            includes_ai: false,
            includes_premium_recipe_packs: false
          }
        }
      })
    ).toThrow("SF-SYS-032");
  });

  it("persists selected product, billing period, country, and validated org seats across signup and email verification", () => {
    const localStorage = new MemoryWebStorage();
    (globalThis as { window?: unknown }).window = {
      location: {
        href: "https://safidata.app/?verify_email=tok_1234567890",
        pathname: "/",
        search: "?verify_email=tok_1234567890",
        origin: "https://safidata.app"
      },
      history: { replaceState: () => {}, pushState: () => {} },
      localStorage,
      sessionStorage: new MemoryWebStorage()
    };

    const saved = savePendingWebPurchaseIntent({
      product: "organization_seat_annual",
      billing_country_iso2: "EG",
      seats: 7
    });
    expect(saved.product).toBe("organization_seat_annual");
    expect(saved.billing_period).toBe("annual");
    expect(saved.billing_country_iso2).toBe("EG");
    expect(saved.seats).toBe(7);

    // Seat floor enforcement (<3 clamped to 3)
    expect(normalizeOrganizationSeats(1)).toBe(3);

    // URL serialization/parsing
    const url = buildBillingUrlFromIntent({
      product: "organization_seat_annual",
      billing_country_iso2: "EG",
      seats: 7
    });
    expect(url).toBe("/billing?product=organization_seat_annual&country=EG&seats=7");
    expect(
      parsePurchaseIntentFromSearch("?product=organization_seat_annual&country=EG&seats=7")
    ).toEqual({
      product: "organization_seat_annual",
      billing_period: "annual",
      billing_country_iso2: "EG",
      seats: 7
    });

    // Reloading after email verification routes directly back into /billing with selected purchase preserved!
    expect(loadPendingWebPurchaseIntent()?.seats).toBe(7);
    const html = renderToString(
      createElement(ApplicationShell, {
        initialVerificationToken: "tok_1234567890"
      })
    );
    expect(html).toContain('data-runtime="billing-web-portal"');
    expect(html).toContain('data-billing-preauth-product="organization_seat_annual"');
    expect(html).toContain("71,400 EGP");
  });
});
