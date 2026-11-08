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
    expect(svg1).toContain('fill="#0B7C8C"');
  });

  it("generates a deterministic polyline preview for line charts", () => {
    const spec: ChartSpec = {
      version: "1.0",
      type: "line",
      titleAr: "المبيعات الشهرية",
      xAxisKey: "month",
      yAxisKey: "value",
      data: [
        { month: "يناير", value: 20 },
        { month: "فبراير", value: 50 },
        { month: "مارس", value: 80 }
      ]
    };

    const svg = generateDeterministicSvg(spec);
    expect(svg).toBe(generateDeterministicSvg(spec));
    expect(svg).toContain("<polyline");
    expect(svg).toContain('stroke="#0B7C8C"');
    expect(contrastWithWhite("#0B7C8C")).toBeGreaterThanOrEqual(3);
  });

  it.each(["pie", "donut"] as const)("generates deterministic accessible %s segments", (type) => {
    const spec: ChartSpec = {
      version: "1.0",
      type,
      titleAr: "توزيع المبيعات",
      xAxisKey: "category",
      yAxisKey: "value",
      data: [
        { category: "أ", value: 10 },
        { category: "ب", value: 15 },
        { category: "ج", value: 20 },
        { category: "د", value: 20 },
        { category: "هـ", value: 25 },
        { category: "و", value: 10 }
      ]
    };

    const svg = generateDeterministicSvg(spec);
    expect(svg).toBe(generateDeterministicSvg(spec));
    expect(svg.match(/stroke-dasharray=/g)).toHaveLength(6);
    const colors = Array.from(
      svg.matchAll(/stroke="(#[0-9A-Fa-f]{6})"/g),
      (match) => match[1]
    ).filter((color): color is string => color !== undefined);
    expect(colors).toHaveLength(6);
    for (const color of colors) {
      expect(contrastWithWhite(color)).toBeGreaterThanOrEqual(3);
    }
  });
});

function contrastWithWhite(hex: string): number {
  const channel = (offset: number) => {
    const value = Number.parseInt(hex.slice(offset, offset + 2), 16) / 255;
    return value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  };
  const luminance = 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5);
  return 1.05 / (luminance + 0.05);
}
