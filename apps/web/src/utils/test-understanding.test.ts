import { describe, it, expect } from "vitest";
import { generateUnderstandingSentences } from "./test-understanding";

describe("Test My Understanding Engine (Section 16.4)", () => {
  it("generates natural descriptive Arabic sentences matching table rows and headers", () => {
    const headers = ["الاسم", "المبلغ"];
    const rows = [
      ["أحمد", "5000"],
      ["سارة", "7000"]
    ];

    const sentences = generateUnderstandingSentences(headers, rows, 10);
    expect(sentences.length).toBe(2);
    expect(sentences[0]?.sentenceAr).toContain("الاسم: (أحمد)");
    expect(sentences[0]?.sentenceAr).toContain("المبلغ: (5000)");
  });
});
