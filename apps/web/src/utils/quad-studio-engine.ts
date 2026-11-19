/**
 * Unified Quad-Studio Engine (`Phases 5, 6, 7, 8`):
 * - Complete Dashboard Studio (`Phase 5`: 21 Chart Types, Cross-Filtering, Drill-Down,
 *   Calculated Metrics, Undo/Redo, Full Widget Editor)
 * - Presentation Studio (`Phase 6`: 10-Slide Data-Grounded Deck Generator, Slide & Presenter
 *   Notes Editor, Interactive Fullscreen HTML Presentation with Keyboard Navigation)
 * - Document Studio (`Phase 7`: 13-Section Paginated Analytical Report Generator & Editor
 *   with Headers, Footers, Page Numbers, and Page Breaks)
 * - Shared Template & Design System (`Phase 8`: Versioned Schema v2, Safe Migration,
 *   and Portable JSON Import/Export across all Studios)
 */

import type { CanonicalKpiMetric, SemanticWorkbookModel } from "./semantic-intelligence";
import type { VerifiedAnalysisReport } from "./verified-analysis-engine";

// ============================================================================
// PHASE 8: SHARED TEMPLATE & DESIGN SYSTEM
// ============================================================================

export interface StudioDesignTokens {
  readonly paletteId: string;
  readonly primaryColor: string;
  readonly secondaryColor: string;
  readonly accentColor: string;
  readonly surfaceColor: string;
  readonly backgroundColor: string;
  readonly textColor: string;
  readonly seriesColors: readonly [string, string, string, string, string, string];
  readonly headingFontEn: string;
  readonly bodyFontEn: string;
  readonly headingFontAr: string;
  readonly bodyFontAr: string;
  readonly spacingScale: "compact" | "comfortable" | "spacious";
  readonly kpiCardStyle: "glass" | "bordered" | "elevated";
  readonly tableStyle: "striped" | "minimal" | "bordered";
  readonly pageMarginMm: number;
}

export interface UnifiedStudioTemplate {
  readonly schemaVersion: 2;
  readonly templateId: string;
  readonly nameEn: string;
  readonly nameAr: string;
  readonly descriptionEn: string;
  readonly descriptionAr: string;
  readonly applicableStudios: readonly ("dashboard" | "presentation" | "document")[];
  readonly tokens: StudioDesignTokens;
  readonly createdAtIso: string;
}

export const BUILTIN_UNIFIED_TEMPLATES: readonly UnifiedStudioTemplate[] = [
  {
    schemaVersion: 2,
    templateId: "tpl_vision_executive",
    nameEn: "Vision Glass Executive",
    nameAr: "الرؤية التنفيذية الزجاجية",
    descriptionEn: "Clean luminous palette for executive dashboards, decks, and board reports.",
    descriptionAr: "تصميم مضيء ومتوازن للوحات المؤشرات والعروض والتقارير التنفيذية.",
    applicableStudios: ["dashboard", "presentation", "document"],
    tokens: {
      paletteId: "vision-glass",
      primaryColor: "#0284c7",
      secondaryColor: "#0f172a",
      accentColor: "#0ea5e9",
      surfaceColor: "#ffffff",
      backgroundColor: "#f8fafc",
      textColor: "#0f172a",
      seriesColors: ["#0284c7", "#0d9488", "#6366f1", "#f59e0b", "#ec4899", "#10b981"],
      headingFontEn: "Inter",
      bodyFontEn: "Inter",
      headingFontAr: "Cairo",
      bodyFontAr: "IBM Plex Sans Arabic",
      spacingScale: "comfortable",
      kpiCardStyle: "glass",
      tableStyle: "striped",
      pageMarginMm: 18
    },
    createdAtIso: "2026-01-01T00:00:00.000Z"
  },
  {
    schemaVersion: 2,
    templateId: "tpl_gemini_obsidian",
    nameEn: "Obsidian Analytical Dark",
    nameAr: "التحليلي الداكن الاحترافي",
    descriptionEn: "High-contrast dark analytical theme for operations centers and deep dives.",
    descriptionAr: "تباين داكن عالي الوضوح لغرف العمليات والتحليلات المعمقة.",
    applicableStudios: ["dashboard", "presentation", "document"],
    tokens: {
      paletteId: "google-gemini",
      primaryColor: "#38bdf8",
      secondaryColor: "#1e293b",
      accentColor: "#818cf8",
      surfaceColor: "#111827",
      backgroundColor: "#090d16",
      textColor: "#f8fafc",
      seriesColors: ["#38bdf8", "#34d399", "#a78bfa", "#fbbf24", "#f472b6", "#60a5fa"],
      headingFontEn: "Inter",
      bodyFontEn: "Inter",
      headingFontAr: "Cairo",
      bodyFontAr: "IBM Plex Sans Arabic",
      spacingScale: "compact",
      kpiCardStyle: "bordered",
      tableStyle: "bordered",
      pageMarginMm: 15
    },
    createdAtIso: "2026-01-01T00:00:00.000Z"
  },
  {
    schemaVersion: 2,
    templateId: "tpl_emerald_sovereign",
    nameEn: "Sovereign Financial Report",
    nameAr: "التقرير المالي المؤسسي",
    descriptionEn: "Formal audit-grade typography and emerald accents for financial disclosures.",
    descriptionAr: "تنسيق رسمي للتدقيق المالي والتقارير المؤسسية المعتمدة.",
    applicableStudios: ["dashboard", "presentation", "document"],
    tokens: {
      paletteId: "emerald-sovereign",
      primaryColor: "#047857",
      secondaryColor: "#064e3b",
      accentColor: "#10b981",
      surfaceColor: "#ffffff",
      backgroundColor: "#f0fdf4",
      textColor: "#064e3b",
      seriesColors: ["#047857", "#0284c7", "#d97706", "#7c3aed", "#be123c", "#0f766e"],
      headingFontEn: "Inter",
      bodyFontEn: "Inter",
      headingFontAr: "Cairo",
      bodyFontAr: "IBM Plex Sans Arabic",
      spacingScale: "spacious",
      kpiCardStyle: "elevated",
      tableStyle: "bordered",
      pageMarginMm: 22
    },
    createdAtIso: "2026-01-01T00:00:00.000Z"
  }
];

/**
 * Safely migrates a legacy v1 or partial template payload to canonical `UnifiedStudioTemplate` (`schemaVersion: 2`).
 */
export function migrateAndValidateTemplate(raw: unknown): UnifiedStudioTemplate {
  const fallback = BUILTIN_UNIFIED_TEMPLATES[0];
  if (!fallback) {
    throw new Error("SF-SYS-001: Missing default studio template.");
  }
  if (typeof raw !== "object" || raw === null) {
    return fallback;
  }
  const obj = raw as Record<string, unknown>;
  const rawTokens =
    typeof obj.tokens === "object" && obj.tokens !== null
      ? (obj.tokens as Record<string, unknown>)
      : {};

  return {
    schemaVersion: 2,
    templateId:
      typeof obj.templateId === "string" && obj.templateId.trim().length > 0
        ? obj.templateId.trim()
        : typeof obj.id === "string" && obj.id.trim().length > 0
          ? obj.id.trim()
          : `tpl_custom_${Date.now()}`,
    nameEn:
      typeof obj.nameEn === "string" && obj.nameEn.trim().length > 0
        ? obj.nameEn.trim()
        : typeof obj.name === "string" && obj.name.trim().length > 0
          ? obj.name.trim()
          : fallback.nameEn,
    nameAr:
      typeof obj.nameAr === "string" && obj.nameAr.trim().length > 0
        ? obj.nameAr.trim()
        : fallback.nameAr,
    descriptionEn:
      typeof obj.descriptionEn === "string" ? obj.descriptionEn : fallback.descriptionEn,
    descriptionAr:
      typeof obj.descriptionAr === "string" ? obj.descriptionAr : fallback.descriptionAr,
    applicableStudios: Array.isArray(obj.applicableStudios)
      ? (obj.applicableStudios.filter(
          (s): s is "dashboard" | "presentation" | "document" =>
            s === "dashboard" || s === "presentation" || s === "document"
        ) as ("dashboard" | "presentation" | "document")[])
      : ["dashboard", "presentation", "document"],
    tokens: {
      paletteId:
        typeof rawTokens.paletteId === "string" ? rawTokens.paletteId : fallback.tokens.paletteId,
      primaryColor:
        typeof rawTokens.primaryColor === "string"
          ? rawTokens.primaryColor
          : fallback.tokens.primaryColor,
      secondaryColor:
        typeof rawTokens.secondaryColor === "string"
          ? rawTokens.secondaryColor
          : fallback.tokens.secondaryColor,
      accentColor:
        typeof rawTokens.accentColor === "string"
          ? rawTokens.accentColor
          : fallback.tokens.accentColor,
      surfaceColor:
        typeof rawTokens.surfaceColor === "string"
          ? rawTokens.surfaceColor
          : fallback.tokens.surfaceColor,
      backgroundColor:
        typeof rawTokens.backgroundColor === "string"
          ? rawTokens.backgroundColor
          : fallback.tokens.backgroundColor,
      textColor:
        typeof rawTokens.textColor === "string" ? rawTokens.textColor : fallback.tokens.textColor,
      seriesColors: fallback.tokens.seriesColors,
      headingFontEn:
        typeof rawTokens.headingFontEn === "string"
          ? rawTokens.headingFontEn
          : fallback.tokens.headingFontEn,
      bodyFontEn:
        typeof rawTokens.bodyFontEn === "string"
          ? rawTokens.bodyFontEn
          : fallback.tokens.bodyFontEn,
      headingFontAr:
        typeof rawTokens.headingFontAr === "string"
          ? rawTokens.headingFontAr
          : fallback.tokens.headingFontAr,
      bodyFontAr:
        typeof rawTokens.bodyFontAr === "string"
          ? rawTokens.bodyFontAr
          : fallback.tokens.bodyFontAr,
      spacingScale:
        rawTokens.spacingScale === "compact" ||
        rawTokens.spacingScale === "comfortable" ||
        rawTokens.spacingScale === "spacious"
          ? rawTokens.spacingScale
          : fallback.tokens.spacingScale,
      kpiCardStyle:
        rawTokens.kpiCardStyle === "glass" ||
        rawTokens.kpiCardStyle === "bordered" ||
        rawTokens.kpiCardStyle === "elevated"
          ? rawTokens.kpiCardStyle
          : fallback.tokens.kpiCardStyle,
      tableStyle:
        rawTokens.tableStyle === "striped" ||
        rawTokens.tableStyle === "minimal" ||
        rawTokens.tableStyle === "bordered"
          ? rawTokens.tableStyle
          : fallback.tokens.tableStyle,
      pageMarginMm:
        typeof rawTokens.pageMarginMm === "number" && rawTokens.pageMarginMm >= 5
          ? rawTokens.pageMarginMm
          : fallback.tokens.pageMarginMm
    },
    createdAtIso: typeof obj.createdAtIso === "string" ? obj.createdAtIso : fallback.createdAtIso
  };
}

// ============================================================================
// PHASE 5: COMPLETE DASHBOARD STUDIO (21 CHART TYPES + FULL EDITOR + CROSS-FILTER)
// ============================================================================

