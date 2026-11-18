import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import type {
  DesktopChartSeries,
  DesktopRowPage,
  DesktopStudioState,
  DesktopWorkbookDetails,
  StudioChartType
} from "./studio-bridge";
import { buildDesktopIntelligenceSnapshotAsync } from "./desktop-intelligence-bridge";
import { CHART_TYPES, DashboardChartsView } from "./components/DashboardChartsView";
import { WorkbookIntelligenceDrawer } from "./components/WorkbookIntelligenceDrawer";
import { IntelligenceAndAssistantPanel } from "./components/IntelligenceAndAssistantPanel";
import {
  DESKTOP_TUTORIAL_STEPS,
  DeliverableExportModal,
  InteractiveTutorialOverlay,
  UnifiedTooltipBadge
} from "./components/DesktopTutorialAndExport";
import { KeynoteSlidesView } from "./components/KeynoteSlidesView";
import { VerifiedDocView } from "./components/VerifiedDocView";
import {
  answerAnalyticalQuestion,
  applyQuestionResolution
} from "../utils/verified-analysis-engine";

function makeSampleState(): DesktopStudioState {
  return {
    revision: 7,
    datasetVersion: 2,
    originFingerprint: "orig_fp_1",
    sourceId: "messy_enterprise.xlsx",
    rowCount: 4,
    columns: [
      { id: 1, name: "Branch", type: "Text", originColumnIndex: 0 },
      { id: 2, name: "Revenue", type: "Decimal", originColumnIndex: 1 },
      { id: 3, name: "Phone", type: "Text", originColumnIndex: 2 },
      { id: 4, name: "OrderDate", type: "Date", originColumnIndex: 3 }
    ],
    calculated: [],
    lastRecomputed: ["rev_total"],
    errors: [],
    parameters: [{ name: "growth", value: "0.1" }],
    scenarios: [{ id: "base", name: "Base Case", parameters: { growth: "0.1" } }],
    activeScenario: "base",
    metrics: [
      {
        id: "rev_total",
        name: "Total Revenue",
        unit: "EGP",
        formula: "SUM(Revenue)",
        formulaVersion: 1,
        sourceId: "messy_enterprise.xlsx",
        sourceVersion: 1,
        sourceFingerprint: "src_fp",
        fingerprint: "m_fp",
        status: "Verified",
        value: "12500",
        rows: [0, 1, 2, 3],
        sourceRowIds: [1, 2, 3, 4],
        columns: ["Revenue"],
        columnIds: [2],
        transformationTrace: [],
        engineVersion: "5.0.0",
        computedAtMs: 1700000000000
      }
    ],
    anomalies: [],
    workbookSummary: {
      detectedFormat: "Xlsx",
      supportLevel: "SupportedWithMacroNeutralization",
      dateSystem: "Standard1900",
      macrosNeutralized: true,
      totalSheets: 2,
      visibleSheets: 1,
      hiddenSheets: 1,
      totalTablesExtracted: 2,
      totalAnalyticalRows: 4,
      totalFormulasDetected: 4,
      totalFormulasVerified: 2,
      warningsCount: 1,
      activeSheet: "Sales_2026",
      activeTableId: "tbl_1",
      sheets: []
    },
    artifacts: {
      charts: [],
      slides: [],
      docBlocks: [],
      freeformBlocks: [],
      assets: [],
      templateConfig: null,
      questionResolutions: [],
      privacySettings: { externalAiConsentGranted: false, maskPiiOnCloud: true }
    }
  };
}

