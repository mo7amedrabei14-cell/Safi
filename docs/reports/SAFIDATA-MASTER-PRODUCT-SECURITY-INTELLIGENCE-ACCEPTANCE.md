# SafiData — P0 Acceptance Corrections, Complete Desktop-to-Website Billing Flow & Master Product Acceptance Report

---

## 1. Repository Baseline, Git State & Scope Verification

- **Repository:** `mo7amedrabei14-cell/Safi`
- **Repository Visibility Disclosure:** GitHub repository is currently **Public**. Per mandatory execution rules, repository visibility and production secrets were **not** changed; the owner controls visibility and any credential rotation.
- **Target Branch:** `main` ONLY (zero branches created; zero `git commit`, `git push`, deploy, release, or publish performed; zero demo preparation started).
- **Verified Pushed `main` Baseline HEAD SHA:** `47ad76a11ef879a6324308a03a362bf9b1cb3e49`
- **Previous Pushed Baseline SHA:** `0f95f815dcacc829f65667fcb69c97f1e24a9f4c`
- **Working Tree State:** Contains the unpushed local implementation on top of `47ad76a11ef879a6324308a03a362bf9b1cb3e49` closing all P0 acceptance gaps: Complete Desktop-to-Website Billing Flow (Migration `0016`, backend handoff & scoped `sfb_*` website billing sessions, native Tauri external browser launcher + `safi://billing` deep-link handler, real Website `/billing` UI directly integrating `BillingCheckoutController`, Desktop Upgrade modal with zero card inputs), live PostgreSQL 18.4 direct HTTP Free-plan isolation / multi-instance / cold-restart / fail-closed proof, deletion of `.github/workflows/format-repair-once.yml`, live Chromium `/billing` E2E verifier, and this updated report — ready for manual owner review, commit, and push.

---

## 2. Dedicated Acceptance Section — Complete Desktop-to-Website Billing Flow

### 2.1 Architecture & Security Contract (Zero Card Processing Inside Desktop)

- **Strict Boundary:** SafiData Desktop NEVER collects, renders, or processes payment card inputs. Every Upgrade / Subscribe / Buy action launches the user's default system browser at the official SafiData website `/billing` route.
- **Single-Use Short-Lived Handoff (`180s` TTL):**
  - Desktop calls `POST /v1/billing/handoff/start` (`Authorization: Bearer sfs_*` Desktop session only) with `{ product, seats?, billing_country_iso2, coupon_code?, state }`.
  - Backend persists BLAKE3 hashes of `handoff_code` (`sfbh_*`) and `state` in PostgreSQL 18 table `desktop_billing_handoffs` (Migration `0016`, forced RLS), strictly bound to `(user_id, workspace_id)` with `expires_at = now + 180s` and single-use `consumed_at`.
  - Returned URL is strictly `<official_origin>/billing?handoff=sfbh_...&state=...` — **ZERO** access tokens, refresh tokens, session tokens, or reusable credentials appear in the URL.
  - Native Desktop Tauri command `desktop_open_external_billing_url` validates the URL via `sf_desktop::content::validate_desktop_billing_external_url` before launching the OS default browser.
- **Scoped Website Billing Session (`sfb_*`) Preserving Single-Device Desktop Exclusivity:**
  - Unauthenticated website visitors on `/billing` sign in (or register) via `POST /v1/billing/web/login`, which verifies Argon2id credentials (IP + account rate limited) and issues a scoped `sfb_*` Website Billing Session stored hashed in PostgreSQL 18 table `billing_web_sessions`.
  - `POST /v1/billing/web/login` **NEVER** revokes the user's active Desktop `sfs_*` installation.
  - Conversely, `BillingSessionGuard` restricts `sfb_*` tokens strictly to billing & entitlement-read routes (`GET /v1/entitlements/me`, `POST /v1/billing/coupons/preview`, `POST /v1/billing/checkout`, `GET /v1/billing/invoices`, `POST /v1/billing/handoff/redeem`, `GET /v1/billing/orders/{order_id}`, `DELETE /v1/billing/web/session`) and **rejects (`401`)** `sfb_*` tokens on Desktop Studio, AI, Organizations, Sync, Owner, and `/v1/billing/handoff/start` routes.
