import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { spawn } from "node:child_process";
import { setTimeout as delay } from "node:timers/promises";

const moduleUrl = process.env.PLAYWRIGHT_MODULE_URL;
if (!moduleUrl) {
  throw new Error("PLAYWRIGHT_MODULE_URL must point to the pinned Playwright installation.");
}
const { chromium } = await import(moduleUrl);

const baseUrl = "http://127.0.0.1:4173";
const artifactDir = resolve("artifacts/intro-visual");
await mkdir(artifactDir, { recursive: true });

const server = spawn(
  "pnpm",
  ["--filter", "@safi/web", "exec", "vite", "preview", "--host", "127.0.0.1", "--port", "4173", "--strictPort"],
  { stdio: "inherit" }
);

async function waitForServer() {
  for (let attempt = 0; attempt < 120; attempt += 1) {
    if (server.exitCode !== null) {
      throw new Error(`Vite preview exited early with code ${server.exitCode}.`);
    }
    try {
      const response = await fetch(baseUrl);
      if (response.ok) return;
    } catch {
      // The preview server has not bound its port yet.
    }
    await delay(250);
  }
  throw new Error("Vite preview did not become ready within 30 seconds.");
}

let browser;
try {
  await waitForServer();
  browser = await chromium.launch({ headless: true, args: ["--no-sandbox"] });

  const context = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    deviceScaleFactor: 1
  });
  const page = await context.newPage();
  const pageErrors = [];
  page.on("pageerror", (error) => pageErrors.push(error.message));

  const cdp = await context.newCDPSession(page);
  await cdp.send("Network.enable");
  await cdp.send("Network.setCacheDisabled", { cacheDisabled: true });
  await cdp.send("Network.emulateNetworkConditions", {
    offline: false,
    latency: 150,
    downloadThroughput: 200 * 1024,
    uploadThroughput: 75 * 1024,
    connectionType: "cellular3g"
  });
  await cdp.send("Emulation.setCPUThrottlingRate", { rate: 4 });

  await page.addInitScript(() => {
    window.__safiIntroTrace = {
      fontsReadyAt: null,
      motionStartedAt: null,
      progressFullAt: null,
      progressWidthRatio: null,
      introRemovedAt: null
    };
  });

  await page.goto(baseUrl, { waitUntil: "domcontentloaded" });
  await page.waitForSelector(".safi-intro-splash", { state: "visible", timeout: 20_000 });
  await page.waitForFunction(
    () => document.querySelector(".safi-intro-splash")?.dataset.fontsReady === "true",
    undefined,
    { timeout: 30_000 }
  );
  await page.evaluate(() => {
    const trace = window.__safiIntroTrace;
    if (trace && trace.fontsReadyAt === null) {
      trace.fontsReadyAt = performance.now();
    }
  });

  const pausedFrame = await page.evaluate(() => {
    const root = document.querySelector(".safi-intro-splash");
    const fill = root?.querySelector(".safi-intro-progress-fill");
    const ring = root?.querySelector(".safi-intro-orb-ring");
    const dots = root ? Array.from(root.querySelectorAll("[data-intro-dot]")) : [];
    const title = root?.querySelector(".safi-intro-title");
    return {
      motionStartedAt: window.__safiIntroTrace?.motionStartedAt ?? null,
      progressTransform: fill ? getComputedStyle(fill).transform : null,
      progressScale: fill ? new DOMMatrixReadOnly(getComputedStyle(fill).transform).a : null,
      visibleDataDots: dots.filter((dot) => Number(getComputedStyle(dot).opacity) > 0.9).length,
      ringAnimation: ring ? getComputedStyle(ring).animationName : null,
      arabicTitle: title?.querySelector('bdi[dir="rtl"]')?.textContent ?? "",
      englishTitle: title?.querySelector('bdi[dir="ltr"]')?.textContent ?? ""
    };
  });

  assert.equal(pausedFrame.motionStartedAt, null, "Motion started before the one-second still frame.");
  assert.equal(pausedFrame.ringAnimation, "none", "The decorative ring must be still during the opening pause.");
  assert.equal(pausedFrame.progressScale, 0, "The progress bar must remain at zero during the opening pause.");
  assert.equal(pausedFrame.visibleDataDots, 9, "All nine data points must form a visible, still logo before motion begins.");
  assert.equal(pausedFrame.arabicTitle, "صافي البيانات", "The Arabic title must remain a single RTL run.");
  assert.equal(pausedFrame.englishTitle, "SafiData", "The product name must remain an isolated LTR run.");
  await page.screenshot({ path: resolve(artifactDir, "intro-still-frame.png"), fullPage: true });

  await page.waitForFunction(
    () => document.querySelector(".safi-intro-splash")?.dataset.motionReady === "true",
    undefined,
    { timeout: 10_000 }
  );
  await page.evaluate(() => {
    const trace = window.__safiIntroTrace;
    if (trace && trace.motionStartedAt === null) {
      trace.motionStartedAt = performance.now();
    }
  });

  const timing = await page.evaluate(() => {
    const trace = window.__safiIntroTrace;
    return {
      fontsReadyToMotionMs: trace.motionStartedAt - trace.fontsReadyAt
    };
  });
  assert.ok(
    timing.fontsReadyToMotionMs >= 950,
    `Expected a one-second pause after fonts became ready; measured ${timing.fontsReadyToMotionMs}ms.`
  );

  try {
    await page.waitForFunction(() => {
      const root = document.querySelector(".safi-intro-splash");
      const fill = root?.querySelector(".safi-intro-progress-fill");
      const track = root?.querySelector(".safi-intro-progress-track");
      if (!fill || !track) return false;

      const trackWidth = track.getBoundingClientRect().width;
      const fillWidth = fill.getBoundingClientRect().width;
      const ratio = trackWidth > 0 ? fillWidth / trackWidth : 0;
      if (ratio < 0.995) return false;

      const trace = window.__safiIntroTrace;
      if (trace && trace.progressFullAt === null) {
        trace.progressFullAt = performance.now();
        trace.progressWidthRatio = ratio;
      }
      return true;
    }, undefined, { timeout: 10_000 });
  } catch (error) {
    const state = await page.evaluate(() => {
      const root = document.querySelector(".safi-intro-splash");
      const fill = root?.querySelector(".safi-intro-progress-fill");
      const track = root?.querySelector(".safi-intro-progress-track");
      const trackWidth = track?.getBoundingClientRect().width ?? 0;
      const fillWidth = fill?.getBoundingClientRect().width ?? 0;
      return {
        introPresent: Boolean(root),
        motionReady: root?.dataset.motionReady ?? null,
        progressTransform: fill ? getComputedStyle(fill).transform : null,
        progressWidth: fillWidth,
        trackWidth,
        progressWidthRatio: trackWidth > 0 ? fillWidth / trackWidth : null,
        trace: window.__safiIntroTrace
      };
    });
    await page.screenshot({ path: resolve(artifactDir, "intro-progress-timeout.png"), fullPage: true });
    await writeFile(resolve(artifactDir, "visual-debug.json"), JSON.stringify(state, null, 2) + "\\n");
    throw new Error(`The intro progress bar did not reach 100% in Chromium: ${JSON.stringify(state)}; ${String(error)}`);
  }

  const completedFrame = await page.evaluate(() => ({
    progressWidthRatio: window.__safiIntroTrace?.progressWidthRatio ?? 0
  }));
  assert.ok(
    completedFrame.progressWidthRatio >= 0.995,
    `Expected the progress fill to cover the full track; ratio was ${completedFrame.progressWidthRatio}.`
  );
  await page.screenshot({ path: resolve(artifactDir, "intro-progress-100-percent.png"), fullPage: true });

  await page.waitForSelector(".safi-intro-splash", { state: "detached", timeout: 30_000 });
  await page.evaluate(() => {
    const trace = window.__safiIntroTrace;
    if (trace) trace.introRemovedAt = performance.now();
  });

  const finalTrace = await page.evaluate(() => ({ ...window.__safiIntroTrace }));
  const fullProgressHoldMs = finalTrace.introRemovedAt - finalTrace.progressFullAt;
  assert.ok(
    fullProgressHoldMs >= 180,
    `The full progress bar was not held before handoff; measured ${fullProgressHoldMs}ms.`
  );
  assert.deepEqual(pageErrors, [], `Unexpected browser runtime errors: ${pageErrors.join("; ")}`);

  const report = {
    viewport: "1440x900",
    cacheDisabled: true,
    networkProfile: "3G throttled (150ms latency, 200KB/s download)",
    cpuThrottleRate: 4,
    fontsReadyToMotionMs: timing.fontsReadyToMotionMs,
    progressWidthRatio: completedFrame.progressWidthRatio,
    fullProgressHoldMs,
    pageErrors
  };
  await writeFile(resolve(artifactDir, "visual-check.json"), JSON.stringify(report, null, 2) + "\n");
  console.log("Intro visual verification passed:", JSON.stringify(report));
  await context.close();
} finally {
  if (browser) await browser.close();
  server.kill("SIGTERM");
}