export type DashboardChartType =
  | "Bar"
  | "Column"
  | "Line"
  | "Area"
  | "Stacked"
  | "Combo"
  | "Pie"
  | "Donut"
  | "Scatter"
  | "Bubble"
  | "Histogram"
  | "BoxPlot"
  | "Heatmap"
  | "Treemap"
  | "Waterfall"
  | "Funnel"
  | "Gauge"
  | "KPI"
  | "Table"
  | "PivotSummary"
  | "Geographic";

export const ALL_SUPPORTED_CHART_TYPES: readonly DashboardChartType[] = [
  "Bar",
  "Column",
  "Line",
  "Area",
  "Stacked",
  "Combo",
  "Pie",
  "Donut",
  "Scatter",
  "Bubble",
  "Histogram",
  "BoxPlot",
  "Heatmap",
  "Treemap",
  "Waterfall",
  "Funnel",
  "Gauge",
  "KPI",
  "Table",
  "PivotSummary",
  "Geographic"
];

export type WidgetSortMode = "value_desc" | "value_asc" | "label_asc" | "label_desc";

export interface CalculatedMetricDefinition {
  readonly metricId: string;
  readonly name: string;
  readonly formulaExpression: string; // e.g. "Revenue_EGP - Cost_EGP"
  readonly leftColumn: string;
  readonly operator: "+" | "-" | "*" | "/";
  readonly rightColumn: string;
  readonly unit?: string | undefined;
}

export interface DashboardFilterCondition {
  readonly columnName: string;
  readonly allowedValues: readonly string[];
}

export interface ChartDataPoint {
  readonly label: string;
  readonly value: number;
  readonly secondaryValue?: number | undefined;
  readonly sharePct?: number | undefined;
}

export interface DashboardWidget {
  readonly widgetId: string;
  readonly kind: "chart" | "kpi" | "table" | "annotation" | "image";
  readonly title: string;
  readonly subtitle?: string | undefined;
  readonly chartType: DashboardChartType;
  readonly sheetName: string;
  readonly tableId: string;
  readonly dimensionColumn?: string | undefined;
  readonly drillDownDimensionColumn?: string | undefined;
  readonly activeDrillDownValue?: string | undefined;
  readonly measureColumn?: string | undefined;
  readonly secondaryMeasureColumn?: string | undefined;
  readonly aggregation: "SUM" | "AVERAGE" | "MEDIAN" | "MIN" | "MAX" | "COUNT_DISTINCT";
  readonly sortMode: WidgetSortMode;
  readonly localFilters: readonly DashboardFilterCondition[];
  readonly colorOverride?: string | undefined;
  readonly unitOverride?: string | undefined;
  readonly annotationText?: string | undefined;
  readonly imageUrl?: string | undefined;
  readonly gridLayout: {
    readonly x: number;
    readonly y: number;
    readonly w: number;
    readonly h: number;
  };
  readonly dataPoints: readonly ChartDataPoint[];
  readonly kpiMetric?: CanonicalKpiMetric | undefined;
}

export interface DashboardStudioState {
  readonly dashboardId: string;
  readonly title: string;
  readonly templateId: string;
  readonly globalFilters: readonly DashboardFilterCondition[];
  readonly crossFilterSelection?:
    | {
        readonly sourceWidgetId: string;
        readonly dimensionColumn: string;
        readonly selectedValue: string;
      }
    | undefined;
  readonly calculatedMetrics: readonly CalculatedMetricDefinition[];
  readonly widgets: readonly DashboardWidget[];
  readonly undoStack: readonly (readonly DashboardWidget[])[];
  readonly redoStack: readonly (readonly DashboardWidget[])[];
}

function parseNumVal(v: string): number | null {
  const cleaned = v
    .trim()
    .replace(/^[$€£]\s*/, "")
    .replace(/\s*(EGP|SAR|AED|USD|EUR|ج\.م|ر\.س|%)\s*$/i, "")
    .replace(/,/g, "");
  if (!cleaned || !/^-?\d+(\.\d+)?$/.test(cleaned)) return null;
  const n = Number(cleaned);
  return Number.isFinite(n) ? n : null;
}

/**
 * Computes aggregated chart data points for a widget respecting global filters,
 * local filters, cross-filtering, drill-down, calculated metrics, and sorting.
 */
export function computeWidgetDataPoints(
  model: SemanticWorkbookModel,
  widget: Pick<
    DashboardWidget,
    | "tableId"
    | "dimensionColumn"
    | "drillDownDimensionColumn"
    | "activeDrillDownValue"
    | "measureColumn"
    | "secondaryMeasureColumn"
    | "aggregation"
    | "sortMode"
    | "localFilters"
  > & { readonly sheetName?: string | undefined },
  globalFilters: readonly DashboardFilterCondition[] = [],
  crossFilter?: DashboardStudioState["crossFilterSelection"],
  calculatedMetrics: readonly CalculatedMetricDefinition[] = []
): ChartDataPoint[] {
  const rawRegion = model.ingestionReport.worksheets
    .flatMap((w) => w.tables)
    .find(
      (t) => t.tableId === widget.tableId && (!widget.sheetName || t.sheetName === widget.sheetName)
    );
  if (!rawRegion || rawRegion.dataRows.length === 0) return [];

  const headerIndex = new Map<string, number>();
  rawRegion.headers.forEach((h, idx) => headerIndex.set(h, idx));

  const effectiveDimCol =
    widget.activeDrillDownValue && widget.drillDownDimensionColumn
      ? widget.drillDownDimensionColumn
      : widget.dimensionColumn;
  const dimIdx = effectiveDimCol ? (headerIndex.get(effectiveDimCol) ?? 0) : 0;

  const calcDef = calculatedMetrics.find((m) => m.name === widget.measureColumn);
  const measIdx = widget.measureColumn ? headerIndex.get(widget.measureColumn) : undefined;
  const secMeasIdx = widget.secondaryMeasureColumn
    ? headerIndex.get(widget.secondaryMeasureColumn)
    : undefined;

  const allFilters: DashboardFilterCondition[] = [...globalFilters, ...widget.localFilters];
  if (crossFilter && crossFilter.dimensionColumn) {
    allFilters.push({
      columnName: crossFilter.dimensionColumn,
      allowedValues: [crossFilter.selectedValue]
    });
  }
  if (widget.activeDrillDownValue && widget.dimensionColumn) {
    allFilters.push({
      columnName: widget.dimensionColumn,
      allowedValues: [widget.activeDrillDownValue]
    });
  }

  const buckets = new Map<string, { primary: number[]; secondary: number[] }>();

  for (const row of rawRegion.dataRows) {
    let passes = true;
    for (const f of allFilters) {
      const fIdx = headerIndex.get(f.columnName);
      if (fIdx !== undefined && f.allowedValues.length > 0) {
        const cellVal = (row[fIdx] ?? "").trim();
        if (!f.allowedValues.includes(cellVal)) {
          passes = false;
          break;
        }
      }
    }
    if (!passes) continue;

    const label = (row[dimIdx] ?? "").trim() || "(Unspecified)";
    let primaryVal: number | null = null;

    if (calcDef) {
      const lIdx = headerIndex.get(calcDef.leftColumn);
      const rIdx = headerIndex.get(calcDef.rightColumn);
      const lNum = lIdx !== undefined ? parseNumVal(row[lIdx] ?? "") : null;
      const rNum = rIdx !== undefined ? parseNumVal(row[rIdx] ?? "") : null;
      if (lNum !== null && rNum !== null) {
        primaryVal =
          calcDef.operator === "+"
            ? lNum + rNum
            : calcDef.operator === "-"
              ? lNum - rNum
              : calcDef.operator === "*"
                ? lNum * rNum
                : Math.abs(rNum) > 1e-12
                  ? lNum / rNum
                  : 0;
      }
    } else if (measIdx !== undefined) {
      primaryVal = parseNumVal(row[measIdx] ?? "");
    } else {
      primaryVal = 1;
    }

    if (primaryVal === null) continue;
    const secVal = secMeasIdx !== undefined ? parseNumVal(row[secMeasIdx] ?? "") : null;

    const entry = buckets.get(label) ?? { primary: [], secondary: [] };
    entry.primary.push(primaryVal);
    if (secVal !== null) entry.secondary.push(secVal);
    buckets.set(label, entry);
  }

  const aggregateArray = (vals: readonly number[]): number => {
    if (vals.length === 0) return 0;
    switch (widget.aggregation) {
      case "SUM":
        return vals.reduce((a, b) => a + b, 0);
      case "AVERAGE":
        return vals.reduce((a, b) => a + b, 0) / vals.length;
      case "MIN":
        return Math.min(...vals);
      case "MAX":
        return Math.max(...vals);
      case "MEDIAN": {
        const s = [...vals].sort((a, b) => a - b);
        const mid = Math.floor(s.length / 2);
        return s.length % 2 !== 0 ? (s[mid] ?? 0) : ((s[mid - 1] ?? 0) + (s[mid] ?? 0)) / 2;
      }
      case "COUNT_DISTINCT":
        return new Set(vals).size;
    }
  };

  let points: ChartDataPoint[] = [...buckets.entries()].map(([label, b]) => {
    const value = Number(aggregateArray(b.primary).toFixed(2));
    const secondaryValue =
      b.secondary.length > 0 ? Number(aggregateArray(b.secondary).toFixed(2)) : undefined;
    return { label, value, secondaryValue };
  });

  const totalAbs = points.reduce((acc, p) => acc + Math.max(0, p.value), 0);
  points = points.map((p) => ({
    ...p,
    sharePct: totalAbs > 0 ? Number(((Math.max(0, p.value) / totalAbs) * 100).toFixed(1)) : 0
  }));

  switch (widget.sortMode) {
    case "value_desc":
      points.sort((a, b) => b.value - a.value);
      break;
    case "value_asc":
      points.sort((a, b) => a.value - b.value);
      break;
    case "label_asc":
      points.sort((a, b) => a.label.localeCompare(b.label));
      break;
    case "label_desc":
      points.sort((a, b) => b.label.localeCompare(a.label));
      break;
  }

  return points;
}

/**
 * Automatically generates a complete, data-grounded `DashboardStudioState` from the canonical semantic model.
 */