function makeSampleDetails(): DesktopWorkbookDetails {
  return {
    detectedFormat: "Xlsx",
    supportLevel: "SupportedWithMacroNeutralization",
    dateSystem: "Standard1900",
    macrosNeutralized: true,
    activeSheet: "Sales_2026",
    activeTableId: "tbl_1",
    totalSheets: 2,
    visibleSheets: 1,
    hiddenSheets: 1,
    totalTablesExtracted: 2,
    totalAnalyticalRows: 4,
    totalFormulasDetected: 4,
    totalFormulasVerified: 2,
    warnings: ["Hidden worksheet Audit_Internal excluded from default analytical outputs."],
    definedNames: [{ name: "SalesArea", refersTo: "Sales_2026!$A$2:$D$5", isBuiltIn: false }],
    embeddedObjects: [
      {
        kind: "VbaMacroProject",
        location: "xl/vbaProject.bin",
        handling: "NeutralizedBySecurityPolicy",
        summary: "VBA binary stripped safely"
      }
    ],
    worksheets: [
      {
        name: "Sales_2026",
        index: 0,
        visibility: "Visible",
        classification: "MainDataset",
        classificationConfidencePct: 94,
        classificationReason: "Primary transactional sheet",
        rawRowCount: 6,
        rawColCount: 4,
        mergedRanges: ["A1:D1"],
        hasFrozenPanes: true,
        autoFilterRange: "A2:D5",
        printArea: null,
        isProtected: false,
        formulaCount: 3,
        verifiedFormulaCount: 2,
        formulaSamples: [
          {
            address: "B6",
            row: 5,
            col: 1,
            rawValue: "=SUM(B2:B5)",
            formula: "SUM(B2:B5)",
            cachedValue: "12500",
            recalculatedValue: "12500",
            formulaStatus: "VerifiedCachedAndRecalculated",
            dataType: "Numeric",
            numberFormat: "#,##0",
            comment: "Checked by finance",
            hyperlink: null
          }
        ],
        tables: [
          {
            tableId: "tbl_1",
            title: "Main Regional Sales",
            sheetName: "Sales_2026",
            rowCount: 4,
            headers: ["Branch", "Revenue", "Phone", "OrderDate"],
            separatedRows: [{ sheetRow: 1, kind: "Title", cells: ["2026 Regional Summary"] }],
            confidencePct: 95
          },
          {
            tableId: "tbl_2",
            title: "Secondary Summary",
            sheetName: "Sales_2026",
            rowCount: 2,
            headers: ["Channel", "Total"],
            separatedRows: [],
            confidencePct: 91
          }
        ]
      },
      {
        name: "Audit_Internal",
        index: 1,
        visibility: "Hidden",
        classification: "SupportingCalculations",
        classificationConfidencePct: 80,
        classificationReason: "Hidden audit sheet",
        rawRowCount: 3,
        rawColCount: 2,
        mergedRanges: [],
        hasFrozenPanes: false,
        autoFilterRange: null,
        printArea: null,
        isProtected: true,
        formulaCount: 1,
        verifiedFormulaCount: 0,
        formulaSamples: [],
        tables: [
          {
            tableId: "tbl_1",
            title: "Internal Adjustments",
            sheetName: "Audit_Internal",
            rowCount: 2,
            headers: ["Code", "Adjustment"],
            separatedRows: [],
            confidencePct: 88
          }
        ]
      }
    ]
  };
}

function makeSamplePage(): DesktopRowPage {
  const mkCell = (v: string) => ({
    raw: v,
    normalized: v,
    visible: v,
    changed: false,
    validation_error: null
  });
  return {
    revision: 7,
    offset: 0,
    limit: 50,
    total_rows: 4,
    filtered_rows: 4,
    rows: [
      {
        working_row: 0,
        row_id: 1,
        origin_row_id: 1,
        origin_row_index: 1,
        cells: [mkCell("Cairo"), mkCell("5000"), mkCell("01012345678"), mkCell("2026-01-15")]
      },
      {
        working_row: 1,
        row_id: 2,
        origin_row_id: 2,
        origin_row_index: 2,
        cells: [mkCell("Alexandria"), mkCell("3500"), mkCell("01223344556"), mkCell("2026-02-10")]
      },
      {
        working_row: 2,
        row_id: 3,
        origin_row_id: 3,
        origin_row_index: 3,
        cells: [mkCell("Giza"), mkCell("2500"), mkCell("01198765432"), mkCell("2026-03-05")]
      },
      {
        working_row: 3,
        row_id: 4,
        origin_row_id: 4,
        origin_row_index: 4,
        cells: [mkCell("Mansoura"), mkCell("1500"), mkCell("01555555555"), mkCell("2026-04-01")]
      }
    ]
  };
}

