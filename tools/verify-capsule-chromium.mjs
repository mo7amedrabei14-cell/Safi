import assert from "node:assert/strict";
import { mkdir, stat } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { execFileSync } from "node:child_process";

async function loadPlaywright() {
  const candidates = [
    process.env.PLAYWRIGHT_MODULE_URL,
    process.platform === "win32"
      ? pathToFileURL(
          resolve(process.env.TEMP ?? "C:/Temp", "safi-playwright/node_modules/playwright/index.mjs")
        ).href
      : "file:///tmp/safi-playwright/node_modules/playwright/index.mjs"
  ].filter(Boolean);

  for (const url of candidates) {
    try {
      return await import(url);
    } catch {
      // Try next candidate.
    }
  }
  throw new Error(
    "PLAYWRIGHT_MODULE_URL is not set and no fallback Playwright installation was found."
  );
}

const { chromium } = await loadPlaywright();

const artifactDir = resolve("artifacts/intro-visual");
await mkdir(artifactDir, { recursive: true });
const capsulePath = resolve(artifactDir, "quad-studio-sample.safi.html");

console.log("[1/3] Generating real .safi.html capsule and verifying SF-CAL-006 missing-metric gate via Rust...");
execFileSync(
  "cargo",
  [
    "test",
    "-p",
    "sf-desktop",
    "--test",
    "studio_phase5_e2e",
    "studio_eighteen_journeys_end_to_end",
    "--",
    "--exact"
  ],
  {
    stdio: "inherit",
    env: {
      ...process.env,
      SAFI_SAMPLE_CAPSULE_OUT: capsulePath
    }
  }
);

const fileStat = await stat(capsulePath);
assert.ok(fileStat.size > 2048, `Expected exported .safi.html capsule to be > 2 KiB, got ${fileStat.size} bytes.`);

const fileUrl = pathToFileURL(capsulePath).href;
console.log(`[2/3] Opening exported capsule over file:// in headless Chromium: ${fileUrl}`);

