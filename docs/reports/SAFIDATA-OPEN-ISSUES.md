# SafiData Desktop 1.0 — Open Issues & Deferred Manual Items

- **Repository:** `mo7amedrabei14-cell/Safi` | **Branch:** `main` | **Baseline HEAD:** `da5c9c4f997158078aee05c3eee04f24af7f44aa`
- **Software Blockers Remaining in Local Scope:** **0**
- **External / Manual Setup Items Blocking Live Public Launch:** All tracked in `docs/reports/SAFIDATA-FINAL-MANUAL-SETUP-MAP.md` (`NOT_CONFIGURED` by design in local-only mode).

---

## 1. Non-Blocking Technical Observations & Follow-Ups

| ID | Severity | Description | Current State & Mitigation |
| :--- | :--- | :--- | :--- |
| OPEN-01 | Info (External) | Live PostgreSQL 18 integration tests (`account_postgres.rs`, `accounts_postgres.rs`) are `#[ignore]` when `SF_TEST_DATABASE_URL` is absent locally. | Migrations `0001..0009` passed static/RLS verification (`verify-migrations.py`); run the 3 ignored Postgres tests against a live PostgreSQL 18 instance during manual staging setup (Item 02). |
| OPEN-02 | Info (External) | Live external providers (SMTP, SMS, WhatsApp, Google/Apple/GitHub OAuth, Paddle, Paymob, Cloud AI, OTLP/GlitchTip, Windows EV Authenticode signing) are `NOT_CONFIGURED` locally. | All adapters fail closed with canonical error codes (`SF-SYS-032`, `SF-PAY-001`, `SF-AUTH-012`, `SF-AIX-001`) and are mapped step-by-step in `SAFIDATA-FINAL-MANUAL-SETUP-MAP.md`. |
| OPEN-03 | Info (External) | Independent third-party live penetration test & production load soak test against real PostgreSQL/Valkey infrastructure. | Local unit/concurrency/attack suites pass (14 Owner MFA attacks, 12-way quota concurrency, Paddle/Paymob HMAC/replay); run external pen-test once staging is deployed. |
| OPEN-04 | Low (React Warning) | `CleaningStudio` emits a non-fatal React list `key` warning during SSR test rendering in `studios.test.tsx` (pre-existing). | All 231 Vitest tests pass; clean up list key prop in a future polish slice. |
| OPEN-05 | Info (Scope) | Mobile platforms (Android/iOS/mobile sync) and Enterprise SSO/SCIM IdP wiring. | Explicitly deferred out of Desktop 1.0 scope; SSO/SCIM endpoint returns `SF-SYN-006` (`NOT_CONFIGURED`). |
| OPEN-06 | Info (Persistence Wiring) | New `sf-server` services (`owner_mfa`, `entitlements`, `billing`, `organizations`, `encrypted_sync`, `log_center`) have full PostgreSQL 18 schema migrations (`0007..0009`) plus thread-safe runtime stores; `platform_owner` is loaded from PostgreSQL on startup in `main.rs`, while full SQLx repository implementations for the newly added tables (`owner_totp_credentials`, `owner_audit_log`, `monthly_usage_counters`, `organizations`, `encrypted_sync_records`) should be bound to `PgPool` before multi-replica horizontal scaling. | Single-node deployment & `--bootstrap-owner` work cleanly; bind SQLx repository adapters to migrations `0007..0009` prior to multi-instance production rollout. |
