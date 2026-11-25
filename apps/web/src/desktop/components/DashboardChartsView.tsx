import React, { useEffect, useState } from "react";
import { ArrowDown, ArrowUp, BarChart3, Check, Pencil, Plus, Share2, Trash2 } from "lucide-react";
import {
  chartData as fetchChartData,
  grouped,
  type DesktopChartPoint,
  type DesktopChartSeries,
  type DesktopColumn,
  type DesktopMetric,
  type StudioChartArtifact,
  type StudioChartType
} from "../studio-bridge";
import { VerifiedNumberChip } from "./VerifiedNumberChip";

const AGGREGATES = ["SUM", "AVG", "COUNT", "MIN", "MAX"] as const;
export const CHART_TYPES: readonly StudioChartType[] = [
  "bar",
  "column",
  "line",
  "area",
  "stacked",
  "combo",
  "pie",
  "donut",
  "scatter",
  "bubble",
  "histogram",
  "boxplot",
  "heatmap",
  "treemap",
  "waterfall",
  "funnel",
  "gauge",
  "kpi",
  "table",
  "pivotsummary",
  "geographic"
];

const CHART_TYPE_LABELS: Readonly<
  Record<StudioChartType, { readonly en: string; readonly ar: string }>
> = {
  bar: { en: "Bar", ar: "شريطي أفقي (Bar)" },
  column: { en: "Column", ar: "أعمدة عمودية (Column)" },
  line: { en: "Line", ar: "خطي (Line)" },
  area: { en: "Area", ar: "مساحي (Area)" },
  stacked: { en: "Stacked", ar: "مكدّس (Stacked)" },
  combo: { en: "Combo", ar: "مركّب (Combo)" },
  pie: { en: "Pie", ar: "دائري (Pie)" },
  donut: { en: "Donut", ar: "حلقي (Donut)" },
  scatter: { en: "Scatter", ar: "انتشار (Scatter)" },
  bubble: { en: "Bubble", ar: "فقاعي (Bubble)" },
  histogram: { en: "Histogram", ar: "مدرج تكراري (Histogram)" },
  boxplot: { en: "Box Plot", ar: "صندوقي (BoxPlot)" },
  heatmap: { en: "Heatmap", ar: "خريطة حرارية (Heatmap)" },
  treemap: { en: "Treemap", ar: "خريطة شجرية (Treemap)" },
  waterfall: { en: "Waterfall", ar: "شلالي (Waterfall)" },
  funnel: { en: "Funnel", ar: "قمعي (Funnel)" },
  gauge: { en: "Gauge", ar: "مؤشر قياس (Gauge)" },
  kpi: { en: "KPI", ar: "بطاقة مؤشر (KPI)" },
  table: { en: "Table", ar: "جدول ملخص (Table)" },
  pivotsummary: { en: "Pivot Summary", ar: "جدول محوري (Pivot)" },
  geographic: { en: "Geographic", ar: "جغرافي (Geographic)" }
};

const CHART_SLICE_COLORS = [
  "#2563eb",
  "#0891b2",
  "#7c3aed",
  "#059669",
  "#d97706",
  "#db2777"
] as const;

function computeFallbackSectorPath(
  startFrac: number,
  sweepFrac: number,
  isDonut: boolean
): string | null {
  if (sweepFrac <= 0) return null;
  if (sweepFrac >= 0.999999) {
    return isDonut
      ? "M 50.00 10.00 A 40 40 0 1 1 50.00 90.00 A 40 40 0 1 1 50.00 10.00 M 50.00 28.00 A 22 22 0 1 0 50.00 72.00 A 22 22 0 1 0 50.00 28.00 Z"
      : "M 50.00 10.00 A 40 40 0 1 1 50.00 90.00 A 40 40 0 1 1 50.00 10.00 Z";
  }
  const startRad = -Math.PI / 2 + startFrac * Math.PI * 2;
  const endRad = startRad + sweepFrac * Math.PI * 2;
  const largeArc = sweepFrac > 0.5 ? 1 : 0;
  const ox1 = (50 + 40 * Math.cos(startRad)).toFixed(2);
  const oy1 = (50 + 40 * Math.sin(startRad)).toFixed(2);
  const ox2 = (50 + 40 * Math.cos(endRad)).toFixed(2);
  const oy2 = (50 + 40 * Math.sin(endRad)).toFixed(2);
  if (isDonut) {
    const ix2 = (50 + 22 * Math.cos(endRad)).toFixed(2);
    const iy2 = (50 + 22 * Math.sin(endRad)).toFixed(2);
    const ix1 = (50 + 22 * Math.cos(startRad)).toFixed(2);
    const iy1 = (50 + 22 * Math.sin(startRad)).toFixed(2);
    return `M ${ox1} ${oy1} A 40 40 0 ${largeArc} 1 ${ox2} ${oy2} L ${ix2} ${iy2} A 22 22 0 ${largeArc} 0 ${ix1} ${iy1} Z`;
  }
  return `M 50.00 50.00 L ${ox1} ${oy1} A 40 40 0 ${largeArc} 1 ${ox2} ${oy2} Z`;
}

