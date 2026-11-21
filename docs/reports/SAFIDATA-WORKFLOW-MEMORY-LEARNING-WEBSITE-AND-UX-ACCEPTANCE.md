# SafiData — Workflow Memory, Continuous Local Learning, Website Showcase & UX Performance Acceptance Report

- **Project:** SafiData / صافي البيانات (`mo7amedrabei14-cell/Safi`)
- **Branch:** `main` ONLY
- **Baseline Commit (`HEAD` / `origin/main`):** `8474e919d5b7c62af652acadb0e454e5bc14c622`
- **Execution Mode:** Local Development & Unmocked Full-Stack Verification (`git commit` / `git push` / production deployment strictly left to the Owner)

---

## 1. Executive Summary

This round completes the remaining product, intelligence, website showcase, and UX transition capabilities on top of the `8474e919d5b7c62af652acadb0e454e5bc14c622` baseline while preserving every established architectural boundary:

1. **Persistent Per-Sheet Workflow Memory (`crates/sf-desktop/src/workflow_memory.rs` + `DesktopWorkflowMemoryPanel.tsx` + `DesktopStudio.tsx`)**:
   - Persists complete per-sheet / per-table workflow context locally and encrypted (`ChaCha20-Poly1305` in `workflow_memory.safi.enc`), including semantic & structural column signatures, business domain classification (`sales`, `employees`, `inventory`, `finance`, `branch_operations`, `general`), clarification question answers/skips/deferrals/custom explanations keyed by rename-invariant `(semantic_topic, canonical_concept)`, column role/type/date/currency/identifier decisions, duplicate & missing-value policies, business rules, ordered deterministic cleaning steps, verified metric formulas, dashboard preferences, decision history, and non-destructive version snapshots (`restore_version`, `duplicate`, `rename`, `delete`).
   - Recognizes sheets by semantic column signature even when the workbook filename changes, the worksheet tab is renamed, columns are reordered, headers switch between Arabic and English aliases, or new monthly rows arrive.
   - Enforces strict per-sheet isolation inside multi-sheet workbooks (`Sales` workflow never leaks into `Employees` or `Inventory`) and suppresses already-answered questions on High-confidence (`>= 80%`) matches while asking only about new or structurally changed elements on Medium-confidence (`55%..79%`) matches.
2. **Continuous Local Learning & Explainable Personal Profile (`PersonalLearningProfile` in `learning_profile.safi.enc`)**:
   - Learns from explicit user feedback (`explicit_correction`, `preferred_rule`, `explicit_acceptance`, `explicit_rejection`, `repeated_choice`) across column role classification, date format, currency/unit, duplicate handling, missing-value treatment, metric formulas, chart layout, export format, and transformation rules, scoped to `this_sheet_only` or `matching_sheets`.
   - Generates explainable suggestions citing `influencingRuleId` and bilingual provenance (`whyEn` / `whyAr`), supports single-click undo/delete of individual learned rules or full profile reset, and remains inspectable in Offline Read-Only Mode.
3. **Complete Recipe Library & Local Search UI Integration (`DesktopRecipeLibraryPanel.tsx` + `DesktopLocalSearchPanel.tsx`)**:
   - Added in-place editing of user-owned recipes (`desktop_recipes_update_existing`), step reordering/removal, recipe duplication (`desktop_recipes_duplicate`), direct application of any Official Template or personal recipe to the active Studio sheet (`desktop_recipes_apply_to_project`), project-ID filtering in Local Search, and one-click **"Open in Studio"** on spreadsheet search hits.
4. **Public Website Complete 17-Area Bilingual Product Showcase (`BentoCommandCenter.tsx`)**:
   - Preserved 100% of the existing visual identity, animations, color tokens, and typography while adding the bilingual Arabic (RTL) and English (LTR) **Verified Product Capabilities & Architecture Matrix** (`#verified-capabilities-matrix`) covering all 17 required discovery areas and the 7 input / 9 output formats strip.
