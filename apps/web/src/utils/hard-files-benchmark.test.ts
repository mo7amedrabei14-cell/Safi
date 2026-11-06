import { describe, it, expect } from "vitest";
import { runHardFilesBenchmark } from "./hard-files-benchmark";

describe("Hard Files Benchmark (Section 16.6 / T2.15)", () => {
  it("measures type accuracy and confidence alert rate successfully", () => {
    const samples = [
      {
        name: "test-arabic-file.csv",
        expectedTypes: ["string", "number", "date"],
        detectedTypes: ["string", "number", "date"],
        confidenceScores: [0.95, 0.88, 0.6] // 1 low confidence
      }
    ];

    const result = runHardFilesBenchmark(samples);
    expect(result.totalFilesTested).toBe(1);
    expect(result.typeAccuracy).toBe(1.0);
    expect(result.lowConfidenceAlertRate).toBeCloseTo(0.333, 2);
    expect(result.passed).toBe(true);
  });
});
