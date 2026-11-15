# Safi Quad-Studio Completion Re-Audit (`STUDIO-COMPLETION-RE-AUDIT.md`)

**Date:** 2026-11-15  
**Repository:** `mo7amedrabei14-cell/Safi`  
**Branch:** `main` (no other local branches; working tree clean at start)  
**Baseline Commit SHA:** `f7a1fc6b6f14c914a44caf5960ba74500f4f060e` (`origin/main`)

---

## 1. Executive Baseline & Gate Verification

### Commits Reviewed Since P0.05 (`119c361`)
- `2726c04` — `fix(clean): keep original text, typed values, and structural duplicate keys`
- `0653971` — `feat(studio): add datagraph and verified metric tokens (STUDIO-PHASE-1)`
- `b124b07` — `docs(adr): record the studio engine stack and code ownership` (ADR-0013)
- `c482242` — `docs(report): record the real STUDIO-PHASE-1 state and next blockers`
- `8d16e5d` — `feat(studio): add validated DSL, anomaly detection, templates, and offline capsule`
- `d09b3a2` — `fix(studio): keep the rebuild_provenance doc comment on its own function`
- `5a69601` / `76623e3` — `sf-wasm` datagraph exposure and revert
- `2e006d3` / `4ba969d` — web Quad-Studio surface and revert
- `3a5f7eb` — `docs(studio): record that the web stays a showcase, not an executor` (ADR-0014)
- `12e02f8` — `feat(desktop): run the studio engine behind allowlisted Tauri commands`
- `c751741` — `chore(desktop): lock the engine crates the studio commands now depend on`
- `7a17142` — `feat(desktop): add the studio screen inside the app window`
- `6c22862` — `style: apply repository formatters`
- `20822c8` — `Update` (`Safi.docx`)
- `f7a1fc6` — `Merge branch 'main' of https://github.com/mo7amedrabei14-cell/Safi`

### Actual Baseline Check Results (2026-11-15, Windows x64)
- **Toolchain observed:** `rustc 1.99.1`, `node v22.23.2`, `pnpm 10.8.1`, `git 2.55.0.windows.4`; `gh` CLI is not installed on this host.
- **Note on `make check` under default PowerShell:** `C:\Users\mo7am\AppData\Local\Microsoft\WindowsApps\bash.exe` (WSL) lacks Windows `cargo` in PATH and fails on `tools/check-no-network.sh`. Running under Git Bash (`C:\Program Files\Git\bin\bash.exe`) with Windows toolchain in PATH produced:
  - `make check-no-network` — **PASS** (10 engine crates verified, self-test passed).
  - `make check-errors` — **PASS** (`errors.toml` and generated Rust/TS catalogs consistent, self-test passed).
  - `make check-contrast` — **PASS** (46 text pairs, 36 UI/chart pairs, self-test passed).
  - `make check-i18n` — **PASS** (17 i18n keys in `ar.json`/`en.json`, self-test passed).
  - `make check-migrations` — **PASS** (6 migration pairs verified, self-test passed).
  - `make fmt-check` — **FAIL** on `apps/desktop/src-tauri/src/main.rs` due to working-tree CRLF (`i/lf w/crlf attr/text eol=lf`). `cargo fmt --all -- --check` passed.
  - `make lint` (`cargo clippy --workspace --all-targets -- -D warnings`, `pnpm run typecheck`, `pnpm run lint`) — **PASS**.
  - Desktop Clippy (`cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml -- -D warnings`) — **PASS**.
  - `make test` (`cargo test --workspace` + `pnpm run test`) — **PASS** (all Rust unit/integration tests passed; 3 Postgres-backed `sf-server` tests ignored when `SF_TEST_DATABASE_URL` is unset; Vitest: 58 files / 182 tests passed).
  - `make build` (`pnpm run build`) — **PASS**.
  - `make codeburn` — **PASS**.
  - `make security` (`verify-security.py`, `no-cost-gen.py --verify`, `pnpm audit`) — **PASS** (0 vulnerabilities).

---

## 2. Architecture Decisions Superseding Original v4 Wording