- **URL Sanitization & Auto-Resume:**
  - `BillingPortalPage` immediately strips `handoff` and `state` from `window.location` via `window.history.replaceState` on mount, keeps them transiently in `sessionStorage` until the user signs in, automatically redeems `POST /v1/billing/handoff/redeem` after login, and clears them immediately afterward.
- **Authoritative Server-Only Confirmation:**
  - Neither `?checkout=success` on the website nor `safi://billing?order_id=...` returning to Desktop is EVER trusted as proof of payment. Entitlements activate strictly after verified provider webhook confirmation (`Paddle` HMAC-SHA256 or `Paymob` 20-field HMAC-SHA512) and are read via authenticated `GET /v1/entitlements/me` & `GET /v1/billing/orders/{order_id}`.

### 2.2 Exact Source Paths & Integration Proof (`BillingCheckoutController` in Real UI)

| Layer | Exact File Path | Functions / Components | Responsibility |
| --- | --- | --- | --- |
| **PostgreSQL 18 Schema** | `crates/sf-server/migrations/0016_desktop_billing_handoffs.{up,down}.sql` | `desktop_billing_handoffs`, `billing_web_sessions` | BLAKE3 hashed single-use handoffs (`180s` TTL) & scoped `sfb_*` website billing sessions with `FORCE ROW LEVEL SECURITY`. |
| **Backend Handoff Service** | `crates/sf-server/src/billing/handoff.rs` | `BillingHandoffService::{start_handoff_at, redeem_handoff_at, issue_billing_web_session_at, authenticate_billing_web_session_at, revoke_billing_web_session}` | Authoritative PG18 persistence + `require_durable_pg` fail-closed (`503 SF-AUTH-005`). |
| **Auth Scope Middleware** | `crates/sf-server/src/auth/middleware.rs` | `BillingSessionGuard`, `require_billing_session` | Accepts Desktop `sfs_*` OR Website Billing `sfb_*` on billing/entitlement-read routes while isolating `sfb_*` from all Desktop/AI/Org/Sync/Owner endpoints. |
| **HTTP Routes** | `crates/sf-server/src/routes.rs` | `billing_web_login`, `billing_web_logout`, `billing_handoff_start`, `billing_handoff_redeem`, `billing_order_status` | Wires `/v1/billing/web/login`, `/v1/billing/web/session`, `/v1/billing/handoff/start`, `/v1/billing/handoff/redeem`, `/v1/billing/orders/{order_id}`. |
| **Native Desktop Boundary** | `crates/sf-desktop/src/content.rs`, `deep_link.rs`, `commands.rs`, `apps/desktop/src-tauri/src/main.rs` | `validate_desktop_billing_external_url`, `DeepLinkAction::BillingReturn`, `desktop_open_external_billing_url` | Strictly validates `/billing?handoff=sfbh_...&state=...`, launches OS default browser, validates `safi://billing?order_id=<uuid>` return (rejecting `status=paid`/`token=`). |
| **Desktop Handoff Client** | `apps/web/src/lib/desktop-billing-handoff.ts`, `apps/web/src/desktop/studio-bridge.ts` | `launchDesktopWebsiteBilling`, `openDesktopExternalBillingUrl` | Calls `POST /v1/billing/handoff/start`, fails closed on tampered URLs (`SF-SYS-009`), invokes allowlisted Tauri IPC command. |
| **Desktop Upgrade UI** | `apps/web/src/desktop/DesktopUpgradeModal.tsx`, `DesktopStudio.tsx`, `ApplicationShell.tsx` | `DesktopUpgradeModal`, `DesktopStudio` (`Plan & Upgrade` toolbar trigger), `safi://billing` listener | Displays live server entitlements, zero card inputs, all 5 purchase triggers launching official website `/billing`, auto-refreshes `GET /v1/entitlements/me` on button click, window focus, and `safi://billing` deep-link receipt. |
| **Website `/billing` UI & Checkout Controller** | `apps/web/src/pages/BillingPortalPage.tsx`, `apps/web/src/lib/billing-checkout.ts`, `apps/web/src/ApplicationShell.tsx` | `BillingPortalPage`, `BillingCheckoutController::{startCheckout, verifyServerPaymentCompletion, fetchOrderStatus, fetchEntitlements, loginBillingWeb, redeemDesktopHandoff, previewCoupon}` | Mounted at `/billing` in `ApplicationShell.tsx`; strips `handoff`/`state` via `history.replaceState`; directly invokes `BillingCheckoutController` with synchronous double-click lock (`checkoutInFlightRef`), stable `sessionStorage` idempotency keys, hosted provider `checkout_url` open, `checkout_pending`/`reconciliation_required`/`failed`/`canceled`/`expired`/`paid` states, and `safi://billing?order_id=...` return link. |

