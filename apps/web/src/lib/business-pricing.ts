/**
 * `apps/web/src/lib/business-pricing.ts`
 *
 * Single Authoritative Frontend Loader for `config/business.json`:
 * - Strictly validates every required plan, micro-purchase, trial, and Free-plan limit from
 *   `config/business.json` at load time.
 * - ZERO hardcoded fallback prices or limits: any missing or invalid business configuration
 *   throws `SF-SYS-032` immediately rather than silently substituting a second set of prices.
 * - Also manages persistent Direct Website Purchase intent across `/pricing` -> `/billing` ->
 *   Sign-In / Sign-Up / Email Verification links.
 */
import rawBusinessConfig from "../../../../config/business.json";
import type { CheckoutProduct } from "./billing-checkout";

export interface DualCurrencyPrice {
  readonly USD: number;
  readonly EGP: number;
}

export interface ValidatedFreePlanConfig {
  readonly operationsPerMonth: number;
  readonly maxRows: number;
  readonly maxColumns: number;
  readonly watermark: string;
  readonly watermarkText: string;
  readonly savedRecipes: boolean;
  readonly recipesAfterDowngrade: string;
  readonly premiumRecipePacks: boolean;
  readonly ai: boolean;
  readonly teams: boolean;
}

export interface ValidatedTrialConfig {
  readonly days: number;
  readonly requiresCard: boolean;
  readonly requiresVerifiedEmail: boolean;
  readonly requiresVerifiedPhone: boolean;
  readonly aiCallsCap: number;
}

export interface ValidatedBusinessPricing {
  readonly freePlan: ValidatedFreePlanConfig;
  readonly trial: ValidatedTrialConfig;
  readonly individualMonthly: {
    readonly price: DualCurrencyPrice;
    readonly aiCallsPerMonth: number;
    readonly includesPremiumRecipePacks: boolean;
  };
  readonly individualAnnual: {
    readonly price: DualCurrencyPrice;
    readonly aiCallsPerMonth: number;
    readonly includesPremiumRecipePacks: boolean;
  };
  readonly organizationSeat: {
    readonly minSeats: number;
    readonly pricePerSeatPerMonth: DualCurrencyPrice;
    readonly pricePerSeatPerYear: DualCurrencyPrice;
    readonly aiCallsPerSeatPerMonth: number;
    readonly includesPremiumRecipePacks: boolean;
  };
  readonly creditPack25: {
    readonly operations: number;
    readonly price: DualCurrencyPrice;
    readonly expires: boolean;
    readonly removesWatermark: boolean;
    readonly includesAi: boolean;
    readonly includesPremiumRecipePacks: boolean;
  };
  readonly payPerFile: {
    readonly maxRows: number;
    readonly price: DualCurrencyPrice;
    readonly removesWatermark: boolean;
    readonly includesAi: boolean;
    readonly includesPremiumRecipePacks: boolean;
  };
}

export interface PricingCatalogEntry {
  readonly product: CheckoutProduct;
  readonly category: "subscription" | "micro_purchase";
  readonly billingPeriod: "monthly" | "annual" | "one_time";
  readonly titleEn: string;
  readonly titleAr: string;
  readonly periodEn: string;
  readonly periodAr: string;
  readonly subtitleEn: string;
  readonly subtitleAr: string;
  readonly usdMajor: number;
  readonly egpMajor: number;
  readonly perSeat?: boolean;
  readonly perSeatMonthlyUsd?: number;
  readonly perSeatMonthlyEgp?: number;
  readonly minSeats?: number;
}

function asRecord(value: unknown, path: string): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error(`SF-SYS-032: invalid config/business.json object at ${path}`);
  }
  return value as Record<string, unknown>;
}

function requirePositiveNumber(value: unknown, path: string): number {
  if (typeof value !== "number" || !Number.isFinite(value) || value <= 0) {
    throw new Error(`SF-SYS-032: invalid positive number in config/business.json at ${path}`);
  }
  return value;
}

function requireBoolean(value: unknown, path: string): boolean {
  if (typeof value !== "boolean") {
    throw new Error(`SF-SYS-032: invalid boolean in config/business.json at ${path}`);
  }
  return value;
}

function requireNonEmptyString(value: unknown, path: string): string {
  if (typeof value !== "string" || value.trim().length === 0) {
    throw new Error(`SF-SYS-032: invalid string in config/business.json at ${path}`);
  }
  return value;
}

function requireDualCurrency(value: unknown, path: string): DualCurrencyPrice {
  const rec = asRecord(value, path);
  return {
    USD: requirePositiveNumber(rec.USD, `${path}.USD`),
    EGP: requirePositiveNumber(rec.EGP, `${path}.EGP`)
  };
}

