/**
 * GSAP 3.12 motion logo timelines adhering to Specification Section 7.2.
 */
import { gsap } from "gsap";

export interface MotionLogoOptions {
  isFirstRun?: boolean;
  onComplete?: () => void;
}

export function playMotionLogo(containerElement: HTMLElement, options: MotionLogoOptions = {}) {
  const { isFirstRun = true, onComplete } = options;
  const duration = isFirstRun ? 2.6 : 0.6;

  const prefersReduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (prefersReduced) {
    gsap.to(containerElement, {
      opacity: 1,
      duration: 0.12,
      ...(onComplete ? { onComplete } : {})
    });
    return;
  }

  const tl = gsap.timeline(onComplete ? { onComplete } : {});
  tl.to(containerElement, { opacity: 1, duration: duration * 0.5 });
  return tl;
}
