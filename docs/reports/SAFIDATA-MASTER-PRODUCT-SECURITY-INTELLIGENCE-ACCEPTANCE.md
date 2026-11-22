# SafiData — Master Product, Security, Optimal Decision Engine, Local Intelligence & Pre-Demo Polish Acceptance Report

---

## 1. Repository Baseline & Git State (Verified)

- **Repository:** `mo7amedrabei14-cell/Safi`
- **Repository Visibility Risk Disclosure:** GitHub repository is currently **Public**. Per mandatory execution rules, the agent did **not** modify repository visibility; the owner will decide and perform any visibility change and secret rotation.
- **Target Branch:** `main` ONLY (no branches created, no `git commit`, no `git push`, no deployment, no production secrets modified).
- **Current Pushed `main` HEAD SHA:** `b623bc973f735c4fc39c366ec39ee0435277583d`
- **Previous Commit SHA:** `49e85e191268de20ee3443238e8dc36ea42b9b4c`
- **Working Tree State:** Dirty on `main` (contains only this pre-demo polish, default theme, security follow-up, repository cleanup, and acceptance report update — awaiting manual owner review and push).
- **GitHub Actions CI Status for Pushed `main` HEAD `b623bc973f735c4fc39c366ec39ee0435277583d` (Queried Live via GitHub Checks & Actions API — Run `44957830319`):**
  - `format`: **`success`**
  - `Full Gate (ubuntu-latest)`: **`success`**
  - `Full Gate (windows-latest)`: **`success`**
  - `Desktop shell compile (windows-latest)`: **`success`**
  - `Intro visual regression (Chromium)`: **`failure`** — failed at Step 9 (`node tools/verify-intro-visual.mjs`); reproduced locally on Windows (`TimeoutError` at `verify-intro-visual.mjs:249` because the legacy script expects `#root .safi-intro-splash` to remain mounted longer and then transition into `.safi-auth-modal`, whereas the public website showcase now opens directly after intro exit without forcing an anonymous sign-in modal).
  - `PostgreSQL 18.6 migration apply / rollback`: **`failure`** — failed at Step 6 (`bash tools/test-migrations.sh`); cannot be executed locally without a live PostgreSQL 18 instance and requires reviewing the GitHub Actions job log / running against PostgreSQL 18.
  - **Overall Remote CI Verdict for `b623bc9`:** **`FAIL`** (4 of 6 CI jobs passed; 2 failed and are reported honestly above).

---

## 2. Complete Plan, Pre-Demo Polish & Follow-Up Audit Status Matrix

Allowed states: `PASS`, `PARTIAL`, `FAIL`, `BLOCKED BY EXTERNAL SETUP`, `NOT IMPLEMENTED`.

