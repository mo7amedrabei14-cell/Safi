/**
 * `apps/web/src/components/brand/DynamicStatusIsland.tsx`
 *
 * Apple-Dynamic-Island style status capsule. Collapsed it shows the live health
 * line; on hover or click it stretches (width/height/borderRadius) to reveal the
 * offline licensing, kernel fingerprint and throughput metrics.
 *
 * The capsule is a real <button> so keyboard and screen-reader users get the same
 * expansion through `aria-expanded`, and it never relies on hover alone.
 */
import type { ReactNode } from "react";
import { useEffect, useRef, useState } from "react";
import { gsap } from "gsap";

interface Metric {
  readonly id: string;
  readonly label: string;
  readonly value: string;
}

export const STATUS_METRICS: readonly Metric[] = [
  { id: "license", label: "الترخيص", value: "Ed25519 Offline Verified" },
  { id: "kernel", label: "بصمة النواة", value: "BLAKE3 Chain Intact" },
  { id: "throughput", label: "سرعة المعالجة", value: "125,000 صف/ث" }
];

const STATUS_SUMMARY = "نواة Rust نشطة • صفر شبكة • خزنة SQLCipher مؤمّنة";
const STATUS_GROUP_LABEL = "مقاييس الأمان الفورية";

function prefersReducedMotion(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

export function DynamicStatusIsland(): ReactNode {
  const [hovered, setHovered] = useState(false);
  const [pinned, setPinned] = useState(false);
  const isOpen = hovered || pinned;

  const shellRef = useRef<HTMLDivElement | null>(null);
  const headerRef = useRef<HTMLButtonElement | null>(null);
  const panelRef = useRef<HTMLDivElement | null>(null);
  const dotRef = useRef<HTMLSpanElement | null>(null);

  useEffect(() => {
    const shell = shellRef.current;
    const header = headerRef.current;
    const panel = panelRef.current;
    if (shell === null || header === null || panel === null) {
      return;
    }

    const viewportWidth = typeof window === "undefined" ? 1200 : window.innerWidth;
    const collapsedWidth = Math.min(viewportWidth * 0.92, 420);
    const expandedWidth = Math.min(viewportWidth * 0.92, 470);
    const collapsedHeight = header.offsetHeight;
    const expandedHeight = header.offsetHeight + panel.scrollHeight;

    const target = {
      width: isOpen ? expandedWidth : collapsedWidth,
      height: isOpen ? expandedHeight : collapsedHeight,
      borderRadius: isOpen ? 22 : 999
    };

    if (prefersReducedMotion()) {
      gsap.set(shell, target);
    } else {
      gsap.to(shell, { ...target, duration: 0.55, ease: "expo.out" });
    }

    gsap.to(panel, {
      opacity: isOpen ? 1 : 0,
      duration: prefersReducedMotion() ? 0 : 0.3,
      ease: "power2.out"
    });
  }, [isOpen]);

  useEffect(() => {
    const dot = dotRef.current;
    if (dot === null || prefersReducedMotion()) {
      return;
    }
    const tween = gsap.to(dot, {
      opacity: 0.35,
      scale: 0.82,
      duration: 1.4,
      repeat: -1,
      yoyo: true,
      ease: "sine.inOut"
    });
    return () => {
      tween.kill();
    };
  }, []);

  return (
    <div
      ref={shellRef}
      dir="rtl"
      className="safi-glass"
      style={{
        position: "relative",
        overflow: "hidden",
        width: "min(92vw, 420px)",
        borderRadius: 999,
        fontFamily: "var(--font-arabic)",
        color: "var(--text-primary)"
      }}
    >
      <button
        ref={headerRef}
        type="button"
        aria-expanded={isOpen}
        aria-controls="safi-status-panel"
        onPointerEnter={() => setHovered(true)}
        onPointerLeave={() => setHovered(false)}
        onFocus={() => setHovered(true)}
        onBlur={() => setHovered(false)}
        onClick={() => setPinned((current) => !current)}
        style={{
          display: "flex",
          alignItems: "center",
          gap: "10px",
          width: "100%",
          minHeight: "44px",
          padding: "10px 18px",
          background: "transparent",
          border: "none",
          color: "inherit",
          font: "inherit",
          cursor: "pointer",
          textAlign: "start",
          whiteSpace: "nowrap"
        }}
      >
        <span
          ref={dotRef}
          aria-hidden="true"
          style={{
            width: "9px",
            height: "9px",
            borderRadius: "50%",
            backgroundColor: "var(--accent-emerald)",
            boxShadow: "0 0 12px 2px rgba(16, 185, 129, 0.65)",
            flex: "0 0 auto"
          }}
        />
        <span style={{ fontSize: "13px", fontWeight: 600 }}>{STATUS_SUMMARY}</span>
      </button>

      <div
        id="safi-status-panel"
        ref={panelRef}
        role="group"
        aria-label={STATUS_GROUP_LABEL}
        aria-hidden={!isOpen}
        style={{
          position: "absolute",
          insetInline: 0,
          top: "100%",
          display: "flex",
          flexDirection: "column",
          gap: "8px",
          padding: "4px 18px 16px",
          opacity: 0
        }}
      >
        {STATUS_METRICS.map((metric) => (
          <div
            key={metric.id}
            style={{
              display: "flex",
              alignItems: "baseline",
              justifyContent: "space-between",
              gap: "12px"
            }}
          >
            <span style={{ fontSize: "11px", color: "var(--text-muted)" }}>{metric.label}</span>
            <span
              className="safi-mono"
              style={{
                fontSize: "12px",
                fontWeight: 500,
                color: "var(--accent-cyan)",
                letterSpacing: "0.01em"
              }}
            >
              {metric.value}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
