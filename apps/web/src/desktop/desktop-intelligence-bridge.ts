/**
 * Bridges native Rust Desktop Studio state (`DesktopStudioState`, `DesktopWorkbookDetails`,
 * bounded `DesktopRowPage`) with the bilingual Semantic Workbook Intelligence, Contextual
 * Question, Verified Analysis, and Quad-Studio deliverable engines (`apps/web/src/utils/*`).
 *
 * Strictly imported inside `apps/web/src/desktop/` — never included on the public website.
 */

import type {
  DetectedTableRegion,
  FormatSupportLevel,
  SheetVisibility,
  WorkbookDateSystem,
  WorkbookFormat,
  WorkbookIngestionReport,
  WorksheetIntelligence
} from "../utils/workbook-ingestion";
import type { SemanticWorkbookModel } from "../utils/semantic-intelligence";
import { buildSemanticWorkbookModel } from "../utils/semantic-intelligence";
import type {
  ContextualClarificationQuestion,
  VerifiedAnalysisReport
} from "../utils/verified-analysis-engine";
import {
  applyQuestionResolution,
  buildVerifiedAnalysisReport,
  generateContextualQuestions
} from "../utils/verified-analysis-engine";
import type {
  DashboardStudioState,
  DocumentReportState,
  PresentationDeckState
} from "../utils/quad-studio-engine";
import {
  generateAutoDashboard,
  generateDocumentReport,
  generatePresentationDeck
} from "../utils/quad-studio-engine";
import type {
  DesktopRowPage,
  DesktopStudioState,
  DesktopWorkbookDetails,
  StudioDocBlockArtifact,
  StudioQuestionResolutionArtifact,
  StudioSlideArtifact
} from "./studio-bridge";

export interface DesktopIntelligenceSnapshot {
  readonly ingestionReport: WorkbookIngestionReport;
  readonly semanticModel: SemanticWorkbookModel;
  readonly questions: readonly ContextualClarificationQuestion[];
  readonly verifiedReport: VerifiedAnalysisReport;
}

function mapFormat(raw: string): WorkbookFormat {
  switch (raw) {
    case "Xlsx":
      return "Xlsx";
    case "Xlsm":
      return "Xlsm";
    case "Xlsb":
      return "Xlsb";
    case "Xls":
      return "Xls";
    case "Ods":
      return "Ods";
    case "Csv":
      return "Csv";
    default:
      return "Xlsx";
  }
}

function mapSupport(raw: string): FormatSupportLevel {
  switch (raw) {
    case "FullSupport":
      return "FullSupport";
    case "SupportedWithMacroNeutralization":
      return "SupportedWithMacroNeutralization";
    default:
      return "SupportedTabularAndFormulas";
  }
}

function mapVisibility(raw: string): SheetVisibility {
  if (raw === "Hidden") return "Hidden";
  if (raw === "VeryHidden") return "VeryHidden";
  return "Visible";
}

function mapDateSystem(raw: string): WorkbookDateSystem {
  return raw === "Macintosh1904" ? "Macintosh1904" : "Standard1900";
}

/**
 * Reconstructs a `WorkbookIngestionReport` backed by Rust's authoritative `workbookDetails`
 * and working rows for the active sheet/table. Supports cooperative cancellation via `AbortSignal`.
 */
