/**
 * `apps/web/src/components/ui/BlurWordText.tsx`
 *
 * React Bits "blur text" entrance, adapted for Arabic:
 * the text is split into WORDS only — never into characters — because separating
 * Arabic letters destroys the joined glyph shapes.
 *
 * Uses `gsap.fromTo` + `clearProps` on cleanup so React 19 `<StrictMode>`
 * double-invocation never leaves words stuck at `opacity: 0`.
 */
import type { CSSProperties, ReactNode } from "react";
import { createElement, useEffect, useRef } from "react";
import { gsap } from "gsap";
import { canAnimate } from "./motion-preference";

export type BlurWordTextTag = "h1" | "h2" | "h3" | "p" | "span";

export interface BlurWordTextProps {
  /** Full sentence. Split on spaces; Arabic letters stay joined inside a word. */
  readonly text: string;
  readonly as?: BlurWordTextTag;
  readonly className?: string;
  readonly style?: CSSProperties;
  /** Seconds between words. */
  readonly stagger?: number;
  readonly delay?: number;
  /** Per-word reveal duration in seconds. */
  readonly duration?: number;
  /**
   * Accessible name for the whole sentence. Word-splitting must not make a screen
   * reader announce the heading word by word, so the full text is exposed here.
   */
  readonly ariaLabel?: string;
}

/** Exported for tests: word boundaries must never fall inside an Arabic word. */
export function splitIntoWords(text: string): string[] {
  return text
    .split(" ")
    .map((word) => word.trim())
    .filter((word) => word.length > 0);
}

export function BlurWordText({
  text,
  as = "p",
  className,
  style,
  stagger = 0.04,
  delay = 0,
  duration = 0.55,
  ariaLabel
}: BlurWordTextProps): ReactNode {
  const containerRef = useRef<HTMLElement | null>(null);
  const words = splitIntoWords(text);

  useEffect(() => {
    const container = containerRef.current;
    if (container === null || !canAnimate()) {
      return;
    }

    const targets = Array.from(container.querySelectorAll<HTMLSpanElement>("[data-safi-word]"));
    if (targets.length === 0) {
      return;
    }

    const tween = gsap.fromTo(
      targets,
      { opacity: 0.2, y: 10, filter: "blur(6px)" },
      {
        opacity: 1,
        y: 0,
        filter: "blur(0px)",
        duration,
        delay,
        stagger,
        ease: "power3.out",
        clearProps: "filter,transform,opacity"
      }
    );

    return () => {
      tween.kill();
      gsap.set(targets, { clearProps: "filter,transform,opacity" });
    };
  }, [delay, duration, stagger, text]);

  const children = words.map((word, index) => (
    <span key={`${word}-${index}`} data-safi-word="true" style={{ display: "inline-block" }}>
      {word}
      {index < words.length - 1 ? "\u00a0" : ""}
    </span>
  ));

  return createElement(
    as,
    {
      ref: containerRef,
      className,
      style,
      "aria-label": ariaLabel,
      "data-safi-blur-text": "true"
    },
    children
  );
}
