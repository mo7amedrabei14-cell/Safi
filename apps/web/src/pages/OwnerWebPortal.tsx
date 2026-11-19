/**
 * `apps/web/src/pages/OwnerWebPortal.tsx`
 *
 * Protected Website Owner Administration Portal (`/owner`):
 * - Uses the dedicated Owner Web Session (`POST /v1/owner/web/login` -> `sfw_*`), which
 *   NEVER counts as a second Desktop installation and NEVER revokes the Owner's Desktop device.
 * - Regular users are strictly rejected by the backend (`SF-AUTH-008` / HTTP 403).
 * - Immediately gates central management behind Google Authenticator RFC 6238 TOTP MFA
 *   via the shared `OwnerDashboard` component using the exact same backend APIs as Desktop.
 */
import type { FormEvent, ReactNode } from "react";
import { useState } from "react";
import { Lock, LogOut, ShieldCheck } from "lucide-react";
import type { SafiLang } from "../components/layout/FloatingGlassDock";
import { SafiAuthApiError } from "../lib/auth-api";
import { ownerApi } from "../lib/owner-api";
import { OwnerDashboard } from "./OwnerDashboard";

export interface OwnerWebPortalProps {
  readonly lang?: SafiLang;
  readonly onBackToShowcase?: () => void;
}

export function OwnerWebPortal({ lang = "en", onBackToShowcase }: OwnerWebPortalProps): ReactNode {
  const isAr = lang === "ar";
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [webSessionToken, setWebSessionToken] = useState<string | null>(null);
  const [ownerEmail, setOwnerEmail] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleLogin = async (event: FormEvent): Promise<void> => {
    event.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const resp = await ownerApi.ownerWebLogin(email.trim(), password);
      if (!resp.is_owner) {
        setError(
          isAr
            ? "هذا المدخل مخصص لمالك منصة صافي البيانات فقط (SF-AUTH-008)."
            : "This portal is restricted to the sealed Platform Owner account only (SF-AUTH-008)."
        );
        return;
      }
      setWebSessionToken(resp.token);
      setOwnerEmail(resp.email);
      setPassword("");
    } catch (err) {
      if (err instanceof SafiAuthApiError) {
        if (err.code === "SF-AUTH-013" || err.code === "SF-AUTH-008" || err.status === 403) {
          setError(
            isAr
              ? "الوصول مرفوض: هذه البوابة مخصصة لحساب المالك فقط (SF-AUTH-013)."
              : "Access denied: this portal is restricted exclusively to the Platform Owner (SF-AUTH-013)."
          );
        } else {
          setError(
            isAr
              ? `تعذّر تسجيل الدخول (${err.code ?? `HTTP ${err.status}`}).`
              : `Sign in failed (${err.code ?? `HTTP ${err.status}`}).`
          );
        }
      } else {
        setError(
          isAr
            ? "تعذّر الاتصال بخادم الإدارة المركزي."
            : "Could not reach the central administration server."
        );
      }
    } finally {
      setBusy(false);
    }
  };

  const handleSignOut = async (): Promise<void> => {
    const token = webSessionToken;
    setWebSessionToken(null);
    setOwnerEmail(null);
    if (token) {
      try {
        await ownerApi.ownerWebLogout(token);
      } catch {
        // Local web session already cleared.
      }
    }
  };

  if (webSessionToken !== null) {
    return (
      <div className="safi-owner-web-portal" data-owner-web-portal="authenticated">
        <div
          className="safi-panel"
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            flexWrap: "wrap",
            gap: "12px",
            marginBottom: "16px"
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
            <ShieldCheck size={20} aria-hidden="true" />
            <div>
              <strong>
                {isAr
                  ? "بوابة المالك المركزية على الويب (جلسة ويب معزولة)"
                  : "Central Owner Web Portal (Isolated Web Session)"}
              </strong>
              <div style={{ fontSize: "12px", color: "var(--text-secondary)" }}>
                {ownerEmail ?? ""} •{" "}
                {isAr
                  ? "لا تؤثر على جهاز سطح المكتب النشط ولا تعيد تعيينه"
                  : "Does not revoke or count against your active Desktop installation"}
              </div>
            </div>
          </div>
          <button
            type="button"
            className="safi-btn safi-btn-secondary"
            data-owner-web-logout="true"
            onClick={() => void handleSignOut()}
          >
            <LogOut size={15} aria-hidden="true" />
            <span>{isAr ? "تسجيل خروج الويب" : "Sign Out Web Portal"}</span>
          </button>
        </div>

        <OwnerDashboard lang={lang} sessionToken={webSessionToken} isOwner={true} />
      </div>
    );
  }

  return (
    <section
      className="safi-panel"
      data-owner-web-portal="login"
      style={{ maxWidth: "480px", margin: "48px auto", padding: "28px" }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: "10px", marginBottom: "12px" }}>
        <Lock size={22} aria-hidden="true" />
        <h1 style={{ fontSize: "20px", fontWeight: 800, margin: 0 }}>
          {isAr ? "بوابة إدارة مالك صافي البيانات" : "SafiData Owner Administration Portal"}
        </h1>
      </div>
      <p style={{ fontSize: "13px", color: "var(--text-secondary)", marginBottom: "20px" }}>
        {isAr
          ? "مدخل مركزي محمي لحساب المالك فقط. يتم التحقق عبر الخادم المركزي متبوعًا برمز Google Authenticator (TOTP)، دون إلغاء جلسة سطح المكتب."
          : "Protected central administration entry restricted to the sealed Platform Owner account. Server-verified credentials plus Google Authenticator (TOTP) required; never revokes your Desktop installation."}
      </p>

      {error !== null ? (
        <div
          role="alert"
          data-owner-web-error="true"
          style={{
            padding: "10px 14px",
            borderRadius: "10px",
            border: "1px solid var(--safi-danger, #ef4444)",
            marginBottom: "14px",
            fontSize: "13px"
          }}
        >
          {error}
        </div>
      ) : null}

      <form onSubmit={(e) => void handleLogin(e)} style={{ display: "grid", gap: "12px" }}>
        <label style={{ display: "grid", gap: "6px", fontSize: "13px", fontWeight: 600 }}>
          <span>{isAr ? "البريد الإلكتروني للمالك" : "Owner Email"}</span>
          <input
            type="email"
            required
            autoComplete="username"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
            placeholder="owner@domain.com"
            data-owner-web-email="true"
            className="safi-input"
          />
        </label>

        <label style={{ display: "grid", gap: "6px", fontSize: "13px", fontWeight: 600 }}>
          <span>{isAr ? "كلمة المرور" : "Password"}</span>
          <input
            type="password"
            required
            autoComplete="current-password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            data-owner-web-password="true"
            className="safi-input"
          />
        </label>

        <button
          type="submit"
          className="safi-btn safi-btn-primary"
          disabled={busy}
          data-owner-web-submit="true"
        >
          {busy
            ? isAr
              ? "جارٍ التحقق..."
              : "Verifying..."
            : isAr
              ? "متابعة إلى التحقق بخطوتين (TOTP)"
              : "Continue to Google Authenticator (TOTP)"}
        </button>

        {onBackToShowcase ? (
          <button type="button" className="safi-btn safi-btn-secondary" onClick={onBackToShowcase}>
            {isAr ? "العودة إلى الموقع العام" : "Back to Public Website"}
          </button>
        ) : null}
      </form>
    </section>
  );
}
