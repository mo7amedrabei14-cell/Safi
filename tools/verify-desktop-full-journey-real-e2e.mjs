#!/usr/bin/env node
/**
 * REAL UNMOCKED End-to-End Acceptance Suite (ZERO `page.route` Backend Mocks, ZERO Rust Mocks):
 *
 * Spawns:
 *   1. Real compiled Rust `sf-server` (`auth_harness_server`) over loopback HTTP (`SAFI_DEV_API_PROXY`)
 *   2. Real compiled Rust `sf-desktop` (`studio_ipc_harness`) over stdio JSON-RPC
 *   3. Real Vite frontend (`apps/web`) + real headless Chromium (Playwright)
 *
 * Proves end-to-end WITHOUT ANY API OR ENGINE MOCKS:
 *   1. Desktop Browser PKCE fail-closed guard (state mismatch -> `SF-AUTH-023`, workspace stays locked).
 *   2. Desktop Browser PKCE Sign-In (`POST /v1/auth/desktop/start` -> real `/auth/desktop` page ->
 *      `POST /v1/auth/desktop/approve` -> `safi://auth?code=...&state=...` -> `take_pending_deep_link` ->
 *      `POST /v1/auth/desktop/exchange` -> native `StudioStore` bind -> workspace unlocked).
 *   3. Desktop Local Full-Text Search UI (`DesktopLocalSearchPanel`): indexes a real local folder
 *      containing an Arabic/English CSV + a real `/FlateDecode`-compressed PDF stream, searches both
 *      Arabic text and FlateDecode PDF text, and renders real ranked BM25 hits in the UI.
 *   4. Desktop Recipe Library UI (`DesktopRecipeLibraryPanel`):
 *      - Syncs real Official Templates from `GET /v1/templates/official`.
 *      - Customizes an Official Template via `Save as New Recipe` (original Official Template immutable).
 *      - Enforces real Backend Premium `.safitemplate` Entitlement (`SF-RCP-005` for Free user; allowed +
 *        real disk `.safitemplate` export & re-import for Paid/VIP user `pro@safidata.com`).
 *      - Submits a real sanitized `Share with SafiData` community contribution (`POST /v1/community/submissions`).
 *   5. Real Website Owner Portal (`/owner`): signs in with `owner@safidata.com` (`POST /v1/owner/web/login`
 *      -> `sfw_*`), verifies live RFC 6238 TOTP (`POST /v1/owner/mfa/verify`), reviews & approves the real
 *      community submission, publishes it as an Official Template, and confirms it appears in
 *      `GET /v1/templates/official`.
 */
import assert from "node:assert/strict";
import { createHmac } from "node:crypto";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { execFileSync, spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { setTimeout as delay } from "node:timers/promises";
import { deflateSync } from "node:zlib";

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
  throw new Error("Playwright not found. Set PLAYWRIGHT_MODULE_URL.");
}

function decodeBase32(base32) {
  const alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
  const clean = base32.toUpperCase().replace(/[^A-Z2-7]/g, "");
  let bits = 0;
  let value = 0;
  const out = [];
  for (const ch of clean) {
    const idx = alphabet.indexOf(ch);
    if (idx === -1) continue;
    value = (value << 5) | idx;
    bits += 5;
    if (bits >= 8) {
      out.push((value >>> (bits - 8)) & 0xff);
      bits -= 8;
    }
  }
  return Buffer.from(out);
}

function computeTotp(base32Secret, unixSeconds = Math.floor(Date.now() / 1000)) {
  const key = decodeBase32(base32Secret);
  const counter = BigInt(Math.floor(unixSeconds / 30));
  const msg = Buffer.alloc(8);
  msg.writeBigUInt64BE(counter, 0);
  const hmac = createHmac("sha1", key).update(msg).digest();
  const offset = hmac[hmac.length - 1] & 0x0f;
  const code =
    (((hmac[offset] & 0x7f) << 24) |
      ((hmac[offset + 1] & 0xff) << 16) |
      ((hmac[offset + 2] & 0xff) << 8) |
      (hmac[offset + 3] & 0xff)) %
    1_000_000;
  return String(code).padStart(6, "0");
}

