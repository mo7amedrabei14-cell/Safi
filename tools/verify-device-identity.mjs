import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { setTimeout as delay } from "node:timers/promises";

const moduleUrl = process.env.PLAYWRIGHT_MODULE_URL;
if (!moduleUrl) {
  throw new Error("PLAYWRIGHT_MODULE_URL must point to the pinned Playwright installation.");
}
const { chromium } = await import(moduleUrl);

const baseUrl = "http://127.0.0.1:4174";
const server = spawn(
  process.platform === "win32" ? "pnpm.cmd" : "pnpm",
  [
    "--filter",
    "@safi/web",
    "exec",
    "vite",
    "--host",
    "127.0.0.1",
    "--port",
    "4174",
    "--strictPort"
  ],
  { stdio: "inherit" }
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
      // The Vite dev server has not bound its port yet.
    }
    await delay(250);
  }
  throw new Error("Vite dev server did not become ready within 30 seconds.");
}

let browser;
try {
  await waitForServer();
  browser = await chromium.launch({ headless: true, args: ["--no-sandbox"] });
  const context = await browser.newContext();
  const page = await context.newPage();
  await page.goto(baseUrl, { waitUntil: "domcontentloaded" });

  await page.locator(".safi-auth-modal").waitFor({ state: "visible" });
  assert.equal(
    await page.locator(".safi-auth-close").count(),
    0,
    "The initial authentication gate must not expose a close button."
  );
  assert.equal(
    await page.locator(".safi-topbar").count(),
    0,
    "Product navigation must not mount before authentication."
  );
  assert.equal(
    await page.locator("#safi-main").count(),
    0,
    "The product workspace must not mount before authentication."
  );

  await page.keyboard.press("Escape");
  assert.equal(
    await page.locator(".safi-auth-modal").count(),
    1,
    "Escape must not dismiss the mandatory authentication gate."
  );

  const first = await page.evaluate(async () => {
    const request = indexedDB.deleteDatabase("safidata-installation");
    await new Promise((resolve, reject) => {
      request.onsuccess = () => resolve();
      request.onerror = () => reject(request.error);
      request.onblocked = () => reject(new Error("The installation database is blocked."));
    });
    const module = await import("/src/lib/device-identity.ts");
    return module.getSafiDeviceIdentity();
  });

  const secondPage = await context.newPage();
  await secondPage.goto(baseUrl, { waitUntil: "domcontentloaded" });
  const second = await secondPage.evaluate(async () => {
    const module = await import("/src/lib/device-identity.ts");
    return module.getSafiDeviceIdentity();
  });

  assert.match(
    first.installationId,
    /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i
  );
  assert.equal(first.platform, "web");
  assert.equal(second.installationId, first.installationId);
  assert.equal(second.platform, "web");

  const localStorageIdentity = await page.evaluate(() =>
    localStorage.getItem("installation_id")
  );
  assert.equal(localStorageIdentity, null, "The raw installation id must not use localStorage.");

  console.log(
    "Safi installation identity passed in real Chromium IndexedDB:",
    JSON.stringify({ platform: first.platform, stableAcrossPages: true, localStorageUsed: false })
  );
  await context.close();
} finally {
  if (browser) await browser.close();
  server.kill("SIGTERM");
}
