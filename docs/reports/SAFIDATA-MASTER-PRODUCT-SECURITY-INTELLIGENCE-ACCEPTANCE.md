# SafiData — Public Website Pricing, Direct Website Purchase, Origin Hardening & Master Product Acceptance Report

---

## 1. Repository Baseline, Git State & Scope Verification

- **Repository:** `mo7amedrabei14-cell/Safi`
- **Repository Visibility Disclosure:** GitHub repository is currently **Public**. Per mandatory execution rules, repository visibility and production secrets were **not** changed; the owner controls visibility and any credential rotation.
- **Target Branch:** `main` ONLY (zero feature branches created; zero `git commit`, `git push`, deploy, release, publish, or demo preparation performed).
- **Verified Pushed `main` Baseline HEAD SHA:** `bcad6ee92fbc2300f75a29a78b86472c3bce9b5a`
- **Previous Pushed Baseline SHA:** `47ad76a11ef879a6324308a03a362bf9b1cb3e49`
- **Working Tree State:** Contains the unpushed local changes on top of `bcad6ee92fbc2300f75a29a78b86472c3bce9b5a` implementing:
  1. Public Website Pricing experience (`/pricing`) with visible `Plans & Pricing` / `الأسعار والاشتراك` navigation (desktop, tablet, mobile) and balanced Hero CTA.
  2. Strict single-source frontend pricing loader (`apps/web/src/lib/business-pricing.ts`) reading 100% of approved values from `config/business.json` with ZERO hardcoded fallback prices.
  3. Complete Direct Website Purchase journey WITHOUT requiring Desktop (pre-auth selected plan summary, `display_name` signup fix, durable purchase intent surviving registration and email-verification link navigation, authoritative `POST /v1/billing/web/login` + `POST /v1/billing/checkout` + webhook-only entitlement confirmation + post-purchase Download Desktop CTA).
  4. Strict separation of `SAFI_API_BASE_URL` vs `SAFI_PUBLIC_WEB_ORIGIN` across `sf-server`, `sf-desktop`, Tauri `build.rs`/`main.rs`, and Vite `vite.config.ts` with fail-closed staging/production validation and native Tauri contract test.
  5. Root-cause fix for the `bcad6ee` GitHub Actions `make check` failure (`verify-migrations.py` missing RLS on `0013` `workspace_billing_watermarks` & `0015` `trial_anti_abuse_claims`, plus native `uuidv7()` in `0015`).

---

## 2. GitHub Actions CI & PostgreSQL 18.6 Verification (Honest Status)

### 2.1 Pushed Baseline `bcad6ee92fbc2300f75a29a78b86472c3bce9b5a` (Verified via GitHub Actions API)

- **Workflow Run ID:** `45207458048`
- **Workflow Run URL:** `https://github.com/mo7amedrabei14-cell/Safi/actions/runs/45207458048`
- **Job Results on `bcad6ee`:**
  - `PostgreSQL 18.6 migration apply / rollback` (`.../job/137806905337`): **`PASS` (`success`)** — applied `0001..0016`, ran all 6 PG18 suites, rolled back `0016..0001`.
  - `Intro visual regression (Chromium)` (`.../job/137806904363`): **`PASS` (`success`)**.
  - `Desktop shell compile (windows-latest)` (`.../job/137806905084`): **`PASS` (`success`)**.
  - `Full Gate (ubuntu-latest)` (`.../job/137806904901`): **`FAILED`** at `Run full verification gate (make check)` — root cause reproduced locally: `tools/verify-migrations.py` rejected missing RLS/policy on `0013` `workspace_billing_watermarks` and `0015` `trial_anti_abuse_claims` + missing `uuidv7()` in `0015`.
  - `Full Gate (windows-latest)` (`.../job/137806905034`): **`FAILED`** at `Run full verification gate (make check)` — exact same `verify-migrations.py` root cause.
- **Local Remediation Status:** Fixed in `0013_financial_integrity_ledger.up.sql` and `0015_durable_entitlements_usage_and_idempotency.up.sql`; ALL `Makefile` `check` sub-gates (including `verify-migrations.py`, `verify-i18n.py`, `codeburn`, `verify-security.py`, `pnpm audit`) and live PostgreSQL **18.4** `tools/test-migrations.sh` now pass with exit code `0`.

### 2.2 New Unpushed Local Changes on Top of `bcad6ee92fbc2300f75a29a78b86472c3bce9b5a`

- **GitHub Actions CI Status:** **`NOT VERIFIED`** — per mandatory execution rules, no `git commit` or `git push` was run. After the owner reviews and pushes to `main`, record the new commit SHA, run ID, and confirm `Full Gate (ubuntu-latest)`, `Full Gate (windows-latest)`, `PostgreSQL 18.6 migration apply / rollback`, `Intro visual regression`, and `Desktop shell compile` all report `success`.

