import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { App } from "./App";
import { ApplicationShell, SHELL_TABS, renderStudio } from "./ApplicationShell";
import { DOCK_TABS } from "./components/layout/FloatingGlassDock";

const noop = (): void => undefined;

describe("SafiData startup and application shell", () => {
  it("renders only the intro during the initial application commit", () => {
    const html = renderToString(createElement(App, null));

    expect(html).toContain('class="safi-intro-splash"');
    expect(html).toContain("SafiData cinematic startup");
    expect(html).toContain('<bdi dir="ltr">SafiData</bdi>');
    expect(html).not.toContain("صافي البيانات");
    expect(html).not.toContain('class="safi-shell"');
    expect(html).not.toContain("data-safi-ambient");
    expect(html).not.toContain('data-active-tab="home"');
    expect(html).not.toContain("data-safi-word");
  });

  it("renders the home stage and six-tab dock after the intro handoff", () => {
    const html = renderToString(createElement(ApplicationShell, { onReady: noop }));

    expect(SHELL_TABS).toEqual(["home", "cleaning", "recipes", "charts", "library", "owner"]);
    expect(html).toContain('data-active-tab="home"');
    expect(html).toContain("Welcome");
    expect(html).toContain("SafiData");
    expect(html).toContain("Intelligent Universal Data Engineering");
    expect(html).toContain("Certified Gold Edition v1.0.0-s5-gold");

    for (const label of ["Website", "Library", "Settings"]) {
      expect(html).toContain(label);
    }
    expect(html.split("data-tab-id=").length - 1).toBe(DOCK_TABS.length);
    expect(html).toContain("⌘K");
    expect(html).not.toContain('role="dialog"');
    expect(html).toContain('role="status"');
  });

  it("keeps the PWA name and public share card aligned with the English default", () => {
    const englishManifest = JSON.parse(
      readFileSync(new URL("../public/manifest.webmanifest", import.meta.url), "utf8")
    );
    const arabicManifest = JSON.parse(
      readFileSync(new URL("../public/manifest.ar.webmanifest", import.meta.url), "utf8")
    );
    const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
    const bootLocale = readFileSync(new URL("../public/boot-locale.js", import.meta.url), "utf8");
    const shareCard = readFileSync(new URL("../public/og-preview.svg", import.meta.url), "utf8");

    expect(englishManifest.name).toBe("SafiData");
    expect(englishManifest.short_name).toBe("SafiData");
    expect(arabicManifest.name).toBe("صافي البيانات");
    expect(arabicManifest.short_name).toBe("صافي");
    expect(html).toContain('id="safi-manifest"');
    expect(bootLocale).toContain("/manifest.ar.webmanifest");
    expect(shareCard).toContain("SafiData");
    expect(shareCard).not.toMatch(/[\\u0600-\\u06FF]/);
  });

  it("uses English as the first-run locale for the default studio", () => {
    const html = renderToString(renderStudio("home", noop));
    expect(html).toContain("Welcome to SafiData");
    expect(html).not.toContain("مرحبًا بك في منصة صافي البيانات");
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
      const html = renderToString(renderStudio(tab, noop, "ar"));
      const match = expectations.find(([id]) => id === tab);
      expect(match).toBeDefined();
      expect(html).toContain(match?.[1] ?? "");
    }

    const owner = renderToString(renderStudio("owner", noop, "ar"));
    expect(owner.split("انشر تحديثًا").length - 1).toBe(1);
    expect(owner).toContain("6. الأمان وسجل التدقيق");
    expect(owner).toContain("safi-owner-frame");
  });
});
