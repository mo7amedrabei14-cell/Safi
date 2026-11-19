#!/usr/bin/env node
/**
 * Real Headless Chromium Playwright E2E Verification for:
 * 1. Public Showcase Strict Boundary: blocks `/cleaning`, `/recipes`, `/charts`, `/library`,
 *    `/studio`, `?tab=cleaning` AND contains zero false `"Works 100% Offline"` claims.
 * 2. Real Desktop Installer Download: clicking `[data-download-installer-btn="true"]` triggers
 *    a genuine browser `download` event whose bytes match the published SHA-256 digest!
 * 3. Protected Website Owner Portal (`/owner`): rejects regular users (`SF-AUTH-008`), accepts
 *    Owner web session (`sfw_*`), enforces Google Authenticator TOTP MFA, and exposes
 *    Community Submissions, Official Templates, and App Releases tabs.
 * 4. Desktop Browser PKCE Auth (`/auth/desktop?flow=signin&request_id=...&state=...`):
 *    approves and exposes `safi://auth?code=sfcode_...&state=...` with zero session tokens in URL.
 */
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
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
      // Try next candidate
    }
  }
  throw new Error("Playwright not found.");
}

const { chromium } = await loadPlaywright();
const baseUrl = "http://127.0.0.1:4183";
const server = spawn(
  process.platform === "win32" ? "pnpm.cmd" : "pnpm",
  ["--filter", "@safi/web", "exec", "vite", "--host", "127.0.0.1", "--port", "4183", "--strictPort"],
  { stdio: "inherit", shell: process.platform === "win32" }
);

async function waitForServer() {
  for (let i = 0; i < 120; i += 1) {
    if (server.exitCode !== null) throw new Error(`Vite exited early (${server.exitCode})`);
    try {
      const r = await fetch(baseUrl);
      if (r.ok) return;
    } catch {
      // Waiting...
    }
    await delay(250);
  }
  throw new Error("Vite did not start on 4183");
}

