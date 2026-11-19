# SafiData Desktop 1.0 — Owner Security Acceptance Report

- **Repository:** `mo7amedrabei14-cell/Safi` | **Branch:** `main` | **Baseline HEAD:** `da5c9c4f997158078aee05c3eee04f24af7f44aa`
- **Owner Auth Model:** Immutable Internal `user_id` (`platform_owner` singleton) + Standard Server-Validated RFC 6238 TOTP (Google Authenticator compatible)

---

## 1. Architectural Guarantees Verified

1. **Zero Hardcoded Owner Email:** No personal or owner email exists anywhere in `config/business.json`, Rust crates, TypeScript sources, tests, or compiled `apps/web/dist` bundles.
2. **One-Time Sealed Bootstrap:** `sf-server --bootstrap-owner` reads server-only `SAFI_OWNER_BOOTSTRAP_EMAIL`, confirms the account exists, is active, and has verified email, inserts the singleton `platform_owner` row (`CHECK (singleton = TRUE)`), and permanently seals bootstrap. Any subsequent attempt returns `SF-AUTH-017` (`409 Conflict`).
3. **Immutable `owner_user_id` Authorization:** All Owner authorization (`owner_mfa.rs`, `auth/identity.rs`, `accounts.rs`, `zero_trust.rs`, `routes.rs`) checks `session.principal.user_id == platform_owner.owner_user_id`. Email rotation preserves Owner status; registering the old email creates a separate UUID with zero Owner privileges.
4. **Google Authenticator RFC 6238 TOTP:**
   - 160-bit CSPRNG secret encoded in RFC 4648 Base32 + standard `otpauth://totp/SafiData:...` URI.
   - Encrypted at rest with `ring::aead::CHACHA20_POLY1305` using `SAFI_TOTP_ENCRYPTION_KEY` + key versioning (`key_version`), zeroized immediately after verification.
   - 6 digits, 30-second period, ±1 step skew window, constant-time code comparison (`subtle::ConstantTimeEq`).
   - Strict anti-replay: `last_accepted_step` rejects any reuse of a code in the same or earlier 30-second window.
   - Brute-force lockout: 5 failed attempts trigger a 15-minute lockout (`SF-AUTH-015`, HTTP `429`), during which even valid codes are rejected.
   - 10 single-use recovery codes stored as BLAKE3 hashes; each works once and is permanently invalidated upon consumption.
   - Elevated Owner session TTL: 30 minutes (`ELEVATED_SESSION_TTL_SECS`); Step-up TOTP TTL for high-risk actions (Friend VIP, suspend/reactivate, trial extension, cancel, refund, maintenance mode): 5 minutes (`STEP_UP_TTL_SECS`, `SF-AUTH-016`).
5. **Desktop & UI Separation:** Public browser Showcase never mounts Owner navigation or `OwnerDashboard`; Desktop mounts Owner tab only after backend login confirms `is_owner: true`, and immediately gates it behind Google Authenticator TOTP verification.

---

## 2. Mandatory 14-Case Attack Suite Results

All 14 attack scenarios are automated and passing in `crates/sf-server/src/owner_mfa.rs` (`mandatory_owner_security_attack_suite_all_14_cases`) and `crates/sf-server/tests/zero_trust_guard.rs` (`owner_mfa_control_plane_email_change_and_billing_http_flow`):

| # | Attack / Security Test | Expected | Actual Result |
| :--- | :--- | :--- | :--- |
| 1 | Unverified account bootstrap attempt | Reject `SF-AUTH-017` | PASS |
| 2 | Repeated bootstrap after sealing | Reject `SF-AUTH-017` | PASS |
| 3 | Regular user calling `/v1/owner/*` or MFA enroll | Reject `403 SF-AUTH-013` | PASS |
| 4 | Unauthenticated caller or fake legacy YubiKey/tunnel headers | Reject `401` | PASS |
| 5 | Owner without enrolled/confirmed TOTP calling Owner endpoints | Reject `401 SF-AUTH-014` | PASS |
| 6 | Stored TOTP secret inspection | Encrypted ciphertext only, zero plaintext | PASS |
| 7 | Replay of same valid TOTP code in same 30s window | Reject `401 SF-AUTH-014` | PASS |
| 8 | Expired TOTP code outside ±1 step window | Reject `401 SF-AUTH-014` | PASS |
| 9 | Elevated session expiry after 30 minutes | Reject `401 SF-AUTH-014` | PASS |
| 10 | High-risk action after 5-minute step-up expiry | Reject `401 SF-AUTH-016` | PASS |
| 11 | Recovery code single-use & replay rejection | 1st succeeds, 2nd rejected `SF-AUTH-014` | PASS |
| 12 | Repeated wrong TOTP codes (5 attempts) | Lockout `429 SF-AUTH-015` | PASS |
| 13 | Valid TOTP presented during active lockout | Still rejected `429 SF-AUTH-015` | PASS |
| 14 | Owner email changed & old email registered by attacker + missing encryption key | Owner kept; old email gets `403 SF-AUTH-013`; missing key fails closed `SF-AUTH-018` | PASS |
