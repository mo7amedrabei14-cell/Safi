/**
 * Chromium UI + Real sf-server Auth + Real Rust StudioStore Subprocess Harness Integration Test.
 *
 * Scope & honesty note:
 * - Spawns real Rust `sf-server` (`auth_harness_server`) for genuine `/v1/auth/login` (Argon2id),
 *   `/v1/auth/sessions/current` (`X-Safi-Installation-Id`), and `DELETE /v1/auth/sessions/current`.
 * - Spawns real Rust `sf-desktop` (`studio_ipc_harness`) over stdio JSON-RPC for `DataGraph`,
 *   `rust_decimal` calculation, atomic bundle persistence, and `.safi.html` export.
 * - Bridges Chromium's `window.__TAURI_INTERNALS__.invoke` into the subprocess harness rather than
 *   driving a packaged WebView2/WebKitGTK OS window via Tauri WebDriver.
 */
import assert from "node:assert/strict";
import { mkdir, readFile, stat, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { spawn, execFileSync } from "node:child_process";
import { createInterface } from "node:readline";
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
    "PLAYWRIGHT_MODULE_URL is not set and no fallback Playwright installation was found."
  );
}

const { chromium } = await loadPlaywright();

const artifactDir = resolve("artifacts/intro-visual");
const e2eWorkDir = resolve(artifactDir, "studio-e2e");
const bundlesDir = resolve(e2eWorkDir, "bundles");
await mkdir(bundlesDir, { recursive: true });

const csvPath = resolve(e2eWorkDir, "sales-e2e.csv");
const capsulePath = resolve(e2eWorkDir, "exported-e2e.safi.html");
await writeFile(csvPath, "region,amount\nCairo,100\nAlex,200\nCairo,50\n", "utf8");

console.log(
  "[0/10] Building Rust Studio IPC harness + real sf-server auth harness (Chromium UI + Real Rust StudioStore Subprocess Harness Integration Test)..."
);
execFileSync("cargo", ["build", "-p", "sf-desktop", "--example", "studio_ipc_harness"], {
  stdio: "inherit"
});
execFileSync("cargo", ["build", "-p", "sf-server", "--example", "auth_harness_server"], {
  stdio: "inherit"
});

const binExt = process.platform === "win32" ? ".exe" : "";
const harnessBin = resolve("target/debug/examples", `studio_ipc_harness${binExt}`);
const authServerBin = resolve("target/debug/examples", `auth_harness_server${binExt}`);

const authProc = spawn(authServerBin, [], {
  stdio: ["ignore", "pipe", "inherit"],
  env: {
    ...process.env,
    AUTH_HARNESS_ADDR: "127.0.0.1:0"
  }
});
const authRl = createInterface({ input: authProc.stdout });
const authAddr = await new Promise((resolveAddr, rejectAddr) => {
  const timer = setTimeout(() => rejectAddr(new Error("sf-server auth harness timeout")), 90_000);
  authRl.on("line", (line) => {
    const trimmed = line.trim();
    if (!trimmed) return;
    const msg = JSON.parse(trimmed);
    if (msg.ready && msg.addr) {
      clearTimeout(timer);
      resolveAddr(String(msg.addr));
    }
  });
  authProc.once("exit", (code) => {
    clearTimeout(timer);
    rejectAddr(new Error(`sf-server auth harness exited unexpectedly with code ${code}`));
  });
});

const rustProc = spawn(harnessBin, [], {
  stdio: ["pipe", "pipe", "inherit"],
  env: {
    ...process.env,
    HARNESS_STORAGE_DIR: bundlesDir,
    HARNESS_CSV_PATH: csvPath,
    HARNESS_CAPSULE_PATH: capsulePath
  }
});

const rl = createInterface({ input: rustProc.stdout });
const pendingRpc = new Map();
let rpcSeq = 0;
let harnessReadyResolve;
const harnessReady = new Promise((res) => {
  harnessReadyResolve = res;
});

