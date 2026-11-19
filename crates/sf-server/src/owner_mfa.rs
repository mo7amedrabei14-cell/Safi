//! Platform Owner immutable identity, one-time sealed bootstrap, and genuine
//! Google Authenticator-compatible RFC 6238 TOTP MFA with encrypted secret storage,
//! anti-replay, rate limiting/lockout, hashed single-use recovery codes, and
//! time-bound elevated/step-up sessions.
//!
//! Security invariants:
//! - Owner authorization NEVER checks an email address. It checks ONLY the immutable
//!   `owner_user_id: Uuid` sealed in `platform_owner`.
//! - Changing the owner's email preserves owner identity; registering the old email
//!   grants ZERO owner privileges.
//! - TOTP secrets are encrypted at rest with ChaCha20-Poly1305 (`SAFI_TOTP_ENCRYPTION_KEY`)
//!   with key versioning and zeroized after use.
//! - Replaying the same TOTP code within the same 30-second window is rejected.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use hmac::{Hmac, Mac};
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, CHACHA20_POLY1305, NONCE_LEN};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use subtle::ConstantTimeEq;
use tokio::sync::RwLock;
use uuid::Uuid;
use zeroize::Zeroize;

use crate::auth::unix_now;
use crate::error::AuthError;

type HmacSha1 = Hmac<Sha1>;

/// RFC 6238 TOTP period in seconds (standard Google Authenticator = 30s).
pub const TOTP_PERIOD_SECS: i64 = 30;
/// RFC 6238 TOTP digits (standard Google Authenticator = 6).
pub const TOTP_DIGITS: u32 = 6;
/// Allowed clock skew in steps (±1 step = ±30s).
pub const TOTP_SKEW_STEPS: i64 = 1;
/// Maximum consecutive failed MFA attempts before lockout.
pub const MAX_MFA_FAILED_ATTEMPTS: i32 = 5;
/// Lockout duration in seconds after repeated failed attempts (15 minutes).
pub const MFA_LOCKOUT_SECS: i64 = 15 * 60;
/// Elevated Owner session lifetime in seconds (30 minutes).
pub const ELEVATED_SESSION_TTL_SECS: i64 = 30 * 60;
/// Step-up TOTP validity window for high-risk actions in seconds (5 minutes).
pub const STEP_UP_TTL_SECS: i64 = 5 * 60;
/// Number of one-time recovery codes generated on enrollment.
pub const RECOVERY_CODE_COUNT: usize = 10;

const TOTP_AAD: &[u8] = b"safidata.owner_totp.v1";
const BASE32_ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// Compute RFC 6238 / RFC 4226 6-digit TOTP code for a given raw secret and time step.
pub fn compute_totp_code(secret: &[u8], step: i64) -> Result<String, AuthError> {
    if secret.is_empty() || step < 0 {
        return Err(AuthError::OwnerMfaRequired);
    }
    let counter = (step as u64).to_be_bytes();
    let mut mac = HmacSha1::new_from_slice(secret).map_err(|_| AuthError::OwnerMfaRequired)?;
    mac.update(&counter);
    let digest = mac.finalize().into_bytes();
    let offset = (digest[19] & 0x0f) as usize;
    let binary = ((u32::from(digest[offset]) & 0x7f) << 24)
        | (u32::from(digest[offset + 1]) << 16)
        | (u32::from(digest[offset + 2]) << 8)
        | u32::from(digest[offset + 3]);
    let code = binary % 10u32.pow(TOTP_DIGITS);
    Ok(format!("{code:06}"))
}

/// Encode bytes as RFC 4648 Base32 (no padding) for Google Authenticator compatibility.
#[must_use]
pub fn encode_base32_nopad(input: &[u8]) -> String {
    let mut out = String::with_capacity((input.len() * 8).div_ceil(5));
    let mut buffer: u32 = 0;
    let mut bits_left: u32 = 0;
    for &byte in input {
        buffer = (buffer << 8) | u32::from(byte);
        bits_left += 8;
        while bits_left >= 5 {
            bits_left -= 5;
            let idx = ((buffer >> bits_left) & 0x1f) as usize;
            out.push(BASE32_ALPHABET[idx] as char);
        }
    }
    if bits_left > 0 {
        let idx = ((buffer << (5 - bits_left)) & 0x1f) as usize;
        out.push(BASE32_ALPHABET[idx] as char);
    }
    out
}

