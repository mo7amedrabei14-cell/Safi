import { describe, expect, it, vi } from "vitest";
import fs from "node:fs";
import path from "node:path";
import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { DesktopStudio } from "./DesktopStudio";
import { AnomalyPanel } from "./components/AnomalyPanel";
import { CommandBarPanel } from "./components/CommandBarPanel";
import { DashboardChartsView } from "./components/DashboardChartsView";
import { FreeFormBuilderView } from "./components/FreeFormBuilderView";
import { KeynoteSlidesView } from "./components/KeynoteSlidesView";
import { NarrativeTextRenderer, listMissingMetricRefs } from "./components/NarrativeTextRenderer";
import { TemplatesAndProjectsPanel } from "./components/TemplatesAndProjectsPanel";
import { VerifiedDocView } from "./components/VerifiedDocView";
import {
  VerifiedAuditModal,
  VerifiedNumberChip,
  resolveChipStatus
} from "./components/VerifiedNumberChip";
import { WhatIfPanel } from "./components/WhatIfPanel";
import {
  DEFAULT_AI_PROVIDER,
  interpretPromptWithRealAi,
  previewDslCommand,
  validateUntrustedDslPayload
} from "./ai-interpreter";
import type { DesktopMetric, DesktopStudioState } from "./studio-bridge";
import {
  activateScenario,
  addScenario,
  addSourceColumn,
  applyAnomalyFix,
  applyDslCommand,
  compareScenarios,
  createRequestTracker,
  deleteColumn,
  deleteProjectBundle,
  exportCapsule,
  fetchRowsPage,
  instantiateTemplate,
  isDesktopRuntime,
  isNewerOrEqualRevision,
  listProjectBundles,
  listTemplates,
  loadProjectBundle,
  openProject,
  pickCapsuleOutputFile,
  renameColumn,
  renderNarrative,
  renameScenario,
  saveProjectBundle,
  setArtifacts,
  setCell,
  setCellById,
  setScenarioParameter,
  trimColumn
} from "./studio-bridge";
import { DOCK_TABS, STUDIO_TAB } from "../components/layout/FloatingGlassDock";

function sampleMetric(overrides: Partial<DesktopMetric> = {}): DesktopMetric {
  return {
    id: "rev_total",
    name: "Total Revenue",
    value: "350.5",
    unit: "EGP",
    formula: "SUM(amount)",
    formulaVersion: 2,
    sourceId: "sales",
    sourceVersion: 1,
    sourceFingerprint: "abc123source",
    fingerprint: "def456token",
    status: "Verified",
    rows: [1, 2, 3, 4, 5],
    sourceRowIds: Array.from({ length: 30 }, (_, idx) => idx + 1),
    columns: ["amount"],
    columnIds: [2],
    transformationTrace: ["t-sort-1", "t-edit-2"],
    engineVersion: "studio-engine-1.0.0",
    computedAtMs: 12345,
    ...overrides
  };
}

function sampleState(overrides: Partial<DesktopStudioState> = {}): DesktopStudioState {
  return {
    sourceId: "sales",
    revision: 4,
    datasetVersion: 1,
    originFingerprint: "abc123source",
    rowCount: 30,
    columns: [
      { id: 1, name: "region", type: "GovernorateOrCity", originColumnIndex: 0 },
      { id: 2, name: "amount", type: "MoneyAndCurrency", originColumnIndex: 1 }
    ],
    metrics: [sampleMetric()],
    errors: [],
    anomalies: [],
    calculated: [],
    lastRecomputed: ["rev_total"],
    activeScenario: "upside",
    scenarios: [
      { id: "base", name: "Base", parameters: { growth: "1.0" } },
      { id: "upside", name: "Upside Q4", parameters: { growth: "1.5" } }
    ],
    parameters: [{ name: "growth", value: "1.0" }],
    ...overrides
  };
}

