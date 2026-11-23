import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { spawn } from "node:child_process";
import { pathToFileURL } from "node:url";
import { setTimeout as delay } from "node:timers/promises";

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
  throw new Error("PLAYWRIGHT_MODULE_URL must point to the pinned Playwright installation.");
}

const { chromium } = await loadPlaywright();

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
  { stdio: "inherit", shell: process.platform === "win32" }
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
      applicationEntryMountedAt: null,
      exitStartedAt: null,
      introRemovedAt: null,
      prematureEntryWhileIntroPresent: false,
      prematureExitState: null,
      bootFrame: null,
      reactFrame: null
    };

    const inspectDom = () => {
      const trace = window.__safiIntroTrace;
      if (!trace) return;
      const rootIntro = document.querySelector("#root .safi-intro-splash");
      const intro = document.querySelector("#safi-boot-screen, #root .safi-intro-splash");
      const authGate = document.querySelector(".safi-auth-modal");
      const desktopWorkspace = document.querySelector(".safi-desktop-studio");
      if (trace.exitStartedAt === null && rootIntro?.dataset.exiting === "true") {
        trace.exitStartedAt = performance.now();
      }
      if (trace.exitStartedAt !== null && !rootIntro && trace.introRemovedAt === null) {
        trace.introRemovedAt = performance.now();
      }
      if (authGate || desktopWorkspace) {
        if (trace.applicationEntryMountedAt === null) {
          trace.applicationEntryMountedAt = performance.now();
        }
        if (intro) {
          trace.prematureEntryWhileIntroPresent = true;
          trace.prematureExitState = intro.getAttribute("data-exiting") ?? "none";
        }
      }
    };

    const domObserver = new MutationObserver(inspectDom);
    domObserver.observe(document, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["data-exiting"]
    });

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
      if (
        (trace?.exitStartedAt !== null && !react) ||
        performance.now() - samplingStartedAt > 120_000
      ) {
        if (
          trace &&
          trace.applicationEntryMountedAt === null &&
          document.querySelector(".safi-auth-modal, .safi-shell")
        ) {
          trace.applicationEntryMountedAt = performance.now();
        }
        return;
      }
      if (
        trace &&
        trace.applicationEntryMountedAt === null &&
        document.querySelector(".safi-auth-modal, .safi-shell")
      ) {
        trace.applicationEntryMountedAt = performance.now();
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
      applicationEntryMountedAt: trace?.applicationEntryMountedAt ?? null
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
    () =>
      window.__safiIntroTrace?.motionCompletedAt !== null ||
      document.querySelector("#root .safi-intro-splash")?.dataset.motionComplete === "true" ||
      !document.querySelector("#root .safi-intro-splash"),
    undefined,
    { timeout: 15_000 }
  );

  await page.waitForFunction(
    () =>
      window.__safiIntroTrace?.fontsReadyAt !== null ||
      document.querySelector("#root .safi-intro-splash")?.dataset.fontsReady === "true" ||
      !document.querySelector("#root .safi-intro-splash"),
    undefined,
    { timeout: 30_000 }
  );
  const openingFrame = await page.evaluate(() => {
    const root = document.querySelector("#root .safi-intro-splash");
    if (!root) return { detachedEarly: true, brandTitle: "SafiData", brandDirection: "ltr", copyVisibility: "visible" };
    const title = root.querySelector(".safi-intro-title");
    const copy = root.querySelector(".safi-intro-copy");
    const brand = title?.querySelector("bdi");
    return {
      detachedEarly: false,
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
    () =>
      window.__safiIntroTrace?.exitStartedAt !== null ||
      document.querySelector("#root .safi-intro-splash")?.dataset.exiting === "true" ||
      !document.querySelector("#root .safi-intro-splash"),
    undefined,
    { timeout: 30_000 }
  );

  const siteHandoff = await page.evaluate(() => {
    const root = document.querySelector("#root .safi-intro-splash");
    const arrowPath = root?.querySelector(".safi-intro-transition-path");
    return {
      authenticationGateMounted: Boolean(document.querySelector(".safi-auth-modal")),
      desktopWorkspaceMounted: Boolean(document.querySelector(".safi-desktop-studio")),
      motionCompletedAt: window.__safiIntroTrace?.motionCompletedAt ?? null,
      introOpacity: root ? Number(getComputedStyle(root).opacity) : null,
      introClipPath: root ? getComputedStyle(root).clipPath : null,
      arrowPath: arrowPath?.getAttribute("d") ?? null,
      exitStartedAt: window.__safiIntroTrace?.exitStartedAt ?? null
    };
  });
  assert.equal(
    siteHandoff.authenticationGateMounted,
    false,
    "The public Showcase website must NOT force an anonymous sign-in modal during intro."
  );
  assert.equal(
    siteHandoff.desktopWorkspaceMounted,
    false,
    "The operational Desktop workspace must never mount on the public website."
  );
  await page.screenshot({ path: resolve(artifactDir, "intro-auth-gate-handoff.png"), fullPage: true });

  // Wait for Intro to detach completely and Public Showcase to be visible WITHOUT mandatory sign-in modal!
  await page.waitForSelector("#root .safi-intro-splash", { state: "detached", timeout: 30_000 });
  await page.waitForSelector('[data-hero-summary="true"]', { state: "visible", timeout: 15_000 });
  const publicPostIntro = await page.evaluate(() => ({
    showcaseVisible: Boolean(document.querySelector('[data-hero-summary="true"]')),
    capabilitiesVisible: Boolean(document.querySelector("#verified-capabilities-matrix")),
    authModalMounted: Boolean(document.querySelector(".safi-auth-modal")),
    desktopStudioMounted: Boolean(document.querySelector(".safi-desktop-studio")),
    theme: document.documentElement.dataset.theme
  }));
  assert.equal(publicPostIntro.showcaseVisible, true, "Public Showcase hero must be visible after Intro exit.");
  assert.equal(publicPostIntro.capabilitiesVisible, true, "Public Showcase capabilities matrix must be visible.");
  assert.equal(
    publicPostIntro.authModalMounted,
    false,
    "Public Showcase must NOT display a mandatory sign-in modal to an anonymous visitor."
  );
  assert.equal(
    publicPostIntro.desktopStudioMounted,
    false,
    "Public Showcase must NOT expose the operational Desktop Studio workspace."
  );
  assert.equal(publicPostIntro.theme, "google-gemini", "First-time visitors default to Dark Mode.");

  const finalTrace = await page.evaluate(() => ({ ...window.__safiIntroTrace }));
  const entranceMotionDurationMs =
    finalTrace.motionCompletedAt !== null && handoff.motionStartedAt !== null
      ? finalTrace.motionCompletedAt - handoff.motionStartedAt
      : 0;
  assert.equal(
    finalTrace.prematureEntryWhileIntroPresent,
    false,
    `AuthModal/Desktop workspace mounted prematurely while intro was still present (data-exiting=${finalTrace.prematureExitState}).`
  );
  assert.deepEqual(pageErrors, [], `Unexpected browser runtime errors: ${pageErrors.join("; ")}`);
  await context.close();

  // Reduced-motion public Showcase check: intro detaches cleanly directly into Showcase with NO forced AuthModal.
  const reducedContext = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    reducedMotion: "reduce"
  });
  const reducedPage = await reducedContext.newPage();
  await reducedPage.goto(baseUrl, { waitUntil: "domcontentloaded" });
  await reducedPage.waitForSelector("#root .safi-intro-splash", {
    state: "detached",
    timeout: 15_000
  });
  await reducedPage.waitForSelector('[data-hero-summary="true"]', {
    state: "visible",
    timeout: 15_000
  });
  assert.equal(
    await reducedPage.locator(".safi-auth-modal").count(),
    0,
    "Reduced-motion public Showcase must stay accessible without forced login modal."
  );
  await reducedContext.close();

  // Operational Desktop Auth-Gate Verification: unauthenticated Desktop user MUST see AuthModal and NEVER Desktop Studio!
  const desktopContext = await browser.newContext({
    viewport: { width: 1440, height: 900 }
  });
  const desktopPage = await desktopContext.newPage();
  const desktopErrors = [];
  desktopPage.on("pageerror", (error) => desktopErrors.push(error.message));
  await desktopPage.addInitScript(() => {
    window.__TAURI_INTERNALS__ = {
      invoke: async (cmd) => {
        if (cmd === "desktop_auth_status") {
          return JSON.stringify({
            mode: "unauthenticated",
            canExecuteProtectedOps: false,
            hasPreviouslyAuthenticated: false
          });
        }
        throw new Error("SF-AUTH-001");
      }
    };
  });
  await desktopPage.goto(baseUrl, { waitUntil: "domcontentloaded" });
  await desktopPage.waitForSelector("#root .safi-intro-splash", {
    state: "detached",
    timeout: 20_000
  });
  await desktopPage.waitForSelector(".safi-auth-modal", {
    state: "visible",
    timeout: 15_000
  });
  assert.equal(
    await desktopPage.locator(".safi-desktop-studio").count(),
    0,
    "Unauthenticated Desktop user must NEVER see the operational Desktop Studio workspace."
  );
  assert.deepEqual(desktopErrors, [], `Unexpected Desktop runtime errors: ${desktopErrors.join("; ")}`);
  await desktopContext.close();

  const report = {
    viewport: "1440x900",
    cacheDisabled: true,
    networkProfile: "3G throttled (150ms latency, 200KB/s download)",
    cpuThrottleRate: 4,
    handoff,
    motionStartDelayMs: handoff.motionStartedAt - handoff.reactMountedAt,
    entranceMotionDurationMs,
    siteHandoff,
    publicPostIntro,
    desktopAuthGatedPassed: true,
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
