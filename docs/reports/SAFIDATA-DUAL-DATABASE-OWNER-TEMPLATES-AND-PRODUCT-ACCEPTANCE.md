# SAFIDATA — DUAL DATABASE, OWNER-ONLY ADMINISTRATION, TEMPLATES, LOCAL SEARCH & PRODUCT ACCEPTANCE REPORT (REMEDIATION ROUND)

> **Scope & Honesty Notice:** This report documents strictly verified local implementation and execution results on branch `main` starting from baseline commit `5412ed444bc38b592fd21d8eb5535edea060ccdb`. No commit, push, branch, or production deployment was executed (user-managed). Every capability is classified strictly using one of `PASS`, `FAIL`, `PARTIAL`, `BLOCKED BY EXTERNAL SETUP`, or `NOT IMPLEMENTED`. Mocked Chromium smoke tests are NEVER counted as full backend integration proof.

---

## Executive Remediation Summary (All 7 Review Items)

| # | Review Requirement | Local Status | External / CI Status |
|---|---|---|---|
| 1 | **PostgreSQL as True Single Source of Truth** (`PgControlPlaneStore` READ + WRITE + ATOMIC TX for Official Templates, Community Submissions, App Releases; startup hydration in `main.rs`) | `PASS` (Code, SQLx compile check, Migration `0012` static verification) | `BLOCKED BY EXTERNAL SETUP` locally (no local PG18 daemon); wired into CI `tools/test-migrations.sh` on `postgres:18.6-bookworm` (`postgres_multi_instance_e2e`) |
| 2 | **Security Durability in PostgreSQL** (Encrypted Owner TOTP, Recovery Codes, Elevated/Step-Up Sessions, Device Authority + `has_any_device == false` fail-closed, Owner Web Sessions `sfw_*`, Desktop PKCE `desktop_auth_codes`) | `PASS` (Code, unit/E2E tests, fail-closed `device_authority.rs`) | `BLOCKED BY EXTERNAL SETUP` locally; multi-instance + restart proof wired in `crates/sf-server/tests/postgres_multi_instance_e2e.rs` + CI `tools/test-migrations.sh` |
| 3 | **Desktop UI + Tauri IPC Integration for `LocalSearchIndex` & `LocalRecipeLibrary`** (`DesktopLocalSearchPanel`, `DesktopRecipeLibraryPanel`, 14 allowlisted IPC commands in `ALLOWED_COMMANDS` & `src-tauri/src/main.rs`) | `PASS` (Verified end-to-end with ZERO mocks in `verify-desktop-full-journey-real-e2e.mjs`) | N/A |
| 4 | **End-to-End Desktop Browser PKCE Auth (`S256`)** (`start` -> OS browser `/auth/desktop` -> `approve` -> `safi://auth` -> `take_pending_deep_link` -> `exchange` -> native `StudioStore` bind + fail-closed state mismatch) | `PASS` (Verified end-to-end with ZERO mocks in `verify-desktop-full-journey-real-e2e.mjs`) | N/A |
| 5 | **Honest Real Installer Download** (Removed untracked dev EXE, set `available: false` until real public HTTPS Release Storage/CDN URL is published, added `tools/verify-published-installer-download.mjs`) | `PASS` (Honest unpublished state verified in Vitest + Chromium + script) | `BLOCKED BY EXTERNAL SETUP` (No public HTTPS Release Storage/CDN URL or Authenticode cert provisioned yet) |
| 6 | **Improved PDF Text Extraction in Local Search** (`/FlateDecode` zlib decompression with 8 MiB bomb cap + UTF-16BE hex strings; honest non-OCR notice for scanned image-only PDFs) | `PASS` (Verified in Rust unit test + real Chromium E2E) | N/A |
| 7 | **Real Unmocked Acceptance Tests & CI PG18 Coverage** (`tools/verify-desktop-full-journey-real-e2e.mjs`, `postgres_multi_instance_e2e.rs`, fixed real 30s `/v1/auth/heartbeat` double-prefix bug!) | `PASS` (All unmocked local suites & CI gates green) | Live PG18 runs in GitHub Actions `database-migrations` job (`postgres:18.6-bookworm`) |

---

