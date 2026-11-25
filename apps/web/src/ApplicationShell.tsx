/**
 * `apps/web/src/ApplicationShell.tsx`
 *
 * SafiData Runtime Boundary Shell (Phase 01):
 * - Synchronous runtime detection (`isDesktopRuntime()`) with zero first-render flash.
 * - Public browser build renders the Showcase freely without mandatory product login and
 *   NEVER exposes Desktop Quad-Studio or Owner Control Plane controls.
 * - Packaged Desktop build enforces mandatory login, binds the native session, exposes
 *   Quad-Studio, and exposes the Owner Control Plane ONLY when the server confirms Owner
 *   status via authenticated session metadata (zero hardcoded owner email on client).
 */
import type { ReactNode } from "react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { gsap } from "gsap";
import { AmbientBackground } from "./components/motion/AmbientBackground";
import { SmoothScrollProvider } from "./components/motion/SmoothScrollProvider";
import {
  DOCK_TABS,
  FloatingGlassDock,
  OWNER_TAB,
  SHOWCASE_TABS,
  STUDIO_TAB
} from "./components/layout/FloatingGlassDock";
import type { SafiDockTabId, SafiLang } from "./components/layout/FloatingGlassDock";
import { CommandPalette } from "./components/layout/CommandPalette";
import { SettingsSheet } from "./components/layout/SettingsSheet";
import { AccountAccessSheet } from "./components/layout/AccountAccessSheet";
import { AuthModal } from "./components/auth/AuthModal";
import { PasswordResetModal } from "./components/auth/PasswordResetModal";
import { useAnimatedFavicon } from "./components/brand/useAnimatedFavicon";
import { BentoCommandCenter } from "./components/home/BentoCommandCenter";
import { CleaningStudio } from "./components/studios/CleaningStudio";
import { RecipesStudio } from "./components/studios/RecipesStudio";
import { ChartsStudio } from "./components/studios/ChartsStudio";
import { LibraryStudio } from "./components/studios/LibraryStudio";
import { DesktopStudio } from "./desktop/DesktopStudio";
import {
  clearStudioAuthSession,
  enterStudioOfflineReadOnly,
  getStudioAuthorizationStatus,
  isDesktopRuntime,
  revalidateStudioSession,
  setStudioAuthSession,
  takePendingDeepLink
} from "./desktop/studio-bridge";
import {
  clearPendingDesktopPkceFlow,
  completeDesktopBrowserPkce,
  startDesktopBrowserPkce,
  type PendingDesktopPkceFlow
} from "./lib/desktop-pkce";
import { OwnerDashboard } from "./pages/OwnerDashboard";
import { OwnerWebPortal } from "./pages/OwnerWebPortal";
import { DesktopBrowserAuthPage } from "./pages/DesktopBrowserAuthPage";
import { BillingPortalPage } from "./pages/BillingPortalPage";
import { PricingPage } from "./pages/PricingPage";
import { OrganizationWorkspacePanel } from "./components/org/OrganizationWorkspacePanel";
import { buildBillingUrlFromIntent, loadPendingWebPurchaseIntent } from "./lib/business-pricing";
import { canAnimate } from "./components/ui/motion-preference";
import { postSafiHeartbeat, SafiAuthApiError } from "./lib/auth-api";
import { getSafiDeviceIdentity } from "./lib/device-identity";
import { applySafiLang, getInitialSafiLang } from "./lib/locale";
import { applySafiTheme, getInitialSafiTheme, syncDomTheme, type SafiThemeId } from "./lib/theme";

export const SHELL_TABS: readonly SafiDockTabId[] = DOCK_TABS.map((tab) => tab.id);