/**
 * Strictly parses and validates `config/business.json`.
 * Throws `SF-SYS-032` immediately if any approved price or limit is missing or malformed.
 */
export function parseBusinessPricingConfig(raw: unknown): ValidatedBusinessPricing {
  const root = asRecord(raw, "root");
  const freeRaw = asRecord(root.free_plan, "free_plan");
  const trialRaw = asRecord(root.trial, "trial");

  if (!Array.isArray(root.plans)) {
    throw new Error("SF-SYS-032: config/business.json plans must be an array");
  }
  const plansById = new Map<string, Record<string, unknown>>();
  for (let i = 0; i < root.plans.length; i += 1) {
    const plan = asRecord(root.plans[i], `plans[${i}]`);
    const id = requireNonEmptyString(plan.id, `plans[${i}].id`);
    plansById.set(id, plan);
  }

  const indMonthly = plansById.get("individual_monthly");
  const indAnnual = plansById.get("individual_annual");
  const orgSeat = plansById.get("organization_seat");
  if (!indMonthly || !indAnnual || !orgSeat) {
    throw new Error(
      "SF-SYS-032: config/business.json is missing required plan definitions (individual_monthly, individual_annual, organization_seat)"
    );
  }

  const microRaw = asRecord(root.micro_purchases, "micro_purchases");
  const cp25Raw = asRecord(microRaw.credit_pack_25, "micro_purchases.credit_pack_25");
  const ppfRaw = asRecord(microRaw.pay_per_file, "micro_purchases.pay_per_file");

  const minSeats = requirePositiveNumber(orgSeat.min_seats, "plans.organization_seat.min_seats");
  if (!Number.isInteger(minSeats) || minSeats < 3) {
    throw new Error("SF-SYS-032: organization_seat.min_seats must be an integer >= 3");
  }

  const freeAi = requireBoolean(freeRaw.ai, "free_plan.ai");
  const freeTeams = requireBoolean(freeRaw.teams, "free_plan.teams");
  const freeSavedRecipes = requireBoolean(freeRaw.saved_recipes, "free_plan.saved_recipes");
  const freePremiumPacks = requireBoolean(
    freeRaw.premium_recipe_packs,
    "free_plan.premium_recipe_packs"
  );
  if (freeAi || freeTeams || freeSavedRecipes || freePremiumPacks) {
    throw new Error(
      "SF-SYS-032: free_plan must keep ai, teams, saved_recipes, and premium_recipe_packs disabled"
    );
  }

  const cp25Ai = requireBoolean(cp25Raw.includes_ai, "micro_purchases.credit_pack_25.includes_ai");
  const cp25Packs = requireBoolean(
    cp25Raw.includes_premium_recipe_packs,
    "micro_purchases.credit_pack_25.includes_premium_recipe_packs"
  );
  const ppfAi = requireBoolean(ppfRaw.includes_ai, "micro_purchases.pay_per_file.includes_ai");
  const ppfPacks = requireBoolean(
    ppfRaw.includes_premium_recipe_packs,
    "micro_purchases.pay_per_file.includes_premium_recipe_packs"
  );
  if (cp25Ai || cp25Packs || ppfAi || ppfPacks) {
    throw new Error("SF-SYS-032: micro_purchases must not unlock AI or premium recipe packs");
  }

  return {
    freePlan: {
      operationsPerMonth: requirePositiveNumber(
        freeRaw.operations_per_month,
        "free_plan.operations_per_month"
      ),
      maxRows: requirePositiveNumber(freeRaw.max_rows, "free_plan.max_rows"),
      maxColumns: requirePositiveNumber(freeRaw.max_columns, "free_plan.max_columns"),
      watermark: requireNonEmptyString(freeRaw.watermark, "free_plan.watermark"),
      watermarkText: requireNonEmptyString(freeRaw.watermark_text, "free_plan.watermark_text"),
      savedRecipes: freeSavedRecipes,
      recipesAfterDowngrade: requireNonEmptyString(
        freeRaw.recipes_after_downgrade,
        "free_plan.recipes_after_downgrade"
      ),
      premiumRecipePacks: freePremiumPacks,
      ai: freeAi,
      teams: freeTeams
    },
    trial: {
      days: requirePositiveNumber(trialRaw.days, "trial.days"),
      requiresCard: requireBoolean(trialRaw.requires_card, "trial.requires_card"),
      requiresVerifiedEmail: requireBoolean(
        trialRaw.requires_verified_email,
        "trial.requires_verified_email"
      ),
      requiresVerifiedPhone: requireBoolean(
        trialRaw.requires_verified_phone,
        "trial.requires_verified_phone"
      ),
      aiCallsCap: requirePositiveNumber(trialRaw.ai_calls_cap, "trial.ai_calls_cap")
    },
    individualMonthly: {
      price: requireDualCurrency(indMonthly.price, "plans.individual_monthly.price"),
      aiCallsPerMonth: requirePositiveNumber(
        indMonthly.ai_calls_per_month,
        "plans.individual_monthly.ai_calls_per_month"
      ),
      includesPremiumRecipePacks: requireBoolean(
        indMonthly.includes_premium_recipe_packs,
        "plans.individual_monthly.includes_premium_recipe_packs"
      )
    },
    individualAnnual: {
      price: requireDualCurrency(indAnnual.price, "plans.individual_annual.price"),
      aiCallsPerMonth: requirePositiveNumber(
        indAnnual.ai_calls_per_month,
        "plans.individual_annual.ai_calls_per_month"
      ),
      includesPremiumRecipePacks: requireBoolean(
        indAnnual.includes_premium_recipe_packs,
        "plans.individual_annual.includes_premium_recipe_packs"
      )
    },
    organizationSeat: {
      minSeats,
      pricePerSeatPerMonth: requireDualCurrency(
        orgSeat.price_per_seat_per_month,
        "plans.organization_seat.price_per_seat_per_month"
      ),
      pricePerSeatPerYear: requireDualCurrency(
        orgSeat.price_per_seat_per_year,
        "plans.organization_seat.price_per_seat_per_year"
      ),
      aiCallsPerSeatPerMonth: requirePositiveNumber(
        orgSeat.ai_calls_per_seat_per_month,
        "plans.organization_seat.ai_calls_per_seat_per_month"
      ),
      includesPremiumRecipePacks: requireBoolean(
        orgSeat.includes_premium_recipe_packs,
        "plans.organization_seat.includes_premium_recipe_packs"
      )
    },
    creditPack25: {
      operations: requirePositiveNumber(
        cp25Raw.operations,
        "micro_purchases.credit_pack_25.operations"
      ),
      price: requireDualCurrency(cp25Raw.price, "micro_purchases.credit_pack_25.price"),
      expires: requireBoolean(cp25Raw.expires, "micro_purchases.credit_pack_25.expires"),
      removesWatermark: requireBoolean(
        cp25Raw.removes_watermark,
        "micro_purchases.credit_pack_25.removes_watermark"
      ),
      includesAi: cp25Ai,
      includesPremiumRecipePacks: cp25Packs
    },
    payPerFile: {
      maxRows: requirePositiveNumber(ppfRaw.max_rows, "micro_purchases.pay_per_file.max_rows"),
      price: requireDualCurrency(ppfRaw.price, "micro_purchases.pay_per_file.price"),
      removesWatermark: requireBoolean(
        ppfRaw.removes_watermark,
        "micro_purchases.pay_per_file.removes_watermark"
      ),
      includesAi: ppfAi,
      includesPremiumRecipePacks: ppfPacks
    }
  };
}

