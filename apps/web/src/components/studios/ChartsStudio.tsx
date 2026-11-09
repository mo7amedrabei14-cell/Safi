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
import {
  AlertTriangle,
  BarChart3,
  CheckCircle2,
  Download,
  Eye,
  FileCode2,
  Home,
  Library,
  ListChecks,
  Plus,
  ShieldCheck,
  Sparkles,
  XCircle
} from "lucide-react";
import type { SafiDockTabId, SafiLang } from "../layout/FloatingGlassDock";
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
import { CountUpMetric } from "../ui/CountUpMetric";
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
  titleEn: "Data completeness by governorate (sample data)",
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

const CHART_TYPE_LABELS: Readonly<Record<ChartType, { ar: string; en: string }>> = {
  bar: { ar: "أعمدة", en: "Bar" },
  line: { ar: "خطي", en: "Line" },
  area: { ar: "مساحي", en: "Area" },
  pie: { ar: "دائري", en: "Pie" },
  donut: { ar: "حلقي", en: "Donut" },
  scatter: { ar: "انتشار", en: "Scatter" },
  histogram: { ar: "تكرار", en: "Histogram" },
  kpi: { ar: "بطاقة مؤشر", en: "KPI" }
};

const WHITE_CANVAS = "#ffffff";

const CHART_VALIDATION_EN: Readonly<Record<string, string>> = {
  "SF-CHT-001": "Chart specification version is missing.",
  "SF-CHT-002": "Chart type is missing.",
  "SF-CHT-003": "Chart data is missing or invalid.",
  "SF-CHT-004": "The x-axis key is missing.",
  "SF-CHT-005": "The y-axis key is missing."
};

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

const CHART_PRESETS: ReadonlyArray<{
  readonly id: string;
  readonly labelAr: string;
  readonly labelEn: string;
  readonly titleAr: string;
  readonly titleEn: string;
  readonly rows: ReadonlyArray<{ readonly region: string; readonly completion: number }>;
}> = [
  {
    id: "governorates",
    labelAr: "اكتمال بيانات المحافظات",
    labelEn: "Regional Completion",
    titleAr: "نسبة اكتمال البيانات حسب المحافظة (بيانات تجريبية)",
    titleEn: "Data completeness by governorate (sample data)",
    rows: SAMPLE_CHART_DATA
  },
  {
    id: "revenue",
    labelAr: "نمو الإيرادات الفصلية",
    labelEn: "Quarterly Revenue Growth",
    titleAr: "معدل نمو الإيرادات التشغيلية عبر الفروع (%)",
    titleEn: "Quarterly operational revenue growth by branch (%)",
    rows: [
      { region: "الربع الأول", completion: 72 },
      { region: "الربع الثاني", completion: 84 },
      { region: "الربع الثالث", completion: 91 },
      { region: "الربع الرابع", completion: 98 }
    ]
  },
  {
    id: "sectors",
    labelAr: "جودة القطاعات المؤسسية",
    labelEn: "Enterprise Sector Quality",
    titleAr: "درجة نقاء البيانات حسب القطاع المؤسسي (%)",
    titleEn: "Data quality score by business sector (%)",
    rows: [
      { region: "القطاع المالي", completion: 99 },
      { region: "الرعاية الصحية", completion: 94 },
      { region: "التجارة الإلكترونية", completion: 89 },
      { region: "الخدمات اللوجستية", completion: 86 },
      { region: "التعليم والبحث", completion: 92 }
    ]
  }
];

export interface ChartsStudioProps {
  readonly lang?: SafiLang;
  readonly onSelectTab?: (tab: SafiDockTabId) => void;
}