describe("desktop studio surface", () => {
  it("refuses to render the workspace outside the desktop app", () => {
    const html = renderToString(createElement(DesktopStudio, { lang: "ar" }));

    expect(isDesktopRuntime()).toBe(false);
    expect(html).toContain("safi-desktop-studio--locked");
    expect(html).toContain("الاستوديو يعمل داخل التطبيق");
    expect(html).not.toContain("safi-desktop-studio__table");
  });

  it("keeps the engine unreachable from a browser tab", async () => {
    await expect(openProject("main", "/tmp/input.csv")).rejects.toThrow("SF-SYS-010");
    await expect(setCell("main", 0, "amount", "1")).rejects.toThrow("SF-SYS-010");
    await expect(exportCapsule("main", "/tmp/out.safi.html", "{}")).rejects.toThrow("SF-SYS-010");
  });

  it("adds the studio tab only through the desktop tab list", () => {
    expect(DOCK_TABS.some((tab) => tab.id === "studio")).toBe(false);
    expect(STUDIO_TAB.id).toBe("studio");
    expect([...DOCK_TABS, STUDIO_TAB].some((tab) => tab.id === "studio")).toBe(true);
  });
});

describe("phase 1: verified number chip & audit provenance", () => {
  it("distinguishes Verified, Stale, Unverified, and Error chip states cleanly", () => {
    const metric = sampleMetric();
    expect(resolveChipStatus(metric, false, null)).toBe("Verified");
    expect(resolveChipStatus(metric, true, null)).toBe("Stale");
    expect(resolveChipStatus(sampleMetric({ status: "Unverified" }), false, null)).toBe(
      "Unverified"
    );
    expect(resolveChipStatus(null, false, "SF-CAL-003")).toBe("Error");

    const verifiedHtml = renderToString(createElement(VerifiedNumberChip, { metric, lang: "en" }));
    expect(verifiedHtml).toContain('data-status="Verified"');
    expect(verifiedHtml).toContain("350.5 EGP");

    const staleHtml = renderToString(
      createElement(VerifiedNumberChip, { metric, recalculating: true, lang: "en" })
    );
    expect(staleHtml).toContain('data-status="Stale"');

    const errorHtml = renderToString(
      createElement(VerifiedNumberChip, {
        metric: null,
        errorCode: "SF-CAL-003",
        fallbackLabel: "Margin",
        lang: "en"
      })
    );
    expect(errorHtml).toContain('data-status="Error"');
    expect(errorHtml).toContain("SF-CAL-003");
  });

  it("renders full provenance and paginated source row references in the audit modal", () => {
    const metric = sampleMetric();
    const html = renderToString(
      createElement(VerifiedAuditModal, {
        metric,
        status: "Verified",
        lang: "en",
        onClose: () => undefined
      })
    );

    expect(html).toContain("SUM(amount)");
    expect(html).toContain("(v2)");
    expect(html).toContain("sales · v1");
    expect(html).toContain("abc123source");
    expect(html).toContain("def456token");
    expect(html).toContain("studio-engine-1.0.0");
    expect(html).toContain("amount (c2)");
    expect(html).toContain("t-sort-1 -&gt; t-edit-2");
    // 30 row IDs paginate at 25 per page -> page 1 / 2 rendered
    expect(html).toContain("r1, r2");
    expect(html).toContain("r25");
    expect(html).not.toContain("r26");
    expect(html).toContain("1 / 2");
  });
});

