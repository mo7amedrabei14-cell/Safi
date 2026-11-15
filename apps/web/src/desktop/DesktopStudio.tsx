/**
 * `apps/web/src/desktop/DesktopStudio.tsx`
 *
 * Quad-Studio workspace for the packaged desktop app. Execution is remote from this component:
 * every number arrives from `crates/sf-desktop/src/studio.rs` through the IPC bridge, so the
 * component never aggregates, parses a file or computes a percentage of user data.
 *
 * In a browser the runtime guard renders a notice instead of the workspace, which keeps the
 * public site a showcase.
 */
import type { ReactNode } from "react";
import { useCallback, useState } from "react";
import {
  AlertTriangle,
  BarChart3,
  CheckCircle2,
  Download,
  FileSpreadsheet,
  Redo2,
  Save,
  ShieldCheck,
  Undo2,
  XCircle
} from "lucide-react";
import "./desktop-studio.css";
import type { SafiLang } from "../components/layout/FloatingGlassDock";
import type { DesktopGroup, DesktopMetric, DesktopStudioState } from "./studio-bridge";
import {
  addCalculatedColumn,
  defineMetric,
  exportCapsule,
  grouped,
  isDesktopRuntime,
  openProject,
  pickInputFile,
  pickOutputFile,
  projectState,
  redo,
  restoreProject,
  setCell,
  snapshot,
  undo
} from "./studio-bridge";

const PROJECT = "main";
const SAVED_KEY = "safi.desktop.studio.snapshot.v1";
const AGGREGATES = ["SUM", "AVG", "COUNT", "MIN", "MAX"] as const;
type Aggregate = (typeof AGGREGATES)[number];

export interface DesktopStudioProps {
  readonly lang?: SafiLang;
}

