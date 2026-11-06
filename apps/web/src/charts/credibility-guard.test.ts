import { describe, it, expect } from "vitest";
import { checkChartCredibility } from "./credibility-guard";

describe("Credibility Guard (T3.04 / Section 17.5)", () => {
  it("warns when bar chart starts above zero without blocking", () => {
    const spec = {
      type: "bar",
      yAxisKey: "val",
      data: [{ val: 100 }, { val: 120 }]
    };

    const warnings = checkChartCredibility(spec);
    expect(warnings.length).toBe(1);
    expect(warnings[0]?.code).toBe("SF-CHT-006");
  });
});
