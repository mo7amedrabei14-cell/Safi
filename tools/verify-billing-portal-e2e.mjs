import { spawn } from "node:child_process";
import assert from "node:assert/strict";

const playwrightUrl = process.env.PLAYWRIGHT_MODULE_URL;
if (!playwrightUrl) {
  throw new Error("PLAYWRIGHT_MODULE_URL is required");
}
const { chromium } = await import(playwrightUrl);

const PORT = 4179;
const BASE_URL = `http://127.0.0.1:${PORT}`;

const server = spawn("pnpm", ["--dir", "apps/web", "exec", "vite", "preview", "--host", "127.0.0.1", "--port", String(PORT), "--strictPort"], {
  stdio: ["ignore", "pipe", "pipe"],
  shell: process.platform === "win32"
});

await new Promise((resolve, reject) => {
  const timer = setTimeout(() => reject(new Error("vite preview startup timeout")), 20000);
  server.stdout.on("data", (chunk) => {
    if (String(chunk).includes(String(PORT))) {
      clearTimeout(timer);
      resolve();
    }
  });
  server.stderr.on("data", (chunk) => {
    if (String(chunk).includes(String(PORT))) {
      clearTimeout(timer);
      resolve();
    }
  });
  server.on("exit", (code) => {
    clearTimeout(timer);
    reject(new Error(`vite preview exited prematurely with code ${code}`));
  });
});

