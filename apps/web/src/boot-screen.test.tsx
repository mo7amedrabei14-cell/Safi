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

  it("keeps one shared intro stylesheet with the required layout rules", () => {
    expect(bootCss).toContain(".safi-intro-splash");
    expect(bootCss.toLowerCase()).toContain("color: #f5f5f7;");
    expect(bootCss).toContain(".safi-intro-copy");
    expect(bootCss).toContain(".safi-intro-progress-fill");
    expect(bootCss).toContain('data-fonts-ready="false"');
    expect(bootCss).toContain(
      '#safi-boot-screen[data-fonts-ready="false"] .safi-intro-progress-fill'
    );
    expect(bootCss).not.toContain(
      '.safi-intro-splash[data-fonts-ready="false"] .safi-intro-progress-fill'
    );
    expect(appCss).not.toContain(".safi-intro-splash {");
    expect(appCss.trimStart().startsWith("import ")).toBe(false);
    expect(appCss).not.toContain('describe("UI-Phase-5');
  });

  it("starts motion immediately and exits only after a complete progress handoff", () => {
    expect(introSource).not.toContain("INTRO_START_PAUSE_MS");
    expect(introSource).toContain("INTRO_PROGRESS_DURATION_SECONDS = 2.25");
    expect(introSource).toContain("INTRO_PROGRESS_COMPLETION_HOLD_SECONDS = 0.22");
    expect(introSource).toContain("INTRO_EXIT_DURATION_SECONDS = 0.65");
    expect(introSource.indexOf('root.dataset.motionReady = "true"')).toBeLessThan(
      introSource.indexOf("gsap.timeline({")
    );
    expect(introSource).toContain("onExitComplete?.()");
    expect(introSource).toContain(
      'gsap.set(progress, { scaleX: 1, transformOrigin: "right center" })'
    );
    expect(bootCss).toContain(
      '.safi-intro-splash:not([data-motion-ready="true"]) .safi-intro-progress-fill'
    );
    expect(bootCss).toContain(
      '.safi-intro-splash:not([data-motion-ready="true"]) .safi-intro-orb-ring'
    );
  });

  it("uses the exact React opening composition before JavaScript loads", () => {
    const curtainIndex = entryHtml.indexOf('id="safi-boot-screen"');
    const rootIndex = entryHtml.indexOf('id="root"');

    expect(entryHtml).toContain('<link rel="stylesheet" href="/boot.css" />');
    expect(curtainIndex).toBeGreaterThanOrEqual(0);
    expect(curtainIndex).toBeLessThan(rootIndex);
    expect(bootCss.toLowerCase()).toContain("background: #090b10;");
    expect(bootCss).toContain("@media (prefers-reduced-motion: reduce)");
    const firstPaintMarkup = entryHtml.slice(curtainIndex, rootIndex);
    expect(firstPaintMarkup).toContain('class="safi-intro-splash"');
    expect(firstPaintMarkup).toContain('class="safi-intro-stage"');
    expect(firstPaintMarkup).toContain('class="safi-intro-copy"');
    expect(firstPaintMarkup).toContain('class="safi-intro-progress-fill"');
    expect(firstPaintMarkup.split("data-intro-dot=").length - 1).toBe(9);
    expect(firstPaintMarkup.split("data-intro-bar=").length - 1).toBe(3);
    expect(firstPaintMarkup).not.toContain("safi-boot-progress");
  });
});