rl.on("line", (line) => {
  const trimmed = line.trim();
  if (!trimmed) return;
  const msg = JSON.parse(trimmed);
  if (msg.ready) {
    harnessReadyResolve();
    return;
  }
  const waiter = pendingRpc.get(msg.id);
  if (waiter) {
    pendingRpc.delete(msg.id);
    if (msg.ok) {
      waiter.resolve(msg.result);
    } else {
      waiter.reject(new Error(String(msg.error)));
    }
  }
});

function callRust(command, args = {}) {
  rpcSeq += 1;
  const id = rpcSeq;
  return new Promise((resolvePromise, rejectPromise) => {
    pendingRpc.set(id, { resolve: resolvePromise, reject: rejectPromise });
    rustProc.stdin.write(`${JSON.stringify({ id, command, args })}\n`);
  });
}

await harnessReady;

const baseUrl = "http://127.0.0.1:4175";
const server = spawn(
  process.execPath,
  [
    resolve("apps/web/node_modules/vite/bin/vite.js"),
    "--host",
    "127.0.0.1",
    "--port",
    "4175",
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

async function waitForServer() {
  for (let attempt = 0; attempt < 120; attempt += 1) {
    if (server.exitCode !== null) {
      throw new Error(`Vite dev server exited early with code ${server.exitCode}.`);
    }
    try {
      const response = await fetch(baseUrl);
      if (response.ok) return;
    } catch {
      // Waiting for Vite dev server to bind port 4175.
    }
    await delay(250);
  }
  throw new Error("Vite dev server did not become ready within 30 seconds.");
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
      // ignore JSON parse errors
    }
    throw new Error(code);
  }
  return await resp.json();
}

let browser;
let activeSessionToken = null;
let activeInstallationId = null;