---

## 3. Public Website Pricing (`/pricing`) & Direct Website Purchase Acceptance

### 3.1 Public Pricing Discovery & Approved Catalog (`config/business.json` Single Source of Truth)

- **Public Route:** `/pricing` (`apps/web/src/pages/PricingPage.tsx`, mounted in `apps/web/src/ApplicationShell.tsx`) — accessible anonymously without signing in and 100% independent of Desktop.
- **Visible Navigation Entry Points:**
  - Public Topbar Navigation (`apps/web/src/components/layout/FloatingGlassDock.tsx`): directly visible `Plans & Pricing` (EN) / `الأسعار والاشتراك` (AR) link (`data-nav-pricing="true"`, `href="/pricing"`, `min-height: 44px`) accessible on desktop, tablet, and mobile without hover, plus entry inside the morphing section pill.
  - Website Hero CTA (`apps/web/src/components/home/BentoCommandCenter.tsx`): prominent, balanced `Plans & Pricing` / `الأسعار والاشتراك` button (`data-hero-pricing-cta="true"`) alongside `Download Desktop App`.
- **Zero Hardcoded Frontend Fallback Prices:**
  - `apps/web/src/lib/business-pricing.ts` (`parseBusinessPricingConfig`) strictly validates `config/business.json` at load time and throws `SF-SYS-032` on any missing or tampered price/limit.
  - Both `BillingPortalPage.tsx` and `DesktopUpgradeModal.tsx` now consume `BUSINESS_PRICING` / `AUTHORITATIVE_CATALOG` exclusively (zero `?? 20` / `?? 1000` / `?? 3` fallback literals).
- **Approved Products & Honest Disclosures Verified:**
  - **Recurring Subscriptions:**
    - Individual Monthly: **`$20`** / **`1,000 EGP`** (Monthly)
    - Individual Annual: **`$168`** / **`8,400 EGP`** (Annual, Save 30%)
    - Organization: **`$17 / seat / month`** / **`850 EGP / seat / month`** (Annual contract: `$204` / `10,200 EGP` per seat/yr; strict minimum `3` seats enforced with live annual total)
  - **One-Time Micro-Purchases (Explicitly Separated from Subscriptions):**
    - Credit Pack 25: **`$5`** / **`250 EGP`** (One-time; 25 additional data operations only, never expires, removes watermark; does NOT unlock AI, teams, or premium recipe packs)
    - Pay-per-file: **`$2`** / **`50 EGP`** (One-time; up to 100,000 rows per purchased file, removes watermark; does NOT unlock AI, teams, or premium recipe packs)
  - **Free Plan (Explicitly Separated):**
    - `5` operations/month, max `5,000` rows, max `20` columns, mandatory Free watermark, NO AI, NO teams, NO premium recipe packs, NO saved recipe writing.

### 3.2 Direct Website Purchase Flow (Without Desktop) + Preserved Desktop Handoff

| Entry Path | Convergent Authoritative Flow | Verified Evidence |
| --- | --- | --- |
| **Direct Website Customer** | Public `/pricing` → select plan/country/seats → `/billing?product=...&country=...[&seats=...]` (pre-auth summary shown immediately) → Sign Up (`POST /v1/auth/register` with `display_name`) → Email Verification (`/?verify_email=...` auto-resumes `/billing` with selected plan/country/seats intact) → `POST /v1/billing/web/login` (`sfb_*`) → `BillingCheckoutController` → `POST /v1/billing/checkout` → Hosted Paddle/Paymob URL → Verified Webhook → Server-Confirmed `paid` + `Download Desktop App` CTA | `PricingPage.test.tsx` (`5/5`), `verify-billing-portal-e2e.mjs` Suite B (live Chromium: 6-seat Egypt EGP `61,200 EGP` survived signup + email verification), `desktop_billing_handoff_and_pg_e2e.rs` (Bob direct web purchase without Desktop) |
| **Desktop Customer (Preserved)** | Desktop `DesktopUpgradeModal` (zero card inputs) → `POST /v1/billing/handoff/start` (`sfs_*`) → Native Tauri `desktop_open_external_billing_url` (validated strictly against `SAFI_PUBLIC_WEB_ORIGIN`) → Website `/billing?handoff=sfbh_...&state=...` (`history.replaceState` strips query immediately) → `POST /v1/billing/web/login` (preserves Desktop `sfs_*`) → `POST /v1/billing/handoff/redeem` → `BillingCheckoutController` → Hosted Provider Checkout → Verified Webhook → `safi://billing?order_id=...` → Desktop refreshes `GET /v1/entitlements/me` | `desktop_billing_handoff_and_pg_e2e.rs` (all 5 products + live PG18), `DesktopUpgradeModal.test.tsx` (`3/3`), `BillingPortalPage.test.tsx` (`5/5`), `verify-billing-portal-e2e.mjs` Suite C (live Chromium), `apps/desktop/src-tauri` native tests (`4/4`) |