5. **Intro-to-Main-Interface 1–2s Headline Transition Lag Eliminated (`0ms` Post-Intro Crisp Delay)**:
   - Profiled in headless Chromium (`tools/verify-intro-transition-performance.mjs`), identified the three compounding root causes (`ApplicationShell` lazy mount gated on `introMounted === false`, initial-mount stage dim `opacity: 0.35`, and `BlurWordText` `opacity: 0.2 + filter: blur(6px)` 1,005ms stagger), and fixed them so the hero headline is 100% crisp (`opacity: 1`, `filter: none`, `0ms` delay) the instant the intro exit wipe completes across all 5 desktop viewports/locales.

---

## 2. Reconciled Architecture Matrix

| Architectural Boundary | Enforcement Mechanism | Verified Behavior |
| --- | --- | --- |
| **Customer Local Encrypted Database vs. Central Control Plane** | `crates/sf-desktop/src/vault.rs` (`ChaCha20-Poly1305` + `Argon2id`) vs. `crates/sf-server` (`PostgreSQL 18` + `AuthControlPlaneStore`) | Customer files, spreadsheets, cleaned outputs, local search index (`local_search_index.safi.enc`), recipes (`recipe_library.safi.enc`), per-sheet workflow memory (`workflow_memory.safi.enc`), and personal learning profile (`learning_profile.safi.enc`) remain 100% on-device. |
| **Zero Network in Engine Crates** | `tools/check-no-network.sh` | All `crates/sf-*` engine crates have zero network dependencies; only `sf-server` and the Tauri auth/update boundary perform HTTPS calls. |
| **Owner Identity & Surface Isolation** | `crates/sf-server/src/owner_admin.rs` + `crates/sf-server/src/auth_control_plane.rs` | Exactly one Owner account; non-owner requests to `/v1/owner/*` return `403 SF-AUTH-013`. Owner Web sessions (`sfw_*` at `/owner`) and Owner Desktop sessions (`sfs_*`) are strictly isolated with independent RFC 6238 TOTP MFA and separate revocation. |
| **Desktop PKCE + Single-Device Exclusivity** | `POST /v1/auth/desktop/pkce/{start,authorize,exchange}` + `StudioStore::gate_command` | Browser PKCE (`S256`) binds session token `sfs_*` to `installation_id` + Ed25519 device keypair. Logging in on Device B revokes Device A (`401 SF-AUTH-019`). |
| **Online Authorization & Offline Read-Only Mode** | `StudioStore::gate_command` + `enter_offline_read_only_mode` | Protected mutations require online authorization. Previously authenticated installs with a sealed local lease can enter Offline Read-Only Mode (`SF-AUTH-021` on mutations); fresh unauthenticated installs cannot bypass login (`SF-AUTH-001`). |

---

## 3. Source-Based Gap Matrix (6 Categories)

### Category 1 — Already Implemented and Verified (Prior Baseline `8474e91`)
- Dual-database separation, Owner-only `/v1/owner/*` RBAC (`SF-AUTH-013`), Owner Web (`sfw_*`) vs. Desktop (`sfs_*`) isolation, and RFC 6238 TOTP MFA.
- Desktop Browser PKCE (`S256`) flow, single-device exclusivity (`SF-AUTH-019`), and Offline Read-Only Mode (`SF-AUTH-021`).
- PostgreSQL 18 runtime wiring (`PgControlPlaneStore`) with startup hydration (`load_hub_store`) and fail-closed `SF-DB-001` error propagation.
- Multi-format spreadsheet ingestion (`.xlsx`, `.xlsm`, `.xlsb`, `.xls`, `.ods`, `.csv`, `.tsv`), 21 chart types, and 9 deliverable export formats (`.xlsx`, `.csv`, `.pptx`, `.docx`, `.pdf`, `.png`, `.svg`, `.safi.html`, Power BI package).
- Embedded local full-text search (`crates/sf-desktop/src/local_search.rs`) including uncompressed and `FlateDecode`-compressed PDF text streams and honest non-OCR scanned-PDF flagging.