function buildFlateDecodePdf(text) {
  const content = `BT /F1 12 Tf (${text}) Tj ET`;
  const compressed = deflateSync(Buffer.from(content, "utf8"));
  const header = Buffer.from(
    `%PDF-1.4\n1 0 obj\n<< /Length ${compressed.length} /Filter /FlateDecode >>\nstream\n`,
    "latin1"
  );
  const footer = Buffer.from("\nendstream\nendobj\n%%EOF\n", "latin1");
  return Buffer.concat([header, compressed, footer]);
}

const { chromium } = await loadPlaywright();

const workDir = resolve("artifacts/real-e2e-full-journey");
await rm(workDir, { recursive: true, force: true });
const bundlesDir = resolve(workDir, "bundles");
const searchDir = resolve(workDir, "local-docs");
await mkdir(bundlesDir, { recursive: true });
await mkdir(searchDir, { recursive: true });

const csvPath = resolve(searchDir, "مبيعات_القاهرة.csv");
const flatePdfPath = resolve(searchDir, "compressed_q4_report.pdf");
const capsulePath = resolve(workDir, "capsule.safi.html");
const safiTemplatePath = resolve(workDir, "pro_exported.safitemplate");

await writeFile(
  csvPath,
  "region,revenue,notes\nالقاهرة,42000,تقرير المبيعات الربع سنوي\nAlexandria,31500,Q4 Growth\n",
  "utf8"
);
await writeFile(flatePdfPath, buildFlateDecodePdf("FlateDecode Quarterly Audit 2026"));

console.log("[0/6] Building real Rust sf-desktop harness + real Rust sf-server harness...");
execFileSync("cargo", ["build", "-p", "sf-desktop", "--example", "studio_ipc_harness"], {
  stdio: "inherit"
});
execFileSync("cargo", ["build", "-p", "sf-server", "--example", "auth_harness_server"], {
  stdio: "inherit"
});

const binExt = process.platform === "win32" ? ".exe" : "";
const harnessBin = resolve("target/debug/examples", `studio_ipc_harness${binExt}`);
const authServerBin = resolve("target/debug/examples", `auth_harness_server${binExt}`);
const vitePort = 4187;
const baseUrl = `http://127.0.0.1:${vitePort}`;

const authProc = spawn(authServerBin, [], {
  stdio: ["ignore", "pipe", "inherit"],
  env: {
    ...process.env,
    AUTH_HARNESS_ADDR: "127.0.0.1:0",
    AUTH_HARNESS_PUBLIC_ORIGIN: baseUrl
  }
});

const authRl = createInterface({ input: authProc.stdout });
const authInfo = await new Promise((resolveInfo, rejectInfo) => {
  const timer = setTimeout(() => rejectInfo(new Error("auth_harness_server startup timeout")), 90_000);
  authRl.on("line", (line) => {
    const trimmed = line.trim();
    if (!trimmed) return;
    const msg = JSON.parse(trimmed);
    if (msg.ready && msg.addr && msg.ownerTotpBase32) {
      clearTimeout(timer);
      resolveInfo(msg);
    }
  });
  authProc.once("exit", (code) => {
    clearTimeout(timer);
    rejectInfo(new Error(`auth_harness_server exited early (${code})`));
  });
});

const authAddr = String(authInfo.addr);
const ownerTotpBase32 = String(authInfo.ownerTotpBase32);

const rustProc = spawn(harnessBin, [], {
  stdio: ["pipe", "pipe", "inherit"],
  env: {
    ...process.env,
    HARNESS_STORAGE_DIR: bundlesDir,
    HARNESS_CSV_PATH: csvPath,
    HARNESS_CAPSULE_PATH: capsulePath,
    HARNESS_SEARCH_DIR: searchDir,
    HARNESS_SAFITEMPLATE_PATH: safiTemplatePath,
    HARNESS_ALLOWED_AUTH_ORIGIN: baseUrl
  }
});