export const BUSINESS_PRICING: ValidatedBusinessPricing =
  parseBusinessPricingConfig(rawBusinessConfig);

export function buildAuthoritativePricingCatalog(
  cfg: ValidatedBusinessPricing = BUSINESS_PRICING
): readonly PricingCatalogEntry[] {
  return [
    {
      product: "individual_monthly",
      category: "subscription",
      billingPeriod: "monthly",
      titleEn: "Individual Monthly",
      titleAr: "الباقة الفردية — شهريًا",
      periodEn: "Monthly subscription",
      periodAr: "اشتراك شهري",
      subtitleEn: `All paid features · ${cfg.individualMonthly.aiCallsPerMonth} AI calls/mo · No watermark · Saved recipes · Premium recipe packs`,
      subtitleAr: `كافة المزايا المدفوعة · ${cfg.individualMonthly.aiCallsPerMonth} طلب ذكاء/شهر · بدون علامة مائية · وصفات محفوظة · حزم احترافية`,
      usdMajor: cfg.individualMonthly.price.USD,
      egpMajor: cfg.individualMonthly.price.EGP
    },
    {
      product: "individual_annual",
      category: "subscription",
      billingPeriod: "annual",
      titleEn: "Individual Annual",
      titleAr: "الباقة الفردية — سنويًا",
      periodEn: "Annual subscription",
      periodAr: "اشتراك سنوي",
      subtitleEn: `Save 30% · All paid features · ${cfg.individualAnnual.aiCallsPerMonth} AI calls/mo · No watermark · Premium recipe packs`,
      subtitleAr: `وفّر 30% · كافة المزايا المدفوعة · ${cfg.individualAnnual.aiCallsPerMonth} طلب ذكاء/شهر · بدون علامة مائية · حزم احترافية`,
      usdMajor: cfg.individualAnnual.price.USD,
      egpMajor: cfg.individualAnnual.price.EGP
    },
    {
      product: "organization_seat_annual",
      category: "subscription",
      billingPeriod: "annual",
      titleEn: "Organization (Teams)",
      titleAr: "باقة المؤسسات والفرق",
      periodEn: "Annual contract per seat",
      periodAr: "عقد سنوي لكل مقعد",
      subtitleEn: `Minimum ${cfg.organizationSeat.minSeats} seats · ${cfg.organizationSeat.aiCallsPerSeatPerMonth} AI calls/seat/mo · Shared workspace & RBAC`,
      subtitleAr: `الحد الأدنى ${cfg.organizationSeat.minSeats} مقاعد · ${cfg.organizationSeat.aiCallsPerSeatPerMonth} طلب ذكاء/مقعد/شهر · مساحة فريق وصلاحيات`,
      usdMajor: cfg.organizationSeat.pricePerSeatPerYear.USD,
      egpMajor: cfg.organizationSeat.pricePerSeatPerYear.EGP,
      perSeat: true,
      perSeatMonthlyUsd: cfg.organizationSeat.pricePerSeatPerMonth.USD,
      perSeatMonthlyEgp: cfg.organizationSeat.pricePerSeatPerMonth.EGP,
      minSeats: cfg.organizationSeat.minSeats
    },
    {
      product: "credit_pack_25",
      category: "micro_purchase",
      billingPeriod: "one_time",
      titleEn: "Credit Pack 25",
      titleAr: "حزمة 25 عملية (Credit Pack 25)",
      periodEn: "One-time purchase",
      periodAr: "شراء لمرة واحدة",
      subtitleEn: `${cfg.creditPack25.operations} additional data operations only · Never expires · Removes watermark · Does NOT unlock AI, teams, or premium packs`,
      subtitleAr: `${cfg.creditPack25.operations} عملية بيانات إضافية فقط · لا تنتهي صلاحيتها · بدون علامة مائية · لا تفتح الذكاء الاصطناعي أو الفرق أو الحزم الاحترافية`,
      usdMajor: cfg.creditPack25.price.USD,
      egpMajor: cfg.creditPack25.price.EGP
    },
    {
      product: "pay_per_file",
      category: "micro_purchase",
      billingPeriod: "one_time",
      titleEn: "Pay-Per-File",
      titleAr: "الدفع لكل ملف (Pay-per-file)",
      periodEn: "One-time purchase",
      periodAr: "شراء لمرة واحدة",
      subtitleEn: `1 purchased file up to ${cfg.payPerFile.maxRows.toLocaleString("en-US")} rows · Removes watermark · Does NOT unlock AI, teams, or premium packs`,
      subtitleAr: `ملف واحد حتى ${cfg.payPerFile.maxRows.toLocaleString("en-US")} صف · بدون علامة مائية · لا يفتح الذكاء الاصطناعي أو الفرق أو الحزم الاحترافية`,
      usdMajor: cfg.payPerFile.price.USD,
      egpMajor: cfg.payPerFile.price.EGP
    }
  ];
}

