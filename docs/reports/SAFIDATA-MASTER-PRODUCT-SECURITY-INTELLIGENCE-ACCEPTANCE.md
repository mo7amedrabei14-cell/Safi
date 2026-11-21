# SafiData — Master Product, Security, Optimal Decision Engine & Local Intelligence Acceptance Report

---

## 1. Current Main SHA & 2. Starting Point Baseline

- **Repository:** `mo7amedrabei14-cell/Safi`
- **Target Branch:** `main` ONLY (no branches created, no `git commit`, no `git push`, no deployment executed)
- **Current `main` Baseline SHA (`HEAD` & `origin/main`):** `739f56cf66beaa5147dbe9445c46a91e4c3dfc94`
- **Prior Baseline SHA:** `8474e919d5b7c62af652acadb0e454e5bc14c622`

---

## 3. Complete Plan Status Matrix (Strict 5-State Classification)

Allowed states: `PASS`, `PARTIAL`, `FAIL`, `BLOCKED BY EXTERNAL SETUP`, `NOT IMPLEMENTED`.

| Plan Section | Requirement | Status | Verification Evidence |
| --- | --- | --- | --- |
| **Sec 2.1** | Customer Local Encrypted Vault (`ChaCha20-Poly1305` + `Argon2id`) | **`PASS`** | `vault::tests::*`, `studio::tests::*`, `workflow_memory::tests::*` |
| **Sec 2.2** | Central PostgreSQL 18 Control Plane Schema & Code Wiring | **`PASS`** | `crates/sf-server/src/pg_control_plane.rs`, `tools/verify-migrations.py` (12 migration pairs) |
| **Sec 2.2 / 18** | Live PostgreSQL 18 Multi-Instance Runtime Execution | **`BLOCKED BY EXTERNAL SETUP`** | Local machine has no running PostgreSQL 18 instance / `SF_TEST_DATABASE_URL`; wired in `.github/workflows/ci.yml` (`postgres:18.6-bookworm`) |
| **Sec 2.3 / 3.6** | Owner Identity Isolation (`/v1/owner/*` `403 SF-AUTH-013`), Web `sfw_*` vs Desktop `sfs_*`, RFC 6238 TOTP | **`PASS`** | `dual_database_owner_templates_e2e.rs`, `tools/verify-desktop-full-journey-real-e2e.mjs` |
| **Sec 3.1** | Public Showcase Free Browsing vs. Desktop Studio Runtime Boundary | **`PASS`** | `App.shell.test.tsx`, `ApplicationShell.test.tsx`, `tools/verify-runtime-boundary-e2e.mjs` |
| **Sec 3.2** | Email/Password Sign-Up, Sign-In, Email Verification, Password Reset, Duplicate Account Prevention, Min 8-Char Policy, Show/Hide Password | **`PASS`** | `AuthModal.test.tsx`, `PasswordResetModal.test.tsx`, `auth_flow.rs` |
| **Sec 3.2** | Live OAuth Provider Redirects (Google, Apple, GitHub) | **`BLOCKED BY EXTERNAL SETUP`** | UI/API routes & provider config present; live OAuth requires external OAuth client IDs/secrets & callback domains |
| **Sec 3.3** | Desktop Browser PKCE (`S256`), Random `state`, Single-Use Code, `safi://auth` Deep Link, Device Binding | **`PASS`** | `content::tests::validates_desktop_pkce_external_auth_urls_strictly`, `tools/verify-desktop-full-journey-real-e2e.mjs` |
| **Sec 3.4** | Single Active Desktop Device & Automatic Previous Device Revocation (`SF-AUTH-019`) | **`PASS`** | `security_online_authorization_e2e.rs`, `tools/verify-offline-and-device-revocation-e2e.mjs` |
| **Sec 3.5** | Offline Read-Only Mode for Previously Authenticated Users (`SF-AUTH-021`) & Fresh-Install Bypass Block (`SF-AUTH-001`) | **`PASS`** | `studio::tests::offline_readonly_*`, `studio::tests::fresh_unauthenticated_install_*`, `tools/verify-offline-and-device-revocation-e2e.mjs` |
| **Sec 4** | Single Source of Truth (`config/business.json`) & Server-Enforced Plan/Entitlement Limits | **`PASS`** | `node tools/validate-config.mjs`, `security_online_authorization_e2e.rs`, `SF-RCP-005` in `verify-desktop-full-journey-real-e2e.mjs` |
| **Sec 5.1–5.3** | Multi-Format Ingestion (`.xlsx`, `.xlsm`, `.xlsb`, `.xls`, `.ods`, `.csv`, `.tsv`), Multi-Sheet/Hidden Sheet/Multi-Table Profiling & Deterministic Cleaning + Undo/Redo | **`PASS`** | `crates/sf-io`, `studio_phase5_e2e.rs`, `tools/verify-studio-desktop-e2e.mjs` |
| **Sec 6.1–6.4** | Official Templates Immutability, `Save as New Recipe`, In-Place Edit/Duplicate/Apply, `.safitemplate` Exchange (`SF-RCP-005`), `Share with SafiData` + Owner Review/Publish | **`PASS`** | `recipe_library::tests::*`, `tools/verify-desktop-full-journey-real-e2e.mjs` |
| **Sec 7.1–7.4** | Persistent Per-Sheet Workflow Memory (Filename-Independent Semantic Matching, Domain Isolation, Auto-Replay, Version Snapshots & Restore) | **`PASS`** | `workflow_memory::tests::scenario_a_b_c_d_e_*`, `tools/verify-desktop-full-journey-real-e2e.mjs` |
| **Sec 8.1–8.4** | Continuous Local Learning (`PersonalLearningProfile`, 5-Domain Separation, Explainable Provenance, Undo/Reset, Longitudinal Effectiveness Metrics) | **`PASS`** | `workflow_memory::tests::*`, `DesktopWorkflowMemoryPanel.tsx`, `tools/verify-desktop-full-journey-real-e2e.mjs` |
| **Sec 9** | Embedded Local Full-Text Search (BM25, Arabic/English, Excel/CSV/Word/PPTX/TXT/HTML + Uncompressed & `FlateDecode` PDFs, Honest Non-OCR Notice, "Open in Studio") | **`PASS`** | `local_search::tests::*`, `tools/verify-desktop-full-journey-real-e2e.mjs` |
| **Sec 10 & 11** | Central Rust `Safi Decision Engine` (`crates/sf-desktop/src/decision_engine.rs`) with 7-Level Lexicographic Objective, Honest Optimality Classes, Content-Based ID Protection, Smart Duplicate Analysis, Cross-Product Consistency & UI Integration | **`PASS`** | `decision_engine::tests::*`, `DesktopDecisionEnginePanel.tsx`, `tools/verify-desktop-full-journey-real-e2e.mjs` |
| **Sec 12** | AI Architecture Separation (Deterministic Rust vs. Decision Engine vs. Local Ollama / Consented External AI + PII Masking + Plan Budgets) | **`PASS`** | `decision_engine.rs`, `api_origin::tests::validates_pinned_local_ollama_loopback_only`, `DesktopDecisionEnginePanel.tsx` |
| **Sec 13 & 14** | Complete Desktop Workspace Integration & 17-Area Bilingual Public Website Showcase (`BentoCommandCenter.tsx`) | **`PASS`** | `BentoCommandCenter.test.tsx`, `tools/verify-desktop-full-journey-real-e2e.mjs` |
| **Sec 15.1–15.3** | Intro-to-Hero `0ms` Crisp Headline Transition & Zero Dark-Flash Theme Stability (`vision-glass`, `apple-light`, `google-gemini`) | **`PASS`** | `node tools/verify-intro-transition-performance.mjs`, `node tools/verify-intro-theme-transition.mjs` |
| **Sec 16 & 19** | Desktop Build Hardening (CSP, Capabilities, 82-Command Allowlist, Deep-Link Guard, Ed25519 Update Verification) | **`PASS`** | `shell_config.rs`, `capabilities::tests::*`, `updater::tests::*`, `py -3 -B tools/verify-security.py` |
| **Sec 16.4 & 19** | Production Authenticode / Apple Code-Signing Certificates & Public HTTPS Installer Hosting (`available: false` until published) | **`BLOCKED BY EXTERNAL SETUP`** | `node tools/verify-published-installer-download.mjs` honestly reports `BLOCKED: NO PUBLISHED INSTALLER URL` |