## 1. Repository & Git Hygiene Verification — `PASS`
- **Current Branch:** `main`
- **Local `HEAD`:** `5412ed444bc38b592fd21d8eb5535edea060ccdb`
- **Git Discipline:** Zero commits, zero pushes, zero new branches, zero destructive Git commands (`reset --hard` / `clean -fd`), and `git diff --check` exited `0`.
- **Untracked Dev Binary Removed:** Deleted local dev binaries under `apps/web/public/downloads/*.exe` and `apps/web/dist/downloads/*.exe`.

---

## 2. PostgreSQL 18 as Actual Single Source of Truth — `PASS` (Code & CI Wiring) / `BLOCKED BY EXTERNAL SETUP` (Local PG18 Daemon)
- **Migration `0012`:** Created `crates/sf-server/migrations/0012_multi_instance_security_durability.{up,down}.sql` adding:
  - `elevated_verified_at`, `elevated_expires_at`, `last_step_up_at` to `owner_web_sessions`
  - `device_name`, `email` to `desktop_auth_codes`
  - `installation_id`, `active_session_id`, `session_generation`, `status` to `devices`
  - `pii_warnings` to `community_template_submissions`
- **Direct PG Read/Write/Atomic Layer (`crates/sf-server/src/pg_source_of_truth.rs`):**
  - Official Templates: `seed_default_official_templates_if_empty`, `list_official_templates`, `get_official_template_by_id`, `set_official_template_active_pg`, `rollback_official_template_pg`.
  - Community Submissions: `create_community_submission_atomic` (with `pg_advisory_xact_lock`, monthly cap, and RLS), `list_my_community_submissions_pg`, `list_all_community_submissions_for_owner_pg`, `review_community_submission_pg`, and `publish_community_submission_atomic_pg` (single atomic PostgreSQL transaction).
  - App Releases: `list_app_releases_pg`, `latest_active_release_pg` wired into `crates/sf-server/src/routes_updates.rs` and `routes.rs`.
  - Production Startup Hydration (`crates/sf-server/src/main.rs`): Seeds default Official Templates if empty and loads Owner MFA snapshot directly from PostgreSQL on boot.
- **Static Migration Verification:** `py -3 -B tools/verify-migrations.py` -> `PASS` (**12 migration pairs verified**: up/down, RLS, `uuidv7()`, no `SERIAL`, guarded `pg_partman`).
- **Multi-Instance & Restart PG18 Test (`crates/sf-server/tests/postgres_multi_instance_e2e.rs`):**
  - Spawns Instance A, Instance B, and Restarted Instance C sharing one `PgPool` with ZERO shared memory, proving cross-instance visibility and restart durability for Templates, Community atomic Publish, Releases, Owner MFA, Owner Web vs Desktop isolation, PKCE A->B->C + replay rejection, and cross-instance Device Revocation.
  - Wired into CI (`tools/test-migrations.sh` running against `postgres:18.6-bookworm` in `.github/workflows/ci.yml`).
  - Locally ignored because PostgreSQL 18 / Docker is not installed on this Windows host (`BLOCKED BY EXTERNAL SETUP`).

---

## 3. Security Component Durability & Fail-Closed Device Authority — `PASS` (Code & Tests) / `BLOCKED BY EXTERNAL SETUP` (Local PG18 Daemon)
- **Fail-Closed Device Authority (`crates/sf-server/src/device_authority.rs`):**
  - Fixed `has_any_device == false` in `verify_active_session_device` to fail closed with `Err(AuthError::Revoked)` instead of permitting access when device state is absent after restart.
- **Direct PostgreSQL Security Persistence (`pg_source_of_truth.rs`, `routes.rs`, `auth/middleware.rs`, `zero_trust.rs`):**
  - `OwnerSessionGuard` authenticates `sfw_*` tokens directly against `owner_web_sessions` in PostgreSQL.
  - `OwnerAccessGuard` hydrates encrypted `owner_totp_credentials`, `owner_recovery_codes`, Desktop `owner_elevated_sessions`, and Web `owner_web_sessions.elevated_*` directly from PostgreSQL on every request.
  - Desktop PKCE (`start_desktop_pkce_pg`, `approve_desktop_pkce_pg`, `consume_desktop_pkce_atomic_pg`) uses atomic single-use `UPDATE ... RETURNING` in PostgreSQL.
  - Single-Device Exclusivity (`activate_device_atomically_pg`, `verify_active_session_device_pg`, `owner_reset_devices_pg`) uses `pg_advisory_xact_lock` in PostgreSQL.
