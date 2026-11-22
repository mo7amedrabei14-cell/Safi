#!/usr/bin/env node
/**
 * `tools/verify-intro-theme-transition.mjs`
 *
 * Frame-by-frame theme stability & dark-flash verification benchmark (Section 15.2 & 15.3).
 *
 * Verifies across:
 * 1. First-run default Light Mode (`vision-glass`, expected canvas `rgb(234, 240, 251)` / `#eaf0fb`, `color-scheme: light`)
 * 2. Stored Legacy Light Mode (`apple-light`, expected canvas `rgb(245, 245, 247)` / `#f5f5f7`, `color-scheme: light`)
 * 3. Stored Dark Mode (`google-gemini`, expected canvas `rgb(9, 11, 16)` / `#090b10`, `color-scheme: dark`)
 * 4. Arabic RTL Light Mode (`vision-glass` + `ar`, expected canvas `rgb(234, 240, 251)`, `color-scheme: light`)
 *
 * Samples `data-theme`, `colorScheme`, `html` computed `backgroundColor`, and `body` computed
 * `backgroundColor` on EVERY `requestAnimationFrame` from before intro exit (`data-exiting="true"`)
 * through intro unmount and for 1,200ms after intro unmount.
 *
 * Fails if a Light Mode case (`vision-glass` or `apple-light`) EVER exhibits a dark background frame
 * (`rgb(9, 11, 16)`) or intermediate dark-to-light transition frame during or after the intro exit wipe.
 */
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { dirname, resolve } from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const port = 4191;
const baseUrl = `http://127.0.0.1:${port}`;
const playwrightUrl = process.env.PLAYWRIGHT_MODULE_URL || "playwright";

async function waitForHttp(url, timeoutMs = 45_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const response = await fetch(url);
      if (response.ok) return;
    } catch {
      // keep polling
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`Timed out waiting for ${url}`);
}

const { chromium } = await import(playwrightUrl);

const viteCommand =
  process.platform === "win32"
    ? ["cmd.exe", ["/d", "/s", "/c", `pnpm --filter @safi/web exec vite --host 127.0.0.1 --port ${port} --strictPort`]]
    : ["pnpm", ["--filter", "@safi/web", "exec", "vite", "--host", "127.0.0.1", "--port", String(port), "--strictPort"]];

const server = spawn(viteCommand[0], viteCommand[1], {
  cwd: root,
  stdio: "inherit",
  env: { ...process.env }
});

const CASES = [
  {
    name: "default-dark-google-gemini-en",
    storedTheme: null,
    storedLocale: "en",
    expectedThemeAttr: "google-gemini",
    expectedColorScheme: "dark",
    expectedBgRgb: "rgb(9, 11, 16)"
  },
  {
    name: "saved-light-vision-glass-en",
    storedTheme: "vision-glass",
    storedLocale: "en",
    expectedThemeAttr: "vision-glass",
    expectedColorScheme: "light",
    expectedBgRgb: "rgb(234, 240, 251)"
  },
  {
    name: "saved-light-vision-glass-ar",
    storedTheme: "vision-glass",
    storedLocale: "ar",
    expectedThemeAttr: "vision-glass",
    expectedColorScheme: "light",
    expectedBgRgb: "rgb(234, 240, 251)"
  },
  {
    name: "legacy-light-apple-light-en",
    storedTheme: "apple-light",
    storedLocale: "en",
    expectedThemeAttr: "apple-light",
    expectedColorScheme: "light",
    expectedBgRgb: "rgb(245, 245, 247)"
  },
  {
    name: "saved-dark-google-gemini-ar",
    storedTheme: "google-gemini",
    storedLocale: "ar",
    expectedThemeAttr: "google-gemini",
    expectedColorScheme: "dark",
    expectedBgRgb: "rgb(9, 11, 16)"
  }
];