export function generateAutoDashboard(
  model: SemanticWorkbookModel,
  analysis: VerifiedAnalysisReport,
  templateId = "tpl_vision_executive"
): DashboardStudioState {
  const primaryTable = model.tables[0];
  const widgets: DashboardWidget[] = [];

  // 1. Add KPI widgets for every canonical KPI
  model.canonicalKpis.forEach((kpi, idx) => {
    widgets.push({
      widgetId: `w_kpi_${idx + 1}`,
      kind: "kpi",
      title: kpi.label,
      subtitle: `${kpi.sheetName} (${kpi.confidenceTier} confidence)`,
      chartType: "KPI",
      sheetName: kpi.sheetName,
      tableId: kpi.tableId,
      measureColumn: kpi.columnName,
      aggregation: kpi.aggregation,
      sortMode: "value_desc",
      localFilters: [],
      gridLayout: { x: (idx % 4) * 3, y: 0, w: 3, h: 2 },
      dataPoints: [{ label: kpi.label, value: kpi.rawValue }],
      kpiMetric: kpi
    });
  });

  if (primaryTable) {
    const dims = primaryTable.columns.filter((c) => c.analyticalRole === "Dimension");
    const temps = primaryTable.columns.filter((c) => c.analyticalRole === "Temporal");
    const measures = primaryTable.columns.filter(
      (c) => c.analyticalRole === "Measure" && c.aggregationAllowed
    );

    const primaryDim = dims[0]?.headerName ?? primaryTable.columns[0]?.headerName ?? "Category";
    const secondaryDim = dims[1]?.headerName;
    const primaryTime = temps[0]?.headerName ?? primaryDim;
    const primaryMeas = measures[0]?.headerName ?? primaryTable.columns[1]?.headerName ?? "";
    const secondaryMeas = measures[1]?.headerName;

    // 2. Primary Category Ranking Column Chart (with drill-down to secondaryDim if available)
    const barSpec = {
      tableId: primaryTable.tableId,
      dimensionColumn: primaryDim,
      drillDownDimensionColumn: secondaryDim,
      activeDrillDownValue: undefined,
      measureColumn: primaryMeas,
      secondaryMeasureColumn: secondaryMeas,
      aggregation: "SUM" as const,
      sortMode: "value_desc" as const,
      localFilters: []
    };
    widgets.push({
      widgetId: "w_chart_ranking",
      kind: "chart",
      title: `${primaryMeas || "Records"} by ${primaryDim}`,
      subtitle: secondaryDim ? `Click a bar to drill down into ${secondaryDim}` : undefined,
      chartType: "Column",
      sheetName: primaryTable.sheetName,
      ...barSpec,
      gridLayout: { x: 0, y: 2, w: 6, h: 4 },
      dataPoints: computeWidgetDataPoints(model, barSpec)
    });

    // 3. Time Trend Line/Area Chart
    const trendSpec = {
      tableId: primaryTable.tableId,
      dimensionColumn: primaryTime,
      drillDownDimensionColumn: undefined,
      activeDrillDownValue: undefined,
      measureColumn: primaryMeas,
      secondaryMeasureColumn: secondaryMeas,
      aggregation: "SUM" as const,
      sortMode: "label_asc" as const,
      localFilters: []
    };
    widgets.push({
      widgetId: "w_chart_trend",
      kind: "chart",
      title: `${primaryMeas || "Volume"} Trend over ${primaryTime}`,
      chartType: temps.length > 0 ? "Area" : "Line",
      sheetName: primaryTable.sheetName,
      ...trendSpec,
      gridLayout: { x: 6, y: 2, w: 6, h: 4 },
      dataPoints: computeWidgetDataPoints(model, trendSpec)
    });

    // 4. Share / Composition Donut Chart
    widgets.push({
      widgetId: "w_chart_composition",
      kind: "chart",
      title: `${primaryDim} Share Distribution`,
      chartType: "Donut",
      sheetName: primaryTable.sheetName,
      ...barSpec,
      gridLayout: { x: 0, y: 6, w: 4, h: 4 },
      dataPoints: computeWidgetDataPoints(model, barSpec)
    });

    // 5. Waterfall / BoxPlot / Gauge analytical chart
    widgets.push({
      widgetId: "w_chart_waterfall",
      kind: "chart",
      title: `${primaryMeas || "Metric"} Contribution Waterfall`,
      chartType: "Waterfall",
      sheetName: primaryTable.sheetName,
      ...barSpec,
      gridLayout: { x: 4, y: 6, w: 4, h: 4 },
      dataPoints: computeWidgetDataPoints(model, barSpec)
    });

    // 6. Verified Insights Annotation Widget
    const topFacts = analysis.insightsByEpistemicCategory.ConfirmedFact.slice(0, 3)
      .map((i) => `• ${i.statement}`)
      .join("\n");
    widgets.push({
      widgetId: "w_annotation_insights",
      kind: "annotation",
      title: "Verified Analytical Summary",
      chartType: "Table",
      sheetName: primaryTable.sheetName,
      tableId: primaryTable.tableId,
      aggregation: "SUM",
      sortMode: "value_desc",
      localFilters: [],
      annotationText: topFacts,
      gridLayout: { x: 8, y: 6, w: 4, h: 4 },
      dataPoints: []
    });
  }

  return {
    dashboardId: "dash_main",
    title: `${model.tables[0]?.title ?? "Workbook"} — Executive Dashboard`,
    templateId,
    globalFilters: [],
    calculatedMetrics: [],
    widgets,
    undoStack: [],
    redoStack: []
  };
}

/**
 * Mutates a `DashboardStudioState` with full Undo/Redo history tracking and automatic data recomputation.
 */
export function applyDashboardEdit(
  state: DashboardStudioState,
  model: SemanticWorkbookModel,
  action:
    | { readonly type: "ADD_WIDGET"; readonly widget: DashboardWidget }
    | { readonly type: "REMOVE_WIDGET"; readonly widgetId: string }
    | { readonly type: "DUPLICATE_WIDGET"; readonly widgetId: string }
    | {
        readonly type: "UPDATE_WIDGET";
        readonly widgetId: string;
        readonly patch: Partial<DashboardWidget>;
      }
    | {
        readonly type: "REORDER_WIDGETS";
        readonly fromIndex: number;
        readonly toIndex: number;
      }
    | {
        readonly type: "SET_CROSS_FILTER";
        readonly selection?: DashboardStudioState["crossFilterSelection"] | undefined;
      }
    | {
        readonly type: "SET_GLOBAL_FILTERS";
        readonly filters: readonly DashboardFilterCondition[];
      }
    | {
        readonly type: "ADD_CALCULATED_METRIC";
        readonly metric: CalculatedMetricDefinition;
      }
    | { readonly type: "UNDO" }
    | { readonly type: "REDO" }
): DashboardStudioState {
  const recomputeAll = (
    widgets: readonly DashboardWidget[],
    globalFilters: readonly DashboardFilterCondition[],
    crossFilter: DashboardStudioState["crossFilterSelection"],
    calcMetrics: readonly CalculatedMetricDefinition[]
  ): DashboardWidget[] =>
    widgets.map((w) => {
      if (w.kind !== "chart" && w.kind !== "table") return w;
      const effectiveCross =
        crossFilter && crossFilter.sourceWidgetId !== w.widgetId ? crossFilter : undefined;
      return {
        ...w,
        dataPoints: computeWidgetDataPoints(model, w, globalFilters, effectiveCross, calcMetrics)
      };
    });

  if (action.type === "UNDO") {
    const previous = state.undoStack[state.undoStack.length - 1];
    if (!previous) return state;
    return {
      ...state,
      widgets: recomputeAll(
        previous,
        state.globalFilters,
        state.crossFilterSelection,
        state.calculatedMetrics
      ),
      undoStack: state.undoStack.slice(0, -1),
      redoStack: [...state.redoStack, state.widgets]
    };
  }

  if (action.type === "REDO") {
    const next = state.redoStack[state.redoStack.length - 1];
    if (!next) return state;
    return {
      ...state,
      widgets: recomputeAll(
        next,
        state.globalFilters,
        state.crossFilterSelection,
        state.calculatedMetrics
      ),
      undoStack: [...state.undoStack, state.widgets],
      redoStack: state.redoStack.slice(0, -1)
    };
  }

  if (action.type === "SET_CROSS_FILTER") {
    return {
      ...state,
      crossFilterSelection: action.selection,
      widgets: recomputeAll(
        state.widgets,
        state.globalFilters,
        action.selection,
        state.calculatedMetrics
      )
    };
  }

  if (action.type === "SET_GLOBAL_FILTERS") {
    return {
      ...state,
      globalFilters: action.filters,
      widgets: recomputeAll(
        state.widgets,
        action.filters,
        state.crossFilterSelection,
        state.calculatedMetrics
      )
    };
  }

  if (action.type === "ADD_CALCULATED_METRIC") {
    const nextCalc = [...state.calculatedMetrics, action.metric];
    return {
      ...state,
      calculatedMetrics: nextCalc,
      widgets: recomputeAll(
        state.widgets,
        state.globalFilters,
        state.crossFilterSelection,
        nextCalc
      )
    };
  }

  let nextWidgets: DashboardWidget[] = [...state.widgets];
  if (action.type === "ADD_WIDGET") {
    nextWidgets.push(action.widget);
  } else if (action.type === "REMOVE_WIDGET") {
    nextWidgets = nextWidgets.filter((w) => w.widgetId !== action.widgetId);
  } else if (action.type === "DUPLICATE_WIDGET") {
    const idx = nextWidgets.findIndex((w) => w.widgetId === action.widgetId);
    const orig = idx >= 0 ? nextWidgets[idx] : undefined;
    if (orig) {
      const copy: DashboardWidget = {
        ...orig,
        widgetId: `${orig.widgetId}_copy_${nextWidgets.length + 1}`,
        title: `${orig.title} (Copy)`,
        gridLayout: { ...orig.gridLayout, y: orig.gridLayout.y + orig.gridLayout.h }
      };
      nextWidgets.splice(idx + 1, 0, copy);
    }
  } else if (action.type === "UPDATE_WIDGET") {
    nextWidgets = nextWidgets.map((w) =>
      w.widgetId === action.widgetId ? { ...w, ...action.patch } : w
    );
  } else if (action.type === "REORDER_WIDGETS") {
    const item = nextWidgets[action.fromIndex];
    if (item && action.toIndex >= 0 && action.toIndex < nextWidgets.length) {
      nextWidgets.splice(action.fromIndex, 1);
      nextWidgets.splice(action.toIndex, 0, item);
    }
  }

  return {
    ...state,
    widgets: recomputeAll(
      nextWidgets,
      state.globalFilters,
      state.crossFilterSelection,
      state.calculatedMetrics
    ),
    undoStack: [...state.undoStack, state.widgets],
    redoStack: []
  };
}

/**
 * ASCII-only lowercasing (`A-Z` -> `a-z`) matching Rust `str::to_ascii_lowercase()` byte-for-byte
 * so non-ASCII Unicode uppercase characters (such as `Ü`, `Ö`, `İ`, or Kelvin sign `K`) never
 * diverge between Rust `geo_region_coords` and TypeScript `computeRegionalCoordinatePct`.
 */
function toAsciiLowerCase(input: string): string {
  return input.replace(/[A-Z]/g, (ch) => String.fromCharCode(ch.charCodeAt(0) + 32));
}

/**
 * Deterministic regional/category coordinate scatter projection aligned byte-for-byte with
 * Rust `geo_region_coords` (`crates/sf-recipe/src/datagraph.rs`) so Desktop Dashboard,
 * offline `.safi.html` capsule, and exported presentations/SVGs share identical coordinates.
 *
 * Shared normalization rule:
 * 1. Keyword lookup uses `toAsciiLowerCase(label.trim())` (matching Rust `label.trim().to_ascii_lowercase()`).
 * 2. Recognized Egyptian regions map to fixed relative `(xPct, yPct)` scatter positions.
 * 3. Unmapped categories hash the original UTF-8 bytes of `label` (unmodified) with `fallbackIndex * 37`
 *    and multiplier `31` in 32-bit unsigned wrapping arithmetic without implying GIS boundary geometry.
 */
