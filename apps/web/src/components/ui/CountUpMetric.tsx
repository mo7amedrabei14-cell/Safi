/**
 * `apps/web/src/components/ui/CountUpMetric.tsx`
 *
 * React Bits "count up" for KPI numbers. GSAP tweens a plain object and writes the
 * formatted value into the element (`textContent`), so the count never re-renders
 * React and stays on `tabular-nums` without glyph jitter.
 *
 * The server renders the final value, which keeps the metric readable with JS
 * disabled and correct for assistive technology.
 */
import type { ReactNode } from "react";
import { createElement, useEffect, useRef } from "react";
import { gsap } from "gsap";
import { canAnimate } from "./motion-preference";

export interface CountUpMetricProps {
  /** Target value. */
  readonly value: number;
  /** Unit that follows the number, e.g. `%`, `صف/ث`, `ms`. */
  readonly suffix?: string;
  readonly prefix?: string;
  /** Fraction digits to render. */
  readonly decimals?: number;
  /** Animation duration in seconds. */
  readonly duration?: number;
  readonly className?: string;
  /** Optional caption rendered under the number. */
  readonly label?: string;
}

export function formatMetricValue(value: number, decimals = 0): string {
  return value.toLocaleString("en-US", {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals
  });
}

export function CountUpMetric({
  value,
  suffix,
  prefix,
  decimals = 0,
  duration = 1.6,
  className,
  label
}: CountUpMetricProps): ReactNode {
  const numberRef = useRef<HTMLSpanElement | null>(null);
  const finalText = formatMetricValue(value, decimals);

  useEffect(() => {
    const element = numberRef.current;
    if (element === null || !canAnimate()) {
      if (element !== null) {
        element.textContent = finalText;
      }
      return;
    }

    const counter = { current: 0 };
    const tween = gsap.to(counter, {
      current: value,
      duration,
      ease: "power2.out",
      onUpdate: () => {
        element.textContent = formatMetricValue(counter.current, decimals);
      },
      onComplete: () => {
        element.textContent = finalText;
      }
    });

    return () => {
      tween.kill();
      element.textContent = finalText;
    };
  }, [decimals, duration, finalText, value]);

  return createElement(
    "span",
    {
      className,
      "data-safi-count-up": "true",
      style: {
        display: "inline-flex",
        flexDirection: "column" as const,
        alignItems: "flex-start",
        gap: "2px",
        fontFamily: "var(--font-arabic)"
      }
    },
    [
      createElement(
        "span",
        {
          key: "value",
          style: {
            display: "inline-flex",
            alignItems: "baseline",
            gap: "6px",
            fontSize: "32px",
            fontWeight: 700,
            lineHeight: 1.1,
            fontVariantNumeric: "tabular-nums",
            color: "var(--text-primary)"
          }
        },
        [
          prefix === undefined
            ? null
            : createElement(
                "span",
                { key: "prefix", style: { fontSize: "18px", opacity: 0.8 } },
                prefix
              ),
          createElement(
            "span",
            { key: "number", ref: numberRef, "data-metric-value": String(value) },
            finalText
          ),
          suffix === undefined
            ? null
            : createElement(
                "span",
                {
                  key: "suffix",
                  style: { fontSize: "14px", fontWeight: 500, color: "var(--accent-cyan)" }
                },
                suffix
              )
        ]
      ),
      label === undefined
        ? null
        : createElement(
            "span",
            { key: "label", style: { fontSize: "12px", color: "var(--text-muted)" } },
            label
          )
    ]
  );
}
