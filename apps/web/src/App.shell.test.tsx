import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { App, SHELL_TABS, renderStudio } from "./App";
import { DOCK_TABS } from "./components/layout/FloatingGlassDock";

const noop = (): void => undefined;

describe("UI-Phase-5 application shell", () => {
  it("renders the home stage, the six-tab dock and the preserved semantic strings", () => {
    const html = renderToString(createElement(App, null));

    expect(SHELL_TABS).toEqual(["home", "cleaning", "recipes", "charts", "library", "owner"]);
    expect(html).toContain('data-active-tab="home"');
    expect(html).toContain("مرحبًا بك في منصة صافي البيانات");
    expect(html).toContain("الإصدار الذهبي المعتمد v1.0.0-s5-gold");

    for (const label of ["الرئيسية", "المكتبة", "الإعدادات"]) {
      expect(html).toContain(label);
    }
    expect(html.split("data-tab-id=").length - 1).toBe(DOCK_TABS.length);
    expect(html).toContain("⌘K");

    // Both overlays start closed, so no dialog is in the tree.
    expect(html).not.toContain('role="dialog"');
    expect(html).toContain('role="status"');
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
