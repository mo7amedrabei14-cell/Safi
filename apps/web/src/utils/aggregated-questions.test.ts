import { describe, it, expect } from "vitest";
import { aggregateAndRankQuestions, type DataIssueItem } from "./aggregated-questions";

describe("Aggregated Questions & Impact-Ranked Review (Section 16.4)", () => {
  it("aggregates issues, ranks them by impact score, limits to 3 max, and respects deferral", () => {
    const issues: DataIssueItem[] = [
      { id: "1", category: "ambiguity", titleAr: "تواريخ غامضة", impactScore: 50, deferred: false },
      {
        id: "2",
        category: "schema",
        titleAr: "عمود ناقص في الملف",
        impactScore: 95,
        deferred: false
      },
      {
        id: "3",
        category: "outlier",
        titleAr: "قيمة شاذة في المبالغ",
        impactScore: 80,
        deferred: false
      },
      { id: "4", category: "type", titleAr: "نوع عمود غير مؤكد", impactScore: 70, deferred: false },
      { id: "5", category: "type", titleAr: "مؤجل", impactScore: 99, deferred: true }
    ];

    const ranked = aggregateAndRankQuestions(issues);
    expect(ranked.length).toBe(3);
    expect(ranked[0]?.id).toBe("2");
    expect(ranked[1]?.id).toBe("3");
    expect(ranked[2]?.id).toBe("4");
    expect(ranked.some((i) => i.id === "5")).toBe(false);
  });
});
