/**
 * Password reset portal: request a one-time link, or choose a new password
 * when the page was opened from the emailed `#reset_password=` link.
 * The token is never persisted and never echoed back to the DOM.
 */
import type { FormEvent, ReactNode } from "react";
import { useEffect, useRef, useState } from "react";
import { Check, Eye, EyeOff, KeyRound, LoaderCircle, Mail, ShieldCheck, X } from "lucide-react";
import { postSafiAuth, SafiAuthApiError } from "../../lib/auth-api";
import { MIN_PASSWORD_LENGTH } from "../../lib/business-config";
import type { SafiLang } from "../layout/FloatingGlassDock";

export interface PasswordResetModalProps {
  readonly open: boolean;
  readonly lang?: SafiLang;
  readonly initialToken?: string | null;
  readonly onClose: () => void;
}

type Feedback = { readonly kind: "success" | "error"; readonly message: string } | null;

export function PasswordResetModal({
  open,
  lang = "en",
  initialToken = null,
  onClose
}: PasswordResetModalProps): ReactNode {
  const isAr = lang === "ar";
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [busy, setBusy] = useState(false);
  const [feedback, setFeedback] = useState<Feedback>(null);
  const closeRef = useRef<HTMLButtonElement | null>(null);
  const confirming = initialToken !== null && initialToken.length > 0;
  const passwordReady = Array.from(password).length >= MIN_PASSWORD_LENGTH;
  const passwordsMatch = password.length > 0 && password === confirmPassword;

  useEffect(() => {
    if (!open) return;
    closeRef.current?.focus();
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key === "Escape" && !busy) onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, busy, onClose]);

  useEffect(() => {
    if (!open) return;
    setFeedback(null);
    setPassword("");
    setConfirmPassword("");
  }, [open, confirming]);

  const submit = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    if (busy) return;
    setFeedback(null);

    if (confirming) {
      if (!passwordReady || !passwordsMatch) {
        setFeedback({
          kind: "error",
          message: isAr
            ? `كلمة المرور يجب ألا تقل عن ${MIN_PASSWORD_LENGTH} خانات وأن تتطابق مع التأكيد.`
            : `Use at least ${MIN_PASSWORD_LENGTH} characters and make both passwords match.`
        });
        return;
      }
      setBusy(true);
      try {
        await postSafiAuth<{ message: string }>("/password/reset/confirm", {
          token: initialToken,
          password
        });
        setPassword("");
        setConfirmPassword("");
        setFeedback({
          kind: "success",
          message: isAr
            ? "تم تغيير كلمة المرور وإخراج كل الأجهزة. سجّل الدخول بكلمة المرور الجديدة."
            : "Password changed and every device signed out. Sign in with the new password."
        });
      } catch (error) {
        setFeedback({
          kind: "error",
          message:
            error instanceof SafiAuthApiError && error.status === 422
              ? isAr
                ? "الرابط غير صالح أو انتهت صلاحيته. اطلب رابطًا جديدًا."
                : "This link is invalid or expired. Request a new one."
              : isAr
                ? "تعذّر تغيير كلمة المرور الآن. حاول مرة أخرى."
                : "We could not change the password right now. Try again."
        });
      } finally {
        setBusy(false);
      }
      return;
    }

    setBusy(true);
    try {
      await postSafiAuth<{ message: string }>("/password/reset/request", {
        email: email.trim()
      });
      setFeedback({
        kind: "success",
        message: isAr
          ? "إن كان لهذا البريد حساب، فسيصلك رابط لإعادة التعيين خلال دقائق. افحص البريد المزعج أيضًا."
          : "If this address has an account, a reset link is on its way. Check spam too."
      });
    } catch {
      setFeedback({
        kind: "error",
        message: isAr
          ? "أدخل بريدًا إلكترونيًا صالحًا وحاول مرة أخرى."
          : "Enter a valid email address and try again."
      });
    } finally {
      setBusy(false);
    }
  };

  if (!open) return null;

  return (
    <div className="safi-auth-backdrop" role="presentation">
      <div
        className="safi-auth-panel"
        role="dialog"
        aria-modal="true"
        aria-labelledby="safi-password-reset-title"
        dir={isAr ? "rtl" : "ltr"}
      >
        <button
          ref={closeRef}
          type="button"
          className="safi-auth-close"
          onClick={onClose}
          aria-label={isAr ? "إغلاق استعادة كلمة المرور" : "Close password reset"}
        >
          <X size={18} aria-hidden="true" />
        </button>

        <header className="safi-auth-header">
          <span className="safi-auth-eyebrow">
            <ShieldCheck size={14} aria-hidden="true" />
            {isAr ? "حماية الحساب" : "ACCOUNT SECURITY"}
          </span>
          <h2 id="safi-password-reset-title">
            {confirming
              ? isAr
                ? "اختر كلمة مرور جديدة"
                : "Choose a new password"
              : isAr
                ? "استعادة كلمة المرور"
                : "Reset your password"}
          </h2>
          <p className="safi-auth-subtitle">
            {confirming
              ? isAr
                ? "سيتم إخراج كل الأجهزة الأخرى بعد الحفظ."
                : "Every other device will be signed out after saving."
              : isAr
                ? "سنرسل رابطًا لمرة واحدة صالحًا لمدة ساعة."
                : "We will email a one-time link valid for one hour."}
          </p>
        </header>

        <form className="safi-auth-form" onSubmit={(event) => void submit(event)}>
          {confirming ? (
            <>
              <label className="safi-auth-field">
                <span>{isAr ? "كلمة المرور الجديدة" : "New password"}</span>
                <span className="safi-auth-input-wrap">
                  <KeyRound size={15} aria-hidden="true" />
                  <input
                    type={showPassword ? "text" : "password"}
                    autoComplete="new-password"
                    minLength={MIN_PASSWORD_LENGTH}
                    maxLength={128}
                    required
                    value={password}
                    onChange={(event) => setPassword(event.currentTarget.value)}
                    disabled={busy}
                    dir="ltr"
                  />
                  <button
                    type="button"
                    className="safi-auth-eye"
                    onClick={() => setShowPassword((prev) => !prev)}
                    aria-label={
                      showPassword
                        ? isAr
                          ? "إخفاء كلمة المرور"
                          : "Hide password"
                        : isAr
                          ? "إظهار كلمة المرور"
                          : "Show password"
                    }
                    aria-pressed={showPassword}
                  >
                    {showPassword ? (
                      <EyeOff size={17} aria-hidden="true" />
                    ) : (
                      <Eye size={17} aria-hidden="true" />
                    )}
                  </button>
                </span>
              </label>

              <label className="safi-auth-field">
                <span>{isAr ? "تأكيد كلمة المرور" : "Confirm password"}</span>
                <span className="safi-auth-input-wrap">
                  {passwordsMatch ? (
                    <Check size={15} aria-hidden="true" />
                  ) : (
                    <KeyRound size={15} aria-hidden="true" />
                  )}
                  <input
                    type={showPassword ? "text" : "password"}
                    autoComplete="new-password"
                    minLength={MIN_PASSWORD_LENGTH}
                    maxLength={128}
                    required
                    value={confirmPassword}
                    onChange={(event) => setConfirmPassword(event.currentTarget.value)}
                    disabled={busy}
                    dir="ltr"
                  />
                </span>
              </label>

              <div
                className={`safi-auth-password-meter ${passwordReady && passwordsMatch ? "is-ready" : ""}`}
                role="status"
                aria-live="polite"
              >
                <span className="safi-auth-meter-dot">
                  {passwordReady && passwordsMatch ? <Check size={13} aria-hidden="true" /> : null}
                </span>
                <span>
                  {isAr
                    ? `${passwordReady ? "✓" : ""} ${MIN_PASSWORD_LENGTH} خانات على الأقل، وكلمتان متطابقتان`
                    : `${passwordReady ? "✓" : ""} At least ${MIN_PASSWORD_LENGTH} characters, both fields matching`}
                </span>
              </div>
            </>
          ) : (
            <label className="safi-auth-field">
              <span>{isAr ? "البريد الإلكتروني" : "Email address"}</span>
              <span className="safi-auth-input-wrap">
                <Mail size={15} aria-hidden="true" />
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
                  dir="ltr"
                />
              </span>
            </label>
          )}

          {feedback !== null ? (
            <p
              className={`safi-auth-feedback is-${feedback.kind}`}
              role={feedback.kind === "error" ? "alert" : "status"}
            >
              {feedback.message}
            </p>
          ) : null}

          <button
            type="submit"
            className="safi-btn safi-btn-primary safi-auth-submit"
            disabled={busy}
          >
            {busy ? <LoaderCircle className="safi-spin" size={16} aria-hidden="true" /> : null}
            {confirming
              ? isAr
                ? "حفظ كلمة المرور الجديدة"
                : "Save new password"
              : isAr
                ? "إرسال رابط الاستعادة"
                : "Send reset link"}
          </button>
        </form>
      </div>
    </div>
  );
}
