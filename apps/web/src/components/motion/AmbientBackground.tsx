/**
 * `apps/web/src/components/motion/AmbientBackground.tsx`
 *
 * Subtle Ambient Mesh locked to the 4-Color Palette:
 * `#F9F7F7`, `#DBE2EF`, `#3F72AF`, `#112D4E`.
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
    id: "safi-mist",
    gradient: "radial-gradient(circle at 50% 50%, #DBE2EF 0%, transparent 70%)",
    top: "-18%",
    left: "-10%",
    size: "52vw",
    drift: { x: 80, y: 50, scale: 1.08, duration: 20 }
  },
  {
    id: "safi-steel",
    gradient: "radial-gradient(circle at 50% 50%, #3F72AF 0%, transparent 70%)",
    top: "12%",
    left: "48%",
    size: "48vw",
    drift: { x: -90, y: 65, scale: 1.06, duration: 24 }
  },
  {
    id: "safi-ink",
    gradient: "radial-gradient(circle at 50% 50%, #112D4E 0%, transparent 70%)",
    top: "54%",
    left: "8%",
    size: "42vw",
    drift: { x: 60, y: -70, scale: 1.1, duration: 28 }
  }
];

const DOT_GRID_STYLE = {
  position: "absolute",
  inset: 0,
  backgroundImage: "radial-gradient(rgba(17, 45, 78, 0.08) 1px, transparent 1px)",
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
            opacity: 0.1,
            willChange: "transform"
          }}
        />
      ))}
      <div style={DOT_GRID_STYLE} />
    </div>
  );
}
