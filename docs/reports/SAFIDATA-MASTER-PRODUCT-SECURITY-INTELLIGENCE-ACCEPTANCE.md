# SafiData — Complete Product Acceptance, Financial Integrity & Provider Protocol Master Report

---

## 1. Repository Baseline, Git State & Live GitHub Actions CI (Verified)

- **Repository:** `mo7amedrabei14-cell/Safi`
- **Repository Visibility Risk Disclosure:** GitHub repository is currently **Public**. Per mandatory execution rules, repository visibility and production secrets were **not** modified; the owner decides visibility and any secret rotation.
- **Target Branch:** `main` ONLY (zero branches created; zero `git commit`, `git push`, deploy, release, or publish performed).
- **Latest Pushed `main` HEAD SHA (Verified):** `e7b6472e072cdd38bdb2739f8ee4a873ee13dedb`
- **Previous Pushed Commit SHA:** `c1c4d234ceca740ca89957c3c992da9a68209e67`
- **Working Tree State:** Contains the post-push remediation edits on top of `e7b6472e072cdd38bdb2739f8ee4a873ee13dedb` (generated error artifacts sync, `CreditPack25` `422` & cross-instance checkout leader fixes, official Paymob & Paddle webhook envelopes + JSON fixtures, Paddle `ready`/`draft` checkout lifecycle, real Paymob customer billing data, cumulative Paymob partial refunds, recurring monthly/annual subscription renewals, Migration `0014`, webhook payload-hash anomaly tracking `SF-PAY-009`, zero swallowed DB writes, and this updated report — awaiting manual owner review and push).
- **GitHub Actions CI Status for Pushed `main` HEAD `e7b6472e072cdd38bdb2739f8ee4a873ee13dedb` (Run `45149820772`):**
  - `Intro visual regression (Chromium)`: **`success`** (fixed in `e7b6472`)
  - `Desktop shell compile (windows-latest)`: **`success`**
  - `Full Gate (ubuntu-latest)` & `Full Gate (windows-latest)`: **`failure`** at `python3 tools/gen-errors.py --check` because `crates/sf-core/src/generated_errors.rs` and `packages/i18n/src/generated-errors.ts` were out of sync with `errors.toml`. **Fixed and verified locally (`PASS`)** via `tools/gen-errors.py` & `tools/gen-errors.py --check`.
  - `PostgreSQL 18.6 migration apply / rollback`: **`failure`** — applied `0001..0013` and passed `postgres_multi_instance_e2e`, then failed `p0_postgres_18_financial_integrity_multi_instance_concurrency_and_restart` with HTTP `422 Unprocessable Entity` on `POST /v1/billing/checkout` (root cause: `CheckoutProduct::CreditPack25` serialized under serde `snake_case` as `"credit_pack25"` instead of `"credit_pack_25"`, mismatching `CheckoutProduct::as_str()`/`from_str_code()`). **Fixed and verified live against real local PostgreSQL 18.4 (`tools/test-migrations.sh` exit code `0`, `PASS`)**, which also uncovered and led to fixing a real cross-instance checkout leader race (`mock.paymob_create_calls == 1` across 20 concurrent calls on 2 instances) and a `0013.down.sql` partial-refund rollback guard.
  - **Remote CI Verdict on `e7b6472` (Pre-Push of This Remediation):** **`FAIL`** — remote CI will only be considered green after the owner pushes this working tree and every GitHub Actions job completes `success`.

---

## 2. Seven-Layer Financial Proof Matrix & Explicit Post-Push Remediation Status

Allowed statuses ONLY: `PASS`, `PARTIAL`, `FAIL`, `BLOCKED BY EXTERNAL SETUP`, `NOT IMPLEMENTED`.

Proof layers distinguished explicitly:
**(1) Source implementation**, **(2) Unit tests**, **(3) Local HTTP contract tests with official payload fixtures**, **(4) Real PostgreSQL 18 concurrency/restart/migration tests**, **(5) Real Paddle Sandbox tests**, **(6) Real Paymob Test/Sandbox tests**, **(7) Production readiness with owner credentials & public HTTPS webhooks**.

