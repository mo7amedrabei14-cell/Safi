/**
 * Reusable VerifiedNumberChip & Provenance Audit Modal (`Phase 1.2`).
 *
 * Displays an exact metric value + unit + verification status from Rust, distinguishes
 * Verified / Stale / Unverified / Error states, and opens a full navigable audit modal
 * with formula, dataset version, paginated source row identities, transformation history,
 * BLAKE3 fingerprint, and engine version.
 */
import type { ReactNode } from "react";
import { useEffect, useState } from "react";
import { AlertTriangle, CheckCircle2, Clock3, ShieldAlert, XCircle } from "lucide-react";
import type { SafiLang } from "../../components/layout/FloatingGlassDock";
import type { DesktopMetric, MetricVerificationStatus } from "../studio-bridge";

const ROW_REF_PAGE_SIZE = 25;

export interface VerifiedNumberChipProps {
  readonly metric: DesktopMetric | null;
  readonly lang?: SafiLang;
  readonly recalculating?: boolean;
  readonly errorCode?: string | null;
  readonly fallbackLabel?: string;
  readonly onInspect?: (metric: DesktopMetric) => void;
}

export function resolveChipStatus(
  metric: DesktopMetric | null,
  recalculating: boolean,
  errorCode: string | null | undefined
): MetricVerificationStatus {
  if (errorCode !== undefined && errorCode !== null && errorCode.length > 0) {
    return "Error";
  }
  if (metric === null) {
    return "Unverified";
  }
  if (recalculating || metric.status === "Stale") {
    return "Stale";
  }
  if (metric.status === "Verified") {
    return "Verified";
  }
  return "Unverified";
}

export function VerifiedNumberChip({
  metric,
  lang = "en",
  recalculating = false,
  errorCode = null,
  fallbackLabel,
  onInspect
}: VerifiedNumberChipProps): ReactNode {
  const L = (ar: string, en: string): string => (lang === "ar" ? ar : en);
  const [modalOpen, setModalOpen] = useState(false);

  const status = resolveChipStatus(metric, recalculating, errorCode);
  const statusLabel: Record<MetricVerificationStatus, string> = {
    Verified: L("موثق", "Verified"),
    Stale: L("قديم — جاري التحديث", "Stale"),
    Unverified: L("غير موثق", "Unverified"),
    Error: errorCode !== null ? `${L("خطأ", "Error")} (${errorCode})` : L("خطأ", "Error")
  };

  const statusIcon = (): ReactNode => {
    switch (status) {
      case "Verified":
        return <CheckCircle2 size={14} aria-hidden="true" />;
      case "Stale":
        return <Clock3 size={14} aria-hidden="true" />;
      case "Error":
        return <XCircle size={14} aria-hidden="true" />;
      case "Unverified":
        return <AlertTriangle size={14} aria-hidden="true" />;
    }
  };

  const title = metric?.name ?? fallbackLabel ?? L("مؤشر", "Metric");
  const displayValue =
    status === "Error"
      ? (errorCode ?? L("خطأ", "Error"))
      : metric !== null
        ? `${metric.value}${metric.unit !== null && metric.unit.length > 0 ? ` ${metric.unit}` : ""}`
        : "—";

  const handleClick = (): void => {
    if (metric === null) {
      return;
    }
    if (onInspect !== undefined) {
      onInspect(metric);
    } else {
      setModalOpen(true);
    }
  };

  return (
    <>
      <button
        type="button"
        className={`safi-desktop-chip safi-desktop-chip--${status.toLowerCase()}`}
        data-status={status}
        onClick={handleClick}
        disabled={metric === null}
        aria-label={`${title}: ${displayValue} (${statusLabel[status]})`}
      >
        <span className="safi-desktop-chip__name">{title}</span>
        <span className="safi-desktop-chip__value">{displayValue}</span>
        <span className="safi-desktop-chip__state">
          {statusIcon()}
          {statusLabel[status]}
        </span>
      </button>
      {modalOpen && metric !== null ? (
        <VerifiedAuditModal
          metric={metric}
          status={status}
          errorCode={errorCode}
          lang={lang}
          onClose={() => setModalOpen(false)}
        />
      ) : null}
    </>
  );
}

export interface VerifiedAuditModalProps {
  readonly metric: DesktopMetric;
  readonly status: MetricVerificationStatus;
  readonly errorCode?: string | null;
  readonly lang?: SafiLang;
  readonly onClose: () => void;
}

