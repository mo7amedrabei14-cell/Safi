import React, { useState } from "react";
import type { AnomalyFixPayload, DesktopAnomaly } from "../studio-bridge";

export interface AnomalyPanelProps {
  readonly language: "en" | "ar";
  readonly anomalies: ReadonlyArray<DesktopAnomaly>;
  readonly isMutating: boolean;
  readonly onApplyFix: (fix: AnomalyFixPayload) => Promise<void>;
  readonly onFocusRowId?: (rowId: number) => void;
}

const KIND_LABELS: Record<
  string,
  {
    en: string;
    ar: string;
    severityEn: string;
    severityAr: string;
    reasonEn: string;
    reasonAr: string;
  }
> = {
  MissingValue: {
    en: "Missing values",
    ar: "قيم مفقودة",
    severityEn: "Medium",
    severityAr: "متوسطة",
    reasonEn: "Empty or recognized null sentinels found in column cells.",
    reasonAr: "تم رصد خلايا فارغة أو قيم تدل على الفراغ داخل العمود."
  },
  DuplicateRow: {
    en: "Duplicate rows",
    ar: "صفوف مكررة",
    severityEn: "High",
    severityAr: "عالية",
    reasonEn: "Rows share an identical structural key across all columns.",
    reasonAr: "صفوف متطابقة تمامًا عبر جميع الأعمدة."
  },
  InconsistentType: {
    en: "Mixed value types",
    ar: "أنواع بيانات مختلطة",
    severityEn: "Medium",
    severityAr: "متوسطة",
    reasonEn: "Column mixes numeric values with non-numeric text entries.",
    reasonAr: "العمود يحتوي على قيم رقمية ونصوص غير رقمية معًا."
  },
  ImpossibleDate: {
    en: "Impossible calendar dates",
    ar: "تواريخ غير صالحة تقويميًا",
    severityEn: "High",
    severityAr: "عالية",
    reasonEn: "ISO date string fails real Gregorian month/leap-year day bounds.",
    reasonAr: "صيغة التاريخ تتجاوز أيام الشهر الفعلية أو قواعد السنة الكبيسة."
  },
  StatisticalOutlier: {
    en: "Statistical outliers (1.5 × IQR)",
    ar: "قيم شاذة إحصائيًا (1.5 × IQR)",
    severityEn: "Low (Review)",
    severityAr: "منخفضة (للمراجعة)",
    reasonEn: "Exact rust_decimal quantile detection outside 1.5 × interquartile range.",
    reasonAr: "رصد دقيق عبر rust_decimal خارج نطاق 1.5 × المدى الربيعي."
  }
};

