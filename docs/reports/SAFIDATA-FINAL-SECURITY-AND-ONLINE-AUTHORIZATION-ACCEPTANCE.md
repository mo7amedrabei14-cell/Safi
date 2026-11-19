# SafiData Desktop — Final Security, Subscription, Device Exclusivity & Online-Required Authorization Acceptance Report

> **Scope & Honesty Notice**: This report documents the architectural audit, root-cause remediation, and local runtime verification of SafiData Desktop's online-required authorization model, strict single-device account exclusivity, offline read-only local data access, billing/chargeback/coupon enforcement, durable central Owner database layer, and 14-section Owner Control Center. Every feature is classified honestly against actual runtime evidence collected on this workstation.

---

## 1. Executive Summary

- **Repository**: `mo7amedrabei14-cell/Safi` (local workspace `C:\Users\mo7am\OneDrive\Work\Safi`)
- **Branch**: `main`
- **Local `HEAD` (untouched by agent)**: `39118ec195d5f00b4207085c887c37c01909c0de` (all Directive 2 changes are local unstaged/untracked working-tree modifications; zero commits or pushes were performed).
- **Core Architectural Outcomes**:
  1. **Local-Only Customer Data + Mandatory Online Authorization**: Customer files, datasets, and encrypted local project bundles (`.safi.enc` via Argon2id + ChaCha20-Poly1305) remain strictly on the customer's machine. Protected operations (`ProtectedOnline`: import, edit, clean, transform, analyze, dashboard edits, save, delete, export, AI) require live online server authorization backed by 60-second Ed25519-signed leases and 30-second heartbeats.
  2. **Strict Offline Read-Only Mode (`SF-AUTH-021`)**: When offline, the Desktop app opens previously saved local projects for read-only viewing (`ReadOnlyOffline`: `studio_list_projects`, `studio_load_project`, `studio_state`, `studio_rows_page`, `studio_workbook_details`) while failing closed on every protected operation (`SF-AUTH-021`) and disabling Import / Save / Export controls in the UI.
  3. **One Account, One Active Device (`SF-AUTH-019`, `SF-AUTH-020`)**: Every login atomically replaces any previous active device, increments `session_generation`, revokes prior sessions, and binds an Ed25519 cryptographic device public key. Online replaced devices receive `SF-AUTH-019` on next heartbeat/action and are automatically signed out with an explicit explanation banner; offline replaced devices stay read-only and are rejected upon reconnect.
  4. **Subscription, Refund, Chargeback (`SF-PAY-006`) & Server-Side Coupons (`SF-PAY-005`)**: Server-enforced plans, idempotent webhooks (Paddle HMAC-SHA256 + Paymob HMAC-SHA512), immediate revocation on refunds and chargebacks, and Owner-controlled coupons with server-calculated discounts, expiration, per-user single-use, usage caps, and concurrent-redemption locking.
  5. **Durable Central Owner Database & 14-Section Owner Control Center**: PostgreSQL 18 migration `0010` + SQLx repository (`PgControlPlaneStore`) wired for production `DATABASE_URL`, plus crash-safe atomic `DurableStateJournal` verified across full server restarts locally, powering all 14 Owner Control Center sections behind immutable Owner identity + Google Authenticator RFC 6238 TOTP MFA + 5-minute step-up verification.

---

## 2. Classification Matrix (Per-Feature Honest Status)

