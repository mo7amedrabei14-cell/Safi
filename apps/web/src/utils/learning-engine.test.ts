import { describe, it, expect } from "vitest";
import { inferRuleFromExamples } from "./learning-engine";

describe("Learning by Example Engine (Section 16.4)", () => {
  it("infers prefix addition rule from manual cell edits", () => {
    const examples = [
      { rowIndex: 0, originalValue: "501234567", editedValue: "+966501234567" },
      { rowIndex: 1, originalValue: "509876543", editedValue: "+966509876543" }
    ];
    const rows = ["501234567", "509876543", "501112233"];

    const rule = inferRuleFromExamples(examples, rows);
    expect(rule).not.toBeNull();
    expect(rule?.patternType).toBe("prefix_add");
    expect(rule?.descriptionAr).toContain("+966");
    expect(rule?.previewBeforeAfter[2]?.after).toBe("+966501112233");
  });
});
