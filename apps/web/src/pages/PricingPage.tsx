/**
 * `apps/web/src/pages/PricingPage.tsx`
 *
 * Public SafiData Plans & Pricing Experience (`/pricing`):
 * - Accessible anonymously without signing in or opening the Desktop app.
 * - Reads 100% of prices, seat minimums, and Free-plan / micro-purchase limits from
 *   `config/business.json` via `apps/web/src/lib/business-pricing.ts` (zero hardcoded fallbacks).
 * - Clearly separates Recurring Subscriptions, One-Time Micro-Purchases, and the Free Plan.
 * - Supports Arabic RTL and English LTR, Dark & Light themes, and responsive Desktop/Tablet/Mobile.
 */
import type { ReactNode } from "react";
import { useCallback, useState } from "react";
import {
  ArrowLeft,
  Building2,
  Check,
  CreditCard,
  Download,
  Info,
  ShieldCheck,
  Sparkles,
  Users,
  X
} from "lucide-react";
import type { SafiLang } from "../components/layout/FloatingGlassDock";
import type { CheckoutProduct } from "../lib/billing-checkout";
import {
  AUTHORITATIVE_CATALOG,
  BUSINESS_PRICING,
  normalizeOrganizationSeats,
  savePendingWebPurchaseIntent
} from "../lib/business-pricing";

export interface SelectedPricingIntent {
  readonly product: CheckoutProduct;
  readonly billing_country_iso2: "US" | "EG";
  readonly seats: number;
}

export interface PricingPageProps {
  readonly lang?: SafiLang;
  readonly onSelectPurchase: (intent: SelectedPricingIntent) => void;
  readonly onBackToShowcase?: () => void;
  readonly onDownloadDesktop?: () => void;
}

function formatDualCurrency(
  usd: number,
  egp: number,
  primaryCurrency: "USD" | "EGP"
): { readonly primary: string; readonly secondary: string } {
  const usdText = `$${usd.toLocaleString("en-US")}`;
  const egpText = `${egp.toLocaleString("en-US")} EGP`;
  return primaryCurrency === "EGP"
    ? { primary: egpText, secondary: usdText }
    : { primary: usdText, secondary: egpText };
}