/// Decode RFC 4648 Base32 (case-insensitive, ignores spaces/hyphens and optional '=').
pub fn decode_base32(input: &str) -> Result<Vec<u8>, AuthError> {
    let mut out = Vec::new();
    let mut buffer: u32 = 0;
    let mut bits_left: u32 = 0;
    for ch in input.bytes() {
        if ch == b'=' || ch == b' ' || ch == b'-' {
            continue;
        }
        let val: u32 = match ch.to_ascii_uppercase() {
            b'A'..=b'Z' => u32::from(ch.to_ascii_uppercase() - b'A'),
            b'2'..=b'7' => u32::from(ch - b'2') + 26,
            _ => return Err(AuthError::InvalidRequest),
        };
        buffer = (buffer << 5) | val;
        bits_left += 5;
        if bits_left >= 8 {
            bits_left -= 8;
            out.push(((buffer >> bits_left) & 0xff) as u8);
        }
    }
    Ok(out)
}

/// Encrypt a raw TOTP secret for database persistence using ChaCha20-Poly1305.
pub fn encrypt_totp_secret(key: &[u8; 32], secret: &[u8]) -> Result<(Vec<u8>, Vec<u8>), AuthError> {
    let rng = SystemRandom::new();
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rng.fill(&mut nonce_bytes)
        .map_err(|_| AuthError::BackendUnavailable)?;
    let unbound =
        UnboundKey::new(&CHACHA20_POLY1305, key).map_err(|_| AuthError::OwnerTotpKeyMissing)?;
    let sealing_key = LessSafeKey::new(unbound);
    let mut ciphertext = secret.to_vec();
    sealing_key
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce_bytes),
            Aad::from(TOTP_AAD),
            &mut ciphertext,
        )
        .map_err(|_| AuthError::BackendUnavailable)?;
    Ok((ciphertext, nonce_bytes.to_vec()))
}

/// Decrypt an encrypted TOTP secret from persistence.
pub fn decrypt_totp_secret(
    key: &[u8; 32],
    ciphertext: &[u8],
    nonce: &[u8],
) -> Result<Vec<u8>, AuthError> {
    if nonce.len() != NONCE_LEN {
        return Err(AuthError::OwnerTotpKeyMissing);
    }
    let mut nonce_bytes = [0u8; NONCE_LEN];
    nonce_bytes.copy_from_slice(nonce);
    let unbound =
        UnboundKey::new(&CHACHA20_POLY1305, key).map_err(|_| AuthError::OwnerTotpKeyMissing)?;
    let opening_key = LessSafeKey::new(unbound);
    let mut buf = ciphertext.to_vec();
    let plaintext = opening_key
        .open_in_place(
            Nonce::assume_unique_for_key(nonce_bytes),
            Aad::from(TOTP_AAD),
            &mut buf,
        )
        .map_err(|_| AuthError::OwnerTotpKeyMissing)?;
    Ok(plaintext.to_vec())
}

fn hash_recovery_code(code: &str) -> String {
    let normalized: String = code
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    blake3::hash(format!("safi:owner-recovery:v1:{normalized}").as_bytes())
        .to_hex()
        .to_string()
}

fn generate_recovery_codes() -> Result<(Vec<String>, Vec<String>), AuthError> {
    let rng = SystemRandom::new();
    let mut plaintexts = Vec::with_capacity(RECOVERY_CODE_COUNT);
    let mut hashes = Vec::with_capacity(RECOVERY_CODE_COUNT);
    for _ in 0..RECOVERY_CODE_COUNT {
        let mut raw = [0u8; 6];
        rng.fill(&mut raw)
            .map_err(|_| AuthError::BackendUnavailable)?;
        let hex_code = hex::encode_upper(raw);
        let formatted = format!(
            "{}-{}-{}",
            &hex_code[0..4],
            &hex_code[4..8],
            &hex_code[8..12]
        );
        hashes.push(hash_recovery_code(&formatted));
        plaintexts.push(formatted);
    }
    Ok((plaintexts, hashes))
}

/// Sealed record identifying the single immutable platform Owner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformOwnerRecord {
    pub owner_user_id: Uuid,
    pub bootstrap_sealed: bool,
    pub bootstrapped_at: i64,
}

/// Stored encrypted TOTP credential (never holds plaintext secret).
#[derive(Clone, Serialize, Deserialize)]
pub struct StoredTotpCredential {
    pub user_id: Uuid,
    pub secret_ciphertext: Vec<u8>,
    pub secret_nonce: Vec<u8>,
    pub key_version: i32,
    pub confirmed: bool,
    pub last_accepted_step: Option<i64>,
    pub failed_attempts: i32,
    pub locked_until: Option<i64>,
    pub enrolled_at: i64,
    pub confirmed_at: Option<i64>,
}