### 2.3 All 5 Purchase Types & Failure/Retry Matrix Verification

| Scenario / Purchase Type | Local Automated & Chromium E2E Verification | Real Paddle / Paymob Sandbox | Status |
| --- | --- | --- | --- |
| **Individual Monthly** (`$20` / `1,000 EGP`) | `desktop_billing_handoff_and_pg_e2e`, `BillingPortalPage.test.tsx`, `DesktopUpgradeModal.test.tsx` | Requires owner sandbox credentials | **`PASS`** (Local/PG18/Mock) · **`BLOCKED BY EXTERNAL SETUP`** (Live Sandbox) |
| **Individual Annual** (`$168` / `8,400 EGP`) | `desktop_billing_handoff_and_pg_e2e`, `BillingPortalPage.test.tsx`, `DesktopUpgradeModal.test.tsx` | Requires owner sandbox credentials | **`PASS`** (Local/PG18/Mock) · **`BLOCKED BY EXTERNAL SETUP`** (Live Sandbox) |
| **Organization Subscription (Seats `>= 3`)** (`$17`/`850 EGP` per seat/mo; `<3` rejected) | `desktop_billing_handoff_and_pg_e2e` (5 seats = `$1,020`), `verify-billing-portal-e2e.mjs` (live Chromium), `DesktopUpgradeModal.test.tsx` | Requires owner sandbox credentials | **`PASS`** (Local/PG18/Mock/Chromium) · **`BLOCKED BY EXTERNAL SETUP`** (Live Sandbox) |
| **Credit Pack 25** (`$5` / `250 EGP`) | `desktop_billing_handoff_and_pg_e2e`, `BillingPortalPage.test.tsx`, `DesktopUpgradeModal.test.tsx` | Requires owner sandbox credentials | **`PASS`** (Local/PG18/Mock) · **`BLOCKED BY EXTERNAL SETUP`** (Live Sandbox) |
| **Pay-Per-File** (`$2` / `50 EGP`) | `desktop_billing_handoff_and_pg_e2e`, `BillingPortalPage.test.tsx`, `DesktopUpgradeModal.test.tsx` | Requires owner sandbox credentials | **`PASS`** (Local/PG18/Mock) · **`BLOCKED BY EXTERNAL SETUP`** (Live Sandbox) |
| **Unauthenticated browser sign-in + auto-resume handoff** | `desktop_billing_handoff_and_pg_e2e`, `verify-billing-portal-e2e.mjs` (live Chromium) | N/A (SafiData auth) | **`PASS`** |
| **Cross-user handoff theft blocked (`User B` redeeming `User A` handoff)** | `desktop_billing_handoff_and_pg_e2e` (`403 SF-AUTH-012`) | N/A | **`PASS`** |
| **Expired (`>180s`), replayed, & wrong-`state` handoffs rejected** | `desktop_billing_handoff_and_pg_e2e` (in-memory + live PG18 across 3 instances) | N/A | **`PASS`** |
| **Scoped `sfb_*` preserves active Desktop `sfs_*` & blocked from Desktop Studio/AI/Org/Sync/Owner** | `desktop_billing_handoff_and_pg_e2e` (`sfs_*` remains `200 OK`; `sfb_*` gets `401` on `/v1/ai/route`, `/v1/organizations`, `/v1/billing/handoff/start`, `/v1/owner/stats`) | N/A | **`PASS`** |
| **Double-click Pay button -> strictly 1 API call & 1 checkout tab** | `verify-billing-portal-e2e.mjs` (live Chromium caught & verified `checkoutInFlightRef` fix: `checkoutCalls=1`, `openedUrls.length=1`) | N/A | **`PASS`** |
| **Return before webhook (`?checkout=success` or `safi://billing`) -> still Free/pending** | `desktop_billing_handoff_and_pg_e2e`, `BillingPortalPage.test.tsx`, `verify-billing-portal-e2e.mjs` | N/A | **`PASS`** |
| **Provider timeout -> `reconciliation_required` blocks duplicate charge** | `desktop_billing_handoff_and_pg_e2e` (`503 SF-PAY-008`, `provider_calls=1`), `BillingPortalPage.test.tsx` | Requires live provider timeout test | **`PASS`** (Local/PG18/Mock) · **`BLOCKED BY EXTERNAL SETUP`** (Live Sandbox) |