1. **ADR-0013 (Engine Stack & Ownership):**
   - `sf_core::table::Table` + `rust_decimal::Decimal` remain the authoritative engine representation instead of Apache Arrow / Polars.
   - Shared Studio identifiers and `VerifiedMetricToken` live in `crates/sf-core/src/datagraph.rs`.
   - `DataGraph`, formula AST, transformations, scenarios, DSL, anomaly detector, templates, and capsule exporter live in `crates/sf-recipe/` rather than creating new `sf-calc` or `sf-detect` crates (preserving `verify-security.py` scope governance and avoiding `sf-core <-> sf-clean` dependency cycles).
   - No 16ms or 500k-row claim may be made without real measured benchmarks.
2. **ADR-0014 (Desktop-Only Execution vs Public Web Showcase):**
   - The public web build (`apps/web` in a browser) is a showcase site only; it carries no WASM executor and no file parsing for Studio.
   - Original v4/Phase 5 references to `SmartDropzone` -> WASM Web Worker execution in browser tabs are superseded by ADR-0014: Studio execution lives in `crates/sf-desktop/src/studio.rs` behind allowlisted Tauri IPC commands (`ALLOWED_COMMANDS` in `crates/sf-desktop/src/commands.rs`) using the native desktop open-file dialog (`pick_input_file`).
   - `DesktopStudio` renders inside the authenticated `ApplicationShell` only when `isDesktopRuntime()` detects `__TAURI_INTERNALS__.invoke`.
3. **P0.05 Mandatory Authentication Gate (ADR-0012):**
   - Product workspace (`ApplicationShell`) mounts only when `sessionToken` is non-null and non-empty. Must remain completely untouched.

---

## 3. Detailed Item-by-Item Status Matrix

### A. Already Implemented and Tested
1. **Immutable Origin & Replayable DataGraph (`crates/sf-recipe/src/datagraph.rs`):**
   - Immutable `origin: Table`, `origin_fingerprint` (BLAKE3), `working: Table`, replayable `transforms`, calculated column formulas (`+`, `-`, `*`, `/`, unary `-`), metric formulas (`SUM`, `AVG`, `COUNT`, `MIN`, `MAX` + arithmetic + parameters), dependency propagation (`propagate`), `undo`/`redo`, `to_json`/`from_json`, `grouped_aggregate`, `Scenario` & `compare_scenarios` & `reset_scenario`.
2. **VerifiedMetricToken (`crates/sf-core/src/datagraph.rs`):**
   - `SourceId`, `MetricId`, `ScenarioId`, `DatasetVersion`, `Revision`, `VerificationStatus` (`Verified`, `Stale`, `Unverified`), `VerificationOutcome`, `BLAKE3` content fingerprint excluding `computed_at_ms`, `verify_against` / `verify_strict` (`SF-CAL-008`).
3. **DSL Schema, Validator & Narrative Resolver (`crates/sf-recipe/src/studio_dsl.rs`):**
   - Strict `#[serde(deny_unknown_fields)]` `StudioCommand` enum, `parse_command`, atomic `apply_command`, `render_narrative`, `render_narrative_strict` (`{{metric:ID}}`).
4. **Anomaly Detector Core (`crates/sf-recipe/src/studio_anomaly.rs`):**
   - Deterministic read-only detection for `MissingValue`, `DuplicateRow`, `InconsistentType`, `ImpossibleDate`, `StatisticalOutlier` (1.5x IQR with `Decimal`) using i18n keys.
5. **Template Definitions Core (`crates/sf-recipe/src/studio_templates.rs`):**
   - Five template definitions (`boardroom-keynote`, `velocity-radar`, `impact-atlas`, `forensic-xray`, `time-machine`) and `instantiate()` failing loudly with `SF-CAL-006` on missing metric bindings.
6. **Desktop IPC Allowlist & Core Bridge (`crates/sf-desktop/src/commands.rs`, `studio.rs`, `apps/desktop/src-tauri/src/main.rs`, `apps/web/src/desktop/studio-bridge.ts`):**
   - 17 allowlisted commands (`13` `studio_*` + `4` shell/dialog commands), `50 MiB` import limit, extension check before open, atomic file write (`write_atomic`), `SF-SYS-010` browser guard, `DESKTOP_TAB` conditional dock integration, P0.05 auth gating in `ApplicationShell`.