impl fmt::Debug for StoredTotpCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StoredTotpCredential")
            .field("user_id", &self.user_id)
            .field("key_version", &self.key_version)
            .field("confirmed", &self.confirmed)
            .field("failed_attempts", &self.failed_attempts)
            .field("locked_until", &self.locked_until)
            .finish()
    }
}

/// Stored hashed single-use recovery code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredRecoveryCode {
    pub id: Uuid,
    pub user_id: Uuid,
    pub code_hash: String,
    pub used_at: Option<i64>,
}

/// Time-bound elevated Owner session state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElevatedOwnerSession {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub verified_at: i64,
    pub expires_at: i64,
    pub last_step_up_at: i64,
}

/// One-time enrollment bundle returned ONLY when the Owner starts TOTP setup.
#[derive(Clone, Serialize)]
pub struct TotpEnrollmentBundle {
    pub otpauth_uri: String,
    pub manual_secret_base32: String,
    pub issuer: String,
    pub account_label: String,
    pub period_secs: i64,
    pub digits: u32,
    pub recovery_codes: Vec<String>,
}

impl fmt::Debug for TotpEnrollmentBundle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TotpEnrollmentBundle")
            .field("issuer", &self.issuer)
            .field("manual_secret_base32", &"[REDACTED]")
            .field("recovery_codes_count", &self.recovery_codes.len())
            .finish()
    }
}

/// Public MFA status for the authenticated Owner (never contains secrets).
#[derive(Debug, Clone, Serialize)]
pub struct OwnerMfaStatus {
    pub owner_bootstrapped: bool,
    pub is_owner: bool,
    pub totp_enrolled: bool,
    pub totp_confirmed: bool,
    pub elevated_session_active: bool,
    pub elevated_expires_at: Option<i64>,
    pub step_up_fresh: bool,
    pub recovery_codes_remaining: usize,
    pub locked_until: Option<i64>,
}

/// Serializable snapshot of Owner MFA state (TOTP secrets remain ChaCha20-Poly1305 encrypted).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OwnerMfaSnapshot {
    pub owner: Option<PlatformOwnerRecord>,
    pub totp: Vec<StoredTotpCredential>,
    pub recovery_codes: Vec<StoredRecoveryCode>,
    pub elevated_sessions: Vec<ElevatedOwnerSession>,
}

#[derive(Default)]
struct OwnerMfaState {
    owner: Option<PlatformOwnerRecord>,
    totp: HashMap<Uuid, StoredTotpCredential>,
    recovery_codes: Vec<StoredRecoveryCode>,
    elevated_sessions: HashMap<Uuid, ElevatedOwnerSession>,
}

/// Authoritative Owner identity & RFC 6238 TOTP MFA manager.
#[derive(Clone)]
pub struct OwnerMfaService {
    state: Arc<RwLock<OwnerMfaState>>,
    encryption_key: Option<[u8; 32]>,
    key_version: i32,
    issuer: String,
}

impl fmt::Debug for OwnerMfaService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OwnerMfaService")
            .field("key_configured", &self.encryption_key.is_some())
            .field("key_version", &self.key_version)
            .field("issuer", &self.issuer)
            .finish()
    }
}

impl OwnerMfaService {
    #[must_use]
    pub fn new(encryption_key: Option<[u8; 32]>, key_version: i32, issuer: String) -> Self {
        Self {
            state: Arc::new(RwLock::new(OwnerMfaState::default())),
            encryption_key,
            key_version,
            issuer,
        }
    }

    fn require_encryption_key(&self) -> Result<&[u8; 32], AuthError> {
        self.encryption_key
            .as_ref()
            .ok_or(AuthError::OwnerTotpKeyMissing)
    }

    /// One-time sealed bootstrap of the immutable Platform Owner by `user_id`.
    /// Any second attempt ALWAYS fails closed with `SF-AUTH-017`.
    pub async fn bootstrap_owner_once(
        &self,
        owner_user_id: Uuid,
        email_verified: bool,
    ) -> Result<PlatformOwnerRecord, AuthError> {
        if !email_verified {
            return Err(AuthError::OwnerBootstrapSealed);
        }
        let mut guard = self.state.write().await;
        if guard.owner.is_some() {
            return Err(AuthError::OwnerBootstrapSealed);
        }
        let record = PlatformOwnerRecord {
            owner_user_id,
            bootstrap_sealed: true,
            bootstrapped_at: unix_now(),
        };
        guard.owner = Some(record.clone());
        Ok(record)
    }

