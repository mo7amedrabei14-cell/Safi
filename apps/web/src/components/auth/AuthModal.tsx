/**
 * SafiData motion auth portal (Apple ID / Google Sign-In calibre).
 *
 * - Silky GSAP transition between Sign In and Sign Up.
 * - Staggered, magnetically responsive fields and provider buttons.
 * - Google / Apple / GitHub OAuth entry points.
 * - Eight-character password floor (from `config/business.json`) with a live
 *   green check and an eye visibility toggle.
 * - Duplicate-account guard with a one-click path to the Sign In tab.
 * - Owner recognition for `mo7amed.rabei14@gmail.com`.
 */
import type {
  FormEvent,
  KeyboardEvent as ReactKeyboardEvent,
  PointerEvent as ReactPointerEvent,
  ReactNode
} from "react";
import { useCallback, useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { gsap } from "gsap";
import {
  ArrowRight,
  Check,
  Eye,
  EyeOff,
  LoaderCircle,
  LockKeyhole,
  LogIn,
  Mail,
  ShieldCheck,
  Sparkles,
  UserPlus,
  X
} from "lucide-react";
import {
  deleteSafiAuth,
  postSafiAuth,
  SafiAuthApiError,
  SafiAuthConfigurationError,
  type SafiLoginResponse
} from "../../lib/auth-api";
import { getSafiDeviceIdentity, SafiDeviceIdentityError } from "../../lib/device-identity";
import type { SafiLang, SafiThemeId } from "../layout/FloatingGlassDock";
import {
  DUPLICATE_ACCOUNT_MESSAGE_AR,
  DUPLICATE_ACCOUNT_MESSAGE_EN,
  PASSWORD_TOO_SHORT_MESSAGE_AR,
  PASSWORD_TOO_SHORT_MESSAGE_EN,
  isOwnerEmail,
  meetsMinPasswordLength,
  normalizeEmail,
  validateSignUp,
  type ExistingAccountIdentity
} from "../../lib/auth-rules";
import {
  MIN_PASSWORD_LENGTH,
  OAUTH_PROVIDERS,
  OWNER_EMAIL,
  TRIAL_DAYS
} from "../../lib/business-config";
import { canAnimate } from "../ui/motion-preference";

export type AuthMode = "signin" | "signup";

export interface AuthPortalSession {
  readonly token: string;
  readonly email: string;
  readonly isOwner: boolean;
}

export interface AuthModalProps {
  readonly open: boolean;
  readonly mandatory?: boolean;
  readonly lang?: SafiLang;
  readonly theme?: SafiThemeId;
  readonly initialMode?: AuthMode;
  readonly onClose: () => void;
  readonly onAuthenticated?: (session: AuthPortalSession) => void | Promise<void>;
  readonly onOwnerDetected?: (email: string) => void;
  readonly onOAuth?: (provider: (typeof OAUTH_PROVIDERS)[number]) => void;
  readonly existingAccounts?: readonly ExistingAccountIdentity[];
  readonly sessionToken?: string | null;
  readonly sessionEmail?: string | null;
  readonly onSignedOut?: () => void | Promise<void>;
  readonly onForgotPassword?: () => void;
}

type Feedback =
  | { readonly kind: "success"; readonly message: string }
  | { readonly kind: "error"; readonly message: string }
  | { readonly kind: "duplicate"; readonly message: string };

function providerLabel(provider: string, isAr: boolean): string {
  const names: Record<string, { ar: string; en: string }> = {
    google: { ar: "Google", en: "Google" },
    apple: { ar: "Apple", en: "Apple" },
    github: { ar: "GitHub", en: "GitHub" }
  };
  const name = names[provider]?.ar ?? provider;
  return isAr ? `المتابعة باستخدام ${name}` : `Continue with ${names[provider]?.en ?? provider}`;
}

function GoogleMark(): ReactNode {
  return (
    <svg viewBox="0 0 48 48" width="18" height="18" aria-hidden="true" focusable="false">
      <path
        fill="#EA4335"
        d="M24 9.5c3.54 0 6.71 1.22 9.21 3.6l6.85-6.85C35.9 2.38 30.47 0 24 0 14.62 0 6.51 5.38 2.56 13.22l7.98 6.19C12.43 13.72 17.74 9.5 24 9.5Z"
      />
      <path
        fill="#4285F4"
        d="M46.98 24.55c0-1.57-.15-3.09-.38-4.55H24v9.02h12.94c-.58 2.96-2.26 5.48-4.78 7.18l7.73 6c4.51-4.18 7.09-10.36 7.09-17.65Z"
      />
      <path
        fill="#FBBC05"
        d="M10.53 28.59A14.5 14.5 0 0 1 9.77 24c0-1.59.27-3.13.76-4.59l-7.98-6.19A23.94 23.94 0 0 0 0 24c0 3.88.92 7.54 2.56 10.78l7.97-6.19Z"
      />
      <path
        fill="#34A853"
        d="M24 48c6.48 0 11.93-2.13 15.89-5.81l-7.73-6c-2.15 1.45-4.92 2.3-8.16 2.3-6.26 0-11.57-4.22-13.46-9.91l-7.98 6.19C6.51 42.62 14.62 48 24 48Z"
      />
    </svg>
  );
}

function AppleMark(): ReactNode {
  return (
    <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true" focusable="false">
      <path
        fill="currentColor"
        d="M16.37 12.79c.02-2.2 1.79-3.26 1.87-3.31-1.02-1.5-2.61-1.7-3.18-1.72-1.35-.14-2.64.8-3.33.8-.69 0-1.75-.78-2.88-.76-1.48.02-2.85.87-3.61 2.2-1.54 2.68-.39 6.64 1.11 8.82.73 1.07 1.6 2.27 2.75 2.23 1.1-.05 1.52-.72 2.85-.72 1.33 0 1.71.72 2.88.7 1.19-.02 1.94-1.09 2.67-2.16.84-1.24 1.19-2.44 1.21-2.5-.03-.01-2.32-.89-2.34-3.58Zm-2.09-6.47c.61-.74 1.02-1.78.91-2.82-.88.04-1.95.59-2.58 1.33-.56.65-1.05 1.71-.92 2.72.98.08 1.98-.5 2.59-1.23Z"
      />
    </svg>
  );
}

function GithubMark(): ReactNode {
  return (
    <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true" focusable="false">
      <path
        fill="currentColor"
        d="M12 .5A11.5 11.5 0 0 0 .5 12.02c0 5.09 3.29 9.4 7.86 10.93.58.1.79-.25.79-.56v-2.17c-3.2.7-3.88-1.36-3.88-1.36-.52-1.34-1.28-1.7-1.28-1.7-1.05-.72.08-.7.08-.7 1.16.08 1.77 1.2 1.77 1.2 1.03 1.78 2.7 1.27 3.36.97.1-.75.4-1.27.73-1.56-2.55-.3-5.24-1.29-5.24-5.73 0-1.27.45-2.3 1.19-3.11-.12-.3-.52-1.48.11-3.07 0 0 .97-.31 3.18 1.19a10.9 10.9 0 0 1 5.79 0c2.2-1.5 3.17-1.19 3.17-1.19.63 1.59.23 2.77.11 3.07.74.81 1.19 1.84 1.19 3.11 0 4.45-2.7 5.42-5.26 5.71.41.36.78 1.06.78 2.14v3.18c0 .31.21.67.8.56A11.51 11.51 0 0 0 23.5 12.02C23.5 5.66 18.35.5 12 .5Z"
      />
    </svg>
  );
}

function ProviderMark({ provider }: { readonly provider: string }): ReactNode {
  if (provider === "google") return <GoogleMark />;
  if (provider === "apple") return <AppleMark />;
  return <GithubMark />;
}

function isSensitiveAuthInput(input: HTMLInputElement): boolean {
  const name = (input.getAttribute("name") ?? input.name ?? "").toLowerCase();
  const type = (input.getAttribute("type") ?? input.type ?? "").toLowerCase();
  const autocomplete = (
    input.getAttribute("autocomplete") ??
    input.autocomplete ??
    ""
  ).toLowerCase();
  return type === "password" || name.includes("password") || autocomplete.includes("password");
}

/**
 * Resolve WAI-ARIA tablist keyboard navigation with full RTL mirroring.
 */
export function resolveAuthTabKeyboardTarget(
  current: AuthMode,
  key: string,
  isRtl: boolean
): AuthMode | null {
  const logicalOrder: readonly AuthMode[] = ["signin", "signup"];
  const index = logicalOrder.indexOf(current);
  const length = logicalOrder.length;

  switch (key) {
    case "Home":
      return "signin";
    case "End":
      return "signup";
    case "ArrowRight": {
      const delta = isRtl ? -1 : 1;
      return logicalOrder[(index + delta + length) % length] ?? null;
    }
    case "ArrowLeft": {
      const delta = isRtl ? 1 : -1;
      return logicalOrder[(index + delta + length) % length] ?? null;
    }
    case "ArrowDown":
      return logicalOrder[(index + 1) % length] ?? null;
    case "ArrowUp":
      return logicalOrder[(index - 1 + length) % length] ?? null;
    default:
      return null;
  }
}

/**
 * Build a non-interactive visual exit clone of the outgoing form without ever
 * exposing password characters or secrets in the transient DOM layer.
 */
export function buildSanitizedExitGhost(formEl: HTMLElement): HTMLElement {
  const clone = formEl.cloneNode(true) as HTMLElement;
  clone.className = "safi-auth-form safi-auth-form-exit-ghost";
  clone.setAttribute("aria-hidden", "true");

  const sourceInputs = formEl.querySelectorAll("input");
  const clonedInputs = clone.querySelectorAll("input");
  sourceInputs.forEach((input, index) => {
    const copy = clonedInputs[index];
    if (copy === undefined) return;

    if (isSensitiveAuthInput(input)) {
      copy.type = "password";
      copy.value = "";
      copy.defaultValue = "";
      copy.removeAttribute("value");
      return;
    }

    copy.value = input.value;
  });

  clone.querySelectorAll("[id]").forEach((el) => el.removeAttribute("id"));
  clone.querySelectorAll("[name]").forEach((el) => el.removeAttribute("name"));
  clone.querySelectorAll("[for]").forEach((el) => el.removeAttribute("for"));
  clone.querySelectorAll("button, input").forEach((el) => {
    el.setAttribute("tabindex", "-1");
    el.setAttribute("disabled", "true");
  });
  return clone;
}

export function AuthModal({
  open,
  mandatory = false,
  lang = "en",
  initialMode = "signin",
  onClose,
  onAuthenticated,
  onOwnerDetected,
  onOAuth,
  existingAccounts = [],
  sessionToken = null,
  sessionEmail = null,
  onSignedOut,
  onForgotPassword
}: AuthModalProps): ReactNode {
  const isAr = lang === "ar";
  const tabBaseId = useId();
  const signinTabId = `${tabBaseId}-tab-signin`;
  const signupTabId = `${tabBaseId}-tab-signup`;
  const authPanelId = `${tabBaseId}-panel`;
  const [mode, setMode] = useState<AuthMode>(initialMode);
  const [email, setEmail] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [phone, setPhone] = useState("");
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [busy, setBusy] = useState(false);
  const [feedback, setFeedback] = useState<Feedback | null>(null);
  const [ownerRecognized, setOwnerRecognized] = useState(() => isOwnerEmail(email));
  const panelRef = useRef<HTMLDivElement | null>(null);
  const headerRef = useRef<HTMLElement | null>(null);
  const tablistRef = useRef<HTMLDivElement | null>(null);
  const tabIndicatorRef = useRef<HTMLSpanElement | null>(null);
  const signinTabRef = useRef<HTMLButtonElement | null>(null);
  const signupTabRef = useRef<HTMLButtonElement | null>(null);
  const formStageRef = useRef<HTMLDivElement | null>(null);
  const formRef = useRef<HTMLFormElement | null>(null);
  const closeRef = useRef<HTMLButtonElement | null>(null);
  const emailInputRef = useRef<HTMLInputElement | null>(null);
  const wasOpenRef = useRef(false);
  const entrancePlayedRef = useRef(false);
  const entranceCtxRef = useRef<gsap.Context | null>(null);
  const renderedTabModeRef = useRef<AuthMode | null>(null);
  const prevModeRef = useRef<AuthMode>(mode);
  const transitionDirRef = useRef<1 | -1>(1);
  const prevHeightRef = useRef<number | null>(null);
  const exitGhostRef = useRef<HTMLElement | null>(null);

  const passwordReady = meetsMinPasswordLength(password);

  useLayoutEffect(() => {
    if (!open) {
      wasOpenRef.current = false;
      renderedTabModeRef.current = null;
      return;
    }
    if (!wasOpenRef.current) {
      wasOpenRef.current = true;
      if (mandatory) {
        emailInputRef.current?.focus({ preventScroll: true });
      } else {
        closeRef.current?.focus({ preventScroll: true });
      }
    }
  }, [open, mandatory]);

  useEffect(() => {
    if (!open) return;
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key === "Escape" && !busy && !mandatory) onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, busy, mandatory, onClose]);

  useLayoutEffect(() => {
    if (!open) {
      entrancePlayedRef.current = false;
      entranceCtxRef.current?.revert();
      entranceCtxRef.current = null;
      return;
    }
    if (entrancePlayedRef.current) return;
    const panel = panelRef.current;
    if (panel === null || !canAnimate()) return;

    entrancePlayedRef.current = true;
    if (typeof gsap.ticker?.tick === "function") {
      gsap.ticker.tick();
    }

    const ctx = gsap.context(() => {
      gsap.fromTo(
        panel,
        { opacity: 0, y: 8 },
        {
          opacity: 1,
          y: 0,
          duration: 0.28,
          ease: "power2.out",
          immediateRender: true,
          overwrite: "auto",
          clearProps: "transform,opacity"
        }
      );
    }, panel);
    entranceCtxRef.current = ctx;

    return () => {
      if (!panel.isConnected) {
        entrancePlayedRef.current = false;
        ctx.revert();
        entranceCtxRef.current = null;
      }
    };
  }, [open]);

  const updateTabIndicator = useCallback(
    (animate: boolean): void => {
      const tablist = tablistRef.current;
      const indicator = tabIndicatorRef.current;
      const activeBtn = mode === "signin" ? signinTabRef.current : signupTabRef.current;
      if (tablist === null || indicator === null || activeBtn === null) return;

      const listRect = tablist.getBoundingClientRect();
      const btnRect = activeBtn.getBoundingClientRect();
      const scaleX = tablist.offsetWidth > 0 ? listRect.width / tablist.offsetWidth : 1;
      const scaleY = tablist.offsetHeight > 0 ? listRect.height / tablist.offsetHeight : 1;

      const targetX =
        listRect.width > 0 && btnRect.width > 0
          ? Number(((btnRect.left - listRect.left) / scaleX - tablist.clientLeft).toFixed(2))
          : activeBtn.offsetLeft;
      const targetY =
        listRect.height > 0 && btnRect.height > 0
          ? Number(((btnRect.top - listRect.top) / scaleY - tablist.clientTop).toFixed(2))
          : activeBtn.offsetTop;
      const targetWidth =
        btnRect.width > 0 ? Number((btnRect.width / scaleX).toFixed(2)) : activeBtn.offsetWidth;
      const targetHeight =
        btnRect.height > 0 ? Number((btnRect.height / scaleY).toFixed(2)) : activeBtn.offsetHeight;
      if (targetWidth <= 0 || targetHeight <= 0) return;

      indicator.style.opacity = "1";
      if (!animate || !canAnimate()) {
        gsap.killTweensOf(indicator);
        gsap.set(indicator, {
          left: targetX,
          top: targetY,
          width: targetWidth,
          height: targetHeight
        });
        indicator.style.left = `${targetX}px`;
        indicator.style.top = `${targetY}px`;
        indicator.style.width = `${targetWidth}px`;
        indicator.style.height = `${targetHeight}px`;
        return;
      }
      gsap.to(indicator, {
        left: targetX,
        top: targetY,
        width: targetWidth,
        height: targetHeight,
        duration: 0.36,
        ease: "back.out(1.25)",
        overwrite: "auto"
      });
    },
    [mode]
  );

  useLayoutEffect(() => {
    if (!open || sessionToken !== null) {
      renderedTabModeRef.current = null;
      return;
    }
    const shouldAnimate =
      renderedTabModeRef.current !== null && renderedTabModeRef.current !== mode;
    renderedTabModeRef.current = mode;
    updateTabIndicator(shouldAnimate);
    if (!shouldAnimate || !canAnimate()) {
      const rafId = window.requestAnimationFrame(() => updateTabIndicator(false));
      return () => window.cancelAnimationFrame(rafId);
    }
    return undefined;
  }, [open, sessionToken, mode, isAr, updateTabIndicator]);

  useEffect(() => {
    if (!open || sessionToken !== null) return;
    let cancelled = false;
    if (typeof document !== "undefined" && document.fonts?.ready) {
      void document.fonts.ready.then(() => {
        if (!cancelled) updateTabIndicator(false);
      });
    }
    const tablist = tablistRef.current;
    if (tablist === null || typeof ResizeObserver !== "function") {
      return () => {
        cancelled = true;
      };
    }
    const observer = new ResizeObserver(() => updateTabIndicator(false));
    observer.observe(tablist);
    if (panelRef.current !== null) observer.observe(panelRef.current);
    if (signinTabRef.current !== null) observer.observe(signinTabRef.current);
    if (signupTabRef.current !== null) observer.observe(signupTabRef.current);
    return () => {
      cancelled = true;
      observer.disconnect();
    };
  }, [open, sessionToken, updateTabIndicator]);

  useLayoutEffect(() => {
    const previousMode = prevModeRef.current;
    prevModeRef.current = mode;

    const stage = formStageRef.current;
    const formEl = formRef.current;
    const ghost = exitGhostRef.current;
    const prevHeight = prevHeightRef.current;
    exitGhostRef.current = null;
    prevHeightRef.current = null;

    if (!open || previousMode === mode || stage === null || formEl === null || !canAnimate()) {
      ghost?.remove();
      return;
    }

    const axisSign = (isAr ? -1 : 1) * transitionDirRef.current;
    const nextHeight = stage.getBoundingClientRect().height;
    const tweens: gsap.core.Tween[] = [];

    if (prevHeight !== null && Math.abs(nextHeight - prevHeight) > 1) {
      tweens.push(
        gsap.fromTo(
          stage,
          { height: prevHeight, overflow: "hidden" },
          {
            height: nextHeight,
            duration: 0.34,
            ease: "power3.out",
            clearProps: "height,overflow"
          }
        )
      );
    }

    if (ghost !== null) {
      stage.appendChild(ghost);
      tweens.push(
        gsap.fromTo(
          ghost,
          { opacity: 0.9, x: 0, scale: 1 },
          {
            opacity: 0,
            x: -axisSign * 16,
            scale: 0.985,
            duration: 0.12,
            ease: "power2.in",
            onComplete: () => ghost.remove()
          }
        )
      );
    }

    tweens.push(
      gsap.fromTo(
        formEl,
        { opacity: 0, x: axisSign * 16, scale: 0.988 },
        {
          opacity: 1,
          x: 0,
          scale: 1,
          duration: 0.28,
          delay: ghost !== null ? 0.07 : 0,
          ease: "power3.out",
          clearProps: "transform,opacity"
        }
      )
    );

    const staggerTargets = formEl.querySelectorAll<HTMLElement>(
      '[data-auth-stagger="field"], [data-auth-stagger="meta"], [data-auth-stagger="submit"]'
    );
    if (staggerTargets.length > 0) {
      tweens.push(
        gsap.fromTo(
          staggerTargets,
          { opacity: 0.25, y: 6 },
          {
            opacity: 1,
            y: 0,
            duration: 0.24,
            delay: ghost !== null ? 0.08 : 0,
            stagger: 0.028,
            ease: "power2.out",
            clearProps: "transform,opacity"
          }
        )
      );
    }

    if (headerRef.current !== null) {
      const headingNodes =
        headerRef.current.querySelectorAll<HTMLElement>("h2, .safi-auth-subtitle");
      tweens.push(
        gsap.fromTo(
          headingNodes,
          { opacity: 0.6, y: 4 },
          {
            opacity: 1,
            y: 0,
            duration: 0.24,
            stagger: 0.03,
            ease: "power2.out",
            clearProps: "transform,opacity"
          }
        )
      );
    }

    return () => {
      for (const tween of tweens) {
        tween.kill();
      }
      ghost?.remove();
      gsap.set(stage, { clearProps: "height,overflow" });
      gsap.set(formEl, { clearProps: "transform,opacity" });
      if (staggerTargets.length > 0) {
        gsap.set(staggerTargets, { clearProps: "transform,opacity" });
      }
    };
  }, [open, mode, isAr]);

  const magnetize = (event: ReactPointerEvent<HTMLElement>): void => {
    const element = event.currentTarget;
    if (!canAnimate() || event.pointerType !== "mouse") return;
    if (element instanceof HTMLButtonElement && element.disabled) return;
    const rect = element.getBoundingClientRect();
    if (rect.width === 0 || rect.height === 0) return;
    const relX = ((event.clientX - rect.left) / rect.width) * 100;
    const relY = ((event.clientY - rect.top) / rect.height) * 100;
    element.style.setProperty("--glare-x", `${relX.toFixed(1)}%`);
    element.style.setProperty("--glare-y", `${relY.toFixed(1)}%`);

    const dx = (event.clientX - (rect.left + rect.width / 2)) / rect.width;
    const dy = (event.clientY - (rect.top + rect.height / 2)) / rect.height;
    const maxShiftX = element.classList.contains("safi-auth-submit") ? 5 : 8;
    gsap.to(element, {
      x: dx * maxShiftX,
      y: dy * 4,
      duration: 0.25,
      ease: "power2.out",
      overwrite: "auto"
    });
  };

  const demagnetize = (event: ReactPointerEvent<HTMLElement>): void => {
    if (!canAnimate() || event.pointerType !== "mouse") return;
    gsap.to(event.currentTarget, {
      x: 0,
      y: 0,
      duration: 0.45,
      ease: "elastic.out(1, 0.5)",
      overwrite: "auto",
      clearProps: "transform"
    });
  };

  const switchMode = (next: AuthMode): void => {
    if (next === mode) return;
    transitionDirRef.current = next === "signup" ? 1 : -1;

    const stage = formStageRef.current;
    const formEl = formRef.current;
    if (stage !== null && formEl !== null && canAnimate()) {
      stage.querySelectorAll(".safi-auth-form-exit-ghost").forEach((el) => el.remove());
      prevHeightRef.current = stage.getBoundingClientRect().height;
      exitGhostRef.current = buildSanitizedExitGhost(formEl);
    }

    setMode(next);
    setFeedback(null);
  };

  const onTabKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>): void => {
    const targetMode = resolveAuthTabKeyboardTarget(mode, event.key, isAr);
    if (targetMode === null) return;
    event.preventDefault();
    switchMode(targetMode);
    if (targetMode === "signin") {
      signinTabRef.current?.focus();
    } else {
      signupTabRef.current?.focus();
    }
  };

  const showDuplicate = (): void => {
    setFeedback({
      kind: "duplicate",
      message: isAr ? DUPLICATE_ACCOUNT_MESSAGE_AR : DUPLICATE_ACCOUNT_MESSAGE_EN
    });
  };

  const submit = async (event: FormEvent<HTMLFormElement>): Promise<void> => {
    event.preventDefault();
    if (busy) return;

    const normalizedEmail = normalizeEmail(email);
    setFeedback(null);

    if (mode === "signup") {
      const validation = validateSignUp({ email, phone, password }, existingAccounts);
      if (!validation.ok) {
        if (validation.violation.kind === "password_too_short") {
          setFeedback({
            kind: "error",
            message: isAr ? PASSWORD_TOO_SHORT_MESSAGE_AR : PASSWORD_TOO_SHORT_MESSAGE_EN
          });
          return;
        }
        if (
          validation.violation.kind === "duplicate_email" ||
          validation.violation.kind === "duplicate_phone"
        ) {
          showDuplicate();
          return;
        }
        setFeedback({
          kind: "error",
          message: isAr ? "أدخل بريدًا إلكترونيًا صالحًا." : "Enter a valid email address."
        });
        return;
      }
    }

    setBusy(true);
    try {
      if (mode === "signup") {
        await postSafiAuth<{ message: string }>("/register", {
          email: normalizedEmail,
          display_name: displayName.trim(),
          password
        });
        setPassword("");
        setFeedback({
          kind: "success",
          message: isAr
            ? "تم إنشاء الحساب. افحص بريدك لتأكيد العنوان ثم سجّل الدخول."
            : "Account created. Check your inbox to verify the address, then sign in."
        });
      } else {
        const device = await getSafiDeviceIdentity();
        const result = await postSafiAuth<SafiLoginResponse>("/login", {
          email: normalizedEmail,
          password,
          device_name: "Web browser",
          installation_id: device.installationId,
          platform: device.platform
        });
        const owner = isOwnerEmail(normalizedEmail);
        await onAuthenticated?.({ token: result.token, email: normalizedEmail, isOwner: owner });
        setOwnerRecognized(owner);
        setPassword("");
        if (owner) onOwnerDetected?.(normalizedEmail);
        setFeedback({
          kind: "success",
          message: owner
            ? isAr
              ? "👑 تم التعرف على حساب المالك الموثق وفتح جميع صلاحيات الإدارة."
              : "👑 Verified owner account recognized. Full administration is unlocked."
            : isAr
              ? "تم تسجيل الدخول بنجاح."
              : "Signed in successfully."
        });
      }
    } catch (error) {
      if (error instanceof SafiDeviceIdentityError) {
        setFeedback({
          kind: "error",
          message: isAr
            ? "تعذّر إنشاء هوية ثابتة لهذا التثبيت. اسمح باستخدام قاعدة بيانات المتصفح ثم أعد المحاولة."
            : "This installation could not create a stable device identity. Allow browser database storage and try again."
        });
      } else if (error instanceof SafiAuthConfigurationError) {
        setFeedback({
          kind: "error",
          message: isAr
            ? "عنوان خدمة الحساب غير مهيأ في هذا الإصدار. أعد تثبيت التطبيق بإعداد صالح أو تواصل مع الدعم."
            : "The account service address is not configured in this build. Reinstall with a valid setup or contact support."
        });
      } else if (error instanceof SafiAuthApiError && error.code === "SF-AUTH-008") {
        showDuplicate();
      } else if (
        error instanceof SafiAuthApiError &&
        (error.status === 400 || error.status === 422)
      ) {
        setFeedback({
          kind: "error",
          message: isAr ? PASSWORD_TOO_SHORT_MESSAGE_AR : PASSWORD_TOO_SHORT_MESSAGE_EN
        });
      } else if (error instanceof Error && error.message.includes("SF-SYS-031")) {
        setFeedback({
          kind: "error",
          message: isAr
            ? "فشل التحقق المحلي: عنوان خدمة المصادقة غير مهيأ (SF-SYS-031)."
            : "Native verification failed: account service address is not configured (SF-SYS-031)."
        });
      } else if (error instanceof Error && error.message.includes("SF-AUTH-005")) {
        setFeedback({
          kind: "error",
          message: isAr
            ? "تعذّر على النواة التحقق من الجلسة مع خادم المصادقة (SF-AUTH-005)."
            : "Native core could not reach the authentication server to verify the session (SF-AUTH-005)."
        });
      } else if (
        error instanceof Error &&
        (error.message.includes("SF-AUTH-001") ||
          error.message.includes("SF-AUTH-002") ||
          error.message.includes("SF-AUTH-003"))
      ) {
        setFeedback({
          kind: "error",
          message: isAr
            ? `تعذّر ربط الجلسة بنواة سطح المكتب (${error.message}).`
            : `Could not verify and bind the session in native desktop core (${error.message}).`
        });
      } else {
        setFeedback({
          kind: "error",
          message: isAr
            ? "تعذّر إكمال الطلب الآن. تحقق من بياناتك وحاول مرة أخرى."
            : "We could not complete that request. Check your details and try again."
        });
      }
    } finally {
      setBusy(false);
    }
  };

  const signOut = async (): Promise<void> => {
    if (sessionToken === null || busy) return;
    setBusy(true);
    let remoteRevoked = true;
    try {
      try {
        await deleteSafiAuth("/sessions/current", sessionToken);
      } catch {
        remoteRevoked = false;
      }
      await onSignedOut?.();
      setFeedback({
        kind: "success",
        message: remoteRevoked
          ? isAr
            ? "تم تسجيل الخروج."
            : "You have signed out."
          : isAr
            ? "تم مسح الجلسة محليًا (تعذّر الوصول للخادم لإلغائها عن بُعد)."
            : "Signed out locally (could not reach the server to revoke remotely)."
      });
    } finally {
      setBusy(false);
    }
  };

  if (!open) return null;

  const ownerSession = sessionEmail !== null && isOwnerEmail(sessionEmail);

  return (
    <div
      className="safi-auth-backdrop"
      data-lenis-prevent="true"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget && !busy && !mandatory) onClose();
      }}
    >
      <div
        ref={panelRef}
        className="safi-auth-modal"
        data-safi-own-scrollbar="true"
        data-lenis-prevent="true"
        role="dialog"
        aria-modal="true"
        aria-labelledby="safi-auth-title"
        dir={isAr ? "rtl" : "ltr"}
      >
        {!mandatory ? (
          <button
            ref={closeRef}
            type="button"
            className="safi-auth-close"
            onClick={onClose}
            aria-label={isAr ? "إغلاق بوابة الحساب" : "Close account portal"}
          >
            <X size={18} aria-hidden="true" />
          </button>
        ) : null}

        <header ref={headerRef} className="safi-auth-header">
          <span className="safi-auth-eyebrow">
            <ShieldCheck size={14} aria-hidden="true" />
            {isAr ? "هوية صافي البيانات" : "SAFIDATA IDENTITY"}
          </span>
          <h2 id="safi-auth-title">
            {sessionToken !== null
              ? isAr
                ? "حسابك"
                : "Your account"
              : mode === "signup"
                ? isAr
                  ? "إنشاء حساب جديد"
                  : "Create your account"
                : isAr
                  ? "تسجيل الدخول"
                  : "Sign in"}
          </h2>
          <p className="safi-auth-subtitle">
            {sessionToken !== null
              ? isAr
                ? "جلسة آمنة في ذاكرة هذه الصفحة فقط."
                : "A secure session held in this page's memory only."
              : mode === "signup"
                ? isAr
                  ? `تجربة مجانية ${TRIAL_DAYS} أيام — بدون بطاقة بنكية.`
                  : `Free ${TRIAL_DAYS}-day trial — no credit card required.`
                : isAr
                  ? "أدخل إلى مساحة العمل النظيفة والآمنة لبياناتك."
                  : "Step into the clean, private workspace for your data."}
          </p>
        </header>

        {sessionToken !== null ? (
          <div className="safi-auth-session">
            {ownerSession ? (
              <div className="safi-owner-verified" role="status">
                <span aria-hidden="true">👑</span>
                <span>
                  {isAr
                    ? "حساب المالك الموثق (Owner — Mohamed Rabea)"
                    : "Verified Owner — Mohamed Rabea"}
                </span>
              </div>
            ) : null}
            <p>
              {isAr
                ? "أنت مسجل الدخول. لا نحفظ رمز الجلسة في التخزين المحلي أو ملفات تعريف الارتباط."
                : "You are signed in. The session token is never written to local storage or cookies."}
            </p>
            <button
              type="button"
              className="safi-btn safi-btn-secondary safi-auth-submit"
              onClick={() => void signOut()}
              disabled={busy}
            >
              {isAr ? "تسجيل الخروج" : "Sign out"}
            </button>
          </div>
        ) : (
          <>
            <div
              ref={tablistRef}
              className="safi-auth-tabs"
              role="tablist"
              aria-orientation="horizontal"
              aria-label={isAr ? "نوع الحساب" : "Account action"}
              onKeyDown={onTabKeyDown}
            >
              <button
                ref={signinTabRef}
                id={signinTabId}
                type="button"
                role="tab"
                aria-selected={mode === "signin"}
                aria-controls={authPanelId}
                tabIndex={mode === "signin" ? 0 : -1}
                className={mode === "signin" ? "is-active" : ""}
                onClick={() => switchMode("signin")}
              >
                <LogIn size={15} aria-hidden="true" />
                {isAr ? "تسجيل الدخول (Sign In)" : "Sign In"}
              </button>
              <button
                ref={signupTabRef}
                id={signupTabId}
                type="button"
                role="tab"
                aria-selected={mode === "signup"}
                aria-controls={authPanelId}
                tabIndex={mode === "signup" ? 0 : -1}
                className={mode === "signup" ? "is-active" : ""}
                onClick={() => switchMode("signup")}
              >
                <UserPlus size={15} aria-hidden="true" />
                {isAr
                  ? `إنشاء حساب جديد — تجربة مجانية ${TRIAL_DAYS} أيام (Sign Up)`
                  : `Create account — free ${TRIAL_DAYS}-day trial (Sign Up)`}
              </button>
              <span ref={tabIndicatorRef} className="safi-auth-tabs-indicator" aria-hidden="true" />
            </div>

            <div className="safi-auth-providers">
              {OAUTH_PROVIDERS.map((provider) => (
                <button
                  key={provider}
                  type="button"
                  data-auth-stagger="provider"
                  className={`safi-auth-provider safi-auth-provider--${provider}`}
                  onPointerMove={magnetize}
                  onPointerLeave={demagnetize}
                  onClick={() => {
                    if (onOAuth !== undefined) {
                      onOAuth(provider);
                      return;
                    }
                    setFeedback({
                      kind: "error",
                      message: isAr
                        ? `لم يتم ربط مزود ${provider} في هذه النسخة بعد. استخدم البريد وكلمة المرور.`
                        : `${provider} sign-in is not connected in this deployment yet. Use email and password.`
                    });
                  }}
                  aria-label={providerLabel(provider, isAr)}
                >
                  <ProviderMark provider={provider} />
                  <span>{providerLabel(provider, isAr)}</span>
                </button>
              ))}
            </div>

            <div className="safi-auth-divider" aria-hidden="true">
              <span>{isAr ? "أو بالبريد الإلكتروني" : "or with email"}</span>
            </div>

            <div
              ref={formStageRef}
              id={authPanelId}
              role="tabpanel"
              aria-labelledby={mode === "signin" ? signinTabId : signupTabId}
              className="safi-auth-form-stage"
              data-auth-mode={mode}
            >
              <form
                ref={formRef}
                className="safi-auth-form"
                onSubmit={(event) => void submit(event)}
              >
                <label className="safi-auth-field" data-auth-stagger="field">
                  <span>{isAr ? "البريد الإلكتروني" : "Email address"}</span>
                  <span className="safi-auth-input-wrap">
                    <Mail size={15} aria-hidden="true" />
                    <input
                      ref={emailInputRef}
                      type="email"
                      name="email"
                      autoComplete="email"
                      maxLength={254}
                      required
                      value={email}
                      onChange={(event) => {
                        const next = event.currentTarget.value;
                        setEmail(next);
                        setOwnerRecognized(isOwnerEmail(next));
                      }}
                      placeholder="name@example.com"
                      disabled={busy}
                      dir="ltr"
                    />
                    {ownerRecognized ? (
                      <span className="safi-auth-owner-hint" role="status">
                        👑 {isAr ? "حساب المالك" : "Owner"}
                      </span>
                    ) : null}
                  </span>
                </label>

                {mode === "signup" ? (
                  <div className="safi-auth-signup-row" data-auth-stagger="field">
                    <label className="safi-auth-field">
                      <span>{isAr ? "الاسم الظاهر" : "Display name"}</span>
                      <input
                        type="text"
                        name="display_name"
                        autoComplete="name"
                        maxLength={80}
                        required
                        value={displayName}
                        onChange={(event) => setDisplayName(event.currentTarget.value)}
                        disabled={busy}
                      />
                    </label>
                    <label className="safi-auth-field">
                      <span>
                        {isAr ? "رقم الهاتف (اختياري للتحقق)" : "Phone (optional verification)"}
                      </span>
                      <input
                        type="tel"
                        name="phone"
                        autoComplete="tel"
                        maxLength={20}
                        value={phone}
                        onChange={(event) => setPhone(event.currentTarget.value)}
                        placeholder="+20 100 000 0000"
                        disabled={busy}
                        dir="ltr"
                      />
                    </label>
                  </div>
                ) : null}

                <label className="safi-auth-field" data-auth-stagger="field">
                  <span>{isAr ? "كلمة المرور" : "Password"}</span>
                  <span className="safi-auth-input-wrap">
                    <LockKeyhole size={15} aria-hidden="true" />
                    <input
                      type={showPassword ? "text" : "password"}
                      name="password"
                      autoComplete={mode === "signup" ? "new-password" : "current-password"}
                      minLength={mode === "signup" ? MIN_PASSWORD_LENGTH : undefined}
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
                      title={
                        showPassword
                          ? isAr
                            ? "إخفاء كلمة المرور"
                            : "Hide password"
                          : isAr
                            ? "إظهار كلمة المرور"
                            : "Show password"
                      }
                    >
                      <span
                        className="safi-auth-eye-icon"
                        data-visible={showPassword ? "true" : "false"}
                      >
                        {showPassword ? (
                          <EyeOff size={17} aria-hidden="true" />
                        ) : (
                          <Eye size={17} aria-hidden="true" />
                        )}
                      </span>
                    </button>
                  </span>
                </label>

                {mode === "signin" && onForgotPassword !== undefined ? (
                  <button
                    type="button"
                    className="safi-auth-forgot"
                    data-auth-stagger="meta"
                    onClick={onForgotPassword}
                    disabled={busy}
                  >
                    {isAr ? "نسيت كلمة المرور؟" : "Forgot password?"}
                  </button>
                ) : null}

                {mode === "signup" ? (
                  <div
                    className={`safi-auth-password-meter ${passwordReady ? "is-ready" : ""}`}
                    data-auth-stagger="meta"
                    role="status"
                    aria-live="polite"
                  >
                    <span className="safi-auth-meter-dot">
                      {passwordReady ? <Check size={13} aria-hidden="true" /> : null}
                    </span>
                    <span>
                      {passwordReady
                        ? isAr
                          ? "✓ كلمة المرور مستوفية (8 خانات على الأقل)"
                          : "✓ Password meets the 8-character minimum"
                        : isAr
                          ? `8 خانات كحد أدنى — أدخلت ${Array.from(password).length}`
                          : `At least 8 characters — ${Array.from(password).length} entered`}
                    </span>
                  </div>
                ) : null}

                {feedback !== null ? (
                  <div
                    className={`safi-auth-feedback safi-auth-feedback--${feedback.kind}`}
                    role={feedback.kind === "success" ? "status" : "alert"}
                  >
                    <span>{feedback.message}</span>
                    {feedback.kind === "duplicate" ? (
                      <button
                        type="button"
                        className="safi-auth-switch"
                        onClick={() => switchMode("signin")}
                      >
                        {isAr ? "الانتقال إلى تسجيل الدخول" : "Go to sign in"}
                        <ArrowRight size={14} aria-hidden="true" />
                      </button>
                    ) : null}
                  </div>
                ) : null}

                <button
                  type="submit"
                  className="safi-btn safi-btn-primary safi-auth-submit"
                  disabled={busy || (mode === "signup" && !passwordReady)}
                  onPointerMove={magnetize}
                  onPointerLeave={demagnetize}
                  data-auth-stagger="submit"
                >
                  {busy ? (
                    <LoaderCircle size={16} className="safi-auth-spinner" aria-hidden="true" />
                  ) : mode === "signup" ? (
                    <Sparkles size={16} aria-hidden="true" />
                  ) : (
                    <LogIn size={16} aria-hidden="true" />
                  )}
                  {busy
                    ? isAr
                      ? "جارٍ المعالجة..."
                      : "Working…"
                    : mode === "signup"
                      ? isAr
                        ? "إنشاء الحساب وبدء التجربة"
                        : "Create account and start trial"
                      : isAr
                        ? "تسجيل الدخول"
                        : "Sign in"}
                </button>
              </form>
            </div>

            <p className="safi-auth-legal">
              {isAr
                ? "بالمتابعة أنت توافق على سياسة الخصوصية. بيانات ملفاتك لا تترك جهازك."
                : "By continuing you accept the privacy policy. Your file data never leaves your device."}
            </p>
            <p className="safi-auth-owner-email" dir="ltr">
              {OWNER_EMAIL}
            </p>
          </>
        )}
      </div>
    </div>
  );
}
