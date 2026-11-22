import assert from "node:assert/strict";
import { mkdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
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
  throw new Error(
    "PLAYWRIGHT_MODULE_URL is not set and no pinned Playwright installation was found."
  );
}

const { chromium } = await loadPlaywright();

const evidenceDir = resolve(process.env.SAFI_UX_EVIDENCE_DIR ?? join(tmpdir(), "safi-ux-e2e"));
await mkdir(evidenceDir, { recursive: true });
const saveScreenshots = Boolean(process.env.SAFI_UX_EVIDENCE_DIR);

const baseUrl = "http://127.0.0.1:4178";
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
    "4178",
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
      // Server not ready yet.
    }
    await delay(250);
  }
  throw new Error("Vite preview did not bind port 4178 within 30 seconds.");
}

async function mockAuthRoute(page) {
  await page.route("**/v1/auth/login", async (route) => {
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        token: "ux_e2e_local_session_token",
        session: {
          id: "sess_ux_e2e",
          workspace_id: "ws_ux_e2e",
          device_name: "Chromium",
          expires_at: 9999999999,
          created_at: 1,
          last_seen_at: 1
        }
      })
    });
  });
}

async function signIn(page) {
  await page.waitForSelector(".safi-auth-modal", { state: "visible", timeout: 15_000 });
  await page.fill('input[name="email"]', "tester@safidata.dev");
  await page.fill('input[name="password"]', "VerifyPass123!");
  await page.click('.safi-auth-modal button[type="submit"]');
  await page.waitForSelector(".safi-shell", { state: "visible", timeout: 15_000 });
}

async function shot(page, name) {
  if (!saveScreenshots) return;
  await page.screenshot({ path: join(evidenceDir, name) });
}

function parseMatrixScaleAndY(transform) {
  if (!transform || transform === "none") {
    return { scaleX: 1, scaleY: 1, translateY: 0 };
  }
  const match = transform.match(/^matrix\(([^)]+)\)$/);
  if (!match) {
    return { scaleX: 1, scaleY: 1, translateY: 0 };
  }
  const parts = match[1].split(",").map((v) => Number(v.trim()));
  return {
    scaleX: parts[0] ?? 1,
    scaleY: parts[3] ?? 1,
    translateY: parts[5] ?? 0
  };
}

function assertEntranceTrace(raw, pageErrors, { reducedMotion }) {
  assert.deepEqual(pageErrors, []);
  assert.equal(raw.prematureAuthWhileIntroPresent, false, "AuthModal must never mount while intro is still present.");
  assert.equal(raw.authMountCount, 1, `Expected exactly 1 AuthModal mount, got ${raw.authMountCount}.`);
  assert.equal(raw.authUnmountCount, 0, `Expected 0 AuthModal unmounts, got ${raw.authUnmountCount}.`);
  assert.notEqual(raw.introDetachedAt, null, "Expected React intro splash detachment timestamp.");
  assert.ok(
    raw.authMountedAt >= raw.introDetachedAt - 2,
    `AuthModal mounted (${raw.authMountedAt}) before intro detached (${raw.introDetachedAt}).`
  );

  const samples = raw.modalOpacitySamples;
  assert.ok(samples.length >= 2, "Expected opacity telemetry samples for AuthModal.");

  let entranceCycles = 0;
  let inLowState = samples[0].modalOp < 0.99;
  if (reducedMotion) {
    assert.equal(samples[0].modalOp, 1, `Reduced-motion AuthModal must start directly at opacity 1, got ${samples[0].modalOp}`);
  } else {
    assert.ok(
      samples[0].modalOp < 0.85,
      `First painted frame of AuthModal must start inside the entrance curve (< 0.85 opacity) instead of flashing 1.0; got ${samples[0].modalOp}`
    );
    entranceCycles = 1;
  }

  let peakSoFar = samples[0].modalOp;
  let droppedAfterPeak = false;
  let minBackdropOp = 1;
  let backdropTransformed = false;
  let minChildOp = 1;
  let childTransformed = false;
  let modalScaledOrBounced = false;

  for (let i = 0; i < samples.length; i += 1) {
    const s = samples[i];
    const op = s.modalOp;
    if (i > 0) {
      if (peakSoFar >= 0.85 && op < peakSoFar - 0.15) {
        droppedAfterPeak = true;
      }
      if (inLowState && op >= 0.99) {
        inLowState = false;
      } else if (!inLowState && op < 0.5) {
        inLowState = true;
        entranceCycles += 1;
      }
      if (op > peakSoFar) peakSoFar = op;
    }

    if (s.backdropOp !== null && s.backdropOp < minBackdropOp) {
      minBackdropOp = s.backdropOp;
    }
    if (s.backdropTf && s.backdropTf !== "none") {
      backdropTransformed = true;
    }

    for (const childOp of [s.providerOp, s.fieldOp, s.submitOp]) {
      if (childOp !== null && childOp < minChildOp) {
        minChildOp = childOp;
      }
    }
    for (const childTf of [s.providerTf, s.fieldTf, s.submitTf]) {
      if (childTf && childTf !== "none") {
        childTransformed = true;
      }
    }

    const { scaleX, scaleY, translateY } = parseMatrixScaleAndY(s.modalTf);
    if (Math.abs(scaleX - 1) > 0.001 || Math.abs(scaleY - 1) > 0.001 || translateY < -0.5 || translateY > 10.5) {
      modalScaledOrBounced = true;
    }
  }

  assert.equal(droppedAfterPeak, false, `AuthModal opacity dropped after rising: ${JSON.stringify(samples.slice(0, 10))}`);
  assert.equal(minBackdropOp, 1, `Backdrop must remain calm and static at opacity 1 without independent fade; got min ${minBackdropOp}`);
  assert.equal(backdropTransformed, false, "Backdrop must never transform during entrance.");
  assert.equal(minChildOp, 1, `Inner AuthModal elements must not stagger opacity during initial entrance; got min ${minChildOp}`);
  assert.equal(childTransformed, false, "Inner AuthModal elements must not stagger transform during initial entrance.");
  assert.equal(modalScaledOrBounced, false, "AuthModal must only translate vertically (0..10px) without zoom/scale or bounce.");

  if (!reducedMotion) {
    assert.equal(entranceCycles, 1, `Expected 1 continuous entrance animation cycle, got ${entranceCycles}`);
  } else {
    assert.equal(entranceCycles, 0, `Expected 0 entrance animation dips in reduced-motion, got ${entranceCycles}`);
  }
  assert.equal(samples[samples.length - 1].modalOp, 1, "AuthModal must settle at opacity 1.");

  return {
    dir: raw.dir,
    introMountedAt: raw.introMountedAt,
    introMotionCompletedAt: raw.introMotionCompletedAt,
    introExitingAt: raw.introExitingAt,
    introDetachedAt: raw.introDetachedAt,
    authMountedAt: raw.authMountedAt,
    authMountCount: raw.authMountCount,
    authUnmountCount: raw.authUnmountCount,
    firstFrameOpacity: samples[0].modalOp,
    firstFrameTransform: samples[0].modalTf,
    lastFrameOpacity: samples[samples.length - 1].modalOp,
    lastFrameTransform: samples[samples.length - 1].modalTf,
    minBackdropOp,
    minChildOp,
    modalScaledOrBounced,
    sampleCount: samples.length,
    entranceCycles,
    droppedAfterPeak
  };
}