export function PricingPage({
  lang = "en",
  onSelectPurchase,
  onBackToShowcase,
  onDownloadDesktop
}: PricingPageProps): ReactNode {
  const isAr = lang === "ar";
  const L = useCallback((ar: string, en: string) => (isAr ? ar : en), [isAr]);

  const [countryIso2, setCountryIso2] = useState<"US" | "EG">("US");
  const [orgSeats, setOrgSeats] = useState<number>(BUSINESS_PRICING.organizationSeat.minSeats);
  const currency: "USD" | "EGP" = countryIso2 === "EG" ? "EGP" : "USD";

  const subscriptions = AUTHORITATIVE_CATALOG.filter((item) => item.category === "subscription");
  const microPurchases = AUTHORITATIVE_CATALOG.filter((item) => item.category === "micro_purchase");
  const free = BUSINESS_PRICING.freePlan;
  const orgMinSeats = BUSINESS_PRICING.organizationSeat.minSeats;
  const safeOrgSeats = normalizeOrganizationSeats(orgSeats);

  const handleChoose = (product: CheckoutProduct): void => {
    const seats = product === "organization_seat_annual" ? safeOrgSeats : 1;
    savePendingWebPurchaseIntent({
      product,
      billing_country_iso2: countryIso2,
      seats
    });
    onSelectPurchase({
      product,
      billing_country_iso2: countryIso2,
      seats
    });
  };

  return (
    <section
      data-pricing-page="true"
      dir={isAr ? "rtl" : "ltr"}
      style={{
        maxWidth: "1180px",
        margin: "12px auto 56px",
        padding: "clamp(16px, 2.5vw, 32px)",
        display: "grid",
        gap: "28px"
      }}
    >
      {/* Top Header & Currency Switcher */}
      <header
        className="safi-panel"
        style={{
          padding: "clamp(20px, 3vw, 34px)",
          display: "grid",
          gap: "18px",
          borderRadius: "20px"
        }}
      >
        <div
          style={{
            display: "flex",
            flexWrap: "wrap",
            alignItems: "center",
            justifyContent: "space-between",
            gap: "12px"
          }}
        >
          <span
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: "8px",
              padding: "6px 14px",
              borderRadius: "999px",
              fontSize: "12px",
              fontWeight: 700,
              background: "rgba(16, 185, 129, 0.14)",
              color: "var(--status-ok, #34d399)"
            }}
          >
            <ShieldCheck size={14} aria-hidden="true" />
            <span>
              {L(
                "تسعير شفاف · اشترك مباشرة عبر الموقع دون الحاجة لفتح تطبيق سطح المكتب",
                "Transparent pricing · Subscribe directly on the web — no Desktop app required"
              )}
            </span>
          </span>

          {onBackToShowcase ? (
            <button
              type="button"
              className="safi-btn safi-btn-ghost"
              onClick={onBackToShowcase}
              style={{ minHeight: "44px" }}
              data-pricing-back-home="true"
            >
              <ArrowLeft size={15} aria-hidden="true" />
              <span>{L("العودة للموقع الرئيسي", "Back to Website")}</span>
            </button>
          ) : null}
        </div>

        <div style={{ display: "grid", gap: "10px", maxWidth: "820px" }}>
          <h1
            style={{
              margin: 0,
              fontSize: "clamp(28px, 3.6vw, 44px)",
              fontWeight: 800,
              lineHeight: 1.15
            }}
          >
            {L("الأسعار والباقات المعتمدة لمنصة صافي البيانات", "Plans & Pricing for SafiData")}
          </h1>
          <p
            style={{
              margin: 0,
              fontSize: "15.5px",
              lineHeight: 1.6,
              color: "var(--text-secondary)"
            }}
          >
            {L(
              "قارن جميع الباقات وحزم الاستخدام بدون تسجيل دخول. تُحسب الأسعار الرسمية من ملف الإعداد المعتمد (USD عبر Paddle دوليًا، وEGP عبر Paymob داخل مصر)، ولا يتفعل الاشتراك إلا بعد تأكيد الخادم لعملية الدفع.",
              "Compare every subscription, one-time add-on, and the Free tier without an account. All prices come directly from SafiData's authoritative billing catalog (USD via Paddle internationally, EGP via Paymob in Egypt), and entitlements activate strictly after verified server payment confirmation."
            )}
          </p>
        </div>

        {/* Currency / Region Toggle */}
        <div
          role="group"
          aria-label={L("اختر عملة العرض ومنطقة الفوترة", "Choose billing currency & region")}
          style={{
            display: "flex",
            flexWrap: "wrap",
            alignItems: "center",
            gap: "10px"
          }}
        >
          <button
            type="button"
            className={
              countryIso2 === "US" ? "safi-btn safi-btn-primary" : "safi-btn safi-btn-ghost"
            }
            onClick={() => setCountryIso2("US")}
            aria-pressed={countryIso2 === "US"}
            data-pricing-currency="USD"
            style={{ minHeight: "44px" }}
          >
            <span>
              {L("دولي — الدولار الأمريكي (USD · Paddle)", "International — USD (Paddle)")}
            </span>
          </button>
          <button
            type="button"
            className={
              countryIso2 === "EG" ? "safi-btn safi-btn-primary" : "safi-btn safi-btn-ghost"
            }
            onClick={() => setCountryIso2("EG")}
            aria-pressed={countryIso2 === "EG"}
            data-pricing-currency="EGP"
            style={{ minHeight: "44px" }}
          >
            <span>{L("مصر — الجنيه المصري (EGP · Paymob)", "Egypt — EGP (Paymob)")}</span>
          </button>
        </div>
      </header>

      {/* SECTION 1: RECURRING SUBSCRIPTIONS */}
      <div style={{ display: "grid", gap: "16px" }} data-pricing-section="subscriptions">
        <div style={{ display: "grid", gap: "4px" }}>
          <h2 style={{ margin: 0, fontSize: "clamp(20px, 2.2vw, 26px)", fontWeight: 800 }}>
            {L(
              "1. الاشتراكات المتجددة (كامل المزايا المدفوعة)",
              "1. Recurring Subscriptions (Full Paid Access)"
            )}
          </h2>
          <p style={{ margin: 0, fontSize: "14px", color: "var(--text-secondary)" }}>
            {L(
              "تشمل معالجة بلا حدود، إزالة العلامة المائية، الوصفات المحفوظة، حزم الوصفات الاحترافية، و400 طلب ذكاء اصطناعي شهريًا.",
              "Includes unlimited data operations, watermark-free exports, saved recipes, premium recipe packs, and 400 AI calls per month."
            )}
          </p>
        </div>

        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit, minmax(280px, 1fr))",
            gap: "16px",
            alignItems: "stretch"
          }}
        >
          {subscriptions.map((plan) => {
            const isAnnual = plan.product === "individual_annual";
            const isOrg = plan.product === "organization_seat_annual";
            const monthlySeatDisplay =
              isOrg && plan.perSeatMonthlyUsd && plan.perSeatMonthlyEgp
                ? formatDualCurrency(plan.perSeatMonthlyUsd, plan.perSeatMonthlyEgp, currency)
                : null;
            const totalUsd = isOrg ? plan.usdMajor * safeOrgSeats : plan.usdMajor;
            const totalEgp = isOrg ? plan.egpMajor * safeOrgSeats : plan.egpMajor;
            const displayPrice = formatDualCurrency(totalUsd, totalEgp, currency);

            return (
              <article
                key={plan.product}
                className="safi-panel"
                data-pricing-card={plan.product}
                style={{
                  padding: "22px",
                  borderRadius: "18px",
                  display: "flex",
                  flexDirection: "column",
                  justifyContent: "space-between",
                  gap: "16px",
                  border: isAnnual
                    ? "2px solid var(--accent-primary, #10b981)"
                    : "1px solid var(--border-subtle, #3f3f46)"
                }}
              >
                <div style={{ display: "grid", gap: "12px" }}>
                  <div
                    style={{
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "space-between",
                      gap: "8px",
                      flexWrap: "wrap"
                    }}
                  >
                    <span
                      style={{
                        display: "inline-flex",
                        alignItems: "center",
                        gap: "6px",
                        fontSize: "12px",
                        fontWeight: 700,
                        opacity: 0.85
                      }}
                    >
                      {isOrg ? (
                        <Building2 size={14} aria-hidden="true" />
                      ) : (
                        <Sparkles size={14} aria-hidden="true" />
                      )}
                      <span>{isAr ? plan.periodAr : plan.periodEn}</span>
                    </span>
                    {isAnnual ? (
                      <span
                        style={{
                          padding: "3px 10px",
                          borderRadius: "999px",
                          fontSize: "11px",
                          fontWeight: 800,
                          background: "rgba(16, 185, 129, 0.16)",
                          color: "var(--status-ok, #34d399)"
                        }}
                      >
                        {L("وفّر 30%", "Save 30%")}
                      </span>
                    ) : null}
                  </div>

                  <h3 style={{ margin: 0, fontSize: "20px", fontWeight: 800 }}>
                    {isAr ? plan.titleAr : plan.titleEn}
                  </h3>

                  {/* Price Block */}
                  {isOrg && monthlySeatDisplay ? (
                    <div style={{ display: "grid", gap: "4px" }}>
                      <div style={{ fontSize: "28px", fontWeight: 800 }}>
                        <span data-pricing-amount={plan.product}>{monthlySeatDisplay.primary}</span>
                        <span style={{ fontSize: "13px", fontWeight: 600, opacity: 0.8 }}>
                          {" "}
                          {L("/ مقعد / شهر", "/ seat / month")}
                        </span>
                      </div>
                      <div style={{ fontSize: "12.5px", color: "var(--text-secondary)" }}>
                        {L("السعر بالعملة الأخرى:", "Other currency:")}{" "}
                        {monthlySeatDisplay.secondary} {L("/ مقعد / شهر", "/ seat / month")} ·{" "}
                        {L("عقد سنوي", "Annual contract")}
                      </div>
                      <div
                        data-pricing-org-total="true"
                        style={{
                          marginTop: "4px",
                          fontSize: "13px",
                          fontWeight: 700,
                          color: "var(--text-primary)"
                        }}
                      >
                        {L(
                          `إجمالي العقد السنوي لـ ${safeOrgSeats} مقاعد: ${displayPrice.primary} (${displayPrice.secondary})`,
                          `Annual total for ${safeOrgSeats} seats: ${displayPrice.primary} (${displayPrice.secondary})`
                        )}
                      </div>
                    </div>
                  ) : (
                    <div style={{ display: "grid", gap: "4px" }}>
                      <div style={{ fontSize: "30px", fontWeight: 800 }}>
                        <span data-pricing-amount={plan.product}>{displayPrice.primary}</span>
                        <span style={{ fontSize: "13px", fontWeight: 600, opacity: 0.8 }}>
                          {" "}
                          {plan.billingPeriod === "monthly"
                            ? L("/ شهريًا", "/ month")
                            : L("/ سنويًا", "/ year")}
                        </span>
                      </div>
                      <div style={{ fontSize: "12.5px", color: "var(--text-secondary)" }}>
                        {L("أو", "Or")} {displayPrice.secondary}{" "}
                        {plan.billingPeriod === "monthly"
                          ? L("/ شهريًا", "/ month")
                          : L("/ سنويًا", "/ year")}
                      </div>
                    </div>
                  )}

                  {/* Organization Seat Selector (strict min >= 3) */}
                  {isOrg ? (
                    <label
                      style={{
                        display: "grid",
                        gap: "6px",
                        fontSize: "12.5px",
                        fontWeight: 700,
                        padding: "10px 12px",
                        borderRadius: "10px",
                        background: "var(--surface-elevated, rgba(255,255,255,0.04))",
                        border: "1px solid var(--border-subtle, #3f3f46)"
                      }}
                    >
                      <span style={{ display: "inline-flex", alignItems: "center", gap: "6px" }}>
                        <Users size={14} aria-hidden="true" />
                        <span>
                          {L(
                            `عدد المقاعد (الحد الأدنى ${orgMinSeats} مقاعد)`,
                            `Seat count (minimum ${orgMinSeats} seats)`
                          )}
                        </span>
                      </span>
                      <input
                        type="number"
                        min={orgMinSeats}
                        max={5000}
                        className="safi-input"
                        value={orgSeats}
                        onChange={(e) => {
                          const raw = Number(e.target.value);
                          setOrgSeats(Number.isFinite(raw) ? raw : orgMinSeats);
                        }}
                        onBlur={() => setOrgSeats(normalizeOrganizationSeats(orgSeats))}
                        data-pricing-org-seats="true"
                      />
                    </label>
                  ) : null}

                  <ul
                    style={{
                      listStyle: "none",
                      padding: 0,
                      margin: 0,
                      display: "grid",
                      gap: "8px",
                      fontSize: "13.5px"
                    }}
                  >
                    <li style={{ display: "flex", alignItems: "flex-start", gap: "8px" }}>
                      <Check
                        size={15}
                        style={{ flexShrink: 0, marginTop: "3px" }}
                        aria-hidden="true"
                      />
                      <span>
                        {L(
                          "عمليات بيانات وصفوف وأعمدة غير محدودة",
                          "Unlimited operations, rows, and columns"
                        )}
                      </span>
                    </li>
                    <li style={{ display: "flex", alignItems: "flex-start", gap: "8px" }}>
                      <Check
                        size={15}
                        style={{ flexShrink: 0, marginTop: "3px" }}
                        aria-hidden="true"
                      />
                      <span>
                        {L("بدون علامة مائية على جميع المخرجات", "No Free watermark on any export")}
                      </span>
                    </li>
                    <li style={{ display: "flex", alignItems: "flex-start", gap: "8px" }}>
                      <Check
                        size={15}
                        style={{ flexShrink: 0, marginTop: "3px" }}
                        aria-hidden="true"
                      />
                      <span>
                        {isOrg
                          ? L(
                              `${(BUSINESS_PRICING.organizationSeat.aiCallsPerSeatPerMonth * safeOrgSeats).toLocaleString("en-US")} طلب ذكاء اصطناعي/شهر (${BUSINESS_PRICING.organizationSeat.aiCallsPerSeatPerMonth} لكل مقعد)`,
                              `${(BUSINESS_PRICING.organizationSeat.aiCallsPerSeatPerMonth * safeOrgSeats).toLocaleString("en-US")} AI calls/mo (${BUSINESS_PRICING.organizationSeat.aiCallsPerSeatPerMonth} per seat)`
                            )
                          : L(
                              `${BUSINESS_PRICING.individualMonthly.aiCallsPerMonth} طلب ذكاء اصطناعي شهريًا`,
                              `${BUSINESS_PRICING.individualMonthly.aiCallsPerMonth} AI calls per month`
                            )}
                      </span>
                    </li>
                    <li style={{ display: "flex", alignItems: "flex-start", gap: "8px" }}>
                      <Check
                        size={15}
                        style={{ flexShrink: 0, marginTop: "3px" }}
                        aria-hidden="true"
                      />
                      <span>
                        {L(
                          "حفظ الوصفات + حزم الوصفات الاحترافية طوال الاشتراك",
                          "Saved recipes + premium recipe packs while subscribed"
                        )}
                      </span>
                    </li>
                    {isOrg ? (
                      <li style={{ display: "flex", alignItems: "flex-start", gap: "8px" }}>
                        <Check
                          size={15}
                          style={{ flexShrink: 0, marginTop: "3px" }}
                          aria-hidden="true"
                        />
                        <span>
                          {L(
                            "مساحة عمل مشتركة للفرق + صلاحيات RBAC + مزامنة مشفّرة",
                            "Shared team workspace + RBAC + encrypted sync"
                          )}
                        </span>
                      </li>
                    ) : null}
                  </ul>
                </div>

                <button
                  type="button"
                  className="safi-btn safi-btn-primary"
                  onClick={() => handleChoose(plan.product)}
                  data-pricing-choose={plan.product}
                  style={{ minHeight: "46px", width: "100%", justifyContent: "center" }}
                >
                  <CreditCard size={15} aria-hidden="true" />
                  <span>
                    {isOrg
                      ? L(
                          `اشترك الآن (${safeOrgSeats} مقاعد)`,
                          `Subscribe Now (${safeOrgSeats} seats)`
                        )
                      : L("اشترك الآن", "Subscribe Now")}
                  </span>
                </button>
              </article>
            );
          })}
        </div>
      </div>

      {/* SECTION 2: ONE-TIME MICRO-PURCHASES (CLEARLY SEPARATED FROM FULL SUBSCRIPTIONS) */}
      <div style={{ display: "grid", gap: "16px" }} data-pricing-section="micro-purchases">
        <div style={{ display: "grid", gap: "6px" }}>
          <h2 style={{ margin: 0, fontSize: "clamp(20px, 2.2vw, 26px)", fontWeight: 800 }}>
            {L(
              "2. مشتريات لمرة واحدة (إضافات محدودة النطاق — ليست اشتراكًا كاملًا)",
              "2. One-Time Micro-Purchases (Narrowly Scoped Add-Ons — Not Full Subscriptions)"
            )}
          </h2>
          <p
            style={{
              margin: 0,
              fontSize: "14px",
              color: "var(--text-secondary)",
              display: "flex",
              alignItems: "flex-start",
              gap: "8px"
            }}
          >
            <Info size={16} style={{ flexShrink: 0, marginTop: "2px" }} aria-hidden="true" />
            <span>
              {L(
                "مهم: حزمة 25 عملية والدفع لكل ملف هما شراءان لمرة واحدة لتوسيع حد العمليات أو الصفوف وإزالة العلامة المائية فقط، ولا يفتحان الذكاء الاصطناعي أو الفرق أو حزم الوصفات الاحترافية أو كتابة الوصفات المحفوظة.",
                "Important: Credit Pack 25 and Pay-per-file are one-time add-ons only. Neither micro-purchase unlocks AI, teams, premium recipe packs, or saved recipe writing."
              )}
            </span>
          </p>
        </div>

        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit, minmax(300px, 1fr))",
            gap: "16px"
          }}
        >
          {microPurchases.map((addon) => {
            const displayPrice = formatDualCurrency(addon.usdMajor, addon.egpMajor, currency);
            const isCreditPack = addon.product === "credit_pack_25";

            return (
              <article
                key={addon.product}
                className="safi-panel"
                data-pricing-card={addon.product}
                style={{
                  padding: "22px",
                  borderRadius: "18px",
                  display: "flex",
                  flexDirection: "column",
                  justifyContent: "space-between",
                  gap: "16px",
                  border: "1px solid var(--border-subtle, #3f3f46)"
                }}
              >
                <div style={{ display: "grid", gap: "10px" }}>
                  <span
                    style={{
                      display: "inline-flex",
                      alignItems: "center",
                      width: "fit-content",
                      padding: "4px 10px",
                      borderRadius: "999px",
                      fontSize: "11.5px",
                      fontWeight: 700,
                      background: "rgba(59, 130, 246, 0.14)",
                      color: "var(--accent-secondary, #60a5fa)"
                    }}
                  >
                    {L(
                      "دفعة واحدة · بدون تجديد تلقائي",
                      "One-time purchase · No recurring billing"
                    )}
                  </span>

                  <h3 style={{ margin: 0, fontSize: "20px", fontWeight: 800 }}>
                    {isAr ? addon.titleAr : addon.titleEn}
                  </h3>

                  <div style={{ display: "grid", gap: "2px" }}>
                    <div style={{ fontSize: "28px", fontWeight: 800 }}>
                      <span data-pricing-amount={addon.product}>{displayPrice.primary}</span>
                      <span style={{ fontSize: "13px", fontWeight: 600, opacity: 0.8 }}>
                        {" "}
                        {L("مرة واحدة", "one-time")}
                      </span>
                    </div>
                    <div style={{ fontSize: "12.5px", color: "var(--text-secondary)" }}>
                      {L("أو", "Or")} {displayPrice.secondary} {L("مرة واحدة", "one-time")}
                    </div>
                  </div>

                  <ul
                    style={{
                      listStyle: "none",
                      padding: 0,
                      margin: 0,
                      display: "grid",
                      gap: "8px",
                      fontSize: "13.5px"
                    }}
                  >
                    <li style={{ display: "flex", alignItems: "flex-start", gap: "8px" }}>
                      <Check
                        size={15}
                        style={{ flexShrink: 0, marginTop: "3px" }}
                        aria-hidden="true"
                      />
                      <span>
                        {isCreditPack
                          ? L(
                              `يمنحك ${BUSINESS_PRICING.creditPack25.operations} عملية بيانات إضافية فقط (لا تنتهي صلاحيتها) ويزيل العلامة المائية`,
                              `Provides ${BUSINESS_PRICING.creditPack25.operations} additional data operations only (never expires) and removes the Free watermark`
                            )
                          : L(
                              `يسمح بمعالجة ملف واحد حتى ${BUSINESS_PRICING.payPerFile.maxRows.toLocaleString("en-US")} صف ويزيل العلامة المائية`,
                              `Allows up to ${BUSINESS_PRICING.payPerFile.maxRows.toLocaleString("en-US")} rows per purchased file and removes the Free watermark`
                            )}
                      </span>
                    </li>
                    <li
                      style={{
                        display: "flex",
                        alignItems: "flex-start",
                        gap: "8px",
                        opacity: 0.85
                      }}
                    >
                      <X
                        size={15}
                        style={{
                          flexShrink: 0,
                          marginTop: "3px",
                          color: "var(--safi-danger, #ef4444)"
                        }}
                        aria-hidden="true"
                      />
                      <span>
                        {L(
                          "لا يفتح الذكاء الاصطناعي (No AI)، ولا الفرق (No Teams)، ولا حزم الوصفات الاحترافية",
                          "Does NOT unlock AI, teams, or premium recipe packs"
                        )}
                      </span>
                    </li>
                  </ul>
                </div>

                <button
                  type="button"
                  className="safi-btn safi-btn-ghost"
                  onClick={() => handleChoose(addon.product)}
                  data-pricing-choose={addon.product}
                  style={{
                    minHeight: "46px",
                    width: "100%",
                    justifyContent: "center",
                    border: "1px solid var(--border-strong, #52525b)"
                  }}
                >
                  <CreditCard size={15} aria-hidden="true" />
                  <span>{L("شراء هذه الحزمة الآن", "Buy One-Time Add-On")}</span>
                </button>
              </article>
            );
          })}
        </div>
      </div>

      {/* SECTION 3: FREE PLAN (SEPARATE HONEST DISCLOSURE) */}
      <article
        className="safi-panel"
        data-pricing-card="free"
        style={{
          padding: "clamp(20px, 2.6vw, 28px)",
          borderRadius: "18px",
          display: "grid",
          gap: "16px",
          border: "1px solid var(--border-subtle, #3f3f46)"
        }}
      >
        <div
          style={{
            display: "flex",
            flexWrap: "wrap",
            alignItems: "center",
            justifyContent: "space-between",
            gap: "12px"
          }}
        >
          <div style={{ display: "grid", gap: "4px" }}>
            <h2 style={{ margin: 0, fontSize: "22px", fontWeight: 800 }}>
              {L(
                "3. الباقة المجانية (Free Plan) — حدودها الفعلية بصراحة",
                "3. Free Plan — Actual Limits Disclosed Honestly"
              )}
            </h2>
            <p style={{ margin: 0, fontSize: "13.5px", color: "var(--text-secondary)" }}>
              {L(
                "$0 / 0 EGP شهريًا — مناسبة للتجربة الأولية والملفات الصغيرة فقط مع قيود الخادم التالية:",
                "$0 / 0 EGP per month — designed for small starter files with these exact backend-enforced limits:"
              )}
            </p>
          </div>

          {onDownloadDesktop ? (
            <button
              type="button"
              className="safi-btn safi-btn-ghost"
              onClick={onDownloadDesktop}
              data-pricing-free-download="true"
              style={{ minHeight: "44px", border: "1px solid var(--border-strong, #52525b)" }}
            >
              <Download size={15} aria-hidden="true" />
              <span>{L("حمّل تطبيق سطح المكتب مجانًا", "Download Desktop App Free")}</span>
            </button>
          ) : null}
        </div>

        <ul
          data-pricing-free-limits="true"
          style={{
            listStyle: "none",
            padding: 0,
            margin: 0,
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit, minmax(240px, 1fr))",
            gap: "10px",
            fontSize: "13.5px"
          }}
        >
          <li style={{ display: "flex", alignItems: "flex-start", gap: "8px" }}>
            <Info size={15} style={{ flexShrink: 0, marginTop: "3px" }} aria-hidden="true" />
            <span>
              {L(
                `${free.operationsPerMonth} عمليات فقط شهريًا`,
                `${free.operationsPerMonth} operations per month`
              )}
            </span>
          </li>
          <li style={{ display: "flex", alignItems: "flex-start", gap: "8px" }}>
            <Info size={15} style={{ flexShrink: 0, marginTop: "3px" }} aria-hidden="true" />
            <span>
              {L(
                `حد أقصى ${free.maxRows.toLocaleString("en-US")} صف لكل ملف`,
                `Maximum ${free.maxRows.toLocaleString("en-US")} rows per file`
              )}
            </span>
          </li>
          <li style={{ display: "flex", alignItems: "flex-start", gap: "8px" }}>
            <Info size={15} style={{ flexShrink: 0, marginTop: "3px" }} aria-hidden="true" />
            <span>
              {L(
                `حد أقصى ${free.maxColumns} عمودًا لكل ملف`,
                `Maximum ${free.maxColumns} columns per file`
              )}
            </span>
          </li>
          <li style={{ display: "flex", alignItems: "flex-start", gap: "8px" }}>
            <Info size={15} style={{ flexShrink: 0, marginTop: "3px" }} aria-hidden="true" />
            <span>
              {L(
                "علامة مائية إلزامية للباقة المجانية على جميع المخرجات",
                "Mandatory Free watermark on all exports"
              )}
            </span>
          </li>
          <li style={{ display: "flex", alignItems: "flex-start", gap: "8px" }}>
            <X
              size={15}
              style={{ flexShrink: 0, marginTop: "3px", color: "var(--safi-danger, #ef4444)" }}
              aria-hidden="true"
            />
            <span>{L("بدون ذكاء اصطناعي (No AI)", "No AI")}</span>
          </li>
          <li style={{ display: "flex", alignItems: "flex-start", gap: "8px" }}>
            <X
              size={15}
              style={{ flexShrink: 0, marginTop: "3px", color: "var(--safi-danger, #ef4444)" }}
              aria-hidden="true"
            />
            <span>{L("بدون فرق أو مؤسسات (No Teams)", "No teams")}</span>
          </li>
          <li style={{ display: "flex", alignItems: "flex-start", gap: "8px" }}>
            <X
              size={15}
              style={{ flexShrink: 0, marginTop: "3px", color: "var(--safi-danger, #ef4444)" }}
              aria-hidden="true"
            />
            <span>
              {L(
                "بدون حزم الوصفات الاحترافية (No Premium Recipe Packs)",
                "No premium recipe packs"
              )}
            </span>
          </li>
          <li style={{ display: "flex", alignItems: "flex-start", gap: "8px" }}>
            <X
              size={15}
              style={{ flexShrink: 0, marginTop: "3px", color: "var(--safi-danger, #ef4444)" }}
              aria-hidden="true"
            />
            <span>
              {L(
                "بدون حفظ أو كتابة وصفات جديدة (الوصفات السابقة للقراءة فقط)",
                "No saved recipe writing (existing recipes are read-only)"
              )}
            </span>
          </li>
        </ul>
      </article>
    </section>
  );
}