export const AUTHORITATIVE_CATALOG: readonly PricingCatalogEntry[] =
  buildAuthoritativePricingCatalog(BUSINESS_PRICING);

export function getCatalogEntry(product: CheckoutProduct): PricingCatalogEntry {
  const entry = AUTHORITATIVE_CATALOG.find((item) => item.product === product);
  if (!entry) {
    throw new Error(`SF-SYS-032: unknown checkout product: ${product}`);
  }
  return entry;
}

export function isValidCheckoutProduct(value: string): value is CheckoutProduct {
  return (
    value === "individual_monthly" ||
    value === "individual_annual" ||
    value === "organization_seat_annual" ||
    value === "credit_pack_25" ||
    value === "pay_per_file"
  );
}

export function normalizeOrganizationSeats(seats: number): number {
  const min = BUSINESS_PRICING.organizationSeat.minSeats;
  if (!Number.isFinite(seats)) return min;
  return Math.min(5000, Math.max(min, Math.floor(seats)));
}

export interface PendingWebPurchaseIntent {
  readonly product: CheckoutProduct;
  readonly billing_period: "monthly" | "annual" | "one_time";
  readonly billing_country_iso2: "US" | "EG";
  readonly seats: number;
  readonly saved_at_ms: number;
}

const PENDING_WEB_INTENT_KEY = "safi.web.pending_purchase_intent.v1";
const PENDING_WEB_INTENT_TTL_MS = 2 * 60 * 60 * 1000; // 2 hours (survives email verification!)

