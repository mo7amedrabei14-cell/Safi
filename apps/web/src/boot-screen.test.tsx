import { readFileSync } from "node:fs";
import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { App } from "./App";

const entryHtml = readFileSync(new URL("../index.html", import.meta.url), "utf8");
const bootCss = readFileSync(new URL("../public/boot.css", import.meta.url), "utf8");
const appCss = readFileSync(new URL("./index.css", import.meta.url), "utf8");
const introSource = readFileSync(
  new URL("./components/brand/SafiIntroSplash.tsx", import.meta.url),
  "utf8"
);

describe("SafiData first-paint introduction", () => {
  it("renders the cinematic intro on the first React commit", () => {
    const html = renderToString(createElement(App, null));

    expect(html).toContain('class="safi-intro-splash"');
    expect(html).toContain("عرض إقلاع صافي البيانات السينمائي");
  });

  it("keeps the intro stylesheet valid and restores its required layout rules", () => {
    expect(appCss).toContain(".safi-intro-splash");
    expect(appCss).toContain("color: #F5F5F7;");
    expect(appCss).toContain(".safi-intro-copy");
    expect(appCss).toContain(".safi-intro-progress-fill");
    expect(appCss).toContain('data-fonts-ready="false"');
    expect(appCss.trimStart().startsWith("import ")).toBe(false);
    expect(appCss).not.toContain('describe("UI-Phase-5');
  });

  it("holds the opening frame still and guarantees a complete progress handoff", () => {
    expect(introSource).toContain("INTRO_START_PAUSE_MS = 1_000");
    expect(introSource).toContain("INTRO_PROGRESS_DURATION_SECONDS = 2.25");
    expect(introSource).toContain("INTRO_PROGRESS_COMPLETION_HOLD_SECONDS = 0.22");
    const pauseWaitIndex = introSource.indexOf("window.setTimeout(resolve, INTRO_START_PAUSE_MS)");
    expect(pauseWaitIndex).toBeGreaterThan(-1);
    expect(pauseWaitIndex).toBeLessThan(introSource.indexOf("gsap.timeline({"));
    expect(introSource).toContain(
      'gsap.set(progress, { scaleX: 1, transformOrigin: "right center" })'
    );
    expect(appCss).toContain(
      '.safi-intro-splash:not([data-motion-ready="true"]) .safi-intro-progress-fill'
    );
    expect(appCss).toContain(
      '.safi-intro-splash:not([data-motion-ready="true"]) .safi-intro-orb-ring'
    );
  });

  it("provides a boot curtain before the React root with local styles", () => {
    const curtainIndex = entryHtml.indexOf('id="safi-boot-screen"');
    const rootIndex = entryHtml.indexOf('id="root"');

    expect(entryHtml).toContain('<link rel="stylesheet" href="/boot.css" />');
    expect(curtainIndex).toBeGreaterThanOrEqual(0);
    expect(curtainIndex).toBeLessThan(rootIndex);
    expect(bootCss).toContain("background: #090B10;");
    expect(bootCss).toContain("@media (prefers-reduced-motion: reduce)");
    const firstPaintMarkup = entryHtml.slice(curtainIndex, rootIndex);
    const visibleFirstPaintMarkup = firstPaintMarkup.split("<noscript>")[0] ?? firstPaintMarkup;
    expect(visibleFirstPaintMarkup).not.toContain("صافي البيانات");
    expect(visibleFirstPaintMarkup).not.toContain("SafiData");
  });
});