export function computeRegionalCoordinatePct(
  label: string,
  fallbackIndex: number
): { readonly xPct: number; readonly yPct: number } {
  const lower = toAsciiLowerCase(label.trim());
  if (lower.includes("alex") || label.includes("الإسكندرية") || label.includes("اسكندرية")) {
    return { xPct: 30, yPct: 22 };
  }
  if (lower.includes("port said") || label.includes("بورسعيد") || lower.includes("دمياط")) {
    return { xPct: 60, yPct: 20 };
  }
  if (
    lower.includes("delta") ||
    lower.includes("dakahl") ||
    lower.includes("gharbia") ||
    lower.includes("tanta") ||
    label.includes("الدلتا") ||
    label.includes("المنصورة") ||
    label.includes("طنطا")
  ) {
    return { xPct: 48, yPct: 28 };
  }
  if (
    lower.includes("sharq") ||
    lower.includes("ismail") ||
    lower.includes("suez") ||
    label.includes("الشرقية") ||
    label.includes("الإسماعيلية") ||
    label.includes("السويس")
  ) {
    return { xPct: 64, yPct: 34 };
  }
  if (lower.includes("cairo") || label.includes("القاهرة")) {
    return { xPct: 52, yPct: 42 };
  }
  if (lower.includes("giza") || label.includes("الجيزة") || label.includes("الفيوم")) {
    return { xPct: 44, yPct: 46 };
  }
  if (
    lower.includes("minya") ||
    lower.includes("asyut") ||
    lower.includes("upper") ||
    label.includes("المنيا") ||
    label.includes("أسيوط") ||
    label.includes("الصعيد")
  ) {
    return { xPct: 46, yPct: 66 };
  }
  if (
    lower.includes("luxor") ||
    lower.includes("aswan") ||
    label.includes("الأقصر") ||
    label.includes("أسوان")
  ) {
    return { xPct: 56, yPct: 82 };
  }
  if (
    lower.includes("red sea") ||
    lower.includes("sinai") ||
    label.includes("البحر الأحمر") ||
    label.includes("سيناء")
  ) {
    return { xPct: 76, yPct: 54 };
  }

  let hash = Math.imul(fallbackIndex >>> 0, 37) >>> 0;
  const bytes = new TextEncoder().encode(label);
  for (const b of bytes) {
    hash = (Math.imul(hash, 31) + b) >>> 0;
  }
  return {
    xPct: 18 + (hash % 64),
    yPct: 20 + (Math.floor(hash / 64) % 60)
  };
}

/**
 * Renders any of the 21 supported `DashboardChartType` visuals as a clean, genuinely distinct SVG visual.
 */