| # | Area / Requirement | Status | Proof Layers & Concrete Verification Evidence |
| --- | --- | --- | --- |
| **1** | **Generated Error Files Sync (`errors.toml` -> Rust & TS)** | **`PASS`** | Regenerated `crates/sf-core/src/generated_errors.rs` & `packages/i18n/src/generated-errors.ts` (117 error codes including `SF-PAY-007`, `SF-PAY-008`, `SF-PAY-009`). Verified `python tools/gen-errors.py --check` exits `0`. |
| **2** | **PostgreSQL 18 Checkout `422` & Cross-Instance Concurrency** | **`PASS`** (Layers 1–4 **`PASS`** on real PostgreSQL 18.4) | Added `#[serde(rename = "credit_pack_25")]` on `CheckoutProduct::CreditPack25`, captured raw non-JSON bodies in `send_http_json`, enforced PostgreSQL cross-instance leader wait (only `PgCheckoutClaim::Created` calls provider; followers wait for persisted `provider_order_ref` with `COALESCE` immutability). Verified live on PostgreSQL 18.4 via `tools/test-migrations.sh`: all 20 concurrent calls across Instance A & B return `200 OK`, identical `order_id` & `provider_order_ref`, and `mock.paymob_create_calls == 1`. |
| **3** | **Real Paymob Callback DTO Compatibility (`{"type":"TRANSACTION","obj":{...}}`)** | **`PARTIAL`** (Layers 1–4 **`PASS`** against official Paymob JSON fixtures & PG18; Layers 6–7 **`BLOCKED BY EXTERNAL SETUP`**) | Replaced custom flat `PaymobTransactionObj` with official `PaymobCallbackEnvelope` (`type == "TRANSACTION"`, nested `obj.id`, `obj.order.id`, `obj.source_data.{pan,sub_type,type}`, `obj.refunded_amount_cents`, `issuer_bank`, `transaction_processed_callback_responses`). HMAC-SHA512 verified over all 20 nested fields in exact documented order; `routes.rs::paymob_webhook` parses raw bytes & rejects missing `?hmac` with `401 SF-PAY-002`; never confuses `obj.id` with `obj.order.id`; rejects tampered unsigned `obj.data` metadata. Verified with official fixtures (`tests/fixtures/paymob/*.json`) in `financial_integrity_e2e` (in-memory & live PG18). |
| **4** | **Paymob Cumulative Partial Refunds (`obj.refunded_amount_cents`)** | **`PARTIAL`** (Layers 1–4 **`PASS`**; Layers 6–7 **`BLOCKED BY EXTERNAL SETUP`**) | Processes only positive delta against PostgreSQL `billing_invoices.refunded_amount_minor` (`FOR UPDATE`), ignores repeated or out-of-order lower cumulative values without corruption, rejects amounts exceeding captured `amount_cents`, keeps subscription active on partial refund, revokes entitlement on full cumulative refund, and persists every transition in `billing_refunds`. Verified in `financial_integrity_e2e` (unit/mock & live PG18). |
| **5** | **Paymob Real Customer Billing Data (Zero Fake Personal Details)** | **`PARTIAL`** (Layers 1–4 **`PASS`**; Layers 6–7 **`BLOCKED BY EXTERNAL SETUP`**) | Removed hardcoded `billing@safidata.app`, `+201000000000`, `SafiData`/`Customer` from `PaymobApiAdapter::create_intention()`. Requires validated `CustomerBillingData { first_name, last_name, email, phone_number, country }` (defaults blank email only to verified authenticated account email; never collects/stores card PAN/CVV; rejects missing/invalid billing data with `InvalidRequest`). |
| **6** | **Real Paddle Webhook DTO Compatibility (`event_id`, `event_type`, RFC 3339 `occurred_at`, `notification_id`, nested `data`)** | **`PARTIAL`** (Layers 1–4 **`PASS`** against official Paddle JSON fixtures & PG18; Layers 5 & 7 **`BLOCKED BY EXTERNAL SETUP`**) | Implemented official `PaddleWebhookEnvelope` + exact zero-dependency RFC 3339 UTC parser (`parse_rfc3339_to_unix_secs`), strict `currency_code` requirement (never defaults missing currency to USD), `data.items[].price.id`, `data.details.totals.total`, `data.subscription_id`, `data.invoice_id`, `data.billing_period`, and official `adjustment.{created,updated}` (`refund` approved vs `pending_approval`, `chargeback`). Verified against official fixtures (`tests/fixtures/paddle/*.json`) and payload-hash mismatch security anomaly (`409 SF-PAY-009` persisted in `billing_security_anomalies`). |
| **7** | **Paddle Draft / Ready Checkout API Lifecycle** | **`PARTIAL`** (Layers 1–4 **`PASS`**; Layers 5 & 7 **`BLOCKED BY EXTERNAL SETUP`**) | `PaddleApiAdapter::create_transaction` supports BOTH `ready` transactions (`customer_id` + `address_id` -> `CheckoutMode::RedirectUrl` with hosted `checkout.url`) AND `draft` transactions (without customer/address -> `CheckoutMode::PaddleJsOverlay` with nullable `checkout.url` and server-verified `transaction_id`). Validates non-missing `currency_code == "USD"`, `status`, `items[].price.id`, and `details.totals.total`, and reconciles timeouts via `GET /transactions?status=draft,ready,billed,completed&per_page=50`. |
| **8** | **Paddle Recurring Subscription Renewals (Separate From Initial Purchase)** | **`PASS`** (Layers 1–4 **`PASS`** on real PostgreSQL 18.4; Layers 5 & 7 **`BLOCKED BY EXTERNAL SETUP`**) | Migration `0014_recurring_subscriptions_and_integrity.{up,down}.sql` persists `provider_subscription_id` across `billing_orders`, `billing_invoices`, `billing_entitlement_grants`, and `workspace_entitlement_state`; scopes initial-order uniqueness to non-renewal grants while enforcing `UNIQUE (provider, provider_transaction_id)` and `UNIQUE (provider, provider_subscription_id, period_end)` for renewals. Verified 2+ successive monthly renewals (including on real PG18 across 2 instances + restart) and 2+ successive annual renewals, plus duplicate/different-event-id replay protection. |
| **9** | **Financial Error Propagation, Failure Classification & Safe Refund Retries** | **`PASS`** (Layers 1–4 **`PASS`** on real PostgreSQL 18.4) | Removed all swallowed `let _ =` DB writes in `pg_ledger.rs` & `billing/mod.rs`. Strict `DefinitiveFailure` (pre-send TCP/TLS error or `4xx`) vs `UncertainOutcome` (post-send I/O error, timeout, `5xx`, truncated/oversized/malformed response -> `SF-PAY-008`, coupon NOT released, refund marked `reconciliation_required` keeping `refund_pending` and entitlements intact, idempotent retry does NOT double-refund). |
| **10** | **PostgreSQL 18.4 Migrations (`0001..0014`) & Non-Destructive Rollbacks** | **`PASS`** (Real local PostgreSQL 18.4 verified; GitHub Actions PG18.6 awaits owner push) | Executed `bash tools/test-migrations.sh` live against PostgreSQL 18.4: `0001..0014` apply, RLS under `NOBYPASSRLS` role, `account_postgres`, `accounts_postgres`, `postgres_multi_instance_e2e`, `financial_integrity_e2e`, explicit non-destructive rollback guards (`0014`, `0013`, `0008`), and full `0014..0001` rollback all exited `0`. |
| **11** | **Provider Sandbox & Production Live Integration (Paddle & Paymob)** | **`BLOCKED BY EXTERNAL SETUP`** | Real Paddle Sandbox (Layer 5), real Paymob Test/Sandbox (Layer 6), and production credentials + public HTTPS webhook URLs (Layer 7) are not configured on this machine. Local HTTP mock servers prove protocol/concurrency logic only and are NOT equated with live provider sandboxes. |
| **12** | **OS-Backed Vault Key Custody (`SFK2`) — Per-Platform Evidence** | **`PARTIAL`** (Windows **`PASS`** live; macOS & Linux **`PARTIAL`** — compiled only, not runtime-tested on actual macOS/Linux hosts) | **Windows:** Live runtime test `native_os_keystore_sfk1_to_sfk2_migration_and_zero_secret_on_disk` **`PASS`** against real Windows Credential Manager (verifies `SFK1 -> SFK2`, zero raw secret bytes on disk, existing `.safi.enc` preserved, plus recovery key export/restore & locked keystore rollback in `sf-desktop`). **macOS Keychain & Linux Secret Service:** Compiled via pinned `keyring = "=3.6.3"` platform targets, but NOT yet runtime-tested on real macOS or Linux hosts. |
| **13** | **Local Ollama Inference vs Mock & Cloud AI Status** | **`PARTIAL`** (Local HTTP contract, PII redaction, untrusted DSL validation **`PASS`**; Live Ollama model **`BLOCKED BY EXTERNAL SETUP`**; Optional External Cloud AI **`NOT IMPLEMENTED`** / disabled) | `apps/web/src/desktop/ai-interpreter.ts` & `ai-interpreter.test.ts` prove loopback-only boundary, explicit consent, `sanitizePromptPii()` wire redaction, formula/script/prototype injection rejection, abort/error handling against a real local Node HTTP mock (`4 passed; 1 skipped`). Live Ollama daemon is not installed locally (`SAFI_LIVE_OLLAMA_TEST=1` skipped). External Cloud AI remains disabled and is NOT claimed implemented. |

