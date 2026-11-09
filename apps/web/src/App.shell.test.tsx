import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { App, SHELL_TABS, renderStudio } from "./App";
import { DOCK_TABS } from "./components/layout/FloatingGlassDock";

const noop = (): void => undefined;
const entryHtml = readFileSync(new URL("../index.html", import.meta.url), "utf8");
const bootCss = readFileSync(new URL("../public/boot.css", import.meta.url), "utf8");

describe("UI-Phase-5 application shell", () => {
  it("renders the home stage, the six-tab dock and the preserved semantic strings", () => {
    const html = renderToString(createElement(App, null));

    expect(SHELL_TABS).toEqual(["home", "cleaning", "recipes", "charts", "library", "owner"]);
    expect(html).toContain('data-active-tab="home"');
    expect(html).toContain('class="safi-intro-splash"');
    expect(html).toContain("مرحبًا بك في منصة صافي البيانات");
    expect(html).toContain("الإصدار الذهبي المعتمد v1.0.0-s5-gold");

    for (const label of ["الرئيسية", "المكتبة", "الإعدادات"]) {
      expect(html).toContain(label);
    }
    expect(html.split("data-tab-id=").length - 1).toBe(DOCK_TABS.length);
    expect(html).toContain("⌘K");

    // The intro is present on the first React render, before any effect can run.
    expect(html).toContain("عرض إقلاع صافي البيانات السينمائي");
    expect(html).not.toContain('role="dialog"');
    expect(html).toContain('role="status"');
  });

  it("ships a first-paint curtain before the React root with CSP-compatible local styles", () => {
    expect(entryHtml).toContain('<link rel="stylesheet" href="/boot.css" />');
    expect(entryHtml.indexOf('id="safi-boot-screen"')).toBeLessThan(
      entryHtml.indexOf('id="root"')
    );
    expect(entryHtml).toContain('id="safi-boot-screen"');
    expect(bootCss).toContain("background: #090B10;");
    expect(bootCss).toContain("@media (prefers-reduced-motion: reduce)");
    expect(bootCss).not.toContain("style-src");
  });

  it("renders a distinct studio for every tab", () => {
    const expectations: Array<[string, string]> = [
      ["home", "الإصدار الذهبي المعتمد"],
      ["cleaning", "مختبر التنقية والخصوصية"],
      ["recipes", "محرك الوصفات الحتمية"],
      ["charts", "التحقق من تباين WCAG 2.2 AA"],
      ["library", "مكتبة القوالب وتقارير الجودة"],
      ["owner", "لوحة تحكم المالك"]
    ];

    for (const tab of SHELL_TABS) {
      const html = renderToString(renderStudio(tab, noop));
      const match = expectations.find(([id]) => id === tab);
      expect(match).toBeDefined();
      expect(html).toContain(match?.[1] ?? "");
    }

    const owner = renderToString(renderStudio("owner", noop));
    expect(owner.split("انشر تحديثًا").length - 1).toBe(1);
    expect(owner).toContain("6. الأمان وسجل التدقيق");
    expect(owner).toContain("safi-owner-frame");
  });
});