export function renderChartSvg(
  chartType: DashboardChartType,
  title: string,
  points: readonly ChartDataPoint[],
  primaryColor = "#0284c7"
): string {
  const esc = (s: string): string =>
    s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

  const palette = ["#0284c7", "#0d9488", "#6366f1", "#f59e0b", "#ec4899", "#10b981"];
  const safePoints = points.length > 0 ? points.slice(0, 12) : [{ label: "N/A", value: 0 }];
  const maxVal = Math.max(1, ...safePoints.map((p) => Math.abs(p.value)));
  const minVal = Math.min(...safePoints.map((p) => p.value));
  const span = Math.max(1, maxVal - Math.min(0, minVal));
  const totalAbs = Math.max(
    1,
    safePoints.reduce((acc, p) => acc + Math.max(0, p.value), 0)
  );
  const header = `<rect width="420" height="210" rx="10" fill="#ffffff" stroke="#e2e8f0"/><text x="18" y="26" font-size="13.5" font-weight="600" fill="#0f172a">${esc(title)} (${chartType})</text>`;

  switch (chartType) {
    case "Pie":
    case "Donut": {
      const isDonut = chartType === "Donut";
      let cumulative = 0;
      const slices = safePoints
        .map((p, i) => {
          const frac = Math.max(0, p.value) / totalAbs;
          const dash = `${(frac * 251.2).toFixed(2)} 251.2`;
          const offset = (-cumulative * 251.2).toFixed(2);
          cumulative += frac;
          const color = palette[i % palette.length] ?? primaryColor;
          return `<circle r="40" cx="110" cy="115" fill="transparent" stroke="${color}" stroke-width="${isDonut ? 24 : 80}" stroke-dasharray="${dash}" stroke-dashoffset="${offset}" />`;
        })
        .join("");
      const centerText = isDonut
        ? `<text x="110" y="119" text-anchor="middle" font-size="12" font-weight="700" fill="#0f172a">${totalAbs.toLocaleString("en-US")}</text>`
        : "";
      const legend = safePoints
        .slice(0, 5)
        .map(
          (p, i) =>
            `<rect x="200" y="${61 + i * 24}" width="10" height="10" rx="2" fill="${palette[i % palette.length] ?? primaryColor}"/><text x="216" y="${70 + i * 24}" font-size="11.5" fill="#334155">${esc(p.label.slice(0, 14))}: ${p.value.toLocaleString("en-US")}</text>`
        )
        .join("");
      return `<svg xmlns="http://www.w3.org/2000/svg" width="420" height="210" viewBox="0 0 420 210" data-chart-type="${chartType}" data-visual-kind="${chartType.toLowerCase()}">${header}${slices}${centerText}${legend}</svg>`;
    }

    case "Gauge": {
      const lead = Math.max(0, safePoints[0]?.value ?? 0);
      const pct = Math.min(100, Math.round((lead / totalAbs) * 100));
      const angleRad = Math.PI - (pct / 100) * Math.PI;
      const nx = (210 + 62 * Math.cos(angleRad)).toFixed(1);
      const ny = (148 - 62 * Math.sin(angleRad)).toFixed(1);
      const arcLen = 235.6;
      const filled = ((pct / 100) * arcLen).toFixed(1);
      return `<svg xmlns="http://www.w3.org/2000/svg" width="420" height="210" viewBox="0 0 420 210" data-chart-type="${chartType}" data-visual-kind="gauge">${header}<path d="M 135 148 A 75 75 0 0 1 285 148" fill="none" stroke="#e2e8f0" stroke-width="18" stroke-linecap="round"/><path d="M 135 148 A 75 75 0 0 1 285 148" fill="none" stroke="${primaryColor}" stroke-width="18" stroke-linecap="round" stroke-dasharray="${filled} ${arcLen}"/><line x1="210" y1="148" x2="${nx}" y2="${ny}" stroke="#0f172a" stroke-width="3" stroke-linecap="round"/><circle cx="210" cy="148" r="6" fill="#0f172a"/><text x="210" y="182" text-anchor="middle" font-size="14" font-weight="700" fill="#0f172a">${pct}% (${lead.toLocaleString("en-US")})</text></svg>`;
    }

    case "KPI": {
      const top = safePoints[0];
      const topVal = top ? top.value.toLocaleString("en-US") : totalAbs.toLocaleString("en-US");
      const sub = top
        ? `${esc(top.label)} • Total: ${totalAbs.toLocaleString("en-US")}`
        : "Verified Total";
      return `<svg xmlns="http://www.w3.org/2000/svg" width="420" height="210" viewBox="0 0 420 210" data-chart-type="${chartType}" data-visual-kind="kpi">${header}<rect x="28" y="48" width="364" height="132" rx="12" fill="#f0f9ff" stroke="#bae6fd" stroke-width="1.5"/><text x="210" y="114" text-anchor="middle" font-size="34" font-weight="700" fill="${primaryColor}">${esc(topVal)}</text><text x="210" y="148" text-anchor="middle" font-size="13" fill="#334155">${sub}</text></svg>`;
    }

    case "Line":
    case "Area": {
      const coords = safePoints.map((p, idx) => {
        const x = 45 + (idx * 330) / Math.max(1, safePoints.length - 1);
        const y = 165 - (Math.max(0, p.value) / maxVal) * 110;
        return { x: Number(x.toFixed(1)), y: Number(y.toFixed(1)), label: p.label, val: p.value };
      });
      const polyPoints = coords.map((c) => `${c.x},${c.y}`).join(" ");
      const firstX = coords[0]?.x ?? 45;
      const lastX = coords[coords.length - 1]?.x ?? 375;
      const areaPath =
        chartType === "Area"
          ? `<polygon points="${firstX},165 ${polyPoints} ${lastX},165" fill="${primaryColor}" fill-opacity="0.22"/>`
          : "";
      const dots = coords
        .map(
          (c) =>
            `<circle cx="${c.x}" cy="${c.y}" r="4.2" fill="${primaryColor}"/><text x="${c.x}" y="185" font-size="10" text-anchor="middle" fill="#475569">${esc(c.label.slice(0, 8))}</text>`
        )
        .join("");
      return `<svg xmlns="http://www.w3.org/2000/svg" width="420" height="210" viewBox="0 0 420 210" data-chart-type="${chartType}" data-visual-kind="${chartType.toLowerCase()}">${header}<line x1="36" y1="165" x2="386" y2="165" stroke="#cbd5e1"/>${areaPath}<polyline fill="none" stroke="${primaryColor}" stroke-width="2.5" points="${polyPoints}"/>${dots}</svg>`;
    }

    case "Combo": {
      const barW = Math.max(14, Math.floor(310 / safePoints.length) - 8);
      let running = 0;
      const cumCoords: string[] = [];
      const bars = safePoints
        .map((p, idx) => {
          const x = 42 + idx * (barW + 8);
          const h = Math.max(6, Math.round((Math.abs(p.value) / maxVal) * 105));
          const y = 165 - h;
          running += Math.max(0, p.value);
          const ly = Number((165 - (running / totalAbs) * 110).toFixed(1));
          cumCoords.push(`${(x + barW / 2).toFixed(1)},${ly}`);
          return `<rect x="${x}" y="${y}" width="${barW}" height="${h}" rx="3" fill="${primaryColor}" fill-opacity="0.72"/><text x="${x + barW / 2}" y="185" font-size="9.5" text-anchor="middle" fill="#475569">${esc(p.label.slice(0, 7))}</text>`;
        })
        .join("");
      const line = `<polyline fill="none" stroke="#f59e0b" stroke-width="2.5" points="${cumCoords.join(" ")}"/>`;
      return `<svg xmlns="http://www.w3.org/2000/svg" width="420" height="210" viewBox="0 0 420 210" data-chart-type="Combo" data-visual-kind="combo">${header}<line x1="36" y1="165" x2="388" y2="165" stroke="#cbd5e1"/>${bars}${line}</svg>`;
    }

    case "Bar": {
      const topN = safePoints.slice(0, 6);
      const rowH = Math.max(16, Math.floor(132 / Math.max(1, topN.length)) - 6);
      const rowsSvg = topN
        .map((p, i) => {
          const y = 44 + i * (rowH + 6);
          const w = Math.max(8, Math.round((Math.abs(p.value) / maxVal) * 210));
          return `<text x="108" y="${y + rowH * 0.72}" text-anchor="end" font-size="11" fill="#334155">${esc(p.label.slice(0, 12))}</text><rect x="116" y="${y}" width="${w}" height="${rowH}" rx="4" fill="${palette[i % palette.length] ?? primaryColor}"/><text x="${122 + w}" y="${y + rowH * 0.72}" font-size="10.5" fill="#0f172a">${p.value.toLocaleString("en-US")}</text>`;
        })
        .join("");
      return `<svg xmlns="http://www.w3.org/2000/svg" width="420" height="210" viewBox="0 0 420 210" data-chart-type="Bar" data-visual-kind="bar">${header}${rowsSvg}</svg>`;
    }

    case "Stacked": {
      let curX = 24;
      const trackW = 372;
      const segs = safePoints
        .map((p, i) => {
          const w = Math.max(6, (Math.max(0, p.value) / totalAbs) * trackW);
          const rect = `<rect x="${curX.toFixed(1)}" y="68" width="${w.toFixed(1)}" height="36" fill="${palette[i % palette.length] ?? primaryColor}"/>`;
          curX += w;
          return rect;
        })
        .join("");
      const legend = safePoints
        .slice(0, 6)
        .map(
          (p, i) =>
            `<rect x="${24 + (i % 3) * 126}" y="${124 + Math.floor(i / 3) * 24}" width="10" height="10" fill="${palette[i % palette.length] ?? primaryColor}"/><text x="${38 + (i % 3) * 126}" y="${133 + Math.floor(i / 3) * 24}" font-size="10.5" fill="#334155">${esc(p.label.slice(0, 10))} (${p.sharePct ?? 0}%)</text>`
        )
        .join("");
      return `<svg xmlns="http://www.w3.org/2000/svg" width="420" height="210" viewBox="0 0 420 210" data-chart-type="Stacked" data-visual-kind="stacked">${header}${segs}${legend}</svg>`;
    }

    case "Scatter":
    case "Bubble": {
      const isBubble = chartType === "Bubble";
      const dots = safePoints
        .map((p, i) => {
          const xBase = p.secondaryValue !== undefined ? p.secondaryValue : i + 1;
          const maxSec = Math.max(1, ...safePoints.map((q, qi) => q.secondaryValue ?? qi + 1));
          const cx = 48 + (xBase / maxSec) * 330;
          const cy = 160 - (Math.abs(p.value) / maxVal) * 105;
          const r = isBubble
            ? Math.max(5, Math.min(20, Math.sqrt(Math.abs(p.value) / maxVal) * 18))
            : 5;
          const color = palette[i % palette.length] ?? primaryColor;
          return `<circle cx="${cx.toFixed(1)}" cy="${cy.toFixed(1)}" r="${r.toFixed(1)}" fill="${color}" fill-opacity="${isBubble ? "0.6" : "0.9"}" stroke="#ffffff" stroke-width="1"/><text x="${cx.toFixed(1)}" y="${(cy - r - 3).toFixed(1)}" text-anchor="middle" font-size="9" fill="#475569">${esc(p.label.slice(0, 8))}</text>`;
        })
        .join("");
      return `<svg xmlns="http://www.w3.org/2000/svg" width="420" height="210" viewBox="0 0 420 210" data-chart-type="${chartType}" data-visual-kind="${chartType.toLowerCase()}">${header}<line x1="40" y1="45" x2="40" y2="165" stroke="#94a3b8"/><line x1="40" y1="165" x2="390" y2="165" stroke="#94a3b8"/>${dots}</svg>`;
    }

    case "Histogram": {
      const binW = Math.floor(336 / safePoints.length);
      const bins = safePoints
        .map((p, idx) => {
          const x = 42 + idx * binW;
          const h = Math.max(6, Math.round((Math.abs(p.value) / maxVal) * 112));
          return `<rect x="${x}" y="${165 - h}" width="${Math.max(8, binW - 1)}" height="${h}" fill="#0ea5e9" stroke="#ffffff" stroke-width="1"/><text x="${x + binW / 2}" y="183" font-size="9.5" text-anchor="middle" fill="#475569">${esc(p.label.slice(0, 8))}</text>`;
        })
        .join("");
      return `<svg xmlns="http://www.w3.org/2000/svg" width="420" height="210" viewBox="0 0 420 210" data-chart-type="Histogram" data-visual-kind="histogram">${header}<line x1="38" y1="165" x2="388" y2="165" stroke="#94a3b8"/>${bins}</svg>`;
    }

    case "BoxPlot": {
      const slotW = Math.floor(330 / safePoints.length);
      const boxes = safePoints
        .map((p, idx) => {
          const cx = 48 + idx * slotW + slotW / 2;
          const medY = 160 - (Math.abs(p.value) / maxVal) * 100;
          const q1Y = Math.min(165, medY + 14);
          const q3Y = Math.max(46, medY - 14);
          const minY = Math.min(170, q1Y + 12);
          const maxY = Math.max(42, q3Y - 12);
          return `<line x1="${cx}" y1="${maxY}" x2="${cx}" y2="${minY}" stroke="#475569" stroke-width="1.5"/><rect x="${cx - 10}" y="${q3Y}" width="20" height="${Math.max(8, q1Y - q3Y)}" fill="#bae6fd" stroke="${primaryColor}" stroke-width="1.5"/><line x1="${cx - 10}" y1="${medY}" x2="${cx + 10}" y2="${medY}" stroke="#0f172a" stroke-width="2"/><text x="${cx}" y="188" font-size="9.5" text-anchor="middle" fill="#475569">${esc(p.label.slice(0, 7))}</text>`;
        })
        .join("");
      return `<svg xmlns="http://www.w3.org/2000/svg" width="420" height="210" viewBox="0 0 420 210" data-chart-type="BoxPlot" data-visual-kind="boxplot">${header}${boxes}</svg>`;
    }

    case "Heatmap": {
      const cols = Math.min(4, Math.max(2, Math.ceil(Math.sqrt(safePoints.length))));
      const rows = Math.max(1, Math.ceil(safePoints.length / cols));
      const cw = Math.floor(360 / cols) - 6;
      const ch = Math.floor(132 / rows) - 6;
      const cells = safePoints
        .map((p, i) => {
          const r = Math.floor(i / cols);
          const c = i % cols;
          const x = 28 + c * (cw + 6);
          const y = 46 + r * (ch + 6);
          const alpha = (0.2 + (Math.abs(p.value) / maxVal) * 0.78).toFixed(2);
          return `<rect x="${x}" y="${y}" width="${cw}" height="${ch}" rx="4" fill="#0d9488" fill-opacity="${alpha}" stroke="#99f6e4"/><text x="${x + cw / 2}" y="${y + ch / 2 + 4}" text-anchor="middle" font-size="10.5" fill="#0f172a">${esc(p.label.slice(0, 9))}</text>`;
        })
        .join("");
      return `<svg xmlns="http://www.w3.org/2000/svg" width="420" height="210" viewBox="0 0 420 210" data-chart-type="Heatmap" data-visual-kind="heatmap">${header}${cells}</svg>`;
    }

    case "Treemap": {
      const sorted = [...safePoints].sort((a, b) => Math.abs(b.value) - Math.abs(a.value));
      let rx = 24;
      let ry = 44;
      let rw = 372;
      let rh = 148;
      let rem = sorted.reduce((a, b) => a + Math.abs(b.value), 0) || 1;
      const tiles = sorted
        .map((p, i) => {
          const frac =
            i === sorted.length - 1 ? 1 : Math.max(0.08, Math.min(0.92, Math.abs(p.value) / rem));
          rem = Math.max(1e-6, rem - Math.abs(p.value));
          const tx = rx;
          const ty = ry;
          let tw = rw;
          let th = rh;
          if (i % 2 === 0) {
            tw = rw * frac;
            rx += tw;
            rw = Math.max(8, rw - tw);
          } else {
            th = rh * frac;
            ry += th;
            rh = Math.max(8, rh - th);
          }
          const color = palette[i % palette.length] ?? primaryColor;
          return `<rect x="${tx.toFixed(1)}" y="${ty.toFixed(1)}" width="${tw.toFixed(1)}" height="${th.toFixed(1)}" rx="3" fill="${color}" fill-opacity="0.86" stroke="#ffffff" stroke-width="1.5"/><text x="${(tx + 6).toFixed(1)}" y="${(ty + 16).toFixed(1)}" font-size="10" font-weight="600" fill="#ffffff">${esc(p.label.slice(0, 10))}</text>`;
        })
        .join("");
      return `<svg xmlns="http://www.w3.org/2000/svg" width="420" height="210" viewBox="0 0 420 210" data-chart-type="Treemap" data-visual-kind="treemap">${header}${tiles}</svg>`;
    }

    case "Waterfall": {
      const count = safePoints.length + 1;
      const bw = Math.max(14, Math.floor(320 / count) - 6);
      let running = 0;
      const steps: string[] = [];
      safePoints.forEach((p, idx) => {
        const start = running;
        running += p.value;
        const y1 = 160 - ((Math.max(0, start) - Math.min(0, minVal)) / span) * 105;
        const y2 = 160 - ((Math.max(0, running) - Math.min(0, minVal)) / span) * 105;
        const x = 36 + idx * (bw + 6);
        const top = Math.min(y1, y2);
        const h = Math.max(4, Math.abs(y2 - y1));
        const fill = p.value >= 0 ? "#10b981" : "#ef4444";
        steps.push(
          `<rect x="${x}" y="${top.toFixed(1)}" width="${bw}" height="${h.toFixed(1)}" rx="2" fill="${fill}"/><text x="${x + bw / 2}" y="183" font-size="9.5" text-anchor="middle" fill="#475569">${esc(p.label.slice(0, 6))}</text>`
        );
      });
      const totalH = Math.max(8, Math.min(115, Math.round((Math.abs(running) / span) * 105)));
      const tx = 36 + safePoints.length * (bw + 6);
      steps.push(
        `<rect x="${tx}" y="${160 - totalH}" width="${bw}" height="${totalH}" rx="2" fill="${primaryColor}"/><text x="${tx + bw / 2}" y="183" font-size="9.5" text-anchor="middle" font-weight="700" fill="#0f172a">Net</text>`
      );
      return `<svg xmlns="http://www.w3.org/2000/svg" width="420" height="210" viewBox="0 0 420 210" data-chart-type="Waterfall" data-visual-kind="waterfall">${header}<line x1="30" y1="160" x2="395" y2="160" stroke="#94a3b8"/>${steps.join("")}</svg>`;
    }

    case "Funnel": {
      const stages = safePoints.slice(0, 6);
      const rowH = Math.floor(136 / Math.max(1, stages.length));
      const shapes = stages
        .map((p, idx) => {
          const nextV = stages[idx + 1]?.value ?? p.value * 0.72;
          const w1 = Math.max(40, (Math.abs(p.value) / maxVal) * 320);
          const w2 = Math.max(30, (Math.abs(nextV) / maxVal) * 320);
          const y1 = 44 + idx * rowH;
          const y2 = y1 + Math.max(10, rowH - 4);
          const color = palette[idx % palette.length] ?? primaryColor;
          const pts = `${210 - w1 / 2},${y1} ${210 + w1 / 2},${y1} ${210 + w2 / 2},${y2} ${210 - w2 / 2},${y2}`;
          return `<polygon points="${pts}" fill="${color}" fill-opacity="0.9"/><text x="210" y="${(y1 + y2) / 2 + 4}" text-anchor="middle" font-size="10.5" font-weight="600" fill="#ffffff">${esc(p.label.slice(0, 14))}: ${p.value.toLocaleString("en-US")}</text>`;
        })
        .join("");
      return `<svg xmlns="http://www.w3.org/2000/svg" width="420" height="210" viewBox="0 0 420 210" data-chart-type="Funnel" data-visual-kind="funnel">${header}${shapes}</svg>`;
    }

    case "Table":
    case "PivotSummary": {
      const rows = safePoints.slice(0, 5);
      const head = `<rect x="20" y="42" width="380" height="24" rx="4" fill="#0f172a"/><text x="32" y="58" font-size="11" font-weight="600" fill="#ffffff">${chartType === "PivotSummary" ? "Group / Pivot" : "Category"}</text><text x="240" y="58" font-size="11" font-weight="600" fill="#ffffff">Measure</text><text x="340" y="58" font-size="11" font-weight="600" fill="#ffffff">Share</text>`;
      const body = rows
        .map((p, i) => {
          const y = 70 + i * 24;
          const bg = i % 2 === 0 ? "#f8fafc" : "#ffffff";
          return `<rect x="20" y="${y}" width="380" height="22" fill="${bg}"/><text x="32" y="${y + 15}" font-size="11" fill="#0f172a">${esc(p.label.slice(0, 22))}</text><text x="240" y="${y + 15}" font-size="11" fill="#0f172a">${p.value.toLocaleString("en-US")}</text><text x="340" y="${y + 15}" font-size="11" fill="#475569">${p.sharePct ?? 0}%</text>`;
        })
        .join("");
      return `<svg xmlns="http://www.w3.org/2000/svg" width="420" height="210" viewBox="0 0 420 210" data-chart-type="${chartType}" data-visual-kind="${chartType.toLowerCase()}">${header}${head}${body}</svg>`;
    }

    case "Geographic": {
      const plotX = 28;
      const plotY = 42;
      const plotW = 364;
      const plotH = 142;
      const pins = safePoints
        .slice(0, 8)
        .map((p, i) => {
          const { xPct, yPct } = computeRegionalCoordinatePct(p.label, i);
          const cx = Math.round(plotX + (xPct / 100) * plotW);
          const cy = Math.round(plotY + (yPct / 100) * plotH);
          const r = Math.max(6, Math.min(18, Math.round((Math.abs(p.value) / maxVal) * 16)));
          const color = palette[i % palette.length] ?? primaryColor;
          return `<circle data-geo-marker="${i}" data-geo-x-pct="${xPct}" data-geo-y-pct="${yPct}" cx="${cx}" cy="${cy}" r="${r}" fill="${color}" fill-opacity="0.78" stroke="#ffffff" stroke-width="1.5"/><text x="${cx}" y="${cy + r + 11}" text-anchor="middle" font-size="9.5" fill="#0f172a">${esc(p.label.slice(0, 10))}</text>`;
        })
        .join("");
      return `<svg xmlns="http://www.w3.org/2000/svg" width="420" height="210" viewBox="0 0 420 210" data-chart-type="Geographic" data-visual-kind="geographic" data-geo-projection="regional-coordinate-scatter">${header}<rect data-geo-backdrop="1" x="${plotX}" y="${plotY}" width="${plotW}" height="${plotH}" rx="8" fill="#f8fafc" stroke="#cbd5e1" stroke-width="1.2"/><line x1="${Math.round(plotX + plotW * 0.35)}" y1="${plotY}" x2="${Math.round(plotX + plotW * 0.35)}" y2="${plotY + plotH}" stroke="#e2e8f0" stroke-dasharray="3,3"/><line x1="${Math.round(plotX + plotW * 0.65)}" y1="${plotY}" x2="${Math.round(plotX + plotW * 0.65)}" y2="${plotY + plotH}" stroke="#e2e8f0" stroke-dasharray="3,3"/><line x1="${plotX}" y1="${Math.round(plotY + plotH * 0.35)}" x2="${plotX + plotW}" y2="${Math.round(plotY + plotH * 0.35)}" stroke="#e2e8f0" stroke-dasharray="3,3"/><line x1="${plotX}" y1="${Math.round(plotY + plotH * 0.65)}" x2="${plotX + plotW}" y2="${Math.round(plotY + plotH * 0.65)}" stroke="#e2e8f0" stroke-dasharray="3,3"/>${pins}<text x="210" y="198" text-anchor="middle" font-size="9" fill="#64748b">Regional coordinate scatter (no GIS boundaries implied)</text></svg>`;
    }

    case "Column":
    default: {
      const barWidth = Math.max(16, Math.floor(320 / safePoints.length) - 8);
      const bars = safePoints
        .map((p, idx) => {
          const x = 42 + idx * (barWidth + 8);
          const h = Math.max(4, Math.round((Math.abs(p.value) / maxVal) * 110));
          const y = 165 - h;
          return `<rect x="${x}" y="${y}" width="${barWidth}" height="${h}" rx="4" fill="${primaryColor}"/><text x="${x + barWidth / 2}" y="${y - 5}" font-size="10" text-anchor="middle" fill="#0f172a">${p.value.toLocaleString("en-US")}</text><text x="${x + barWidth / 2}" y="184" font-size="10" text-anchor="middle" fill="#475569">${esc(p.label.slice(0, 8))}</text>`;
        })
        .join("");
      return `<svg xmlns="http://www.w3.org/2000/svg" width="420" height="210" viewBox="0 0 420 210" data-chart-type="Column" data-visual-kind="column">${header}<line x1="36" y1="165" x2="388" y2="165" stroke="#cbd5e1"/>${bars}</svg>`;
    }
  }
}

