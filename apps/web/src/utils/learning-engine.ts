/**
 * `apps/web/src/utils/learning-engine.ts` — Learning by Example inference engine (Section 16.4).
 */

export interface CellEditExample {
  rowIndex: number;
  originalValue: string;
  editedValue: string;
}

export interface InferredRule {
  patternType: "prefix_add" | "suffix_add" | "replace" | "transform";
  descriptionAr: string;
  previewBeforeAfter: { before: string; after: string }[];
  operation: {
    op: string;
    params: object;
  };
}

export function inferRuleFromExamples(
  examples: CellEditExample[],
  allRows: string[]
): InferredRule | null {
  if (examples.length < 2) return null;

  const ex1 = examples[0];
  const ex2 = examples[1];
  if (!ex1 || !ex2) return null;

  if (ex1.editedValue.endsWith(ex1.originalValue) && ex2.editedValue.endsWith(ex2.originalValue)) {
    const prefix = ex1.editedValue.replace(ex1.originalValue, "");
    if (ex2.editedValue.replace(ex2.originalValue, "") === prefix) {
      const preview = allRows.slice(0, 3).map((val) => ({
        before: val,
        after: val.startsWith(prefix) ? val : prefix + val
      }));
      return {
        patternType: "prefix_add",
        descriptionAr: `إضافة البادئة "${prefix}" لجميع قيم العمود.`,
        previewBeforeAfter: preview,
        operation: {
          op: "normalize_text",
          params: { add_prefix: prefix }
        }
      };
    }
  }

  if (
    ex1.editedValue.startsWith(ex1.originalValue) &&
    ex2.editedValue.startsWith(ex2.originalValue)
  ) {
    const suffix = ex1.editedValue.replace(ex1.originalValue, "");
    if (ex2.editedValue.replace(ex2.originalValue, "") === suffix) {
      const preview = allRows.slice(0, 3).map((val) => ({
        before: val,
        after: val.endsWith(suffix) ? val : val + suffix
      }));
      return {
        patternType: "suffix_add",
        descriptionAr: `إضافة اللاحقة "${suffix}" لجميع قيم العمود.`,
        previewBeforeAfter: preview,
        operation: {
          op: "normalize_text",
          params: { add_suffix: suffix }
        }
      };
    }
  }

  const preview = allRows.slice(0, 3).map((val) => ({
    before: val,
    after: ex1.editedValue
  }));
  return {
    patternType: "transform",
    descriptionAr: `تطبيق تحويل استبدالي قياسي بناءً على الأمثلة المُدخلة.`,
    previewBeforeAfter: preview,
    operation: {
      op: "normalize_text",
      params: { inferred: true }
    }
  };
}