async function runSingleEntranceProbe(context, url, { reducedMotion = false, testTabSwitch = false, testReload = false, captureStoryboard = false } = {}) {
  const page = await context.newPage();
  const pageErrors = [];
  page.on("pageerror", (err) => pageErrors.push(err.message));

  await page.addInitScript(() => {
    window.__authEntranceTrace = {
      introMountedAt: null,
      introMotionCompletedAt: null,
      introExitingAt: null,
      introDetachedAt: null,
      authMountedAt: null,
      authMountCount: 0,
      authUnmountCount: 0,
      modalOpacitySamples: [],
      prematureAuthWhileIntroPresent: false
    };

    const sampleState = () => {
      const t = Math.round(performance.now() * 10) / 10;
      const intro = document.querySelector("#root .safi-intro-splash");
      if (intro && intro.dataset.motionComplete === "true" && window.__authEntranceTrace.introMotionCompletedAt === null) {
        window.__authEntranceTrace.introMotionCompletedAt = t;
      }
      const modal = document.querySelector(".safi-auth-modal");
      const backdrop = document.querySelector(".safi-auth-backdrop");
      const provider = document.querySelector(".safi-auth-provider");
      const field = document.querySelector(".safi-auth-field");
      const submit = document.querySelector(".safi-auth-submit");
      if (modal) {
        const mCs = getComputedStyle(modal);
        const bCs = backdrop ? getComputedStyle(backdrop) : null;
        const pCs = provider ? getComputedStyle(provider) : null;
        const fCs = field ? getComputedStyle(field) : null;
        const sCs = submit ? getComputedStyle(submit) : null;
        window.__authEntranceTrace.modalOpacitySamples.push({
          t,
          modalOp: Number(mCs.opacity),
          modalTf: mCs.transform,
          backdropOp: bCs ? Number(bCs.opacity) : null,
          backdropTf: bCs?.transform ?? null,
          providerOp: pCs ? Number(pCs.opacity) : null,
          providerTf: pCs?.transform ?? null,
          fieldOp: fCs ? Number(fCs.opacity) : null,
          fieldTf: fCs?.transform ?? null,
          submitOp: sCs ? Number(sCs.opacity) : null,
          submitTf: sCs?.transform ?? null
        });
      }
      if (window.__authEntranceTrace.authMountedAt === null || performance.now() - window.__authEntranceTrace.authMountedAt < 900) {
        requestAnimationFrame(sampleState);
      }
    };
    requestAnimationFrame(sampleState);

    const obs = new MutationObserver((mutations) => {
      const trace = window.__authEntranceTrace;
      const t = Math.round(performance.now() * 10) / 10;
      for (const m of mutations) {
        for (const node of m.addedNodes) {
          if (!(node instanceof Element)) continue;
          const intro = node.matches?.("#root .safi-intro-splash")
            ? node
            : node.querySelector?.("#root .safi-intro-splash");
          if (intro && trace.introMountedAt === null) {
            trace.introMountedAt = t;
          }
          const modal = node.matches?.(".safi-auth-modal")
            ? node
            : node.querySelector?.(".safi-auth-modal");
          if (modal) {
            trace.authMountCount += 1;
            if (trace.authMountedAt === null) trace.authMountedAt = t;
            if (document.querySelector("#safi-boot-screen, #root .safi-intro-splash")) {
              trace.prematureAuthWhileIntroPresent = true;
            }
          }
        }
        for (const node of m.removedNodes) {
          if (!(node instanceof Element)) continue;
          const isReactIntroNode =
            (node.matches?.(".safi-intro-splash") &&
              node.id !== "safi-boot-screen" &&
              m.target instanceof Element &&
              m.target.id === "root") ||
            Boolean(node.querySelector?.(".safi-intro-splash:not(#safi-boot-screen)"));
          if (isReactIntroNode) {
            if (trace.introDetachedAt === null) trace.introDetachedAt = t;
          }
          if (node.matches?.(".safi-auth-modal") || node.querySelector?.(".safi-auth-modal")) {
            trace.authUnmountCount += 1;
          }
        }
        if (m.type === "attributes" && m.target instanceof Element && m.target.matches("#root .safi-intro-splash")) {
          if (m.target.getAttribute("data-exiting") === "true" && trace.introExitingAt === null) {
            trace.introExitingAt = t;
          }
        }
      }
    });
    obs.observe(document, { childList: true, subtree: true, attributes: true, attributeFilter: ["data-exiting"] });
  });

  await page.goto(url, { waitUntil: "domcontentloaded" });
  if (captureStoryboard && saveScreenshots) {
    await page.waitForSelector("#root .safi-intro-splash", { state: "visible", timeout: 20_000 });
    await delay(180);
    await shot(page, "00a-intro-splash.png");
    await page.waitForFunction(
      () => document.querySelector("#root .safi-intro-splash")?.dataset.exiting === "true",
      undefined,
      { timeout: 15_000 }
    );
    await delay(180);
    await shot(page, "00b-intro-exiting.png");
  }

  await page.waitForSelector(".safi-auth-modal", { state: "visible", timeout: 20_000 });
  if (captureStoryboard && saveScreenshots) {
    await shot(page, "00c-auth-entrance-in-progress.png");
  }
  await delay(1000);
  if (captureStoryboard && saveScreenshots) {
    await shot(page, "00d-auth-settled-single.png");
  }

  if (testTabSwitch) {
    await page.click('.safi-auth-tabs [role="tab"]:nth-child(2)');
    await delay(420);
    await page.click('.safi-auth-tabs [role="tab"]:nth-child(1)');
    await delay(420);
  }

  const rawFirst = await page.evaluate(() => ({
    ...window.__authEntranceTrace,
    dir: document.documentElement.dir || "ltr"
  }));
  const firstResult = assertEntranceTrace(rawFirst, pageErrors, { reducedMotion });

  let reloadResult = null;
  if (testReload) {
    await page.reload({ waitUntil: "domcontentloaded" });
    await page.waitForSelector(".safi-auth-modal", { state: "visible", timeout: 20_000 });
    await delay(1000);
    const rawReload = await page.evaluate(() => ({
      ...window.__authEntranceTrace,
      dir: document.documentElement.dir || "ltr"
    }));
    reloadResult = assertEntranceTrace(rawReload, pageErrors, { reducedMotion });
  }

  await page.close();
  return testReload ? { initial: firstResult, reload: reloadResult } : firstResult;
}

