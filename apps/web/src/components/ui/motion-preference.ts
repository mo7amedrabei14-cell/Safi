/**
 * Shared motion guard for the UI kit.
 *
 * Every animated component consults this before starting a GSAP timeline so that
 * `prefers-reduced-motion: reduce` users get the final, static state (WCAG 2.2 AA).
 * Outside a browser (SSR / Vitest) it always reports "reduced".
 */
export function prefersReducedMotion(): boolean {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
    return true;
  }
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

export function canAnimate(): boolean {
  return (
    typeof window !== "undefined" && typeof document !== "undefined" && !prefersReducedMotion()
  );
}