export function VerifiedAuditModal({
  metric,
  status,
  errorCode = null,
  lang = "en",
  onClose
}: VerifiedAuditModalProps): ReactNode {
  const L = (ar: string, en: string): string => (lang === "ar" ? ar : en);
  const [rowPage, setRowPage] = useState(0);

  const rowIds = metric.sourceRowIds.length > 0 ? metric.sourceRowIds : metric.rows;
  const totalPages = Math.max(1, Math.ceil(rowIds.length / ROW_REF_PAGE_SIZE));
  const clampedPage = Math.min(rowPage, totalPages - 1);
  const sliceStart = clampedPage * ROW_REF_PAGE_SIZE;
  const visibleRowIds = rowIds.slice(sliceStart, sliceStart + ROW_REF_PAGE_SIZE);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key === "Escape") {
        onClose();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  const statusNote = (): string => {
    switch (status) {
      case "Verified":
        return L(
          "نتيجة موثقة ومطابقة لأحدث إصدار من البيانات والمعادلة.",
          "Verified result matching the current dataset version and formula."
        );
      case "Stale":
        return L(
          "تنبيه: هذه القيمة قديمة وتنتظر إعادة الحساب أو تغيرت المدخلات.",
          "Warning: this value is stale because inputs changed or recalculation is in progress."
        );
      case "Error":
        return L(
          `فشل الحساب بكود ${errorCode ?? "SF-CAL"}. القيمة السابقة لم تعد موثقة.`,
          `Calculation failed (${errorCode ?? "SF-CAL"}). Previous values are invalidated.`
        );
      case "Unverified":
        return L(
          "هذه النتيجة غير موثقة ولا يجب الاعتماد عليها حتى يكتمل التحقق.",
          "This result is unverified and must not be trusted until recomputed."
        );
    }
  };

  return (
    <div
      className="safi-desktop-studio__audit"
      role="dialog"
      aria-modal="true"
      aria-label={metric.name}
    >
      <div className="safi-desktop-studio__audit-card">
        <div className="safi-desktop-studio__audit-head">
          <h3>{metric.name}</h3>
          <span
            className={`safi-desktop-chip__state safi-desktop-chip__badge--${status.toLowerCase()}`}
          >
            <ShieldAlert size={14} aria-hidden="true" />
            {status}
          </span>
        </div>
        <p className="safi-desktop-studio__audit-value">
          {metric.value} {metric.unit ?? ""}
        </p>
        <p
          className={`safi-desktop-studio__status-note safi-desktop-studio__status-note--${status.toLowerCase()}`}
        >
          {statusNote()}
        </p>
        <dl>
          <div>
            <dt>{L("معرّف المؤشر", "Metric ID")}</dt>
            <dd>{metric.id}</dd>
          </div>
          <div>
            <dt>{L("المعادلة", "Formula")}</dt>
            <dd>{`${metric.formula} (v${metric.formulaVersion})`}</dd>
          </div>
          <div>
            <dt>{L("المصدر والنسخة", "Source & dataset version")}</dt>
            <dd>{`${metric.sourceId} · v${metric.sourceVersion}`}</dd>
          </div>
          <div>
            <dt>{L("بصمة البيانات الأصلية", "Source dataset fingerprint")}</dt>
            <dd className="safi-desktop-studio__fingerprint">{metric.sourceFingerprint}</dd>
          </div>
          <div>
            <dt>{L("الأعمدة المصدرية", "Source columns")}</dt>
            <dd>
              {metric.columns
                .map((col, idx) =>
                  metric.columnIds[idx] !== undefined ? `${col} (c${metric.columnIds[idx]})` : col
                )
                .join(", ")}
            </dd>
          </div>
          <div>
            <dt>
              {L("معرّفات الصفوف المصدرية", "Source row identities")} ({rowIds.length})
            </dt>
            <dd>
              {rowIds.length === 0 ? (
                L("لا توجد صفوف", "None")
              ) : (
                <div className="safi-desktop-studio__rowrefs">
                  <div className="safi-desktop-studio__rowrefs-list">
                    {visibleRowIds.map((rowId) => `r${rowId}`).join(", ")}
                  </div>
                  {totalPages > 1 ? (
                    <div className="safi-desktop-studio__rowrefs-pager">
                      <button
                        type="button"
                        onClick={() => setRowPage((prev) => Math.max(0, prev - 1))}
                        disabled={clampedPage === 0}
                      >
                        {L("السابق", "Prev")}
                      </button>
                      <span>{`${clampedPage + 1} / ${totalPages}`}</span>
                      <button
                        type="button"
                        onClick={() => setRowPage((prev) => Math.min(totalPages - 1, prev + 1))}
                        disabled={clampedPage >= totalPages - 1}
                      >
                        {L("التالي", "Next")}
                      </button>
                    </div>
                  ) : null}
                </div>
              )}
            </dd>
          </div>
          <div>
            <dt>
              {L("سجل التحويلات", "Transformation history")} ({metric.transformationTrace.length})
            </dt>
            <dd className="safi-desktop-studio__fingerprint">
              {metric.transformationTrace.length === 0
                ? L("البيانات الأصلية دون تعديل", "Unmodified source")
                : metric.transformationTrace.join(" -> ")}
            </dd>
          </div>
          <div>
            <dt>{L("بصمة BLAKE3", "BLAKE3 fingerprint")}</dt>
            <dd className="safi-desktop-studio__fingerprint">{metric.fingerprint}</dd>
          </div>
          <div>
            <dt>{L("محرك الحساب", "Engine version")}</dt>
            <dd>{metric.engineVersion}</dd>
          </div>
        </dl>
        <button type="button" onClick={onClose}>
          {L("إغلاق", "Close")}
        </button>
      </div>
    </div>
  );
}