### B. Implemented in Rust, NOT Integrated into Desktop Bridge / UI
1. **What-If Scenarios & Parameters (Phase 1.3):** `define_parameter`, `add_scenario`, `activate_scenario`, `reset_scenario`, `compare_scenarios` exist in `DataGraph` (`sf-recipe`), and `studio_state` serializes `activeScenario`, `scenarios`, `parameters`, **but** `sf-desktop/src/studio.rs`, `main.rs`, `ALLOWED_COMMANDS`, `studio-bridge.ts`, and `DesktopStudio.tsx` expose zero commands or UI for creating, renaming, activating, comparing, resetting scenarios, or adjusting What-If sliders.
2. **DSL Command Execution & Narrative Rendering (Phase 2.1, 2.3):** `studio_apply_command` and `studio_narrative` exist in `sf-desktop` and `main.rs`, **but** `studio-bridge.ts` has no wrappers for them and `DesktopStudio.tsx` has no Command Bar or Narrative editor.
3. **Five Living Templates (Phase 4.1):** `TEMPLATES`, `template_by_id`, and `instantiate` exist in `sf-recipe`, **but** have zero integration in `sf-desktop`, `main.rs`, `studio-bridge.ts`, or `DesktopStudio.tsx`.

### C. Implemented Partially / Defective
1. **Provenance & Row/Column Identities (Phase 1.1):**
   - **Defect:** `VerifiedMetricToken.row_refs` (`Vec<u64>`), `column_refs` (`Vec<String>`), and `Transformation::SetCell { row: usize, column: String, ... }` use positional row indices and mutable display column names instead of stable `RowId` and `ColumnId` identities. Sorting, filtering, deleting, or inserting rows will corrupt row provenance.
2. **VerifiedNumberChip (Phase 1.2):**
   - **Defect:** Implemented inline inside `DesktopStudio.tsx` rather than a reusable component; lacks `Error` vs `Unverified` vs `Stale` vs `Verified` distinct handling; hides `sourceId`, `originFingerprint`, and `transformation_trace`; truncates `rows.slice(0, 20)` without pagination/navigation.
3. **Reactive Results & Concurrency (Phase 1.4):**
   - **Defect:** Dashboard chart requires clicking a manual `Refresh chart` button (`refreshChart`). No automatic recomputation of charts on cell edit/undo/redo/scenario change; no `expectedRevision` / request-sequence guard in `studio-bridge.ts` or UI to reject stale async responses; no in-flight recalculation/stale badge while Rust recomputes.
4. **Large Dataset Serialization (Phase 1.5):**
   - **Defect:** `state_json` in `crates/sf-desktop/src/studio.rs` serializes every row in `working` on every single mutation and `DesktopStudio.tsx` renders `state.rows.slice(0, 50)`. No paginated row query, no Rust-side search/sort/filter, no incremental state metadata response.
5. **Anomaly Detector UI & Fixes (Phase 2.4):**
   - **Defect:** `DesktopStudio.tsx` renders raw `anomaly.kind` and raw i18n key string `(anomaly.suggestionKey)` instead of translated explanation, method, expected impact, severity/confidence, and user-approved undoable Rust corrections.
6. **Dashboard Charting (Phase 3.2 & 4.4):**
   - **Defect (Rule Violation):** `crates/sf-desktop/src/studio.rs:288` uses `AggFunc::from_name(&chart.function).unwrap_or(AggFunc::Sum)`, silently falling back to `SUM` on unknown aggregation names.
   - **Defect:** `DesktopStudio.tsx` uses `Number(group.value)` for bar ratios and supports only 1 chart.
7. **Capsule Exporter & CSP (Phase 4.2):**
   - **Defect:** `studio_capsule.rs` "Slides" section only renders metric cards, not actual user slides, structured executive report blocks, template sections, or embedded assets. CSP uses `style-src 'unsafe-inline'` and inline `style="width:...%"` attributes.