export function AnomalyPanel({
  language,
  anomalies,
  isMutating,
  onApplyFix,
  onFocusRowId
}: AnomalyPanelProps): React.ReactElement {
  const isAr = language === "ar";
  const [fillValues, setFillValues] = useState<Record<string, string>>({});
  const [pendingKey, setPendingKey] = useState<string | null>(null);
  const [errorText, setErrorText] = useState<string | null>(null);

  if (anomalies.length === 0) {
    return (
      <section className="studio-panel studio-anomaly-panel" data-testid="anomaly-panel">
        <div className="studio-panel-header">
          <h3>{isAr ? "كاشف الشذوذ والجودة" : "Anomaly & Quality Inspector"}</h3>
        </div>
        <p className="studio-muted-note">
          {isAr
            ? "لم تُكتشف أي حالات شذوذ أو قيم مفقودة أو تكرارات في جدول العمل الحالي."
            : "No anomalies, missing cells, invalid dates, or duplicate rows detected."}
        </p>
      </section>
    );
  }

  return (
    <section className="studio-panel studio-anomaly-panel" data-testid="anomaly-panel">
      <div className="studio-panel-header">
        <div>
          <h3>{isAr ? "كاشف الشذوذ التفاعلي" : "Interactive Anomaly Detector"}</h3>
          <p className="studio-muted-note">
            {isAr
              ? "لا يُطبّق أي إصلاح تلقائيًا؛ كل إجراء يتطلب تأكيدك ويمكن التراجع عنه بضغطة واحدة."
              : "No fix runs automatically; every remediation requires confirmation and is undoable in one step."}
          </p>
        </div>
        <span className="studio-badge-warn">
          {`${anomalies.length} ${isAr ? "تنبيه" : "issues"}`}
        </span>
      </div>

      {errorText ? (
        <div className="studio-alert-error" role="alert">
          {errorText}
        </div>
      ) : null}

      <ul className="studio-anomaly-list">
        {anomalies.map((item, idx) => {
          const itemKey = `${item.kind}-${item.column ?? "table"}-${idx}`;
          const meta = KIND_LABELS[item.kind] ?? {
            en: item.kind,
            ar: item.kind,
            severityEn: "Info",
            severityAr: "تنبيه",
            reasonEn: item.methodKey,
            reasonAr: item.methodKey
          };
          const rowIds = item.rowIds ?? [];
          const fillVal = fillValues[itemKey] ?? "0";
          const canApplyFill =
            item.kind === "MissingValue" && Boolean(item.column) && rowIds.length > 0;
          const canBlankDates =
            item.kind === "ImpossibleDate" && Boolean(item.column) && rowIds.length > 0;
          const canDropDups = item.kind === "DuplicateRow" && rowIds.length > 0;

          return (
            <li key={itemKey} className="studio-anomaly-card">
              <div className="studio-anomaly-top">
                <strong>{isAr ? meta.ar : meta.en}</strong>
                <span className="studio-badge-muted">
                  {`${isAr ? "الخطورة" : "Severity"}: ${isAr ? meta.severityAr : meta.severityEn}`}
                </span>
                {item.column ? (
                  <span className="studio-badge-col">
                    {`${isAr ? "العمود" : "Column"}: ${item.column}`}
                  </span>
                ) : null}
              </div>

              <div className="studio-anomaly-why">
                <span>{isAr ? meta.reasonAr : meta.reasonEn}</span>
                <code className="studio-method-key">{item.methodKey}</code>
              </div>

              <div className="studio-anomaly-rows">
                <span>{isAr ? "الصفوف المتأثرة (معرّفات ثابتة):" : "Affected RowIds:"}</span>
                {rowIds.slice(0, 12).map((rowId) => (
                  <button
                    key={rowId}
                    type="button"
                    className="studio-row-chip"
                    onClick={() => onFocusRowId?.(rowId)}
                  >
                    {`#${rowId}`}
                  </button>
                ))}
                {rowIds.length > 12 ? (
                  <span className="studio-muted-note">{`+${rowIds.length - 12}`}</span>
                ) : null}
              </div>

              <div className="studio-anomaly-actions">
                {canApplyFill && item.column ? (
                  <div className="studio-anomaly-fix-row">
                    <input
                      type="text"
                      value={fillVal}
                      disabled={isMutating}
                      onChange={(e) =>
                        setFillValues((prev) => ({ ...prev, [itemKey]: e.target.value }))
                      }
                      aria-label={isAr ? "قيمة التعويض" : "Replacement value"}
                    />
                    {pendingKey === itemKey ? (
                      <>
                        <button
                          type="button"
                          className="studio-btn-primary"
                          disabled={isMutating}
                          onClick={() => {
                            setErrorText(null);
                            void onApplyFix({
                              action: "fill_missing",
                              column: item.column as string,
                              row_ids: rowIds,
                              replacement: fillVal
                            })
                              .then(() => setPendingKey(null))
                              .catch((e) =>
                                setErrorText(e instanceof Error ? e.message : String(e))
                              );
                          }}
                        >
                          {isAr ? "تأكيد ملء الخلايا الفارغة" : "Confirm Fill Missing"}
                        </button>
                        <button
                          type="button"
                          className="studio-btn-secondary"
                          onClick={() => setPendingKey(null)}
                        >
                          {isAr ? "إلغاء" : "Cancel"}
                        </button>
                      </>
                    ) : (
                      <button
                        type="button"
                        className="studio-btn-secondary"
                        disabled={isMutating}
                        onClick={() => setPendingKey(itemKey)}
                      >
                        {isAr
                          ? `ملء ${rowIds.length} خلية مفقودة...`
                          : `Fix ${rowIds.length} missing cells...`}
                      </button>
                    )}
                  </div>
                ) : null}

                {canBlankDates && item.column ? (
                  pendingKey === itemKey ? (
                    <div className="studio-anomaly-fix-row">
                      <button
                        type="button"
                        className="studio-btn-primary"
                        disabled={isMutating}
                        onClick={() => {
                          setErrorText(null);
                          void onApplyFix({
                            action: "blank_invalid_dates",
                            column: item.column as string,
                            row_ids: rowIds
                          })
                            .then(() => setPendingKey(null))
                            .catch((e) => setErrorText(e instanceof Error ? e.message : String(e)));
                        }}
                      >
                        {isAr ? "تأكيد تفريغ التواريخ المستحيلة" : "Confirm Clear Invalid Dates"}
                      </button>
                      <button
                        type="button"
                        className="studio-btn-secondary"
                        onClick={() => setPendingKey(null)}
                      >
                        {isAr ? "إلغاء" : "Cancel"}
                      </button>
                    </div>
                  ) : (
                    <button
                      type="button"
                      className="studio-btn-secondary"
                      disabled={isMutating}
                      onClick={() => setPendingKey(itemKey)}
                    >
                      {isAr
                        ? `تفريغ ${rowIds.length} تاريخ غير صالح...`
                        : `Clear ${rowIds.length} invalid dates...`}
                    </button>
                  )
                ) : null}

                {canDropDups ? (
                  pendingKey === itemKey ? (
                    <div className="studio-anomaly-fix-row">
                      <button
                        type="button"
                        className="studio-btn-primary"
                        disabled={isMutating}
                        onClick={() => {
                          setErrorText(null);
                          void onApplyFix({
                            action: "drop_duplicate_rows",
                            row_ids: rowIds
                          })
                            .then(() => setPendingKey(null))
                            .catch((e) => setErrorText(e instanceof Error ? e.message : String(e)));
                        }}
                      >
                        {isAr ? "تأكيد حذف الصفوف المكررة" : "Confirm Drop Duplicates"}
                      </button>
                      <button
                        type="button"
                        className="studio-btn-secondary"
                        onClick={() => setPendingKey(null)}
                      >
                        {isAr ? "إلغاء" : "Cancel"}
                      </button>
                    </div>
                  ) : (
                    <button
                      type="button"
                      className="studio-btn-secondary"
                      disabled={isMutating}
                      onClick={() => setPendingKey(itemKey)}
                    >
                      {isAr
                        ? `حذف ${rowIds.length} صف مكرر...`
                        : `Drop ${rowIds.length} duplicate rows...`}
                    </button>
                  )
                ) : null}

                {item.kind === "InconsistentType" || item.kind === "StatisticalOutlier" ? (
                  <span className="studio-muted-note">
                    {isAr
                      ? "راجع الصفوف المعروضة أعلاه وعدّلها يدويًا من الجدول عند الحاجة."
                      : "Inspect flagged RowIds above and edit cells manually in Sheet if needed."}
                  </span>
                ) : null}
              </div>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