    /// Check whether a `user_id` is the sealed immutable Platform Owner.
    /// NEVER uses email comparison.
    pub async fn is_owner_user(&self, user_id: Uuid) -> bool {
        let guard = self.state.read().await;
        guard
            .owner
            .as_ref()
            .is_some_and(|record| record.owner_user_id == user_id)
    }

    /// Return the sealed Platform Owner record (if bootstrapped).
    pub async fn platform_owner(&self) -> Option<PlatformOwnerRecord> {
        self.state.read().await.owner.clone()
    }

    /// Enforce that `user_id` is the sealed Owner, returning `SF-AUTH-013` otherwise.
    pub async fn require_owner_identity(&self, user_id: Uuid) -> Result<(), AuthError> {
        if self.is_owner_user(user_id).await {
            Ok(())
        } else {
            Err(AuthError::OwnerRequired)
        }
    }

    /// Start Google Authenticator-compatible TOTP enrollment for the verified Owner.
    /// Returns the `otpauth://` URI, manual Base32 secret, and 10 one-time recovery codes.
    pub async fn enroll_totp(
        &self,
        user_id: Uuid,
        account_label: &str,
    ) -> Result<TotpEnrollmentBundle, AuthError> {
        self.require_owner_identity(user_id).await?;
        let key = self.require_encryption_key()?;

        let mut guard = self.state.write().await;
        if let Some(existing) = guard.totp.get(&user_id) {
            if existing.confirmed {
                // Re-enrollment requires explicit reset/rotation after step-up, never silent overwrite.
                return Err(AuthError::InvalidRequest);
            }
        }

        let rng = SystemRandom::new();
        let mut raw_secret = [0u8; 20]; // 160-bit RFC 6238 secret
        rng.fill(&mut raw_secret)
            .map_err(|_| AuthError::BackendUnavailable)?;

        let base32_secret = encode_base32_nopad(&raw_secret);
        let (ciphertext, nonce) = encrypt_totp_secret(key, &raw_secret)?;
        raw_secret.zeroize();

        let (recovery_plaintexts, recovery_hashes) = generate_recovery_codes()?;

        guard.totp.insert(
            user_id,
            StoredTotpCredential {
                user_id,
                secret_ciphertext: ciphertext,
                secret_nonce: nonce,
                key_version: self.key_version,
                confirmed: false,
                last_accepted_step: None,
                failed_attempts: 0,
                locked_until: None,
                enrolled_at: unix_now(),
                confirmed_at: None,
            },
        );
        guard.recovery_codes.retain(|c| c.user_id != user_id);
        for hash in recovery_hashes {
            guard.recovery_codes.push(StoredRecoveryCode {
                id: Uuid::now_v7(),
                user_id,
                code_hash: hash,
                used_at: None,
            });
        }

        let label = format!("{}:{}", self.issuer, account_label.trim());
        let otpauth_uri = format!(
            "otpauth://totp/{}?secret={}&issuer={}&algorithm=SHA1&digits={}&period={}",
            urlencoding_minimal(&label),
            base32_secret,
            urlencoding_minimal(&self.issuer),
            TOTP_DIGITS,
            TOTP_PERIOD_SECS
        );

        Ok(TotpEnrollmentBundle {
            otpauth_uri,
            manual_secret_base32: base32_secret,
            issuer: self.issuer.clone(),
            account_label: account_label.trim().to_owned(),
            period_secs: TOTP_PERIOD_SECS,
            digits: TOTP_DIGITS,
            recovery_codes: recovery_plaintexts,
        })
    }

    /// Confirm TOTP enrollment by verifying a valid live 6-digit code at `now_secs`.
    /// Immediately elevates `session_id` on success.
    pub async fn confirm_totp_at(
        &self,
        user_id: Uuid,
        session_id: Uuid,
        code: &str,
        now_secs: i64,
    ) -> Result<ElevatedOwnerSession, AuthError> {
        self.verify_totp_internal(user_id, session_id, code, now_secs, true)
            .await
    }

    /// Verify a live 6-digit TOTP code to create or refresh an elevated Owner session (and step-up).
    pub async fn verify_totp_at(
        &self,
        user_id: Uuid,
        session_id: Uuid,
        code: &str,
        now_secs: i64,
    ) -> Result<ElevatedOwnerSession, AuthError> {
        self.verify_totp_internal(user_id, session_id, code, now_secs, false)
            .await
    }

