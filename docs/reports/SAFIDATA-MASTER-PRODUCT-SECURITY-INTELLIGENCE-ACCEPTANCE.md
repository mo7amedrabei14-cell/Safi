# SafiData — Unified Production Organization, RBAC, Zero-Knowledge Encrypted Sync & Master Acceptance Report

---

## 1. Repository Baseline, Git State & Scope Verification

- **Repository:** `mo7amedrabei14-cell/Safi`
- **Repository Visibility Disclosure:** GitHub repository is currently **Public**. Per mandatory execution rules, repository visibility and production secrets were **not** changed; the owner controls visibility and credential rotation.
- **Target Branch:** `main` ONLY (zero feature branches created; zero `git commit`, `git push`, deploy, release, or publish performed).
- **Verified Starting & Current `main` HEAD SHA:** `33941d873b5463c41222b2d5a0c94412c640109b` (`main...origin/main`)
- **Previous Pushed Baseline SHA:** `bcad6ee92fbc2300f75a29a78b86472c3bce9b5a`
- **Working Tree State:** Uncommitted local changes directly on `main` (`33941d873b5463c41222b2d5a0c94412c640109b`) implementing:
  1. Complete **Origin Separation** (`SAFI_PUBLIC_WEB_ORIGIN` vs `SAFI_API_BASE_URL`) across Backend (`config.rs`, `auth/account.rs`, `billing/mod.rs` Paymob `notification_url` vs `redirection_url`, `routes.rs::oauth_start`, `billing_handoff_start`, `desktop_pkce_start`, `cors.rs`), Web (`vite.config.ts`), Desktop (`apps/desktop/src-tauri/{build.rs,src/main.rs}`), and CI (`.github/workflows/ci.yml` + `tools/verify-desktop-build-config.mjs`).
  2. Root-cause fix for the `33941d8` GitHub Actions failure (`https://github.com/mo7amedrabei14-cell/Safi/actions/runs/45361032321`, job `Desktop shell compile (windows-latest)`).
  3. Production **Organization Shared Team Workspaces**, **Verified-Email Single-Use Invitations**, and **Full 4-Role Server RBAC (`Owner`, `Admin`, `Member`, `Viewer`)** persisted durably in PostgreSQL 18 (`0017_production_org_rbac_and_e2ee_sync.{up,down}.sql`, `organizations_pg.rs`, `organizations.rs`, `routes.rs`).
  4. Production **Zero-Knowledge End-to-End Encrypted Sync (`ChaCha20-Poly1305` + `X25519` + `HKDF-SHA256` + `BLAKE3`)** across `crates/sf-desktop/src/org_sync.rs` (OS-keystore `SFK2`-encrypted local vault, offline queue, conflict resolution, epoch key rotation), `crates/sf-server/src/encrypted_sync_pg.rs` (`BYTEA` ciphertext-only PostgreSQL 18 storage, monotonic `workspace_seq`, optimistic concurrency, idempotency replay safety, tombstones), Website `/team`, and Desktop Studio (`OrganizationWorkspacePanel.tsx`).
  5. Root-cause fix in `crates/sf-server/src/pg_source_of_truth.rs::activate_device_atomically_pg` aligning `installation_key_hash` with `DeviceRegistration::from_installation_id` (`com.safidata.installation-key.v1`) so `X-Safi-Installation-Id` binding works seamlessly with `SqlxSessionRepository` on PostgreSQL 18.

---

## 2. GitHub Actions CI & PostgreSQL 18 Verification (Honest Status)

### 2.1 Pushed Baseline `33941d873b5463c41222b2d5a0c94412c640109b`

- **Workflow Run URL:** `https://github.com/mo7amedrabei14-cell/Safi/actions/runs/45361032321`
- **Root Cause Reproduced & Closed:** `desktop-shell (windows-latest)` set `SAFI_API_BASE_URL` without `SAFI_PUBLIC_WEB_ORIGIN`, triggering the fail-closed release origin guard in `apps/web/vite.config.ts` and `apps/desktop/src-tauri/build.rs`.
- **Remediation:** Added `SAFI_PUBLIC_WEB_ORIGIN: https://ci-web.safidata.dev` (distinct from `SAFI_API_BASE_URL: https://ci-verify.safidata.dev`) in `.github/workflows/ci.yml` and upgraded `tools/verify-desktop-build-config.mjs` to enforce fail-closed rejection and mutual independence for both origins. Verified locally with exit code `0`.

### 2.2 New Uncommitted Local Changes on Top of `33941d873b5463c41222b2d5a0c94412c640109b`

- **Remote GitHub Actions Status:** **`NOT VERIFIED`** — per mandatory rules, no `git commit` or `git push` was run. Once the owner inspects, commits, and pushes to `main`, confirm all CI jobs (`Full Gate ubuntu/windows`, `Desktop shell compile`, `PostgreSQL 18.6 migration apply / rollback`, `Intro visual regression`) report `success`.

---

## 3. Strict 4-Tier Test Classification & Results