export function renderStudio(
  tab: SafiDockTabId,
  onSelectTab: (tab: SafiDockTabId) => void,
  lang: SafiLang = "en",
  options?: {
    readonly desktopRuntime?: boolean;
    readonly sessionToken?: string | null;
    readonly isOwner?: boolean;
    readonly readOnlyOffline?: boolean;
    readonly entitlementRefreshNonce?: number;
    readonly onRevalidateOnline?: () => void;
    readonly onOpenPricing?: () => void;
  }
): ReactNode {
  const desktopRuntime = options?.desktopRuntime ?? false;
  const sessionToken = options?.sessionToken ?? null;
  const isOwner = options?.isOwner ?? false;
  const readOnlyOffline = options?.readOnlyOffline ?? false;
  const onOpenPricing = options?.onOpenPricing;

  switch (tab) {
    case "home":
      return (
        <BentoCommandCenter
          lang={lang}
          onStart={() => onSelectTab("cleaning")}
          onOpenLibrary={() => onSelectTab("library")}
          {...(onOpenPricing !== undefined ? { onOpenPricing } : {})}
        />
      );
    case "cleaning":
      return desktopRuntime ? (
        <CleaningStudio lang={lang} onSelectTab={onSelectTab} />
      ) : (
        <BentoCommandCenter
          lang={lang}
          onStart={() => onSelectTab("home")}
          onOpenLibrary={() => onSelectTab("home")}
        />
      );
    case "recipes":
      return desktopRuntime ? (
        <RecipesStudio lang={lang} onSelectTab={onSelectTab} />
      ) : (
        <BentoCommandCenter
          lang={lang}
          onStart={() => onSelectTab("home")}
          onOpenLibrary={() => onSelectTab("home")}
        />
      );
    case "charts":
      return desktopRuntime ? (
        <ChartsStudio lang={lang} onSelectTab={onSelectTab} />
      ) : (
        <BentoCommandCenter
          lang={lang}
          onStart={() => onSelectTab("home")}
          onOpenLibrary={() => onSelectTab("home")}
        />
      );
    case "studio":
      return desktopRuntime ? (
        <DesktopStudio
          lang={lang}
          sessionToken={sessionToken}
          readOnlyOffline={readOnlyOffline}
          entitlementRefreshNonce={options?.entitlementRefreshNonce ?? 0}
          onRevalidateOnline={options?.onRevalidateOnline}
        />
      ) : (
        <BentoCommandCenter
          lang={lang}
          onStart={() => onSelectTab("home")}
          onOpenLibrary={() => onSelectTab("home")}
        />
      );
    case "library":
      return desktopRuntime ? (
        <LibraryStudio lang={lang} onSelectTab={onSelectTab} />
      ) : (
        <BentoCommandCenter
          lang={lang}
          onStart={() => onSelectTab("home")}
          onOpenLibrary={() => onSelectTab("home")}
        />
      );
    case "owner":
      return desktopRuntime && isOwner ? (
        <div className="safi-owner-frame">
          <OwnerDashboard
            lang={lang}
            onSelectTab={onSelectTab}
            sessionToken={sessionToken}
            isOwner={isOwner}
          />
        </div>
      ) : (
        <BentoCommandCenter
          lang={lang}
          onStart={() => onSelectTab("home")}
          onOpenLibrary={() => onSelectTab("home")}
        />
      );
    default: {
      const exhaustive: never = tab;
      return exhaustive;
    }
  }
}

export interface ApplicationShellProps {
  readonly onReady?: (() => void) | undefined;
  readonly introComplete?: boolean | undefined;
  readonly showcaseUnderlayReady?: boolean | undefined;
  readonly initialVerificationToken?: string | null;
  readonly initialResetToken?: string | null;
}