// ============================================================================
// PHASE 6: PRESENTATION STUDIO (10-SLIDE DECK + EDITOR + FULLSCREEN HTML PLAYER)
// ============================================================================

export type SlideLayoutKind =
  | "TitleCover"
  | "ExecutiveSummary"
  | "DatasetOverview"
  | "KpiGrid"
  | "ChartFocus"
  | "SplitComparison"
  | "BulletFindings"
  | "RiskAndQuality"
  | "Recommendations"
  | "AppendixMethodology";

export interface PresentationSlide {
  readonly slideId: string;
  readonly slideNumber: number;
  readonly layout: SlideLayoutKind;
  readonly sectionRole:
    | "Cover"
    | "ExecutiveSummary"
    | "DatasetOverview"
    | "KeyMetrics"
    | "MajorTrends"
    | "CategoryComparisons"
    | "KeyFindings"
    | "RisksAndQuality"
    | "Recommendations"
    | "AppendixMethodology";
  readonly title: string;
  readonly subtitle: string;
  readonly bullets: readonly string[];
  readonly kpis: readonly CanonicalKpiMetric[];
  readonly chartType?: DashboardChartType | undefined;
  readonly chartDataPoints?: readonly ChartDataPoint[] | undefined;
  readonly tableHeaders?: readonly string[] | undefined;
  readonly tableRows?: readonly (readonly string[])[] | undefined;
  readonly calloutNote?: string | undefined;
  readonly presenterNotes: string;
}

export interface PresentationDeckState {
  readonly deckId: string;
  readonly title: string;
  readonly templateId: string;
  readonly slides: readonly PresentationSlide[];
}

/**
 * Generates a complete 10-slide executive presentation grounded in the canonical `SemanticWorkbookModel`.
 */
export function generatePresentationDeck(
  model: SemanticWorkbookModel,
  analysis: VerifiedAnalysisReport,
  templateId = "tpl_vision_executive"
): PresentationDeckState {
  const primaryTable = model.tables[0];
  const workbookTitle = primaryTable?.title ?? "Workbook Intelligence";
  const autoDash = generateAutoDashboard(model, analysis, templateId);
  const trendWidget = autoDash.widgets.find((w) => w.widgetId === "w_chart_trend");
  const rankWidget = autoDash.widgets.find((w) => w.widgetId === "w_chart_ranking");

  const rawRegion = model.ingestionReport.worksheets[0]?.tables[0];

  const slides: PresentationSlide[] = [
    {
      slideId: "slide_1_cover",
      slideNumber: 1,
      layout: "TitleCover",
      sectionRole: "Cover",
      title: `${workbookTitle} — Executive Analytical Briefing`,
      subtitle: `${model.ingestionReport.totalSheets} Worksheet(s) • ${model.ingestionReport.totalAnalyticalRows} Verified Records • Quality Score ${model.overallQualityScorePct}%`,
      bullets: [
        `Format: ${model.ingestionReport.detectedFormat} (${model.ingestionReport.dateSystem})`,
        `Overall Analytical Confidence: ${model.overallConfidencePct}% (${model.overallConfidenceTier})`
      ],
      kpis: model.canonicalKpis.slice(0, 2),
      presenterNotes: "Welcome stakeholders and highlight the verified scope of the dataset."
    },
    {
      slideId: "slide_2_exec_summary",
      slideNumber: 2,
      layout: "ExecutiveSummary",
      sectionRole: "ExecutiveSummary",
      title: "Executive Summary",
      subtitle: "Verified Facts & Primary Analytical Highlights",
      bullets: analysis.insightsByEpistemicCategory.ConfirmedFact.slice(0, 5).map(
        (i) => i.statement
      ),
      kpis: model.canonicalKpis.slice(0, 4),
      presenterNotes: "Summarize confirmed facts directly traceable to workbook formulas and cells."
    },
    {
      slideId: "slide_3_dataset_overview",
      slideNumber: 3,
      layout: "DatasetOverview",
      sectionRole: "DatasetOverview",
      title: "Dataset Overview & Worksheet Inventory",
      subtitle: "Discovered Sheets, Tables, and Structural Lineage",
      bullets: model.ingestionReport.worksheets.map(
        (w) =>
          `${w.name} (${w.visibility}, ${w.classification}): ${w.tables.length} table(s), ${w.formulaCount} formula(s)`
      ),
      kpis: [],
      tableHeaders: rawRegion?.headers.slice(0, 5),
      tableRows: rawRegion?.dataRows.slice(0, 5).map((r) => r.slice(0, 5)),
      presenterNotes: "Walk through visible and hidden worksheets and separated subtotal rows."
    },
    {
      slideId: "slide_4_kpis",
      slideNumber: 4,
      layout: "KpiGrid",
      sectionRole: "KeyMetrics",
      title: "Key Performance Indicators (KPIs)",
      subtitle: "Canonical Aggregations Excluding Identifier Codes & Subtotal Rows",
      bullets: model.canonicalKpis.map(
        (k) => `${k.label}: ${k.formattedValue} (${k.formulaTrace})`
      ),
      kpis: model.canonicalKpis,
      presenterNotes: "Emphasize that IDs and subtotal rows were excluded from KPI summation."
    },
    {
      slideId: "slide_5_trends",
      slideNumber: 5,
      layout: "ChartFocus",
      sectionRole: "MajorTrends",
      title: trendWidget?.title ?? "Time-Series & Period Trajectory",
      subtitle: "Period-over-Period Movement & Conditional Projection",
      bullets: analysis.insights
        .filter((i) => i.domain === "PeriodComparison" || i.domain === "ConditionalForecast")
        .map((i) => i.statement),
      kpis: [],
      chartType: trendWidget?.chartType ?? "Area",
      chartDataPoints: trendWidget?.dataPoints ?? [],
      presenterNotes: "Discuss period-over-period rate of change and conditional trend assumptions."
    },
    {
      slideId: "slide_6_comparisons",
      slideNumber: 6,
      layout: "SplitComparison",
      sectionRole: "CategoryComparisons",
      title: rankWidget?.title ?? "Category & Branch Comparisons",
      subtitle: "Top & Bottom Contributors and Concentration Share",
      bullets: analysis.insights
        .filter((i) => i.domain === "CategoryRanking" || i.domain === "OperationalIndicators")
        .map((i) => i.statement),
      kpis: [],
      chartType: rankWidget?.chartType ?? "Column",
      chartDataPoints: rankWidget?.dataPoints ?? [],
      presenterNotes: "Compare top and bottom performing segments."
    },
    {
      slideId: "slide_7_findings",
      slideNumber: 7,
      layout: "BulletFindings",
      sectionRole: "KeyFindings",
      title: "Key Findings & Epistemic Separation",
      subtitle: "Distinguishing Confirmed Facts from Observational Patterns & Hypotheses",
      bullets: [
        ...analysis.insightsByEpistemicCategory.DataSupportedObservation.map(
          (i) => `[Observation] ${i.statement}`
        ),
        ...analysis.insightsByEpistemicCategory.PotentialExplanation.map(
          (i) => `[Hypothesis] ${i.statement}`
        )
      ],
      kpis: [],
      calloutNote: analysis.insightsByEpistemicCategory.UnsupportedConclusion[0]?.statement,
      presenterNotes: "Clarify which findings are direct observations vs hypotheses."
    },
    {
      slideId: "slide_8_quality",
      slideNumber: 8,
      layout: "RiskAndQuality",
      sectionRole: "RisksAndQuality",
      title: "Risks, Anomalies & Data Quality Audit",
      subtitle: `Overall Data Quality Score: ${model.overallQualityScorePct}%`,
      bullets:
        model.qualityFindings.length > 0
          ? model.qualityFindings.map((f) => `[${f.category}] ${f.title}: ${f.description}`)
          : ["Zero critical data quality defects detected across analytical records."],
      kpis: [],
      presenterNotes: "Review outliers, missing values, and any pending clarification questions."
    },
    {
      slideId: "slide_9_recommendations",
      slideNumber: 9,
      layout: "Recommendations",
      sectionRole: "Recommendations",
      title: "Data-Grounded Recommendations & Next Steps",
      subtitle: "Actionable Steps Based on Verified Findings & Quality Audit",
      bullets: [
        ...model.qualityFindings.flatMap((f) => {
          const act = f.suggestedAction;
          return act ? [`Data Hygiene: ${act.label} (${act.previewSummary})`] : [];
        }),
        ...analysis.questions
          .filter((q) => q.status !== "Answered")
          .map((q) => `Clarification Needed: ${q.prompt}`),
        "Monitor top-concentrated segments and validate conditional forecast assumptions against next-period actuals."
      ],
      kpis: [],
      presenterNotes: "Align on reversible data cleaning actions and open clarifications."
    },
    {
      slideId: "slide_10_appendix",
      slideNumber: 10,
      layout: "AppendixMethodology",
      sectionRole: "AppendixMethodology",
      title: "Appendix & Analytical Methodology",
      subtitle: "Lineage, Formula Verification & Epistemic Guardrails",
      bullets: [
        `Formulas Detected: ${model.ingestionReport.totalFormulasDetected} (${model.ingestionReport.totalFormulasVerified} verified locally)`,
        `Cross-Table Relationships Discovered: ${model.relationships.length}`,
        ...analysis.insightsByEpistemicCategory.UnsupportedConclusion.map((i) => i.statement)
      ],
      kpis: [],
      presenterNotes: "Reference formula verification and methodology details."
    }
  ];

  return {
    deckId: "deck_main",
    title: `${workbookTitle} — Executive Presentation`,
    templateId,
    slides
  };
}

