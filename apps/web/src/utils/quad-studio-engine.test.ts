import { describe, expect, it } from "vitest";
import { ingestWorkbookStructure } from "./workbook-ingestion";
import { buildSemanticWorkbookModel } from "./semantic-intelligence";
import { buildVerifiedAnalysisReport } from "./verified-analysis-engine";
import {
  ALL_REQUIRED_DOCUMENT_SECTIONS,
  ALL_SUPPORTED_CHART_TYPES,
  applyDashboardEdit,
  applyDocumentEdit,
  applyPresentationEdit,
  buildInteractiveHtmlPresentation,
  generateAutoDashboard,
  generateDocumentReport,
  generatePresentationDeck,
  migrateAndValidateTemplate,
  renderChartSvg
} from "./quad-studio-engine";

describe("Unified Quad-Studio Engine (Phases 5, 6, 7, 8)", () => {
  const sampleIngestion = ingestWorkbookStructure([
    {
      name: "Sales_2026",
      grid: [
        ["Branch", "Product", "Date", "Revenue_EGP", "Cost_EGP"],
        ["Cairo", "Enterprise", "2026-01-01", "5000", "3000"],
        ["Cairo", "Pro", "2026-02-01", "3000", "1800"],
        ["Alexandria", "Enterprise", "2026-03-01", "4000", "2500"],
        ["Giza", "Starter", "2026-04-01", "2000", "1200"],
        ["Total", "", "", "14000", "8500"]
      ]
    }
  ]);
  const sampleModel = buildSemanticWorkbookModel(sampleIngestion);
  const sampleAnalysis = buildVerifiedAnalysisReport(sampleModel);

  it("supports all 21 required chart types and full Dashboard editing (cross-filter, drill-down, calculated metrics, undo/redo)", () => {
    expect(ALL_SUPPORTED_CHART_TYPES).toHaveLength(21);
    for (const chartType of ALL_SUPPORTED_CHART_TYPES) {
      const svg = renderChartSvg(chartType, "Test Visual", [
        { label: "Cairo", value: 8000 },
        { label: "Alexandria", value: 4000 }
      ]);
      expect(svg).toContain(`data-chart-type="${chartType}"`);
      expect(svg).toContain("<svg");
    }

    let dash = generateAutoDashboard(sampleModel, sampleAnalysis);
    expect(dash.widgets.length).toBeGreaterThanOrEqual(5);

    const rankingWidget = dash.widgets.find((w) => w.widgetId === "w_chart_ranking")!;
    expect(rankingWidget.dataPoints[0]).toMatchObject({ label: "Cairo", value: 8000 });

    // Cross-filtering by Branch = Alexandria filters sibling trend chart to Alexandria (4000)
    dash = applyDashboardEdit(dash, sampleModel, {
      type: "SET_CROSS_FILTER",
      selection: {
        sourceWidgetId: "w_chart_ranking",
        dimensionColumn: "Branch",
        selectedValue: "Alexandria"
      }
    });
    const trendAfterCross = dash.widgets.find((w) => w.widgetId === "w_chart_trend")!;
    expect(trendAfterCross.dataPoints.reduce((a, b) => a + b.value, 0)).toBe(4000);

    // Clear cross-filter
    dash = applyDashboardEdit(dash, sampleModel, {
      type: "SET_CROSS_FILTER",
      selection: undefined
    });

    // Add calculated metric Profit = Revenue_EGP - Cost_EGP (14000 - 8500 = 5500)
    dash = applyDashboardEdit(dash, sampleModel, {
      type: "ADD_CALCULATED_METRIC",
      metric: {
        metricId: "calc_profit",
        name: "Net_Profit_EGP",
        formulaExpression: "Revenue_EGP - Cost_EGP",
        leftColumn: "Revenue_EGP",
        operator: "-",
        rightColumn: "Cost_EGP",
        unit: "EGP"
      }
    });
    dash = applyDashboardEdit(dash, sampleModel, {
      type: "UPDATE_WIDGET",
      widgetId: "w_chart_ranking",
      patch: { measureColumn: "Net_Profit_EGP", chartType: "Bar" }
    });
    const updatedRanking = dash.widgets.find((w) => w.widgetId === "w_chart_ranking")!;
    expect(updatedRanking.chartType).toBe("Bar");
    expect(updatedRanking.dataPoints.reduce((a, b) => a + b.value, 0)).toBe(5500);

    // Duplicate & Undo/Redo
    const countBeforeDup = dash.widgets.length;
    dash = applyDashboardEdit(dash, sampleModel, {
      type: "DUPLICATE_WIDGET",
      widgetId: "w_chart_ranking"
    });
    expect(dash.widgets).toHaveLength(countBeforeDup + 1);

    dash = applyDashboardEdit(dash, sampleModel, { type: "UNDO" });
    expect(dash.widgets).toHaveLength(countBeforeDup);

    dash = applyDashboardEdit(dash, sampleModel, { type: "REDO" });
    expect(dash.widgets).toHaveLength(countBeforeDup + 1);
  });

  it("generates a 10-slide Presentation Deck, supports slide editing, and builds fullscreen HTML playback", () => {
    let deck = generatePresentationDeck(sampleModel, sampleAnalysis);
    expect(deck.slides).toHaveLength(10);
    expect(deck.slides[0]?.sectionRole).toBe("Cover");
    expect(deck.slides[9]?.sectionRole).toBe("AppendixMethodology");

    deck = applyPresentationEdit(deck, {
      type: "UPDATE_SLIDE",
      slideId: "slide_1_cover",
      patch: { presenterNotes: "Updated custom speaker notes for board meeting" }
    });
    expect(deck.slides[0]?.presenterNotes).toContain("Updated custom speaker notes");

    deck = applyPresentationEdit(deck, {
      type: "DUPLICATE_SLIDE",
      slideId: "slide_2_exec_summary"
    });
    expect(deck.slides).toHaveLength(11);

    const html = buildInteractiveHtmlPresentation(deck, "rtl");
    expect(html).toContain('dir="rtl"');
    expect(html).toContain('data-safi-presentation-player="true"');
    expect(html).toContain('data-slide-count="11"');
    expect(html).toContain("Updated custom speaker notes for board meeting");
  });

  it("generates all 13 required Document Studio sections, supports editing, and guarantees identical KPIs across all 3 Studios", () => {
    const dash = generateAutoDashboard(sampleModel, sampleAnalysis);
    const deck = generatePresentationDeck(sampleModel, sampleAnalysis);
    let doc = generateDocumentReport(sampleModel, sampleAnalysis);

    expect(doc.sections.map((s) => s.kind)).toEqual(ALL_REQUIRED_DOCUMENT_SECTIONS);

    // Cross-Studio numerical consistency check:
    // Total Revenue_EGP (14,000 EGP) and Total Cost_EGP (8,500 EGP) must match across Dashboard, Presentation, and Document!
    const dashKpiValues = dash.widgets
      .filter((w) => w.kind === "kpi")
      .map((w) => w.kpiMetric?.formattedValue);
    const deckKpiValues = deck.slides
      .find((s) => s.sectionRole === "KeyMetrics")!
      .kpis.map((k) => k.formattedValue);
    const docKpiValues = doc.sections
      .find((s) => s.kind === "KpiSummary")!
      .kpis!.map((k) => k.formattedValue);

    expect(dashKpiValues).toEqual(["14,000 EGP", "8,500 EGP"]);
    expect(deckKpiValues).toEqual(dashKpiValues);
    expect(docKpiValues).toEqual(dashKpiValues);

    doc = applyDocumentEdit(doc, {
      type: "UPDATE_METADATA",
      patch: { headerText: "Custom Board Header", showPageNumbers: true }
    });
    expect(doc.headerText).toBe("Custom Board Header");
  });

  it("migrates legacy v1 templates safely to UnifiedStudioTemplate schemaVersion 2", () => {
    const migrated = migrateAndValidateTemplate({
      id: "legacy_v1",
      name: "Legacy Corporate",
      tokens: { primaryColor: "#1d4ed8", spacingScale: "compact" }
    });
    expect(migrated.schemaVersion).toBe(2);
    expect(migrated.templateId).toBe("legacy_v1");
    expect(migrated.nameEn).toBe("Legacy Corporate");
    expect(migrated.tokens.primaryColor).toBe("#1d4ed8");
    expect(migrated.tokens.spacingScale).toBe("compact");
  });
});
