import { describe, it, expect } from "vitest";
import { validateChartSpec, generateDeterministicSvg } from "./charts/chart-spec";
import { checkChartCredibility } from "./charts/credibility-guard";
import { recommendCharts } from "./charts/smart-chart-recommender";

describe("Slice S3 Comprehensive Integration Test (T3.06)", () => {
  it("integrates chart spec, credibility guard, and smart recommender successfully", () => {
    const spec = {
      version: "1.0",
      type: "bar" as const,
      titleAr: "مخطط تكاملي للشريحة الثالثة",
      xAxisKey: "cat",
      yAxisKey: "val",
      data: [{ cat: "أ", val: 10 }]
    };

    const validation = validateChartSpec(spec);
    expect(validation.valid).toBe(true);

    const svg = generateDeterministicSvg(spec);
    expect(svg).toContain("مخطط تكاملي للشريحة الثالثة");

    const warnings = checkChartCredibility({ type: "bar", yAxisKey: "val", data: [{ val: 10 }] });
    expect(warnings.length).toBe(1); // 1 warning because minVal = 10 > 0

    const recs = recommendCharts([
      { name: "cat", type: "string" },
      { name: "val", type: "number" }
    ]);
    expect(recs.length).toBeGreaterThan(0);
  });
});