export async function buildDesktopIntelligenceSnapshotAsync(params: {
  readonly state: DesktopStudioState;
  readonly details: DesktopWorkbookDetails | null;
  readonly page: DesktopRowPage | null;
  readonly savedResolutions?: readonly StudioQuestionResolutionArtifact[];
  readonly signal?: AbortSignal | undefined;
  readonly onProgress?: (pct: number, phase: string) => void;
}): Promise<DesktopIntelligenceSnapshot> {
  const { state, details, page, savedResolutions = [], signal, onProgress } = params;
  const checkCancelled = (): void => {
    if (signal?.aborted) {
      throw new DOMException("Intelligence analysis cancelled by user.", "AbortError");
    }
  };

  checkCancelled();
  onProgress?.(20, "Converting Rust workbook structure");
  await new Promise((r) => setTimeout(r, 0));
  checkCancelled();

  const activeSheetName = details?.activeSheet ?? state.workbookSummary?.activeSheet ?? "Sheet1";
  const activeTableId = details?.activeTableId ?? state.workbookSummary?.activeTableId ?? "tbl_1";
  const activeHeaders = state.columns.map((c) => c.name);
  const activeDataRows: string[][] = (page?.rows ?? []).map((row) =>
    row.cells.map((cell) => cell.visible)
  );

  const worksheets: WorksheetIntelligence[] =
    details && details.worksheets.length > 0
      ? details.worksheets.map((ws) => {
          const tables: DetectedTableRegion[] = ws.tables.map((t) => {
            const isActive = ws.name === activeSheetName && t.tableId === activeTableId;
            const tableHeaders = isActive ? activeHeaders : [...t.headers];
            const tableRows = isActive ? activeDataRows : [];
            return {
              tableId: t.tableId,
              title: t.title,
              sheetName: t.sheetName,
              bounds: {
                startRow: 0,
                startCol: 0,
                endRow: Math.max(1, isActive ? tableRows.length : t.rowCount),
                endCol: Math.max(0, tableHeaders.length - 1)
              },
              a1Range: `A1:Z${Math.max(2, t.rowCount + 1)}`,
              headerRows: [0],
              headers: tableHeaders,
              dataRows: tableRows,
              sourceRowIndices: tableRows.map((_, idx) => idx + 1),
              separatedRows: t.separatedRows.map((sr) => ({
                sheetRow: sr.sheetRow,
                kind:
                  (sr.kind as
                    | "Title"
                    | "Subtitle"
                    | "Subtotal"
                    | "GrandTotal"
                    | "RepeatedHeader"
                    | "Footnote") || "Title",
                values: [...sr.cells]
              })),
              confidencePct: t.confidencePct
            };
          });
          return {
            name: ws.name,
            index: ws.index,
            visibility: mapVisibility(ws.visibility),
            classification: "MainDataset",
            classificationConfidencePct: ws.classificationConfidencePct,
            classificationReason: ws.classificationReason,
            rawRowCount: ws.rawRowCount,
            rawColCount: ws.rawColCount,
            mergedRanges: [...ws.mergedRanges],
            hasFrozenPanes: ws.hasFrozenPanes,
            autoFilterRange: ws.autoFilterRange ?? undefined,
            printArea: ws.printArea ?? undefined,
            isProtected: ws.isProtected,
            tables,
            cellMetadata: ws.formulaSamples.map((fs) => ({
              sheetName: ws.name,
              address: fs.address,
              row: fs.row,
              col: fs.col,
              rawValue: fs.rawValue,
              formula: fs.formula ?? undefined,
              cachedValue: fs.cachedValue ?? undefined,
              recalculatedValue: fs.recalculatedValue ?? undefined,
              formulaStatus:
                fs.formulaStatus === "UnsupportedFormula"
                  ? "UnverifiedFormula"
                  : (fs.formulaStatus as
                      | "LiteralValue"
                      | "VerifiedCachedAndRecalculated"
                      | "CachedResultOnly"
                      | "RecalculatedLocally"
                      | "VolatileFormula"
                      | "ExternalWorkbookReference"
                      | "FormulaError"
                      | "UnverifiedFormula") || "LiteralValue",
              dataType: "Text",
              numberFormat: fs.numberFormat ?? undefined,
              comment: fs.comment ?? undefined,
              hyperlink: fs.hyperlink ?? undefined
            })),
            formulaCount: ws.formulaCount,
            verifiedFormulaCount: ws.verifiedFormulaCount
          };
        })
      : [
          {
            name: activeSheetName,
            index: 0,
            visibility: "Visible",
            classification: "MainDataset",
            classificationConfidencePct: 90,
            classificationReason: "Loaded from native Rust working table",
            rawRowCount: state.rowCount + 1,
            rawColCount: activeHeaders.length,
            mergedRanges: [],
            hasFrozenPanes: false,
            isProtected: false,
            tables: [
              {
                tableId: activeTableId,
                title: state.sourceId,
                sheetName: activeSheetName,
                bounds: {
                  startRow: 0,
                  startCol: 0,
                  endRow: Math.max(1, activeDataRows.length),
                  endCol: Math.max(0, activeHeaders.length - 1)
                },
                a1Range: `A1:Z${Math.max(2, state.rowCount + 1)}`,
                headerRows: [0],
                headers: activeHeaders,
                dataRows: activeDataRows,
                sourceRowIndices: activeDataRows.map((_, idx) => idx + 1),
                separatedRows: [],
                confidencePct: 95
              }
            ],
            cellMetadata: [],
            formulaCount: 0,
            verifiedFormulaCount: 0
          }
        ];

  const ingestionReport: WorkbookIngestionReport = {
    detectedFormat: mapFormat(details?.detectedFormat ?? "Xlsx"),
    supportLevel: mapSupport(details?.supportLevel ?? "FullSupport"),
    dateSystem: mapDateSystem(details?.dateSystem ?? "Standard1900"),
    macrosNeutralized: details?.macrosNeutralized ?? false,
    totalSheets: details?.totalSheets ?? worksheets.length,
    visibleSheets: details?.visibleSheets ?? worksheets.length,
    hiddenSheets: details?.hiddenSheets ?? 0,
    totalTablesExtracted: details?.totalTablesExtracted ?? 1,
    totalAnalyticalRows: state.rowCount,
    totalFormulasDetected: details?.totalFormulasDetected ?? 0,
    totalFormulasVerified: details?.totalFormulasVerified ?? 0,
    definedNames: (details?.definedNames ?? []).map((dn) => ({
      name: dn.name,
      refersTo: dn.refersTo,
      isBuiltIn: dn.isBuiltIn
    })),
    embeddedObjects: (details?.embeddedObjects ?? []).map((obj) => ({
      kind: obj.kind as
        | "Chart"
        | "Image"
        | "CommentOrNote"
        | "Hyperlink"
        | "DrawingOrShape"
        | "PivotTable"
        | "ExcelTable"
        | "VbaMacroProject",
      location: obj.location,
      handling: obj.handling as
        | "Extracted"
        | "Summarized"
        | "NeutralizedBySecurityPolicy"
        | "Unsupported",
      summary: obj.summary
    })),
    worksheets,
    warnings: [...(details?.warnings ?? [])],
    progressLog: []
  };

  onProgress?.(55, "Profiling semantic columns & relationships");
  await new Promise((r) => setTimeout(r, 0));
  checkCancelled();

  const semanticModel = buildSemanticWorkbookModel(ingestionReport);
  let questions = generateContextualQuestions(semanticModel);

  // Replay persisted clarification question answers/skips/defers cleanly
  for (const res of savedResolutions) {
    const kind =
      res.action === "answered"
        ? ("Answer" as const)
        : res.action === "skipped"
          ? ("Skip" as const)
          : res.action === "deferred"
            ? ("Defer" as const)
            : res.action === "corrected"
              ? ("Correct" as const)
              : ("Custom" as const);
    questions = applyQuestionResolution(questions, res.questionId, {
      kind,
      optionId: res.answer,
      customText: res.customText ?? res.answer,
      timestampIso: new Date(res.updatedAtMs || Date.now()).toISOString()
    });
  }

  onProgress?.(85, "Synthesizing verified epistemic insights");
  await new Promise((r) => setTimeout(r, 0));
  checkCancelled();

  const verifiedReport = buildVerifiedAnalysisReport(semanticModel, questions);
  onProgress?.(100, "Completed");

  return {
    ingestionReport,
    semanticModel,
    questions,
    verifiedReport
  };
}