---

## 3. PostgreSQL 18.4 Direct HTTP Verification (Durability, Isolation & Fail-Closed)

Executed against a **real live PostgreSQL 18.4** server via `tools/test-migrations.sh` (`postgres_18_direct_http_free_isolation_desktop_handoff_multi_instance_and_fail_closed` + `postgres_18_durable_entitlements_multi_instance_concurrency_and_restart` + `p0_postgres_18_financial_integrity_multi_instance_concurrency_and_restart`):

1. **Direct HTTP Free-Plan Isolation on Live PG18 (Instance A):**
   - `POST /v1/ai/route` -> **`403 SF-LIC-008`**
   - `POST /v1/organizations` -> **`403 SF-LIC-008`**
   - `POST /v1/sync/envelopes` -> **`403 SF-LIC-008`**
   - `POST /v1/templates/entitlement/export-import` -> **`403 SF-RCP-005`**
2. **Direct HTTP Multi-Instance Quota Durability & Fingerprint Enforcement (Instances A, B & Cold-Restart C):**
   - 5 Free `data_operation` calls split across Instance A & Instance B all returned **`200 OK`**.
   - Cold-Restart Instance C immediately read `operations_used_this_month == 5` from PostgreSQL 18 via `GET /v1/entitlements/me`.
   - 6th Free operation on Instance C rejected with **`429 SF-LIC-005`**.
   - Replaying op #1 idempotency key with tampered `row_count` or `kind: ai_call` rejected with **`409 SF-PAY-007`**.
3. **Direct HTTP Cross-Instance Handoff Lifecycle (Instances A -> B -> C):**
   - `POST /v1/billing/handoff/start` on Instance A persisted hashed single-use code in PG18 `desktop_billing_handoffs`.
   - `POST /v1/billing/handoff/redeem` on Instance B succeeded (`200 OK`).
   - Replaying the exact same handoff code on Cold-Restart Instance C rejected (`401 SF-AUTH-006`).
4. **Direct HTTP PostgreSQL Outage Fail-Closed Proof:**
   - Closed `pool_c.close().await` simulating live PostgreSQL outage on strict Instance C:
   - `GET /v1/entitlements/me` -> **`503 SF-AUTH-005`**
   - `POST /v1/entitlements/consume` -> **`503 SF-AUTH-005`**
   - `POST /v1/billing/handoff/start` -> **`503 SF-AUTH-005`**
   - `POST /v1/billing/handoff/redeem` -> **`503 SF-AUTH-005`**
   - Zero silent fallback to in-memory Free/Paid state.

---

## 4. GitHub Actions & PostgreSQL 18.6 CI Status (Honest Reporting)

- **Status for Current Local Unpushed Changes (on top of `47ad76a11ef879a6324308a03a362bf9b1cb3e49`):** **`NOT VERIFIED`**
- **Reason:** Per mandatory rules, **no `git commit` or `git push`** was executed by the agent. Therefore, **no GitHub Actions workflow run exists yet** for these local edits.
- **Fields to Record After Owner Manual Commit & Push:**
  - Commit SHA: `<pending owner commit & push>`
  - GitHub Actions Run URL: `<pending owner push>`
  - `Full Gate (ubuntu-latest)`: **`NOT VERIFIED`** (awaiting push)
  - `Full Gate (windows-latest)`: **`NOT VERIFIED`** (awaiting push)
  - `PostgreSQL 18.6 Migrations & E2E`: **`NOT VERIFIED`** (awaiting push; verified locally on real PostgreSQL **18.4** via `tools/test-migrations.sh`, exit code `0`)
  - `CI` overall status: **`NOT VERIFIED`** (awaiting push)

---

## 5. 27-Point Security Audit Summary (Updated)