### Category 2 — Implemented in Backend/Rust but Not Fully Integrated in UI (Remediated in This Round)
- **Personal Recipe Editing, Duplication & Direct Sheet Application**: `LocalRecipeLibrary::update_my_recipe` and `duplicate_my_recipe` existed in Rust `recipe_library.rs` but were not exposed via Tauri IPC or `DesktopRecipeLibraryPanel.tsx`. **Fixed**: Added `desktop_recipes_update_existing`, `desktop_recipes_duplicate`, and `desktop_recipes_apply_to_project` across `commands.rs`, `studio.rs`, `src-tauri/src/main.rs`, `studio_ipc_harness.rs`, `studio-bridge.ts`, and `DesktopRecipeLibraryPanel.tsx`.
- **Local Search Project Filter & "Open in Studio" Action**: `SearchQuery::project_id` existed in Rust `local_search.rs` but was not wired into `DesktopLocalSearchPanel.tsx`, and search hits could not be opened directly in Studio. **Fixed**: Added `projectIdFilter` input and `onOpenFileInStudio` (`[data-search-open-in-studio="true"]`) in `DesktopLocalSearchPanel.tsx` and `DesktopStudio.tsx`.

### Category 3 — Implemented Earlier but Defective (Remediated in This Round)
- **1–2 Second Post-Intro Hero Headline Transition Lag**: `App.tsx` waited until `introMounted === false` before mounting `<Suspense><ApplicationShell /></Suspense>`, and post-paint `useEffect` hooks in `ApplicationShell.tsx` (`opacity: 0.35`), `BlurWordText.tsx` (`opacity: 0.2, filter: blur(6px)`, 1,005ms stagger), and `BentoCommandCenter.tsx` (`opacity: 0.25`) caused a visible 627ms–705ms blurred/dimmed headline lag after intro exit. **Fixed and verified at `0ms` delay across 5 viewports/locales**.
- **Recipe Step Schema Mismatch in `DesktopRecipeLibraryPanel.tsx`**: `DEFAULT_CUSTOM_STEPS` previously used a nested object for `op` that did not match `sf_recipe::recipe::RecipeStep` when creating standalone recipes. **Fixed** both in `DesktopRecipeLibraryPanel.tsx` (canonical `RecipeStep` format) and in `crates/sf-desktop/src/studio.rs` (`parse_recipe_steps_flexible`).

### Category 4 — Not Implemented Prior to This Round (Implemented & Verified Now)
- **Persistent Per-Sheet Workflow Memory (`crates/sf-desktop/src/workflow_memory.rs` + `DesktopWorkflowMemoryPanel.tsx`)**: Filename-independent semantic sheet recognition, per-sheet isolation in multi-sheet workbooks, confidence-tiered auto-replay (`High >= 80%`) vs. partial reuse (`Medium 55%..79%`), suppression of already-answered clarification questions, and version snapshots (`restore_version`, `duplicate`, `rename`, `delete`).
- **Continuous Local Learning & Explainable Personal Profile (`PersonalLearningProfile`)**: Explicit correction/preference learning (`this_sheet_only` / `matching_sheets`), explainable suggestions citing `influencingRuleId` and provenance, and single-rule undo/delete or profile reset.
- **Public Website Complete 17-Area Bilingual Showcase Matrix (`BentoCommandCenter.tsx`)**: Added `#verified-capabilities-matrix` and `#verified-io-formats-strip` in Arabic and English.

### Category 5 — Blocked by External Manual Setup (Honest Classification)
- **Live PostgreSQL 18 Execution in This Local Environment**: Local machine does not have `psql`/`pg_isready` or `DATABASE_URL` configured (`cargo test -p sf-server --test postgres_control_plane_roundtrip` cleanly skips when `DATABASE_URL` is unset).
- **Public Signed HTTPS Desktop Installer Binary (`available: false`)**: `apps/web/public/releases/desktop-release-manifest.json` honestly keeps `available: false` until a real code-signed installer binary is uploaded to public HTTPS CDN/Release storage.
- **Production Secrets & External Webhooks**: Production SMTP, Paymob/Lemon Squeezy live webhooks, and OS code-signing certificates remain external deployment steps.

### Category 6 — Recommended Future Capabilities Out of Current Scope
- Optional OCR plugin for image-only scanned PDFs (currently flagged honestly without OCR).
- Mobile companion apps (deferred to 1.1/1.2 per `AGENTS.md`).

---

## 4. Dual-Database and Owner Isolation Verification