describe("Desktop Workbook Intelligence, 21 Charts, Epistemic Assistant & Deliverables", () => {
  it("renders all 21 distinct StudioChartType visuals without falling back to bar", () => {
    expect(CHART_TYPES).toHaveLength(21);
    const state = makeSampleState();

    const makeSeries = (ct: StudioChartType): DesktopChartSeries => ({
      chartType: ct,
      function: "SUM",
      column: "Revenue",
      groupBy: "Branch",
      totalOrKpi: "12500",
      points: [
        {
          label: "Cairo",
          value: "5000",
          xValue: "10",
          yValue: "5000",
          sharePct: "40.0",
          svgPath: "M 15.00 68.00 A 35 35 0 0 1 65.00 35.00",
          ratioPct: 40,
          startPct: 0,
          xPct: 25,
          yPct: 45,
          rowRefs: [1]
        },
        {
          label: "Alex",
          value: "3500",
          xValue: "20",
          yValue: "3500",
          sharePct: "28.0",
          svgPath: "M 50.00 10.00 A 40 40 0 0 1 88.00 50.00 Z",
          ratioPct: 28,
          startPct: 40,
          xPct: 65,
          yPct: 70,
          rowRefs: [2]
        }
      ]
    });

    const seriesMap = Object.fromEntries(CHART_TYPES.map((ct, i) => [`c_${i}`, makeSeries(ct)]));
    const html = renderToString(
      createElement(DashboardChartsView, {
        language: "en",
        project: "main",
        revision: 7,
        activeScenario: "base",
        columns: state.columns,
        metrics: state.metrics,
        charts: CHART_TYPES.map((ct, i) => ({
          id: `c_${i}`,
          title: `Visual ${ct}`,
          chartType: ct,
          function: "SUM",
          column: "Revenue",
          groupBy: "Branch"
        })),
        initialSeriesByChart: seriesMap,
        onUpdateCharts: vi.fn()
      })
    );

    for (const ct of CHART_TYPES) {
      expect(html).toContain(`data-chart-type="${ct}"`);
      expect(html).toContain(`Visual ${ct}`);
    }
    expect(html).toContain("data-combo-bar=");
    expect(html).toContain("data-gauge-arc=");
    expect(html).toContain("data-boxplot-whisker=");
    expect(html).toContain("data-heatmap-cell=");
    expect(html).toContain("data-treemap-tile=");
    expect(html).toContain("data-waterfall-step=");
    expect(html).toContain("data-funnel-stage=");
    expect(html).toContain("data-geo-marker=");
    expect(html).toContain('data-chart-table="table"');
    expect(html).toContain('data-chart-table="pivotsummary"');
  });

  it("renders WorkbookIntelligenceDrawer disclosures, macro neutralization, hidden sheets & table switches", () => {
    const details = makeSampleDetails();
    const htmlEn = renderToString(
      createElement(WorkbookIntelligenceDrawer, {
        language: "en",
        details,
        busy: false,
        onSelectRegion: vi.fn(async () => {})
      })
    );
    expect(htmlEn).toContain("Workbook Explorer &amp; Audit");
    expect(htmlEn).toContain("macros-neutralized-banner");
    expect(htmlEn).toContain("workbook-warnings-box");
    expect(htmlEn).toContain("Show Hidden (1)");
    expect(htmlEn).toContain("Main Regional Sales");
    expect(htmlEn).toContain("Secondary Summary");
    expect(htmlEn).toContain("VerifiedCachedAndRecalculated");
    expect(htmlEn).toContain("Switch Active Table");

    const htmlAr = renderToString(
      createElement(WorkbookIntelligenceDrawer, {
        language: "ar",
        details,
        busy: false,
        onSelectRegion: vi.fn(async () => {})
      })
    );
    expect(htmlAr).toContain("مستكشف المصنف والميتاداتا");
    expect(htmlAr).toContain("عرض المخفية (1)");
  });

  it("builds intelligence snapshot, answers contextual questions, enforces grounded Q&A, and renders privacy/epistemic UI", async () => {
    const state = makeSampleState();
    const details = makeSampleDetails();
    const page = makeSamplePage();

    const snapshot = await buildDesktopIntelligenceSnapshotAsync({
      state,
      details,
      page
    });

    expect(snapshot.ingestionReport.macrosNeutralized).toBe(true);
    expect(snapshot.semanticModel.tables.length).toBeGreaterThanOrEqual(1);
    expect(snapshot.verifiedReport.insights.length).toBeGreaterThan(0);

    // Resolve first question if generated and re-verify deterministic grounded assistant
    let updatedQuestions = snapshot.questions;
    if (updatedQuestions[0]) {
      updatedQuestions = applyQuestionResolution(updatedQuestions, updatedQuestions[0].questionId, {
        kind: "Skip"
      });
      expect(updatedQuestions[0]?.status).toBe("Skipped");
    }

    const answer = answerAnalyticalQuestion(
      "What does this workbook contain and what is total revenue?",
      snapshot.semanticModel,
      snapshot.verifiedReport
    );
    expect(answer.answerText.length).toBeGreaterThan(20);
    expect(answer.citations.length).toBeGreaterThan(0);

    const panelHtml = renderToString(
      createElement(IntelligenceAndAssistantPanel, {
        language: "en",
        model: snapshot.semanticModel,
        report: snapshot.verifiedReport,
        questions: updatedQuestions,
        privacySettings: { externalAiConsentGranted: false, maskPiiOnCloud: true },
        onResolveQuestion: vi.fn(),
        onUpdatePrivacy: vi.fn()
      })
    );
    expect(panelHtml).toContain("Strict Offline Local-Only Mode (Default)");
    expect(panelHtml).toContain("Epistemic Verified Insights");
    expect(panelHtml).toContain("Confirmed Fact");
    expect(panelHtml).toContain("Unsupported Conclusion (Disclosed Gap)");
    expect(panelHtml).toContain("Grounded Data Assistant");
  });

  it("renders DeliverableExportModal (all 9 formats + hidden disclosure), KeynoteSlidesView notes, VerifiedDocView page breaks & tooltips", () => {
    const modalHtml = renderToString(
      createElement(DeliverableExportModal, {
        language: "en",
        open: true,
        busy: false,
        hiddenSheetCount: 2,
        lastStatus: "Written atomically",
        onClose: vi.fn(),
        onExportFormat: vi.fn(async () => {})
      })
    );
    for (const fmt of [
      "xlsx",
      "pdf",
      "docx",
      "pptx",
      "safi.html",
      "png",
      "svg",
      "csv",
      "powerbi"
    ]) {
      expect(modalHtml).toContain(`value="${fmt}"`);
    }
    expect(modalHtml).toContain("Include 2 hidden worksheet(s)");
    expect(modalHtml).toContain("Written atomically");

    const tooltipHtml = renderToString(
      createElement(UnifiedTooltipBadge, {
        tooltipId: "workbook-help",
        text: "Local parsing info"
      })
    );
    expect(tooltipHtml).toContain('data-tooltip-id="workbook-help"');

    const tutorialHtml = renderToString(
      createElement(InteractiveTutorialOverlay, {
        language: "ar",
        open: true,
        onClose: vi.fn()
      })
    );
    expect(tutorialHtml).toContain("جولة تعريفية تفاعلية");
    expect(tutorialHtml).toContain("افتح أي ملف بيانات");
    expect(tutorialHtml).toContain("خطوة 1 من");

    const slidesHtml = renderToString(
      createElement(KeynoteSlidesView, {
        language: "en",
        metrics: makeSampleState().metrics,
        slides: [
          {
            id: "s1",
            title: "Executive Summary",
            body: "Revenue reached {{metric:rev_total}}",
            metricId: "rev_total",
            chartIndex: null,
            layout: "kpi_split",
            presenterNotes: "Highlight Cairo branch growth."
          }
        ],
        onUpdateSlides: vi.fn()
      })
    );
    expect(slidesHtml).toContain("12500 EGP");
    expect(slidesHtml).toContain("Highlight Cairo branch growth.");

    const docHtml = renderToString(
      createElement(VerifiedDocView, {
        language: "en",
        metrics: makeSampleState().metrics,
        docBlocks: [
          {
            id: "d1",
            kind: "callout",
            title: "Methodology",
            content: "Verified total {{metric:rev_total}}",
            metricId: "rev_total",
            authoredBy: "user",
            pageBreakBefore: true
          }
        ],
        onUpdateDocBlocks: vi.fn()
      })
    );
    expect(docHtml).toContain("12500 EGP");
    expect(docHtml).toContain("Start new page before section (PDF / DOCX)");

    // Verify all DESKTOP_TUTORIAL_STEPS target selectors match live attributes across Desktop Studio components
    const desktopSources = [
      readFileSync(resolve(__dirname, "DesktopStudio.tsx"), "utf8"),
      readFileSync(resolve(__dirname, "components/WorkbookIntelligenceDrawer.tsx"), "utf8"),
      readFileSync(resolve(__dirname, "components/IntelligenceAndAssistantPanel.tsx"), "utf8")
    ].join("\n");
    for (const step of DESKTOP_TUTORIAL_STEPS) {
      const match = step.targetSelector.match(/data-tutorial-target="([^"]+)"/);
      const targetName = match?.[1];
      expect(targetName).toBeTruthy();
      if (targetName) {
        expect(desktopSources).toContain(`data-tutorial-target="${targetName}"`);
      }
    }
  });

  it("resists catastrophic ReDoS backtracking on vendored picomatch extglobs", () => {
    const require = createRequire(import.meta.url);
    const picomatch = require(resolve(__dirname, "../../../../tools/vendor/picomatch")) as (
      glob: string
    ) => (input: string) => boolean;

    const payloads = ["+(*)", "+(*a)", "*(*(a|b)*)", "+(+(a|b))"];
    const adversarialInput = `${"a".repeat(80)}!`;
    for (const pattern of payloads) {
      const start = performance.now();
      const matcher = picomatch(pattern);
      const matched = matcher(adversarialInput);
      const elapsedMs = performance.now() - start;
      expect(matched).toBe(false);
      expect(elapsedMs).toBeLessThan(50);
    }
  });
});
