//! Production Zero-Knowledge Client-Side Encrypted Sync & Share Links (Migrations `0009` + `0017`).
//!
//! Cryptographic & durability invariants:
//! - Clients encrypt payloads locally using `ChaCha20-Poly1305` before uploading.
//! - The server stores ONLY opaque ciphertext (`BYTEA`), 12-byte nonce (`BYTEA`), key identifier,
//!   key epoch, and `BLAKE3` integrity digest — NEVER plaintext data and NEVER decryption keys.
//! - Authoritative PostgreSQL 18 persistence via `encrypted_sync_pg` when `PgControlPlaneStore`
//!   is attached; strict environments (`Staging`/`Production`) fail closed (`SF-AUTH-005`) if
//!   PostgreSQL is absent.
//! - Optimistic concurrency (`expected_version`) prevents silent overwrites (`SF-SYN-002`).
//! - Idempotency keys + BLAKE3 request fingerprints guarantee network retries after a dropped
//!   response return the committed revision instead of duplicating or failing with false conflicts.
//! - Nonce reuse across distinct versions of a resource is rejected (`SF-SYN-001`).
//! - Monotonic per-workspace sequence cursor (`workspace_seq`) powers incremental pull,
//!   interruption resume, and tombstones (`deleted = true`).

use std::collections::HashMap;
use std::sync::Arc;

use base64::Engine;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::auth::unix_now;
use crate::encrypted_sync_pg;
use crate::error::AuthError;
use crate::pg_store::PgControlPlaneStore;

pub const NONCE_BYTES: usize = 12;
pub const MIN_CIPHERTEXT_BYTES: usize = 17; // >= 1 byte + 16-byte Poly1305 tag
pub const MAX_CIPHERTEXT_BYTES: usize = 16 * 1024 * 1024; // 16 MiB envelope safety cap

const fn default_key_epoch() -> u32 {
    1
}

fn default_origin_device_id() -> String {
    "desktop-client".to_owned()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PushEncryptedSyncRequest {
    pub resource_kind: String,
    pub resource_id: String,
    pub expected_version: u64,
    pub ciphertext_b64: String,
    pub nonce_b64: String,
    pub key_id: String,
    pub content_hash: String,
    #[serde(default = "default_key_epoch")]
    pub key_epoch: u32,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default = "default_origin_device_id")]
    pub origin_device_id: String,
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedSyncRecord {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub resource_kind: String,
    pub resource_id: String,
    pub version: u64,
    #[serde(default = "default_workspace_seq")]
    pub workspace_seq: u64,
    pub ciphertext_b64: String,
    pub nonce_b64: String,
    pub key_id: String,
    #[serde(default = "default_key_epoch")]
    pub key_epoch: u32,
    pub content_hash: String,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default = "default_origin_device_id")]
    pub origin_device_id: String,
    pub updated_at: i64,
    #[serde(default, skip_serializing)]
    pub last_idempotency_key: Option<String>,
    #[serde(default, skip_serializing)]
    pub last_request_fingerprint: Option<String>,
}