| # | Capability / Requirement | Classification | Evidence / Notes |
|---|---|---|---|
| 1 | Local-only customer dataset & encrypted project storage (`.safi.enc`) | **IMPLEMENTED AND VERIFIED** | `crates/sf-desktop/src/studio.rs`, `vault.rs` (`cargo test -p sf-desktop`: 70/70 passed; Chromium privacy check confirmed 0 leaked rows) |
| 2 | Offline Read-Only Mode (`SF-AUTH-021`) for saved local projects | **IMPLEMENTED AND VERIFIED** | Native Rust `CommandAccessClass` + `DesktopStudio.tsx` banner & disabled buttons (`sf-desktop` + Chromium `verify-offline-and-device-revocation-e2e.mjs`) |
| 3 | Mandatory Online Authorization for Protected Operations (60s Ed25519 Lease + 30s Heartbeat) | **IMPLEMENTED AND VERIFIED** | `POST /v1/auth/authorize`, `POST /v1/auth/heartbeat`, `DeviceAuthorityService::sign_online_lease`, `ApplicationShell.tsx` |
| 4 | Local Clock Rollback Guard (`SF-AUTH-022`) | **IMPLEMENTED AND VERIFIED** | `StudioStore::gate_command` monotonic `high_water_clock_ms` guard tested in `sf-desktop` (`>5s` rollback rejected with `SF-AUTH-022`) |
| 5 | Strict Single-Active-Device Exclusivity & Automatic Revocation (`SF-AUTH-019`) | **IMPLEMENTED AND VERIFIED** | Atomic replacement + `session_generation` + 10-way concurrent login race (`security_online_authorization_e2e.rs` + Chromium auto-logout E2E) |
| 6 | Ed25519 Cryptographic Device Identity & Proof (`SF-AUTH-020`) | **IMPLEMENTED AND VERIFIED** | `crates/sf-desktop/src/device_identity.rs` + `crates/sf-server/src/device_authority.rs` (stolen token without/wrong proof rejected `SF-AUTH-020`) |
| 7 | Audited Owner Administrative Device Reset | **IMPLEMENTED AND VERIFIED** | `POST /v1/owner/accounts/{user_id}/devices/reset` (requires Owner step-up TOTP, revokes devices/sessions, increments generation, logs BLAKE3 audit entry) |
| 8 | Subscription, Trial, AI Quota & Account Suspension Enforcement (`SF-AUTH-009`, `SF-LIC-008`) | **IMPLEMENTED AND VERIFIED** | `crates/sf-server/src/entitlements.rs`, `routes.rs`, verified in `security_online_authorization_e2e.rs` |
| 9 | Billing Webhooks (Paddle HMAC-SHA256 + Paymob HMAC-SHA512), Refunds & Chargebacks (`SF-PAY-006`) | **IMPLEMENTED AND VERIFIED** | `crates/sf-server/src/billing/mod.rs` (forged signatures rejected, duplicate events idempotent, chargeback immediately revokes paid access) |
| 10 | Owner-Controlled Coupons & Promotions (`SF-PAY-005`) | **IMPLEMENTED AND VERIFIED** | `crates/sf-server/src/coupons.rs` + `routes.rs` (percentage/fixed, expiry, limits, eligibility, single-use per user, 10-way concurrent redemption race) |
| 11 | Durable Server Restart Persistence (Local Crash-Safe Atomic Disk Journal) | **IMPLEMENTED AND VERIFIED** | `DurableStateJournal` (`temp -> sync_all -> rename`) verified across full `AppState` drop & rebuild in `security_online_authorization_e2e.rs` |
| 12 | PostgreSQL 18 Migrations `0001..0010` + `PgControlPlaneStore` SQLx Repositories | **IMPLEMENTED BUT NOT RUNTIME-VERIFIED** / **BLOCKED BY EXTERNAL SETUP** | Schema & SQLx queries compile & pass `verify-migrations.py` (10/10 pairs with RLS & `uuidv7()`), but live Postgres daemon/Docker is not installed on this workstation (3 Postgres integration tests ignored) |
| 13 | 14-Section Owner Control Center APIs & UI | **IMPLEMENTED AND VERIFIED** | Real backend endpoints + `apps/web/src/lib/owner-api.ts` + `OwnerDashboard.tsx` (zero mocks, Owner TOTP + Step-Up enforced) |
| 14 | Live External Provider Execution (Paddle/Paymob live checkout, SMTP, SMS/WhatsApp, OAuth, AI LLM) | **BLOCKED BY EXTERNAL SETUP** | Adapters implemented & fail closed (`NOT_CONFIGURED` / `SF-PAY-001` / `SF-AIX-001`) until owner provisions production credentials |
| 15 | Public Showcase vs Desktop Runtime Separation | **IMPLEMENTED AND VERIFIED** | `tools/verify-runtime-boundary-e2e.mjs` passed in real headless Chromium |

---

## 3. Product Operating Model Architecture

