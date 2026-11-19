//! Authoritative Single-Active-Device Registry, Ed25519 Cryptographic Device Proof,
//! Monotonic Session Revocation Generation, Audited Owner Device Reset, and
//! Short-Lived Server-Signed Online Authorization Leases (Sections 4 & 5).
//!
//! Security policy:
//! - Exactly ONE active device per account at any time.
//! - A successful new-device login atomically revokes the previous device, revokes all
//!   previous sessions, increments `session_generation`, registers the new device, and
//!   appends a `DeviceReplacementEvent` record.
//! - Concurrent login attempts serialize under one atomic lock (and `pg_advisory_xact_lock`
//!   in PostgreSQL) so two active devices can NEVER coexist.
//! - Online authorization leases have a strict 60-second TTL (`ONLINE_LEASE_TTL_SECS`) with
//!   a recommended 30-second heartbeat (`ONLINE_HEARTBEAT_SECS`) and are Ed25519-signed.

use std::collections::HashMap;
use std::sync::Arc;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::auth::unix_now;
use crate::entitlements::EffectiveEntitlements;
use crate::error::AuthError;

/// Strict online authorization lease lifetime (60 seconds).
pub const ONLINE_LEASE_TTL_SECS: i64 = 60;
/// Recommended Desktop authorization heartbeat interval (30 seconds).
pub const ONLINE_HEARTBEAT_SECS: i64 = 30;
/// Maximum allowed clock skew for signed Ed25519 device proof headers (120 seconds).
pub const DEVICE_PROOF_SKEW_SECS: i64 = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceStatus {
    Active,
    ReplacedByNewDevice,
    RevokedByOwner,
    SignedOut,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredDeviceRecord {
    pub device_id: Uuid,
    pub user_id: Uuid,
    pub workspace_id: Uuid,
    pub device_name: String,
    pub installation_id: Uuid,
    pub public_key_hex: Option<String>,
    pub active_session_id: Option<Uuid>,
    pub session_generation: u64,
    pub status: DeviceStatus,
    pub registered_at: i64,
    pub last_seen_at: i64,
    pub revoked_at: Option<i64>,
    pub replaced_by_device_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceReplacementEvent {
    pub id: Uuid,
    pub user_id: Uuid,
    pub previous_device_id: Option<Uuid>,
    pub new_device_id: Option<Uuid>,
    pub trigger_kind: String,
    pub actor_user_id: Option<Uuid>,
    pub reason: String,
    pub new_session_generation: u64,
    pub created_at: i64,
}

/// Short-lived server-authoritative online authorization lease (60s TTL).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OnlineAuthorizationLeasePayload {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub workspace_id: Uuid,
    pub device_id: Option<Uuid>,
    pub session_generation: u64,
    pub server_time_secs: i64,
    pub expires_at_secs: i64,
    pub heartbeat_interval_secs: i64,
    pub can_execute_protected_ops: bool,
    pub entitlements_digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedOnlineAuthorizationLease {
    pub payload: OnlineAuthorizationLeasePayload,
    pub signature_hex: Option<String>,
    pub verifying_key_hex: Option<String>,
    pub entitlements: EffectiveEntitlements,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceAuthoritySnapshot {
    pub session_generations: HashMap<Uuid, u64>,
    pub devices: Vec<RegisteredDeviceRecord>,
    pub revoked_session_reasons: HashMap<Uuid, DeviceStatus>,
    #[serde(default)]
    pub revoked_token_reasons: HashMap<String, DeviceStatus>,
    #[serde(default)]
    pub active_token_hashes: HashMap<Uuid, String>,
    pub replacement_events: Vec<DeviceReplacementEvent>,
}

/// Verify an Ed25519 cryptographic device proof header bound to the active device public key,
/// session token hash, and current timestamp (`SF-AUTH-020`).
pub fn verify_device_ed25519_proof(
    public_key_hex: &str,
    session_token_hash: &str,
    timestamp_secs: i64,
    signature_hex: &str,
    now_secs: i64,
) -> Result<(), AuthError> {
    if (now_secs - timestamp_secs).abs() > DEVICE_PROOF_SKEW_SECS {
        return Err(AuthError::DeviceProofInvalid);
    }
    let pk_bytes = hex::decode(public_key_hex.trim()).map_err(|_| AuthError::DeviceProofInvalid)?;
    let pk_arr: [u8; 32] = pk_bytes
        .try_into()
        .map_err(|_| AuthError::DeviceProofInvalid)?;
    let verifying_key =
        VerifyingKey::from_bytes(&pk_arr).map_err(|_| AuthError::DeviceProofInvalid)?;

    let sig_bytes = hex::decode(signature_hex.trim()).map_err(|_| AuthError::DeviceProofInvalid)?;
    let signature = Signature::from_slice(&sig_bytes).map_err(|_| AuthError::DeviceProofInvalid)?;

    let canonical = format!("safidata-device-proof-v1|{session_token_hash}|{timestamp_secs}");
    verifying_key
        .verify(canonical.as_bytes(), &signature)
        .map_err(|_| AuthError::DeviceProofInvalid)
}

#[derive(Debug, Default)]
struct DeviceAuthorityStore {
    session_generations: HashMap<Uuid, u64>,
    devices: Vec<RegisteredDeviceRecord>,
    revoked_session_reasons: HashMap<Uuid, DeviceStatus>,
    revoked_token_reasons: HashMap<String, DeviceStatus>,
    active_token_hashes: HashMap<Uuid, String>,
    replacement_events: Vec<DeviceReplacementEvent>,
}

/// Server-authoritative single-active-device & lease service.
#[derive(Debug, Clone, Default)]
pub struct DeviceAuthorityService {
    store: Arc<RwLock<DeviceAuthorityStore>>,
}

impl DeviceAuthorityService {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn export_snapshot(&self) -> DeviceAuthoritySnapshot {
        let guard = self.store.read().await;
        DeviceAuthoritySnapshot {
            session_generations: guard.session_generations.clone(),
            devices: guard.devices.clone(),
            revoked_session_reasons: guard.revoked_session_reasons.clone(),
            revoked_token_reasons: guard.revoked_token_reasons.clone(),
            active_token_hashes: guard.active_token_hashes.clone(),
            replacement_events: guard.replacement_events.clone(),
        }
    }

    pub async fn import_snapshot(&self, snap: DeviceAuthoritySnapshot) {
        let mut guard = self.store.write().await;
        guard.session_generations = snap.session_generations;
        guard.devices = snap.devices;
        guard.revoked_session_reasons = snap.revoked_session_reasons;
        guard.revoked_token_reasons = snap.revoked_token_reasons;
        guard.active_token_hashes = snap.active_token_hashes;
        guard.replacement_events = snap.replacement_events;
    }

    /// Atomically replace any existing active device for `user_id` with this new login,
    /// advancing `session_generation` and marking every previous active device & session
    /// as `ReplacedByNewDevice` (`SF-AUTH-019`).
    #[allow(clippy::too_many_arguments)]
    pub async fn activate_device_atomically(
        &self,
        user_id: Uuid,
        workspace_id: Uuid,
        device_name: &str,
        installation_id: Uuid,
        public_key_hex: Option<String>,
        new_session_id: Uuid,
        new_token_hash: String,
    ) -> Result<(RegisteredDeviceRecord, Option<Uuid>), AuthError> {
        if let Some(ref pk) = public_key_hex {
            let bytes = hex::decode(pk.trim()).map_err(|_| AuthError::DeviceProofInvalid)?;
            let arr: [u8; 32] = bytes
                .try_into()
                .map_err(|_| AuthError::DeviceProofInvalid)?;
            VerifyingKey::from_bytes(&arr).map_err(|_| AuthError::DeviceProofInvalid)?;
        }

        let now = unix_now();
        let mut guard = self.store.write().await;

        let next_gen = guard
            .session_generations
            .get(&user_id)
            .copied()
            .unwrap_or(0)
            .saturating_add(1);
        guard.session_generations.insert(user_id, next_gen);

        let new_device_id = Uuid::now_v7();
        let mut previous_device_id: Option<Uuid> = None;
        let mut sessions_to_revoke: Vec<Uuid> = Vec::new();

        for dev in guard.devices.iter_mut().filter(|d| d.user_id == user_id) {
            if dev.status == DeviceStatus::Active {
                previous_device_id = Some(dev.device_id);
                if let Some(old_sid) = dev.active_session_id {
                    sessions_to_revoke.push(old_sid);
                }
                dev.status = DeviceStatus::ReplacedByNewDevice;
                dev.revoked_at = Some(now);
                dev.replaced_by_device_id = Some(new_device_id);
                dev.active_session_id = None;
            }
        }
        for old_sid in sessions_to_revoke {
            guard
                .revoked_session_reasons
                .insert(old_sid, DeviceStatus::ReplacedByNewDevice);
            if let Some(old_hash) = guard.active_token_hashes.remove(&old_sid) {
                guard
                    .revoked_token_reasons
                    .insert(old_hash, DeviceStatus::ReplacedByNewDevice);
            }
        }
        guard
            .active_token_hashes
            .insert(new_session_id, new_token_hash);

        let active_record = RegisteredDeviceRecord {
            device_id: new_device_id,
            user_id,
            workspace_id,
            device_name: device_name.trim().to_owned(),
            installation_id,
            public_key_hex: public_key_hex.map(|s| s.trim().to_ascii_lowercase()),
            active_session_id: Some(new_session_id),
            session_generation: next_gen,
            status: DeviceStatus::Active,
            registered_at: now,
            last_seen_at: now,
            revoked_at: None,
            replaced_by_device_id: None,
        };
        guard.devices.push(active_record.clone());

        guard.replacement_events.push(DeviceReplacementEvent {
            id: Uuid::now_v7(),
            user_id,
            previous_device_id,
            new_device_id: Some(new_device_id),
            trigger_kind: "new_device_login".to_owned(),
            actor_user_id: Some(user_id),
            reason: if previous_device_id.is_some() {
                "automatic_single_device_replacement".to_owned()
            } else {
                "initial_device_registration".to_owned()
            },
            new_session_generation: next_gen,
            created_at: now,
        });

        Ok((active_record, previous_device_id))
    }

    /// Audited Owner administrative device reset: revokes all devices & sessions for `target_user_id`,
    /// bumps `session_generation`, and appends an audit event.
    pub async fn owner_reset_devices(
        &self,
        owner_user_id: Uuid,
        target_user_id: Uuid,
        reason: &str,
    ) -> Result<DeviceReplacementEvent, AuthError> {
        let trimmed = reason.trim();
        if trimmed.is_empty() || trimmed.len() > 240 {
            return Err(AuthError::InvalidRequest);
        }
        let now = unix_now();
        let mut guard = self.store.write().await;
        let next_gen = guard
            .session_generations
            .get(&target_user_id)
            .copied()
            .unwrap_or(0)
            .saturating_add(1);
        guard.session_generations.insert(target_user_id, next_gen);

        let mut prev_dev = None;
        let mut revoked_sids = Vec::new();
        for dev in guard
            .devices
            .iter_mut()
            .filter(|d| d.user_id == target_user_id && d.status == DeviceStatus::Active)
        {
            prev_dev = Some(dev.device_id);
            if let Some(sid) = dev.active_session_id {
                revoked_sids.push(sid);
            }
            dev.status = DeviceStatus::RevokedByOwner;
            dev.revoked_at = Some(now);
            dev.active_session_id = None;
        }
        for sid in revoked_sids {
            guard
                .revoked_session_reasons
                .insert(sid, DeviceStatus::RevokedByOwner);
            if let Some(old_hash) = guard.active_token_hashes.remove(&sid) {
                guard
                    .revoked_token_reasons
                    .insert(old_hash, DeviceStatus::RevokedByOwner);
            }
        }

        let event = DeviceReplacementEvent {
            id: Uuid::now_v7(),
            user_id: target_user_id,
            previous_device_id: prev_dev,
            new_device_id: None,
            trigger_kind: "owner_reset".to_owned(),
            actor_user_id: Some(owner_user_id),
            reason: trimmed.to_owned(),
            new_session_generation: next_gen,
            created_at: now,
        };
        guard.replacement_events.push(event.clone());
        Ok(event)
    }

    /// Verify whether a session is still the sole active device for its account and matches
    /// the latest `session_generation` (and optional Ed25519 device proof).
    pub async fn verify_active_session_device(
        &self,
        user_id: Uuid,
        session_id: Uuid,
        session_token_hash: &str,
        device_sig_header: Option<(&str, i64)>,
        now_secs: i64,
    ) -> Result<(Option<RegisteredDeviceRecord>, u64), AuthError> {
        let mut guard = self.store.write().await;

        if let Some(reason) = guard.revoked_session_reasons.get(&session_id).copied() {
            return Err(match reason {
                DeviceStatus::ReplacedByNewDevice => AuthError::DeviceReplaced,
                _ => AuthError::Revoked,
            });
        }

        let current_gen = guard
            .session_generations
            .get(&user_id)
            .copied()
            .unwrap_or(1);

        // If the user has registered devices, the presenting session MUST be the single active one.
        let has_any_device = guard.devices.iter().any(|d| d.user_id == user_id);
        if !has_any_device {
            return Ok((None, current_gen));
        }

        let active = guard
            .devices
            .iter_mut()
            .find(|d| d.user_id == user_id && d.status == DeviceStatus::Active)
            .ok_or(AuthError::Revoked)?;

        if active.active_session_id != Some(session_id) || active.session_generation != current_gen
        {
            return Err(AuthError::DeviceReplaced);
        }

        if let Some(ref pk_hex) = active.public_key_hex {
            let Some((sig_hex, ts_secs)) = device_sig_header else {
                return Err(AuthError::DeviceProofInvalid);
            };
            verify_device_ed25519_proof(pk_hex, session_token_hash, ts_secs, sig_hex, now_secs)?;
        }

        active.last_seen_at = now_secs;
        Ok((Some(active.clone()), current_gen))
    }

    /// Inspect all registered devices (active & historic) for a user (Owner Control Center).
    pub async fn list_devices_for_user(&self, user_id: Uuid) -> Vec<RegisteredDeviceRecord> {
        self.store
            .read()
            .await
            .devices
            .iter()
            .filter(|d| d.user_id == user_id)
            .cloned()
            .collect()
    }

    /// Inspect replacement history for a user.
    pub async fn list_replacements_for_user(&self, user_id: Uuid) -> Vec<DeviceReplacementEvent> {
        self.store
            .read()
            .await
            .replacement_events
            .iter()
            .filter(|e| e.user_id == user_id)
            .cloned()
            .collect()
    }

    /// Inspect why a token hash was revoked (`ReplacedByNewDevice` -> `SF-AUTH-019` vs `RevokedByOwner` -> `SF-AUTH-003`).
    pub async fn revocation_reason_for_token_hash(&self, token_hash: &str) -> Option<DeviceStatus> {
        self.store
            .read()
            .await
            .revoked_token_reasons
            .get(token_hash)
            .copied()
    }

    /// Return all registered devices across the platform (for Owner Control Center overview).
    pub async fn list_all_devices(&self) -> Vec<RegisteredDeviceRecord> {
        self.store.read().await.devices.clone()
    }

    /// Return recent device replacement events across the platform.
    pub async fn list_recent_replacements(&self, limit: usize) -> Vec<DeviceReplacementEvent> {
        let guard = self.store.read().await;
        guard
            .replacement_events
            .iter()
            .rev()
            .take(limit.clamp(1, 200))
            .cloned()
            .collect()
    }

    /// Count currently active devices for an account (invariant: always `<= 1`).
    pub async fn active_device_count(&self, user_id: Uuid) -> usize {
        self.store
            .read()
            .await
            .devices
            .iter()
            .filter(|d| d.user_id == user_id && d.status == DeviceStatus::Active)
            .count()
    }

    /// Issue a short-lived (60s) Ed25519-signed Online Authorization Lease.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn sign_online_lease(
        session_id: Uuid,
        user_id: Uuid,
        workspace_id: Uuid,
        device_id: Option<Uuid>,
        session_generation: u64,
        entitlements: EffectiveEntitlements,
        can_execute_protected_ops: bool,
        signing_seed: Option<[u8; 32]>,
        now_secs: i64,
    ) -> SignedOnlineAuthorizationLease {
        let ent_json = serde_json::to_vec(&entitlements).unwrap_or_default();
        let entitlements_digest = blake3::hash(&ent_json).to_hex().to_string();
        let payload = OnlineAuthorizationLeasePayload {
            session_id,
            user_id,
            workspace_id,
            device_id,
            session_generation,
            server_time_secs: now_secs,
            expires_at_secs: now_secs + ONLINE_LEASE_TTL_SECS,
            heartbeat_interval_secs: ONLINE_HEARTBEAT_SECS,
            can_execute_protected_ops,
            entitlements_digest,
        };
        let (signature_hex, verifying_key_hex) = match signing_seed {
            Some(seed) => {
                let key = SigningKey::from_bytes(&seed);
                let bytes = serde_json::to_vec(&payload).unwrap_or_default();
                let sig = key.sign(&bytes);
                (
                    Some(hex::encode(sig.to_bytes())),
                    Some(hex::encode(VerifyingKey::from(&key).to_bytes())),
                )
            }
            None => (None, None),
        };
        SignedOnlineAuthorizationLease {
            payload,
            signature_hex,
            verifying_key_hex,
            entitlements,
        }
    }
}