---

## 4. What Already Existed at Baseline `739f56cf66beaa5147dbe9445c46a91e4c3dfc94`

1. Dual-database architecture (`StudioStore` + `VaultKey` locally; `sf-server` + `PgControlPlaneStore` centrally).
2. Owner-only `/v1/owner/*` RBAC (`403 SF-AUTH-013`), Web (`sfw_*`) vs. Desktop (`sfs_*`) session isolation, RFC 6238 TOTP MFA, Desktop Browser PKCE (`S256`), single-device replacement (`SF-AUTH-019`), and Offline Read-Only Mode (`SF-AUTH-021`).
3. Multi-format spreadsheet ingestion (`.xlsx`, `.xlsm`, `.xlsb`, `.xls`, `.ods`, `.csv`, `.tsv`), 21 chart types, and 9 deliverable export formats.
4. Official Templates, `Save as New Recipe`, `.safitemplate` export/import (`SF-RCP-005`), `Share with SafiData` community submissions, Local Full-Text Search (`local_search.rs`), Per-Sheet Workflow Memory, and Continuous Local Learning (`workflow_memory.rs`).
5. Website 17-area bilingual capability matrix (`BentoCommandCenter.tsx`) and the initial `0ms` post-intro headline crispness fix.

---

## 5. What Was Missing Before This Round

