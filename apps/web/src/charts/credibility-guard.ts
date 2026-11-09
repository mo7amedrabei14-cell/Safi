/**
 * `apps/web/src/charts/credibility-guard.ts` — Credibility Guard for Charts (Section 17.5).
 * Warns and suggests improvements without blocking the user.
 */

export interface CredibilityWarning {
  code: string;
  messageAr: string;
  suggestionAr: string;
  messageEn: string;
  suggestionEn: string;
}

export function checkChartCredibility(spec: {
  type: string;
  yAxisKey: string;
  data: Array<Record<string, unknown>>;
}): CredibilityWarning[] {
  const warnings: CredibilityWarning[] = [];

  // Example check: Bar chart starting Y axis above zero
  if (spec.type === "bar" && spec.data.length > 0) {
    const values = spec.data.map((d) => Number(d[spec.yAxisKey]) || 0);
    const minVal = Math.min(...values);
    if (minVal > 0) {
      warnings.push({
        code: "SF-CHT-006",
        messageAr: "مخطط الأعمدة يبدأ بمحور صادي أعلى من الصفر، مما قد يوحي بفروق مبالغ فيها.",
        suggestionAr: "نفضل بدء محور صادي من الصفر لضمان التمثيل البصري العادل.",
        messageEn: "The bar chart starts its value axis above zero, which may exaggerate differences.",
        suggestionEn: "Start the value axis at zero to preserve a fair visual comparison."
      });
    }
  }

  return warnings;
}
