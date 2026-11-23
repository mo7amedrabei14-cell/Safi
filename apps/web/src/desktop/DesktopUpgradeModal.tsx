/**
 * `apps/web/src/desktop/DesktopUpgradeModal.tsx`
 *
 * Desktop Plan, Entitlements & External Website Upgrade Modal:
 * - Displays the server-authoritative current plan & usage fetched from `GET /v1/entitlements/me`.
 * - NEVER collects or renders credit-card inputs inside the Desktop UI.
 * - Clicking Upgrade / Subscribe / Buy for ANY of the 5 approved purchase types initiates a
 *   short-lived single-use Desktop-to-Website handoff (`POST /v1/billing/handoff/start`) and
 *   opens the official SafiData website `/billing` route in the user's system default browser.
 * - Refreshes entitlements automatically on explicit button click, window focus, or when Desktop
 *   receives a `safi://billing` deep-link return notification (NEVER trusts deep link as proof!).
 */
import type { ReactNode } from "react";
import { useCallback, useEffect, useState } from "react";
import { Crown, ExternalLink, RefreshCw, ShieldCheck, X } from "lucide-react";
import type { SafiLang } from "../components/layout/FloatingGlassDock";
import rawBusinessConfig from "../../../../config/business.json";
import { getDefaultAuthApiBaseUrl } from "../lib/auth-api";
import {
  BillingCheckoutController,
  type CheckoutProduct,
  type ServerEntitlementVerification
} from "../lib/billing-checkout";
import { launchDesktopWebsiteBilling } from "../lib/desktop-billing-handoff";

export interface DesktopUpgradeModalProps {
  readonly open: boolean;
  readonly lang: SafiLang;
  readonly sessionToken: string | null;
  readonly refreshNonce?: number;
  readonly onClose: () => void;
  readonly fetchImpl?: typeof fetch;
  readonly apiBaseUrl?: string;
  readonly openExternalUrl?: (url: string) => Promise<{ readonly opened: boolean }>;
}

const BUSINESS_CFG = rawBusinessConfig as {
  readonly plans: ReadonlyArray<{
    readonly id: string;
    readonly price?: { readonly USD: number; readonly EGP: number };
    readonly min_seats?: number;
    readonly price_per_seat_per_year?: { readonly USD: number; readonly EGP: number };
  }>;
  readonly micro_purchases: {
    readonly pay_per_file: { readonly price: { readonly USD: number; readonly EGP: number } };
    readonly credit_pack_25: { readonly price: { readonly USD: number; readonly EGP: number } };
  };
};

const ORG_MIN_SEATS = BUSINESS_CFG.plans.find((p) => p.id === "organization_seat")?.min_seats ?? 3;

