/**
 * `apps/web/src/components/studios/ChartsStudio.tsx`
 *
 * Interactive Chart Studio (Spec Section 17):
 * - Live editable chart title & data values
 * - Deterministic SVG preview + one-click SVG download
 * - Credibility Guard (`SF-CHT-006`) + Smart Chart Recommender
 * - Real WCAG 2.2 AA contrast audit of the rendered SVG colours
 */
import type { ReactNode } from "react";
import { useMemo, useState } from "react";
import { AlertTriangle, BarChart3, CheckCircle2, Download, Eye, Plus, XCircle } from "lucide-react";
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
import { downloadTextFile } from "../../lib/download";
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

export function extractSvgColors(svg: string): string[] {
  const colors = new Set<string>();
  for (const match of svg.matchAll(/(?:fill|stroke)="(#[0-9a-fA-F]{6})"/g)) {
    const value = match[1];
    if (value !== undefined && value.toLowerCase() !== WHITE_CANVAS) {
      colors.add(value.toUpperCase());
    }
  }
  return [...colors];
}

export function ChartsStudio(): ReactNode {
  const [chartType, setChartType] = useState<ChartType>("bar");
  const [titleAr, setTitleAr] = useState<string>(SAMPLE_CHART_SPEC.titleAr);
  const [rows, setRows] = useState<Array<{ region: string; completion: number }>>(
    SAMPLE_CHART_DATA.map((r) => ({ ...r }))
  );

  const effectiveRenderType: ChartType =
    chartType === "area" || chartType === "scatter"
      ? "line"
      : chartType === "histogram" || chartType === "kpi"
        ? "bar"
        : chartType;

  const spec = useMemo<ChartSpec>(
    () => ({
      ...SAMPLE_CHART_SPEC,
      type: effectiveRenderType,
      titleAr,
      data: rows
    }),
    [effectiveRenderType, titleAr, rows]
  );

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
    () => checkChartCredibility({ type: chartType, yAxisKey: spec.yAxisKey, data: spec.data }),
    [chartType, spec]
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
      extractSvgColors(svg).map((colour) => ({
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

  const handleDownloadSvg = (): void => {
    if (svg.length > 0) {
      downloadTextFile(`safi-chart-${chartType}.svg`, svg, "image/svg+xml");
    }
  };

  return (
    <section className="safi-studio" aria-labelledby="safi-charts-title" dir="rtl">
      <header className="safi-studio-header">
        <span className="safi-eyebrow">استوديو الرسوم</span>
        <h1 id="safi-charts-title" className="safi-studio-title">
          الرسوم البيانية
        </h1>
        <p className="safi-studio-lead">
          مخططات حتمية من نفس المواصفة، مع تحرير حي للبيانات، وحارس المصداقية، وفحص تباين WCAG 2.2
          AA لكل لون يرسمه المخطط فعلًا.
        </p>
      </header>

      <div className="safi-grid-2">
        <SpotlightCard padding={24}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", gap: "8px", marginBottom: "8px", flexWrap: "wrap" }}>
            <label className="safi-field-label" htmlFor="safi-chart-title-input" style={{ margin: 0 }}>
              عنوان المخطط
            </label>
            <button
              type="button"
              className="safi-btn safi-btn-primary"
              onClick={handleDownloadSvg}
              style={{ minHeight: "36px", padding: "4px 12px", fontSize: "12px" }}
            >
              <Download size={13} aria-hidden="true" />
              تنزيل SVG
            </button>
          </div>
          <input
            id="safi-chart-title-input"
            className="safi-input"
            value={titleAr}
            onChange={(e) => setTitleAr(e.target.value)}
          />

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

          <img
            src={imageUri}
            alt={titleAr}
            className="safi-chart-canvas"
            data-testid="safi-chart-image"
          />

          <p className="safi-hint">
            <BarChart3 size={13} aria-hidden="true" /> المواصفة صالحة:{" "}
            {validation.valid ? "نعم" : validation.errors.join(" · ")}
          </p>
        </SpotlightCard>

        <SpotlightCard padding={24}>
          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "8px" }}>
            <h2 className="safi-card-title" style={{ margin: 0 }}>
              تحرير بيانات المخطط مباشرةً
            </h2>
            <button
              type="button"
              onClick={() => setRows([...rows, { region: `محافظة ${rows.length + 1}`, completion: 80 }])}
              style={{ minHeight: "32px", padding: "4px 10px", fontSize: "11px" }}
            >
              <Plus size={12} aria-hidden="true" /> إضافة صف
            </button>
          </div>

          <div style={{ display: "flex", flexDirection: "column", gap: "6px", marginBottom: "18px" }}>
            {rows.map((r, idx) => (
              <div key={idx} style={{ display: "grid", gridTemplateColumns: "1fr 110px", gap: "8px", alignItems: "center" }}>
                <input
                  value={r.region}
                  onChange={(e) => {
                    const next = [...rows];
                    next[idx] = { ...r, region: e.target.value };
                    setRows(next);
                  }}
                  style={{ minHeight: "34px", padding: "4px 8px", fontSize: "12px" }}
                  aria-label={`اسم المحافظة ${idx + 1}`}
                />
                <input
                  type="number"
                  min={0}
                  max={100}
                  value={r.completion}
                  onChange={(e) => {
                    const next = [...rows];
                    next[idx] = { ...r, completion: Number(e.target.value) };
                    setRows(next);
                  }}
                  style={{ minHeight: "34px", padding: "4px 8px", fontSize: "12px" }}
                  aria-label={`نسبة الاكتمال ${idx + 1}`}
                />
              </div>
            ))}
          </div>

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

      <SpotlightCard padding={24}>
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
                    <span
                      style={{
                        display: "inline-block",
                        width: "10px",
                        height: "10px",
                        borderRadius: "2px",
                        backgroundColor: row.colour,
                        marginInlineEnd: "6px"
                      }}
                    />
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
