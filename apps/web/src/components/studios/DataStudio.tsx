/**
 * `apps/web/src/components/studios/DataStudio.tsx`
 *
 * Quad-Studio workspace (Excel / Power BI / PowerPoint / Word in one surface).
 *
 * Every number shown here comes from the Rust engine as a `VerifiedMetricToken`. Clicking a chip
 * opens its audit trail (formula, formula version, dataset version, contributing rows, BLAKE3
 * fingerprint). The browser never aggregates or recomputes anything: edits are forwarded to Rust
 * and the returned state snapshot is rendered as-is.
 */
import type { ReactNode } from "react";
import { useCallback, useRef, useState } from "react";
import {
  AlertTriangle,
  BarChart3,
  CheckCircle2,
  Download,
  Redo2,
  Save,
  ShieldCheck,
  Undo2,
  Upload,
  XCircle
} from "lucide-react";
import "../../studio/studio.css";
import type { SafiDockTabId, SafiLang } from "../layout/FloatingGlassDock";
import {
  importCsv,
  importExcel,
  openStudioSession,
  restoreStudioSession
} from "../../studio/engine";
import type {
  StudioGroup,
  StudioMetric,
  StudioSession,
  StudioState
} from "../../studio/engine";

const PROJECT_KEY = "safi.studio.project.v1";

export interface DataStudioProps {
  readonly lang?: SafiLang;
  readonly onSelectTab?: (tab: SafiDockTabId) => void;
}

type AggName = "SUM" | "AVG" | "COUNT" | "MIN" | "MAX";

