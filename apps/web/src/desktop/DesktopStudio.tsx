/**
 * `apps/web/src/desktop/DesktopStudio.tsx`
 *
 * Quad-Studio workspace for the packaged desktop app. Execution runs exclusively inside
 * `crates/sf-desktop/src/studio.rs` behind the IPC bridge; this component never parses files,
 * aggregates numbers, or recomputes metrics in TypeScript.
 *
 * In a browser the runtime guard renders a notice instead of the workspace, which keeps the
 * public site a showcase.
 */
import type { ReactNode } from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  ArrowDownUp,
  BarChart3,
  Download,
  FileSpreadsheet,
  Plus,
  Redo2,
  RefreshCw,
  Save,
  ShieldCheck,
  Trash2,
  Undo2,
  XCircle
} from "lucide-react";
import "./desktop-studio.css";
import type { SafiLang } from "../components/layout/FloatingGlassDock";
import type {
  DesktopGroup,
  DesktopRowPage,
  DesktopStudioState
} from "./studio-bridge";
import {
  activateScenario,
  addCalculatedColumn,
  addScenario,
  applyAnomalyFix,
  applyDslCommand,
  compareScenarios,
  createRequestTracker,
  defineMetric,
  defineParameter,
  deleteRow,
  exportCapsule,
  fetchRowsPage,
  grouped,
  insertRow,
  isDesktopRuntime,
  isNewerOrEqualRevision,
  openProject,
  pickCapsuleOutputFile,
  pickInputFile,
  projectState,
  redo,
  renameScenario,
  resetScenario,
  restoreProject,
  setCellById,
  setScenarioParameter,
  snapshot,
  sortRows,
  undo,
  updateParameter
} from "./studio-bridge";
import { AnomalyPanel } from "./components/AnomalyPanel";
import { CommandBarPanel } from "./components/CommandBarPanel";
import { VerifiedNumberChip } from "./components/VerifiedNumberChip";
import { WhatIfPanel } from "./components/WhatIfPanel";

const PROJECT = "main";
const SAVED_KEY = "safi.desktop.studio.snapshot.v1";
const AGGREGATES = ["SUM", "AVG", "COUNT", "MIN", "MAX"] as const;
const PAGE_SIZES = [25, 50, 100, 200] as const;
type Aggregate = (typeof AGGREGATES)[number];

export interface DesktopStudioProps {
  readonly lang?: SafiLang;
}

