# SafiData — Master Product, Security, Optimal Decision Engine, Local Intelligence & Follow-Up Audit Acceptance Report

---

## 1. Repository Baseline & Git State (Verified)

- **Repository:** `mo7amedrabei14-cell/Safi`
- **Repository Visibility Risk Disclosure:** GitHub repository is currently **Public**. Per mandatory execution rules, the agent did **not** modify repository visibility; the owner will decide and perform any visibility change and secret rotation.
- **Target Branch:** `main` ONLY (no branches created, no `git commit`, no `git push`, no deployment, no production secrets modified).
- **Current Pushed `main` HEAD SHA:** `49e85e191268de20ee3443238e8dc36ea42b9b4c`
- **Parent Baseline SHA:** `739f56cf66beaa5147dbe9445c46a91e4c3dfc94`
- **Working Tree State:** Dirty on `main` (contains only this follow-up audit's remediation changes and this updated acceptance report, awaiting manual owner review and push).
- **GitHub CI vs. Local Verification Distinction:** All commands and test results recorded in Section 6 below were executed **locally** on this Windows workstation. The combined GitHub status query for `49e85e191268de20ee3443238e8dc36ea42b9b4c` returned **no status entries**; therefore GitHub CI is **not** claimed as passed (`UNVERIFIED / NO GITHUB STATUS ENTRIES`).

---

## 2. Complete Plan & Follow-Up Audit Status Matrix

Allowed states: `PASS`, `PARTIAL`, `FAIL`, `BLOCKED BY EXTERNAL SETUP`, `NOT IMPLEMENTED`.

| Area | Requirement | Status | Concrete Runtime / Test Evidence & Notes |
| --- | --- | --- | --- |
| **Follow-Up P0 — Local Encryption** | Mandatory `VaultKey` keystore lifecycle (CSPRNG + Argon2id + ChaCha20-Poly1305 self-verified keystore, zero machine-fingerprint secret, fail-closed `SF-SYS-024`, atomic verified migration from legacy plaintext `.json`, zero plaintext `.json` writes in production) | **`PASS`** | 7 vault tests in `crates/sf-desktop/src/vault.rs` + `studio::tests::studio_store_writes_only_encrypted_files_and_atomic_workflow_replay_rolls_back_on_error` + Tauri `.setup()` `ensure_local_vault_key()` in `apps/desktop/src-tauri/src/main.rs` |
| **Follow-Up Sec 2 — Per-Sheet Workflow Memory** | Content-based matching (observed types, numeric/date parse ratios, null ratios, distinct cardinality, leading-zero IDs, currency detection; generic placeholder headers blocked from `High` confidence), atomic staged replay rollback on failure (`SF-RCP-001`), error propagation, and idempotent High-confidence auto-replay in `DesktopStudio.tsx` | **`PASS`** | `workflow_memory::tests::content_based_matching_generic_headers_and_honest_learning_metrics`, `studio::tests::studio_store_writes_only_encrypted_files_and_atomic_workflow_replay_rolls_back_on_error`, `tools/verify-desktop-full-journey-real-e2e.mjs` |
| **Follow-Up Sec 3 — Continuous Learning Metrics** | Honest auditable raw counters (`completedReplaysTotal`, `questionsReusedTotal`, `verifiedStepsAppliedTotal`, corrections, acceptances, rejections, undos) and `observedRetentionRatePct: Option<u8>` (`null` / `"No evaluation evidence yet (0/0)"` when denominator is `0`, never `100%` on empty history) | **`PASS`** | `workflow_memory::tests::content_based_matching_generic_headers_and_honest_learning_metrics`, `DesktopWorkflowMemoryPanel.tsx`, `studio-bridge.ts` |
| **Follow-Up Sec 4 — Safi Decision Engine** | Data-driven lexicographic scoring across all valid measures and feasible chart families (`bar`, `column`, `line`, `area`, `stacked`, `pie`, `donut`, `scatter`, `histogram`, `treemap`, `waterfall`, `pareto`, `kpi`), zero hardcoded `"EGP"`, mixed-currency `InsufficientEvidenceRequiresQuestion`, zero-measure `COUNT()` safety, fixed Report & Presentation formula/column bug, and 4-way duplicate classification (exact / verified repeated entity tx / conflicting key / ambiguous review) | **`PASS`** | `decision_engine::tests::decision_engine_protects_numeric_employee_id_distinguishes_duplicates_and_optimizes_lexicographically`, `decision_engine::tests::decision_engine_covers_currency_safety_zero_measures_charts_duplicates_learning_and_ollama`, `tools/verify-desktop-full-journey-real-e2e.mjs` |
| **Follow-Up Sec 5 — AI Architecture & Local Ollama Runtime Probe** | Deterministic Rust engine separated from optional LLM; real bounded loopback probe (`127.0.0.1:11434/api/tags`) at Desktop shell boundary (`apps/desktop/src-tauri/src/main.rs` & `studio_ipc_harness.rs`) reporting honest status (`available_with_models`, `reachable_zero_models`, `service_unreachable`) while keeping `crates/sf-*/src` 100% network-free | **`PASS`** (Local boundary & unreachable probe verified; live model inference requires local Ollama service + pulled model on user device) | `decision_engine::tests::*`, `apps/desktop/src-tauri/src/main.rs` (`probe_local_ollama_loopback`), zero `std::net` in `crates/sf-*/src` |
| **Follow-Up Sec 6 — Website Layout & Hero Copy** | Balanced `3×2` capability cards grid (`#verified-capabilities-matrix`) on desktop (`1920`, `1810`, `1440`, `1280`), `2×3` on tablet (`768`), `1×6` on mobile (`390`); `#compare` 12-row comparison table intact; updated full-journey bilingual hero summary removing `"Offline Read-Only access to saved projects"` from marketing copy only (keeping real Offline Read-Only feature intact) | **`PASS`** | `verify-website-visual-audit.mjs` (`6/6` viewports `ok: true`, `widthSpread: 0px`, `compareRows: 12`), Light (`vision-glass`) & Dark (`google-gemini`), Arabic RTL & English LTR screenshots verified |
| **Auth, RBAC & Single-Device Exclusivity** | Owner `/v1/owner/*` RBAC (`403 SF-AUTH-013`), Web `sfw_*` vs Desktop `sfs_*` separation, RFC 6238 TOTP MFA, Desktop Browser PKCE (`S256`), single-device revocation (`SF-AUTH-019`), Offline Read-Only Mode (`SF-AUTH-021`) & fresh-install bypass block (`SF-AUTH-001`) | **`PASS`** | `cargo test --workspace` (`auth_flow`, `dual_database_owner_templates_e2e`, `security_online_authorization_e2e`, `zero_trust_guard`), `apps/desktop/src-tauri` tests, `verify-offline-and-device-revocation-e2e.mjs`, `verify-runtime-boundary-e2e.mjs` |
| **Multi-Format Ingestion, Cleaning, Recipes, Local Search & Deliverables** | `.xlsx`, `.xlsm`, `.xlsb`, `.xls`, `.ods`, `.csv`, `.tsv` ingestion; 21 charts; 9 outputs (`.xlsx`, `.csv`, `.pptx`, `.docx`, `.pdf`, `.png`, `.svg`, `.safi.html`, Power BI); Official Templates immutability; `.safitemplate` (`SF-RCP-005`); `Share with SafiData`; BM25 Local Search (Arabic/English + `FlateDecode` PDFs) | **`PASS`** | `cargo test --workspace`, `verify-desktop-full-journey-real-e2e.mjs`, `verify-studio-desktop-e2e.mjs`, `verify-exports-roundtrip.mjs`, `verify-capsule-chromium.mjs` |
| **Intro Transition & Theme Stability** | `0ms` crisp hero headline on intro exit and `0` dark-flash frames across `vision-glass`, `apple-light`, and `google-gemini` | **`PASS`** | `tools/verify-intro-transition-performance.mjs` (`0ms` across 5 cases), `tools/verify-intro-theme-transition.mjs` (`0` dark-flash frames across 4 cases) |
| **Billing — Paddle (International USD)** | Code & cryptographic webhook handler (`crates/sf-server/src/billing.rs`, `routes.rs` `/v1/billing/checkout` & `/v1/billing/webhooks/paddle`, HMAC-SHA256 verification, idempotency, plan activation/cancellation/chargeback, `config/business.json`) | **`PARTIAL` (Code/Unit/HTTP tests `PASS` locally; Live Checkout & Webhooks `BLOCKED BY EXTERNAL SETUP`)** | Local signature & subscription lifecycle tested in `sf-server` (`security_online_authorization_e2e.rs`). **Missing for live acceptance:** real `SAFI_PADDLE_API_KEY`, `SAFI_PADDLE_WEBHOOK_SECRET`, Paddle product/price IDs, and public HTTPS webhook endpoint. |
| **Billing — Paymob (Egypt EGP)** | Code & cryptographic webhook handler (`crates/sf-server/src/billing.rs`, `routes.rs` `/v1/billing/checkout` & `/v1/billing/webhooks/paymob`, HMAC-SHA512 verification, idempotency, plan activation/refund, `config/business.json`) | **`PARTIAL` (Code/Unit/HTTP tests `PASS` locally; Live Checkout & Webhooks `BLOCKED BY EXTERNAL SETUP`)** | Local signature & subscription lifecycle tested in `sf-server` (`security_online_authorization_e2e.rs`). **Missing for live acceptance:** real `SAFI_PAYMOB_API_KEY`, `SAFI_PAYMOB_HMAC_SECRET`, Paymob integration/iframe IDs, and public HTTPS webhook endpoint. |
| **Central Database — Live PostgreSQL 18 Multi-Instance Runtime** | `PgControlPlaneStore` (`crates/sf-server/src/pg_control_plane.rs`) + migrations `0001..0012` + startup hydration | **`BLOCKED BY EXTERNAL SETUP`** | `postgres_multi_instance_e2e.rs` was skipped (`ignored`) because no live PostgreSQL 18 server / `SF_TEST_DATABASE_URL` is running on this local Windows machine. **Needed to test:** Provision a PostgreSQL 18 database, run migrations `0001..0012`, and run `SF_TEST_DATABASE_URL=postgres://... cargo test -p sf-server --test postgres_multi_instance_e2e -- --ignored`. |
| **Live OAuth Providers (Google, Apple, GitHub)** | OAuth configuration flags & callback wiring in `sf-server` | **`BLOCKED BY EXTERNAL SETUP`** | **Missing for live acceptance:** Live Google, Apple, and GitHub OAuth client IDs/secrets and verified production HTTPS redirect URIs. |
| **Production Code-Signing & Public Installer Hosting** | Authenticode / Apple notarization certificates & public HTTPS installer release storage (`available: false` in release manifests) | **`BLOCKED BY EXTERNAL SETUP`** | `tools/verify-published-installer-download.mjs` honestly confirms `available=false`. **Needed to test:** Code-signing certificates, signed `.msi`/`.exe`/`.dmg`/`.AppImage` build artifacts uploaded to HTTPS storage, and updated SHA-256 + Ed25519 signed `release-manifest.json`. |
| **GitHub Actions CI Verification** | Remote GitHub Actions workflow execution status for pushed `main` commit `49e85e191268de20ee3443238e8dc36ea42b9b4c` | **`PARTIAL` / `UNVERIFIED`** | Combined GitHub status query returned no status entries. Must be verified directly in GitHub Actions once the owner pushes this follow-up round. |

---

## 3. Exact Files Changed in This Follow-Up Audit

| File | Summary of Changes |
| --- | --- |
| [vault.rs](file:///C:/Users/mo7am/OneDrive/Work/Safi/crates/sf-desktop/src/vault.rs) | Implemented CSPRNG + Argon2id + ChaCha20-Poly1305 self-verified keystore (`VaultKey::load_or_init_for_storage_dir`), fail-closed `write_encrypted_mandatory` & `read_encrypted_or_migrate_legacy` (verifying ciphertext decryption on disk before deleting legacy `.json`), and 7 mandatory encryption/migration/tamper tests. |
| [workflow_memory.rs](file:///C:/Users/mo7am/OneDrive/Work/Safi/crates/sf-desktop/src/workflow_memory.rs) | Eliminated plaintext JSON fallback; added content-based table profiling (`build_sheet_semantic_signature_with_table`, `save_sheet_workflow_with_table`, `match_incoming_sheet_with_table`), generic placeholder header downgrade, content-type drift detection, honest numerator/denominator `LearningEffectivenessMetrics` (`observed_retention_rate_pct: Option<u8>`), and unit tests. |
| [recipe_library.rs](file:///C:/Users/mo7am/OneDrive/Work/Safi/crates/sf-desktop/src/recipe_library.rs) | Switched persistence to `write_encrypted_mandatory` and `read_encrypted_or_migrate_legacy` (zero plaintext fallback). |
| [local_search.rs](file:///C:/Users/mo7am/OneDrive/Work/Safi/crates/sf-desktop/src/local_search.rs) | Switched persistence (`save_to_dir` / `load_from_dir`) to `write_encrypted_mandatory` and `read_encrypted_or_migrate_legacy` (zero plaintext fallback). |
| [studio.rs](file:///C:/Users/mo7am/OneDrive/Work/Safi/crates/sf-desktop/src/studio.rs) | Auto-initialized `VaultKey` in `with_storage_dir` / `with_storage_dir_and_verifier` + added fail-closed `ensure_local_vault_key()`; wired content-based matching into workflow memory save/match/replay & decision engine; refactored `desktop_workflow_memory_apply_replay` to run on an isolated staged `DataGraph` + `StudioWorkspaceArtifacts` clone with atomic commit and full error propagation; added `ollama_probe` injection; added encrypted-only & atomic replay rollback tests. |
| [decision_engine.rs](file:///C:/Users/mo7am/OneDrive/Work/Safi/crates/sf-desktop/src/decision_engine.rs) | Replaced hardcoded winning scores with measured dataset statistics; evaluated all valid measures (`SUM`/`AVG`) and all feasible chart families; eliminated hardcoded `"EGP"` and added currency/unit detection + mixed-currency `InsufficientEvidenceRequiresQuestion`; fixed zero-measure `COUNT()` safety and Report & Presentation `SUM(group_col)` bug; upgraded duplicate analysis to 4-way classification (exact / verified repeated entity tx / conflicting key / ambiguous review); added `OllamaRuntimeProbe`; expanded unit tests across all scenarios. |
| [main.rs](file:///C:/Users/mo7am/OneDrive/Work/Safi/apps/desktop/src-tauri/src/main.rs) | Enforced `store.ensure_local_vault_key()` during Tauri `.setup()` startup and added real bounded `probe_local_ollama_loopback()` (`127.0.0.1:11434/api/tags`) in `desktop_decision_engine_evaluate` (outside air-gapped engine crates). |
| [studio_ipc_harness.rs](file:///C:/Users/mo7am/OneDrive/Work/Safi/crates/sf-desktop/examples/studio_ipc_harness.rs) | Wired `probe_local_ollama_loopback()` into `desktop_decision_engine_evaluate` for unmocked E2E verification. |
| [studio-bridge.ts](file:///C:/Users/mo7am/OneDrive/Work/Safi/apps/web/src/desktop/studio-bridge.ts) | Added honest learning retention metric fields (`observedRetentionNum`, `observedEvaluationDenom`, `observedRetentionRatePct`, definitions), optional `projectId` in `matchDesktopSheetWorkflowMemory`, and `ambiguousDuplicateReviewRowIds`. |
| [DesktopWorkflowMemoryPanel.tsx](file:///C:/Users/mo7am/OneDrive/Work/Safi/apps/web/src/desktop/components/DesktopWorkflowMemoryPanel.tsx) | Passed `projectId` for content-based sheet matching; removed hardcoded `"EGP"` on workflow column save; updated learning metrics strip to display honest Observed Decision Retention Rate with numerator/denominator (`0/0` -> `"No evaluation evidence yet (0/0)"`) and definition disclosure. |
| [DesktopStudio.tsx](file:///C:/Users/mo7am/OneDrive/Work/Safi/apps/web/src/desktop/DesktopStudio.tsx) | Passed `projectId` to sheet workflow matching; implemented idempotent automatic High-confidence workflow replay guarded by `autoReplayedKeysRef` with instant Undo banner. |
| [index.css](file:///C:/Users/mo7am/OneDrive/Work/Safi/apps/web/src/index.css) | Added `.safi-capabilities-grid-3x2` responsive CSS grid (`3` columns on desktop `>= 1024px`, `2` columns on tablet `640px..1023px`, `1` column on mobile `< 640px`, equal stretched card heights). |
| [BentoCommandCenter.tsx](file:///C:/Users/mo7am/OneDrive/Work/Safi/apps/web/src/components/home/BentoCommandCenter.tsx) | Applied `.safi-capabilities-grid-3x2` to the 6 capability cards inside `#verified-capabilities-matrix` (leaving `#compare` untouched) and updated the hero opening summary (`data-hero-summary="true"`) with the exact full-journey bilingual copy (without the Offline Read-Only marketing phrase). |
| [SAFIDATA-MASTER-PRODUCT-SECURITY-INTELLIGENCE-ACCEPTANCE.md](file:///C:/Users/mo7am/OneDrive/Work/Safi/docs/reports/SAFIDATA-MASTER-PRODUCT-SECURITY-INTELLIGENCE-ACCEPTANCE.md) | Updated baseline SHAs, evidence matrix, Paddle/Paymob rows, local vs. CI distinction, and exact verification commands & outputs. |

---

## 4. Local Verification Commands, Exact Exit Codes & Raw Results

All commands below were executed locally on Windows PowerShell against the current working tree on `main` (`HEAD = 49e85e191268de20ee3443238e8dc36ea42b9b4c`):

| Command | Exit Code | Concise Raw Result |
| --- | --- | --- |
| `cargo fmt --all -- --check` | `0` | Clean Rust formatting across workspace |
| `cargo clippy --workspace --all-targets -- -D warnings` | `0` | `0` warnings / `0` errors across all workspace crates & targets |
| `cargo test --workspace` | `0` | All workspace tests passed (`sf-arabic` 4, `sf-clean` 10, `sf-cli` 4, `sf-core` 17+2, `sf-desktop` 85 lib + integration, `sf-io`, `sf-profile`, `sf-recipe`, `sf-report`, `sf-server` unit + `auth_flow` 11 + `dual_database_owner_templates_e2e` 1 + `log_center` 1 + `security_online_authorization_e2e` 2 + `zero_trust_guard` 1, `sf-wasm` 5). `postgres_multi_instance_e2e` was `ignored` (`1 ignored`, requires `SF_TEST_DATABASE_URL`). |
| `cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml` | `0` | Tauri Desktop shell compiled cleanly (Rust 2021 edition) |
| `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` | `0` | `2 passed; 0 failed; 1 ignored` (manual TLS perf benchmark ignored) |
| `Select-String -Path "crates\sf-*\src\*.rs" -Pattern "std::net\|core::net"` | `0` | `0` matches inside `crates/sf-*/src/*.rs` (zero network in engine crates) |
| `pnpm run format:check` | `0` | `All matched files use Prettier code style!` |
| `pnpm --dir apps/web run typecheck` | `0` | `tsc --noEmit` succeeded with `0` errors |
| `pnpm run lint` | `0` | ESLint passed with `0` errors |
| `pnpm run test` | `0` | All Vitest suites passed across workspace packages and `apps/web` |
| `pnpm run build` | `0` | Production Vite build succeeded (`dist/index.html` + bundles built) |
| `pnpm run check:contrast` | `0` | `Verified 46 text pairs (min 4.58:1 >= 4.5:1) and 36 UI/chart pairs (min 4.37:1 >= 3.0:1) across Light and Dark themes` + self-test `OK` |
| `node tools/validate-config.mjs` | `0` | `business.json OK` |
| `node tools/verify-intro-transition-performance.mjs` | `0` | `0ms` headline crisp delay across all 5 cases (`en-1440x900`, `ar-1440x900`, `en-1920x1080`, `ar-1280x800`, `en-reduced-motion`) |
| `node tools/verify-intro-theme-transition.mjs` | `0` | `0` dark-flash frames and `0` mismatched background frames across `vision-glass` (EN/AR), `apple-light`, and `google-gemini` |
| `node <scratch>/verify-website-visual-audit.mjs` | `0` | `6/6` viewports `ok: true` (`1810x1000` AR Light `[3,3]`, `1920x1080` EN Dark `[3,3]`, `1440x900` EN Light `[3,3]`, `1280x800` AR Dark `[3,3]`, `768x1024` EN Light `[2,2,2]`, `390x844` AR Light `[1,1,1,1,1,1]`), `widthSpread: 0`, `compareRows: 12`, `hasForbiddenOfflinePhrase: false`, `hasCompleteJourney: true` |
| `node tools/verify-desktop-full-journey-real-e2e.mjs` | `0` | `PASS (100% REAL, ZERO BACKEND/RUST MOCKS)`: PKCE Auth + Local Search + Workflow Memory & Learning + Safi Decision Engine + Recipe Library + `.safitemplate` (`SF-RCP-005`/Paid) + Share + Owner Portal TOTP Review & Publish |
| `node tools/verify-studio-desktop-e2e.mjs` | `0` | Passed all 14 steps (`SF-AUTH-001` pre-auth/forged/logout, real `sf-server`, CSV + messy `.xlsx` + binary `.xlsb`, metrics, What-If, charts, `.safi.html`, `.xlsx`, `.pdf`) |
| `node tools/verify-offline-and-device-revocation-e2e.mjs` | `0` | `OFFLINE_AND_DEVICE_REVOCATION_E2E_OK offline_readonly=SF-AUTH-021 protected_disabled=true device_replaced_auto_logout=SF-AUTH-019 privacy_clean=true` |
| `node tools/verify-runtime-boundary-e2e.mjs` | `0` | `RUNTIME_BOUNDARY_E2E_OK showcase=unlocked desktop_regular=auth_gated_no_owner desktop_owner=totp_mfa_gated` |
| `node tools/verify-desktop-download-and-owner-web-e2e.mjs` | `0` | `PASS (UI-boundary smoke test)` |
| `node tools/verify-exports-roundtrip.mjs` | `0` | `PYTHON_VALIDATION_OK`, `ALL_21_SVG_VISUALS_VERIFIED count=21`, `CHROMIUM_IMAGE_AND_SVG_PREVIEW_OK`, `CHROMIUM_SAFI_HTML_PLAYER_OK` |
| `node tools/verify-capsule-chromium.mjs` | `0` | `.safi.html` verified over `file://` (`scripts: 0`, `externalRequests: 0`, `consoleErrors: 0`, `pageErrors: 0`) |
| `node tools/verify-published-installer-download.mjs` | `0` | `BLOCKED BY EXTERNAL SETUP: release-manifest.json honestly reports available=false` |

---

## 5. Updated Website Visual Verification Screenshots

Captured against the production `apps/web/dist` build:

- **1810×1000 Arabic RTL Light (3×2 Balanced Cards):** `safi-cards-desktop-1810-ar-light.png`
- **1810×1000 Arabic RTL Light (Revised Hero Copy):** `safi-hero-desktop-1810-ar-light.png`
- **1920×1080 English LTR Dark (3×2 Balanced Cards):** `safi-cards-desktop-1920-en-dark.png`
- **1920×1080 English LTR Dark (Revised Hero Copy):** `safi-hero-desktop-1920-en-dark.png`
- **1440×900 English LTR Light (Revised Hero Copy):** `safi-hero-desktop-1440-en-light.png`
- **768×1024 Tablet English LTR Light (2×3 Cards):** `safi-cards-tablet-768-en-light.png`
- **390×844 Mobile Arabic RTL Light (1×6 Cards):** `safi-cards-mobile-390-ar-light.png`
