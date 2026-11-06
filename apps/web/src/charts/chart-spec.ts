/**
 * `apps/web/src/charts/chart-spec.ts` — Deterministic Chart-Spec and Validator for Chart Studio (Section 17.1, 17.7).
 */

export type ChartType = "bar" | "line" | "area" | "pie" | "donut" | "scatter" | "histogram" | "kpi";

export interface ChartSpec {
  version: string;
  type: ChartType;
  titleAr: string;
  data: Array<Record<string, string | number>>;
  xAxisKey: string;
  yAxisKey: string;
  width?: number;
  height?: number;
}

export interface ValidationResult {
  valid: boolean;
  errors: string[];
}

export function validateChartSpec(spec: ChartSpec): ValidationResult {
  const errors: string[] = [];
  if (!spec.version) errors.push("SF-CHT-001: إصدار مواصفة المخطط مفقود");
  if (!spec.type) errors.push("SF-CHT-002: نوع المخطط مفقود");
  if (!spec.data || !Array.isArray(spec.data))
    errors.push("SF-CHT-003: بيانات المخطط مفقودة أو غير صالحة");
  if (!spec.xAxisKey) errors.push("SF-CHT-004: مفتاح المحور السيني مفقود");
  if (!spec.yAxisKey) errors.push("SF-CHT-005: مفتاح المحور الصادي مفقود");

  return {
    valid: errors.length === 0,
    errors
  };
}

export function generateDeterministicSvg(spec: ChartSpec): string {
  const validation = validateChartSpec(spec);
  if (!validation.valid) {
    throw new Error(`Invalid ChartSpec: ${validation.errors.join(", ")}`);
  }

  const width = spec.width ?? 600;
  const height = spec.height ?? 400;

  // Deterministic SVG rendering
  let svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">`;
  svg += `<rect width="100%" height="100%" fill="#ffffff"/>`;
  svg += `<text x="20" y="30" font-family="Cairo, sans-serif" font-size="18" font-weight="bold" fill="#111827">${spec.titleAr}</text>`;

  const count = spec.data.length;
  if (count > 0) {
    const barWidth = Math.max(10, (width - 80) / count - 10);
    spec.data.forEach((row, i) => {
      const val = Number(row[spec.yAxisKey]) || 0;
      const barHeight = Math.min(height - 100, val * 2);
      const x = 40 + i * (barWidth + 10);
      const y = height - 50 - barHeight;
      svg += `<rect x="${x}" y="${y}" width="${barWidth}" height="${barHeight}" fill="#0ea5e9" rx="4"/>`;
      svg += `<text x="${x + barWidth / 2}" y="${height - 30}" font-family="Cairo, sans-serif" font-size="12" text-anchor="middle" fill="#4b5563">${row[spec.xAxisKey]}</text>`;
    });
  }

  svg += `</svg>`;
  return svg;
}