export function DesktopStudio({ lang = "en" }: DesktopStudioProps): ReactNode {
  const L = useCallback((ar: string, en: string): string => (lang === "ar" ? ar : en), [lang]);
  const desktop = isDesktopRuntime();
  const [state, setState] = useState<DesktopStudioState | null>(null);
  const [rowPage, setRowPage] = useState<DesktopRowPage | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [recalculating, setRecalculating] = useState(false);

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

  // Paginated viewport query state (UI only holds one bounded page <= 200 rows)
  const [pageOffset, setPageOffset] = useState(0);
  const [pageLimit, setPageLimit] = useState<number>(50);
  const [searchQuery, setSearchQuery] = useState("");
  const [filterColumn, setFilterColumn] = useState("");
  const [filterValue, setFilterValue] = useState("");
  const [sortColumn, setSortColumn] = useState<string | null>(null);
  const [sortAscending, setSortAscending] = useState(true);

  const stateRef = useRef<DesktopStudioState | null>(null);
  stateRef.current = state;

  const stateTracker = useRef(createRequestTracker());
  const rowsTracker = useRef(createRequestTracker());
  const chartTracker = useRef(createRequestTracker());

  const run = useCallback(async (action: () => Promise<DesktopStudioState>): Promise<void> => {
    const seq = stateTracker.current.next();
    setBusy(true);
    setRecalculating(true);
    try {
      const next = await action();
      if (!stateTracker.current.isCurrent(seq)) {
        return;
      }
      if (isNewerOrEqualRevision(next, stateRef.current)) {
        setState(next);
        setError(null);
      }
    } catch (cause) {
      if (stateTracker.current.isCurrent(seq)) {
        setError(cause instanceof Error ? cause.message : String(cause));
      }
    } finally {
      if (stateTracker.current.isCurrent(seq)) {
        setBusy(false);
        setRecalculating(false);
      }
    }
  }, []);

  // Automatically load paginated rows whenever revision or query parameters change.
  const loadRows = useCallback(async (): Promise<void> => {
    if (!desktop || stateRef.current === null) {
      return;
    }
    const seq = rowsTracker.current.next();
    const expectedRevision = stateRef.current.revision;
    try {
      const page = await fetchRowsPage(PROJECT, {
        offset: pageOffset,
        limit: pageLimit,
        search: searchQuery.trim().length > 0 ? searchQuery : null,
        sort_column: sortColumn,
        ascending: sortAscending,
        filter_column: filterColumn.length > 0 ? filterColumn : null,
        filter_value: filterValue.trim().length > 0 ? filterValue : null
      });
      if (!rowsTracker.current.isCurrent(seq)) {
        return;
      }
      if (stateRef.current !== null && page.revision < stateRef.current.revision) {
        return;
      }
      if (page.revision >= expectedRevision) {
        setRowPage(page);
      }
    } catch (cause) {
      if (rowsTracker.current.isCurrent(seq)) {
        setError(cause instanceof Error ? cause.message : String(cause));
      }
    }
  }, [desktop, filterColumn, filterValue, pageLimit, pageOffset, searchQuery, sortAscending, sortColumn]);

  // Automatically recompute chart grouping in Rust whenever data or chart spec changes.
  const loadChart = useCallback(async (): Promise<void> => {
    if (!desktop || stateRef.current === null || groupBy.length === 0 || valueColumn.length === 0) {
      return;
    }
    const seq = chartTracker.current.next();
    try {
      const nextGroups = await grouped(PROJECT, aggregate, valueColumn, groupBy);
      if (chartTracker.current.isCurrent(seq)) {
        setGroups(nextGroups);
      }
    } catch (cause) {
      if (chartTracker.current.isCurrent(seq)) {
        setError(cause instanceof Error ? cause.message : String(cause));
      }
    }
  }, [aggregate, desktop, groupBy, valueColumn]);

  useEffect(() => {
    if (state !== null) {
      void loadRows();
    }
  }, [state, state?.revision, loadRows]);

  useEffect(() => {
    if (state !== null) {
      void loadChart();
    }
  }, [state, state?.revision, loadChart]);

  const openFile = useCallback(async (): Promise<void> => {
    const seq = stateTracker.current.next();
    setBusy(true);
    setRecalculating(true);
    try {
      const path = await pickInputFile();
      if (path === null || !stateTracker.current.isCurrent(seq)) {
        return;
      }
      const next = await openProject(PROJECT, path);
      if (!stateTracker.current.isCurrent(seq)) {
        return;
      }
      setState(next);
      setPageOffset(0);
      setSortColumn(null);
      setSearchQuery("");
      setFilterValue("");
      setGroups([]);
      const first = next.columns[0]?.name ?? "";
      setGroupBy(first);
      setValueColumn(next.columns[1]?.name ?? first);
      setError(null);
    } catch (cause) {
      if (stateTracker.current.isCurrent(seq)) {
        setError(cause instanceof Error ? cause.message : String(cause));
      }
    } finally {
      if (stateTracker.current.isCurrent(seq)) {
        setBusy(false);
        setRecalculating(false);
      }
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
    await run(async () => {
      const next = await restoreProject(PROJECT, saved);
      const first = next.columns[0]?.name ?? "";
      if (first.length > 0 && groupBy.length === 0) {
        setGroupBy(first);
        setValueColumn(next.columns[1]?.name ?? first);
      }
      return next;
    });
  }, [L, groupBy.length, run]);

  const writeCapsule = useCallback(async (): Promise<void> => {
    setBusy(true);
    try {
      const target = await pickCapsuleOutputFile();
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
        <button type="button" onClick={() => void restore()} disabled={busy}>
          {L("استعادة", "Restore")}
        </button>
        <button
          type="button"
          onClick={() => void run(() => undo(PROJECT))}
          disabled={state === null || busy}
        >
          <Undo2 size={16} aria-hidden="true" /> {L("تراجع", "Undo")}
        </button>
        <button
          type="button"
          onClick={() => void run(() => redo(PROJECT))}
          disabled={state === null || busy}
        >
          <Redo2 size={16} aria-hidden="true" /> {L("إعادة", "Redo")}
        </button>
        <button type="button" onClick={() => void writeCapsule()} disabled={state === null || busy}>
          <Download size={16} aria-hidden="true" />
          {L("تصدير كبسولة (.safi.html)", "Export capsule (.safi.html)")}
        </button>
        {state !== null ? (
          <button
            type="button"
            onClick={() => void run(() => projectState(PROJECT))}
            disabled={busy}
          >
            <RefreshCw size={16} aria-hidden="true" /> {L("تحديث", "Refresh")}
          </button>
        ) : null}
      </div>

      {recalculating ? (
        <p className="safi-desktop-studio__note" role="status" aria-live="polite">
          {L("جارٍ إعادة الحساب في Rust — النتائج السابقة موسومة كقديمة.", "Recalculating in Rust — previous values are marked Stale.")}
        </p>
      ) : null}

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
              <dd>{state.rowCount}</dd>
            </div>
            <div>
              <dt>{L("أعمدة", "Columns")}</dt>
              <dd>{state.columns.length}</dd>
            </div>
          </dl>

          <h2>{L("المؤشرات الموثقة", "Verified metrics")}</h2>
          <div className="safi-desktop-studio__chips">
            {state.metrics.map((metric) => (
              <VerifiedNumberChip
                key={metric.id}
                metric={metric}
                lang={lang}
                recalculating={recalculating}
              />
            ))}
            {state.errors.map((metricErr) => (
              <VerifiedNumberChip
                key={`err-${metricErr.id}`}
                metric={null}
                fallbackLabel={metricErr.id}
                errorCode={metricErr.code}
                lang={lang}
              />
            ))}
            {state.metrics.length === 0 && state.errors.length === 0 ? (
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

          <WhatIfPanel
            state={state}
            lang={lang}
            busy={busy}
            onDefineParameter={(name, value) => run(() => defineParameter(PROJECT, name, value))}
            onUpdateParameter={(name, value) => run(() => updateParameter(PROJECT, name, value))}
            onAddScenario={(id, name) => run(() => addScenario(PROJECT, id, name, {}))}
            onRenameScenario={(id, newName) => run(() => renameScenario(PROJECT, id, newName))}
            onActivateScenario={(id) => run(() => activateScenario(PROJECT, id))}
            onResetScenario={() => run(() => resetScenario(PROJECT))}
            onSetScenarioParameter={(sid, p, v) =>
              run(() => setScenarioParameter(PROJECT, sid, p, v))
            }
            onCompareScenarios={(left, right) => compareScenarios(PROJECT, left, right)}
          />

          <div className="safi-desktop-studio__panel-head">
            <h2>{L("الجدول الحي (مقسّم الصفحات)", "Live sheet (paginated)")}</h2>
            <button
              type="button"
              disabled={busy}
              onClick={() => void run(() => insertRow(PROJECT, null, {}))}
            >
              <Plus size={14} aria-hidden="true" /> {L("أضف صفاً", "Insert row")}
            </button>
          </div>

          <div className="safi-desktop-studio__form">
            <label>
              {L("بحث", "Search")}
              <input
                value={searchQuery}
                onChange={(event) => {
                  setSearchQuery(event.target.value);
                  setPageOffset(0);
                }}
                placeholder={L("ابحث في الأعمدة...", "Search columns...")}
              />
            </label>
            <label>
              {L("تصفية العمود", "Filter column")}
              <select
                value={filterColumn}
                onChange={(event) => {
                  setFilterColumn(event.target.value);
                  setPageOffset(0);
                }}
              >
                <option value="">{L("الكل", "All")}</option>
                {state.columns.map((col) => (
                  <option key={col.id} value={col.name}>
                    {col.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              {L("قيمة التصفية", "Filter value")}
              <input
                value={filterValue}
                onChange={(event) => {
                  setFilterValue(event.target.value);
                  setPageOffset(0);
                }}
              />
            </label>
            <label>
              {L("حجم الصفحة", "Page size")}
              <select
                value={pageLimit}
                onChange={(event) => {
                  setPageLimit(Number(event.target.value));
                  setPageOffset(0);
                }}
              >
                {PAGE_SIZES.map((size) => (
                  <option key={size} value={size}>
                    {size}
                  </option>
                ))}
              </select>
            </label>
          </div>

          <div className="safi-desktop-studio__table-wrap">
            <table className="safi-desktop-studio__table">
              <thead>
                <tr>
                  <th scope="col">{L("هوية الصف", "Row ID")}</th>
                  {state.columns.map((column) => (
                    <th key={column.id} scope="col">
                      <button
                        type="button"
                        className="safi-desktop-studio__col-sort"
                        onClick={() => {
                          if (sortColumn === column.name) {
                            const nextAsc = !sortAscending;
                            setSortAscending(nextAsc);
                            void run(() => sortRows(PROJECT, column.name, nextAsc));
                          } else {
                            setSortColumn(column.name);
                            setSortAscending(true);
                            void run(() => sortRows(PROJECT, column.name, true));
                          }
                        }}
                      >
                        <span>{column.name}</span>
                        <ArrowDownUp size={12} aria-hidden="true" />
                      </button>
                      {state.calculated.includes(column.name) ? (
                        <span className="safi-desktop-studio__calc">
                          {" "}
                          · {L("محسوب", "computed")}
                        </span>
                      ) : null}
                    </th>
                  ))}
                  <th scope="col">{L("إجراء", "Action")}</th>
                </tr>
              </thead>
              <tbody>
                {(rowPage?.rows ?? []).map((rowItem) => (
                  <tr key={rowItem.row_id}>
                    <td>
                      <span className="safi-desktop-studio__rowid">
                        r{rowItem.row_id}
                        {rowItem.origin_row_id !== null
                          ? ` (src r${rowItem.origin_row_id})`
                          : ` (${L("جديد", "new")})`}
                      </span>
                    </td>
                    {rowItem.cells.map((cell, columnIndex) => {
                      const columnLabel = state.columns[columnIndex]?.name ?? "";
                      const editable = !state.calculated.includes(columnLabel);
                      return (
                        <td
                          key={`${rowItem.row_id}-${columnLabel}`}
                          data-changed={cell.changed ? "true" : undefined}
                        >
                          {editable ? (
                            <input
                              key={`${rowItem.row_id}-${columnLabel}-${state.revision}`}
                              defaultValue={cell.raw}
                              aria-label={`${columnLabel} r${rowItem.row_id}`}
                              onBlur={(event) => {
                                if (event.target.value !== cell.raw) {
                                  const nextVal = event.target.value;
                                  void run(() =>
                                    setCellById(
                                      PROJECT,
                                      rowItem.row_id,
                                      columnLabel,
                                      nextVal,
                                      state.revision
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
                            <span>{cell.visible}</span>
                          )}
                        </td>
                      );
                    })}
                    <td>
                      <button
                        type="button"
                        aria-label={`${L("حذف", "Delete")} r${rowItem.row_id}`}
                        disabled={busy}
                        onClick={() => void run(() => deleteRow(PROJECT, rowItem.row_id))}
                      >
                        <Trash2 size={14} aria-hidden="true" />
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          {rowPage !== null ? (
            <div className="safi-desktop-studio__pager">
              <button
                type="button"
                disabled={pageOffset === 0}
                onClick={() => setPageOffset(Math.max(0, pageOffset - pageLimit))}
              >
                {L("السابق", "Previous")}
              </button>
              <span>
                {rowPage.filtered_rows === 0
                  ? L("0 صفوف مطابقة", "0 matching rows")
                  : `${rowPage.offset + 1}–${rowPage.offset + rowPage.rows.length} / ${rowPage.filtered_rows} (${L("الإجمالي", "total")}: ${rowPage.total_rows})`}
              </span>
              <button
                type="button"
                disabled={rowPage.offset + rowPage.rows.length >= rowPage.filtered_rows}
                onClick={() => setPageOffset(pageOffset + pageLimit)}
              >
                {L("التالي", "Next")}
              </button>
            </div>
          ) : null}

          <h2>{L("اللوحة (تحديث تفاعلي)", "Dashboard (reactive)")}</h2>
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
                  <option key={column.id} value={column.name}>
                    {column.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              {L("التجميع حسب", "Group by")}
              <select value={groupBy} onChange={(event) => setGroupBy(event.target.value)}>
                {state.columns.map((column) => (
                  <option key={column.id} value={column.name}>
                    {column.name}
                  </option>
                ))}
              </select>
            </label>
            <button type="button" onClick={() => void loadChart()} disabled={busy}>
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
                "القيم مجمّعة في Rust وتُحدَّث تلقائياً عند كل تعديل؛ الواجهة ترسم النسبة فقط.",
                "Values are aggregated in Rust and update automatically on every edit; the UI only draws ratios."
              )}
            </p>
          ) : null}

          <h2>{L("لوحة الأوامر والذكاء الاصطناعي", "Command Bar & AI Interpreter")}</h2>
          <CommandBarPanel
            language={lang}
            projectLoaded={state !== null}
            schema={{
              columns: state.columns.map((c) => ({ name: c.name, type: c.type })),
              metricIds: state.metrics.map((m) => m.id),
              parameterNames: state.parameters.map((p) => p.name)
            }}
            onExecuteDsl={(dslJson) => run(() => applyDslCommand(PROJECT, dslJson))}
          />

          <h2>{L("جودة البيانات", "Data quality")}</h2>
          <AnomalyPanel
            language={lang}
            anomalies={state.anomalies}
            isMutating={busy}
            onApplyFix={(fix) => run(() => applyAnomalyFix(PROJECT, fix, state.revision))}
            onFocusRowId={(rowId) => {
              setSearchQuery(String(rowId));
              setPageOffset(0);
            }}
          />
        </>
      )}
    </section>
  );
}
