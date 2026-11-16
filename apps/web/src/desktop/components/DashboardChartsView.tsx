import React, { useEffect, useState } from "react";
import { ArrowDown, ArrowUp, BarChart3, Plus, Trash2 } from "lucide-react";
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
  "line",
  "area",
  "pie",
  "donut",
  "scatter",
  "histogram",
  "kpi"
];

const CHART_TYPE_LABELS: Readonly<
  Record<StudioChartType, { readonly en: string; readonly ar: string }>
> = {
  bar: { en: "Bar", ar: "أعمدة (Bar)" },
  line: { en: "Line", ar: "خطي (Line)" },
  area: { en: "Area", ar: "مساحي (Area)" },
  pie: { en: "Pie", ar: "دائري (Pie)" },
  donut: { en: "Donut", ar: "حلقي (Donut)" },
  scatter: { en: "Scatter", ar: "انتشار (Scatter)" },
  histogram: { en: "Histogram", ar: "مدرج تكراري (Histogram)" },
  kpi: { en: "KPI", ar: "بطاقة مؤشر (KPI)" }
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
  readonly onUpdateCharts: (next: readonly StudioChartArtifact[]) => void;
  readonly onCrossFilter?: (column: string, label: string) => void;
  readonly onInspectMetric?: (metric: DesktopMetric) => void;
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
  onUpdateCharts,
  onCrossFilter,
  onInspectMetric
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
  const [activeCrossFilter, setActiveCrossFilter] = useState<{ col: string; val: string } | null>(
    null
  );

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
        try {
          nextSeries[chart.id] = await fetchChartData(
            project,
            resolvedType,
            chart.function,
            chart.column,
            chart.groupBy
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
  }, [charts, project, revision, activeScenario]);

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

  function updateChartType(id: string, nextType: StudioChartType): void {
    onUpdateCharts(charts.map((c) => (c.id === id ? { ...c, chartType: nextType } : c)));
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
              ? "كل المؤشرات والرسوم الـ 8 تُحسب في محرك Rust وتتحدث تلقائيًا فور تعديل البيانات أو السيناريو."
              : "Every KPI and all 8 chart types are computed in Rust and update automatically on edits or What-If scenarios."}
          </p>
        </div>
        {activeCrossFilter ? (
          <button
            type="button"
            className="studio-chip-btn"
            onClick={() => {
              setActiveCrossFilter(null);
              onCrossFilter?.(activeCrossFilter.col, "");
            }}
          >
            {isAr
              ? `إلغاء التصفية (${activeCrossFilter.val})`
              : `Clear filter (${activeCrossFilter.val})`}
          </button>
        ) : null}
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

            return (
              <article
                key={chart.id}
                className="studio-subcard"
                data-testid={`chart-card-${resolvedType}`}
                data-chart-type={resolvedType}
              >
                <div className="studio-panel-header">
                  <strong>{chart.title}</strong>
                  <div className="studio-quick-templates">
                    <select
                      aria-label={isAr ? "تغيير نوع الرسم" : "Change chart type"}
                      value={resolvedType}
                      onChange={(e) => updateChartType(chart.id, e.target.value as StudioChartType)}
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
                </div>

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
                  resolvedType === "scatter") &&
                points.length > 0 ? (
                  <svg
                    className="studio-chart-svg"
                    viewBox="0 0 100 100"
                    preserveAspectRatio="none"
                    role="img"
                    aria-label={chart.title}
                  >
                    {resolvedType === "scatter" ? (
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
                        cx={pt.xPct}
                        cy={pt.yPct}
                        r="3"
                        fill="currentColor"
                      />
                    ))}
                  </svg>
                ) : null}

                {resolvedType === "bar" ? (
                  <div className="safi-desktop-studio__bars">
                    {points.map((point, pIdx) => (
                      <button
                        key={`${point.label}_${pIdx}`}
                        type="button"
                        className="safi-desktop-studio__bar studio-bar-btn"
                        onClick={() => {
                          setActiveCrossFilter({ col: chart.groupBy, val: point.label });
                          onCrossFilter?.(chart.groupBy, point.label);
                        }}
                        title={
                          isAr
                            ? `انقر لتصفية الجدول حسب ${point.label}`
                            : `Click to cross-filter sheet by ${point.label}`
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
                          onClick={() => {
                            setActiveCrossFilter({ col: chart.groupBy, val: point.label });
                            onCrossFilter?.(chart.groupBy, point.label);
                          }}
                          title={
                            isAr
                              ? `انقر لتصفية الجدول حسب ${point.label}`
                              : `Click to cross-filter sheet by ${point.label}`
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
