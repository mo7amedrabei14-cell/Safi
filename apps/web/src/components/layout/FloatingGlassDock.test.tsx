import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { DOCK_TABS, FloatingGlassDock } from "./FloatingGlassDock";

describe("UI-Phase-4 floating glass dock", () => {
  it("renders the six navigation tabs with the semantic labels", () => {
    const html = renderToString(
      createElement(FloatingGlassDock, { activeTab: "home", onSelect: () => undefined })
    );

    expect(DOCK_TABS).toHaveLength(6);
    for (const label of [
      "الرئيسية",
      "التنظيف وPII",
      "الوصفات",
      "الرسوم البيانية",
      "المكتبة",
      "لوحة المالك"
    ]) {
      expect(html).toContain(label);
    }

    // Legacy semantic strings must survive so earlier suites keep passing.
    expect(html).toContain("المكتبة");
    expect(html).toContain("الإعدادات");
    expect(html).toContain("التنقل الرئيسي");
  });

  it("marks exactly the active tab and keeps the brand, island and palette triggers", () => {
    const html = renderToString(
      createElement(FloatingGlassDock, { activeTab: "charts", onSelect: () => undefined })
    );

    expect(html.split('aria-current="page"').length - 1).toBe(1);
    expect(html).toContain('data-tab-id="charts"');
    expect(html).toContain('aria-label="شعار صافي البيانات"');
    expect(html).toContain('aria-label="لوحة الأوامر السريعة"');
    expect(html).toContain("⌘K");
    expect(html).toContain("نواة Rust نشطة");
    expect(html).toContain('aria-label="الإعدادات"');
  });

  it("keeps every dock control at the 44px touch target", () => {
    const html = renderToString(
      createElement(FloatingGlassDock, { activeTab: "home", onSelect: () => undefined })
    );

    const targets = html.split("min-height:44px").length - 1;
    expect(targets).toBeGreaterThanOrEqual(8);
  });
});