---

## 4. API Origin vs Public Website Origin Separation (`SAFI_API_BASE_URL` vs `SAFI_PUBLIC_WEB_ORIGIN`)

- **Problem Closed:** Previously, `billing_handoff_start` and `desktop_pkce_start` derived website URLs from `SAFI_API_BASE_URL`, and Tauri validated external billing/auth browser launches against `SAFI_API_BASE_URL` — breaking whenever API (`https://api.safidata.app`) and Website (`https://safidata.app`) live on separate origins.
- **Implemented Contract:**
  - **`crates/sf-server/src/config.rs`:** Added dedicated `public_web_origin: Option<String>` loaded from `SAFI_PUBLIC_WEB_ORIGIN` and validated via `validate_public_web_origin(raw, strict)`. Staging/Production startup FAILS CLOSED (`SF-SYS-032`) if `SAFI_PUBLIC_WEB_ORIGIN` is missing, `CHANGE_ME_*`, HTTP, localhost, IP literal, path/query/credential-bearing, or a reserved placeholder host (`.example`, `.test`, `.local`, `.invalid`).
  - **`crates/sf-server/src/routes.rs`:** Both `billing_handoff_start` and `desktop_pkce_start` derive redirect URLs strictly from `state.config.resolved_public_web_origin()?` (never `api_base_url` in strict mode).
  - **`crates/sf-server/src/cors.rs` & `security_headers.rs` & `lib.rs`:** Exact configured `SAFI_PUBLIC_WEB_ORIGIN` is trusted for cross-origin API calls (e.g. `https://safidata.app` → `https://api.safidata.app`) while foreign origins (`https://evil.example`) remain blocked (`403 Forbidden`).
  - **`crates/sf-desktop/src/api_origin.rs` & `apps/desktop/src-tauri/{build.rs,src/main.rs}`:** Release desktop builds require and validate BOTH `SAFI_API_BASE_URL` and `SAFI_PUBLIC_WEB_ORIGIN` independently. `desktop_open_external_billing_url` and `desktop_open_external_auth_url` validate strictly against `option_env!("SAFI_PUBLIC_WEB_ORIGIN")` — proven in native Rust unit test `native_external_billing_origin_contract_separates_api_and_public_web_origins` (accepts `https://safidata.app/billing?...`, rejects API-only `https://api.safidata.app/billing?...`, rejects `https://evil.example`, rejects missing origin in release mode, rejects smuggled tokens).
  - **`apps/web/vite.config.ts` & `apps/web/src/lib/desktop-billing-handoff.ts`:** Exposes and validates `__SAFI_PUBLIC_WEB_ORIGIN__` on `SAFI_DESKTOP_RELEASE_BUILD=1`.

---

## 5. Complete Local Verification Results (All Executed Commands Exit Code `0`)

