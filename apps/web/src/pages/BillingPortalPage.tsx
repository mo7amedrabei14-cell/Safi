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
 * - Full field-level validation (Paymob Egypt phone normalization, name validation, verified account
 *   email display & syntax check, seat min/max bounds, coupon error states, focus-first-invalid-field).
 * - NEVER exposes Desktop Studio or local customer data on the website.
 */
import type { FormEvent, ReactNode } from "react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  AlertCircle,
  AlertTriangle,
  ArrowLeft,
  Building2,
  Check,
  CheckCircle2,
  ChevronDown,
  CreditCard,
  Download,
  ExternalLink,
  Globe,
  Loader2,
  Lock,
  Mail,
  RefreshCw,
  ShieldCheck,
  Tag,
  Users
} from "lucide-react";
import "./billing-portal.css";
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
  BILLING_MAX_ORGANIZATION_SEATS,
  BILLING_MIN_ORGANIZATION_SEATS,
  validateAndNormalizeEgyptianPhone,
  validateBillingCheckoutForm,
  type BillingFieldKey
} from "../lib/billing-validation";
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
import {
  fetchBillingOrganizationsSummary,
  type EnrichedOrganizationSummary
} from "../lib/organization-api";

export interface BillingPortalPageProps {
  readonly lang?: SafiLang;
  readonly onToggleLang?: () => void;
  readonly onBackToShowcase?: () => void;
  readonly onOpenPricing?: () => void;
  readonly onOpenTeam?: () => void;
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
  onToggleLang,
  onBackToShowcase,
  onOpenPricing,
  onOpenTeam,
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
  const initialSeatsValue = initialUrl.directIntent?.seats ?? ORG_MIN_SEATS;
  const [product, setProduct] = useState<CheckoutProduct>(
    initialUrl.directIntent?.product ?? "individual_monthly"
  );
  const [seats, setSeats] = useState<number>(initialSeatsValue);
  const [seatsInput, setSeatsInput] = useState<string>(String(initialSeatsValue));
  const [countryIso2, setCountryIso2] = useState<"US" | "EG">(
    initialUrl.directIntent?.billing_country_iso2 ?? "US"
  );
  const [couponCode, setCouponCode] = useState<string>("");
  const [couponPreview, setCouponPreview] = useState<CouponPreviewApiResponse | null>(null);
  const [couponError, setCouponError] = useState<string | null>(null);
  const [applyingCoupon, setApplyingCoupon] = useState<boolean>(false);
  const [fromDesktopHandoff, setFromDesktopHandoff] = useState<boolean>(
    initialUrl.handoff !== null
  );
  const [verificationNotice, setVerificationNotice] = useState<string | null>(null);
  const [verificationError, setVerificationError] = useState<string | null>(null);
  const verificationAttemptedRef = useRef<string | null>(null);
  const [userOrganizations, setUserOrganizations] = useState<
    readonly EnrichedOrganizationSummary[]
  >([]);

  useEffect(() => {
    if (!authToken) {
      setUserOrganizations([]);
      return;
    }
    void fetchBillingOrganizationsSummary(authToken)
      .then((res) => setUserOrganizations(res.organizations))
      .catch(() => setUserOrganizations([]));
  }, [authToken]);

  // Real Paymob EGP customer billing details (zero fake personal data!)
  const [firstName, setFirstName] = useState("");
  const [lastName, setLastName] = useState("");
  const [billingPhone, setBillingPhone] = useState("");

  // Field-level touch & submit validation state (no premature nagging before blur or submit)
  const [touched, setTouched] = useState<Partial<Record<BillingFieldKey, boolean>>>({});
  const [submitAttempted, setSubmitAttempted] = useState(false);

  // Input refs for focusing the first invalid field on submit
  const seatsInputRef = useRef<HTMLInputElement | null>(null);
  const emailInputRef = useRef<HTMLInputElement | null>(null);
  const firstNameInputRef = useRef<HTMLInputElement | null>(null);
  const lastNameInputRef = useRef<HTMLInputElement | null>(null);
  const phoneInputRef = useRef<HTMLInputElement | null>(null);

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
  const effectiveBillingEmail = (authEmail || loginEmail).trim();

  const validation = useMemo(
    () =>
      validateBillingCheckoutForm(
        {
          product,
          country: countryIso2,
          email: effectiveBillingEmail,
          firstName,
          lastName,
          phone: billingPhone,
          seatsInput
        },
        isAr
      ),
    [
      product,
      countryIso2,
      effectiveBillingEmail,
      firstName,
      lastName,
      billingPhone,
      seatsInput,
      isAr
    ]
  );

  const getVisibleFieldError = useCallback(
    (field: BillingFieldKey): string | undefined => {
      if (!submitAttempted && !touched[field]) return undefined;
      return validation.errors[field];
    },
    [submitAttempted, touched, validation.errors]
  );

  const markTouched = useCallback((field: BillingFieldKey) => {
    setTouched((prev) => (prev[field] ? prev : { ...prev, [field]: true }));
  }, []);

  const focusField = useCallback((field: BillingFieldKey | null) => {
    if (!field) return;
    const map: Record<BillingFieldKey, HTMLInputElement | null> = {
      seats: seatsInputRef.current,
      email: emailInputRef.current,
      firstName: firstNameInputRef.current,
      lastName: lastNameInputRef.current,
      phone: phoneInputRef.current
    };
    const el = map[field];
    if (el) {
      el.focus();
      el.scrollIntoView?.({ behavior: "smooth", block: "center" });
    }
  }, []);

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
  const discountMinor = couponPreview ? couponPreview.discount_amount_minor : 0;

  const normalizedPhoneForProvider = useMemo(() => {
    if (countryIso2 !== "EG") return "";
    const parsed = validateAndNormalizeEgyptianPhone(billingPhone, isAr);
    return parsed.normalized ?? billingPhone.trim();
  }, [countryIso2, billingPhone, isAr]);