| # | Audit Area | Status | Concrete Evidence / Residual Notes |
| --- | --- | --- | --- |
| 1 | Secrets & credentials scan (working tree + Git history) | **`PASS`** (Custom scanner; external `gitleaks`/`trufflehog` not installed locally) | `0` secrets / `.env` / `.key` / `.pem` in repo or `apps/web/dist`; zero tokens in Desktop-to-Website handoff URLs (`validate_desktop_billing_external_url` + `history.replaceState`). |
| 2 | Authentication & session management | **`PASS`** | Argon2id, hashed `sfs_*`/`sfw_*`/new scoped `sfb_*` sessions, single-device exclusivity preserved across Website Billing login, PKCE `S256`, Ed25519 online leases. |
| 3 | Password reset & email verification | **`PASS`** | Single-use hashed tokens, expiry, existence-blind responses, session revocation on reset. |
| 4 | Authorization & RBAC | **`PASS`** | Sealed Owner UUID + RFC 6238 TOTP + step-up TOTP + server-side Free/Paid/Org guards + `sfb_*` strict billing-only scope isolation. |
| 5 | IDOR / tenant isolation | **`PASS`** | `list_org_members` IDOR blocked (`403 SF-SYN-005`), `GET /v1/billing/orders/{order_id}` tenant-scoped, cross-user handoff redemption blocked (`403 SF-AUTH-012`), PG18 `FORCE ROW LEVEL SECURITY` verified under `NOBYPASSRLS`. |
| 6 | Billing & payment security | **`PARTIAL`** (Local/PG18/Mock/Chromium **`PASS`**; Real Paddle & Paymob Sandbox/Prod **`BLOCKED BY EXTERNAL SETUP`**) | Desktop-to-Website external browser flow + Migration `0016` + Paddle HMAC-SHA256 + Paymob 20-field HMAC-SHA512 + `reconciliation_required` duplicate-charge block (`SF-PAY-008`) + real `BillingCheckoutController` UI integration. |
| 7 | Entitlement & quota security | **`PASS`** | Durable PG18 Migration `0015` + 32-byte SHA-256 fingerprint (`409 SF-PAY-007`) + direct HTTP Free-plan isolation, multi-instance, cold restart, and closed-pool `503 SF-AUTH-005` fail-closed verified on live PG18.4. |
| 8 | Input validation & schema validation | **`PASS`** | Strict Rust/serde validation, `validate_desktop_billing_external_url`, `DeepLinkAction::BillingReturn` UUID validation, `validate-config.mjs`, `errors.toml` sync. |
| 9 | SQL injection & query safety | **`PASS`** | 100% parameterized SQLx queries (`$1..$N`) across all repositories including Migration `0016` `desktop_billing_handoffs` & `billing_web_sessions`. |
| 10 | Command / OS injection | **`PASS`** | External browser launch uses direct OS APIs (`cmd /C start` avoided; uses validated exact-origin URL only, zero shell interpolation of untrusted args). |
| 11 | SSRF & outbound network control | **`PASS`** | Engine crates verified `0` network deps (`check-no-network.sh`); Ollama loopback-only; provider base URLs strictly from server env. |
| 12 | Path traversal & file handling | **`PASS`** | Atomic temp -> `fsync` -> rename writes; never overwrites original user files; Tauri IPC allowlist (`83` commands) & payload caps verified. |
| 13 | XSS & DOM sanitization | **`PASS`** | Zero `eval`/`new Function`/`innerHTML`; ChartStudio SVG renderer XML-escaped (`chart-spec.test.ts`). |
| 14 | CSRF & CORS | **`PASS`** | Strict CORS allowlist + `security_headers_and_csrf_middleware` blocking foreign `Origin` / `Sec-Fetch-Site: cross-site` on state-changing routes (`403`). |
| 15 | File upload / workbook parsing DoS | **`PASS`** | `sf-io` enforces `PlatformLimits` (`SF-IMP-003`) & ZIP-bomb caps (`SF-IMP-005`), never executes macros or external formulas. |
| 16 | Formula injection & export safety | **`PASS`** | `sanitizeCellForExport` neutralizes `=`, `+`, `-`, `@`, `\t`, `\r` in CSV/Excel exports. |
| 17 | Cryptography & local encryption | **`PARTIAL`** (Windows **`PASS`** live; macOS/Linux **`PARTIAL`** compile-only) | Audited RustCrypto (`XChaCha20-Poly1305`, `Argon2id`, `Ed25519`, `BLAKE3`, `HMAC-SHA256/512`, `subtle`); Windows Credential Manager `SFK1 -> SFK2` verified live; macOS/Linux require live host verification. |
| 18 | Desktop / Tauri / WebView security | **`PASS`** | `withGlobalTauri: false`, strict CSP, allowlisted IPC parity (`shell_config` + `apps/desktop/src-tauri` tests passed), strict external billing URL allowlist. |
| 19 | Rate limiting & abuse prevention | **`PARTIAL`** (Single-node **`PASS`**; shared Valkey RL pending multi-node scale-out) | Independent IP + account sliding windows across auth, owner login, `POST /v1/billing/web/login`, checkout, consume, and trial claim; per-instance memory limitation explicitly noted. |
| 20 | Error handling & info leakage | **`PASS`** | RFC 9457 `problem+json` with canonical `errors.toml` codes; zero stack traces or secret leaks. |
| 21 | Logging & telemetry privacy | **`PASS`** | BLAKE3 hash-chained `AuditLogCenter` + redacted `Debug` impls; zero raw user cell/file values or handoff tokens in logs. |
| 22 | Security headers & TLS | **`PASS`** (App & static headers; hosting TLS external) | API middleware & static `_headers` emit HSTS, CSP, `nosniff`, `DENY`, `no-referrer`, `Permissions-Policy`; API emits `no-store`. Residual: static `_headers` retains `style-src 'self' 'unsafe-inline'` for React inline geometry styles. |
| 23 | Frontend bundle & public asset exposure | **`PASS`** | Clean Vite production build (`apps/web/dist`), zero sourcemaps, zero leaked server env/secret values. |
| 24 | Dependency & supply chain security | **`PARTIAL`** (`pnpm audit` **`PASS`**; `cargo-audit` not installed locally) | All Cargo & pnpm deps locked (`--frozen-lockfile --ignore-scripts`); `cargo-audit` to be run in CI / pre-launch. |
| 25 | Configuration & production fail-closed | **`PASS`** | `ServerConfig::validate_for_startup()` + `require_durable_pg` across `EntitlementService`, `BillingService`, and `BillingHandoffService` fail closed (`503 SF-AUTH-005`) on DB unavailability. |
| 26 | CI/CD & workflow security | **`PASS`** | **Deleted** one-time `.github/workflows/format-repair-once.yml` (`contents: write`) completely; `ci.yml` keeps `permissions: contents: read` & SHA-pinned actions (`dtolnay/rust-toolchain@stable` moving ref noted). |
| 27 | Local AI & privacy boundaries | **`PARTIAL`** (Local HTTP contract **`PASS`**; live Ollama **`BLOCKED BY EXTERNAL SETUP`**; Cloud AI **`NOT IMPLEMENTED`**) | Loopback-only Ollama, explicit consent, PII masking (`sanitizePromptPii`), untrusted DSL/formula rejection verified (`4 passed; 1 skipped` live Ollama). Cloud AI remains disabled / **`NOT IMPLEMENTED`**. |

