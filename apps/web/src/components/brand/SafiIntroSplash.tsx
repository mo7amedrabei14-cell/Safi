/**
 * Cinematic SafiData intro.
 *
 * The title is kept as intact Arabic/English runs. Logo motion starts immediately,
 * local fonts load in parallel, and the intro exits over the committed application.
 */
import type { ReactNode } from "react";
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { gsap } from "gsap";
import { MOTION_DOTS } from "./SafiPrismLogo";
import type { SafiLang } from "../layout/FloatingGlassDock";

export interface SafiIntroSplashProps {
  readonly open: boolean;
  readonly lang?: SafiLang;
  readonly exiting?: boolean | undefined;
  readonly onAnimationComplete?: (() => void) | undefined;
  readonly onExitComplete?: (() => void) | undefined;
}

export const INTRO_MOTION_DURATION_SECONDS = 0.58;
export const INTRO_EXIT_DURATION_SECONDS = 0.48;

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

export function SafiIntroSplash({
  open,
  lang = "en",
  exiting = false,
  onAnimationComplete,
  onExitComplete
}: SafiIntroSplashProps): ReactNode {
  const rootRef = useRef<HTMLDivElement | null>(null);
  const svgRef = useRef<SVGSVGElement | null>(null);
  const transitionPathRef = useRef<SVGPathElement | null>(null);
  const timelineRef = useRef<gsap.core.Timeline | null>(null);
  const finishedRef = useRef(false);
  const [fontsReady, setFontsReady] = useState(false);

  const finishOnce = useCallback((): void => {
    if (finishedRef.current) {
      return;
    }
    finishedRef.current = true;
    if (rootRef.current !== null) {
      rootRef.current.dataset.motionComplete = "true";
    }
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
      return;
    }

    let cancelled = false;
    const revealCopy = async (): Promise<void> => {
      try {
        await waitForIntroFonts();
      } catch {
        // A local font failure falls back to the system font without blocking motion.
      }
      if (!cancelled) {
        setFontsReady(true);
        const root = rootRef.current;
        if (root !== null) {
          root.dataset.fontsReady = "true";
        }
      }
    };

    void revealCopy();
    return () => {
      cancelled = true;
    };
  }, [open]);

  useLayoutEffect(() => {
    if (!open) {
      timelineRef.current?.kill();
      timelineRef.current = null;
      return;
    }

    let fallbackTimer: number | undefined;
    const root = rootRef.current;
    const svg = svgRef.current;
    if (root === null || svg === null) {
      finishOnce();
      return;
    }

    const dots = Array.from(svg.querySelectorAll<SVGCircleElement>("[data-intro-dot]"));
    const bars = Array.from(svg.querySelectorAll<SVGRectElement>("[data-intro-bar]"));
    const trendPath = svg.querySelector<SVGPathElement>("[data-intro-trend]");
    if (trendPath !== null && typeof trendPath.getTotalLength === "function") {
      const length = trendPath.getTotalLength();
      trendPath.style.strokeDasharray = String(length);
      trendPath.style.strokeDashoffset = String(length);
    }

    root.dataset.motionReady = "true";

    if (!canRunIntroTimeline()) {
      gsap.set(dots, { opacity: 0 });
      gsap.set(bars, { opacity: 1, scaleY: 1, transformOrigin: "center bottom" });
      if (trendPath !== null) {
        trendPath.style.strokeDasharray = "none";
        trendPath.style.strokeDashoffset = "0";
      }
      finishOnce();
      return;
    }

    gsap.set(bars, { opacity: 0, scaleY: 0, transformOrigin: "center bottom" });
    const timeline = gsap.timeline({
      defaults: { ease: "power3.out" },
      onComplete: () => {
        if (fallbackTimer !== undefined) {
          window.clearTimeout(fallbackTimer);
          fallbackTimer = undefined;
        }
        timelineRef.current = null;
        finishOnce();
      }
    });
    timelineRef.current = timeline;

    dots.forEach((dot, index) => {
      const spec = MOTION_DOTS[index];
      if (spec !== undefined) {
        timeline.to(
          dot,
          {
            attr: { cx: spec.gridX, cy: spec.gridY },
            duration: 0.27,
            ease: "back.out(1.35)"
          },
          0.04 + index * 0.012
        );
      }
    });

    timeline.to(
      dots,
      {
        opacity: 0,
        scale: 0.5,
        duration: 0.15,
        ease: "power2.in"
      },
      0.25
    );

    if (bars.length > 0) {
      timeline.to(
        bars,
        {
          scaleY: 1,
          opacity: 1,
          duration: 0.2,
          stagger: 0.035,
          ease: "back.out(1.45)"
        },
        0.27
      );
    }

    if (trendPath !== null) {
      timeline.to(
        trendPath,
        {
          strokeDashoffset: 0,
          duration: 0.2,
          ease: "power2.out"
        },
        0.36
      );
    }

    timeline.call(() => undefined, [], INTRO_MOTION_DURATION_SECONDS);

    fallbackTimer = window.setTimeout(() => {
      timeline.kill();
      timelineRef.current = null;
      gsap.set(dots, { opacity: 0 });
      gsap.set(bars, { opacity: 1, scaleY: 1, transformOrigin: "center bottom" });
      if (trendPath !== null) {
        trendPath.style.strokeDasharray = "none";
        trendPath.style.strokeDashoffset = "0";
      }
      finishOnce();
    }, 6000);

    return () => {
      if (fallbackTimer !== undefined) {
        window.clearTimeout(fallbackTimer);
      }
      timelineRef.current?.kill();
      timelineRef.current = null;
    };
  }, [finishOnce, open]);

  useEffect(() => {
    if (!open || !exiting) {
      return;
    }

    const root = rootRef.current;
    if (root === null) {
      onExitComplete?.();
      return;
    }

    root.dataset.exiting = "true";
    if (!canRunIntroTimeline()) {
      onExitComplete?.();
      return;
    }

    const stage = root.querySelector<HTMLElement>(".safi-intro-stage");
    const arrow = root.querySelector<SVGSVGElement>(".safi-intro-arrow");
    const transitionPath = transitionPathRef.current;
    if (stage === null || arrow === null || transitionPath === null) {
      onExitComplete?.();
      return;
    }

    const arrowBounds = arrow.getBoundingClientRect();
    const isRtl = lang === "ar";
    const startX = isRtl ? arrowBounds.left : arrowBounds.right;
    const endX = isRtl ? 24 : window.innerWidth - 24;
    const centerY = arrowBounds.top + arrowBounds.height / 2;
    transitionPath.setAttribute("d", `M ${startX} ${centerY} H ${endX}`);
    const pathLength = transitionPath.getTotalLength();
    transitionPath.style.strokeDasharray = `${pathLength}`;
    transitionPath.style.strokeDashoffset = `${pathLength}`;

    const radius = Math.hypot(window.innerWidth, window.innerHeight) + 24;
    gsap.set(root, { clipPath: `circle(${radius}px at ${endX}px ${centerY}px)` });
    gsap.set(transitionPath, { opacity: 1 });

    const exitTimeline = gsap.timeline({ onComplete: () => onExitComplete?.() });
    exitTimeline.to(
      transitionPath,
      {
        strokeDashoffset: 0,
        duration: INTRO_EXIT_DURATION_SECONDS * 0.46,
        ease: "power2.out"
      },
      0
    );
    exitTimeline.to(
      stage,
      { autoAlpha: 0, y: -20, scale: 0.96, duration: 0.32, ease: "power2.in" },
      0.12
    );
    exitTimeline.to(
      root,
      {
        clipPath: `circle(0px at ${endX}px ${centerY}px)`,
        duration: INTRO_EXIT_DURATION_SECONDS * 0.57,
        ease: "power2.inOut"
      },
      0.28
    );

    return () => {
      exitTimeline.kill();
    };
  }, [exiting, lang, onExitComplete, open]);

  if (!open) {
    return null;
  }

  return (
    <div
      ref={rootRef}
      className="safi-intro-splash"
      data-fonts-ready={fontsReady ? "true" : "false"}
      role="region"
      aria-label={
        lang === "ar" ? "عرض إقلاع صافي البيانات السينمائي" : "SafiData cinematic startup"
      }
      dir={lang === "ar" ? "rtl" : "ltr"}
    >
      <svg className="safi-intro-transition" aria-hidden="true" focusable="false">
        <defs>
          <linearGradient id="safi-intro-transition-gradient">
            <stop offset="0%" stopColor="var(--sf-color-primary-soft)" stopOpacity="0.35" />
            <stop offset="55%" stopColor="var(--sf-color-info)" />
            <stop offset="100%" stopColor="var(--sf-color-text-primary)" />
          </linearGradient>
          <marker
            id="safi-intro-transition-arrowhead"
            viewBox="0 0 10 10"
            refX="8"
            refY="5"
            markerWidth="8"
            markerHeight="8"
            orient="auto-start-reverse"
          >
            <path d="M 0 0 L 10 5 L 0 10 z" fill="var(--sf-color-text-primary)" />
          </marker>
        </defs>
        <path
          ref={transitionPathRef}
          className="safi-intro-transition-path"
          markerEnd="url(#safi-intro-transition-arrowhead)"
          d=""
        />
      </svg>
      <div className="safi-intro-stage">
        <div className="safi-intro-orb">
          <div className="safi-intro-orb-ring" aria-hidden="true" />
          <svg
            ref={svgRef}
            width={168}
            height={168}
            viewBox="0 0 128 128"
            fill="none"
            aria-label={
              lang === "ar"
                ? "حركة مصفوفة البيانات ومؤشر التحليل"
                : "Data matrix and analytics indicator animation"
            }
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
          <h2
            className="safi-intro-title"
            aria-label={lang === "ar" ? "صافي البيانات" : "SafiData"}
          >
            <bdi dir={lang === "ar" ? "rtl" : "ltr"}>
              {lang === "ar" ? "صافي البيانات" : "SafiData"}
            </bdi>
          </h2>
          <p className="safi-intro-tagline">
            {lang === "ar"
              ? "حوّل بياناتك الفوضوية إلى بيانات واضحة في ثوانٍ • خصوصيتك تبقى على جهازك"
              : "Turn messy data into clear, reliable data in seconds • Your files stay on your device"}
          </p>
        </div>

        <button
          type="button"
          className="safi-intro-skip"
          onClick={skipIntro}
          aria-label={lang === "ar" ? "الدخول إلى الموقع" : "Enter SafiData"}
        >
          <span>{lang === "ar" ? "الدخول إلى الموقع" : "Enter SafiData"}</span>
          <svg
            className="safi-intro-arrow"
            viewBox="0 0 24 24"
            width="18"
            height="18"
            aria-hidden="true"
          >
            <path
              d="M4 12h15m-6-6 6 6-6 6"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.8"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        </button>
      </div>
    </div>
  );
}
