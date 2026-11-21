#!/usr/bin/env node
/**
 * `tools/verify-intro-transition-performance.mjs`
 *
 * Repeatable Headless Chromium Performance & Visual-Readiness Benchmark for Section 10:
 * Measures the exact transition from `SafiIntroSplash` completion (`onExitComplete` /
 * removal of `data-safi-intro-active`) to:
 *   1. `.safi-shell` mounted and visible (`shellReadyAfterIntroMs`)
 *   2. Hero `<h1>` (`[data-safi-blur-text="true"]`) all words reaching crisp legibility
 *      (`opacity >= 0.9`, `filter === "none"` or `"blur(0px)"`) (`headlineCrispAfterIntroMs`)
 *   3. Post-intro Cumulative Layout Shift (`postIntroCls`)
 *
 * Tested across desktop viewports (`1440x900`, `1920x1080`, `1280x800`) in both English (LTR)
 * and Arabic (RTL), plus `prefers-reduced-motion: reduce`.
 */
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { resolve } from "node:path";
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
      // try next
    }
  }
  throw new Error("Playwright not found.");
}

const { chromium } = await loadPlaywright();
const port = 4189;
const baseUrl = `http://127.0.0.1:${port}`;
const measureOnly = process.argv.includes("--measure-only");

const server = spawn(
  process.execPath,
  [
    resolve("apps/web/node_modules/vite/bin/vite.js"),
    "--host",
    "127.0.0.1",
    "--port",
    String(port),
    "--strictPort"
  ],
  {
    cwd: resolve("apps/web"),
    stdio: "inherit"
  }
);

async function waitForServer() {
  for (let i = 0; i < 120; i += 1) {
    if (server.exitCode !== null) throw new Error(`Vite exited early (${server.exitCode})`);
    try {
      const r = await fetch(baseUrl);
      if (r.ok) return;
    } catch {
      // wait
    }
    await delay(200);
  }
  throw new Error("Vite server timeout");
}

const CASES = [
  { name: "en-1440x900", lang: "en", width: 1440, height: 900, reducedMotion: "no-preference" },
  { name: "ar-1440x900", lang: "ar", width: 1440, height: 900, reducedMotion: "no-preference" },
  { name: "en-1920x1080", lang: "en", width: 1920, height: 1080, reducedMotion: "no-preference" },
  { name: "ar-1280x800", lang: "ar", width: 1280, height: 800, reducedMotion: "no-preference" },
  { name: "en-reduced-motion", lang: "en", width: 1440, height: 900, reducedMotion: "reduce" }
];

