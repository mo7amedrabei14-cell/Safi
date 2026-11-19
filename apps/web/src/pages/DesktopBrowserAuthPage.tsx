/**
 * `apps/web/src/pages/DesktopBrowserAuthPage.tsx`
 *
 * System-browser Desktop PKCE Authentication Page (`/auth/desktop?flow=signin|signup&request_id=...&state=...`):
 * - Approves a PKCE Desktop sign-in request via `POST /v1/auth/desktop/approve`.
 * - Returns ONLY a short-lived single-use authorization code via `safi://auth?code=...&state=...`.
 * - NEVER embeds session tokens, passwords, or secrets in callback URLs.
 */
import type { FormEvent, ReactNode } from "react";
import { useMemo, useState } from "react";
import { CheckCircle2, KeyRound, ShieldCheck } from "lucide-react";
import type { SafiLang } from "../components/layout/FloatingGlassDock";
import { postSafiAuth, SafiAuthApiError } from "../lib/auth-api";
import { ownerApi } from "../lib/owner-api";

export interface DesktopBrowserAuthPageProps {
  readonly lang?: SafiLang;
}

export function DesktopBrowserAuthPage({ lang = "en" }: DesktopBrowserAuthPageProps): ReactNode {
  const isAr = lang === "ar";
  const params = useMemo(() => {
    if (typeof window === "undefined") {
      return { flow: "signin", requestId: "", state: "" };
    }
    const qs = new URLSearchParams(window.location.search);
    return {
      flow: qs.get("flow") === "signup" ? "signup" : "signin",
      requestId: (qs.get("request_id") ?? "").trim(),
      state: (qs.get("state") ?? "").trim()
    };
  }, []);

  const [activeFlow, setActiveFlow] = useState<"signin" | "signup">(
    params.flow === "signup" ? "signup" : "signin"
  );
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [signupMessage, setSignupMessage] = useState<string | null>(null);
  const [callbackUrl, setCallbackUrl] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const hasValidParams = params.requestId.length >= 8 && params.state.length >= 16;

  const handleApprove = async (event: FormEvent): Promise<void> => {
    event.preventDefault();
    if (!hasValidParams) {
      setError(
        isAr
          ? "رابط المصادقة غير مكتمل أو مفقود (SF-AUTH-023)."
          : "Missing or invalid PKCE request parameters (SF-AUTH-023)."
      );
      return;
    }
    setBusy(true);
    setError(null);
    setSignupMessage(null);
    try {
      if (activeFlow === "signup") {
        await postSafiAuth("/register", {
          email: email.trim(),
          name: name.trim() || email.trim().split("@")[0] || "Safi User",
          password
        });
        setSignupMessage(
          isAr
            ? "تم إنشاء الحساب بنجاح. بعد تأكيد بريدك الإلكتروني يمكنك المتابعة بتسجيل الدخول لتفويض تطبيق سطح المكتب."
            : "Account created. After verifying your email, sign in below to authorize SafiData Desktop."
        );
        setActiveFlow("signin");
        return;
      }
      const approved = await ownerApi.approveDesktopPkceAuth({
        request_id: params.requestId,
        state: params.state,
        email: email.trim(),
        password
      });
      setCallbackUrl(approved.callback_uri);
      setPassword("");
    } catch (err) {
      if (err instanceof SafiAuthApiError) {
        setError(
          isAr
            ? `تعذّر إتمام المصادقة (${err.code ?? `HTTP ${err.status}`}).`
            : `Desktop authentication failed (${err.code ?? `HTTP ${err.status}`}).`
        );
      } else {
        setError(
          isAr ? "تعذّر الاتصال بخادم المصادقة." : "Could not reach the authentication server."
        );
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <section
      className="safi-panel"
      data-desktop-browser-auth="true"
      style={{ maxWidth: "500px", margin: "48px auto", padding: "28px" }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: "10px", marginBottom: "12px" }}>
        <KeyRound size={22} aria-hidden="true" />
        <h1 style={{ fontSize: "20px", fontWeight: 800, margin: 0 }}>
          {activeFlow === "signup"
            ? isAr
              ? "إنشاء حساب وتوصيل تطبيق صافي لسطح المكتب"
              : "Sign Up & Authorize SafiData Desktop"
            : isAr
              ? "تسجيل الدخول الآمن لتطبيق صافي لسطح المكتب"
              : "Secure Sign-In for SafiData Desktop"}
        </h1>
      </div>

      <p style={{ fontSize: "13px", color: "var(--text-secondary)", marginBottom: "18px" }}>
        {isAr
          ? "تستخدم هذه الصفحة بروتوكول PKCE (S256) مع رمز تفويض يُستخدم لمرة واحدة فقط عبر safi://auth دون وضع أي رموز جلسة في الرابط."
          : "Protected by OAuth-style PKCE (S256) with a short-lived, single-use authorization code returned via safi://auth. Zero session tokens ever appear in URLs."}
      </p>

      {!hasValidParams ? (
        <div role="alert" data-desktop-auth-missing-params="true" style={{ fontSize: "13px" }}>
          {isAr
            ? "افتح هذه الصفحة من داخل تطبيق صافي لسطح المكتب لتوليد طلب PKCE صالح."
            : "Launch sign-in from the SafiData Desktop application to generate a valid PKCE request."}
        </div>
      ) : callbackUrl !== null ? (
        <div data-desktop-auth-approved="true" style={{ display: "grid", gap: "14px" }}>
          <div style={{ display: "flex", alignItems: "center", gap: "8px", fontWeight: 700 }}>
            <CheckCircle2 size={18} aria-hidden="true" />
            <span>
              {isAr
                ? "تم التحقق بنجاح! افتح تطبيق صافي لسطح المكتب لإكمال ربط الجهاز."
                : "Approved! Open SafiData Desktop to complete device binding."}
            </span>
          </div>
          <a
            href={callbackUrl}
            className="safi-btn safi-btn-primary"
            data-desktop-auth-callback-link="true"
          >
            <ShieldCheck size={16} aria-hidden="true" />
            <span>{isAr ? "العودة إلى تطبيق صافي لسطح المكتب" : "Return to SafiData Desktop"}</span>
          </a>
        </div>
      ) : (
        <form onSubmit={(e) => void handleApprove(e)} style={{ display: "grid", gap: "12px" }}>
          {error !== null ? (
            <div role="alert" style={{ fontSize: "13px", color: "var(--safi-danger, #ef4444)" }}>
              {error}
            </div>
          ) : null}

          {signupMessage !== null ? (
            <div role="status" data-desktop-auth-signup-status="true" style={{ fontSize: "13px" }}>
              {signupMessage}
            </div>
          ) : null}

          {activeFlow === "signup" ? (
            <label style={{ display: "grid", gap: "6px", fontSize: "13px", fontWeight: 600 }}>
              <span>{isAr ? "الاسم" : "Full name"}</span>
              <input
                type="text"
                required
                value={name}
                onChange={(e) => setName(e.target.value)}
                className="safi-input"
                data-desktop-auth-name="true"
              />
            </label>
          ) : null}

          <label style={{ display: "grid", gap: "6px", fontSize: "13px", fontWeight: 600 }}>
            <span>{isAr ? "البريد الإلكتروني" : "Email"}</span>
            <input
              type="email"
              required
              autoComplete="username"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              className="safi-input"
              data-desktop-auth-email="true"
            />
          </label>

          <label style={{ display: "grid", gap: "6px", fontSize: "13px", fontWeight: 600 }}>
            <span>{isAr ? "كلمة المرور" : "Password"}</span>
            <input
              type="password"
              required
              autoComplete={activeFlow === "signup" ? "new-password" : "current-password"}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              className="safi-input"
              data-desktop-auth-password="true"
            />
          </label>

          <button
            type="submit"
            className="safi-btn safi-btn-primary"
            disabled={busy}
            data-desktop-auth-submit="true"
          >
            {busy
              ? isAr
                ? "جارٍ المعالجة..."
                : "Processing..."
              : activeFlow === "signup"
                ? isAr
                  ? "إنشاء الحساب والمتابعة"
                  : "Create Account & Continue"
                : isAr
                  ? "تفويض تطبيق سطح المكتب"
                  : "Authorize Desktop App"}
          </button>

          <button
            type="button"
            className="safi-btn safi-btn-ghost"
            onClick={() => setActiveFlow((prev) => (prev === "signup" ? "signin" : "signup"))}
          >
            {activeFlow === "signup"
              ? isAr
                ? "لديك حساب بالفعل؟ سجّل الدخول لتفويض سطح المكتب"
                : "Already have an account? Sign in to authorize Desktop"
              : isAr
                ? "جديد في صافي؟ أنشئ حسابًا أولًا"
                : "New to SafiData? Create an account first"}
          </button>
        </form>
      )}
    </section>
  );
}
