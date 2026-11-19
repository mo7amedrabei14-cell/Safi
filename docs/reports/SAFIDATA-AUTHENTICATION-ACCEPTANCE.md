# SafiData Desktop 1.0 — Authentication & Account Lifecycle Acceptance Report

- **Repository:** `mo7amedrabei14-cell/Safi` | **Branch:** `main` | **Baseline HEAD:** `da5c9c4f997158078aee05c3eee04f24af7f44aa`
- **Status:** `READY_FOR_MANUAL_SETUP` (core auth code & tests complete; live SMTP, SMS, WhatsApp, and OAuth provider credentials remain `NOT_CONFIGURED`)

---

## 1. Implemented & Verified Capabilities

| Flow | Endpoint / Module | Verification Evidence | External Dependency Status |
| :--- | :--- | :--- | :--- |
| Sign Up + 8-char password floor + Argon2id | `POST /v1/auth/register` (`auth/account.rs`) | Rust unit + Axum route tests (`axum_register_verify_and_login_routes_issue_existing_session`, `password_floor_accepts_eight_characters_and_rejects_seven`) | SMTP `NOT_CONFIGURED` |
| Duplicate Account Prevention (Email & Phone) | `auth/identity.rs`, `auth/account.rs`, `apps/web/src/lib/auth-rules.ts` | Verified in Rust (`duplicate_registration_is_rejected_and_does_not_send_twice`) and Vitest (`auth-rules.test.ts`) | None (DB unique index in migrations `0001`, `0008`) |
| Email Verification & Resend | `POST /v1/auth/email/verify`, `/v1/auth/verify-email`, `/v1/auth/email/resend` | Verified one-time token consumption, expiry, replay rejection | SMTP `NOT_CONFIGURED` |
| Password Reset | `POST /v1/auth/password/forgot`, `/v1/auth/password/reset` | Verified token rotation, single-use, session revocation on reset | SMTP `NOT_CONFIGURED` |
| Sign In + Single Active Device Binding | `POST /v1/auth/login`, `GET/DELETE /v1/auth/sessions/current`, `X-Safi-Installation-Id` | Verified in `tests/auth_flow.rs` (11/11), `sf-desktop` (`native_auth_gate_enforces_session_device_expiry_revocation_logout_and_ipc_limits`), and Chromium IndexedDB (`verify-device-identity.mjs`) | PostgreSQL + Valkey `NOT_CONFIGURED` in prod |
| Change Email Preserving Immutable `user_id` | `POST /v1/auth/email/change` | Verified in `tests/zero_trust_guard.rs` (Owner identity survives email change; old email grants zero privileges) | PostgreSQL `NOT_CONFIGURED` |
| Account Suspension Enforcement | `POST /v1/owner/accounts/{id}/suspend`, `login_account`, `whoami` | Verified `SF-AUTH-009` enforcement & audit logging | None |
| Phone OTP (SMS Primary + WhatsApp Fallback) | `POST /v1/auth/phone/request-otp`, `/v1/auth/phone/verify-otp`, migration `0008` | Attempt cap (5), 5-min expiry, BLAKE3 hashed OTP, Egyptian `+20`/`0` normalization, fail-closed in strict env when unconfigured | SMS & WhatsApp `NOT_CONFIGURED` |
| OAuth (Google / Apple / GitHub) | `GET /v1/auth/oauth/{provider}/start`, migration `0008` (`oauth_identities`) | PKCE + redirect URI validation; explicitly returns `SF-AUTH-012` (`503`) when client credentials are `NOT_CONFIGURED` | Google, Apple, GitHub OAuth `NOT_CONFIGURED` |
| Rate Limiting | `check_rate_limit` in `routes.rs` | Sliding-window request caps returning `SF-AUTH-010` (`429`) | Valkey recommended for multi-node prod |

## 2. Privacy & Security Invariants

- Session tokens (`sfs_*`) are 256-bit random values held in memory only (never `localStorage` or cookies); only BLAKE3 hashes are stored server-side.
- Installation device identity is stored in browser/webview IndexedDB (`safidata-installation`), never `localStorage` (`localStorageUsed: false` verified in Chromium).
- Debug formatting for login requests, tokens, and API keys prints `[REDACTED]`.
- Unknown email, wrong password, unverified account, and deleted account return unified `SF-AUTH-001` (`401`) to prevent account enumeration.
