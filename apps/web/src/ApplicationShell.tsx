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
import { useAnimatedFavicon } from "./components/brand/useAnimatedFavicon";
import { BentoCommandCenter } from "./components/home/BentoCommandCenter";
import { CleaningStudio } from "./components/studios/CleaningStudio";
import { RecipesStudio } from "./components/studios/RecipesStudio";
import { ChartsStudio } from "./components/studios/ChartsStudio";
import { LibraryStudio } from "./components/studios/LibraryStudio";
import { OwnerDashboard } from "./pages/OwnerDashboard";
import { canAnimate } from "./components/ui/motion-preference";

export const SHELL_TABS: readonly SafiDockTabId[] = DOCK_TABS.map((tab) => tab.id);

export function renderStudio(
  tab: SafiDockTabId,
  onSelectTab: (tab: SafiDockTabId) => void,
  lang: SafiLang = "ar"
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
}

export function ApplicationShell({ onReady }: ApplicationShellProps): ReactNode {
  const [activeTab, setActiveTab] = useState<SafiDockTabId>("home");
  const [theme, setTheme] = useState<SafiThemeId>("apple-light");
  const [lang, setLang] = useState<SafiLang>("ar");
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
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
    setLang((prev) => (prev === "ar" ? "en" : "ar"));
  }, []);

  useEffect(() => {
    if (typeof document !== "undefined") {
      document.documentElement.setAttribute("data-theme", theme);
    }
  }, [theme]);

  useEffect(() => {
    if (typeof document !== "undefined") {
      document.documentElement.lang = lang;
      document.documentElement.dir = lang === "ar" ? "rtl" : "ltr";
    }
  }, [lang]);

  useEffect(() => {
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
  }, []);

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

  const activeLabel = DOCK_TABS.find((tab) => tab.id === activeTab)?.label ?? "الرئيسية";

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
          />

          <main id="safi-main" className="safi-container safi-stage">
            <p className="safi-visually-hidden" role="status">
              {`الآن في ${activeLabel}`}
            </p>
            <div ref={stageRef} key={`${activeTab}-${lang}`} data-active-tab={activeTab}>
              {renderStudio(activeTab, selectTab, lang)}
            </div>
          </main>

          <CommandPalette
            open={paletteOpen}
            onClose={() => setPaletteOpen(false)}
            onSelectTab={selectTab}
          />

          <SettingsSheet open={settingsOpen} onClose={() => setSettingsOpen(false)} />
        </div>
      </SmoothScrollProvider>
    </>
  );
}