- **Data Locality Guarantee**: All customer CSV/XLSX/XLSM/XLSB/XLS/ODS ingestion, `DataGraph` state, cleaning, formula computation, charting, capsule generation, and deliverable exports run locally inside the Rust Desktop engine (`crates/sf-desktop`, `crates/sf-io`, `crates/sf-clean`, `crates/sf-recipe`). Zero raw dataset rows or local filenames are transmitted to `sf-server`.
- **Two Strict Execution Classes (`crates/sf-desktop/src/commands.rs`)**:
  - `CommandAccessClass::ReadOnlyOffline`: `app_info`, `take_pending_deep_link`, `studio_authorization_status`, `studio_enter_offline_readonly`, `studio_revalidate_session`, `studio_list_projects`, `studio_load_project`, `studio_state`, `studio_rows_page`, `studio_workbook_details`, `studio_list_templates`.
  - `CommandAccessClass::ProtectedOnline`: Every import (`studio_open_file`, `studio_select_region`), edit (`studio_set_cell*`, `studio_*_row`, `studio_*_column`), cleaning (`studio_apply_command`, `studio_apply_anomaly_fix`, `studio_trim_column`, `studio_undo`, `studio_redo`), analysis (`studio_define_*`, `studio_*_scenario*`, `studio_grouped`, `studio_chart_data`, `studio_narrative`), dashboard/report mutation (`studio_set_artifacts`, `studio_instantiate_template`, `studio_restore`), persistence mutation (`studio_save_project`, `studio_delete_project`), and export (`studio_export_capsule`, `studio_export_deliverable`, file pickers).

---

## 4. Offline Read-Only Mode & Reconnection Behavior

- **Offline Startup & Mid-Session Drop**:
  - Native Rust `StudioStore::enter_offline_read_only()` switches the desktop engine into `offline_read_only` mode without deleting saved local projects or stored credentials needed for reconnect.
  - Read-only commands load `.safi.json` and encrypted `.safi.enc` bundles locally (verifying `BLAKE3` integrity `SF-CAL-008` and ChaCha20-Poly1305 authentication tag `SF-SYS-024`), ignoring any stray `.tmp` crash artifacts.
  - Every `ProtectedOnline` command immediately returns `SF-AUTH-021`.
  - `DesktopStudio.tsx` renders an accessible banner (`[data-offline-readonly="true"]`) explaining the `SF-AUTH-021` read-only state and disables Import, Save, Undo/Redo, Restore, and Export controls.
- **Automatic Online Revalidation**:
  - On `window` `"online"` event or clicking "Reconnect & Revalidate", `ApplicationShell.tsx` invokes `POST /v1/auth/heartbeat` and native `studio_revalidate_session` before lifting Offline Read-Only Mode.
  - If the server confirms the session, device, and account are valid, protected operations resume.
  - If the server returns `SF-AUTH-019` (replaced by another device), `SF-AUTH-003` (revoked), `SF-AUTH-009` (suspended), or `SF-AUTH-002` (expired), the Desktop app immediately clears the local session (`clearStudioAuthSession`), locks to the login screen, and displays the exact reason banner.

---

## 5. Strict Single-Active-Device Exclusivity & Automatic Revocation

- **Atomic Replacement (`crates/sf-server/src/device_authority.rs` + `auth/repository.rs`)**:
  - `DeviceAuthorityService::activate_device_atomically` holds an exclusive write lock (mirrored by `pg_advisory_xact_lock` + partial unique active device index in PostgreSQL `0006`/`0010`) so concurrent logins serialize cleanly and `active_device_count(user_id) <= 1` holds unconditionally.
  - Logging in on Device B atomically marks Device A as `ReplacedByNewDevice`, records Device A's session token hash in `revoked_token_reasons` with `ReplacedByNewDevice`, increments `session_generation`, activates Device B, and appends an immutable `DeviceReplacementEvent` + BLAKE3 audit entry.