---

## 6. Exact Local Verification Commands, Exit Codes & Raw Results (Post-`47ad76a` P0 Corrections)

| Command | Layer | Exit Code | Raw Result |
| --- | --- | --- | --- |
| `bash tools/test-migrations.sh` (live disposable PostgreSQL **18.4**) | Real PostgreSQL 18.4 | **`0`** | `PASS: PostgreSQL 18 apply/constraints/RLS/rollback verified.` — applied `0001..0016`, verified `NOBYPASSRLS` RLS, passed ALL 6 PG18 suites: `account_postgres` (1), `accounts_postgres` (2), `postgres_multi_instance_e2e` (1), `financial_integrity_e2e` (1), `entitlement_and_security_matrix_e2e` (1), **`desktop_billing_handoff_and_pg_e2e` (1 — direct HTTP Free isolation + multi-instance handoff + cold restart + closed-pool `503 SF-AUTH-005` fail-closed)**, verified rollback guards `0014`/`0013`/`0008`, cleanly rolled back `0016..0001`. |
| `cargo test -p sf-desktop` | Desktop Core & IPC Boundary | **`0`** | **`102 passed; 0 failed`** (`91` unit including `validate_desktop_billing_external_url` & `safi://billing` deep-link + `5` `shell_config` confirming 83-command parity + `3` `studio_phase5_e2e` + `1` `deep_link_fuzz` + `1` `high_dpi_platform` + `1` `s5_acceptance`). |
| `cargo test -p sf-server` | Server Unit + HTTP + Mock Provider | **`0`** | `79` unit + `11` `auth_flow` + `1` `desktop_billing_handoff_and_pg_e2e` (all 5 purchase types + full failure matrix) + `1` `dual_database_owner_templates_e2e` + `3` `entitlement_and_security_matrix_e2e` + `3` `financial_integrity_e2e` + `1` `log_center` + `2` `security_online_authorization_e2e` + `1` `zero_trust_guard` = **`102 passed; 0 failed`** (+ all `7` ignored PG18 tests passed live above). |
| `cargo fmt --all -- --check` & `cargo clippy --workspace --all-targets -- -D warnings` | Rust Workspace Quality | **`0`** | Clean (`0` warnings / `0` errors across all 11 crates). |
| `cargo fmt/clippy/test --manifest-path apps/desktop/src-tauri/Cargo.toml` | Native Windows Tauri Shell + OS Keystore | **`0`** | `3 passed; 0 failed; 1 ignored` (including `desktop_open_external_billing_url` registration & live Windows Credential Manager `SFK1 -> SFK2`). |
| `pnpm run typecheck` · `lint` · `format:check` · `test` · `build` · `check:contrast` | Frontend & Shared Packages | **`0`** | **`70/70`** Vitest files (**`249 passed | 1 skipped`** live Ollama, including `BillingPortalPage.test.tsx` [5], `DesktopUpgradeModal.test.tsx` [3], `billing-checkout.test.ts` [5]); Vite production build clean; `46` text + `36` UI/chart contrast pairs OK. |
| `node tools/verify-billing-portal-e2e.mjs` | Real Chromium Playwright (`/billing` Portal) | **`0`** | `VERIFY_BILLING_PORTAL_E2E_PASSED`: URL `handoff`/`state` stripped via `history.replaceState`, Website login auto-redeemed 5-seat Org handoff (`$1,020`), double-click Pay fired strictly `1` API call & `1` checkout tab, return-before-webhook stayed non-paid, post-webhook verification showed `paid` + `safi://billing?order_id=...`. |
| `node tools/verify-ux-e2e.mjs` | Real Chromium Playwright (Showcase + Auth) | **`0`** | `VERIFY_UX_E2E_PASSED` (all 7 viewports, single Desktop Auth entrance, public showcase untouched, `0` page errors). |
| `python tools/gen-errors.py --check` · `bash tools/check-no-network.sh` · `node tools/validate-config.mjs` | Static Contracts | **`0`** | `errors.toml` synced (`117` codes); `10` engine crates have `0` network deps; `business.json OK`. |