export function DataStudio({ lang = "en" }: DataStudioProps): ReactNode {
  const L = useCallback(
    (ar: string, en: string): string => (lang === "ar" ? ar : en),
    [lang]
  );
  const sessionRef = useRef<StudioSession | null>(null);
  const [state, setState] = useState<StudioState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [audit, setAudit] = useState<StudioMetric | null>(null);
  const [metricId, setMetricId] = useState("total_amount");
  const [metricName, setMetricName] = useState("Total amount");
  const [metricUnit, setMetricUnit] = useState("EGP");
  const [metricFormula, setMetricFormula] = useState("SUM(amount)");
  const [calcName, setCalcName] = useState("line_total");
  const [calcFormula, setCalcFormula] = useState("amount * quantity");
  const [groupBy, setGroupBy] = useState("");
  const [valueColumn, setValueColumn] = useState("");
  const [aggregate, setAggregate] = useState<AggName>("SUM");
  const [groups, setGroups] = useState<readonly StudioGroup[]>([]);

  const run = useCallback(
    async (action: (session: StudioSession) => string): Promise<void> => {
      const session = sessionRef.current;
      if (session === null) {
        return;
      }
      setBusy(true);
      try {
        const raw = action(session);
        setState(JSON.parse(raw) as StudioState);
        setError(null);
      } catch (cause) {
        setError(String(cause));
      } finally {
        setBusy(false);
      }
    },
    []
  );

  const openFile = useCallback(
    async (file: File): Promise<void> => {
      setBusy(true);
      setError(null);
      try {
        const bytes = new Uint8Array(await file.arrayBuffer());
        const isSpreadsheet = /\.(xlsx|xlsm|ods|xls)$/i.test(file.name);
        const tableJson = isSpreadsheet ? await importExcel(bytes) : await importCsv(bytes);
        const session = await openStudioSession(tableJson);
        sessionRef.current = session;
        const snapshot = JSON.parse(session.state_json()) as StudioState;
        setState(snapshot);
        setGroups([]);
        setGroupBy(snapshot.columns[0]?.name ?? "");
        const numericGuess = snapshot.columns.find((column) => column.name !== snapshot.columns[0]?.name);
        setValueColumn(numericGuess?.name ?? snapshot.columns[0]?.name ?? "");
      } catch (cause) {
        setError(String(cause));
      } finally {
        setBusy(false);
      }
    },
    []
  );

  const loadProject = useCallback(async (): Promise<void> => {
    const saved = window.localStorage.getItem(PROJECT_KEY);
    if (saved === null) {
      setError(L("لا يوجد مشروع محفوظ على هذا الجهاز.", "No saved project on this device."));
      return;
    }
    setBusy(true);
    try {
      const session = await restoreStudioSession(saved);
      sessionRef.current = session;
      setState(JSON.parse(session.state_json()) as StudioState);
      setError(null);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  }, [L]);

  const saveProject = useCallback(async (): Promise<void> => {
    const session = sessionRef.current;
    if (session === null) {
      return;
    }
    window.localStorage.setItem(PROJECT_KEY, session.snapshot_json());
    setError(null);
  }, []);

  const exportCapsule = useCallback((): void => {
    const session = sessionRef.current;
    if (session === null) {
      return;
    }
    const charts =
      groupBy.length > 0 && valueColumn.length > 0
        ? [
            {
              title: `${aggregate} ${valueColumn} / ${groupBy}`,
              function: aggregate,
              column: valueColumn,
              group_by: groupBy
            }
          ]
        : [];
    const html = session.capsule(
      JSON.stringify({
        title: metricName.length > 0 ? metricName : "Safi capsule",
        created_at: new Date().toISOString(),
        charts
      })
    );
    const blob = new Blob([html], { type: "text/html;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = "safi-capsule.safi.html";
    anchor.click();
    URL.revokeObjectURL(url);
  }, [aggregate, groupBy, metricName, valueColumn]);

  const refreshGroups = useCallback(async (): Promise<void> => {
    const session = sessionRef.current;
    if (session === null || groupBy.length === 0 || valueColumn.length === 0) {
      return;
    }
    setBusy(true);
    try {
      const raw = session.grouped(aggregate, valueColumn, groupBy);
      setGroups(JSON.parse(raw) as StudioGroup[]);
      setError(null);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  }, [aggregate, groupBy, valueColumn]);

  const selectableColumns = state?.columns ?? [];

  return (
    <section className="safi-studio" aria-busy={busy}>
      <header className="safi-studio__head">
        <h1>{L("الاستوديو الرباعي", "Quad-Studio")}</h1>
        <p>
          {L(
            "كل رقم هنا محسوب في محرك Rust ومربوط ببصمة تحقق قابلة للفحص.",
            "Every number here is computed by the Rust engine and bound to a checkable fingerprint."
          )}
        </p>
      </header>

      <div className="safi-studio__toolbar">
        <label className="safi-studio__file">
          <Upload size={16} aria-hidden="true" />
          {L("استيراد ملف", "Import file")}
          <input
            type="file"
            accept=".csv,.xlsx,.xlsm,.ods,.xls"
            onChange={(event) => {
              const file = event.target.files?.[0];
              if (file !== undefined) {
                void openFile(file);
              }
            }}
          />
        </label>
        <button type="button" onClick={() => void saveProject()} disabled={state === null}>
          <Save size={16} aria-hidden="true" /> {L("حفظ", "Save")}
        </button>
        <button type="button" onClick={() => void loadProject()}>
          {L("استعادة", "Restore")}
        </button>
        <button
          type="button"
          onClick={() => void run((session) => session.undo(Date.now()))}
          disabled={state === null}
        >
          <Undo2 size={16} aria-hidden="true" /> {L("تراجع", "Undo")}
        </button>
        <button
          type="button"
          onClick={() => void run((session) => session.redo(Date.now()))}
          disabled={state === null}
        >
          <Redo2 size={16} aria-hidden="true" /> {L("إعادة", "Redo")}
        </button>
        <button type="button" onClick={exportCapsule} disabled={state === null}>
          <Download size={16} aria-hidden="true" />
          {L("تصدير كبسولة تفاعلية (.safi.html)", "Export interactive capsule (.safi.html)")}
        </button>
      </div>

      {error !== null ? (
        <p className="safi-studio__error" role="alert">
          <XCircle size={16} aria-hidden="true" /> {error}
        </p>
      ) : null}

      {state === null ? (
        <p className="safi-studio__empty">
          {L(
            "ابدأ باستيراد ملف CSV أو Excel. لا تُرسل بياناتك إلى أي خادم.",
            "Start by importing a CSV or Excel file. Your data is never sent to any server."
          )}
        </p>
      ) : (
        <>
          <dl className="safi-studio__meta">
            <div>
              <dt>{L("الإصدار", "Revision")}</dt>
              <dd>{state.revision}</dd>
            </div>
            <div>
              <dt>{L("نسخة البيانات", "Dataset version")}</dt>
              <dd>{state.datasetVersion}</dd>
            </div>
            <div>
              <dt>{L("صفوف", "Rows")}</dt>
              <dd>{state.rows.length}</dd>
            </div>
            <div>
              <dt>{L("أعمدة", "Columns")}</dt>
              <dd>{state.columns.length}</dd>
            </div>
          </dl>

          <h2>{L("المؤشرات الموثقة", "Verified metrics")}</h2>
          <div className="safi-studio__chips">
            {state.metrics.map((metric) => (
              <button
                key={metric.id}
                type="button"
                className={`safi-chip safi-chip--${metric.status.toLowerCase()}`}
                onClick={() => setAudit(metric)}
              >
                <span className="safi-chip__name">{metric.name}</span>
                <span className="safi-chip__value">
                  {metric.value} {metric.unit ?? ""}
                </span>
                <span className="safi-chip__state">
                  {metric.status === "Verified" ? (
                    <CheckCircle2 size={14} aria-hidden="true" />
                  ) : (
                    <AlertTriangle size={14} aria-hidden="true" />
                  )}
                  {metric.status}
                </span>
              </button>
            ))}
            {state.metrics.length === 0 ? (
              <p className="safi-studio__empty">
                {L("لا مؤشرات بعد.", "No metrics yet.")}
              </p>
            ) : null}
          </div>
          {state.errors.length > 0 ? (
            <ul className="safi-studio__errors">
              {state.errors.map((entry) => (
                <li key={entry.id}>
                  <AlertTriangle size={14} aria-hidden="true" /> {entry.id}: {entry.code}
                </li>
              ))}
            </ul>
          ) : null}

          <h2>{L("تعريف مؤشر", "Define a metric")}</h2>
          <form
            className="safi-studio__form"
            onSubmit={(event) => {
              event.preventDefault();
              void run((session) =>
                session.define_metric(
                  metricId,
                  metricName,
                  metricUnit.length > 0 ? metricUnit : undefined,
                  metricFormula,
                  Date.now()
                )
              );
            }}
          >
            <label>
              {L("المعرّف", "Id")}
              <input value={metricId} onChange={(event) => setMetricId(event.target.value)} />
            </label>
            <label>
              {L("الاسم", "Name")}
              <input value={metricName} onChange={(event) => setMetricName(event.target.value)} />
            </label>
            <label>
              {L("الوحدة", "Unit")}
              <input value={metricUnit} onChange={(event) => setMetricUnit(event.target.value)} />
            </label>
            <label>
              {L("المعادلة", "Formula")}
              <input
                value={metricFormula}
                onChange={(event) => setMetricFormula(event.target.value)}
                placeholder="SUM(amount) / COUNT(region)"
              />
            </label>
            <button type="submit">
              <ShieldCheck size={16} aria-hidden="true" /> {L("أضف المؤشر", "Add metric")}
            </button>
          </form>

          <h2>{L("عمود محسوب", "Calculated column")}</h2>
          <form
            className="safi-studio__form"
            onSubmit={(event) => {
              event.preventDefault();
              void run((session) =>
                session.add_calculated_column(calcName, calcFormula, Date.now())
              );
            }}
          >
            <label>
              {L("الاسم", "Name")}
              <input value={calcName} onChange={(event) => setCalcName(event.target.value)} />
            </label>
            <label>
              {L("المعادلة", "Formula")}
              <input value={calcFormula} onChange={(event) => setCalcFormula(event.target.value)} />
            </label>
            <button type="submit">
              <BarChart3 size={16} aria-hidden="true" /> {L("أضف العمود", "Add column")}
            </button>
          </form>

          <h2>{L("الجدول الحي", "Live sheet")}</h2>
          <div className="safi-studio__table-wrap">
            <table className="safi-studio__table">
              <thead>
                <tr>
                  {state.columns.map((column) => (
                    <th key={column.name} scope="col">
                      {column.name}
                      {state.calculated.includes(column.name) ? (
                        <span className="safi-studio__calc"> · {L("محسوب", "computed")}</span>
                      ) : null}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {state.rows.slice(0, 50).map((row, rowIndex) => (
                  <tr key={rowIndex}>
                    {row.map((cell, columnIndex) => {
                      const columnName = state.columns[columnIndex]?.name ?? "";
                      const editable = !state.calculated.includes(columnName);
                      return (
                        <td key={`${rowIndex}-${columnIndex}`}>
                          {editable ? (
                            <input
                              defaultValue={cell}
                              aria-label={`${columnName} ${rowIndex + 1}`}
                              onBlur={(event) => {
                                if (event.target.value !== cell) {
                                  void run((session) =>
                                    session.set_cell(
                                      rowIndex,
                                      columnName,
                                      event.target.value,
                                      Date.now()
                                    )
                                  );
                                }
                              }}
                              onKeyDown={(event) => {
                                if (event.key === "Enter") {
                                  event.currentTarget.blur();
                                }
                              }}
                            />
                          ) : (
                            <span>{cell}</span>
                          )}
                        </td>
                      );
                    })}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          <h2>{L("اللوحة", "Dashboard")}</h2>
          <div className="safi-studio__form">
            <label>
              {L("التجميع", "Aggregate")}
              <select
                value={aggregate}
                onChange={(event) => setAggregate(event.target.value as AggName)}
              >
                {(["SUM", "AVG", "COUNT", "MIN", "MAX"] as const).map((name) => (
                  <option key={name} value={name}>
                    {name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              {L("العمود", "Column")}
              <select value={valueColumn} onChange={(event) => setValueColumn(event.target.value)}>
                {selectableColumns.map((column) => (
                  <option key={column.name} value={column.name}>
                    {column.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              {L("التجميع حسب", "Group by")}
              <select value={groupBy} onChange={(event) => setGroupBy(event.target.value)}>
                {state.columns.map((column) => (
                  <option key={column.name} value={column.name}>
                    {column.name}
                  </option>
                ))}
              </select>
            </label>
            <button type="button" onClick={() => void refreshGroups()}>
              <BarChart3 size={16} aria-hidden="true" /> {L("حدّث الرسم", "Refresh chart")}
            </button>
          </div>
          <div className="safi-studio__bars">
            {groups.map((group) => {
              const numeric = Number(group.value);
              const maximum = Math.max(
                ...groups.map((candidate) => Math.abs(Number(candidate.value))),
                1
              );
              const width = Number.isFinite(numeric)
                ? Math.round((Math.abs(numeric) / maximum) * 100)
                : 0;
              return (
                <div key={group.label} className="safi-studio__bar">
                  <span>{group.label}</span>
                  <div className="safi-studio__track">
                    <div className="safi-studio__fill" style={{ width: `${width}%` }} />
                  </div>
                  <strong>{group.value}</strong>
                </div>
              );
            })}
          </div>
          {groups.length > 0 ? (
            <p className="safi-studio__note">
              {L(
                "قيم الرسم مجمّعة في Rust. العرض يعرض النسبة فقط، ولا يحسب أي رقم.",
                "Chart values are aggregated in Rust. The UI only renders a ratio and computes no number."
              )}
            </p>
          ) : null}

          <h2>{L("جودة البيانات", "Data quality")}</h2>
          {state.anomalies.length === 0 ? (
            <p className="safi-studio__note">
              <CheckCircle2 size={14} aria-hidden="true" />{" "}
              {L("لا شذوذ مكتشف.", "No anomalies detected.")}
            </p>
          ) : (
            <ul className="safi-studio__anomalies">
              {state.anomalies.map((anomaly, index) => (
                <li key={`${anomaly.kind}-${index}`}>
                  <strong>{anomaly.kind}</strong>
                  {anomaly.column !== null ? ` · ${anomaly.column}` : ""} ·{" "}
                  {anomaly.rows.length} {L("صف", "row(s)")}
                  <span className="safi-studio__note"> ({anomaly.suggestionKey})</span>
                </li>
              ))}
            </ul>
          )}
        </>
      )}

      {audit !== null ? (
        <div className="safi-studio__audit" role="dialog" aria-modal="true" aria-label={audit.name}>
          <div className="safi-studio__audit-card">
            <h3>{audit.name}</h3>
            <p className="safi-studio__audit-value">
              {audit.value} {audit.unit ?? ""}
            </p>
            <dl>
              <div>
                <dt>{L("المعادلة", "Formula")}</dt>
                <dd>{audit.formula}</dd>
              </div>
              <div>
                <dt>{L("إصدار المعادلة", "Formula version")}</dt>
                <dd>{audit.formulaVersion}</dd>
              </div>
              <div>
                <dt>{L("نسخة البيانات", "Dataset version")}</dt>
                <dd>{audit.sourceVersion}</dd>
              </div>
              <div>
                <dt>{L("الأعمدة المصدرية", "Source columns")}</dt>
                <dd>{audit.columns.join(", ")}</dd>
              </div>
              <div>
                <dt>{L("الصفوف المصدرية", "Source rows")}</dt>
                <dd>{audit.rows.slice(0, 20).map((row) => row + 1).join(", ")}</dd>
              </div>
              <div>
                <dt>{L("بصمة BLAKE3", "BLAKE3 fingerprint")}</dt>
                <dd className="safi-studio__fingerprint">{audit.fingerprint}</dd>
              </div>
              <div>
                <dt>{L("محرك الحساب", "Engine")}</dt>
                <dd>{audit.engineVersion}</dd>
              </div>
            </dl>
            <button type="button" onClick={() => setAudit(null)}>
              {L("إغلاق", "Close")}
            </button>
          </div>
        </div>
      ) : null}
    </section>
  );
}
