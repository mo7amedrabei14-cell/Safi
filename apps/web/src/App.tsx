/**
 * `apps/web/src/App.tsx`
 *
 * The SafiData v5.0 Full-Viewport Shell
 * Locked to the 4-Color Palette: `#F9F7F7`, `#DBE2EF`, `#3F72AF`, `#112D4E`.
 * Renders the main interface immediately at 100% opacity so `<StrictMode>`
 * or background tabs never show a blank screen, while allowing instant replay
 * of the 2.2s `SafiIntroSplash` from the topbar.
 */
import type { ReactNode } from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import { gsap } from "gsap";
import { AmbientBackground } from "./components/motion/AmbientBackground";
import { SmoothScrollProvider } from "./components/motion/SmoothScrollProvider";
import { DOCK_TABS, FloatingGlassDock } from "./components/layout/FloatingGlassDock";
import type { SafiDockTabId, SafiThemeId } from "./components/layout/FloatingGlassDock";
import { CommandPalette } from "./components/layout/CommandPalette";
import { SettingsSheet } from "./components/layout/SettingsSheet";
import { SafiIntroSplash } from "./components/brand/SafiIntroSplash";
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
  onSelectTab: (tab: SafiDockTabId) => void
): ReactNode {
  switch (tab) {
    case "home":
      return (
        <BentoCommandCenter
          onStart={() => onSelectTab("cleaning")}
          onOpenLibrary={() => onSelectTab("library")}
        />
      );
    case "cleaning":
      return <CleaningStudio />;
    case "recipes":
      return <RecipesStudio />;
    case "charts":
      return <ChartsStudio />;
    case "library":
      return <LibraryStudio />;
    case "owner":
      return (
        <div className="safi-owner-frame">
          <OwnerDashboard />
        </div>
      );
    default: {
      const exhaustive: never = tab;
      return exhaustive;
    }
  }
}

export function App(): ReactNode {
  const [activeTab, setActiveTab] = useState<SafiDockTabId>("home");
  const [theme, setTheme] = useState<SafiThemeId>("apple-light");
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [introOpen, setIntroOpen] = useState<boolean>(false);
  const [introKey, setIntroKey] = useState<number>(0);
  const stageRef = useRef<HTMLDivElement | null>(null);

  const selectTab = useCallback((tab: SafiDockTabId): void => {
    setActiveTab(tab);
  }, []);

  const selectTheme = useCallback((nextTheme: SafiThemeId): void => {
    setTheme(nextTheme);
  }, []);

  const replayIntro = useCallback((): void => {
    setIntroKey((prev) => prev + 1);
    setIntroOpen(true);
  }, []);

  const closeIntro = useCallback((): void => {
    setIntroOpen(false);
  }, []);

  useEffect(() => {
    if (typeof document !== "undefined") {
      document.documentElement.setAttribute("data-theme", theme);
    }
  }, [theme]);

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
      { opacity: 0.35, y: 8 },
      {
        opacity: 1,
        y: 0,
        duration: 0.24,
        ease: "power2.out",
        clearProps: "transform,opacity"
      }
    );
    return () => {
      tween.kill();
      gsap.set(stage, { clearProps: "transform,opacity" });
    };
  }, [activeTab]);

  const activeLabel = DOCK_TABS.find((tab) => tab.id === activeTab)?.label ?? "الرئيسية";

  return (
    <>
      <AmbientBackground />
      <SmoothScrollProvider>
        <div className="safi-shell">
          <FloatingGlassDock
            activeTab={activeTab}
            onSelect={selectTab}
            onOpenCommandPalette={() => setPaletteOpen(true)}
            onOpenSettings={() => setSettingsOpen(true)}
            theme={theme}
            onSelectTheme={selectTheme}
            onReplayIntro={replayIntro}
          />

          <main id="safi-main" className="safi-container safi-stage">
            <p className="safi-visually-hidden" role="status">
              {`الآن في ${activeLabel}`}
            </p>
            <div ref={stageRef} key={activeTab} data-active-tab={activeTab}>
              {renderStudio(activeTab, selectTab)}
            </div>
          </main>

          <SafiIntroSplash key={introKey} open={introOpen} onComplete={closeIntro} />

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