describe("phase 1: what-if panel & reactive stale-revision guards", () => {
  it("renders scenario controls, numeric sliders, and affected metrics", () => {
    const state = sampleState();
    const html = renderToString(
      createElement(WhatIfPanel, {
        state,
        lang: "en",
        busy: false,
        onDefineParameter: async () => undefined,
        onUpdateParameter: async () => undefined,
        onAddScenario: async () => undefined,
        onRenameScenario: async () => undefined,
        onActivateScenario: async () => undefined,
        onResetScenario: async () => undefined,
        onSetScenarioParameter: async () => undefined,
        onCompareScenarios: async () => []
      })
    );

    expect(html).toContain("Upside Q4");
    expect(html).toContain("whatif-slider-growth");
    expect(html).toContain("1.5");
    expect(html).toContain("rev_total");
  });

  it("rejects out-of-order async revisions and superseded requests", () => {
    const newer = sampleState({ revision: 8 });
    const older = sampleState({ revision: 7 });
    expect(isNewerOrEqualRevision(older, newer)).toBe(false);
    expect(isNewerOrEqualRevision(newer, older)).toBe(true);

    const tracker = createRequestTracker();
    const seq1 = tracker.next();
    const seq2 = tracker.next();
    expect(tracker.isCurrent(seq1)).toBe(false);
    expect(tracker.isCurrent(seq2)).toBe(true);
  });

  it("forwards paginated row queries, stable-id edits, scenario IPC, and capsule save dialog", async () => {
    const invoke = vi.fn(async (command: string) => {
      if (command === "studio_rows_page") {
        return JSON.stringify({
          total_rows: 30,
          filtered_rows: 30,
          offset: 0,
          limit: 50,
          revision: 4,
          rows: []
        });
      }
      if (command === "studio_compare_scenarios") {
        return JSON.stringify([
          { metricId: "rev_total", metricName: "Total Revenue", left: "350.5", right: "525.75" }
        ]);
      }
      if (command === "pick_capsule_output_file") {
        return JSON.stringify("/tmp/out.safi.html");
      }
      return JSON.stringify(sampleState());
    });

    const originalWindow = globalThis.window;
    Object.defineProperty(globalThis, "window", {
      configurable: true,
      value: { __TAURI_INTERNALS__: { invoke } }
    });

    try {
      const page = await fetchRowsPage("main", { offset: 0, limit: 50, search: "Cairo" });
      expect(page.total_rows).toBe(30);
      await setCellById("main", 1, "amount", "500", 4);
      await addScenario("main", "s2", "Downside", { growth: "0.8" });
      await renameScenario("main", "s2", "Downside Q4");
      await activateScenario("main", "s2");
      await setScenarioParameter("main", "s2", "growth", "0.9");
      const cmp = await compareScenarios("main", "base", "upside");
      expect(cmp[0]?.right).toBe("525.75");
      const capsulePath = await pickCapsuleOutputFile();
      expect(capsulePath).toBe("/tmp/out.safi.html");

      expect(invoke.mock.calls.map((c) => c[0])).toEqual([
        "studio_rows_page",
        "studio_set_cell_by_id",
        "studio_add_scenario",
        "studio_rename_scenario",
        "studio_activate_scenario",
        "studio_set_scenario_parameter",
        "studio_compare_scenarios",
        "pick_capsule_output_file"
      ]);
    } finally {
      Object.defineProperty(globalThis, "window", {
        configurable: true,
        value: originalWindow
      });
    }
  });

  it("reports honest SF-AIX-001 blocker when AI is unconfigured and rejects untrusted DSL payloads", async () => {
    const schema = {
      columns: [{ name: "amount", type: "MoneyAndCurrency" }],
      metricIds: ["rev_total"],
      parameterNames: ["growth"]
    };

    // Offline / unconfigured AI never fakes success
    const unconfigured = await interpretPromptWithRealAi(
      "Analyze total revenue by region",
      schema,
      DEFAULT_AI_PROVIDER
    );
    expect(unconfigured).toMatchObject({ ok: false, code: "SF-AIX-001" });

    // Configured endpoint without explicit schema consent is still blocked
    const noConsent = await interpretPromptWithRealAi("Analyze total revenue", schema, {
      kind: "openai_compatible",
      endpointUrl: "http://127.0.0.1:11434/v1/chat/completions",
      modelName: "qwen2.5:7b",
      consentedToSchemaContext: false
    });
    expect(noConsent).toMatchObject({ ok: false, code: "SF-AIX-001" });

    // Consented provider sends schema-only payload (no raw rows) and untrusted validator verifies output
    const fetchMock = vi.fn(async (_url: string, init?: RequestInit) => {
      const sentBody = String(init?.body ?? "");
      expect(sentBody).not.toContain("Cairo");
      expect(sentBody).toContain("amount");
      return {
        ok: true,
        json: async () => ({
          choices: [
            {
              message: {
                content: JSON.stringify({
                  op: "define_metric",
                  id: "avg_rev",
                  name: "Average",
                  unit: "EGP",
                  formula: "AVG(amount)"
                })
              }
            }
          ]
        })
      } as unknown as Response;
    });
    vi.stubGlobal("fetch", fetchMock);
    try {
      const interpreted = await interpretPromptWithRealAi("Calculate average revenue", schema, {
        kind: "openai_compatible",
        endpointUrl: "http://127.0.0.1:11434/v1/chat/completions",
        modelName: "qwen2.5:7b",
        consentedToSchemaContext: true
      });
      expect(interpreted.ok).toBe(true);
      if (interpreted.ok) {
        expect(interpreted.command.op).toBe("define_metric");
        expect(previewDslCommand(interpreted.command, "ar")).toContain("AVG(amount)");
      }
    } finally {
      vi.unstubAllGlobals();
    }

    const rejected = validateUntrustedDslPayload(
      JSON.stringify({
        op: "define_metric",
        id: "x",
        name: "X",
        unit: null,
        formula: "1",
        extra: true
      })
    );
    expect(rejected).toMatchObject({ ok: false, code: "SF-AIX-002" });
  });

  it("renders Phase 2 CommandBarPanel, AnomalyPanel with RowIds, and NarrativeTextRenderer", async () => {
    const metric = sampleMetric();
    const metricsById = new Map([[metric.id, metric]]);

    const cmdHtml = renderToString(
      createElement(CommandBarPanel, {
        language: "en",
        projectLoaded: true,
        schema: {
          columns: [{ name: "amount", type: "MoneyAndCurrency" }],
          metricIds: ["rev_total"],
          parameterNames: []
        },
        onExecuteDsl: vi.fn(async () => {})
      })
    );
    expect(cmdHtml).toContain("AI Provider: Unconfigured");
    expect(cmdHtml).toContain("Approve &amp; Execute in Rust");

    const anomHtml = renderToString(
      createElement(AnomalyPanel, {
        language: "en",
        anomalies: [
          {
            kind: "MissingValue",
            messageKey: "studio.anomaly.missing_value",
            methodKey: "studio.method.empty_cell",
            suggestionKey: "studio.fix.fill_or_remove_missing",
            column: "region",
            rows: [1],
            rowIds: [42]
          }
        ],
        isMutating: false,
        onApplyFix: vi.fn(async () => {})
      })
    );
    expect(anomHtml).toContain("#42");
    expect(anomHtml).toContain("studio.method.empty_cell");
    expect(anomHtml).toContain("Fix 1 missing cells");

    const narrativeTpl = "Total is {{metric:rev_total}} and missing {{metric:ghost_metric}}.";
    expect(listMissingMetricRefs(narrativeTpl, metricsById)).toEqual(["ghost_metric"]);
    const narHtml = renderToString(
      createElement(NarrativeTextRenderer, {
        template: narrativeTpl,
        metricsById,
        language: "en",
        onUnlinkMetric: vi.fn()
      })
    );
    expect(narHtml).toContain("350.5 EGP");
    expect(narHtml).toContain("Missing: ghost_metric");
    expect(narHtml).toContain("Unlink");

    // Verify Phase 2 IPC wrappers
    const invoke = vi.fn(async (command: string) => {
      if (command === "studio_narrative") {
        return JSON.stringify({
          text: "Total 350.5",
          missing: [],
          references: [
            { id: "rev_total", value: "350.5", unit: "EGP", status: "Verified", fingerprint: "f" }
          ]
        });
      }
      return JSON.stringify(sampleState());
    });
    const originalWindow = globalThis.window;
    Object.defineProperty(globalThis, "window", {
      configurable: true,
      value: { __TAURI_INTERNALS__: { invoke } }
    });
    try {
      await applyDslCommand("main", '{"op":"sort_rows","column":"amount","ascending":true}');
      await applyAnomalyFix("main", { action: "drop_duplicate_rows", row_ids: [3] }, 4);
      const nar = await renderNarrative("main", "Total {{metric:rev_total}}");
      expect(nar.references[0]?.unit).toBe("EGP");
      expect(invoke.mock.calls.map((c) => c[0])).toEqual([
        "studio_apply_command",
        "studio_apply_anomaly_fix",
        "studio_narrative"
      ]);
    } finally {
      Object.defineProperty(globalThis, "window", {
        configurable: true,
        value: originalWindow
      });
    }
  });
});