const BRIDGE_TO_ENGINE_CHART_TYPE = {
  bar: "Bar",
  column: "Column",
  line: "Line",
  area: "Area",
  stacked: "Stacked",
  combo: "Combo",
  pie: "Pie",
  donut: "Donut",
  scatter: "Scatter",
  bubble: "Bubble",
  histogram: "Histogram",
  boxplot: "BoxPlot",
  heatmap: "Heatmap",
  treemap: "Treemap",
  waterfall: "Waterfall",
  funnel: "Funnel",
  gauge: "Gauge",
  kpi: "KPI",
  table: "Table",
  pivotsummary: "PivotSummary",
  geographic: "Geographic"
} as const;

export function buildExportDashboardState(
  model: SemanticWorkbookModel,
  verified: VerifiedAnalysisReport,
  state: DesktopStudioState
): DashboardStudioState {
  const auto = generateAutoDashboard(model, verified);
  const customCharts = state.artifacts?.charts ?? [];
  if (customCharts.length === 0) return auto;
  const customWidgets = customCharts.map((c, idx) => ({
    widgetId: c.id,
    kind: "chart" as const,
    title: c.title,
    chartType: BRIDGE_TO_ENGINE_CHART_TYPE[c.chartType ?? "bar"] ?? ("Column" as const),
    sheetName: model.tables[0]?.sheetName ?? state.workbookSummary?.activeSheet ?? "Sheet1",
    tableId: model.tables[0]?.tableId ?? "tbl_1",
    dimensionColumn: c.groupBy,
    measureColumn: c.column,
    aggregation: (c.function === "AVG"
      ? "AVERAGE"
      : c.function === "COUNT"
        ? "COUNT_DISTINCT"
        : c.function) as "SUM" | "AVERAGE" | "MIN" | "MAX" | "MEDIAN" | "COUNT_DISTINCT",
    sortMode: "value_desc" as const,
    localFilters: [],
    gridLayout: {
      x: (idx % 2) * 6,
      y: 2 + Math.floor(idx / 2) * 2,
      w: 6,
      h: 2
    },
    dataPoints: auto.widgets.find((w) => w.kind === "chart")?.dataPoints ?? []
  }));
  return {
    ...auto,
    widgets: [...auto.widgets.filter((w) => w.kind === "kpi"), ...customWidgets]
  };
}

