/**
 * `apps/web/src/pages/OwnerDashboard.tsx`
 *
 * Real SafiData Owner Control Plane (Phases 04 & 05):
 * - Authorized ONLY for the server-confirmed sealed Platform Owner (`isOwner === true`).
 * - Protected by real Google Authenticator-compatible RFC 6238 TOTP MFA (6-digit 30s)
 *   with one-time recovery codes and fresh 5-minute step-up verification for high-risk actions.
 * - Zero hardcoded stats, zero fake audit rows, zero hardware-token stubs, zero hardcoded owner email.
 * - Calm default layout (6 top-level executive metric cards per `i-have-adhd` guidance).
 */
import type { FormEvent, ReactNode } from "react";
import { useCallback, useEffect, useState } from "react";
import {
  Activity,
  AlertTriangle,
  CheckCircle2,
  Crown,
  KeyRound,
  Lock,
  RefreshCw,
  Search,
  ShieldAlert,
  ShieldCheck,
  Sliders,
  UserCheck,
  UserX,
  Wrench
} from "lucide-react";
import type { SafiDockTabId, SafiLang } from "../components/layout/FloatingGlassDock";
import { SafiAuthApiError } from "../lib/auth-api";
import {
  ownerApi,
  type OwnerAccountView,
  type OwnerAnnouncement,
  type OwnerAppRelease,
  type OwnerAuditLogEntry,
  type OwnerBillingEvent,
  type OwnerCommunitySubmission,
  type OwnerCouponRecord,
  type OwnerCustomerProfileResponse,
  type OwnerDeviceRecord,
  type OwnerDeviceReplacementEvent,
  type OwnerFeatureFlag,
  type OwnerMfaStatus,
  type OwnerOfficialTemplate,
  type OwnerOrganizationRow,
  type OwnerStatsResponse,
  type OwnerSupportTicket,
  type TotpEnrollmentBundle
} from "../lib/owner-api";

export interface OwnerDashboardProps {
  readonly lang?: SafiLang;
  readonly onSelectTab?: (tab: SafiDockTabId) => void;
  readonly sessionToken?: string | null;
  readonly isOwner?: boolean;
}

type OwnerPanelTab =
  | "overview"
  | "accounts"
  | "billing"
  | "coupons"
  | "devices"
  | "orgs"
  | "community"
  | "templates"
  | "releases"
  | "system"
  | "audit";