---

## 3. Preserved Earlier SafiData Product Agreements (Section 12 Status)

| Capability | Status | Evidence |
| --- | --- | --- |
| Local-first dual database architecture & zero-network engine crates | **`PASS`** | `bash tools/check-no-network.sh` (`0` network deps in all 10 engine crates); raw user data never leaves device. |
| Website Showcase public (no forced sign-in) vs Desktop mandatory auth | **`PASS`** | `verify-runtime-boundary-e2e.mjs`, `verify-intro-visual.mjs`, `verify-ux-e2e.mjs` (`PASS`). |
| Desktop PKCE browser login, single-device exclusivity & Offline Read-Only | **`PASS`** (Live Google/Apple/GitHub OAuth **`BLOCKED BY EXTERNAL SETUP`**) | `auth_flow` (11), `security_online_authorization_e2e` (2), `dual_database_owner_templates_e2e`, `verify-offline-and-device-revocation-e2e.mjs`, `verify-desktop-full-journey-real-e2e.mjs`. |
| Owner Portal (`/owner` + Desktop Owner tab), backend RBAC, TOTP & BLAKE3 audit logs | **`PASS`** | `zero_trust_guard`, `log_center`, `dual_database_owner_templates_e2e`, `verify-desktop-full-journey-real-e2e.mjs`. |
| `config/business.json` single source of truth & all plan/VIP/Org/PPF/Credit Pack rules | **`PASS`** | `node tools/validate-config.mjs` (`business.json OK`), `sf-server` entitlement/billing suites. |
| Official templates, Save as New, `.safitemplate` gating (`SF-RCP-005`), sanitized Community Share | **`PASS`** | `dual_database_owner_templates_e2e`, `verify-desktop-full-journey-real-e2e.mjs`. |
| 7-format ingestion, deterministic cleaning, Undo/Redo & operation lineage | **`PASS`** | `sf-io`, `sf-clean`, `sf-recipe`, `sf-core`, `verify-studio-desktop-e2e.mjs`. |
| Per-Sheet Workflow Memory (High-only Rust IPC gate), Continuous Learning & 6-Domain Safi Decision Engine | **`PASS`** | `sf-desktop` (89 unit + 11 integration tests), `verify-desktop-full-journey-real-e2e.mjs`. |
| Local BM25 search, Quad-Studio, 21 chart types, What-If & 9 atomic deliverables | **`PASS`** | `export.test.ts`, `verify-exports-roundtrip.mjs` (`ALL_21_SVG_VISUALS_VERIFIED count=21`), `verify-capsule-chromium.mjs`, `verify-studio-desktop-e2e.mjs`. |
| Dark default (`google-gemini`), saved Light (`vision-glass`/`apple-light`), EN/AR RTL, WCAG AA contrast, zero intro flash | **`PASS`** | `pnpm run check:contrast`, `verify-intro-visual.mjs`, `verify-ux-e2e.mjs`, `verify-intro-theme-transition.mjs`, `verify-intro-transition-performance.mjs`. |
| Code-signed public Desktop installers & live HTTPS auto-updater | **`BLOCKED BY EXTERNAL SETUP`** | Fail-closed Ed25519 manifest verification tested; public installers stay `available=false` until signing certs & HTTPS hosting exist. |

