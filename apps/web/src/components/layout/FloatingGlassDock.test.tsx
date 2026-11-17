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
      "Website",
      "Cleaning App",
      "Recipes App",
      "Charts App",
      "Library App",
      "Owner App"
    ]) {
      expect(html).toContain(label);
    }

    // Legacy semantic strings must survive so earlier suites keep passing.
    expect(html).toContain("Library");
    expect(html).toContain("Settings");
    expect(html).toContain("Account");
    expect(html).toContain("Main navigation");
  });

  it("marks exactly the active tab and keeps the brand, island and palette triggers", () => {
    const html = renderToString(
      createElement(FloatingGlassDock, { activeTab: "charts", onSelect: () => undefined })
    );

    expect(html.split('aria-current="page"').length - 1).toBe(1);
    expect(html).toContain('data-tab-id="charts"');
    expect(html).toContain('aria-label="SafiData logo"');
    expect(html).toContain('aria-label="Command palette"');
    expect(html).toContain("⌘K");
    expect(html).toContain("Rust engine active");
    expect(html).toContain('aria-label="Settings"');
  });

  it("renders the Arabic brand only when Arabic is selected", () => {
    const html = renderToString(
      createElement(FloatingGlassDock, { activeTab: "home", onSelect: () => undefined, lang: "ar" })
    );
    expect(html).toContain('aria-label="شعار صافي البيانات"');
    expect(html).toContain("صافي البيانات");
    expect(html).not.toContain('aria-label="SafiData logo"');
  });

  it("labels the language switch with the destination language", () => {
    const english = renderToString(
      createElement(FloatingGlassDock, { activeTab: "home", onSelect: () => undefined, lang: "en" })
    );
    const arabic = renderToString(
      createElement(FloatingGlassDock, { activeTab: "home", onSelect: () => undefined, lang: "ar" })
    );

    expect(english).toContain(">العربية</span>");
    expect(english).not.toContain(">Arabic</span>");
    expect(arabic).toContain(">English</span>");
  });

  it("renders localized 2-mode Light Mode / Dark Mode controls backed by vision-glass and google-gemini", () => {
    const english = renderToString(
      createElement(FloatingGlassDock, { activeTab: "home", onSelect: () => undefined, lang: "en" })
    );
    const arabic = renderToString(
      createElement(FloatingGlassDock, {
        activeTab: "home",
        onSelect: () => undefined,
        lang: "ar",
        theme: "google-gemini"
      })
    );

    expect(english).toContain('data-theme-current="vision-glass"');
    expect(english).toContain("Light Mode");
    expect(english).toContain("Dark Mode");
    expect(english).not.toContain("Vision Glass");
    expect(english).not.toContain("Gemini Dark");
    expect(english).not.toContain("Apple Light");

    expect(arabic).toContain('data-theme-current="google-gemini"');
    expect(arabic).toContain("الوضع الفاتح");
    expect(arabic).toContain("الوضع الداكن");
  });

  it("keeps every dock control at the 44px touch target", () => {
    const html = renderToString(
      createElement(FloatingGlassDock, { activeTab: "home", onSelect: () => undefined })
    );

    const targets = html.split("min-height:44px").length - 1;
    expect(targets).toBeGreaterThanOrEqual(8);
  });
});
