/**
 * `apps/web/src/components/brand/SafiPrismLogo.tsx`
 *
 * The SafiData identity mark, built as pure SVG + GSAP motion graphics:
 * - Zero-Trust hex shield drawn progressively on mount (strokeDasharray/Offset);
 * - crystal prism with a glowing BLAKE3-style core;
 * - three purification waves entering raw and leaving clean, each carrying a node.
 *
 * All motion is transform/opacity/stroke only (no layout reads per frame), and the
 * whole component freezes when `prefers-reduced-motion: reduce` is set.
 */
import type { PointerEvent as ReactPointerEvent, ReactNode } from "react";
import { useEffect, useRef } from "react";
import { gsap } from "gsap";

export type SafiPrismLogoSize = "sm" | "md" | "lg";

export interface SafiPrismLogoProps {
  readonly size?: SafiPrismLogoSize;
}

interface SizeScale {
  readonly svg: number;
  readonly title: number;
  readonly subtitle: number;
  readonly gap: number;
}

const SIZES: Record<SafiPrismLogoSize, SizeScale> = {
  sm: { svg: 40, title: 15, subtitle: 10, gap: 10 },
  md: { svg: 72, title: 22, subtitle: 12, gap: 14 },
  lg: { svg: 112, title: 32, subtitle: 15, gap: 18 }
};

const WORDMARK_TITLE = "صافي البيانات";
const WORDMARK_SUBTITLE = "SafiData • Gold Master v1.0";
const LOGO_LABEL = "شعار صافي البيانات";

