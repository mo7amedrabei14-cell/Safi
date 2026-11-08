/**
 * `apps/web/src/App.tsx`
 *
 * The SafiData shell: ambient aurora background, one cinematic GSAP stage that
 * swaps between the six studios, the floating glass dock, the ⌘K palette and the
 * settings sheet.
 *
 * Navigation is state-driven (no router dependency was added): the active tab is
 * the single source of truth, and the stage re-enters with
 * `opacity 0 → 1`, `y 12 → 0`, `scale 0.99 → 1` on every tab change.
 */
import type { ReactNode } from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import { gsap } from "gsap";
import { AmbientBackground } from "./components/motion/AmbientBackground";
import { SmoothScrollProvider } from "./components/motion/SmoothScrollProvider";
import { DOCK_TABS, FloatingGlassDock } from "./components/layout/FloatingGlassDock";
import type { SafiDockTabId } from "./components/layout/FloatingGlassDock";
import { CommandPalette } from "./components/layout/CommandPalette";
import { SettingsSheet } from "./components/layout/SettingsSheet";
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
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const stageRef = useRef<HTMLDivElement | null>(null);

  const selectTab = useCallback((tab: SafiDockTabId): void => {
    setActiveTab(tab);
  }, []);

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
      { opacity: 0, y: 12, scale: 0.99 },
      { opacity: 1, y: 0, scale: 1, duration: 0.35, ease: "expo.out", clearProps: "transform" }
    );
    return () => {
      tween.kill();
    };
  }, [activeTab]);

  const activeLabel = DOCK_TABS.find((tab) => tab.id === activeTab)?.label ?? "الرئيسية";

  return (
    <>
      <AmbientBackground />
      <SmoothScrollProvider>
        <div className="safi-shell">
          <main id="safi-main" className="safi-stage">
            <p className="safi-visually-hidden" role="status">
              {`الآن في ${activeLabel}`}
            </p>
            <div ref={stageRef} key={activeTab} data-active-tab={activeTab}>
              {renderStudio(activeTab, selectTab)}
            </div>
          </main>

          <FloatingGlassDock
            activeTab={activeTab}
            onSelect={selectTab}
            onOpenCommandPalette={() => setPaletteOpen(true)}
            onOpenSettings={() => setSettingsOpen(true)}
          />

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