export function ChartsStudio({ lang = "en", onSelectTab }: ChartsStudioProps = {}): ReactNode {
  const isAr = lang === "ar";
  const [activePresetId, setActivePresetId] = useState<string>("governorates");
  const [chartType, setChartType] = useState<ChartType>("bar");
  const [customTitles, setCustomTitles] = useState<{ ar: string; en: string } | null>(null);
  const [rows, setRows] = useState<Array<{ region: string; completion: number }>>(
    SAMPLE_CHART_DATA.map((r) => ({ ...r }))
  );

  const activePreset =
    CHART_PRESETS.find((preset) => preset.id === activePresetId) ??
    CHART_PRESETS.find((preset) => preset.id === "governorates");
  if (activePreset === undefined) {
    throw new Error("At least one chart preset must be defined.");
  }
  const chartTitle =
    customTitles?.[isAr ? "ar" : "en"] ?? (isAr ? activePreset.titleAr : activePreset.titleEn);

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
      titleAr: customTitles?.ar ?? activePreset.titleAr,
      titleEn: customTitles?.en ?? activePreset.titleEn,
      data: rows
    }),
    [effectiveRenderType, customTitles, activePreset, rows]
  );

  const validation = useMemo(() => validateChartSpec(spec), [spec]);
  const svg = useMemo(
    () => (validation.valid ? generateDeterministicSvg(spec, lang) : ""),
    [spec, validation.valid, lang]
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
      extractSvgColors(svg).map((colour, idx) => ({
        colour,
        nameAr: `لون المخطط #${idx + 1}`,
        nameEn: `Chart colour #${idx + 1}`,
        bgNameAr: "لوحة الرسم القياسية",
        bgNameEn: "Standard chart canvas",
        background: WHITE_CANVAS,
        ratio: contrastRatio(colour, WHITE_CANVAS),
        minimum: WCAG_AA_UI_GRAPHIC_MIN_RATIO
      })),
    [svg]
  );

  const paletteAudit = useMemo(
    () =>
      [
        { c: DARK_COLORS.primary, n: "الأزرق الملكي الأساسي", en: "Primary royal blue" },
        { c: DARK_COLORS.success, n: "الأخضر الزمردي للنجاح", en: "Success emerald green" },
        { c: DARK_COLORS.warning, n: "الكهرماني التنبيهي", en: "Warning amber" },
        { c: DARK_COLORS.danger, n: "الأحمر المرجاني للخطر", en: "Danger coral red" },
        { c: DARK_COLORS.info, n: "السماوي الفيروزي للمعلومات", en: "Information turquoise" }
      ].map((item) => ({
        colour: item.c,
        nameAr: item.n,
        nameEn: item.en,
        bgNameAr: "السطح الليلي الداكن",
        bgNameEn: "Dark surface",
        background: DARK_COLORS.surface,
        ratio: contrastRatio(item.c, DARK_COLORS.surface),
        minimum: WCAG_AA_TEXT_MIN_RATIO
      })),
    []
  );

  const allPass = [...svgAudit, ...paletteAudit].every((row) => row.ratio >= row.minimum);

  const avgCompletion = useMemo(() => {
    if (rows.length === 0) return 0;
    const sum = rows.reduce((acc, r) => acc + r.completion, 0);
    return Math.round(sum / rows.length);
  }, [rows]);

  const maxCompletion = useMemo(() => {
    if (rows.length === 0) return 0;
    return Math.max(...rows.map((r) => r.completion));
  }, [rows]);

  const handleDownloadSvg = (): void => {
    if (svg.length > 0) {
      downloadTextFile(`safi-chart-${chartType}.svg`, svg, "image/svg+xml");
    }
  };

  const handleDownloadSpecJson = (): void => {
    downloadTextFile(
      `safi-chart-spec-${chartType}.json`,
      JSON.stringify(spec, null, 2),
      "application/json"
    );
  };

  return (
    <section className="safi-studio" aria-labelledby="safi-charts-title" dir={isAr ? "rtl" : "ltr"}>
      {/* Apple Dynamic Island Contrast & App Switcher Bar */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: "12px",
          flexWrap: "wrap"
        }}
      >
        <div className="safi-dynamic-island" style={{ cursor: "default" }}>
          <span className="safi-island-dot" />
          <span style={{ fontSize: "12.5px", fontWeight: 800, color: "#F8FAFC" }}>
            {isAr
              ? "محرك الرسوم المتجهة • Vega-Lite RTL + WCAG 2.2 AA"
              : "Vector Chart Engine • Vega-Lite RTL + WCAG 2.2 AA"}
          </span>
          <span
            className="safi-mono"
            dir="ltr"
            style={{ fontSize: "11.5px", color: "#34D399", fontWeight: 800 }}
          >
            {isAr ? "المتوسط" : "Avg"} <CountUpMetric value={avgCompletion} suffix="%" /> •{" "}
            {isAr ? "الذروة" : "Peak"} {maxCompletion}%
          </span>
        </div>

        {onSelectTab ? (
          <div style={{ display: "flex", gap: "6px", flexWrap: "wrap" }}>
            <button
              type="button"
              onClick={() => onSelectTab("home")}
              className="safi-btn safi-btn-secondary"
              style={{ minHeight: "36px", padding: "4px 12px", fontSize: "12px" }}
            >
              <Home size={13} /> {isAr ? "الرئيسية" : "Website"}
            </button>
            <button
              type="button"
              onClick={() => onSelectTab("cleaning")}
              className="safi-btn safi-btn-secondary"
              style={{ minHeight: "36px", padding: "4px 12px", fontSize: "12px" }}
            >
              <Sparkles size={13} /> {isAr ? "التنظيف وPII" : "Cleaning"}
            </button>
            <button
              type="button"
              onClick={() => onSelectTab("recipes")}
              className="safi-btn safi-btn-secondary"
              style={{ minHeight: "36px", padding: "4px 12px", fontSize: "12px" }}
            >
              <ListChecks size={13} /> {isAr ? "الوصفات" : "Recipes"}
            </button>
            <button
              type="button"
              onClick={() => onSelectTab("library")}
              className="safi-btn safi-btn-secondary"
              style={{ minHeight: "36px", padding: "4px 12px", fontSize: "12px" }}
            >
              <Library size={13} /> {isAr ? "المكتبة" : "Library"}
            </button>
            <button
              type="button"
              onClick={() => onSelectTab("owner")}
              className="safi-btn safi-btn-secondary"
              style={{ minHeight: "36px", padding: "4px 12px", fontSize: "12px" }}
            >
              <ShieldCheck size={13} /> {isAr ? "لوحة المالك" : "Owner"}
            </button>
          </div>
        ) : null}
      </div>

      {/* Open Unboxed Header Sitting Directly on the Interactive Background */}
      <header
        className="safi-studio-header"
        style={{
          background: "transparent",
          border: "none",
          boxShadow: "none",
          padding: "8px 4px"
        }}
      >
        <span className="safi-eyebrow">{isAr ? "استوديو الرسوم" : "Chart Studio"}</span>
        <h1 id="safi-charts-title" className="safi-studio-title safi-rb-shiny-text">
          {isAr ? "الرسوم البيانية" : "Charts"}
        </h1>
        <p className="safi-studio-lead">
          {isAr
            ? "مخططات حتمية من نفس المواصفة، مع تحرير حي للبيانات، وحارس المصداقية، وفحص تباين WCAG 2.2 AA لكل لون يرسمه المخطط فعلًا."
            : "Deterministic vector charts from the exact same spec, with live data editing, Credibility Guard, and real-time WCAG 2.2 AA contrast verification."}
        </p>
      </header>

      {/* Preset Dataset Switcher + Live KPI Summary Bar */}
      <div
        style={{
          padding: "14px 18px",
          borderRadius: "20px",
          background: "linear-gradient(135deg, #090D16 0%, #111827 100%)",
          color: "#F8FAFC",
          border: "1px solid rgba(96, 165, 250, 0.35)",
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: "12px",
          flexWrap: "wrap"
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: "8px", flexWrap: "wrap" }}>
          <span style={{ fontSize: "12.5px", fontWeight: 800, color: "#93C5FD" }}>
            {isAr ? "مجموعات البيانات الجاهزة:" : "Chart Datasets:"}
          </span>
          {CHART_PRESETS.map((preset) => {
            const active = preset.id === activePresetId;
            return (
              <button
                key={preset.id}
                type="button"
                onClick={() => {
                  setActivePresetId(preset.id);
                  setCustomTitles(null);
                  setRows(preset.rows.map((r) => ({ ...r })));
                }}
                style={{
                  minHeight: "34px",
                  padding: "4px 14px",
                  borderRadius: "999px",
                  fontSize: "12px",
                  fontWeight: 700,
                  cursor: "pointer",
                  border: active ? "1px solid #60A5FA" : "1px solid rgba(255,255,255,0.16)",
                  background: active ? "#2563EB" : "rgba(255,255,255,0.06)",
                  color: "#F8FAFC"
                }}
              >
                {isAr ? preset.labelAr : preset.labelEn}
              </button>
            );
          })}
        </div>

        <button
          type="button"
          onClick={handleDownloadSpecJson}
          style={{
            minHeight: "34px",
            padding: "4px 14px",
            borderRadius: "999px",
            fontSize: "12px",
            fontWeight: 700,
            cursor: "pointer",
            border: "1px solid rgba(52, 211, 153, 0.45)",
            background: "rgba(16, 185, 129, 0.14)",
            color: "#34D399",
            display: "inline-flex",
            alignItems: "center",
            gap: "6px"
          }}
        >
          <FileCode2 size={13} />
          <span>{isAr ? "تصدير مواصفة JSON" : "Export JSON Spec"}</span>
        </button>
      </div>

      <div className="safi-grid-2">
        <SpotlightCard padding={24}>
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              gap: "8px",
              marginBottom: "8px",
              flexWrap: "wrap"
            }}
          >
            <label
              className="safi-field-label"
              htmlFor="safi-chart-title-input"
              style={{ margin: 0 }}
            >
              {isAr ? "عنوان المخطط" : "Chart title"}
            </label>
            <button
              type="button"
              className="safi-btn safi-btn-primary"
              onClick={handleDownloadSvg}
              style={{ minHeight: "36px", padding: "4px 12px", fontSize: "12px" }}
            >
              <Download size={13} aria-hidden="true" />
              {isAr ? "تنزيل SVG" : "Download SVG"}
            </button>
          </div>
          <input
            id="safi-chart-title-input"
            className="safi-input"
            value={chartTitle}
            onChange={(e) =>
              setCustomTitles((current) => ({
                ar: current?.ar ?? activePreset.titleAr,
                en: current?.en ?? activePreset.titleEn,
                [isAr ? "ar" : "en"]: e.target.value
              }))
            }
          />

          <div
            className="safi-chip-row"
            role="group"
            aria-label={isAr ? "نوع المخطط" : "Chart type"}
          >
            {CHART_TYPES.map((type) => (
              <button
                key={type}
                type="button"
                className={type === chartType ? "safi-chip safi-chip--active" : "safi-chip"}
                aria-pressed={type === chartType}
                onClick={() => setChartType(type)}
              >
                {CHART_TYPE_LABELS[type][isAr ? "ar" : "en"]}
              </button>
            ))}
          </div>

          <img
            src={imageUri}
            alt={chartTitle}
            className="safi-chart-canvas"
            data-testid="safi-chart-image"
          />

          <p className="safi-hint">
            <BarChart3 size={13} aria-hidden="true" />{" "}
            {isAr ? "المواصفة صالحة:" : "Valid specification:"}{" "}
            {validation.valid
              ? isAr
                ? "نعم"
                : "Yes"
              : validation.errors
                  .map((error) => {
                    if (isAr) return error;
                    const code = error.match(/^SF-CHT-\d{3}/)?.[0] ?? "";
                    const message = CHART_VALIDATION_EN[code];
                    return message ? `${code}: ${message}` : error;
                  })
                  .join(" · ")}
          </p>
        </SpotlightCard>

        <SpotlightCard padding={24}>
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              marginBottom: "8px"
            }}
          >
            <h2 className="safi-card-title" style={{ margin: 0 }}>
              {isAr ? "تحرير بيانات المخطط مباشرةً" : "Edit chart data"}
            </h2>
            <button
              type="button"
              onClick={() =>
                setRows([
                  ...rows,
                  {
                    region: isAr ? `محافظة ${rows.length + 1}` : `Region ${rows.length + 1}`,
                    completion: 80
                  }
                ])
              }
              style={{ minHeight: "32px", padding: "4px 10px", fontSize: "11px" }}
            >
              <Plus size={12} aria-hidden="true" /> {isAr ? "إضافة صف" : "Add row"}
            </button>
          </div>

          <div
            style={{ display: "flex", flexDirection: "column", gap: "6px", marginBottom: "18px" }}
          >
            {rows.map((r, idx) => (
              <div
                key={idx}
                style={{
                  display: "grid",
                  gridTemplateColumns: "1fr 110px",
                  gap: "8px",
                  alignItems: "center"
                }}
              >
                <input
                  value={r.region}
                  onChange={(e) => {
                    const next = [...rows];
                    next[idx] = { ...r, region: e.target.value };
                    setRows(next);
                  }}
                  style={{ minHeight: "34px", padding: "4px 8px", fontSize: "12px" }}
                  aria-label={isAr ? `اسم المحافظة ${idx + 1}` : `Region name ${idx + 1}`}
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
                  aria-label={isAr ? `نسبة الاكتمال ${idx + 1}` : `Completion rate ${idx + 1}`}
                />
              </div>
            ))}
          </div>

          <h2 className="safi-card-title">
            <AlertTriangle size={16} aria-hidden="true" />{" "}
            {isAr ? "حارس المصداقية" : "Credibility Guard"}
          </h2>
          {credibility.length === 0 ? (
            <p className="safi-status safi-status--ok">
              <CheckCircle2 size={14} aria-hidden="true" />{" "}
              {isAr ? "لا توجد ملاحظات على هذا المخطط." : "No issues found in this chart."}
            </p>
          ) : (
            <ul className="safi-list">
              {credibility.map((warning) => (
                <li key={warning.code} data-credibility-code={warning.code}>
                  <strong className="safi-mono">{warning.code}</strong> —{" "}
                  {isAr ? warning.messageAr : warning.messageEn}
                  <span className="safi-hint">
                    {isAr ? warning.suggestionAr : warning.suggestionEn}
                  </span>
                </li>
              ))}
            </ul>
          )}

          <h2 className="safi-card-title" style={{ marginTop: "18px" }}>
            <Eye size={16} aria-hidden="true" />{" "}
            {isAr ? "التوصيات الذكية" : "Smart recommendations"}
          </h2>
          <ul className="safi-list">
            {recommendations.map((recommendation) => (
              <li key={recommendation.type}>
                <strong>{isAr ? recommendation.titleAr : recommendation.titleEn}</strong>
                <span className="safi-hint">
                  {isAr ? recommendation.reasonAr : recommendation.reasonEn}
                </span>
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
          {isAr ? "التحقق من تباين WCAG 2.2 AA" : "WCAG 2.2 AA contrast audit"}
        </h2>

        <div className="safi-table-wrap">
          <table className="safi-table">
            <thead>
              <tr>
                <th scope="col">{isAr ? "اللون" : "Colour"}</th>
                <th scope="col">{isAr ? "الخلفية" : "Background"}</th>
                <th scope="col">{isAr ? "النسبة" : "Ratio"}</th>
                <th scope="col">{isAr ? "الحد المطلوب" : "Minimum"}</th>
                <th scope="col">{isAr ? "الحالة" : "Status"}</th>
              </tr>
            </thead>
            <tbody>
              {[...svgAudit, ...paletteAudit].map((row) => (
                <tr key={`${row.colour}-${row.background}`}>
                  <td>
                    <span
                      style={{
                        display: "inline-block",
                        width: "12px",
                        height: "12px",
                        borderRadius: "4px",
                        backgroundColor: row.colour,
                        marginInlineEnd: "8px",
                        verticalAlign: "middle"
                      }}
                    />
                    <strong>{isAr ? row.nameAr : row.nameEn}</strong>
                  </td>
                  <td>{isAr ? row.bgNameAr : row.bgNameEn}</td>
                  <td className="safi-mono" dir="ltr">
                    {row.ratio.toFixed(2)}:1
                  </td>
                  <td className="safi-mono" dir="ltr">
                    {row.minimum.toFixed(1)}:1
                  </td>
                  <td>
                    {row.ratio >= row.minimum ? (
                      <span className="safi-status safi-status--ok">
                        {isAr ? "✓ يجتاز" : "✓ Pass"}
                      </span>
                    ) : (
                      <span className="safi-status safi-status--bad">
                        {isAr ? "✗ يرسب" : "✗ Fail"}
                      </span>
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