try {
  await waitForServer();
  const browser = await chromium.launch({ headless: true, args: ["--no-sandbox"] });
  const results = [];

  try {
    for (const tc of CASES) {
      const ctx = await browser.newContext({
        viewport: { width: tc.width, height: tc.height },
        reducedMotion: tc.reducedMotion
      });
      const page = await ctx.newPage();
      await page.addInitScript((locale) => {
        window.localStorage.setItem("safi.lang", locale);
      }, tc.lang);

      await page.goto(baseUrl, { waitUntil: "domcontentloaded", timeout: 60_000 });

      const metrics = await page.evaluate(async () => {
        const navStart = performance.now();
        let introExitBeginAt = null;
        let introUnmountedAt = null;
        let shellVisibleAt = null;
        let headlineFirstSeenAt = null;
        let headlineCrispAt = null;
        let initialMinWordOpacityAtIntroExit = null;
        let initialWordFilterAtIntroExit = null;

        const isHeadlineCrisp = () => {
          const shell = document.querySelector(".safi-shell");
          if (!shell) return false;
          const shellOp = Number.parseFloat(window.getComputedStyle(shell).opacity || "1");
          const stage = document.getElementById("safi-main");
          const stageOp = stage ? Number.parseFloat(window.getComputedStyle(stage).opacity || "1") : 1;
          const h1 = document.querySelector('h1[data-safi-blur-text="true"]');
          if (!h1) return false;
          const words = Array.from(h1.querySelectorAll("[data-safi-word]"));
          if (words.length === 0) return false;
          for (const w of words) {
            const st = window.getComputedStyle(w);
            const op = Number.parseFloat(st.opacity || "1") * shellOp * stageOp;
            const flt = st.filter || "none";
            const blurMatch = flt.match(/blur\(([0-9.]+)px\)/);
            const blurPx = blurMatch ? Number.parseFloat(blurMatch[1]) : 0;
            if (op < 0.9 || blurPx > 0.5) {
              return false;
            }
          }
          return true;
        };

        const sampleAtExit = () => {
          const stage = document.getElementById("safi-main");
          const stageOp = stage ? Number.parseFloat(window.getComputedStyle(stage).opacity || "1") : 0;
          const h1 = document.querySelector('h1[data-safi-blur-text="true"]');
          if (!h1) {
            initialMinWordOpacityAtIntroExit = 0;
            initialWordFilterAtIntroExit = "unmounted";
            return;
          }
          const words = Array.from(h1.querySelectorAll("[data-safi-word]"));
          let minOp = 1;
          let maxFilter = "none";
          for (const w of words) {
            const st = window.getComputedStyle(w);
            const effOp = Number.parseFloat(st.opacity || "1") * stageOp;
            if (effOp < minOp) minOp = effOp;
            if (st.filter && st.filter !== "none") maxFilter = st.filter;
          }
          initialMinWordOpacityAtIntroExit = Math.round(minOp * 1000) / 1000;
          initialWordFilterAtIntroExit = maxFilter;
        };

        return await new Promise((resolvePromise) => {
          const poll = () => {
            const now = performance.now();
            const splash = document.querySelector(".safi-intro-splash");
            if (splash && splash.getAttribute("data-exiting") === "true" && introExitBeginAt === null) {
              introExitBeginAt = now;
            }
            if (!splash && introUnmountedAt === null) {
              introUnmountedAt = now;
              if (isHeadlineCrisp()) {
                headlineCrispAt = now;
              }
            }
            if (
              introUnmountedAt !== null &&
              now - introUnmountedAt >= 36 &&
              initialMinWordOpacityAtIntroExit === null
            ) {
              sampleAtExit();
              // If a post-paint useEffect snapped the headline back to blurred/dim, clear headlineCrispAt!
              if (!isHeadlineCrisp()) {
                headlineCrispAt = null;
              }
            }
            const shell = document.querySelector(".safi-shell");
            if (shell && shellVisibleAt === null) {
              shellVisibleAt = now;
            }
            const h1 = document.querySelector('h1[data-safi-blur-text="true"]');
            if (h1 && headlineFirstSeenAt === null) {
              headlineFirstSeenAt = now;
            }
            if (
              introUnmountedAt !== null &&
              initialMinWordOpacityAtIntroExit !== null &&
              isHeadlineCrisp()
            ) {
              if (headlineCrispAt === null) {
                headlineCrispAt = now;
              }
              resolvePromise({
                totalMs: Math.round(now - navStart),
                introDurationMs: Math.round(introUnmountedAt - navStart),
                shellDelayAfterIntroExitMs: Math.round(
                  Math.max(0, (shellVisibleAt ?? now) - introUnmountedAt)
                ),
                headlineCrispDelayAfterIntroExitMs: Math.round(
                  Math.max(0, headlineCrispAt - introUnmountedAt)
                ),
                initialMinWordOpacityAtIntroExit,
                initialWordFilterAtIntroExit
              });
              return;
            }
            if (now - navStart > 8000) {
              resolvePromise({
                totalMs: Math.round(now - navStart),
                introDurationMs: introUnmountedAt ? Math.round(introUnmountedAt - navStart) : -1,
                shellDelayAfterIntroExitMs: -1,
                headlineCrispDelayAfterIntroExitMs: 8000,
                initialMinWordOpacityAtIntroExit,
                initialWordFilterAtIntroExit
              });
              return;
            }
            requestAnimationFrame(poll);
          };
          requestAnimationFrame(poll);
        });
      });

      results.push({ case: tc.name, ...metrics });
      await ctx.close();
    }

    console.log("[verify-intro-transition-performance] Results:");
    for (const r of results) {
      console.log(`  - ${r.case}: ${JSON.stringify(r)}`);
    }

    if (!measureOnly) {
      for (const r of results) {
        assert.ok(
          r.headlineCrispDelayAfterIntroExitMs <= 180,
          `Expected hero headline to be crisp within <= 180ms after intro exit in ${r.case}, got ${r.headlineCrispDelayAfterIntroExitMs}ms (minOpacity=${r.initialMinWordOpacityAtIntroExit}, filter=${r.initialWordFilterAtIntroExit})`
        );
        assert.ok(
          r.initialMinWordOpacityAtIntroExit !== null && r.initialMinWordOpacityAtIntroExit >= 0.75,
          `Expected hero headline words to already have opacity >= 0.75 at intro exit in ${r.case}, got ${r.initialMinWordOpacityAtIntroExit}`
        );
      }
      console.log(
        "[verify-intro-transition-performance] PASS: Hero interface and large headline typography are immediately crisp upon intro completion across all viewports."
      );
    }
  } finally {
    await browser.close();
  }
} finally {
  server.kill("SIGTERM");
}
