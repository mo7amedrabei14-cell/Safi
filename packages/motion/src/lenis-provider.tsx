/**
 * Lenis Smooth Scroll provider for SafiData v5.0 (Specification Section 20.1).
 */
import React, { useEffect } from "react";
import Lenis from "lenis";

export interface LenisProviderProps {
  children: React.ReactNode;
  enabled?: boolean;
}

export const LenisProvider: React.FC<LenisProviderProps> = ({ children, enabled = true }) => {
  useEffect(() => {
    if (!enabled) return;
    const lenis = new Lenis({
      duration: 1.2,
      easing: (t) => Math.min(1, 1.001 - Math.pow(2, -10 * t)),
      smoothWheel: true
    });

    function raf(time: number) {
      lenis.raf(time);
      requestAnimationFrame(raf);
    }

    requestAnimationFrame(raf);

    return () => {
      lenis.destroy();
    };
  }, [enabled]);

  return React.createElement(React.Fragment, null, children);
};