- **No Physical Access Required**: Device B never needs Device A's approval or physical proximity; Device A is invalidated immediately on the server.
- **Online Device A Detection**: On next 30-second heartbeat (`POST /v1/auth/heartbeat`) or authorization check (`POST /v1/auth/authorize`), Device A receives HTTP `401` with `SF-AUTH-019`, wipes its session, and presents `"Your account was activated on another device. This device has been signed out automatically (SF-AUTH-019)."`.
- **Offline Device A Behavior**: While offline, Device A cannot execute protected operations (`SF-AUTH-021`); the moment Device A reconnects, revalidation returns `SF-AUTH-019` and clears its session.

---

## 6. Cryptographic Device Identity (`Ed25519`) & Replay Prevention

- **Desktop Keypair (`crates/sf-desktop/src/device_identity.rs`)**:
  - Generated via OS CSPRNG (`ring::rand::SystemRandom`), zeroized on drop (`zeroize`).
  - Sealed to disk encrypted at rest with `vault.rs` Argon2id + ChaCha20-Poly1305 via atomic `temp -> sync_all -> rename` (verified zero plaintext seed bytes on disk).
  - Signs canonical proof `safidata-device-proof-v1|{session_token_hash}|{timestamp_secs}`.
- **Server Verification (`crates/sf-server/src/device_authority.rs`)**:
  - `verify_device_ed25519_proof` validates the `X-Safi-Device-Signature` and `X-Safi-Device-Timestamp` headers (±120s skew window) against the active device's registered `public_key_hex`.
  - A stolen `sfs_*` bearer token replayed without a valid Ed25519 proof or signed by a different key fails closed with HTTP `401` (`SF-AUTH-020`).

---

## 7. Short-Lived Online Authorization Leases & Clock Tampering Defense

- **60-Second Server-Signed Leases**: `POST /v1/auth/login`, `POST /v1/auth/authorize`, and `POST /v1/auth/heartbeat` issue `SignedOnlineAuthorizationLease` with `ONLINE_LEASE_TTL_SECS = 60`, `ONLINE_HEARTBEAT_SECS = 30`, `server_time_secs`, `expires_at_secs`, `session_generation`, `entitlements_digest` (BLAKE3), and Ed25519 signature (`signature_hex` + `verifying_key_hex`).
- **Monotonic Clock Rollback Guard**: `StudioStore` tracks `high_water_clock_ms`. Rolling the OS clock backward by more than 5 seconds (`MAX_CLOCK_ROLLBACK_TOLERANCE_MS = 5_000`) during an active session fails closed with `SF-AUTH-022`.

---

## 8. Subscription, Billing, Refund & Chargeback Protection

- **Authoritative Server Entitlements (`crates/sf-server/src/entitlements.rs`)**: Free, 7-day Trial (anti-abuse across email/phone/device hashes, 25 AI cap), Individual Monthly/Annual (400 AI/mo), Organization (min 3 seats), Friend VIP, Pay-Per-File, and Credit Pack 25 are resolved exclusively on the server.
- **Signed Webhooks (`crates/sf-server/src/billing/mod.rs`)**:
  - Paddle: HMAC-SHA256 over `"{ts}:{raw_body}"` with constant-time comparison (`subtle`) and 5-minute replay window (`SF-PAY-002`, `SF-PAY-003`).
  - Paymob: HMAC-SHA512 over official 20-field sorted transaction concatenation with constant-time comparison.
  - Persistent idempotency (`processed_events`) and monotonic out-of-order protection (`last_event_ts_by_workspace`).
- **Refund & Chargeback Revocation (`SF-PAY-006`)**:
  - Owner refunds enforce the 3-day policy window (unless statutory override is flagged) and immediately downgrade entitlements to Free.
  - Paddle `"adjustment.chargeback"` / `"chargeback.created"`, Paymob `is_chargeback: true`, and Owner `POST /v1/owner/billing/chargeback` mark the invoice `"chargeback"` and **immediately revoke paid entitlements**, downgrading the workspace to Free (`Canceled`).

---

## 9. Owner-Controlled Coupons & Promotions Engine

