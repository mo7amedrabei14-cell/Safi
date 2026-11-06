import { describe, it, expect } from "vitest";
import { parseNaturalLanguageRule } from "./column-understanding";

describe("Column Understanding & Natural Language Rule Parser (Section 16.3)", () => {
  it('parses "القيمة 0 معناها لم يستلم" into a confirmed pending null_mapping rule', () => {
    const rule = parseNaturalLanguageRule("القيمة 0 معناها لم يستلم", "الحالة");
    expect(rule.ruleType).toBe("null_mapping");
    expect(rule.confidence).toBeGreaterThan(0.9);
    expect(rule.confirmed).toBe(false);
    expect(rule.descriptionAr).toContain("0");
  });

  it("parses code table rule from free text", () => {
    const rule = parseNaturalLanguageRule("1 ذكر و2 أنثى", "الجنس");
    expect(rule.ruleType).toBe("code_table");
    expect(rule.confidence).toBeGreaterThan(0.85);
  });
});
