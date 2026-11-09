/**
 * `apps/web/src/charts/smart-chart-recommender.ts` — Smart Chart Recommender (Section 17.2).
 * Suggests up to 3 optimal charts with a clear "Why?" explanation.
 */

export interface ChartRecommendation {
  type: "bar" | "line" | "pie" | "scatter" | "kpi";
  titleAr: string;
  reasonAr: string;
  titleEn: string;
  reasonEn: string;
}

export function recommendCharts(
  columns: Array<{ name: string; type: "string" | "number" | "date" }>
): ChartRecommendation[] {
  const numericCols = columns.filter((c) => c.type === "number");
  const stringCols = columns.filter((c) => c.type === "string");
  const dateCols = columns.filter((c) => c.type === "date");

  const recommendations: ChartRecommendation[] = [];

  if (numericCols.length > 0 && stringCols.length > 0) {
    recommendations.push({
      type: "bar",
      titleAr: "مخطط الأعمدة المقارن",
      reasonAr:
        "مناسب جدًا لمقارنة القيم العددية عبر الفئات النصية المختلفة مثل الأقسام أو الأسماء.",
      titleEn: "Comparative bar chart",
      reasonEn: "Compare numeric values across categories such as departments, regions, or names."
    });
  }

  if (numericCols.length > 0 && dateCols.length > 0) {
    recommendations.push({
      type: "line",
      titleAr: "مخطط الخط الزمني",
      reasonAr: "أفضل طريقة لعرض تطور القيم العددية بمرور الوقت وتحديد الاتجاهات.",
      titleEn: "Time-series line chart",
      reasonEn: "Show how numeric values change over time and identify trends."
    });
  }

  if (numericCols.length > 0) {
    recommendations.push({
      type: "kpi",
      titleAr: "بطاقة المؤشر الرئيسي (KPI)",
      reasonAr: "تعرض الإجمالي أو المتوسط العام بشكل بارز ومباشر لمتخذ القرار.",
      titleEn: "Key performance indicator (KPI)",
      reasonEn: "Highlight the total or overall average for a quick decision."
    });
  }

  return recommendations.slice(0, 3);
}