const fn default_workspace_seq() -> u64 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncChangesResponse {
    pub workspace_id: Uuid,
    pub latest_workspace_seq: u64,
    pub has_more: bool,
    pub records: Vec<EncryptedSyncRecord>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateShareLinkRequest {
    pub resource_id: String,
    pub wrapped_key_ciphertext_b64: String,
    pub wrapped_key_nonce_b64: String,
    pub ttl_secs: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatedShareLink {
    pub id: Uuid,
    pub share_token: String,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EncryptedSyncSnapshot {
    pub records: Vec<EncryptedSyncRecord>,
    pub share_links: Vec<ShareLinkRecord>,
}

#[derive(Debug, Default)]
struct SyncStore {
    records: HashMap<(Uuid, String, String), EncryptedSyncRecord>,
    workspace_seq: HashMap<Uuid, u64>,
    share_links: HashMap<String, ShareLinkRecord>,
}

pub(crate) struct ValidatedPushEnvelope {
    pub resource_kind: String,
    pub resource_id: String,
    pub expected_version: u64,
    pub ciphertext: Vec<u8>,
    pub ciphertext_b64: String,
    pub nonce: Vec<u8>,
    pub nonce_b64: String,
    pub key_id: String,
    pub key_epoch: u32,
    pub content_hash: String,
    pub deleted: bool,
    pub origin_device_id: String,
    pub idempotency_key: Option<String>,
    pub request_fingerprint: String,
}

pub(crate) fn is_valid_resource_kind(kind: &str) -> bool {
    matches!(
        kind,
        "project" | "recipe" | "dashboard" | "presentation" | "document"
    )
}

pub(crate) fn validate_push_request(
    req: &PushEncryptedSyncRequest,
) -> Result<ValidatedPushEnvelope, AuthError> {
    let resource_kind = req.resource_kind.trim();
    let resource_id = req.resource_id.trim();
    let key_id = req.key_id.trim();
    let origin_device_id = if req.origin_device_id.trim().is_empty() {
        "desktop-client"
    } else {
        req.origin_device_id.trim()
    };
    if !is_valid_resource_kind(resource_kind)
        || resource_id.is_empty()
        || resource_id.len() > 256
        || key_id.is_empty()
        || key_id.len() > 128
        || req.key_epoch == 0
        || origin_device_id.len() > 128
    {
        return Err(AuthError::SyncEnvelopeInvalid);
    }

    let idempotency_key = match req.idempotency_key.as_deref().map(str::trim) {
        Some("") | None => None,
        Some(k) if k.len() > 128 => return Err(AuthError::InvalidRequest),
        Some(k) => Some(k.to_owned()),
    };

    let b64 = base64::engine::general_purpose::STANDARD;
    let ciphertext = b64
        .decode(req.ciphertext_b64.trim())
        .map_err(|_| AuthError::SyncEnvelopeInvalid)?;
    let nonce = b64
        .decode(req.nonce_b64.trim())
        .map_err(|_| AuthError::SyncEnvelopeInvalid)?;

    if nonce.len() != NONCE_BYTES
        || ciphertext.len() < MIN_CIPHERTEXT_BYTES
        || ciphertext.len() > MAX_CIPHERTEXT_BYTES
    {
        return Err(AuthError::SyncEnvelopeInvalid);
    }

    // Verify BLAKE3 integrity digest over `nonce || ciphertext`.
    let mut hasher = blake3::Hasher::new();
    hasher.update(&nonce);
    hasher.update(&ciphertext);
    let expected_hash = hasher.finalize().to_hex().to_string();
    if expected_hash != req.content_hash.trim() {
        return Err(AuthError::SyncEnvelopeInvalid);
    }

    let mut fp = blake3::Hasher::new();
    fp.update(b"safi-sync-push-fp-v1:");
    fp.update(resource_kind.as_bytes());
    fp.update(b":");
    fp.update(resource_id.as_bytes());
    fp.update(b":");
    fp.update(&req.expected_version.to_le_bytes());
    fp.update(&req.key_epoch.to_le_bytes());
    fp.update(&[u8::from(req.deleted)]);
    fp.update(key_id.as_bytes());
    fp.update(expected_hash.as_bytes());
    let request_fingerprint = fp.finalize().to_hex().to_string();

    Ok(ValidatedPushEnvelope {
        resource_kind: resource_kind.to_owned(),
        resource_id: resource_id.to_owned(),
        expected_version: req.expected_version,
        ciphertext_b64: b64.encode(&ciphertext),
        ciphertext,
        nonce_b64: b64.encode(&nonce),
        nonce,
        key_id: key_id.to_owned(),
        key_epoch: req.key_epoch,
        content_hash: expected_hash,
        deleted: req.deleted,
        origin_device_id: origin_device_id.to_owned(),
        idempotency_key,
        request_fingerprint,
    })
}

/// Zero-knowledge encrypted sync & share-link repository service.
#[derive(Debug, Clone, Default)]
pub struct EncryptedSyncService {
    store: Arc<RwLock<SyncStore>>,
    pg_store: Option<PgControlPlaneStore>,
    require_durable_pg: bool,
}

impl EncryptedSyncService {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_pg_store(mut self, pg_store: PgControlPlaneStore) -> Self {
        self.pg_store = Some(pg_store);
        self
    }

    #[must_use]
    pub fn with_require_durable_pg(mut self, required: bool) -> Self {
        self.require_durable_pg = required;
        self
    }

    fn ensure_durable_ready(&self) -> Result<(), AuthError> {
        if self.require_durable_pg && self.pg_store.is_none() {
            return Err(AuthError::BackendUnavailable);
        }
        Ok(())
    }

    pub async fn export_snapshot(&self) -> EncryptedSyncSnapshot {
        let guard = self.store.read().await;
        EncryptedSyncSnapshot {
            records: guard.records.values().cloned().collect(),
            share_links: guard.share_links.values().cloned().collect(),
        }
    }

    pub async fn import_snapshot(&self, snap: EncryptedSyncSnapshot) {
        let mut guard = self.store.write().await;
        guard.records.clear();
        guard.workspace_seq.clear();
        for r in snap.records {
            let cur = guard.workspace_seq.entry(r.workspace_id).or_insert(0);
            if r.workspace_seq > *cur {
                *cur = r.workspace_seq;
            }
            guard.records.insert(
                (
                    r.workspace_id,
                    r.resource_kind.clone(),
                    r.resource_id.clone(),
                ),
                r,
            );
        }
        guard.share_links = snap
            .share_links
            .into_iter()
            .map(|s| (s.token_hash.clone(), s))
            .collect();
    }

    /// Push a client-encrypted envelope enforcing minimum active key epoch, integrity hash,
    /// nonce uniqueness, idempotency replay safety, and optimistic concurrency (`SF-SYN-002`).
    pub async fn push_envelope_with_min_epoch(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        req: &PushEncryptedSyncRequest,
        min_key_epoch: u32,
    ) -> Result<EncryptedSyncRecord, AuthError> {
        self.ensure_durable_ready()?;
        let validated = validate_push_request(req)?;
        if validated.key_epoch < min_key_epoch {
            return Err(AuthError::SyncVersionConflict);
        }

        if let Some(pg) = &self.pg_store {
            return encrypted_sync_pg::push_envelope_pg(
                pg,
                workspace_id,
                user_id,
                &validated,
                min_key_epoch,
                unix_now(),
            )
            .await;
        }

        let key = (
            workspace_id,
            validated.resource_kind.clone(),
            validated.resource_id.clone(),
        );
        let mut guard = self.store.write().await;
        if let Some(existing) = guard.records.get(&key) {
            if let Some(ref idem) = validated.idempotency_key {
                if existing.last_idempotency_key.as_deref() == Some(idem.as_str()) {
                    if existing.last_request_fingerprint.as_deref()
                        == Some(validated.request_fingerprint.as_str())
                    {
                        return Ok(existing.clone());
                    }
                    return Err(AuthError::IdempotencyConflict);
                }
            }
            if validated.expected_version != existing.version {
                return Err(AuthError::SyncVersionConflict);
            }
            // Reject ChaCha20-Poly1305 12-byte nonce reuse across distinct versions under the same key epoch!
            if existing.key_epoch == validated.key_epoch
                && existing.nonce_b64 == validated.nonce_b64
            {
                return Err(AuthError::SyncEnvelopeInvalid);
            }
        } else if validated.expected_version != 0 {
            return Err(AuthError::SyncVersionConflict);
        }

        let next_seq = {
            let seq = guard.workspace_seq.entry(workspace_id).or_insert(0);
            *seq += 1;
            *seq
        };
        let next_version = validated.expected_version + 1;
        let record_id = guard.records.get(&key).map_or_else(Uuid::now_v7, |r| r.id);
        let record = EncryptedSyncRecord {
            id: record_id,
            workspace_id,
            user_id,
            resource_kind: validated.resource_kind,
            resource_id: validated.resource_id,
            version: next_version,
            workspace_seq: next_seq,
            ciphertext_b64: validated.ciphertext_b64,
            nonce_b64: validated.nonce_b64,
            key_id: validated.key_id,
            key_epoch: validated.key_epoch,
            content_hash: validated.content_hash,
            deleted: validated.deleted,
            origin_device_id: validated.origin_device_id,
            updated_at: unix_now(),
            last_idempotency_key: validated.idempotency_key,
            last_request_fingerprint: Some(validated.request_fingerprint),
        };
        guard.records.insert(key, record.clone());
        Ok(record)
    }

    /// Push a client-encrypted envelope with default minimum epoch `1`.
    pub async fn push_envelope(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        req: &PushEncryptedSyncRequest,
    ) -> Result<EncryptedSyncRecord, AuthError> {
        self.push_envelope_with_min_epoch(workspace_id, user_id, req, 1)
            .await
    }

    /// Fetch the latest encrypted envelope for a workspace resource.
    pub async fn pull_envelope_checked(
        &self,
        workspace_id: Uuid,
        resource_kind: &str,
        resource_id: &str,
    ) -> Result<Option<EncryptedSyncRecord>, AuthError> {
        self.ensure_durable_ready()?;
        if let Some(pg) = &self.pg_store {
            return encrypted_sync_pg::pull_envelope_pg(
                pg,
                workspace_id,
                resource_kind.trim(),
                resource_id.trim(),
            )
            .await;
        }
        Ok(self
            .store
            .read()
            .await
            .records
            .get(&(
                workspace_id,
                resource_kind.trim().to_owned(),
                resource_id.trim().to_owned(),
            ))
            .cloned())
    }

    /// Convenience pull wrapper preserving existing unit-test signature.
    pub async fn pull_envelope(
        &self,
        workspace_id: Uuid,
        resource_kind: &str,
        resource_id: &str,
    ) -> Option<EncryptedSyncRecord> {
        self.pull_envelope_checked(workspace_id, resource_kind, resource_id)
            .await
            .ok()
            .flatten()
    }

    /// List incremental encrypted workspace changes (`workspace_seq > since_seq`) including tombstones.
    pub async fn list_changes_since(
        &self,
        workspace_id: Uuid,
        since_seq: u64,
        limit: usize,
    ) -> Result<SyncChangesResponse, AuthError> {
        self.ensure_durable_ready()?;
        let clamped_limit = limit.clamp(1, 500);
        if let Some(pg) = &self.pg_store {
            return encrypted_sync_pg::list_changes_since_pg(
                pg,
                workspace_id,
                since_seq,
                clamped_limit,
            )
            .await;
        }
        let guard = self.store.read().await;
        let latest_seq = guard.workspace_seq.get(&workspace_id).copied().unwrap_or(0);
        let mut matching: Vec<EncryptedSyncRecord> = guard
            .records
            .values()
            .filter(|r| r.workspace_id == workspace_id && r.workspace_seq > since_seq)
            .cloned()
            .collect();
        matching.sort_by_key(|r| r.workspace_seq);
        let has_more = matching.len() > clamped_limit;
        if has_more {
            matching.truncate(clamped_limit);
        }
        Ok(SyncChangesResponse {
            workspace_id,
            latest_workspace_seq: latest_seq,
            has_more,
            records: matching,
        })
    }

    /// Create an encrypted share link carrying only a client-wrapped key (server never sees raw key).
    pub async fn create_share_link(
        &self,
        workspace_id: Uuid,
        created_by: Uuid,
        req: &CreateShareLinkRequest,
    ) -> Result<CreatedShareLink, AuthError> {
        self.ensure_durable_ready()?;
        if req.resource_id.trim().is_empty()
            || req.resource_id.trim().len() > 256
            || req.ttl_secs <= 0
            || req.ttl_secs > 90 * 86400
        {
            return Err(AuthError::InvalidRequest);
        }
        let b64 = base64::engine::general_purpose::STANDARD;
        let wrapped = b64
            .decode(req.wrapped_key_ciphertext_b64.trim())
            .map_err(|_| AuthError::SyncEnvelopeInvalid)?;
        let nonce = b64
            .decode(req.wrapped_key_nonce_b64.trim())
            .map_err(|_| AuthError::SyncEnvelopeInvalid)?;
        if nonce.len() != NONCE_BYTES
            || wrapped.len() < MIN_CIPHERTEXT_BYTES
            || wrapped.len() > 4096
        {
            return Err(AuthError::SyncEnvelopeInvalid);
        }

        let token = format!(
            "sfsh_{}{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        );
        let token_hash = blake3::hash(token.as_bytes()).to_hex().to_string();
        let now = unix_now();
        let expires_at = now + req.ttl_secs;

        if let Some(pg) = &self.pg_store {
            let id = encrypted_sync_pg::create_share_link_pg(
                pg,
                workspace_id,
                created_by,
                req.resource_id.trim(),
                &token_hash,
                &wrapped,
                &nonce,
                now,
                expires_at,
            )
            .await?;
            return Ok(CreatedShareLink {
                id,
                share_token: token,
                expires_at,
            });
        }

        let record = ShareLinkRecord {
            id: Uuid::now_v7(),
            workspace_id,
            created_by,
            resource_id: req.resource_id.trim().to_owned(),
            token_hash: token_hash.clone(),
            wrapped_key_ciphertext_b64: b64.encode(&wrapped),
            wrapped_key_nonce_b64: b64.encode(&nonce),
            expires_at,
            revoked_at: None,
            created_at: now,
        };
        let id = record.id;
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
        self.ensure_durable_ready()?;
        let token_hash = blake3::hash(token.trim().as_bytes()).to_hex().to_string();
        if let Some(pg) = &self.pg_store {
            return encrypted_sync_pg::resolve_share_link_pg(pg, &token_hash, now_secs).await;
        }
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
        self.ensure_durable_ready()?;
        let now = unix_now();
        if let Some(pg) = &self.pg_store {
            return encrypted_sync_pg::revoke_share_link_pg(pg, workspace_id, link_id, now).await;
        }
        let mut guard = self.store.write().await;
        let rec = guard
            .share_links
            .values_mut()
            .find(|r| r.id == link_id && r.workspace_id == workspace_id)
            .ok_or(AuthError::ShareLinkInvalid)?;
        rec.revoked_at = Some(now);
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
                    key_epoch: 1,
                    deleted: false,
                    origin_device_id: "dev-a".to_owned(),
                    idempotency_key: Some("idem-push-1".to_owned()),
                },
            )
            .await
            .expect("push v1");
        assert_eq!(pushed.version, 1);
        assert_eq!(pushed.workspace_seq, 1);
        assert!(!pushed.ciphertext_b64.contains("CONFIDENTIAL"));

        // Exact retry with same idempotency_key + payload returns committed v1 instead of conflict!
        let replayed = svc
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
                    key_epoch: 1,
                    deleted: false,
                    origin_device_id: "dev-a".to_owned(),
                    idempotency_key: Some("idem-push-1".to_owned()),
                },
            )
            .await
            .expect("idempotent replay");
        assert_eq!(replayed.version, 1);
        assert_eq!(replayed.workspace_seq, 1);

        // Reusing idempotency_key with a different expected_version / payload fails closed (`SF-PAY-007`)!
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
                    content_hash: content_hash.clone(),
                    key_epoch: 1,
                    deleted: false,
                    origin_device_id: "dev-a".to_owned(),
                    idempotency_key: Some("idem-push-1".to_owned()),
                },
            )
            .await
            .unwrap_err(),
            AuthError::IdempotencyConflict
        );

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
                    content_hash: content_hash.clone(),
                    key_epoch: 1,
                    deleted: false,
                    origin_device_id: "dev-b".to_owned(),
                    idempotency_key: None,
                },
            )
            .await
            .unwrap_err(),
            AuthError::SyncVersionConflict
        );

        // Nonce reuse on v2 under the same key epoch is rejected (`SF-SYN-001`)!
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
                    content_hash,
                    key_epoch: 1,
                    deleted: false,
                    origin_device_id: "dev-a".to_owned(),
                    idempotency_key: Some("idem-push-2".to_owned()),
                },
            )
            .await
            .unwrap_err(),
            AuthError::SyncEnvelopeInvalid
        );

        // Tampered digest rejected (`SF-SYN-001`).
        let nonce_v2 = [0x22u8; 12];
        assert_eq!(
            svc.push_envelope(
                ws,
                user,
                &PushEncryptedSyncRequest {
                    resource_kind: "project".to_owned(),
                    resource_id: "proj_01".to_owned(),
                    expected_version: 1,
                    ciphertext_b64: b64.encode(&buf),
                    nonce_b64: b64.encode(nonce_v2),
                    key_id: "client-key-v1".to_owned(),
                    content_hash: "0".repeat(64),
                    key_epoch: 1,
                    deleted: false,
                    origin_device_id: "dev-a".to_owned(),
                    idempotency_key: None,
                },
            )
            .await
            .unwrap_err(),
            AuthError::SyncEnvelopeInvalid
        );

        let changes = svc.list_changes_since(ws, 0, 50).await.expect("changes");
        assert_eq!(changes.records.len(), 1);
        assert_eq!(changes.latest_workspace_seq, 1);

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
