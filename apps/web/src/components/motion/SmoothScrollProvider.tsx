/**
 * `apps/web/src/components/motion/SmoothScrollProvider.tsx`
 *
 * Binds Lenis (lerp 0.09) to the GSAP ticker so scrolling runs on the same frame
 * budget as the rest of the motion layer (single rAF loop, no double scheduling).
 *
 * Guard rails:
 * - server / jsdom / Vitest: render children untouched, never construct Lenis;
 * - `prefers-reduced-motion: reduce`: keep native scrolling (WCAG 2.2 AA).
 */
import type { ReactNode } from "react";
import { useEffect } from "react";
import { gsap } from "gsap";
import Lenis from "lenis";

export interface SmoothScrollProviderProps {
  readonly children: ReactNode;
  /** Scroll interpolation factor. Lower is silkier, higher is snappier. */
  readonly lerp?: number;
}

function isBrowserEnvironment(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof document !== "undefined" &&
    typeof window.requestAnimationFrame === "function" &&
    typeof window.matchMedia === "function"
  );
}

function prefersReducedMotion(): boolean {
  return isBrowserEnvironment() && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** Exported for tests: the provider must stay inert outside a real browser. */
export function shouldEnableSmoothScroll(): boolean {
  return isBrowserEnvironment() && !prefersReducedMotion();
}

export function SmoothScrollProvider({
  children,
  lerp = 0.09
}: SmoothScrollProviderProps): ReactNode {
  useEffect(() => {
    if (!shouldEnableSmoothScroll()) {
      return;
    }

    const lenis = new Lenis({ lerp, smoothWheel: true });

    const onTick = (time: number): void => {
      // GSAP reports elapsed time in seconds; Lenis expects milliseconds.
      lenis.raf(time * 1000);
    };

    gsap.ticker.add(onTick);
    gsap.ticker.lagSmoothing(0);

    return () => {
      gsap.ticker.remove(onTick);
      lenis.destroy();
    };
  }, [lerp]);

  return children;
}