try {
  await waitForServer();
  browser = await chromium.launch({ headless: true, args: ["--no-sandbox"] });
  const context = await browser.newContext({
    viewport: { width: 1440, height: 960 },
    deviceScaleFactor: 1
  });
  const page = await context.newPage();

  let nextOpenFileOverride = null;
  await page.exposeFunction("__safiSetNextOpenFile", (targetPath) => {
    nextOpenFileOverride = targetPath;
  });

  await page.exposeFunction("__safiRustInvoke", async (command, args) => {
    const safeArgs = args ?? {};
    if (command === "pick_input_file" && nextOpenFileOverride) {
      const chosen = nextOpenFileOverride;
      nextOpenFileOverride = null;
      await callRust("pick_input_file", safeArgs);
      return JSON.stringify(chosen);
    }
    if (command === "studio_set_auth_session") {
      const token = String(safeArgs.token ?? "");
      const installationId = String(safeArgs.installationId ?? safeArgs.installation_id ?? "");
      const verified = await verifySessionAgainstRealServer(token, installationId);
      activeSessionToken = token;
      activeInstallationId = installationId;
      await callRust("__harness_register_session", {
        token,
        sessionId: String(verified.id),
        workspaceId: String(verified.workspace_id),
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

  // 1. Open app in locked pre-authentication state and verify native Rust auth gate rejects unauthenticated calls
  console.log("[1/10] Opening application in locked state before authentication...");
  await page.goto(baseUrl, { waitUntil: "domcontentloaded" });
  await page.locator(".safi-intro-skip").click();
  await page.locator(".safi-auth-modal").waitFor({ state: "visible" });
  assert.equal(
    await page.locator("#safi-main").count(),
    0,
    "Workspace #safi-main must not mount before authentication."
  );

  const preAuthReject = await page.evaluate(async (targetCsv) => {
    try {
      await window.__TAURI_INTERNALS__.invoke("studio_open_file", {
        project: "main",
        path: targetCsv
      });
      return "ALLOWED";
    } catch (err) {
      return err instanceof Error ? err.message : String(err);
    }
  }, csvPath);
  assert.ok(
    preAuthReject.includes("SF-AUTH-001"),
    `Expected studio_open_file before login to fail with SF-AUTH-001, got: ${preAuthReject}`
  );

  const forgedTokenReject = await page.evaluate(async () => {
    try {
      await window.__TAURI_INTERNALS__.invoke("studio_set_auth_session", {
        token: `sfs_${"f".repeat(64)}`,
        installationId: "018f4b2a-1111-7000-8000-000000000001"
      });
      return "ALLOWED";
    } catch (err) {
      return err instanceof Error ? err.message : String(err);
    }
  });
  assert.ok(
    forgedTokenReject.includes("SF-AUTH-001"),
    `Expected forged token to be rejected by Rust verifier with SF-AUTH-001, got: ${forgedTokenReject}`
  );

  // 2. Sign in with a valid session
  console.log("[2/10] Signing in through AuthModal and binding native Studio session...");
  await page.locator('.safi-auth-modal input[type="email"]').fill("analyst@safidata.com");
  await page.locator('.safi-auth-modal input[type="password"]').fill("SafiAnalyst#2026");
  await page.locator('.safi-auth-modal button[type="submit"]').click();
  await page.locator("#safi-main").waitFor({ state: "visible", timeout: 15_000 });

  // Ensure native session is bound in StudioStore (in case async IndexedDB promise is still settling)
  await page.waitForFunction(
    async () => {
      try {
        await window.__TAURI_INTERNALS__.invoke("studio_list_templates", {});
        return true;
      } catch {
        return false;
      }
    },
    undefined,
    { timeout: 10_000 }
  );

  // 3. Open Studio after authentication
  console.log("[3/10] Opening Studio workspace from the desktop dock...");
  await page.locator('button[data-desktop-studio-trigger="true"]').click();
  await page.locator(".safi-desktop-studio").waitFor({ state: "visible" });
  assert.equal(
    await page.locator(".safi-desktop-studio--locked").count(),
    0,
    "Studio must be unlocked inside desktop runtime after authentication."
  );

  // 4. Import CSV file
  console.log("[4/10] Importing CSV data into Rust DataGraph...");
  await page
    .locator(".safi-desktop-studio__toolbar button")
    .filter({ hasText: /Open a file|افتح ملفاً/ })
    .click();
  await page.locator(".safi-desktop-studio__table").waitFor({ state: "visible" });
  await page.waitForFunction(() => {
    const text = document.querySelector(".safi-desktop-studio")?.textContent ?? "";
    return text.includes("Cairo") && text.includes("Alex");
  });

  // 5. Define metric, edit a cell in Sheet, and verify VerifiedNumberChip updates
  console.log("[5/10] Defining SUM(amount) metric, editing cell r1 amount (100 -> 250), and verifying chip update...");
  await page
    .locator(".safi-desktop-studio button[type='submit']")
    .filter({ hasText: /Add metric|أضف المؤشر/ })
    .click();
  await page.waitForFunction(() => {
    const chip = document.querySelector(".safi-desktop-chip");
    return (
      chip?.getAttribute("data-status") === "Verified" &&
      (chip.textContent ?? "").includes("350")
    );
  });

  const cellInput = page.locator('input[aria-label="amount r1"]');
  await cellInput.fill("250");
  await cellInput.press("Enter");
  await page.waitForFunction(() => {
    const chip = document.querySelector(".safi-desktop-chip");
    return (
      chip?.getAttribute("data-status") === "Verified" &&
      (chip.textContent ?? "").includes("500")
    );
  });

  // 6. Run What-If scenario and update metric
  console.log("[6/10] Defining What-If parameter, scenario-linked metric, and activating scenario...");
  await page
    .locator('section[aria-label="What-If Scenarios"] button[type="submit"]')
    .filter({ hasText: /Define parameter|أضف المتغير/ })
    .click();

  // Update metric formula to SUM(amount) * growth
  const formulaInput = page.locator('input[placeholder="SUM(amount) / COUNT(region)"]');
  await formulaInput.fill("SUM(amount) * growth");
  await page
    .locator(".safi-desktop-studio button[type='submit']")
    .filter({ hasText: /Add metric|أضف المؤشر/ })
    .click();

  // Wait for parameter slider to appear, then create & activate scenario "optimistic" and set growth = 2
  await page.locator('input[aria-label="growth value"]').waitFor({ state: "visible" });
  await page
    .locator('section[aria-label="What-If Scenarios"] button[type="submit"]')
    .filter({ hasText: /Create scenario|أنشئ سيناريو/ })
    .click();
  await page
    .locator(".safi-desktop-studio__scenario-list button")
    .filter({ hasText: /Optimistic Q4/ })
    .click();
  await page.waitForFunction(() => {
    const panel = document.querySelector('section[aria-label="What-If Scenarios"]');
    return (panel?.textContent ?? "").includes("Active scenario: Optimistic Q4");
  });
  const growthValueInput = page.locator('input[aria-label="growth value"]');
  await growthValueInput.fill("2");
  await page.waitForFunction(() => {
    const chip = document.querySelector(".safi-desktop-chip");
    return (
      chip?.getAttribute("data-status") === "Verified" &&
      (chip.textContent ?? "").includes("1000")
    );
  });

  // 7. Create an element in every Quad-Studio surface (Dashboard Chart, Slide, Document Block, Free-Form Block)
  console.log("[7/10] Creating artifacts in Dashboard, Slides, Doc, and Free-Form Canvas...");
  // Dashboard
  await page.locator('button[role="tab"]').filter({ hasText: /Dashboard/ }).click();
  await page.locator('[data-testid="dashboard-charts-view"]').waitFor({ state: "visible" });
  await page.locator('select[aria-label="Chart Type"]').selectOption("line");
  await page
    .locator('[data-testid="dashboard-charts-view"] button')
    .filter({ hasText: /Add Chart/ })
    .click();
  await page
    .locator('[data-testid="dashboard-charts-view"] [data-chart-type="line"]')
    .waitFor({ state: "visible" });

  // Slides
  await page.locator('button[role="tab"]').filter({ hasText: /Slides/ }).click();
  await page.locator('[data-testid="keynote-slides-view"]').waitFor({ state: "visible" });
  await page
    .locator('[data-testid="keynote-slides-view"] button')
    .filter({ hasText: /Add Slide/ })
    .click();
  await page.waitForFunction(() => {
    const el = document.querySelector('[data-testid="keynote-slides-view"]');
    return (el?.textContent ?? "").includes("1000 EGP");
  });

  // Doc
  await page.locator('button[role="tab"]').filter({ hasText: /Doc/ }).click();
  await page.locator('[data-testid="verified-doc-view"]').waitFor({ state: "visible" });
  await page
    .locator('[data-testid="verified-doc-view"] button')
    .filter({ hasText: /Paragraph/ })
    .click();
  await page.waitForFunction(() => {
    const el = document.querySelector('[data-testid="verified-doc-view"]');
    return (el?.textContent ?? "").includes("1000 EGP");
  });

  // Free-Form Canvas (including +H rowSpan height resize)
  await page.locator('button[role="tab"]').filter({ hasText: /Canvas/ }).click();
  await page.locator('[data-testid="freeform-builder-view"]').waitFor({ state: "visible" });
  await page
    .locator('[data-testid="freeform-builder-view"] button')
    .filter({ hasText: /KPI Card/ })
    .click();
  await page.locator('button[aria-label="Increase height"]').click();
  await page.waitForFunction(() => {
    const el = document.querySelector('[data-testid="freeform-builder-view"]');
    return (el?.textContent ?? "").includes("H:3");
  });

  // 8. Save the project bundle
  console.log("[8/10] Saving project bundle atomically with BLAKE3 checksum...");
  await page.locator('button[role="tab"]').filter({ hasText: /Templates & Projects/ }).click();
  await page.locator('[data-testid="templates-projects-panel"]').waitFor({ state: "visible" });
  await page
    .locator('[data-testid="templates-projects-panel"] button')
    .filter({ hasText: /Save Project/ })
    .click();
  await page.waitForFunction(() => {
    const el = document.querySelector('[data-testid="templates-projects-panel"]');
    return (el?.textContent ?? "").includes("BLAKE3 OK");
  });

  // 9. Reset in-memory StudioStore and reload saved project bundle
  console.log("[9/10] Resetting in-memory StudioStore and reloading saved project bundle...");
  await callRust("__harness_reset_store_keep_bundles", {});
  // Verify that after store reset (before re-binding session), commands fail with SF-AUTH-001
  const afterResetUnauth = await page.evaluate(async () => {
    try {
      await window.__TAURI_INTERNALS__.invoke("studio_state", { project: "main" });
      return "ALLOWED";
    } catch (err) {
      return err instanceof Error ? err.message : String(err);
    }
  });
  assert.ok(
    afterResetUnauth.includes("SF-AUTH-001"),
    `Expected studio_state after store reset to require re-authentication, got: ${afterResetUnauth}`
  );

  // Re-bind the authenticated session and click Open on the saved bundle
  await callRust("studio_set_auth_session", {
    token: activeSessionToken,
    installationId: activeInstallationId
  });
  await page
    .locator('[data-testid="templates-projects-panel"] button')
    .filter({ hasText: /^Open$/ })
    .first()
    .click();

  await page.waitForFunction(() => {
    const text = document.querySelector(".safi-desktop-studio")?.textContent ?? "";
    return (
      text.includes("Dashboard (1)") &&
      text.includes("Slides (1)") &&
      text.includes("Doc (1)") &&
      text.includes("Canvas (1)") &&
      text.includes("1000 EGP")
    );
  });

  // Switch back to Canvas to confirm H:3 (rowSpan=3) persisted
  await page.locator('button[role="tab"]').filter({ hasText: /Canvas/ }).click();
  await page.waitForFunction(() => {
    const el = document.querySelector('[data-testid="freeform-builder-view"]');
    return (el?.textContent ?? "").includes("H:3");
  });

  // 10. Export .safi.html capsule and verify sign-out revokes native access
  console.log("[10/10] Exporting .safi.html capsule and verifying sign-out revokes native access...");
  await page
    .locator(".safi-desktop-studio__toolbar button")
    .filter({ hasText: /Export capsule \(\.safi\.html\)/ })
    .click();

  let exportedStat = null;
  for (let attempt = 0; attempt < 50; attempt += 1) {
    try {
      const candidate = await stat(capsulePath);
      if (candidate.size > 2048) {
        exportedStat = candidate;
        break;
      }
    } catch {
      // Wait for async pick_capsule_output_file + studio_export_capsule IPC to finish writing
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  if (!exportedStat) {
    const errText = await page
      .locator(".safi-desktop-studio__error")
      .textContent()
      .catch(() => null);
    throw new Error(`Capsule file was not written at ${capsulePath}. UI error: ${errText}`);
  }
  assert.ok(exportedStat.size > 2048, "Exported .safi.html capsule must be > 2 KiB.");
  const exportedHtml = await readFile(capsulePath, "utf8");
  assert.ok(!exportedHtml.includes("<script"), "Exported capsule must contain zero <script> tags.");
  assert.ok(
    exportedHtml.includes('data-chart-type="line"'),
    "Exported capsule must contain the saved line chart."
  );
  assert.ok(
    exportedHtml.includes('data-grid-row-span="3"'),
    "Exported capsule must preserve Free-Form rowSpan=3."
  );
  assert.ok(
    exportedHtml.includes("1000") && exportedHtml.includes("EGP"),
    `Exported capsule must contain the verified What-If metric value 1000 EGP. Tail: ${exportedHtml.slice(-1200)}`
  );

  // 11. Universal Multi-Sheet XLSX Ingestion & Region Switching (messy_enterprise.xlsx)
  console.log("[11/14] Opening multi-sheet messy_enterprise.xlsx, inspecting hidden sheets & switching regions...");
  const messyXlsxPath = resolve("tests/fixtures/workbooks/messy_enterprise.xlsx");
  await page.evaluate((p) => window.__safiSetNextOpenFile(p), messyXlsxPath);
  await page.locator('[data-tutorial-target="open-workbook"]').click();
  await page.locator('[data-testid="workbook-intelligence-drawer"]').waitFor({ state: "visible", timeout: 15_000 });
  await page.waitForFunction(() => {
    const drawer = document.querySelector('[data-testid="workbook-intelligence-drawer"]');
    return (drawer?.textContent ?? "").includes("Sales & KPIs") && (drawer?.textContent ?? "").includes("Show Hidden (2)");
  });

  // Reveal hidden/veryHidden tabs ("Lookup_Rates", "Secret_Params")
  await page.locator('[data-testid="toggle-hidden-sheets-btn"]').click();
  await page.waitForFunction(() => {
    const drawer = document.querySelector('[data-testid="workbook-intelligence-drawer"]');
    const t = drawer?.textContent ?? "";
    return t.includes("Lookup_Rates") && t.includes("Secret_Params");
  });

  // Switch back to Sheet tab so paginated grid assertions match live rows
  await page.locator('[aria-label="Quad-Studio workspaces"] button[role="tab"]').first().click();

  // Switch active table on Sales & KPIs from tbl_1 -> tbl_2 ("Target Name", "North", "South")
  await page.locator('[data-testid="activate-region-0-tbl_2"]').click();
  try {
    await page.waitForFunction(() => {
      const tbl = document.querySelector(".safi-desktop-studio__table");
      if (!tbl) return false;
      const headers = tbl.textContent ?? "";
      const vals = Array.from(tbl.querySelectorAll("input")).map((i) => i.value);
      return headers.includes("Target Name") && vals.includes("North") && vals.includes("South");
    }, undefined, { timeout: 15_000 });
  } catch (err) {
    const diag = await page.evaluate(() => ({
      err: document.querySelector(".safi-desktop-studio__error")?.textContent ?? null,
      headers: document.querySelector(".safi-desktop-studio__table thead")?.textContent ?? null,
      vals: Array.from(document.querySelectorAll(".safi-desktop-studio__table input")).map((i) => i.value),
      activeTab: document.querySelector('[aria-label="Quad-Studio workspaces"] [aria-selected="true"]')?.textContent ?? null
    }));
    throw new Error(`Region switch tbl_2 failed (${err instanceof Error ? err.message : err}): ${JSON.stringify(diag)}`);
  }

  // Switch back to tbl_1 ("القاهرة", "00105")
  await page.locator('[data-testid="activate-region-0-tbl_1"]').click();
  await page.waitForFunction(() => {
    const tbl = document.querySelector(".safi-desktop-studio__table");
    if (!tbl) return false;
    const vals = Array.from(tbl.querySelectorAll("input")).map((i) => i.value);
    return vals.includes("القاهرة") && vals.includes("00105");
  });

  // 12. Intelligence, Epistemic Separation & Grounded Assistant
  console.log("[12/14] Testing Contextual Intelligence & Grounded Data Assistant...");
  await page.locator('[data-testid="tab-intelligence"]').click();
  await page.locator('[data-testid="grounded-assistant-panel"]').waitFor({ state: "visible", timeout: 15_000 });
  await page.locator('[data-testid="privacy-consent-panel"]').waitFor({ state: "visible" });
  await page.locator('[data-testid="assistant-question-input"]').fill("What does this workbook contain?");
  await page.locator('[data-testid="assistant-ask-btn"]').click();
  await page.locator('[data-testid="assistant-answer-0"]').waitFor({ state: "visible" });
  const assistantText = await page.locator('[data-testid="assistant-answer-0"]').textContent();
  assert.ok(assistantText && assistantText.includes("Sales & KPIs"), `Expected citation to Sales & KPIs, got: ${assistantText}`);

  // 13. Add Waterfall + Bar charts, edit inline config, and verify live Cross-Filter across sibling charts & sheet rows
  console.log("[13/14] Creating Waterfall + Bar charts, editing card config, and verifying cross-filter...");
  await page.locator('[data-tutorial-target="dashboard-tab"]').click();
  await page.locator('[data-testid="dashboard-charts-view"]').waitFor({ state: "visible" });
  await page.locator('select[aria-label="Chart Type"]').selectOption("waterfall");
  await page.locator('[data-testid="dashboard-charts-view"] label:has-text("Value Column") select').selectOption("Financials — Revenue");
  await page.locator('[data-testid="dashboard-charts-view"] label:has-text("Group By / X Axis") select').selectOption("Account Info — Region");
  await page
    .locator('[data-testid="dashboard-charts-view"] button')
    .filter({ hasText: /Add Chart/ })
    .click();
  await page
    .locator('[data-testid="dashboard-charts-view"] [data-chart-type="waterfall"]')
    .waitFor({ state: "visible" });

  // Add second sibling Bar chart (Units by Region) so cross-filter must update it in Rust
  await page.locator('select[aria-label="Chart Type"]').selectOption("bar");
  await page.locator('[data-testid="dashboard-charts-view"] label:has-text("Value Column") select').selectOption("Financials — Units");
  await page
    .locator('[data-testid="dashboard-charts-view"] button')
    .filter({ hasText: /Add Chart/ })
    .click();
  const barCard = page.locator('[data-testid="dashboard-charts-view"] [data-chart-type="bar"]').last();
  await barCard.waitFor({ state: "visible" });

  // Inline edit the waterfall card title + aggregation
  const wfCard = page.locator('[data-testid="dashboard-charts-view"] [data-chart-type="waterfall"]').first();
  const wfChartId = await wfCard.getAttribute("data-chart-id");
  assert.ok(wfChartId, "Waterfall card must expose data-chart-id");
  await page.locator(`[data-testid="edit-chart-${wfChartId}"]`).click();
  await page.locator(`[data-testid="chart-editor-${wfChartId}"] input[aria-label="Edit chart title"]`).fill("Edited Waterfall E2E");
  await page.locator(`[data-testid="chart-editor-${wfChartId}"] select[aria-label="Edit aggregation"]`).selectOption("AVG");
  await page.locator(`[data-testid="edit-chart-${wfChartId}"]`).click();
  await page.waitForFunction(() => (document.querySelector('[data-testid="dashboard-charts-view"]')?.textContent ?? "").includes("Edited Waterfall E2E"));

  // Cross-filter from the bar card ("القاهرة") -> verify sibling waterfall marks data-cross-filtered="true"
  await barCard.locator(".studio-bar-btn").filter({ hasText: "القاهرة" }).click();
  await page.waitForFunction(
    (id) => document.querySelector(`[data-chart-id="${id}"]`)?.getAttribute("data-cross-filtered") === "true",
    wfChartId
  );
  // Also verify filtered sheet rows when clicking "View sheet rows"
  await page.locator('[data-testid="view-filtered-sheet-btn"]').click();
  await page.waitForFunction(() => {
    const vals = Array.from(document.querySelectorAll(".safi-desktop-studio__table input")).map((i) => i.value);
    return vals.includes("القاهرة") && !vals.includes("أسوان");
  });
  await page.locator('[data-tutorial-target="dashboard-tab"]').click();
  await page.locator('[data-testid="clear-cross-filter-btn"]').click();

  // 13b. Real Microsoft Excel Binary (.xlsb) Desktop End-to-End Ingestion
  console.log("[13b/14] Opening genuine Microsoft Excel binary workbook (enterprise_binary.xlsb)...");
  const binaryXlsbPath = resolve("tests/fixtures/workbooks/enterprise_binary.xlsb");
  await page.evaluate((p) => window.__safiSetNextOpenFile(p), binaryXlsbPath);
  await page.locator('[data-tutorial-target="open-workbook"]').click();
  await page.waitForFunction(() => {
    const drawer = document.querySelector('[data-testid="workbook-intelligence-drawer"]');
    return Boolean(drawer && (drawer.textContent ?? "").includes("Sales & KPIs"));
  });
  await page.locator('[aria-label="Quad-Studio workspaces"] button[role="tab"]').first().click();
  await page.waitForFunction(() => {
    const vals = Array.from(document.querySelectorAll(".safi-desktop-studio__table input")).map((i) => i.value);
    return vals.includes("00105") && vals.includes("القاهرة");
  });
  await page.locator('[data-tutorial-target="dashboard-tab"]').click();

  // 14. Export Real Binary Multi-Sheet .xlsx and Multi-Page .pdf Deliverables via Rust Atomic Writer
  console.log("[14/14] Exporting atomic .xlsx and .pdf deliverables via DeliverableExportModal...");
  const deliverableXlsxPath = resolve(e2eWorkDir, "safi_deliverable.xlsx");
  const deliverablePdfPath = resolve(e2eWorkDir, "safi_deliverable.pdf");
  await page.locator('[data-tutorial-target="export-deliverable-btn"]').click();
  await page.locator('[data-testid="deliverable-export-modal"]').waitFor({ state: "visible" });

  await page.locator('[data-testid="export-format-select"]').selectOption("xlsx");
  await page.locator('[data-testid="confirm-export-deliverable-btn"]').click();
  await page.waitForFunction(() => (document.querySelector('[data-testid="export-deliverable-status"]')?.textContent ?? "").includes(".xlsx"));
  const xlsxBytes = await readFile(deliverableXlsxPath);
  assert.ok(xlsxBytes.length > 3000 && xlsxBytes[0] === 0x50 && xlsxBytes[1] === 0x4b, "Exported .xlsx must be a valid OOXML ZIP binary.");

  await page.locator('[data-testid="export-format-select"]').selectOption("pdf");
  await page.locator('[data-testid="confirm-export-deliverable-btn"]').click();
  await page.waitForFunction(() => (document.querySelector('[data-testid="export-deliverable-status"]')?.textContent ?? "").includes(".pdf"));
  const pdfBytes = await readFile(deliverablePdfPath);
  const pdfAscii = pdfBytes.toString("latin1");
  assert.ok(pdfAscii.startsWith("%PDF-1.7") && pdfAscii.includes("xref") && pdfAscii.trim().endsWith("%%EOF"), "Exported .pdf must be valid PDF 1.7.");

  await page.locator('[data-testid="deliverable-export-modal"] button[aria-label="Close"]').click();

  // Clear native session (sign-out via real DELETE /v1/auth/sessions/current + native clear) and verify immediate rejection
  const remoteRevokeResp = await fetch(`http://${authAddr}/v1/auth/sessions/current`, {
    method: "DELETE",
    headers: {
      Authorization: `Bearer ${activeSessionToken}`,
      "X-Safi-Installation-Id": activeInstallationId
    }
  });
  assert.equal(
    remoteRevokeResp.status,
    204,
    `Expected real sf-server DELETE /v1/auth/sessions/current to return 204, got ${remoteRevokeResp.status}`
  );
  await page.evaluate(async () => {
    await window.__TAURI_INTERNALS__.invoke("studio_clear_auth_session", {});
  });
  await assert.rejects(
    verifySessionAgainstRealServer(activeSessionToken, activeInstallationId),
    /SF-AUTH-003/,
    "Expected revoked session on sf-server to fail with SF-AUTH-003"
  );
  const afterLogoutReject = await page.evaluate(async () => {
    try {
      await window.__TAURI_INTERNALS__.invoke("studio_state", { project: "main" });
      return "ALLOWED";
    } catch (err) {
      return err instanceof Error ? err.message : String(err);
    }
  });
  assert.ok(
    afterLogoutReject.includes("SF-AUTH-001"),
    `Expected studio_state after logout to fail with SF-AUTH-001, got: ${afterLogoutReject}`
  );

  const screenshotPath = resolve(artifactDir, "studio-desktop-e2e.png");
  await page.screenshot({ path: screenshotPath, fullPage: true });
  console.log(
    "Chromium UI + Real sf-server Auth + Real Rust StudioStore Subprocess Harness Integration Test passed (10 steps):",
    JSON.stringify({
      preAuthReject: "SF-AUTH-001",
      forgedTokenReject: "SF-AUTH-001",
      realAuthServerAddr: authAddr,
      initialMetric: "350 EGP",
      afterCellEditMetric: "500 EGP",
      afterWhatIfMetric: "1000 EGP",
      surfacesCreated: ["dashboard:line", "slides:1", "doc:1", "freeform:H:3"],
      bundleSavedAndRestored: true,
      capsuleExportedBytes: exportedStat.size,
      afterLogoutReject: "SF-AUTH-001",
      screenshotPath
    })
  );

  await context.close();
} finally {
  if (browser) await browser.close();
  server.kill("SIGTERM");
  rustProc.kill("SIGTERM");
  authProc.kill("SIGTERM");
}