    async fn verify_totp_internal(
        &self,
        user_id: Uuid,
        session_id: Uuid,
        code: &str,
        now_secs: i64,
        confirming: bool,
    ) -> Result<ElevatedOwnerSession, AuthError> {
        self.require_owner_identity(user_id).await?;
        let key = self.require_encryption_key()?;

        let trimmed = code.trim();
        if trimmed.len() != TOTP_DIGITS as usize || !trimmed.bytes().all(|b| b.is_ascii_digit()) {
            self.record_failed_attempt(user_id, now_secs).await;
            return Err(AuthError::OwnerMfaRequired);
        }

        let mut guard = self.state.write().await;
        let cred = guard
            .totp
            .get_mut(&user_id)
            .ok_or(AuthError::OwnerMfaRequired)?;

        if !confirming && !cred.confirmed {
            return Err(AuthError::OwnerMfaRequired);
        }
        if let Some(locked_until) = cred.locked_until {
            if now_secs < locked_until {
                return Err(AuthError::OwnerMfaLocked);
            }
            cred.locked_until = None;
            cred.failed_attempts = 0;
        }

        let mut secret = decrypt_totp_secret(key, &cred.secret_ciphertext, &cred.secret_nonce)?;
        let current_step = now_secs / TOTP_PERIOD_SECS;
        let mut matched_step: Option<i64> = None;

        for offset in -TOTP_SKEW_STEPS..=TOTP_SKEW_STEPS {
            let candidate_step = current_step + offset;
            if candidate_step < 0 {
                continue;
            }
            let expected = compute_totp_code(&secret, candidate_step)?;
            if trimmed.as_bytes().ct_eq(expected.as_bytes()).unwrap_u8() == 1 {
                matched_step = Some(candidate_step);
                break;
            }
        }
        secret.zeroize();

        let Some(accepted_step) = matched_step else {
            cred.failed_attempts += 1;
            if cred.failed_attempts >= MAX_MFA_FAILED_ATTEMPTS {
                cred.locked_until = Some(now_secs + MFA_LOCKOUT_SECS);
                return Err(AuthError::OwnerMfaLocked);
            }
            return Err(AuthError::OwnerMfaRequired);
        };

        // Strict anti-replay: reject any step <= last_accepted_step.
        if let Some(last) = cred.last_accepted_step {
            if accepted_step <= last {
                cred.failed_attempts += 1;
                if cred.failed_attempts >= MAX_MFA_FAILED_ATTEMPTS {
                    cred.locked_until = Some(now_secs + MFA_LOCKOUT_SECS);
                    return Err(AuthError::OwnerMfaLocked);
                }
                return Err(AuthError::OwnerMfaRequired);
            }
        }

        cred.last_accepted_step = Some(accepted_step);
        cred.failed_attempts = 0;
        cred.locked_until = None;
        if confirming {
            cred.confirmed = true;
            cred.confirmed_at = Some(now_secs);
        }

        let elevated = ElevatedOwnerSession {
            session_id,
            user_id,
            verified_at: now_secs,
            expires_at: now_secs + ELEVATED_SESSION_TTL_SECS,
            last_step_up_at: now_secs,
        };
        guard.elevated_sessions.insert(session_id, elevated.clone());
        Ok(elevated)
    }

    async fn record_failed_attempt(&self, user_id: Uuid, now_secs: i64) {
        let mut guard = self.state.write().await;
        if let Some(cred) = guard.totp.get_mut(&user_id) {
            cred.failed_attempts += 1;
            if cred.failed_attempts >= MAX_MFA_FAILED_ATTEMPTS {
                cred.locked_until = Some(now_secs + MFA_LOCKOUT_SECS);
            }
        }
    }

