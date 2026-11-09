import { describe, expect, it } from "vitest";
import { renderToString } from "react-dom/server";
import { WCAG_AA_UI_GRAPHIC_MIN_RATIO, contrastRatio } from "@safi/design-tokens";
import { CleaningStudio, SAMPLE_CLEANING_TEXT } from "./CleaningStudio";
import { RecipesStudio } from "./RecipesStudio";
import { ChartsStudio, SAMPLE_CHART_SPEC, extractSvgColors } from "./ChartsStudio";
import { LibraryStudio } from "./LibraryStudio";
import { generateDeterministicSvg } from "../../charts/chart-spec";
import { normalizeLines } from "../../lib/arabic-normalize";
import { detectPii } from "../../lib/pii";
import { RECIPE_TEMPLATES, recipeDigest } from "../../lib/recipe";

describe("UI-Phase-5 Cleaning & PII studio", () => {
  it("renders the normalization, masking and token tables", () => {
    const html = renderToString(<CleaningStudio lang="ar" />);

    expect(html).toContain("التنظيف وPII");
    expect(html).toContain("نتيجة التطبيع العربي");
    expect(html).toContain("الاستبدال الرمزي الحتمي (BLAKE3)");
    expect(html).toContain("01012345678");
    expect(html).toContain("010••••5678");
    expect(html.split("data-token-row=").length - 1).toBe(4);
    expect(html).toContain("sf_ph_");
    expect(html).toContain("لا يُرسل هذا النص إلى أي خادم");
  });

  it("defaults to English UI while retaining Arabic sample data", () => {
    const html = renderToString(<CleaningStudio />);

    expect(html).toContain("Cleaning, Privacy &amp; Workspace Lab");
    expect(html).toContain("Cleaning &amp; PII");
    expect(html).toContain("Normalization result");
    expect(html).toContain("Deterministic tokenization (BLAKE3)");
    expect(html).not.toContain("نتيجة التطبيع العربي");
    expect(html).not.toContain("الاستبدال الرمزي الحتمي (BLAKE3)");
    expect(html).toContain("أَحْمَدُ محمد عبد الله");
    expect(html).toContain("الاسم الكامل");
    expect(html).toContain("الرقم القومي");
    expect(html).toContain("تاريخ الصرف");
    expect(html).toContain("المبلغ (ج.م)");
    expect(html).toContain("رقم الهاتف");
    expect(html).not.toContain("Full name");
    expect(html).not.toContain("Payment date");
  });

  it("finds the four personal fields in the sample text", () => {
    const matches = detectPii(normalizeLines(SAMPLE_CLEANING_TEXT));
    expect(matches.map((match) => match.kind)).toEqual(["name", "phone", "email", "national_id"]);
  });
});

describe("UI-Phase-5 Recipes studio", () => {
  it("renders a deterministic canonical recipe pointing at a valid F1 folder", () => {
    const html = renderToString(<RecipesStudio lang="ar" />);

    expect(html).toContain("الوصفات");
    expect(html).toContain("الوصفة القانونية");
    expect(html).toContain("&quot;schema&quot;:&quot;safi.recipe&quot;");
    expect(html).toContain("مسار مطلق صالح");
    expect(html).toContain("تنزيل الوصفة");
    expect(html).toMatch(/[0-9a-f]{64}/);
    expect(html).toContain("الكتابة الذرية");
  });

  it("defaults to English and keeps the recipe output copy localized", () => {
    const html = renderToString(<RecipesStudio />);

    expect(html).toContain("Recipes");
    expect(html).toContain("Canonical recipe");
    expect(html).toContain("Atomic file writing");
    expect(html).not.toContain("الوصفة القانونية");
    expect(html).not.toContain("الكتابة الذرية");
  });
});

describe("UI-Phase-5 Chart studio", () => {
  it.each(["bar", "line", "pie", "donut"] as const)(
    "audits the real %s SVG colours against WCAG 2.2 AA",
    (type) => {
      const svg = generateDeterministicSvg({ ...SAMPLE_CHART_SPEC, type });
      const colors = extractSvgColors(svg);

      expect(colors.length).toBeGreaterThanOrEqual(3);
      for (const color of colors) {
        expect(contrastRatio(color, "#ffffff")).toBeGreaterThanOrEqual(
          WCAG_AA_UI_GRAPHIC_MIN_RATIO
        );
      }
    }
  );

  it("uses the selected locale for the deterministic SVG title", () => {
    const englishSvg = generateDeterministicSvg(
      { ...SAMPLE_CHART_SPEC, titleEn: "English chart title" },
      "en"
    );
    const arabicSvg = generateDeterministicSvg(
      { ...SAMPLE_CHART_SPEC, titleEn: "English chart title" },
      "ar"
    );

    expect(englishSvg).toContain(">English chart title</text>");
    expect(arabicSvg).toContain("نسبة اكتمال البيانات");
  });

  it("renders the preview, credibility guard and contrast table", () => {
    const html = renderToString(<ChartsStudio lang="ar" />);

    expect(html).toContain("الرسوم البيانية");
    expect(html).toContain("التحقق من تباين WCAG 2.2 AA");
    expect(html).toContain("حارس المصداقية");
    expect(html).toContain("SF-CHT-006");
    expect(html).toContain("يجتاز");
    expect(html).toContain("data:image/svg+xml");
  });

  it("defaults to English labels and validation guidance", () => {
    const html = renderToString(<ChartsStudio />);

    expect(html).toContain("Charts");
    expect(html).toContain("WCAG 2.2 AA contrast audit");
    expect(html).toContain("Credibility Guard");
    expect(html).not.toContain("الرسوم البيانية");
    expect(html).not.toContain("حارس المصداقية");
  });
});

describe("UI-Phase-5 Library studio", () => {
  it("lists the four templates with stable digests and honest signing copy", () => {
    const html = renderToString(<LibraryStudio lang="ar" />);

    expect(html.split("data-template-id=").length - 1).toBe(RECIPE_TEMPLATES.length);
    for (const template of RECIPE_TEMPLATES) {
      expect(html).toContain(template.nameAr);
      expect(recipeDigest(template.nameAr, template.toggles)).toMatch(/^[0-9a-f]{64}$/);
    }
    expect(html).toContain("عن التوقيع الرقمي");
    expect(html).toContain("ولا تدّعي هذه الصفحة أنها وقّعت شيئًا");
  });

  it("defaults to English template and signature guidance", () => {
    const html = renderToString(<LibraryStudio />);

    expect(html).toContain("Template library &amp; quality reports");
    expect(html).toContain("Recipe digest (BLAKE3)");
    expect(html).toContain("About digital signatures");
    expect(html).not.toContain("بصمة الوصفة (BLAKE3)");
    expect(html).not.toContain("عن التوقيع الرقمي");
  });
});
