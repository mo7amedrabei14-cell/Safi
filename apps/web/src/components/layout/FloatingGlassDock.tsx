/**
 * `apps/web/src/components/layout/FloatingGlassDock.tsx`
 *
 * VisionOS-style floating glass dock:
 * - start (right, RTL): the Safi prism logo;
 * - centre: six navigation buttons with a sliding glass "active pill" driven by GSAP;
 * - end (left, RTL): status island, ⌘K command-palette trigger and settings.
 *
 * The pill is measured from the active button (`offsetLeft`/`offsetWidth`) and
 * animated with transform/width only, so switching tabs never triggers layout work
 * beyond one measurement.
 */
import type { ReactNode } from "react";
import { useEffect, useRef } from "react";
import {
  BarChart3,
  Command,
  Home,
  Library,
  ListChecks,
  Settings,
  ShieldCheck,
  Sparkles
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { gsap } from "gsap";
import { SafiPrismLogo } from "../brand/SafiPrismLogo";
import { DynamicStatusIsland } from "../brand/DynamicStatusIsland";
import { canAnimate } from "../ui/motion-preference";

export type SafiDockTabId = "home" | "cleaning" | "recipes" | "charts" | "library" | "owner";

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

export interface FloatingGlassDockProps {
  readonly activeTab: SafiDockTabId;
  readonly onSelect: (tab: SafiDockTabId) => void;
  readonly onOpenCommandPalette?: () => void;
  readonly onOpenSettings?: () => void;
}

export function FloatingGlassDock({
  activeTab,
  onSelect,
  onOpenCommandPalette,
  onOpenSettings
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
        gsap.to(pill, { ...target, duration: 0.45, ease: "expo.out" });
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
    <nav
      aria-label="التنقل الرئيسي"
      dir="rtl"
      className="safi-glass"
      style={{
        position: "fixed",
        bottom: "18px",
        insetInline: "16px",
        zIndex: 60,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        gap: "12px",
        width: "fit-content",
        maxWidth: "calc(100vw - 32px)",
        marginInline: "auto",
        padding: "10px 14px",
        borderRadius: "999px"
      }}
    >
      <div style={{ flex: "0 0 auto", paddingInlineEnd: "4px" }}>
        <SafiPrismLogo size="sm" />
      </div>

      <div
        ref={tabsRef}
        style={{ position: "relative", display: "flex", alignItems: "center", gap: "2px" }}
      >
        <span
          ref={pillRef}
          aria-hidden="true"
          style={{
            position: "absolute",
            top: 0,
            bottom: 0,
            left: 0,
            width: 0,
            borderRadius: "999px",
            background: "rgba(255, 255, 255, 0.12)",
            boxShadow: "inset 0 1px 0 rgba(255, 255, 255, 0.22)",
            pointerEvents: "none"
          }}
        />
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
              style={{
                position: "relative",
                zIndex: 1,
                display: "inline-flex",
                alignItems: "center",
                gap: "6px",
                minHeight: "44px",
                minWidth: "44px",
                padding: "8px 12px",
                borderRadius: "999px",
                border: "none",
                background: "transparent",
                color: isActive ? "var(--text-primary)" : "var(--text-muted)",
                fontFamily: "var(--font-arabic)",
                fontSize: "13px",
                fontWeight: isActive ? 700 : 500,
                cursor: "pointer",
                whiteSpace: "nowrap"
              }}
            >
              <Icon size={16} strokeWidth={2} aria-hidden="true" />
              {tab.label}
            </button>
          );
        })}
      </div>

      <div className="safi-dock-status">
        <DynamicStatusIsland />
      </div>

      <button
        type="button"
        onClick={onOpenCommandPalette}
        aria-label="لوحة الأوامر السريعة"
        title="لوحة الأوامر السريعة (⌘K)"
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: "6px",
          minHeight: "44px",
          minWidth: "44px",
          padding: "8px 12px",
          borderRadius: "999px",
          border: "1px solid var(--glass-border)",
          background: "rgba(255, 255, 255, 0.04)",
          color: "var(--text-muted)",
          fontFamily: "var(--font-mono)",
          fontSize: "12px",
          cursor: "pointer"
        }}
      >
        <Command size={15} aria-hidden="true" />
        ⌘K
      </button>

      <button
        type="button"
        onClick={onOpenSettings}
        aria-label="الإعدادات"
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: "6px",
          minHeight: "44px",
          minWidth: "44px",
          padding: "8px 12px",
          borderRadius: "999px",
          border: "1px solid var(--glass-border)",
          background: "rgba(255, 255, 255, 0.04)",
          color: "var(--text-muted)",
          fontFamily: "var(--font-arabic)",
          fontSize: "13px",
          cursor: "pointer"
        }}
      >
        <Settings size={15} aria-hidden="true" />
        الإعدادات
      </button>
    </nav>
  );
}