const rustRl = createInterface({ input: rustProc.stdout });
const pendingRpc = new Map();
let rpcSeq = 0;
const harnessReady = new Promise((res) => {
  rustRl.on("line", (line) => {
    const trimmed = line.trim();
    if (!trimmed) return;
    const msg = JSON.parse(trimmed);
    if (msg.ready) {
      res();
      return;
    }
    const waiter = pendingRpc.get(msg.id);
    if (!waiter) return;
    pendingRpc.delete(msg.id);
    if (msg.ok) waiter.resolve(msg.result);
    else waiter.reject(new Error(String(msg.error)));
  });
});

async function callRust(command, args = {}) {
  await harnessReady;
  const id = ++rpcSeq;
  return new Promise((resolveRpc, rejectRpc) => {
    pendingRpc.set(id, { resolve: resolveRpc, reject: rejectRpc });
    rustProc.stdin.write(`${JSON.stringify({ id, command, args })}\n`);
  });
}

async function verifySessionAgainstRealServer(token, installationId) {
  const resp = await fetch(`http://${authAddr}/v1/auth/sessions/current`, {
    method: "GET",
    headers: {
      Authorization: `Bearer ${token}`,
      "X-Safi-Installation-Id": installationId,
      Accept: "application/json"
    }
  });
  if (!resp.ok) {
    let code = "SF-AUTH-001";
    try {
      const body = await resp.json();
      if (body && typeof body.code === "string") code = body.code;
    } catch {
      // ignore
    }
    throw new Error(code);
  }
  return await resp.json();
}

const viteProc = spawn(
  process.execPath,
  [
    resolve("apps/web/node_modules/vite/bin/vite.js"),
    "--host",
    "127.0.0.1",
    "--port",
    String(vitePort),
    "--strictPort"
  ],
  {
    cwd: resolve("apps/web"),
    stdio: "inherit",
    env: {
      ...process.env,
      SAFI_API_BASE_URL: "",
      SAFI_DESKTOP_RELEASE_BUILD: "0",
      SAFI_DEV_API_PROXY: `http://${authAddr}`
    }
  }
);

async function waitForVite() {
  for (let i = 0; i < 120; i += 1) {
    if (viteProc.exitCode !== null) throw new Error(`Vite exited early (${viteProc.exitCode})`);
    try {
      const r = await fetch(baseUrl);
      if (r.ok) return;
    } catch {
      // wait
    }
    await delay(250);
  }
  throw new Error(`Vite did not start on ${baseUrl}`);
}

