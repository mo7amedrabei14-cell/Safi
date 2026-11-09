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
const appSource = readFileSync(new URL("./App.tsx", import.meta.url), "utf8");

describe("SafiData first-paint introduction", () => {
  it("renders the cinematic intro on the first React commit", () => {
    const html = renderToString(createElement(App, null));

    expect(html).toContain('class="safi-intro-splash"');
    expect(html).toContain("SafiData cinematic startup");
    expect(html).not.toContain("صافي البيانات");
  });

  it("keeps one shared intro stylesheet with the required layout rules", () => {
    expect(bootCss).toContain(".safi-intro-splash");
    expect(bootCss.toLowerCase()).toContain("color: #f5f5f7;");
    expect(bootCss).toContain(".safi-intro-copy");
    expect(bootCss).not.toContain("safi-intro-progress");
    expect(bootCss).toContain(".safi-intro-transition-path");
    expect(bootCss).toContain('[dir="rtl"] .safi-intro-arrow');
    expect(bootCss).toContain(".safi-intro-skip:focus-visible");
    expect(bootCss).toContain("safiIntroBootHalo 1.2s ease-in-out infinite alternate");
    expect(bootCss).toContain('data-fonts-ready="false"');
    expect(appCss).not.toContain(".safi-intro-splash {");
    expect(appCss.trimStart().startsWith("import ")).toBe(false);
    expect(appCss).not.toContain('describe("UI-Phase-5');
  });

  it("starts the intro and requests the application without a timed wait", () => {
    expect(introSource).not.toContain("INTRO_START_PAUSE_MS");
    expect(introSource).toContain("INTRO_MOTION_DURATION_SECONDS = 0.58");
    expect(introSource).toContain("INTRO_EXIT_DURATION_SECONDS = 0.48");
    expect(introSource).not.toContain("INTRO_PROGRESS");
    expect(introSource.indexOf('root.dataset.motionReady = "true"')).toBeLessThan(
      introSource.indexOf("gsap.timeline({")
    );
    expect(introSource).toContain("onExitComplete?.()");
    expect(introSource).toContain('rootRef.current.dataset.motionComplete = "true"');
    expect(introSource).toContain("strokeDashoffset: 0");
    expect(introSource).toContain("clipPath: `circle(0px at ${endX}px ${centerY}px)`");
    expect(introSource).toContain('className="safi-intro-transition-path"');
    expect(appSource).toContain("<ApplicationShell");
    expect(appSource).not.toContain("preloadTimer");
    expect(appSource).not.toContain("applicationRequested");
    expect(bootCss).not.toContain("safi-intro-progress");
    expect(bootCss).toContain(".safi-intro-splash .safi-pm-logo-orbit");
    expect(bootCss).toContain("animation: safiIntroOrbit 1.4s linear infinite");
    expect(introSource).toContain("useLayoutEffect(() =>");
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
    expect(firstPaintMarkup).not.toContain("safi-intro-progress");
    expect(firstPaintMarkup).toContain('class="safi-intro-arrow"');
    expect(firstPaintMarkup.split("data-intro-dot=").length - 1).toBe(9);
    expect(firstPaintMarkup.split("data-intro-bar=").length - 1).toBe(3);
    expect(firstPaintMarkup).not.toContain("safi-boot-progress");
  });
});