| Area | Requirement | Status | Concrete Runtime / Test Evidence & Notes |
| --- | --- | --- | --- |
| **Pre-Demo Sec 1 — Default Dark Mode (`google-gemini`)** | Reverse first-run default theme to Dark Mode (`google-gemini`) across Website & Desktop while preserving existing saved Light Mode (`vision-glass` / `light`) and legacy `apple-light` preferences, with zero flash during load or Intro exit | **`PASS`** | Updated `apps/web/src/lib/theme.ts`, `apps/web/index.html`, `apps/web/public/boot-locale.js`, `apps/web/src/index.css`, `theme.test.ts`, `FloatingGlassDock.test.tsx`, `verify-intro-theme-transition.mjs`. Verified `0` flash frames across fresh Dark (`google-gemini`), saved Light (`vision-glass` EN/AR), legacy `apple-light`, and saved Dark AR. |
| **Pre-Demo Sec 2 — Selected Category & Tool Pill Contrast** | Minimal contrast fix (`color: active ? "var(--text-inverse)" : "var(--text-primary)"`) so active pills/categories render dark text on bright `--accent-primary` in Dark Mode and crisp white text in Light Mode | **`PASS`** | `apps/web/src/components/home/BentoCommandCenter.tsx` (lines 1473 & 1846). Measured in Chromium across all 6 viewports: **`18.08:1` (WCAG AAA)** in Dark Mode (`rgb(9,11,16)` on `rgb(245,245,247)`), **`6.24:1` (WCAG AA)** in Light Mode `vision-glass` (`rgb(255,255,255)` on `rgb(0,87,217)`), **`14.63:1` (WCAG AAA)** in `apple-light`. |
| **Pre-Demo Sec 3 — Rotating Hero Feature Strip (5 Phrases)** | Expand `"One Desktop App that:"` / `"تطبيق واحد على جهازك:"` rotating strip to 5 English and 5 Arabic phrases covering cleaning/understanding, interactive dashboards, 21 chart types, PowerPoint presentations, and executive Word & PDF reports | **`PASS`** | Updated `ROTATING_WORDS_EN` & `ROTATING_WORDS_AR` in `apps/web/src/components/home/BentoCommandCenter.tsx`; verified in Vitest (`BentoCommandCenter.test.tsx`) and Chromium EN/AR screenshots. |
| **Pre-Demo Sec 4 — Vault Key Custody (`vault.rs`)** | Audit raw local `vault.master.key` custody risk; implement safe OS keystore wrapping if feasible without risking encrypted data/CI, or classify honestly as `PARTIAL` with exact threat model & remediation plan | **`PARTIAL`** | **Honest Threat Model:** `SFK1` stores CSPRNG `master_secret` (32B) + `salt` (16B) + AEAD verify tag in `storage_dir/.keystore/vault.master.key` (`0o700`/`0o600` on Unix; user-profile ACLs on Windows). Protects against plaintext `.json` persistence, casual `.safi.enc` inspection/tampering, and cloud-sync exposure when `.keystore/` is excluded, but does **not** protect against an attacker who can read both `.keystore/vault.master.key` and `.safi.enc` under the same OS user or an offline disk image. Because workspace `Cargo.toml` enforces `unsafe_code = "forbid"` and adding unverified native OS keyring crates risks breaking `ubuntu-latest` CI (`libdbus`/`keyutils`) and existing encrypted vaults right before demo, raw FFI/new crates were NOT forced in. Documented threat model in `crates/sf-desktop/src/vault.rs` and defined zero-data-loss `SFK2` migration plan (Windows DPAPI / Credential Manager, macOS Keychain, Linux Secret Service via audited safe wrapper with verified round-trip unwrap before atomic keystore replacement). |
| **Pre-Demo Sec 5 — Workflow Replay High-Only Rust Boundary** | Enforce `WorkflowMatchTier::High` + `auto_replay_eligible == true` + zero missing required columns at `StudioStore::desktop_workflow_memory_apply_replay`; reject `Medium` & `Low` with `SF-RCP-001` at Rust IPC boundary; never silently swallow auto-replay errors in UI | **`PASS`** | Added `LocalWorkflowMemoryStore::match_specific_workflow_with_table` (`workflow_memory.rs`), enforced strict High-only gate in `StudioStore::desktop_workflow_memory_apply_replay` (`studio.rs`), exposed non-destructive bilingual notice in `DesktopStudio.tsx`, and added direct Rust IPC test `studio::tests::workflow_replay_ipc_enforces_high_only_confidence_and_rejects_medium_and_low` (`PASS`). |
| **Pre-Demo Sec 6 — Clean Generated E2E Artifacts & `.gitignore`** | Remove generated E2E files accidentally added in `b623bc9` without broadly deleting `artifacts/`, and add targeted `.gitignore` rules | **`PASS`** | Removed 9 generated files (`artifacts/real-e2e-full-journey/*` [8 files] + `artifacts/intro-visual/studio-e2e/bundles/studio_q4.safi.enc`) while keeping reference assets intact; added targeted rules in `.gitignore` (`artifacts/real-e2e-full-journey/`, `artifacts/intro-visual/studio-e2e/bundles/.keystore/`, `*.safi.enc`, `*.tmp`, `artifacts/**/prior_auth.marker`). |
| **Pre-Demo Sec 7 — Final Website Visual Verification** | Verify `1920×1080`, `1810×1000`, `1440×900`, `1280×800`, `768×1024`, `390×844` across Dark default, preserved Light, legacy `apple-light`, selected category contrast, 5 rotating phrases, `3×2`/`2×3`/`1×6` cards, `#compare` 12 rows, zero intro flash | **`PASS`** | `verify-website-visual-audit.mjs` passed `7/7` cases across all 6 viewports (`widthSpread: 0`, `cardCount: 6`, `compareRows: 12`, contrast `18.08:1` Dark / `6.24:1` Light / `14.63:1` Apple Light), plus `verify-intro-theme-transition.mjs` (`0` flash frames) and `verify-intro-transition-performance.mjs` (`0ms` crisp delay). Fresh screenshots visually inspected. |
| **Local Encryption at Rest (Files & Migration)** | Mandatory `VaultKey` (CSPRNG + Argon2id + ChaCha20-Poly1305), fail-closed `SF-SYS-024`, atomic verified migration from legacy plaintext `.json`, zero plaintext `.json` writes in production | **`PASS`** (File encryption & migration `PASS`; OS keystore custody tracked separately above as `PARTIAL`) | 7 vault tests (`vault.rs`) + `studio_store_writes_only_encrypted_files_and_atomic_workflow_replay_rolls_back_on_error` + Tauri `.setup()` `ensure_local_vault_key()`. |
| **Per-Sheet Workflow Memory & Continuous Learning** | Content-based matching, generic header downgrade, atomic staged replay rollback (`SF-RCP-001`), honest numerator/denominator `LearningEffectivenessMetrics` (`null` / `0/0` when empty) | **`PASS`** | `workflow_memory::tests::*`, `studio::tests::*`, `tools/verify-desktop-full-journey-real-e2e.mjs` (`PASS`). |
| **Safi Decision Engine & Local Ollama Probe** | Data-driven lexicographic scoring across all valid measures & feasible chart families, zero hardcoded `"EGP"`, mixed-currency safety, 4-way duplicate classification, bounded loopback probe (`127.0.0.1:11434/api/tags`) outside air-gapped engine crates | **`PASS`** (Local boundary & unreachable probe verified; live model inference requires local Ollama service + pulled model on user device) | `decision_engine::tests::*`, `apps/desktop/src-tauri/src/main.rs`, zero `std::net` in `crates/sf-*/src`. |
| **Auth, RBAC, Single-Device Exclusivity & Desktop Studio** | Owner `/v1/owner/*` RBAC (`403 SF-AUTH-013`), Web `sfw_*` vs Desktop `sfs_*`, RFC 6238 TOTP MFA, Desktop PKCE (`S256`), single-device revocation (`SF-AUTH-019`), Offline Read-Only (`SF-AUTH-021`), 7 input formats, 21 charts, 9 output formats, BM25 Local Search | **`PASS`** | `cargo test --workspace` (`0` failed), `apps/desktop/src-tauri` tests (`2` passed), `verify-desktop-full-journey-real-e2e.mjs` (`PASS`). |
| **GitHub Actions CI (`SafiData CI Gate` on `b623bc9`)** | Remote GitHub Actions checks on pushed `main` commit `b623bc973f735c4fc39c366ec39ee0435277583d` | **`FAIL`** | 4/6 passed (`format`, `Full Gate (ubuntu-latest)`, `Full Gate (windows-latest)`, `Desktop shell compile (windows-latest)`); 2/6 failed (`Intro visual regression (Chromium)` at Step 9 `verify-intro-visual.mjs`, and `PostgreSQL 18.6 migration apply / rollback` at Step 6 `test-migrations.sh`). |
| **Billing — Paddle (USD) & Paymob (EGP)** | Cryptographic webhook handlers & subscription lifecycle in `crates/sf-server/src/billing.rs` | **`PARTIAL` (Local tests `PASS`; Live Checkout/Webhooks `BLOCKED BY EXTERNAL SETUP`)** | Requires live `SAFI_PADDLE_*` / `SAFI_PAYMOB_*` credentials, product/price IDs, and public HTTPS webhook endpoints. |
| **Central Database — Live PostgreSQL 18 Runtime** | `PgControlPlaneStore` (`crates/sf-server/src/pg_control_plane.rs`) + migrations `0001..0012` | **`BLOCKED BY EXTERNAL SETUP` (Locally) / `FAIL` (Remote CI `b623bc9`)** | Local `postgres_multi_instance_e2e` is `ignored` (no local PostgreSQL 18 server); remote CI `PostgreSQL 18.6 migration apply / rollback` failed Step 6 on `b623bc9` and needs investigation against a live PostgreSQL 18 instance. |
| **Live OAuth Providers & Signed Public Installers** | Google/Apple/GitHub OAuth credentials + Authenticode/notarized installers hosted over HTTPS | **`BLOCKED BY EXTERNAL SETUP`** | Live OAuth secrets and signed public binaries (`available=false`) are not configured. |