export function DesktopUpgradeModal({
  open,
  lang,
  sessionToken,
  refreshNonce = 0,
  onClose,
  fetchImpl,
  apiBaseUrl,
  openExternalUrl
}: DesktopUpgradeModalProps): ReactNode {
  const isAr = lang === "ar";
  const L = useCallback((ar: string, en: string) => (isAr ? ar : en), [isAr]);

  const [countryIso2, setCountryIso2] = useState<"US" | "EG">("US");
  const [orgSeats, setOrgSeats] = useState<number>(ORG_MIN_SEATS);
  const [entitlements, setEntitlements] = useState<ServerEntitlementVerification | null>(null);
  const [loadingPlan, setLoadingPlan] = useState(false);
  const [launchingProduct, setLaunchingProduct] = useState<CheckoutProduct | null>(null);
  const [statusMessage, setStatusMessage] = useState<string | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const refreshEntitlements = useCallback(async (): Promise<void> => {
    if (!sessionToken) return;
    setLoadingPlan(true);
    setErrorMessage(null);
    try {
      const ctrl = new BillingCheckoutController({
        apiBaseUrl: apiBaseUrl ?? getDefaultAuthApiBaseUrl(),
        ...(fetchImpl !== undefined ? { fetchImpl } : {})
      });
      const ent = await ctrl.fetchEntitlements(sessionToken);
      setEntitlements(ent);
    } catch {
      setErrorMessage(
        L(
          "تعذّر تحديث حالة الاشتراك من الخادم الآن.",
          "Could not refresh entitlement status from server."
        )
      );
    } finally {
      setLoadingPlan(false);
    }
  }, [sessionToken, apiBaseUrl, fetchImpl, L]);

  useEffect(() => {
    if (!open || !sessionToken) return;
    void refreshEntitlements();
  }, [open, sessionToken, refreshNonce, refreshEntitlements]);

  useEffect(() => {
    if (!open || !sessionToken || typeof window === "undefined") return;
    const onFocus = (): void => {
      void refreshEntitlements();
    };
    window.addEventListener("focus", onFocus);
    return () => {
      window.removeEventListener("focus", onFocus);
    };
  }, [open, sessionToken, refreshEntitlements]);

  if (!open) return null;

  const indM = BUSINESS_CFG.plans.find((p) => p.id === "individual_monthly")?.price;
  const indA = BUSINESS_CFG.plans.find((p) => p.id === "individual_annual")?.price;
  const org = BUSINESS_CFG.plans.find((p) => p.id === "organization_seat")?.price_per_seat_per_year;
  const cp25 = BUSINESS_CFG.micro_purchases.credit_pack_25.price;
  const ppf = BUSINESS_CFG.micro_purchases.pay_per_file.price;

  const handleLaunch = async (product: CheckoutProduct): Promise<void> => {
    if (!sessionToken) {
      setErrorMessage(L("يلزم تسجيل الدخول أولًا.", "Please sign in first."));
      return;
    }
    setLaunchingProduct(product);
    setErrorMessage(null);
    setStatusMessage(null);
    try {
      await launchDesktopWebsiteBilling(
        sessionToken,
        {
          product,
          ...(product === "organization_seat_annual"
            ? { seats: Math.max(ORG_MIN_SEATS, orgSeats) }
            : {}),
          billing_country_iso2: countryIso2
        },
        {
          ...(apiBaseUrl !== undefined ? { apiBaseUrl } : {}),
          ...(fetchImpl !== undefined ? { fetchImpl } : {}),
          ...(openExternalUrl !== undefined ? { openExternalUrl } : {})
        }
      );
      setStatusMessage(
        L(
          "تم فتح بوابة الدفع الرسمية في متصفح النظام. بعد إتمام الدفع سيتحدث اشتراكك هنا فور تأكيد الخادم.",
          "Opened the official SafiData billing portal in your system browser. After payment, your plan will update here once confirmed by the server."
        )
      );
    } catch (err) {
      const code = err instanceof Error ? err.message : "SF-PAY-004";
      setErrorMessage(
        L(
          `تعذّر فتح بوابة الدفع الرسمية (${code}).`,
          `Could not launch official website billing portal (${code}).`
        )
      );
    } finally {
      setLaunchingProduct(null);
    }
  };

  const options: ReadonlyArray<{
    readonly product: CheckoutProduct;
    readonly title: string;
    readonly price: string;
    readonly cta: string;
  }> = [
    {
      product: "individual_monthly",
      title: L("فردي شهري", "Individual Monthly"),
      price: countryIso2 === "EG" ? `${indM?.EGP ?? 1000} EGP / شهر` : `$${indM?.USD ?? 20} / mo`,
      cta: L("اشترك عبر الموقع", "Subscribe on Website")
    },
    {
      product: "individual_annual",
      title: L("فردي سنوي (وفر 30%)", "Individual Annual (Save 30%)"),
      price: countryIso2 === "EG" ? `${indA?.EGP ?? 8400} EGP / سنة` : `$${indA?.USD ?? 168} / yr`,
      cta: L("اشترك عبر الموقع", "Subscribe on Website")
    },
    {
      product: "organization_seat_annual",
      title: L(
        `المؤسسات (${Math.max(ORG_MIN_SEATS, orgSeats)} مقاعد)`,
        `Organization (${Math.max(ORG_MIN_SEATS, orgSeats)} seats)`
      ),
      price:
        countryIso2 === "EG"
          ? `${((org?.EGP ?? 10200) * Math.max(ORG_MIN_SEATS, orgSeats)).toLocaleString("en-US")} EGP / سنة`
          : `$${((org?.USD ?? 204) * Math.max(ORG_MIN_SEATS, orgSeats)).toLocaleString("en-US")} / yr`,
      cta: L("اشترك للمؤسسة عبر الموقع", "Buy Team Plan on Website")
    },
    {
      product: "credit_pack_25",
      title: L("حزمة 25 عملية (لا تنتهي)", "Credit Pack 25 (Never Expires)"),
      price: countryIso2 === "EG" ? `${cp25.EGP} EGP` : `$${cp25.USD}`,
      cta: L("شراء عبر الموقع", "Buy on Website")
    },
    {
      product: "pay_per_file",
      title: L("الدفع لكل ملف (حتى 100 ألف صف)", "Pay-Per-File (Up to 100k rows)"),
      price: countryIso2 === "EG" ? `${ppf.EGP} EGP` : `$${ppf.USD}`,
      cta: L("شراء عبر الموقع", "Buy on Website")
    }
  ];

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label={L("الاشتراك والترقية", "Plan & Upgrade")}
      data-desktop-upgrade-modal="true"
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 9500,
        background: "rgba(0,0,0,0.6)",
        display: "grid",
        placeItems: "center",
        padding: "16px"
      }}
    >
      <div
        className="safi-panel"
        style={{
          width: "100%",
          maxWidth: "700px",
          maxHeight: "90vh",
          overflowY: "auto",
          padding: "24px",
          display: "grid",
          gap: "16px"
        }}
      >
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            gap: "12px"
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
            <Crown size={20} aria-hidden="true" />
            <h2 style={{ margin: 0, fontSize: "18px", fontWeight: 800 }}>
              {L(
                "الخطة والترقية — عبر موقع صافي الرسمي",
                "Plan & Upgrade — Official Website Checkout"
              )}
            </h2>
          </div>
          <button
            type="button"
            className="safi-btn safi-btn-ghost"
            onClick={onClose}
            aria-label={L("إغلاق", "Close")}
          >
            <X size={16} aria-hidden="true" />
          </button>
        </div>

        <p style={{ margin: 0, fontSize: "13px", opacity: 0.85 }}>
          <ShieldCheck
            size={15}
            aria-hidden="true"
            style={{ verticalAlign: "middle", marginInlineEnd: "6px" }}
          />
          {L(
            "تطبيق سطح المكتب لا يطلب بيانات بطاقتك أبدًا. الضغط على أي باقة يفتح متصفحك الرسمي على بوابة الدفع المؤمّنة الخاصة بحسابك.",
            "SafiData Desktop never collects card details. Selecting any plan opens your system browser at the secure official SafiData billing portal."
          )}
        </p>

        {/* Live Authoritative Plan Status */}
        <div
          data-desktop-plan-status={entitlements?.effective_plan ?? "loading"}
          style={{
            display: "flex",
            flexWrap: "wrap",
            alignItems: "center",
            justifyContent: "space-between",
            gap: "10px",
            padding: "12px",
            borderRadius: "10px",
            border: "1px solid var(--border-subtle, #3f3f46)"
          }}
        >
          <div style={{ fontSize: "13px" }}>
            {entitlements ? (
              <>
                {L("خطتك المؤكّدة من الخادم:", "Server-confirmed plan:")}{" "}
                <strong>{entitlements.effective_plan}</strong> ({entitlements.state}) ·{" "}
                {L("رصيد العمليات:", "Credits:")}{" "}
                <strong>{entitlements.operation_credits ?? 0}</strong> ·{" "}
                {L("ملفات Pay-Per-File:", "Pay-Per-File:")}{" "}
                <strong>{entitlements.pay_per_file_tokens ?? 0}</strong>
              </>
            ) : (
              L("جارٍ قراءة حالة الاشتراك من الخادم...", "Loading authoritative plan status...")
            )}
          </div>
          <button
            type="button"
            className="safi-btn safi-btn-ghost"
            onClick={() => void refreshEntitlements()}
            disabled={loadingPlan}
            data-desktop-refresh-entitlements="true"
          >
            <RefreshCw size={14} aria-hidden="true" />
            <span>{L("تحديث حالة الاشتراك", "Refresh Plan Status")}</span>
          </button>
        </div>

        {/* Region & Organization Seats */}
        <div style={{ display: "flex", flexWrap: "wrap", gap: "12px" }}>
          <label style={{ display: "grid", gap: "4px", fontSize: "12px", fontWeight: 600 }}>
            <span>{L("العملة / الدولة", "Currency / Country")}</span>
            <select
              className="safi-input"
              value={countryIso2}
              onChange={(e) => setCountryIso2(e.target.value === "EG" ? "EG" : "US")}
              data-desktop-upgrade-country="true"
            >
              <option value="US">USD (Paddle)</option>
              <option value="EG">EGP — مصر (Paymob)</option>
            </select>
          </label>
          <label style={{ display: "grid", gap: "4px", fontSize: "12px", fontWeight: 600 }}>
            <span>
              {L(`مقاعد المؤسسة (≥ ${ORG_MIN_SEATS})`, `Organization seats (≥ ${ORG_MIN_SEATS})`)}
            </span>
            <input
              type="number"
              min={ORG_MIN_SEATS}
              max={5000}
              className="safi-input"
              value={orgSeats}
              onChange={(e) => {
                const parsed = Number.parseInt(e.target.value, 10);
                setOrgSeats(
                  Number.isFinite(parsed) ? Math.max(ORG_MIN_SEATS, parsed) : ORG_MIN_SEATS
                );
              }}
              data-desktop-upgrade-seats="true"
            />
          </label>
        </div>

        {statusMessage ? (
          <div
            role="status"
            data-desktop-upgrade-status="true"
            style={{ fontSize: "13px", fontWeight: 600 }}
          >
            {statusMessage}
          </div>
        ) : null}
        {errorMessage ? (
          <div
            role="alert"
            data-desktop-upgrade-error="true"
            style={{ fontSize: "13px", color: "var(--safi-danger, #ef4444)" }}
          >
            {errorMessage}
          </div>
        ) : null}

        {/* All 5 Purchase Types launching external website /billing */}
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fit, minmax(210px, 1fr))",
            gap: "10px"
          }}
        >
          {options.map((opt) => (
            <div
              key={opt.product}
              style={{
                padding: "14px",
                borderRadius: "10px",
                border: "1px solid var(--border-subtle, #3f3f46)",
                display: "grid",
                gap: "8px"
              }}
            >
              <strong>{opt.title}</strong>
              <span style={{ fontSize: "15px", fontWeight: 800 }}>{opt.price}</span>
              <button
                type="button"
                className="safi-btn safi-btn-primary"
                disabled={launchingProduct !== null}
                onClick={() => void handleLaunch(opt.product)}
                data-desktop-buy-product={opt.product}
              >
                <ExternalLink size={14} aria-hidden="true" />
                <span>
                  {launchingProduct === opt.product ? L("جارٍ الفتح...", "Opening...") : opt.cta}
                </span>
              </button>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
