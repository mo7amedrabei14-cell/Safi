/**
 * `apps/web/src/components/brand/SafiPrismLogo.tsx`
 *
 * SafiData Data-Engineering & Analytics Emblem (Apple Studio Slate & Calm Gradient)
 * - Clean, meaningful mark representing Data Structuring + 3 Ascending Analytics Columns + Trend Line
 * - Calm Apple Slate & Soft Sky-Silver gradients (no harsh colors)
 */
import type { PointerEvent as ReactPointerEvent, ReactNode } from "react";
import { useCallback, useEffect, useRef } from "react";
import { gsap } from "gsap";
import { BRAND_CONFIG } from "@safi/brand";

export type SafiPrismLogoSize = "sm" | "md" | "lg";

export interface SafiPrismLogoProps {
  readonly size?: SafiPrismLogoSize;
  readonly showTagline?: boolean;
  readonly hideText?: boolean;
  readonly lang?: "ar" | "en";
}

interface SizeScale {
  readonly svg: number;
  readonly title: number;
  readonly subtitle: number;
  readonly gap: number;
}

const SIZES: Record<SafiPrismLogoSize, SizeScale> = {
  sm: { svg: 40, title: 16, subtitle: 11, gap: 10 },
  md: { svg: 72, title: 22, subtitle: 12, gap: 14 },
  lg: { svg: 112, title: 32, subtitle: 14, gap: 18 }
};

const WORDMARK_SUBTITLE = "Gold Master v1.0";

export const MOTION_DOTS: ReadonlyArray<{
  readonly id: number;
  readonly startX: number;
  readonly startY: number;
  readonly gridX: number;
  readonly gridY: number;
}> = [
  { id: 1, startX: 20, startY: 30, gridX: 44, gridY: 58 },
  { id: 2, startX: 96, startY: 22, gridX: 64, gridY: 58 },
  { id: 3, startX: 112, startY: 70, gridX: 84, gridY: 58 },
  { id: 4, startX: 30, startY: 104, gridX: 44, gridY: 78 },
  { id: 5, startX: 70, startY: 118, gridX: 64, gridY: 78 },
  { id: 6, startX: 8, startY: 64, gridX: 84, gridY: 78 },
  { id: 7, startX: 108, startY: 108, gridX: 44, gridY: 98 },
  { id: 8, startX: 52, startY: 12, gridX: 64, gridY: 98 },
  { id: 9, startX: 84, startY: 60, gridX: 84, gridY: 98 }
];