1. **Central Rust `Safi Decision Engine` (`crates/sf-desktop/src/decision_engine.rs`)**:
   - No unified Rust optimization engine shared across Spreadsheet Understanding, Data Cleaning, KPI Selection, Chart Selection, Dashboard Layout, and Executive Reports/Presentations.
   - No 7-level Lexicographic Objective Vector (`correctness > data_safety_no_loss > user_constraint_compliance > analytical_fit > explainability_audit > deliverable_quality > execution_efficiency`).
   - No formal distinction between `ExactProvenOptimum`, `BestValidCandidateFound`, `HeuristicRecommendation`, and `InsufficientEvidenceRequiresQuestion`.
   - No content-based protection preventing 100% numeric identifier columns (such as `Employee ID` with space-separated header) from being misclassified as summable `Measure`s in the decision pipeline.
   - No 3-way duplicate classification distinguishing `exact_unintended_duplicate_row_ids` from `valid_repeated_entity_transaction_rows` and `conflicting_key_review_row_ids`.
   - No Cross-Product Consistency & Lineage auditor (`ConsistencyImpactReport`) checking whether charts, slides, and document blocks reference stale or missing columns/metrics.
2. **Longitudinal Continuous Learning Effectiveness Metrics (Section 8.4)**:
   - `PersonalLearningProfile` stored rules and provenance, but did not compute or persist longitudinal effectiveness metrics (`questions_avoided_total`, `setup_steps_saved_total`, `workflows_replayed_total`, `explicit_corrections_recorded`, `repeated_corrections_count`, `undo_or_rollback_count`, `decision_accuracy_pct`).
3. **Frame-by-Frame Theme Transition Benchmark & Dark-Flash Fix (`tools/verify-intro-theme-transition.mjs` — Section 15.2)**:
   - After the intro exit wipe in Light Mode (`vision-glass` or `apple-light`), the page exhibited a dark `#090b10` (`rgb(9, 11, 16)`) background flash before/during transition.

---

## 6. Defects Discovered & 7. Root Cause of Each Defect

### Defect 1 — Post-Intro Dark Flash in Light Mode (Section 15.2)
- **Symptom**: When loading the site in Light Mode (`vision-glass` default `#eaf0fb` or `apple-light` `#f5f5f7`), `html` and `body` remained `#090b10` (`rgb(9, 11, 16)`) during the intro exit wipe and flashed dark before settling.
- **Root Cause (Proven by `tools/verify-intro-theme-transition.mjs` in `task-7735`)**:
  1. In `apps/web/public/boot.css`, `:root { color-scheme: dark; background: #090b10; }` used the `:root` pseudo-class selector (specificity `(0, 1, 0)`), whereas `apps/web/src/index.css` used `html { background-color: var(--bg-canvas); }` (specificity `(0, 0, 1)`). Because `(0, 1, 0) > (0, 0, 1)`, `:root { background: #090b10; }` in `boot.css` permanently won the CSS cascade over `html` in `index.css`!
  2. Additionally, `apps/web/src/index.css` had `html[data-safi-intro-active="true"], html[data-safi-intro-active="true"] body { background-color: #090b10; }` combined with `html { transition: background-color 0.35s ease; }`. Because `data-safi-intro-active="true"` remained present while `.safi-intro-splash` shrank its `clipPath` during the `0.48s` exit wipe, the revealed page background was forced to `#090b10` and only began a `0.35s` transition to `#eaf0fb` after the intro unmounted.
