/**
 * Cinematic SafiData intro.
 *
 * The title is kept as intact Arabic/English runs. The animation waits for the
 * actual local fonts and hands off to the application without unmounting the
 * intro until the application has committed.
 */
import type { ReactNode } from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import { gsap } from "gsap";
import { MOTION_DOTS } from "./SafiPrismLogo";

export interface SafiIntroSplashProps {
  readonly open: boolean;
  readonly onAnimationComplete?: (() => void) | undefined;
}

export const INTRO_START_PAUSE_MS = 1_000;
export const INTRO_PROGRESS_DURATION_SECONDS = 2.25;
export const INTRO_PROGRESS_COMPLETION_HOLD_SECONDS = 0.22;

function canRunIntroTimeline(): boolean {
  if (
    typeof window === "undefined" ||
    typeof document === "undefined" ||
    typeof window.requestAnimationFrame !== "function"
  ) {
    return false;
  }

  return (
    typeof window.matchMedia !== "function" ||
    !window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

async function waitForIntroFonts(): Promise<void> {
  if (typeof document === "undefined" || !document.fonts) {
    return;
  }

  await Promise.all([
    document.fonts.load('700 44px "IBM Plex Sans Arabic"'),
    document.fonts.load('600 16px "IBM Plex Sans Arabic"'),
    document.fonts.load('700 44px "Inter"')
  ]);
  await document.fonts.ready;
}

export function SafiIntroSplash({ open, onAnimationComplete }: SafiIntroSplashProps): ReactNode {
  const rootRef = useRef<HTMLDivElement | null>(null);
  const svgRef = useRef<SVGSVGElement | null>(null);
  const titleRef = useRef<HTMLHeadingElement | null>(null);
  const progressRef = useRef<HTMLDivElement | null>(null);
  const timelineRef = useRef<gsap.core.Timeline | null>(null);
  const finishedRef = useRef(false);
  const [fontsReady, setFontsReady] = useState(false);

  const finishOnce = useCallback((): void => {
    if (finishedRef.current) {
      return;
    }
    finishedRef.current = true;
    onAnimationComplete?.();
  }, [onAnimationComplete]);

  const skipIntro = useCallback((): void => {
    const timeline = timelineRef.current;
    if (timeline !== null) {
      timeline.progress(1);
    }
    finishOnce();
  }, [finishOnce]);

  useEffect(() => {
    if (!open) {
      timelineRef.current?.kill();
      timelineRef.current = null;
      return;
    }

    let cancelled = false;
    let fallbackTimer: number | undefined;
    let startDelayTimer: number | undefined;
    let startDelayResolve: (() => void) | null = null;

    const startTimeline = async (): Promise<void> => {
      try {
        await waitForIntroFonts();
      } catch {
        // A local font failure falls back to the system font; it must not trap startup.
      }

      if (cancelled || finishedRef.current) {
        return;
      }

      setFontsReady(true);

      const root = rootRef.current;
      const svg = svgRef.current;
      if (root === null || svg === null) {
        finishOnce();
        return;
      }

      // Make the font-ready frame visible before starting the one-second pause.
      root.dataset.fontsReady = "true";

      const dots = Array.from(svg.querySelectorAll<SVGCircleElement>("[data-intro-dot]"));
      const bars = Array.from(svg.querySelectorAll<SVGRectElement>("[data-intro-bar]"));
      const trendPath = svg.querySelector<SVGPathElement>("[data-intro-trend]");
      const title = titleRef.current;
      const progress = progressRef.current;

      if (trendPath !== null && typeof trendPath.getTotalLength === "function") {
        const length = trendPath.getTotalLength();
        trendPath.style.strokeDasharray = String(length);
        trendPath.style.strokeDashoffset = String(length);
      }

      // Keep every part of the intro still for one full second after fonts are ready.
      await new Promise<void>((resolve) => {
        startDelayResolve = resolve;
        startDelayTimer = window.setTimeout(resolve, INTRO_START_PAUSE_MS);
      });
      startDelayTimer = undefined;
      startDelayResolve = null;

      if (cancelled || finishedRef.current) {
        return;
      }

      root.dataset.motionReady = "true";

      if (!canRunIntroTimeline()) {
        if (progress !== null) {
          gsap.set(progress, { scaleX: 1 });
        }
        if (trendPath !== null) {
          trendPath.style.strokeDasharray = "none";
          trendPath.style.strokeDashoffset = "0";
        }
        finishOnce();
        return;
      }

      const timeline = gsap.timeline({
        defaults: { ease: "power3.out" },
        onComplete: () => {
          // Never hand off with a partially filled progress indicator.
          if (progress !== null) {
            gsap.set(progress, { scaleX: 1, transformOrigin: "right center" });
          }
          if (fallbackTimer !== undefined) {
            window.clearTimeout(fallbackTimer);
            fallbackTimer = undefined;
          }
          timelineRef.current = null;
          finishOnce();
        }
      });
      timelineRef.current = timeline;

      if (title !== null) {
        timeline.fromTo(
          title,
          { autoAlpha: 0, y: 8, filter: "blur(4px)" },
          {
            autoAlpha: 1,
            y: 0,
            filter: "blur(0px)",
            duration: 0.42,
            ease: "power3.out",
            clearProps: "filter,transform,opacity,visibility"
          },
          0
        );
      }

      if (progress !== null) {
        timeline.fromTo(
          progress,
          { scaleX: 0, transformOrigin: "right center" },
          { scaleX: 1, duration: INTRO_PROGRESS_DURATION_SECONDS, ease: "power2.inOut" },
          0
        );
      }

      dots.forEach((dot, index) => {
        const spec = MOTION_DOTS[index];
        if (spec !== undefined) {
          timeline.to(
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

      timeline.to(
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
        timeline.fromTo(
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
        timeline.to(
          trendPath,
          {
            strokeDashoffset: 0,
            duration: 0.4,
            ease: "power2.out"
          },
          1.22
        );
      }

      // Mark the progress completion, then delay the handoff without changing the composition.
      timeline.call(
        () => undefined,
        [],
        INTRO_PROGRESS_DURATION_SECONDS + INTRO_PROGRESS_COMPLETION_HOLD_SECONDS
      );

      fallbackTimer = window.setTimeout(() => {
        timeline.kill();
        timelineRef.current = null;
        if (progress !== null) {
          gsap.set(progress, { scaleX: 1, transformOrigin: "right center" });
        }
        finishOnce();
      }, 6000);
    };

    void startTimeline();

    return () => {
      cancelled = true;
      if (fallbackTimer !== undefined) {
        window.clearTimeout(fallbackTimer);
      }
      if (startDelayTimer !== undefined) {
        window.clearTimeout(startDelayTimer);
        startDelayResolve?.();
      }
      timelineRef.current?.kill();
      timelineRef.current = null;
    };
  }, [finishOnce, open]);

  if (!open) {
    return null;
  }

  return (
    <div
      ref={rootRef}
      className="safi-intro-splash"
      data-fonts-ready={fontsReady ? "true" : "false"}
      role="region"
      aria-label="عرض إقلاع صافي البيانات السينمائي"
      dir="rtl"
    >
      <div className="safi-intro-stage">
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
            <g className="safi-pm-logo-orbit">
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
              {MOTION_DOTS.map((dot, index) => (
                <circle
                  key={dot.id}
                  data-intro-dot={dot.id}
                  cx={dot.startX}
                  cy={dot.startY}
                  r="5.5"
                  fill={index % 3 === 0 ? "#F8FAFC" : index % 3 === 1 ? "#94A3B8" : "#60A5FA"}
                />
              ))}
            </g>

            <g>
              <rect data-intro-bar="1" x="26" y="68" width="18" height="30" rx="4" fill="#64748B" />
              <rect data-intro-bar="2" x="55" y="50" width="18" height="48" rx="4" fill="#94A3B8" />
              <rect data-intro-bar="3" x="84" y="34" width="18" height="64" rx="4" fill="#F8FAFC" />
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

        <div className="safi-intro-copy">
          <h2 ref={titleRef} className="safi-intro-title" aria-label="صافي البيانات • SafiData">
            <bdi dir="rtl">صافي البيانات</bdi>
            <span aria-hidden="true">•</span>
            <bdi dir="ltr">SafiData</bdi>
          </h2>
          <p className="safi-intro-tagline">
            بياناتك الفوضوية تصبح واضحة في ثوانٍ • خصوصية مطلقة على جهازك
          </p>
        </div>

        <div className="safi-intro-progress-track" aria-hidden="true">
          <div ref={progressRef} className="safi-intro-progress-fill" />
        </div>

        <button
          type="button"
          className="safi-intro-skip"
          onClick={skipIntro}
          aria-label="الدخول إلى الموقع"
        >
          الدخول إلى الموقع ←
        </button>
      </div>
    </div>
  );
}
