import { describe, it, expect } from "vitest";
import { generateDataDictionaryMarkdown, type ColumnDictionaryEntry } from "./data-dictionary";

describe("Automated Data Dictionary Generator (Section 16.4)", () => {
  it("generates markdown data dictionary without manual intervention", () => {
    const entries: ColumnDictionaryEntry[] = [
      {
        columnName: "الاسم",
        inferredType: "Text",
        completenessRate: 1.0,
        meaning: "اسم العميل الكامل"
      },
      {
        columnName: "المبلغ",
        inferredType: "Number",
        completenessRate: 0.95,
        meaning: "إجمالي مبلغ الفاتورة"
      }
    ];

    const md = generateDataDictionaryMarkdown("ملف نوفمبر", entries);
    expect(md).toContain("قاموس البيانات التلقائي");
    expect(md).toContain("الاسم");
    expect(md).toContain("100%");
    expect(md).toContain("إجمالي مبلغ الفاتورة");
  });
});
