import { describe, it, expect } from "vitest";
import { renderToString } from "react-dom/server";
import { ChartStudio } from "./ChartStudio";

describe("ChartStudio Component (T3.03 / Section 17.2)", () => {
  it("renders chart studio successfully", () => {
    const spec = {
      version: "1.0",
      type: "bar" as const,
      titleAr: "اختبار الاستديو",
      xAxisKey: "cat",
      yAxisKey: "val",
      data: [{ cat: "أ", val: 10 }]
    };

    const html = renderToString(<ChartStudio initialSpec={spec} />);
    expect(html).toContain("استديو الرسوم البيانية");
    expect(html).toContain("اختبار الاستديو");
  });
});
