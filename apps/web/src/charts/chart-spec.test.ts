import { describe, it, expect } from "vitest";
import { validateChartSpec, generateDeterministicSvg, type ChartSpec } from "./chart-spec";

describe("Chart Studio Spec & Deterministic Renderer (T3.01 / Section 17.1)", () => {
  it("validates correct chart spec successfully", () => {
    const spec: ChartSpec = {
      version: "1.0",
      type: "bar",
      titleAr: "مبيعات الأقسام",
      xAxisKey: "category",
      yAxisKey: "value",
      data: [
        { category: "أ", value: 50 },
        { category: "ب", value: 80 }
      ]
    };

    const res = validateChartSpec(spec);
    expect(res.valid).toBe(true);
    expect(res.errors.length).toBe(0);
  });

  it("generates deterministic SVG markup", () => {
    const spec: ChartSpec = {
      version: "1.0",
      type: "bar",
      titleAr: "مبيعات الأقسام",
      xAxisKey: "category",
      yAxisKey: "value",
      data: [{ category: "أ", value: 50 }]
    };

    const svg1 = generateDeterministicSvg(spec);
    const svg2 = generateDeterministicSvg(spec);
    expect(svg1).toBe(svg2);
    expect(svg1).toContain("مبيعات الأقسام");
  });
});
