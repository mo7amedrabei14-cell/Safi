/**
 * `apps/web/src/pages/BillingPortalPage.tsx`
 *
 * Official SafiData Website Billing Portal (`/billing`):
 * - Dedicated website billing & upgrade destination for BOTH direct website visitors and
 *   Desktop-to-Website handoffs (`/billing?handoff=sfbh_...&state=...`).
 * - Requires authentication (`POST /v1/billing/web/login` issuing a scoped `sfb_*` session that
 *   NEVER revokes the user's active Desktop installation).
 * - Immediately strips `handoff` & `state` from the URL bar via `history.replaceState`.
 * - Supports ALL 5 approved purchase types sourced directly from `config/business.json`:
 *   1. Individual Monthly ($20 / 1,000 EGP)
 *   2. Individual Annual ($168 / 8,400 EGP)
 *   3. Organization Annual ($17 / 850 EGP per seat/mo, min 3 seats)
 *   4. Credit Pack 25 ($5 / 250 EGP)
 *   5. Pay-per-file ($2 / 50 EGP)
 * - Integrates `BillingCheckoutController` directly for double-click locking, refresh/back-forward
 *   idempotency key stability, reconciliation blocking, hosted checkout redirect, and authoritative
 *   server-only payment confirmation (NEVER trusts `?checkout=success` or client-side URL params).
 * - NEVER exposes Desktop Studio or local customer data on the website.
 */
import type { FormEvent, ReactNode } from "react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  AlertTriangle,
  ArrowLeft,
  CheckCircle2,
  CreditCard,
  Download,
  ExternalLink,
  Lock,
  RefreshCw,
  ShieldCheck
} from "lucide-react";
import type { SafiLang } from "../components/layout/FloatingGlassDock";
import { getDefaultAuthApiBaseUrl, postSafiAuth, SafiAuthApiError } from "../lib/auth-api";
import {
  BillingCheckoutController,
  CheckoutLifecycleError,
  type CheckoutApiResponse,
  type CheckoutIntent,
  type CheckoutProduct,
  type CouponPreviewApiResponse,
  type CustomerBillingData,
  type PaymentOrderState,
  type ServerEntitlementVerification
} from "../lib/billing-checkout";
import {
  AUTHORITATIVE_CATALOG,
  BUSINESS_PRICING,
  clearPendingWebPurchaseIntent,
  getCatalogEntry,
  loadPendingWebPurchaseIntent,
  normalizeOrganizationSeats,
  parsePurchaseIntentFromSearch,
  savePendingWebPurchaseIntent,
  type PricingCatalogEntry
} from "../lib/business-pricing";

export interface BillingPortalPageProps {
  readonly lang?: SafiLang;
  readonly onBackToShowcase?: () => void;
  readonly onOpenPricing?: () => void;
  readonly onDownloadDesktop?: () => void;
  readonly initialVerificationToken?: string | null;
  readonly controller?: BillingCheckoutController;
  readonly initialSessionToken?: string | null;
  readonly initialEmail?: string | null;
  readonly onOpenProviderCheckout?: (url: string) => void;
}

interface PendingHandoffParams {
  readonly handoffCode: string;
  readonly state: string;
}

const ORG_MIN_SEATS = BUSINESS_PRICING.organizationSeat.minSeats;
const CATALOG = AUTHORITATIVE_CATALOG;
const HANDOFF_SESSION_KEY = "safi.billing.pending_handoff.v1";

function readInitialUrlState(): {
  readonly handoff: PendingHandoffParams | null;
  readonly sawRedirectReturn: boolean;
  readonly directIntent: {
    readonly product: CheckoutProduct;
    readonly billing_country_iso2: "US" | "EG";
    readonly seats: number;
  } | null;
} {
  if (typeof window === "undefined") {
    return { handoff: null, sawRedirectReturn: false, directIntent: null };
  }
  const url = new URL(window.location.href);
  const handoffCode = (url.searchParams.get("handoff") ?? "").trim();
  const state = (url.searchParams.get("state") ?? "").trim();
  const sawRedirectReturn =
    url.searchParams.has("checkout") ||
    url.searchParams.has("status") ||
    url.searchParams.has("success");

  const fromQuery = parsePurchaseIntentFromSearch(url.search);
  const directIntent = fromQuery
    ? (() => {
        savePendingWebPurchaseIntent(fromQuery);
        return fromQuery;
      })()
    : loadPendingWebPurchaseIntent();

  if (handoffCode.startsWith("sfbh_") && state.length >= 16) {
    try {
      window.sessionStorage.setItem(
        HANDOFF_SESSION_KEY,
        JSON.stringify({ handoffCode, state } satisfies PendingHandoffParams)
      );
    } catch {
      // Memory fallback handled by React state.
    }
    url.searchParams.delete("handoff");
    url.searchParams.delete("state");
    window.history.replaceState(window.history.state, "", `${url.pathname}${url.search}`);
    return { handoff: { handoffCode, state }, sawRedirectReturn, directIntent };
  }

  try {
    const saved = window.sessionStorage.getItem(HANDOFF_SESSION_KEY);
    if (saved) {
      const parsed = JSON.parse(saved) as PendingHandoffParams;
      if (parsed?.handoffCode?.startsWith("sfbh_") && parsed?.state?.length >= 16) {
        return { handoff: parsed, sawRedirectReturn, directIntent };
      }
    }
  } catch {
    // Ignore storage parse error.
  }

  return { handoff: null, sawRedirectReturn, directIntent };
}

function formatMinorPrice(minor: number, currency: "USD" | "EGP"): string {
  const major = minor / 100;
  return currency === "EGP"
    ? `${major.toLocaleString("en-US")} EGP`
    : `$${major.toLocaleString("en-US")}`;
}

