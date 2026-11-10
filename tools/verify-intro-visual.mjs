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
      motionCompletedAt: null,
      fontsReadyAt: null,
      applicationMountedAt: null,
      exitStartedAt: null,
      introRemovedAt: null,
      bootFrame: null,
      reactFrame: null
    };

    const readFrame = (root) => {
      const orb = root.querySelector(".safi-intro-orb")?.getBoundingClientRect();
      const ring = root.querySelector(".safi-intro-orb-ring");
      const dots = Array.from(root.querySelectorAll("[data-intro-dot]"));
      const bars = Array.from(root.querySelectorAll("[data-intro-bar]"));
      return {
        orb: orb ? { x: orb.x, y: orb.y, width: orb.width, height: orb.height } : null,
        ringAnimation: ring ? getComputedStyle(ring).animationName : null,
        orbitAnimation: root.querySelector(".safi-pm-logo-orbit")
          ? getComputedStyle(root.querySelector(".safi-pm-logo-orbit")).animationName
          : null,
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
        if (trace.motionCompletedAt === null && react.dataset.motionComplete === "true") {
          trace.motionCompletedAt = performance.now();
        }
        if (trace.exitStartedAt === null && react.dataset.exiting === "true") {
          trace.exitStartedAt = performance.now();
        }
      }
      if (trace && trace.applicationMountedAt === null && document.querySelector(".safi-shell")) {
        trace.applicationMountedAt = performance.now();
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
      motionStartedAt: trace?.motionStartedAt ?? null,
      motionCompletedAt: trace?.motionCompletedAt ?? null,
      applicationMountedAt: trace?.applicationMountedAt ?? null
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
    assert.equal(
      frame.ringAnimation,
      "safiIntroBootHalo",
      `The ${name} frame must start the halo motion immediately.`
    );
    assert.equal(
      frame.orbitAnimation,
      "safiIntroOrbit",
      `The ${name} frame must start the orbit motion immediately.`
    );
  }
  assert.ok(
    handoff.motionStartedAt - handoff.reactMountedAt < 100,
    `React intro motion must start during the first paint; measured ${handoff.motionStartedAt - handoff.reactMountedAt}ms.`
  );
  await page.waitForFunction(
    () => document.querySelector("#root .safi-intro-splash")?.dataset.motionComplete === "true",
    undefined,
    { timeout: 5_000 }
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

  await page.waitForFunction(
    () => document.querySelector("#root .safi-intro-splash")?.dataset.exiting === "true",
    undefined,
    { timeout: 30_000 }
  );
  await page.waitForFunction(
    () => {
      const root = document.querySelector("#root .safi-intro-splash");
      const arrowPath = root?.querySelector(".safi-intro-transition-path");
      if (!root || !arrowPath || !arrowPath.getAttribute("d")) return false;

      const pathLength = Number.parseFloat(arrowPath.style.strokeDasharray);
      const pathOffset = Number.parseFloat(arrowPath.style.strokeDashoffset);
      const clipPath = getComputedStyle(root).clipPath;
      const radius = Number(clipPath.match(/circle\(([\d.]+)px at/)?.[1] ?? NaN);
      const initialRadius = Math.hypot(window.innerWidth, window.innerHeight) + 24;
      return (
        Number.isFinite(pathLength) &&
        pathLength > 0 &&
        pathOffset < pathLength &&
        Number.isFinite(radius) &&
        radius < initialRadius - 4
      );
    },
    undefined,
    { timeout: 5_000 }
  );

  const siteHandoff = await page.evaluate(() => {
    const root = document.querySelector("#root .safi-intro-splash");
    const arrowPath = root?.querySelector(".safi-intro-transition-path");
    return {
      applicationMounted: Boolean(document.querySelector(".safi-shell")),
      motionCompletedAt: window.__safiIntroTrace?.motionCompletedAt ?? null,
      introOpacity: root ? Number(getComputedStyle(root).opacity) : null,
      introClipPath: root ? getComputedStyle(root).clipPath : null,
      arrowPath: arrowPath?.getAttribute("d") ?? null,
      arrowDasharray: arrowPath ? Number.parseFloat(arrowPath.style.strokeDasharray) : null,
      arrowDashoffset: arrowPath ? Number.parseFloat(arrowPath.style.strokeDashoffset) : null,
      exitStartedAt: window.__safiIntroTrace?.exitStartedAt ?? null
    };
  });
  assert.equal(
    siteHandoff.applicationMounted,
    true,
    "The site must be mounted under the exiting intro."
  );
  assert.ok(siteHandoff.introOpacity > 0.95);
  assert.match(siteHandoff.introClipPath ?? "", /^circle\(/);
  assert.ok(siteHandoff.arrowPath);
  assert.ok(siteHandoff.arrowDasharray > 0);
  assert.ok(siteHandoff.arrowDashoffset < siteHandoff.arrowDasharray);
  await page.screenshot({ path: resolve(artifactDir, "intro-site-handoff.png"), fullPage: true });

  await page.waitForSelector("#root .safi-intro-splash", { state: "detached", timeout: 30_000 });
  await page.evaluate(() => {
    const trace = window.__safiIntroTrace;
    if (trace) trace.introRemovedAt = performance.now();
  });

  const finalTrace = await page.evaluate(() => ({ ...window.__safiIntroTrace }));
  const exitDurationMs = finalTrace.introRemovedAt - finalTrace.exitStartedAt;
  const entranceMotionDurationMs = finalTrace.motionCompletedAt - handoff.motionStartedAt;
  assert.ok(finalTrace.applicationMountedAt !== null, "The application was not loaded under the intro.");
  assert.ok(
    entranceMotionDurationMs <= 1_000,
    `The entrance motion felt delayed: ${entranceMotionDurationMs}ms.`
  );
  assert.ok(exitDurationMs >= 350, `The animated site handoff was too short: ${exitDurationMs}ms.`);
  // This test deliberately throttles Chromium CPU by 4x. Measure the full
  // wall-clock handoff with enough headroom for throttled frame scheduling;
  // the product timeline itself remains under one second at normal speed.
  assert.ok(
    exitDurationMs <= 3_000,
    `The handoff exceeded the 4x-CPU test budget: ${exitDurationMs}ms.`
  );
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
    entranceMotionDurationMs,
    applicationMountedAt: finalTrace.applicationMountedAt,
    siteHandoff,
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
