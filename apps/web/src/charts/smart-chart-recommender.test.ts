import { describe, it, expect } from "vitest";
import { recommendCharts } from "./smart-chart-recommender";

describe("Smart Chart Recommender (T3.05 / Section 17.2)", () => {
  it("recommends optimal charts with reasons based on column types", () => {
    const cols = [
      { name: "القسم", type: "string" as const },
      { name: "المبيعات", type: "number" as const },
      { name: "التاريخ", type: "date" as const }
    ];

    const recs = recommendCharts(cols);
    expect(recs.length).toBeGreaterThan(0);
    expect(recs[0]?.reasonAr).toBeDefined();
  });
});