8. **Output Save Dialog (Phase 4.3):**
   - **Defect:** `DesktopStudio.tsx` calls `pickOutputFile()` (hardcoded to `"CSV", &["csv"]`, `"cleaned.csv"`) when exporting `.safi.html` capsules.
9. **Project Persistence (Phase 3.6):**
   - **Defect:** `DesktopStudio.tsx` uses a single `window.localStorage` key (`safi.desktop.studio.snapshot.v1`), losing multi-project support, slides/doc/dashboard/layout/template state, atomic disk persistence, integrity verification, and storage-limit handling.
10. **CI Chromium Runner (Phase 5.3):**
    - **Defect:** `.github/workflows/ci.yml:134` uses `runs-on: ubuntu-latest` for `intro-visual`, which fails with pinned `playwright@1.56.1` on `ubuntu26.04-x64` runners; must be pinned to `ubuntu-24.04`.
11. **Working-Tree Line Endings on Windows:**
    - `apps/desktop/src-tauri/src/main.rs` has CRLF in working tree (`w/crlf`) causing `make fmt-check` to fail on Windows.

### D. Not Implemented
1. **Real AI Provider / Local Model Runtime (Phase 2.2 — EXTERNAL BLOCKER):**
   - Zero AI providers exist in `sf-server`, `apps/web/package.json`, or `crates/*`. Neither Ollama, llama.cpp, WebLLM model weights, nor any configured API key/proxy exists locally or in the repo, and Tauri CSP forbids external connections (`connect-src 'self' ipc: http://ipc.localhost`). Phase 2.2 cannot fake a keyword parser as AI; the command bar + DSL interpreter contract + validation must be built cleanly while reporting the missing local model / AI provider blocker honestly unless one is configured.
2. **LiveCleanSheetView Full Editor (Phase 3.1):** Missing column add/remove, per-column cleaning ops, sorting, filtering, search, pagination, column type display, raw vs normalized view, cell validation errors, changed-cell markers, provenance jump.
3. **CodeMotionDashboardView Full Editor (Phase 3.2):** Missing multi-chart dashboard, add/edit/duplicate/delete charts, multiple chart types (`bar`, `line`, `area`, `pie`, `donut`, `scatter`, `histogram`, `kpi`), grouping/sorting/filters/cross-filtering/drill-down, legends, display options, persistence.
4. **LiveKeynoteSlidesView (Phase 3.3):** Not implemented at all.
5. **SmartVerifiedDocView Rich-Blocks Editor (Phase 3.4):** Not implemented at all.
6. **Custom Free-Form Builder (Phase 3.5):** Not implemented at all (must include keyboard alternatives for move/resize/reorder and persisted structured layout).
7. **IPC Session Gate & Comprehensive Integration / Browser / Offline Capsule / Benchmark Tests (Phase 5.1, 5.2, 5.4):**
   - Tauri `studio_*` commands currently rely on the UI authentication gate (`ApplicationShell`) without an IPC-level session check.
   - Only 3 SSR unit tests exist in `desktop-studio.test.tsx`; no end-to-end 18-journey tests, offline Chromium `file://` capsule test, accessibility/theme/RTL tests, or real multi-size performance benchmarks exist yet.

---

## 4. Final Hardening & Acceptance v6 Remediation Summary (`2026-11-15`)

Following the code review at commit `361d3a20f8d80d34c536ecd273bfcb16b7c14dd5`, the six remaining audit findings were remediated directly on `main`:

1. **Native Fail-Closed Tauri Auth & IPC Payload Gate (`crates/sf-desktop/src/{commands.rs,studio.rs}`, `apps/desktop/src-tauri/src/main.rs`, `apps/web/src/{ApplicationShell.tsx,desktop/studio-bridge.ts}`):**
   - Added `StudioSessionAuthority`, `StudioSessionVerifier`, `VerifiedStudioSession`, `ActiveStudioSession` (with `[REDACTED]` `Debug`), `set_auth_session`, `clear_auth_session`, and `gate_command` inside `StudioStore`.
   - Every sensitive Studio and file-picker command (`43` commands in `apps/desktop/src-tauri/src/main.rs`) now executes `authorize_command`, `validate_ipc_payload_len` (`MAX_IPC_BYTES = 1 MiB` -> `SF-SYS-007`), and fail-closed session + single-device verification (`SF-AUTH-001` / `SF-AUTH-002`) before touching `DataGraph` or the filesystem.
   - `DesktopShellSessionVerifier` in `apps/desktop/src-tauri/src/main.rs` verifies tokens against the in-process `StudioSessionAuthority` or loopback `GET /v1/auth/sessions/current`, never trusting a token merely because it arrived from React.
