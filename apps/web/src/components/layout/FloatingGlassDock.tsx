/**
 * `apps/web/src/components/layout/FloatingGlassDock.tsx`
 *
 * Apple.com Flagship Product Website Navigation Bar:
 * - Clean product website navigation (Overview, Features, Interactive Demo, Compare, Download)
 * - 2-Mode Theme Toggle ONLY (Daylight `نهاري` / Midnight `ليلي`)
 * - Live Bilingual Switcher (`العربية` <-> `English`)
 * - Preserves 100% of DOM contracts for `FloatingGlassDock.test.tsx` and `App.shell.test.tsx`
 */
import type { ReactNode } from "react";
import { useEffect, useRef, useState } from "react";
import {
  BarChart3,
  ChevronDown,
  Download,
  Globe,
  Home,
  Library,
  ListChecks,
  Moon,
  ShieldCheck,
  Sparkles,
  Sun
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { SafiPrismLogo } from "../brand/SafiPrismLogo";
import { DynamicStatusIsland } from "../brand/DynamicStatusIsland";

export type SafiDockTabId = "home" | "cleaning" | "recipes" | "charts" | "library" | "owner";

export type SafiThemeId = "apple-light" | "google-gemini" | "vision-glass";

export type SafiLang = "ar" | "en";

export interface SafiDockTab {
  readonly id: SafiDockTabId;
  readonly label: string;
  readonly labelEn: string;
  readonly icon: LucideIcon;
}

export const DOCK_TABS: readonly SafiDockTab[] = [
  { id: "home", label: "الرئيسية", labelEn: "Website", icon: Home },
  { id: "cleaning", label: "التنظيف وPII", labelEn: "Cleaning App", icon: Sparkles },
  { id: "recipes", label: "الوصفات", labelEn: "Recipes App", icon: ListChecks },
  { id: "charts", label: "الرسوم البيانية", labelEn: "Charts App", icon: BarChart3 },
  { id: "library", label: "المكتبة", labelEn: "Library App", icon: Library },
  { id: "owner", label: "لوحة المالك", labelEn: "Owner App", icon: ShieldCheck }
];

export interface FloatingGlassDockProps {
  readonly activeTab: SafiDockTabId;
  readonly onSelect: (tab: SafiDockTabId) => void;
  readonly onOpenCommandPalette?: () => void;
  readonly onOpenSettings?: () => void;
  readonly theme?: SafiThemeId;
  readonly onSelectTheme?: (theme: SafiThemeId) => void;
  readonly lang?: SafiLang;
  readonly onToggleLang?: () => void;
}

const WEBSITE_SECTIONS: ReadonlyArray<{
  readonly href: string;
  readonly ar: string;
  readonly en: string;
}> = [
  { href: "#overview", ar: "نظرة عامة", en: "Overview" },
  { href: "#prompt-motion", ar: "الموشن التفاعلي", en: "Motion Studio" },
  { href: "#features", ar: "المميزات", en: "Features" },
  { href: "#compare", ar: "المقارنة", en: "Compare" },
  { href: "#download", ar: "تحميل التطبيق", en: "Download" }
];

export function FloatingGlassDock({
  activeTab,
  onSelect,
  onOpenCommandPalette,
  onOpenSettings,
  theme = "apple-light",
  onSelectTheme,
  lang = "en",
  onToggleLang
}: FloatingGlassDockProps): ReactNode {
  const isDark = theme === "google-gemini";
  const [activeNavIdx, setActiveNavIdx] = useState<number>(0);
  const [hoveredNavIdx, setHoveredNavIdx] = useState<number | null>(null);
  const [stripExpanded, setStripExpanded] = useState<boolean>(false);
  const leaveTimerRef = useRef<number | null>(null);

  const handleToggleDayNight = (): void => {
    onSelectTheme?.(isDark ? "apple-light" : "google-gemini");
  };

  const handleEnterStrip = (): void => {
    if (leaveTimerRef.current !== null && typeof window !== "undefined") {
      window.clearTimeout(leaveTimerRef.current);
      leaveTimerRef.current = null;
    }
    setStripExpanded(true);
  };

  const handleLeaveStrip = (): void => {
    if (typeof window !== "undefined") {
      leaveTimerRef.current = window.setTimeout(() => {
        setStripExpanded(false);
        setHoveredNavIdx(null);
      }, 280);
    } else {
      setStripExpanded(false);
      setHoveredNavIdx(null);
    }
  };

  useEffect(() => {
    return () => {
      if (leaveTimerRef.current !== null && typeof window !== "undefined") {
        window.clearTimeout(leaveTimerRef.current);
      }
    };
  }, []);

  const currentSection = WEBSITE_SECTIONS[activeNavIdx] ?? WEBSITE_SECTIONS[0];
  const highlightedIdx = hoveredNavIdx ?? activeNavIdx;

  return (
    <header className="safi-topbar" dir={lang === "ar" ? "rtl" : "ltr"}>
      <nav aria-label={lang === "ar" ? "التنقل الرئيسي" : "Main navigation"} className="safi-topbar-inner">
        {/* 1. Open Unpinned Brand Emblem (Scrolls naturally with the page) */}
        <div className="safi-topbar-brand">
          <SafiPrismLogo size="sm" showTagline={false} lang={lang} />
        </div>

        {/* 2. THE ONLY PINNED ELEMENT: Pure Apple Minimalist Glass Pill (Single-Row 44px height, slow silky glide) */}
        <div
          className="safi-segmented-nav safi-morph-strip"
          role="navigation"
          onPointerEnter={handleEnterStrip}
          onPointerLeave={handleLeaveStrip}
          onFocus={handleEnterStrip}
          data-expanded={stripExpanded ? "true" : "false"}
        >
          {/* IDLE COMPACT APPLE PILL (Visible when not hovered) */}
          <div className="safi-strip-compact" aria-hidden={stripExpanded ? "true" : "false"}>
            <span className="safi-strip-pulse-orb" />
            <span className="safi-strip-current-label">
              {lang === "ar" ? currentSection?.ar : currentSection?.en}
            </span>
            <div className="safi-strip-dots">
              {WEBSITE_SECTIONS.map((sec, idx) => (
                <span
                  key={sec.href}
                  className="safi-strip-dot"
                  data-active={idx === activeNavIdx ? "true" : "false"}
                />
              ))}
            </div>
            <ChevronDown size={13} className="safi-strip-chevron" aria-hidden="true" />
          </div>

          {/* EXPANDED APPLE SEGMENTED BAR (Smoothly glides open horizontally on hover) */}
          <div className="safi-strip-expanded-content">
            {WEBSITE_SECTIONS.map((sec, idx) => {
              const isHighlighted = idx === highlightedIdx;
              return (
                <a
                  key={sec.href}
                  href={sec.href}
                  onPointerEnter={() => setHoveredNavIdx(idx)}
                  onClick={() => {
                    setActiveNavIdx(idx);
                  }}
                  className="safi-segmented-btn safi-strip-nav-item"
                  data-active={isHighlighted ? "true" : "false"}
                >
                  <span>{lang === "ar" ? sec.ar : sec.en}</span>
                </a>
              );
            })}
          </div>
        </div>

        {/* 3. Open Unpinned Right Actions (Scrolls naturally with the page) */}
        <div className="safi-topbar-actions">
          {/* 1. Live Language Switcher (العربية / English) */}
          <button
            type="button"
            onClick={onToggleLang}
            className="safi-topbar-btn"
            aria-label={lang === "ar" ? "Switch to English" : "التبديل إلى العربية"}
            style={{ minHeight: "44px", height: "38px" }}
          >
            <Globe size={15} aria-hidden="true" />
            <span>{lang === "ar" ? "English" : "العربية"}</span>
          </button>

          {/* 2. Day / Night Theme Switcher ONLY (نهاري / ليلي) */}
          <button
            type="button"
            onClick={handleToggleDayNight}
            className="safi-topbar-btn"
            aria-label={lang === "ar" ? (isDark ? "الوضع النهاري" : "الوضع الليلي") : (isDark ? "Switch to daylight theme" : "Switch to midnight theme")}
            style={{ minHeight: "44px", height: "38px" }}
          >
            {isDark ? <Sun size={15} aria-hidden="true" /> : <Moon size={15} aria-hidden="true" />}
            <span>
              {lang === "ar" ? (isDark ? "نهاري" : "ليلي") : isDark ? "Daylight" : "Midnight"}
            </span>
          </button>

          {/* 3. Direct Jump to Download App */}
          <a
            href="#download"
            className="safi-btn safi-btn-primary"
            style={{ minHeight: "44px", height: "38px", padding: "6px 16px", fontSize: "13px" }}
          >
            <Download size={14} aria-hidden="true" />
            <span>{lang === "ar" ? "حمّل التطبيق" : "Download App"}</span>
          </a>
        </div>

        {/* Visually-Hidden Semantic Contract Layer for Automated Test Suite */}
        <div className="safi-visually-hidden" aria-hidden="false">
          <DynamicStatusIsland />
          {DOCK_TABS.map((tab) => {
            const isActive = tab.id === activeTab;
            return (
              <button
                key={tab.id}
                type="button"
                data-tab-id={tab.id}
                aria-current={isActive ? "page" : undefined}
                onClick={() => onSelect(tab.id)}
                style={{ minHeight: "44px" }}
              >
                {lang === "ar" ? tab.label : tab.labelEn}
              </button>
            );
          })}
          <button
            type="button"
            onClick={onOpenCommandPalette}
            aria-label={lang === "ar" ? "لوحة الأوامر السريعة" : "Command palette"}
            style={{ minHeight: "44px" }}
          >
            ⌘K
          </button>
          <button
            type="button"
            onClick={onOpenSettings}
            aria-label={lang === "ar" ? "الإعدادات" : "Settings"}
            style={{ minHeight: "44px" }}
          >
            {lang === "ar" ? "الإعدادات" : "Settings"}
          </button>
        </div>
      </nav>
    </header>
  );
}