describe("phase 3 & 4: Quad-Studio editors, artifacts persistence, templates & bundles", () => {
  it("renders Dashboard, Keynote Slides, Smart Doc, Free-Form Canvas, and Templates & Projects", () => {
    const metric = sampleMetric();
    const state = sampleState();

    const dashHtml = renderToString(
      createElement(DashboardChartsView, {
        language: "en",
        project: "main",
        revision: state.revision,
        activeScenario: state.activeScenario,
        columns: state.columns,
        metrics: state.metrics,
        charts: [
          {
            id: "c1",
            title: "Revenue by region",
            function: "SUM",
            column: "amount",
            groupBy: "region"
          }
        ],
        onUpdateCharts: vi.fn(),
        onCrossFilter: vi.fn()
      })
    );
    expect(dashHtml).toContain("Revenue by region");
    expect(dashHtml).toContain("350.5 EGP");
    expect(dashHtml).toContain("SUM(amount)");

    const slidesHtml = renderToString(
      createElement(KeynoteSlidesView, {
        language: "en",
        metrics: state.metrics,
        slides: [
          {
            id: "s1",
            title: "Executive Summary",
            body: "Verified {{metric:rev_total}} vs missing {{metric:ghost}}",
            metricId: "rev_total",
            chartIndex: null,
            layout: "title-body"
          }
        ],
        onUpdateSlides: vi.fn()
      })
    );
    expect(slidesHtml).toContain("Executive Summary");
    expect(slidesHtml).toContain("350.5 EGP");
    expect(slidesHtml).toContain("Missing: ghost");

    const docHtml = renderToString(
      createElement(VerifiedDocView, {
        language: "en",
        metrics: state.metrics,
        docBlocks: [
          {
            id: "d1",
            kind: "callout",
            title: "Board Note",
            content: "Baseline confirmed at {{metric:rev_total}}",
            metricId: "rev_total",
            authoredBy: "ai"
          }
        ],
        onUpdateDocBlocks: vi.fn()
      })
    );
    expect(docHtml).toContain("Board Note");
    expect(docHtml).toContain("AI-Assisted Draft");
    expect(docHtml).toContain("350.5 EGP");

    const freeformHtml = renderToString(
      createElement(FreeFormBuilderView, {
        language: "en",
        metrics: state.metrics,
        freeformBlocks: [
          {
            id: "ff1",
            kind: "kpi",
            title: "Revenue Tile",
            body: "Headline {{metric:rev_total}}",
            metricId: "rev_total",
            assetId: null,
            col: 1,
            row: 1,
            colSpan: 4,
            rowSpan: 2,
            zIndex: 3
          }
        ],
        assets: [{ id: "img1", name: "Logo", dataUri: "data:image/png;base64,iVBORw0KGgo=" }],
        onUpdateFreeForm: vi.fn()
      })
    );
    expect(freeformHtml).toContain("Revenue Tile");
    expect(freeformHtml).toContain("350.5 EGP");
    expect(freeformHtml).toContain("W:4");
    expect(freeformHtml).toContain("Z:3");

    const templatesHtml = renderToString(
      createElement(TemplatesAndProjectsPanel, {
        language: "en",
        project: "main",
        projectLoaded: true,
        metrics: [metric],
        currentTemplateConfig: null,
        onApplyTemplateToWorkspace: vi.fn(),
        onSaveBundle: vi.fn(async () => {}),
        onLoadBundle: vi.fn(async () => {})
      })
    );
    expect(templatesHtml).toContain("Living Templates Gallery (5 Built-In)");
    expect(templatesHtml).toContain("Save Project");
  });

  it("forwards Phase 3/4 column ops, artifact persistence, templates, and bundle IPC commands", async () => {
    const invoke = vi.fn(async (command: string) => {
      if (command === "studio_list_templates") {
        return JSON.stringify([
          {
            id: "boardroom-keynote",
            nameKey: "studio.template.boardroom",
            sections: [{ headingKey: "sec.head", role: "headline" }]
          }
        ]);
      }
      if (command === "studio_instantiate_template") {
        return JSON.stringify([
          { headingKey: "sec.head", role: "headline", token: sampleMetric() }
        ]);
      }
      if (command === "studio_save_project") {
        return JSON.stringify({
          id: "q4_board",
          displayName: "Q4 Board Pack",
          updatedAtMs: 99999,
          validChecksum: true
        });
      }
      if (command === "studio_list_projects") {
        return JSON.stringify([
          {
            id: "q4_board",
            displayName: "Q4 Board Pack",
            updatedAtMs: 99999,
            validChecksum: true
          }
        ]);
      }
      return JSON.stringify(sampleState());
    });

    const originalWindow = globalThis.window;
    Object.defineProperty(globalThis, "window", {
      configurable: true,
      value: { __TAURI_INTERNALS__: { invoke } }
    });

    try {
      await addSourceColumn("main", "notes");
      await renameColumn("main", "notes", "comments");
      await trimColumn("main", "comments");
      await deleteColumn("main", "comments");
      await setArtifacts("main", {
        charts: [],
        slides: [],
        docBlocks: [],
        freeformBlocks: [],
        assets: [],
        templateConfig: null
      });
      const templates = await listTemplates();
      expect(templates[0]?.id).toBe("boardroom-keynote");
      const sections = await instantiateTemplate("main", "boardroom-keynote", {
        headline: "rev_total"
      });
      expect(sections[0]?.token.id).toBe("rev_total");
      const saved = await saveProjectBundle("main", "q4_board", "Q4 Board Pack");
      expect(saved.id).toBe("q4_board");
      const listed = await listProjectBundles();
      expect(listed[0]?.validChecksum).toBe(true);
      await loadProjectBundle("main", "q4_board");

      expect(invoke.mock.calls.map((c) => c[0])).toEqual([
        "studio_add_source_column",
        "studio_rename_column",
        "studio_trim_column",
        "studio_delete_column",
        "studio_set_artifacts",
        "studio_list_templates",
        "studio_instantiate_template",
        "studio_save_project",
        "studio_list_projects",
        "studio_load_project"
      ]);
    } finally {
      Object.defineProperty(globalThis, "window", {
        configurable: true,
        value: originalWindow
      });
    }
  });
});

