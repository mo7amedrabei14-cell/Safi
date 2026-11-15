import React, { useEffect, useState } from "react";
import { ArrowDown, ArrowUp, BarChart3, Plus, Trash2 } from "lucide-react";
import {
  grouped,
  type DesktopColumn,
  type DesktopGroup,
  type DesktopMetric,
  type StudioChartArtifact
} from "../studio-bridge";
import { VerifiedNumberChip } from "./VerifiedNumberChip";

const AGGREGATES = ["SUM", "AVG", "COUNT", "MIN", "MAX"] as const;

export interface DashboardChartsViewProps {
  readonly language: "en" | "ar";
  readonly project: string;
  readonly revision: number;
  readonly activeScenario: string | null;
  readonly columns: readonly DesktopColumn[];
  readonly metrics: readonly DesktopMetric[];
  readonly charts: readonly StudioChartArtifact[];
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
  onUpdateCharts,
  onCrossFilter,
  onInspectMetric
}: DashboardChartsViewProps): React.ReactElement {
  const isAr = language === "ar";
  const [chartData, setChartData] = useState<Record<string, readonly DesktopGroup[]>>({});
  const [title, setTitle] = useState(isAr ? "توزيع الإيرادات" : "Grouped Breakdown");
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
      const next: Record<string, readonly DesktopGroup[]> = {};
      for (const chart of charts) {
        try {
          next[chart.id] = await grouped(project, chart.function, chart.column, chart.groupBy);
        } catch {
          next[chart.id] = [];
        }
      }
      if (!cancelled) {
        setChartData(next);
      }
    }
    if (charts.length > 0) {
      void loadAll();
    } else {
      setChartData({});
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
        function: func,
        column: col,
        groupBy: groupCol
      }
    ]);
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
              ? "كل المؤشرات والرسوم تُحسب في محرك Rust وتتحدث تلقائيًا فور تعديل البيانات أو السيناريو."
              : "Every KPI and grouped chart is computed in Rust and updates automatically on edits or What-If scenarios."}
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
          <span>{isAr ? "تجميع حسب" : "Group By"}</span>
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
            const groups = chartData[chart.id] ?? [];
            const maxAbs = Math.max(...groups.map((g) => Math.abs(Number(g.value))), 1);
            return (
              <article key={chart.id} className="studio-subcard">
                <div className="studio-panel-header">
                  <strong>{chart.title}</strong>
                  <div className="studio-quick-templates">
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
                  {`${chart.function}(${chart.column}) · ${isAr ? "حسب" : "by"} ${chart.groupBy}`}
                </div>
                <div className="safi-desktop-studio__bars">
                  {groups.map((group) => {
                    const width = Math.round((Math.abs(Number(group.value)) / maxAbs) * 100);
                    return (
                      <button
                        key={group.label}
                        type="button"
                        className="safi-desktop-studio__bar studio-bar-btn"
                        onClick={() => {
                          setActiveCrossFilter({ col: chart.groupBy, val: group.label });
                          onCrossFilter?.(chart.groupBy, group.label);
                        }}
                        title={
                          isAr
                            ? `انقر لتصفية الجدول حسب ${group.label}`
                            : `Click to cross-filter sheet by ${group.label}`
                        }
                      >
                        <span>{group.label}</span>
                        <div className="safi-desktop-studio__track">
                          <div
                            className="safi-desktop-studio__fill"
                            style={{ width: `${width}%` }}
                          />
                        </div>
                        <strong>{group.value}</strong>
                      </button>
                    );
                  })}
                </div>
              </article>
            );
          })}
        </div>
      )}
    </section>
  );
}