    /// Consume a single-use recovery code to establish an elevated Owner session.
    pub async fn verify_recovery_code_at(
        &self,
        user_id: Uuid,
        session_id: Uuid,
        recovery_code: &str,
        now_secs: i64,
    ) -> Result<ElevatedOwnerSession, AuthError> {
        self.require_owner_identity(user_id).await?;
        let candidate_hash = hash_recovery_code(recovery_code);
        let mut guard = self.state.write().await;

        if let Some(cred) = guard.totp.get(&user_id) {
            if let Some(locked_until) = cred.locked_until {
                if now_secs < locked_until {
                    return Err(AuthError::OwnerMfaLocked);
                }
            }
        }

        let mut matched_idx = None;
        for (idx, stored) in guard.recovery_codes.iter().enumerate() {
            if stored.user_id == user_id
                && stored.used_at.is_none()
                && stored
                    .code_hash
                    .as_bytes()
                    .ct_eq(candidate_hash.as_bytes())
                    .unwrap_u8()
                    == 1
            {
                matched_idx = Some(idx);
                break;
            }
        }

        let Some(idx) = matched_idx else {
            if let Some(cred) = guard.totp.get_mut(&user_id) {
                cred.failed_attempts += 1;
                if cred.failed_attempts >= MAX_MFA_FAILED_ATTEMPTS {
                    cred.locked_until = Some(now_secs + MFA_LOCKOUT_SECS);
                    return Err(AuthError::OwnerMfaLocked);
                }
            }
            return Err(AuthError::OwnerMfaRequired);
        };

        guard.recovery_codes[idx].used_at = Some(now_secs);
        if let Some(cred) = guard.totp.get_mut(&user_id) {
            cred.failed_attempts = 0;
            cred.locked_until = None;
        }
        let elevated = ElevatedOwnerSession {
            session_id,
            user_id,
            verified_at: now_secs,
            expires_at: now_secs + ELEVATED_SESSION_TTL_SECS,
            last_step_up_at: now_secs,
        };
        guard.elevated_sessions.insert(session_id, elevated.clone());
        Ok(elevated)
    }

    /// Verify that the caller is the sealed Owner AND holds an active non-expired elevated session.
    pub async fn require_elevated_session_at(
        &self,
        user_id: Uuid,
        session_id: Uuid,
        now_secs: i64,
    ) -> Result<ElevatedOwnerSession, AuthError> {
        self.require_owner_identity(user_id).await?;
        let guard = self.state.read().await;
        let cred = guard
            .totp
            .get(&user_id)
            .ok_or(AuthError::OwnerMfaRequired)?;
        if !cred.confirmed {
            return Err(AuthError::OwnerMfaRequired);
        }
        let session = guard
            .elevated_sessions
            .get(&session_id)
            .ok_or(AuthError::OwnerMfaRequired)?;
        if session.user_id != user_id || now_secs >= session.expires_at {
            return Err(AuthError::OwnerMfaRequired);
        }
        Ok(session.clone())
    }

    /// Enforce fresh step-up TOTP verification for high-risk actions (refunds, Friend VIP, suspend, maintenance).
    pub async fn require_step_up_at(
        &self,
        user_id: Uuid,
        session_id: Uuid,
        now_secs: i64,
    ) -> Result<ElevatedOwnerSession, AuthError> {
        let elevated = self
            .require_elevated_session_at(user_id, session_id, now_secs)
            .await?;
        if now_secs - elevated.last_step_up_at > STEP_UP_TTL_SECS {
            return Err(AuthError::OwnerStepUpRequired);
        }
        Ok(elevated)
    }

    /// Revoke elevated state when a session logs out or is terminated.
    pub async fn revoke_elevated_session(&self, session_id: Uuid) {
        self.state
            .write()
            .await
            .elevated_sessions
            .remove(&session_id);
    }

    /// Return secret-safe Owner MFA status for UI presentation.
    pub async fn status_at(
        &self,
        user_id: Uuid,
        session_id: Uuid,
        now_secs: i64,
    ) -> OwnerMfaStatus {
        let guard = self.state.read().await;
        let owner_bootstrapped = guard.owner.is_some();
        let is_owner = guard
            .owner
            .as_ref()
            .is_some_and(|record| record.owner_user_id == user_id);
        if !is_owner {
            return OwnerMfaStatus {
                owner_bootstrapped,
                is_owner: false,
                totp_enrolled: false,
                totp_confirmed: false,
                elevated_session_active: false,
                elevated_expires_at: None,
                step_up_fresh: false,
                recovery_codes_remaining: 0,
                locked_until: None,
            };
        }

        let cred = guard.totp.get(&user_id);
        let totp_enrolled = cred.is_some();
        let totp_confirmed = cred.is_some_and(|c| c.confirmed);
        let locked_until = cred
            .and_then(|c| c.locked_until)
            .filter(|&ts| ts > now_secs);
        let elevated = guard
            .elevated_sessions
            .get(&session_id)
            .filter(|s| s.user_id == user_id && now_secs < s.expires_at);
        let recovery_codes_remaining = guard
            .recovery_codes
            .iter()
            .filter(|c| c.user_id == user_id && c.used_at.is_none())
            .count();

        OwnerMfaStatus {
            owner_bootstrapped,
            is_owner: true,
            totp_enrolled,
            totp_confirmed,
            elevated_session_active: elevated.is_some(),
            elevated_expires_at: elevated.map(|s| s.expires_at),
            step_up_fresh: elevated
                .is_some_and(|s| now_secs - s.last_step_up_at <= STEP_UP_TTL_SECS),
            recovery_codes_remaining,
            locked_until,
        }
    }

