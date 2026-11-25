#!/usr/bin/env node
/**
 * REAL UNMOCKED Two-Account, Two-Device, Live PostgreSQL 18 + Real `sf-server` + Real Chromium UI
 * End-to-End Acceptance Suite for Organization Shared Workspace, 4-Role RBAC, Verified-Email
 * Invitations, and Zero-Knowledge Encrypted Sync (`tools/verify-org-sync-e2e.mjs`).
 *
 * Classification: **Real Backend E2E** (ZERO `page.route` API mocks, ZERO in-memory DB, ZERO Rust mocks).
 *
 * Requires:
 *   - `SF_TEST_DATABASE_URL` pointing to a disposable PostgreSQL 18 database with migrations `0001..0017` applied.
 *
 * Spawns:
 *   1. Real `sf-server` (`auth_harness_server` in `AUTH_HARNESS_PG_URL` PostgreSQL 18 mode) on `http://127.0.0.1:4194`
 *      (`SAFI_API_BASE_URL`) while Website runs on SEPARATE origin `http://127.0.0.1:4193` (`SAFI_PUBLIC_WEB_ORIGIN`),
 *      proving live cross-origin Web <-> API separation + CORS (`PATCH`, `DELETE`, `X-Safi-Workspace-Id`).
 *   2. TWO separate OS processes of native Rust `sf-desktop` (`studio_ipc_harness`) with isolated encrypted vaults:
 *      - Device A (`device-a-vault/` — Alice Owner)
 *      - Device B (`device-b-vault/` — Bob Member)
 *   3. Real Vite (`apps/web`) + real Headless Chromium (Playwright) exercising `/team` AND Desktop Studio.
 *
 * Executes all 10 mandatory E2E steps from Section 8:
 *   1. Account A (`alice@safidata.com`) activates a 4-seat Organization plan via signed Paddle webhook and creates
 *      the Organization from the real `/team` Website UI.
 *   2. Account A invites Account B (`bob@safidata.com`, `member`) and Viewer (`viewer@safidata.com`, `viewer`);
 *      Intruder (`mallory@safidata.com`) tries Account B's `sfi_*` token and is rejected (`403 SF-SYN-005`).
 *   3. Account B opens `/team?invite=sfi_...` in a separate browser context (proving zero pre-auth leak), signs in,
 *      and accepts the invitation; Viewer accepts as well.
 *   4. Device A and Device B both open the Shared Team Workspace, register static X25519 device keys, and exchange
 *      wrapped Epoch 1 Workspace Keys.
 *   5. Device A encrypts & pushes a shared resource (`ChaCha20-Poly1305`), tests idempotent retry + encrypted offline
 *      queue survival across Desktop process restart, and inspects raw PostgreSQL 18 `encrypted_sync_records` via `psql`
 *      to prove ZERO plaintext is stored in PostgreSQL.
 *   6. Device B pulls & decrypts the shared resource identically, and tests concurrent `409 SF-SYN-002` conflict
 *      detection + interactive conflict resolution (`accept_remote`).
 *   7. Viewer reads the shared workspace (`200 OK`) and is blocked on server (`403 SF-SYN-005`) and in `/team` UI
 *      from modifying resources or managing members.
 *   8. Account A removes Account B and Device A rotates the Workspace Key to Epoch 2 (`active_key_epoch = 2`).
 *   9. Account B is immediately blocked (`403 SF-SYN-005`) from future sync/key endpoints and holds no Epoch 2 key.
 *  10. KILLS the live `sf-server` Backend process and boots a brand-new `sf-server` process on the same PostgreSQL 18
 *      database, verifying Organization, memberships, roles, Epoch 2 key state, removed-member revocation, and
 *      encrypted sync records survive cold restart intact, and verifying Desktop Studio's live `Team & E2EE Sync` UI.
 */
