# AGENTS.md (SafiData / صافي — v5.0 Full Gold Master)

## What this is
English-default, bilingual (English/Arabic), local-first data cleaning, charting, and reporting platform (Web PWA, Windows, Linux, CLI; Mobile 1.1/1.2).
The full specification is in `docs/spec/`. Precedence: `docs/spec/20-v5-architecture-upgrades.md` & `docs/spec/18-review.md` > all earlier sections.
The owner receives all reports in simple, structured Arabic following `.agents/skills/i-have-adhd/SKILL.md`.

## Hard rules
1. Read this file and the spec files named in the task before coding. Never build features outside the current task.
2. No TODO, no placeholder code, no fake data in shipped code. Every feature ships with automated tests.
3. All architectural decisions are locked in v5.0. Record implementation ADRs in `docs/DECISIONS.md` tracked via **Ponytail**.
4. User data never leaves the device in tests, logs, crash reports or telemetry. No cell values, file names or column names in any log.
5. Never write your own cryptography. Use audited RustCrypto / libsodium (`XChaCha20-Poly1305`, `Argon2id`, `Ed25519`, `X25519`, `BLAKE3`, `PASETO v4`).
6. Pin every dependency to an exact version with a lockfile. Disable `postinstall` scripts.
7. The engine crates (`crates/sf-*`) have ZERO network access. CI fails if a network crate appears in them (`tools/check-no-network.sh`).
8. All user-visible strings come from i18n ICU files (English is the first-run default; Arabic is fully supported and selectable). Never hard-code interface text. CI must catch missing translations instead of silently leaking another language. Localized brand names must use BRAND_CONFIG.
9. Every error has a code from `errors.toml` (`SF-<DOMAIN>-<NNN>`). No silent failures.
10. UI: LTR by default for English and RTL for Arabic, with both directions fully supported; Tailwind CSS v4 design tokens only, light and dark both, WCAG 2.2 AA, every drag has a click alternative, respect `prefers-reduced-motion`. Supervised by **UI/UX Pro Max Skill** and **I-Have-ADHD Skill**.
11. Security: strict CSP (no `unsafe-inline`, no `unsafe-eval`); sandbox file parsers with memory and time limits; neutralize CSV/Excel formula injection on export; never execute formulas or macros. Audited in every slice by **Cloudflare Security Audit Skill** and **Strix AI Pentesting**.
12. Determinism: same file + same recipe = byte-identical output.
13. Never modify the user's original file. Exports are always new files, written atomically (temp file -> `fsync` -> rename).

## Locked Stack (Zero Next.js, Zero Supabase)
- **Engine & PDF**: Rust workspace (`crates/*`) compiled to WASM (Web Worker) and Native (Tauri 2.2 + CLI). Powered by Apache Arrow, Polars (`LazyFrame`), DuckDB-WASM, `rust_decimal`, and embedded **Typst** for instant offline Arabic vector PDF generation.
- **Frontend & Local Storage**: Vite 6 + React 19 + TypeScript 5.8 (strict) + TanStack Router + TanStack Query v5 + Zustand v5 + **OPFS (`wa-sqlite`)** + **Glide Data Grid** (Canvas 120 FPS RTL). Marketing site: **Astro 5** + **DQNamo** live WASM playground.
- **Motion & Graphs**: **GSAP 3.12** + **Lenis Smooth Scroll** + **React Bits** + **Prompt Motion** + **Graphify** (for Lineage N11, table relations U10, and visual recipes) + Vega-Lite RTL Spec.
- **Server & Database**: Rust (**Axum 0.8 + SQLx 0.8** compile-time checked SQL) + native **PostgreSQL 18** (built-in `uuidv7()`, Async I/O, virtual generated columns, B-Tree Skip Scan, native RLS) + **PgBouncer** + `pg_partman` + **Valkey 8**.
- **Observability & Billing**: OpenTelemetry + **Vector** + **VictoriaLogs** + Grafana + self-hosted GlitchTip. Billing: **Paymob** (Egypt) + **Lemon Squeezy** (Global MoR).
- **AI & Agent Orchestration**: **Claudex Loop** + **Ruflo Swarm** + **Vercel Skills** + **Anthropic Knowledge Work Plugins** + **Smart Task Router** + **Headroom** (context compression) + **Codeburn** + **WebLLM** + **Awesome Free LLM APIs** / **No-Cost AI**.

## Workflow per task (Claudex Loop)
1. Read `docs/tasks/<id>.md` and referenced spec files.
2. Write `docs/plans/<id>.md` BEFORE coding.
3. Implement directly on `main` with small conventional commits; do not create task branches.
4. Run full local gate (`make check` including **Codeburn**, **Cloudflare Security Skill**, and **Strix**).
5. Write `docs/reports/<id>.md` in simple Arabic using the owner report template.
6. STOP and wait for owner acceptance.

## Owner report template (write in Arabic)
- ما الذي أُنجز (3 أسطر)
- كيف تجرّبه بنفسك (خطوات مرقّمة بسيطة جدًا)
- ما الذي فشل أو لم يكتمل (بصراحة)
- قرارات أحتاجها منك
- الملفات التي تغيّرت
