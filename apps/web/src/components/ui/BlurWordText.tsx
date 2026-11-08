/**
 * `apps/web/src/components/ui/BlurWordText.tsx`
 *
 * React Bits "blur text" entrance, adapted for Arabic:
 * the text is split into WORDS only — never into characters — because separating
 * Arabic letters destroys the joined glyph shapes.
 *
 * Each word is animatable on its own layer (`transform` + `filter` only), so the
 * heading never triggers layout while it reveals.
 */
import type { ReactNode } from "react";
import { createElement, useEffect, useRef } from "react";
import { gsap } from "gsap";
import { canAnimate } from "./motion-preference";

export type BlurWordTextTag = "h1" | "h2" | "h3" | "p" | "span";

export interface BlurWordTextProps {
  /** Full sentence. Split on spaces; Arabic letters stay joined inside a word. */
  readonly text: string;
  readonly as?: BlurWordTextTag;
  readonly className?: string;
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
  stagger = 0.04,
  delay = 0,
  duration = 0.6,
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

    const tween = gsap.from(targets, {
      opacity: 0,
      y: 14,
      filter: "blur(10px)",
      duration,
      delay,
      stagger,
      ease: "power3.out",
      clearProps: "filter"
    });

    return () => {
      tween.kill();
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
    { ref: containerRef, className, "aria-label": ariaLabel, "data-safi-blur-text": "true" },
    children
  );
}
