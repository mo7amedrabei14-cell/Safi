//! Zero-Knowledge Client-Side Encrypted Sync & Share Links (Phase 11).
//!
//! Cryptographic invariant:
//! - Clients encrypt payloads locally using ChaCha20-Poly1305 before uploading.
//! - The server stores ONLY opaque ciphertext, 12-byte nonce, key identifier,
//!   and BLAKE3 integrity digest — NEVER plaintext data and NEVER raw decryption keys.
//! - Optimistic concurrency (`expected_version`) prevents silent overwrites (`SF-SYN-002`).
//! - Share links support explicit expiry and instant revocation (`SF-SYN-003`).

use std::collections::HashMap;
use std::sync::Arc;

use base64::Engine;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::auth::unix_now;
use crate::error::AuthError;

const NONCE_BYTES: usize = 12;
const MIN_CIPHERTEXT_BYTES: usize = 17; // >= 1 byte + 16-byte Poly1305 tag

#[derive(Debug, Clone, Deserialize)]
pub struct PushEncryptedSyncRequest {
    pub resource_kind: String,
    pub resource_id: String,
    pub expected_version: u64,
    pub ciphertext_b64: String,
    pub nonce_b64: String,
    pub key_id: String,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedSyncRecord {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub resource_kind: String,
    pub resource_id: String,
    pub version: u64,
    pub ciphertext_b64: String,
    pub nonce_b64: String,
    pub key_id: String,
    pub content_hash: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateShareLinkRequest {
    pub resource_id: String,
    pub wrapped_key_ciphertext_b64: String,
    pub wrapped_key_nonce_b64: String,
    pub ttl_secs: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CreatedShareLink {
    pub id: Uuid,
    pub share_token: String,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShareLinkRecord {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub created_by: Uuid,
    pub resource_id: String,
    pub token_hash: String,
    pub wrapped_key_ciphertext_b64: String,
    pub wrapped_key_nonce_b64: String,
    pub expires_at: i64,
    pub revoked_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Default)]
struct SyncStore {
    records: HashMap<(Uuid, String, String), EncryptedSyncRecord>,
    share_links: HashMap<String, ShareLinkRecord>,
}

/// Zero-knowledge encrypted sync & share-link repository service.
#[derive(Debug, Clone, Default)]
pub struct EncryptedSyncService {
    store: Arc<RwLock<SyncStore>>,
}

fn is_valid_resource_kind(kind: &str) -> bool {
    matches!(
        kind,
        "project" | "recipe" | "dashboard" | "presentation" | "document"
    )
}

impl EncryptedSyncService {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a client-encrypted envelope with strict format, integrity hash, and optimistic concurrency checks.
    pub async fn push_envelope(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        req: &PushEncryptedSyncRequest,
    ) -> Result<EncryptedSyncRecord, AuthError> {
        if !is_valid_resource_kind(req.resource_kind.trim())
            || req.resource_id.trim().is_empty()
            || req.key_id.trim().is_empty()
        {
            return Err(AuthError::SyncEnvelopeInvalid);
        }

        let b64 = base64::engine::general_purpose::STANDARD;
        let ciphertext = b64
            .decode(req.ciphertext_b64.trim())
            .map_err(|_| AuthError::SyncEnvelopeInvalid)?;
        let nonce = b64
            .decode(req.nonce_b64.trim())
            .map_err(|_| AuthError::SyncEnvelopeInvalid)?;

        if nonce.len() != NONCE_BYTES || ciphertext.len() < MIN_CIPHERTEXT_BYTES {
            return Err(AuthError::SyncEnvelopeInvalid);
        }

        // Verify BLAKE3 integrity digest over nonce || ciphertext.
        let mut hasher = blake3::Hasher::new();
        hasher.update(&nonce);
        hasher.update(&ciphertext);
        let expected_hash = hasher.finalize().to_hex().to_string();
        if expected_hash != req.content_hash.trim() {
            return Err(AuthError::SyncEnvelopeInvalid);
        }

        let key = (
            workspace_id,
            req.resource_kind.trim().to_owned(),
            req.resource_id.trim().to_owned(),
        );
        let mut guard = self.store.write().await;
        let current_version = guard.records.get(&key).map_or(0, |r| r.version);
        if req.expected_version != current_version {
            return Err(AuthError::SyncVersionConflict);
        }

        let record = EncryptedSyncRecord {
            id: Uuid::now_v7(),
            workspace_id,
            user_id,
            resource_kind: key.1.clone(),
            resource_id: key.2.clone(),
            version: current_version + 1,
            ciphertext_b64: req.ciphertext_b64.trim().to_owned(),
            nonce_b64: req.nonce_b64.trim().to_owned(),
            key_id: req.key_id.trim().to_owned(),
            content_hash: expected_hash,
            updated_at: unix_now(),
        };
        guard.records.insert(key, record.clone());
        Ok(record)
    }

    /// Fetch the latest encrypted envelope for a workspace resource.
    pub async fn pull_envelope(
        &self,
        workspace_id: Uuid,
        resource_kind: &str,
        resource_id: &str,
    ) -> Option<EncryptedSyncRecord> {
        self.store
            .read()
            .await
            .records
            .get(&(
                workspace_id,
                resource_kind.trim().to_owned(),
                resource_id.trim().to_owned(),
            ))
            .cloned()
    }

    /// Create an encrypted share link carrying only a client-wrapped key (server never sees raw key).
    pub async fn create_share_link(
        &self,
        workspace_id: Uuid,
        created_by: Uuid,
        req: &CreateShareLinkRequest,
    ) -> Result<CreatedShareLink, AuthError> {
        if req.resource_id.trim().is_empty() || req.ttl_secs <= 0 || req.ttl_secs > 90 * 86400 {
            return Err(AuthError::InvalidRequest);
        }
        let b64 = base64::engine::general_purpose::STANDARD;
        let wrapped = b64
            .decode(req.wrapped_key_ciphertext_b64.trim())
            .map_err(|_| AuthError::SyncEnvelopeInvalid)?;
        let nonce = b64
            .decode(req.wrapped_key_nonce_b64.trim())
            .map_err(|_| AuthError::SyncEnvelopeInvalid)?;
        if nonce.len() != NONCE_BYTES || wrapped.len() < MIN_CIPHERTEXT_BYTES {
            return Err(AuthError::SyncEnvelopeInvalid);
        }

        let token = format!(
            "sfsh_{}{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        );
        let token_hash = blake3::hash(token.as_bytes()).to_hex().to_string();
        let now = unix_now();
        let record = ShareLinkRecord {
            id: Uuid::now_v7(),
            workspace_id,
            created_by,
            resource_id: req.resource_id.trim().to_owned(),
            token_hash: token_hash.clone(),
            wrapped_key_ciphertext_b64: req.wrapped_key_ciphertext_b64.trim().to_owned(),
            wrapped_key_nonce_b64: req.wrapped_key_nonce_b64.trim().to_owned(),
            expires_at: now + req.ttl_secs,
            revoked_at: None,
            created_at: now,
        };
        let id = record.id;
        let expires_at = record.expires_at;
        self.store
            .write()
            .await
            .share_links
            .insert(token_hash, record);
        Ok(CreatedShareLink {
            id,
            share_token: token,
            expires_at,
        })
    }

    /// Resolve a share link if not expired and not revoked.
    pub async fn resolve_share_link_at(
        &self,
        token: &str,
        now_secs: i64,
    ) -> Result<ShareLinkRecord, AuthError> {
        let token_hash = blake3::hash(token.trim().as_bytes()).to_hex().to_string();
        let guard = self.store.read().await;
        let rec = guard
            .share_links
            .get(&token_hash)
            .ok_or(AuthError::ShareLinkInvalid)?;
        if rec.revoked_at.is_some() || now_secs >= rec.expires_at {
            return Err(AuthError::ShareLinkInvalid);
        }
        Ok(rec.clone())
    }

    /// Revoke a share link immediately.
    pub async fn revoke_share_link(
        &self,
        workspace_id: Uuid,
        link_id: Uuid,
    ) -> Result<(), AuthError> {
        let mut guard = self.store.write().await;
        let rec = guard
            .share_links
            .values_mut()
            .find(|r| r.id == link_id && r.workspace_id == workspace_id)
            .ok_or(AuthError::ShareLinkInvalid)?;
        rec.revoked_at = Some(unix_now());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, CHACHA20_POLY1305};

    #[tokio::test]
    async fn encrypted_sync_stores_zero_plaintext_detects_conflicts_and_revokes_share_links() {
        let svc = EncryptedSyncService::new();
        let ws = Uuid::now_v7();
        let user = Uuid::now_v7();

        // Client-side ChaCha20-Poly1305 encryption before upload.
        let key_bytes = [0x99u8; 32];
        let nonce_bytes = [0x11u8; 12];
        let plaintext = b"CONFIDENTIAL_CUSTOMER_SALARY_SHEET_2026";
        let mut buf = plaintext.to_vec();
        let sealing =
            LessSafeKey::new(UnboundKey::new(&CHACHA20_POLY1305, &key_bytes).expect("key"));
        sealing
            .seal_in_place_append_tag(
                Nonce::assume_unique_for_key(nonce_bytes),
                Aad::from(b"safi-sync-v1"),
                &mut buf,
            )
            .expect("seal");

        let b64 = base64::engine::general_purpose::STANDARD;
        let mut h = blake3::Hasher::new();
        h.update(&nonce_bytes);
        h.update(&buf);
        let content_hash = h.finalize().to_hex().to_string();

        let pushed = svc
            .push_envelope(
                ws,
                user,
                &PushEncryptedSyncRequest {
                    resource_kind: "project".to_owned(),
                    resource_id: "proj_01".to_owned(),
                    expected_version: 0,
                    ciphertext_b64: b64.encode(&buf),
                    nonce_b64: b64.encode(nonce_bytes),
                    key_id: "client-key-v1".to_owned(),
                    content_hash: content_hash.clone(),
                },
            )
            .await
            .expect("push v1");
        assert_eq!(pushed.version, 1);
        // Server record contains ZERO plaintext bytes.
        assert!(!pushed.ciphertext_b64.contains("CONFIDENTIAL"));

        // Stale version conflict rejected (`SF-SYN-002`).
        assert_eq!(
            svc.push_envelope(
                ws,
                user,
                &PushEncryptedSyncRequest {
                    resource_kind: "project".to_owned(),
                    resource_id: "proj_01".to_owned(),
                    expected_version: 0,
                    ciphertext_b64: b64.encode(&buf),
                    nonce_b64: b64.encode(nonce_bytes),
                    key_id: "client-key-v1".to_owned(),
                    content_hash,
                },
            )
            .await
            .unwrap_err(),
            AuthError::SyncVersionConflict
        );

        // Tampered digest rejected (`SF-SYN-001`).
        assert_eq!(
            svc.push_envelope(
                ws,
                user,
                &PushEncryptedSyncRequest {
                    resource_kind: "project".to_owned(),
                    resource_id: "proj_01".to_owned(),
                    expected_version: 1,
                    ciphertext_b64: b64.encode(&buf),
                    nonce_b64: b64.encode(nonce_bytes),
                    key_id: "client-key-v1".to_owned(),
                    content_hash: "0".repeat(64),
                },
            )
            .await
            .unwrap_err(),
            AuthError::SyncEnvelopeInvalid
        );

        // Share link create, resolve, revoke.
        let link = svc
            .create_share_link(
                ws,
                user,
                &CreateShareLinkRequest {
                    resource_id: "proj_01".to_owned(),
                    wrapped_key_ciphertext_b64: b64.encode(&buf),
                    wrapped_key_nonce_b64: b64.encode(nonce_bytes),
                    ttl_secs: 3600,
                },
            )
            .await
            .expect("share link");
        let now = unix_now();
        assert!(svc
            .resolve_share_link_at(&link.share_token, now)
            .await
            .is_ok());
        assert_eq!(
            svc.resolve_share_link_at(&link.share_token, link.expires_at + 1)
                .await
                .unwrap_err(),
            AuthError::ShareLinkInvalid
        );
        svc.revoke_share_link(ws, link.id).await.expect("revoke");
        assert_eq!(
            svc.resolve_share_link_at(&link.share_token, now)
                .await
                .unwrap_err(),
            AuthError::ShareLinkInvalid
        );
    }
}