export function ApplicationShell({
  onReady,
  introComplete = true,
  showcaseUnderlayReady = introComplete,
  initialVerificationToken = null,
  initialResetToken = null
}: ApplicationShellProps): ReactNode {
  const [desktopRuntime] = useState<boolean>(() => isDesktopRuntime());
  const [activeTab, setActiveTab] = useState<SafiDockTabId>("home");
  const [theme, setTheme] = useState<SafiThemeId>(getInitialSafiTheme);
  const [lang, setLang] = useState<SafiLang>(getInitialSafiLang);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [accountOpen, setAccountOpen] = useState(initialVerificationToken !== null);
  const [resetOpen, setResetOpen] = useState(initialResetToken !== null);
  const [sessionToken, setSessionToken] = useState<string | null>(null);
  const [sessionEmail, setSessionEmail] = useState<string | null>(null);
  const [ownerAccount, setOwnerAccount] = useState(false);
  const [offlineReadOnly, setOfflineReadOnly] = useState(false);
  const [hasPreviouslyAuthenticated, setHasPreviouslyAuthenticated] = useState(false);
  const [revocationNotice, setRevocationNotice] = useState<string | null>(null);
  const [pkcePending, setPkcePending] = useState<PendingDesktopPkceFlow | null>(null);
  const [pkceError, setPkceError] = useState<string | null>(null);
  const [entitlementRefreshNonce, setEntitlementRefreshNonce] = useState(0);
  const [browserRoute, setBrowserRoute] = useState<
    "showcase" | "pricing" | "owner" | "desktop-auth" | "billing" | "team"
  >(() => {
    if (typeof window === "undefined" || isDesktopRuntime()) {
      return "showcase";
    }
    const path = window.location.pathname.replace(/\/+$/, "").toLowerCase();
    if (path === "/owner") return "owner";
    if (path === "/auth/desktop") return "desktop-auth";
    if (path === "/billing") return "billing";
    if (path === "/pricing") return "pricing";
    if (path === "/team") return "team";
    try {
      if (
        initialVerificationToken !== null &&
        (window.sessionStorage.getItem("safi.org.pending_invite.v1") ?? "").startsWith("sfi_")
      ) {
        return "team";
      }
    } catch {
      // Ignore storage lookup error.
    }
    // Preserve direct website purchase flow across email-verification link navigation:
    // if the customer clicked a verification link while holding a pending web purchase intent,
    // resume directly in `/billing` with their selected plan, currency, and seat count intact!
    if (initialVerificationToken !== null && loadPendingWebPurchaseIntent() !== null) {
      return "billing";
    }
    return "showcase";
  });

  useEffect(() => {
    if (typeof window === "undefined" || desktopRuntime) return;
    const onPopState = (): void => {
      const path = window.location.pathname.replace(/\/+$/, "").toLowerCase();
      if (path === "/owner") setBrowserRoute("owner");
      else if (path === "/auth/desktop") setBrowserRoute("desktop-auth");
      else if (path === "/billing") setBrowserRoute("billing");
      else if (path === "/pricing") setBrowserRoute("pricing");
      else if (path === "/team") setBrowserRoute("team");
      else setBrowserRoute("showcase");
    };
    window.addEventListener("popstate", onPopState);
    return () => {
      window.removeEventListener("popstate", onPopState);
    };
  }, [desktopRuntime]);

  const navigateToPricing = useCallback((): void => {
    if (typeof window !== "undefined") {
      window.history.pushState({}, "", "/pricing");
      window.scrollTo?.({ top: 0, behavior: "auto" });
    }
    setBrowserRoute("pricing");
  }, []);

  const navigateToTeam = useCallback((): void => {
    if (typeof window !== "undefined") {
      window.history.pushState({}, "", "/team");
      window.scrollTo?.({ top: 0, behavior: "auto" });
    }
    setBrowserRoute("team");
  }, []);

  const navigateToShowcase = useCallback((hash?: string): void => {
    if (typeof window !== "undefined") {
      const target = hash && hash.startsWith("#") ? `/${hash}` : "/";
      window.history.pushState({}, "", target);
      if (hash && hash.startsWith("#")) {
        window.setTimeout(() => {
          document.getElementById(hash.slice(1))?.scrollIntoView({ behavior: "smooth" });
        }, 40);
      } else {
        window.scrollTo?.({ top: 0, behavior: "auto" });
      }
    }
    setBrowserRoute("showcase");
  }, []);

  const navigateToBillingWithIntent = useCallback(
    (intent: {
      readonly product:
        | "individual_monthly"
        | "individual_annual"
        | "organization_seat_annual"
        | "credit_pack_25"
        | "pay_per_file";
      readonly billing_country_iso2: "US" | "EG";
      readonly seats: number;
    }): void => {
      if (typeof window !== "undefined") {
        window.history.pushState({}, "", buildBillingUrlFromIntent(intent));
        window.scrollTo?.({ top: 0, behavior: "auto" });
      }
      setBrowserRoute("billing");
    },
    []
  );
  const stageRef = useRef<HTMLDivElement | null>(null);
  const initialStageMountRef = useRef(true);

  useAnimatedFavicon();

  useEffect(() => {
    onReady?.();
  }, [onReady]);

  // Query native Desktop authorization state once on startup so fresh unauthenticated installs
  // never expose the Offline Read-Only Mode button.
  useEffect(() => {
    if (!desktopRuntime) return;
    let active = true;
    void getStudioAuthorizationStatus()
      .then((status) => {
        if (active) {
          setHasPreviouslyAuthenticated(Boolean(status.hasPreviouslyAuthenticated));
        }
      })
      .catch(() => {
        if (active) {
          setHasPreviouslyAuthenticated(false);
        }
      });
    return () => {
      active = false;
    };
  }, [desktopRuntime]);

  // Strict Runtime Boundary:
  // - Browser runtime: Showcase website ONLY (NO Cleaning/Recipes/Charts/Library/Quad-Studio).
  //   Owner web administration lives strictly at the protected `/owner` route.
  // - Desktop runtime: Full local studios + Quad-Studio, plus Owner tab ONLY for server-verified Owner.
  const dockTabs = useMemo(() => {
    if (!desktopRuntime) {
      return SHOWCASE_TABS.slice(0, 1);
    }
    return ownerAccount
      ? [...SHOWCASE_TABS, STUDIO_TAB, OWNER_TAB]
      : [...SHOWCASE_TABS, STUDIO_TAB];
  }, [desktopRuntime, ownerAccount]);

  const selectTab = useCallback(
    (tab: SafiDockTabId): void => {
      if (!desktopRuntime) {
        setActiveTab("home");
        return;
      }
      if (tab === "owner" && !ownerAccount) {
        setActiveTab("home");
        return;
      }
      setActiveTab(tab);
    },
    [desktopRuntime, ownerAccount]
  );

  const selectTheme = useCallback((nextTheme: SafiThemeId): void => {
    applySafiTheme(nextTheme);
    setTheme(nextTheme);
  }, []);

  const toggleLang = useCallback((): void => {
    setLang((prev) => {
      const next = prev === "ar" ? "en" : "ar";
      applySafiLang(next);
      return next;
    });
  }, []);

  useEffect(() => {
    syncDomTheme(theme);
  }, [theme]);

  useEffect(() => {
    applySafiLang(lang);
  }, [lang]);

  useEffect(() => {
    if (desktopRuntime && sessionToken === null) return;
    const onKeyDown = (event: KeyboardEvent): void => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setSettingsOpen(false);
        setPaletteOpen(true);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [desktopRuntime, sessionToken]);

  useEffect(() => {
    if (initialStageMountRef.current) {
      initialStageMountRef.current = false;
      return;
    }
    const stage = stageRef.current;
    if (stage === null || !canAnimate()) {
      return;
    }
    const tween = gsap.fromTo(
      stage,
      { opacity: 0.82, y: 6 },
      {
        opacity: 1,
        y: 0,
        duration: 0.2,
        ease: "power2.out",
        clearProps: "transform,opacity"
      }
    );
    return () => {
      tween.kill();
      gsap.set(stage, { clearProps: "transform,opacity" });
    };
  }, [activeTab, lang]);

  const activeTabConfig = dockTabs.find((tab) => tab.id === activeTab);
  const activeLabel =
    lang === "ar"
      ? (activeTabConfig?.label ?? "الرئيسية")
      : (activeTabConfig?.labelEn ?? "Website");

  // Phase 01 runtime separation: Desktop mandates product login unless user explicitly enters
  // Offline Read-Only Mode to inspect previously saved local projects.
  const hasActiveSession = sessionToken !== null && sessionToken.trim().length > 0;
  const authenticationRequired = desktopRuntime && !hasActiveSession && !offlineReadOnly;
  const verificationSheetOpen = initialVerificationToken !== null && accountOpen && !resetOpen;
  const authModalOpen =
    !resetOpen && !verificationSheetOpen && (authenticationRequired || accountOpen);

  const bindSessionAndUnlock = useCallback(
    async (token: string, email: string, owner: boolean): Promise<void> => {
      if (desktopRuntime) {
        const device = await getSafiDeviceIdentity();
        await setStudioAuthSession(token, device.installationId);
      }
      setSessionToken(token);
      setSessionEmail(email);
      setOwnerAccount(desktopRuntime && owner);
      setHasPreviouslyAuthenticated(true);
      setOfflineReadOnly(false);
      setRevocationNotice(null);
      setAccountOpen(false);
    },
    [desktopRuntime]
  );

  const clearSessionAndLock = useCallback(
    async (notice?: string): Promise<void> => {
      try {
        if (desktopRuntime) {
          await clearStudioAuthSession();
        }
      } catch {
        // Always lock client state even if native IPC clear throws.
      } finally {
        setSessionToken(null);
        setSessionEmail(null);
        setOwnerAccount(false);
        setOfflineReadOnly(false);
        if (notice !== undefined) {
          setRevocationNotice(notice);
        }
        if (activeTab === "owner") {
          setActiveTab("home");
        }
      }
    },
    [desktopRuntime, activeTab]
  );

  const enterOfflineMode = useCallback(async (): Promise<void> => {
    if (!desktopRuntime) return;
    if (!hasPreviouslyAuthenticated && !hasActiveSession) {
      return;
    }
    try {
      await enterStudioOfflineReadOnly();
    } catch {
      return;
    }
    setOfflineReadOnly(true);
    setActiveTab("studio");
  }, [desktopRuntime, hasPreviouslyAuthenticated, hasActiveSession]);

  const revalidateOnlineAccess = useCallback(async (): Promise<void> => {
    if (!desktopRuntime || !sessionToken) return;
    if (typeof navigator !== "undefined" && !navigator.onLine) {
      await enterOfflineMode();
      return;
    }
    try {
      await postSafiHeartbeat(sessionToken);
      try {
        await revalidateStudioSession();
      } catch {
        // Native store may already be synchronized or in test harness.
      }
      setOfflineReadOnly(false);
      setRevocationNotice(null);
    } catch (err) {
      if (err instanceof SafiAuthApiError) {
        if (err.status === 0 || err.code === "SF-AUTH-005") {
          await enterOfflineMode();
          return;
        }
        if (err.code === "SF-AUTH-019") {
          await clearSessionAndLock(
            lang === "ar"
              ? "تم تفعيل حسابك على جهاز آخر. تم تسجيل خروج هذا الجهاز تلقائيًا (SF-AUTH-019)."
              : "Your account was activated on another device. This device has been signed out automatically (SF-AUTH-019)."
          );
          return;
        }
        if (
          err.code === "SF-AUTH-003" ||
          err.code === "SF-AUTH-009" ||
          err.code === "SF-AUTH-002" ||
          err.status === 401 ||
          err.status === 403
        ) {
          await clearSessionAndLock(
            lang === "ar"
              ? `انتهت صلاحية الجلسة أو تم إلغاؤها من الخادم (${err.code ?? `HTTP ${err.status}`}). يرجى تسجيل الدخول مرة أخرى.`
              : `Your session is no longer authorized by the server (${err.code ?? `HTTP ${err.status}`}). Please sign in again.`
          );
          return;
        }
      }
      await enterOfflineMode();
    }
  }, [desktopRuntime, sessionToken, lang, enterOfflineMode, clearSessionAndLock]);

  // Automatic 30-second online authorization heartbeat + instant offline/online transition listeners.
  useEffect(() => {
    if (!desktopRuntime || !sessionToken) return;
    const onOffline = (): void => {
      void enterOfflineMode();
    };
    const onOnline = (): void => {
      void revalidateOnlineAccess();
    };
    window.addEventListener("offline", onOffline);
    window.addEventListener("online", onOnline);
    const interval = window.setInterval(() => {
      void revalidateOnlineAccess();
    }, 30_000);
    return () => {
      window.removeEventListener("offline", onOffline);
      window.removeEventListener("online", onOnline);
      window.clearInterval(interval);
    };
  }, [desktopRuntime, sessionToken, enterOfflineMode, revalidateOnlineAccess]);

  const accountAccessPortal =
    initialVerificationToken === null ? null : (
      <AccountAccessSheet
        open={verificationSheetOpen}
        onClose={() => setAccountOpen(false)}
        lang={lang}
        verificationToken={initialVerificationToken}
        sessionToken={sessionToken}
        onAuthenticated={(token, email) => bindSessionAndUnlock(token, email, false)}
        onLoggedOut={() => clearSessionAndLock()}
      />
    );

  const handleStartDesktopPkce = useCallback(
    async (flow: "signin" | "signup"): Promise<void> => {
      if (!desktopRuntime) return;
      setPkceError(null);
      try {
        const pending = await startDesktopBrowserPkce(flow);
        setPkcePending(pending);
      } catch (err) {
        clearPendingDesktopPkceFlow();
        setPkcePending(null);
        const code =
          err instanceof SafiAuthApiError
            ? (err.code ?? `HTTP ${err.status}`)
            : err instanceof Error
              ? err.message
              : "SF-AUTH-023";
        setPkceError(
          lang === "ar"
            ? `تعذّر بدء مصادقة المتصفح الآمنة (${code}).`
            : `Could not launch system-browser PKCE authentication (${code}).`
        );
      }
    },
    [desktopRuntime, lang]
  );

  const handleCancelDesktopPkce = useCallback((): void => {
    clearPendingDesktopPkceFlow();
    setPkcePending(null);
    setPkceError(null);
  }, []);

  // Poll `take_pending_deep_link` while a Desktop Browser PKCE flow is awaiting `safi://auth`.
  useEffect(() => {
    if (!desktopRuntime || pkcePending === null) return;
    let active = true;
    const poll = async (): Promise<void> => {
      if (!active) return;
      try {
        const link = await takePendingDeepLink();
        if (!active || link === null) return;
        if (
          link.action === "auth_callback" &&
          typeof link.code === "string" &&
          typeof link.state === "string"
        ) {
          try {
            const exchanged = await completeDesktopBrowserPkce(link.code, link.state);
            await bindSessionAndUnlock(exchanged.token, exchanged.email ?? "", exchanged.is_owner);
            if (active) {
              setPkcePending(null);
              setPkceError(null);
              setAccountOpen(false);
            }
          } catch (err) {
            clearPendingDesktopPkceFlow();
            const code =
              err instanceof SafiAuthApiError
                ? (err.code ?? `HTTP ${err.status}`)
                : err instanceof Error
                  ? err.message
                  : "SF-AUTH-023";
            if (active) {
              setPkcePending(null);
              setPkceError(
                lang === "ar"
                  ? `فشل التحقق من رمز PKCE (${code}). بقي تطبيق سطح المكتب مقفلًا.`
                  : `PKCE callback exchange rejected (${code}). Desktop workspace remains locked.`
              );
            }
          }
        }
      } catch {
        // Ignore transient poll errors while waiting for browser callback.
      }
    };
    const timer = window.setInterval(() => {
      void poll();
    }, 350);
    void poll();
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [desktopRuntime, pkcePending, bindSessionAndUnlock, lang]);

  // Poll `take_pending_deep_link` while authenticated in Desktop for `safi://billing` return
  // notifications, then independently refetch authoritative entitlements via `GET /v1/entitlements/me`
  // (NEVER trusting the deep link as proof of payment!).
  useEffect(() => {
    if (!desktopRuntime || !hasActiveSession || pkcePending !== null) return;
    let active = true;
    const pollBillingReturn = async (): Promise<void> => {
      if (!active) return;
      try {
        const link = await takePendingDeepLink();
        if (!active || link === null) return;
        if (link.action === "billing_return") {
          setEntitlementRefreshNonce((prev) => prev + 1);
          void revalidateOnlineAccess();
        }
      } catch {
        // Ignore transient poll errors.
      }
    };
    const timer = window.setInterval(() => {
      void pollBillingReturn();
    }, 1000);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [desktopRuntime, hasActiveSession, pkcePending, revalidateOnlineAccess]);

  const authPortal = (
    <AuthModal
      open={authModalOpen}
      mandatory={authenticationRequired}
      desktopRuntime={desktopRuntime}
      pkcePending={pkcePending !== null}
      pkceError={pkceError}
      onStartDesktopPkce={handleStartDesktopPkce}
      onCancelDesktopPkce={handleCancelDesktopPkce}
      onClose={() => setAccountOpen(false)}
      lang={lang}
      theme={theme}
      sessionToken={sessionToken}
      sessionEmail={sessionEmail}
      sessionIsOwner={ownerAccount}
      onAuthenticated={(session) =>
        bindSessionAndUnlock(session.token, session.email, session.isOwner)
      }
      onOwnerDetected={() => {
        if (desktopRuntime) setOwnerAccount(true);
      }}
      onSignedOut={() => clearSessionAndLock()}
      onForgotPassword={() => {
        setAccountOpen(false);
        setResetOpen(true);
      }}
    />
  );

  const passwordResetPortal = (
    <PasswordResetModal
      open={resetOpen}
      lang={lang}
      initialToken={initialResetToken}
      onClose={() => setResetOpen(false)}
    />
  );

  if (authenticationRequired ? !introComplete : !showcaseUnderlayReady) {
    return null;
  }

  // Protected Website Owner Portal (`/owner`) and Desktop Browser PKCE Auth (`/auth/desktop`)
  if (!desktopRuntime && browserRoute === "owner") {
    return (
      <>
        <AmbientBackground />
        <div
          className="safi-shell"
          dir={lang === "ar" ? "rtl" : "ltr"}
          data-runtime="owner-web-portal"
        >
          <main id="safi-main" className="safi-container safi-stage">
            <OwnerWebPortal
              lang={lang}
              onBackToShowcase={() => {
                if (typeof window !== "undefined") {
                  window.history.pushState({}, "", "/");
                }
                setBrowserRoute("showcase");
              }}
            />
          </main>
        </div>
      </>
    );
  }

  if (!desktopRuntime && browserRoute === "desktop-auth") {
    return (
      <>
        <AmbientBackground />
        <div
          className="safi-shell"
          dir={lang === "ar" ? "rtl" : "ltr"}
          data-runtime="desktop-browser-auth"
        >
          <main id="safi-main" className="safi-container safi-stage">
            <DesktopBrowserAuthPage lang={lang} />
          </main>
        </div>
      </>
    );
  }

  if (!desktopRuntime && browserRoute === "pricing") {
    return (
      <>
        <AmbientBackground />
        <div
          className="safi-shell"
          dir={lang === "ar" ? "rtl" : "ltr"}
          data-runtime="pricing-public"
        >
          <FloatingGlassDock
            tabs={dockTabs}
            activeTab={activeTab}
            onSelect={selectTab}
            onOpenCommandPalette={() => setPaletteOpen(true)}
            onOpenSettings={() => setSettingsOpen(true)}
            theme={theme}
            onSelectTheme={selectTheme}
            lang={lang}
            onToggleLang={toggleLang}
            onOpenAccount={() => setAccountOpen(true)}
            onOpenPricing={navigateToPricing}
            onOpenTeam={navigateToTeam}
            onOpenHomeSection={(hash) => navigateToShowcase(hash)}
            activeBrowserRoute="pricing"
            authenticated={hasActiveSession}
            ownerAccount={false}
          />
          <main id="safi-main" className="safi-container safi-stage">
            <PricingPage
              lang={lang}
              onSelectPurchase={navigateToBillingWithIntent}
              onBackToShowcase={() => navigateToShowcase()}
              onDownloadDesktop={() => navigateToShowcase("#download")}
            />
          </main>
        </div>
        {accountAccessPortal}
        {authPortal}
        {passwordResetPortal}
      </>
    );
  }

  if (!desktopRuntime && browserRoute === "team") {
    return (
      <>
        <AmbientBackground />
        <div
          className="safi-shell"
          dir={lang === "ar" ? "rtl" : "ltr"}
          data-runtime="team-web-portal"
        >
          <FloatingGlassDock
            tabs={dockTabs}
            activeTab={activeTab}
            onSelect={selectTab}
            onOpenCommandPalette={() => setPaletteOpen(true)}
            onOpenSettings={() => setSettingsOpen(true)}
            theme={theme}
            onSelectTheme={selectTheme}
            lang={lang}
            onToggleLang={toggleLang}
            onOpenAccount={() => setAccountOpen(true)}
            onOpenPricing={navigateToPricing}
            onOpenTeam={navigateToTeam}
            onOpenHomeSection={(hash) => navigateToShowcase(hash)}
            activeBrowserRoute="team"
            authenticated={hasActiveSession}
            ownerAccount={false}
          />
          <main id="safi-main" className="safi-container safi-stage">
            <OrganizationWorkspacePanel
              lang={lang}
              sessionToken={sessionToken}
              desktopRuntime={false}
              onRequestSignIn={() => setAccountOpen(true)}
              onOpenBillingPortal={(seats) =>
                navigateToBillingWithIntent({
                  product: "organization_seat_annual",
                  billing_country_iso2: "US",
                  seats
                })
              }
              onBackToShowcase={() => navigateToShowcase()}
            />
          </main>
        </div>
        {accountAccessPortal}
        {authPortal}
        {passwordResetPortal}
      </>
    );
  }

  if (!desktopRuntime && browserRoute === "billing") {
    return (
      <>
        <AmbientBackground />
        <div
          className="safi-shell"
          dir={lang === "ar" ? "rtl" : "ltr"}
          data-runtime="billing-web-portal"
        >
          <main id="safi-main" className="safi-container safi-stage">
            <BillingPortalPage
              lang={lang}
              initialVerificationToken={initialVerificationToken}
              onOpenPricing={navigateToPricing}
              onOpenTeam={navigateToTeam}
              onBackToShowcase={() => navigateToShowcase()}
              onDownloadDesktop={() => navigateToShowcase("#download")}
            />
          </main>
        </div>
      </>
    );
  }

  if (authenticationRequired) {
    return (
      <>
        {revocationNotice !== null ? (
          <div
            role="alert"
            data-session-revoked-banner="true"
            style={{
              position: "fixed",
              top: "1rem",
              left: "50%",
              transform: "translateX(-50%)",
              zIndex: 9999,
              maxWidth: "36rem",
              padding: "0.875rem 1.25rem",
              borderRadius: "0.75rem",
              background: "var(--safi-surface-elevated, #1c1917)",
              border: "1px solid var(--safi-danger, #ef4444)",
              color: "var(--safi-text, #fafaf9)",
              fontWeight: 600,
              boxShadow: "0 12px 32px rgba(0,0,0,0.4)"
            }}
          >
            {revocationNotice}
          </div>
        ) : null}
        {hasPreviouslyAuthenticated ? (
          <div
            style={{
              position: "fixed",
              bottom: "1.25rem",
              left: "50%",
              transform: "translateX(-50%)",
              zIndex: 9998
            }}
          >
            <button
              type="button"
              className="safi-btn"
              data-enter-offline-readonly="true"
              onClick={() => void enterOfflineMode()}
            >
              {lang === "ar"
                ? "فتح المشاريع المحفوظة محليًا (وضع القراءة فقط بدون إنترنت)"
                : "Open Saved Local Projects (Offline Read-Only Mode)"}
            </button>
          </div>
        ) : null}
        {accountAccessPortal}
        {authPortal}
        {passwordResetPortal}
      </>
    );
  }

  return (
    <>
      <AmbientBackground />
      <SmoothScrollProvider>
        <div
          className="safi-shell"
          dir={lang === "ar" ? "rtl" : "ltr"}
          data-runtime={desktopRuntime ? "desktop" : "showcase"}
        >
          <FloatingGlassDock
            tabs={dockTabs}
            activeTab={activeTab}
            onSelect={selectTab}
            onOpenCommandPalette={() => setPaletteOpen(true)}
            onOpenSettings={() => setSettingsOpen(true)}
            theme={theme}
            onSelectTheme={selectTheme}
            lang={lang}
            onToggleLang={toggleLang}
            onOpenAccount={() => setAccountOpen(true)}
            onOpenPricing={navigateToPricing}
            onOpenTeam={navigateToTeam}
            onOpenHomeSection={(hash) => navigateToShowcase(hash)}
            activeBrowserRoute="showcase"
            authenticated={hasActiveSession}
            ownerAccount={desktopRuntime && ownerAccount}
          />

          <main id="safi-main" className="safi-container safi-stage">
            <p className="safi-visually-hidden" role="status">
              {lang === "ar" ? `الآن في ${activeLabel}` : `You are viewing ${activeLabel}`}
            </p>
            <div ref={stageRef} key={`${activeTab}-${lang}`} data-active-tab={activeTab}>
              {renderStudio(activeTab, selectTab, lang, {
                desktopRuntime,
                sessionToken,
                isOwner: ownerAccount,
                readOnlyOffline: offlineReadOnly,
                entitlementRefreshNonce,
                onRevalidateOnline: () => void revalidateOnlineAccess(),
                onOpenPricing: navigateToPricing
              })}
            </div>
          </main>

          <CommandPalette
            open={paletteOpen}
            onClose={() => setPaletteOpen(false)}
            onSelectTab={selectTab}
            lang={lang}
          />

          <SettingsSheet
            open={settingsOpen}
            onClose={() => setSettingsOpen(false)}
            lang={lang}
            theme={theme}
            onSelectTheme={selectTheme}
          />
        </div>
      </SmoothScrollProvider>
      {accountAccessPortal}
      {authPortal}
      {passwordResetPortal}
    </>
  );
}
