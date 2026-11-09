import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { BentoCommandCenter } from "./BentoCommandCenter";
import { inspectImportFile, MAX_FILE_BYTES, WARN_FILE_BYTES } from "./SmartDropzone";

describe("UI-Phase-4 Bento command centre", () => {
  it("renders the gold badge, the word-split headline and the magnetic CTAs", () => {
    const html = renderToString(createElement(BentoCommandCenter, { lang: "ar" }));

    expect(html).toContain("الإصدار الذهبي المعتمد v1.0.0-s5-gold • معالجة محلية 100% بلا إنترنت");
    expect(html.startsWith("<div")).toBe(true);
    expect(html).toContain("<h1");
    expect(html).toContain("مرحبًا");
    expect(html).toContain("الهندسة");
    expect(html).toContain("العربية");
    expect(html).toContain("ابدأ الآن");
    expect(html).toContain("استعرض القوالب");
  });

  it("defaults to English copy when no locale is supplied", () => {
    const html = renderToString(createElement(BentoCommandCenter, null));
    expect(html).toContain("Welcome to SafiData");
    expect(html).not.toContain("مرحبًا بك في منصة صافي البيانات");
  });

  it("renders all four Bento cards with live metrics and the offline shield", () => {
    const html = renderToString(createElement(BentoCommandCenter, { lang: "ar" }));

    expect(html.split('data-bento-card="true"').length - 1).toBe(4);
    expect(html).toContain("safi-bento-wide");
    expect(html).toContain("99.9");
    expect(html).toContain("185");
    expect(html).toContain("ms");
    expect(html).toContain("اسحب ملف CSV أو XLSX هنا");
    expect(html).toContain("خزنة SQLCipher مع مفاتيح Argon2id");
    expect(html).toContain("Anti-Rollback");
    expect(html).toContain("حارس Zero Trust");
    expect(html).toContain("محاكاة خط الأنابيب الحي");
  });

  it("rejects unsupported, empty and oversized files before parsing", () => {
    expect(inspectImportFile({ name: "report.pdf", size: 1024 })).toMatchObject({
      ok: false,
      code: "SF-IMP-001"
    });
    expect(inspectImportFile({ name: "empty.csv", size: 0 })).toMatchObject({
      ok: false,
      code: "SF-IMP-001"
    });
    expect(inspectImportFile({ name: "huge.xlsx", size: MAX_FILE_BYTES + 1 })).toMatchObject({
      ok: false,
      code: "SF-IMP-005"
    });
    expect(inspectImportFile({ name: "big.csv", size: WARN_FILE_BYTES + 1 })).toMatchObject({
      ok: false,
      code: "SF-IMP-003"
    });
  });

  it("accepts CSV and XLSX within the safe size window", () => {
    expect(inspectImportFile({ name: "CLIENTS.CSV", size: 2048 })).toEqual({
      ok: true,
      kind: "csv",
      fileName: "CLIENTS.CSV"
    });
    expect(inspectImportFile({ name: "orders.xlsx", size: 4096 })).toEqual({
      ok: true,
      kind: "xlsx",
      fileName: "orders.xlsx"
    });
  });
});