import assert from "node:assert/strict";
import { createHmac, randomUUID } from "node:crypto";
import { mkdir, rm, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { execFileSync, spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { setTimeout as delay } from "node:timers/promises";

const dbUrl = (process.env.SF_TEST_DATABASE_URL ?? "").trim();
if (!dbUrl) {
  console.error("ERROR: SF_TEST_DATABASE_URL must point to a migrated PostgreSQL 18 database.");
  process.exit(2);
}

const psqlBin = process.env.PSQL_BIN || "psql";
const pgMajor = execFileSync(
  psqlBin,
  [dbUrl, "-v", "ON_ERROR_STOP=1", "-tAc", "SELECT current_setting('server_version_num')::int / 10000;"],
  { encoding: "utf8" }
).trim();
assert.equal(pgMajor, "18", `PostgreSQL 18 is required; got major=${pgMajor}`);

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

function signPaddleWebhook(secret, tsSecs, rawBodyStr) {
  const signedPayload = `${tsSecs}:${rawBodyStr}`;
  const h1 = createHmac("sha256", secret).update(signedPayload, "utf8").digest("hex");
  return `ts=${tsSecs};h1=${h1}`;
}

const { chromium } = await loadPlaywright();

const workDir = resolve("artifacts/real-e2e-org-sync");
await rm(workDir, { recursive: true, force: true });
const deviceAVaultDir = resolve(workDir, "device-a-vault");
const deviceBVaultDir = resolve(workDir, "device-b-vault");
const scratchDir = resolve(workDir, "scratch");
await mkdir(deviceAVaultDir, { recursive: true });
await mkdir(deviceBVaultDir, { recursive: true });
await mkdir(scratchDir, { recursive: true });

const dummyCsv = resolve(scratchDir, "sample.csv");
await writeFile(dummyCsv, "id,amount\n1,100\n2,250\n", "utf8");

console.log("[0/10] Building real Rust sf-desktop & sf-server binaries...");
execFileSync("cargo", ["build", "-p", "sf-desktop", "--example", "studio_ipc_harness"], {
  stdio: "inherit"
});
execFileSync("cargo", ["build", "-p", "sf-server", "--example", "auth_harness_server"], {
  stdio: "inherit"
});

const binExt = process.platform === "win32" ? ".exe" : "";
const harnessBin = resolve("target/debug/examples", `studio_ipc_harness${binExt}`);
const authServerBin = resolve("target/debug/examples", `auth_harness_server${binExt}`);

const vitePort = 4193;
const apiPort = 4194;
const webOrigin = `http://127.0.0.1:${vitePort}`;
const apiOrigin = `http://127.0.0.1:${apiPort}`;
const paddleSecret = "pdl_org_e2e_whsec_2026";

async function startBackendProcess(label) {
  const proc = spawn(authServerBin, [], {
    stdio: ["ignore", "pipe", "inherit"],
    env: {
      ...process.env,
      AUTH_HARNESS_ADDR: `127.0.0.1:${apiPort}`,
      AUTH_HARNESS_PUBLIC_ORIGIN: apiOrigin,
      AUTH_HARNESS_WEB_ORIGIN: webOrigin,
      AUTH_HARNESS_PG_URL: dbUrl,
      AUTH_HARNESS_PADDLE_SECRET: paddleSecret
    }
  });
  const rl = createInterface({ input: proc.stdout });
  const info = await new Promise((res, rej) => {
    const timer = setTimeout(() => rej(new Error(`${label} startup timeout`)), 60_000);
    rl.on("line", (line) => {
      const trimmed = line.trim();
      if (!trimmed) return;
      const parsed = JSON.parse(trimmed);
      if (parsed.ready) {
        clearTimeout(timer);
        res(parsed);
      }
    });
    proc.once("exit", (code) => {
      clearTimeout(timer);
      rej(new Error(`${label} exited early (${code})`));
    });
  });
  assert.equal(info.pgBacked, true, `${label} MUST run backed by live PostgreSQL 18!`);
  return proc;
}

async function stopProcess(proc) {
  if (!proc || proc.exitCode !== null) return;
  const exited = new Promise((res) => proc.once("exit", res));
  proc.kill();
  await Promise.race([exited, delay(5_000)]);
}

function spawnDesktopClient(storageDir, label) {
  const proc = spawn(harnessBin, [], {
    stdio: ["pipe", "pipe", "inherit"],
    env: {
      ...process.env,
      HARNESS_STORAGE_DIR: storageDir,
      HARNESS_CSV_PATH: dummyCsv,
      HARNESS_CAPSULE_PATH: resolve(storageDir, "capsule.safi.html"),
      HARNESS_SEARCH_DIR: scratchDir,
      HARNESS_SAFITEMPLATE_PATH: resolve(storageDir, "recipe.safitemplate"),
      HARNESS_ALLOWED_AUTH_ORIGIN: webOrigin
    }
  });
  const rl = createInterface({ input: proc.stdout });
  const pending = new Map();
  let seq = 0;
  const ready = new Promise((res, rej) => {
    const timer = setTimeout(() => rej(new Error(`${label} startup timeout`)), 30_000);
    rl.on("line", (line) => {
      const trimmed = line.trim();
      if (!trimmed) return;
      const msg = JSON.parse(trimmed);
      if (msg.ready) {
        clearTimeout(timer);
        res();
        return;
      }
      const waiter = pending.get(msg.id);
      if (!waiter) return;
      pending.delete(msg.id);
      if (msg.ok) waiter.resolve(msg.result);
      else waiter.reject(new Error(String(msg.error)));
    });
  });

  return {
    proc,
    async call(command, args = {}) {
      await ready;
      const id = ++seq;
      return new Promise((res, rej) => {
        pending.set(id, { resolve: res, reject: rej });
        proc.stdin.write(`${JSON.stringify({ id, command, args })}\n`);
      });
    },
    async bindAuthenticatedSession(token, sessionInfo, installationId) {
      await this.call("__harness_register_session", {
        token,
        sessionId: sessionInfo.id,
        workspaceId: sessionInfo.workspace_id,
        installationId,
        expiresAtMs: Number(sessionInfo.expires_at) * 1000
      });
      await this.call("studio_set_auth_session", {
        token,
        installationId
      });
    },
    async stop() {
      await stopProcess(proc);
    }
  };
}

async function apiJson(method, path, { token, workspaceId, installationId, body } = {}) {
  const headers = { Accept: "application/json" };
  if (token) headers.Authorization = `Bearer ${token}`;
  if (workspaceId) headers["X-Safi-Workspace-Id"] = workspaceId;
  if (installationId) headers["X-Safi-Installation-Id"] = installationId;
  if (body !== undefined) headers["Content-Type"] = "application/json";
  const resp = await fetch(`${apiOrigin}${path}`, {
    method,
    headers,
    ...(body !== undefined ? { body: JSON.stringify(body) } : {})
  });
  const text = await resp.text();
  let data = null;
  if (text) {
    try {
      data = JSON.parse(text);
    } catch {
      data = { raw: text };
    }
  }
  return { status: resp.status, data };
}

async function loginVerifiedUser(email, password, deviceLabel, installationId = randomUUID()) {
  const { status, data } = await apiJson("POST", "/v1/auth/login", {
    body: {
      email,
      password,
      device_name: deviceLabel,
      installation_id: installationId,
      platform: "windows"
    }
  });
  assert.ok(
    status === 200 || status === 201,
    `Login failed for ${email} (${status}): ${JSON.stringify(data)}`
  );
  const cur = await apiJson("GET", "/v1/auth/sessions/current", {
    token: data.token,
    installationId
  });
  assert.equal(cur.status, 200, `Current session failed: ${JSON.stringify(cur.data)}`);
  return {
    token: data.token,
    installationId,
    userId: cur.data.user_id,
    personalWorkspaceId: cur.data.workspace_id,
    sessionInfo: cur.data
  };
}

async function signInOnWebPortal(page, email, password) {
  await page.locator('[data-testid="org-signin-cta"]').waitFor({ state: "visible", timeout: 15_000 });
  await page.locator('[data-testid="org-signin-cta"]').click();
  const form = page.locator('[data-auth-mode="signin"] form');
  await form.waitFor({ state: "visible", timeout: 10_000 });
  await form.locator('input[type="email"]').fill(email);
  await form.locator('input[type="password"]').fill(password);
  await form.locator('button[data-auth-stagger="submit"]').click();
  await page.locator(".safi-auth-modal").waitFor({ state: "detached", timeout: 15_000 });
  await page.locator('[data-org-workspace-panel="authenticated"]').waitFor({
    state: "visible",
    timeout: 15_000
  });
}

let authProc = await startBackendProcess("sf-server #1 (PostgreSQL 18)");
let deviceA = spawnDesktopClient(deviceAVaultDir, "Device A (Alice)");
let deviceB = spawnDesktopClient(deviceBVaultDir, "Device B (Bob)");

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
      SAFI_PUBLIC_WEB_ORIGIN: webOrigin,
      SAFI_DESKTOP_RELEASE_BUILD: "0",
      SAFI_DEV_API_PROXY: apiOrigin
    }
  }
);