| Command / Suite | Layer | Exit Code | Raw Result |
| --- | --- | --- | --- |
| `bash tools/test-migrations.sh` (live disposable PostgreSQL **18.4**) | Real PostgreSQL 18.4 | **`0`** | `PASS: PostgreSQL 18 apply/constraints/RLS/rollback verified.` — applied `0001..0016` (including updated `0013` & `0015` RLS policies), verified `NOBYPASSRLS` RLS, passed ALL 6 PG18 suites (`account_postgres` [1], `accounts_postgres` [2], `postgres_multi_instance_e2e` [1], `financial_integrity_e2e` [1], `entitlement_and_security_matrix_e2e` [1], `desktop_billing_handoff_and_pg_e2e` [1]), verified rollback guards `0014`/`0013`/`0008`, cleanly rolled back `0016..0001`. |
| `python tools/verify-migrations.py` (+ `--self-test`) | Static PG18 Migration Gate | **`0`** | `OK: 16 migration pair(s) verified (up/down, RLS, uuidv7, no SERIAL, guarded pg_partman).` |
| `cargo fmt --all -- --check` & `cargo clippy --workspace --all-targets -- -D warnings` | Rust Workspace | **`0`** | `0` warnings / `0` errors across all 11 crates. |
| `cargo test -p sf-desktop` & `cargo test -p sf-server` | Server & Desktop Core + Mock Provider | **`0`** | `sf-desktop`: **`102 passed; 0 failed`** · `sf-server`: **`80` unit + `23` integration = `103 passed; 0 failed`** (including `SAFI_PUBLIC_WEB_ORIGIN` strict validation, all 5 Desktop handoff products, Direct Website Purchase without Desktop, cross-origin CORS/CSRF, and failure/retry matrix; + all `7` ignored PG18 tests passed live above). |
| `cargo fmt/clippy/test --manifest-path apps/desktop/src-tauri/Cargo.toml` | Native Windows Tauri Shell + Origin Contract + OS Keystore | **`0`** | **`4 passed; 0 failed; 1 ignored`** (including `native_external_billing_origin_contract_separates_api_and_public_web_origins`, `native_os_keystore_sfk1_to_sfk2_migration_and_zero_secret_on_disk`, IPC payload caps, and 11-case auth verifier). |
| `pnpm run typecheck` · `lint` · `format:check` · `test` · `build` · `check:contrast` | Frontend & Shared Packages | **`0`** | **`71/71`** Vitest files (**`254 passed | 1 skipped`** live Ollama, including `PricingPage.test.tsx` [5], `BillingPortalPage.test.tsx` [5], `DesktopUpgradeModal.test.tsx` [3], `billing-checkout.test.ts` [5]); Vite build clean; `46` text + `36` UI/chart WCAG 2.2 AA contrast pairs OK. |
| `node tools/verify-billing-portal-e2e.mjs` | **Real Chromium Playwright** (`/pricing` + Direct Purchase + Desktop Handoff) | **`0`** | `VERIFY_BILLING_PORTAL_E2E_PASSED`: Desktop (`1440x900`) & Mobile (`390x844`) EN/AR `/pricing` (`0` overflow, 4 full-page screenshots visually verified), Direct Website Purchase without Desktop surviving signup + email verification (`organization_seat_annual`, 6 seats, `EG`, `61,200 EGP` → `paid` + Download Desktop CTA), AND preserved Desktop handoff (`checkoutCalls=1`, `openedUrls.length=1`, `safi://billing?order_id=...`). |
| `node tools/verify-ux-e2e.mjs` | Real Chromium Playwright (Showcase + Auth) | **`0`** | `VERIFY_UX_E2E_PASSED` (all 7 viewports, single Desktop Auth entrance, public Showcase preserved, `0` page errors). |
| `bash tools/codeburn/burn.sh` · `verify-i18n.py` · `gen-errors.py --check` · `check-no-network.sh` · `verify-security.py` · `no-cost-gen.py --verify` · `validate-config.mjs` · `pnpm audit` | Full `Makefile` `check` Sub-Gates | **`0`** | All passed (`No known vulnerabilities found`, `0` network deps in 10 engine crates, `117` synced error codes, `28` i18n keys across 7 locales, `business.json OK`). |

---

## 6. Honest Remaining Blockers & Non-PASS Classifications

1. **GitHub Actions CI Run on These New Local Edits (on top of `bcad6ee`):** **`NOT VERIFIED`** — awaiting owner manual commit & push to `main`.
2. **Real Paddle Sandbox & Production Verification:** **`BLOCKED BY EXTERNAL SETUP`** — local tests use a mock Paddle HTTP server; live Sandbox/Production verification requires owner `SAFI_PADDLE_API_KEY`, `SAFI_PADDLE_WEBHOOK_SECRET`, 5 `SAFI_PADDLE_PRICE_*` IDs, and public HTTPS webhook URL.
3. **Real Paymob Sandbox & Production Verification:** **`BLOCKED BY EXTERNAL SETUP`** — local tests use a mock Paymob HTTP server; live Sandbox/Production verification requires owner `SAFI_PAYMOB_API_KEY`, `SAFI_PAYMOB_PUBLIC_KEY`, `SAFI_PAYMOB_HMAC_SECRET`, `SAFI_PAYMOB_INTEGRATION_ID`, and public HTTPS callback URL.
4. **Live macOS & Linux `SFK2` OS Keystore Runtime Verification:** **`PARTIAL`** — compiled on all 3 OS targets; live runtime tested on Windows Credential Manager only.
5. **Live Local Ollama Model Verification:** **`BLOCKED BY EXTERNAL SETUP`** — Ollama daemon not installed locally (`SAFI_LIVE_OLLAMA_TEST=1` skipped).
6. **External Cloud AI Integration:** **`NOT IMPLEMENTED`** (intentionally disabled by architecture until approved).
7. **Multi-Instance Shared Valkey Rate Limiting & `cargo-audit`/`gitleaks`:** **`PARTIAL`** — single-node IP + account rate limiting verified; shared Valkey rate limiting recommended before multi-node scale-out; `cargo-audit`/`gitleaks` binaries not installed locally.
