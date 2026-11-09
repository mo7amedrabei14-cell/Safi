/**
 * `apps/web/src/utils/column-understanding.ts` — Natural language column understanding & rule parser (Section 16.3).
 */

export interface ParsedUnderstandingRule {
  ruleType: "null_mapping" | "code_table" | "format" | "meaning";
  descriptionAr: string;
  condition: object;
  action: object;
  confidence: number;
  confirmed: boolean;
}

export function parseNaturalLanguageRule(
  text: string,
  columnName: string
): ParsedUnderstandingRule {
  const trimmed = text.trim();

  if (
    trimmed.includes("0") &&
    (trimmed.includes("معناها") ||
      trimmed.includes("تعني") ||
      trimmed.includes("فراغ") ||
      trimmed.includes("لم يستلم"))
  ) {
    return {
      ruleType: "null_mapping",
      descriptionAr: `اعتبار القيمة (0) في عمود (${columnName}) بمثابة قيمة فارغة (Null/غير متاح).`,
      condition: { op: "eq", value: 0 },
      action: { set_null: true },
      confidence: 0.95,
      confirmed: false
    };
  }

  if (trimmed.includes("1") && trimmed.includes("2")) {
    return {
      ruleType: "code_table",
      descriptionAr: `جدول رموز مقترح لـ (${columnName}): 1 = ذكر، 2 = أنثى.`,
      condition: { type: "categorical" },
      action: { code_table: { "1": "ذكر", "2": "أنثى" } },
      confidence: 0.9,
      confirmed: false
    };
  }

  return {
    ruleType: "meaning",
    descriptionAr: `تحديد معنى العمود (${columnName}): "${trimmed}".`,
    condition: { column: columnName },
    action: { meaning: trimmed },
    confidence: 0.8,
    confirmed: false
  };
}