2. **Real Desktop E2E & Standalone `file://` Capsule Browser Verification (`tools/verify-studio-desktop-e2e.mjs`, `tools/verify-capsule-chromium.mjs`, `crates/sf-desktop/examples/studio_ipc_harness.rs`, `.github/workflows/ci.yml`):**
   - Added `tools/verify-capsule-chromium.mjs` opening the Rust-exported `.safi.html` capsule over `file://` in headless Chromium and asserting all 8 chart types, Slides, Executive Report, Free-Form (`rowSpan=3`), Verified Metrics, embedded `data:image/png;base64` asset decoding, zero `<script>` tags, zero external network requests, and zero Console/CSP errors.
   - Added `tools/verify-studio-desktop-e2e.mjs` backed by the real compiled Rust `studio_ipc_harness` process over stdio JSON-RPC, exercising the full 10-step journey from locked pre-login state through CSV import, cell edit, What-If scenario, all 4 creative surfaces, atomic `BLAKE3` bundle save/reload, `.safi.html` export, and sign-out revocation.
3. **Complete 8 Dashboard Chart Types (`crates/sf-recipe/src/{datagraph.rs,studio_capsule.rs}`, `crates/sf-desktop/src/studio.rs`, `apps/web/src/desktop/components/DashboardChartsView.tsx`):**
   - Added `ChartType` (`bar`, `line`, `area`, `pie`, `donut`, `scatter`, `histogram`, `kpi`) and `DataGraph::compute_chart_series` in Rust with strict column/data suitability checks (`SF-CAL-004` on non-numeric scatter/histogram columns or negative pie/donut slices).
4. **Free-Form Builder `rowSpan` Controls & Strict Bounds (`crates/sf-desktop/src/studio.rs`, `crates/sf-recipe/src/studio_capsule.rs`, `apps/web/src/desktop/components/FreeFormBuilderView.tsx`):**
   - Added `-H`, `H:n`, `+H` height buttons and `Shift+ArrowUp`/`Shift+ArrowDown` keyboard resizing, enforcing `col: 1..=12`, `row: 1..=32`, `colSpan: 1..=12`, `rowSpan: 1..=8`, and `zIndex: 1..=50` in both Rust and UI, and preserving `grid-row: <row> / span <rowSpan>` in `.safi.html` capsules.
5. **`AnomalyPanel` Human-Readable EN/AR Translations (`apps/web/src/desktop/components/AnomalyPanel.tsx`):**
   - Translated all `messageKey`, `methodKey`, and `suggestionKey` entries across all 5 anomaly kinds (`MissingValue`, `DuplicateRow`, `InconsistentType`, `ImpossibleDate`, `StatisticalOutlier`) into human-readable English and Arabic with a safe fallback humanizer so raw `studio.*` keys never appear.
6. **AI Integration Privacy Disclosure & Pinned Loopback CSP (`apps/web/src/desktop/ai-interpreter.ts`, `apps/web/src/desktop/components/CommandBarPanel.tsx`, `crates/sf-desktop/src/api_origin.rs`, `apps/desktop/src-tauri/tauri.conf.json`):**
   - Updated the consent checkbox and added a live outbound payload preview (`data-testid="ai-outbound-payload-preview"`) explicitly disclosing that the user's prompt text, column names & types, metric IDs, and parameter names are sent (and never row values, cell contents, or filenames).
   - Permitted only the pinned loopback Ollama origin (`http://127.0.0.1:11434`) or `https://` endpoints in `validateAiEndpointUrl`, `api_origin.rs`, and `tauri.conf.json`.