const summary = {};
let browser;

try {
  await waitForServer();
  browser = await chromium.launch({ headless: true, args: ["--no-sandbox"] });

  // =========================================================================
  // 0. Single Auth Entrance Lifecycle Verification (EN, EN Reload, AR RTL, Reduced Motion, Tauri Desktop)
  // =========================================================================
  {
    const enCtx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
    const { initial: enFirst, reload: enReload } = await runSingleEntranceProbe(enCtx, baseUrl, {
      testTabSwitch: true,
      testReload: true
    });
    assert.equal(enFirst.dir, "ltr");
    assert.equal(enReload.dir, "ltr");
    if (saveScreenshots) {
      await runSingleEntranceProbe(enCtx, baseUrl, { captureStoryboard: true });
    }
    await enCtx.close();

    const arCtx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
    await arCtx.addInitScript(() => {
      localStorage.setItem("safi.locale", "ar");
    });
    const arFirst = await runSingleEntranceProbe(arCtx, baseUrl, { testTabSwitch: true });
    assert.equal(arFirst.dir, "rtl");
    if (saveScreenshots) {
      const arShotPage = await arCtx.newPage();
      await arShotPage.goto(baseUrl, { waitUntil: "domcontentloaded" });
      await arShotPage.waitForSelector(".safi-auth-modal", { state: "visible", timeout: 20_000 });
      await delay(600);
      await shot(arShotPage, "00e-auth-settled-ar-rtl.png");
      await arShotPage.close();
    }
    await arCtx.close();

    const rmSingleCtx = await browser.newContext({
      viewport: { width: 1440, height: 900 },
      reducedMotion: "reduce"
    });
    const rmSingle = await runSingleEntranceProbe(rmSingleCtx, baseUrl, {
      reducedMotion: true,
      testTabSwitch: true
    });
    await rmSingleCtx.close();

    const tauriCtx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
    await tauriCtx.addInitScript(() => {
      window.__TAURI_INTERNALS__ = {
        invoke: async () => null
      };
    });
    const tauriDesktop = await runSingleEntranceProbe(tauriCtx, baseUrl, { testTabSwitch: true });
    await tauriCtx.close();

    summary.singleAuthEntrance = { enFirst, enReload, arFirst, rmSingle, tauriDesktop };
  }

  // =========================================================================
  // 1. Auth Scrollbar Separation, Password Ghost Security, ARIA Keyboard Tabs, Motion & Light/Dark
  // =========================================================================
  {
    const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
    const page = await ctx.newPage();
    const errors = [];
    page.on("pageerror", (err) => errors.push(err.message));

    await page.goto(baseUrl, { waitUntil: "domcontentloaded" });
    await page.waitForSelector("#root .safi-intro-splash", { state: "detached", timeout: 20_000 });
    await page.waitForSelector(".safi-auth-modal", { state: "visible", timeout: 10_000 });
    await delay(450);

    // Main page arrow scrollbar must stay unmounted while Auth modal owns scrolling
    assert.equal(await page.locator('[data-safi-custom-scrollbar="true"]').count(), 0);
    await shot(page, "01-auth-signin-light-1440x900.png");

    // Verify ARIA tablist semantics and keyboard navigation in LTR
    const ariaCheck = await page.evaluate(() => {
      const tablist = document.querySelector('.safi-auth-tabs[role="tablist"]');
      const tabs = Array.from(document.querySelectorAll('.safi-auth-tabs [role="tab"]'));
      const panel = document.querySelector('.safi-auth-form-stage[role="tabpanel"]');
      return {
        orientation: tablist?.getAttribute("aria-orientation"),
        tab0Controls: tabs[0]?.getAttribute("aria-controls"),
        tab1Controls: tabs[1]?.getAttribute("aria-controls"),
        panelId: panel?.id,
        panelLabelledBy: panel?.getAttribute("aria-labelledby"),
        tab0Id: tabs[0]?.id,
        tab0TabIndex: tabs[0]?.getAttribute("tabindex"),
        tab1TabIndex: tabs[1]?.getAttribute("tabindex")
      };
    });
    assert.equal(ariaCheck.orientation, "horizontal");
    assert.ok(ariaCheck.panelId);
    assert.equal(ariaCheck.tab0Controls, ariaCheck.panelId);
    assert.equal(ariaCheck.tab1Controls, ariaCheck.panelId);
    assert.equal(ariaCheck.panelLabelledBy, ariaCheck.tab0Id);
    assert.equal(ariaCheck.tab0TabIndex, "0");
    assert.equal(ariaCheck.tab1TabIndex, "-1");

    // Fill email & visible password (showPassword = true!) then install DOM spy BEFORE switching tabs
    const secretPassword = "PlaintextGhostTrap!42";
    await page.fill('input[name="email"]', "security@safidata.dev");
    await page.fill('input[name="password"]', secretPassword);
    await page.click(".safi-auth-eye");
    assert.equal(await page.getAttribute('input[name="password"]', "type"), "text");

    await page.evaluate((secret) => {
      window.__ghostSpy = {
        sawGhostElement: false,
        leakedPasswordInGhost: false,
        nonPasswordTypeInGhost: false
      };
      const stage = document.querySelector(".safi-auth-form-stage");
      const observer = new MutationObserver((mutations) => {
        for (const mutation of mutations) {
          for (const node of mutation.addedNodes) {
            if (node instanceof HTMLElement && node.classList.contains("safi-auth-form-exit-ghost")) {
              window.__ghostSpy.sawGhostElement = true;
              const inputs = node.querySelectorAll("input");
              for (const input of inputs) {
                if (
                  input.value === secret ||
                  input.defaultValue === secret ||
                  input.getAttribute("value") === secret
                ) {
                  window.__ghostSpy.leakedPasswordInGhost = true;
                }
                if (input.type === "text" && input.value === "") {
                  // Only non-secret email input may remain text; make sure none look unmasked for password
                }
                const maybePasswordIdx = input.getAttribute("autocomplete") ?? "";
                if (maybePasswordIdx.includes("password") && input.type !== "password") {
                  window.__ghostSpy.nonPasswordTypeInGhost = true;
                }
              }
            }
          }
        }
      });
      if (stage) observer.observe(stage, { childList: true });
    }, secretPassword);

    // Keyboard ArrowRight switches Sign In -> Sign Up and moves focus to Sign Up tab!
    await page.locator('.safi-auth-tabs [role="tab"]').first().focus();
    await page.keyboard.press("ArrowRight");
    await delay(460);

    const postKeyboardForward = await page.evaluate(() => {
      const tabs = document.querySelectorAll('.safi-auth-tabs [role="tab"]');
      const ind = document.querySelector(".safi-auth-tabs-indicator").getBoundingClientRect();
      const tab1Rect = tabs[1].getBoundingClientRect();
      const m = document.querySelector(".safi-auth-modal");
      return {
        activeElementIsTab1: document.activeElement === tabs[1],
        tab1Selected: tabs[1].getAttribute("aria-selected"),
        panelLabelledBy: document.querySelector('[role="tabpanel"]')?.getAttribute("aria-labelledby"),
        tab1Id: tabs[1].id,
        emailPreserved: document.querySelector('input[name="email"]')?.value,
        passwordPreserved: document.querySelector('input[name="password"]')?.value,
        ghostSpy: window.__ghostSpy,
        overflowsAt1440: m.scrollHeight > m.clientHeight + 1,
        gliderAligned: Math.abs(ind.left - tab1Rect.left) <= 3
      };
    });
    assert.equal(postKeyboardForward.activeElementIsTab1, true);
    assert.equal(postKeyboardForward.tab1Selected, "true");
    assert.equal(postKeyboardForward.panelLabelledBy, postKeyboardForward.tab1Id);
    assert.equal(postKeyboardForward.emailPreserved, "security@safidata.dev");
    assert.equal(postKeyboardForward.passwordPreserved, secretPassword);
    assert.equal(postKeyboardForward.ghostSpy.sawGhostElement, true);
    assert.equal(postKeyboardForward.ghostSpy.leakedPasswordInGhost, false);
    assert.equal(postKeyboardForward.ghostSpy.nonPasswordTypeInGhost, false);
    assert.equal(postKeyboardForward.overflowsAt1440, false);
    assert.equal(postKeyboardForward.gliderAligned, true);

    await page.fill('input[name="display_name"]', "Alex Rabea");
    await shot(page, "02-auth-signup-light-1440x900.png");

    // Keyboard ArrowLeft, End & Home from focused tablist
    await page.locator('.safi-auth-tabs [role="tab"]:nth-child(2)').focus();
    await page.keyboard.press("ArrowLeft");
    await delay(450);
    assert.equal(await page.getAttribute('.safi-auth-tabs [role="tab"]:nth-child(1)', "aria-selected"), "true");
    await page.keyboard.press("End");
    await delay(450);
    assert.equal(await page.getAttribute('.safi-auth-tabs [role="tab"]:nth-child(2)', "aria-selected"), "true");
    assert.equal(await page.inputValue('input[name="display_name"]'), "Alex Rabea");
    await page.keyboard.press("Home");
    await delay(450);
    assert.equal(await page.getAttribute('.safi-auth-tabs [role="tab"]:nth-child(1)', "aria-selected"), "true");
    await page.keyboard.press("End");
    await delay(450);

    // Verify slim bespoke scrollbar on short viewport 1366x520
    await page.setViewportSize({ width: 1366, height: 520 });
    await delay(200);
    assert.equal(await page.locator('[data-safi-custom-scrollbar="true"]').count(), 0);
    const box = await page.locator(".safi-auth-modal").boundingBox();
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.wheel(0, 240);
    await delay(220);
    const shortModalY = await page.evaluate(() => ({
      scrollTop: document.querySelector(".safi-auth-modal").scrollTop,
      scrollbarWidth: getComputedStyle(document.querySelector(".safi-auth-modal")).scrollbarWidth,
      windowY: window.scrollY
    }));
    assert.ok(shortModalY.scrollTop > 0);
    assert.equal(shortModalY.scrollbarWidth, "thin");
    assert.equal(shortModalY.windowY, 0);
    await shot(page, "03-auth-slim-scrollbar-1366x520.png");

    assert.deepEqual(errors, []);
    summary.authAndSecurity = { ariaCheck, postKeyboardForward, shortModalY };
    await ctx.close();
  }

  // =========================================================================
  // 2. Desktop Viewports (1920x1080, 1440x900, 1366x768) Themes, Persistence, Main Scrollbar, RTL/LTR
  // =========================================================================
  for (const vp of [
    { name: "1920x1080", width: 1920, height: 1080 },
    { name: "1440x900", width: 1440, height: 900 },
    { name: "1366x768", width: 1366, height: 768 }
  ]) {
    const ctx = await browser.newContext({ viewport: { width: vp.width, height: vp.height }, deviceScaleFactor: 1 });
    const page = await ctx.newPage();
    const errors = [];
    page.on("pageerror", (err) => errors.push(err.message));
    await mockAuthRoute(page);

    await page.goto(baseUrl, { waitUntil: "domcontentloaded" });
    await page.waitForSelector("#root .safi-intro-splash", { state: "detached", timeout: 20_000 });
    assert.equal(await page.getAttribute("html", "data-theme"), "google-gemini");
    await signIn(page);
    await delay(450);

    // Main Light Mode UI check
    assert.equal(await page.textContent('[data-theme-option="vision-glass"]'), "Light Mode");
    assert.equal(await page.textContent('[data-theme-option="google-gemini"]'), "Dark Mode");
    assert.equal(
      await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth),
      true
    );
    await shot(page, `04-main-light-${vp.name}.png`);

    // Main Scrollbar Arrows + Thumb Drag + Keyboard + Wheel
    const upArrow = page.locator('[data-safi-custom-scrollbar="true"] [data-scrollbar-arrow="up"]');
    const downArrow = page.locator('[data-safi-custom-scrollbar="true"] [data-scrollbar-arrow="down"]');
    const thumb = page.locator('[data-safi-custom-scrollbar="true"] [role="scrollbar"]');
    await downArrow.waitFor({ state: "visible", timeout: 5_000 });
    assert.equal(await upArrow.isDisabled(), true);
    assert.equal(await downArrow.isDisabled(), false);

    await downArrow.click();
    await delay(350);
    const yAfterDown = await page.evaluate(() => window.scrollY);
    assert.ok(yAfterDown > 0);
    assert.equal(await upArrow.isDisabled(), false);

    const tBox = await thumb.boundingBox();
    await page.mouse.move(tBox.x + tBox.width / 2, tBox.y + tBox.height / 2);
    await page.mouse.down();
    await page.mouse.move(tBox.x + tBox.width / 2, tBox.y + tBox.height / 2 + 240, { steps: 5 });
    await page.mouse.up();
    await delay(250);
    const yAfterDrag = await page.evaluate(() => window.scrollY);
    assert.ok(yAfterDrag > yAfterDown);

    await thumb.focus();
    await page.keyboard.press("End");
    await page.waitForFunction(
      () =>
        document
          .querySelector('[data-safi-custom-scrollbar="true"] [data-scrollbar-arrow="down"]')
          ?.hasAttribute("disabled") === true,
      undefined,
      { timeout: 3_000 }
    );
    assert.equal(await downArrow.isDisabled(), true);
    await page.keyboard.press("Home");
    await page.waitForFunction(
      () =>
        document
          .querySelector('[data-safi-custom-scrollbar="true"] [data-scrollbar-arrow="up"]')
          ?.hasAttribute("disabled") === true,
      undefined,
      { timeout: 3_000 }
    );
    assert.equal(await upArrow.isDisabled(), true);

    // Dark Mode Toggle + Reload Persistence
    await page.click('[data-theme-option="google-gemini"]');
    await delay(300);
    assert.equal(await page.getAttribute("html", "data-theme"), "google-gemini");
    assert.equal(await page.evaluate(() => localStorage.getItem("safi.theme")), "google-gemini");
    if (vp.name === "1440x900" || vp.name === "1920x1080") {
      await shot(page, `05-main-dark-${vp.name}.png`);
    }

    if (vp.name === "1440x900") {
      await page.reload({ waitUntil: "domcontentloaded" });
      await page.waitForSelector("#root .safi-intro-splash", { state: "detached", timeout: 20_000 });
      assert.equal(await page.getAttribute("html", "data-theme"), "google-gemini");
      await signIn(page);
      assert.equal(await page.getAttribute("html", "data-theme"), "google-gemini");
    }

    // Arabic RTL test on 1366x768
    if (vp.name === "1366x768") {
      await page.click('.safi-topbar-actions button:has-text("العربية")');
      await delay(350);
      assert.equal(await page.getAttribute("html", "dir"), "rtl");
      assert.equal(await page.textContent('[data-theme-option="vision-glass"]'), "الوضع الفاتح");
      assert.equal(await page.textContent('[data-theme-option="google-gemini"]'), "الوضع الداكن");
      const rtlSb = await page
        .locator('[data-safi-custom-scrollbar="true"]')
        .evaluate((el) => el.getBoundingClientRect().toJSON());
      assert.ok(rtlSb.left < 50);
      await downArrow.click();
      await delay(350);
      await shot(page, "06-main-arabic-rtl-dark-1366x768.png");
    }

    assert.deepEqual(errors, []);
    summary[vp.name] = { yAfterDown, yAfterDrag };
    await ctx.close();
  }

  // =========================================================================
  // 3. Arabic RTL Auth Arrow Navigation + Legacy apple-light + Mobile + Reduced Motion
  // =========================================================================
  {
    const rtlCtx = await browser.newContext({ viewport: { width: 1366, height: 768 } });
    const rtlPage = await rtlCtx.newPage();
    await rtlPage.addInitScript(() => {
      localStorage.setItem("safi.locale", "ar");
    });
    await rtlPage.goto(baseUrl, { waitUntil: "domcontentloaded" });
    await rtlPage.waitForSelector("#root .safi-intro-splash", { state: "detached", timeout: 20_000 });
    await rtlPage.waitForSelector(".safi-auth-modal", { state: "visible", timeout: 10_000 });
    await delay(350);

    // In RTL, ArrowLeft from Sign In (right side) moves left to Sign Up; ArrowRight moves back right
    await rtlPage.locator('.safi-auth-tabs [role="tab"]').first().focus();
    await rtlPage.keyboard.press("ArrowLeft");
    await delay(420);
    assert.equal(
      await rtlPage.getAttribute('.safi-auth-tabs [role="tab"]:nth-child(2)', "aria-selected"),
      "true"
    );
    await shot(rtlPage, "07-auth-arabic-rtl-signup-1366x768.png");
    await rtlPage.keyboard.press("ArrowRight");
    await delay(420);
    assert.equal(
      await rtlPage.getAttribute('.safi-auth-tabs [role="tab"]:nth-child(1)', "aria-selected"),
      "true"
    );
    await rtlCtx.close();
  }

  {
    const mobileCtx = await browser.newContext({
      viewport: { width: 390, height: 844 },
      hasTouch: true,
      isMobile: true
    });
    const mPage = await mobileCtx.newPage();
    await mockAuthRoute(mPage);
    await mPage.goto(baseUrl, { waitUntil: "domcontentloaded" });
    await mPage.waitForSelector("#root .safi-intro-splash", { state: "detached", timeout: 20_000 });
    await mPage.waitForSelector(".safi-auth-modal", { state: "visible", timeout: 10_000 });
    await delay(350);
    await mPage.locator('.safi-auth-tabs [role="tab"]:nth-child(2)').tap();
    await delay(420);
    await shot(mPage, "08-auth-mobile-390x844.png");
    await mPage.locator('.safi-auth-tabs [role="tab"]:nth-child(1)').tap();
    await delay(400);
    await mPage.fill('input[name="email"]', "mobile@safidata.dev");
    await mPage.fill('input[name="password"]', "MobilePass123!");
    await mPage.locator('.safi-auth-modal button[type="submit"]').tap();
    await mPage.waitForSelector(".safi-shell", { state: "visible", timeout: 15_000 });
    await mPage.locator('[data-safi-custom-scrollbar="true"] [data-scrollbar-arrow="down"]').tap();
    await delay(350);
    const mobileY = await mPage.evaluate(() => ({
      scrollY: window.scrollY,
      overflowX: document.documentElement.scrollWidth > window.innerWidth
    }));
    assert.ok(mobileY.scrollY > 0);
    assert.equal(mobileY.overflowX, false);
    await shot(mPage, "09-main-mobile-390x844.png");
    summary.mobile = mobileY;
    await mobileCtx.close();
  }

  {
    const legacyCtx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
    const lPage = await legacyCtx.newPage();
    await mockAuthRoute(lPage);
    await lPage.addInitScript(() => {
      if (!window.__safiSeededLegacy) {
        localStorage.setItem("safi.theme", "apple-light");
        window.__safiSeededLegacy = true;
      }
    });
    await lPage.goto(baseUrl, { waitUntil: "domcontentloaded" });
    await lPage.waitForSelector("#root .safi-intro-splash", { state: "detached", timeout: 20_000 });
    assert.equal(await lPage.getAttribute("html", "data-theme"), "apple-light");
    await signIn(lPage);
    assert.equal(await lPage.getAttribute("html", "data-theme"), "apple-light");
    assert.equal(await lPage.evaluate(() => localStorage.getItem("safi.theme")), "apple-light");
    await lPage.locator('button[aria-label="Settings"]').dispatchEvent("click");
    await delay(200);
    const settingsCopy = await lPage.textContent(".safi-settings");
    assert.ok(settingsCopy && settingsCopy.includes("Classic Light"));
    await lPage.click('.safi-settings button:has-text("Dark Mode")');
    await delay(200);
    assert.equal(await lPage.getAttribute("html", "data-theme"), "google-gemini");
    summary.legacyAppleLightPreserved = true;
    await legacyCtx.close();
  }

  {
    const rmCtx = await browser.newContext({ viewport: { width: 1440, height: 900 }, reducedMotion: "reduce" });
    const rmPage = await rmCtx.newPage();
    await rmPage.goto(baseUrl, { waitUntil: "domcontentloaded" });
    await rmPage.waitForSelector(".safi-auth-modal", { state: "visible", timeout: 15_000 });
    await rmPage.waitForSelector("#root .safi-intro-splash", { state: "detached", timeout: 10_000 });
    await rmPage.click('.safi-auth-tabs [role="tab"]:nth-child(2)');
    await rmPage.waitForSelector('.safi-auth-tabs [role="tab"]:nth-child(2)[aria-selected="true"]', {
      timeout: 5_000
    });
    await delay(100);
    const rmAligned = await rmPage.evaluate(() => {
      const ind = document.querySelector(".safi-auth-tabs-indicator").getBoundingClientRect();
      const t1 = document.querySelectorAll('.safi-auth-tabs [role="tab"]')[1].getBoundingClientRect();
      return Math.abs(ind.left - t1.left) <= 3;
    });
    assert.equal(rmAligned, true);
    summary.reducedMotionAligned = rmAligned;
    await rmCtx.close();
  }

  // =========================================================================
  // 4. Seven-Viewport AuthModal Fit, Internal Scrollbar, Tab Alignment & Compact RTL Check
  // =========================================================================
  {
    const sevenViewports = [
      { name: "1920x1080", width: 1920, height: 1080, dir: "ltr" },
      { name: "1440x900", width: 1440, height: 900, dir: "ltr" },
      { name: "1366x768", width: 1366, height: 768, dir: "ltr" },
      { name: "1366x520", width: 1366, height: 520, dir: "ltr" },
      { name: "390x844", width: 390, height: 844, dir: "ltr" },
      { name: "360x800", width: 360, height: 800, dir: "ltr" },
      { name: "320x700", width: 320, height: 700, dir: "rtl" }
    ];
    const viewportMatrix = {};
    for (const vp of sevenViewports) {
      const vpCtx = await browser.newContext({
        viewport: { width: vp.width, height: vp.height },
        deviceScaleFactor: 1,
        hasTouch: vp.width < 500,
        isMobile: vp.width < 500
      });
      const vpPage = await vpCtx.newPage();
      if (vp.dir === "rtl") {
        await vpPage.addInitScript(() => localStorage.setItem("safi.locale", "ar"));
      }
      await vpPage.goto(baseUrl, { waitUntil: "domcontentloaded" });
      await vpPage.waitForSelector("#root .safi-intro-splash", { state: "detached", timeout: 20_000 });
      await vpPage.waitForSelector(".safi-auth-modal", { state: "visible", timeout: 10_000 });
      await delay(300);

      const inspectAuthFit = async () =>
        vpPage.evaluate(() => {
          const modal = document.querySelector(".safi-auth-modal");
          const rect = modal.getBoundingClientRect();
          const ind = document.querySelector(".safi-auth-tabs-indicator").getBoundingClientRect();
          const activeTab = document.querySelector('.safi-auth-tabs [role="tab"][aria-selected="true"]').getBoundingClientRect();
          return {
            modalLeft: rect.left,
            modalRight: rect.right,
            modalTop: rect.top,
            modalBottom: rect.bottom,
            modalScrollWidth: modal.scrollWidth,
            modalClientWidth: modal.clientWidth,
            modalScrollHeight: modal.scrollHeight,
            modalClientHeight: modal.clientHeight,
            docScrollWidth: document.documentElement.scrollWidth,
            innerWidth: window.innerWidth,
            innerHeight: window.innerHeight,
            tabAlignedDelta: Math.abs(ind.left - activeTab.left),
            canScrollInternally: modal.scrollHeight > modal.clientHeight + 2
          };
        });

      const signInMetrics = await inspectAuthFit();
      assert.ok(signInMetrics.modalLeft >= -1 && signInMetrics.modalRight <= vp.width + 1, `${vp.name} SignIn horizontal clip: ${JSON.stringify(signInMetrics)}`);
      assert.ok(signInMetrics.modalTop >= -1 && signInMetrics.modalBottom <= vp.height + 1, `${vp.name} SignIn vertical clip: ${JSON.stringify(signInMetrics)}`);
      assert.ok(signInMetrics.modalScrollWidth <= signInMetrics.modalClientWidth + 1, `${vp.name} SignIn internal horizontal overflow`);
      assert.ok(signInMetrics.docScrollWidth <= signInMetrics.innerWidth + 1, `${vp.name} SignIn document horizontal overflow`);
      assert.ok(signInMetrics.tabAlignedDelta <= 3, `${vp.name} SignIn indicator misaligned by ${signInMetrics.tabAlignedDelta}px`);

      await vpPage.locator('.safi-auth-tabs [role="tab"]:nth-child(2)').click();
      await delay(400);
      const signUpMetrics = await inspectAuthFit();
      assert.ok(signUpMetrics.modalLeft >= -1 && signUpMetrics.modalRight <= vp.width + 1, `${vp.name} SignUp horizontal clip`);
      assert.ok(signUpMetrics.modalTop >= -1 && signUpMetrics.modalBottom <= vp.height + 1, `${vp.name} SignUp vertical clip`);
      assert.ok(signUpMetrics.modalScrollWidth <= signUpMetrics.modalClientWidth + 1, `${vp.name} SignUp internal horizontal overflow`);
      assert.ok(signUpMetrics.tabAlignedDelta <= 3, `${vp.name} SignUp indicator misaligned by ${signUpMetrics.tabAlignedDelta}px`);

      let scrolledInternally = false;
      if (signUpMetrics.canScrollInternally) {
        const sc = await vpPage.evaluate(() => {
          const m = document.querySelector(".safi-auth-modal");
          m.scrollTop = 120;
          return { scrollTop: m.scrollTop, windowY: window.scrollY };
        });
        assert.ok(sc.scrollTop > 0, `${vp.name} SignUp modal failed to scroll internally`);
        assert.equal(sc.windowY, 0, `${vp.name} window scrolled behind modal`);
        scrolledInternally = true;
      }

      viewportMatrix[vp.name] = { dir: vp.dir, signInMetrics, signUpMetrics, scrolledInternally };
      await vpCtx.close();
    }
    summary.sevenViewportAuthMatrix = viewportMatrix;
  }

  // =========================================================================
  // 5. Public Website Preservation Check (No Desktop-Only Studio Surfaces on Web)
  // =========================================================================
  {
    const webCtx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
    const webPage = await webCtx.newPage();
    await mockAuthRoute(webPage);
    await webPage.goto(baseUrl, { waitUntil: "domcontentloaded" });
    await signIn(webPage);
    await delay(250);
    const leakedDesktopSelectors = await webPage.evaluate(() => ({
      isDesktopFlag: typeof window.__TAURI_INTERNALS__ !== "undefined",
      desktopTriggerCount: document.querySelectorAll('button[data-desktop-studio-trigger="true"]').length,
      desktopStudioCount: document.querySelectorAll(".safi-desktop-studio").length,
      workbookDrawerCount: document.querySelectorAll('[data-testid="workbook-intelligence-drawer"]').length,
      intelligencePanelCount: document.querySelectorAll('[data-testid="intelligence-assistant-panel"]').length,
      exportModalCount: document.querySelectorAll('[data-testid="deliverable-export-modal"]').length,
      tutorialDialogCount: document.querySelectorAll('[data-testid="interactive-tutorial-dialog"]').length,
      hasPublicShell: document.querySelector(".safi-shell") !== null
    }));
    assert.equal(leakedDesktopSelectors.hasPublicShell, true);
    assert.equal(leakedDesktopSelectors.isDesktopFlag, false);
    assert.equal(leakedDesktopSelectors.desktopTriggerCount, 0);
    assert.equal(leakedDesktopSelectors.desktopStudioCount, 0);
    assert.equal(leakedDesktopSelectors.workbookDrawerCount, 0);
    assert.equal(leakedDesktopSelectors.intelligencePanelCount, 0);
    assert.equal(leakedDesktopSelectors.exportModalCount, 0);
    assert.equal(leakedDesktopSelectors.tutorialDialogCount, 0);
    summary.websitePreservation = leakedDesktopSelectors;
    await webCtx.close();
  }

  await writeFile(join(evidenceDir, "ux-e2e-summary.json"), JSON.stringify(summary, null, 2));
  console.log("VERIFY_UX_E2E_PASSED", JSON.stringify(summary));
} finally {
  if (browser) await browser.close();
  server.kill("SIGTERM");
}
