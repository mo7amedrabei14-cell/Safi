import { readFileSync } from "node:fs";
import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { App } from "./App";

const entryHtml = readFileSync(new URL("../index.html", import.meta.url), "utf8");
const bootCss = readFileSync(new URL("../public/boot.css", import.meta.url), "utf8");

describe("SafiData first-paint introduction", () => {
  it("renders the cinematic intro on the first React commit", () => {
    const html = renderToString(createElement(App, null));

    expect(html).toContain('class="safi-intro-splash"');
    expect(html).toContain("عرض إقلاع صافي البيانات السينمائي");
  });

  it("provides a boot curtain before the React root with local styles", () => {
    const curtainIndex = entryHtml.indexOf('id="safi-boot-screen"');
    const rootIndex = entryHtml.indexOf('id="root"');

    expect(entryHtml).toContain('<link rel="stylesheet" href="/boot.css" />');
    expect(curtainIndex).toBeGreaterThanOrEqual(0);
    expect(curtainIndex).toBeLessThan(rootIndex);
    expect(bootCss).toContain("background: #090B10;");
    expect(bootCss).toContain("@media (prefers-reduced-motion: reduce)");
  });
});
