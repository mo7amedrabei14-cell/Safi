import React from "react";
import type { DesktopMetric } from "../studio-bridge";
import { VerifiedNumberChip } from "./VerifiedNumberChip";

export interface NarrativeTextRendererProps {
  readonly template: string;
  readonly metricsById: ReadonlyMap<string, DesktopMetric>;
  readonly language: "en" | "ar";
  readonly onInspectMetric?: (metric: DesktopMetric) => void;
  readonly onUnlinkMetric?: (metricId: string, frozenText: string) => void;
}

const TOKEN_RE = /(\{\{metric:([^}]+)\}\})/g;

/**
 * Renders user-written prose mixed cleanly with live `{{metric:metric_id}}` Rust tokens.
 * - Unresolved tokens show explicit missing-reference warning badges (and export blocks them).
 * - Resolved tokens render live VerifiedNumberChip badges backed by Rust tokens.
 * - Optional unlink button converts a live `{{metric:id}}` placeholder into frozen plain text.
 */
export function NarrativeTextRenderer({
  template,
  metricsById,
  language,
  onInspectMetric,
  onUnlinkMetric,
}: NarrativeTextRendererProps): React.ReactElement {
  const isAr = language === "ar";
  const parts: React.ReactNode[] = [];
  let cursor = 0;
  let matchIndex = 0;

  for (const match of template.matchAll(TOKEN_RE)) {
    const start = match.index ?? 0;
    const rawPlaceholder = match[1] ?? "";
    const metricId = (match[2] ?? "").trim();

    if (start > cursor) {
      parts.push(<span key={`txt-${matchIndex}`}>{template.slice(cursor, start)}</span>);
    }

    const token = metricsById.get(metricId);
    if (token) {
      const frozenText = token.unit ? `${token.value} ${token.unit}` : token.value;
      parts.push(
        <span key={`ref-${matchIndex}`} className="studio-inline-metric-ref">
          <VerifiedNumberChip
            metric={token}
            lang={language}
            {...(onInspectMetric !== undefined ? { onInspect: onInspectMetric } : {})}
          />
          {onUnlinkMetric ? (
            <button
              type="button"
              className="studio-unlink-btn"
              title={
                isAr
                  ? "فك الربط وتحويله لنص ثابت غير موثّق"
                  : "Unlink to plain static text"
              }
              onClick={() => onUnlinkMetric(metricId, frozenText)}
            >
              {isAr ? "فك الربط" : "Unlink"}
            </button>
          ) : null}
        </span>
      );
    } else {
      parts.push(
        <span
          key={`miss-${matchIndex}`}
          className="studio-missing-metric-ref"
          title={
            isAr
              ? `مرجع غير موجود (${rawPlaceholder}) — سيُمنع التصدير حتى الإصلاح`
              : `Missing metric reference (${rawPlaceholder}) — export blocked`
          }
        >
          {`Missing: ${metricId || rawPlaceholder}`}
        </span>
      );
    }

    cursor = start + rawPlaceholder.length;
    matchIndex += 1;
  }

  if (cursor < template.length) {
    parts.push(<span key="txt-tail">{template.slice(cursor)}</span>);
  }

  return <div className="studio-narrative-rendered">{parts}</div>;
}

export function listMissingMetricRefs(
  template: string,
  metricsById: ReadonlyMap<string, DesktopMetric>
): string[] {
  const missing: string[] = [];
  for (const match of template.matchAll(TOKEN_RE)) {
    const metricId = (match[2] ?? "").trim();
    if (!metricId || !metricsById.has(metricId)) {
      if (metricId && !missing.includes(metricId)) {
        missing.push(metricId);
      }
    }
  }
  return missing;
}