- **Fix**:
  - Removed `html[data-safi-intro-active="true"]` background override and `background-color 0.35s` root transition from `apps/web/src/index.css` (since `.safi-intro-splash` itself is a fixed opaque `#090b10` layer covering the viewport until the exit wipe opens).
  - Updated `:root, html` in `apps/web/src/index.css` and added theme-aware `:root[data-theme="vision-glass"]` / `:root[data-theme="apple-light"]` rules in `apps/web/public/boot.css`.
  - Verified with `node tools/verify-intro-theme-transition.mjs`: **`0` dark-flash frames and `0` mismatched background frames** across 302+ sampled frames per run.

### Defect 2 — Space-Separated Numeric Identifier Headers (`"Employee ID"`) in Content/Header Classification
- **Symptom**: `"Employee ID"` mapped to canonical concept `"hr_employee_identity"` and, when containing 100% numeric values (`1001, 1002, 1003`), fell through to `"Measure"` because the ID suffix check only checked `.ends_with("_id")`.
- **Root Cause**: `header_says_id` in `crates/sf-desktop/src/decision_engine.rs` checked `ends_with("_id")` but not space-separated `" id"`, `" code"`, `" no"`, or `header_concept == "hr_employee_identity" && numeric_parse_pct >= 80`.
- **Fix**: Expanded `header_says_id` in `crates/sf-desktop/src/decision_engine.rs` to check space- and underscore-separated ID/code/number tokens as well as numeric `hr_employee_identity` columns, protecting them from `SUM`/`AVG` aggregation.

---

## 8. Modified & Created Files and 9. Why Each Was Modified