---

## 4. Exact Local Verification Commands, Exit Codes & Raw Results (Post-`e7b6472` Remediation)

| Command | Exit Code | Raw Result |
| --- | --- | --- |
| `python tools/gen-errors.py --check` | `0` | `OK: Generated error artifacts match errors.toml and all referenced error codes are valid.` (`117` codes) |
| `bash tools/test-migrations.sh` (live ephemeral PostgreSQL 18.4) | `0` | `PASS: PostgreSQL 18 apply/constraints/RLS/rollback verified.` (`0001..0014`, `account_postgres` [1], `accounts_postgres` [2], `postgres_multi_instance_e2e` [1], `financial_integrity_e2e` PG18 [1], rollback guards `0014`/`0013`/`0008`, full `0014..0001` rollback) |
| `cargo fmt --all -- --check` | `0` | Clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | `0` | `0` warnings / `0` errors across all 11 crates |
| `cargo test --workspace` | `0` | `339 passed; 0 failed; 5 ignored` (all 5 ignored PG18 tests passed live in `test-migrations.sh` above) |
| `bash tools/check-no-network.sh` | `0` | `OK: Verified 10 engine crates -- 0 network dependencies found.` |
| `cargo fmt/clippy/test --manifest-path apps/desktop/src-tauri/Cargo.toml` | `0` | `3 passed; 0 failed; 1 ignored` (including live Windows Credential Manager `SFK1 -> SFK2`) |
| `pnpm run format:check` · `typecheck` · `lint` · `test` · `build` · `check:contrast` | `0` | `67/67` Vitest files (`235 passed \| 1 skipped` live Ollama), `46` text + `36` UI/chart contrast pairs OK |
| `node tools/validate-config.mjs` | `0` | `business.json OK` |
| `node tools/verify-intro-visual.mjs` | `0` | `PASS` (`showcaseVisible=true`, `theme="google-gemini"`, `desktopAuthGatedPassed=true`, `reducedMotionPassed=true`, `pageErrors=[]`) |
| `node tools/verify-ux-e2e.mjs` | `0` | `PASS` (all 7 viewports, single Desktop Auth entrance, `0` overflow at `1440x900`) |