---

## 3. Exact Files Changed in This Pre-Demo Polish Round

| File | Summary of Changes |
| --- | --- |
| [theme.ts](file:///C:/Users/mo7am/OneDrive/Work/Safi/apps/web/src/lib/theme.ts) | Switched `SAFI_DEFAULT_THEME` to `"google-gemini"` (Dark Mode default) while preserving saved `"vision-glass"`, `"light"`, and legacy `"apple-light"` preferences. |
| [index.html](file:///C:/Users/mo7am/OneDrive/Work/Safi/apps/web/index.html) | Set initial `<html ... data-theme="google-gemini">` so first paint matches Dark Mode default with zero flash. |
| [boot-locale.js](file:///C:/Users/mo7am/OneDrive/Work/Safi/apps/web/public/boot-locale.js) | Updated fallback when no saved theme exists to `"google-gemini"` while respecting saved Light Mode (`"vision-glass"`, `"light"`, `"apple-light"`). |
| [index.css](file:///C:/Users/mo7am/OneDrive/Work/Safi/apps/web/src/index.css) | Moved `:root` default CSS variables to the Dark Mode (`google-gemini`) block while keeping explicit Light Mode (`vision-glass` / `light`) and `apple-light` blocks intact. |
| [theme.test.ts](file:///C:/Users/mo7am/OneDrive/Work/Safi/apps/web/src/lib/theme.test.ts) | Updated default theme assertions and added runtime `localStorage` tests verifying fresh Dark default, preserved Light Mode (`vision-glass`), preserved legacy `apple-light`, and bidirectional switching. |
| [FloatingGlassDock.test.tsx](file:///C:/Users/mo7am/OneDrive/Work/Safi/apps/web/src/components/layout/FloatingGlassDock.test.tsx) | Updated default theme expectation to `google-gemini` and verified explicit Light Mode (`vision-glass`) rendering. |
| [BentoCommandCenter.tsx](file:///C:/Users/mo7am/OneDrive/Work/Safi/apps/web/src/components/home/BentoCommandCenter.tsx) | Replaced hardcoded active `#FFFFFF` text with `var(--text-inverse)` on selected Visual Editor pills & Well-Organized Category rows (Sec 2), and expanded `ROTATING_WORDS_EN` & `ROTATING_WORDS_AR` to all 5 required phrases (Sec 3). |
| [DesktopStudio.tsx](file:///C:/Users/mo7am/OneDrive/Work/Safi/apps/web/src/desktop/DesktopStudio.tsx) | Exposed non-destructive bilingual notice via `setAutoReplayBanner` if automatic replay returns an error instead of silently swallowing it (Sec 5). |
| [workflow_memory.rs](file:///C:/Users/mo7am/OneDrive/Work/Safi/crates/sf-desktop/src/workflow_memory.rs) | Added `LocalWorkflowMemoryStore::match_specific_workflow_with_table` to evaluate the exact requested workflow ID with domain isolation for strict replay gating (Sec 5). |
| [studio.rs](file:///C:/Users/mo7am/OneDrive/Work/Safi/crates/sf-desktop/src/studio.rs) | Enforced `match_tier == High && auto_replay_eligible && missing_required_columns.is_empty()` in `desktop_workflow_memory_apply_replay` (failing closed with `SF-RCP-001` for `Medium`/`Low`) and added direct Rust IPC test `workflow_replay_ipc_enforces_high_only_confidence_and_rejects_medium_and_low` (Sec 5). |
| [vault.rs](file:///C:/Users/mo7am/OneDrive/Work/Safi/crates/sf-desktop/src/vault.rs) | Documented the exact `SFK1` key-custody threat model, known limitation, and `SFK2` OS keystore migration architecture honestly in rustdoc without overclaiming or breaking `unsafe_code = "forbid"` (Sec 4). |
| [.gitignore](file:///C:/Users/mo7am/OneDrive/Work/Safi/.gitignore) | Added targeted ignore rules for generated E2E runtime artifacts (`artifacts/real-e2e-full-journey/`, `.keystore/`, `*.safi.enc`, `*.tmp`, `prior_auth.marker`) (Sec 6). |
| `artifacts/real-e2e-full-journey/*` (8 files) & `artifacts/intro-visual/studio-e2e/bundles/studio_q4.safi.enc` | Deleted the 9 accidentally committed generated E2E artifacts from `b623bc9` (Sec 6). |
| [verify-intro-theme-transition.mjs](file:///C:/Users/mo7am/OneDrive/Work/Safi/tools/verify-intro-theme-transition.mjs) | Updated Case 1 to verify fresh Dark Mode default (`storedTheme: null -> google-gemini`) plus saved Light (`vision-glass` EN/AR), legacy `apple-light`, and saved Dark AR. |
| [verify-ux-e2e.mjs](file:///C:/Users/mo7am/OneDrive/Work/Safi/tools/verify-ux-e2e.mjs) | Updated initial `data-theme` expectation from `vision-glass` to `google-gemini`. |
| [SAFIDATA-MASTER-PRODUCT-SECURITY-INTELLIGENCE-ACCEPTANCE.md](file:///C:/Users/mo7am/OneDrive/Work/Safi/docs/reports/SAFIDATA-MASTER-PRODUCT-SECURITY-INTELLIGENCE-ACCEPTANCE.md) | Updated baseline SHAs (`b623bc9` / `49e85e1`), live GitHub Actions CI results, honest `PARTIAL` vault custody disclosure, pre-demo polish matrix, verification commands, and final screenshots. |

---

## 4. Local Verification Commands, Exact Exit Codes & Raw Results

All commands below were executed locally on Windows PowerShell against the current working tree on `main` (`HEAD = b623bc973f735c4fc39c366ec39ee0435277583d`):

| Command | Exit Code | Concise Raw Result |
| --- | --- | --- |
| `cargo fmt --all -- --check` | `0` | Clean Rust formatting across workspace |
| `cargo clippy --workspace --all-targets -- -D warnings` | `0` | `0` warnings / `0` errors across all workspace crates & targets |
| `cargo test -p sf-desktop --all-targets` | `0` | `86` lib unit tests (including new `workflow_replay_ipc_enforces_high_only_confidence_and_rejects_medium_and_low`) + `11` integration tests passed; `0` failed |
| `cargo test --workspace` | `0` | All workspace tests passed across all 11 crates (`sf-arabic`, `sf-clean`, `sf-cli`, `sf-core`, `sf-desktop`, `sf-io`, `sf-profile`, `sf-recipe`, `sf-report`, `sf-server`, `sf-wasm`); `postgres_multi_instance_e2e` ignored (`1 ignored`, requires live PostgreSQL 18) |
| `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml` | `0` | `2 passed; 0 failed; 1 ignored` (manual TLS benchmark) |
| `pnpm run format:check` | `0` | `All matched files use Prettier code style!` |
| `pnpm --dir apps/web run typecheck` | `0` | `tsc --noEmit` succeeded with `0` errors |
| `pnpm run lint` | `0` | ESLint passed with `0` errors |
| `pnpm run test` | `0` | `66/66` Vitest test files (`231/231` tests) passed |
| `pnpm run build` | `0` | Production Vite build succeeded (`apps/web/dist`) |
| `pnpm run check:contrast` | `0` | `Verified 46 text pairs (min 4.58:1 >= 4.5:1) and 36 UI/chart pairs (min 4.37:1 >= 3.0:1)` + self-test `OK` |
| `node <scratch>/verify-website-visual-audit.mjs` | `0` | `7/7` cases `ok: true` across `1920×1080`, `1810×1000`, `1440×900`, `1280×800`, `768×1024`, `390×844` (fresh Dark default `google-gemini`, saved Light `vision-glass`, legacy `apple-light`; active category & editor pill contrast `18.08:1` Dark / `6.24:1` Light / `14.63:1` Apple Light; `widthSpread: 0`; `cardCount: 6`; `compareRows: 12`) |
| `node tools/verify-intro-theme-transition.mjs` | `0` | `0` dark-flash frames and `0` mismatched background frames across fresh Dark (`google-gemini`), saved Light (`vision-glass` EN/AR), legacy `apple-light`, and saved Dark AR |
| `node tools/verify-intro-transition-performance.mjs` | `0` | `0ms` headline crisp delay across all 5 cases |
| `node tools/verify-desktop-full-journey-real-e2e.mjs` | `0` | `PASS (100% REAL, ZERO BACKEND/RUST MOCKS)`: PKCE Auth + Local Search + Workflow Memory & Learning + Safi Decision Engine + Recipe Library + `.safitemplate` (`SF-RCP-005`/Paid) + Share + Owner Portal TOTP Review & Publish |
| `node tools/verify-intro-visual.mjs` | `1` | Reproduced remote CI failure (`TimeoutError` at line 249): legacy script expects `.safi-auth-modal` auto-mount on initial anonymous web load instead of the public showcase landing page |
| `node tools/verify-ux-e2e.mjs` | `1` | Same legacy expectation (`waiting for locator('.safi-auth-modal') to be visible` on initial anonymous web load) |

---

## 5. Final Visual Verification Screenshots (Production `apps/web/dist`)

Captured and visually inspected for this pre-demo polish round:

- **1920×1080 English LTR Dark Mode Default (Hero + 5-phrase strip):** `safi-final-hero-desktop-1920-en-default-dark.png`
- **1920×1080 English LTR Dark Mode Default (3×2 Capabilities Matrix):** `safi-final-cards-desktop-1920-en-default-dark.png`
- **1920×1080 English LTR Dark Mode Default (Selected Category & Tool Pill Contrast — 18.08:1):** `safi-final-categories-desktop-1920-en-default-dark.png`
- **1810×1000 Arabic RTL Dark Mode Default (Hero + 5-phrase strip):** `safi-final-hero-desktop-1810-ar-default-dark.png`
- **1810×1000 Arabic RTL Dark Mode Default (Selected Category & Tool Pill Contrast — 18.08:1):** `safi-final-categories-desktop-1810-ar-default-dark.png`
- **1440×900 English LTR Preserved Light Mode (Hero):** `safi-final-hero-desktop-1440-en-saved-light.png`
- **1440×900 English LTR Preserved Light Mode (Selected Category Contrast — 6.24:1):** `safi-final-categories-desktop-1440-en-saved-light.png`
- **390×844 Mobile Arabic RTL Dark Mode Default (Hero):** `safi-final-hero-mobile-390-ar-default-dark.png`