export interface DashboardChartsViewProps {
  readonly language: "en" | "ar";
  readonly project: string;
  readonly revision: number;
  readonly activeScenario: string | null;
  readonly columns: readonly DesktopColumn[];
  readonly metrics: readonly DesktopMetric[];
  readonly charts: readonly StudioChartArtifact[];
  readonly initialSeriesByChart?: Readonly<Record<string, DesktopChartSeries>>;
  readonly activeFilterColumn?: string;
  readonly activeFilterValue?: string;
  readonly onUpdateCharts: (next: readonly StudioChartArtifact[]) => void;
  readonly onCrossFilter?: (column: string, label: string) => void;
  readonly onOpenFilteredSheet?: () => void;
  readonly onInspectMetric?: (metric: DesktopMetric) => void;
  readonly onShareDashboardToTeam?: () => void;
}

export function DashboardChartsView({
  language,
  project,
  revision,
  activeScenario,
  columns,
  metrics,
  charts,
  initialSeriesByChart,
  activeFilterColumn,
  activeFilterValue,
  onUpdateCharts,
  onCrossFilter,
  onOpenFilteredSheet,
  onInspectMetric,
  onShareDashboardToTeam
}: DashboardChartsViewProps): React.ReactElement {
  const isAr = language === "ar";
  const [seriesByChart, setSeriesByChart] = useState<Record<string, DesktopChartSeries>>(() => ({
    ...(initialSeriesByChart ?? {})
  }));
  const [errorsByChart, setErrorsByChart] = useState<Record<string, string>>({});
  const [title, setTitle] = useState(isAr ? "توزيع الإيرادات" : "Grouped Breakdown");
  const [chartType, setChartType] = useState<StudioChartType>("bar");
  const [func, setFunc] = useState<(typeof AGGREGATES)[number]>("SUM");
  const [col, setCol] = useState("");
  const [groupCol, setGroupCol] = useState("");
  const [editingChartId, setEditingChartId] = useState<string | null>(null);
  const [draftEdit, setDraftEdit] = useState<{
    readonly title: string;
    readonly function: string;
    readonly column: string;
    readonly groupBy: string;
  } | null>(null);
  const [localCrossFilter, setLocalCrossFilter] = useState<{
    col: string;
    val: string;
    sourceChartId?: string;
  } | null>(null);

  const effectiveCrossFilter =
    activeFilterColumn && activeFilterValue
      ? {
          col: activeFilterColumn,
          val: activeFilterValue,
          ...(localCrossFilter?.sourceChartId
            ? { sourceChartId: localCrossFilter.sourceChartId }
            : {})
        }
      : localCrossFilter;

  useEffect(() => {
    const firstCol = columns[0];
    if (firstCol === undefined) {
      return;
    }
    const secondOrFirst = columns[1] ?? firstCol;
    if (!col || !columns.some((c) => c.name === col)) {
      setCol(secondOrFirst.name);
    }
    if (!groupCol || !columns.some((c) => c.name === groupCol)) {
      setGroupCol(firstCol.name);
    }
  }, [columns, col, groupCol]);

  useEffect(() => {
    let cancelled = false;
    async function loadAll(): Promise<void> {
      const nextSeries: Record<string, DesktopChartSeries> = {};
      const nextErrors: Record<string, string> = {};
      for (const chart of charts) {
        const resolvedType: StudioChartType = chart.chartType ?? "bar";
        const shouldFilterThisCard =
          effectiveCrossFilter !== null &&
          effectiveCrossFilter.col.length > 0 &&
          effectiveCrossFilter.val.length > 0 &&
          chart.id !== effectiveCrossFilter.sourceChartId;
        try {
          nextSeries[chart.id] = await fetchChartData(
            project,
            resolvedType,
            chart.function,
            chart.column,
            chart.groupBy,
            shouldFilterThisCard ? effectiveCrossFilter.col : undefined,
            shouldFilterThisCard ? effectiveCrossFilter.val : undefined
          );
        } catch (err) {
          const code = err instanceof Error ? err.message : String(err);
          if (code === "SF-SYS-012" || code.includes("unknown")) {
            try {
              const fallbackGroups = await grouped(
                project,
                chart.function,
                chart.column,
                chart.groupBy
              );
              const pts: DesktopChartPoint[] = fallbackGroups.map((g, i) => ({
                label: g.label,
                value: g.value,
                ratioPct: 75,
                startPct: 0,
                xPct:
                  fallbackGroups.length > 1
                    ? Math.round(10 + (i * 80) / (fallbackGroups.length - 1))
                    : 50,
                yPct: 40
              }));
              nextSeries[chart.id] = {
                chartType: resolvedType,
                function: chart.function,
                column: chart.column,
                groupBy: chart.groupBy,
                totalOrKpi: fallbackGroups[0]?.value ?? "0",
                points: pts
              };
            } catch (innerErr) {
              nextErrors[chart.id] = innerErr instanceof Error ? innerErr.message : "SF-CAL-004";
            }
          } else {
            nextErrors[chart.id] = code;
          }
        }
      }
      if (!cancelled) {
        setSeriesByChart(nextSeries);
        setErrorsByChart(nextErrors);
      }
    }
    if (charts.length > 0) {
      void loadAll();
    } else {
      setSeriesByChart({});
      setErrorsByChart({});
    }
    return () => {
      cancelled = true;
    };
  }, [
    charts,
    project,
    revision,
    activeScenario,
    effectiveCrossFilter?.col,
    effectiveCrossFilter?.val,
    effectiveCrossFilter?.sourceChartId
  ]);

  function addChart(): void {
    if (!col || !groupCol) {
      return;
    }
    const id = `chart_${Date.now()}_${charts.length + 1}`;
    onUpdateCharts([
      ...charts,
      {
        id,
        title: title.trim() || `${func}(${col})`,
        chartType,
        function: func,
        column: col,
        groupBy: groupCol
      }
    ]);
  }

  function updateChartPatch(
    id: string,
    patch: Partial<
      Pick<StudioChartArtifact, "title" | "chartType" | "function" | "column" | "groupBy">
    >
  ): void {
    onUpdateCharts(charts.map((c) => (c.id === id ? { ...c, ...patch } : c)));
  }

  function triggerCrossFilter(chartId: string, groupColumn: string, label: string): void {
    setLocalCrossFilter({ col: groupColumn, val: label, sourceChartId: chartId });
    onCrossFilter?.(groupColumn, label);
  }

  function moveChart(idx: number, delta: -1 | 1): void {
    const target = idx + delta;
    if (target < 0 || target >= charts.length) {
      return;
    }
    const copy = [...charts];
    const [item] = copy.splice(idx, 1);
    if (item) {
      copy.splice(target, 0, item);
      onUpdateCharts(copy);
    }
  }

  return (
    <section className="studio-panel" data-testid="dashboard-charts-view">
      <div className="studio-panel-header">
        <div>
          <h3>{isAr ? "لوحة المؤشرات والرسوم التفاعلية" : "CodeMotion Reactive Dashboard"}</h3>
          <p className="studio-muted-note">
            {isAr
              ? "كل المؤشرات والرسوم الـ 21 تُحسب في محرك Rust وتتحدث تلقائيًا فور تعديل البيانات أو السيناريو."
              : "Every KPI and all 21 chart types are computed in Rust and update automatically on edits or What-If scenarios."}
          </p>
        </div>
        <div className="studio-quick-templates">
          {onShareDashboardToTeam && charts.length > 0 ? (
            <button
              type="button"
              className="studio-btn-primary"
              data-testid="dashboard-share-to-team"
              onClick={onShareDashboardToTeam}
            >
              <Share2 size={14} aria-hidden="true" />{" "}
              {isAr ? "مشاركة اللوحة مع الفريق" : "Share Dashboard to Team"}
            </button>
          ) : null}
          {effectiveCrossFilter ? (
            <div className="studio-quick-templates" data-testid="active-cross-filter-bar">
              {onOpenFilteredSheet ? (
                <button
                  type="button"
                  className="studio-chip-btn"
                  data-testid="view-filtered-sheet-btn"
                  onClick={onOpenFilteredSheet}
                >
                  {isAr
                    ? `عرض صفوف الجدول (${effectiveCrossFilter.val})`
                    : `View sheet rows (${effectiveCrossFilter.val})`}
                </button>
              ) : null}
              <button
                type="button"
                className="studio-chip-btn"
                data-testid="clear-cross-filter-btn"
                onClick={() => {
                  setLocalCrossFilter(null);
                  onCrossFilter?.(effectiveCrossFilter.col, "");
                }}
              >
                {isAr
                  ? `إلغاء التصفية (${effectiveCrossFilter.val})`
                  : `Clear filter (${effectiveCrossFilter.val})`}
              </button>
            </div>
          ) : null}
        </div>
      </div>

      {/* KPI Showcase */}
      <div className="safi-desktop-studio__metrics">
        {metrics.map((metric) => (
          <VerifiedNumberChip
            key={metric.id}
            metric={metric}
            lang={language}
            {...(onInspectMetric ? { onInspect: onInspectMetric } : {})}
          />
        ))}
      </div>

      {/* Add Chart Form */}
      <div className="studio-inline-fields">
        <label>
          <span>{isAr ? "عنوان الرسم" : "Chart Title"}</span>
          <input value={title} onChange={(e) => setTitle(e.target.value)} />
        </label>
        <label>
          <span>{isAr ? "نوع الرسم" : "Chart Type"}</span>
          <select
            aria-label={isAr ? "نوع الرسم" : "Chart Type"}
            value={chartType}
            onChange={(e) => setChartType(e.target.value as StudioChartType)}
          >
            {CHART_TYPES.map((kind) => (
              <option key={kind} value={kind}>
                {isAr ? CHART_TYPE_LABELS[kind].ar : CHART_TYPE_LABELS[kind].en}
              </option>
            ))}
          </select>
        </label>
        <label>
          <span>{isAr ? "الدالة" : "Function"}</span>
          <select
            value={func}
            onChange={(e) => setFunc(e.target.value as (typeof AGGREGATES)[number])}
          >
            {AGGREGATES.map((fn) => (
              <option key={fn} value={fn}>
                {fn}
              </option>
            ))}
          </select>
        </label>
        <label>
          <span>{isAr ? "عمود القيم" : "Value Column"}</span>
          <select value={col} onChange={(e) => setCol(e.target.value)}>
            {columns.map((c) => (
              <option key={c.id} value={c.name}>
                {c.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          <span>{isAr ? "تجميع / محور X" : "Group By / X Axis"}</span>
          <select value={groupCol} onChange={(e) => setGroupCol(e.target.value)}>
            {columns.map((c) => (
              <option key={c.id} value={c.name}>
                {c.name}
              </option>
            ))}
          </select>
        </label>
        <div className="studio-inline-action-end">
          <button type="button" className="studio-btn-primary" onClick={addChart}>
            <Plus size={14} aria-hidden="true" /> {isAr ? "إضافة رسم" : "Add Chart"}
          </button>
        </div>
      </div>

      {/* Charts Grid */}
      {charts.length === 0 ? (
        <p className="studio-muted-note">
          <BarChart3 size={14} aria-hidden="true" />{" "}
          {isAr ? "أضف رسمًا بيانيًا من النموذج أعلاه." : "Add a grouped chart card above."}
        </p>
      ) : (
        <div className="studio-card-grid">
          {charts.map((chart, idx) => {
            const resolvedType: StudioChartType = chart.chartType ?? "bar";
            const series = seriesByChart[chart.id];
            const errorCode = errorsByChart[chart.id];
            const points = series?.points ?? [];
            const polyPoints = points.map((p) => `${p.xPct},${p.yPct}`).join(" ");
            const firstPt = points[0];
            const lastPt = points[points.length - 1];

            const isEditing = editingChartId === chart.id;
            const isFilteredSibling =
              effectiveCrossFilter !== null &&
              effectiveCrossFilter.col.length > 0 &&
              effectiveCrossFilter.val.length > 0 &&
              chart.id !== effectiveCrossFilter.sourceChartId;

            return (
              <article
                key={chart.id}
                className="studio-subcard"
                data-testid={`chart-card-${resolvedType}`}
                data-chart-type={resolvedType}
                data-chart-id={chart.id}
                data-cross-filtered={isFilteredSibling ? "true" : "false"}
              >
                <div className="studio-panel-header">
                  <strong>{chart.title}</strong>
                  <div className="studio-quick-templates">
                    <select
                      aria-label={isAr ? "تغيير نوع الرسم" : "Change chart type"}
                      value={resolvedType}
                      onChange={(e) =>
                        updateChartPatch(chart.id, { chartType: e.target.value as StudioChartType })
                      }
                    >
                      {CHART_TYPES.map((kind) => (
                        <option key={kind} value={kind}>
                          {isAr ? CHART_TYPE_LABELS[kind].ar : CHART_TYPE_LABELS[kind].en}
                        </option>
                      ))}
                    </select>
                    <button
                      type="button"
                      className="studio-chip-btn"
                      data-testid={`edit-chart-${chart.id}`}
                      aria-label={
                        isEditing
                          ? isAr
                            ? "حفظ تعديلات الرسم"
                            : "Apply chart configuration"
                          : isAr
                            ? "تعديل إعدادات الرسم"
                            : "Edit chart configuration"
                      }
                      onClick={() => {
                        if (isEditing && draftEdit) {
                          updateChartPatch(chart.id, {
                            title: draftEdit.title.trim() || chart.title,
                            function: draftEdit.function,
                            column: draftEdit.column,
                            groupBy: draftEdit.groupBy
                          });
                          setEditingChartId(null);
                          setDraftEdit(null);
                        } else {
                          setEditingChartId(chart.id);
                          setDraftEdit({
                            title: chart.title,
                            function: chart.function,
                            column: chart.column,
                            groupBy: chart.groupBy
                          });
                        }
                      }}
                    >
                      {isEditing ? (
                        <Check size={13} aria-hidden="true" />
                      ) : (
                        <Pencil size={13} aria-hidden="true" />
                      )}
                    </button>
                    <button
                      type="button"
                      className="studio-chip-btn"
                      aria-label={isAr ? "نقل للأعلى" : "Move chart up"}
                      disabled={idx === 0}
                      onClick={() => moveChart(idx, -1)}
                    >
                      <ArrowUp size={13} aria-hidden="true" />
                    </button>
                    <button
                      type="button"
                      className="studio-chip-btn"
                      aria-label={isAr ? "نقل للأسفل" : "Move chart down"}
                      disabled={idx === charts.length - 1}
                      onClick={() => moveChart(idx, 1)}
                    >
                      <ArrowDown size={13} aria-hidden="true" />
                    </button>
                    <button
                      type="button"
                      className="studio-chip-btn"
                      aria-label={isAr ? "حذف الرسم" : "Remove chart"}
                      onClick={() => onUpdateCharts(charts.filter((c) => c.id !== chart.id))}
                    >
                      <Trash2 size={13} aria-hidden="true" />
                    </button>
                  </div>
                </div>
                <div className="studio-muted-note">
                  {`${resolvedType.toUpperCase()} · ${chart.function}(${chart.column}) · ${isAr ? "حسب" : "by"} ${chart.groupBy}`}
                  {isFilteredSibling && effectiveCrossFilter
                    ? ` · ${isAr ? "مُصفّى حسب" : "Filtered by"} ${effectiveCrossFilter.col}=${effectiveCrossFilter.val}`
                    : ""}
                </div>

                {isEditing && draftEdit ? (
                  <div className="studio-inline-fields" data-testid={`chart-editor-${chart.id}`}>
                    <label>
                      <span>{isAr ? "العنوان" : "Title"}</span>
                      <input
                        aria-label={isAr ? "عنوان البطاقة" : "Edit chart title"}
                        value={draftEdit.title}
                        onChange={(e) => setDraftEdit({ ...draftEdit, title: e.target.value })}
                      />
                    </label>
                    <label>
                      <span>{isAr ? "الدالة" : "Aggregation"}</span>
                      <select
                        aria-label={isAr ? "تعديل الدالة" : "Edit aggregation"}
                        value={draftEdit.function}
                        onChange={(e) => setDraftEdit({ ...draftEdit, function: e.target.value })}
                      >
                        {AGGREGATES.map((fn) => (
                          <option key={fn} value={fn}>
                            {fn}
                          </option>
                        ))}
                      </select>
                    </label>
                    <label>
                      <span>{isAr ? "عمود القيمة" : "Value Field"}</span>
                      <select
                        aria-label={isAr ? "تعديل عمود القيمة" : "Edit value field"}
                        value={draftEdit.column}
                        onChange={(e) => setDraftEdit({ ...draftEdit, column: e.target.value })}
                      >
                        {columns.map((c) => (
                          <option key={c.name} value={c.name}>
                            {c.name}
                          </option>
                        ))}
                      </select>
                    </label>
                    <label>
                      <span>{isAr ? "التجميع حسب" : "Group By"}</span>
                      <select
                        aria-label={isAr ? "تعديل عمود التجميع" : "Edit grouping field"}
                        value={draftEdit.groupBy}
                        onChange={(e) => setDraftEdit({ ...draftEdit, groupBy: e.target.value })}
                      >
                        {columns.map((c) => (
                          <option key={c.name} value={c.name}>
                            {c.name}
                          </option>
                        ))}
                      </select>
                    </label>
                  </div>
                ) : null}

                {errorCode ? (
                  <div className="studio-alert-error" role="alert">
                    {isAr
                      ? `الأعمدة المختارة غير متوافقة مع هذا النوع من الرسوم (${errorCode}).`
                      : `Selected columns are not suitable for this chart type (${errorCode}).`}
                  </div>
                ) : null}

                {resolvedType === "kpi" && series ? (
                  <div className="studio-kpi-callout" data-testid="chart-kpi-total">
                    <strong>{series.totalOrKpi}</strong>
                    <span className="studio-muted-note">
                      {`${chart.function}(${chart.column})`}
                    </span>
                  </div>
                ) : null}

                {(resolvedType === "pie" || resolvedType === "donut") && points.length > 0 ? (
                  <svg
                    className="studio-chart-svg studio-chart-svg--circle"
                    viewBox="0 0 100 100"
                    role="img"
                    aria-label={chart.title}
                  >
                    {points.map((pt, pIdx) => {
                      const d =
                        pt.svgPath ??
                        computeFallbackSectorPath(
                          pt.startPct / 100,
                          pt.ratioPct / 100,
                          resolvedType === "donut"
                        );
                      if (!d) return null;
                      const sliceAttr =
                        resolvedType === "donut"
                          ? { "data-donut-slice": pIdx }
                          : { "data-pie-slice": pIdx };
                      return (
                        <path
                          key={`${pt.label}_${pIdx}`}
                          {...sliceAttr}
                          d={d}
                          fill={CHART_SLICE_COLORS[pIdx % CHART_SLICE_COLORS.length]}
                        />
                      );
                    })}
                    {resolvedType === "donut" && series ? (
                      <text
                        x="50"
                        y="53"
                        textAnchor="middle"
                        fill="currentColor"
                        fontSize="9"
                        fontWeight="700"
                      >
                        {series.totalOrKpi}
                      </text>
                    ) : null}
                  </svg>
                ) : null}

                {resolvedType === "histogram" && points.length > 0 ? (
                  <svg
                    className="studio-chart-svg"
                    viewBox="0 0 100 100"
                    preserveAspectRatio="none"
                    role="img"
                    aria-label={chart.title}
                  >
                    {points.map((pt, pIdx) => {
                      const slotWidth = 84 / Math.max(points.length, 1);
                      const barWidth = Math.max(slotWidth - 2, 4);
                      const x = 8 + pIdx * slotWidth;
                      const height = Math.max((Math.min(pt.ratioPct, 100) * 78) / 100, 2);
                      const y = 90 - height;
                      return (
                        <rect
                          key={`${pt.label}_${pIdx}`}
                          data-histogram-bin={pIdx}
                          x={x.toFixed(2)}
                          y={y.toFixed(2)}
                          width={barWidth.toFixed(2)}
                          height={height.toFixed(2)}
                          fill="currentColor"
                        />
                      );
                    })}
                  </svg>
                ) : null}

                {(resolvedType === "line" ||
                  resolvedType === "area" ||
                  resolvedType === "scatter" ||
                  resolvedType === "bubble") &&
                points.length > 0 ? (
                  <svg
                    className="studio-chart-svg"
                    viewBox="0 0 100 100"
                    preserveAspectRatio="none"
                    role="img"
                    aria-label={chart.title}
                  >
                    {resolvedType === "scatter" || resolvedType === "bubble" ? (
                      <>
                        <line
                          x1="8"
                          y1="92"
                          x2="94"
                          y2="92"
                          stroke="currentColor"
                          strokeOpacity="0.3"
                          strokeWidth="1"
                        />
                        <line
                          x1="8"
                          y1="8"
                          x2="8"
                          y2="92"
                          stroke="currentColor"
                          strokeOpacity="0.3"
                          strokeWidth="1"
                        />
                      </>
                    ) : null}
                    {resolvedType === "area" && firstPt && lastPt ? (
                      <polygon
                        points={`${firstPt.xPct},90 ${polyPoints} ${lastPt.xPct},90`}
                        fill="currentColor"
                        fillOpacity="0.2"
                      />
                    ) : null}
                    {(resolvedType === "line" || resolvedType === "area") && points.length > 1 ? (
                      <polyline
                        points={polyPoints}
                        fill="none"
                        stroke="currentColor"
                        strokeWidth="2.5"
                      />
                    ) : null}
                    {points.map((pt, pIdx) => (
                      <circle
                        key={`${pt.label}_${pIdx}`}
                        data-bubble-point={resolvedType === "bubble" ? pIdx : undefined}
                        data-scatter-point={resolvedType === "scatter" ? pIdx : undefined}
                        cx={pt.xPct}
                        cy={pt.yPct}
                        r={resolvedType === "bubble" ? Number(pt.sharePct ?? 4.5) : 3}
                        fill={
                          resolvedType === "bubble"
                            ? CHART_SLICE_COLORS[pIdx % CHART_SLICE_COLORS.length]
                            : "currentColor"
                        }
                        fillOpacity={resolvedType === "bubble" ? 0.72 : 1}
                      />
                    ))}
                  </svg>
                ) : null}

                {resolvedType === "column" && points.length > 0 ? (
                  <svg
                    className="studio-chart-svg"
                    viewBox="0 0 100 100"
                    preserveAspectRatio="none"
                    role="img"
                    aria-label={chart.title}
                  >
                    <line
                      x1="8"
                      y1="90"
                      x2="94"
                      y2="90"
                      stroke="currentColor"
                      strokeOpacity="0.35"
                      strokeWidth="1"
                    />
                    {points.map((pt, pIdx) => {
                      const slotW = 80 / Math.max(points.length, 1);
                      const barW = Math.max(slotW * 0.65, 5);
                      const x = 10 + pIdx * slotW + (slotW - barW) * 0.5;
                      const topY = Math.max(
                        12,
                        Math.min(90, pt.startPct || 90 - (pt.ratioPct * 76) / 100)
                      );
                      const h = Math.max(2, 90 - topY);
                      return (
                        <rect
                          key={`${pt.label}_${pIdx}`}
                          data-column-bar={pIdx}
                          x={x.toFixed(2)}
                          y={topY.toFixed(2)}
                          width={barW.toFixed(2)}
                          height={h.toFixed(2)}
                          rx="1.5"
                          fill={CHART_SLICE_COLORS[pIdx % CHART_SLICE_COLORS.length]}
                        />
                      );
                    })}
                  </svg>
                ) : null}

                {resolvedType === "stacked" && points.length > 0 ? (
                  <svg
                    className="studio-chart-svg"
                    viewBox="0 0 100 44"
                    preserveAspectRatio="none"
                    role="img"
                    aria-label={chart.title}
                  >
                    {points.map((pt, pIdx) => (
                      <rect
                        key={`${pt.label}_${pIdx}`}
                        data-stacked-segment={pIdx}
                        x={(4 + Math.min(100, pt.startPct) * 0.92).toFixed(2)}
                        y="12"
                        width={Math.max(1.5, Math.min(100, pt.ratioPct) * 0.92).toFixed(2)}
                        height="20"
                        fill={CHART_SLICE_COLORS[pIdx % CHART_SLICE_COLORS.length]}
                      />
                    ))}
                  </svg>
                ) : null}

                {resolvedType === "combo" && points.length > 0 ? (
                  <svg
                    className="studio-chart-svg"
                    viewBox="0 0 100 100"
                    preserveAspectRatio="none"
                    role="img"
                    aria-label={chart.title}
                  >
                    <line
                      x1="8"
                      y1="90"
                      x2="94"
                      y2="90"
                      stroke="currentColor"
                      strokeOpacity="0.3"
                      strokeWidth="1"
                    />
                    {points.map((pt, pIdx) => {
                      const slotW = 76 / Math.max(points.length, 1);
                      const barW = Math.max(slotW * 0.55, 4);
                      const x = 12 + pIdx * slotW + (slotW - barW) * 0.5;
                      const topY = Math.max(
                        12,
                        Math.min(90, pt.startPct || 90 - (pt.ratioPct * 76) / 100)
                      );
                      return (
                        <rect
                          key={`${pt.label}_${pIdx}`}
                          data-combo-bar={pIdx}
                          x={x.toFixed(2)}
                          y={topY.toFixed(2)}
                          width={barW.toFixed(2)}
                          height={Math.max(2, 90 - topY).toFixed(2)}
                          fill="#2563eb"
                          fillOpacity="0.78"
                        />
                      );
                    })}
                    <polyline
                      data-combo-line="1"
                      points={polyPoints}
                      fill="none"
                      stroke="#f59e0b"
                      strokeWidth="2.4"
                    />
                  </svg>
                ) : null}

                {resolvedType === "gauge" && points.length > 0 ? (
                  <svg
                    className="studio-chart-svg studio-chart-svg--circle"
                    viewBox="0 0 100 80"
                    role="img"
                    aria-label={chart.title}
                  >
                    <path
                      data-gauge-track="1"
                      d="M 15.00 68.00 A 35 35 0 0 1 85.00 68.00"
                      fill="none"
                      stroke="currentColor"
                      strokeOpacity="0.18"
                      strokeWidth="10"
                      strokeLinecap="round"
                    />
                    {points[0]?.svgPath ? (
                      <path
                        data-gauge-arc="1"
                        d={points[0].svgPath}
                        fill="none"
                        stroke="#22d3ee"
                        strokeWidth="10"
                        strokeLinecap="round"
                      />
                    ) : null}
                    <text
                      x="50"
                      y="64"
                      textAnchor="middle"
                      fill="currentColor"
                      fontSize="11"
                      fontWeight="700"
                    >
                      {`${points[0]?.ratioPct ?? 0}%`}
                    </text>
                  </svg>
                ) : null}

                {resolvedType === "boxplot" && points.length > 0 ? (
                  <svg
                    className="studio-chart-svg"
                    viewBox="0 0 100 100"
                    preserveAspectRatio="none"
                    role="img"
                    aria-label={chart.title}
                  >
                    <line
                      x1="8"
                      y1="92"
                      x2="94"
                      y2="92"
                      stroke="currentColor"
                      strokeOpacity="0.3"
                      strokeWidth="1"
                    />
                    {points.map((pt, pIdx) =>
                      pt.svgPath ? (
                        <path
                          key={`${pt.label}_${pIdx}`}
                          data-boxplot-whisker={pIdx}
                          d={pt.svgPath}
                          fill="#1d4ed8"
                          fillOpacity="0.4"
                          stroke="#38bdf8"
                          strokeWidth="1.5"
                        />
                      ) : null
                    )}
                  </svg>
                ) : null}

                {resolvedType === "heatmap" && points.length > 0 ? (
                  <svg
                    className="studio-chart-svg"
                    viewBox="0 0 100 100"
                    preserveAspectRatio="none"
                    role="img"
                    aria-label={chart.title}
                  >
                    {points.map((pt, pIdx) => {
                      const cols = Math.max(2, points[0]?.startPct ?? 2);
                      const rows = Math.max(1, Math.ceil(points.length / cols));
                      const cw = 84 / cols;
                      const ch = 76 / rows;
                      const c = pIdx % cols;
                      const r = Math.floor(pIdx / cols);
                      const opacity = Math.min(
                        0.98,
                        0.2 + (Math.min(100, pt.ratioPct) / 100) * 0.78
                      );
                      return (
                        <rect
                          key={`${pt.label}_${pIdx}`}
                          data-heatmap-cell={pIdx}
                          x={(8 + c * cw + 1).toFixed(2)}
                          y={(12 + r * ch + 1).toFixed(2)}
                          width={Math.max(4, cw - 2).toFixed(2)}
                          height={Math.max(4, ch - 2).toFixed(2)}
                          rx="2"
                          fill="#06b6d4"
                          fillOpacity={opacity.toFixed(2)}
                        />
                      );
                    })}
                  </svg>
                ) : null}

                {resolvedType === "treemap" && points.length > 0 ? (
                  <svg
                    className="studio-chart-svg"
                    viewBox="0 0 100 100"
                    preserveAspectRatio="none"
                    role="img"
                    aria-label={chart.title}
                  >
                    {points.map((pt, pIdx) =>
                      pt.svgPath ? (
                        <g key={`${pt.label}_${pIdx}`}>
                          <path
                            data-treemap-tile={pIdx}
                            d={pt.svgPath}
                            fill={CHART_SLICE_COLORS[pIdx % CHART_SLICE_COLORS.length]}
                            fillOpacity="0.85"
                            stroke="currentColor"
                            strokeOpacity="0.25"
                            strokeWidth="1"
                          />
                          <text
                            x={pt.xPct}
                            y={pt.yPct}
                            textAnchor="middle"
                            fill="#ffffff"
                            fontSize="5.5"
                          >
                            {pt.label}
                          </text>
                        </g>
                      ) : null
                    )}
                  </svg>
                ) : null}

                {resolvedType === "waterfall" && points.length > 0 ? (
                  <svg
                    className="studio-chart-svg"
                    viewBox="0 0 100 100"
                    preserveAspectRatio="none"
                    role="img"
                    aria-label={chart.title}
                  >
                    <line
                      x1="8"
                      y1="90"
                      x2="94"
                      y2="90"
                      stroke="currentColor"
                      strokeOpacity="0.3"
                      strokeWidth="1"
                    />
                    {points.map((pt, pIdx) => {
                      const slotW = 80 / Math.max(points.length, 1);
                      const barW = Math.max(4, slotW * 0.65);
                      const x = 10 + pIdx * slotW;
                      const top = Math.min(90, pt.ratioPct);
                      const bottom = Math.min(92, pt.startPct);
                      const h = Math.max(3, Math.abs(bottom - top));
                      const fill =
                        pIdx + 1 === points.length
                          ? "#2563eb"
                          : pt.value.startsWith("-")
                            ? "#ef4444"
                            : "#10b981";
                      return (
                        <rect
                          key={`${pt.label}_${pIdx}`}
                          data-waterfall-step={pIdx}
                          x={x.toFixed(2)}
                          y={top.toFixed(2)}
                          width={barW.toFixed(2)}
                          height={h.toFixed(2)}
                          fill={fill}
                        />
                      );
                    })}
                  </svg>
                ) : null}

                {resolvedType === "funnel" && points.length > 0 ? (
                  <svg
                    className="studio-chart-svg"
                    viewBox="0 0 100 100"
                    preserveAspectRatio="none"
                    role="img"
                    aria-label={chart.title}
                  >
                    {points.map((pt, pIdx) => {
                      const rowH = 80 / Math.max(points.length, 1);
                      const y1 = 10 + pIdx * rowH;
                      const y2 = y1 + Math.max(4, rowH - 2);
                      const wTop = Math.max(12, Math.min(100, pt.ratioPct)) * 0.84;
                      const wBot =
                        Math.max(10, Math.min(100, pt.startPct || pt.ratioPct * 0.75)) * 0.84;
                      const pts = `${50 - wTop / 2},${y1} ${50 + wTop / 2},${y1} ${50 + wBot / 2},${y2} ${50 - wBot / 2},${y2}`;
                      return (
                        <polygon
                          key={`${pt.label}_${pIdx}`}
                          data-funnel-stage={pIdx}
                          points={pts}
                          fill={CHART_SLICE_COLORS[pIdx % CHART_SLICE_COLORS.length]}
                          fillOpacity="0.88"
                        />
                      );
                    })}
                  </svg>
                ) : null}

                {resolvedType === "geographic" && points.length > 0 ? (
                  <div>
                    <svg
                      className="studio-chart-svg"
                      viewBox="0 0 100 100"
                      role="img"
                      data-geo-projection="regional-coordinate-scatter"
                      aria-label={chart.title}
                    >
                      <rect
                        data-geo-backdrop="1"
                        x="4"
                        y="4"
                        width="92"
                        height="92"
                        rx="4"
                        fill="currentColor"
                        fillOpacity="0.04"
                        stroke="currentColor"
                        strokeOpacity="0.22"
                        strokeWidth="1"
                      />
                      <line
                        x1="35"
                        y1="4"
                        x2="35"
                        y2="96"
                        stroke="currentColor"
                        strokeOpacity="0.12"
                        strokeDasharray="2,2"
                      />
                      <line
                        x1="65"
                        y1="4"
                        x2="65"
                        y2="96"
                        stroke="currentColor"
                        strokeOpacity="0.12"
                        strokeDasharray="2,2"
                      />
                      <line
                        x1="4"
                        y1="35"
                        x2="96"
                        y2="35"
                        stroke="currentColor"
                        strokeOpacity="0.12"
                        strokeDasharray="2,2"
                      />
                      <line
                        x1="4"
                        y1="65"
                        x2="96"
                        y2="65"
                        stroke="currentColor"
                        strokeOpacity="0.12"
                        strokeDasharray="2,2"
                      />
                      {points.map((pt, pIdx) => (
                        <circle
                          key={`${pt.label}_${pIdx}`}
                          data-geo-marker={pIdx}
                          cx={pt.xPct}
                          cy={pt.yPct}
                          r={2.5 + (Math.min(100, pt.ratioPct) / 100) * 6}
                          fill={CHART_SLICE_COLORS[pIdx % CHART_SLICE_COLORS.length]}
                          fillOpacity="0.82"
                        />
                      ))}
                    </svg>
                    <p className="studio-muted-note">
                      {isAr
                        ? "إسقاط إحداثي للمناطق/الفئات (مناطق مصر المعروفة؛ باقي الفئات دون حدود خرائطية GIS)."
                        : "Regional / category coordinate scatter (recognized Egypt regions mapped; arbitrary labels plotted without GIS map boundaries)."}
                    </p>
                  </div>
                ) : null}

                {(resolvedType === "table" || resolvedType === "pivotsummary") &&
                points.length > 0 ? (
                  <div className="studio-mini-table-wrap">
                    <table className="studio-mini-table" data-chart-table={resolvedType}>
                      <thead>
                        <tr>
                          <th>
                            {resolvedType === "pivotsummary"
                              ? isAr
                                ? "المجموعة"
                                : "Group"
                              : isAr
                                ? "الفئة"
                                : "Category"}
                          </th>
                          <th>{isAr ? "القيمة" : "Value"}</th>
                          <th>
                            {resolvedType === "pivotsummary"
                              ? isAr
                                ? "التفاصيل"
                                : "Stats"
                              : isAr
                                ? "الحصة"
                                : "Share"}
                          </th>
                        </tr>
                      </thead>
                      <tbody>
                        {points.map((pt, pIdx) => (
                          <tr key={`${pt.label}_${pIdx}`}>
                            <td>{pt.label}</td>
                            <td>{pt.value}</td>
                            <td>
                              {resolvedType === "pivotsummary"
                                ? `${pt.xValue ?? ""} (${pt.sharePct ?? 0}%)`
                                : `${pt.sharePct ?? 0}%`}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                ) : null}

                {resolvedType === "bar" ? (
                  <div className="safi-desktop-studio__bars">
                    {points.map((point, pIdx) => (
                      <button
                        key={`${point.label}_${pIdx}`}
                        type="button"
                        className="safi-desktop-studio__bar studio-bar-btn"
                        onClick={() => triggerCrossFilter(chart.id, chart.groupBy, point.label)}
                        title={
                          isAr
                            ? `انقر لتصفية اللوحة والجدول حسب ${point.label}`
                            : `Click to cross-filter sibling charts and sheet by ${point.label}`
                        }
                      >
                        <span>{point.label}</span>
                        <div className="safi-desktop-studio__track">
                          <div
                            className="safi-desktop-studio__fill"
                            style={{ width: `${point.ratioPct}%` }}
                          />
                        </div>
                        <strong>{point.value}</strong>
                      </button>
                    ))}
                  </div>
                ) : resolvedType !== "kpi" && points.length > 0 ? (
                  <div
                    className="safi-desktop-studio__bars"
                    data-testid={`chart-legend-${resolvedType}`}
                  >
                    {points.map((point, pIdx) => {
                      const detail = point.sharePct
                        ? `${point.value} (${point.sharePct}%)`
                        : point.xValue && point.yValue
                          ? `(${point.xValue}, ${point.yValue})`
                          : point.value;
                      return (
                        <button
                          key={`${point.label}_${pIdx}`}
                          type="button"
                          className="safi-desktop-studio__bar studio-bar-btn"
                          onClick={() => triggerCrossFilter(chart.id, chart.groupBy, point.label)}
                          title={
                            isAr
                              ? `انقر لتصفية اللوحة والجدول حسب ${point.label}`
                              : `Click to cross-filter sibling charts and sheet by ${point.label}`
                          }
                        >
                          <span>{point.label}</span>
                          <strong>{detail}</strong>
                        </button>
                      );
                    })}
                  </div>
                ) : null}
              </article>
            );
          })}
        </div>
      )}
    </section>
  );
}