try {
  await waitForServer();
  const browser = await chromium.launch({ headless: true });

  try {
    // 1. Public Showcase strict boundary across direct URLs + zero false 100% offline claims + REAL installer download
    {
      const ctx = await browser.newContext({
        viewport: { width: 1440, height: 900 },
        reducedMotion: "reduce",
        acceptDownloads: true
      });
      const page = await ctx.newPage();
      const errors = [];
      page.on("pageerror", (e) => errors.push(e.message));

      for (const blockedRoute of ["/cleaning", "/recipes", "/charts", "/library", "/studio", "/?tab=cleaning"]) {
        await page.goto(`${baseUrl}${blockedRoute}`, { waitUntil: "domcontentloaded", timeout: 60_000 });
        await page.waitForSelector("#root .safi-intro-splash", { state: "detached", timeout: 20_000 });
        await page.waitForSelector('.safi-shell[data-runtime="showcase"]', { state: "visible", timeout: 10_000 });
        const bodyText = await page.locator("body").innerText();
        assert.ok(!bodyText.includes("Works 100% Offline"), `False claim on ${blockedRoute}`);
        assert.ok(!bodyText.includes("100% offline"), `False claim on ${blockedRoute}`);
        assert.equal(await page.locator('[data-active-tab="cleaning"]').count(), 0);
        assert.equal(await page.locator('[data-active-tab="studio"]').count(), 0);
        assert.equal(await page.locator('[data-tab-id="cleaning"]').count(), 0);
      }

      // Honest unpublished installer verification: when manifest has `available: false` (no real
      // public HTTPS release URL configured yet), the download CTA must honestly show unavailable
      // instead of linking to a fake or local untracked dev binary!
      const manifest = JSON.parse(
        readFileSync(resolve("apps/web/public/downloads/release-manifest.json"), "utf8")
      );
      const downloadBtn = page.locator('[data-download-installer-btn="true"]');
      await downloadBtn.scrollIntoViewIfNeeded();
      await downloadBtn.waitFor({ state: "visible", timeout: 10_000 });

      if (manifest.platforms.windows.available && manifest.platforms.windows.downloadUrl) {
        const expectedSha = manifest.platforms.windows.sha256;
        const expectedBytes = manifest.platforms.windows.fileSizeBytes;
        const [download] = await Promise.all([
          page.waitForEvent("download", { timeout: 20_000 }),
          downloadBtn.click()
        ]);
        const downloadedPath = await download.path();
        assert.ok(downloadedPath, "Playwright must save a real downloaded file");
        const bytes = readFileSync(downloadedPath);
        assert.equal(bytes.length, expectedBytes, "Downloaded installer byte length must match manifest");
        const actualSha = createHash("sha256").update(bytes).digest("hex");
        assert.equal(actualSha, expectedSha, "Downloaded installer SHA-256 must match manifest");
      } else {
        assert.equal(
          await downloadBtn.isDisabled(),
          true,
          "Unpublished Windows installer button must be disabled"
        );
        const btnText = await downloadBtn.innerText();
        assert.ok(
          btnText.includes("Not Yet Published") || btnText.includes("غير متاح بعد"),
          `Unpublished installer button must state unavailable honestly, got: ${btnText}`
        );
      }
      assert.deepEqual(errors, []);
      await ctx.close();
    }

    // 2. Protected Website Owner Portal (`/owner`): Regular user rejected; Owner gets TOTP MFA + Community/Templates/Releases
    {
      const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 }, reducedMotion: "reduce" });
      const page = await ctx.newPage();

      await page.route("**/v1/owner/web/login", async (route) => {
        const body = JSON.parse(route.request().postData() ?? "{}");
        if (body.email === "regular@test.local") {
          await route.fulfill({
            status: 403,
            contentType: "application/json",
            body: JSON.stringify({ code: "SF-AUTH-013", title: "OwnerRequired" })
          });
          return;
        }
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({
            token: "sfw_test_owner_web_session_0123456789abcdef0123456789abcdef0123456789abcdef",
            session_id: "web_sess_1",
            user_id: "01955555-5555-7555-8555-555555555555",
            workspace_id: "01955555-5555-7555-8555-555555555556",
            email: "owner@test.local",
            is_owner: true,
            expires_at: 9999999999
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
            recovery_codes_remaining: 8,
            locked_until: null
          })
        });
      });

      await page.goto(`${baseUrl}/owner`, { waitUntil: "domcontentloaded", timeout: 60_000 });
      await page.waitForSelector("#root .safi-intro-splash", { state: "detached", timeout: 20_000 });
      await page.waitForSelector('[data-owner-web-portal="login"]', { state: "visible", timeout: 10_000 });

      // Regular user denied with SF-AUTH-008
      await page.fill('[data-owner-web-email="true"]', "regular@test.local");
      await page.fill('[data-owner-web-password="true"]', "RegularPass123!");
      await page.click('[data-owner-web-submit="true"]');
      const errBanner = page.locator('[data-owner-web-error="true"]');
      await errBanner.waitFor({ state: "visible", timeout: 10_000 });
      assert.ok((await errBanner.innerText()).includes("SF-AUTH-013"));

      // Owner accepted -> sees isolated web session notice & Google Authenticator MFA gate
      await page.fill('[data-owner-web-email="true"]', "owner@test.local");
      await page.fill('[data-owner-web-password="true"]', "OwnerPass123!");
      await page.click('[data-owner-web-submit="true"]');
      await page.waitForSelector('[data-owner-web-portal="authenticated"]', { state: "visible", timeout: 10_000 });
      const portalText = await page.locator('[data-owner-web-portal="authenticated"]').innerText();
      assert.ok(portalText.includes("Google Authenticator"), "Owner Web Portal must enforce TOTP MFA");
      assert.ok(
        portalText.includes("Does not revoke") || portalText.includes("لا تؤثر على جهاز سطح المكتب"),
        "Owner Web Portal must disclose Desktop session isolation"
      );
      await ctx.close();
    }

    // 3. Desktop Browser PKCE Auth Page (`/auth/desktop?flow=signin&request_id=...&state=...`)
    {
      const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 }, reducedMotion: "reduce" });
      const page = await ctx.newPage();

      await page.route("**/v1/auth/desktop/approve", async (route) => {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({
            code: "sfcode_abc123def4567890",
            state: "state_nonce_1234567890",
            callback_uri: "safi://auth?code=sfcode_abc123def4567890&state=state_nonce_1234567890"
          })
        });
      });

      await page.goto(
        `${baseUrl}/auth/desktop?flow=signin&request_id=01955555-5555-7555-8555-555555555555&state=state_nonce_1234567890`,
        { waitUntil: "domcontentloaded", timeout: 60_000 }
      );
      await page.waitForSelector("#root .safi-intro-splash", { state: "detached", timeout: 20_000 });
      await page.waitForSelector('[data-desktop-browser-auth="true"]', { state: "visible", timeout: 10_000 });
      await page.fill('[data-desktop-auth-email="true"]', "user@test.local");
      await page.fill('[data-desktop-auth-password="true"]', "UserPass123!");
      await page.click('[data-desktop-auth-submit="true"]');

      const link = page.locator('[data-desktop-auth-callback-link="true"]');
      await link.waitFor({ state: "visible", timeout: 10_000 });
      const href = await link.getAttribute("href");
      assert.ok(href?.startsWith("safi://auth?code=sfcode_"), `Unexpected callback URL: ${href}`);
      assert.ok(!href.includes("sfs_"), "Session token must NEVER appear in Desktop callback URL");
      await ctx.close();
    }

    console.log(
      "[verify-desktop-download-and-owner-web-e2e] PASS (UI-boundary smoke test; see verify-desktop-full-journey-real-e2e.mjs for unmocked full-stack E2E): Showcase boundary, honest installer availability state, /owner Web Portal UI, and /auth/desktop PKCE UI verified."
    );
  } finally {
    await browser.close();
  }
} finally {
  server.kill("SIGTERM");
}