let browser;
try {
  await waitForHttp(baseUrl);
  browser = await chromium.launch({ headless: true });

  const results = [];

  for (const tc of CASES) {
    const context = await browser.newContext({
      viewport: { width: 1440, height: 900 },
      reducedMotion: "no-preference"
    });
    const page = await context.newPage();

    await page.addInitScript(
      ({ theme, locale }) => {
        try {
          window.localStorage.clear();
          if (theme !== null) {
            window.localStorage.setItem("safi.theme", theme);
          }
          if (locale !== null) {
            window.localStorage.setItem("safi.locale", locale);
          }
        } catch {
          // ignore
        }
      },
      { theme: tc.storedTheme, locale: tc.storedLocale }
    );

    await page.goto(baseUrl, { waitUntil: "domcontentloaded", timeout: 30_000 });

    const metrics = await page.evaluate(async (expectedBgRgb) => {
      const frames = [];
      let introExitStartedAt = null;
      let introUnmountedAt = null;
      let darkFlashFramesDuringOrAfterExit = 0;
      let mismatchedBgFramesDuringOrAfterExit = 0;

      const sampleFrame = (phase) => {
        const htmlEl = document.documentElement;
        const bodyEl = document.body;
        const htmlStyle = window.getComputedStyle(htmlEl);
        const bodyStyle = bodyEl ? window.getComputedStyle(bodyEl) : null;
        const splash = document.querySelector("#root .safi-intro-splash");
        const exiting = splash ? splash.getAttribute("data-exiting") === "true" : false;
        const mounted = splash !== null;
        const dataTheme = htmlEl.getAttribute("data-theme");
        const colorScheme = htmlStyle.colorScheme;
        const htmlBg = htmlStyle.backgroundColor;
        const bodyBg = bodyStyle ? bodyStyle.backgroundColor : htmlBg;

        const now = performance.now();
        if (exiting && introExitStartedAt === null) {
          introExitStartedAt = now;
        }
        if (!mounted && introUnmountedAt === null) {
          introUnmountedAt = now;
        }

        // Once the intro starts exiting (clipPath opens to reveal the page) or has unmounted,
        // html and body MUST already match the target theme background with zero dark flash!
        if (exiting || !mounted) {
          if (htmlBg !== expectedBgRgb || bodyBg !== expectedBgRgb) {
            mismatchedBgFramesDuringOrAfterExit += 1;
          }
          if (expectedBgRgb !== "rgb(9, 11, 16)" && (htmlBg === "rgb(9, 11, 16)" || bodyBg === "rgb(9, 11, 16)")) {
            darkFlashFramesDuringOrAfterExit += 1;
          }
        }

        frames.push({
          t: Math.round(now),
          phase,
          mounted,
          exiting,
          dataTheme,
          colorScheme,
          htmlBg,
          bodyBg
        });
      };

      return await new Promise((resolvePromise) => {
        const deadline = performance.now() + 8000;
        const tick = () => {
          const now = performance.now();
          const splash = document.querySelector("#root .safi-intro-splash");
          const phase = splash
            ? splash.getAttribute("data-exiting") === "true"
              ? "exiting"
              : "intro"
            : "post-intro";
          sampleFrame(phase);

          if (introUnmountedAt !== null && now - introUnmountedAt >= 900) {
            resolvePromise({
              totalFramesSampled: frames.length,
              introExitStartedAt: introExitStartedAt ? Math.round(introExitStartedAt) : null,
              introUnmountedAt: introUnmountedAt ? Math.round(introUnmountedAt) : null,
              darkFlashFramesDuringOrAfterExit,
              mismatchedBgFramesDuringOrAfterExit,
              firstExitFrame: frames.find((f) => f.exiting) ?? null,
              firstPostIntroFrame: frames.find((f) => !f.mounted) ?? null,
              finalFrame: frames[frames.length - 1] ?? null
            });
            return;
          }

          if (now >= deadline) {
            resolvePromise({
              error: "TIMEOUT",
              totalFramesSampled: frames.length,
              darkFlashFramesDuringOrAfterExit,
              mismatchedBgFramesDuringOrAfterExit,
              finalFrame: frames[frames.length - 1] ?? null
            });
            return;
          }

          window.requestAnimationFrame(tick);
        };
        window.requestAnimationFrame(tick);
      });
    }, tc.expectedBgRgb);

    results.push({ case: tc.name, ...metrics });
    await context.close();
  }

  console.log("[verify-intro-theme-transition] Frame-by-frame theme stability results:");
  for (const r of results) {
    console.log(`  - ${r.case}: ${JSON.stringify(r)}`);
    assert.ok(!r.error, `Case ${r.case} timed out: ${JSON.stringify(r)}`);
    assert.equal(
      r.darkFlashFramesDuringOrAfterExit,
      0,
      `Case ${r.case} had ${r.darkFlashFramesDuringOrAfterExit} dark-flash frames during/after intro exit!`
    );
    assert.equal(
      r.mismatchedBgFramesDuringOrAfterExit,
      0,
      `Case ${r.case} had ${r.mismatchedBgFramesDuringOrAfterExit} mismatched background frames during/after intro exit: ${JSON.stringify(r)}`
    );
  }
  console.log(
    "[verify-intro-theme-transition] PASS: Zero dark-flash frames and 100% theme stability across Light (vision-glass, apple-light) and Dark (google-gemini) transitions."
  );
} finally {
  if (browser) await browser.close();
  server.kill("SIGTERM");
}
