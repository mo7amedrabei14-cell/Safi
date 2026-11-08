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

const CHART_COLORS = ["#0B7C8C", "#1D4ED8", "#047857", "#B45309", "#BE185D", "#6D28D9"];

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
    if (spec.type === "line") {
      const values = spec.data.map((row) => Number(row[spec.yAxisKey]) || 0);
      const minValue = Math.min(...values, 0);
      const maxValue = Math.max(...values, 0);
      const valueRange = maxValue - minValue || 1;
      const points = spec.data.map((row, index) => {
        const value = Number(row[spec.yAxisKey]) || 0;
        const x = count === 1 ? width / 2 : 40 + (index * (width - 80)) / (count - 1);
        const y = height - 50 - ((value - minValue) / valueRange) * (height - 110);
        return { x, y, label: row[spec.xAxisKey] };
      });
      const pointList = points.map(({ x, y }) => `${x},${y}`).join(" ");
      svg += `<polyline points="${pointList}" fill="none" stroke="${CHART_COLORS[0]}" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>`;
      points.forEach(({ x, y, label }) => {
        svg += `<circle cx="${x}" cy="${y}" r="4" fill="${CHART_COLORS[0]}"/>`;
        svg += `<text x="${x}" y="${height - 30}" font-family="Cairo, sans-serif" font-size="12" text-anchor="middle" fill="#4b5563">${label}</text>`;
      });
    } else if (spec.type === "pie" || spec.type === "donut") {
      const values = spec.data.map((row) => Math.max(0, Number(row[spec.yAxisKey]) || 0));
      const total = values.reduce((sum, value) => sum + value, 0);
      const centerX = width / 2;
      const centerY = (height + 40) / 2;
      const outerRadius = Math.max(10, Math.min(width, height - 100) / 2 - 20);
      const circleRadius = spec.type === "pie" ? outerRadius / 2 : outerRadius - 24;
      const strokeWidth = spec.type === "pie" ? outerRadius : 48;
      const circumference = 2 * Math.PI * circleRadius;
      let offset = 0;
      values.forEach((value, index) => {
        if (total === 0 || value === 0) return;
        const segmentLength = (value / total) * circumference;
        svg += `<circle cx="${centerX}" cy="${centerY}" r="${circleRadius}" fill="none" stroke="${CHART_COLORS[index % CHART_COLORS.length]}" stroke-width="${strokeWidth}" stroke-dasharray="${segmentLength} ${circumference - segmentLength}" stroke-dashoffset="${-offset}" transform="rotate(-90 ${centerX} ${centerY})"/>`;
        offset += segmentLength;
      });
    } else {
      const barWidth = Math.max(10, (width - 80) / count - 10);
      spec.data.forEach((row, i) => {
        const val = Number(row[spec.yAxisKey]) || 0;
        const barHeight = Math.min(height - 100, val * 2);
        const x = 40 + i * (barWidth + 10);
        const y = height - 50 - barHeight;
        // WCAG 2.2 AA non-text contrast: 4.91:1 on the white chart canvas
        // (the previous #0ea5e9 measured 2.77:1 and failed the 3:1 graphic minimum).
        svg += `<rect x="${x}" y="${y}" width="${barWidth}" height="${barHeight}" fill="${CHART_COLORS[0]}" rx="4"/>`;
        svg += `<text x="${x + barWidth / 2}" y="${height - 30}" font-family="Cairo, sans-serif" font-size="12" text-anchor="middle" fill="#4b5563">${row[spec.xAxisKey]}</text>`;
      });
    }
  }

  svg += `</svg>`;
  return svg;
}