- **Implementation (`crates/sf-server/src/coupons.rs` + `routes.rs`)**:
  - Owner CRUD (`GET/POST /v1/owner/coupons`, `POST /v1/owner/coupons/{code}`, `GET /v1/owner/coupons/redemptions`) gated behind Owner identity + fresh 5-minute step-up TOTP.
  - Customer endpoints: `POST /v1/billing/coupons/preview` (read-only validation) and server-side redemption inside `POST /v1/billing/checkout` (never trusts client-supplied price or discount).
  - Enforces percentage (`1..=100`) and fixed-currency (`fixed_usd`, `fixed_egp`) discounts, start/expiration timestamps, enable/disable/archive flags, global redemption caps, single redemption per `(coupon_id, user_id)`, eligible plan list, eligible user list, new-customers-only, org-only, minimum purchase threshold, and exclusive write-lock concurrency serialization (`SF-PAY-005`).

---

## 10. Durable Central Owner Database & Restart Persistence

- **PostgreSQL 18 Schema (`crates/sf-server/migrations/0010_device_exclusivity_coupons_and_control_plane.{up,down}.sql`)**:
  - Adds `users.session_generation`, `devices.public_key_hex`, `devices.replaced_by_device_id`, `sessions.session_generation`, `sessions.revocation_reason`, `device_replacement_events`, `coupons`, `coupon_redemptions` (with workspace RLS), `support_tickets` (with workspace RLS), `platform_announcements`, and extends `billing_invoices.status` CHECK constraint to include `'chargeback'`.
  - Verified by `py -3 -B tools/verify-migrations.py`: `10 migration pair(s) verified (up/down, RLS, uuidv7, no SERIAL, guarded pg_partman)`.
- **SQLx Control-Plane Repository (`crates/sf-server/src/pg_store.rs`)**:
  - `PgControlPlaneStore` wired into `crates/sf-server/src/main.rs` when `DATABASE_URL` is configured.
- **Crash-Safe Atomic Disk Journal (`DurableStateJournal`)**:
  - Persists devices, session generations, revoked token reasons, active sessions, coupons/redemptions, entitlements, billing invoices/idempotency, encrypted Owner TOTP state, support tickets, announcements, feature flags, and BLAKE3 audit chain via `temp -> sync_all -> rename`, verified across real `AppState` destruction and reconstruction.

---

## 11. Complete 14-Section Owner Control Center

All 14 sections are backed by live server endpoints in `crates/sf-server/src/routes.rs`, typed in `apps/web/src/lib/owner-api.ts`, and surfaced in `apps/web/src/pages/OwnerDashboard.tsx` (preserving the calm 6-card Overview default and Google Authenticator RFC 6238 TOTP + 5-minute step-up MFA):

1. **Executive Overview**: `GET /v1/owner/stats` (6 KPI cards: users, MRR/ARR, trials/paid/org seats, Friend VIP, payment providers, platform usage + live counts of devices, coupons, chargebacks, tickets).
2. **Customer Management**: `GET /v1/owner/accounts?q=...`, suspend (`POST /v1/owner/accounts/{id}/suspend`), reactivate (`POST /v1/owner/accounts/{id}/reactivate`), Friend VIP grant/revoke.
3. **Customer Profiles (Drill-Down)**: `GET /v1/owner/accounts/{id}/profile` (account, entitlements, invoices, devices, device replacement history).
4. **Subscription Management**: `POST /v1/owner/subscriptions/{ws}/extend-trial`, `POST /v1/owner/subscriptions/{ws}/cancel`.
5. **Billing, Refunds & Chargebacks**: `GET /v1/owner/billing/events`, `GET /v1/owner/billing/invoices`, `POST /v1/owner/billing/refund`, `POST /v1/owner/billing/chargeback`.
6. **Revenue Analytics**: Live `BillingRevenueSummary` (gross, refunded, chargeback in USD & EGP) + `EntitlementAggregateStats` (MRR/ARR).
7. **Promotions & Coupons**: `GET/POST /v1/owner/coupons`, `POST /v1/owner/coupons/{code}`, `GET /v1/owner/coupons/redemptions`.
8. **Device & Session Management**: `GET /v1/owner/devices`, `POST /v1/owner/accounts/{id}/devices/reset` (audited, reason-required, step-up TOTP).
9. **Organization Management**: `GET /v1/owner/organizations` (purchased seats vs active members).
10. **Customer Support**: `GET/POST /v1/owner/support/tickets`, `POST /v1/owner/support/tickets/{id}`.
11. **Announcements & Updates**: `GET/POST /v1/owner/announcements` + Ed25519-signed `/v1/updates/latest`.
12. **Product Configuration**: `GET /v1/owner/product-config`, `GET/POST /v1/owner/system/maintenance`, `/ready`.
13. **Feature Flags**: `GET/POST /v1/owner/feature-flags`.
14. **Security & Audit Logs**: `GET /v1/owner/audit-logs` (append-only, secret-redacted, cryptographically verified BLAKE3 hash chain).

