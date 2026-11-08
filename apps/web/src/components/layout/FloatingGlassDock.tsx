/**
 * `apps/web/src/components/layout/FloatingGlassDock.tsx`
 *
 * Full-Viewport Sticky Topbar (`.safi-topbar`, `max-width: 1820px`)
 * Locked to the 4-Color Palette: `#F9F7F7`, `#DBE2EF`, `#3F72AF`, `#112D4E`
 */
import type { ReactNode } from "react";
import { useEffect, useRef } from "react";
import {
  BarChart3,
  Command,
  Home,
  Layers,
  Library,
  ListChecks,
  Moon,
  Play,
  Settings,
  ShieldCheck,
  Sparkles,
  Sun
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { gsap } from "gsap";
import { SafiPrismLogo } from "../brand/SafiPrismLogo";
import { DynamicStatusIsland } from "../brand/DynamicStatusIsland";
import { canAnimate } from "../ui/motion-preference";

export type SafiDockTabId = "home" | "cleaning" | "recipes" | "charts" | "library" | "owner";

export type SafiThemeId = "apple-light" | "google-gemini" | "vision-glass";

export interface SafiDockTab {
  readonly id: SafiDockTabId;
  readonly label: string;
  readonly icon: LucideIcon;
}

export const DOCK_TABS: readonly SafiDockTab[] = [
  { id: "home", label: "الرئيسية", icon: Home },
  { id: "cleaning", label: "التنظيف وPII", icon: Sparkles },
  { id: "recipes", label: "الوصفات", icon: ListChecks },
  { id: "charts", label: "الرسوم البيانية", icon: BarChart3 },
  { id: "library", label: "المكتبة", icon: Library },
  { id: "owner", label: "لوحة المالك", icon: ShieldCheck }
];

export const SAFI_THEMES: ReadonlyArray<{
  readonly id: SafiThemeId;
  readonly labelAr: string;
  readonly badge: string;
}> = [
  { id: "apple-light", labelAr: "فاتح #F9F7F7", badge: "#F9F7F7 • #DBE2EF • #3F72AF • #112D4E" },
  { id: "vision-glass", labelAr: "هادئ #DBE2EF", badge: "#DBE2EF • #F9F7F7 • #112D4E" },
  { id: "google-gemini", labelAr: "داكن #112D4E", badge: "#112D4E • #3F72AF • #F9F7F7" }
];

export interface FloatingGlassDockProps {
  readonly activeTab: SafiDockTabId;
  readonly onSelect: (tab: SafiDockTabId) => void;
  readonly onOpenCommandPalette?: () => void;
  readonly onOpenSettings?: () => void;
  readonly theme?: SafiThemeId;
  readonly onSelectTheme?: (theme: SafiThemeId) => void;
  readonly onReplayIntro?: () => void;
}

export function FloatingGlassDock({
  activeTab,
  onSelect,
  onOpenCommandPalette,
  onOpenSettings,
  theme = "apple-light",
  onSelectTheme,
  onReplayIntro
}: FloatingGlassDockProps): ReactNode {
  const tabsRef = useRef<HTMLDivElement | null>(null);
  const pillRef = useRef<HTMLSpanElement | null>(null);
  const positionedRef = useRef(false);

  useEffect(() => {
    const tabs = tabsRef.current;
    const pill = pillRef.current;
    if (tabs === null || pill === null) {
      return;
    }

    const movePill = (animate: boolean): void => {
      const active = tabs.querySelector<HTMLButtonElement>(`[data-tab-id="${activeTab}"]`);
      if (active === null) {
        return;
      }
      const target = { x: active.offsetLeft, width: active.offsetWidth };
      if (animate && canAnimate()) {
        gsap.to(pill, { ...target, duration: 0.35, ease: "expo.out" });
      } else {
        gsap.set(pill, target);
      }
    };

    const first = !positionedRef.current;
    positionedRef.current = true;
    movePill(!first);

    const onResize = (): void => {
      movePill(false);
    };
    window.addEventListener("resize", onResize);
    return () => {
      window.removeEventListener("resize", onResize);
    };
  }, [activeTab]);

  return (
    <header className="safi-topbar" dir="rtl">
      <nav aria-label="التنقل الرئيسي" className="safi-topbar-inner">
        <div className="safi-topbar-brand">
          <SafiPrismLogo size="sm" showTagline={false} />
        </div>

        <div ref={tabsRef} className="safi-segmented-nav">
          <span ref={pillRef} aria-hidden="true" className="safi-segmented-pill" />
          {DOCK_TABS.map((tab) => {
            const Icon = tab.icon;
            const isActive = tab.id === activeTab;
            return (
              <button
                key={tab.id}
                type="button"
                data-tab-id={tab.id}
                aria-current={isActive ? "page" : undefined}
                onClick={() => onSelect(tab.id)}
                className="safi-segmented-btn"
                style={{ minHeight: "44px", height: "36px" }}
              >
                <Icon size={14} strokeWidth={2} aria-hidden="true" />
                {tab.label}
              </button>
            );
          })}
        </div>

        <div className="safi-topbar-actions">
          <DynamicStatusIsland />

          <div className="safi-theme-switcher" role="group" aria-label="تبديل مظهر المنصة">
            {SAFI_THEMES.map((item) => {
              const active = theme === item.id;
              return (
                <button
                  key={item.id}
                  type="button"
                  aria-pressed={active}
                  onClick={() => onSelectTheme?.(item.id)}
                  className="safi-theme-option"
                  title={item.badge}
                  style={{ minHeight: "44px", height: "32px" }}
                >
                  {item.id === "apple-light" ? (
                    <Sun size={12} aria-hidden="true" />
                  ) : item.id === "google-gemini" ? (
                    <Moon size={12} aria-hidden="true" />
                  ) : (
                    <Layers size={12} aria-hidden="true" />
                  )}
                  <span>{item.labelAr}</span>
                </button>
              );
            })}
          </div>

          {onReplayIntro !== undefined ? (
            <button
              type="button"
              onClick={onReplayIntro}
              className="safi-topbar-btn"
              aria-label="إعادة عرض الإنترو السينمائي"
              title="تشغيل إنترو الموشن جرافيك (2.2s)"
              style={{ minHeight: "44px", height: "36px" }}
            >
              <Play size={13} aria-hidden="true" />
              <span>▶ عرض الإنترو</span>
            </button>
          ) : null}

          <button
            type="button"
            onClick={onOpenCommandPalette}
            className="safi-topbar-btn safi-mono"
            aria-label="لوحة الأوامر السريعة"
            title="لوحة الأوامر السريعة (⌘K)"
            style={{ minHeight: "44px", height: "36px" }}
          >
            <Command size={13} aria-hidden="true" />
            ⌘K
          </button>

          <button
            type="button"
            onClick={onOpenSettings}
            className="safi-topbar-btn"
            aria-label="الإعدادات"
            style={{ minHeight: "44px", height: "36px" }}
          >
            <Settings size={14} aria-hidden="true" />
            الإعدادات
          </button>
        </div>
      </nav>
    </header>
  );
}