export function buildExportPresentationDeck(
  model: SemanticWorkbookModel,
  verified: VerifiedAnalysisReport,
  slides: readonly StudioSlideArtifact[]
): PresentationDeckState {
  const generated = generatePresentationDeck(model, verified);
  if (slides.length === 0) return generated;
  return {
    ...generated,
    slides: slides.map((s, idx) => {
      const base = generated.slides[idx % generated.slides.length];
      return {
        slideId: s.id,
        slideNumber: idx + 1,
        layout: base?.layout ?? "ExecutiveSummary",
        sectionRole: base?.sectionRole ?? "KeyFindings",
        title: s.title,
        subtitle: base?.subtitle ?? "",
        bullets: s.body
          .split("\n")
          .map((l) => l.trim())
          .filter(Boolean),
        kpis: base?.kpis ?? model.canonicalKpis.slice(0, 3),
        chartType: base?.chartType,
        chartDataPoints: base?.chartDataPoints,
        tableHeaders: base?.tableHeaders,
        tableRows: base?.tableRows,
        presenterNotes: s.presenterNotes ?? base?.presenterNotes ?? ""
      };
    })
  };
}

export function buildExportDocumentReport(
  model: SemanticWorkbookModel,
  verified: VerifiedAnalysisReport,
  docBlocks: readonly StudioDocBlockArtifact[]
): DocumentReportState {
  const generated = generateDocumentReport(model, verified);
  if (docBlocks.length === 0) return generated;
  return {
    ...generated,
    headerText: docBlocks[0]?.headerText ?? generated.headerText,
    footerText: docBlocks[0]?.footerText ?? generated.footerText,
    sections: docBlocks.map((b, idx) => ({
      sectionId: b.id,
      sectionNumber: idx + 1,
      kind:
        b.kind === "callout" || b.kind === "metric_callout" ? "KeyFindings" : "DetailedAnalysis",
      heading: b.title || `Section ${idx + 1}`,
      paragraphs: b.content
        .split("\n")
        .map((l) => l.trim())
        .filter(Boolean),
      bulletItems: [],
      calloutText:
        b.authoredBy === "ai"
          ? "AI-assisted draft (verified against canonical metrics)"
          : undefined,
      pageBreakBefore: Boolean(b.pageBreakBefore)
    }))
  };
}
