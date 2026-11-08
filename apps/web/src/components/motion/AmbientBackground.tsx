/**
 * `apps/web/src/components/motion/AmbientBackground.tsx`
 *
 * Two GPU-cheap layers behind the whole app:
 * 1. Aurora mesh — three blurred colour halos drifting with GSAP (transform only).
 * 2. Dot grid — a 28px dot lattice faded out with a radial mask.
 *
 * The container is `aria-hidden`, never receives pointer events, and both layers
 * are frozen when the user asks for reduced motion.
 */
import type { ReactNode } from "react";
import { useEffect, useRef } from "react";
import { gsap } from "gsap";

interface AuroraHalo {
  readonly id: string;
  readonly gradient: string;
  readonly top: string;
  readonly left: string;
  readonly size: string;
  readonly drift: {
    readonly x: number;
    readonly y: number;
    readonly scale: number;
    readonly duration: number;
  };
}

const AURORA_HALOS: readonly AuroraHalo[] = [
  {
    id: "emerald",
    gradient: "radial-gradient(circle at 50% 50%, #10b981 0%, transparent 70%)",
    top: "-18%",
    left: "-12%",
    size: "58vw",
    drift: { x: 90, y: 60, scale: 1.12, duration: 18 }
  },
  {
    id: "cyan",
    gradient: "radial-gradient(circle at 50% 50%, #06b6d4 0%, transparent 70%)",
    top: "18%",
    left: "48%",
    size: "52vw",
    drift: { x: -110, y: 80, scale: 1.08, duration: 22 }
  },
  {
    id: "blue",
    gradient: "radial-gradient(circle at 50% 50%, #3b82f6 0%, transparent 70%)",
    top: "52%",
    left: "6%",
    size: "46vw",
    drift: { x: 70, y: -90, scale: 1.16, duration: 26 }
  }
];

const DOT_GRID_STYLE = {
  position: "absolute",
  inset: 0,
  backgroundImage: "radial-gradient(rgba(255, 255, 255, 0.07) 1px, transparent 1px)",
  backgroundSize: "28px 28px",
  maskImage: "radial-gradient(ellipse at center, black 45%, transparent 85%)",
  WebkitMaskImage: "radial-gradient(ellipse at center, black 45%, transparent 85%)"
} as const;

function shouldAnimate(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    !window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

export function AmbientBackground(): ReactNode {
  const halosRef = useRef<Array<HTMLDivElement | null>>([]);

  useEffect(() => {
    if (!shouldAnimate()) {
      return;
    }

    const halos = halosRef.current.filter((halo): halo is HTMLDivElement => halo !== null);
    if (halos.length === 0) {
      return;
    }

    const context = gsap.context(() => {
      halos.forEach((halo, index) => {
        const config = AURORA_HALOS[index];
        if (config === undefined) {
          return;
        }
        gsap.to(halo, {
          x: config.drift.x,
          y: config.drift.y,
          scale: config.drift.scale,
          duration: config.drift.duration,
          delay: index * 1.4,
          repeat: -1,
          yoyo: true,
          ease: "sine.inOut"
        });
      });
    });

    return () => {
      context.revert();
    };
  }, []);

  return (
    <div
      aria-hidden="true"
      data-testid="safi-ambient-background"
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 0,
        overflow: "hidden",
        pointerEvents: "none"
      }}
    >
      {AURORA_HALOS.map((halo, index) => (
        <div
          key={halo.id}
          ref={(element) => {
            halosRef.current[index] = element;
          }}
          style={{
            position: "absolute",
            top: halo.top,
            left: halo.left,
            width: halo.size,
            height: halo.size,
            background: halo.gradient,
            borderRadius: "50%",
            filter: "blur(130px)",
            opacity: 0.16,
            willChange: "transform"
          }}
        />
      ))}
      <div style={DOT_GRID_STYLE} />
    </div>
  );
}