export function OwnerDashboard({
  lang = "en",
  sessionToken = null,
  isOwner = false
}: OwnerDashboardProps): ReactNode {
  const isAr = lang === "ar";
  const [panelTab, setPanelTab] = useState<OwnerPanelTab>("overview");
  const [mfaStatus, setMfaStatus] = useState<OwnerMfaStatus | null>(null);
  const [enrollment, setEnrollment] = useState<TotpEnrollmentBundle | null>(null);
  const [totpCode, setTotpCode] = useState("");
  const [recoveryCode, setRecoveryCode] = useState("");
  const [useRecovery, setUseRecovery] = useState(false);

  const [stats, setStats] = useState<OwnerStatsResponse | null>(null);
  const [accounts, setAccounts] = useState<readonly OwnerAccountView[]>([]);
  const [accountQuery, setAccountQuery] = useState("");
  const [billingEvents, setBillingEvents] = useState<readonly OwnerBillingEvent[]>([]);
  const [featureFlags, setFeatureFlags] = useState<readonly OwnerFeatureFlag[]>([]);
  const [auditEntries, setAuditEntries] = useState<readonly OwnerAuditLogEntry[]>([]);
  const [chainValid, setChainValid] = useState<boolean>(true);
  const [coupons, setCoupons] = useState<readonly OwnerCouponRecord[]>([]);
  const [devices, setDevices] = useState<readonly OwnerDeviceRecord[]>([]);
  const [replacements, setReplacements] = useState<readonly OwnerDeviceReplacementEvent[]>([]);
  const [organizations, setOrganizations] = useState<readonly OwnerOrganizationRow[]>([]);
  const [tickets, setTickets] = useState<readonly OwnerSupportTicket[]>([]);
  const [announcements, setAnnouncements] = useState<readonly OwnerAnnouncement[]>([]);
  const [productConfig, setProductConfig] = useState<Record<string, unknown> | null>(null);
  const [selectedProfile, setSelectedProfile] = useState<OwnerCustomerProfileResponse | null>(null);
  const [communitySubmissions, setCommunitySubmissions] = useState<
    readonly OwnerCommunitySubmission[]
  >([]);
  const [officialTemplates, setOfficialTemplates] = useState<readonly OwnerOfficialTemplate[]>([]);
  const [releases, setReleases] = useState<readonly OwnerAppRelease[]>([]);
  const [reviewNotes, setReviewNotes] = useState("");
  const [relVersion, setRelVersion] = useState("1.0.1");
  const [relUrl, setRelUrl] = useState("https://downloads.safidata.app/SafiData-1.0.1-x64.msi");
  const [relSha256, setRelSha256] = useState("");
  const [relBytes, setRelBytes] = useState("6240768");
  const [relNotes, setRelNotes] = useState("");

  const [newCouponCode, setNewCouponCode] = useState("");
  const [newCouponPct, setNewCouponPct] = useState("20");
  const [newCouponMax, setNewCouponMax] = useState("100");
  const [deviceResetReason, setDeviceResetReason] = useState("");
  const [newAnnTitle, setNewAnnTitle] = useState("");
  const [newAnnBody, setNewAnnBody] = useState("");
  const [newFlagKey, setNewFlagKey] = useState("");
  const [newFlagDesc, setNewFlagDesc] = useState("");
  const [refundInvoiceId, setRefundInvoiceId] = useState("");
  const [maintenanceReason, setMaintenanceReason] = useState("");
  const [stepUpPrompt, setStepUpPrompt] = useState(false);
  const [stepUpCode, setStepUpCode] = useState("");
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<{ kind: "ok" | "err"; text: string } | null>(null);

  const formatApiError = useCallback(
    (err: unknown): string => {
      if (err instanceof SafiAuthApiError) {
        if (err.code === "SF-AUTH-016") {
          setStepUpPrompt(true);
          return isAr
            ? "يلزم إدخال رمز Google Authenticator جديد لتأكيد هذا الإجراء الحساس (SF-AUTH-016)."
            : "Fresh Google Authenticator step-up code required for this high-risk action (SF-AUTH-016).";
        }
        return isAr
          ? `تعذّر تنفيذ الطلب (${err.code ?? `HTTP ${err.status}`}).`
          : `Request rejected (${err.code ?? `HTTP ${err.status}`}).`;
      }
      return isAr ? "تعذّر الاتصال بخادم الإدارة." : "Could not reach the control plane server.";
    },
    [isAr]
  );

  const loadControlPlaneData = useCallback(
    async (token: string): Promise<void> => {
      const [statsRes, accountsRes, billingRes, flagsRes, auditRes] = await Promise.all([
        ownerApi.getStats(token),
        ownerApi.listAccounts(token, accountQuery),
        ownerApi.listBillingEvents(token),
        ownerApi.listFeatureFlags(token),
        ownerApi.listAuditLogs(token)
      ]);
      setStats(statsRes);
      setAccounts(accountsRes);
      setBillingEvents(billingRes);
      setFeatureFlags(flagsRes);
      setAuditEntries(auditRes.entries);
      setChainValid(auditRes.is_chain_valid);

      const extended = await Promise.allSettled([
        ownerApi.listCoupons(token),
        ownerApi.listDevices(token),
        ownerApi.listOrganizations(token),
        ownerApi.listSupportTickets(token),
        ownerApi.listAnnouncements(token),
        ownerApi.getProductConfig(token),
        ownerApi.listCommunitySubmissions(token),
        ownerApi.listOfficialTemplates(token),
        ownerApi.listReleases(token)
      ]);
      if (extended[0].status === "fulfilled") setCoupons(extended[0].value);
      if (extended[1].status === "fulfilled") {
        setDevices(extended[1].value.devices);
        setReplacements(extended[1].value.replacements);
      }
      if (extended[2].status === "fulfilled") setOrganizations(extended[2].value);
      if (extended[3].status === "fulfilled") setTickets(extended[3].value);
      if (extended[4].status === "fulfilled") setAnnouncements(extended[4].value);
      if (extended[5].status === "fulfilled") setProductConfig(extended[5].value);
      if (extended[6].status === "fulfilled") setCommunitySubmissions(extended[6].value);
      if (extended[7].status === "fulfilled") setOfficialTemplates(extended[7].value);
      if (extended[8].status === "fulfilled") setReleases(extended[8].value);
    },
    [accountQuery]
  );

  const refreshAll = useCallback(async (): Promise<void> => {
    if (!sessionToken || !isOwner) return;
    setBusy(true);
    try {
      const status = await ownerApi.getMfaStatus(sessionToken);
      setMfaStatus(status);
      if (status.is_owner && status.elevated_session_active) {
        await loadControlPlaneData(sessionToken);
      }
    } catch (err) {
      setNotice({ kind: "err", text: formatApiError(err) });
    } finally {
      setBusy(false);
    }
  }, [sessionToken, isOwner, loadControlPlaneData, formatApiError]);

  useEffect(() => {
    void refreshAll();
  }, [refreshAll]);

  // Hard client-side deny when not server-authenticated as Owner.
  if (!isOwner || !sessionToken) {
    return (
      <section
        className="safi-panel safi-owner-denied"
        dir={isAr ? "rtl" : "ltr"}
        role="alert"
        data-owner-access="denied"
      >
        <ShieldAlert size={28} aria-hidden="true" />
        <h1>{isAr ? "الوصول محصور بمالك المنصة المعتمد" : "Platform Owner Access Required"}</h1>
        <p>
          {isAr
            ? "هذه اللوحة متاحة فقط داخل تطبيق سطح المكتب بعد التحقق من هوية المالك الداخلية والتحقق الثنائي عبر Google Authenticator (SF-AUTH-013)."
            : "This control plane is restricted to the sealed Platform Owner account inside the Desktop application with Google Authenticator TOTP verification (SF-AUTH-013)."}
        </p>
      </section>
    );
  }

  const handleStartEnroll = async (): Promise<void> => {
    setBusy(true);
    setNotice(null);
    try {
      const bundle = await ownerApi.enrollTotp(sessionToken);
      setEnrollment(bundle);
      setNotice({
        kind: "ok",
        text: isAr
          ? "أضف المفتاح في تطبيق Google Authenticator واحفظ رموز الاسترداد فورًا، ثم أدخل الرمز المكوّن من 6 أرقام للتأكيد."
          : "Add this key to Google Authenticator, store your one-time recovery codes safely, then enter the 6-digit code to confirm."
      });
    } catch (err) {
      setNotice({ kind: "err", text: formatApiError(err) });
    } finally {
      setBusy(false);
    }
  };

  const handleSubmitTotp = async (event: FormEvent): Promise<void> => {
    event.preventDefault();
    setBusy(true);
    setNotice(null);
    try {
      if (useRecovery) {
        await ownerApi.recoverWithCode(sessionToken, recoveryCode.trim());
        setRecoveryCode("");
      } else if (enrollment !== null || (mfaStatus !== null && !mfaStatus.totp_confirmed)) {
        await ownerApi.confirmTotp(sessionToken, totpCode.trim());
        setEnrollment(null);
        setTotpCode("");
      } else {
        await ownerApi.verifyTotp(sessionToken, totpCode.trim());
        setTotpCode("");
      }
      await refreshAll();
      setNotice({
        kind: "ok",
        text: isAr
          ? "تم فتح جلسة مالك المنصة المرتفعة بنجاح."
          : "Elevated Owner session unlocked via Google Authenticator."
      });
    } catch (err) {
      setNotice({ kind: "err", text: formatApiError(err) });
    } finally {
      setBusy(false);
    }
  };

  const handleStepUpSubmit = async (event: FormEvent): Promise<void> => {
    event.preventDefault();
    setBusy(true);
    setNotice(null);
    try {
      await ownerApi.stepUpTotp(sessionToken, stepUpCode.trim());
      setStepUpCode("");
      setStepUpPrompt(false);
      await refreshAll();
      setNotice({
        kind: "ok",
        text: isAr
          ? "تم تأكيد التحقق المرحلي (Step-Up) لمدة 5 دقائق."
          : "Step-up TOTP verified for the next 5 minutes."
      });
    } catch (err) {
      setNotice({ kind: "err", text: formatApiError(err) });
    } finally {
      setBusy(false);
    }
  };

  const runOwnerAction = async (
    action: () => Promise<unknown>,
    successAr: string,
    successEn: string
  ): Promise<void> => {
    setBusy(true);
    setNotice(null);
    try {
      await action();
      await loadControlPlaneData(sessionToken);
      setNotice({ kind: "ok", text: isAr ? successAr : successEn });
    } catch (err) {
      setNotice({ kind: "err", text: formatApiError(err) });
    } finally {
      setBusy(false);
    }
  };

  // MFA Gate: require real Google Authenticator TOTP before displaying live Owner Control Plane.
  if (mfaStatus === null || !mfaStatus.elevated_session_active) {
    return (
      <section
        className="safi-panel safi-owner-mfa-gate"
        dir={isAr ? "rtl" : "ltr"}
        data-owner-mfa="required"
      >
        <header className="safi-owner-mfa-header">
          <Lock size={24} aria-hidden="true" />
          <div>
            <h1>
              {isAr
                ? "التحقق الثنائي لمالك المنصة (Google Authenticator)"
                : "Owner MFA Verification (Google Authenticator)"}
            </h1>
            <p>
              {isAr
                ? "جميع وظائف لوحة المالك محمية برمز TOTP قياسي (6 أرقام كل 30 ثانية) مع حماية ضد إعادة الاستخدام."
                : "All Owner Control Plane operations require a standard RFC 6238 6-digit TOTP code with anti-replay enforcement."}
            </p>
          </div>
        </header>

        {notice !== null ? (
          <div role="status" className={`safi-owner-notice safi-owner-notice--${notice.kind}`}>
            {notice.text}
          </div>
        ) : null}

        {mfaStatus !== null && !mfaStatus.totp_confirmed && enrollment === null ? (
          <div className="safi-owner-enroll-prompt">
            <p>
              {isAr
                ? "لم يتم ربط تطبيق Google Authenticator بعد. ابدأ التفعيل لتوليد المفتاح ورموز الاسترداد."
                : "Google Authenticator is not yet enrolled for this Owner account. Start enrollment to generate your secret and recovery codes."}
            </p>
            <button
              type="button"
              className="safi-btn safi-btn-primary"
              disabled={busy}
              onClick={() => void handleStartEnroll()}
            >
              <KeyRound size={16} aria-hidden="true" />
              <span>{isAr ? "بدء ربط Google Authenticator" : "Enroll Google Authenticator"}</span>
            </button>
          </div>
        ) : null}

        {enrollment !== null ? (
          <div className="safi-owner-enroll-bundle">
            <h2>
              {isAr ? "بيانات الربط (تُعرض مرة واحدة فقط)" : "Enrollment Secret (Shown Once)"}
            </h2>
            <p className="safi-mono" dir="ltr">
              <strong>URI:</strong> {enrollment.otpauth_uri}
            </p>
            <p className="safi-mono" dir="ltr">
              <strong>Manual Key (Base32):</strong> {enrollment.manual_secret_base32}
            </p>
            <div>
              <h3>
                {isAr
                  ? "رموز الاسترداد الاحتياطية (تُستخدم كل منها مرة واحدة فقط):"
                  : "One-Time Recovery Codes (Store Offline):"}
              </h3>
              <ul className="safi-recovery-list" dir="ltr">
                {enrollment.recovery_codes.map((code) => (
                  <li key={code} className="safi-mono">
                    {code}
                  </li>
                ))}
              </ul>
            </div>
          </div>
        ) : null}

        <form className="safi-owner-totp-form" onSubmit={(e) => void handleSubmitTotp(e)}>
          {!useRecovery ? (
            <label>
              <span>
                {isAr
                  ? "أدخل الرمز المكوّن من 6 أرقام من Google Authenticator"
                  : "Enter 6-digit code from Google Authenticator"}
              </span>
              <input
                type="text"
                inputMode="numeric"
                pattern="[0-9]{6}"
                maxLength={6}
                required
                value={totpCode}
                onChange={(e) => setTotpCode(e.currentTarget.value)}
                placeholder="123456"
                dir="ltr"
              />
            </label>
          ) : (
            <label>
              <span>{isAr ? "أدخل رمز استرداد احتياطي" : "Enter one-time recovery code"}</span>
              <input
                type="text"
                required
                value={recoveryCode}
                onChange={(e) => setRecoveryCode(e.currentTarget.value)}
                placeholder="ABCD-1234-EF56"
                dir="ltr"
              />
            </label>
          )}

          <div className="safi-owner-totp-actions">
            <button type="submit" className="safi-btn safi-btn-primary" disabled={busy}>
              <ShieldCheck size={16} aria-hidden="true" />
              <span>{isAr ? "تحقق وافتح لوحة المالك" : "Verify & Unlock Owner Console"}</span>
            </button>
            <button
              type="button"
              className="safi-btn"
              onClick={() => setUseRecovery((v) => !v)}
              disabled={busy}
            >
              {useRecovery
                ? isAr
                  ? "استخدم رمز Google Authenticator"
                  : "Use Google Authenticator code"
                : isAr
                  ? "استخدم رمز استرداد احتياطي"
                  : "Use recovery code"}
            </button>
          </div>
        </form>
      </section>
    );
  }

  return (
    <section className="safi-owner-console" dir={isAr ? "rtl" : "ltr"} data-owner-access="granted">
      <header className="safi-owner-header">
        <div>
          <h1>
            <Crown size={22} aria-hidden="true" />{" "}
            {isAr ? "لوحة تحكم مالك المنصة" : "Platform Owner Control Plane"}
          </h1>
          <p>
            {isAr
              ? "متصلة مباشرة بواجهة الخادم الموثقة — جميع الأرقام والإجراءات وسجل التدقيق حقيقية."
              : "Connected to live backend APIs — all metrics, actions, and BLAKE3 audit logs are authoritative."}
          </p>
        </div>
        <button
          type="button"
          className="safi-btn"
          onClick={() => void refreshAll()}
          disabled={busy}
        >
          <RefreshCw size={15} aria-hidden="true" />
          <span>{isAr ? "تحديث" : "Refresh"}</span>
        </button>
      </header>

      {notice !== null ? (
        <div role="status" className={`safi-owner-notice safi-owner-notice--${notice.kind}`}>
          {notice.text}
        </div>
      ) : null}

      {stepUpPrompt ? (
        <form className="safi-panel safi-owner-stepup" onSubmit={(e) => void handleStepUpSubmit(e)}>
          <strong>
            <AlertTriangle size={16} aria-hidden="true" />{" "}
            {isAr
              ? "مطلوب إعادة تحقق ثنائي (Step-Up) للإجراءات الحساسة"
              : "Step-Up TOTP Required for High-Risk Action"}
          </strong>
          <input
            type="text"
            inputMode="numeric"
            pattern="[0-9]{6}"
            maxLength={6}
            required
            value={stepUpCode}
            onChange={(e) => setStepUpCode(e.currentTarget.value)}
            placeholder="123456"
            dir="ltr"
          />
          <button type="submit" className="safi-btn safi-btn-primary" disabled={busy}>
            {isAr ? "تأكيد الرمز" : "Confirm Step-Up"}
          </button>
        </form>
      ) : null}

      <nav className="safi-owner-tabs" aria-label={isAr ? "أقسام لوحة المالك" : "Owner sections"}>
        {(
          [
            ["overview", isAr ? "نظرة عامة" : "Overview"],
            ["accounts", isAr ? "المستخدمون وVIP" : "Users & Profiles"],
            ["billing", isAr ? "الفوترة والاسترداد" : "Billing & Revenue"],
            ["coupons", isAr ? "الكوبونات والعروض" : "Coupons & Promos"],
            ["devices", isAr ? "الأجهزة والجلسات" : "Devices & Sessions"],
            ["orgs", isAr ? "المؤسسات والدعم" : "Orgs, Support & Notices"],
            ["community", isAr ? "مشاركات المجتمع" : "Community Submissions"],
            ["templates", isAr ? "القوالب الرسمية" : "Official Templates"],
            ["releases", isAr ? "إصدارات التطبيق" : "App Releases"],
            ["system", isAr ? "الإعدادات والأعلام" : "Config & Flags"],
            ["audit", isAr ? "سجل التدقيق" : "Audit Chain"]
          ] as const
        ).map(([id, label]) => (
          <button
            key={id}
            type="button"
            className="safi-btn"
            aria-current={panelTab === id ? "page" : undefined}
            onClick={() => setPanelTab(id)}
          >
            {label}
          </button>
        ))}
      </nav>

      {/* 6 Calm Executive Summary Cards (per i-have-adhd 6-card default) */}
      {panelTab === "overview" && stats !== null ? (
        <div className="safi-owner-kpi-grid" data-owner-kpi-count="6">
          <article className="safi-panel">
            <h3>{isAr ? "إجمالي المستخدمين" : "Total Users"}</h3>
            <p className="safi-kpi-value">{stats.total_users}</p>
            <small>
              {isAr ? `موقوف: ${stats.suspended_users}` : `Suspended: ${stats.suspended_users}`}
            </small>
          </article>
          <article className="safi-panel">
            <h3>{isAr ? "الإيراد الشهري (MRR)" : "Monthly Recurring Revenue"}</h3>
            <p className="safi-kpi-value">
              ${stats.entitlements.mrr_usd} / {stats.entitlements.mrr_egp} EGP
            </p>
            <small>
              ARR: ${stats.entitlements.arr_usd} / {stats.entitlements.arr_egp} EGP
            </small>
          </article>
          <article className="safi-panel">
            <h3>{isAr ? "التجارب والمشتركون" : "Trials & Paid Subscribers"}</h3>
            <p className="safi-kpi-value">
              {stats.entitlements.active_trials} /{" "}
              {stats.entitlements.monthly_subscribers + stats.entitlements.annual_subscribers}
            </p>
            <small>
              {isAr
                ? `مقاعد المؤسسات: ${stats.entitlements.organization_seats}`
                : `Org seats: ${stats.entitlements.organization_seats}`}
            </small>
          </article>
          <article className="safi-panel">
            <h3>{isAr ? "أصدقاء المالك (VIP)" : "Friend VIP Accounts"}</h3>
            <p className="safi-kpi-value">{stats.entitlements.friend_vip_users}</p>
            <small>
              {isAr ? "مجانًا بالكامل + 400 نداء ذكاء اصطناعي" : "$0 full access + 400 AI/mo"}
            </small>
          </article>
          <article className="safi-panel">
            <h3>{isAr ? "حالة بوابات الدفع" : "Payment Providers"}</h3>
            <p className="safi-kpi-value">
              Paddle: {stats.billing_providers.paddle} | Paymob: {stats.billing_providers.paymob}
            </p>
            <small>
              {isAr
                ? `وضع الصيانة: ${stats.maintenance_mode ? "مفعل" : "متوقف"}`
                : `Maintenance: ${stats.maintenance_mode ? "ON" : "OFF"}`}
            </small>
          </article>
          <article className="safi-panel">
            <h3>{isAr ? "استهلاك المنصة" : "Platform Usage"}</h3>
            <p className="safi-kpi-value">{stats.entitlements.rows_cleaned_total}</p>
            <small>
              {isAr
                ? `صفوف منظفة | AI: ${stats.entitlements.ai_calls_used_total}`
                : `Rows cleaned | AI calls: ${stats.entitlements.ai_calls_used_total}`}
            </small>
          </article>
        </div>
      ) : null}

      {panelTab === "accounts" ? (
        <section className="safi-panel">
          <div className="safi-owner-search-row">
            <Search size={16} aria-hidden="true" />
            <input
              type="search"
              value={accountQuery}
              onChange={(e) => setAccountQuery(e.currentTarget.value)}
              placeholder={isAr ? "بحث بالبريد أو المعرف..." : "Search by email or user ID..."}
            />
            <button
              type="button"
              className="safi-btn"
              onClick={() => void loadControlPlaneData(sessionToken)}
              disabled={busy}
            >
              {isAr ? "بحث" : "Search"}
            </button>
          </div>
          {accounts.length === 0 ? (
            <p>{isAr ? "لا توجد حسابات مطابقة بعد." : "No matching accounts found."}</p>
          ) : (
            <table className="safi-owner-table">
              <thead>
                <tr>
                  <th>{isAr ? "الحساب" : "Account"}</th>
                  <th>{isAr ? "الخطة" : "Plan"}</th>
                  <th>{isAr ? "الحالة" : "Status"}</th>
                  <th>{isAr ? "إجراءات المالك" : "Owner Actions"}</th>
                </tr>
              </thead>
              <tbody>
                {accounts.map((row) => (
                  <tr key={row.account.user_id}>
                    <td>
                      <strong>{row.account.email}</strong>
                      <div className="safi-mono">{row.account.user_id}</div>
                    </td>
                    <td>{row.entitlements.effective_plan}</td>
                    <td>
                      {row.account.suspended
                        ? isAr
                          ? "موقوف"
                          : "Suspended"
                        : isAr
                          ? "نشط"
                          : "Active"}
                    </td>
                    <td className="safi-owner-row-actions">
                      <button
                        type="button"
                        className="safi-btn"
                        disabled={busy}
                        onClick={() => {
                          void ownerApi
                            .getCustomerProfile(sessionToken, row.account.user_id)
                            .then((prof) => setSelectedProfile(prof))
                            .catch((err: unknown) =>
                              setNotice({ kind: "err", text: formatApiError(err) })
                            );
                        }}
                      >
                        {isAr ? "الملف" : "Profile"}
                      </button>
                      {row.entitlements.effective_plan !== "friend_vip" ? (
                        <button
                          type="button"
                          className="safi-btn"
                          disabled={busy}
                          onClick={() =>
                            void runOwnerAction(
                              () =>
                                ownerApi.grantFriendVip(
                                  sessionToken,
                                  row.account.user_id,
                                  row.account.workspace_id,
                                  "Owner console grant"
                                ),
                              "تم منح صلاحية Friend VIP.",
                              "Friend VIP granted."
                            )
                          }
                        >
                          <Crown size={14} aria-hidden="true" /> VIP
                        </button>
                      ) : (
                        <button
                          type="button"
                          className="safi-btn"
                          disabled={busy}
                          onClick={() =>
                            void runOwnerAction(
                              () =>
                                ownerApi.revokeFriendVip(
                                  sessionToken,
                                  row.account.user_id,
                                  row.account.workspace_id
                                ),
                              "تم سحب صلاحية Friend VIP.",
                              "Friend VIP revoked."
                            )
                          }
                        >
                          {isAr ? "سحب VIP" : "Revoke VIP"}
                        </button>
                      )}
                      <button
                        type="button"
                        className="safi-btn"
                        disabled={busy}
                        onClick={() =>
                          void runOwnerAction(
                            () =>
                              ownerApi.extendTrial(
                                sessionToken,
                                row.account.workspace_id,
                                row.account.user_id,
                                7
                              ),
                            "تم تمديد التجربة 7 أيام.",
                            "Trial extended by 7 days."
                          )
                        }
                      >
                        +7d Trial
                      </button>
                      {!row.account.suspended ? (
                        <button
                          type="button"
                          className="safi-btn"
                          disabled={busy}
                          onClick={() =>
                            void runOwnerAction(
                              () => ownerApi.suspendAccount(sessionToken, row.account.user_id),
                              "تم إيقاف الحساب.",
                              "Account suspended."
                            )
                          }
                        >
                          <UserX size={14} aria-hidden="true" /> {isAr ? "إيقاف" : "Suspend"}
                        </button>
                      ) : (
                        <button
                          type="button"
                          className="safi-btn"
                          disabled={busy}
                          onClick={() =>
                            void runOwnerAction(
                              () => ownerApi.reactivateAccount(sessionToken, row.account.user_id),
                              "تمت إعادة تفعيل الحساب.",
                              "Account reactivated."
                            )
                          }
                        >
                          <UserCheck size={14} aria-hidden="true" /> {isAr ? "تفعيل" : "Reactivate"}
                        </button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </section>
      ) : null}

      {panelTab === "billing" ? (
        <section className="safi-panel">
          <h2>{isAr ? "استرداد فاتورة (سياسة 3 أيام)" : "Issue Refund (3-Day Policy Window)"}</h2>
          <div className="safi-owner-inline-form">
            <input
              type="text"
              value={refundInvoiceId}
              onChange={(e) => setRefundInvoiceId(e.currentTarget.value)}
              placeholder={isAr ? "رقم الفاتورة الخارجي..." : "External invoice ID..."}
              dir="ltr"
            />
            <button
              type="button"
              className="safi-btn safi-btn-primary"
              disabled={busy || refundInvoiceId.trim().length === 0}
              onClick={() =>
                void runOwnerAction(
                  () => ownerApi.issueRefund(sessionToken, refundInvoiceId.trim(), false),
                  "تم تنفيذ الاسترداد.",
                  "Refund issued."
                )
              }
            >
              {isAr ? "تنفيذ الاسترداد" : "Issue Refund"}
            </button>
          </div>
          <h3>{isAr ? "أحداث الفوترة (Paddle / Paymob)" : "Billing Events (Paddle / Paymob)"}</h3>
          {billingEvents.length === 0 ? (
            <p>
              {isAr ? "لا توجد أحداث فوترة مسجلة بعد." : "No billing webhook events recorded yet."}
            </p>
          ) : (
            <ul>
              {billingEvents.map((evt) => (
                <li key={evt.id}>
                  <strong>{evt.provider}</strong> — {evt.event_type} ({evt.outcome})
                </li>
              ))}
            </ul>
          )}
        </section>
      ) : null}

      {panelTab === "system" ? (
        <section className="safi-panel">
          <h2>
            <Wrench size={18} aria-hidden="true" />{" "}
            {isAr ? "وضع الصيانة الطارئ" : "Maintenance Mode"}
          </h2>
          <div className="safi-owner-inline-form">
            <input
              type="text"
              value={maintenanceReason}
              onChange={(e) => setMaintenanceReason(e.currentTarget.value)}
              placeholder={isAr ? "سبب الصيانة..." : "Maintenance reason..."}
            />
            <button
              type="button"
              className="safi-btn safi-btn-primary"
              disabled={busy}
              onClick={() =>
                void runOwnerAction(
                  () =>
                    ownerApi.setMaintenance(
                      sessionToken,
                      !(stats?.maintenance_mode ?? false),
                      maintenanceReason
                    ),
                  "تم تحديث وضع الصيانة.",
                  "Maintenance mode updated."
                )
              }
            >
              {stats?.maintenance_mode
                ? isAr
                  ? "إيقاف الصيانة"
                  : "Disable Maintenance"
                : isAr
                  ? "تفعيل الصيانة"
                  : "Enable Maintenance"}
            </button>
          </div>

          <h2>
            <Sliders size={18} aria-hidden="true" /> {isAr ? "أعلام الميزات" : "Feature Flags"}
          </h2>
          <div className="safi-owner-inline-form">
            <input
              type="text"
              value={newFlagKey}
              onChange={(e) => setNewFlagKey(e.currentTarget.value)}
              placeholder="flag_key"
              dir="ltr"
            />
            <input
              type="text"
              value={newFlagDesc}
              onChange={(e) => setNewFlagDesc(e.currentTarget.value)}
              placeholder={isAr ? "الوصف" : "Description"}
            />
            <button
              type="button"
              className="safi-btn"
              disabled={busy || newFlagKey.trim().length === 0}
              onClick={() =>
                void runOwnerAction(
                  () => ownerApi.setFeatureFlag(sessionToken, newFlagKey.trim(), true, newFlagDesc),
                  "تم حفظ علم الميزة.",
                  "Feature flag saved."
                )
              }
            >
              {isAr ? "حفظ" : "Save"}
            </button>
          </div>
          <ul>
            {featureFlags.map((flag) => (
              <li key={flag.key}>
                <strong>{flag.key}</strong>: {flag.enabled ? "ON" : "OFF"} — {flag.description}
              </li>
            ))}
          </ul>
          {productConfig !== null ? (
            <div>
              <h3>{isAr ? "إعدادات المنتج والتفويض" : "Product & Authorization Config"}</h3>
              <pre className="safi-mono">{JSON.stringify(productConfig, null, 2)}</pre>
            </div>
          ) : null}
        </section>
      ) : null}

      {panelTab === "coupons" ? (
        <section className="safi-panel">
          <h2>{isAr ? "إدارة الكوبونات والعروض" : "Promotions & Coupons Management"}</h2>
          <div className="safi-owner-inline-form">
            <input
              type="text"
              value={newCouponCode}
              onChange={(e) => setNewCouponCode(e.currentTarget.value)}
              placeholder="LAUNCH25"
              dir="ltr"
            />
            <input
              type="number"
              min={1}
              max={100}
              value={newCouponPct}
              onChange={(e) => setNewCouponPct(e.currentTarget.value)}
              placeholder="20%"
              dir="ltr"
            />
            <input
              type="number"
              min={1}
              value={newCouponMax}
              onChange={(e) => setNewCouponMax(e.currentTarget.value)}
              placeholder="Max redemptions"
              dir="ltr"
            />
            <button
              type="button"
              className="safi-btn safi-btn-primary"
              disabled={busy || newCouponCode.trim().length < 3}
              onClick={() =>
                void runOwnerAction(
                  () =>
                    ownerApi.createCoupon(sessionToken, {
                      code: newCouponCode.trim().toUpperCase(),
                      discount_kind: "percent",
                      discount_value: Math.max(1, Math.min(100, Number(newCouponPct) || 10)),
                      max_redemptions: Math.max(1, Number(newCouponMax) || 100)
                    }),
                  "تم إنشاء الكوبون بنجاح.",
                  "Coupon created."
                )
              }
            >
              {isAr ? "إنشاء كوبون" : "Create Coupon"}
            </button>
          </div>
          {coupons.length === 0 ? (
            <p>{isAr ? "لا توجد كوبونات مسجلة بعد." : "No coupons created yet."}</p>
          ) : (
            <ul>
              {coupons.map((c) => (
                <li key={c.id}>
                  <strong>{c.code}</strong> ({c.discount_kind}: {c.discount_value}) —{" "}
                  {c.redemptions_count}/{c.max_redemptions ?? "∞"} —{" "}
                  {c.enabled ? (isAr ? "مفعل" : "Enabled") : isAr ? "معطل" : "Disabled"}{" "}
                  <button
                    type="button"
                    className="safi-btn"
                    disabled={busy}
                    onClick={() =>
                      void runOwnerAction(
                        () => ownerApi.updateCoupon(sessionToken, c.code, { enabled: !c.enabled }),
                        "تم تحديث حالة الكوبون.",
                        "Coupon status updated."
                      )
                    }
                  >
                    {c.enabled ? (isAr ? "تعطيل" : "Disable") : isAr ? "تفعيل" : "Enable"}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </section>
      ) : null}

      {panelTab === "devices" ? (
        <section className="safi-panel">
          <h2>
            {isAr
              ? "إدارة الأجهزة النشطة وإعادة التعيين المدققة"
              : "Active Devices & Audited Device Reset"}
          </h2>
          <div className="safi-owner-inline-form">
            <input
              type="text"
              value={deviceResetReason}
              onChange={(e) => setDeviceResetReason(e.currentTarget.value)}
              placeholder={
                isAr ? "سبب إعادة تعيين الجهاز (إلزامي)" : "Device reset reason (required)"
              }
            />
          </div>
          {devices.length === 0 ? (
            <p>{isAr ? "لا توجد أجهزة مسجلة بعد." : "No registered devices yet."}</p>
          ) : (
            <ul>
              {devices.map((d) => (
                <li key={d.device_id}>
                  <strong>{d.device_name}</strong> — {d.status} (gen #{d.session_generation}){" "}
                  {d.status === "active" ? (
                    <button
                      type="button"
                      className="safi-btn"
                      disabled={busy || deviceResetReason.trim().length === 0}
                      onClick={() =>
                        void runOwnerAction(
                          () =>
                            ownerApi.resetAccountDevices(
                              sessionToken,
                              d.user_id,
                              deviceResetReason.trim()
                            ),
                          "تم إلغاء الجهاز وإعادة التعيين بنجاح.",
                          "Account devices reset and audited."
                        )
                      }
                    >
                      {isAr ? "إلغاء الجهاز" : "Reset Device"}
                    </button>
                  ) : null}
                </li>
              ))}
            </ul>
          )}
          <h3>{isAr ? "سجل استبدال الأجهزة" : "Device Replacement Audit History"}</h3>
          <ul>
            {replacements.map((ev) => (
              <li key={ev.id}>
                <strong>{ev.trigger_kind}</strong> — {ev.reason} (gen #{ev.new_session_generation})
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      {panelTab === "orgs" ? (
        <section className="safi-panel">
          <h2>
            {isAr ? "المؤسسات وتذاكر الدعم والإعلانات" : "Organizations, Support & Announcements"}
          </h2>
          <h3>
            {isAr ? "المؤسسات" : "Organizations"} ({organizations.length})
          </h3>
          <ul>
            {organizations.map((row) => (
              <li key={row.organization.id}>
                <strong>{row.organization.name}</strong> — {row.active_members}/
                {row.organization.purchased_seats} seats
              </li>
            ))}
          </ul>
          <h3>
            {isAr ? "تذاكر الدعم" : "Support Tickets"} ({tickets.length})
          </h3>
          <ul>
            {tickets.map((t) => (
              <li key={t.id}>
                <strong>{t.subject}</strong> [{t.status} / {t.priority}] — {t.body}{" "}
                {t.status !== "resolved" ? (
                  <button
                    type="button"
                    className="safi-btn"
                    disabled={busy}
                    onClick={() =>
                      void runOwnerAction(
                        () =>
                          ownerApi.updateSupportTicket(sessionToken, t.id, { status: "resolved" }),
                        "تم إغلاق التذكرة.",
                        "Ticket marked resolved."
                      )
                    }
                  >
                    {isAr ? "تم الحل" : "Resolve"}
                  </button>
                ) : null}
              </li>
            ))}
          </ul>
          <h3>{isAr ? "الإعلانات والتحديثات" : "Announcements & Updates"}</h3>
          <div className="safi-owner-inline-form">
            <input
              type="text"
              value={newAnnTitle}
              onChange={(e) => setNewAnnTitle(e.currentTarget.value)}
              placeholder={isAr ? "عنوان الإعلان" : "Announcement title"}
            />
            <input
              type="text"
              value={newAnnBody}
              onChange={(e) => setNewAnnBody(e.currentTarget.value)}
              placeholder={isAr ? "نص الإعلان" : "Announcement body"}
            />
            <button
              type="button"
              className="safi-btn"
              disabled={busy || newAnnTitle.trim().length === 0 || newAnnBody.trim().length === 0}
              onClick={() =>
                void runOwnerAction(
                  () =>
                    ownerApi.createAnnouncement(
                      sessionToken,
                      newAnnTitle.trim(),
                      newAnnBody.trim()
                    ),
                  "تم نشر الإعلان.",
                  "Announcement published."
                )
              }
            >
              {isAr ? "نشر" : "Publish"}
            </button>
          </div>
          <ul>
            {announcements.map((a) => (
              <li key={a.id}>
                <strong>{a.title}</strong> ({a.severity}) — {a.body}
              </li>
            ))}
          </ul>
        </section>
      ) : null}

      {selectedProfile !== null ? (
        <section className="safi-panel">
          <h3>
            {isAr ? "ملف العميل التفصيلي:" : "Customer Profile:"} {selectedProfile.account.email}
          </h3>
          <p>
            Plan: {selectedProfile.entitlements.effective_plan} | Devices:{" "}
            {selectedProfile.devices.length} | Invoices: {selectedProfile.invoices.length}
          </p>
          <button type="button" className="safi-btn" onClick={() => setSelectedProfile(null)}>
            {isAr ? "إغلاق الملف" : "Close Profile"}
          </button>
        </section>
      ) : null}

      {panelTab === "community" ? (
        <section className="safi-panel" data-owner-section="community-submissions">
          <h3>{isAr ? "مشاركات القوالب من المجتمع" : "Community Template Submissions"}</h3>
          <p style={{ fontSize: "13px", color: "var(--text-secondary)" }}>
            {isAr
              ? "مراجعة المساهمات المرسلة بموافقة صريحة من المستخدمين. النشر للمكتبة الرسمية يتطلب قرارًا صريحًا منفصلًا."
              : "Review explicitly shared user recipes. Publishing to the Official Template Library requires a separate explicit Owner publish action."}
          </p>
          <div style={{ marginBottom: "12px" }}>
            <input
              type="text"
              className="safi-input"
              value={reviewNotes}
              onChange={(e) => setReviewNotes(e.target.value)}
              placeholder={isAr ? "ملاحظات المراجعة..." : "Reviewer notes..."}
            />
          </div>
          {communitySubmissions.length === 0 ? (
            <p>{isAr ? "لا توجد مشاركات مجتمع بعد." : "No community submissions yet."}</p>
          ) : (
            <ul style={{ display: "grid", gap: "10px", listStyle: "none", padding: 0 }}>
              {communitySubmissions.map((sub) => (
                <li
                  key={sub.id}
                  className="safi-panel"
                  style={{ display: "grid", gap: "6px" }}
                  data-community-submission-id={sub.id}
                >
                  <div>
                    <strong>{sub.name}</strong> ({sub.recipe_type}) — <code>{sub.status}</code>
                  </div>
                  <div style={{ fontSize: "12px", color: "var(--text-secondary)" }}>
                    {sub.description}
                  </div>
                  {sub.pii_warnings.length > 0 ? (
                    <div style={{ fontSize: "12px", color: "var(--safi-warning, #f59e0b)" }}>
                      {isAr ? "تحذيرات الفحص:" : "Scanner warnings:"} {sub.pii_warnings.join(" | ")}
                    </div>
                  ) : null}
                  <div style={{ display: "flex", gap: "8px", flexWrap: "wrap" }}>
                    <button
                      type="button"
                      className="safi-btn"
                      onClick={() =>
                        void runOwnerAction(
                          () =>
                            ownerApi.reviewCommunitySubmission(
                              sessionToken,
                              sub.id,
                              "approved",
                              reviewNotes.trim() || "Approved by Owner"
                            ),
                          "تمت الموافقة على المساهمة.",
                          "Submission approved."
                        )
                      }
                    >
                      {isAr ? "موافقة" : "Approve"}
                    </button>
                    <button
                      type="button"
                      className="safi-btn"
                      onClick={() =>
                        void runOwnerAction(
                          () =>
                            ownerApi.reviewCommunitySubmission(
                              sessionToken,
                              sub.id,
                              "rejected",
                              reviewNotes.trim() || "Rejected by Owner"
                            ),
                          "تم رفض المساهمة.",
                          "Submission rejected."
                        )
                      }
                    >
                      {isAr ? "رفض" : "Reject"}
                    </button>
                    {sub.status === "approved" ? (
                      <button
                        type="button"
                        className="safi-btn safi-btn-primary"
                        onClick={() =>
                          void runOwnerAction(
                            () =>
                              ownerApi.publishCommunitySubmission(sessionToken, sub.id, {
                                slug: `community-${sub.id.slice(0, 8)}`,
                                official_name: sub.name,
                                official_description: sub.description,
                                category: sub.recipe_type,
                                release_notes: reviewNotes.trim() || "Published to Official Library"
                              }),
                            "تم النشر في المكتبة الرسمية.",
                            "Published to Official Template Library."
                          )
                        }
                      >
                        {isAr ? "نشر في المكتبة الرسمية" : "Publish to Official Library"}
                      </button>
                    ) : null}
                  </div>
                </li>
              ))}
            </ul>
          )}
        </section>
      ) : null}

      {panelTab === "templates" ? (
        <section className="safi-panel" data-owner-section="official-templates">
          <h3>
            {isAr ? "مكتبة القوالب الرسمية (الإصدارات والتراجع)" : "Official Template Library"}
          </h3>
          {officialTemplates.length === 0 ? (
            <p>{isAr ? "لا توجد قوالب رسمية." : "No official templates loaded."}</p>
          ) : (
            <ul style={{ display: "grid", gap: "10px", listStyle: "none", padding: 0 }}>
              {officialTemplates.map((tpl) => (
                <li
                  key={tpl.id}
                  className="safi-panel"
                  style={{
                    display: "flex",
                    justifyContent: "space-between",
                    flexWrap: "wrap",
                    gap: "10px"
                  }}
                >
                  <div>
                    <strong>{tpl.name}</strong> (<code>{tpl.slug}</code>) • v{tpl.current_version} •{" "}
                    <span>
                      {tpl.active ? (isAr ? "نشط" : "Active") : isAr ? "موقوف" : "Inactive"}
                    </span>
                    <div style={{ fontSize: "12px", color: "var(--text-secondary)" }}>
                      {tpl.description}
                    </div>
                  </div>
                  <div style={{ display: "flex", gap: "8px", alignItems: "center" }}>
                    <button
                      type="button"
                      className="safi-btn"
                      onClick={() =>
                        void runOwnerAction(
                          () =>
                            ownerApi.setOfficialTemplateStatus(sessionToken, tpl.id, !tpl.active),
                          "تم تحديث حالة القالب.",
                          "Template status updated."
                        )
                      }
                    >
                      {tpl.active ? (isAr ? "تعطيل" : "Deactivate") : isAr ? "تفعيل" : "Activate"}
                    </button>
                    {tpl.current_version > 1 ? (
                      <button
                        type="button"
                        className="safi-btn"
                        onClick={() =>
                          void runOwnerAction(
                            () =>
                              ownerApi.rollbackOfficialTemplate(
                                sessionToken,
                                tpl.id,
                                tpl.current_version - 1
                              ),
                            "تم التراجع إلى الإصدار السابق.",
                            "Rolled back to previous template version."
                          )
                        }
                      >
                        {isAr ? "تراجع إصدار" : "Rollback Version"}
                      </button>
                    ) : null}
                  </div>
                </li>
              ))}
            </ul>
          )}
        </section>
      ) : null}

      {panelTab === "releases" ? (
        <section className="safi-panel" data-owner-section="app-releases">
          <h3>
            {isAr
              ? "إدارة إصدارات تطبيق سطح المكتب الموقّعة"
              : "Signed Desktop Application Releases"}
          </h3>
          <div style={{ display: "grid", gap: "8px", marginBottom: "14px", maxWidth: "560px" }}>
            <input
              type="text"
              className="safi-input"
              value={relVersion}
              onChange={(e) => setRelVersion(e.target.value)}
              placeholder="1.0.1"
            />
            <input
              type="url"
              className="safi-input"
              value={relUrl}
              onChange={(e) => setRelUrl(e.target.value)}
              placeholder="https://downloads.safidata.app/..."
            />
            <input
              type="text"
              className="safi-input"
              value={relSha256}
              onChange={(e) => setRelSha256(e.target.value)}
              placeholder="64-char SHA-256 hex"
            />
            <input
              type="number"
              className="safi-input"
              value={relBytes}
              onChange={(e) => setRelBytes(e.target.value)}
              placeholder="File size bytes"
            />
            <input
              type="text"
              className="safi-input"
              value={relNotes}
              onChange={(e) => setRelNotes(e.target.value)}
              placeholder={isAr ? "ملاحظات الإصدار..." : "Release notes..."}
            />
            <button
              type="button"
              className="safi-btn safi-btn-primary"
              onClick={() =>
                void runOwnerAction(
                  () =>
                    ownerApi.publishRelease(sessionToken, {
                      version_code: releases.length + 2,
                      version_name: relVersion.trim(),
                      channel: "stable",
                      platform: "windows-x86_64",
                      download_url: relUrl.trim(),
                      file_size_bytes: Number.parseInt(relBytes, 10) || 1,
                      sha256_hex: relSha256.trim().toLowerCase(),
                      release_notes: relNotes.trim()
                    }),
                  "تم نشر الإصدار الموقّع.",
                  "Signed release published."
                )
              }
            >
              {isAr ? "نشر إصدار جديد" : "Publish Release"}
            </button>
          </div>
          {releases.length === 0 ? (
            <p>
              {isAr
                ? "لا توجد إصدارات منشورة في قاعدة البيانات بعد."
                : "No releases published yet."}
            </p>
          ) : (
            <ul style={{ display: "grid", gap: "8px", listStyle: "none", padding: 0 }}>
              {releases.map((rel) => (
                <li key={rel.id} className="safi-panel">
                  <strong>v{rel.version_name}</strong> ({rel.channel} / {rel.platform}) —{" "}
                  <code>{rel.sha256_hex.slice(0, 16)}…</code> ({rel.file_size_bytes} bytes)
                </li>
              ))}
            </ul>
          )}
        </section>
      ) : null}

      {panelTab === "audit" ? (
        <section className="safi-panel">
          <p role="status">
            {chainValid ? (
              <CheckCircle2 size={16} aria-hidden="true" />
            ) : (
              <AlertTriangle size={16} aria-hidden="true" />
            )}{" "}
            {isAr
              ? `سلامة سلسلة تدقيق BLAKE3: ${chainValid ? "سليمة" : "تحذير!"}`
              : `BLAKE3 Audit Chain Integrity: ${chainValid ? "VALID" : "TAMPER ALERT"}`}
          </p>
          {auditEntries.length === 0 ? (
            <p>{isAr ? "لا توجد سجلات تدقيق بعد." : "No audit entries recorded yet."}</p>
          ) : (
            <ul className="safi-owner-audit-list">
              {auditEntries.map((entry) => (
                <li key={entry.id}>
                  <Activity size={14} aria-hidden="true" />{" "}
                  <span className="safi-mono">#{entry.seq}</span> <strong>{entry.action}</strong> (
                  {entry.outcome}) — <span className="safi-mono">{entry.timestamp}</span>
                </li>
              ))}
            </ul>
          )}
        </section>
      ) : null}
    </section>
  );
}