- **Customer Local Storage**: All user spreadsheets, project bundles (`*.safi.enc`), local search index (`local_search_index.safi.enc`), recipes (`recipe_library.safi.enc`), per-sheet workflow memories (`workflow_memory.safi.enc`), and personal learning profile (`learning_profile.safi.enc`) are stored strictly in the customer's local encrypted vault directory.
- **Central Control Plane**: Only account identity, workspace membership, subscription state, device leases, Official Templates metadata, and explicitly consented `Share with SafiData` declarative recipe submissions touch `sf-server`.
- **Zero Sensitive Data Leakage (`AGENTS.md` Rule 4)**: Verified via unit tests and `py -3 -B tools/verify-security.py` that no cell values, filenames, sheet names, or column names are emitted in logs or telemetry.

---

## 5. Desktop Authentication, Single-Device Exclusivity, and Offline Read-Only Verification

- **PKCE Browser Login**: Verified in `tools/verify-desktop-full-journey-real-e2e.mjs` and `tools/verify-studio-desktop-e2e.mjs` against real `sf-server` (`/v1/auth/desktop/pkce/start` -> `/authorize` -> `/exchange`).
- **Single-Device Exclusivity (`SF-AUTH-019`)**: Verified in `tools/verify-offline-and-device-revocation-e2e.mjs` and `studio::tests::native_auth_gate_enforces_session_device_expiry_revocation_logout_and_ipc_limits`.
- **Offline Read-Only Mode (`SF-AUTH-021`)**: Verified in `tools/verify-offline-and-device-revocation-e2e.mjs`, `studio::tests::offline_readonly_preserves_saved_projects_blocks_protected_ops_and_revalidates_on_reconnect`, and `workflow_memory::tests::scenario_a_b_c_d_e_workflow_memory_and_continuous_learning_end_to_end` (Scenario E).

---

## 6. Official Templates, Personal Recipes, `.safitemplate`, and Community Contribution Verification

- **Official Templates Immutability & `Save as New Recipe`**: Synced from `GET /v1/templates/official`, stored read-only locally, and customized via `Save as New Recipe` (`desktop_recipes_save_as_new`) without mutating the official template.
- **In-Place Personal Recipe Editing, Duplication & Sheet Application**: Verified via `desktop_recipes_update_existing`, `desktop_recipes_duplicate`, and `desktop_recipes_apply_to_project`.
- **Server-Enforced `.safitemplate` Entitlement (`SF-RCP-005`)**: Free/Trial accounts receive `403 SF-RCP-005` from `POST /v1/templates/entitlement/export-import`; Paid/VIP/Owner accounts export and importBLAKE3-verified `.safitemplate` bundles.
- **`Share with SafiData` & Owner Review/Publication**: Sanitized declarative recipe preview (zero data rows), PII/secret scan, mandatory consent (`2026-11-v1`), submission to `POST /v1/community/submissions`, and Owner TOTP-verified approval and publication to `GET /v1/templates/official`.

---

## 7. Persistent Per-Sheet Workflow Memory Design and Verification (Section 5)