  const currentIntent: CheckoutIntent = useMemo(() => {
    const trimmedCoupon = couponCode.trim();
    const billingData: CustomerBillingData | undefined =
      countryIso2 === "EG"
        ? {
            first_name: firstName.trim(),
            last_name: lastName.trim(),
            email: effectiveBillingEmail,
            phone_number: normalizedPhoneForProvider,
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
    effectiveBillingEmail,
    normalizedPhoneForProvider
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
          const redeemedSeats = normalizeOrganizationSeats(redeemed.seats);
          setProduct(redeemed.product);
          setSeats(redeemedSeats);
          setSeatsInput(String(redeemedSeats));
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
    if (!authToken) return;
    const trimmed = couponCode.trim();
    if (!trimmed) {
      setCouponPreview(null);
      setCouponError(
        L("أدخل رمز الخصم أولًا قبل الضغط على تطبيق.", "Enter a coupon code before applying.")
      );
      return;
    }
    setApplyingCoupon(true);
    setCouponError(null);
    try {
      const preview = await controller.previewCoupon(currentIntent, authToken);
      setCouponPreview(preview);
    } catch (err) {
      setCouponPreview(null);
      const code = err instanceof CheckoutLifecycleError ? err.code : "SF-PAY-005";
      setCouponError(
        L(
          `رمز الخصم «${trimmed}» غير صالح أو منتهي الصلاحية (${code}).`,
          `Coupon code "${trimmed}" is invalid or expired (${code}).`
        )
      );
    } finally {
      setApplyingCoupon(false);
    }
  };

  const handleSeatsInputChange = (raw: string): void => {
    setSeatsInput(raw);
    setCouponPreview(null);
    setCheckoutError(null);
    const trimmed = raw.trim();
    if (/^\d+$/.test(trimmed)) {
      const parsed = Number.parseInt(trimmed, 10);
      if (parsed >= BILLING_MIN_ORGANIZATION_SEATS && parsed <= BILLING_MAX_ORGANIZATION_SEATS) {
        setSeats(parsed);
      }
    }
  };

  const handleStepSeats = (delta: number): void => {
    const base = /^\d+$/.test(seatsInput.trim()) ? Number.parseInt(seatsInput.trim(), 10) : seats;
    const next = Math.min(
      BILLING_MAX_ORGANIZATION_SEATS,
      Math.max(BILLING_MIN_ORGANIZATION_SEATS, base + delta)
    );
    setSeats(next);
    setSeatsInput(String(next));
    setCouponPreview(null);
    setCheckoutError(null);
    markTouched("seats");
  };

  const checkoutInFlightRef = useRef(false);

  const executeCheckout = async (allowReconcileRetry: boolean): Promise<void> => {
    if (!authToken || checkoutInFlightRef.current) return;

    // Strict client-side validation BEFORE issuing any network checkout request!
    if (!validation.valid) {
      setSubmitAttempted(true);
      setTouched({
        seats: true,
        email: true,
        firstName: true,
        lastName: true,
        phone: true
      });
      setCheckoutError(
        L(
          "يرجى مراجعة الحقول المميّزة باللون الأحمر أدناه قبل المتابعة إلى الدفع.",
          "Please fix the highlighted billing fields before continuing to checkout."
        )
      );
      focusField(validation.firstInvalidField);
      return;
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

  const seatsError = getVisibleFieldError("seats");
  const emailError = getVisibleFieldError("email");
  const firstNameError = getVisibleFieldError("firstName");
  const lastNameError = getVisibleFieldError("lastName");
  const phoneError = getVisibleFieldError("phone");

  return (
    <section className="safi-billing-portal" dir={isAr ? "rtl" : "ltr"} data-billing-portal="true">
      {/* Compact Header & Security Context */}
      <div className="safi-bp-topbar">
        <div className="safi-bp-topbar__nav">
          {onBackToShowcase ? (
            <button type="button" className="safi-btn safi-btn-ghost" onClick={onBackToShowcase}>
              <ArrowLeft size={16} aria-hidden="true" />
              <span>{L("العودة للموقع", "Back to Website")}</span>
            </button>
          ) : null}
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
        </div>

        <div className="safi-bp-topbar__nav">
          <div className="safi-bp-trust-pill">
            <ShieldCheck size={15} aria-hidden="true" />
            <span>
              {L(
                "بوابة دفع رسمية مستضافة · لا نجمع أرقام بطاقات داخل التطبيق",
                "Official Hosted Checkout · Zero Card Data Stored"
              )}
            </span>
          </div>
          {onToggleLang ? (
            <button
              type="button"
              className="safi-btn safi-btn-ghost"
              onClick={onToggleLang}
              aria-label={isAr ? "Switch to English" : "Switch to Arabic"}
              data-billing-toggle-lang="true"
              style={{ minHeight: "40px" }}
            >
              <Globe size={15} aria-hidden="true" />
              <span>{isAr ? "English" : "العربية"}</span>
            </button>
          ) : null}
        </div>
      </div>

      {/* Hero Title Card */}
      <header className="safi-bp-card safi-bp-card--hero">
        <div className="safi-bp-hero-row">
          <div className="safi-bp-title-group">
            <span className="safi-bp-kicker">
              <CreditCard size={14} aria-hidden="true" />
              {L("فوترة صافي البيانات الآمنة", "SafiData Secure Billing")}
            </span>
            <h1 className="safi-bp-heading">
              {L(
                "بوابة الاشتراكات والدفع الرسمية — صافي البيانات",
                "Official SafiData Billing Portal"
              )}
            </h1>
            <p className="safi-bp-subtitle">
              {L(
                "تتم جميع المدفوعات عبر بوابات Paddle (دولار) وPaymob (جنيه مصري) المستضافة رسميًا. لا تُفعّل أي باقة إلا بعد تأكيد Webhook موقّع من الخادم.",
                "All purchases complete through official hosted Paddle (USD) or Paymob (EGP) checkouts. Card details never touch SafiData Desktop, and entitlements activate strictly after verified server webhook confirmation."
              )}
            </p>
          </div>
        </div>

        {verificationNotice ? (
          <div
            role="status"
            className="safi-bp-alert safi-bp-alert--success"
            data-billing-verification-notice="true"
          >
            <CheckCircle2 size={17} aria-hidden="true" />
            <span>{verificationNotice}</span>
          </div>
        ) : null}

        {verificationError ? (
          <div
            role="alert"
            className="safi-bp-alert safi-bp-alert--danger"
            data-billing-verification-error="true"
          >
            <AlertCircle size={17} aria-hidden="true" />
            <span>{verificationError}</span>
          </div>
        ) : null}

        {sawRedirectReturn && orderState !== "paid" ? (
          <div
            role="status"
            className="safi-bp-alert safi-bp-alert--info"
            data-billing-redirect-notice="true"
          >
            <ShieldCheck size={17} aria-hidden="true" />
            <span>
              {L(
                "عدت من بوابة الدفع — جارٍ التحقق من تأكيد الخادم الفعلي فقط (العودة من المتصفح وحدها ليست إثباتًا للدفع).",
                "Returned from checkout provider — awaiting authoritative server confirmation (browser redirect parameters are never treated as proof of payment)."
              )}
            </span>
          </div>
        ) : null}
      </header>

      {!authToken ? (
        /* =========================================================================
         * PRE-AUTHENTICATION EXPERIENCE (Plan Summary + Login/Signup side-by-side)
         * ========================================================================= */
        <div className="safi-bp-auth-grid">
          <div className="safi-bp-card">
            <form
              onSubmit={(e) => void handleWebsiteAuthSubmit(e)}
              data-billing-auth-gate="true"
              style={{ display: "grid", gap: "14px" }}
            >
              <div className="safi-bp-section-head">
                <h2 className="safi-bp-section-title">
                  <Lock size={17} aria-hidden="true" />
                  <span>
                    {pendingHandoff
                      ? L(
                          "سجّل الدخول لإكمال الترقية من تطبيق سطح المكتب",
                          "Sign in to continue your Desktop upgrade"
                        )
                      : authMode === "signup"
                        ? L(
                            "أنشئ حساب صافي البيانات لإكمال اشتراكك",
                            "Create your SafiData account"
                          )
                        : L(
                            "سجّل الدخول للمتابعة إلى الدفع الرسمي",
                            "Sign in to continue to checkout"
                          )}
                  </span>
                </h2>
              </div>

              <p className="safi-bp-section-hint">
                {L(
                  "تسجيل الدخول في بوابة الفوترة على الموقع يصدر جلسة مستقلة ولا يؤثر على جلسة تطبيق سطح المكتب.",
                  "Signing in to the billing portal uses an isolated web billing session and never revokes your Desktop app."
                )}
              </p>

              {authError ? (
                <div
                  role="alert"
                  className="safi-bp-alert safi-bp-alert--danger"
                  data-billing-auth-error="true"
                >
                  <AlertCircle size={16} aria-hidden="true" />
                  <span>{authError}</span>
                </div>
              ) : null}

              {authMessage ? (
                <div
                  role="status"
                  className="safi-bp-alert safi-bp-alert--success"
                  data-billing-auth-message="true"
                >
                  <CheckCircle2 size={16} aria-hidden="true" />
                  <span>{authMessage}</span>
                </div>
              ) : null}

              {authMode === "signup" ? (
                <div className="safi-bp-field">
                  <label className="safi-bp-label" htmlFor="safi-bp-signup-name">
                    <span>{L("الاسم الكامل", "Full name")}</span>
                  </label>
                  <input
                    id="safi-bp-signup-name"
                    type="text"
                    required
                    className="safi-bp-input"
                    value={loginName}
                    onChange={(e) => setLoginName(e.target.value)}
                    placeholder={L("مثال: محمد ربيع", "e.g. Mohamed Rabei")}
                    data-billing-signup-name="true"
                  />
                </div>
              ) : null}

              <div className="safi-bp-field">
                <label className="safi-bp-label" htmlFor="safi-bp-login-email">
                  <span>{L("البريد الإلكتروني", "Email address")}</span>
                </label>
                <input
                  id="safi-bp-login-email"
                  type="email"
                  dir="ltr"
                  required
                  autoComplete="username"
                  className="safi-bp-input"
                  value={loginEmail}
                  onChange={(e) => setLoginEmail(e.target.value)}
                  placeholder="name@company.com"
                  data-billing-login-email="true"
                />
              </div>

              <div className="safi-bp-field">
                <label className="safi-bp-label" htmlFor="safi-bp-login-password">
                  <span>{L("كلمة المرور", "Password")}</span>
                </label>
                <input
                  id="safi-bp-login-password"
                  type="password"
                  required
                  minLength={8}
                  autoComplete={authMode === "signup" ? "new-password" : "current-password"}
                  className="safi-bp-input"
                  value={loginPassword}
                  onChange={(e) => setLoginPassword(e.target.value)}
                  data-billing-login-password="true"
                />
              </div>

              <button
                type="submit"
                className="safi-btn safi-btn-primary safi-bp-checkout-btn"
                disabled={authBusy}
                data-billing-login-submit="true"
              >
                {authBusy ? (
                  <Loader2 size={16} className="safi-bp-spin" aria-hidden="true" />
                ) : null}
                <span>
                  {authBusy
                    ? L("جارٍ التحقق...", "Signing in...")
                    : authMode === "signup"
                      ? L("إنشاء الحساب والمتابعة", "Create Account & Continue")
                      : L("تسجيل الدخول والمتابعة للدفع", "Sign In & Continue to Billing")}
                </span>
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

          {/* Pre-auth Selected Purchase Summary & Quick Plan Chooser */}
          <aside className="safi-bp-card" data-billing-preauth-summary="true">
            <div className="safi-bp-preauth-summary">
              <div className="safi-bp-section-head">
                <span className="safi-bp-helper">
                  {L("الخطة المختارة للشراء", "Selected purchase")}
                </span>
                <span className="safi-bp-account-badge">{providerLabel}</span>
              </div>
              <strong data-billing-preauth-product={product} style={{ fontSize: "1.1rem" }}>
                {isAr ? selectedPlan.titleAr : selectedPlan.titleEn}
                {product === "organization_seat_annual" ? ` × ${effectiveSeats}` : ""}
              </strong>
              <div className="safi-bp-helper">
                {isAr ? selectedPlan.periodAr : selectedPlan.periodEn} ·{" "}
                {isAr ? selectedPlan.subtitleAr : selectedPlan.subtitleEn}
              </div>
              <div className="safi-bp-total-row" style={{ paddingTop: "6px" }}>
                <span className="safi-bp-total-label">
                  {L("السعر المعتمد", "Authoritative total")}
                </span>
                <strong className="safi-bp-total-value" data-billing-preauth-price="true">
                  {formatMinorPrice(baseMinor, currency)}
                </strong>
              </div>
            </div>

            {!pendingHandoff ? (
              <div style={{ display: "grid", gap: "12px" }}>
                <div className="safi-bp-field">
                  <label className="safi-bp-label" htmlFor="safi-bp-preauth-product">
                    <span>{L("المنتج", "Product")}</span>
                  </label>
                  <div className="safi-bp-select-wrap">
                    <select
                      id="safi-bp-preauth-product"
                      className="safi-bp-select"
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
                    <ChevronDown size={16} className="safi-bp-select-chevron" aria-hidden="true" />
                  </div>
                </div>

                <div className="safi-bp-field">
                  <label className="safi-bp-label" htmlFor="safi-bp-preauth-country">
                    <span>{L("العملة / الدولة", "Country / Currency")}</span>
                  </label>
                  <div className="safi-bp-select-wrap">
                    <select
                      id="safi-bp-preauth-country"
                      className="safi-bp-select"
                      value={countryIso2}
                      onChange={(e) => setCountryIso2(e.target.value === "EG" ? "EG" : "US")}
                      data-billing-preauth-select-country="true"
                    >
                      <option value="US">USD — International (Paddle)</option>
                      <option value="EG">EGP — {L("مصر", "Egypt")} (Paymob)</option>
                    </select>
                    <ChevronDown size={16} className="safi-bp-select-chevron" aria-hidden="true" />
                  </div>
                </div>

                {product === "organization_seat_annual" ? (
                  <div className="safi-bp-field">
                    <label className="safi-bp-label" htmlFor="safi-bp-preauth-seats">
                      <span>{L(`المقاعد (≥ ${ORG_MIN_SEATS})`, `Seats (≥ ${ORG_MIN_SEATS})`)}</span>
                    </label>
                    <input
                      id="safi-bp-preauth-seats"
                      type="number"
                      min={ORG_MIN_SEATS}
                      max={BILLING_MAX_ORGANIZATION_SEATS}
                      className="safi-bp-input"
                      value={seats}
                      onChange={(e) => {
                        const next = normalizeOrganizationSeats(Number(e.target.value));
                        setSeats(next);
                        setSeatsInput(String(next));
                      }}
                      data-billing-preauth-seats="true"
                    />
                  </div>
                ) : null}
              </div>
            ) : null}
          </aside>
        </div>
      ) : (
        /* =========================================================================
         * AUTHENTICATED CHECKOUT EXPERIENCE (Account Strip + 2-Column Order Layout)
         * ========================================================================= */
        <div data-billing-authenticated="true" style={{ display: "grid", gap: "18px" }}>
          {/* Account Context Strip */}
          <div className="safi-bp-account-strip">
            <div className="safi-bp-account-meta">
              <span className="safi-bp-account-badge">
                <ShieldCheck size={13} aria-hidden="true" />
                {L("حساب موثّق", "Verified Account")}
              </span>
              <span className="safi-bp-account-email" dir="ltr">
                {effectiveBillingEmail || L("حساب موثّق", "Authenticated Account")}
              </span>
              {entitlements ? (
                <span
                  className="safi-bp-helper"
                  data-billing-current-plan={entitlements.effective_plan}
                >
                  · {L("الخطة الحالية:", "Current plan:")}{" "}
                  <strong>{entitlements.effective_plan}</strong> ({entitlements.state})
                </span>
              ) : null}
            </div>

            <div style={{ display: "flex", flexWrap: "wrap", gap: "8px" }}>
              <button
                type="button"
                className="safi-btn safi-btn-ghost"
                onClick={() => void handleVerifyServerStatus()}
                disabled={verifyingServer}
                data-billing-verify-status="true"
              >
                <RefreshCw
                  size={14}
                  className={verifyingServer ? "safi-bp-spin" : undefined}
                  aria-hidden="true"
                />
                <span>{L("تحديث الحالة من الخادم", "Verify Payment Status")}</span>
              </button>
              {onOpenTeam ? (
                <button
                  type="button"
                  className="safi-btn safi-btn-ghost"
                  onClick={onOpenTeam}
                  data-billing-open-team="true"
                >
                  <Users size={14} aria-hidden="true" />
                  <span>{L("مساحة عمل الفريق (/team)", "Team Workspace (/team)")}</span>
                </button>
              ) : null}
            </div>
          </div>

          {userOrganizations.length > 0 ? (
            <div className="safi-bp-card" data-billing-org-summary="true" style={{ gap: "10px" }}>
              <div className="safi-bp-section-head">
                <strong className="safi-bp-section-title" style={{ fontSize: "0.95rem" }}>
                  <Building2 size={16} aria-hidden="true" />
                  {L(
                    "المؤسسات ومساحات العمل المرتبطة بحسابك",
                    "Organizations Linked to Your Account"
                  )}
                </strong>
              </div>
              <div className="safi-bp-org-list">
                {userOrganizations.map((item) => (
                  <div key={item.organization.id} className="safi-bp-org-item">
                    <span>
                      <strong>{item.organization.name}</strong> · [{item.my_role.toUpperCase()}] ·{" "}
                      {item.active_members_count + item.pending_invitations_count}/{item.paid_seats}{" "}
                      {L("مقاعد", "seats")}
                    </span>
                    <span className="safi-bp-account-badge">
                      {item.teams_enabled ? L("نشط", "Active") : L("غير نشط", "Inactive")}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          ) : null}

          {handoffNotice ? (
            <div
              role="status"
              className="safi-bp-alert safi-bp-alert--success"
              data-billing-handoff-redeemed="true"
            >
              <ShieldCheck size={17} aria-hidden="true" />
              <span>{handoffNotice}</span>
            </div>
          ) : null}

          {handoffError ? (
            <div
              role="alert"
              className="safi-bp-alert safi-bp-alert--danger"
              data-billing-handoff-error="true"
            >
              <AlertTriangle size={17} aria-hidden="true" />
              <span>{handoffError}</span>
            </div>
          ) : null}

          {/* Two-Column Checkout Grid */}
          <div className="safi-bp-grid">
            {/* LEFT / START COLUMN: Plan Selection, Configuration & Billing Information */}
            <div className="safi-bp-main-col">
              {/* Step 1: Plan Selector */}
              <div className="safi-bp-card">
                <div className="safi-bp-section-head">
                  <h2 className="safi-bp-section-title">
                    {L("1. اختر الخطة أو الحزمة", "1. Choose your plan or add-on")}
                  </h2>
                  <span className="safi-bp-helper">
                    {L("الأسعار معتمدة من الخادم", "Server-authoritative pricing")}
                  </span>
                </div>

                <div className="safi-bp-plan-grid">
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
                        className="safi-bp-plan-card"
                        data-billing-plan-option={item.product}
                        data-selected={active ? "true" : "false"}
                        aria-pressed={active}
                        disabled={submittingCheckout}
                        onClick={() => {
                          setProduct(item.product);
                          setCouponPreview(null);
                          setCheckoutError(null);
                        }}
                      >
                        <div className="safi-bp-plan-card__top">
                          <span className="safi-bp-plan-card__name">
                            {isAr ? item.titleAr : item.titleEn}
                          </span>
                          <span className="safi-bp-plan-card__check" aria-hidden="true">
                            <Check size={13} strokeWidth={3} />
                          </span>
                        </div>
                        <span className="safi-bp-plan-card__price">
                          {priceDisplay}
                          {item.perSeat ? (isAr ? " / مقعد / سنة" : " / seat / yr") : ""}
                        </span>
                        <span className="safi-bp-plan-card__meta">
                          {isAr ? item.periodAr : item.periodEn} ·{" "}
                          {isAr ? item.subtitleAr : item.subtitleEn}
                        </span>
                      </button>
                    );
                  })}
                </div>
              </div>

              {/* Step 2: Currency, Seats & Coupon */}
              <div className="safi-bp-card">
                <div className="safi-bp-section-head">
                  <h2 className="safi-bp-section-title">
                    <Globe size={17} aria-hidden="true" />
                    {L("2. العملة، المقاعد ورمز الخصم", "2. Currency, Seats & Coupon")}
                  </h2>
                </div>

                <div className="safi-bp-form-grid">
                  <div className="safi-bp-field">
                    <label className="safi-bp-label" htmlFor="safi-bp-country">
                      <span>{L("بلد الفوترة والعملة", "Billing country & currency")}</span>
                    </label>
                    <div className="safi-bp-select-wrap">
                      <select
                        id="safi-bp-country"
                        className="safi-bp-select"
                        value={countryIso2}
                        disabled={submittingCheckout}
                        onChange={(e) => {
                          setCountryIso2(e.target.value === "EG" ? "EG" : "US");
                          setCouponPreview(null);
                          setCheckoutError(null);
                        }}
                        data-billing-country="true"
                      >
                        <option value="US">
                          {L("دولي — USD (Paddle)", "International — USD (Paddle)")}
                        </option>
                        <option value="EG">
                          {L("مصر — EGP (Paymob)", "Egypt — EGP (Paymob)")}
                        </option>
                      </select>
                      <ChevronDown
                        size={16}
                        className="safi-bp-select-chevron"
                        aria-hidden="true"
                      />
                    </div>
                    <p className="safi-bp-helper">
                      {countryIso2 === "EG"
                        ? L(
                            "الدفع بالجنيه المصري يتم عبر بوابة Paymob الرسمية.",
                            "Egyptian Pound payments process via official Paymob hosted checkout."
                          )
                        : L(
                            "الدفع بالدولار الأمريكي يتم عبر بوابة Paddle الرسمية.",
                            "US Dollar payments process via official Paddle hosted checkout."
                          )}
                    </p>
                  </div>

                  {product === "organization_seat_annual" ? (
                    <div className="safi-bp-field">
                      <label className="safi-bp-label" htmlFor="safi-bp-seats">
                        <span>
                          {L(
                            `عدد المقاعد (الحد الأدنى ${ORG_MIN_SEATS})`,
                            `Organization Seats (min ${ORG_MIN_SEATS})`
                          )}
                        </span>
                      </label>
                      <div className="safi-bp-seat-stepper">
                        <button
                          type="button"
                          className="safi-bp-seat-btn"
                          aria-label={L("تقليل المقاعد", "Decrease seats")}
                          data-billing-seat-dec="true"
                          disabled={submittingCheckout || effectiveSeats <= ORG_MIN_SEATS}
                          onClick={() => handleStepSeats(-1)}
                        >
                          −
                        </button>
                        <input
                          ref={seatsInputRef}
                          id="safi-bp-seats"
                          type="number"
                          inputMode="numeric"
                          min={ORG_MIN_SEATS}
                          max={BILLING_MAX_ORGANIZATION_SEATS}
                          className="safi-bp-input"
                          value={seatsInput}
                          disabled={submittingCheckout}
                          aria-invalid={seatsError ? "true" : "false"}
                          aria-describedby={seatsError ? "safi-bp-err-seats" : "safi-bp-help-seats"}
                          onChange={(e) => handleSeatsInputChange(e.target.value)}
                          onBlur={() => markTouched("seats")}
                          data-billing-seats="true"
                        />
                        <button
                          type="button"
                          className="safi-bp-seat-btn"
                          aria-label={L("زيادة المقاعد", "Increase seats")}
                          data-billing-seat-inc="true"
                          disabled={
                            submittingCheckout || effectiveSeats >= BILLING_MAX_ORGANIZATION_SEATS
                          }
                          onClick={() => handleStepSeats(1)}
                        >
                          +
                        </button>
                      </div>
                      {seatsError ? (
                        <p
                          id="safi-bp-err-seats"
                          role="alert"
                          className="safi-bp-field-error"
                          data-billing-field-error="seats"
                        >
                          <AlertCircle size={14} aria-hidden="true" />
                          <span>{seatsError}</span>
                        </p>
                      ) : (
                        <p id="safi-bp-help-seats" className="safi-bp-helper">
                          {L(
                            `من ${ORG_MIN_SEATS} إلى ${BILLING_MAX_ORGANIZATION_SEATS} مقعدًا في الفاتورة السنوية الواحدة.`,
                            `Supports ${ORG_MIN_SEATS} to ${BILLING_MAX_ORGANIZATION_SEATS} seats per annual subscription.`
                          )}
                        </p>
                      )}
                    </div>
                  ) : null}

                  <div className="safi-bp-field safi-bp-field--full">
                    <label className="safi-bp-label" htmlFor="safi-bp-coupon">
                      <span>
                        <Tag
                          size={13}
                          aria-hidden="true"
                          style={{ verticalAlign: "middle", marginInlineEnd: "6px" }}
                        />
                        {L("كود خصم (اختياري)", "Coupon code (optional)")}
                      </span>
                    </label>
                    <div className="safi-bp-coupon-row">
                      <input
                        id="safi-bp-coupon"
                        type="text"
                        dir="ltr"
                        className="safi-bp-input"
                        value={couponCode}
                        disabled={submittingCheckout || applyingCoupon}
                        aria-invalid={couponError ? "true" : "false"}
                        aria-describedby={couponError ? "safi-bp-err-coupon" : undefined}
                        onChange={(e) => {
                          setCouponCode(e.target.value);
                          setCouponError(null);
                          if (!e.target.value.trim()) setCouponPreview(null);
                        }}
                        placeholder="WELCOME20"
                        data-billing-coupon-input="true"
                      />
                      <button
                        type="button"
                        className="safi-btn safi-btn-ghost"
                        disabled={submittingCheckout || applyingCoupon}
                        onClick={() => void handlePreviewCoupon()}
                        data-billing-coupon-apply="true"
                        style={{ minHeight: "46px" }}
                      >
                        {applyingCoupon ? (
                          <Loader2 size={14} className="safi-bp-spin" aria-hidden="true" />
                        ) : null}
                        <span>{L("تطبيق", "Apply")}</span>
                      </button>
                    </div>
                    {couponError ? (
                      <p
                        id="safi-bp-err-coupon"
                        role="alert"
                        className="safi-bp-field-error"
                        data-billing-field-error="coupon"
                      >
                        <AlertCircle size={14} aria-hidden="true" />
                        <span>{couponError}</span>
                      </p>
                    ) : couponPreview ? (
                      <p
                        className="safi-bp-helper"
                        style={{ color: "var(--status-ok)", fontWeight: 600 }}
                        data-billing-coupon-applied="true"
                      >
                        {L(
                          `تم تطبيق الخصم (${formatMinorPrice(couponPreview.discount_amount_minor, currency)}).`,
                          `Discount applied (${formatMinorPrice(couponPreview.discount_amount_minor, currency)}).`
                        )}
                      </p>
                    ) : null}
                  </div>
                </div>
              </div>

              {/* Step 3: Billing Information (Verified Account Email + Paymob Egypt Fields) */}
              <div className="safi-bp-card">
                <div className="safi-bp-section-head">
                  <h2 className="safi-bp-section-title">
                    <Mail size={17} aria-hidden="true" />
                    {L("3. بيانات الفوترة والحساب", "3. Billing & Account Information")}
                  </h2>
                </div>

                {/* Verified Account Email — Always Visible & Read-Only (Binds Receipt & Entitlements) */}
                <div className="safi-bp-field">
                  <label className="safi-bp-label" htmlFor="safi-bp-verified-email">
                    <span>{L("البريد الإلكتروني للفوترة والحساب", "Billing & Account Email")}</span>
                    <span className="safi-bp-label__tag">
                      <ShieldCheck size={12} aria-hidden="true" />
                      {L("حساب موثّق", "Verified Account")}
                    </span>
                  </label>
                  <input
                    ref={emailInputRef}
                    id="safi-bp-verified-email"
                    type="email"
                    dir="ltr"
                    readOnly
                    className="safi-bp-input"
                    value={effectiveBillingEmail}
                    aria-invalid={emailError ? "true" : "false"}
                    aria-describedby={emailError ? "safi-bp-err-email" : "safi-bp-help-email"}
                    data-billing-verified-email="true"
                  />
                  {emailError ? (
                    <p
                      id="safi-bp-err-email"
                      role="alert"
                      className="safi-bp-field-error"
                      data-billing-field-error="email"
                    >
                      <AlertCircle size={14} aria-hidden="true" />
                      <span>{emailError}</span>
                    </p>
                  ) : (
                    <p id="safi-bp-help-email" className="safi-bp-helper">
                      {countryIso2 === "EG"
                        ? L(
                            "يُربط الاشتراك وإيصال الدفع وبيانات Paymob تلقائيًا ببريد حسابك الموثّق هذا (للقراءة فقط).",
                            "Your subscription, receipt, and Paymob billing record bind automatically to this verified account email (read-only)."
                          )
                        : L(
                            "يُربط الاشتراك وإيصال الدفع تلقائيًا ببريد حسابك الموثّق هذا (للقراءة فقط).",
                            "Your subscription and payment receipt bind automatically to this verified account email (read-only)."
                          )}
                    </p>
                  )}
                </div>

                {/* Paymob Egypt Customer Details (Revealed cleanly when Egypt / EGP is selected) */}
                {countryIso2 === "EG" ? (
                  <div className="safi-bp-paymob-panel" data-billing-paymob-details="true">
                    <p className="safi-bp-section-hint">
                      {L(
                        "مطلوب بواسطة بوابة Paymob مصر لإصدار جلسة الدفع بالجنيه المصري (بياناتك الحقيقية فقط):",
                        "Required by Paymob Egypt to initialize your EGP hosted checkout (real billing details only):"
                      )}
                    </p>

                    <div className="safi-bp-form-grid">
                      <div className="safi-bp-field">
                        <label className="safi-bp-label" htmlFor="safi-bp-first-name">
                          <span>
                            {L("الاسم الأول", "First Name")}{" "}
                            <span className="safi-bp-label__req" aria-hidden="true">
                              *
                            </span>
                          </span>
                        </label>
                        <input
                          ref={firstNameInputRef}
                          id="safi-bp-first-name"
                          type="text"
                          autoComplete="given-name"
                          required
                          className="safi-bp-input"
                          value={firstName}
                          disabled={submittingCheckout}
                          placeholder={L("مثال: محمد", "e.g. Mohamed")}
                          aria-invalid={firstNameError ? "true" : "false"}
                          aria-describedby={firstNameError ? "safi-bp-err-first" : undefined}
                          onChange={(e) => {
                            setFirstName(e.target.value);
                            setCheckoutError(null);
                          }}
                          onBlur={() => markTouched("firstName")}
                          data-billing-first-name="true"
                        />
                        {firstNameError ? (
                          <p
                            id="safi-bp-err-first"
                            role="alert"
                            className="safi-bp-field-error"
                            data-billing-field-error="firstName"
                          >
                            <AlertCircle size={14} aria-hidden="true" />
                            <span>{firstNameError}</span>
                          </p>
                        ) : null}
                      </div>

                      <div className="safi-bp-field">
                        <label className="safi-bp-label" htmlFor="safi-bp-last-name">
                          <span>
                            {L("اسم العائلة", "Last Name")}{" "}
                            <span className="safi-bp-label__req" aria-hidden="true">
                              *
                            </span>
                          </span>
                        </label>
                        <input
                          ref={lastNameInputRef}
                          id="safi-bp-last-name"
                          type="text"
                          autoComplete="family-name"
                          required
                          className="safi-bp-input"
                          value={lastName}
                          disabled={submittingCheckout}
                          placeholder={L("مثال: ربيع", "e.g. Rabei")}
                          aria-invalid={lastNameError ? "true" : "false"}
                          aria-describedby={lastNameError ? "safi-bp-err-last" : undefined}
                          onChange={(e) => {
                            setLastName(e.target.value);
                            setCheckoutError(null);
                          }}
                          onBlur={() => markTouched("lastName")}
                          data-billing-last-name="true"
                        />
                        {lastNameError ? (
                          <p
                            id="safi-bp-err-last"
                            role="alert"
                            className="safi-bp-field-error"
                            data-billing-field-error="lastName"
                          >
                            <AlertCircle size={14} aria-hidden="true" />
                            <span>{lastNameError}</span>
                          </p>
                        ) : null}
                      </div>

                      <div className="safi-bp-field safi-bp-field--full">
                        <label className="safi-bp-label" htmlFor="safi-bp-phone">
                          <span>
                            {L("رقم الهاتف المصري", "Egyptian Mobile Number")}{" "}
                            <span className="safi-bp-label__req" aria-hidden="true">
                              *
                            </span>
                          </span>
                        </label>
                        <input
                          ref={phoneInputRef}
                          id="safi-bp-phone"
                          type="tel"
                          dir="ltr"
                          inputMode="tel"
                          autoComplete="tel"
                          required
                          className="safi-bp-input"
                          value={billingPhone}
                          disabled={submittingCheckout}
                          placeholder="01012345678 / +201012345678"
                          aria-invalid={phoneError ? "true" : "false"}
                          aria-describedby={phoneError ? "safi-bp-err-phone" : "safi-bp-help-phone"}
                          onChange={(e) => {
                            setBillingPhone(e.target.value);
                            setCheckoutError(null);
                          }}
                          onBlur={() => markTouched("phone")}
                          data-billing-phone="true"
                        />
                        {phoneError ? (
                          <p
                            id="safi-bp-err-phone"
                            role="alert"
                            className="safi-bp-field-error"
                            data-billing-field-error="phone"
                          >
                            <AlertCircle size={14} aria-hidden="true" />
                            <span>{phoneError}</span>
                          </p>
                        ) : (
                          <p id="safi-bp-help-phone" className="safi-bp-helper">
                            {L(
                              "يقبل الصيغة المحلية (010/011/012/015) أو الدولية (+201...) ويُنسّق تلقائيًا لـ Paymob.",
                              "Accepts local (010/011/012/015) or international (+201...) Egyptian mobile numbers; normalized automatically for Paymob."
                            )}
                          </p>
                        )}
                      </div>
                    </div>
                  </div>
                ) : null}
              </div>
            </div>

            {/* RIGHT / END COLUMN: Sticky Order Summary, Checkout CTA & Payment Lifecycle */}
            <aside className="safi-bp-summary-col">
              <div className="safi-bp-summary-card" data-billing-summary="true">
                <div className="safi-bp-section-head">
                  <h2 className="safi-bp-section-title">{L("ملخص الطلب", "Order Summary")}</h2>
                  <span className="safi-bp-account-badge">{providerLabel}</span>
                </div>

                <div className="safi-bp-summary-lines">
                  <div className="safi-bp-summary-row">
                    <span className="safi-bp-summary-row__label">{L("الباقة", "Plan")}</span>
                    <span className="safi-bp-summary-row__value">
                      {isAr ? selectedPlan.titleAr : selectedPlan.titleEn}
                    </span>
                  </div>
                  <div className="safi-bp-summary-row">
                    <span className="safi-bp-summary-row__label">
                      {L("دورة الفوترة", "Billing period")}
                    </span>
                    <span className="safi-bp-summary-row__value">
                      {isAr ? selectedPlan.periodAr : selectedPlan.periodEn}
                    </span>
                  </div>
                  {product === "organization_seat_annual" ? (
                    <div className="safi-bp-summary-row">
                      <span className="safi-bp-summary-row__label">{L("المقاعد", "Seats")}</span>
                      <span className="safi-bp-summary-row__value">× {effectiveSeats}</span>
                    </div>
                  ) : null}
                  <div className="safi-bp-summary-row">
                    <span className="safi-bp-summary-row__label">
                      {L("حساب الفوترة", "Billing account")}
                    </span>
                    <span
                      className="safi-bp-summary-row__value"
                      dir="ltr"
                      style={{ maxWidth: "200px", overflow: "hidden", textOverflow: "ellipsis" }}
                    >
                      {effectiveBillingEmail || "—"}
                    </span>
                  </div>
                  <div className="safi-bp-summary-row">
                    <span className="safi-bp-summary-row__label">
                      {L("المجموع الفرعي", "Subtotal")}
                    </span>
                    <span className="safi-bp-summary-row__value">
                      {formatMinorPrice(baseMinor, currency)}
                    </span>
                  </div>
                  {discountMinor > 0 ? (
                    <div className="safi-bp-summary-row safi-bp-summary-row--discount">
                      <span className="safi-bp-summary-row__label">
                        {L("الخصم", "Coupon discount")}
                      </span>
                      <span className="safi-bp-summary-row__value">
                        − {formatMinorPrice(discountMinor, currency)}
                      </span>
                    </div>
                  ) : null}
                </div>

                <div className="safi-bp-total-row">
                  <span className="safi-bp-total-label">{L("الإجمالي المستحق", "Total due")}</span>
                  <strong className="safi-bp-total-value" data-billing-final-price="true">
                    {formatMinorPrice(finalMinor, currency)}
                  </strong>
                </div>

                {checkoutError ? (
                  <div
                    role="alert"
                    className="safi-bp-alert safi-bp-alert--danger"
                    data-billing-checkout-error="true"
                  >
                    <AlertCircle size={16} aria-hidden="true" />
                    <span>{checkoutError}</span>
                  </div>
                ) : null}

                {/* Payment Lifecycle State Display */}
                {orderState === "paid" ? (
                  <div role="status" className="safi-bp-status-card" data-billing-state="paid">
                    <div
                      style={{ display: "flex", alignItems: "center", gap: "8px", fontWeight: 700 }}
                    >
                      <CheckCircle2
                        size={18}
                        aria-hidden="true"
                        style={{ color: "var(--status-ok)", flexShrink: 0 }}
                      />
                      <span>
                        {L(
                          "تم تأكيد الدفع وتفعيل استحقاقك من الخادم بنجاح!",
                          "Payment confirmed by server and entitlements activated!"
                        )}
                      </span>
                    </div>
                    <div className="safi-bp-status-actions">
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
                    className="safi-bp-status-card"
                    data-billing-state="reconciliation_required"
                  >
                    <div
                      style={{ display: "flex", alignItems: "center", gap: "8px", fontWeight: 700 }}
                    >
                      <AlertTriangle
                        size={18}
                        aria-hidden="true"
                        style={{ color: "var(--status-warn)", flexShrink: 0 }}
                      />
                      <span>
                        {L(
                          "مطلوب التحقق من الطلب السابق قبل أي محاولة دفع جديدة (منع الخصم المزدوج).",
                          "Reconciliation required before starting a new charge (duplicate-charge protection)."
                        )}
                      </span>
                    </div>
                    <button
                      type="button"
                      className="safi-btn safi-btn-primary safi-bp-checkout-btn"
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
                    className="safi-bp-status-card"
                    data-billing-state="checkout_pending"
                  >
                    <div style={{ fontSize: "0.88rem", fontWeight: 600, lineHeight: 1.5 }}>
                      {L(
                        "بوابة الدفع جاهزة. أكمل الدفع في صفحة المزود الرسمية ثم اضغط «تحديث الحالة من الخادم».",
                        "Official hosted checkout created. Complete payment on the provider page, then click Verify Payment Status."
                      )}
                    </div>
                    <div className="safi-bp-status-actions">
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
                  <div style={{ display: "grid", gap: "10px" }}>
                    <button
                      type="button"
                      className="safi-btn safi-btn-primary safi-bp-checkout-btn"
                      disabled={submittingCheckout}
                      onClick={() => void executeCheckout(false)}
                      data-billing-pay-submit="true"
                    >
                      {submittingCheckout ? (
                        <Loader2 size={16} className="safi-bp-spin" aria-hidden="true" />
                      ) : (
                        <CreditCard size={16} aria-hidden="true" />
                      )}
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
                        style={{ width: "100%", justifyContent: "center", minHeight: "44px" }}
                      >
                        {L("بدء محاولة جديدة", "Start Fresh Attempt")}
                      </button>
                    ) : null}
                  </div>
                )}

                <div className="safi-bp-security-foot">
                  <Lock size={14} aria-hidden="true" />
                  <span>
                    {L(
                      "دفع محمي بمفاتيح Idempotency لمنع الخصم المزدوج، ولا يتم جمع أو تخزين أي بيانات بطاقة ائتمان على خوادم صافي.",
                      "Protected with stable idempotency keys to prevent duplicate charges. SafiData never collects or stores raw card numbers."
                    )}
                  </span>
                </div>
              </div>
            </aside>
          </div>
        </div>
      )}
    </section>
  );
}