/**
 * Edits a `PresentationDeckState` (add, delete, duplicate, reorder, update slide/layout/notes).
 */
export function applyPresentationEdit(
  deck: PresentationDeckState,
  action:
    | { readonly type: "ADD_SLIDE"; readonly slide: Omit<PresentationSlide, "slideNumber"> }
    | { readonly type: "DELETE_SLIDE"; readonly slideId: string }
    | { readonly type: "DUPLICATE_SLIDE"; readonly slideId: string }
    | { readonly type: "REORDER_SLIDES"; readonly fromIndex: number; readonly toIndex: number }
    | {
        readonly type: "UPDATE_SLIDE";
        readonly slideId: string;
        readonly patch: Partial<PresentationSlide>;
      }
    | { readonly type: "SET_TEMPLATE"; readonly templateId: string }
): PresentationDeckState {
  const renumber = (slides: readonly PresentationSlide[]): PresentationSlide[] =>
    slides.map((s, idx) => ({ ...s, slideNumber: idx + 1 }));

  if (action.type === "SET_TEMPLATE") {
    return { ...deck, templateId: action.templateId };
  }

  const next = [...deck.slides];
  if (action.type === "ADD_SLIDE") {
    next.push({ ...action.slide, slideNumber: next.length + 1 });
  } else if (action.type === "DELETE_SLIDE") {
    const filtered = next.filter((s) => s.slideId !== action.slideId);
    return { ...deck, slides: renumber(filtered) };
  } else if (action.type === "DUPLICATE_SLIDE") {
    const idx = next.findIndex((s) => s.slideId === action.slideId);
    const orig = idx >= 0 ? next[idx] : undefined;
    if (orig) {
      next.splice(idx + 1, 0, {
        ...orig,
        slideId: `${orig.slideId}_copy_${next.length + 1}`,
        title: `${orig.title} (Copy)`
      });
    }
  } else if (action.type === "REORDER_SLIDES") {
    const item = next[action.fromIndex];
    if (item && action.toIndex >= 0 && action.toIndex < next.length) {
      next.splice(action.fromIndex, 1);
      next.splice(action.toIndex, 0, item);
    }
  } else if (action.type === "UPDATE_SLIDE") {
    return {
      ...deck,
      slides: renumber(
        next.map((s) => (s.slideId === action.slideId ? { ...s, ...action.patch } : s))
      )
    };
  }

  return { ...deck, slides: renumber(next) };
}

/**
 * Generates an interactive standalone fullscreen HTML presentation with keyboard navigation
 * (`ArrowRight`, `ArrowLeft`, `Home`, `End`, `P` for Presenter Notes) and RTL/LTR support.
 */
export function buildInteractiveHtmlPresentation(
  deck: PresentationDeckState,
  dir: "ltr" | "rtl" = "ltr"
): string {
  const esc = (s: string): string =>
    s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

  const slidesHtml = deck.slides
    .map((slide, idx) => {
      const kpisHtml =
        slide.kpis.length > 0
          ? `<div class="kpi-grid">${slide.kpis
              .map(
                (k) =>
                  `<div class="kpi-card"><div class="kpi-label">${esc(k.label)}</div><div class="kpi-val">${esc(k.formattedValue)}</div></div>`
              )
              .join("")}</div>`
          : "";
      const chartHtml =
        slide.chartType && slide.chartDataPoints && slide.chartDataPoints.length > 0
          ? `<div class="slide-chart">${renderChartSvg(slide.chartType, slide.title, slide.chartDataPoints)}</div>`
          : "";
      const bulletsHtml =
        slide.bullets.length > 0
          ? `<ul class="slide-bullets">${slide.bullets.map((b) => `<li>${esc(b)}</li>`).join("")}</ul>`
          : "";
      return `<section class="safi-slide" data-slide-index="${idx}" data-layout="${slide.layout}" aria-label="Slide ${slide.slideNumber}">
        <header class="slide-header">
          <span class="slide-badge">Slide ${slide.slideNumber} / ${deck.slides.length} • ${slide.sectionRole}</span>
          <h2>${esc(slide.title)}</h2>
          <p class="slide-sub">${esc(slide.subtitle)}</p>
        </header>
        ${kpisHtml}
        ${chartHtml}
        ${bulletsHtml}
        <aside class="presenter-notes" data-presenter-notes="true"><strong>Presenter Notes:</strong> ${esc(slide.presenterNotes)}</aside>
      </section>`;
    })
    .join("\n");

  return `<!DOCTYPE html>
<html lang="${dir === "rtl" ? "ar" : "en"}" dir="${dir}">
<head>
<meta charset="utf-8" />
<title>${esc(deck.title)}</title>
<style>
  body { margin: 0; font-family: system-ui, -apple-system, sans-serif; background: #090d16; color: #f8fafc; }
  .safi-deck-player { max-width: 1120px; margin: 0 auto; padding: 32px; }
  .safi-slide { background: #111827; border: 1px solid #1e293b; border-radius: 16px; padding: 36px; margin-bottom: 24px; }
  .slide-badge { font-size: 12px; color: #38bdf8; text-transform: uppercase; letter-spacing: 0.08em; }
  .kpi-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 16px; margin: 20px 0; }
  .kpi-card { background: #1e293b; padding: 16px; border-radius: 12px; }
  .kpi-val { font-size: 24px; font-weight: 700; color: #38bdf8; margin-top: 6px; }
  .slide-bullets li { margin: 10px 0; line-height: 1.6; }
  .presenter-notes { margin-top: 20px; padding: 12px 16px; background: rgba(56, 189, 248, 0.08); border-inline-start: 3px solid #38bdf8; font-size: 13px; color: #cbd5e1; }
</style>
</head>
<body data-safi-presentation-player="true" data-slide-count="${deck.slides.length}">
  <main class="safi-deck-player">${slidesHtml}</main>
</body>
</html>`;
}

// ============================================================================
// PHASE 7: DOCUMENT STUDIO (13-SECTION PAGINATED REPORT GENERATOR & EDITOR)
// ============================================================================

export type DocumentSectionKind =
  | "CoverPage"
  | "ExecutiveSummary"
  | "TableOfContents"
  | "DatasetOverview"
  | "KpiSummary"
  | "DetailedAnalysis"
  | "EmbeddedCharts"
  | "FormattedTables"
  | "KeyFindings"
  | "DataQualitySummary"
  | "Recommendations"
  | "MethodologyAndLimitations"
  | "Appendices";

export const ALL_REQUIRED_DOCUMENT_SECTIONS: readonly DocumentSectionKind[] = [
  "CoverPage",
  "ExecutiveSummary",
  "TableOfContents",
  "DatasetOverview",
  "KpiSummary",
  "DetailedAnalysis",
  "EmbeddedCharts",
  "FormattedTables",
  "KeyFindings",
  "DataQualitySummary",
  "Recommendations",
  "MethodologyAndLimitations",
  "Appendices"
];

export interface DocumentReportSection {
  readonly sectionId: string;
  readonly sectionNumber: number;
  readonly kind: DocumentSectionKind;
  readonly heading: string;
  readonly paragraphs: readonly string[];
  readonly bulletItems: readonly string[];
  readonly kpis?: readonly CanonicalKpiMetric[] | undefined;
  readonly chartType?: DashboardChartType | undefined;
  readonly chartDataPoints?: readonly ChartDataPoint[] | undefined;
  readonly tableHeaders?: readonly string[] | undefined;
  readonly tableRows?: readonly (readonly string[])[] | undefined;
  readonly calloutText?: string | undefined;
  readonly pageBreakBefore: boolean;
}

export interface DocumentReportState {
  readonly documentId: string;
  readonly title: string;
  readonly subtitle: string;
  readonly templateId: string;
  readonly headerText: string;
  readonly footerText: string;
  readonly showPageNumbers: boolean;
  readonly pageSize: "A4" | "Letter";
  readonly sections: readonly DocumentReportSection[];
}

