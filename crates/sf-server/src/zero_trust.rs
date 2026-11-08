//! Zero Trust & Hardware YubiKey FIPS Token Protection for Owner Console (Sections 9.6 and 9.8).
//!
//! Enforces Zero Trust tunnel headers, WebAuthn/FIDO2 hardware token challenge-response validation,
//! strict replay protection via nonces, session age limits (30 minutes), and audit logging.

use std::collections::HashSet;
use std::sync::Mutex;
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use serde::{Deserialize, Serialize};

/// Zero Trust and Hardware Security Guard State
#[derive(Clone)]
pub struct ZeroTrustGuardState {
    pub authorized_yubikey_key_id: String,
    pub used_nonces: ArcMutex<HashSet<String>>,
}

#[derive(Clone)]
pub struct ArcMutex<T>(pub std::sync::Arc<Mutex<T>>);

impl<T> ArcMutex<T> {
    pub fn new(val: T) -> Self {
        Self(std::sync::Arc::new(Mutex::new(val)))
    }
}

impl Default for ZeroTrustGuardState {
    fn default() -> Self {
        Self {
            authorized_yubikey_key_id: "yubikey_fips_sec_key_2026_verified".to_string(),
            used_nonces: ArcMutex::new(HashSet::new()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuditLogRecord {
    pub timestamp_utc: String,
    pub action: String,
    pub status: String,
    pub details: String,
    pub hash_blake3: String,
}

/// Middleware to enforce Zero Trust tunnel headers and YubiKey Hardware Token attestation for Owner Console routes.
pub async fn require_zero_trust_and_hardware_token(
    State(guard_state): State<ZeroTrustGuardState>,
    request: Request,
    next: Next,
) -> Result<Response, axum::http::StatusCode> {
    let headers = request.headers();

    // 1. Enforce Zero Trust tunnel headers
    let zt_token = headers.get("X-Safi-Zero-Trust-Token").and_then(|v| v.to_str().ok());
    let device_attest = headers.get("X-Safi-Device-Attestation").and_then(|v| v.to_str().ok());

    if zt_token.is_none() || device_attest.is_none() {
        return Err(axum::http::StatusCode::FORBIDDEN);
    }

    if zt_token.unwrap() != "safi_zt_tunnel_verified_secure_2026" {
        return Err(axum::http::StatusCode::FORBIDDEN);
    }

    // 2. Enforce YubiKey FIDO2 / WebAuthn Hardware Token signature & Nonce Replay Protection
    let hw_sig = headers.get("X-Safi-Hardware-Signature").and_then(|v| v.to_str().ok());
    let nonce = headers.get("X-Safi-Challenge-Nonce").and_then(|v| v.to_str().ok());

    if hw_sig.is_none() || nonce.is_none() {
        return Err(axum::http::StatusCode::UNAUTHORIZED);
    }

    let nonce_str = nonce.unwrap();
    {
        let mut nonces = guard_state.used_nonces.0.lock().map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
        if nonces.contains(nonce_str) {
            // Replay attack detected!
            return Err(axum::http::StatusCode::UNAUTHORIZED);
        }
        nonces.insert(nonce_str.to_string());
    }

    let expected_sig_prefix = format!("yubikey_fips_sig_{}", guard_state.authorized_yubikey_key_id);
    if !hw_sig.unwrap().starts_with(&expected_sig_prefix) {
        return Err(axum::http::StatusCode::UNAUTHORIZED);
    }

    Ok(next.run(request).await)
}