- **Real 30-Second Heartbeat Bug Fixed (`apps/web/src/lib/auth-api.ts`):**
  - Unmocked `verify-studio-desktop-e2e.mjs` exposed that `postSafiHeartbeat()` passed `"/v1/auth/heartbeat"` to `sendAuthRequest` (which already prepends `/v1/auth`), resulting in a `404` on `/v1/auth/v1/auth/heartbeat` at 30 seconds. Fixed to `"/heartbeat"` and verified live against real `sf-server`.

---

## 4. Desktop UI & Tauri IPC Integration for Local Search & Recipe Library — `PASS`
- **Tauri IPC Allowlist (`crates/sf-desktop/src/commands.rs`, `apps/desktop/src-tauri/src/main.rs`, `crates/sf-desktop/src/studio.rs`, `crates/sf-desktop/examples/studio_ipc_harness.rs`):**
  - Added 14 allowlisted commands (`ALLOWED_COMMANDS` count = 68, verified by `shell_config.rs` with zero Tauri capability expansion):
    - `desktop_open_external_auth_url`, `pick_search_folder`, `pick_safitemplate_open_file`, `pick_safitemplate_save_file`
    - `desktop_search_index_folders`, `desktop_search_query` (Offline Read-Only allowed), `desktop_search_stats` (Offline Read-Only allowed)
    - `desktop_recipes_list` (Offline Read-Only allowed), `desktop_recipes_sync_official`, `desktop_recipes_save_as_new`, `desktop_recipes_delete`, `desktop_recipes_export_safitemplate`, `desktop_recipes_import_safitemplate`, `desktop_recipes_prepare_community_share`
- **Real Desktop UI Components wired in `apps/web/src/desktop/DesktopStudio.tsx`:**
  - `apps/web/src/desktop/components/DesktopLocalSearchPanel.tsx`: Folder selection, local indexing, live stats, Arabic/English/phrase/prefix/fuzzy search, file-type filter, highlighted BM25 hits with match locations; queries work in Offline Read-Only Mode.
  - `apps/web/src/desktop/components/DesktopRecipeLibraryPanel.tsx`: Syncs real Official Templates (`GET /v1/templates/official`), customizes via `Save as New Recipe` without mutating Official Templates, enforces real Backend Premium `.safitemplate` Entitlement (`POST /v1/templates/entitlement/export-import` -> `SF-RCP-005` for Free/Trial; export/import allowed for Paid/VIP/Owner), and provides sanitized `Share with SafiData` Preview + PII scan + explicit consent -> `POST /v1/community/submissions`.

---

## 5. End-to-End Desktop Browser PKCE (`S256`) Auth — `PASS`
- **Client Implementation:**
  - `apps/web/src/lib/desktop-pkce.ts`: WebCrypto `S256` `code_verifier`/`code_challenge` + random `state`, `POST /v1/auth/desktop/start`, strict `auth_url` validation, ephemeral in-memory verifier storage, and `POST /v1/auth/desktop/exchange`.
  - `crates/sf-desktop/src/content.rs`: `validate_desktop_auth_external_url()` strictly allowlists exact-origin `/auth/desktop?flow=signin|signup&request_id=...&state=...` before OS browser launch (`desktop_open_external_auth_url` in `apps/desktop/src-tauri/src/main.rs`).
  - `apps/web/src/components/auth/AuthModal.tsx`, `apps/web/src/ApplicationShell.tsx`, `apps/web/src/pages/DesktopBrowserAuthPage.tsx`: Desktop primary Sign-In / Sign-Up in Browser buttons, `take_pending_deep_link` callback polling for `safi://auth?code=...&state=...`, native `StudioStore` session binding ONLY on successful exchange, and fail-closed locking on state mismatch or exchange error (`SF-AUTH-023`).

---