function getPersistentStorage(): Storage | null {
  if (typeof window === "undefined") return null;
  try {
    return window.localStorage;
  } catch {
    try {
      return window.sessionStorage;
    } catch {
      return null;
    }
  }
}

export function savePendingWebPurchaseIntent(input: {
  readonly product: CheckoutProduct;
  readonly billing_country_iso2: "US" | "EG";
  readonly seats?: number;
}): PendingWebPurchaseIntent {
  const entry = getCatalogEntry(input.product);
  const intent: PendingWebPurchaseIntent = {
    product: input.product,
    billing_period: entry.billingPeriod,
    billing_country_iso2: input.billing_country_iso2 === "EG" ? "EG" : "US",
    seats:
      input.product === "organization_seat_annual"
        ? normalizeOrganizationSeats(input.seats ?? BUSINESS_PRICING.organizationSeat.minSeats)
        : 1,
    saved_at_ms: Date.now()
  };
  const storage = getPersistentStorage();
  if (storage) {
    try {
      storage.setItem(PENDING_WEB_INTENT_KEY, JSON.stringify(intent));
    } catch {
      // Non-fatal if storage quota blocked.
    }
  }
  return intent;
}

export function loadPendingWebPurchaseIntent(nowMs = Date.now()): PendingWebPurchaseIntent | null {
  const storage = getPersistentStorage();
  if (!storage) return null;
  try {
    const raw = storage.getItem(PENDING_WEB_INTENT_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<PendingWebPurchaseIntent>;
    if (
      typeof parsed.product !== "string" ||
      !isValidCheckoutProduct(parsed.product) ||
      typeof parsed.saved_at_ms !== "number"
    ) {
      return null;
    }
    if (nowMs - parsed.saved_at_ms > PENDING_WEB_INTENT_TTL_MS) {
      storage.removeItem(PENDING_WEB_INTENT_KEY);
      return null;
    }
    const entry = getCatalogEntry(parsed.product);
    const country: "US" | "EG" = parsed.billing_country_iso2 === "EG" ? "EG" : "US";
    const seats =
      parsed.product === "organization_seat_annual"
        ? normalizeOrganizationSeats(
            typeof parsed.seats === "number"
              ? parsed.seats
              : BUSINESS_PRICING.organizationSeat.minSeats
          )
        : 1;
    return {
      product: parsed.product,
      billing_period: entry.billingPeriod,
      billing_country_iso2: country,
      seats,
      saved_at_ms: parsed.saved_at_ms
    };
  } catch {
    return null;
  }
}

export function clearPendingWebPurchaseIntent(): void {
  const storage = getPersistentStorage();
  if (!storage) return;
  try {
    storage.removeItem(PENDING_WEB_INTENT_KEY);
  } catch {
    // Ignore storage error.
  }
}

export function buildBillingUrlFromIntent(intent: {
  readonly product: CheckoutProduct;
  readonly billing_country_iso2: "US" | "EG";
  readonly seats?: number;
}): string {
  const params = new URLSearchParams();
  params.set("product", intent.product);
  params.set("country", intent.billing_country_iso2 === "EG" ? "EG" : "US");
  if (intent.product === "organization_seat_annual") {
    params.set(
      "seats",
      String(normalizeOrganizationSeats(intent.seats ?? BUSINESS_PRICING.organizationSeat.minSeats))
    );
  }
  return `/billing?${params.toString()}`;
}

export function parsePurchaseIntentFromSearch(
  search: string
): Omit<PendingWebPurchaseIntent, "saved_at_ms"> | null {
  const params = new URLSearchParams(search);
  const rawProduct = (params.get("product") ?? "").trim();
  if (!isValidCheckoutProduct(rawProduct)) {
    return null;
  }
  const entry = getCatalogEntry(rawProduct);
  const rawCountry = (params.get("country") ?? "US").trim().toUpperCase();
  const billing_country_iso2: "US" | "EG" = rawCountry === "EG" ? "EG" : "US";
  const rawSeats = Number(params.get("seats") ?? BUSINESS_PRICING.organizationSeat.minSeats);
  const seats =
    rawProduct === "organization_seat_annual" ? normalizeOrganizationSeats(rawSeats) : 1;
  return {
    product: rawProduct,
    billing_period: entry.billingPeriod,
    billing_country_iso2,
    seats
  };
}
