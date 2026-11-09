import { useEffect, useRef, useState } from "react";
import type { FormEvent, ReactNode } from "react";
import {
  LogIn,
  LogOut,
  MailCheck,
  Send,
  ShieldCheck,
  UserPlus,
  X
} from "lucide-react";
import {
  deleteSafiAuth,
  postSafiAuth,
  SafiAuthApiError,
  type SafiLoginResponse
} from "../../lib/auth-api";

type AccountMode = "login" | "register" | "resend";

export interface AccountAccessSheetProps {
  readonly open: boolean;
  readonly onClose: () => void;
  readonly lang?: "ar" | "en";
  readonly verificationToken?: string | null;
  readonly sessionToken?: string | null;
  readonly onAuthenticated: (token: string) => void;
  readonly onLoggedOut: () => void;
}

function getRequestError(error: unknown, isAr: boolean, isVerification = false): string {
  if (error instanceof SafiAuthApiError) {
    if (error.status === 0 || error.status >= 500) {
      return isAr
        ? "خدمة الحسابات غير متاحة حاليًا. حاول مرة أخرى لاحقًا."
        : "The account service is unavailable. Please try again later.";
    }

    if (error.status === 401) {
      return isAr
        ? "تعذّر تسجيل الدخول بهذه البيانات. راجع البريد وكلمة المرور، أو أعد إرسال رابط التحقق."
        : "We could not sign in with those details. Check your email and password, or resend the verification link.";
    }

    if (error.status === 400 || error.status === 422) {
      return isVerification
        ? isAr
          ? "رابط التحقق غير صالح أو انتهت صلاحيته. اطلب رابطًا جديدًا."
          : "This verification link is invalid or has expired. Request a new link."
        : isAr
          ? "راجع البيانات. يجب ألا تقل كلمة المرور الجديدة عن 12 حرفًا."
          : "Check your details. New passwords must contain at least 12 characters.";
    }
  }

  return isAr
    ? "حدث خطأ غير متوقع. حاول مرة أخرى."
    : "Something went wrong. Please try again.";
}

