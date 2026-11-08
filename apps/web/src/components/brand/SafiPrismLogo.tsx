/**
 * `apps/web/src/components/brand/SafiPrismLogo.tsx`
 *
 * SafiData Brand Emblem & Motion Graphics ("مصفاة النقاء وحرف ص والقطرة")
 * Strictly locked to the 4-color palette:
 *   - `#F9F7F7`
 *   - `#DBE2EF`
 *   - `#3F72AF`
 *   - `#112D4E`
 */
import type { PointerEvent as ReactPointerEvent, ReactNode } from "react";
import { useCallback, useEffect, useRef } from "react";
import { gsap } from "gsap";

export type SafiPrismLogoSize = "sm" | "md" | "lg";

export interface SafiPrismLogoProps {
  readonly size?: SafiPrismLogoSize;
  readonly showTagline?: boolean;
  readonly hideText?: boolean;
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

const WORDMARK_TITLE = "صافي البيانات";
const WORDMARK_SUBTITLE = "SafiData • Gold Master v1.0";
const LOGO_LABEL = "شعار صافي البيانات";

/**
 * 9 scattered-to-grid coordinates for the Motion Studio showcase (Section 7.2):
 */
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

function prefersReducedMotion(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

function canAnimate(): boolean {
  return (
    typeof window !== "undefined" && typeof document !== "undefined" && !prefersReducedMotion()
  );
}

export function SafiPrismLogo({
  size = "md",
  showTagline = true,
  hideText = false
}: SafiPrismLogoProps): ReactNode {
  const scale = SIZES[size];
  const svgRef = useRef<SVGSVGElement | null>(null);
  const tiltRef = useRef<HTMLDivElement | null>(null);
  const timelineRef = useRef<gsap.core.Timeline | null>(null);
  const tiltSetters = useRef<{
    rotationX: (value: number) => void;
    rotationY: (value: number) => void;
    scale: (value: number) => void;
  } | null>(null);

  const runChoreography = useCallback((): void => {
    const svg = svgRef.current;
    if (svg === null || !canAnimate()) {
      return;
    }

    timelineRef.current?.kill();

    const drawPaths = Array.from(svg.querySelectorAll<SVGPathElement>("path[data-draw]"));
    const nodes = Array.from(svg.querySelectorAll<SVGCircleElement>(".safi-node"));
    const corePulse = svg.querySelector<SVGCircleElement>("[data-safi-core-pulse]");

    const tl = gsap.timeline();
    timelineRef.current = tl;

    drawPaths.forEach((path, index) => {
      let length = 220;
      if (typeof path.getTotalLength === "function") {
        try {
          const measured = path.getTotalLength();
          if (Number.isFinite(measured) && measured > 0) {
            length = measured;
          }
        } catch {
          length = 220;
        }
      }

      tl.fromTo(
        path,
        {
          strokeDasharray: length,
          strokeDashoffset: length,
          opacity: 0.4
        },
        {
          strokeDashoffset: 0,
          opacity: 1,
          duration: 0.9,
          ease: "expo.out",
          clearProps: "strokeDasharray,strokeDashoffset,opacity"
        },
        index * 0.05
      );
    });

    const nodeTargets = [
      { startX: 108, startY: 20, midX: 84, midY: 42 },
      { startX: 98, startY: 12, midX: 76, midY: 34 },
      { startX: 114, startY: 34, midX: 88, midY: 50 }
    ];

    nodes.forEach((node, idx) => {
      const target = nodeTargets[idx] ?? nodeTargets[0]!;
      gsap.fromTo(
        node,
        { attr: { cx: target.startX, cy: target.startY }, opacity: 0.55, scale: 0.9 },
        {
          attr: { cx: target.midX, cy: target.midY },
          opacity: 1,
          scale: 1.15,
          duration: 1.4 + idx * 0.25,
          repeat: -1,
          yoyo: true,
          ease: "sine.inOut",
          delay: idx * 0.2
        }
      );
    });

    if (corePulse !== null) {
      gsap.fromTo(
        corePulse,
        { scale: 0.9, opacity: 0.22, transformOrigin: "64px 58px" },
        {
          scale: 1.18,
          opacity: 0.45,
          duration: 1.8,
          repeat: -1,
          yoyo: true,
          ease: "sine.inOut"
        }
      );
    }
  }, []);

  useEffect(() => {
    runChoreography();
    return () => {
      timelineRef.current?.kill();
    };
  }, [runChoreography]);

  const handlePointerMove = (event: ReactPointerEvent<HTMLDivElement>): void => {
    const element = tiltRef.current;
    if (element === null || !canAnimate()) {
      return;
    }

    if (tiltSetters.current === null) {
      tiltSetters.current = {
        rotationX: gsap.quickTo(element, "rotationX", { duration: 0.38, ease: "power2.out" }),
        rotationY: gsap.quickTo(element, "rotationY", { duration: 0.38, ease: "power2.out" }),
        scale: gsap.quickTo(element, "scale", { duration: 0.38, ease: "power2.out" })
      };
    }

    const bounds = element.getBoundingClientRect();
    const offsetX = (event.clientX - bounds.left) / bounds.width - 0.5;
    const offsetY = (event.clientY - bounds.top) / bounds.height - 0.5;

    tiltSetters.current.rotationY(offsetX * 18);
    tiltSetters.current.rotationX(offsetY * -18);
    tiltSetters.current.scale(1.06);
  };

  const handlePointerLeave = (): void => {
    const element = tiltRef.current;
    if (element === null) {
      return;
    }
    if (prefersReducedMotion()) {
      gsap.set(element, { rotationX: 0, rotationY: 0, scale: 1 });
      return;
    }
    gsap.to(element, {
      rotationX: 0,
      rotationY: 0,
      scale: 1,
      duration: 0.7,
      ease: "elastic.out(1, 0.5)"
    });
  };

  return (
    <div
      dir="rtl"
      onClick={runChoreography}
      title="انقر لإعادة تشغيل موشن شعار صافي"
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: `${scale.gap}px`,
        fontFamily: "var(--font-arabic)",
        cursor: "pointer",
        userSelect: "none"
      }}
    >
      <div
        ref={tiltRef}
        onPointerMove={handlePointerMove}
        onPointerLeave={handlePointerLeave}
        style={{
          width: `${scale.svg}px`,
          height: `${scale.svg}px`,
          perspective: "700px",
          transformStyle: "preserve-3d",
          willChange: "transform",
          flex: "0 0 auto"
        }}
      >
        <svg
          ref={svgRef}
          role="img"
          aria-label={LOGO_LABEL}
          viewBox="0 0 128 128"
          width={scale.svg}
          height={scale.svg}
          focusable="false"
          style={{ overflow: "visible", display: "block" }}
        >
          <defs>
            {/* Locked 4-Color Palette Gradients (`#F9F7F7`, `#DBE2EF`, `#3F72AF`, `#112D4E`) */}
            <linearGradient
              id="safi-crystal-grad"
              x1="18"
              y1="12"
              x2="110"
              y2="116"
              gradientUnits="userSpaceOnUse"
            >
              <stop offset="0%" stopColor="#3F72AF" />
              <stop offset="100%" stopColor="#112D4E" />
            </linearGradient>

            <linearGradient
              id="safi-pure-drop-fill"
              x1="64"
              y1="68"
              x2="64"
              y2="112"
              gradientUnits="userSpaceOnUse"
            >
              <stop offset="0%" stopColor="#3F72AF" />
              <stop offset="100%" stopColor="#112D4E" />
            </linearGradient>

            <linearGradient
              id="safi-sad-grad"
              x1="36"
              y1="44"
              x2="92"
              y2="74"
              gradientUnits="userSpaceOnUse"
            >
              <stop offset="0%" stopColor="#F9F7F7" />
              <stop offset="100%" stopColor="#DBE2EF" />
            </linearGradient>

            <filter id="safi-prism-glow" x="-35%" y="-35%" width="170%" height="170%">
              <feGaussianBlur stdDeviation="2.5" result="safi-glow-blur" />
              <feMerge>
                <feMergeNode in="safi-glow-blur" />
                <feMergeNode in="SourceGraphic" />
              </feMerge>
            </filter>
          </defs>

          <circle
            data-safi-core-pulse="true"
            cx="64"
            cy="58"
            r="34"
            fill="#DBE2EF"
            opacity="0.35"
          />

          {/* 1. Outer Hexagonal Crystal Purifier Shield */}
          <path
            data-draw="true"
            d="M64 10 L106 33 L106 79 L64 102 L22 79 L22 33 Z"
            fill="#112D4E"
            stroke="#3F72AF"
            strokeWidth="3.2"
            strokeLinejoin="round"
            filter="url(#safi-prism-glow)"
          />

          {/* 2. Inner Faceted Purifier Funnel */}
          <path
            data-draw="true"
            d="M32 39 L64 21 L96 39 L82 72 L46 72 Z"
            fill="#3F72AF"
            fillOpacity="0.28"
            stroke="#DBE2EF"
            strokeWidth="1.8"
            strokeLinejoin="round"
          />

          {/* 3. Raw Intake Stream 1 */}
          <path
            data-draw="true"
            d="M110 18 C98 24 90 30 80 39"
            fill="none"
            stroke="#3F72AF"
            strokeWidth="2"
            strokeDasharray="4 3"
            strokeLinecap="round"
          />

          {/* 4. Raw Intake Stream 2 */}
          <path
            data-draw="true"
            d="M116 32 C104 38 94 42 84 48"
            fill="none"
            stroke="#112D4E"
            strokeWidth="1.8"
            strokeDasharray="3 3"
            strokeLinecap="round"
          />

          {/* 5. Prism Internal Refraction Axis */}
          <path
            data-draw="true"
            d="M64 21 L64 74"
            fill="none"
            stroke="#DBE2EF"
            strokeWidth="1.5"
            strokeLinecap="round"
            opacity="0.65"
          />

          {/* 6. Geometric Arabic Letter "ص" (Sad) at the Heart of the Purifier */}
          <path
            data-draw="true"
            d="M88 55 C88 45 73 45 66 53 C62 57 66 61 76 61 L88 61 M65 56 L58 56 C58 68 41 69 40 57 C39 49 44 45 48 45"
            fill="none"
            stroke="url(#safi-sad-grad)"
            strokeWidth="4.2"
            strokeLinecap="round"
            strokeLinejoin="round"
            filter="url(#safi-prism-glow)"
          />

          {/* 7. Pure Data Drop Emerging Below */}
          <path
            data-draw="true"
            d="M64 74 C64 74 51 90 51 99 C51 106.5 56.8 112 64 112 C71.2 112 77 106.5 77 99 C77 90 64 74 64 74 Z"
            fill="url(#safi-pure-drop-fill)"
            stroke="#F9F7F7"
            strokeWidth="2.2"
            filter="url(#safi-prism-glow)"
          />

          {/* 8. Pure Output Harmonic Wave 1 */}
          <path
            data-draw="true"
            d="M44 115 Q64 121 84 115"
            fill="none"
            stroke="#3F72AF"
            strokeWidth="2.2"
            strokeLinecap="round"
          />

          {/* 9. Pure Output Harmonic Wave 2 */}
          <path
            data-draw="true"
            d="M34 120 Q64 127 94 120"
            fill="none"
            stroke="#112D4E"
            strokeWidth="1.8"
            strokeLinecap="round"
          />

          {/* 10. Pure Output Harmonic Wave 3 */}
          <path
            data-draw="true"
            d="M24 125 Q64 132 104 125"
            fill="none"
            stroke="#3F72AF"
            strokeWidth="1.4"
            strokeLinecap="round"
          />

          {/* 3 Raw Data Particles (`.safi-node` x3) */}
          <circle className="safi-node" cx="108" cy="20" r="3.4" fill="#3F72AF" />
          <circle className="safi-node" cx="98" cy="12" r="2.8" fill="#112D4E" />
          <circle className="safi-node" cx="114" cy="34" r="3.1" fill="#DBE2EF" stroke="#112D4E" strokeWidth="1" />
        </svg>
      </div>

      {!hideText ? (
        <span style={{ display: "flex", flexDirection: "column", lineHeight: 1.25 }}>
          <span
            style={{
              fontSize: `${scale.title}px`,
              fontWeight: 700,
              color: "var(--text-primary)",
              letterSpacing: "-0.01em",
              whiteSpace: "nowrap"
            }}
          >
            {WORDMARK_TITLE}
          </span>
          {showTagline ? (
            <span
              className="safi-shiny-text"
              style={{
                fontSize: `${scale.subtitle}px`,
                fontWeight: 600,
                marginTop: "2px",
                whiteSpace: "nowrap"
              }}
            >
              مصفاة النقاء الذكية • SafiData
            </span>
          ) : null}
          <span className="safi-visually-hidden">{WORDMARK_SUBTITLE}</span>
        </span>
      ) : (
        <span className="safi-visually-hidden">
          {WORDMARK_TITLE} — {WORDMARK_SUBTITLE}
        </span>
      )}
    </div>
  );
}
