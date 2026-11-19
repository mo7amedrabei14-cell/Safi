#!/usr/bin/env node
/**
 * tools/verify-runtime-boundary-e2e.mjs — Real Chromium Playwright E2E Proof for Phase 01 & Phase 04/05:
 * 1. Public browser Showcase opens freely without mandatory login and NEVER exposes Desktop Studio or Owner UI.
 * 2. Desktop runtime mandates login before showing workspace, hides Owner UI for normal users, and exposes
 *    the Google Authenticator TOTP MFA gate ONLY when the backend confirms `is_owner: true`.
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
      // Try next candidate.
    }
  }
  throw new Error("Playwright not found.");
}

const { chromium } = await loadPlaywright();
const baseUrl = "http://127.0.0.1:4181";
const server = spawn(
  process.platform === "win32" ? "pnpm.cmd" : "pnpm",
  ["--filter", "@safi/web", "exec", "vite", "--host", "127.0.0.1", "--port", "4181", "--strictPort"],
  { stdio: "inherit", shell: process.platform === "win32" }
);

async function waitForServer() {
  for (let i = 0; i < 120; i += 1) {
    if (server.exitCode !== null) throw new Error(`Vite preview exited early (${server.exitCode})`);
    try {
      const r = await fetch(baseUrl);
      if (r.ok) return;
    } catch {
      // Waiting...
    }
    await delay(250);
  }
  throw new Error("Vite preview did not start on 4181");
}

try {
  await waitForServer();
  const browser = await chromium.launch({ headless: true });

  try {
    // 1. PUBLIC BROWSER SHOWCASE: No mandatory login, no Quad-Studio, no Owner UI.
    {
      const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 }, reducedMotion: "reduce" });
      const page = await ctx.newPage();
      const errors = [];
      page.on("pageerror", (e) => errors.push(e.message));

      await page.goto(baseUrl, { waitUntil: "domcontentloaded" });
      await page.waitForSelector("#root .safi-intro-splash", { state: "detached", timeout: 20_000 });
      await page.waitForSelector('.safi-shell[data-runtime="showcase"]', { state: "visible", timeout: 10_000 });

      assert.equal(await page.locator(".safi-auth-modal").count(), 0, "Showcase must NOT force login modal");
      assert.equal(await page.locator('[data-tab-id="owner"]').count(), 0, "Showcase must NOT render Owner tab");
      assert.equal(await page.locator('[data-tab-id="studio"]').count(), 0, "Showcase must NOT render Studio tab");
      assert.equal(await page.locator('[data-desktop-studio-trigger="true"]').count(), 0);
      assert.deepEqual(errors, []);
      await ctx.close();
    }

    // 2. DESKTOP RUNTIME (Regular User): Mandatory login -> unlocks Quad-Studio -> STILL hides Owner UI.
    {
      const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 }, reducedMotion: "reduce" });
      const page = await ctx.newPage();
      const errors = [];
      page.on("pageerror", (e) => errors.push(e.message));
      await page.addInitScript(() => {
        window.__TAURI_INTERNALS__ = {
          invoke: async () => ({ ok: true })
        };
      });
      await page.route("**/v1/auth/login", async (route) => {
        await route.fulfill({
          status: 201,
          contentType: "application/json",
          body: JSON.stringify({
            token: "sfs_desktop_regular_user_token",
            is_owner: false,
            mfa_required: false,
            session: {
              id: "sess_reg",
              workspace_id: "ws_reg",
              device_name: "Desktop",
              expires_at: 9999999999,
              created_at: 1,
              last_seen_at: 1
            }
          })
        });
      });

      await page.goto(baseUrl, { waitUntil: "domcontentloaded" });
      await page.waitForSelector("#root .safi-intro-splash", { state: "detached", timeout: 20_000 });
      await page.waitForSelector(".safi-auth-modal", { state: "visible", timeout: 10_000 });
      assert.equal(await page.locator(".safi-auth-close").count(), 0, "Desktop auth gate must be mandatory");
      assert.equal(await page.locator(".safi-shell").count(), 0, "Workspace must stay hidden before sign-in");

      await page.fill('input[name="email"]', "analyst@example.test");
      await page.fill('input[name="password"]', "AnalystPass123!");
      await page.click('.safi-auth-modal button[type="submit"]');
      await page.waitForSelector('.safi-shell[data-runtime="desktop"]', { state: "visible", timeout: 10_000 });

      assert.equal(await page.locator('[data-desktop-studio-trigger="true"]').count(), 1, "Desktop unlocks Quad-Studio");
      assert.equal(await page.locator('[data-tab-id="owner"]').count(), 0, "Regular user must NEVER see Owner tab");
      assert.deepEqual(errors, []);
      await ctx.close();
    }

    // 3. DESKTOP RUNTIME (Server-Verified Owner): Exposes Owner tab -> gated by Google Authenticator TOTP MFA.
    {
      const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 }, reducedMotion: "reduce" });
      const page = await ctx.newPage();
      const errors = [];
      page.on("pageerror", (e) => errors.push(e.message));
      await page.addInitScript(() => {
        window.__TAURI_INTERNALS__ = {
          invoke: async () => ({ ok: true })
        };
      });
      await page.route("**/v1/auth/login", async (route) => {
        await route.fulfill({
          status: 201,
          contentType: "application/json",
          body: JSON.stringify({
            token: "sfs_desktop_owner_token",
            is_owner: true,
            mfa_required: true,
            session: {
              id: "sess_own",
              workspace_id: "ws_own",
              device_name: "Desktop",
              expires_at: 9999999999,
              created_at: 1,
              last_seen_at: 1
            }
          })
        });
      });
      await page.route("**/v1/owner/mfa/status", async (route) => {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({
            owner_bootstrapped: true,
            is_owner: true,
            totp_enrolled: true,
            totp_confirmed: true,
            elevated_session_active: false,
            elevated_expires_at: null,
            step_up_fresh: false,
            recovery_codes_remaining: 10,
            locked_until: null
          })
        });
      });

      await page.goto(baseUrl, { waitUntil: "domcontentloaded" });
      await page.waitForSelector("#root .safi-intro-splash", { state: "detached", timeout: 20_000 });
      await page.fill('input[name="email"]', "owner@example.test");
      await page.fill('input[name="password"]', "OwnerPass123!");
      await page.click('.safi-auth-modal button[type="submit"]');
      await page.waitForSelector('.safi-shell[data-runtime="desktop"]', { state: "visible", timeout: 10_000 });

      assert.equal(await page.locator('[data-tab-id="owner"]').count(), 1, "Verified Owner sees Owner tab in Desktop");
      await page.locator('[data-tab-id="owner"]').dispatchEvent("click");
      await page.waitForSelector('[data-owner-mfa="required"]', { state: "visible", timeout: 10_000 });
      const text = await page.textContent('[data-owner-mfa="required"]');
      assert.ok(text && text.includes("Google Authenticator"), "Owner panel must require Google Authenticator TOTP");
      assert.deepEqual(errors, []);
      await ctx.close();
    }

    console.log("RUNTIME_BOUNDARY_E2E_OK showcase=unlocked desktop_regular=auth_gated_no_owner desktop_owner=totp_mfa_gated");
  } finally {
    await browser.close();
  }
} finally {
  server.kill("SIGTERM");
}