---

## 5. Exact Remaining External Setup Blockers (Owner Action Required)

1. **Owner Push & Remote CI Confirmation:** Push this working tree to `main` and confirm all GitHub Actions jobs (`Full Gate ubuntu/windows`, `Desktop shell compile`, `Intro visual regression`, and `PostgreSQL 18.6 migration apply / rollback`) complete `success`.
2. **Real Paddle Sandbox & Production Credentials:** `SAFI_PADDLE_API_KEY`, `SAFI_PADDLE_WEBHOOK_SECRET`, `SAFI_PADDLE_API_BASE_URL`, `SAFI_PADDLE_PRICE_*` (5 price IDs), and public HTTPS webhook endpoint.
3. **Real Paymob Sandbox & Production Credentials:** `SAFI_PAYMOB_API_KEY`, `SAFI_PAYMOB_PUBLIC_KEY`, `SAFI_PAYMOB_HMAC_SECRET`, `SAFI_PAYMOB_INTEGRATION_ID`, `SAFI_PAYMOB_API_BASE_URL`, and public HTTPS callback endpoint.
4. **Live macOS & Linux Runtime Verification of `SFK2` OS Keystore:** Run `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` on real macOS (Keychain) and Linux (Secret Service) machines before upgrading macOS/Linux `SFK2` from `PARTIAL` to `PASS`.
5. **Live Local Ollama Model Verification:** Install Ollama + pull a local model on a test workstation and run `SAFI_LIVE_OLLAMA_TEST=1 pnpm --filter @safi/web test src/desktop/ai-interpreter.test.ts`.
6. **Live OAuth Provider Secrets & Code-Signed Public Installers:** Google/Apple/GitHub OAuth credentials, Authenticode / Apple notarization signing certificates, and HTTPS release hosting.