---

## 7. External Setup Blockers & Honest Status Classifications

1. **GitHub Actions & PostgreSQL 18.6 CI Run on These Edits:** **`NOT VERIFIED`** — awaiting owner manual commit & push to `main`.
2. **Real Paddle Sandbox & Production Verification:** **`BLOCKED BY EXTERNAL SETUP`** — requires owner `SAFI_PADDLE_API_KEY`, `SAFI_PADDLE_WEBHOOK_SECRET`, 5 `SAFI_PADDLE_PRICE_*` IDs, and public HTTPS webhook URL.
3. **Real Paymob Sandbox & Production Verification:** **`BLOCKED BY EXTERNAL SETUP`** — requires owner `SAFI_PAYMOB_API_KEY`, `SAFI_PAYMOB_PUBLIC_KEY`, `SAFI_PAYMOB_HMAC_SECRET`, `SAFI_PAYMOB_INTEGRATION_ID`, and public HTTPS callback URL.
4. **Live macOS & Linux `SFK2` OS Keystore Runtime Verification:** **`PARTIAL`** — compiled on all 3 OS targets; live runtime tested on Windows Credential Manager only.
5. **Live Local Ollama Model Verification:** **`BLOCKED BY EXTERNAL SETUP`** — Ollama daemon not installed locally (`SAFI_LIVE_OLLAMA_TEST=1` skipped).
6. **External Cloud AI Integration:** **`NOT IMPLEMENTED`** (intentionally disabled by architecture until approved).
7. **Multi-Instance Shared Valkey Rate Limiting & `cargo-audit` / `gitleaks`:** **`PARTIAL`** — single-node IP+account rate limiting verified (`PASS`), shared Valkey rate limiting recommended before multi-node scale-out; external `cargo-audit`/`gitleaks` binaries not installed on this workstation.