---

## 12. Error Code Catalog Additions (`errors.toml`)

Six new canonical error codes added to `errors.toml` and regenerated across Rust (`crates/sf-core/src/generated_errors.rs`) and TypeScript (`packages/i18n/src/generated-errors.ts`) — verified via `py -3 -B tools/gen-errors.py --check` (110 canonical codes total):

- `SF-AUTH-019`: Device replaced by a newer login on another device.
- `SF-AUTH-020`: Cryptographic Ed25519 device proof missing or invalid.
- `SF-AUTH-021`: Offline Read-Only Mode — protected operation requires online authorization.
- `SF-AUTH-022`: Online authorization lease expired or local clock rollback detected.
- `SF-PAY-005`: Coupon invalid, expired, ineligible, or redemption limit reached.
- `SF-PAY-006`: Payment chargeback recorded and paid subscription revoked.

---

## 13. Mandatory Runtime Test Results (Actual Commands & Outputs)

| Suite / Gate | Command Executed | Actual Result |
|---|---|---|
| Single-Device, Ed25519 Proof, Concurrency, Coupons, Chargeback & Restart E2E | `cargo test -p sf-server --test security_online_authorization_e2e` | **PASS (2/2)** — `single_device_exclusivity_ed25519_proof_concurrency_reset_and_server_restart`, `subscription_chargeback_coupons_concurrency_control_center_and_restart_suite` |
| Desktop Engine, Offline Read-Only, Clock Rollback, Encrypted Vault & Device Keypair | `cargo test -p sf-desktop` | **PASS (70 unit + 10 integration, 0 failed)** |
| Tauri Native TLS Verifier & IPC Boundary | `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` | **PASS (2/2 passed, 1 manual perf bench ignored)** |
| Full Rust Workspace (`11 crates`) + Determinism Codeburn | `& "C:\Program Files\Git\bin\bash.exe" tools/codeburn/burn.sh` | **PASS (0 failed; 3 live-Postgres tests ignored due to absent Postgres daemon)** |
| Rust Formatting & Strict Clippy (`-D warnings`) | `cargo fmt --all -- --check` + `cargo clippy --workspace --all-targets -- -D warnings` + `src-tauri` | **PASS (0 warnings, 0 errors)** |
| TypeScript Strict Typecheck, ESLint, Vitest, Build, Contrast, Prettier & Audit | `pnpm --dir apps/web run typecheck` + `pnpm run lint` + `pnpm run test` + `pnpm run build` + `pnpm run check:contrast` + `pnpm run format:check` + `pnpm audit --audit-level=high` | **PASS (0 errors, 0 high/critical vulnerabilities)** |
| Headless Chromium Offline Read-Only & `SF-AUTH-019` Auto-Logout E2E | `node tools/verify-offline-and-device-revocation-e2e.mjs` | **PASS** — `OFFLINE_AND_DEVICE_REVOCATION_E2E_OK offline_readonly=SF-AUTH-021 protected_disabled=true device_replaced_auto_logout=SF-AUTH-019 privacy_clean=true` |
| Headless Chromium Runtime Boundary E2E | `node tools/verify-runtime-boundary-e2e.mjs` | **PASS** — `RUNTIME_BOUNDARY_E2E_OK showcase=unlocked desktop_regular=auth_gated_no_owner desktop_owner=totp_mfa_gated` |
| Headless Chromium IndexedDB Device Identity | `node tools/verify-device-identity.mjs` | **PASS** — `stableAcrossPages=true, localStorageUsed=false` |
| Static Governance, Errors, Migrations, i18n, No-Network Engine Guard | `gen-errors.py --check`, `verify-migrations.py`, `verify-i18n.py`, `verify-security.py`, `validate-config.mjs`, `check-no-network.sh` (+ `--self-test`) | **PASS (all 7 checks exit code 0)** |

