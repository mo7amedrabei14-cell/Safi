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
  process.platform === "win32" ? "pnpm.cmd" : "pnpm",
  [
    "--filter",
    "@safi/web",
    "exec",
    "vite",
    "preview",
    "--host",
    "127.0.0.1",
    "--port",
    "4173",
    "--strictPort"
  ],
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
      reactMountedAt: null,
      motionStartedAt: null,
      fontsReadyAt: null,
      progressFullAt: null,
      progressWidthRatio: null,
      applicationMountedAtProgressFull: null,
      exitStartedAt: null,
      introRemovedAt: null,
      bootFrame: null,
      reactFrame: null
    };

    const readFrame = (root) => {
      const orb = root.querySelector(".safi-intro-orb")?.getBoundingClientRect();
      const fill = root.querySelector(".safi-intro-progress-fill");
      const ring = root.querySelector(".safi-intro-orb-ring");
      const dots = Array.from(root.querySelectorAll("[data-intro-dot]"));
      const bars = Array.from(root.querySelectorAll("[data-intro-bar]"));
      return {
        orb: orb ? { x: orb.x, y: orb.y, width: orb.width, height: orb.height } : null,
        progressScale: fill ? new DOMMatrixReadOnly(getComputedStyle(fill).transform).a : null,
        ringAnimation: ring ? getComputedStyle(ring).animationName : null,
        visibleDataDots: dots.filter((dot) => Number(getComputedStyle(dot).opacity) > 0.9).length,
        visibleBars: bars.filter((bar) => Number(getComputedStyle(bar).opacity) > 0.9).length
      };
    };

    const samplingStartedAt = performance.now();
    const sampleIntro = () => {
      const trace = window.__safiIntroTrace;
      const boot = document.querySelector("#safi-boot-screen");
      const react = document.querySelector("#root .safi-intro-splash");
      if (trace && boot) trace.bootFrame = readFrame(boot);
      if (trace && react) {
        if (trace.reactFrame === null) {
          trace.reactFrame = readFrame(react);
          trace.reactMountedAt = performance.now();
        }
        if (trace.motionStartedAt === null && react.dataset.motionReady === "true") {
          trace.motionStartedAt = performance.now();
        }
        if (trace.fontsReadyAt === null && react.dataset.fontsReady === "true") {
          trace.fontsReadyAt = performance.now();
        }
        if (trace.exitStartedAt === null && react.dataset.exiting === "true") {
          trace.exitStartedAt = performance.now();
        }
      }
      if (
        (trace?.exitStartedAt !== null && !react) ||
        performance.now() - samplingStartedAt > 30_000
      ) {
        return;
      }
      requestAnimationFrame(sampleIntro);
    };
    requestAnimationFrame(sampleIntro);
  });

  await page.goto(baseUrl, { waitUntil: "domcontentloaded" });
  await page.waitForSelector("#root .safi-intro-splash", { state: "visible", timeout: 20_000 });
  await page.waitForFunction(() => window.__safiIntroTrace?.motionStartedAt !== null, undefined, {
    timeout: 10_000
  });

  const handoff = await page.evaluate(() => {
    const trace = window.__safiIntroTrace;
    return {
      bootFrame: trace?.bootFrame ?? null,
      reactFrame: trace?.reactFrame ?? null,
      reactMountedAt: trace?.reactMountedAt ?? null,
      motionStartedAt: trace?.motionStartedAt ?? null
    };
  });
  assert.ok(handoff.bootFrame, "The static opening frame was not captured.");
  assert.ok(handoff.reactFrame, "The first React opening frame was not captured.");
  assert.ok(handoff.bootFrame.orb && handoff.reactFrame.orb, "Both frames must contain the logo.");
  assert.ok(
    Math.abs(handoff.bootFrame.orb.x - handoff.reactFrame.orb.x) <= 0.1 &&
      Math.abs(handoff.bootFrame.orb.y - handoff.reactFrame.orb.y) <= 0.1,
    `The logo jumped during handoff: ${JSON.stringify(handoff)}`
  );
  for (const [name, frame] of [
    ["static", handoff.bootFrame],
    ["React", handoff.reactFrame]
  ]) {
    assert.equal(frame.visibleDataDots, 9, `The ${name} frame must show all nine data points.`);
    assert.equal(frame.visibleBars, 0, `The ${name} frame must keep the bars hidden initially.`);
    assert.equal(frame.ringAnimation, "none", `The ${name} frame must keep the ring still.`);
  }
  assert.equal(handoff.bootFrame.progressScale, 0, "The static frame must start at zero progress.");
  assert.ok(
    handoff.motionStartedAt - handoff.reactMountedAt < 950,
    `Motion must start without the old one-second pause; measured ${handoff.motionStartedAt - handoff.reactMountedAt}ms.`
  );

  await page.waitForFunction(
    () => document.querySelector("#root .safi-intro-splash")?.dataset.fontsReady === "true",
    undefined,
    { timeout: 30_000 }
  );
  const openingFrame = await page.evaluate(() => {
    const root = document.querySelector("#root .safi-intro-splash");
    const title = root?.querySelector(".safi-intro-title");
    const copy = root?.querySelector(".safi-intro-copy");
    const brand = title?.querySelector("bdi");
    return {
      brandTitle: brand?.textContent?.trim() ?? "",
      brandDirection: brand?.getAttribute("dir") ?? "",
      copyVisibility: copy ? getComputedStyle(copy).visibility : null
    };
  });
  assert.equal(openingFrame.brandTitle, "SafiData");
  assert.equal(openingFrame.brandDirection, "ltr");
  assert.equal(openingFrame.copyVisibility, "visible");
  await page.screenshot({ path: resolve(artifactDir, "intro-opening-motion.png"), fullPage: true });

  try {
    await page.waitForFunction(
      () => {
        const root = document.querySelector("#root .safi-intro-splash");
        const fill = root?.querySelector(".safi-intro-progress-fill");
        const track = root?.querySelector(".safi-intro-progress-track");
        if (!fill || !track) return false;

        const trackRect = track.getBoundingClientRect();
        const fillRect = fill.getBoundingClientRect();
        const ratio = trackRect.width > 0 ? fillRect.width / trackRect.width : 0;
        const scaleX = new DOMMatrixReadOnly(getComputedStyle(fill).transform).a;
        const rightEdgeGapPx = Math.max(0, trackRect.right - fillRect.right);
        if (scaleX < 0.99999 || rightEdgeGapPx > 0.1) return false;

        const trace = window.__safiIntroTrace;
        if (trace && trace.progressFullAt === null) {
          trace.progressFullAt = performance.now();
          trace.progressWidthRatio = ratio;
          trace.progressScaleX = scaleX;
          trace.progressRightEdgeGapPx = rightEdgeGapPx;
          trace.applicationMountedAtProgressFull = Boolean(document.querySelector(".safi-shell"));
        }
        return true;
      },
      undefined,
      { timeout: 10_000 }
    );
  } catch (error) {
    const state = await page.evaluate(() => ({
      trace: window.__safiIntroTrace,
      introPresent: Boolean(document.querySelector("#root .safi-intro-splash"))
    }));
    await page.screenshot({
      path: resolve(artifactDir, "intro-progress-timeout.png"),
      fullPage: true
    });
    await writeFile(
      resolve(artifactDir, "visual-debug.json"),
      JSON.stringify(state, null, 2) + "\n"
    );
    throw new Error(
      `The intro progress bar did not reach 100%: ${JSON.stringify(state)}; ${String(error)}`
    );
  }

  const completedFrame = await page.evaluate(() => ({
    progressWidthRatio: window.__safiIntroTrace?.progressWidthRatio ?? 0,
    progressScaleX: window.__safiIntroTrace?.progressScaleX ?? 0,
    progressRightEdgeGapPx:
      window.__safiIntroTrace?.progressRightEdgeGapPx ?? Number.POSITIVE_INFINITY,
    applicationMountedAtProgressFull:
      window.__safiIntroTrace?.applicationMountedAtProgressFull ?? null
  }));
  assert.ok(completedFrame.progressScaleX >= 0.99999);
  assert.ok(completedFrame.progressRightEdgeGapPx <= 0.1);
  assert.ok(completedFrame.progressWidthRatio >= 0.9999);
  assert.equal(
    completedFrame.applicationMountedAtProgressFull,
    false,
    "The application must not mount before the entrance timeline completes."
  );
  await page.screenshot({
    path: resolve(artifactDir, "intro-progress-100-percent.png"),
    fullPage: true
  });

  await page.waitForFunction(
    () => document.querySelector("#root .safi-intro-splash")?.dataset.exiting === "true",
    undefined,
    { timeout: 30_000 }
  );
  await page.waitForFunction(
    () => {
      const root = document.querySelector("#root .safi-intro-splash");
      return root !== null && Number(getComputedStyle(root).opacity) < 0.95;
    },
    undefined,
    { timeout: 5_000 }
  );

  const siteHandoff = await page.evaluate(() => {
    const root = document.querySelector("#root .safi-intro-splash");
    return {
      applicationMounted: Boolean(document.querySelector(".safi-shell")),
      introOpacity: root ? Number(getComputedStyle(root).opacity) : null,
      exitStartedAt: window.__safiIntroTrace?.exitStartedAt ?? null
    };
  });
  assert.equal(
    siteHandoff.applicationMounted,
    true,
    "The site must be mounted under the exiting intro."
  );
  assert.ok(siteHandoff.introOpacity > 0 && siteHandoff.introOpacity < 0.95);
  await page.screenshot({ path: resolve(artifactDir, "intro-site-handoff.png"), fullPage: true });

  await page.waitForSelector("#root .safi-intro-splash", { state: "detached", timeout: 30_000 });
  await page.evaluate(() => {
    const trace = window.__safiIntroTrace;
    if (trace) trace.introRemovedAt = performance.now();
  });

  const finalTrace = await page.evaluate(() => ({ ...window.__safiIntroTrace }));
  const fullProgressHoldMs = finalTrace.introRemovedAt - finalTrace.progressFullAt;
  const exitDurationMs = finalTrace.introRemovedAt - finalTrace.exitStartedAt;
  assert.ok(fullProgressHoldMs >= 180, `Full progress was not held: ${fullProgressHoldMs}ms.`);
  assert.ok(exitDurationMs >= 500, `The animated site handoff was too short: ${exitDurationMs}ms.`);
  assert.deepEqual(pageErrors, [], `Unexpected browser runtime errors: ${pageErrors.join("; ")}`);
  await context.close();

  const reducedContext = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    reducedMotion: "reduce"
  });
  const reducedPage = await reducedContext.newPage();
  await reducedPage.goto(baseUrl, { waitUntil: "domcontentloaded" });
  await reducedPage.waitForSelector(".safi-shell", { state: "visible", timeout: 30_000 });
  await reducedPage.waitForSelector("#root .safi-intro-splash", {
    state: "detached",
    timeout: 5_000
  });
  await reducedContext.close();

  const report = {
    viewport: "1440x900",
    cacheDisabled: true,
    networkProfile: "3G throttled (150ms latency, 200KB/s download)",
    cpuThrottleRate: 4,
    handoff,
    motionStartDelayMs: handoff.motionStartedAt - handoff.reactMountedAt,
    progressWidthRatio: completedFrame.progressWidthRatio,
    progressScaleX: completedFrame.progressScaleX,
    progressRightEdgeGapPx: completedFrame.progressRightEdgeGapPx,
    applicationMountedAtProgressFull: completedFrame.applicationMountedAtProgressFull,
    siteHandoff,
    fullProgressHoldMs,
    exitDurationMs,
    reducedMotionPassed: true,
    pageErrors
  };
  await writeFile(
    resolve(artifactDir, "visual-check.json"),
    JSON.stringify(report, null, 2) + "\n"
  );
  console.log("Intro visual verification passed:", JSON.stringify(report));
} finally {
  if (browser) await browser.close();
  server.kill("SIGTERM");
}