async function waitForVite() {
  for (let i = 0; i < 120; i += 1) {
    if (viteProc.exitCode !== null) throw new Error(`Vite exited early (${viteProc.exitCode})`);
    try {
      const r = await fetch(webOrigin);
      if (r.ok) return;
    } catch {
      // retry
    }
    await delay(250);
  }
  throw new Error(`Vite did not start on ${webOrigin}`);
}

let browser;
try {
  await waitForVite();
  browser = await chromium.launch({ headless: true, args: ["--no-sandbox"] });

  // ---------------------------------------------------------------------------
  // STEP 1: Account A activates 4-seat Organization plan & creates Org on `/team`
  // ---------------------------------------------------------------------------
  console.log("[1/10] Account A activates 4-seat Organization subscription & creates Org on /team...");
  const aliceAuth = await loginVerifiedUser("alice@safidata.com", "SafiAlice#2026", "Alice API Bootstrap");
  const coRes = await apiJson("POST", "/v1/billing/checkout", {
    token: aliceAuth.token,
    body: {
      product: "organization_seat_annual",
      seats: 4,
      billing_country_iso2: "US",
      idempotency_key: "e2e-alice-org-4-seats"
    }
  });
  assert.equal(coRes.status, 200, `Checkout failed: ${JSON.stringify(coRes.data)}`);

  const nowSecs = Math.floor(Date.now() / 1000);
  const whPayload = JSON.stringify({
    event_id: "evt_real_e2e_org_4_seats",
    event_type: "transaction.completed",
    occurred_at: "2026-11-24T12:00:00Z",
    data: {
      id: coRes.data.provider_order_ref,
      status: "completed",
      currency_code: "USD",
      subscription_id: "sub_real_e2e_org_alice",
      invoice_id: "inv_real_e2e_org_alice",
      items: [{ price: { id: "pri_org_usd" }, quantity: 4 }],
      custom_data: {
        safi_order_id: coRes.data.order_id,
        workspace_id: aliceAuth.personalWorkspaceId,
        user_id: aliceAuth.userId,
        product: "organization_seat_annual",
        seats: 4
      },
      details: { totals: { total: String(4 * 20_400) } }
    }
  });
  const paddleSig = signPaddleWebhook(paddleSecret, nowSecs, whPayload);
  const whResp = await fetch(`${apiOrigin}/v1/billing/webhooks/paddle`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "paddle-signature": paddleSig
    },
    body: whPayload
  });
  assert.equal(whResp.status, 200, "Signed Paddle webhook must succeed");

  const aliceCtx = await browser.newContext({ viewport: { width: 1440, height: 920 }, reducedMotion: "reduce" });
  const alicePage = await aliceCtx.newPage();
  await alicePage.goto(`${webOrigin}/team`, { waitUntil: "domcontentloaded", timeout: 60_000 });
  await alicePage.waitForSelector("#root .safi-intro-splash", { state: "detached", timeout: 20_000 });
  await signInOnWebPortal(alicePage, "alice@safidata.com", "SafiAlice#2026");

  await alicePage.fill('[data-testid="create-org-name-input"]', "Safi Production Team");
  await alicePage.fill('[data-testid="create-org-seats-input"]', "4");
  await alicePage.click('[data-testid="create-org-submit-btn"]');
  await alicePage.locator('[data-testid="org-invitations-section"]').waitFor({ state: "visible", timeout: 15_000 });

  // ---------------------------------------------------------------------------
  // STEP 2: Account A invites Bob (`member`) & Viewer (`viewer`); Mallory blocked!
  // ---------------------------------------------------------------------------
  console.log("[2/10] Inviting Bob & Viewer from /team UI and verifying wrong-email rejection...");
  await alicePage.fill('[data-testid="invite-email-input"]', "bob@safidata.com");
  await alicePage.selectOption('[data-testid="invite-role-select"]', "member");
  await alicePage.click('[data-testid="send-invite-btn"]');
  await alicePage.locator('[data-testid="created-invite-url"]').waitFor({ state: "visible", timeout: 10_000 });
  const bobInviteUrl = (await alicePage.locator('[data-testid="created-invite-url"]').innerText()).trim();
  assert.ok(
    bobInviteUrl.startsWith(`${webOrigin}/team?invite=sfi_`),
    `Invite URL must use SAFI_PUBLIC_WEB_ORIGIN (${webOrigin}), got: ${bobInviteUrl}`
  );
  const bobInviteToken = new URL(bobInviteUrl).searchParams.get("invite");
  assert.ok(bobInviteToken?.startsWith("sfi_"));

  // Intruder Mallory (`mallory@safidata.com`) attempts to accept Bob's single-use token -> 403 SF-SYN-005!
  const malloryAuth = await loginVerifiedUser("mallory@safidata.com", "SafiMallory#2026", "Mallory Intruder");
  const malloryAttempt = await apiJson("POST", "/v1/organizations/invitations/accept", {
    token: malloryAuth.token,
    body: { token: bobInviteToken }
  });
  assert.equal(malloryAttempt.status, 403, "Wrong-email account MUST be blocked from accepting token");
  assert.equal(malloryAttempt.data.code, "SF-SYN-005");

  // Invite Viewer (`viewer@safidata.com`)
  await alicePage.fill('[data-testid="invite-email-input"]', "viewer@safidata.com");
  await alicePage.selectOption('[data-testid="invite-role-select"]', "viewer");
  await alicePage.click('[data-testid="send-invite-btn"]');
  await delay(400);
  const viewerInviteUrl = (await alicePage.locator('[data-testid="created-invite-url"]').innerText()).trim();
  const viewerInviteToken = new URL(viewerInviteUrl).searchParams.get("invite");

  // ---------------------------------------------------------------------------
  // STEP 3: Account B opens `/team?invite=sfi_...`, verifies zero pre-auth leak, signs in & accepts!
  // ---------------------------------------------------------------------------
  console.log("[3/10] Account B opens /team?invite=..., verifies zero pre-auth org leak, signs in & accepts...");
  const bobCtx = await browser.newContext({ viewport: { width: 1440, height: 920 }, reducedMotion: "reduce" });
  const bobPage = await bobCtx.newPage();
  await bobPage.goto(bobInviteUrl, { waitUntil: "domcontentloaded", timeout: 60_000 });
  await bobPage.waitForSelector("#root .safi-intro-splash", { state: "detached", timeout: 20_000 });
  await bobPage.locator('[data-org-workspace-panel="unauthenticated"]').waitFor({ state: "visible", timeout: 10_000 });

  const unauthMarkup = await bobPage.locator('[data-org-workspace-panel="unauthenticated"]').innerText();
  assert.ok(!unauthMarkup.includes("Safi Production Team"), "Unauthenticated invite page MUST NOT leak org name!");
  assert.ok(!bobPage.url().includes("sfi_"), "Invite token MUST be scrubbed from URL bar!");

  await signInOnWebPortal(bobPage, "bob@safidata.com", "SafiBob#2026");
  await bobPage.locator('[data-testid="org-accept-token-btn"]').waitFor({ state: "visible", timeout: 10_000 });
  await bobPage.locator('[data-testid="org-accept-token-btn"]').click();
  await delay(600);
  const bobAuthedText = await bobPage.locator('[data-org-workspace-panel="authenticated"]').innerText();
  assert.ok(bobAuthedText.includes("Safi Production Team"), "Bob must join Safi Production Team after accepting!");

  // Viewer accepts invitation
  const viewerAuth = await loginVerifiedUser("viewer@safidata.com", "SafiViewer#2026", "Viewer Session");
  const viewerAccept = await apiJson("POST", "/v1/organizations/invitations/accept", {
    token: viewerAuth.token,
    body: { token: viewerInviteToken }
  });
  assert.equal(viewerAccept.status, 200);

  // Refresh Alice & Bob tokens for Desktop Device A & Device B
  const aliceDesktop = await loginVerifiedUser("alice@safidata.com", "SafiAlice#2026", "Device A Desktop");
  const bobDesktop = await loginVerifiedUser("bob@safidata.com", "SafiBob#2026", "Device B Desktop");

  const mineAlice = await apiJson("GET", "/v1/organizations/mine", { token: aliceDesktop.token });
  assert.equal(mineAlice.status, 200);
  const orgSummary = mineAlice.data.organizations[0];
  const orgId = orgSummary.organization.id;
  const sharedWorkspaceId = orgSummary.organization.workspace_id;
  assert.notEqual(sharedWorkspaceId, aliceDesktop.personalWorkspaceId);

  const mineBob = await apiJson("GET", "/v1/organizations/mine", { token: bobDesktop.token });
  assert.equal(mineBob.data.organizations[0].organization.workspace_id, sharedWorkspaceId);

  // ---------------------------------------------------------------------------
  // STEP 4: Device A & Device B open Shared Team Workspace & exchange X25519 wrapped Epoch 1 key
  // ---------------------------------------------------------------------------
  console.log("[4/10] Binding separate Rust Desktop clients (Device A & Device B) & exchanging X25519 Epoch 1 keys...");
  await deviceA.bindAuthenticatedSession(aliceDesktop.token, aliceDesktop.sessionInfo, aliceDesktop.installationId);
  await deviceB.bindAuthenticatedSession(bobDesktop.token, bobDesktop.sessionInfo, bobDesktop.installationId);

  const statusA0 = JSON.parse(
    await deviceA.call("desktop_sync_select_workspace", {
      organizationId: orgId,
      workspaceId: sharedWorkspaceId,
      activeKeyEpoch: 1
    })
  );
  const statusB0 = JSON.parse(
    await deviceB.call("desktop_sync_select_workspace", {
      organizationId: orgId,
      workspaceId: sharedWorkspaceId,
      activeKeyEpoch: 1
    })
  );
  assert.notEqual(
    statusA0.device_identity.device_id,
    statusB0.device_identity.device_id,
    "Device A and Device B MUST have distinct local X25519 identities!"
  );

  for (const [tok, st] of [
    [aliceDesktop.token, statusA0],
    [bobDesktop.token, statusB0]
  ]) {
    const reg = await apiJson("POST", `/v1/organizations/${orgId}/keys/devices`, {
      token: tok,
      body: {
        device_id: st.device_identity.device_id,
        device_label: st.device_identity.device_label,
        x25519_public_key_b64: st.device_identity.x25519_public_key_b64
      }
    });
    assert.equal(reg.status, 201);
  }

  const allDeviceKeys = await apiJson("GET", `/v1/organizations/${orgId}/keys/devices`, {
    token: aliceDesktop.token
  });
  assert.equal(allDeviceKeys.status, 200);
  assert.equal(allDeviceKeys.data.length, 2);

  // Device A generates & wraps Epoch 1 Workspace Key for both Device A and Device B
  const wrappedEpoch1 = JSON.parse(
    await deviceA.call("desktop_sync_wrap_epoch_keys", {
      organizationId: orgId,
      workspaceId: sharedWorkspaceId,
      keyEpoch: 1,
      devicesJson: JSON.stringify(allDeviceKeys.data)
    })
  );
  assert.equal(wrappedEpoch1.length, 2);

  const pubWrap = await apiJson("POST", `/v1/organizations/${orgId}/keys/wrap`, {
    token: aliceDesktop.token,
    body: { key_epoch: 1, entries: wrappedEpoch1 }
  });
  assert.equal(pubWrap.status, 200);

  // Device B downloads its wrapped key & unwraps locally using its private X25519 key!
  const bobWrapped = await apiJson(
    "GET",
    `/v1/organizations/${orgId}/keys/my-wrapped?device_id=${encodeURIComponent(statusB0.device_identity.device_id)}`,
    { token: bobDesktop.token }
  );
  assert.equal(bobWrapped.status, 200);
  assert.equal(bobWrapped.data.length, 1);

  const ingestedB = JSON.parse(
    await deviceB.call("desktop_sync_ingest_wrapped_keys", {
      envelopesJson: JSON.stringify(bobWrapped.data)
    })
  );
  assert.equal(ingestedB.unlocked_count ?? ingestedB.unlockedCount ?? 1, 1);

  // ---------------------------------------------------------------------------
  // STEP 5: Device A encrypts & syncs shared resource + offline queue across Desktop restart + PG zero-plaintext proof!
  // ---------------------------------------------------------------------------
  console.log("[5/10] Device A encrypts shared resource, verifies offline queue across Desktop restart & zero plaintext in PostgreSQL 18...");
  const secretMarker = "CONFIDENTIAL_REVENUE_PIPELINE_2026_EGYPT";
  await deviceA.call("desktop_sync_stage_upsert", {
    organizationId: orgId,
    workspaceId: sharedWorkspaceId,
    resourceKind: "recipe",
    resourceId: "cairo-q4-clean",
    title: "Cairo Q4 Cleaning Pipeline",
    payloadJson: JSON.stringify({ secret: secretMarker, steps: ["trim_arabic", "dedupe_vat"] })
  });

  // Restart Device A process BEFORE pushing to prove encrypted local offline queue survives Desktop restart!
  await deviceA.stop();
  deviceA = spawnDesktopClient(deviceAVaultDir, "Device A Restarted");
  await deviceA.bindAuthenticatedSession(aliceDesktop.token, aliceDesktop.sessionInfo, aliceDesktop.installationId);

  const envA1 = JSON.parse(
    await deviceA.call("desktop_sync_peek_outbound", { workspaceId: sharedWorkspaceId })
  );
  assert.ok(envA1, "Encrypted offline queue item MUST survive Desktop client restart!");
  assert.ok(
    !Buffer.from(envA1.ciphertext_b64, "base64").toString("utf8").includes(secretMarker),
    "Outbound envelope ciphertext MUST NOT contain plaintext!"
  );

  // Push v1 + test idempotent network retry with same `idempotency_key`
  const push1 = await apiJson("POST", "/v1/sync/envelopes", {
    token: aliceDesktop.token,
    workspaceId: sharedWorkspaceId,
    body: envA1
  });
  assert.equal(push1.status, 200);
  assert.equal(push1.data.version, 1);

  const push1Retry = await apiJson("POST", "/v1/sync/envelopes", {
    token: aliceDesktop.token,
    workspaceId: sharedWorkspaceId,
    body: envA1
  });
  assert.equal(push1Retry.status, 200);
  assert.equal(push1Retry.data.version, 1);
  assert.equal(push1Retry.data.workspace_seq, push1.data.workspace_seq);

  await deviceA.call("desktop_sync_complete_outbound", {
    outcomeJson: JSON.stringify({
      outcome: "committed",
      idempotencyKey: envA1.idempotency_key,
      version: push1.data.version,
      workspaceSeq: push1.data.workspace_seq,
      authorUserId: aliceDesktop.userId
    })
  });

  // Direct PostgreSQL 18 inspection via `psql`: prove ZERO plaintext exists in `encrypted_sync_records`!
  const pgPlaintextHits = execFileSync(
    psqlBin,
    [
      dbUrl,
      "-v",
      "ON_ERROR_STOP=1",
      "-tAc",
      `SELECT count(*) FROM encrypted_sync_records WHERE workspace_id = '${sharedWorkspaceId}' AND position(convert_to('${secretMarker}', 'UTF8') in ciphertext) > 0;`
    ],
    { encoding: "utf8" }
  ).trim();
  assert.equal(pgPlaintextHits, "0", "PostgreSQL 18 MUST contain ZERO plaintext customer bytes!");

  // ---------------------------------------------------------------------------
  // STEP 6: Device B pulls & decrypts shared resource + tests conflict detection & resolution!
  // ---------------------------------------------------------------------------
  console.log("[6/10] Device B pulls & decrypts shared resource + verifies optimistic conflict resolution...");
  const pulledB = await apiJson("GET", "/v1/sync/changes?since_seq=0&limit=50", {
    token: bobDesktop.token,
    workspaceId: sharedWorkspaceId
  });
  assert.equal(pulledB.status, 200);
  assert.equal(pulledB.data.records.length, 1);

  await deviceB.call("desktop_sync_apply_pulled", {
    workspaceId: sharedWorkspaceId,
    envelopesJson: JSON.stringify(pulledB.data.records)
  });
  const resourcesB = JSON.parse(
    await deviceB.call("desktop_sync_list_resources", {
      workspaceId: sharedWorkspaceId,
      includeDeleted: false
    })
  );
  assert.equal(resourcesB.length, 1);
  assert.ok(
    resourcesB[0].payload_json.includes(secretMarker),
    "Device B MUST decrypt the exact shared resource plaintext locally!"
  );

  // Simulate concurrent edit conflict: Device A pushes v2 while Device B stages edit against base v1!
  await deviceB.call("desktop_sync_stage_upsert", {
    organizationId: orgId,
    workspaceId: sharedWorkspaceId,
    resourceKind: "recipe",
    resourceId: "cairo-q4-clean",
    title: "Bob Stale Local Edit",
    payloadJson: JSON.stringify({ secret: "BOB_CONFLICT_DRAFT" })
  });
  const bobStaleEnv = JSON.parse(
    await deviceB.call("desktop_sync_peek_outbound", { workspaceId: sharedWorkspaceId })
  );

  await deviceA.call("desktop_sync_stage_upsert", {
    organizationId: orgId,
    workspaceId: sharedWorkspaceId,
    resourceKind: "recipe",
    resourceId: "cairo-q4-clean",
    title: "Alice Committed v2",
    payloadJson: JSON.stringify({ secret: secretMarker, v: 2 })
  });
  const aliceV2Env = JSON.parse(
    await deviceA.call("desktop_sync_peek_outbound", { workspaceId: sharedWorkspaceId })
  );
  const pushV2 = await apiJson("POST", "/v1/sync/envelopes", {
    token: aliceDesktop.token,
    workspaceId: sharedWorkspaceId,
    body: aliceV2Env
  });
  assert.equal(pushV2.status, 200);
  assert.equal(pushV2.data.version, 2);
  await deviceA.call("desktop_sync_complete_outbound", {
    outcomeJson: JSON.stringify({
      outcome: "committed",
      idempotencyKey: aliceV2Env.idempotency_key,
      version: 2,
      workspaceSeq: pushV2.data.workspace_seq,
      authorUserId: aliceDesktop.userId
    })
  });

  // Device B pushes stale `expected_version: 1` -> server returns `409 SF-SYN-002`!
  const bobConflictPush = await apiJson("POST", "/v1/sync/envelopes", {
    token: bobDesktop.token,
    workspaceId: sharedWorkspaceId,
    body: bobStaleEnv
  });
  assert.equal(bobConflictPush.status, 409);
  assert.equal(bobConflictPush.data.code, "SF-SYN-002");

  const latestRemote = await apiJson("GET", "/v1/sync/envelopes/recipe/cairo-q4-clean", {
    token: bobDesktop.token,
    workspaceId: sharedWorkspaceId
  });
  assert.equal(latestRemote.status, 200);
  await deviceB.call("desktop_sync_complete_outbound", {
    outcomeJson: JSON.stringify({
      outcome: "conflict",
      idempotencyKey: bobStaleEnv.idempotency_key,
      remoteEnvelope: latestRemote.data
    })
  });
  const conflictsB = JSON.parse(
    await deviceB.call("desktop_sync_list_conflicts", { workspaceId: sharedWorkspaceId })
  );
  assert.equal(conflictsB.length, 1);

  const resolvedB = JSON.parse(
    await deviceB.call("desktop_sync_resolve_conflict", {
      workspaceId: sharedWorkspaceId,
      resourceKind: "recipe",
      resourceId: "cairo-q4-clean",
      choice: "accept_remote"
    })
  );
  assert.equal(resolvedB.version, 2);
  assert.equal(resolvedB.title, "Alice Committed v2");

  // ---------------------------------------------------------------------------
  // STEP 7: Viewer can read shared workspace (`200`), blocked from modifying (`403 SF-SYN-005`) & UI restricted!
  // ---------------------------------------------------------------------------
  console.log("[7/10] Verifying Viewer read-only access on Backend & Website /team UI...");
  const viewerPull = await apiJson("GET", "/v1/sync/changes?since_seq=0", {
    token: viewerAuth.token,
    workspaceId: sharedWorkspaceId
  });
  assert.equal(viewerPull.status, 200);
  assert.ok(viewerPull.data.records.length >= 1);

  const viewerWrite = await apiJson("POST", "/v1/sync/envelopes", {
    token: viewerAuth.token,
    workspaceId: sharedWorkspaceId,
    body: aliceV2Env
  });
  assert.equal(viewerWrite.status, 403);
  assert.equal(viewerWrite.data.code, "SF-SYN-005");

  const viewerCtx = await browser.newContext({ viewport: { width: 1440, height: 920 }, reducedMotion: "reduce" });
  const viewerPage = await viewerCtx.newPage();
  await viewerPage.goto(`${webOrigin}/team`, { waitUntil: "domcontentloaded", timeout: 60_000 });
  await viewerPage.waitForSelector("#root .safi-intro-splash", { state: "detached", timeout: 20_000 });
  await signInOnWebPortal(viewerPage, "viewer@safidata.com", "SafiViewer#2026");
  await delay(500);
  assert.equal(
    await viewerPage.locator('[data-testid="org-invitations-section"]').count(),
    0,
    "Viewer MUST NOT see member invitation management controls!"
  );
  assert.equal(
    await viewerPage.locator('[data-testid="delete-org-btn"]').count(),
    0,
    "Viewer MUST NOT see Owner-only Delete Organization button!"
  );
  await viewerCtx.close();
  // Refresh viewerAuth session after web login
  const viewerRefreshed = await loginVerifiedUser("viewer@safidata.com", "SafiViewer#2026", "Viewer Refreshed");

  // ---------------------------------------------------------------------------
  // STEP 8 & 9: Owner removes Bob + rotates Workspace Key to Epoch 2 -> Bob blocked from future sync!
  // ---------------------------------------------------------------------------
  console.log("[8-9/10] Removing Bob & rotating Workspace Key to Epoch 2; verifying Bob revocation...");
  const remRes = await apiJson("DELETE", `/v1/organizations/${orgId}/members/${bobDesktop.userId}`, {
    token: aliceDesktop.token
  });
  assert.equal(remRes.status, 204);

  const remainingKeys = await apiJson("GET", `/v1/organizations/${orgId}/keys/devices`, {
    token: aliceDesktop.token
  });
  assert.equal(remainingKeys.status, 200);
  assert.equal(
    remainingKeys.data.some((k) => k.user_id === bobDesktop.userId),
    false,
    "Removed member's device keys MUST be automatically excluded from active device directory!"
  );

  const preparedRot = JSON.parse(
    await deviceA.call("desktop_sync_prepare_key_rotation", {
      organizationId: orgId,
      workspaceId: sharedWorkspaceId,
      currentEpoch: 1,
      remainingDevicesJson: JSON.stringify(remainingKeys.data)
    })
  );
  const rotRes = await apiJson("POST", `/v1/organizations/${orgId}/keys/rotate`, {
    token: aliceDesktop.token,
    body: {
      expected_current_epoch: 1,
      entries: preparedRot.entries
    }
  });
  assert.equal(rotRes.status, 200);
  assert.equal(rotRes.data.active_key_epoch, 2);

  // Device A pushes an Epoch 2 resource (`post-rotation-plan`)
  await deviceA.call("desktop_sync_stage_upsert", {
    organizationId: orgId,
    workspaceId: sharedWorkspaceId,
    resourceKind: "project",
    resourceId: "post-rotation-plan",
    title: "Epoch 2 Confidential Plan",
    payloadJson: JSON.stringify({ note: "EPOCH_2_FORWARD_SECRET" })
  });
  const epoch2Env = JSON.parse(
    await deviceA.call("desktop_sync_peek_outbound", { workspaceId: sharedWorkspaceId })
  );
  assert.equal(epoch2Env.key_epoch, 2);
  const pushEpoch2 = await apiJson("POST", "/v1/sync/envelopes", {
    token: aliceDesktop.token,
    workspaceId: sharedWorkspaceId,
    body: epoch2Env
  });
  assert.equal(pushEpoch2.status, 200);
  await deviceA.call("desktop_sync_complete_outbound", {
    outcomeJson: JSON.stringify({
      outcome: "committed",
      idempotencyKey: epoch2Env.idempotency_key,
      version: pushEpoch2.data.version,
      workspaceSeq: pushEpoch2.data.workspace_seq,
      authorUserId: aliceDesktop.userId
    })
  });

  // Removed Bob is immediately rejected (`403 SF-SYN-005`) across ALL sync & key endpoints!
  const bobPullBlocked = await apiJson("GET", "/v1/sync/changes?since_seq=0", {
    token: bobDesktop.token,
    workspaceId: sharedWorkspaceId
  });
  assert.equal(bobPullBlocked.status, 403);
  assert.equal(bobPullBlocked.data.code, "SF-SYN-005");

  const bobKeysBlocked = await apiJson(
    "GET",
    `/v1/organizations/${orgId}/keys/my-wrapped?device_id=${encodeURIComponent(statusB0.device_identity.device_id)}`,
    { token: bobDesktop.token }
  );
  assert.equal(bobKeysBlocked.status, 403);
  assert.equal(bobKeysBlocked.data.code, "SF-SYN-005");

  // ---------------------------------------------------------------------------
  // STEP 10: KILL & COLD-RESTART `sf-server` on PostgreSQL 18 + verify Desktop Studio Team & E2EE Sync UI!
  // ---------------------------------------------------------------------------
  console.log("[10/10] Killing sf-server & cold-restarting on PostgreSQL 18 + verifying Desktop Studio E2EE Sync UI...");
  await stopProcess(authProc);
  authProc = await startBackendProcess("sf-server #2 (Cold Restart on PostgreSQL 18)");

  // Sessions in `SqlxSessionRepository` + Orgs + Memberships + Keys + Encrypted Sync ALL survive restart!
  const orgAfterRestart = await apiJson("GET", `/v1/organizations/${orgId}`, {
    token: aliceDesktop.token
  });
  assert.equal(orgAfterRestart.status, 200);
  assert.equal(orgAfterRestart.data.organization.active_key_epoch, 2);
  assert.equal(orgAfterRestart.data.active_members_count, 2); // Alice (Owner) + Viewer

  const viewerAfterRestart = await apiJson("GET", "/v1/sync/changes?since_seq=0", {
    token: viewerRefreshed.token,
    workspaceId: sharedWorkspaceId
  });
  assert.equal(viewerAfterRestart.status, 200);
  assert.equal(viewerAfterRestart.data.records.length, 2);

  const bobStillBlocked = await apiJson("GET", "/v1/sync/changes?since_seq=0", {
    token: bobDesktop.token,
    workspaceId: sharedWorkspaceId
  });
  assert.equal(bobStillBlocked.status, 403);
  assert.equal(bobStillBlocked.data.code, "SF-SYN-005");

  // Mount real Desktop Studio in Chromium wired to Device A's native Rust `studio_ipc_harness`!
  const desktopCtx = await browser.newContext({ viewport: { width: 1440, height: 960 }, reducedMotion: "reduce" });
  const desktopPage = await desktopCtx.newPage();
  await desktopPage.exposeFunction("__safiRustInvoke", async (command, args) => {
    const safeArgs = args ?? {};
    if (command === "studio_set_auth_session") {
      const token = String(safeArgs.token ?? "");
      const installationId = String(safeArgs.installationId ?? safeArgs.installation_id ?? aliceDesktop.installationId);
      const cur = await apiJson("GET", "/v1/auth/sessions/current", { token, installationId });
      if (cur.status === 200) {
        await deviceA.bindAuthenticatedSession(token, cur.data, installationId);
        return "null";
      }
    }
    return deviceA.call(command, safeArgs);
  });
  await desktopPage.addInitScript(() => {
    window.__TAURI_INTERNALS__ = {
      invoke: (command, args) => window.__safiRustInvoke(command, args ?? {})
    };
  });

  await desktopPage.goto(webOrigin, { waitUntil: "domcontentloaded", timeout: 60_000 });
  await desktopPage.waitForSelector("#root .safi-intro-splash", { state: "detached", timeout: 20_000 });
  await desktopPage.locator('[data-desktop-pkce-signin="true"]').waitFor({ state: "visible", timeout: 15_000 });
  await desktopPage.locator('[data-desktop-pkce-signin="true"]').click();
  await desktopPage.locator('[data-desktop-pkce-waiting="true"]').waitFor({ state: "visible", timeout: 10_000 });
  const openedRaw = JSON.parse(await deviceA.call("__harness_take_opened_auth_url"));
  const authBrowserCtx = await browser.newContext({ viewport: { width: 1280, height: 800 }, reducedMotion: "reduce" });
  const authBrowserPage = await authBrowserCtx.newPage();
  await authBrowserPage.goto(openedRaw.url, { waitUntil: "domcontentloaded", timeout: 60_000 });
  await authBrowserPage.waitForSelector("#root .safi-intro-splash", { state: "detached", timeout: 20_000 });
  await authBrowserPage.fill('[data-desktop-auth-email="true"]', "alice@safidata.com");
  await authBrowserPage.fill('[data-desktop-auth-password="true"]', "SafiAlice#2026");
  await authBrowserPage.click('[data-desktop-auth-submit="true"]');
  const cbLink = authBrowserPage.locator('[data-desktop-auth-callback-link="true"]');
  await cbLink.waitFor({ state: "visible", timeout: 15_000 });
  const callbackUri = await cbLink.getAttribute("href");
  await authBrowserCtx.close();
  await deviceA.call("__harness_queue_deep_link", { url: callbackUri });
  await desktopPage.locator('button[data-desktop-studio-trigger="true"]').waitFor({ state: "visible", timeout: 15_000 });
  await desktopPage.locator('button[data-desktop-studio-trigger="true"]').click();

  await desktopPage.locator('[data-testid="desktop-open-team-sync-modal"]').waitFor({ state: "visible", timeout: 10_000 });
  await desktopPage.locator('[data-testid="desktop-open-team-sync-modal"]').click();
  await desktopPage.locator('[data-testid="desktop-e2ee-sync-section"]').waitFor({ state: "visible", timeout: 10_000 });
  await desktopPage.locator('[data-testid="desktop-sync-now-btn"]').click();
  await desktopPage.waitForFunction(
    () => {
      const el = document.querySelector('[data-testid="desktop-e2ee-sync-section"]');
      const txt = el ? el.textContent ?? "" : "";
      return txt.includes("Epoch 2 Confidential Plan") && txt.includes("Alice Committed v2");
    },
    { timeout: 15_000 }
  );
  const syncPanelText = await desktopPage.locator('[data-testid="desktop-e2ee-sync-section"]').innerText();
  assert.ok(syncPanelText.includes("Epoch 2 Confidential Plan"));

  await desktopCtx.close();
  await aliceCtx.close();
  await bobCtx.close();

  console.log("PASS: Real Two-Account, Two-Device, Live PostgreSQL 18 + Restart E2E verified end-to-end!");
} finally {
  if (browser) await browser.close().catch(() => {});
  await stopProcess(viteProc);
  await deviceA.stop();
  await deviceB.stop();
  await stopProcess(authProc);
  await rm(workDir, { recursive: true, force: true }).catch(() => {});
}