- **Implementation**: [workflow_memory.rs](file:///C:/Users/mo7am/OneDrive/Work/Safi/crates/sf-desktop/src/workflow_memory.rs), [studio.rs](file:///C:/Users/mo7am/OneDrive/Work/Safi/crates/sf-desktop/src/studio.rs), [DesktopWorkflowMemoryPanel.tsx](file:///C:/Users/mo7am/OneDrive/Work/Safi/apps/web/src/desktop/components/DesktopWorkflowMemoryPanel.tsx), and [DesktopStudio.tsx](file:///C:/Users/mo7am/OneDrive/Work/Safi/apps/web/src/desktop/DesktopStudio.tsx).
- **What Is Persisted Per Sheet**:
  1. `SheetSemanticSignature`: business domain (`sales`, `employees`, `inventory`, `finance`, `branch_operations`, `general`), bilingual normalized header descriptors, canonical concepts (`temporal_date`, `sales_currency_amount`, `dimension_branch_region`, `record_identifier`, `hr_employee_identity`, `hr_salary_amount`, `inventory_sku_code`, `inventory_stock_qty`, etc.), analytical roles, and structural BLAKE3 hash.
  2. `SavedQuestionDecision`: question ID, rename-invariant `semantic_topic` (`date_granularity`, `currency_unit`, `primary_kpi`, `id_handling`, `column_role`), `canonical_concept`, `action` (`answered`, `skipped`, `deferred`, `custom`, `corrected`), `answer`, `custom_text`, and timestamp.
  3. `SavedColumnDecision`: confirmed role, semantic type, date format, currency/unit, and identifier flag.
  4. `duplicate_policy`, `missing_value_policy`, and custom `business_rules`.
  5. Ordered deterministic `cleaning_operations` (`trim_column`, `rename_column`, `dsl_command`, etc.) with `target_concept` fallback so renamed headers map automatically.
  6. Verified `metrics` (`SavedMetricDefinition`) with automatic formula column translation across renamed headers (e.g., `SUM(amount)` -> `SUM(المبيعات)`).
  7. `dashboard_preferences`, `recommendation_feedback`, `decision_history`, and immutable `versions` snapshots (`SheetWorkflowVersionSnapshot`) supporting non-destructive rollback (`restore_version`), `duplicate`, `rename`, and `delete`.

---

## 8. Continuous Local Learning Design, Scope Rules, Explainability, and Reset/Undo Controls (Section 6)

- **Storage Separation**: `PersonalLearningProfile` is stored in `learning_profile.safi.enc`, strictly separate from `workflow_memory.safi.enc`, `recipe_library.safi.enc`, `local_search_index.safi.enc`, and project bundles.
- **Signals & Categories**: Captures `explicit_correction`, `preferred_rule`, `explicit_acceptance`, `explicit_rejection`, and `repeated_choice` across 9 categories (`column_classification`, `date_format`, `currency_unit`, `duplicate_handling`, `missing_value_treatment`, `metric_formula`, `chart_layout`, `export_format`, `transformation_rule`).
- **Scope Rules**: Each rule is scoped to `this_sheet_only` (bound to the sheet's structural hash) or `matching_sheets` (scoped to the business domain + canonical concept).
- **Explainability & Undo**: Every generated `ExplainableLearnedSuggestion` includes `influencingRuleId`, `suggestedValue`, `avoidedValue`, `confidencePct`, `whyEn`, and `whyAr`. Users can delete/undo any individual rule (`desktop_learning_delete_rule`) or reset the entire profile (`desktop_learning_reset_profile`).

---

## 9. Multi-Sheet Isolation and Filename-Independent Sheet Recognition Proof (Scenarios A–E)

Verified in `workflow_memory::tests::scenario_a_b_c_d_e_workflow_memory_and_continuous_learning_end_to_end` ([workflow_memory.rs](file:///C:/Users/mo7am/OneDrive/Work/Safi/crates/sf-desktop/src/workflow_memory.rs#L1940-L2248)) and Playwright E2E (`tools/verify-desktop-full-journey-real-e2e.mjs`):

| Scenario | Test Input & Mutation | Expected & Verified Result | Status |
| --- | --- | --- | --- |
| **Scenario A — Monthly Sales Workbook** | Save workflow on `jan_sales.xlsx` (`Sales_Jan`, headers `["order_id", "order_date", "region", "customer", "amount", "quantity"]`) with answered date/currency questions, trim step, and `SUM(amount)` metric. Open `feb_sales_2026_final.xlsx` with renamed sheet `فبراير_مبيعات` and reordered Arabic/English headers `["المنطقة", "order_date", "المبيعات", "order_id", "العميل", "الكمية"]`. | Recognized with **High confidence (`>= 80%`)**, `auto_replay_eligible = true`, maps `"amount" -> "المبيعات"` and `"region" -> "المنطقة"`, suppresses already-answered `date_granularity` & `currency_unit` questions, and replays cleaning + metric `SUM(المبيعات)`. | **PASS** |
| **Scenario B — Multi-Sheet Isolation** | In the same workbook, switch to `Employees` (`["emp_id", "employee_name", "department", "hire_date", "salary"]`) and `Inventory` (`["sku_code", "warehouse", "stock_qty", "reorder_level"]`). | `Employees` classified as `SheetBusinessDomain::Employees` and `Inventory` as `SheetBusinessDomain::Inventory`; neither matches the `Sales` workflow (`matched_workflow_id == None`). Saving an `Employees` workflow keeps `Sales` and `Employees` 100% isolated. | **PASS** |
| **Scenario C — Structural Change** | Open `march_sales.xlsx` where a new column `"discount_pct"` is added and `observed_date_format` shifts from `YYYY-MM-DD` to `DD/MM/YYYY`. | Classified as **`WorkflowMatchTier::Medium`**, `added_columns = ["discount_pct"]`, emits schema change notices, reuses `currency_unit` answer, and **unsuppresses** `date_granularity` so only the changed date format & new column are asked. | **PASS** |
| **Scenario D — User Correction & Undo** | User corrects `order_id` from `Measure (Sum)` to `Identifier` (`explicit_correction`, scope `matching_sheets`). Re-match sales sheet, then undo/delete the rule (`delete_learning_rule`), and test `restore_workflow_version(..., 1)`. | Learned suggestion recommends `Identifier` for `order_id` (avoiding `Measure (Sum)`) with bilingual provenance citing `rule_id`. Deleting the rule immediately removes the suggestion; restoring `v1` creates non-destructive `v3` with `v1` state. | **PASS** |
| **Scenario E — Offline Read-Only Mode** | Reopen `LocalWorkflowMemoryStore` from encrypted disk after restart and enter Offline Read-Only Mode (`enter_offline_read_only_mode`). | `desktop_workflow_memory_list`, `desktop_workflow_memory_match_sheet`, and `desktop_learning_profile_get` succeed offline; mutating commands (`desktop_workflow_memory_save_sheet`, `desktop_learning_record_feedback`) are rejected with `SF-AUTH-021`. | **PASS** |

---

## 10. Local Full-Text Search Verification

- Verified across CSV, Excel (`.xlsx`), Word (`.docx`), PowerPoint (`.pptx`), TXT, HTML, and extractable uncompressed + `FlateDecode`-compressed PDFs in `local_search::tests::indexes_and_searches_all_formats_arabic_english_phrase_prefix_fuzzy_and_encrypted_persistence`, `local_search::tests::extracts_flatedecode_compressed_pdf_text_and_flags_scanned_pdf_honestly`, `local_search::tests::scale_benchmark_100_and_1000_local_files_with_incremental_skip`, and Playwright E2E `tools/verify-desktop-full-journey-real-e2e.mjs` (including clicking **"Open in Studio"** on `مبيعات_القاهرة.csv`).

---

## 11. Website Complete Product Showcase Content Verification (Section 9)

- **File**: [BentoCommandCenter.tsx](file:///C:/Users/mo7am/OneDrive/Work/Safi/apps/web/src/components/home/BentoCommandCenter.tsx#L2274-L2478)
- **Visual Design Preservation**: Preserved 100% of existing visual style, `SpotlightCard` components, glass badges, color tokens, typography, and layout hierarchy.
- **Complete 17 Discovery Areas Covered in Arabic (RTL) & English (LTR)**:
  - Areas 1–3 (`[data-capability-card="ingestion-multisheet"]`): Product overview, `.xlsx/.xlsm/.xlsb/.xls/.ods/.csv/.tsv` import, multi-sheet & multi-table detection, macro/formula neutralization.
  - Areas 4–5 (`[data-capability-card="cleaning-recipes"]`): Deterministic cleaning, Official Templates, `Save as New Recipe`, `.safitemplate` exchange, `Share with SafiData` community contributions.
  - Areas 6–7 (`[data-capability-card="workflow-memory-learning"]`): Filename-independent per-sheet workflow memory, continuous local learning profile, contextual clarification Q&A, 5-tier epistemic verification.
  - Areas 8–11 (`[data-capability-card="dashboards-charts-deliverables"]`): Dashboard Builder, 21 chart types, Keynote `.pptx` with presenter notes, Word `.docx`, Arabic vector `.pdf`, interactive `.safi.html` capsules, Power BI package.
  - Areas 12–13 (`[data-capability-card="local-search-projects"]`): Embedded BM25 local full-text search (including `FlateDecode` PDFs and honest non-OCR scanned-PDF notice) + `ChaCha20-Poly1305` encrypted project vault.
  - Areas 14–17 (`[data-capability-card="privacy-auth-formats"]` + `[data-verified-io-formats-strip="true"]` + `#download`): 100% local data processing, online account authorization with single-device exclusivity, Offline Read-Only Mode, optional explicit-consent AI, 7 supported input formats, 9 verified output formats, and honest Desktop installer state.

---

## 12. Intro-to-Main-Interface Performance Diagnosis & Before/After Measurements (Section 10)

### Root Causes Identified via Browser Profiling (`tools/verify-intro-transition-performance.mjs`)
1. **Deferred `ApplicationShell` Mount (`apps/web/src/App.tsx`)**: Previously, `App.tsx` passed `introComplete={!introMounted}` to `ApplicationShell` and started lazy-loading `ApplicationShell` only when `<Suspense>` mounted, keeping the shell unmounted until the intro exit wipe finished.
2. **Initial-Mount Stage Dimming (`apps/web/src/ApplicationShell.tsx`)**: An initial `useEffect` in `ApplicationShell.tsx` ran `gsap.fromTo(el, { opacity: 0.35, y: 14 }, { opacity: 1, y: 0, duration: 0.42 })` on first mount, dimming the entire landing page to `35%` opacity right as the intro curtain lifted.
3. **Heavy Software CSS Blur Stagger on 52px Bilingual Headline (`apps/web/src/components/ui/BlurWordText.tsx`)**: A post-paint `useEffect` snapped all 13 hero headline words to `opacity: 0.2, filter: blur(6px), y: 14` and animated them over `duration: 0.55s` + `stagger: 0.035s` (`~1,005ms`), compounding with `ApplicationShell`'s `0.35` opacity (`0.35 × 0.20 = 0.07` effective opacity) for `627ms–705ms` after intro exit (`1.3s–2.2s` after logo animation completion).

### Exact BEFORE vs. AFTER Measurements (Headless Chromium — `tools/verify-intro-transition-performance.mjs`)

| Viewport & Locale Case | Metric | BEFORE Fix | AFTER Fix | Improvement |
| --- | --- | --- | --- | --- |
| **`en-1440x900`** (Desktop LTR) | `headlineCrispDelayAfterIntroExitMs` | `627 ms` (`opacity: 0.2`, `blur(6px)`) | **`0 ms`** (`opacity: 1`, `filter: none`) | **`-627 ms` (100% instant crisp headline)** |
| **`ar-1440x900`** (Desktop RTL) | `headlineCrispDelayAfterIntroExitMs` | `699 ms` (`opacity: 0.2`, `blur(6px)`) | **`0 ms`** (`opacity: 1`, `filter: none`) | **`-699 ms` (100% instant crisp headline)** |
| **`en-1920x1080`** (Full HD LTR) | `headlineCrispDelayAfterIntroExitMs` | `705 ms` (`opacity: 0.2`, `blur(6px)`) | **`0 ms`** (`opacity: 1`, `filter: none`) | **`-705 ms` (100% instant crisp headline)** |
| **`ar-1280x800`** (Laptop RTL) | `headlineCrispDelayAfterIntroExitMs` | `636 ms` (`opacity: 0.2`, `blur(6px)`) | **`0 ms`** (`opacity: 1`, `filter: none`) | **`-636 ms` (100% instant crisp headline)** |
| **`en-reduced-motion`** (`prefers-reduced-motion: reduce`) | `headlineCrispDelayAfterIntroExitMs` | `0 ms` (`introDurationMs: 112 ms`) | **`0 ms`** (`introDurationMs: 85 ms`) | **Instant reduced-motion path preserved** |

---

## 13. Full Verification Matrix (`PASS` / `FAIL` / `BLOCKED BY EXTERNAL SETUP`)

| Verification Gate / Command | Scope | Result |
| --- | --- | --- |
| `cargo fmt --all -- --check` + `src-tauri` | Rust formatting | **PASS** |
| `cargo clippy --workspace --all-targets -- -D warnings` + `src-tauri` | Rust lint & static analysis | **PASS** |
| `cargo test --workspace` + `src-tauri` | All Rust unit & integration tests (including `workflow_memory` Scenarios A–E, `local_search`, `recipe_library`, `studio`, `sf-server`) | **PASS** |
| `pnpm run format:check` | Prettier formatting across repo | **PASS** |
| `pnpm --dir apps/web run typecheck` | TypeScript 5.8 strict check | **PASS** |
| `pnpm run lint` | ESLint across workspace | **PASS** |
| `pnpm run test` | 66 Vitest test suites (231 tests) | **PASS** |
| `pnpm run build` | Production Web bundle build | **PASS** |
| `pnpm run check:contrast` | WCAG 2.2 AA contrast gate (46 text pairs, 36 UI/chart pairs) | **PASS** |
| `node tools/validate-config.mjs` | Locked stack & config validation | **PASS** |
| `py -3 -B tools/gen-errors.py --check` | Error catalog (`errors.toml`) sync check | **PASS** |
| `py -3 -B tools/verify-migrations.py` | SQL migrations integrity check | **PASS** |
| `py -3 -B tools/verify-i18n.py` | Bilingual ICU translation completeness check | **PASS** |
| `py -3 -B tools/verify-security.py` | Privacy, CSP, formula neutralization & secret audit | **PASS** |
| `tools/check-no-network.sh` + `--self-test` | Zero network crates in `crates/sf-*` engine crates | **PASS** |
| `tools/codeburn/burn.sh` | Zero `TODO`/`FIXME`/placeholders in shipped code | **PASS** |
| `node tools/verify-intro-transition-performance.mjs` | 5-viewport/locale Chromium intro-to-hero transition benchmark (`0ms` delay) | **PASS** |
| `node tools/verify-desktop-full-journey-real-e2e.mjs` | Real unmocked `sf-server` + `studio_ipc_harness` + Chromium E2E (PKCE, Local Search + Open in Studio, Per-Sheet Workflow Memory + Continuous Learning, Official Templates, Save as New, `.safitemplate` Free `SF-RCP-005` / Paid OK, `Share with SafiData`, Owner `/owner` live TOTP review & publish) | **PASS** |
| `node tools/verify-studio-desktop-e2e.mjs` | 6-journey Desktop Studio E2E | **PASS** |
| `node tools/verify-desktop-download-and-owner-web-e2e.mjs` | Honest installer availability & `/owner` portal E2E | **PASS** |
| `node tools/verify-runtime-boundary-e2e.mjs` | Public Showcase vs. Desktop runtime boundary E2E | **PASS** |
| `node tools/verify-offline-and-device-revocation-e2e.mjs` | Single-device revocation (`SF-AUTH-019`) & Offline Read-Only (`SF-AUTH-021`) E2E | **PASS** |
| `node tools/verify-published-installer-download.mjs` | Honest `BLOCKED: NO PUBLISHED INSTALLER URL` until real signed HTTPS binary is published | **BLOCKED BY EXTERNAL SETUP** (Expected honest state) |
| Live PostgreSQL 18 (`DATABASE_URL`) execution | `cargo test -p sf-server --test postgres_control_plane_roundtrip` | **BLOCKED BY EXTERNAL SETUP** (No local PostgreSQL 18 server / `DATABASE_URL` on this machine) |

---

## 14. Remaining Blockers Requiring External Infrastructure or Manual Secrets

1. **PostgreSQL 18 Instance (`DATABASE_URL`)**: Provisioning a live PostgreSQL 18 instance and running `migrations/0001_initial.sql` + `migrations/0002_control_plane_durable.sql` + `cargo test -p sf-server --test postgres_control_plane_roundtrip`.
2. **Code-Signed Public HTTPS Desktop Installer Artifact**: Building and signing the Windows/macOS/Linux desktop installer binaries with production certificates, uploading them to public HTTPS release storage, and setting `available: true` with real `downloadUrl` and `sha256` in `apps/web/public/releases/desktop-release-manifest.json`.
3. **Production Environment Secrets**: `SAFI_OWNER_EMAIL`, `SAFI_OWNER_PASSWORD_HASH`, `SAFI_OWNER_TOTP_SECRET_BASE32`, SMTP credentials, and Paymob/Lemon Squeezy webhook secrets.

---

## 15. Exact Manual Commands for the User After Review

Once you review the local changes, you can inspect, commit, and push at your discretion:

```powershell
git status --short
git diff --stat
git diff --check
```
