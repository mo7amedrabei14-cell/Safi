//! Desktop Browser PKCE (`S256`) Authorization Code Flow & Dedicated Owner Web Sessions
//! (Sections 5 & 6.1).
//!
//! Security invariants:
//! - Desktop sign-in/sign-up launches the OS default browser to `/auth/desktop?flow=signin|signup`.
//! - Only a short-lived (120s) one-time authorization `code` + `state` travels in the callback
//!   (`safi://auth?code=...&state=...`). Access or session tokens NEVER appear in URLs.
//! - PKCE `S256` (`BASE64URL_NOPAD(SHA256(code_verifier)) == code_challenge`) is verified in
//!   constant time, bound to the requesting `installation_id` and `state`, and single-use.
//! - Owner Web Sessions (`sfw_*`) are strictly separated from Desktop Installation Sessions
//!   (`sfs_*` / `DeviceAuthorityService`) so Owner logging into `/owner` on the website NEVER
//!   counts as a second Desktop installation or revokes the Owner's Desktop device.

use std::collections::HashMap;
use std::sync::Arc;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::auth::device::DevicePlatform;
use crate::auth::{AuthMethod, AuthenticatedSession, Principal, Scope};
use crate::error::AuthError;

/// Lifetime of a pending PKCE Desktop authorization request before browser approval (5 minutes).
pub const PKCE_REQUEST_TTL_SECS: i64 = 300;
/// Lifetime of an issued one-time Desktop authorization code (120 seconds).
pub const PKCE_CODE_TTL_SECS: i64 = 120;
/// Lifetime of an Owner Web Portal session (4 hours; still requires 30m elevated TOTP & 5m step-up).
pub const OWNER_WEB_SESSION_TTL_SECS: i64 = 4 * 60 * 60;
const OWNER_WEB_TOKEN_PREFIX: &str = "sfw_";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DesktopAuthFlow {
    Signin,
    Signup,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StartDesktopPkceRequest {
    pub flow: DesktopAuthFlow,
    pub code_challenge: String,
    pub state: String,
    pub installation_id: Uuid,
    pub platform: DevicePlatform,
    pub device_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StartDesktopPkceResponse {
    pub request_id: Uuid,
    pub auth_url: String,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingPkceEntry {
    pub request_id: Uuid,
    pub flow: DesktopAuthFlow,
    pub code_challenge: String,
    pub state_hash: String,
    pub installation_id: Uuid,
    pub platform: DevicePlatform,
    pub device_name: String,
    pub approved_user_id: Option<Uuid>,
    pub approved_workspace_id: Option<Uuid>,
    pub approved_email: Option<String>,
    pub code_hash: Option<String>,
    pub expires_at: i64,
    pub consumed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnerWebSessionRecord {
    pub id: Uuid,
    pub user_id: Uuid,
    pub workspace_id: Uuid,
    pub token_hash: String,
    pub user_agent_hint: String,
    pub created_at: i64,
    pub last_seen_at: i64,
    pub expires_at: i64,
    pub revoked: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DesktopAuthSnapshot {
    pub owner_web_sessions: Vec<OwnerWebSessionRecord>,
}

#[derive(Default)]
struct DesktopAuthStore {
    pending_by_request: HashMap<Uuid, PendingPkceEntry>,
    request_by_code_hash: HashMap<String, Uuid>,
    owner_web_by_hash: HashMap<String, OwnerWebSessionRecord>,
}

/// Manages Desktop Browser PKCE authorization codes and dedicated Owner Web sessions.
#[derive(Clone, Default)]
pub struct DesktopAuthService {
    store: Arc<RwLock<DesktopAuthStore>>,
}

impl std::fmt::Debug for DesktopAuthService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DesktopAuthService").finish()
    }
}

fn random_hex(bytes_len: usize) -> Result<String, AuthError> {
    let rng = SystemRandom::new();
    let mut buf = vec![0u8; bytes_len];
    rng.fill(&mut buf)
        .map_err(|_| AuthError::BackendUnavailable)?;
    Ok(hex::encode(buf))
}

fn blake3_hex_str(raw: &str) -> String {
    blake3::hash(raw.as_bytes()).to_hex().to_string()
}

/// Compute RFC 7636 `S256` challenge (`BASE64URL_NOPAD(SHA256(verifier))`).
#[must_use]
pub fn pkce_s256_challenge(code_verifier: &str) -> String {
    let digest = Sha256::digest(code_verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

impl DesktopAuthService {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn export_snapshot(&self) -> DesktopAuthSnapshot {
        let guard = self.store.read().await;
        DesktopAuthSnapshot {
            owner_web_sessions: guard.owner_web_by_hash.values().cloned().collect(),
        }
    }

    pub async fn import_snapshot(&self, snap: DesktopAuthSnapshot) {
        let mut guard = self.store.write().await;
        guard.owner_web_by_hash.clear();
        for rec in snap.owner_web_sessions {
            guard.owner_web_by_hash.insert(rec.token_hash.clone(), rec);
        }
    }

    /// Step 1 (Desktop -> Backend): register PKCE challenge, state, and installation binding.
    pub async fn start_pkce(
        &self,
        req: StartDesktopPkceRequest,
        public_web_origin: &str,
        now_secs: i64,
    ) -> Result<StartDesktopPkceResponse, AuthError> {
        let challenge = req.code_challenge.trim();
        let state = req.state.trim();
        let dev_name = req.device_name.trim();
        if challenge.len() < 43
            || challenge.len() > 128
            || state.len() < 16
            || state.len() > 256
            || req.installation_id.is_nil()
            || dev_name.is_empty()
            || dev_name.len() > 120
        {
            return Err(AuthError::InvalidRequest);
        }

        let request_id = Uuid::now_v7();
        let expires_at = now_secs + PKCE_REQUEST_TTL_SECS;
        let flow_str = match req.flow {
            DesktopAuthFlow::Signin => "signin",
            DesktopAuthFlow::Signup => "signup",
        };
        let entry = PendingPkceEntry {
            request_id,
            flow: req.flow,
            code_challenge: challenge.to_owned(),
            state_hash: blake3_hex_str(state),
            installation_id: req.installation_id,
            platform: req.platform,
            device_name: dev_name.to_owned(),
            approved_user_id: None,
            approved_workspace_id: None,
            approved_email: None,
            code_hash: None,
            expires_at,
            consumed: false,
        };
        self.store
            .write()
            .await
            .pending_by_request
            .insert(request_id, entry);

        let origin = public_web_origin.trim_end_matches('/');
        let auth_url =
            format!("{origin}/auth/desktop?flow={flow_str}&request_id={request_id}&state={state}");
        Ok(StartDesktopPkceResponse {
            request_id,
            auth_url,
            expires_at,
        })
    }

    /// Step 2 (Browser `/auth/desktop` -> Backend): after verifying credentials, mint a single-use
    /// short-lived authorization code (`sfcode_*`) bound to `request_id` and `state`.
    #[allow(clippy::too_many_arguments)]
    pub async fn approve_pkce(
        &self,
        request_id: Uuid,
        state: &str,
        user_id: Uuid,
        workspace_id: Uuid,
        email: &str,
        now_secs: i64,
    ) -> Result<(String, String), AuthError> {
        let state_hash = blake3_hex_str(state.trim());
        let mut guard = self.store.write().await;
        let entry = guard
            .pending_by_request
            .get_mut(&request_id)
            .ok_or(AuthError::DesktopAuthCodeInvalid)?;
        if entry.consumed || now_secs >= entry.expires_at || entry.code_hash.is_some() {
            return Err(AuthError::DesktopAuthCodeInvalid);
        }
        if entry
            .state_hash
            .as_bytes()
            .ct_eq(state_hash.as_bytes())
            .unwrap_u8()
            != 1
        {
            return Err(AuthError::DesktopAuthCodeInvalid);
        }

        let raw_code = format!("sfcode_{}", random_hex(24)?);
        let code_hash = blake3_hex_str(&raw_code);
        entry.approved_user_id = Some(user_id);
        entry.approved_workspace_id = Some(workspace_id);
        entry.approved_email = Some(email.trim().to_ascii_lowercase());
        entry.code_hash = Some(code_hash.clone());
        entry.expires_at = now_secs + PKCE_CODE_TTL_SECS;

        guard.request_by_code_hash.insert(code_hash, request_id);
        let callback_uri = format!("safi://auth?code={raw_code}&state={}", state.trim());
        Ok((raw_code, callback_uri))
    }

    /// Step 3 (Desktop -> Backend): verify one-time `code`, `code_verifier` (PKCE S256), `state`,
    /// `installation_id`, and expiry; mark consumed atomically so replay ALWAYS fails (`SF-AUTH-023`).
    pub async fn consume_pkce_code(
        &self,
        code: &str,
        code_verifier: &str,
        state: &str,
        installation_id: Uuid,
        now_secs: i64,
    ) -> Result<(Uuid, Uuid, String, String, DevicePlatform), AuthError> {
        let verifier = code_verifier.trim();
        if verifier.len() < 43 || verifier.len() > 128 {
            return Err(AuthError::DesktopAuthCodeInvalid);
        }
        let code_hash = blake3_hex_str(code.trim());
        let state_hash = blake3_hex_str(state.trim());

        let mut guard = self.store.write().await;
        let request_id = *guard
            .request_by_code_hash
            .get(&code_hash)
            .ok_or(AuthError::DesktopAuthCodeInvalid)?;
        let entry = guard
            .pending_by_request
            .get_mut(&request_id)
            .ok_or(AuthError::DesktopAuthCodeInvalid)?;

        if entry.consumed || now_secs >= entry.expires_at {
            entry.consumed = true;
            return Err(AuthError::DesktopAuthCodeInvalid);
        }
        if entry.installation_id != installation_id {
            return Err(AuthError::DesktopAuthCodeInvalid);
        }
        if entry
            .state_hash
            .as_bytes()
            .ct_eq(state_hash.as_bytes())
            .unwrap_u8()
            != 1
        {
            return Err(AuthError::DesktopAuthCodeInvalid);
        }
        let computed = pkce_s256_challenge(verifier);
        if entry
            .code_challenge
            .as_bytes()
            .ct_eq(computed.as_bytes())
            .unwrap_u8()
            != 1
        {
            return Err(AuthError::DesktopAuthCodeInvalid);
        }

        let user_id = entry
            .approved_user_id
            .ok_or(AuthError::DesktopAuthCodeInvalid)?;
        let workspace_id = entry
            .approved_workspace_id
            .ok_or(AuthError::DesktopAuthCodeInvalid)?;
        let email = entry
            .approved_email
            .clone()
            .ok_or(AuthError::DesktopAuthCodeInvalid)?;
        let device_name = entry.device_name.clone();
        let platform = entry.platform;
        entry.consumed = true;

        Ok((user_id, workspace_id, email, device_name, platform))
    }

    /// Issue a dedicated Owner Web Portal session (`sfw_*`) that NEVER touches or revokes
    /// the Owner's Desktop device registration (Section 6.1).
    pub async fn issue_owner_web_session(
        &self,
        user_id: Uuid,
        workspace_id: Uuid,
        user_agent_hint: &str,
        now_secs: i64,
    ) -> Result<(String, OwnerWebSessionRecord), AuthError> {
        let token = format!("{OWNER_WEB_TOKEN_PREFIX}{}", random_hex(32)?);
        let token_hash = blake3_hex_str(&token);
        let record = OwnerWebSessionRecord {
            id: Uuid::now_v7(),
            user_id,
            workspace_id,
            token_hash: token_hash.clone(),
            user_agent_hint: user_agent_hint.trim().chars().take(120).collect(),
            created_at: now_secs,
            last_seen_at: now_secs,
            expires_at: now_secs + OWNER_WEB_SESSION_TTL_SECS,
            revoked: false,
        };
        self.store
            .write()
            .await
            .owner_web_by_hash
            .insert(token_hash, record.clone());
        Ok((token, record))
    }

    /// Authenticate an `sfw_*` Owner Web session token.
    pub async fn authenticate_owner_web_session(
        &self,
        presented: &str,
        now_secs: i64,
    ) -> Result<AuthenticatedSession, AuthError> {
        let trimmed = presented.trim();
        if !trimmed.starts_with(OWNER_WEB_TOKEN_PREFIX) || trimmed.len() != 68 {
            return Err(AuthError::InvalidCredentials);
        }
        let hash = blake3_hex_str(trimmed);
        let mut guard = self.store.write().await;
        let rec = guard
            .owner_web_by_hash
            .get_mut(&hash)
            .ok_or(AuthError::InvalidCredentials)?;
        if rec.revoked {
            return Err(AuthError::Revoked);
        }
        if now_secs >= rec.expires_at {
            return Err(AuthError::Expired);
        }
        rec.last_seen_at = now_secs;
        Ok(AuthenticatedSession {
            principal: Principal {
                user_id: rec.user_id,
                workspace_id: rec.workspace_id,
                scopes: vec![Scope::Read, Scope::Write, Scope::Admin],
                auth_method: AuthMethod::Session { session_id: rec.id },
            },
            session_id: rec.id,
            device_name: "Owner Web Portal".to_owned(),
            expires_at: rec.expires_at,
            token_hash: hash,
            device_installation_key_hash: None,
        })
    }

    /// Revoke an Owner Web session on logout.
    pub async fn revoke_owner_web_session(&self, session_id: Uuid) {
        let mut guard = self.store.write().await;
        for rec in guard.owner_web_by_hash.values_mut() {
            if rec.id == session_id {
                rec.revoked = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::unix_now;

    #[tokio::test]
    async fn pkce_code_flow_enforces_s256_state_installation_expiry_and_single_use() {
        let svc = DesktopAuthService::new();
        let verifier = "v".repeat(64);
        let challenge = pkce_s256_challenge(&verifier);
        let inst = Uuid::now_v7();
        let state = "state-random-nonce-1234567890";
        let now = unix_now();

        let started = svc
            .start_pkce(
                StartDesktopPkceRequest {
                    flow: DesktopAuthFlow::Signin,
                    code_challenge: challenge,
                    state: state.to_owned(),
                    installation_id: inst,
                    platform: DevicePlatform::Windows,
                    device_name: "Surface Pro".to_owned(),
                },
                "https://safidata.app",
                now,
            )
            .await
            .expect("start pkce");
        assert!(started.auth_url.contains("/auth/desktop?flow=signin"));

        let uid = Uuid::now_v7();
        let ws = Uuid::now_v7();
        let (code, callback) = svc
            .approve_pkce(
                started.request_id,
                state,
                uid,
                ws,
                "u@example.test",
                now + 5,
            )
            .await
            .expect("approve");
        assert!(callback.starts_with("safi://auth?code=sfcode_"));
        assert!(!callback.contains("sfs_"));

        // Wrong verifier fails with SF-AUTH-023
        assert_eq!(
            svc.consume_pkce_code(&code, &"w".repeat(64), state, inst, now + 10)
                .await
                .unwrap_err(),
            AuthError::DesktopAuthCodeInvalid
        );
        // Wrong installation fails with SF-AUTH-023
        assert_eq!(
            svc.consume_pkce_code(&code, &verifier, state, Uuid::now_v7(), now + 10)
                .await
                .unwrap_err(),
            AuthError::DesktopAuthCodeInvalid
        );
        // Valid exchange succeeds once
        let (out_uid, out_ws, _, _, _) = svc
            .consume_pkce_code(&code, &verifier, state, inst, now + 10)
            .await
            .expect("valid exchange");
        assert_eq!(out_uid, uid);
        assert_eq!(out_ws, ws);

        // Replay of the same code ALWAYS fails with SF-AUTH-023
        assert_eq!(
            svc.consume_pkce_code(&code, &verifier, state, inst, now + 11)
                .await
                .unwrap_err(),
            AuthError::DesktopAuthCodeInvalid
        );
    }
}
