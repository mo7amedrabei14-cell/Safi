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
  isDesktopRuntime,
  setStudioAuthSession
} from "./desktop/studio-bridge";
import { OwnerDashboard } from "./pages/OwnerDashboard";
import { canAnimate } from "./components/ui/motion-preference";
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
  }
): ReactNode {
  const desktopRuntime = options?.desktopRuntime ?? false;
  const sessionToken = options?.sessionToken ?? null;
  const isOwner = options?.isOwner ?? false;

  switch (tab) {
    case "home":
      return (
        <BentoCommandCenter
          lang={lang}
          onStart={() => onSelectTab("cleaning")}
          onOpenLibrary={() => onSelectTab("library")}
        />
      );
    case "cleaning":
      return <CleaningStudio lang={lang} onSelectTab={onSelectTab} />;
    case "recipes":
      return <RecipesStudio lang={lang} onSelectTab={onSelectTab} />;
    case "charts":
      return <ChartsStudio lang={lang} onSelectTab={onSelectTab} />;
    case "studio":
      return desktopRuntime ? (
        <DesktopStudio lang={lang} />
      ) : (
        <BentoCommandCenter
          lang={lang}
          onStart={() => onSelectTab("cleaning")}
          onOpenLibrary={() => onSelectTab("library")}
        />
      );
    case "library":
      return <LibraryStudio lang={lang} onSelectTab={onSelectTab} />;
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
          onStart={() => onSelectTab("cleaning")}
          onOpenLibrary={() => onSelectTab("library")}
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
  readonly initialVerificationToken?: string | null;
  readonly initialResetToken?: string | null;
}

export function ApplicationShell({
  onReady,
  introComplete = true,
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
  const stageRef = useRef<HTMLDivElement | null>(null);

  useAnimatedFavicon();

  useEffect(() => {
    onReady?.();
  }, [onReady]);

  // Strict Phase 01 boundary:
  // - Browser runtime: Showcase tabs only (NO Quad-Studio, NO Owner Console).
  // - Desktop runtime: Showcase + Quad-Studio, plus Owner Console ONLY for server-verified Owner.
  const dockTabs = useMemo(() => {
    if (!desktopRuntime) {
      return SHOWCASE_TABS;
    }
    return ownerAccount
      ? [...SHOWCASE_TABS, STUDIO_TAB, OWNER_TAB]
      : [...SHOWCASE_TABS, STUDIO_TAB];
  }, [desktopRuntime, ownerAccount]);

  const selectTab = useCallback(
    (tab: SafiDockTabId): void => {
      if (tab === "studio" && !desktopRuntime) {
        setActiveTab("home");
        return;
      }
      if (tab === "owner" && (!desktopRuntime || !ownerAccount)) {
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
    const stage = stageRef.current;
    if (stage === null || !canAnimate()) {
      return;
    }
    const tween = gsap.fromTo(
      stage,
      { opacity: 0.35, y: 10 },
      {
        opacity: 1,
        y: 0,
        duration: 0.28,
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

  // Phase 01 runtime separation: Desktop mandates product login; public browser Showcase opens freely.
  const hasActiveSession = sessionToken !== null && sessionToken.trim().length > 0;
  const authenticationRequired = desktopRuntime && !hasActiveSession;
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
    },
    [desktopRuntime]
  );

  const clearSessionAndLock = useCallback(async (): Promise<void> => {
    try {
      if (desktopRuntime) {
        await clearStudioAuthSession();
      }
    } finally {
      setSessionToken(null);
      setSessionEmail(null);
      setOwnerAccount(false);
      if (activeTab === "owner") {
        setActiveTab("home");
      }
    }
  }, [desktopRuntime, activeTab]);

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

  const authPortal = (
    <AuthModal
      open={authModalOpen}
      mandatory={authenticationRequired}
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

  if (!introComplete) {
    return null;
  }

  if (authenticationRequired) {
    return (
      <>
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
                isOwner: ownerAccount
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