/** Entry paths (raw data) and their mirrored exit paths (purified data). */
const WAVES: readonly { readonly id: string; readonly entry: string; readonly exit: string }[] = [
  {
    id: "wave-top",
    entry: "M2 40 C 26 40, 34 60, 54 60",
    exit: "M66 60 C 86 60, 94 40, 118 40"
  },
  {
    id: "wave-mid",
    entry: "M2 60 C 24 60, 34 60, 54 60",
    exit: "M66 60 C 86 60, 96 60, 118 60"
  },
  {
    id: "wave-low",
    entry: "M2 80 C 26 80, 34 60, 54 60",
    exit: "M66 60 C 86 60, 94 80, 118 80"
  }
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

export function SafiPrismLogo({ size = "md" }: SafiPrismLogoProps): ReactNode {
  const scale = SIZES[size];
  const svgRef = useRef<SVGSVGElement | null>(null);
  const tiltRef = useRef<HTMLDivElement | null>(null);
  const tiltSetters = useRef<{
    rotationX: (value: number) => void;
    rotationY: (value: number) => void;
    scale: (value: number) => void;
  } | null>(null);

  useEffect(() => {
    const svg = svgRef.current;
    if (svg === null || !canAnimate()) {
      return;
    }

    const context = gsap.context(() => {
      const drawables = Array.from(svg.querySelectorAll<SVGGeometryElement>("[data-draw]"));
      drawables.forEach((path) => {
        const length = path.getTotalLength();
        gsap.set(path, { strokeDasharray: length, strokeDashoffset: length });
      });
      gsap.to(drawables, {
        strokeDashoffset: 0,
        duration: 1.2,
        ease: "expo.out",
        stagger: 0.12
      });

      const core = svg.querySelector<SVGCircleElement>("[data-prism-core]");
      if (core !== null) {
        gsap.to(core, {
          opacity: 0.45,
          scale: 1.14,
          transformOrigin: "50% 50%",
          duration: 2.6,
          repeat: -1,
          yoyo: true,
          ease: "sine.inOut"
        });
      }

      const nodes = Array.from(svg.querySelectorAll<SVGCircleElement>("[data-node]"));
      nodes.forEach((node, index) => {
        const wave = WAVES[index];
        const path =
          wave === undefined ? null : svg.querySelector<SVGPathElement>(`[data-wave="${wave.id}"]`);
        if (path === null) {
          return;
        }
        const total = path.getTotalLength();
        const progress = { value: 0 };
        gsap.to(progress, {
          value: 1,
          duration: 3.4,
          repeat: -1,
          ease: "none",
          delay: index * 0.55,
          onUpdate: () => {
            const point = path.getPointAtLength(total * progress.value);
            node.setAttribute("cx", point.x.toFixed(2));
            node.setAttribute("cy", point.y.toFixed(2));
            node.setAttribute("opacity", "0.35");
          }
        });
      });
    }, svg);

    return () => {
      context.revert();
    };
  }, []);

  const handlePointerMove = (event: ReactPointerEvent<HTMLDivElement>): void => {
    const element = tiltRef.current;
    if (element === null || !canAnimate()) {
      return;
    }

    if (tiltSetters.current === null) {
      tiltSetters.current = {
        rotationX: gsap.quickTo(element, "rotationX", { duration: 0.5, ease: "power2.out" }),
        rotationY: gsap.quickTo(element, "rotationY", { duration: 0.5, ease: "power2.out" }),
        scale: gsap.quickTo(element, "scale", { duration: 0.5, ease: "power2.out" })
      };
    }

    const bounds = element.getBoundingClientRect();
    const offsetX = (event.clientX - bounds.left) / bounds.width - 0.5;
    const offsetY = (event.clientY - bounds.top) / bounds.height - 0.5;

    tiltSetters.current.rotationY(offsetX * 18);
    tiltSetters.current.rotationX(offsetY * -18);
    tiltSetters.current.scale(1.05);
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
      duration: 0.9,
      ease: "elastic.out(1, 0.4)"
    });
  };

  return (
    <div
      dir="rtl"
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: `${scale.gap}px`,
        fontFamily: "var(--font-arabic)"
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
          willChange: "transform"
        }}
      >
        <svg
          ref={svgRef}
          role="img"
          aria-label={LOGO_LABEL}
          viewBox="0 0 120 120"
          width={scale.svg}
          height={scale.svg}
          focusable="false"
          style={{ overflow: "visible" }}
        >
          <defs>
            <linearGradient id="safi-shield-gradient" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stopColor="#10b981" />
              <stop offset="55%" stopColor="#06b6d4" />
              <stop offset="100%" stopColor="#3b82f6" />
            </linearGradient>
            <linearGradient id="safi-prism-face" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0%" stopColor="#10b981" stopOpacity="0.55" />
              <stop offset="100%" stopColor="#06b6d4" stopOpacity="0.12" />
            </linearGradient>
            <linearGradient id="safi-wave-gradient" x1="0" y1="0" x2="1" y2="0">
              <stop offset="0%" stopColor="#10b981" stopOpacity="0.25" />
              <stop offset="100%" stopColor="#06b6d4" stopOpacity="0.85" />
            </linearGradient>
            <filter id="safi-prism-glow" x="-60%" y="-60%" width="220%" height="220%">
              <feGaussianBlur stdDeviation="2.5" result="safi-glow-blur" />
              <feMerge>
                <feMergeNode in="safi-glow-blur" />
                <feMergeNode in="SourceGraphic" />
              </feMerge>
            </filter>
          </defs>

          {WAVES.map((wave) => (
            <path
              key={wave.id}
              data-draw="true"
              data-wave={wave.id}
              d={wave.entry}
              fill="none"
              stroke="url(#safi-wave-gradient)"
              strokeWidth="1.4"
              strokeLinecap="round"
            />
          ))}

          <path
            data-draw="true"
            d="M60 6 L104 30 L104 78 L60 114 L16 78 L16 30 Z"
            fill="rgba(15, 20, 32, 0.35)"
            stroke="url(#safi-shield-gradient)"
            strokeWidth="2"
            strokeLinejoin="round"
            strokeLinecap="round"
          />

          <path
            data-draw="true"
            d="M60 34 L82 72 L38 72 Z"
            fill="url(#safi-prism-face)"
            stroke="#06b6d4"
            strokeWidth="1.3"
            strokeLinejoin="round"
          />
          <path
            data-draw="true"
            d="M60 34 L60 72 L38 72 Z"
            fill="rgba(6, 182, 212, 0.14)"
            stroke="#10b981"
            strokeWidth="1"
            strokeLinejoin="round"
          />
          <path
            data-draw="true"
            d="M60 34 L82 72 L60 72 Z"
            fill="rgba(59, 130, 246, 0.12)"
            stroke="#3b82f6"
            strokeWidth="1"
            strokeLinejoin="round"
          />

          <circle
            data-prism-core="true"
            cx="60"
            cy="60"
            r="4.6"
            fill="#10b981"
            filter="url(#safi-prism-glow)"
          />

          {WAVES.map((wave) => (
            <path
              key={`${wave.id}-exit`}
              data-draw="true"
              d={wave.exit}
              fill="none"
              stroke="url(#safi-wave-gradient)"
              strokeWidth="1.6"
              strokeLinecap="round"
              opacity="0.9"
            />
          ))}

          {WAVES.map((wave, index) => (
            <circle
              key={`${wave.id}-node`}
              data-node={index}
              className="safi-node"
              cx={index === 0 ? 8 : index === 1 ? 14 : 20}
              cy={index === 0 ? 42 : index === 1 ? 60 : 78}
              r="2.6"
              fill="#10b981"
              filter="url(#safi-prism-glow)"
            />
          ))}
        </svg>
      </div>

      <span style={{ display: "flex", flexDirection: "column", lineHeight: 1.2 }}>
        <span
          style={{
            fontSize: `${scale.title}px`,
            fontWeight: 700,
            color: "var(--text-primary)",
            letterSpacing: "-0.01em"
          }}
        >
          {WORDMARK_TITLE}
        </span>
        <span
          className="safi-shiny-text"
          style={{ fontSize: `${scale.subtitle}px`, fontWeight: 500, marginTop: "2px" }}
        >
          {WORDMARK_SUBTITLE}
        </span>
      </span>
    </div>
  );
}