---

## 14. Privacy & Local Data Isolation Verification

- Verified `check-no-network.sh` (+ negative `--self-test`): all 10 local engine crates (`sf-arabic`, `sf-clean`, `sf-cli`, `sf-core`, `sf-desktop`, `sf-io`, `sf-profile`, `sf-recipe`, `sf-report`, `sf-wasm`) have **0 network dependencies** and zero `std::net` usage.
- Verified in `tools/verify-offline-and-device-revocation-e2e.mjs`: outbound HTTP request inspection confirmed zero customer cell values leave the device.
- Verified in `crates/sf-desktop/src/studio.rs`: encrypted `.safi.enc` local project files contain zero plaintext data bytes on disk and survive simulated crash `.tmp` debris cleanly.
- Verified in `crates/sf-server/src/log_center.rs`: audit logs recursively redact credentials/tokens/secrets and preserve a verifiable BLAKE3 hash chain.

---

## 15. Concurrency & Race-Condition Verification

- **10-Way Concurrent Device Login Race**: Spawned 10 parallel `POST /v1/auth/login` tasks for the same user; verified `active_device_count == 1` and strictly 1 of the 10 issued session tokens survives `POST /v1/auth/heartbeat`.
- **10-Way Concurrent Coupon Redemption Race**: Spawned 10 parallel `POST /v1/billing/checkout` requests competing for `LAUNCH25` (`max_redemptions = 1`); verified strictly 1 succeeds with discounted price (`1500` minor units) and 9 fail closed with `SF-PAY-005`.
- **Concurrent Free Quota Consumption**: Verified in `entitlements::tests::concurrent_consumption_never_exceeds_free_five_op_quota`.

---

## 16. Security Hardening & Threat Model Summary

- **Stolen Session Token Replay**: Mitigated by Ed25519 device proof (`SF-AUTH-020`) + `X-Safi-Installation-Id` binding + single-device server enforcement (`SF-AUTH-019`).
- **Local Clock Manipulation**: Mitigated by server-signed 60s leases (`server_time_secs`) + monotonic `high_water_clock_ms` rollback guard (`SF-AUTH-022`).
- **Client Price/Entitlement Tampering**: Mitigated by computing all prices, coupon discounts, entitlements, and quotas exclusively on `sf-server`.
- **Privilege Escalation to Owner**: Mitigated by immutable sealed `owner_user_id` (never email-based), mandatory RFC 6238 Google Authenticator TOTP MFA with anti-replay & lockout, and 5-minute step-up verification for high-risk actions.
- **Webhook Forgery & Replay**: Mitigated by constant-time HMAC-SHA256 (Paddle + 5-min window) and HMAC-SHA512 (Paymob) plus persistent idempotency and monotonic timestamp ordering.

---

## 17. Residual Risks & Explicit Non-Claims (Honest Engineering Disclosure)

1. **Offline Push Logout & Remote File Wipe Are Physically Impossible**: An already-offline device cannot receive a real-time revocation push until it reconnects, nor can the server remotely delete files on a customer's local hard drive. SafiData mitigates this by enforcing strict Offline Read-Only Mode (`SF-AUTH-021`) so an offline device cannot run protected operations, and immediately revoking it with `SF-AUTH-019` upon reconnection.
2. **Live PostgreSQL 18 Runtime Verification Blocked Locally**: Neither Docker nor a local PostgreSQL 18 daemon (`psql`/`postgres`) is installed on this Windows workstation. `0010_*.sql` and `PgControlPlaneStore` are implemented, compiled, and statically validated (`verify-migrations.py`), while live PostgreSQL execution (`account_postgres`, `accounts_postgres`) remains **BLOCKED BY EXTERNAL SETUP** until run against a real PostgreSQL 18 instance with `SF_TEST_DATABASE_URL`.
3. **Live External Integrations Blocked Locally**: Paddle, Paymob, SMTP, SMS/WhatsApp, OAuth, and external AI LLM providers require production API keys and sandbox/live accounts configured by the Owner (`BLOCKED BY EXTERNAL SETUP`).
4. **Desktop Binary Code Signing & OS Trust**: Production Authenticode / notarization certificates are not present on this local development machine (`BLOCKED BY EXTERNAL SETUP`).