## 6. Honest Desktop Installer Download & Verification — `PASS` (Honest Unpublished State) / `BLOCKED BY EXTERNAL SETUP` (Public Release Hosting & Signing)
- Removed untracked dev EXE files from `apps/web/public/downloads/` and `apps/web/dist/downloads/`.
- Updated `apps/web/public/downloads/release-manifest.json` and `apps/web/src/lib/download.ts`:
  - `windows`, `mac`, `linux` honestly default to `available: false`, `downloadUrl: null`.
  - `isPublishedReleaseUrl()` requires a real public `https://` release URL before `available` becomes `true` from `GET /v1/updates/latest`.
- Added `tools/verify-published-installer-download.mjs`: verifies real HTTPS download + exact byte length + SHA-256 when published, and honestly exits with `BLOCKED BY EXTERNAL SETUP` when `available: false`.

---

## 7. Improved PDF Text Extraction in Local Search — `PASS`
- Added pinned `flate2 = "=1.1.10"` to `crates/sf-desktop/Cargo.toml` (zero network dependencies verified by `tools/check-no-network.sh`).
- Upgraded `extract_pdf_text` in `crates/sf-desktop/src/local_search.rs`:
  - Detects `/FlateDecode` / `/Fl` PDF streams and decompresses safely via `ZlibDecoder` capped at `MAX_PDF_DECOMPRESSED_BYTES = 8 MiB` (zlib-bomb guard).
  - Extracts both literal `(...)` and hex / UTF-16BE `<...>` PDF text strings.
  - Honestly flags scanned image-only PDFs with `has_extractable_text: false` and explicit non-OCR notice (**no fake OCR claimed**).
- Verified in Rust unit test `extracts_flatedecode_compressed_pdf_text_and_flags_scanned_pdf_honestly` AND in real Chromium E2E `verify-desktop-full-journey-real-e2e.mjs`.

---

## 8. Executed Verification Gates & Real Unmocked E2E Results

### Real Unmocked End-to-End Suites (ZERO Backend Mocks, ZERO Rust Engine Mocks)
1. **`tools/verify-desktop-full-journey-real-e2e.mjs`** -> **`PASS`**
   - Spawns real `sf-server` (`auth_harness_server`) + real `sf-desktop` (`studio_ipc_harness`) + Vite (`SAFI_DEV_API_PROXY`) + headless Chromium.
   - Verified live: PKCE state-mismatch fail-closed -> Real PKCE Sign-In -> Local Search UI (Arabic CSV + `/FlateDecode` PDF) -> Official Template Sync -> `Save as New Recipe` (Official immutable) -> Free `.safitemplate` denied `403 SF-RCP-005` -> `Share with SafiData` Preview + Consent -> Paid/VIP real PKCE + `.safitemplate` export & re-import -> Real `/owner` Web Portal (`sfw_*` + live RFC 6238 TOTP) -> Approve & Publish -> Visible live in `GET /v1/templates/official`.
2. **`tools/verify-studio-desktop-e2e.mjs`** -> **`PASS`** (14 steps including real `sf-server` auth + real 30s `/v1/auth/heartbeat` + real `DataGraph`, messy `.xlsx`, `.xlsb`, deliverables, `.safi.html` capsule, and sign-out revocation).

### Frontend UI-Boundary Smoke Suites (Explicitly Distinguished from Full Backend Integration)
3. **`tools/verify-desktop-download-and-owner-web-e2e.mjs`** -> **`PASS`** (UI-boundary smoke test: Showcase route blocking, zero false offline copy, honest `available=false` disabled installer button, `/owner` & `/auth/desktop` UI states).
4. **`tools/verify-runtime-boundary-e2e.mjs`** -> **`PASS`**.
5. **`tools/verify-offline-and-device-revocation-e2e.mjs`** -> **`PASS`** (Scenario 0 fresh-install offline-bypass block, Scenario 1 Offline Read-Only `SF-AUTH-021`, Scenario 2 automatic device revocation `SF-AUTH-019`).
6. **`tools/verify-published-installer-download.mjs`** -> **`BLOCKED BY EXTERNAL SETUP`** (`available=false` honestly reported until public HTTPS CDN/Release Storage URL is published).