    /// Export Owner MFA state (TOTP secrets remain encrypted at rest).
    pub async fn export_snapshot(&self) -> OwnerMfaSnapshot {
        let guard = self.state.read().await;
        OwnerMfaSnapshot {
            owner: guard.owner.clone(),
            totp: guard.totp.values().cloned().collect(),
            recovery_codes: guard.recovery_codes.clone(),
            elevated_sessions: guard.elevated_sessions.values().cloned().collect(),
        }
    }

    /// Restore Owner MFA state from a durable snapshot.
    pub async fn import_snapshot(&self, snap: OwnerMfaSnapshot) {
        let mut guard = self.state.write().await;
        guard.owner = snap.owner;
        guard.totp = snap.totp.into_iter().map(|c| (c.user_id, c)).collect();
        guard.recovery_codes = snap.recovery_codes;
        guard.elevated_sessions = snap
            .elevated_sessions
            .into_iter()
            .map(|s| (s.session_id, s))
            .collect();
    }

    /// Inspect encrypted storage in tests to verify secret is never stored in plaintext.
    #[cfg(test)]
    pub(crate) async fn stored_credential_for_test(
        &self,
        user_id: Uuid,
    ) -> Option<StoredTotpCredential> {
        self.state.read().await.totp.get(&user_id).cloned()
    }
}

