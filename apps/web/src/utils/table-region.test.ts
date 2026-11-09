import { describe, it, expect } from "vitest";
import { detectTableRegion } from "./table-region";

describe("Table Region Detection & Totals Separation (Section 16.2)", () => {
  it("detects table region, ignores top decorative rows, and separates totals rows", () => {
    const rawSheet = [
      ["شركة صافي للبيانات — تقرير شهر نوفمبر"],
      [""],
      ["الاسم", "القسم", "المبلغ"],
      ["أحمد", "المبيعات", "5000"],
      ["فاطمة", "التطوير", "7000"],
      ["المجموع الإجمالي", "", "12000"]
    ];

    const analysis = detectTableRegion(rawSheet);
    expect(analysis.region.startRow).toBe(2);
    expect(analysis.headers).toEqual(["الاسم", "القسم", "المبلغ"]);
    expect(analysis.totalRows).toContain(5);
    expect(analysis.cleanedDataRowsIndex).toEqual([3, 4]);
  });
});
