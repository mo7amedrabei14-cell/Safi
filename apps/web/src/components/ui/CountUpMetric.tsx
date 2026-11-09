/**
 * `apps/web/src/components/ui/CountUpMetric.tsx`
 *
 * KPI numbers with Apple Blue suffix and tabular-nums alignment.
 */
import type { ReactNode } from "react";
import { createElement, useEffect, useRef } from "react";
import { gsap } from "gsap";
import { canAnimate } from "./motion-preference";

export interface CountUpMetricProps {
  readonly value: number;
  readonly suffix?: string;
  readonly prefix?: string;
  readonly decimals?: number;
  readonly duration?: number;
  readonly className?: string;
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
            fontSize: "28px",
            fontWeight: 700,
            lineHeight: 1.15,
            fontVariantNumeric: "tabular-nums",
            color: "var(--text-primary)"
          }
        },
        [
          prefix === undefined
            ? null
            : createElement(
                "span",
                { key: "prefix", style: { fontSize: "16px", color: "var(--text-secondary)" } },
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
                  style: { fontSize: "13px", fontWeight: 600, color: "var(--accent-primary)" }
                },
                suffix
              )
        ]
      ),
      label === undefined
        ? null
        : createElement(
            "span",
            { key: "label", style: { fontSize: "12px", color: "var(--text-secondary)" } },
            label
          )
    ]
  );
}