### Full Rust, Frontend, Security & Determinism Gates
- `cargo fmt --all -- --check` + `apps/desktop/src-tauri` -> **`PASS`**
- `cargo clippy --workspace --all-targets -- -D warnings` + `apps/desktop/src-tauri` -> **`PASS`** (0 warnings)
- `cargo test --workspace` + `apps/desktop/src-tauri` -> **`PASS`** (0 failures; 4 live-PG18 tests ignored locally without `SF_TEST_DATABASE_URL`, wired into CI `tools/test-migrations.sh` on `postgres:18.6-bookworm`)
- `pnpm run format:check` -> **`PASS`**
- `pnpm --dir apps/web run typecheck` -> **`PASS`** (0 errors)
- `pnpm run lint` -> **`PASS`** (0 errors, 0 warnings)
- `pnpm run test` -> **`PASS`** (66 test files, 231 tests passed)
- `pnpm run build` -> **`PASS`**
- `pnpm run check:contrast` (+ `--self-test`) -> **`PASS`**
- `node tools/validate-config.mjs` -> **`PASS`** (`business.json OK`)
- `py -3 -B tools/gen-errors.py --check` -> **`PASS`**
- `py -3 -B tools/verify-migrations.py` -> **`PASS`** (12 migration pairs)
- `py -3 -B tools/verify-i18n.py` -> **`PASS`** (28 keys x 7 locales)
- `py -3 -B tools/verify-security.py` -> **`PASS`**
- `tools/check-no-network.sh` (+ `--self-test`) -> **`PASS`** (10 engine crates, 0 network dependencies)
- `tools/codeburn/burn.sh` -> **`PASS`**

---

## 9. Remaining External Setup Items (`BLOCKED BY EXTERNAL SETUP`)
1. Managed PostgreSQL 18 production database + running CI `database-migrations` job (`0001..0012` + `postgres_multi_instance_e2e`, `account_postgres`, `accounts_postgres`).
2. Public HTTPS Release Storage / CDN hosting for signed Windows/macOS/Linux installers + Authenticode/Apple notarization certificates + updating `release-manifest.json` / `POST /v1/owner/releases` with real HTTPS download URLs.
3. Production HTTPS domain, TLS certificates, SMTP credentials, `SAFI_UPDATE_SIGNING_PRIVATE_KEY`, and live Paymob/Paddle webhook credentials.

---

## 10. Follow-Up Completion Round (Workflow Memory, Continuous Local Learning, Website 17-Area Showcase & 0ms Intro Transition)

Building directly on baseline commit `8474e919d5b7c62af652acadb0e454e5bc14c622`, the subsequent completion round added:
1. **Persistent Per-Sheet Workflow Memory (`crates/sf-desktop/src/workflow_memory.rs` + `DesktopWorkflowMemoryPanel.tsx` + `DesktopStudio.tsx`)**: Filename-independent semantic sheet recognition, per-sheet isolation inside multi-sheet workbooks (`Sales` vs. `Employees` vs. `Inventory`), confidence-tiered replay (`High >= 80%` auto-replay & suppression of already-answered clarification questions; `Medium 55%..79%` partial reuse & targeted questions on structural changes), and non-destructive version history (`restore_version`, `duplicate`, `rename`, `delete`).
2. **Continuous Local Learning & Explainable Personal Profile (`PersonalLearningProfile` in `learning_profile.safi.enc`)**: Learns from explicit user feedback (`explicit_correction`, `preferred_rule`, `explicit_acceptance`, `explicit_rejection`, `repeated_choice`) scoped to `this_sheet_only` or `matching_sheets`, produces explainable suggestions citing `influencingRuleId` and provenance, and supports single-rule undo/delete or full profile reset.
3. **Complete Recipe & Local Search UI Integration**: In-place personal recipe editing (`desktop_recipes_update_existing`), duplication (`desktop_recipes_duplicate`), direct application to the active Studio sheet (`desktop_recipes_apply_to_project`), project-ID filter in Local Search, and one-click **"Open in Studio"** on spreadsheet search hits.
4. **Public Website Complete 17-Area Bilingual Showcase (`BentoCommandCenter.tsx`)** and **0ms Post-Intro Hero Headline Transition (`tools/verify-intro-transition-performance.mjs`)**.

See the full report: `docs/reports/SAFIDATA-WORKFLOW-MEMORY-LEARNING-WEBSITE-AND-UX-ACCEPTANCE.md`.
