/**
 * `apps/web/src/App.tsx`
 *
 * SafiData — Apple.com Flagship Product Website Shell
 * - Supports live Arabic (`rtl`) & English (`ltr`) switching
 * - Supports 2-mode theme toggle (Daylight / Midnight)
 * - Automatically plays the cinematic Intro Splash before entering the website
 */
import type { ReactNode } from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import { gsap } from "gsap";
import { AmbientBackground } from "./components/motion/AmbientBackground";
import { SmoothScrollProvider } from "./components/motion/SmoothScrollProvider";
import { DOCK_TABS, FloatingGlassDock, STUDIO_TAB } from "./components/layout/FloatingGlassDock";
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
import { isDesktopRuntime } from "./desktop/studio-bridge";
import { OwnerDashboard } from "./pages/OwnerDashboard";
import { canAnimate } from "./components/ui/motion-preference";
import { clearStudioAuthSession, setStudioAuthSession } from "./desktop/studio-bridge";
import { getSafiDeviceIdentity } from "./lib/device-identity";
import { applySafiLang, getInitialSafiLang } from "./lib/locale";
import { applySafiTheme, getInitialSafiTheme, syncDomTheme, type SafiThemeId } from "./lib/theme";
import { isOwnerEmail } from "./lib/auth-rules";

export const SHELL_TABS: readonly SafiDockTabId[] = DOCK_TABS.map((tab) => tab.id);

export function renderStudio(
  tab: SafiDockTabId,
  onSelectTab: (tab: SafiDockTabId) => void,
  lang: SafiLang = "en"
): ReactNode {
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
      return <DesktopStudio lang={lang} />;
    case "library":
      return <LibraryStudio lang={lang} onSelectTab={onSelectTab} />;
    case "owner":
      return (
        <div className="safi-owner-frame">
          <OwnerDashboard lang={lang} onSelectTab={onSelectTab} />
        </div>
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
  const [activeTab, setActiveTab] = useState<SafiDockTabId>("home");
  const [desktopRuntime, setDesktopRuntime] = useState(false);
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

  useEffect(() => {
    setDesktopRuntime(isDesktopRuntime());
  }, []);

  const dockTabs = desktopRuntime ? [...DOCK_TABS, STUDIO_TAB] : DOCK_TABS;

  const selectTab = useCallback((tab: SafiDockTabId): void => {
    setActiveTab(tab);
  }, []);

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
    if (sessionToken === null) return;
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
  }, [sessionToken]);

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

  const activeTabConfig = DOCK_TABS.find((tab) => tab.id === activeTab);
  const activeLabel =
    lang === "ar"
      ? (activeTabConfig?.label ?? "الرئيسية")
      : (activeTabConfig?.labelEn ?? "Website");

  const authenticationRequired = sessionToken === null || sessionToken.trim().length === 0;
  const verificationSheetOpen = initialVerificationToken !== null && accountOpen && !resetOpen;
  const authModalOpen =
    !resetOpen && !verificationSheetOpen && (authenticationRequired || accountOpen);

  const bindSessionAndUnlock = useCallback(
    async (token: string, email: string, owner: boolean): Promise<void> => {
      if (isDesktopRuntime()) {
        const device = await getSafiDeviceIdentity();
        await setStudioAuthSession(token, device.installationId);
      }
      setSessionToken(token);
      setSessionEmail(email);
      setOwnerAccount(owner);
    },
    []
  );

  const clearSessionAndLock = useCallback(async (): Promise<void> => {
    try {
      if (isDesktopRuntime()) {
        await clearStudioAuthSession();
      }
    } finally {
      setSessionToken(null);
      setSessionEmail(null);
      setOwnerAccount(false);
    }
  }, []);

  const accountAccessPortal =
    initialVerificationToken === null ? null : (
      <AccountAccessSheet
        open={verificationSheetOpen}
        onClose={() => setAccountOpen(false)}
        lang={lang}
        verificationToken={initialVerificationToken}
        sessionToken={sessionToken}
        onAuthenticated={(token, email) => bindSessionAndUnlock(token, email, isOwnerEmail(email))}
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
      onAuthenticated={(session) =>
        bindSessionAndUnlock(session.token, session.email, session.isOwner)
      }
      onOwnerDetected={() => setOwnerAccount(true)}
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
        <div className="safi-shell" dir={lang === "ar" ? "rtl" : "ltr"}>
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
            authenticated={true}
            ownerAccount={ownerAccount}
          />

          <main id="safi-main" className="safi-container safi-stage">
            <p className="safi-visually-hidden" role="status">
              {lang === "ar" ? `الآن في ${activeLabel}` : `You are viewing ${activeLabel}`}
            </p>
            <div ref={stageRef} key={`${activeTab}-${lang}`} data-active-tab={activeTab}>
              {renderStudio(activeTab, selectTab, lang)}
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
          {accountAccessPortal ?? authPortal}
          {passwordResetPortal}
        </div>
      </SmoothScrollProvider>
    </>
  );
}
