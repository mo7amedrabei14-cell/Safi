/**
 * `apps/web/src/components/studios/ChartsStudio.tsx`
 *
 * Chart Studio: deterministic SVG preview, credibility guard and a real WCAG 2.2 AA
 * contrast audit of the colours actually used by the rendered chart.
 *
 * The contrast table is computed from the generated SVG markup itself, so a colour
 * change in the renderer can never silently escape the audit.
 */
import type { ReactNode } from "react";
import { useMemo, useState } from "react";
import { AlertTriangle, BarChart3, CheckCircle2, Eye, XCircle } from "lucide-react";
import {
  DARK_COLORS,
  WCAG_AA_TEXT_MIN_RATIO,
  WCAG_AA_UI_GRAPHIC_MIN_RATIO,
  contrastRatio
} from "@safi/design-tokens";
import { generateDeterministicSvg, validateChartSpec } from "../../charts/chart-spec";
import type { ChartSpec, ChartType } from "../../charts/chart-spec";
import { checkChartCredibility } from "../../charts/credibility-guard";
import { recommendCharts } from "../../charts/smart-chart-recommender";
import { SpotlightCard } from "../ui/SpotlightCard";

export const SAMPLE_CHART_DATA: ReadonlyArray<{
  readonly region: string;
  readonly completion: number;
}> = [
  { region: "القاهرة", completion: 96 },
  { region: "الجيزة", completion: 74 },
  { region: "الإسكندرية", completion: 58 },
  { region: "أسيوط", completion: 43 },
  { region: "أسوان", completion: 68 }
];

export const SAMPLE_CHART_SPEC: ChartSpec = {
  version: "1.0",
  type: "bar",
  titleAr: "نسبة اكتمال البيانات حسب المحافظة (بيانات تجريبية)",
  data: SAMPLE_CHART_DATA.map((row) => ({ region: row.region, completion: row.completion })),
  xAxisKey: "region",
  yAxisKey: "completion",
  width: 640,
  height: 320
};

const CHART_TYPES: readonly ChartType[] = [
  "bar",
  "line",
  "area",
  "pie",
  "donut",
  "scatter",
  "histogram",
  "kpi"
];

const CHART_TYPE_LABELS: Readonly<Record<ChartType, string>> = {
  bar: "أعمدة",
  line: "خطي",
  area: "مساحي",
  pie: "دائري",
  donut: "حلقي",
  scatter: "انتشار",
  histogram: "تكرار",
  kpi: "بطاقة مؤشر"
};

const WHITE_CANVAS = "#ffffff";

/** Unique fill colours used by the generated SVG, excluding the canvas itself. */
export function extractSvgFills(svg: string): string[] {
  const fills = new Set<string>();
  for (const match of svg.matchAll(/fill="(#[0-9a-fA-F]{6})"/g)) {
    const value = match[1];
    if (value !== undefined && value.toLowerCase() !== WHITE_CANVAS) {
      fills.add(value.toUpperCase());
    }
  }
  return [...fills];
}

