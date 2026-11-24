import { spawn } from "node:child_process";
import assert from "node:assert/strict";
import path from "node:path";

const playwrightUrl = process.env.PLAYWRIGHT_MODULE_URL;
if (!playwrightUrl) {
  throw new Error("PLAYWRIGHT_MODULE_URL is required");
}
const { chromium } = await import(playwrightUrl);

const PORT = 4179;
const BASE_URL = `http://127.0.0.1:${PORT}`;
const SCREENSHOT_DIR = process.env.SAFI_PRICING_SCREENSHOT_DIR || "";

const server = spawn(
  "pnpm",
  ["--dir", "apps/web", "exec", "vite", "preview", "--host", "127.0.0.1", "--port", String(PORT), "--strictPort"],
  {
    stdio: ["ignore", "pipe", "pipe"],
    shell: process.platform === "win32"
  }
);

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

  // =========================================================================
  // SUITE A: PUBLIC PRICING DISCOVERY, ANONYMOUS ACCESS, AR/EN & RESPONSIVE
  // =========================================================================
  const publicCtx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const publicPage = await publicCtx.newPage();

  await publicPage.goto(BASE_URL, { waitUntil: "networkidle" });
  await publicPage.waitForSelector('[data-nav-pricing="true"]');
  await publicPage.waitForSelector('[data-hero-pricing-cta="true"]');

  // Click Hero Plans & Pricing CTA -> navigates cleanly to /pricing without login!
  await publicPage.locator('[data-hero-pricing-cta="true"]').click();
  await publicPage.waitForSelector('[data-pricing-page="true"]');
  assert.equal(publicPage.url(), `${BASE_URL}/pricing`);
  assert.equal(await publicPage.locator('[data-billing-auth-gate="true"]').count(), 0);

  // Verify ALL 5 products + Free plan disclosed on Desktop English
  for (const card of [
    "individual_monthly",
    "individual_annual",
    "organization_seat_annual",
    "credit_pack_25",
    "pay_per_file",
    "free"
  ]) {
    assert.equal(await publicPage.locator(`[data-pricing-card="${card}"]`).count(), 1);
  }
  const pricingTextEn = await publicPage.locator('[data-pricing-page="true"]').innerText();
  for (const mustContain of [
    "$20",
    "1,000 EGP",
    "$168",
    "8,400 EGP",
    "$17",
    "850 EGP",
    "$5",
    "250 EGP",
    "$2",
    "50 EGP",
    "5 operations per month",
    "Maximum 5,000 rows",
    "Maximum 20 columns",
    "No AI",
    "No teams"
  ]) {
    assert.ok(pricingTextEn.includes(mustContain), `Missing on /pricing EN: ${mustContain}`);
  }

  const desktopOverflow = await publicPage.evaluate(
    () => document.documentElement.scrollWidth > window.innerWidth
  );
  assert.equal(desktopOverflow, false, "Desktop /pricing must have zero horizontal overflow");

  if (SCREENSHOT_DIR) {
    await publicPage.screenshot({
      path: path.join(SCREENSHOT_DIR, "safi-pricing-desktop-1440-en.png"),
      fullPage: true
    });
  }

  // Switch to Arabic RTL on /pricing and verify layout + screenshot
  await publicPage.locator('button[aria-label="Switch to Arabic"]').click();
  await publicPage.waitForFunction(() => document.documentElement.dir === "rtl");
  const pricingTextAr = await publicPage.locator('[data-pricing-page="true"]').innerText();
  assert.ok(pricingTextAr.includes("الأسعار والباقات المعتمدة"));
  assert.ok(pricingTextAr.includes("الحد الأدنى 3 مقاعد"));
  if (SCREENSHOT_DIR) {
    await publicPage.screenshot({
      path: path.join(SCREENSHOT_DIR, "safi-pricing-desktop-1440-ar.png"),
      fullPage: true
    });
  }

  // Mobile 390x844 check (Showcase nav CTA + /pricing responsive Arabic & English)
  const mobileCtx = await browser.newContext({ viewport: { width: 390, height: 844 } });
  const mobilePage = await mobileCtx.newPage();
  await mobilePage.goto(BASE_URL, { waitUntil: "networkidle" });
  await mobilePage.waitForSelector('[data-nav-pricing="true"]');
  await mobilePage.locator('[data-nav-pricing="true"]').click();
  await mobilePage.waitForSelector('[data-pricing-page="true"]');
  const mobileOverflow = await mobilePage.evaluate(
    () => document.documentElement.scrollWidth > window.innerWidth
  );
  assert.equal(mobileOverflow, false, "Mobile 390px /pricing must have zero horizontal overflow");
  if (SCREENSHOT_DIR) {
    await mobilePage.screenshot({
      path: path.join(SCREENSHOT_DIR, "safi-pricing-mobile-390-en.png"),
      fullPage: true
    });
    await mobilePage.locator('button[aria-label="Switch to Arabic"]').click();
    await mobilePage.waitForFunction(() => document.documentElement.dir === "rtl");
    await mobilePage.screenshot({
      path: path.join(SCREENSHOT_DIR, "safi-pricing-mobile-390-ar.png"),
      fullPage: true
    });
  }
  await mobileCtx.close();
  await publicCtx.close();

  // =========================================================================
  // SUITE B: DIRECT WEBSITE PURCHASE WITHOUT DESKTOP (SURVIVES SIGNUP + EMAIL VERIFICATION!)
  // =========================================================================
  const directCtx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const directPage = await directCtx.newPage();
  await directPage.addInitScript(() => {
    window.__SAFI_OPENED_URLS__ = [];
    window.open = (url) => {
      window.__SAFI_OPENED_URLS__.push(String(url));
      return null;
    };
  });

  let directRegistered = false;
  let directEmailVerified = false;
  let directWebhookPaid = false;
  let directCheckoutPayload = null;

  await directPage.route("**/v1/**", async (route) => {
    const url = route.request().url();
    const method = route.request().method();

    if (url.endsWith("/v1/auth/register") && method === "POST") {
      const body = route.request().postDataJSON();
      assert.equal(body.email, "newbuyer@safidata.test");
      assert.equal(body.display_name, "Direct Web Buyer");
      directRegistered = true;
      await route.fulfill({
        status: 202,
        contentType: "application/json",
        body: JSON.stringify({ message: "Verification email sent" })
      });
      return;
    }

    if (url.endsWith("/v1/auth/email/verify") && method === "POST") {
      const body = route.request().postDataJSON();
      assert.equal(body.token, "tok_verify_direct_e2e_998877");
      directEmailVerified = true;
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ message: "Email verified" })
      });
      return;
    }

    if (url.endsWith("/v1/billing/web/login") && method === "POST") {
      assert.equal(directEmailVerified, true, "Email verification must be completed before billing login");
      await route.fulfill({
        status: 201,
        contentType: "application/json",
        body: JSON.stringify({
          token: "sfb_direct_web_buyer_session_001",
          session_id: "01950000-0000-7000-8000-000000000101",
          user_id: "01950000-0000-7000-8000-000000000102",
          workspace_id: "01950000-0000-7000-8000-000000000103",
          email: "newbuyer@safidata.test",
          session_type: "billing_web",
          expires_at: 2_000_000_000
        })
      });
      return;
    }

    if (url.endsWith("/v1/billing/checkout") && method === "POST") {
      directCheckoutPayload = route.request().postDataJSON();
      assert.equal(directCheckoutPayload.product, "organization_seat_annual");
      assert.equal(directCheckoutPayload.seats, 6);
      assert.equal(directCheckoutPayload.billing_country_iso2, "EG");
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({
          order_id: "01950000-0000-7000-8000-000000000299",
          idempotency_key: directCheckoutPayload.idempotency_key,
          order_state: "checkout_pending",
          reused_existing_order: false,
          provider: "paymob",
          currency: "EGP",
          original_amount_minor: 6_120_000,
          discount_amount_minor: 0,
          amount_minor: 6_120_000,
          applied_coupon_code: null,
          provider_status: "ready",
          provider_order_ref: "pi_egy_direct_001",
          checkout_session_id: "pi_egy_direct_001",
          checkout_mode: "hosted_redirect",
          checkout_url: "https://accept.paymob.com/unifiedcheckout/?clientSecret=cs_test_direct"
        })
      });
      return;
    }

    if (url.includes("/v1/billing/orders/") && method === "GET") {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({
          order_id: "01950000-0000-7000-8000-000000000299",
          order_state: directWebhookPaid ? "paid" : "checkout_pending",
          provider: "paymob",
          product: "organization_seat_annual",
          seats: 6,
          currency: "EGP",
          original_amount_minor: 6_120_000,
          discount_amount_minor: 0,
          expected_amount_minor: 6_120_000,
          checkout_url: "https://accept.paymob.com/unifiedcheckout/?clientSecret=cs_test_direct",
          paid_at: directWebhookPaid ? 1_900_000_000 : null,
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
          effective_plan: directWebhookPaid ? "organization" : "free",
          state: directWebhookPaid ? "active" : "free",
          seats: directWebhookPaid ? 6 : 1,
          operation_credits: 0,
          pay_per_file_tokens: 0
        })
      });
      return;
    }

    await route.continue();
  });

  // 1. Choose Egypt (EGP) + Organization 6 seats on public /pricing
  await directPage.goto(`${BASE_URL}/pricing`, { waitUntil: "networkidle" });
  await directPage.locator('[data-pricing-currency="EGP"]').click();
  await directPage.locator('[data-pricing-org-seats="true"]').fill("6");
  await directPage.locator('[data-pricing-choose="organization_seat_annual"]').click();

  // 2. Arrives on /billing with pre-auth summary displaying exact selected plan & price BEFORE login!
  await directPage.waitForSelector('[data-billing-preauth-summary="true"]');
  assert.equal(
    await directPage.locator('[data-billing-preauth-product="organization_seat_annual"]').count(),
    1
  );
  const preAuthPrice = await directPage.locator('[data-billing-preauth-price="true"]').innerText();
  assert.ok(preAuthPrice.includes("61,200 EGP"), `Unexpected pre-auth price: ${preAuthPrice}`);

  // 3. Create account directly from /billing
  await directPage.locator('[data-billing-toggle-auth-mode="true"]').click();
  await directPage.fill('[data-billing-signup-name="true"]', "Direct Web Buyer");
  await directPage.fill('[data-billing-login-email="true"]', "newbuyer@safidata.test");
  await directPage.fill('[data-billing-login-password="true"]', "SafiStrongPassword!9");
  await directPage.click('[data-billing-login-submit="true"]');
  await directPage.waitForSelector('[data-billing-auth-message="true"]');
  assert.equal(directRegistered, true);

  // 4. Simulate clicking the email verification link in a fresh browser navigation!
  await directPage.goto(`${BASE_URL}/?verify_email=tok_verify_direct_e2e_998877`, {
    waitUntil: "networkidle"
  });
  await directPage.waitForSelector('[data-billing-verification-notice="true"]');
  assert.equal(directEmailVerified, true);
  // Selected product, country (EG), and 6 seats SURVIVED registration AND email verification!
  assert.equal(
    await directPage.locator('[data-billing-preauth-product="organization_seat_annual"]').count(),
    1
  );
  assert.ok(
    (await directPage.locator('[data-billing-preauth-price="true"]').innerText()).includes("61,200 EGP")
  );

  // 5. Sign in directly on /billing (ZERO Desktop session/handoff!) and complete Paymob EGP checkout
  await directPage.fill('[data-billing-login-email="true"]', "newbuyer@safidata.test");
  await directPage.fill('[data-billing-login-password="true"]', "SafiStrongPassword!9");
  await directPage.click('[data-billing-login-submit="true"]');
  await directPage.waitForSelector('[data-billing-authenticated="true"]');

  // Verify selected 6 seats & 61,200 EGP survived authentication too!
  const authSummaryPrice = await directPage.locator('[data-billing-final-price="true"]').innerText();
  assert.ok(authSummaryPrice.includes("61,200 EGP"), `Expected 61,200 EGP, got ${authSummaryPrice}`);

  // Fill real Paymob EGP customer details & submit checkout
  await directPage.fill('[data-billing-first-name="true"]', "Mohamed");
  await directPage.fill('[data-billing-last-name="true"]', "Rabei");
  await directPage.fill('[data-billing-phone="true"]', "+201012345678");
  await directPage.click('[data-billing-pay-submit="true"]');
  await directPage.waitForSelector('[data-billing-state="checkout_pending"]');

  // Return before webhook -> still unpaid!
  await directPage.click('[data-billing-verify-status="true"]');
  await directPage.waitForTimeout(120);
  assert.equal(await directPage.locator('[data-billing-state="paid"]').count(), 0);

  // After verified webhook -> server confirms PAID + Download Desktop App CTA shown!
  directWebhookPaid = true;
  await directPage.click('[data-billing-verify-status="true"]');
  await directPage.waitForSelector('[data-billing-state="paid"]');
  assert.equal(await directPage.locator('[data-billing-download-desktop="true"]').count(), 1);
  await directCtx.close();

  // =========================================================================
  // SUITE C: PRESERVED DESKTOP-TO-WEBSITE HANDOFF FLOW
  // =========================================================================
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page = await context.newPage();

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

  await page.goto(
    `${BASE_URL}/billing?handoff=sfbh_chromium_handoff_1234567890&state=csrf_chromium_state_1234567890`,
    { waitUntil: "networkidle" }
  );
  await page.waitForSelector('[data-billing-portal="true"]');
  const sanitizedUrl = page.url();
  assert.equal(sanitizedUrl, `${BASE_URL}/billing`);

  await page.waitForSelector('[data-billing-auth-gate="true"]');
  assert.equal(await page.locator('[data-tab-id="studio"]').count(), 0);
  assert.equal(await page.locator('[data-tab-id="owner"]').count(), 0);

  await page.fill('[data-billing-login-email="true"]', "buyer@safidata.test");
  await page.fill('[data-billing-login-password="true"]', "SafiStrongPassword!1");
  await page.click('[data-billing-login-submit="true"]');

  await page.waitForSelector('[data-billing-authenticated="true"]');
  await page.waitForFunction(() => {
    const el = document.querySelector('[data-billing-final-price="true"]');
    return el && el.textContent && el.textContent.includes("$1,020");
  });
  assert.equal(redeemedHandoff, true);

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

  await page.click('[data-billing-verify-status="true"]');
  await page.waitForTimeout(150);
  assert.equal(await page.locator('[data-billing-state="paid"]').count(), 0);

  webhookPaid = true;
  await page.click('[data-billing-verify-status="true"]');
  await page.waitForSelector('[data-billing-state="paid"]');
  const returnHref = await page.locator('[data-billing-return-desktop="true"]').getAttribute("href");
  assert.equal(returnHref, "safi://billing?order_id=01950000-0000-7000-8000-000000000099");

  console.log(
    "VERIFY_BILLING_PORTAL_E2E_PASSED",
    JSON.stringify({
      publicPricingDesktopOverflow: desktopOverflow,
      publicPricingMobileOverflow: mobileOverflow,
      directWebPurchaseSurvivedSignupAndEmailVerification: {
        registered: directRegistered,
        verified: directEmailVerified,
        checkoutProduct: directCheckoutPayload?.product,
        checkoutSeats: directCheckoutPayload?.seats,
        checkoutCountry: directCheckoutPayload?.billing_country_iso2
      },
      desktopHandoffPreserved: {
        sanitizedUrl,
        redeemedHandoff,
        checkoutCalls,
        openedUrls,
        returnHref
      }
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