| Tier | Classification Definition | Suites Executed | Result |
| --- | --- | --- | --- |
| **1. Unit Tests** | Isolated crate/module logic (crypto primitives, config validation, RBAC policy matrix, pricing parser, UI SSR rendering) | `cargo test --workspace --lib` (`sf-server` 82, `sf-desktop` 103, engine crates, Tauri shell 4) · Vitest unit specs (`72/72` files, `257 passed`, including `OrganizationWorkspacePanel.test.tsx`) | **`PASS`** (Exit `0`) |
| **2. Integration Tests** | Multi-module Axum HTTP router + service/SQLx integration (`oneshot` HTTP + live disposable PostgreSQL **18.4** `NOBYPASSRLS` + multi-instance + cold restart) | `organization_rbac_and_encrypted_sync_pg_e2e` (HTTP matrix + PG18 `--ignored`) · `desktop_billing_handoff_and_pg_e2e` · `entitlement_and_security_matrix_e2e` · `financial_integrity_e2e` · `postgres_multi_instance_e2e` · `account_postgres` · `accounts_postgres` · `bash tools/test-migrations.sh` (`0001..0017` up & down) | **`PASS`** (Exit `0`) |
| **3. UI E2E with Mock API** | Real Headless Chromium (Playwright) rendering the real Vite web bundle against mocked HTTP routes (`page.route`) — **explicitly NOT Real Backend E2E** | `node tools/verify-ux-e2e.mjs` (7 viewports, auth motion, RTL/LTR) · `node tools/verify-billing-portal-e2e.mjs` (`/pricing` + `/billing` UI flows with mocked checkout endpoints) | **`PASS`** (Exit `0`) |
| **4. Real Backend E2E** | Real Headless Chromium + **REAL `sf-server` OS process** + **LIVE PostgreSQL 18.4** + **TWO separate native Rust `sf-desktop` (`studio_ipc_harness`) OS processes & encrypted vaults** + **REAL Backend process kill & cold restart** (ZERO `page.route` API mocks) | `node tools/verify-org-sync-e2e.mjs` (all 10 Section-8 team/RBAC/invite/E2EE-sync/key-rotation/cold-restart steps on PostgreSQL 18.4) | **`PASS`** (Exit `0`) |

---

## 4. Production Architecture & Security Summary (`ADR-0015`)

1. **Shared Team Workspace & PostgreSQL 18 Durability (`0017_production_org_rbac_and_e2ee_sync.{up,down}.sql`):**
   - Dedicated `organizations.workspace_id` distinct from the Owner's personal workspace, linked to `billing_workspace_id` for server-side Paddle/Paymob seat & subscription enforcement (minimum `3` seats; `$17 / 850 EGP` monthly, `$204 / 10,200 EGP` annual per seat).
   - `FORCE ROW LEVEL SECURITY` with `USING` + `WITH CHECK` policies across `organizations`, `organization_members`, `organization_invitations`, `organization_member_public_keys`, `organization_wrapped_keys`, `encrypted_sync_records`, and `encrypted_share_links`.
2. **Verified-Email Single-Use Invitations:**
   - 256-bit CSPRNG `sfi_*` tokens stored strictly as `BLAKE3` hashes (`token_hash`).
   - Acceptance/decline enforces normalized verified-email equality (`invited_email_norm == verified_account_email`); wrong-email accounts with a valid token are rejected (`403 SF-SYN-005`). Unauthenticated `/team?invite=sfi_...` scrubs the token from the URL bar and discloses zero organization/member metadata prior to login.
3. **4-Role Server RBAC (`Owner`, `Admin`, `Member`, `Viewer`):**
   - Enforced server-side on every endpoint (`Owner` only: billing/subscription checkout on org workspace, ownership transfer, organization deletion; `Admin`: invites and manages `Member`/`Viewer` only, never `Owner`/`Admin`; `Member`: reads/writes shared E2EE resources; `Viewer`: read-only `GET /v1/sync/changes`, strictly blocked on `POST /v1/sync/envelopes` and all management routes). Sole-Owner self-removal and self-promotion are blocked.
4. **Zero-Knowledge End-to-End Encrypted Sync:**
   - Client-side `ChaCha20-Poly1305` (`ring = "=0.17.14"`) with domain-separated AAD (`safi:org-sync:v1|<workspace_id>|<kind>|<id>|<version>|<epoch>`) and zero-knowledge `BLAKE3(nonce || ciphertext)` integrity verification.
   - Static per-device `X25519` (`curve25519-dalek = "=4.1.3"`) keypairs stored in OS-keystore-backed `org_sync_vault.safi.enc`; per-epoch 256-bit Workspace Keys wrapped via ephemeral-to-static `X25519 + HKDF-SHA256 + ChaCha20-Poly1305`.
   - Member removal + `POST /v1/organizations/{id}/keys/rotate` advances `active_key_epoch` (`epoch + 1`) for remaining devices and immediately revokes the removed user's sync/key access (`403 SF-SYN-005`). Per standard E2EE physics, historical ciphertext previously decrypted on a former member's device before removal cannot be retroactively erased from offline storage, while all post-removal revisions (`epoch + 1`) are cryptographically inaccessible.
5. **Pricing Claim Verification (Section 9):**
   - Verified that `Shared team workspace + RBAC + encrypted sync` on `/pricing` (`apps/web/src/pages/PricingPage.tsx`) is backed 100% by production PostgreSQL 18 storage, server RBAC, client-side E2EE sync, Website `/team` UI, and Desktop Studio UI.

---

## 5. Honest Remaining Non-PASS / Operational Items

- **Unpushed Commit CI Execution:** **`NOT VERIFIED`** on GitHub Actions until the owner commits and pushes `main`.
- **Live External Payment Gateways (Real Money / Live Sandbox Credentials):** **`NOT VERIFIED`** locally (verified against protocol-accurate local HMAC-signed Paddle/Paymob webhook harnesses + PostgreSQL 18 ledger; live credentials reside in deployment secrets).
- **Two Separate Physical Machines Over Public WAN:** **`NOT VERIFIED`** on separate physical hosts (verified locally using two isolated OS processes & encrypted vaults of `studio_ipc_harness`, three browser contexts, and a real `sf-server` process kill + cold restart against live PostgreSQL 18.4).