let browser;
try {
  browser = await chromium.launch({ headless: true });
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page = await context.newPage();

  // Stub window.open so hosted provider redirect does not spawn an uncontrolled popup.
  await page.addInitScript(() => {
    window.__SAFI_OPENED_URLS__ = [];
    window.open = (url) => {
      window.__SAFI_OPENED_URLS__.push(String(url));
      return null;
    };
  });

  let checkoutCalls = 0;
  let webhookPaid = false;
  let lastIdempotencyKey = null;
  let redeemedHandoff = false;

  await page.route("**/v1/**", async (route) => {
    const url = route.request().url();
    const method = route.request().method();

    if (url.endsWith("/v1/billing/web/login") && method === "POST") {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({
          token: "sfb_live_chromium_billing_session_001",
          session_id: "01950000-0000-7000-8000-000000000001",
          user_id: "01950000-0000-7000-8000-000000000002",
          workspace_id: "01950000-0000-7000-8000-000000000003",
          email: "buyer@safidata.test",
          session_type: "billing_web",
          expires_at: 2_000_000_000
        })
      });
      return;
    }

    if (url.endsWith("/v1/billing/handoff/redeem") && method === "POST") {
      const body = route.request().postDataJSON();
      assert.equal(body.handoff_code, "sfbh_chromium_handoff_1234567890");
      assert.equal(body.state, "csrf_chromium_state_1234567890");
      redeemedHandoff = true;
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({
          handoff_id: "01950000-0000-7000-8000-000000000011",
          user_id: "01950000-0000-7000-8000-000000000002",
          workspace_id: "01950000-0000-7000-8000-000000000003",
          product: "organization_seat_annual",
          seats: 5,
          billing_country_iso2: "US",
          coupon_code: null,
          provider: "paddle",
          currency: "USD",
          original_amount_minor: 102000,
          billing_period: "annual"
        })
      });
      return;
    }

    if (url.endsWith("/v1/billing/checkout") && method === "POST") {
      checkoutCalls += 1;
      const body = route.request().postDataJSON();
      lastIdempotencyKey = body.idempotency_key;
      assert.equal(body.product, "organization_seat_annual");
      assert.equal(body.seats, 5);
      await new Promise((r) => setTimeout(r, 120));
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({
          order_id: "01950000-0000-7000-8000-000000000099",
          idempotency_key: body.idempotency_key,
          order_state: "checkout_pending",
          reused_existing_order: checkoutCalls > 1,
          provider: "paddle",
          currency: "USD",
          original_amount_minor: 102000,
          discount_amount_minor: 0,
          amount_minor: 102000,
          applied_coupon_code: null,
          provider_status: "ready",
          provider_order_ref: "txn_chromium_001",
          checkout_session_id: "txn_chromium_001",
          checkout_mode: "hosted_redirect",
          checkout_url: "https://sandbox-checkout.paddle.com/txn_chromium_001"
        })
      });
      return;
    }

    if (url.includes("/v1/billing/orders/") && method === "GET") {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({
          order_id: "01950000-0000-7000-8000-000000000099",
          order_state: webhookPaid ? "paid" : "checkout_pending",
          provider: "paddle",
          product: "organization_seat_annual",
          seats: 5,
          currency: "USD",
          original_amount_minor: 102000,
          discount_amount_minor: 0,
          expected_amount_minor: 102000,
          checkout_url: "https://sandbox-checkout.paddle.com/txn_chromium_001",
          paid_at: webhookPaid ? 1_900_000_000 : null,
          updated_at: 1_900_000_000
        })
      });
      return;
    }

    if (url.endsWith("/v1/entitlements/me") && method === "GET") {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({
          effective_plan: webhookPaid ? "organization" : "free",
          state: webhookPaid ? "active" : "free",
          operation_credits: 0,
          pay_per_file_tokens: 0,
          seats: webhookPaid ? 5 : 1
        })
      });
      return;
    }

    await route.continue();
  });

  // 1. Navigate to /billing with single-use handoff parameters.
  await page.goto(
    `${BASE_URL}/billing?handoff=sfbh_chromium_handoff_1234567890&state=csrf_chromium_state_1234567890`,
    { waitUntil: "networkidle" }
  );

  // 2. Verify URL immediately stripped handoff & state via history.replaceState.
  await page.waitForSelector('[data-billing-portal="true"]');
  const sanitizedUrl = page.url();
  assert.equal(sanitizedUrl, `${BASE_URL}/billing`);

  // 3. Verify unauthenticated auth gate shown, zero Desktop Studio leaked.
  await page.waitForSelector('[data-billing-auth-gate="true"]');
  assert.equal(await page.locator('[data-tab-id="studio"]').count(), 0);
  assert.equal(await page.locator('[data-tab-id="owner"]').count(), 0);

  // 4. Sign in via Website Billing Login -> auto-redeems handoff!
  await page.fill('[data-billing-login-email="true"]', "buyer@safidata.test");
  await page.fill('[data-billing-login-password="true"]', "SafiStrongPassword!1");
  await page.click('[data-billing-login-submit="true"]');

  await page.waitForSelector('[data-billing-authenticated="true"]');
  await page.waitForFunction(() => {
    const el = document.querySelector('[data-billing-final-price="true"]');
    return el && el.textContent && el.textContent.includes("$1,020");
  });
  assert.equal(redeemedHandoff, true);

  // 5. Verify all 5 products rendered and ZERO credit-card inputs exist.
  for (const prod of [
    "individual_monthly",
    "individual_annual",
    "organization_seat_annual",
    "credit_pack_25",
    "pay_per_file"
  ]) {
    assert.equal(await page.locator(`[data-billing-plan-option="${prod}"]`).count(), 1);
  }
  assert.equal(await page.locator('input[autocomplete="cc-number"]').count(), 0);
  assert.equal(await page.locator('input[autocomplete="cc-csc"]').count(), 0);

  // 6. Double-click Continue to Secure Checkout -> strictly ONE POST /v1/billing/checkout!
  await page.waitForSelector('[data-billing-pay-submit="true"]');
  await page.evaluate(() => {
    const btn = document.querySelector('[data-billing-pay-submit="true"]');
    if (!(btn instanceof HTMLButtonElement)) throw new Error("pay button missing");
    btn.click();
    btn.click();
  });
  await page.waitForSelector('[data-billing-state="checkout_pending"]');
  assert.equal(checkoutCalls, 1);
  assert.ok(lastIdempotencyKey && lastIdempotencyKey.length >= 16);

  const openedUrls = await page.evaluate(() => window.__SAFI_OPENED_URLS__);
  assert.deepEqual(openedUrls, ["https://sandbox-checkout.paddle.com/txn_chromium_001"]);

  // 7. Click Verify Payment Status BEFORE webhook -> remains pending, NEVER falsely paid!
  await page.click('[data-billing-verify-status="true"]');
  await page.waitForTimeout(150);
  assert.equal(await page.locator('[data-billing-state="paid"]').count(), 0);

  // 8. Simulate verified server webhook -> click Verify Payment Status -> transitions to PAID + safi://billing return link!
  webhookPaid = true;
  await page.click('[data-billing-verify-status="true"]');
  await page.waitForSelector('[data-billing-state="paid"]');
  const returnHref = await page.locator('[data-billing-return-desktop="true"]').getAttribute("href");
  assert.equal(returnHref, "safi://billing?order_id=01950000-0000-7000-8000-000000000099");

  console.log(
    "VERIFY_BILLING_PORTAL_E2E_PASSED",
    JSON.stringify({
      sanitizedUrl,
      redeemedHandoff,
      checkoutCalls,
      openedUrls,
      returnHref
    })
  );
} finally {
  if (browser) await browser.close();
  if (process.platform === "win32" && server.pid) {
    spawn("taskkill", ["/pid", String(server.pid), "/T", "/F"], { stdio: "ignore" });
  } else {
    server.kill();
  }
}
process.exit(0);