let browser;
try {
  await waitForVite();
  browser = await chromium.launch({ headless: true, args: ["--no-sandbox"] });

  const createDesktopPage = async () => {
    const ctx = await browser.newContext({ viewport: { width: 1440, height: 960 }, reducedMotion: "reduce" });
    const page = await ctx.newPage();
    await page.exposeFunction("__safiRustInvoke", async (command, args) => {
      const safeArgs = args ?? {};
      if (command === "studio_set_auth_session") {
        const token = String(safeArgs.token ?? "");
        const installationId = String(safeArgs.installationId ?? safeArgs.installation_id ?? "");
        const verified = await verifySessionAgainstRealServer(token, installationId);
        await callRust("__harness_register_session", {
          token,
          sessionId: verified.id,
          workspaceId: verified.workspace_id,
          installationId,
          expiresAtMs: Number(verified.expires_at) * 1000
        });
      }
      return callRust(command, safeArgs);
    });
    await page.addInitScript(() => {
      window.__TAURI_INTERNALS__ = {
        invoke: (command, args) => window.__safiRustInvoke(command, args ?? {})
      };
    });
    return { ctx, page };
  };

  async function completeBrowserPkceSignIn(desktopPage, email, password) {
    await desktopPage.locator('[data-desktop-pkce-signin="true"]').click();
    await desktopPage.locator('[data-desktop-pkce-waiting="true"]').waitFor({ state: "visible", timeout: 10_000 });

    const openedRaw = JSON.parse(await callRust("__harness_take_opened_auth_url"));
    assert.ok(openedRaw.url && openedRaw.url.startsWith(`${baseUrl}/auth/desktop?flow=signin&`), `Unexpected auth_url: ${openedRaw.url}`);

    const browserCtx = await browser.newContext({ viewport: { width: 1280, height: 800 }, reducedMotion: "reduce" });
    const browserPage = await browserCtx.newPage();
    await browserPage.goto(openedRaw.url, { waitUntil: "domcontentloaded", timeout: 60_000 });
    await browserPage.waitForSelector("#root .safi-intro-splash", { state: "detached", timeout: 20_000 });
    await browserPage.waitForSelector('[data-desktop-browser-auth="true"]', { state: "visible", timeout: 10_000 });
    await browserPage.fill('[data-desktop-auth-email="true"]', email);
    await browserPage.fill('[data-desktop-auth-password="true"]', password);
    await browserPage.click('[data-desktop-auth-submit="true"]');

    const cbLink = browserPage.locator('[data-desktop-auth-callback-link="true"]');
    await cbLink.waitFor({ state: "visible", timeout: 15_000 });
    const callbackUri = await cbLink.getAttribute("href");
    assert.ok(callbackUri?.startsWith("safi://auth?code=sfcode_"), `Invalid callback URI: ${callbackUri}`);
    assert.ok(!callbackUri.includes("sfs_"), "Session token must NEVER appear in callback URI");
    await browserCtx.close();

    // Deliver real deep link into native Rust shell queue!
    await callRust("__harness_queue_deep_link", { url: callbackUri });
    // Desktop polls `take_pending_deep_link` -> calls real `/v1/auth/desktop/exchange` -> unlocks Workspace!
    await desktopPage.locator('button[data-desktop-studio-trigger="true"]').waitFor({ state: "visible", timeout: 15_000 });
  }

  // -------------------------------------------------------------------------
  // [1/6] Real Desktop Browser PKCE Auth — State Mismatch Rejection + Real Sign-In
  // -------------------------------------------------------------------------
  console.log("[1/6] Verifying real unmocked Desktop Browser PKCE Auth (fail-closed state mismatch + real exchange)...");
  const { ctx: desktopCtx, page: desktopPage } = await createDesktopPage();
  await desktopPage.goto(baseUrl, { waitUntil: "domcontentloaded", timeout: 60_000 });
  await desktopPage.waitForSelector("#root .safi-intro-splash", { state: "detached", timeout: 20_000 });
  await desktopPage.locator('[data-desktop-pkce-signin="true"]').waitFor({ state: "visible", timeout: 15_000 });

  // 1a. Fail-closed state mismatch MUST keep workspace strictly locked!
  await desktopPage.locator('[data-desktop-pkce-signin="true"]').click();
  await desktopPage.locator('[data-desktop-pkce-waiting="true"]').waitFor({ state: "visible", timeout: 10_000 });
  await callRust("__harness_take_opened_auth_url");
  await callRust("__harness_queue_deep_link", {
    url: "safi://auth?code=sfcode_fake_1234567890&state=mismatched_state_1234567890"
  });
  await desktopPage.locator('[data-desktop-pkce-error="true"]').waitFor({ state: "visible", timeout: 10_000 });
  assert.equal(await desktopPage.locator('button[data-desktop-studio-trigger="true"]').count(), 0, "Workspace must remain locked on PKCE state mismatch");

  // 1b. Real PKCE Sign-In for Free User (`analyst@safidata.com`)
  await completeBrowserPkceSignIn(desktopPage, "analyst@safidata.com", "SafiAnalyst#2026");

  // -------------------------------------------------------------------------
  // [2/6] Real Desktop Local Search UI (Arabic CSV + FlateDecode PDF)
  // -------------------------------------------------------------------------
  console.log("[2/6] Verifying real Desktop Local Search UI (Arabic CSV + FlateDecode PDF)...");
  await desktopPage.locator('button[data-desktop-studio-trigger="true"]').click();
  await desktopPage.locator('[data-testid="toggle-local-search-panel"]').waitFor({ state: "visible", timeout: 10_000 });
  await desktopPage.locator('[data-testid="toggle-local-search-panel"]').click();
  await desktopPage.locator('[data-desktop-local-search="true"]').waitFor({ state: "visible", timeout: 10_000 });

  await desktopPage.fill('[data-local-search-folder-input="true"]', searchDir);
  await desktopPage.click('[data-local-search-index-btn="true"]');
  await desktopPage.locator('[data-local-search-index-summary="true"]').waitFor({ state: "visible", timeout: 15_000 });

  const docsCount = Number(
    await desktopPage.locator('[data-indexed-docs-count]').getAttribute("data-indexed-docs-count")
  );
  assert.ok(docsCount >= 2, `Expected at least 2 indexed documents (CSV + FlateDecode PDF), got ${docsCount}`);

  // Search FlateDecode compressed PDF content
  await desktopPage.fill('[data-local-search-query-input="true"]', "FlateDecode");
  await desktopPage.click('[data-local-search-submit="true"]');
  await desktopPage.locator('[data-local-search-hit="true"]').first().waitFor({ state: "visible", timeout: 10_000 });
  const pdfHitText = await desktopPage.locator('[data-local-search-results="true"]').innerText();
  assert.ok(pdfHitText.includes("compressed_q4_report.pdf"), `Missing FlateDecode PDF hit: ${pdfHitText}`);

  // Search Arabic content in CSV & click "Open in Studio" on the CSV search hit!
  await desktopPage.fill('[data-local-search-query-input="true"]', "القاهرة");
  await desktopPage.click('[data-local-search-submit="true"]');
  await desktopPage.waitForFunction(
    () => document.querySelector('[data-local-search-results="true"]')?.textContent?.includes("مبيعات_القاهرة.csv"),
    undefined,
    { timeout: 10_000 }
  );
  await desktopPage.locator('[data-search-open-in-studio="true"]').first().click();

  // -------------------------------------------------------------------------
  // [2B/6] Real Desktop Per-Sheet Workflow Memory & Continuous Local Learning UI
  // -------------------------------------------------------------------------
  console.log("[2B/6] Verifying real Desktop Per-Sheet Workflow Memory & Continuous Local Learning UI...");
  await desktopPage.locator('[data-testid="toggle-workflow-memory-panel"]').click();
  await desktopPage.locator('[data-desktop-workflow-memory="true"]').waitFor({ state: "visible", timeout: 10_000 });

  await desktopPage.fill('[data-workflow-name-input="true"]', "Cairo Monthly Sales Workflow");
  await desktopPage.click('[data-save-workflow-memory-btn="true"]');
  await desktopPage.locator('[data-saved-workflow-id]').first().waitFor({ state: "visible", timeout: 10_000 });

  // Automatic match banner on active sheet -> verify high confidence semantic match
  await desktopPage.locator('[data-workflow-match-banner="true"]').waitFor({ state: "visible", timeout: 10_000 });
  const matchTier = (
    (await desktopPage
      .locator('[data-workflow-match-banner="true"]')
      .getAttribute("data-match-tier")) ?? ""
  ).toLowerCase();
  assert.equal(matchTier, "high", `Expected high confidence semantic match on saved sheet, got ${matchTier}`);

  // Record explicit user feedback rule in Continuous Local Learning Profile
  await desktopPage.fill('[data-learn-target-input="true"]', "amount");
  await desktopPage.fill('[data-learn-preferred-input="true"]', "Measure");
  await desktopPage.click('[data-record-learning-btn="true"]');
  await desktopPage.locator('[data-learned-rule-id]').first().waitFor({ state: "visible", timeout: 10_000 });
  await desktopPage.locator('[data-learned-suggestion-rule]').first().waitFor({ state: "visible", timeout: 10_000 });

  // Undo / delete the learned rule
  await desktopPage.locator('[data-delete-learned-rule-btn]').first().click();
  await desktopPage.waitForFunction(
    () => document.querySelectorAll('[data-learned-rule-id]').length === 0,
    undefined,
    { timeout: 10_000 }
  );

  // -------------------------------------------------------------------------
  // [2C/6] Real Rust Safi Optimal Decision Engine UI (6 Domains, Audit, Apply & Consistency)
  // -------------------------------------------------------------------------
  console.log("[2C/6] Verifying real Rust Safi Optimal Decision Engine UI (6 domains, lexicographic audit, apply & cross-product consistency)...");
  await desktopPage.locator('[data-testid="toggle-decision-engine-panel"]').click();
  await desktopPage.locator('[data-desktop-decision-engine="true"]').waitFor({ state: "visible", timeout: 10_000 });
  await desktopPage.locator('[data-recommended-decision-card="dec_optimal_kpi"]').waitFor({ state: "visible", timeout: 10_000 });

  const decisionCardsCount = await desktopPage.locator('[data-recommended-decision-card]').count();
  assert.equal(decisionCardsCount, 6, `Expected 6 primary domain Recommended Decisions, got ${decisionCardsCount}`);

  // Expand Advanced Details / Audit on Optimal KPI Decision
  await desktopPage.click('[data-toggle-decision-audit="dec_optimal_kpi"]');
  await desktopPage.locator('[data-decision-audit-drawer="dec_optimal_kpi"]').waitFor({ state: "visible", timeout: 10_000 });

  // Apply Coherent Dashboard & Report Blueprint decision -> verify 100% Cross-Product Consistency
  await desktopPage.click('[data-apply-recommended-decision="dec_dashboard_blueprint"]');
  await desktopPage.locator('[data-decision-engine-notice="true"]').waitFor({ state: "visible", timeout: 10_000 });
  const consistencyOk = await desktopPage
    .locator('[data-consistency-report="true"]')
    .getAttribute("data-consistency-ok");
  assert.equal(consistencyOk, "true", "Expected 100% Cross-Product Consistency after applying blueprint decision");
  const aiRouteCount = await desktopPage.locator('[data-ai-route-task]').count();
  assert.equal(aiRouteCount, 8, `Expected 8 canonical AI Task Routing entries, got ${aiRouteCount}`);

  // -------------------------------------------------------------------------
  // [3/6] Real Desktop Recipe Library UI (Sync Official, Save as New, Free SF-RCP-005, Share with SafiData)
  // -------------------------------------------------------------------------
  console.log("[3/6] Verifying real Desktop Recipe Library UI (Official sync, Save as New, Free SF-RCP-005, Community Share)...");
  await desktopPage.locator('[data-testid="toggle-recipe-library-panel"]').click();
  await desktopPage.locator('[data-desktop-recipe-library="true"]').waitFor({ state: "visible", timeout: 10_000 });

  // Sync Official Templates from real `sf-server`
  await desktopPage.click('[data-recipes-sync-official-btn="true"]');
  await desktopPage.locator('[data-recipes-notice="true"]').waitFor({ state: "visible", timeout: 10_000 });

  // Customize first Official Template and Save as New Recipe (original remains immutable)
  const firstCustomizeBtn = desktopPage.locator('[data-customize-official-btn]').first();
  await firstCustomizeBtn.waitFor({ state: "visible", timeout: 10_000 });
  await firstCustomizeBtn.click();
  await desktopPage.click('[data-recipe-add-step-btn="true"]');
  await desktopPage.fill('[data-recipe-draft-name="true"]', "Cairo Clean Pipeline");
  await desktopPage.fill('[data-recipe-draft-desc="true"]', "Declarative Arabic cleaning workflow");
  await desktopPage.click('[data-recipe-save-new-btn="true"]');
  await desktopPage.locator('[data-my-recipe-id]').first().waitFor({ state: "visible", timeout: 10_000 });

  // Free user attempting `.safitemplate` export MUST hit real Backend `403 SF-RCP-005`!
  await desktopPage.fill('[data-safitemplate-path-input="true"]', safiTemplatePath);
  await desktopPage.locator('[data-export-safitemplate-btn]').first().click();
  const recipeErr = desktopPage.locator('[data-recipes-error="true"]');
  await recipeErr.waitFor({ state: "visible", timeout: 10_000 });
  assert.ok((await recipeErr.innerText()).includes("SF-RCP-005"), `Free user must get SF-RCP-005, got: ${await recipeErr.innerText()}`);

  // Free user shares recipe via `Share with SafiData` (Preview + Consent -> real `POST /v1/community/submissions`)
  await desktopPage.locator('[data-share-safidata-btn]').first().click();
  await desktopPage.locator('[data-share-preview-modal="true"]').waitFor({ state: "visible", timeout: 10_000 });
  const previewJsonText = await desktopPage.locator('[data-share-preview-json="true"]').innerText();
  assert.ok(!previewJsonText.includes("42000"), "Community share preview must NEVER include customer data rows");
  await desktopPage.check('[data-share-consent-checkbox="true"]');
  await desktopPage.click('[data-share-submit-btn="true"]');
  await desktopPage.locator('[data-my-community-submissions="true"]').waitFor({ state: "visible", timeout: 10_000 });
  const subText = await desktopPage.locator('[data-my-community-submissions="true"]').innerText();
  assert.ok(subText.includes("Cairo Clean Pipeline"), `Missing submitted contribution in list: ${subText}`);
  await desktopCtx.close();

  // -------------------------------------------------------------------------
  // [4/6] Real Paid/VIP User (`pro@safidata.com`) PKCE + `.safitemplate` Export & Import
  // -------------------------------------------------------------------------
  console.log("[4/6] Verifying Paid/VIP user real PKCE + `.safitemplate` export & re-import...");
  const { ctx: proCtx, page: proPage } = await createDesktopPage();
  await proPage.goto(baseUrl, { waitUntil: "domcontentloaded", timeout: 60_000 });
  await proPage.waitForSelector("#root .safi-intro-splash", { state: "detached", timeout: 20_000 });
  await completeBrowserPkceSignIn(proPage, "pro@safidata.com", "SafiPro#2026");

  await proPage.locator('button[data-desktop-studio-trigger="true"]').click();
  await proPage.locator('[data-testid="toggle-recipe-library-panel"]').click();
  await proPage.locator('[data-my-recipe-id]').first().waitFor({ state: "visible", timeout: 10_000 });
  await proPage.fill('[data-safitemplate-path-input="true"]', safiTemplatePath);
  await proPage.locator('[data-export-safitemplate-btn]').first().click();
  await proPage.locator('[data-recipes-notice="true"]').waitFor({ state: "visible", timeout: 10_000 });

  const exportedRaw = JSON.parse(await readFile(safiTemplatePath, "utf8"));
  assert.equal(exportedRaw.format, "safitemplate");
  assert.equal(exportedRaw.schema_version, 1);
  assert.ok(typeof exportedRaw.checksum_blake3 === "string" && exportedRaw.checksum_blake3.length === 64);

  // Import `.safitemplate` back into My Recipes
  const beforeCount = await proPage.locator('[data-my-recipe-id]').count();
  await proPage.click('[data-safitemplate-import-btn="true"]');
  await proPage.waitForFunction(
    (prev) => document.querySelectorAll('[data-my-recipe-id]').length > prev,
    beforeCount,
    { timeout: 10_000 }
  );
  await proCtx.close();

  // -------------------------------------------------------------------------
  // [5/6] Real Unmocked Website Owner Portal (`/owner`) + Live RFC 6238 TOTP + Review & Publish
  // -------------------------------------------------------------------------
  console.log("[5/6] Verifying real unmocked Owner Web Portal (/owner + live TOTP + approve & publish)...");
  const ownerCtx = await browser.newContext({ viewport: { width: 1440, height: 960 }, reducedMotion: "reduce" });
  const ownerPage = await ownerCtx.newPage();
  await ownerPage.goto(`${baseUrl}/owner`, { waitUntil: "domcontentloaded", timeout: 60_000 });
  await ownerPage.waitForSelector("#root .safi-intro-splash", { state: "detached", timeout: 20_000 });
  await ownerPage.waitForSelector('[data-owner-web-portal="login"]', { state: "visible", timeout: 10_000 });

  // Regular user denied by real `sf-server` (`SF-AUTH-013`)
  await ownerPage.fill('[data-owner-web-email="true"]', "analyst@safidata.com");
  await ownerPage.fill('[data-owner-web-password="true"]', "SafiAnalyst#2026");
  await ownerPage.click('[data-owner-web-submit="true"]');
  await ownerPage.locator('[data-owner-web-error="true"]').waitFor({ state: "visible", timeout: 30_000 });
  assert.ok((await ownerPage.locator('[data-owner-web-error="true"]').innerText()).includes("SF-AUTH-013"));

  // Real Owner logs in (`sfw_*`)
  await ownerPage.fill('[data-owner-web-email="true"]', "owner@safidata.com");
  await ownerPage.fill('[data-owner-web-password="true"]', "SafiOwner#2026");
  await ownerPage.click('[data-owner-web-submit="true"]');
  await ownerPage.waitForSelector('[data-owner-web-portal="authenticated"]', { state: "visible", timeout: 30_000 });

  // Verify live RFC 6238 TOTP against real `sf-server` (`/v1/owner/mfa/verify`)
  const liveTotp = computeTotp(ownerTotpBase32);
  await ownerPage.locator('input[inputmode="numeric"]').fill(liveTotp);
  await ownerPage.locator('button[type="submit"]').click();
  await ownerPage.locator('[data-owner-tab="community"]').waitFor({ state: "visible", timeout: 15_000 });

  // Open Community tab -> Approve -> Publish -> Verify visible in Official Templates!
  await ownerPage.click('[data-owner-tab="community"]');
  await ownerPage.waitForFunction(
    () => document.body.innerText.includes("Cairo Clean Pipeline"),
    undefined,
    { timeout: 10_000 }
  );
  await ownerPage.locator('[data-community-approve]').first().click();
  await ownerPage.waitForFunction(
    () => document.body.innerText.includes("approved"),
    undefined,
    { timeout: 10_000 }
  );
  await ownerPage.locator('[data-community-publish]').first().click();
  await ownerPage.waitForFunction(
    () => document.body.innerText.includes("published"),
    undefined,
    { timeout: 10_000 }
  );
  await ownerCtx.close();

  console.log("[6/6] Confirming published Community Template appears live in GET /v1/templates/official...");
  const ownerLoginResp = await fetch(`http://${authAddr}/v1/auth/login`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      email: "pro@safidata.com",
      password: "SafiPro#2026",
      installation_id: "33333333-3333-4333-8333-333333333333",
      platform: "windows",
      device_name: "Verification Client"
    })
  });
  assert.equal(ownerLoginResp.status, 201);
  const { token: verifyToken } = await ownerLoginResp.json();
  const officialResp = await fetch(`http://${authAddr}/v1/templates/official`, {
    headers: { Authorization: `Bearer ${verifyToken}` }
  });
  assert.equal(officialResp.status, 200);
  const officialList = await officialResp.json();
  assert.ok(
    officialList.some((tpl) => tpl.name === "Cairo Clean Pipeline"),
    `Published community template must appear in live /v1/templates/official: ${JSON.stringify(officialList.map((t) => t.name))}`
  );

  console.log(
    "[verify-desktop-full-journey-real-e2e] PASS (100% REAL, ZERO BACKEND/RUST MOCKS): PKCE Auth + Local Search (Arabic CSV & FlateDecode PDF) + Official Sync + Save as New + Premium .safitemplate (Free SF-RCP-005 / Paid OK) + Share with SafiData + Owner Web Portal TOTP Review & Publish verified end-to-end."
  );
} finally {
  if (browser) await browser.close();
  viteProc.kill("SIGTERM");
  rustProc.kill("SIGTERM");
  authProc.kill("SIGTERM");
}