const browser = await chromium.launch({ headless: true, args: ["--no-sandbox"] });
try {
  const context = await browser.newContext({
    viewport: { width: 1440, height: 1200 },
    deviceScaleFactor: 1
  });
  const page = await context.newPage();

  const pageErrors = [];
  const consoleErrors = [];
  const externalRequests = [];

  page.on("pageerror", (error) => {
    pageErrors.push(error.message);
  });
  page.on("console", (msg) => {
    if (msg.type() === "error") {
      consoleErrors.push(msg.text());
    }
  });
  page.on("request", (req) => {
    const url = req.url();
    if (!url.startsWith("file://") && !url.startsWith("data:")) {
      externalRequests.push(url);
    }
  });

  await page.goto(fileUrl, { waitUntil: "load" });

  const inspection = await page.evaluate(() => {
    const scripts = document.querySelectorAll("script").length;
    const cspMeta =
      document
        .querySelector('meta[http-equiv="Content-Security-Policy"]')
        ?.getAttribute("content") ?? "";

    const isVisible = (selector) => {
      const el = document.querySelector(selector);
      if (!el) return false;
      const rect = el.getBoundingClientRect();
      const style = window.getComputedStyle(el);
      return (
        rect.width > 0 &&
        rect.height > 0 &&
        style.display !== "none" &&
        style.visibility !== "hidden"
      );
    };

    const chartTypes = Array.from(document.querySelectorAll("[data-chart-type]")).map((el) =>
      el.getAttribute("data-chart-type")
    );
    const chartVisualGeometry = {
      barTracks: document.querySelectorAll('[data-chart-type="bar"] .track').length,
      linePolylines: document.querySelectorAll('[data-chart-type="line"] polyline').length,
      areaPolygons: document.querySelectorAll('[data-chart-type="area"] polygon').length,
      pieSlices: document.querySelectorAll('[data-chart-type="pie"] path[data-pie-slice]').length,
      donutSlices: document.querySelectorAll('[data-chart-type="donut"] path[data-donut-slice]').length,
      scatterCircles: document.querySelectorAll('[data-chart-type="scatter"] circle').length,
      histogramBins: document.querySelectorAll('[data-chart-type="histogram"] rect[data-histogram-bin]').length,
      kpiValues: document.querySelectorAll('[data-chart-type="kpi"] [data-kpi-value]').length,
      kpiBars: document.querySelectorAll('[data-chart-type="kpi"] .bars').length
    };

    const freeformRowSpanEl = document.querySelector('[data-grid-row-span="3"]');
    const freeformComputedGridRow = freeformRowSpanEl
      ? window.getComputedStyle(freeformRowSpanEl).gridRow
      : "";
    const freeformComputedGridCol = freeformRowSpanEl
      ? window.getComputedStyle(freeformRowSpanEl).gridColumn
      : "";

    const images = Array.from(document.querySelectorAll("img")).map((img) => ({
      srcPrefix: img.getAttribute("src")?.slice(0, 22) ?? "",
      complete: img.complete,
      naturalWidth: img.naturalWidth,
      naturalHeight: img.naturalHeight
    }));

    const bodyText = document.body.innerText;

    return {
      scripts,
      cspMeta,
      dashboardVisible: isVisible("#dashboard"),
      slidesVisible: isVisible("#slides"),
      reportVisible: isVisible("#report"),
      freeformVisible: isVisible("#canvas"),
      metricsTableVisible: isVisible("#metrics"),
      chartTypes,
      chartVisualGeometry,
      freeformComputedGridRow,
      freeformComputedGridCol,
      images,
      hasVerifiedMetricValue: bodyText.includes("850") && bodyText.includes("EGP"),
      hasMetricFormula: bodyText.includes("SUM(amount)"),
      hasSlideTitle: bodyText.includes("Executive Deck"),
      hasReportCallout: bodyText.includes("Audit Summary")
    };
  });

  assert.equal(inspection.scripts, 0, "Exported .safi.html capsule must contain zero <script> tags.");
  assert.ok(
    inspection.cspMeta.includes("default-src 'none'") &&
      inspection.cspMeta.includes("script-src 'none'"),
    `Capsule CSP meta must lock default-src and script-src to 'none': ${inspection.cspMeta}`
  );
  assert.equal(inspection.dashboardVisible, true, "Dashboard section (#dashboard) must be visible in file:// capsule.");
  assert.equal(inspection.slidesVisible, true, "Keynote Slides section (#slides) must be visible in file:// capsule.");
  assert.equal(inspection.reportVisible, true, "Executive Report section (#report) must be visible in file:// capsule.");
  assert.equal(inspection.freeformVisible, true, "Free-Form Canvas section (#canvas) must be visible in file:// capsule.");
  assert.equal(
    inspection.metricsTableVisible,
    true,
    "Verified Metrics & Provenance section (#metrics) must be visible in file:// capsule."
  );

  for (const expectedType of [
    "bar",
    "line",
    "area",
    "pie",
    "donut",
    "scatter",
    "histogram",
    "kpi"
  ]) {
    assert.ok(
      inspection.chartTypes.includes(expectedType),
      `Capsule Dashboard must render chart type '${expectedType}', found: ${JSON.stringify(inspection.chartTypes)}`
    );
  }
  assert.ok(inspection.chartVisualGeometry.barTracks > 0, "Bar chart must render bar tracks.");
  assert.ok(inspection.chartVisualGeometry.linePolylines > 0, "Line chart must render SVG polyline.");
  assert.ok(inspection.chartVisualGeometry.areaPolygons > 0, "Area chart must render SVG polygon.");
  assert.ok(inspection.chartVisualGeometry.pieSlices > 0, "Pie chart must render SVG sector slices.");
  assert.ok(inspection.chartVisualGeometry.donutSlices > 0, "Donut chart must render SVG ring slices.");
  assert.ok(inspection.chartVisualGeometry.scatterCircles > 0, "Scatter chart must render SVG points.");
  assert.ok(inspection.chartVisualGeometry.histogramBins > 0, "Histogram chart must render SVG bin rectangles.");
  assert.ok(inspection.chartVisualGeometry.kpiValues > 0, "KPI card must render standalone KPI value.");
  assert.equal(inspection.chartVisualGeometry.kpiBars, 0, "KPI card must not render redundant .bars rows.");

  assert.ok(
    inspection.freeformComputedGridRow.includes("span 3"),
    `Free-Form block must preserve rowSpan=3 in computed CSS grid-row, got: ${inspection.freeformComputedGridRow}`
  );
  assert.ok(
    inspection.freeformComputedGridCol.includes("span 6"),
    `Free-Form block must preserve colSpan=6 in computed CSS grid-column, got: ${inspection.freeformComputedGridCol}`
  );

  assert.ok(inspection.images.length >= 1, "Capsule must contain embedded data:image asset.");
  for (const img of inspection.images) {
    assert.equal(img.srcPrefix, "data:image/png;base64,", "Embedded image must use data:image/png;base64 URI.");
    assert.equal(img.complete, true, "Embedded image must finish loading offline.");
    assert.ok(img.naturalWidth > 0 && img.naturalHeight > 0, "Embedded image must decode with non-zero dimensions.");
  }

  assert.equal(inspection.hasVerifiedMetricValue, true, "Capsule must display verified metric value 850 EGP.");
  assert.equal(inspection.hasMetricFormula, true, "Capsule must display verified formula SUM(amount).");
  assert.equal(inspection.hasSlideTitle, true, "Capsule must display slide content (Executive Deck).");
  assert.equal(inspection.hasReportCallout, true, "Capsule must display report callout content (Audit Summary).");
  assert.deepEqual(externalRequests, [], "Opening .safi.html must make zero external network requests.");
  assert.deepEqual(pageErrors, [], "Opening .safi.html must produce zero page errors.");
  assert.deepEqual(consoleErrors, [], "Opening .safi.html must produce zero console/CSP errors.");

  const screenshotPath = resolve(artifactDir, "capsule-file-protocol.png");
  await page.screenshot({ path: screenshotPath, fullPage: true });
  console.log(
    "[3/3] .safi.html capsule verified in real Chromium over file://:",
    JSON.stringify({
      capsulePath,
      screenshotPath,
      scripts: inspection.scripts,
      chartTypes: inspection.chartTypes,
      freeformGridRow: inspection.freeformComputedGridRow,
      imagesLoaded: inspection.images.length,
      externalRequests: externalRequests.length,
      consoleErrors: consoleErrors.length,
      pageErrors: pageErrors.length
    })
  );
  await context.close();
} finally {
  await browser.close();
}
