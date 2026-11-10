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
import { DOCK_TABS, FloatingGlassDock } from "./components/layout/FloatingGlassDock";
import type { SafiDockTabId, SafiLang, SafiThemeId } from "./components/layout/FloatingGlassDock";
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
import { OwnerDashboard } from "./pages/OwnerDashboard";
import { canAnimate } from "./components/ui/motion-preference";
import { applySafiLang, getInitialSafiLang } from "./lib/locale";
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
  readonly initialVerificationToken?: string | null;
  readonly initialResetToken?: string | null;
}

export function ApplicationShell({
  onReady,
  initialVerificationToken = null,
  initialResetToken = null
}: ApplicationShellProps): ReactNode {
  const [activeTab, setActiveTab] = useState<SafiDockTabId>("home");
  const [theme, setTheme] = useState<SafiThemeId>("apple-light");
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

  const selectTab = useCallback((tab: SafiDockTabId): void => {
    setActiveTab(tab);
  }, []);

  const selectTheme = useCallback((nextTheme: SafiThemeId): void => {
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
    if (typeof document !== "undefined") {
      document.documentElement.setAttribute("data-theme", theme);
    }
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

  const authenticationRequired = sessionToken === null;
  const verificationSheetOpen = initialVerificationToken !== null && accountOpen && !resetOpen;
  const authModalOpen =
    !resetOpen && !verificationSheetOpen && (authenticationRequired || accountOpen);

  const accountAccessPortal =
    initialVerificationToken === null ? null : (
      <AccountAccessSheet
        open={verificationSheetOpen}
        onClose={() => setAccountOpen(false)}
        lang={lang}
        verificationToken={initialVerificationToken}
        sessionToken={sessionToken}
        onAuthenticated={(token, email) => {
          setSessionToken(token);
          setSessionEmail(email);
          setOwnerAccount(isOwnerEmail(email));
        }}
        onLoggedOut={() => {
          setSessionToken(null);
          setSessionEmail(null);
          setOwnerAccount(false);
        }}
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
      onAuthenticated={(session) => {
        setSessionToken(session.token);
        setSessionEmail(session.email);
        setOwnerAccount(session.isOwner);
      }}
      onOwnerDetected={() => setOwnerAccount(true)}
      onSignedOut={() => {
        setSessionToken(null);
        setSessionEmail(null);
        setOwnerAccount(false);
      }}
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

          <SettingsSheet open={settingsOpen} onClose={() => setSettingsOpen(false)} lang={lang} />
          {accountAccessPortal ?? authPortal}
          {passwordResetPortal}
        </div>
      </SmoothScrollProvider>
    </>
  );
}