export function AccountAccessSheet({
  open,
  onClose,
  lang = "en",
  verificationToken = null,
  sessionToken = null,
  onAuthenticated,
  onLoggedOut
}: AccountAccessSheetProps): ReactNode {
  const isAr = lang === "ar";
  const [mode, setMode] = useState<AccountMode>("login");
  const [email, setEmail] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [feedback, setFeedback] = useState<{
    readonly kind: "success" | "error";
    readonly message: string;
  } | null>(null);
  const verificationAttempted = useRef<string | null>(null);
  const closeButtonRef = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    if (!open) return;
    closeButtonRef.current?.focus();

    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key === "Escape" && !busy) {
        event.preventDefault();
        onClose();
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, busy, onClose]);

  useEffect(() => {
    if (
      !open ||
      !verificationToken ||
      verificationAttempted.current === verificationToken
    ) {
      return;
    }

    verificationAttempted.current = verificationToken;
    setMode("login");
    setBusy(true);
    setFeedback({
      kind: "success",
      message: isAr ? "جارٍ التحقق من بريدك الإلكتروني…" : "Verifying your email address…"
    });

    void postSafiAuth<{ message: string }>("/email/verify", {
      token: verificationToken
    })
      .then(() => {
        setFeedback({
          kind: "success",
          message: isAr
            ? "تم تأكيد بريدك الإلكتروني. يمكنك تسجيل الدخول الآن."
            : "Your email is verified. You can sign in now."
        });
      })
      .catch((error: unknown) => {
        setFeedback({
          kind: "error",
          message: getRequestError(error, isAr, true)
        });
      })
      .finally(() => setBusy(false));
  }, [open, verificationToken, isAr]);

  const changeMode = (nextMode: AccountMode): void => {
    setMode(nextMode);
    setFeedback(null);
  };

  const submitForm = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    if (busy) return;

    setBusy(true);
    setFeedback(null);
    const normalizedEmail = email.trim().toLowerCase();

    try {
      if (mode === "register") {
        await postSafiAuth<{ message: string }>("/register", {
          email: normalizedEmail,
          display_name: displayName.trim(),
          password
        });
        setMode("resend");
        setPassword("");
        setFeedback({
          kind: "success",
          message: isAr
            ? "إذا كان هذا البريد قابلًا للتسجيل، فسيصل إليه رابط التحقق. افحص الوارد والرسائل غير المرغوب فيها."
            : "If this address can be registered, a verification link will be sent. Check your inbox and spam folder."
        });
      } else if (mode === "resend") {
        await postSafiAuth<{ message: string }>("/email/verification/resend", {
          email: normalizedEmail
        });
        setFeedback({
          kind: "success",
          message: isAr
            ? "إذا كان الحساب يحتاج إلى التحقق، فسيصل إليه رابط جديد."
            : "If this account needs verification, a new link will be sent."
        });
      } else {
        const result = await postSafiAuth<SafiLoginResponse>("/login", {
          email: normalizedEmail,
          password,
          device_name: "Web browser"
        });
        onAuthenticated(result.token);
        setPassword("");
        setFeedback({
          kind: "success",
          message: isAr
            ? "تم تسجيل الدخول. الجلسة محفوظة في ذاكرة الصفحة الحالية فقط."
            : "You are signed in. This session stays in this page's memory only."
        });
      }
    } catch (error) {
      setFeedback({
        kind: "error",
        message: getRequestError(error, isAr)
      });
    } finally {
      setBusy(false);
    }
  };

  const signOut = async (): Promise<void> => {
    if (!sessionToken || busy) return;

    setBusy(true);
    setFeedback(null);
    try {
      await deleteSafiAuth("/sessions/current", sessionToken);
      onLoggedOut();
      setFeedback({
        kind: "success",
        message: isAr ? "تم تسجيل الخروج." : "You have signed out."
      });
    } catch (error) {
      setFeedback({
        kind: "error",
        message: getRequestError(error, isAr)
      });
    } finally {
      setBusy(false);
    }
  };

  if (!open) return null;

  return (
    <div
      className="safi-overlay-backdrop safi-account-backdrop"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !busy) onClose();
      }}
    >
      <section
        className="safi-modal safi-account-modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="safi-account-title"
        dir={isAr ? "rtl" : "ltr"}
      >
        <header className="safi-account-header">
          <div>
            <p className="safi-account-eyebrow">
              <ShieldCheck size={14} aria-hidden="true" />
              {isAr ? "هوية SafiData" : "SAFIDATA IDENTITY"}
            </p>
            <h2 id="safi-account-title">
              {sessionToken
                ? isAr
                  ? "حسابك"
                  : "Your account"
                : mode === "register"
                  ? isAr
                    ? "إنشاء حساب"
                    : "Create your account"
                  : mode === "resend"
                    ? isAr
                      ? "إعادة إرسال التحقق"
                      : "Resend verification"
                    : isAr
                      ? "تسجيل الدخول"
                      : "Sign in"}
            </h2>
          </div>

          <button
            ref={closeButtonRef}
            type="button"
            className="safi-account-close"
            aria-label={isAr ? "إغلاق نافذة الحساب" : "Close account window"}
            onClick={onClose}
            disabled={busy}
          >
            <X size={18} aria-hidden="true" />
          </button>
        </header>

        {sessionToken ? (
          <div className="safi-account-signed-in">
            <div className="safi-account-status-mark">
              <ShieldCheck size={24} aria-hidden="true" />
            </div>
            <h3>{isAr ? "أنت مسجل الدخول" : "You are signed in"}</h3>
            <p>
              {isAr
                ? "مصادقة هذه الصفحة نشطة. لن نحفظ رمز الجلسة في التخزين المحلي أو ملفات تعريف الارتباط."
                : "Authentication is active for this page. The session token is not saved to local storage or cookies."}
            </p>
            <button
              type="button"
              className="safi-btn safi-btn-secondary safi-account-submit"
              onClick={() => void signOut()}
              disabled={busy}
            >
              <LogOut size={16} aria-hidden="true" />
              {busy
                ? isAr
                  ? "جارٍ..."
                  : "Working…"
                : isAr
                  ? "تسجيل الخروج"
                  : "Sign out"}
            </button>
          </div>
        ) : (
          <>
            <div
              className="safi-account-tabs"
              role="tablist"
              aria-label={isAr ? "نوع الحساب" : "Account action"}
            >
              <button
                type="button"
                role="tab"
                aria-selected={mode === "login"}
                className={mode === "login" ? "is-active" : ""}
                onClick={() => changeMode("login")}
              >
                <LogIn size={15} aria-hidden="true" />
                {isAr ? "دخول" : "Sign in"}
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={mode === "register"}
                className={mode === "register" ? "is-active" : ""}
                onClick={() => changeMode("register")}
              >
                <UserPlus size={15} aria-hidden="true" />
                {isAr ? "حساب جديد" : "Create account"}
              </button>
            </div>

            <p className="safi-account-description">
              {mode === "register"
                ? isAr
                  ? "أنشئ حسابك بالبريد الإلكتروني وكلمة المرور، ثم أكّد ملكية البريد."
                  : "Create an account with your email and password, then verify your email address."
                : mode === "resend"
                  ? isAr
                    ? "أدخل بريدك لطلب رابط تحقق جديد، دون كشف ما إذا كان الحساب موجودًا."
                    : "Enter your email to request a new verification link without revealing whether an account exists."
                  : isAr
                    ? "سجّل الدخول باستخدام بريدك الإلكتروني بعد تأكيده."
                    : "Sign in with your email after verifying your address."}
            </p>

            <form className="safi-account-form" onSubmit={(event) => void submitForm(event)}>
              <label className="safi-account-field">
                <span>{isAr ? "البريد الإلكتروني" : "Email address"}</span>
                <input
                  type="email"
                  name="email"
                  autoComplete="email"
                  maxLength={254}
                  required
                  value={email}
                  onChange={(event) => setEmail(event.currentTarget.value)}
                  placeholder="name@example.com"
                  disabled={busy}
                />
              </label>

              {mode === "register" ? (
                <label className="safi-account-field">
                  <span>{isAr ? "الاسم الظاهر" : "Display name"}</span>
                  <input
                    type="text"
                    name="display_name"
                    autoComplete="name"
                    minLength={1}
                    maxLength={80}
                    required
                    value={displayName}
                    onChange={(event) => setDisplayName(event.currentTarget.value)}
                    disabled={busy}
                  />
                </label>
              ) : null}

              {mode !== "resend" ? (
                <label className="safi-account-field">
                  <span>{isAr ? "كلمة المرور" : "Password"}</span>
                  <input
                    type="password"
                    name="password"
                    autoComplete={mode === "register" ? "new-password" : "current-password"}
                    minLength={mode === "register" ? 12 : undefined}
                    maxLength={128}
                    required
                    value={password}
                    onChange={(event) => setPassword(event.currentTarget.value)}
                    disabled={busy}
                  />
                  {mode === "register" ? (
                    <small>
                      {isAr ? "12 حرفًا على الأقل." : "At least 12 characters."}
                    </small>
                  ) : null}
                </label>
              ) : null}

              {feedback ? (
                <p
                  className={`safi-account-feedback safi-account-feedback--${feedback.kind}`}
                  role={feedback.kind === "error" ? "alert" : "status"}
                  aria-live="polite"
                >
                  {feedback.message}
                </p>
              ) : null}

              <button
                type="submit"
                className="safi-btn safi-btn-primary safi-account-submit"
                disabled={busy}
              >
                {mode === "resend" ? (
                  <Send size={16} aria-hidden="true" />
                ) : mode === "register" ? (
                  <UserPlus size={16} aria-hidden="true" />
                ) : (
                  <LogIn size={16} aria-hidden="true" />
                )}
                {busy
                  ? isAr
                    ? "جارٍ المعالجة..."
                    : "Please wait…"
                  : mode === "resend"
                    ? isAr
                      ? "إرسال رابط جديد"
                      : "Send a new link"
                    : mode === "register"
                      ? isAr
                        ? "إنشاء الحساب وإرسال التحقق"
                        : "Create account"
                      : isAr
                        ? "تسجيل الدخول"
                        : "Sign in"}
              </button>
            </form>

            {mode === "login" ? (
              <button
                type="button"
                className="safi-account-text-button"
                onClick={() => changeMode("resend")}
              >
                <MailCheck size={15} aria-hidden="true" />
                {isAr ? "إعادة إرسال رابط التحقق" : "Resend verification email"}
              </button>
            ) : mode === "resend" ? (
              <button
                type="button"
                className="safi-account-text-button"
                onClick={() => changeMode("login")}
              >
                {isAr ? "العودة إلى تسجيل الدخول" : "Back to sign in"}
              </button>
            ) : null}
          </>
        )}

        {sessionToken && feedback ? (
          <p
            className={`safi-account-feedback safi-account-feedback--${feedback.kind}`}
            role={feedback.kind === "error" ? "alert" : "status"}
            aria-live="polite"
          >
            {feedback.message}
          </p>
        ) : null}

        <footer className="safi-account-footer">
          <span>{isAr ? "بياناتك تظل على جهازك." : "Your data stays on your device."}</span>
          <span>SafiData</span>
        </footer>
      </section>
    </div>
  );
}
