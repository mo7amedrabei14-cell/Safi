import { describe, it, expect } from "vitest";
import { splitColumn } from "./advanced-cleaning";

describe("Advanced Cleaning Operations (T2.12)", () => {
  it("splits column by delimiter correctly", () => {
    const rows = [
      ["John Doe", "25"],
      ["Jane Smith", "30"]
    ];
    const res = splitColumn(rows, 0, " ");
    expect(res.success).toBe(true);
    expect(res.processedRows[0]).toEqual(["John", "Doe", "25"]);
  });
});