function isBrowserWithMotion(): boolean {
  if (typeof window === "undefined" || typeof document === "undefined") {
    return false;
  }
  if (typeof window.requestAnimationFrame !== "function") {
    return false;
  }
  if (typeof window.matchMedia !== "function") {
    return false;
  }
  return !window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

export function SafiPrismLogo({
  size = "sm",
  showTagline = false,
  hideText = false,
  lang = "en"
}: SafiPrismLogoProps): ReactNode {
  const scale = SIZES[size];
  const wordmarkTitle = lang === "ar" ? BRAND_CONFIG.displayNameAr : BRAND_CONFIG.displayNameEn;
  const logoLabel = lang === "ar" ? "شعار صافي البيانات" : "SafiData logo";
  const svgRef = useRef<SVGSVGElement | null>(null);
  const pulseTweenRef = useRef<gsap.core.Tween | null>(null);

  useEffect(() => {
    const svg = svgRef.current;
    if (svg === null || !isBrowserWithMotion()) {
      return;
    }

    const drawables = Array.from(svg.querySelectorAll<SVGGeometryElement>("[data-draw='true']"));
    const nodes = Array.from(svg.querySelectorAll<SVGCircleElement>(".safi-node"));

    drawables.forEach((path) => {
      if (typeof path.getTotalLength === "function") {
        const length = path.getTotalLength();
        path.style.strokeDasharray = `${length}`;
        path.style.strokeDashoffset = `${length}`;
      }
    });

    const tl = gsap.timeline({ defaults: { ease: "power3.out" } });
    tl.fromTo(
      drawables,
      { opacity: 0.35 },
      {
        strokeDashoffset: 0,
        opacity: 1,
        duration: 0.75,
        stagger: 0.045,
        onComplete: () => {
          drawables.forEach((path) => {
            path.style.strokeDasharray = "";
            path.style.strokeDashoffset = "";
            path.style.opacity = "";
          });
        }
      }
    );

    if (nodes.length > 0) {
      tl.fromTo(
        nodes,
        { scale: 0.35, opacity: 0.3, transformOrigin: "50% 50%" },
        {
          scale: 1,
          opacity: 1,
          duration: 0.4,
          stagger: 0.07,
          ease: "back.out(2.2)",
          clearProps: "transform,opacity"
        },
        "-=0.35"
      );

      pulseTweenRef.current = gsap.to(nodes, {
        scale: 1.25,
        transformOrigin: "50% 50%",
        duration: 1.5,
        stagger: 0.2,
        repeat: -1,
        yoyo: true,
        ease: "sine.inOut"
      });
    }

    return () => {
      tl.kill();
      pulseTweenRef.current?.kill();
      pulseTweenRef.current = null;
      drawables.forEach((path) => {
        path.style.strokeDasharray = "";
        path.style.strokeDashoffset = "";
        path.style.opacity = "";
      });
    };
  }, []);

  const handlePointerMove = useCallback((event: ReactPointerEvent<SVGSVGElement>): void => {
    const svg = svgRef.current;
    if (svg === null || !isBrowserWithMotion()) {
      return;
    }
    const rect = svg.getBoundingClientRect();
    if (rect.width === 0 || rect.height === 0) {
      return;
    }
    const dx = (event.clientX - (rect.left + rect.width / 2)) / (rect.width / 2);
    const dy = (event.clientY - (rect.top + rect.height / 2)) / (rect.height / 2);
    gsap.to(svg, {
      rotateY: dx * 10,
      rotateX: -dy * 10,
      scale: 1.05,
      duration: 0.3,
      ease: "power2.out"
    });
  }, []);

  const handlePointerLeave = useCallback((): void => {
    const svg = svgRef.current;
    if (svg === null || !isBrowserWithMotion()) {
      return;
    }
    gsap.to(svg, {
      rotateX: 0,
      rotateY: 0,
      scale: 1,
      duration: 0.5,
      ease: "elastic.out(1, 0.5)"
    });
  }, []);

  return (
    <div
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: `${scale.gap}px`,
        userSelect: "none"
      }}
    >
      <svg
        ref={svgRef}
        width={scale.svg}
        height={scale.svg}
        viewBox="0 0 128 128"
        fill="none"
        role="img"
        aria-label={logoLabel}
        onPointerMove={handlePointerMove}
        onPointerLeave={handlePointerLeave}
        style={{
          flexShrink: 0,
          overflow: "visible",
          filter: "drop-shadow(0 8px 18px rgba(15, 23, 42, 0.16))"
        }}
      >
        <defs>
          <linearGradient
            id="safi-slate-tile"
            x1="10"
            y1="8"
            x2="118"
            y2="120"
            gradientUnits="userSpaceOnUse"
          >
            <stop offset="0%" stopColor="#1E293B" />
            <stop offset="100%" stopColor="#0F172A" />
          </linearGradient>
          <linearGradient
            id="safi-bar-a"
            x1="34"
            y1="68"
            x2="34"
            y2="100"
            gradientUnits="userSpaceOnUse"
          >
            <stop offset="0%" stopColor="#94A3B8" />
            <stop offset="100%" stopColor="#64748B" />
          </linearGradient>
          <linearGradient
            id="safi-bar-b"
            x1="64"
            y1="50"
            x2="64"
            y2="100"
            gradientUnits="userSpaceOnUse"
          >
            <stop offset="0%" stopColor="#CBD5E1" />
            <stop offset="100%" stopColor="#94A3B8" />
          </linearGradient>
          <linearGradient
            id="safi-bar-c"
            x1="94"
            y1="32"
            x2="94"
            y2="100"
            gradientUnits="userSpaceOnUse"
          >
            <stop offset="0%" stopColor="#F8FAFC" />
            <stop offset="100%" stopColor="#CBD5E1" />
          </linearGradient>
          <filter id="safi-prism-glow" x="-25%" y="-25%" width="150%" height="150%">
            <feGaussianBlur stdDeviation="2.5" result="blur" />
            <feMerge>
              <feMergeNode in="blur" />
              <feMergeNode in="SourceGraphic" />
            </feMerge>
          </filter>
        </defs>

        {/* Prompt-Motion S2 Orbital Ice Halo + 4 Orbiting Satellite Squares */}
        <g className="safi-pm-logo-orbit" style={{ transformOrigin: "64px 64px" }}>
          <circle
            cx="64"
            cy="64"
            r="60"
            stroke="url(#safi-bar-b)"
            strokeOpacity="0.35"
            strokeWidth="1.2"
            strokeDasharray="6 6"
          />
          {/* Top Satellite (#2F6BFF) */}
          <rect x="60" y="1" width="8" height="8" rx="2.5" fill="#2F6BFF" />
          {/* Right Satellite (#3CC8F0) */}
          <rect x="119" y="60" width="8" height="8" rx="2.5" fill="#3CC8F0" />
          {/* Bottom Satellite (#4ED6A0) */}
          <rect x="60" y="119" width="8" height="8" rx="2.5" fill="#4ED6A0" />
          {/* Left Satellite (#F8FAFC) */}
          <rect
            x="1"
            y="60"
            width="8"
            height="8"
            rx="2.5"
            fill="#F8FAFC"
            stroke="#3B82F6"
            strokeWidth="1"
          />
        </g>

        {/* Apple Squircle Tile */}
        <rect
          x="10"
          y="10"
          width="108"
          height="108"
          rx="26"
          fill="url(#safi-slate-tile)"
          stroke="rgba(255,255,255,0.22)"
          strokeWidth="2"
        />

        {/* 1. Data Matrix Frame (data-draw #1) */}
        <path
          data-draw="true"
          d="M22 26H106V102H22V26Z"
          stroke="#94A3B8"
          strokeOpacity="0.2"
          strokeWidth="1.2"
        />

        {/* 2. Grid Guide 1 (data-draw #2) */}
        <path
          data-draw="true"
          d="M24 52H104"
          stroke="#94A3B8"
          strokeOpacity="0.16"
          strokeWidth="1"
          strokeDasharray="3 3"
        />

        {/* 3. Grid Guide 2 (data-draw #3) */}
        <path
          data-draw="true"
          d="M24 76H104"
          stroke="#94A3B8"
          strokeOpacity="0.16"
          strokeWidth="1"
          strokeDasharray="3 3"
        />

        {/* 4. Animated Analytics Column 1 (data-draw #4) */}
        <path
          data-draw="true"
          className="safi-bar-anim-1"
          d="M28 72C28 69.8 29.8 68 32 68H42C44.2 68 46 69.8 46 72V98H28V72Z"
          fill="url(#safi-bar-a)"
        />

        {/* 5. Animated Analytics Column 2 (data-draw #5) */}
        <path
          data-draw="true"
          className="safi-bar-anim-2"
          d="M55 54C55 51.8 56.8 50 59 50H69C71.2 50 73 51.8 73 54V98H55V54Z"
          fill="url(#safi-bar-b)"
        />

        {/* 6. Animated Analytics Column 3 (data-draw #6) */}
        <path
          data-draw="true"
          className="safi-bar-anim-3"
          d="M82 38C82 35.8 83.8 34 86 34H96C98.2 34 100 35.8 100 38V98H82V38Z"
          fill="url(#safi-bar-c)"
        />

        {/* 7. Baseline Axis (data-draw #7) */}
        <path
          data-draw="true"
          d="M22 98H106"
          stroke="#CBD5E1"
          strokeWidth="2.2"
          strokeLinecap="round"
        />

        {/* 8. Ascending Analytics Trend Vector (data-draw #8) */}
        <path
          data-draw="true"
          d="M32 58L60 40L91 23"
          stroke="#60A5FA"
          strokeWidth="3.5"
          strokeLinecap="round"
          strokeLinejoin="round"
          filter="url(#safi-prism-glow)"
        />

        {/* 9. Trend Vector Arrowhead (data-draw #9) */}
        <path
          data-draw="true"
          d="M81 22H92V33"
          stroke="#60A5FA"
          strokeWidth="3.2"
          strokeLinecap="round"
          strokeLinejoin="round"
        />

        {/* 10. Verified Check Indicator (data-draw #10) */}
        <path
          data-draw="true"
          d="M28 32L33 37L43 26"
          stroke="#34D399"
          strokeWidth="2.8"
          strokeLinecap="round"
          strokeLinejoin="round"
        />

        {/* 3 Precision Data Nodes (.safi-node) */}
        <circle
          className="safi-node"
          cx="32"
          cy="58"
          r="3.8"
          fill="#F8FAFC"
          stroke="#3B82F6"
          strokeWidth="1.6"
        />
        <circle
          className="safi-node"
          cx="60"
          cy="40"
          r="4"
          fill="#F8FAFC"
          stroke="#3B82F6"
          strokeWidth="1.6"
        />
        <circle
          className="safi-node"
          cx="91"
          cy="23"
          r="4.2"
          fill="#34D399"
          stroke="#0F172A"
          strokeWidth="1.6"
        />
      </svg>

      {!hideText ? (
        <div style={{ display: "flex", flexDirection: "column", lineHeight: 1.2 }}>
          <span
            style={{
              fontSize: `${scale.title}px`,
              fontWeight: 800,
              letterSpacing: "-0.02em",
              color: "var(--text-primary)"
            }}
          >
            {wordmarkTitle}
          </span>
          <span
            className="safi-mono"
            style={{
              fontSize: `${scale.subtitle}px`,
              fontWeight: 600,
              color: "var(--text-secondary)",
              opacity: showTagline ? 0.95 : 0.8
            }}
            dir="ltr"
          >
            {lang === "ar" ? `البيانات النظيفة • ${WORDMARK_SUBTITLE}` : WORDMARK_SUBTITLE}
          </span>
        </div>
      ) : null}
    </div>
  );
}