export function DesktopStudio({ lang = "en" }: DesktopStudioProps): ReactNode {
  const L = useCallback((ar: string, en: string): string => (lang === "ar" ? ar : en), [lang]);
  const desktop = isDesktopRuntime();
  const [state, setState] = useState<DesktopStudioState | null>(null);
  const [audit, setAudit] = useState<DesktopMetric | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [metricId, setMetricId] = useState("total_amount");
  const [metricName, setMetricName] = useState("Total amount");
  const [metricUnit, setMetricUnit] = useState("EGP");
  const [metricFormula, setMetricFormula] = useState("SUM(amount)");
  const [columnName, setColumnName] = useState("line_total");
  const [columnFormula, setColumnFormula] = useState("amount * quantity");
  const [aggregate, setAggregate] = useState<Aggregate>("SUM");
  const [valueColumn, setValueColumn] = useState("");
  const [groupBy, setGroupBy] = useState("");
  const [groups, setGroups] = useState<readonly DesktopGroup[]>([]);

  const run = useCallback(async (action: () => Promise<DesktopStudioState>): Promise<void> => {
    setBusy(true);
    try {
      setState(await action());
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  }, []);

  const openFile = useCallback(async (): Promise<void> => {
    setBusy(true);
    try {
      const path = await pickInputFile();
      if (path === null) {
        return;
      }
      const next = await openProject(PROJECT, path);
      setState(next);
      setGroups([]);
      const first = next.columns[0]?.name ?? "";
      setGroupBy(first);
      setValueColumn(next.columns[1]?.name ?? first);
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  }, []);

  const save = useCallback(async (): Promise<void> => {
    try {
      window.localStorage.setItem(SAVED_KEY, await snapshot(PROJECT));
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }, []);

  const restore = useCallback(async (): Promise<void> => {
    const saved = window.localStorage.getItem(SAVED_KEY);
    if (saved === null) {
      setError(L("لا يوجد مشروع محفوظ على هذا الجهاز.", "No saved project on this device."));
      return;
    }
    await run(() => restoreProject(PROJECT, saved));
  }, [L, run]);

  const refreshChart = useCallback(async (): Promise<void> => {
    if (groupBy.length === 0 || valueColumn.length === 0) {
      return;
    }
    setBusy(true);
    try {
      setGroups(await grouped(PROJECT, aggregate, valueColumn, groupBy));
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  }, [aggregate, groupBy, valueColumn]);

  const writeCapsule = useCallback(async (): Promise<void> => {
    setBusy(true);
    try {
      const target = await pickOutputFile();
      if (target === null) {
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
      await exportCapsule(
        PROJECT,
        target,
        JSON.stringify({
          title: metricName.length > 0 ? metricName : "Safi capsule",
          created_at: new Date().toISOString(),
          charts
        })
      );
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  }, [aggregate, groupBy, metricName, valueColumn]);

  if (!desktop) {
    return (
      <section className="safi-desktop-studio safi-desktop-studio--locked" role="note">
        <FileSpreadsheet size={28} aria-hidden="true" />
        <h1>{L("الاستوديو يعمل داخل التطبيق", "The Studio runs inside the app")}</h1>
        <p>
          {L(
            "هذه الشاشة تنفّذ المحرك محلياً داخل تطبيق سطح المكتب فقط. حمّل التطبيق لتفتح الملفات وتحسب المؤشرات.",
            "This screen executes the engine locally inside the desktop app only. Download the app to open files and compute metrics."
          )}
        </p>
      </section>
    );
  }

  return (
    <section className="safi-desktop-studio" aria-busy={busy}>
      <header className="safi-desktop-studio__head">
        <h1>{L("الاستوديو الرباعي", "Quad-Studio")}</h1>
        <p>
          {L(
            "كل رقم هنا محسوب في محرك Rust على جهازك ومربوط ببصمة تحقق.",
            "Every number here is computed by the Rust engine on your machine and bound to a verifiable fingerprint."
          )}
        </p>
      </header>

      <div className="safi-desktop-studio__toolbar">
        <button type="button" onClick={() => void openFile()} disabled={busy}>
          <FileSpreadsheet size={16} aria-hidden="true" />
          {L("افتح ملفاً", "Open a file")}
        </button>
        <button type="button" onClick={() => void save()} disabled={state === null}>
          <Save size={16} aria-hidden="true" /> {L("حفظ", "Save")}
        </button>
        <button type="button" onClick={() => void restore()}>
          {L("استعادة", "Restore")}
        </button>
        <button
          type="button"
          onClick={() => void run(() => undo(PROJECT))}
          disabled={state === null}
        >
          <Undo2 size={16} aria-hidden="true" /> {L("تراجع", "Undo")}
        </button>
        <button
          type="button"
          onClick={() => void run(() => redo(PROJECT))}
          disabled={state === null}
        >
          <Redo2 size={16} aria-hidden="true" /> {L("إعادة", "Redo")}
        </button>
        <button type="button" onClick={() => void writeCapsule()} disabled={state === null}>
          <Download size={16} aria-hidden="true" />
          {L("تصدير كبسولة (.safi.html)", "Export capsule (.safi.html)")}
        </button>
        {state !== null ? (
          <button
            type="button"
            onClick={() => void run(() => projectState(PROJECT))}
            disabled={busy}
          >
            {L("تحديث", "Refresh")}
          </button>
        ) : null}
      </div>

      {error !== null ? (
        <p className="safi-desktop-studio__error" role="alert">
          <XCircle size={16} aria-hidden="true" /> {error}
        </p>
      ) : null}

      {state === null ? (
        <p className="safi-desktop-studio__empty">
          {L(
            "ابدأ بفتح ملف CSV أو Excel. لا يخرج أي جزء من بياناتك من جهازك.",
            "Start by opening a CSV or Excel file. No part of your data leaves this machine."
          )}
        </p>
      ) : (
        <>
          <dl className="safi-desktop-studio__meta">
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
          <div className="safi-desktop-studio__chips">
            {state.metrics.map((metric) => (
              <button
                key={metric.id}
                type="button"
                className={`safi-desktop-chip safi-desktop-chip--${metric.status.toLowerCase()}`}
                onClick={() => setAudit(metric)}
              >
                <span className="safi-desktop-chip__name">{metric.name}</span>
                <span className="safi-desktop-chip__value">
                  {metric.value} {metric.unit ?? ""}
                </span>
                <span className="safi-desktop-chip__state">
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
              <p className="safi-desktop-studio__empty">{L("لا مؤشرات بعد.", "No metrics yet.")}</p>
            ) : null}
          </div>

          <h2>{L("تعريف مؤشر", "Define a metric")}</h2>
          <form
            className="safi-desktop-studio__form"
            onSubmit={(event) => {
              event.preventDefault();
              void run(() =>
                defineMetric(
                  PROJECT,
                  metricId,
                  metricName,
                  metricUnit.length > 0 ? metricUnit : null,
                  metricFormula
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
            <button type="submit" disabled={busy}>
              <ShieldCheck size={16} aria-hidden="true" /> {L("أضف المؤشر", "Add metric")}
            </button>
          </form>

          <h2>{L("عمود محسوب", "Calculated column")}</h2>
          <form
            className="safi-desktop-studio__form"
            onSubmit={(event) => {
              event.preventDefault();
              void run(() => addCalculatedColumn(PROJECT, columnName, columnFormula));
            }}
          >
            <label>
              {L("الاسم", "Name")}
              <input value={columnName} onChange={(event) => setColumnName(event.target.value)} />
            </label>
            <label>
              {L("المعادلة", "Formula")}
              <input
                value={columnFormula}
                onChange={(event) => setColumnFormula(event.target.value)}
              />
            </label>
            <button type="submit" disabled={busy}>
              <BarChart3 size={16} aria-hidden="true" /> {L("أضف العمود", "Add column")}
            </button>
          </form>

          <h2>{L("الجدول الحي", "Live sheet")}</h2>
          <div className="safi-desktop-studio__table-wrap">
            <table className="safi-desktop-studio__table">
              <thead>
                <tr>
                  {state.columns.map((column) => (
                    <th key={column.name} scope="col">
                      {column.name}
                      {state.calculated.includes(column.name) ? (
                        <span className="safi-desktop-studio__calc">
                          {" "}
                          · {L("محسوب", "computed")}
                        </span>
                      ) : null}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {state.rows.slice(0, 50).map((row, rowIndex) => (
                  <tr key={rowIndex}>
                    {row.map((cell, columnIndex) => {
                      const columnLabel = state.columns[columnIndex]?.name ?? "";
                      const editable = !state.calculated.includes(columnLabel);
                      return (
                        <td key={`${rowIndex}-${columnIndex}`}>
                          {editable ? (
                            <input
                              defaultValue={cell}
                              aria-label={`${columnLabel} ${rowIndex + 1}`}
                              onBlur={(event) => {
                                if (event.target.value !== cell) {
                                  void run(() =>
                                    setCell(PROJECT, rowIndex, columnLabel, event.target.value)
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
          <div className="safi-desktop-studio__form">
            <label>
              {L("التجميع", "Aggregate")}
              <select
                value={aggregate}
                onChange={(event) => setAggregate(event.target.value as Aggregate)}
              >
                {AGGREGATES.map((name) => (
                  <option key={name} value={name}>
                    {name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              {L("العمود", "Column")}
              <select value={valueColumn} onChange={(event) => setValueColumn(event.target.value)}>
                {state.columns.map((column) => (
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
            <button type="button" onClick={() => void refreshChart()} disabled={busy}>
              <BarChart3 size={16} aria-hidden="true" /> {L("حدّث الرسم", "Refresh chart")}
            </button>
          </div>
          <div className="safi-desktop-studio__bars">
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
                <div key={group.label} className="safi-desktop-studio__bar">
                  <span>{group.label}</span>
                  <div className="safi-desktop-studio__track">
                    <div className="safi-desktop-studio__fill" style={{ width: `${width}%` }} />
                  </div>
                  <strong>{group.value}</strong>
                </div>
              );
            })}
          </div>
          {groups.length > 0 ? (
            <p className="safi-desktop-studio__note">
              {L(
                "القيم مجمّعة في Rust؛ الواجهة ترسم النسبة فقط.",
                "Values are aggregated in Rust; the UI only draws a ratio."
              )}
            </p>
          ) : null}

          <h2>{L("جودة البيانات", "Data quality")}</h2>
          {state.anomalies.length === 0 ? (
            <p className="safi-desktop-studio__note">
              <CheckCircle2 size={14} aria-hidden="true" />{" "}
              {L("لا شذوذ مكتشف.", "No anomalies detected.")}
            </p>
          ) : (
            <ul className="safi-desktop-studio__anomalies">
              {state.anomalies.map((anomaly, index) => (
                <li key={`${anomaly.kind}-${index}`}>
                  <strong>{anomaly.kind}</strong>
                  {anomaly.column !== null ? ` · ${anomaly.column}` : ""} · {anomaly.rows.length}{" "}
                  {L("صف", "row(s)")}
                  <span className="safi-desktop-studio__note"> ({anomaly.suggestionKey})</span>
                </li>
              ))}
            </ul>
          )}
        </>
      )}

      {audit !== null ? (
        <div
          className="safi-desktop-studio__audit"
          role="dialog"
          aria-modal="true"
          aria-label={audit.name}
        >
          <div className="safi-desktop-studio__audit-card">
            <h3>{audit.name}</h3>
            <p className="safi-desktop-studio__audit-value">
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
                <dd>
                  {audit.rows
                    .slice(0, 20)
                    .map((row) => row + 1)
                    .join(", ")}
                </dd>
              </div>
              <div>
                <dt>{L("بصمة BLAKE3", "BLAKE3 fingerprint")}</dt>
                <dd className="safi-desktop-studio__fingerprint">{audit.fingerprint}</dd>
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