describe("phase 5: browser security boundary, bilingual RTL/LTR, accessibility & reduced motion", () => {
  it("blocks every Phase 3 & 4 Studio IPC call with SF-SYS-010 when running in a public browser", async () => {
    await expect(addSourceColumn("main", "c")).rejects.toThrow("SF-SYS-010");
    await expect(renameColumn("main", "a", "b")).rejects.toThrow("SF-SYS-010");
    await expect(deleteColumn("main", "a")).rejects.toThrow("SF-SYS-010");
    await expect(trimColumn("main", "a")).rejects.toThrow("SF-SYS-010");
    await expect(
      setArtifacts("main", {
        charts: [],
        slides: [],
        docBlocks: [],
        freeformBlocks: [],
        assets: [],
        templateConfig: null
      })
    ).rejects.toThrow("SF-SYS-010");
    await expect(saveProjectBundle("main", "p", "P")).rejects.toThrow("SF-SYS-010");
    await expect(listProjectBundles()).rejects.toThrow("SF-SYS-010");
    await expect(loadProjectBundle("main", "p")).rejects.toThrow("SF-SYS-010");
    await expect(deleteProjectBundle("p")).rejects.toThrow("SF-SYS-010");
    await expect(listTemplates()).rejects.toThrow("SF-SYS-010");
    await expect(instantiateTemplate("main", "boardroom-keynote", {})).rejects.toThrow(
      "SF-SYS-010"
    );
  });

  it("renders clean bilingual EN/AR UI, accessible ARIA roles, and reduced-motion rules", () => {
    const state = sampleState();
    const arHtml = renderToString(
      createElement(DashboardChartsView, {
        language: "ar",
        project: "main",
        revision: state.revision,
        activeScenario: null,
        columns: state.columns,
        metrics: state.metrics,
        charts: [],
        onUpdateCharts: vi.fn()
      })
    );
    expect(arHtml).toContain("لوحة المؤشرات والرسوم التفاعلية");
    expect(arHtml).toContain("إضافة رسم");

    const enLocked = renderToString(createElement(DesktopStudio, { lang: "en" }));
    expect(enLocked).toContain("The Studio runs inside the app");
    expect(enLocked).toContain('role="note"');

    const modalHtml = renderToString(
      createElement(VerifiedAuditModal, {
        metric: sampleMetric(),
        status: "Verified",
        lang: "ar",
        onClose: () => undefined
      })
    );
    expect(modalHtml).toContain('role="dialog"');

    const css = fs.readFileSync(path.join(__dirname, "desktop-studio.css"), "utf8");
    expect(css).toContain("@media (prefers-reduced-motion: reduce)");
  });
});