export function ChartsStudio(): ReactNode {
  const [chartType, setChartType] = useState<ChartType>("bar");

  const spec = useMemo<ChartSpec>(() => ({ ...SAMPLE_CHART_SPEC, type: chartType }), [chartType]);
  const validation = useMemo(() => validateChartSpec(spec), [spec]);
  const svg = useMemo(
    () => (validation.valid ? generateDeterministicSvg(spec) : ""),
    [spec, validation.valid]
  );
  const imageUri = useMemo(
    () => (svg.length === 0 ? "" : `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`),
    [svg]
  );

  const credibility = useMemo(
    () => checkChartCredibility({ type: spec.type, yAxisKey: spec.yAxisKey, data: spec.data }),
    [spec]
  );
  const recommendations = useMemo(
    () =>
      recommendCharts([
        { name: "المحافظة", type: "string" },
        { name: "نسبة الاكتمال", type: "number" }
      ]),
    []
  );

  const svgAudit = useMemo(
    () =>
      extractSvgFills(svg).map((colour) => ({
        colour,
        background: WHITE_CANVAS,
        ratio: contrastRatio(colour, WHITE_CANVAS),
        minimum: WCAG_AA_UI_GRAPHIC_MIN_RATIO
      })),
    [svg]
  );

  const paletteAudit = useMemo(
    () =>
      [
        DARK_COLORS.primary,
        DARK_COLORS.success,
        DARK_COLORS.warning,
        DARK_COLORS.danger,
        DARK_COLORS.info
      ].map((colour) => ({
        colour,
        background: DARK_COLORS.surface,
        ratio: contrastRatio(colour, DARK_COLORS.surface),
        minimum: WCAG_AA_TEXT_MIN_RATIO
      })),
    []
  );

  const allPass = [...svgAudit, ...paletteAudit].every((row) => row.ratio >= row.minimum);

  return (
    <section className="safi-studio" aria-labelledby="safi-charts-title" dir="rtl">
      <header className="safi-studio-header">
        <span className="safi-eyebrow">استوديو الرسوم</span>
        <h1 id="safi-charts-title" className="safi-studio-title">
          الرسوم البيانية
        </h1>
        <p className="safi-studio-lead">
          مخططات حتمية من نفس المواصفة، مع حارس المصداقية وفحص تباين WCAG 2.2 AA لكل لون يرسمه
          المخطط فعلًا.
        </p>
      </header>

      <div className="safi-grid-2">
        <SpotlightCard padding={22}>
          <div className="safi-chip-row" role="group" aria-label="نوع المخطط">
            {CHART_TYPES.map((type) => (
              <button
                key={type}
                type="button"
                className={type === chartType ? "safi-chip safi-chip--active" : "safi-chip"}
                aria-pressed={type === chartType}
                onClick={() => setChartType(type)}
              >
                {CHART_TYPE_LABELS[type]}
              </button>
            ))}
          </div>

          {chartType === "bar" ? (
            <img
              src={imageUri}
              alt={SAMPLE_CHART_SPEC.titleAr}
              className="safi-chart-canvas"
              data-testid="safi-chart-image"
            />
          ) : (
            <p className="safi-hint" role="status">
              المعاينة الحتمية SVG لهذه النسخة ترسم مخطط الأعمدة. أما {CHART_TYPE_LABELS[chartType]}{" "}
              فتُصدَّر عبر مواصفة Vega-Lite في نواة التقارير بنفس البيانات والمواصفة.
            </p>
          )}

          <p className="safi-hint">
            <BarChart3 size={13} aria-hidden="true" /> المواصفة صالحة:{" "}
            {validation.valid ? "نعم" : validation.errors.join(" · ")}
          </p>
        </SpotlightCard>

        <SpotlightCard padding={22}>
          <h2 className="safi-card-title">
            <AlertTriangle size={16} aria-hidden="true" /> حارس المصداقية
          </h2>
          {credibility.length === 0 ? (
            <p className="safi-status safi-status--ok">
              <CheckCircle2 size={14} aria-hidden="true" /> لا توجد ملاحظات على هذا المخطط.
            </p>
          ) : (
            <ul className="safi-list">
              {credibility.map((warning) => (
                <li key={warning.code} data-credibility-code={warning.code}>
                  <strong className="safi-mono">{warning.code}</strong> — {warning.messageAr}
                  <span className="safi-hint">{warning.suggestionAr}</span>
                </li>
              ))}
            </ul>
          )}

          <h2 className="safi-card-title" style={{ marginTop: "18px" }}>
            <Eye size={16} aria-hidden="true" /> التوصيات الذكية
          </h2>
          <ul className="safi-list">
            {recommendations.map((recommendation) => (
              <li key={recommendation.type}>
                <strong>{recommendation.titleAr}</strong>
                <span className="safi-hint">{recommendation.reasonAr}</span>
              </li>
            ))}
          </ul>
        </SpotlightCard>
      </div>

      <SpotlightCard padding={22}>
        <h2 className="safi-card-title">
          {allPass ? (
            <CheckCircle2 size={16} aria-hidden="true" />
          ) : (
            <XCircle size={16} aria-hidden="true" />
          )}
          التحقق من تباين WCAG 2.2 AA
        </h2>

        <div className="safi-table-wrap">
          <table className="safi-table">
            <thead>
              <tr>
                <th scope="col">اللون</th>
                <th scope="col">الخلفية</th>
                <th scope="col">النسبة</th>
                <th scope="col">الحد المطلوب</th>
                <th scope="col">الحالة</th>
              </tr>
            </thead>
            <tbody>
              {[...svgAudit, ...paletteAudit].map((row) => (
                <tr key={`${row.colour}-${row.background}`}>
                  <td className="safi-mono" dir="ltr">
                    {row.colour}
                  </td>
                  <td className="safi-mono" dir="ltr">
                    {row.background}
                  </td>
                  <td className="safi-mono" dir="ltr">
                    {row.ratio.toFixed(2)}:1
                  </td>
                  <td className="safi-mono" dir="ltr">
                    {row.minimum.toFixed(1)}:1
                  </td>
                  <td>
                    {row.ratio >= row.minimum ? (
                      <span className="safi-status safi-status--ok">✓ يجتاز</span>
                    ) : (
                      <span className="safi-status safi-status--bad">✗ يرسب</span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </SpotlightCard>
    </section>
  );
}