fn urlencoding_minimal(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                out.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_service() -> OwnerMfaService {
        OwnerMfaService::new(Some([0x5Au8; 32]), 1, "SafiData".to_owned())
    }

    #[test]
    fn rfc6238_base32_and_totp_generation_matches_standard_vectors() {
        // RFC 4648 / RFC 6238 SHA-1 test secret "12345678901234567890"
        let secret = b"12345678901234567890";
        let b32 = encode_base32_nopad(secret);
        assert_eq!(b32, "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ");
        assert_eq!(decode_base32(&b32).expect("decode"), secret);

        // RFC 6238 Appendix B: T=59s -> step 1 -> 8-digit 94287082 -> 6-digit 287082
        let code = compute_totp_code(secret, 59 / 30).expect("totp");
        assert_eq!(code, "287082");
    }

    #[tokio::test]
    async fn mandatory_owner_security_attack_suite_all_14_cases() {
        let svc = test_service();
        let owner_id = Uuid::now_v7();
        let regular_user_id = Uuid::now_v7();
        let owner_session = Uuid::now_v7();
        let regular_session = Uuid::now_v7();
        let t0: i64 = 1_760_000_000;

        // 1. Bootstrap fails if email not verified; succeeds once when verified.
        assert_eq!(
            svc.bootstrap_owner_once(owner_id, false).await.unwrap_err(),
            AuthError::OwnerBootstrapSealed
        );
        let bootstrapped = svc
            .bootstrap_owner_once(owner_id, true)
            .await
            .expect("initial bootstrap");
        assert_eq!(bootstrapped.owner_user_id, owner_id);

        // 2. Repeated bootstrap attempt fails closed permanently.
        assert_eq!(
            svc.bootstrap_owner_once(regular_user_id, true)
                .await
                .unwrap_err(),
            AuthError::OwnerBootstrapSealed
        );

        // 3. Regular user calling Owner endpoint / enrollment is rejected with SF-AUTH-013.
        assert_eq!(
            svc.enroll_totp(regular_user_id, "attacker")
                .await
                .unwrap_err(),
            AuthError::OwnerRequired
        );
        assert_eq!(
            svc.require_elevated_session_at(regular_user_id, regular_session, t0)
                .await
                .unwrap_err(),
            AuthError::OwnerRequired
        );

        // 4. Owner without TOTP cannot access Owner endpoints.
        assert_eq!(
            svc.require_elevated_session_at(owner_id, owner_session, t0)
                .await
                .unwrap_err(),
            AuthError::OwnerMfaRequired
        );

        // 5. Enroll TOTP and verify secret is encrypted at rest (never plaintext).
        let bundle = svc
            .enroll_totp(owner_id, "owner-account")
            .await
            .expect("enroll");
        assert!(bundle.otpauth_uri.starts_with("otpauth://totp/"));
        assert_eq!(bundle.recovery_codes.len(), RECOVERY_CODE_COUNT);
        let raw_secret = decode_base32(&bundle.manual_secret_base32).expect("decode secret");
        let stored = svc
            .stored_credential_for_test(owner_id)
            .await
            .expect("stored cred");
        assert!(!stored
            .secret_ciphertext
            .windows(raw_secret.len())
            .any(|w| w == raw_secret.as_slice()));

        // 6. Owner with unconfirmed TOTP is still rejected.
        assert_eq!(
            svc.require_elevated_session_at(owner_id, owner_session, t0)
                .await
                .unwrap_err(),
            AuthError::OwnerMfaRequired
        );

        // Confirm TOTP with valid code at t0.
        let valid_code_t0 = compute_totp_code(&raw_secret, t0 / TOTP_PERIOD_SECS).expect("code");
        svc.confirm_totp_at(owner_id, owner_session, &valid_code_t0, t0)
            .await
            .expect("confirm totp");

        // 7. Replaying the same TOTP code in the same 30-second step is REJECTED!
        assert_eq!(
            svc.verify_totp_at(owner_id, owner_session, &valid_code_t0, t0 + 5)
                .await
                .unwrap_err(),
            AuthError::OwnerMfaRequired
        );

        // 8. Expired TOTP code (outside ±1 step window) is rejected.
        let expired_code =
            compute_totp_code(&raw_secret, (t0 / TOTP_PERIOD_SECS) - 3).expect("old code");
        assert_eq!(
            svc.verify_totp_at(owner_id, owner_session, &expired_code, t0 + 60)
                .await
                .unwrap_err(),
            AuthError::OwnerMfaRequired
        );

        // 9. Elevated session expires after 30 minutes and requires fresh TOTP.
        assert_eq!(
            svc.require_elevated_session_at(
                owner_id,
                owner_session,
                t0 + ELEVATED_SESSION_TTL_SECS + 1
            )
            .await
            .unwrap_err(),
            AuthError::OwnerMfaRequired
        );

        // 10. Step-up required after 5 minutes even while elevated session is still valid.
        assert!(svc
            .require_elevated_session_at(owner_id, owner_session, t0 + STEP_UP_TTL_SECS + 10)
            .await
            .is_ok());
        assert_eq!(
            svc.require_step_up_at(owner_id, owner_session, t0 + STEP_UP_TTL_SECS + 10)
                .await
                .unwrap_err(),
            AuthError::OwnerStepUpRequired
        );

        // 11. Recovery code works ONCE, and reuse is rejected!
        let rec0 = &bundle.recovery_codes[0];
        let t_rec = t0 + 120;
        svc.verify_recovery_code_at(owner_id, owner_session, rec0, t_rec)
            .await
            .expect("recovery once");
        assert_eq!(
            svc.verify_recovery_code_at(owner_id, owner_session, rec0, t_rec + 1)
                .await
                .unwrap_err(),
            AuthError::OwnerMfaRequired
        );

        // 12. Repeated wrong TOTP codes trigger lockout (`SF-AUTH-015`).
        let mut got_locked = false;
        for attempt in 0..MAX_MFA_FAILED_ATTEMPTS {
            let res = svc
                .verify_totp_at(
                    owner_id,
                    owner_session,
                    "000000",
                    t_rec + 10 + i64::from(attempt),
                )
                .await;
            if res == Err(AuthError::OwnerMfaLocked) {
                got_locked = true;
                break;
            }
        }
        assert!(got_locked, "must lock after repeated wrong TOTP codes");

        // 13. Even a valid TOTP code is rejected during active lockout.
        let t_during_lock = t_rec + 90;
        let valid_during_lock =
            compute_totp_code(&raw_secret, t_during_lock / TOTP_PERIOD_SECS).expect("code");
        assert_eq!(
            svc.verify_totp_at(owner_id, owner_session, &valid_during_lock, t_during_lock)
                .await
                .unwrap_err(),
            AuthError::OwnerMfaLocked
        );

        // 14. Missing TOTP encryption key fails closed (`SF-AUTH-018`).
        let keyless = OwnerMfaService::new(None, 1, "SafiData".to_owned());
        keyless
            .bootstrap_owner_once(owner_id, true)
            .await
            .expect("bootstrap");
        assert_eq!(
            keyless.enroll_totp(owner_id, "owner").await.unwrap_err(),
            AuthError::OwnerTotpKeyMissing
        );
    }
}