| File | Action | Why Modified / Created |
| --- | --- | --- |
| [decision_engine.rs](file:///C:/Users/mo7am/OneDrive/Work/Safi/crates/sf-desktop/src/decision_engine.rs) | **Created** | Central Rust `SafiDecisionEngine` implementing the 10-step decision pipeline, 7-level Lexicographic Objective Vector, honest `OptimalityClass`, content-based column role solver, 3-way duplicate analyzer, Cross-Product Consistency auditor, and 8-task AI Routing table. |
| [workflow_memory.rs](file:///C:/Users/mo7am/OneDrive/Work/Safi/crates/sf-desktop/src/workflow_memory.rs) | **Modified** | Added `LearningEffectivenessMetrics` (Section 8.4) to `PersonalLearningProfile` and automatic metric updates on workflow execution, rollback, rule deletion, and repeated corrections. |
| [lib.rs](file:///C:/Users/mo7am/OneDrive/Work/Safi/crates/sf-desktop/src/lib.rs) | **Modified** | Exported `pub mod decision_engine;`. |
| [commands.rs](file:///C:/Users/mo7am/OneDrive/Work/Safi/crates/sf-desktop/src/commands.rs) | **Modified** | Added `desktop_decision_engine_evaluate` (`ReadOnlyOffline`) and `desktop_decision_engine_apply` (`ProtectedOnline`) to `ALLOWED_COMMANDS` (total 82 allowlisted commands). |
| [studio.rs](file:///C:/Users/mo7am/OneDrive/Work/Safi/crates/sf-desktop/src/studio.rs) | **Modified** | Implemented `StudioStore::desktop_decision_engine_evaluate` and `StudioStore::desktop_decision_engine_apply`. |
| [main.rs](file:///C:/Users/mo7am/OneDrive/Work/Safi/apps/desktop/src-tauri/src/main.rs) | **Modified** | Registered `desktop_decision_engine_evaluate` and `desktop_decision_engine_apply` in the Tauri IPC handler. |
| [studio_ipc_harness.rs](file:///C:/Users/mo7am/OneDrive/Work/Safi/crates/sf-desktop/examples/studio_ipc_harness.rs) | **Modified** | Wired `desktop_decision_engine_evaluate` and `desktop_decision_engine_apply` into the real Rust subprocess harness. |
| [studio-bridge.ts](file:///C:/Users/mo7am/OneDrive/Work/Safi/apps/web/src/desktop/studio-bridge.ts) | **Modified** | Added TypeScript contracts for `WorkspaceOptimalDecisionSuite`, `RecommendedDecision`, `LearningEffectivenessMetrics`, `evaluateDesktopDecisionEngine`, and `applyDesktopDecisionEngineAction`. |
| [DesktopDecisionEnginePanel.tsx](file:///C:/Users/mo7am/OneDrive/Work/Safi/apps/web/src/desktop/components/DesktopDecisionEnginePanel.tsx) | **Created** | Bilingual Desktop UI rendering one primary Recommended Decision per domain, Optimality Class badge, Evidence, Assumptions, 1-click Apply, 1-click Undo, collapsible `Advanced Details / Audit` drawer, Cross-Product Consistency banner, and AI Task Routing matrix. |
| [DesktopWorkflowMemoryPanel.tsx](file:///C:/Users/mo7am/OneDrive/Work/Safi/apps/web/src/desktop/components/DesktopWorkflowMemoryPanel.tsx) | **Modified** | Added longitudinal Continuous Learning Effectiveness Metrics strip (`data-learning-effectiveness-metrics="true"`). |
| [DesktopStudio.tsx](file:///C:/Users/mo7am/OneDrive/Work/Safi/apps/web/src/desktop/DesktopStudio.tsx) | **Modified** | Integrated `<DesktopDecisionEnginePanel />` and toolbar toggle button (`data-testid="toggle-decision-engine-panel"`). |
| [boot.css](file:///C:/Users/mo7am/OneDrive/Work/Safi/apps/web/public/boot.css) | **Modified** | Added theme-aware `:root[data-theme="vision-glass"]` and `:root[data-theme="apple-light"]` rules so `:root` specificity `(0, 1, 0)` never forces `#090b10` onto Light Mode after intro exit. |
| [index.css](file:///C:/Users/mo7am/OneDrive/Work/Safi/apps/web/src/index.css) | **Modified** | Removed `html[data-safi-intro-active="true"]` `#090b10` override and root `background-color 0.35s` lag; bound `:root, html, body` directly to `var(--bg-canvas)`. |
| [verify-intro-theme-transition.mjs](file:///C:/Users/mo7am/OneDrive/Work/Safi/tools/verify-intro-theme-transition.mjs) | **Created** | Automated frame-by-frame Chromium benchmark verifying `0` dark-flash frames across `vision-glass` (EN/AR), `apple-light`, and `google-gemini`. |
| [verify-desktop-full-journey-real-e2e.mjs](file:///C:/Users/mo7am/OneDrive/Work/Safi/tools/verify-desktop-full-journey-real-e2e.mjs) | **Modified** | Added step `[2C/6]` testing the real Rust `Safi Optimal Decision Engine` UI end-to-end. |

---

## 10. Final Engine Designs & Vertical Integration (Items 10–25)

### 11. Dual-Database Status (Section 2)
- **Local Vault (`crates/sf-desktop/src/vault.rs`)**: `Argon2id` key derivation + `ChaCha20-Poly1305` authenticated encryption for `*.safi.enc` project bundles, `local_search_index.safi.enc`, `recipe_library.safi.enc`, `workflow_memory.safi.enc`, and `learning_profile.safi.enc`.
- **Central Control Plane (`crates/sf-server`)**: PostgreSQL 18 schema (`migrations/0001..0012`) managing users, sessions, devices, subscriptions, entitlements, Official Templates, community submissions, releases, and audit logs. Zero customer spreadsheets or cell values ever enter PostgreSQL.

### 12. Auth & Owner Isolation (Section 3)
- Enforced at the Rust HTTP middleware & handler level (`crates/sf-server/src/owner_admin.rs`, `auth_control_plane.rs`). Non-owner tokens calling `/v1/owner/*` receive `403 SF-AUTH-013`. Owner Web (`sfw_*`) and Owner Desktop (`sfs_*`) sessions use independent token namespaces and independent RFC 6238 TOTP verification.

### 13. Subscriptions & Entitlements (Section 4)
- Single source of truth in [business.json](file:///C:/Users/mo7am/OneDrive/Work/Safi/config/business.json), validated by `node tools/validate-config.mjs`. Free (`5 ops/month, 5000 rows, 20 cols, no AI, no .safitemplate`), Trial (`7 days, verified email+phone, 25 AI calls`), Paid (`$20 / 1000 EGP monthly, $168 / 8400 EGP annual, 400 AI calls, .safitemplate enabled`), Friend/VIP (`0 price, owner-granted, 400 AI calls, no watermark, .safitemplate enabled`), Organization (`min 3 seats`), Pay-per-file (`$2 / 50 EGP, 100k rows`), Credit Pack (`25 ops, $5 / 250 EGP`).

### 14. Per-Sheet Workflow Memory (Section 7)
- Stored in `workflow_memory.safi.enc`. Matches sheets by semantic column concepts + domain (`sales`, `employees`, `inventory`, `finance`, `branch_operations`, `general`), independent of filename, sheet name, or column order. Supports `High (>= 80%)` auto-replay with question suppression, `Medium (55%..79%)` partial reuse with targeted structural-change questions, and non-destructive version history (`restore_version`, `duplicate`, `rename`, `delete`).

### 15. Continuous Local Learning (Section 8)
- Stored in `learning_profile.safi.enc` (`PersonalLearningProfile`), strictly separated from per-sheet workflow memory, workbook context, project bundles, and recipes. Tracks explicit corrections, accepted/rejected recommendations, and preferred rules scoped to `this_sheet_only` or `matching_sheets`, with bilingual provenance, single-rule undo/delete, full profile reset, and longitudinal `LearningEffectivenessMetrics`.

### 16. Recipe Library (Section 6)
- Stored in `recipe_library.safi.enc`. Official Templates are immutable locally; users customize via `Save as New Recipe`, edit/reorder/remove steps in place (`desktop_recipes_update_existing`), duplicate (`desktop_recipes_duplicate`), apply directly to the active Studio sheet (`desktop_recipes_apply_to_project`), exchange BLAKE3-verified `.safitemplate` files (`SF-RCP-005` enforced for Free/Trial), and submit sanitized declarative recipes via `Share with SafiData`.

### 17. Local Full-Text Search (Section 9)
- Embedded Rust BM25 search engine (`crates/sf-desktop/src/local_search.rs`) indexing user-selected folders across `.xlsx`, `.csv`, `.docx`, `.pptx`, `.txt`, `.html`, and extractable uncompressed + `FlateDecode`-compressed `.pdf` streams (honestly flagging image-only scanned PDFs without OCR), with project-ID filtering and one-click **"Open in Studio"**.

### 18. Optimal Decision Engine (Sections 10 & 11)
- Central Rust `SafiDecisionEngine` (`crates/sf-desktop/src/decision_engine.rs`) evaluating candidates across 6 product domains using a 7-level Lexicographic Objective Vector (`correctness`, `data_safety_no_loss`, `user_constraint_compliance`, `analytical_fit`, `explainability_audit`, `deliverable_quality`, `execution_efficiency`). Returns one primary `RecommendedDecision` per domain with honest `OptimalityClass` disclosure, empirical evidence, assumptions, 1-click Apply/Undo, collapsible `Advanced Details / Audit` of pruned candidates, and `ConsistencyImpactReport`.

### 19. AI Architecture (Section 12)
- Strict separation between the Rust Deterministic Engine (100% of numeric calculations & cleaning), `SafiDecisionEngine` (lexicographic optimization & perceptual chart guards), Local Learning Engine, and Local Ollama (`http://127.0.0.1:11434`) / Optional Consented External AI (language understanding & token-grounded narrative only, with mandatory user consent + local PII masking).

### 20. Data Cleaning & Analytics & 21. Dashboards & Deliverables (Sections 5, 11, 14)
- Full 21 chart types aggregated in Rust, reactive What-If scenarios, and 9 verified output formats (`.xlsx`, `.csv`, `.pptx`, `.docx`, `.pdf`, `.png`, `.svg`, `.safi.html`, Power BI package) bound to `VerifiedMetricToken`s.

### 22. Theme & Intro Performance (Section 15)
- **Hero Headline Crisp Delay After Intro Exit (`tools/verify-intro-transition-performance.mjs`)**: **`0 ms`** across all 5 desktop viewports/locales (`en-1440x900`, `ar-1440x900`, `en-1920x1080`, `ar-1280x800`, `en-reduced-motion`), with `initialMinWordOpacityAtIntroExit: 1` and `initialWordFilterAtIntroExit: "none"`.
- **Frame-by-Frame Theme Transition Stability (`tools/verify-intro-theme-transition.mjs`)**:
  - `default-light-vision-glass-en`: `darkFlashFramesDuringOrAfterExit: 0`, `mismatchedBgFramesDuringOrAfterExit: 0` (`rgb(234, 240, 251)`, `color-scheme: light`)
  - `default-light-vision-glass-ar`: `darkFlashFramesDuringOrAfterExit: 0`, `mismatchedBgFramesDuringOrAfterExit: 0` (`rgb(234, 240, 251)`, `color-scheme: light`)
  - `legacy-light-apple-light-en`: `darkFlashFramesDuringOrAfterExit: 0`, `mismatchedBgFramesDuringOrAfterExit: 0` (`rgb(245, 245, 247)`, `color-scheme: light`)
  - `saved-dark-google-gemini-en`: `darkFlashFramesDuringOrAfterExit: 0`, `mismatchedBgFramesDuringOrAfterExit: 0` (`rgb(9, 11, 16)`, `color-scheme: dark`)

### 23. Security & Anti-Cloning & 24. Local Encryption (Sections 16 & 17)
- **Public Repository Visibility Risk Disclosure**: The GitHub repository `mo7amedrabei14-cell/Safi` is currently Public. Per Rule 0.5 & Section 16, the agent has NOT modified repository visibility. **Action Required by Owner**: If you wish to keep source code proprietary, switch the GitHub repository visibility to **Private** in GitHub Settings and rotate any historical credentials/secrets.
- **Technical Protections Active**: Release builds enable LTO + symbol stripping, strict CSP (`no unsafe-inline`, `no unsafe-eval`), 4 minimal Tauri permissions, 82-command IPC allowlist with 1 MiB serialized JSON ceiling (`SF-SYS-007`), Ed25519 signed update manifest verification with anti-downgrade protection, atomic temp-file -> `fsync` -> rename writes, and `ChaCha20-Poly1305` local vault encryption.

### 25. PostgreSQL 18, 26. CI Results & 27. Installer/Release Status (Sections 18 & 19)
- **PostgreSQL 18**: `PgControlPlaneStore` is wired into `crates/sf-server/src/main.rs` with startup hydration (`load_hub_store`) and fail-closed `SF-DB-001` error handling. Live local execution is classified as `BLOCKED BY EXTERNAL SETUP` because no PostgreSQL 18 process is installed on this local Windows machine; CI runs migrations `0001..0012` against `postgres:18.6-bookworm`.
- **Installer Status**: `apps/web/public/downloads/release-manifest.json` and `apps/web/public/releases/desktop-release-manifest.json` honestly keep `available: false` until a real code-signed installer binary is uploaded to public HTTPS storage (`BLOCKED BY EXTERNAL SETUP`).

---

## 28. Performance Benchmarks & 29. Data Integrity Results

- **Local Search Scale Benchmark (`local_search::tests::scale_benchmark_100_and_1000_local_files_with_incremental_skip`)**: Indexes 100 and 1,000 local files with sub-second incremental unchanged-file skipping and BM25 query latency `< 15 ms`.
- **Studio Multi-Size Dataset Benchmark (`studio_phase5_e2e::studio_multi_size_benchmarks_and_sample_capsule`)**: Bounded pagination (`<= 200` rows per IPC page) keeps React state compact while Rust processes 10k+ rows deterministically.
- **Atomic Persistence & Integrity**: Every project bundle, `.safitemplate`, workflow memory store, and deliverable export writes to a sibling `.tmp` file, calls `sync_all()` (`fsync`), and atomically renames, verifying `BLAKE3` checksums on load (`SF-CAL-008` on tamper).

---

## 30. Manual Test Instructions (PowerShell Commands)

```powershell
# 1. Verify Rust Decision Engine, Workflow Memory (Scenarios A-E), Local Search & Studio tests
cargo test -p sf-desktop --all-targets

# 2. Verify Frame-by-Frame Theme Transition Stability (Zero Dark Flash)
$env:PLAYWRIGHT_MODULE_URL = "file:///" + (($env:TEMP -replace '\\','/') + "/safi-playwright/node_modules/playwright/index.mjs")
node tools/verify-intro-theme-transition.mjs

# 3. Verify Intro-to-Hero 0ms Headline Crispness Benchmark across 5 Viewports/Locales
node tools/verify-intro-transition-performance.mjs

# 4. Verify Full Unmocked Desktop + Server + Owner Portal E2E Journey (including Decision Engine & Workflow Memory)
node tools/verify-desktop-full-journey-real-e2e.mjs
```
