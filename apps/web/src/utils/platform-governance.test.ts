import { describe, expect, it } from "vitest";
import {
  INTERACTIVE_TUTORIAL_STEPS,
  advanceTutorialState,
  buildPrivacySafeLogEntry,
  computeVirtualGridWindow,
  createDefaultPrivacyConsentState,
  createInitialTutorialState,
  getTooltipDescriptor,
  maskPiiValue,
  prepareSanitizedExternalAiPayload,
  processRowsInChunks,
  runIngestionAndProfilingBenchmark
} from "./platform-governance";

describe("Platform Governance: Tooltips, Tutorials, Privacy/Security & Performance (Phases 10, 11, 12)", () => {
  it("provides bilingual contextual tooltips and an 8-step replayable tutorial state machine (Phase 10)", () => {
    expect(INTERACTIVE_TUTORIAL_STEPS).toHaveLength(8);
    const enTip = getTooltipDescriptor("epistemic.ConfirmedFact", "en");
    const arTip = getTooltipDescriptor("epistemic.ConfirmedFact", "ar");
    expect(enTip?.title).toBe("Confirmed Fact");
    expect(arTip?.title).toBe("حقيقة مؤكدة");
    expect(enTip?.whatItDoes.length).toBeGreaterThan(10);
    expect(arTip?.impact.length).toBeGreaterThan(10);

    let tut = createInitialTutorialState(true);
    expect(tut.isOpen).toBe(true);
    expect(tut.currentStepIndex).toBe(0);

    tut = advanceTutorialState(tut, { type: "NEXT" });
    expect(tut.currentStepIndex).toBe(1);

    tut = advanceTutorialState(tut, { type: "SKIP" });
    expect(tut.isOpen).toBe(false);
    expect(tut.skipped).toBe(true);

    // Replay from Help menu
    tut = advanceTutorialState(tut, { type: "REPLAY" });
    expect(tut.isOpen).toBe(true);
    expect(tut.currentStepIndex).toBe(0);
    expect(tut.replayCount).toBe(1);

    // Jump to last step and complete
    tut = advanceTutorialState(tut, { type: "JUMP_TO", index: 7 });
    tut = advanceTutorialState(tut, { type: "NEXT" });
    expect(tut.isOpen).toBe(false);
    expect(tut.completed).toBe(true);
  });

  it("enforces StrictlyLocal privacy consent gate, masks PII, and strips cell/column data from logs (Phase 11)", () => {
    const defaultConsent = createDefaultPrivacyConsentState();
    const blocked = prepareSanitizedExternalAiPayload(defaultConsent, [
      { index: 0, role: "Measure", type: "CurrencyAmount" }
    ]);
    expect(blocked.allowed).toBe(false);

    const granted = prepareSanitizedExternalAiPayload(
      {
        ...defaultConsent,
        mode: "CloudAssistedWithConsent",
        userGrantedConsent: true
      },
      [{ index: 0, role: "Measure", type: "CurrencyAmount" }]
    );
    expect(granted.allowed).toBe(true);
    expect(granted.sanitizedPayload).toContain("CurrencyAmount");

    expect(maskPiiValue("ahmed.ali@company.eg")).toBe("a***@company.eg");
    expect(maskPiiValue("+201012345678")).toBe("+20******78");
    expect(maskPiiValue("EG380019000500000000263180002")).toBe("EG38********0002");

    const safeLog = buildPrivacySafeLogEntry({
      eventCode: "SF_PROF_001",
      rowCount: 1500,
      colCount: 12,
      durationMs: 24.567
    });
    expect(safeLog).toEqual({
      eventCode: "SF_PROF_001",
      rowCount: 1500,
      colCount: 12,
      durationMs: 24.57
    });
  });

  it("processes rows in cancellable chunks, computes virtual windows, and passes performance benchmarks (Phase 12)", () => {
    const progressUpdates: number[] = [];
    const results = processRowsInChunks({
      rows: [1, 2, 3, 4, 5],
      chunkSize: 2,
      mapRow: (v) => v * 10,
      onProgress: (_done, _total, pct) => progressUpdates.push(pct)
    });
    expect(results).toEqual([10, 20, 30, 40, 50]);
    expect(progressUpdates).toEqual([40, 80, 100]);

    const abortCtrl = new AbortController();
    abortCtrl.abort();
    expect(() =>
      processRowsInChunks({
        rows: [1, 2, 3],
        chunkSize: 1,
        mapRow: (v) => v,
        signal: abortCtrl.signal
      })
    ).toThrow(/ChunkedProcessingCancelled/);

    const win = computeVirtualGridWindow({
      totalRows: 100_000,
      rowHeightPx: 32,
      scrollTopPx: 32_000,
      viewportHeightPx: 640,
      overscanRows: 5
    });
    expect(win.startRow).toBe(995);
    expect(win.endRow).toBe(1025);
    expect(win.visibleRowCount).toBe(31);

    const bench = runIngestionAndProfilingBenchmark(500);
    expect(bench.rowCount).toBe(500);
    expect(bench.tablesExtracted).toBe(1);
    expect(bench.kpisGenerated).toBe(2);
    expect(bench.elapsedMs).toBeLessThan(2000);
  });
});