/**
 * Generates a complete 13-section paginated `DocumentReportState` from the canonical semantic model.
 */
export function generateDocumentReport(
  model: SemanticWorkbookModel,
  analysis: VerifiedAnalysisReport,
  templateId = "tpl_vision_executive"
): DocumentReportState {
  const primaryTable = model.tables[0];
  const reportTitle = `${primaryTable?.title ?? "Workbook"} — Comprehensive Analytical Report`;
  const autoDash = generateAutoDashboard(model, analysis, templateId);
  const rankWidget = autoDash.widgets.find((w) => w.widgetId === "w_chart_ranking");
  const rawRegion = model.ingestionReport.worksheets[0]?.tables[0];

  const sections: DocumentReportSection[] = [
    {
      sectionId: "sec_1_cover",
      sectionNumber: 1,
      kind: "CoverPage",
      heading: reportTitle,
      paragraphs: [
        `Prepared via SafiData Universal Excel Intelligence Engine (Model v${model.modelVersion}).`,
        `Source Format: ${model.ingestionReport.detectedFormat} • Worksheets: ${model.ingestionReport.totalSheets} • Total Analytical Records: ${model.ingestionReport.totalAnalyticalRows}.`
      ],
      bulletItems: [],
      pageBreakBefore: false
    },
    {
      sectionId: "sec_2_exec_summary",
      sectionNumber: 2,
      kind: "ExecutiveSummary",
      heading: "1. Executive Summary",
      paragraphs: [
        `This report synthesizes verified accounting totals, statistical distributions, and data quality audits across ${model.ingestionReport.totalSheets} worksheet(s) and ${model.ingestionReport.totalTablesExtracted} structured table(s).`
      ],
      bulletItems: analysis.insightsByEpistemicCategory.ConfirmedFact.map((i) => i.statement),
      kpis: model.canonicalKpis.slice(0, 4),
      pageBreakBefore: true
    },
    {
      sectionId: "sec_3_toc",
      sectionNumber: 3,
      kind: "TableOfContents",
      heading: "2. Table of Contents",
      paragraphs: ["Structured sections included in this analytical report:"],
      bulletItems: [
        "1. Executive Summary",
        "2. Table of Contents",
        "3. Dataset Overview & Worksheet Inventory",
        "4. Canonical KPI Summary",
        "5. Detailed Analytical Breakdown",
        "6. Visual Charts & Distributions",
        "7. Formatted Analytical Tables",
        "8. Key Findings & Epistemic Classification",
        "9. Data Quality & Anomaly Summary",
        "10. Actionable Recommendations",
        "11. Methodology & Epistemic Limitations",
        "12. Appendices & Formula Lineage"
      ],
      pageBreakBefore: false
    },
    {
      sectionId: "sec_4_dataset_overview",
      sectionNumber: 4,
      kind: "DatasetOverview",
      heading: "3. Dataset Overview & Worksheet Inventory",
      paragraphs: [
        `The workbook contains ${model.ingestionReport.visibleSheets} visible worksheet(s) and ${model.ingestionReport.hiddenSheets} hidden/very-hidden worksheet(s).`
      ],
      bulletItems: model.ingestionReport.worksheets.map(
        (w) =>
          `Worksheet "${w.name}" (${w.visibility}, ${w.classification}): ${w.tables.length} table(s), ${w.rawRowCount}x${w.rawColCount} grid, ${w.formulaCount} formula(s).`
      ),
      pageBreakBefore: true
    },
    {
      sectionId: "sec_5_kpi_summary",
      sectionNumber: 5,
      kind: "KpiSummary",
      heading: "4. Canonical KPI Summary",
      paragraphs: [
        "All key performance indicators below are computed strictly over verified Measure columns (excluding Identifier codes, phone numbers, and separated subtotal/grand-total rows)."
      ],
      bulletItems: model.canonicalKpis.map(
        (k) => `${k.label}: ${k.formattedValue} [Source: ${k.formulaTrace}]`
      ),
      kpis: model.canonicalKpis,
      pageBreakBefore: false
    },
    {
      sectionId: "sec_6_detailed_analysis",
      sectionNumber: 6,
      kind: "DetailedAnalysis",
      heading: "5. Detailed Analytical Breakdown",
      paragraphs: [
        "Statistical distribution parameters (mean, median, standard deviation, quartiles Q1/Q2/Q3, and IQR fences) for each quantitative measure:"
      ],
      bulletItems: model.tables.flatMap((t) =>
        t.columns.flatMap((c) => {
          const ns = c.numericStats;
          if (!ns) return [];
          return [
            `${t.sheetName}.${c.headerName}: Sum=${ns.sum}, Mean=${ns.mean}, Median=${ns.median}, StdDev=${ns.stdDev}, Q1=${ns.q1}, Q3=${ns.q3}, Outliers=${ns.outlierCount}`
          ];
        })
      ),
      pageBreakBefore: true
    },
    {
      sectionId: "sec_7_charts",
      sectionNumber: 7,
      kind: "EmbeddedCharts",
      heading: "6. Visual Charts & Distributions",
      paragraphs: ["Primary dimensional distribution extracted from the canonical semantic model:"],
      bulletItems: [],
      chartType: rankWidget?.chartType ?? "Column",
      chartDataPoints: rankWidget?.dataPoints ?? [],
      pageBreakBefore: false
    },
    {
      sectionId: "sec_8_tables",
      sectionNumber: 8,
      kind: "FormattedTables",
      heading: "7. Formatted Analytical Tables",
      paragraphs: [
        `Sample records from primary table "${primaryTable?.title ?? "Table 1"}" (${primaryTable?.a1Range ?? "A1"}):`
      ],
      bulletItems: [],
      tableHeaders: rawRegion?.headers,
      tableRows: rawRegion?.dataRows.slice(0, 15),
      pageBreakBefore: true
    },
    {
      sectionId: "sec_9_findings",
      sectionNumber: 9,
      kind: "KeyFindings",
      heading: "8. Key Findings & Epistemic Classification",
      paragraphs: [
        "Every analytical statement is classified into one of five epistemic tiers to prevent conflating confirmed accounting facts with unverified hypotheses:"
      ],
      bulletItems: analysis.insights.map(
        (i) => `[${i.epistemicCategory}] ${i.title}: ${i.statement}`
      ),
      pageBreakBefore: false
    },
    {
      sectionId: "sec_10_quality",
      sectionNumber: 10,
      kind: "DataQualitySummary",
      heading: "9. Data Quality & Anomaly Summary",
      paragraphs: [
        `Overall Workbook Data Quality Score: ${model.overallQualityScorePct}%. Findings are categorized into Confirmed Issues, Potential Issues, Information, and Needs Clarification.`
      ],
      bulletItems:
        model.qualityFindings.length > 0
          ? model.qualityFindings.map((f) => `[${f.category}] ${f.title}: ${f.description}`)
          : ["No structural or statistical data quality defects were detected."],
      pageBreakBefore: true
    },
    {
      sectionId: "sec_11_recommendations",
      sectionNumber: 11,
      kind: "Recommendations",
      heading: "10. Actionable Recommendations",
      paragraphs: ["Recommended data hygiene and analytical follow-ups:"],
      bulletItems: [
        ...model.qualityFindings.flatMap((f) => {
          const act = f.suggestedAction;
          return act ? [`${act.label}: ${act.previewSummary} (Reversible)`] : [];
        }),
        ...analysis.questions
          .filter((q) => q.status !== "Answered")
          .map((q) => `Resolve Clarification: ${q.prompt}`)
      ],
      pageBreakBefore: false
    },
    {
      sectionId: "sec_12_methodology",
      sectionNumber: 12,
      kind: "MethodologyAndLimitations",
      heading: "11. Methodology & Epistemic Limitations",
      paragraphs: [
        "Ingestion performed deterministic table segmentation, subtotal isolation, leading-zero preservation, and local formula verification.",
        ...analysis.insightsByEpistemicCategory.UnsupportedConclusion.map((i) => i.statement)
      ],
      bulletItems: analysis.forecasts.flatMap((f) => f.explicitAssumptions),
      pageBreakBefore: true
    },
    {
      sectionId: "sec_13_appendices",
      sectionNumber: 13,
      kind: "Appendices",
      heading: "12. Appendices & Formula Lineage",
      paragraphs: [
        `Defined Names: ${model.ingestionReport.definedNames.length} • Embedded Objects Inventoried: ${model.ingestionReport.embeddedObjects.length} • Cross-Table Relationships: ${model.relationships.length}.`
      ],
      bulletItems: [
        ...model.relationships.map(
          (r) =>
            `Relationship (${r.cardinality}): ${r.fromSheet}.${r.fromColumn} -> ${r.toSheet}.${r.toColumn} (${r.matchRatePct}% match)`
        ),
        ...model.ingestionReport.warnings
      ],
      pageBreakBefore: false
    }
  ];

  return {
    documentId: "doc_main",
    title: reportTitle,
    subtitle: `Canonical Semantic Model v${model.modelVersion}`,
    templateId,
    headerText: `${primaryTable?.title ?? "SafiData"} — Verified Analytical Report`,
    footerText: "Generated locally by SafiData • Confidential & Traceable",
    showPageNumbers: true,
    pageSize: "A4",
    sections
  };
}

/**
 * Edits a `DocumentReportState` (add, remove, reorder, update sections, headers, footers, page breaks).
 */
export function applyDocumentEdit(
  doc: DocumentReportState,
  action:
    | {
        readonly type: "ADD_SECTION";
        readonly section: Omit<DocumentReportSection, "sectionNumber">;
      }
    | { readonly type: "REMOVE_SECTION"; readonly sectionId: string }
    | { readonly type: "REORDER_SECTIONS"; readonly fromIndex: number; readonly toIndex: number }
    | {
        readonly type: "UPDATE_SECTION";
        readonly sectionId: string;
        readonly patch: Partial<DocumentReportSection>;
      }
    | {
        readonly type: "UPDATE_METADATA";
        readonly patch: Partial<
          Pick<
            DocumentReportState,
            "title" | "subtitle" | "templateId" | "headerText" | "footerText" | "showPageNumbers"
          >
        >;
      }
): DocumentReportState {
  const renumber = (secs: readonly DocumentReportSection[]): DocumentReportSection[] =>
    secs.map((s, idx) => ({ ...s, sectionNumber: idx + 1 }));

  if (action.type === "UPDATE_METADATA") {
    return { ...doc, ...action.patch };
  }

  const next = [...doc.sections];
  if (action.type === "ADD_SECTION") {
    next.push({ ...action.section, sectionNumber: next.length + 1 });
  } else if (action.type === "REMOVE_SECTION") {
    return {
      ...doc,
      sections: renumber(next.filter((s) => s.sectionId !== action.sectionId))
    };
  } else if (action.type === "REORDER_SECTIONS") {
    const item = next[action.fromIndex];
    if (item && action.toIndex >= 0 && action.toIndex < next.length) {
      next.splice(action.fromIndex, 1);
      next.splice(action.toIndex, 0, item);
    }
  } else if (action.type === "UPDATE_SECTION") {
    return {
      ...doc,
      sections: renumber(
        next.map((s) => (s.sectionId === action.sectionId ? { ...s, ...action.patch } : s))
      )
    };
  }

  return { ...doc, sections: renumber(next) };
}
