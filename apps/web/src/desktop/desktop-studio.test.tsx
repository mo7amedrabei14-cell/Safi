import { describe, expect, it, vi } from "vitest";
import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { DesktopStudio } from "./DesktopStudio";
import { VerifiedAuditModal, VerifiedNumberChip, resolveChipStatus } from "./components/VerifiedNumberChip";
import { WhatIfPanel } from "./components/WhatIfPanel";
import type { DesktopMetric, DesktopStudioState } from "./studio-bridge";
import {
  activateScenario,
  addScenario,
  compareScenarios,
  createRequestTracker,
  exportCapsule,
  fetchRowsPage,
  isDesktopRuntime,
  isNewerOrEqualRevision,
  openProject,
  pickCapsuleOutputFile,
  renameScenario,
  setCell,
  setCellById,
  setScenarioParameter
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
    expect(verifiedHtml).toContain("data-status=\"Verified\"");
    expect(verifiedHtml).toContain("350.5 EGP");

    const staleHtml = renderToString(
      createElement(VerifiedNumberChip, { metric, recalculating: true, lang: "en" })
    );
    expect(staleHtml).toContain("data-status=\"Stale\"");

    const errorHtml = renderToString(
      createElement(VerifiedNumberChip, {
        metric: null,
        errorCode: "SF-CAL-003",
        fallbackLabel: "Margin",
        lang: "en"
      })
    );
    expect(errorHtml).toContain("data-status=\"Error\"");
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
});