---

## 18. Changed & Added Files (Local Unstaged Working Tree Only)

**Modified**:
- `errors.toml`, `crates/sf-core/src/generated_errors.rs`, `packages/i18n/src/generated-errors.ts`
- `crates/sf-server/src/{error.rs, lib.rs, main.rs, routes.rs, entitlements.rs, log_center.rs, organizations.rs, owner_mfa.rs}`
- `crates/sf-server/src/auth/{repository.rs, service.rs}`
- `crates/sf-server/src/billing/mod.rs`
- `crates/sf-desktop/src/{lib.rs, commands.rs, studio.rs}`
- `apps/desktop/src-tauri/src/main.rs`
- `apps/web/src/{ApplicationShell.tsx, desktop/DesktopStudio.tsx, desktop/studio-bridge.ts, lib/auth-api.ts, lib/owner-api.ts, pages/OwnerDashboard.tsx}`

**Added**:
- `crates/sf-server/migrations/0010_device_exclusivity_coupons_and_control_plane.{up,down}.sql`
- `crates/sf-server/src/{coupons.rs, device_authority.rs, pg_store.rs}`
- `crates/sf-server/tests/security_online_authorization_e2e.rs`
- `crates/sf-desktop/src/device_identity.rs`
- `tools/verify-offline-and-device-revocation-e2e.mjs`
- `docs/reports/SAFIDATA-FINAL-SECURITY-AND-ONLINE-AUTHORIZATION-ACCEPTANCE.md`

---

## 19. Production Deployment & External Setup Checklist (Owner Actions)

1. Provision PostgreSQL 18 + Valkey 8 and apply migrations `0001..0010`.
2. Run live PostgreSQL integration tests with `SF_TEST_DATABASE_URL` (`cargo test -p sf-server -- --ignored`).
3. Configure environment secrets (`DATABASE_URL`, `SAFI_VALKEY_URL`, `SAFI_TOTP_ENCRYPTION_KEY`, `SAFI_UPDATE_SIGNING_KEY`, `SAFI_LICENSE_SIGNING_KEY`, `SAFI_API_BASE_URL`, Paddle/Paymob keys, SMTP, SMS/WhatsApp, optional AI provider).
4. Run one-time sealed Owner bootstrap (`sf-server --bootstrap-owner`) and enroll Google Authenticator TOTP, then remove `SAFI_OWNER_BOOTSTRAP_EMAIL`.
5. Build packaged Desktop binary with production `SAFI_API_BASE_URL` and Authenticode signing certificate.

---

## 20. Git State & Hygiene Proof

- Branch: `main`
- `HEAD`: `39118ec195d5f00b4207085c887c37c01909c0de` (no commits, no pushes, no branch creations, no remote mutations)
- `git diff --check`: exit code `0` (clean)
- Hardcoded personal owner email or production credentials: **0 occurrences**

---

## 21. Decisions Required From Product Owner

1. **Live PostgreSQL 18 Verification Environment**: Confirm when you would like to run the `0001..0010` migrations and `--ignored` SQLx tests against your staging PostgreSQL 18 database.
2. **Default Local Project Encryption Policy**: Decide whether `.safi.enc` local vault encryption should be opt-in per workspace passphrase or mandatory on first Desktop setup.
3. **Review & Manual Commit/Push**: All changes remain strictly local on `main` awaiting your inspection and manual Git commit/push decision.

---

## 22. Final Acceptance Verdict

- **Software Engineering & Local Security/Runtime Verification**: **PASS** (every local Rust, Tauri, TypeScript, static security, determinism, and headless Chromium E2E test passed with 0 failures and 0 warnings).
- **Production Launch Readiness**: **CONDITIONAL ON EXTERNAL SETUP** (live PostgreSQL 18 migration/test execution, external payment/messaging/AI credentials, Owner sealed bootstrap, and Authenticode code signing).
