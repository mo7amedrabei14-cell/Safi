# SafiData Desktop 1.0 — Final Manual Setup Map

- **Repository:** `mo7amedrabei14-cell/Safi` | **Branch:** `main` | **Baseline HEAD:** `da5c9c4f997158078aee05c3eee04f24af7f44aa`
- **Status:** `READY_FOR_MANUAL_SETUP`
- **Security Rule:** This document contains **ZERO** real secrets, **ZERO** real API keys, **ZERO** real private keys, and **ZERO** personal/owner email addresses. Replace every placeholder strictly on your deployment host or secrets manager.

---

## 1. Strict Dependency Order (Execute Top to Bottom)

1. **Category 1 — Domain, DNS & TLS** (Establishes `https://safidata.app` and `https://api.safidata.app`).
2. **Category 2 — PostgreSQL 18 + PgBouncer + Migrations** (Primary durable state + RLS + `platform_owner` table).
3. **Category 3 — Valkey 8** (Distributed cache & session eviction acceleration; mandatory in `production`).
4. **Category 4 — Cryptography & Ed25519 / TOTP Keys** (Mandatory fail-closed startup keys before starting `sf-server`).
5. **Category 5 — Transactional Email (SMTP)** (Required to verify the Owner's initial account before sealing bootstrap).
6. **Category 6 — One-Time Sealed Platform Owner Bootstrap + Google Authenticator TOTP** (Run once via CLI, then remove `SAFI_OWNER_BOOTSTRAP_EMAIL`).
7. **Category 7 — Phone OTP (SMS Primary + WhatsApp Fallback)** (Unlocks 7-day Trial phone verification).
8. **Category 8 — OAuth Providers (Google / Apple / GitHub)** (Optional social sign-in).
9. **Category 9 — Billing & Payments (Paddle International + Paymob Egypt)** (Unlocks paid checkout & live webhooks).
10. **Category 10 — External AI Provider (Optional)** (Unlocks cloud AI assist behind consent + local PII masking).
11. **Category 11 — Observability & Self-Hosted Error Tracking (OTLP / VictoriaLogs / GlitchTip)**.
12. **Category 12 — Desktop Packaging, Windows Code Signing & Release Distribution**.

---

## 2. Detailed Manual Configuration Entries (All 12 Categories)

### Item 01 — Domain, DNS & HTTPS Reverse Proxy (Category 1)
1. **Item Name:** Primary Domains (`safidata.app`, `api.safidata.app`) & TLS Termination
2. **Category:** Infrastructure & Networking
3. **Priority:** P0 (Launch Blocker)
4. **Why It Is Needed:** Packaged Desktop builds enforce strict HTTPS `SAFI_API_BASE_URL` and reject localhost/IP/placeholder hosts (`SF-SYS-031`).
5. **Service / Provider:** Registrar + DNS (Cloudflare/Hetzner DNS) + TLS Reverse Proxy (Caddy/Nginx/Cloudflare)
6. **Where to Obtain / Configure:** Domain registrar DNS panel and European production server reverse proxy.
7. **Exact Environment Variables / Secrets:** `SAFI_API_BASE_URL`, `SAFI_APP_PUBLIC_URL`, `SAFI_CORS_ORIGINS`
8. **Exact Files / Locations Consuming It:** `.env`, `crates/sf-server/src/config.rs`, `crates/sf-server/src/cors.rs`, `apps/desktop/src-tauri/build.rs`, `apps/web/vite.config.ts`
9. **Example Format (Placeholder Only):** `SAFI_API_BASE_URL=https://api.safidata.app`, `SAFI_APP_PUBLIC_URL=https://safidata.app`, `SAFI_CORS_ORIGINS=https://safidata.app,tauri://localhost,https://tauri.localhost`
10. **Required Permissions / Scopes / Webhooks:** DNS A/AAAA records + valid TLS 1.3 certificate + HTTP->HTTPS redirect + HSTS.
11. **Step-by-Step Setup Instructions:** (a) Point `api.safidata.app` to backend host; (b) Issue Let's Encrypt/TLS cert; (c) Proxy `/v1/*`, `/health`, `/ready` to `sf-server` on `127.0.0.1:8080`.
12. **Local vs Staging vs Production Notes:** Local dev allows `http://127.0.0.1:8080`; staging and production require strict `https://`.
13. **Dependency Order:** Step 1 (prerequisite for SMTP links, OAuth callbacks, webhooks, and Desktop release builds).
14. **Verification Command / Proof:** `curl -i https://api.safidata.app/health`
15. **Expected Passing Outcome:** HTTP `200 OK` JSON with `"status":"ok"` and `X-Request-ID` header.
16. **Failure Symptoms If Missing:** Packaged Desktop build refuses to compile (`SF-SYS-031`) or cannot connect.
17. **Security Precautions:** Enforce TLS 1.2+/1.3, never enable `unsafe-inline` CSP, disable directory listing.
18. **Rotation / Recovery Procedure:** Automated ACME renewal every 60 days; keep DNS registrar protected with TOTP.
19. **Rollback Notes:** Point DNS back to maintenance/static page if backend fails health checks.
20. **Current Status:** `NOT_CONFIGURED`
21. **Can Be Tested Locally Before Live Setup?:** Yes (loopback dev server).
22. **Linked Acceptance Report:** `docs/reports/SAFIDATA-DESKTOP-RELEASE-ACCEPTANCE.md`

---

### Item 02 — PostgreSQL 18 + PgBouncer + Schema Migrations (Category 2)
1. **Item Name:** Managed/Self-Hosted PostgreSQL 18 Database & RLS App Role
2. **Category:** Primary Database
3. **Priority:** P0 (Launch Blocker)
4. **Why It Is Needed:** Persists users, sessions, devices, `platform_owner`, encrypted TOTP secrets, audit chain, entitlements, billing events, organizations, and encrypted sync records.
5. **Service / Provider:** PostgreSQL 18 (Europe region) + PgBouncer transaction pooling
6. **Where to Obtain / Configure:** Provision PostgreSQL 18 instance and create a `NOBYPASSRLS` application role.
7. **Exact Environment Variables / Secrets:** `DATABASE_URL`
8. **Exact Files / Locations Consuming It:** `crates/sf-server/src/main.rs`, `crates/sf-server/src/config.rs`, `crates/sf-server/migrations/0001..0009`
9. **Example Format (Placeholder Only):** `DATABASE_URL=postgres://safi_app:CHANGE_ME_STRONG_PASSWORD@127.0.0.1:6432/safidata?sslmode=require`
10. **Required Permissions / Scopes / Webhooks:** Admin role for applying migrations `0001..0009`; restricted `NOBYPASSRLS` role (`safi_app`) for runtime queries.
11. **Step-by-Step Setup Instructions:** (a) Create DB `safidata`; (b) Apply `crates/sf-server/migrations/0001..0009_*.up.sql` in numerical order; (c) Configure PgBouncer; (d) Set `DATABASE_URL` in server `.env`.
12. **Local vs Staging vs Production Notes:** Never share DB across staging and production; enable automated daily backups via `tools/ops/backup-postgres.sh`.
13. **Dependency Order:** Step 2 (must complete before Owner bootstrap and server startup).
14. **Verification Command / Proof:** `SF_TEST_DATABASE_URL=... cargo test -p sf-server --test account_postgres --test accounts_postgres -- --ignored` and `node tools/ops/validate-env.mjs`
15. **Expected Passing Outcome:** All migrations applied, RLS enforced, `postgresql: "CONFIGURED"`.
16. **Failure Symptoms If Missing:** `sf-server` fails closed at startup with `SF-SYS-032`.
17. **Security Precautions:** Require `sslmode=require`, least-privilege `NOBYPASSRLS` role, encrypted off-host backups.
18. **Rotation / Recovery Procedure:** Rotate DB password via dual-role window; test restore quarterly using `tools/ops/restore-postgres.sh`.
19. **Rollback Notes:** Each migration has a verified reversible `*.down.sql` file (`tools/verify-migrations.py`).
20. **Current Status:** `NOT_CONFIGURED`
21. **Can Be Tested Locally Before Live Setup?:** Yes (local Docker PostgreSQL 18 container).
22. **Linked Acceptance Report:** `docs/reports/SAFIDATA-AUTHENTICATION-ACCEPTANCE.md`

---

### Item 03 — Valkey 8 Distributed Cache (Category 3)
1. **Item Name:** Valkey 8 Cache & Session Eviction Store
2. **Category:** Cache / Rate-Limit State
3. **Priority:** P0 (Launch Blocker in Production)
4. **Why It Is Needed:** Accelerates API-key verification and immediate session revocation eviction; production startup fails closed if absent (zero silent in-memory fallback).
5. **Service / Provider:** Valkey 8 (self-hosted or European managed instance)
6. **Where to Obtain / Configure:** Provision Valkey 8 bound to private VPC/loopback with strong ACL password.
7. **Exact Environment Variables / Secrets:** `SAFI_VALKEY_URL`
8. **Exact Files / Locations Consuming It:** `crates/sf-server/src/main.rs`, `crates/sf-server/src/config.rs`, `crates/sf-server/src/auth/cache.rs`
9. **Example Format (Placeholder Only):** `SAFI_VALKEY_URL=redis://:CHANGE_ME_VALKEY_PASSWORD@127.0.0.1:6379/0`
10. **Required Permissions / Scopes / Webhooks:** Private network only; disable public port `6379`.
11. **Step-by-Step Setup Instructions:** (a) Install Valkey 8; (b) Set `requirepass` / ACL user; (c) Bind private interface; (d) Set `SAFI_VALKEY_URL`.
12. **Local vs Staging vs Production Notes:** Optional in `SAFI_ENV=development|test`; strictly mandatory in `staging|production`.
13. **Dependency Order:** Step 3.
14. **Verification Command / Proof:** `sf-server --check-config` and `GET /ready`
15. **Expected Passing Outcome:** `"valkey": "CONFIGURED"` and server connects without `SF-AUTH-005`.
16. **Failure Symptoms If Missing:** `sf-server` exits with code 1 and `SF-SYS-032`.
17. **Security Precautions:** Never expose port `6379` to public internet; enable TLS if traversing network hops.
18. **Rotation / Recovery Procedure:** Flush cache safely on password rotation (PostgreSQL remains authoritative).
19. **Rollback Notes:** Restart Valkey; PostgreSQL session verification guarantees zero security bypass on cold cache.
20. **Current Status:** `NOT_CONFIGURED`
21. **Can Be Tested Locally Before Live Setup?:** Yes (local Valkey/Redis container).
22. **Linked Acceptance Report:** `docs/reports/SAFIDATA-FINAL-SECURITY-AUDIT.md`

---

### Item 04 — Core Server Cryptographic Keys (TOTP, License Ed25519, Update Ed25519, Token HMAC) (Category 4)
1. **Item Name:** Four 32-Byte Hex Cryptographic Master Keys
2. **Category:** Cryptography & Signing
3. **Priority:** P0 (Launch Blocker)
4. **Why It Is Needed:** Encrypts Owner TOTP secrets at rest (`ChaCha20-Poly1305`), signs offline Desktop licenses (`Ed25519`), signs Desktop release manifests (`Ed25519`), and signs internal OAuth/state tokens.
5. **Service / Provider:** Operator-generated CSPRNG keys stored in server vault / secrets manager
6. **Where to Obtain / Configure:** Generate 4 distinct 32-byte random keys locally using OpenSSL (`openssl rand -hex 32`) or PowerShell `RandomNumberGenerator`.
7. **Exact Environment Variables / Secrets:** `SAFI_TOTP_ENCRYPTION_KEY`, `SAFI_TOTP_KEY_VERSION`, `SAFI_LICENSE_SIGNING_PRIVATE_KEY`, `SAFI_UPDATE_SIGNING_PRIVATE_KEY`, `SAFI_TOKEN_SIGNING_KEY`
8. **Exact Files / Locations Consuming It:** `crates/sf-server/src/config.rs`, `owner_mfa.rs`, `updater.rs`, `routes_updates.rs`
9. **Example Format (Placeholder Only):** `SAFI_TOTP_ENCRYPTION_KEY=<64-hex-chars>` (64 lowercase hex characters = 32 bytes each)
10. **Required Permissions / Scopes / Webhooks:** Server-process read-only secret storage; NEVER embed private keys in frontend, git, or desktop binaries.
11. **Step-by-Step Setup Instructions:** (a) Generate 4 unique 64-hex keys; (b) Store in production secrets manager; (c) Extract Ed25519 public verifying keys for Desktop release verification.
12. **Local vs Staging vs Production Notes:** MUST use completely separate keys per environment.
13. **Dependency Order:** Step 4 (mandatory before starting `sf-server` in production).
14. **Verification Command / Proof:** `SAFI_ENV=production node tools/ops/validate-env.mjs` and `sf-server --check-config`
15. **Expected Passing Outcome:** `"ready": true`, `"owner_totp_encryption": "CONFIGURED"`, `"ed25519_license_signing": "CONFIGURED"`, `"ed25519_update_signing": "CONFIGURED"`.
16. **Failure Symptoms If Missing:** Startup fails closed with `SF-SYS-032`; TOTP enrollment returns `SF-AUTH-018`; updates/licenses return `503`.
17. **Security Precautions:** Keep offline encrypted backup of Ed25519 signing seeds; rotate `SAFI_TOTP_ENCRYPTION_KEY` by incrementing `SAFI_TOTP_KEY_VERSION`.
18. **Rotation / Recovery Procedure:** Re-encrypt `owner_totp_credentials` under new `key_version`; ship new Ed25519 public key in an update before retiring old update key.
19. **Rollback Notes:** Keep previous key version accessible during zero-downtime rotation window.
20. **Current Status:** `NOT_CONFIGURED`
21. **Can Be Tested Locally Before Live Setup?:** Yes (deterministic non-prod test keys in `.env.test.example`).
22. **Linked Acceptance Report:** `docs/reports/SAFIDATA-OWNER-SECURITY-ACCEPTANCE.md`

---

### Item 05 — Transactional Email (SMTP) (Category 5)
1. **Item Name:** Outbound Transactional SMTP Service (Verification & Password Reset)
2. **Category:** Messaging & Email
3. **Priority:** P0 (Launch Blocker)
4. **Why It Is Needed:** Delivers one-time email verification and password reset links via `lettre` TLS SMTP.
5. **Service / Provider:** Postmark / Resend / AWS SES / Mailgun / Brevo (with SPF, DKIM, DMARC configured on `safidata.app`)
6. **Where to Obtain / Configure:** Provider dashboard -> SMTP credentials + domain DNS TXT records (SPF/DKIM/DMARC).
7. **Exact Environment Variables / Secrets:** `SAFI_SMTP_HOST`, `SAFI_SMTP_PORT`, `SAFI_SMTP_USERNAME`, `SAFI_SMTP_PASSWORD`, `SAFI_SMTP_FROM`
8. **Exact Files / Locations Consuming It:** `crates/sf-server/src/auth/account.rs`, `crates/sf-server/src/main.rs`, `crates/sf-server/src/config.rs`
9. **Example Format (Placeholder Only):** `SAFI_SMTP_HOST=smtp.provider.example`, `SAFI_SMTP_PORT=587`, `SAFI_SMTP_FROM=SafiData <no-reply@safidata.app>`
10. **Required Permissions / Scopes / Webhooks:** Send-only SMTP credential; verified sending domain.
11. **Step-by-Step Setup Instructions:** (a) Verify `safidata.app` DNS SPF/DKIM/DMARC; (b) Create SMTP user/password; (c) Populate env vars; (d) Test registration + verification delivery.
12. **Local vs Staging vs Production Notes:** Use sandbox inbox (e.g., Mailpit) in staging; live verified domain in production.
13. **Dependency Order:** Step 5 (required before live account registration & Owner bootstrap).
14. **Verification Command / Proof:** Register test account via `POST /v1/auth/register` and verify email arrives with valid `/#verify_email=...` link.
15. **Expected Passing Outcome:** Email delivered cleanly without spam flagging; `POST /v1/auth/email/verify` returns `200 OK`.
16. **Failure Symptoms If Missing:** Server logs `SF-AUTH-005` on startup or delivery; users cannot verify new accounts.
17. **Security Precautions:** Enforce STARTTLS/TLS, never log verification tokens or recipient emails (`AGENTS.md` Rule 4).
18. **Rotation / Recovery Procedure:** Generate new SMTP password in provider panel, update secret, restart server.
19. **Rollback Notes:** Switch `SAFI_SMTP_HOST` to secondary provider if primary experiences outage.
20. **Current Status:** `NOT_CONFIGURED`
21. **Can Be Tested Locally Before Live Setup?:** Yes (`InMemoryMailer` in unit tests; Mailpit locally).
22. **Linked Acceptance Report:** `docs/reports/SAFIDATA-AUTHENTICATION-ACCEPTANCE.md`

---

### Item 06 — One-Time Sealed Platform Owner Bootstrap & Google Authenticator Enrollment (Category 6)
1. **Item Name:** Immutable Platform Owner Seal (`platform_owner`) + Google Authenticator TOTP
2. **Category:** Owner Identity & Privileged Access
3. **Priority:** P0 (Launch Blocker)
4. **Why It Is Needed:** Permanently binds the single Platform Owner's immutable `users.id` UUID in PostgreSQL without ever hardcoding an email in source code, then protects Owner Control Plane with RFC 6238 TOTP.
5. **Service / Provider:** Self-hosted `sf-server --bootstrap-owner` CLI + free standard Google Authenticator mobile app
6. **Where to Obtain / Configure:** Operator SSH/container shell on production server + Google Authenticator app on Owner's phone.
7. **Exact Environment Variables / Secrets:** `SAFI_OWNER_BOOTSTRAP_EMAIL` (TEMPORARY — remove immediately after sealing!), `SAFI_OWNER_TOTP_ISSUER=SafiData`
8. **Exact Files / Locations Consuming It:** `crates/sf-server/src/main.rs`, `crates/sf-server/src/owner_mfa.rs`, `migrations/0007_owner_mfa_and_audit.up.sql`
9. **Example Format (Placeholder Only):** `SAFI_OWNER_BOOTSTRAP_EMAIL=<your-verified-owner-email>`
10. **Required Permissions / Scopes / Webhooks:** Shell access to run one-time CLI against PostgreSQL; account must already exist and have `email_verified_at IS NOT NULL`.
11. **Step-by-Step Setup Instructions:** (a) Register Owner account normally and complete email verification; (b) Run `SAFI_OWNER_BOOTSTRAP_EMAIL="<owner-email>" sf-server --bootstrap-owner` ONCE; (c) Immediately unset/delete `SAFI_OWNER_BOOTSTRAP_EMAIL` from environment; (d) Sign into Desktop app -> Owner tab -> click `Enroll Google Authenticator` -> scan/enter secret in Google Authenticator -> save all 10 one-time recovery codes offline -> submit 6-digit code to confirm.
12. **Local vs Staging vs Production Notes:** Bootstrap separately per environment; never copy TOTP secrets between staging and production.
13. **Dependency Order:** Step 6 (requires Items 02, 04, 05).
14. **Verification Command / Proof:** Re-run `sf-server --bootstrap-owner` and confirm it fails closed with `SF-AUTH-017`; verify `GET /v1/owner/stats` returns `401 SF-AUTH-014` before TOTP and `200 OK` after TOTP.
15. **Expected Passing Outcome:** Single row in `platform_owner`, TOTP confirmed, 10 recovery codes stored offline, second bootstrap rejected.
16. **Failure Symptoms If Missing:** No user receives `is_owner: true`; all `/v1/owner/*` routes return `403 SF-AUTH-013`.
17. **Security Precautions:** Never store recovery codes in cloud notes or git; keep physical/offline encrypted copy; never reuse a TOTP code.
18. **Rotation / Recovery Procedure:** If phone is lost, unlock with one of the 10 offline recovery codes; if all recovery codes are lost, require direct DBA + out-of-band identity reset on `owner_totp_credentials`.
19. **Rollback Notes:** Never unseal `platform_owner` automatically; manual DBA intervention only.
20. **Current Status:** `NOT_CONFIGURED`
21. **Can Be Tested Locally Before Live Setup?:** Yes (fully verified in `owner_mfa.rs` and `tests/zero_trust_guard.rs`).
22. **Linked Acceptance Report:** `docs/reports/SAFIDATA-OWNER-SECURITY-ACCEPTANCE.md`

---

### Item 07 — Phone Verification Providers (SMS Primary + WhatsApp Fallback) (Category 7)
1. **Item Name:** SMS OTP Primary Provider + WhatsApp OTP Fallback Provider
2. **Category:** Trial Anti-Abuse & Phone Verification
3. **Priority:** P1 (Required to enable 7-Day Free Trial in production)
4. **Why It Is Needed:** Enforces verified phone uniqueness for the 7-day no-card trial (Egypt + international).
5. **Service / Provider:** Twilio / Vonage / MessageBird / VictoryLink (SMS) + Meta WhatsApp Cloud API / Twilio WhatsApp (fallback)
6. **Where to Obtain / Configure:** Provider API console -> sender ID & API key.
7. **Exact Environment Variables / Secrets:** `SAFI_SMS_PROVIDER`, `SAFI_SMS_API_KEY`, `SAFI_SMS_SENDER_ID`, `SAFI_WHATSAPP_PROVIDER`, `SAFI_WHATSAPP_API_KEY`, `SAFI_WHATSAPP_FROM`
8. **Exact Files / Locations Consuming It:** `crates/sf-server/src/config.rs`, `crates/sf-server/src/routes.rs` (`/v1/auth/phone/*`)
9. **Example Format (Placeholder Only):** `SAFI_SMS_PROVIDER=twilio`, `SAFI_SMS_API_KEY=CHANGE_ME`, `SAFI_WHATSAPP_PROVIDER=meta_cloud`
10. **Required Permissions / Scopes / Webhooks:** Outbound transactional OTP permission; approved sender ID/template.
11. **Step-by-Step Setup Instructions:** (a) Register SMS sender ID & WhatsApp OTP template; (b) Set env vars; (c) Verify `/ready` reports `CONFIGURED`; (d) Test `POST /v1/auth/phone/request-otp` and `verify-otp`.
12. **Local vs Staging vs Production Notes:** Strict prod returns `503` when unconfigured; dev/test returns explicit `provider_status: "NOT_CONFIGURED"`.
13. **Dependency Order:** Step 7.
14. **Verification Command / Proof:** `GET /ready` -> `sms_phone_otp: "CONFIGURED"`, `whatsapp_phone_otp: "CONFIGURED"`.
15. **Expected Passing Outcome:** OTP arrives via SMS (or WhatsApp fallback), verified phone saved to `users.verified_phone`.
16. **Failure Symptoms If Missing:** Trial phone OTP endpoint returns `503` in strict production mode.
17. **Security Precautions:** Rate limit OTP requests per phone/IP; never log OTP codes.
18. **Rotation / Recovery Procedure:** Rotate provider API keys every 90 days.
19. **Rollback Notes:** Switch primary/fallback channel if one carrier degrades.
20. **Current Status:** `NOT_CONFIGURED`
21. **Can Be Tested Locally Before Live Setup?:** Yes (local test mode validates hashing, 5-attempt cap, expiry, and Egyptian phone normalization).
22. **Linked Acceptance Report:** `docs/reports/SAFIDATA-AUTHENTICATION-ACCEPTANCE.md`

---

### Item 08 — OAuth Identity Providers (Google / Apple / GitHub) (Category 8)
1. **Item Name:** OAuth 2.0 / OIDC Client Registrations (Google, Apple, GitHub)
2. **Category:** Social Authentication
3. **Priority:** P2 (Optional at launch; email/password works independently)
4. **Why It Is Needed:** Enables "Continue with Google / Apple / GitHub" buttons in Desktop Auth portal with PKCE + state verification.
5. **Service / Provider:** Google Cloud Console, Apple Developer Portal (Sign in with Apple), GitHub OAuth Apps
6. **Where to Obtain / Configure:** Each provider's OAuth client configuration dashboard.
7. **Exact Environment Variables / Secrets:** `SAFI_OAUTH_GOOGLE_CLIENT_ID`, `SAFI_OAUTH_GOOGLE_CLIENT_SECRET`, `SAFI_OAUTH_GOOGLE_REDIRECT_URI`, `SAFI_OAUTH_APPLE_CLIENT_ID`, `SAFI_OAUTH_APPLE_TEAM_ID`, `SAFI_OAUTH_APPLE_KEY_ID`, `SAFI_OAUTH_APPLE_PRIVATE_KEY_PEM`, `SAFI_OAUTH_APPLE_REDIRECT_URI`, `SAFI_OAUTH_GITHUB_CLIENT_ID`, `SAFI_OAUTH_GITHUB_CLIENT_SECRET`, `SAFI_OAUTH_GITHUB_REDIRECT_URI`
8. **Exact Files / Locations Consuming It:** `crates/sf-server/src/config.rs`, `crates/sf-server/src/routes.rs` (`/v1/auth/oauth/{provider}/start`)
9. **Example Format (Placeholder Only):** `SAFI_OAUTH_GOOGLE_REDIRECT_URI=https://api.safidata.app/v1/auth/oauth/google/callback`
10. **Required Permissions / Scopes / Webhooks:** Minimal scopes (`openid email profile` / `user:email`).
11. **Step-by-Step Setup Instructions:** (a) Create OAuth app per provider; (b) Register exact HTTPS callback URL; (c) Populate env vars; (d) Verify `GET /v1/auth/oauth/{provider}/start`.
12. **Local vs Staging vs Production Notes:** Separate OAuth apps for staging vs production.
13. **Dependency Order:** Step 8 (requires Item 01 HTTPS domain).
14. **Verification Command / Proof:** `GET /ready` shows `oauth_google/apple/github: "CONFIGURED"`.
15. **Expected Passing Outcome:** `/v1/auth/oauth/{provider}/start` returns valid state & `CONFIGURED`.
16. **Failure Symptoms If Missing:** Endpoint fails closed with `503 SF-AUTH-012`.
17. **Security Precautions:** Always enforce PKCE + random state + exact redirect URI allowlist.
18. **Rotation / Recovery Procedure:** Rotate client secrets in provider console & server env.
19. **Rollback Notes:** Remove env vars to cleanly return `SF-AUTH-012` without affecting email/password sign-in.
20. **Current Status:** `NOT_CONFIGURED`
21. **Can Be Tested Locally Before Live Setup?:** Yes (fail-closed `SF-AUTH-012` verified locally).
22. **Linked Acceptance Report:** `docs/reports/SAFIDATA-AUTHENTICATION-ACCEPTANCE.md`

---

### Item 09 — Billing & Payment Providers: Paddle (International USD) + Paymob (Egypt EGP) (Category 9)
1. **Item Name:** Paddle Billing (MoR USD) & Paymob Accept (Egypt EGP) Live Merchant Accounts + Webhooks
2. **Category:** Billing & Monetization
3. **Priority:** P0 (Required to accept real paid subscriptions/credits)
4. **Why It Is Needed:** Processes real international USD checkouts/subscriptions/refunds via Paddle and Egyptian EGP card/wallet payments via Paymob with cryptographic webhook verification.
5. **Service / Provider:** Paddle Billing (International MoR) + Paymob (Egypt)
6. **Where to Obtain / Configure:** Paddle Dashboard (Catalog -> Prices, Developer Tools -> API Keys & Notifications) and Paymob Dashboard (Developers -> API Keys, Payment Integrations, HMAC Secret).
7. **Exact Environment Variables / Secrets:** `SAFI_PADDLE_ENV`, `SAFI_PADDLE_API_KEY`, `SAFI_PADDLE_WEBHOOK_SECRET`, `SAFI_PADDLE_PRICE_INDIVIDUAL_MONTHLY`, `SAFI_PADDLE_PRICE_INDIVIDUAL_ANNUAL`, `SAFI_PADDLE_PRICE_ORG_SEAT_ANNUAL`, `SAFI_PADDLE_PRICE_PAY_PER_FILE`, `SAFI_PADDLE_PRICE_CREDIT_PACK_25`, `SAFI_PAYMOB_ENV`, `SAFI_PAYMOB_API_KEY`, `SAFI_PAYMOB_SECRET_KEY`, `SAFI_PAYMOB_PUBLIC_KEY`, `SAFI_PAYMOB_HMAC_SECRET`, `SAFI_PAYMOB_INTEGRATION_ID_CARD`, `SAFI_PAYMOB_INTEGRATION_ID_WALLET`
8. **Exact Files / Locations Consuming It:** `crates/sf-server/src/billing/mod.rs`, `crates/sf-server/src/config.rs`, `crates/sf-server/src/routes.rs` (`/v1/billing/*`)
9. **Example Format (Placeholder Only):** `SAFI_PADDLE_ENV=live`, `SAFI_PADDLE_WEBHOOK_SECRET=pdl_ntfset_CHANGE_ME`, `SAFI_PAYMOB_HMAC_SECRET=CHANGE_ME_HEX`
10. **Required Permissions / Scopes / Webhooks:** Register webhook endpoints `https://api.safidata.app/v1/billing/webhooks/paddle` (events: `subscription.*`, `transaction.*`, `adjustment.refunded`) and `https://api.safidata.app/v1/billing/webhooks/paymob`.
11. **Step-by-Step Setup Instructions:** (a) Create 5 catalog prices matching `config/business.json` ($20/1000 EGP, $168/8400 EGP, $204/10200 EGP org seat min 3, $2/50 EGP pay-per-file, $5/250 EGP credit pack 25); (b) Configure sandbox first and run full test checkout/renewal/cancel/refund; (c) Switch to live keys after verifying sandbox webhooks.
12. **Local vs Staging vs Production Notes:** ALWAYS validate in Paddle Sandbox & Paymob Test Mode before live credentials.
13. **Dependency Order:** Step 9 (requires Items 01, 02).
14. **Verification Command / Proof:** `GET /v1/billing/status` returns `{"paddle":"CONFIGURED","paymob":"CONFIGURED"}`; send test webhook from provider dashboard and verify `200 OK` + event visible in Owner Control Plane `/v1/owner/billing/events`.
15. **Expected Passing Outcome:** Valid signed webhooks update entitlements atomically; tampered signatures return `401 SF-PAY-002`.
16. **Failure Symptoms If Missing:** Checkout & webhooks fail closed with `503 SF-PAY-001`.
17. **Security Precautions:** Never disable HMAC verification; enforce 5-min timestamp replay window on Paddle; keep raw secrets strictly in server env.
18. **Rotation / Recovery Procedure:** Rotate webhook secrets during low-traffic window; re-deliver failed events from provider dashboard (idempotency guarantees zero duplicate charges).
19. **Rollback Notes:** Unset provider keys to pause checkout cleanly without breaking existing active entitlements.
20. **Current Status:** `NOT_CONFIGURED`
21. **Can Be Tested Locally Before Live Setup?:** Yes (all 13 webhook/HMAC/replay/idempotency/refund tests verified in `billing/mod.rs` and `zero_trust_guard.rs`).
22. **Linked Acceptance Report:** `docs/reports/SAFIDATA-BILLING-AND-ENTITLEMENTS-ACCEPTANCE.md`

---

### Item 10 — External AI Provider (Optional Cloud AI Assist) (Category 10)
1. **Item Name:** OpenAI-Compatible External AI Endpoint & API Key
2. **Category:** AI Assistance
3. **Priority:** P2 (Optional — offline/local engine & structured DSL work fully without it)
4. **Why It Is Needed:** Powers optional cloud natural-language AI routing after explicit user consent and local on-device PII masking.
5. **Service / Provider:** OpenAI-compatible provider (or self-hosted vLLM/Ollama on private GPU host)
6. **Where to Obtain / Configure:** Provider API console -> project-scoped key with monthly spend cap.
7. **Exact Environment Variables / Secrets:** `SAFI_AI_PROVIDER_URL`, `SAFI_AI_API_KEY`, `SAFI_AI_MODEL`
8. **Exact Files / Locations Consuming It:** `crates/sf-server/src/config.rs`, `crates/sf-server/src/routes.rs` (`/v1/ai/route`)
9. **Example Format (Placeholder Only):** `SAFI_AI_PROVIDER_URL=https://api.provider.example/v1`, `SAFI_AI_API_KEY=CHANGE_ME`, `SAFI_AI_MODEL=gpt-4o-mini`
10. **Required Permissions / Scopes / Webhooks:** Inference-only API key with zero-data-retention (ZDR) enabled.
11. **Step-by-Step Setup Instructions:** (a) Enable provider zero-data-retention; (b) Set hard billing cap; (c) Populate 3 env vars; (d) Verify `/ready` shows `ai_provider: "CONFIGURED"`.
12. **Local vs Staging vs Production Notes:** Disabled (`NOT_CONFIGURED`) by default until configured.
13. **Dependency Order:** Step 10.
14. **Verification Command / Proof:** `POST /v1/ai/route` with consent & masked flag returns `200 OK` and decrements AI quota.
15. **Expected Passing Outcome:** AI quota enforced (0 Free, 25 Trial, 400 Paid/Friend); missing consent rejected with `422`.
16. **Failure Symptoms If Missing:** Endpoint returns `503 SF-AIX-001` honestly without fabricating answers.
17. **Security Precautions:** Never disable local PII masking (`config/business.json` `local_pii_masking_before_send: true`); never allow AI output to bypass Rust DSL validator (`SF-AIX-002`).
18. **Rotation / Recovery Procedure:** Rotate API key quarterly or immediately on anomaly.
19. **Rollback Notes:** Unset `SAFI_AI_API_KEY` to revert cleanly to offline deterministic mode.
20. **Current Status:** `NOT_CONFIGURED`
21. **Can Be Tested Locally Before Live Setup?:** Yes (consent gate, quota cap, and `SF-AIX-001` verified).
22. **Linked Acceptance Report:** `docs/reports/SAFIDATA-MASTER-COMPLETION-REPORT.md`

---

### Item 11 — Observability, Structured Logging & Self-Hosted Error Tracking (Category 11)
1. **Item Name:** OpenTelemetry Collector (OTLP) + VictoriaLogs + Grafana + Self-Hosted GlitchTip
2. **Category:** Observability & Operations
3. **Priority:** P1 (Strongly recommended before public launch)
4. **Why It Is Needed:** Monitors latency, error rates, uptime, and scrubbed crash reports while obeying `AGENTS.md` Rule 4 (zero user cell values, file names, or column names in logs).
5. **Service / Provider:** Self-hosted VictoriaLogs + Grafana + GlitchTip (Europe host)
6. **Where to Obtain / Configure:** Self-hosted observability stack deployment.
7. **Exact Environment Variables / Secrets:** `SAFI_OTLP_ENDPOINT`, `SAFI_GLITCHTIP_DSN`, `SAFI_LOG_FORMAT=json`
8. **Exact Files / Locations Consuming It:** `crates/sf-server/src/config.rs`, `.env.example`
9. **Example Format (Placeholder Only):** `SAFI_OTLP_ENDPOINT=http://127.0.0.1:4317`, `SAFI_GLITCHTIP_DSN=https://CHANGE_ME@errors.safidata.app/1`
10. **Required Permissions / Scopes / Webhooks:** Ingest-only DSN/endpoint on private network.
11. **Step-by-Step Setup Instructions:** (a) Deploy GlitchTip + VictoriaLogs/Grafana; (b) Configure alert rules on 5xx spikes, DB/Valkey disconnects, and failed Owner MFA lockouts; (c) Set env vars.
12. **Local vs Staging vs Production Notes:** Optional locally; configure before live public traffic.
13. **Dependency Order:** Step 11.
14. **Verification Command / Proof:** `GET /ready` shows `observability_otlp: "CONFIGURED"`, `error_tracking_glitchtip: "CONFIGURED"`.
15. **Expected Passing Outcome:** Traces/logs/alerts arrive with `X-Request-ID` and zero user data.
16. **Failure Symptoms If Missing:** Server runs normally, reporting `NOT_CONFIGURED` in `/ready`.
17. **Security Precautions:** Restrict Grafana/GlitchTip dashboards behind VPN + MFA; verify PII scrubbing on sample error payload.
18. **Rotation / Recovery Procedure:** Rotate GlitchTip DSN if leaked.
19. **Rollback Notes:** Clear env vars if collector causes backpressure.
20. **Current Status:** `NOT_CONFIGURED`
21. **Can Be Tested Locally Before Live Setup?:** Yes (`json` logs locally).
22. **Linked Acceptance Report:** `docs/reports/SAFIDATA-FINAL-SECURITY-AUDIT.md`

---

### Item 12 — Desktop Packaging, Windows Authenticode Signing & Release Hosting (Category 12)
1. **Item Name:** Windows Code Signing Certificate (EV / Azure Trusted Signing) + Final Release Build & Update Hosting
2. **Category:** Desktop Release Distribution
3. **Priority:** P0 (Required before public Windows installer distribution)
4. **Why It Is Needed:** Prevents Windows SmartScreen warnings, guarantees executable authenticity (`SF-UPD-007`/`SF-UPD-008`), and embeds live `SAFI_API_BASE_URL` + Ed25519 public verifying keys.
5. **Service / Provider:** DigiCert / Sectigo / Microsoft Azure Trusted Signing + HTTPS CDN/Release Storage
6. **Where to Obtain / Configure:** Certificate Authority enrollment + Windows `signtool.exe` / Tauri bundler configuration.
7. **Exact Environment Variables / Secrets:** `SAFI_API_BASE_URL=https://api.safidata.app`, `SAFI_DESKTOP_RELEASE_BUILD=1`, `WINDOWS_SIGNING_CERT_THUMBPRINT` (or Azure Trusted Signing env vars)
8. **Exact Files / Locations Consuming It:** `apps/desktop/src-tauri/tauri.conf.json`, `apps/desktop/src-tauri/build.rs`, `apps/web/vite.config.ts`, `crates/sf-desktop/src/signing.rs`
9. **Example Format (Placeholder Only):** `SAFI_API_BASE_URL=https://api.safidata.app SAFI_DESKTOP_RELEASE_BUILD=1 pnpm --filter @safi/web run build`
10. **Required Permissions / Scopes / Webhooks:** Code-signing HSM / Trusted Signing signer role + RFC 3161 timestamp server URL.
11. **Step-by-Step Setup Instructions:** (a) Build `@safi/web` with live `SAFI_API_BASE_URL`; (b) Build Tauri release installer (`msi` / `nsis`); (c) Sign installer & `safi-desktop.exe` with RFC 3161 timestamp; (d) Compute SHA-256 checksum & sign update manifest with server Ed25519 key; (e) Test clean install, launch, login, and uninstall on a clean Windows VM.
12. **Local vs Staging vs Production Notes:** Local unsigned release binary already verified via `verify-desktop-build-config.mjs`; signing required only for external release distribution.
13. **Dependency Order:** Step 12 (final step after Items 01–11).
14. **Verification Command / Proof:** `signtool verify /pa /v safi-desktop.exe` and `node tools/verify-desktop-build-config.mjs`
15. **Expected Passing Outcome:** Valid Authenticode signature with trusted timestamp + matching SHA-256 + valid Ed25519 update manifest.
16. **Failure Symptoms If Missing:** Windows SmartScreen warns users about unknown publisher on unsigned builds.
17. **Security Precautions:** Never export signing private key to developer laptops or Git; use HSM or cloud signing service.
18. **Rotation / Recovery Procedure:** Renew cert 60 days prior to expiry; RFC 3161 timestamps keep previously signed binaries valid.
19. **Rollback Notes:** Keep previous signed installer available and raise `min_required_version` only after staged rollout health confirms boot stability.
20. **Current Status:** `NOT_CONFIGURED`
21. **Can Be Tested Locally Before Live Setup?:** Unsigned release binary build & verification already tested locally (PASS).
22. **Linked Acceptance Report:** `docs/reports/SAFIDATA-DESKTOP-RELEASE-ACCEPTANCE.md`

---

## 3. Configuration Completeness Audit

- Every environment variable referenced in `crates/sf-server/src/config.rs`, `main.rs`, `apps/desktop/src-tauri/build.rs`, and `.env.example` is accounted for in Items 01–12 above.
- Automated pre-flight check commands available right now:
  - `node tools/ops/validate-env.mjs`
  - `cargo run -p sf-server -- --check-config`
  - `GET /ready`