export function BillingPortalPage({
  lang = "en",
  onBackToShowcase,
  onOpenPricing,
  onDownloadDesktop,
  initialVerificationToken = null,
  controller: injectedController,
  initialSessionToken = null,
  initialEmail = null,
  onOpenProviderCheckout
}: BillingPortalPageProps): ReactNode {
  const isAr = lang === "ar";
  const L = useCallback((ar: string, en: string) => (isAr ? ar : en), [isAr]);

  const controllerRef = useRef<BillingCheckoutController | null>(null);
  if (controllerRef.current === null) {
    controllerRef.current =
      injectedController ??
      new BillingCheckoutController({
        apiBaseUrl: getDefaultAuthApiBaseUrl()
      });
  }
  const controller = controllerRef.current;

  const initialUrl = useMemo(readInitialUrlState, []);
  const [pendingHandoff, setPendingHandoff] = useState<PendingHandoffParams | null>(
    initialUrl.handoff
  );
  const [sawRedirectReturn] = useState<boolean>(initialUrl.sawRedirectReturn);

  // Website Billing Portal Authentication state (`sfb_*` session; never revokes Desktop!)
  const [authToken, setAuthToken] = useState<string | null>(initialSessionToken);
  const [authEmail, setAuthEmail] = useState<string>(initialEmail ?? "");
  const [authMode, setAuthMode] = useState<"signin" | "signup">("signin");
  const [loginName, setLoginName] = useState("");
  const [loginEmail, setLoginEmail] = useState(initialEmail ?? "");
  const [loginPassword, setLoginPassword] = useState("");
  const [authBusy, setAuthBusy] = useState(false);
  const [authMessage, setAuthMessage] = useState<string | null>(null);
  const [authError, setAuthError] = useState<string | null>(null);

  // Selected purchase intent (hydrated from URL query or persistent localStorage so it survives
  // signup + email verification links opened in a new tab!)
  const [product, setProduct] = useState<CheckoutProduct>(
    initialUrl.directIntent?.product ?? "individual_monthly"
  );
  const [seats, setSeats] = useState<number>(initialUrl.directIntent?.seats ?? ORG_MIN_SEATS);
  const [countryIso2, setCountryIso2] = useState<"US" | "EG">(
    initialUrl.directIntent?.billing_country_iso2 ?? "US"
  );
  const [couponCode, setCouponCode] = useState<string>("");
  const [couponPreview, setCouponPreview] = useState<CouponPreviewApiResponse | null>(null);
  const [couponError, setCouponError] = useState<string | null>(null);
  const [fromDesktopHandoff, setFromDesktopHandoff] = useState<boolean>(
    initialUrl.handoff !== null
  );
  const [verificationNotice, setVerificationNotice] = useState<string | null>(null);
  const [verificationError, setVerificationError] = useState<string | null>(null);
  const verificationAttemptedRef = useRef<string | null>(null);

  // Real Paymob EGP customer billing details (zero fake personal data!)
  const [firstName, setFirstName] = useState("");
  const [lastName, setLastName] = useState("");
  const [billingPhone, setBillingPhone] = useState("");

  // Handoff & Checkout lifecycle state
  const [handoffNotice, setHandoffNotice] = useState<string | null>(null);
  const [handoffError, setHandoffError] = useState<string | null>(null);
  const [submittingCheckout, setSubmittingCheckout] = useState(false);
  const [activeOrder, setActiveOrder] = useState<CheckoutApiResponse | null>(null);
  const [orderState, setOrderState] = useState<PaymentOrderState | null>(null);
  const [reconciliationRequired, setReconciliationRequired] = useState(false);
  const [checkoutError, setCheckoutError] = useState<string | null>(null);
  const [entitlements, setEntitlements] = useState<ServerEntitlementVerification | null>(null);
  const [verifyingServer, setVerifyingServer] = useState(false);

  const selectedPlan: PricingCatalogEntry = useMemo(() => getCatalogEntry(product), [product]);

  const effectiveSeats =
    product === "organization_seat_annual" ? normalizeOrganizationSeats(seats) : 1;

  // Persist direct website purchase intent across page reloads, signup, and email verification.
  useEffect(() => {
    if (orderState === "paid") {
      clearPendingWebPurchaseIntent();
      return;
    }
    if (!fromDesktopHandoff) {
      savePendingWebPurchaseIntent({
        product,
        billing_country_iso2: countryIso2,
        seats: effectiveSeats
      });
    }
  }, [product, countryIso2, effectiveSeats, orderState, fromDesktopHandoff]);

  // Automatically process email verification tokens arriving inside the direct purchase journey
  // while keeping the customer's selected plan, billing period, country, and seats intact!
  useEffect(() => {
    if (
      !initialVerificationToken ||
      verificationAttemptedRef.current === initialVerificationToken
    ) {
      return;
    }
    verificationAttemptedRef.current = initialVerificationToken;
    setVerificationError(null);
    setVerificationNotice(L("جارٍ تأكيد بريدك الإلكتروني...", "Verifying your email address..."));
    void postSafiAuth<{ message: string }>("/email/verify", {
      token: initialVerificationToken
    })
      .then(() => {
        setAuthMode("signin");
        setVerificationNotice(
          L(
            "تم تأكيد بريدك الإلكتروني بنجاح! سجّل الدخول الآن لإكمال شراء الباقة المختارة أدناه.",
            "Your email is verified! Sign in below to complete your selected purchase."
          )
        );
      })
      .catch((err: unknown) => {
        const code = err instanceof SafiAuthApiError ? (err.code ?? "SF-AUTH-007") : "SF-AUTH-007";
        setVerificationNotice(null);
        setVerificationError(
          L(
            `تعذّر تأكيد رابط البريد الإلكتروني (${code}). يمكنك طلب رابط تحقق جديد أدناه.`,
            `Could not verify email link (${code}). You can request a fresh verification link below.`
          )
        );
      });
  }, [initialVerificationToken, L]);
  const currency: "USD" | "EGP" = countryIso2 === "EG" ? "EGP" : "USD";
  const providerLabel = countryIso2 === "EG" ? "Paymob (EGP)" : "Paddle (USD)";
  const baseMinor =
    (currency === "EGP" ? selectedPlan.egpMajor : selectedPlan.usdMajor) *
    100 *
    (selectedPlan.perSeat ? effectiveSeats : 1);
  const finalMinor = couponPreview ? couponPreview.final_amount_minor : baseMinor;

  const currentIntent: CheckoutIntent = useMemo(() => {
    const trimmedCoupon = couponCode.trim();
    const billingData: CustomerBillingData | undefined =
      countryIso2 === "EG"
        ? {
            first_name: firstName.trim(),
            last_name: lastName.trim(),
            email: authEmail.trim() || loginEmail.trim(),
            phone_number: billingPhone.trim(),
            country: "EG"
          }
        : undefined;
    return {
      product,
      billing_country_iso2: countryIso2,
      ...(product === "organization_seat_annual" ? { seats: effectiveSeats } : {}),
      ...(trimmedCoupon ? { coupon_code: trimmedCoupon } : {}),
      ...(billingData !== undefined ? { billing_data: billingData } : {})
    };
  }, [
    product,
    countryIso2,
    effectiveSeats,
    couponCode,
    firstName,
    lastName,
    authEmail,
    loginEmail,
    billingPhone
  ]);

  // Restore any persisted checkout attempt for this intent on page refresh / back-forward!
  useEffect(() => {
    const persisted = controller.getAttempt(currentIntent);
    if (!persisted) return;
    if (persisted.state === "reconciliation_required") {
      setReconciliationRequired(true);
      setOrderState("reconciliation_required");
    } else if (persisted.state === "checkout_pending" && persisted.order_id) {
      setOrderState("checkout_pending");
    } else if (persisted.state === "paid") {
      setOrderState("paid");
    }
  }, [controller, currentIntent]);

  // Fetch authoritative entitlements & redeem any pending Desktop handoff once authenticated!
  useEffect(() => {
    if (!authToken) return;
    let active = true;
    const syncAfterAuth = async (): Promise<void> => {
      try {
        const ent = await controller.fetchEntitlements(authToken);
        if (active) setEntitlements(ent);
      } catch {
        // Non-fatal initial entitlement load error; user can click Verify Status.
      }

      if (pendingHandoff) {
        try {
          const redeemed = await controller.redeemDesktopHandoff(
            pendingHandoff.handoffCode,
            pendingHandoff.state,
            authToken
          );
          if (!active) return;
          setProduct(redeemed.product);
          setSeats(normalizeOrganizationSeats(redeemed.seats));
          setCountryIso2(redeemed.billing_country_iso2 === "EG" ? "EG" : "US");
          if (redeemed.coupon_code) setCouponCode(redeemed.coupon_code);
          setFromDesktopHandoff(true);
          setPendingHandoff(null);
          try {
            window.sessionStorage.removeItem(HANDOFF_SESSION_KEY);
          } catch {
            // Ignore storage errors.
          }
          setHandoffError(null);
          setHandoffNotice(
            L(
              "تم التحقق من طلب الترقية القادم من تطبيق صافي لسطح المكتب وربطه بحسابك بأمان.",
              "Verified secure upgrade handoff from SafiData Desktop and bound it to your account."
            )
          );
        } catch (err) {
          if (!active) return;
          const code = err instanceof CheckoutLifecycleError ? err.code : "SF-AUTH-023";
          setPendingHandoff(null);
          try {
            window.sessionStorage.removeItem(HANDOFF_SESSION_KEY);
          } catch {
            // Ignore storage errors.
          }
          setHandoffError(
            L(
              `تعذّر استخدام رابط الترقية القادم من سطح المكتب (${code}) — ربما انتهت صلاحيته أو استُخدم مسبقًا أو يخص حسابًا آخر. يمكنك اختيار الخطة أدناه بأمان.`,
              `Desktop billing handoff could not be redeemed (${code}) — it may have expired, already been used, or belong to another account. You can still select a plan below.`
            )
          );
        }
      }
    };
    void syncAfterAuth();
    return () => {
      active = false;
    };
  }, [authToken, pendingHandoff, controller, L]);

  const handleWebsiteAuthSubmit = async (event: FormEvent): Promise<void> => {
    event.preventDefault();
    setAuthBusy(true);
    setAuthError(null);
    setAuthMessage(null);
    try {
      if (authMode === "signup") {
        // Save the chosen purchase intent durably before registration so it survives opening
        // the email verification link in another tab or window!
        if (!fromDesktopHandoff) {
          savePendingWebPurchaseIntent({
            product,
            billing_country_iso2: countryIso2,
            seats: effectiveSeats
          });
        }
        await postSafiAuth("/register", {
          email: loginEmail.trim(),
          display_name: loginName.trim() || loginEmail.trim().split("@")[0] || "Safi User",
          password: loginPassword
        });
        setAuthMessage(
          L(
            "تم إنشاء الحساب وإرسال رابط تأكيد البريد الإلكتروني. بعد تأكيد بريدك سجّل الدخول أدناه — ستبقى الباقة المختارة محفوظة كما هي.",
            "Account created and verification email sent. After verifying your email, sign in below — your selected plan and seats are preserved."
          )
        );
        setAuthMode("signin");
        setLoginPassword("");
        return;
      }
      const res = await controller.loginBillingWeb(loginEmail, loginPassword);
      setAuthToken(res.token);
      setAuthEmail(res.email);
      setLoginPassword("");
    } catch (err) {
      const code =
        err instanceof CheckoutLifecycleError
          ? err.code
          : err instanceof SafiAuthApiError
            ? (err.code ?? "SF-AUTH-001")
            : "SF-AUTH-001";
      setAuthError(
        L(
          `تعذّر تسجيل الدخول لبوابة الفوترة (${code}). تأكد من البيانات ومن تأكيد البريد الإلكتروني.`,
          `Billing portal authentication failed (${code}). Check your credentials and ensure your email is verified.`
        )
      );
    } finally {
      setAuthBusy(false);
    }
  };

  const handleResendVerification = async (): Promise<void> => {
    const emailTrimmed = loginEmail.trim();
    if (!emailTrimmed) {
      setAuthError(
        L(
          "أدخل بريدك الإلكتروني أولًا لإعادة إرسال رابط التحقق.",
          "Enter your email first to resend the verification link."
        )
      );
      return;
    }
    setAuthBusy(true);
    setAuthError(null);
    try {
      await postSafiAuth("/email/resend", { email: emailTrimmed });
      setAuthMessage(
        L(
          "إذا كان الحساب بحاجة للتحقق فسيصل رابط جديد إلى بريدك الإلكتروني.",
          "If this account requires verification, a fresh link has been sent to your email."
        )
      );
    } catch {
      setAuthError(
        L("تعذّر إعادة إرسال رابط التحقق الآن.", "Could not resend verification email right now.")
      );
    } finally {
      setAuthBusy(false);
    }
  };

  const handlePreviewCoupon = async (): Promise<void> => {
    if (!authToken || !couponCode.trim()) return;
    setCouponError(null);
    try {
      const preview = await controller.previewCoupon(currentIntent, authToken);
      setCouponPreview(preview);
    } catch (err) {
      setCouponPreview(null);
      const code = err instanceof CheckoutLifecycleError ? err.code : "SF-PAY-005";
      setCouponError(L(`رمز الخصم غير صالح (${code}).`, `Coupon code is not valid (${code}).`));
    }
  };

  const checkoutInFlightRef = useRef(false);

  const executeCheckout = async (allowReconcileRetry: boolean): Promise<void> => {
    if (!authToken || checkoutInFlightRef.current) return;
    if (countryIso2 === "EG") {
      if (!firstName.trim() || !lastName.trim() || !billingPhone.trim()) {
        setCheckoutError(
          L(
            "الدفع بالجنيه المصري عبر Paymob يتطلب الاسم الأول واسم العائلة ورقم الهاتف الحقيقي.",
            "Paymob EGP checkout requires your real first name, last name, and phone number."
          )
        );
        return;
      }
    }

    checkoutInFlightRef.current = true;
    setSubmittingCheckout(true);
    setCheckoutError(null);
    try {
      const res = await controller.startCheckout(currentIntent, authToken, {
        allowReconcileRetry
      });
      setActiveOrder(res);
      setOrderState(res.order_state);
      setReconciliationRequired(res.order_state === "reconciliation_required");
      if (res.checkout_url) {
        if (onOpenProviderCheckout) {
          onOpenProviderCheckout(res.checkout_url);
        } else if (typeof window !== "undefined") {
          window.open(res.checkout_url, "_blank", "noopener,noreferrer");
        }
      }
    } catch (err) {
      if (err instanceof CheckoutLifecycleError) {
        setReconciliationRequired(err.requiresReconciliation);
        setOrderState(err.requiresReconciliation ? "reconciliation_required" : "failed");
        setCheckoutError(
          err.requiresReconciliation
            ? L(
                `حالة الطلب السابق غير محسومة لدى مزود الدفع (${err.code}). لن ننشئ عملية خصم ثانية — اضغط «التحقق من الطلب القائم» أدناه.`,
                `Previous payment status is uncertain (${err.code}). Refusing to create a second charge — click "Reconcile Existing Order" below.`
              )
            : L(
                `تعذّر بدء الدفع (${err.code}): ${err.message}`,
                `Checkout failed (${err.code}): ${err.message}`
              )
        );
      } else {
        setOrderState("failed");
        setCheckoutError(L("حدث خطأ غير متوقع أثناء بدء الدفع.", "Unexpected checkout error."));
      }
    } finally {
      checkoutInFlightRef.current = false;
      setSubmittingCheckout(false);
    }
  };

  const handleVerifyServerStatus = async (): Promise<void> => {
    if (!authToken) return;
    setVerifyingServer(true);
    setCheckoutError(null);
    try {
      if (activeOrder?.order_id) {
        const status = await controller.fetchOrderStatus(activeOrder.order_id, authToken);
        setOrderState(status.order_state);
        setReconciliationRequired(status.order_state === "reconciliation_required");
      }
      const ent = await controller.verifyServerPaymentCompletion(authToken, currentIntent);
      setEntitlements(ent);
      if (ent.confirmed_paid) {
        setOrderState("paid");
        setReconciliationRequired(false);
      }
    } catch (err) {
      const code = err instanceof CheckoutLifecycleError ? err.code : "SF-SYS-001";
      setCheckoutError(
        L(
          `تعذّر التحقق من حالة الدفع من الخادم (${code}).`,
          `Could not confirm payment status from server (${code}).`
        )
      );
    } finally {
      setVerifyingServer(false);
    }
  };

  const handleResetFailedAttempt = (): void => {
    const cleared = controller.resetForNewPurchase(currentIntent);
    if (cleared) {
      setActiveOrder(null);
      setOrderState(null);
      setCheckoutError(null);
    }
  };

  return (
    <section
      className="safi-panel"
      data-billing-portal="true"
      style={{
        maxWidth: "780px",
        margin: "32px auto",
        padding: "28px",
        display: "grid",
        gap: "20px"
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
        <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
          <CreditCard size={22} aria-hidden="true" />
          <h1 style={{ fontSize: "22px", fontWeight: 800, margin: 0 }}>
            {L(
              "بوابة الاشتراكات والدفع الرسمية — صافي البيانات",
              "Official SafiData Billing Portal"
            )}
          </h1>
        </div>
        <div style={{ display: "flex", flexWrap: "wrap", gap: "8px" }}>
          {onOpenPricing ? (
            <button
              type="button"
              className="safi-btn safi-btn-ghost"
              onClick={onOpenPricing}
              data-billing-view-pricing="true"
            >
              <span>{L("الأسعار والاشتراك", "Plans & Pricing")}</span>
            </button>
          ) : null}
          {onBackToShowcase ? (
            <button type="button" className="safi-btn safi-btn-ghost" onClick={onBackToShowcase}>
              <ArrowLeft size={16} aria-hidden="true" />
              <span>{L("العودة للموقع", "Back to Website")}</span>
            </button>
          ) : null}
        </div>
      </div>

      <p style={{ fontSize: "13px", color: "var(--text-secondary)", margin: 0 }}>
        {L(
          "تتم جميع المدفوعات عبر بوابات Paddle (دولار) وPaymob (جنيه مصري) المستضافة رسميًا. لا يتم حفظ بيانات بطاقاتك داخل تطبيق سطح المكتب ولا تُفعّل أي باقة إلا بعد تأكيد Webhook موقّع من الخادم.",
          "All purchases complete through official hosted Paddle (USD) or Paymob (EGP) checkouts. Card details never touch SafiData Desktop, and entitlements activate strictly after verified server webhook confirmation."
        )}
      </p>

      {verificationNotice ? (
        <div
          role="status"
          data-billing-verification-notice="true"
          style={{
            padding: "12px 14px",
            borderRadius: "10px",
            background: "rgba(16, 185, 129, 0.14)",
            border: "1px solid rgba(16, 185, 129, 0.4)",
            fontSize: "13px",
            fontWeight: 600
          }}
        >
          {verificationNotice}
        </div>
      ) : null}
      {verificationError ? (
        <div
          role="alert"
          data-billing-verification-error="true"
          style={{
            padding: "12px 14px",
            borderRadius: "10px",
            border: "1px solid var(--safi-danger, #ef4444)",
            color: "var(--safi-danger, #ef4444)",
            fontSize: "13px"
          }}
        >
          {verificationError}
        </div>
      ) : null}

      {sawRedirectReturn && orderState !== "paid" ? (
        <div
          role="status"
          data-billing-redirect-notice="true"
          style={{
            padding: "12px 14px",
            borderRadius: "10px",
            border: "1px solid var(--border-subtle, #3f3f46)",
            fontSize: "13px"
          }}
        >
          {L(
            "عدت من بوابة الدفع — جارٍ التحقق من تأكيد الخادم الفعلي فقط (العودة من المتصفح وحدها ليست إثباتًا للدفع).",
            "Returned from checkout provider — awaiting authoritative server confirmation (browser redirect parameters are never treated as proof of payment)."
          )}
        </div>
      ) : null}

      {!authToken ? (
        <div style={{ display: "grid", gap: "18px" }}>
          {/* Selected Purchase Summary & Quick Plan Chooser (visible BEFORE login!) */}
          <div
            data-billing-preauth-summary="true"
            style={{
              display: "grid",
              gap: "12px",
              padding: "16px",
              borderRadius: "14px",
              border: "1px solid var(--border-subtle, #3f3f46)",
              background: "var(--surface-elevated, rgba(255,255,255,0.03))"
            }}
          >
            <div
              style={{
                display: "flex",
                flexWrap: "wrap",
                alignItems: "center",
                justifyContent: "space-between",
                gap: "8px"
              }}
            >
              <div>
                <div style={{ fontSize: "12px", opacity: 0.78 }}>
                  {L("الخطة المختارة للشراء", "Selected purchase")}
                </div>
                <strong data-billing-preauth-product={product} style={{ fontSize: "16px" }}>
                  {isAr ? selectedPlan.titleAr : selectedPlan.titleEn}
                  {product === "organization_seat_annual" ? ` × ${effectiveSeats}` : ""}
                </strong>
                <div style={{ fontSize: "12.5px", opacity: 0.8 }}>
                  {isAr ? selectedPlan.periodAr : selectedPlan.periodEn} · {providerLabel}
                </div>
              </div>
              <strong data-billing-preauth-price="true" style={{ fontSize: "18px" }}>
                {formatMinorPrice(baseMinor, currency)}
              </strong>
            </div>

            {!pendingHandoff ? (
              <div
                style={{
                  display: "grid",
                  gridTemplateColumns: "repeat(auto-fit, minmax(170px, 1fr))",
                  gap: "8px"
                }}
              >
                <label style={{ display: "grid", gap: "4px", fontSize: "12px", fontWeight: 600 }}>
                  <span>{L("المنتج", "Product")}</span>
                  <select
                    className="safi-input"
                    value={product}
                    onChange={(e) => {
                      const val = e.target.value as CheckoutProduct;
                      setProduct(val);
                      setCouponPreview(null);
                    }}
                    data-billing-preauth-select-product="true"
                  >
                    {CATALOG.map((item) => (
                      <option key={item.product} value={item.product}>
                        {isAr ? item.titleAr : item.titleEn}
                      </option>
                    ))}
                  </select>
                </label>

                <label style={{ display: "grid", gap: "4px", fontSize: "12px", fontWeight: 600 }}>
                  <span>{L("العملة / الدولة", "Country / Currency")}</span>
                  <select
                    className="safi-input"
                    value={countryIso2}
                    onChange={(e) => setCountryIso2(e.target.value === "EG" ? "EG" : "US")}
                    data-billing-preauth-select-country="true"
                  >
                    <option value="US">USD (Paddle)</option>
                    <option value="EG">EGP — مصر (Paymob)</option>
                  </select>
                </label>

                {product === "organization_seat_annual" ? (
                  <label style={{ display: "grid", gap: "4px", fontSize: "12px", fontWeight: 600 }}>
                    <span>{L(`المقاعد (≥ ${ORG_MIN_SEATS})`, `Seats (≥ ${ORG_MIN_SEATS})`)}</span>
                    <input
                      type="number"
                      min={ORG_MIN_SEATS}
                      max={5000}
                      className="safi-input"
                      value={seats}
                      onChange={(e) => setSeats(normalizeOrganizationSeats(Number(e.target.value)))}
                      data-billing-preauth-seats="true"
                    />
                  </label>
                ) : null}
              </div>
            ) : null}
          </div>

          <form
            onSubmit={(e) => void handleWebsiteAuthSubmit(e)}
            data-billing-auth-gate="true"
            style={{ display: "grid", gap: "12px", maxWidth: "460px" }}
          >
            <div style={{ display: "flex", alignItems: "center", gap: "8px", fontWeight: 700 }}>
              <Lock size={16} aria-hidden="true" />
              <span>
                {pendingHandoff
                  ? L(
                      "سجّل الدخول بحسابك لإكمال الترقية القادمة من تطبيق سطح المكتب",
                      "Sign in to your SafiData account to continue your Desktop upgrade"
                    )
                  : authMode === "signup"
                    ? L(
                        "أنشئ حساب صافي البيانات لإكمال اشتراكك مباشرة عبر الموقع",
                        "Create your SafiData account to complete checkout on the web"
                      )
                    : L(
                        "سجّل الدخول أو أنشئ حسابًا للمتابعة إلى الدفع الرسمي",
                        "Sign in or create an account to continue to secure checkout"
                      )}
              </span>
            </div>

            {authError ? (
              <div
                role="alert"
                data-billing-auth-error="true"
                style={{ color: "var(--safi-danger, #ef4444)", fontSize: "13px" }}
              >
                {authError}
              </div>
            ) : null}
            {authMessage ? (
              <div role="status" data-billing-auth-message="true" style={{ fontSize: "13px" }}>
                {authMessage}
              </div>
            ) : null}

            {authMode === "signup" ? (
              <label style={{ display: "grid", gap: "6px", fontSize: "13px", fontWeight: 600 }}>
                <span>{L("الاسم الكامل", "Full name")}</span>
                <input
                  type="text"
                  required
                  className="safi-input"
                  value={loginName}
                  onChange={(e) => setLoginName(e.target.value)}
                  data-billing-signup-name="true"
                />
              </label>
            ) : null}

            <label style={{ display: "grid", gap: "6px", fontSize: "13px", fontWeight: 600 }}>
              <span>{L("البريد الإلكتروني", "Email")}</span>
              <input
                type="email"
                required
                autoComplete="username"
                className="safi-input"
                value={loginEmail}
                onChange={(e) => setLoginEmail(e.target.value)}
                data-billing-login-email="true"
              />
            </label>

            <label style={{ display: "grid", gap: "6px", fontSize: "13px", fontWeight: 600 }}>
              <span>{L("كلمة المرور", "Password")}</span>
              <input
                type="password"
                required
                minLength={8}
                autoComplete={authMode === "signup" ? "new-password" : "current-password"}
                className="safi-input"
                value={loginPassword}
                onChange={(e) => setLoginPassword(e.target.value)}
                data-billing-login-password="true"
              />
            </label>

            <button
              type="submit"
              className="safi-btn safi-btn-primary"
              disabled={authBusy}
              data-billing-login-submit="true"
            >
              {authBusy
                ? L("جارٍ التحقق...", "Signing in...")
                : authMode === "signup"
                  ? L("إنشاء الحساب والمتابعة", "Create Account & Continue")
                  : L("تسجيل الدخول والمتابعة للدفع", "Sign In & Continue to Billing")}
            </button>

            <div style={{ display: "flex", flexWrap: "wrap", gap: "8px" }}>
              <button
                type="button"
                className="safi-btn safi-btn-ghost"
                data-billing-toggle-auth-mode="true"
                onClick={() => {
                  setAuthError(null);
                  setAuthMode((prev) => (prev === "signin" ? "signup" : "signin"));
                }}
              >
                {authMode === "signin"
                  ? L("ليس لديك حساب؟ أنشئ حسابًا جديدًا", "New to SafiData? Create an account")
                  : L("لديك حساب بالفعل؟ سجّل الدخول", "Already have an account? Sign in")}
              </button>
              <button
                type="button"
                className="safi-btn safi-btn-ghost"
                disabled={authBusy}
                data-billing-resend-verification="true"
                onClick={() => void handleResendVerification()}
              >
                {L("إعادة إرسال رابط التحقق", "Resend verification email")}
              </button>
            </div>
          </form>
        </div>
      ) : (
        <div data-billing-authenticated="true" style={{ display: "grid", gap: "18px" }}>
          <div
            style={{
              display: "flex",
              flexWrap: "wrap",
              alignItems: "center",
              justifyContent: "space-between",
              gap: "10px",
              padding: "12px 14px",
              borderRadius: "10px",
              background: "var(--surface-elevated, rgba(255,255,255,0.04))"
            }}
          >
            <div style={{ fontSize: "13px" }}>
              <strong>{authEmail || L("حساب موثّق", "Authenticated Account")}</strong>
              {entitlements ? (
                <span
                  data-billing-current-plan={entitlements.effective_plan}
                  style={{ marginInlineStart: "10px" }}
                >
                  · {L("الخطة الحالية:", "Current plan:")}{" "}
                  <strong>{entitlements.effective_plan}</strong> ({entitlements.state})
                </span>
              ) : null}
            </div>
            <button
              type="button"
              className="safi-btn safi-btn-ghost"
              onClick={() => void handleVerifyServerStatus()}
              disabled={verifyingServer}
              data-billing-verify-status="true"
            >
              <RefreshCw size={14} aria-hidden="true" />
              <span>{L("تحديث الحالة من الخادم", "Verify Payment Status")}</span>
            </button>
          </div>

          {handoffNotice ? (
            <div
              role="status"
              data-billing-handoff-redeemed="true"
              style={{ fontSize: "13px", fontWeight: 600 }}
            >
              <ShieldCheck
                size={16}
                aria-hidden="true"
                style={{ verticalAlign: "middle", marginInlineEnd: "6px" }}
              />
              {handoffNotice}
            </div>
          ) : null}
          {handoffError ? (
            <div
              role="alert"
              data-billing-handoff-error="true"
              style={{ fontSize: "13px", color: "var(--safi-danger, #ef4444)" }}
            >
              {handoffError}
            </div>
          ) : null}

          {/* Plan Selection Grid (ALL 5 approved purchase types) */}
          <div style={{ display: "grid", gap: "10px" }}>
            <h2 style={{ fontSize: "15px", fontWeight: 700, margin: 0 }}>
              {L("1. اختر الخطة أو الحزمة", "1. Choose your plan or add-on")}
            </h2>
            <div
              style={{
                display: "grid",
                gridTemplateColumns: "repeat(auto-fit, minmax(220px, 1fr))",
                gap: "10px"
              }}
            >
              {CATALOG.map((item) => {
                const active = item.product === product;
                const priceDisplay =
                  currency === "EGP"
                    ? `${item.egpMajor.toLocaleString("en-US")} EGP`
                    : `$${item.usdMajor.toLocaleString("en-US")}`;
                return (
                  <button
                    key={item.product}
                    type="button"
                    data-billing-plan-option={item.product}
                    aria-pressed={active}
                    onClick={() => {
                      setProduct(item.product);
                      setCouponPreview(null);
                      setCheckoutError(null);
                    }}
                    style={{
                      textAlign: isAr ? "right" : "left",
                      padding: "14px",
                      borderRadius: "12px",
                      cursor: "pointer",
                      border: active
                        ? "2px solid var(--accent-primary, #10b981)"
                        : "1px solid var(--border-subtle, #3f3f46)",
                      background: active ? "rgba(16, 185, 129, 0.1)" : "transparent",
                      color: "inherit",
                      display: "grid",
                      gap: "4px"
                    }}
                  >
                    <strong>{isAr ? item.titleAr : item.titleEn}</strong>
                    <span style={{ fontSize: "16px", fontWeight: 800 }}>
                      {priceDisplay}
                      {item.perSeat ? (isAr ? " / مقعد / سنة" : " / seat / yr") : ""}
                    </span>
                    <span style={{ fontSize: "12px", opacity: 0.8 }}>
                      {isAr ? item.periodAr : item.periodEn}
                    </span>
                    <span style={{ fontSize: "12px", opacity: 0.75 }}>
                      {isAr ? item.subtitleAr : item.subtitleEn}
                    </span>
                  </button>
                );
              })}
            </div>
          </div>

          {/* Organization seats & Billing Country */}
          <div
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(auto-fit, minmax(220px, 1fr))",
              gap: "12px"
            }}
          >
            <label style={{ display: "grid", gap: "6px", fontSize: "13px", fontWeight: 600 }}>
              <span>{L("بلد الفوترة والعملة", "Billing country & currency")}</span>
              <select
                className="safi-input"
                value={countryIso2}
                onChange={(e) => {
                  setCountryIso2(e.target.value === "EG" ? "EG" : "US");
                  setCouponPreview(null);
                }}
                data-billing-country="true"
              >
                <option value="US">
                  {L("دولي — USD (Paddle)", "International — USD (Paddle)")}
                </option>
                <option value="EG">{L("مصر — EGP (Paymob)", "Egypt — EGP (Paymob)")}</option>
              </select>
            </label>

            {product === "organization_seat_annual" ? (
              <label style={{ display: "grid", gap: "6px", fontSize: "13px", fontWeight: 600 }}>
                <span>
                  {L(
                    `عدد المقاعد (الحد الأدنى ${ORG_MIN_SEATS})`,
                    `Seats (minimum ${ORG_MIN_SEATS})`
                  )}
                </span>
                <input
                  type="number"
                  min={ORG_MIN_SEATS}
                  max={5000}
                  className="safi-input"
                  value={seats}
                  onChange={(e) => {
                    const parsed = Number.parseInt(e.target.value, 10);
                    setSeats(
                      Number.isFinite(parsed) ? Math.max(ORG_MIN_SEATS, parsed) : ORG_MIN_SEATS
                    );
                    setCouponPreview(null);
                  }}
                  data-billing-seats="true"
                />
              </label>
            ) : null}

            <div style={{ display: "grid", gap: "6px", fontSize: "13px", fontWeight: 600 }}>
              <span>{L("كود خصم (اختياري)", "Coupon code (optional)")}</span>
              <div style={{ display: "flex", gap: "8px" }}>
                <input
                  type="text"
                  className="safi-input"
                  value={couponCode}
                  onChange={(e) => setCouponCode(e.target.value)}
                  placeholder="WELCOME20"
                  data-billing-coupon-input="true"
                />
                <button
                  type="button"
                  className="safi-btn safi-btn-ghost"
                  onClick={() => void handlePreviewCoupon()}
                  data-billing-coupon-apply="true"
                >
                  {L("تطبيق", "Apply")}
                </button>
              </div>
              {couponError ? (
                <span style={{ fontSize: "12px", color: "var(--safi-danger, #ef4444)" }}>
                  {couponError}
                </span>
              ) : null}
            </div>
          </div>

          {/* Real Customer Billing Details for Paymob Egypt (zero fake PII) */}
          {countryIso2 === "EG" ? (
            <div
              data-billing-paymob-details="true"
              style={{
                display: "grid",
                gridTemplateColumns: "repeat(auto-fit, minmax(180px, 1fr))",
                gap: "10px",
                padding: "14px",
                borderRadius: "10px",
                border: "1px solid var(--border-subtle, #3f3f46)"
              }}
            >
              <label style={{ display: "grid", gap: "4px", fontSize: "12px", fontWeight: 600 }}>
                <span>{L("الاسم الأول (مطلوب لـ Paymob)", "First Name (Paymob)")}</span>
                <input
                  type="text"
                  required
                  className="safi-input"
                  value={firstName}
                  onChange={(e) => setFirstName(e.target.value)}
                  data-billing-first-name="true"
                />
              </label>
              <label style={{ display: "grid", gap: "4px", fontSize: "12px", fontWeight: 600 }}>
                <span>{L("اسم العائلة (مطلوب لـ Paymob)", "Last Name (Paymob)")}</span>
                <input
                  type="text"
                  required
                  className="safi-input"
                  value={lastName}
                  onChange={(e) => setLastName(e.target.value)}
                  data-billing-last-name="true"
                />
              </label>
              <label style={{ display: "grid", gap: "4px", fontSize: "12px", fontWeight: 600 }}>
                <span>{L("رقم الهاتف (+20...)", "Phone Number (+20...)")}</span>
                <input
                  type="tel"
                  required
                  className="safi-input"
                  value={billingPhone}
                  onChange={(e) => setBillingPhone(e.target.value)}
                  placeholder="+201012345678"
                  data-billing-phone="true"
                />
              </label>
            </div>
          ) : null}

          {/* Order Summary & Protected Pay Button */}
          <div
            data-billing-summary="true"
            style={{
              display: "grid",
              gap: "12px",
              padding: "16px",
              borderRadius: "12px",
              border: "1px solid var(--border-subtle, #3f3f46)"
            }}
          >
            <div style={{ display: "flex", justifyContent: "space-between", fontSize: "14px" }}>
              <span>
                {isAr ? selectedPlan.titleAr : selectedPlan.titleEn}
                {product === "organization_seat_annual" ? ` × ${effectiveSeats}` : ""}
              </span>
              <strong data-billing-final-price="true">
                {formatMinorPrice(finalMinor, currency)}
              </strong>
            </div>
            <div style={{ fontSize: "12px", opacity: 0.8 }}>
              {L("مزود الدفع المعتمد من الخادم:", "Authoritative payment provider:")}{" "}
              <strong>{providerLabel}</strong> ·{" "}
              {isAr ? selectedPlan.periodAr : selectedPlan.periodEn}
            </div>

            {checkoutError ? (
              <div
                role="alert"
                data-billing-checkout-error="true"
                style={{ fontSize: "13px", color: "var(--safi-danger, #ef4444)" }}
              >
                {checkoutError}
              </div>
            ) : null}

            {/* Payment Lifecycle State Display */}
            {orderState === "paid" ? (
              <div
                role="status"
                data-billing-state="paid"
                style={{
                  display: "grid",
                  gap: "10px",
                  padding: "12px",
                  borderRadius: "10px",
                  background: "rgba(16,185,129,0.12)"
                }}
              >
                <div style={{ display: "flex", alignItems: "center", gap: "8px", fontWeight: 700 }}>
                  <CheckCircle2 size={18} aria-hidden="true" />
                  <span>
                    {L(
                      "تم تأكيد الدفع وتفعيل استحقاقك من الخادم بنجاح!",
                      "Payment confirmed by server and entitlements activated!"
                    )}
                  </span>
                </div>
                <div style={{ display: "flex", flexWrap: "wrap", gap: "10px" }}>
                  <a
                    href={
                      activeOrder
                        ? `safi://billing?order_id=${activeOrder.order_id}`
                        : "safi://billing"
                    }
                    className="safi-btn safi-btn-primary"
                    data-billing-return-desktop="true"
                  >
                    {L("العودة إلى تطبيق صافي لسطح المكتب", "Return to SafiData Desktop")}
                  </a>
                  {onDownloadDesktop ? (
                    <button
                      type="button"
                      className="safi-btn safi-btn-ghost"
                      onClick={onDownloadDesktop}
                      data-billing-download-desktop="true"
                    >
                      <Download size={15} aria-hidden="true" />
                      <span>
                        {L("حمّل تطبيق سطح المكتب وسجّل الدخول بحسابك", "Download Desktop App")}
                      </span>
                    </button>
                  ) : null}
                </div>
              </div>
            ) : reconciliationRequired ? (
              <div
                role="alert"
                data-billing-state="reconciliation_required"
                style={{
                  display: "grid",
                  gap: "10px",
                  padding: "12px",
                  borderRadius: "10px",
                  background: "rgba(245,158,11,0.12)"
                }}
              >
                <div style={{ display: "flex", alignItems: "center", gap: "8px", fontWeight: 700 }}>
                  <AlertTriangle size={18} aria-hidden="true" />
                  <span>
                    {L(
                      "مطلوب التحقق من الطلب السابق قبل أي محاولة دفع جديدة (منع الخصم المزدوج).",
                      "Reconciliation required before starting a new charge (duplicate-charge protection)."
                    )}
                  </span>
                </div>
                <button
                  type="button"
                  className="safi-btn safi-btn-primary"
                  disabled={submittingCheckout}
                  onClick={() => void executeCheckout(true)}
                  data-billing-reconcile-retry="true"
                >
                  {L("التحقق من الطلب القائم بنفس المفتاح", "Reconcile Existing Order")}
                </button>
              </div>
            ) : orderState === "checkout_pending" || orderState === "payment_pending" ? (
              <div
                role="status"
                data-billing-state="checkout_pending"
                style={{
                  display: "grid",
                  gap: "10px",
                  padding: "12px",
                  borderRadius: "10px",
                  background: "rgba(59,130,246,0.12)"
                }}
              >
                <div style={{ fontSize: "13px", fontWeight: 600 }}>
                  {L(
                    "بوابة الدفع جاهزة. أكمل الدفع في صفحة المزود الرسمية ثم اضغط «تحديث الحالة من الخادم».",
                    "Official hosted checkout created. Complete payment on the provider page, then click Verify Payment Status."
                  )}
                </div>
                <div style={{ display: "flex", flexWrap: "wrap", gap: "8px" }}>
                  {activeOrder?.checkout_url ? (
                    <a
                      href={activeOrder.checkout_url}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="safi-btn safi-btn-primary"
                      data-billing-hosted-link="true"
                    >
                      <ExternalLink size={15} aria-hidden="true" />
                      <span>{L("فتح صفحة الدفع الرسمية", "Open Hosted Checkout")}</span>
                    </a>
                  ) : null}
                  <a
                    href={
                      activeOrder
                        ? `safi://billing?order_id=${activeOrder.order_id}`
                        : "safi://billing"
                    }
                    className="safi-btn safi-btn-ghost"
                    data-billing-return-desktop="true"
                  >
                    {L("العودة إلى تطبيق سطح المكتب", "Return to Desktop App")}
                  </a>
                </div>
              </div>
            ) : (
              <div style={{ display: "flex", flexWrap: "wrap", gap: "10px" }}>
                <button
                  type="button"
                  className="safi-btn safi-btn-primary"
                  disabled={submittingCheckout}
                  onClick={() => void executeCheckout(false)}
                  data-billing-pay-submit="true"
                >
                  <CreditCard size={16} aria-hidden="true" />
                  <span>
                    {submittingCheckout
                      ? L("جارٍ تجهيز جلسة الدفع...", "Creating protected checkout...")
                      : L("المتابعة إلى الدفع الآمن", "Continue to Secure Checkout")}
                  </span>
                </button>
                {orderState === "failed" ||
                orderState === "canceled" ||
                orderState === "expired" ? (
                  <button
                    type="button"
                    className="safi-btn safi-btn-ghost"
                    onClick={handleResetFailedAttempt}
                    data-billing-state={orderState}
                  >
                    {L("بدء محاولة جديدة", "Start Fresh Attempt")}
                  </button>
                ) : null}
              </div>
            )}
          </div>
        </div>
      )}
    </section>
  );
}
