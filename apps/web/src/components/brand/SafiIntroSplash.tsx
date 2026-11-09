/**
 * `apps/web/src/components/brand/SafiIntroSplash.tsx`
 *
 * Apple Keynote-Style Cinematic Intro Before Entering the Website
 * Calm Slate & Silver-Blue Palette (`#090B10`, `#1E293B`, `#60A5FA`, `#34D399`, `#F8FAFC`).
 */
import type { ReactNode } from "react";
import { useCallback, useEffect, useRef } from "react";
import { gsap } from "gsap";
import { MOTION_DOTS } from "./SafiPrismLogo";
import { BlurWordText } from "../ui/BlurWordText";

export interface SafiIntroSplashProps {
  readonly open: boolean;
  readonly onComplete?: (() => void) | undefined;
  readonly onFinish?: (() => void) | undefined;
}

function canRunIntroTimeline(): boolean {
  if (typeof window === "undefined" || typeof document === "undefined") {
    return false;
  }
  if (typeof window.requestAnimationFrame !== "function") {
    return false;
  }
  if (typeof window.matchMedia === "function") {
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      return false;
    }
  }
  return true;
}

export function SafiIntroSplash({
  open,
  onComplete,
  onFinish
}: SafiIntroSplashProps): ReactNode {
  const rootRef = useRef<HTMLDivElement | null>(null);
  const svgRef = useRef<SVGSVGElement | null>(null);
  const progressRef = useRef<HTMLDivElement | null>(null);
  const finishedRef = useRef<boolean>(false);

  const completeOnce = useCallback((): void => {
    if (finishedRef.current) {
      return;
    }
    finishedRef.current = true;
    onComplete?.();
    onFinish?.();
  }, [onComplete, onFinish]);

  useEffect(() => {
    if (!open) {
      finishedRef.current = false;
      return;
    }

    if (!canRunIntroTimeline()) {
      const quickTimer =
        typeof window !== "undefined" ? window.setTimeout(completeOnce, 160) : undefined;
      return () => {
        if (quickTimer !== undefined && typeof window !== "undefined") {
          window.clearTimeout(quickTimer);
        }
      };
    }

    const root = rootRef.current;
    const svg = svgRef.current;
    const progress = progressRef.current;
    if (root === null || svg === null) {
      completeOnce();
      return;
    }

    const hardFallbackTimer = window.setTimeout(() => {
      completeOnce();
    }, 2400);

    const dots = Array.from(svg.querySelectorAll<SVGCircleElement>("[data-intro-dot]"));
    const bars = Array.from(svg.querySelectorAll<SVGRectElement>("[data-intro-bar]"));
    const trendPath = svg.querySelector<SVGPathElement>("[data-intro-trend]");

    if (trendPath !== null && typeof trendPath.getTotalLength === "function") {
      const len = trendPath.getTotalLength();
      trendPath.style.strokeDasharray = `${len}`;
      trendPath.style.strokeDashoffset = `${len}`;
    }

    const tl = gsap.timeline({
      defaults: { ease: "power3.out" },
      onComplete: () => {
        gsap.to(root, {
          opacity: 0,
          y: -18,
          duration: 0.35,
          ease: "power2.inOut",
          onComplete: completeOnce
        });
      }
    });

    if (progress !== null) {
      tl.fromTo(
        progress,
        { scaleX: 0, transformOrigin: "right center" },
        { scaleX: 1, duration: 1.95, ease: "power1.inOut" },
        0
      );
    }

    tl.fromTo(
      dots,
      { scale: 0, opacity: 0, transformOrigin: "50% 50%" },
      {
        scale: 1,
        opacity: 1,
        duration: 0.28,
        stagger: 0.02,
        ease: "back.out(2.2)"
      },
      0
    );

    dots.forEach((dot, idx) => {
      const spec = MOTION_DOTS[idx];
      if (spec !== undefined) {
        tl.to(
          dot,
          {
            attr: { cx: spec.gridX, cy: spec.gridY },
            duration: 0.44,
            ease: "elastic.out(1, 0.75)"
          },
          0.28
        );
      }
    });

    tl.to(
      dots,
      {
        opacity: 0,
        scale: 0.4,
        duration: 0.24,
        ease: "power2.in"
      },
      0.78
    );

    if (bars.length > 0) {
      tl.fromTo(
        bars,
        { scaleY: 0, opacity: 0, transformOrigin: "center bottom" },
        {
          scaleY: 1,
          opacity: 1,
          duration: 0.44,
          stagger: 0.07,
          ease: "back.out(1.6)"
        },
        0.9
      );
    }

    if (trendPath !== null) {
      tl.to(
        trendPath,
        {
          strokeDashoffset: 0,
          duration: 0.4,
          ease: "power2.out"
        },
        1.22
      );
    }

    return () => {
      window.clearTimeout(hardFallbackTimer);
      tl.kill();
    };
  }, [open, completeOnce]);

  if (!open) {
    return null;
  }

  return (
    <div
      ref={rootRef}
      className="safi-intro-splash"
      role="region"
      aria-label="عرض إقلاع صافي البيانات السينمائي"
      dir="rtl"
      onClick={completeOnce}
    >
      <div className="safi-intro-stage" onClick={(e) => e.stopPropagation()}>
        <div className="safi-intro-orb">
          <div className="safi-intro-orb-ring" aria-hidden="true" />
          <svg
            ref={svgRef}
            width={168}
            height={168}
            viewBox="0 0 128 128"
            fill="none"
            aria-label="حركة مصفوفة البيانات ومؤشر التحليل"
          >
            {/* Prompt-Motion S2 Orbital Cross + 4 Satellites */}
            <g className="safi-pm-logo-orbit" style={{ transformOrigin: "64px 64px" }}>
              <circle
                cx="64"
                cy="64"
                r="60"
                stroke="#60A5FA"
                strokeOpacity="0.38"
                strokeWidth="1.4"
                strokeDasharray="6 6"
              />
              <rect x="60" y="1" width="8" height="8" rx="2.5" fill="#2F6BFF" />
              <rect x="119" y="60" width="8" height="8" rx="2.5" fill="#3CC8F0" />
              <rect x="60" y="119" width="8" height="8" rx="2.5" fill="#4ED6A0" />
              <rect x="1" y="60" width="8" height="8" rx="2.5" fill="#F8FAFC" />
            </g>

            <rect
              x="10"
              y="10"
              width="108"
              height="108"
              rx="26"
              fill="#121620"
              stroke="rgba(255,255,255,0.22)"
              strokeWidth="2"
            />

            <g>
              {MOTION_DOTS.map((dot, idx) => (
                <circle
                  key={dot.id}
                  data-intro-dot={dot.id}
                  cx={dot.startX}
                  cy={dot.startY}
                  r="5.5"
                  fill={idx % 3 === 0 ? "#F8FAFC" : idx % 3 === 1 ? "#94A3B8" : "#60A5FA"}
                />
              ))}
            </g>

            <g>
              <rect
                data-intro-bar="1"
                x="26"
                y="68"
                width="18"
                height="30"
                rx="4"
                fill="#64748B"
                style={{ opacity: 0 }}
              />
              <rect
                data-intro-bar="2"
                x="55"
                y="50"
                width="18"
                height="48"
                rx="4"
                fill="#94A3B8"
                style={{ opacity: 0 }}
              />
              <rect
                data-intro-bar="3"
                x="84"
                y="34"
                width="18"
                height="64"
                rx="4"
                fill="#F8FAFC"
                style={{ opacity: 0 }}
              />
              <path
                data-intro-trend="true"
                d="M32 56L62 38L93 22"
                stroke="#60A5FA"
                strokeWidth="4.5"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            </g>
          </svg>
        </div>

        <BlurWordText
          as="h2"
          className="safi-intro-title"
          text="صافي البيانات • SafiData"
          stagger={0.05}
        />

        <p className="safi-intro-tagline">
          بياناتك الفوضوية تصبح واضحة في ثوانٍ • خصوصية مطلقة على جهازك
        </p>

        <div className="safi-intro-progress-track" aria-hidden="true">
          <div
            ref={progressRef}
            style={{
              width: "100%",
              height: "100%",
              borderRadius: "999px",
              background: "linear-gradient(90deg, #60A5FA 0%, #CBD5E1 50%, #F8FAFC 100%)"
            }}
          />
        </div>

        <button
          type="button"
          className="safi-intro-skip"
          onClick={completeOnce}
          aria-label="الدخول إلى الموقع"
        >
          الدخول إلى الموقع ←
        </button>
      </div>
    </div>
  );
}
