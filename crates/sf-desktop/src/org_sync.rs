//! Production Client-Side End-to-End Encrypted Organization Sync Engine (`sf-desktop`).
//!
//! Architecture & Cryptographic Guarantees:
//! - 100% network-free inside `sf-desktop` (`tools/check-no-network.sh`): native HTTPS transport
//!   lives exclusively in `apps/desktop/src-tauri`, while all key generation, X25519 key agreement,
//!   HKDF-SHA256 key derivation, ChaCha20-Poly1305 AEAD encryption/decryption, BLAKE3 integrity
//!   verification, encrypted offline queue persistence, and conflict resolution execute here.
//! - Private X25519 device keys and unwrapped 256-bit Workspace Keys NEVER leave the device in
//!   plaintext, NEVER appear in `Debug` output or logs, are zeroized on drop (`Zeroize`), and are
//!   stored at rest exclusively inside `<storage_dir>/org_sync_vault.safi.enc` sealed with the
//!   OS-keystore-backed installation `VaultKey` (`SFK2`).

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine;
use curve25519_dalek::montgomery::MontgomeryPoint;
use ring::aead::{Aad, CHACHA20_POLY1305, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use ring::hkdf::{HKDF_SHA256, KeyType, Salt};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::vault::{VaultKey, decrypt_vault_payload, encrypt_vault_payload};

const SYNC_VAULT_FILENAME: &str = "org_sync_vault.safi.enc";
const AEAD_TAG_LEN: usize = 16;
const WORKSPACE_KEY_LEN: usize = 32;
const WRAPPED_KEY_CIPHERTEXT_LEN: usize = WORKSPACE_KEY_LEN + AEAD_TAG_LEN; // 48 bytes

struct HkdfLen32;
impl KeyType for HkdfLen32 {
    fn len(&self) -> usize {
        32
    }
}

fn unix_secs_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(0))
        .unwrap_or(0)
}

fn fill_csprng(buf: &mut [u8]) -> Result<(), String> {
    SystemRandom::new()
        .fill(buf)
        .map_err(|_| "SF-SYN-001".to_owned())
}

/// Public device identity safe to display in UI and register with `/v1/organizations/{id}/keys/devices`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicDeviceSyncIdentity {
    pub device_id: String,
    pub device_label: String,
    pub x25519_public_key_b64: String,
    pub key_fingerprint: String,
    pub created_at: i64,
}

/// Member device public key returned by the server directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteMemberDeviceKey {
    pub user_id: String,
    pub device_id: String,
    pub device_label: String,
    pub x25519_public_key_b64: String,
    pub key_fingerprint: String,
}

/// Wire payload uploaded to `/v1/organizations/{id}/keys/wrap` or `/rotate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrappedKeyWireEntry {
    pub recipient_user_id: String,
    pub recipient_device_id: String,
    pub sender_ephemeral_pub_b64: String,
    pub wrapped_key_ciphertext_b64: String,
    pub wrapped_key_nonce_b64: String,
}

/// Wire payload downloaded from `/v1/organizations/{id}/keys/my-wrapped`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteWrappedKeyEnvelope {
    pub organization_id: String,
    pub workspace_id: String,
    pub key_epoch: u32,
    pub recipient_user_id: String,
    pub recipient_device_id: String,
    pub sender_ephemeral_pub_b64: String,
    pub wrapped_key_ciphertext_b64: String,
    pub wrapped_key_nonce_b64: String,
}

/// Zero-knowledge encrypted envelope sent to `POST /v1/sync/envelopes`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboundEncryptedEnvelope {
    pub workspace_id: String,
    pub resource_kind: String,
    pub resource_id: String,
    pub expected_version: u64,
    pub ciphertext_b64: String,
    pub nonce_b64: String,
    pub key_id: String,
    pub key_epoch: u32,
    pub content_hash: String,
    pub deleted: bool,
    pub origin_device_id: String,
    pub idempotency_key: String,
}

/// Encrypted envelope received from `GET /v1/sync/changes` or `GET /v1/sync/envelopes/...`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboundEncryptedEnvelope {
    pub id: String,
    pub workspace_id: String,
    pub user_id: String,
    pub resource_kind: String,
    pub resource_id: String,
    pub version: u64,
    pub workspace_seq: u64,
    pub ciphertext_b64: String,
    pub nonce_b64: String,
    pub key_id: String,
    pub key_epoch: u32,
    pub content_hash: String,
    pub deleted: bool,
    pub origin_device_id: String,
    pub updated_at: i64,
}

/// Decrypted shared resource stored inside the local encrypted Desktop vault.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedWorkspaceResource {
    pub workspace_id: String,
    pub resource_kind: String,
    pub resource_id: String,
    pub title: String,
    pub payload_json: String,
    pub version: u64,
    pub key_epoch: u32,
    pub deleted: bool,
    pub dirty: bool,
    pub last_author_user_id: Option<String>,
    pub origin_device_id: String,
    pub updated_at: i64,
}

/// Interactive conflict surfaced when an optimistic concurrency mismatch (`409 SF-SYN-002`)
/// or concurrent offline edit is detected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncConflictItem {
    pub workspace_id: String,
    pub resource_kind: String,
    pub resource_id: String,
    pub local_title: String,
    pub local_payload_json: String,
    pub local_base_version: u64,
    pub remote_title: String,
    pub remote_payload_json: String,
    pub remote_version: u64,
    pub remote_deleted: bool,
    pub remote_user_id: String,
    pub detected_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolutionChoice {
    KeepLocal,
    AcceptRemote,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncHealthState {
    Idle,
    Synced,
    OfflineQueued,
    ConflictDetected,
    KeyRequired,
    Error,
}

/// Complete secret-free status snapshot returned to the Desktop UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopWorkspaceSyncStatus {
    pub active_organization_id: Option<String>,
    pub active_workspace_id: Option<String>,
    pub device_identity: PublicDeviceSyncIdentity,
    pub active_key_epoch: u32,
    pub unlocked_epochs: Vec<u32>,
    pub last_workspace_seq: u64,
    pub pending_offline_count: usize,
    pub conflict_count: usize,
    pub resource_count: usize,
    pub health: SyncHealthState,
    pub last_synced_at: Option<i64>,
    pub last_error_code: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct PersistedPlaintextEnvelopePayload {
    title: String,
    payload_json: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct QueuedOfflineMutation {
    idempotency_key: String,
    organization_id: String,
    workspace_id: String,
    resource_kind: String,
    resource_id: String,
    expected_version: u64,
    deleted: bool,
    title: String,
    payload_json: String,
    cached_envelope: Option<OutboundEncryptedEnvelope>,
    created_at: i64,
}

#[derive(Clone, Serialize, Deserialize, Zeroize)]
#[zeroize(drop)]
struct SecretKeyBytes32([u8; 32]);

impl fmt::Debug for SecretKeyBytes32 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED_32B_KEY]")
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct WorkspaceVaultState {
    organization_id: String,
    workspace_id: String,
    active_key_epoch: u32,
    epoch_keys: BTreeMap<u32, SecretKeyBytes32>,
    last_workspace_seq: u64,
    resources: BTreeMap<String, SharedWorkspaceResource>,
    conflicts: BTreeMap<String, SyncConflictItem>,
    last_synced_at: Option<i64>,
    last_error_code: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct PersistedSyncVault {
    device_id: String,
    device_label: String,
    device_x25519_secret: SecretKeyBytes32,
    device_x25519_public_b64: String,
    device_key_fingerprint: String,
    device_created_at: i64,
    active_organization_id: Option<String>,
    active_workspace_id: Option<String>,
    workspaces: BTreeMap<String, WorkspaceVaultState>,
    offline_queue: VecDeque<QueuedOfflineMutation>,
}

fn resource_map_key(kind: &str, id: &str) -> String {
    format!("{}:{}", kind.trim(), id.trim())
}

fn build_sync_aad(
    workspace_id: &str,
    resource_kind: &str,
    resource_id: &str,
    key_epoch: u32,
    deleted: bool,
) -> String {
    format!(
        "safi-e2ee-sync-v1:{}:{}:{}:{}:{}",
        workspace_id.trim(),
        resource_kind.trim(),
        resource_id.trim(),
        key_epoch,
        u8::from(deleted)
    )
}

fn build_wrap_info(
    organization_id: &str,
    workspace_id: &str,
    key_epoch: u32,
    recipient_user_id: &str,
    recipient_device_id: &str,
) -> String {
    format!(
        "safi-org-key-wrap-v1:{}:{}:{}:{}:{}",
        organization_id.trim(),
        workspace_id.trim(),
        key_epoch,
        recipient_user_id.trim(),
        recipient_device_id.trim()
    )
}

fn derive_wrap_aead_key(
    shared_secret: &[u8; 32],
    sender_ephemeral_pub: &[u8; 32],
    recipient_static_pub: &[u8; 32],
    info_str: &str,
) -> Result<[u8; 32], String> {
    if shared_secret.iter().all(|b| *b == 0) {
        return Err("SF-SYN-001".to_owned());
    }
    let mut salt_bytes = [0u8; 64];
    salt_bytes[..32].copy_from_slice(sender_ephemeral_pub);
    salt_bytes[32..].copy_from_slice(recipient_static_pub);
    let salt = Salt::new(HKDF_SHA256, &salt_bytes);
    let prk = salt.extract(shared_secret);
    let info_parts = [info_str.as_bytes()];
    let okm = prk
        .expand(&info_parts, HkdfLen32)
        .map_err(|_| "SF-SYN-001".to_owned())?;
    let mut derived = [0u8; 32];
    okm.fill(&mut derived)
        .map_err(|_| "SF-SYN-001".to_owned())?;
    Ok(derived)
}

/// Pure cryptographic helper: wrap a 32-byte Workspace Key for a target member device's
/// X25519 public key using ephemeral-to-static X25519 + HKDF-SHA256 + ChaCha20-Poly1305.
pub fn wrap_workspace_key_for_device(
    organization_id: &str,
    workspace_id: &str,
    key_epoch: u32,
    workspace_key: &[u8; 32],
    recipient: &RemoteMemberDeviceKey,
) -> Result<WrappedKeyWireEntry, String> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let recipient_pub_vec = b64
        .decode(recipient.x25519_public_key_b64.trim())
        .map_err(|_| "SF-SYN-001".to_owned())?;
    if recipient_pub_vec.len() != 32 || recipient_pub_vec.iter().all(|b| *b == 0) {
        return Err("SF-SYN-001".to_owned());
    }
    let mut recipient_pub = [0u8; 32];
    recipient_pub.copy_from_slice(&recipient_pub_vec);

    let mut ephemeral_sk = [0u8; 32];
    fill_csprng(&mut ephemeral_sk)?;
    let ephemeral_pub = MontgomeryPoint::mul_base_clamped(ephemeral_sk).to_bytes();
    let mut shared_secret = MontgomeryPoint(recipient_pub)
        .mul_clamped(ephemeral_sk)
        .to_bytes();
    ephemeral_sk.zeroize();

    let info_str = build_wrap_info(
        organization_id,
        workspace_id,
        key_epoch,
        &recipient.user_id,
        &recipient.device_id,
    );
    let mut aead_key_bytes =
        derive_wrap_aead_key(&shared_secret, &ephemeral_pub, &recipient_pub, &info_str)?;
    shared_secret.zeroize();

    let mut nonce_bytes = [0u8; NONCE_LEN];
    fill_csprng(&mut nonce_bytes)?;

    let unbound = UnboundKey::new(&CHACHA20_POLY1305, &aead_key_bytes)
        .map_err(|_| "SF-SYN-001".to_owned())?;
    aead_key_bytes.zeroize();
    let sealing_key = LessSafeKey::new(unbound);

    let mut buf = workspace_key.to_vec();
    sealing_key
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce_bytes),
            Aad::from(info_str.as_bytes()),
            &mut buf,
        )
        .map_err(|_| "SF-SYN-001".to_owned())?;

    Ok(WrappedKeyWireEntry {
        recipient_user_id: recipient.user_id.clone(),
        recipient_device_id: recipient.device_id.clone(),
        sender_ephemeral_pub_b64: b64.encode(ephemeral_pub),
        wrapped_key_ciphertext_b64: b64.encode(&buf),
        wrapped_key_nonce_b64: b64.encode(nonce_bytes),
    })
}

/// Pure cryptographic helper: unwrap a 32-byte Workspace Key addressed to this device.
fn unwrap_workspace_key_with_device_secret(
    device_secret: &[u8; 32],
    device_public: &[u8; 32],
    envelope: &RemoteWrappedKeyEnvelope,
) -> Result<[u8; 32], String> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let eph_vec = b64
        .decode(envelope.sender_ephemeral_pub_b64.trim())
        .map_err(|_| "SF-SYN-001".to_owned())?;
    let ct_vec = b64
        .decode(envelope.wrapped_key_ciphertext_b64.trim())
        .map_err(|_| "SF-SYN-001".to_owned())?;
    let nonce_vec = b64
        .decode(envelope.wrapped_key_nonce_b64.trim())
        .map_err(|_| "SF-SYN-001".to_owned())?;

    if eph_vec.len() != 32
        || eph_vec.iter().all(|b| *b == 0)
        || ct_vec.len() != WRAPPED_KEY_CIPHERTEXT_LEN
        || nonce_vec.len() != NONCE_LEN
    {
        return Err("SF-SYN-001".to_owned());
    }

    let mut eph_pub = [0u8; 32];
    eph_pub.copy_from_slice(&eph_vec);
    let mut nonce_bytes = [0u8; NONCE_LEN];
    nonce_bytes.copy_from_slice(&nonce_vec);

    let mut shared_secret = MontgomeryPoint(eph_pub)
        .mul_clamped(*device_secret)
        .to_bytes();
    let info_str = build_wrap_info(
        &envelope.organization_id,
        &envelope.workspace_id,
        envelope.key_epoch,
        &envelope.recipient_user_id,
        &envelope.recipient_device_id,
    );
    let mut aead_key_bytes =
        derive_wrap_aead_key(&shared_secret, &eph_pub, device_public, &info_str)?;
    shared_secret.zeroize();

    let unbound = UnboundKey::new(&CHACHA20_POLY1305, &aead_key_bytes)
        .map_err(|_| "SF-SYN-001".to_owned())?;
    aead_key_bytes.zeroize();
    let opening_key = LessSafeKey::new(unbound);

    let mut buf = ct_vec;
    let opened = opening_key
        .open_in_place(
            Nonce::assume_unique_for_key(nonce_bytes),
            Aad::from(info_str.as_bytes()),
            &mut buf,
        )
        .map_err(|_| "SF-SYN-001".to_owned())?;
    if opened.len() != WORKSPACE_KEY_LEN {
        return Err("SF-SYN-001".to_owned());
    }
    let mut ws_key = [0u8; WORKSPACE_KEY_LEN];
    ws_key.copy_from_slice(opened);
    buf.zeroize();
    Ok(ws_key)
}

#[allow(clippy::too_many_arguments)]
fn seal_resource_payload(
    organization_id: &str,
    workspace_id: &str,
    key_epoch: u32,
    workspace_key: &[u8; 32],
    resource_kind: &str,
    resource_id: &str,
    expected_version: u64,
    deleted: bool,
    title: &str,
    payload_json: &str,
    origin_device_id: &str,
    idempotency_key: &str,
) -> Result<OutboundEncryptedEnvelope, String> {
    let plaintext_struct = if deleted {
        PersistedPlaintextEnvelopePayload {
            title: String::new(),
            payload_json: "{\"deleted\":true}".to_owned(),
        }
    } else {
        PersistedPlaintextEnvelopePayload {
            title: title.to_owned(),
            payload_json: payload_json.to_owned(),
        }
    };
    let mut plaintext_bytes =
        serde_json::to_vec(&plaintext_struct).map_err(|_| "SF-SYN-001".to_owned())?;

    let mut nonce_bytes = [0u8; NONCE_LEN];
    fill_csprng(&mut nonce_bytes)?;

    let aad_str = build_sync_aad(workspace_id, resource_kind, resource_id, key_epoch, deleted);
    let unbound =
        UnboundKey::new(&CHACHA20_POLY1305, workspace_key).map_err(|_| "SF-SYN-001".to_owned())?;
    let sealing_key = LessSafeKey::new(unbound);

    let mut buf = plaintext_bytes.clone();
    plaintext_bytes.zeroize();
    sealing_key
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce_bytes),
            Aad::from(aad_str.as_bytes()),
            &mut buf,
        )
        .map_err(|_| "SF-SYN-001".to_owned())?;

    let mut hasher = blake3::Hasher::new();
    hasher.update(&nonce_bytes);
    hasher.update(&buf);
    let content_hash = hasher.finalize().to_hex().to_string();

    let b64 = base64::engine::general_purpose::STANDARD;
    Ok(OutboundEncryptedEnvelope {
        workspace_id: workspace_id.to_owned(),
        resource_kind: resource_kind.to_owned(),
        resource_id: resource_id.to_owned(),
        expected_version,
        ciphertext_b64: b64.encode(&buf),
        nonce_b64: b64.encode(nonce_bytes),
        key_id: format!("org-{organization_id}-epoch-{key_epoch}"),
        key_epoch,
        content_hash,
        deleted,
        origin_device_id: origin_device_id.to_owned(),
        idempotency_key: idempotency_key.to_owned(),
    })
}

fn open_resource_payload(
    workspace_key: &[u8; 32],
    inbound: &InboundEncryptedEnvelope,
) -> Result<PersistedPlaintextEnvelopePayload, String> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let ciphertext = b64
        .decode(inbound.ciphertext_b64.trim())
        .map_err(|_| "SF-SYN-001".to_owned())?;
    let nonce_vec = b64
        .decode(inbound.nonce_b64.trim())
        .map_err(|_| "SF-SYN-001".to_owned())?;
    if nonce_vec.len() != NONCE_LEN || ciphertext.len() < AEAD_TAG_LEN + 1 {
        return Err("SF-SYN-001".to_owned());
    }

    let mut hasher = blake3::Hasher::new();
    hasher.update(&nonce_vec);
    hasher.update(&ciphertext);
    if hasher.finalize().to_hex().as_str() != inbound.content_hash.trim() {
        return Err("SF-SYN-001".to_owned());
    }

    let mut nonce_bytes = [0u8; NONCE_LEN];
    nonce_bytes.copy_from_slice(&nonce_vec);
    let aad_str = build_sync_aad(
        &inbound.workspace_id,
        &inbound.resource_kind,
        &inbound.resource_id,
        inbound.key_epoch,
        inbound.deleted,
    );

    let unbound =
        UnboundKey::new(&CHACHA20_POLY1305, workspace_key).map_err(|_| "SF-SYN-001".to_owned())?;
    let opening_key = LessSafeKey::new(unbound);

    let mut buf = ciphertext;
    let opened = opening_key
        .open_in_place(
            Nonce::assume_unique_for_key(nonce_bytes),
            Aad::from(aad_str.as_bytes()),
            &mut buf,
        )
        .map_err(|_| "SF-SYN-001".to_owned())?;

    let decoded: PersistedPlaintextEnvelopePayload =
        serde_json::from_slice(opened).map_err(|_| "SF-SYN-001".to_owned())?;
    buf.zeroize();
    Ok(decoded)
}

/// Encrypted local Desktop store managing device X25519 keys, unwrapped workspace keys,
/// local synced resources, offline push queue, and conflict resolution.
pub struct OrgSyncStore {
    storage_dir: PathBuf,
    vault_key: VaultKey,
    inner: Mutex<PersistedSyncVault>,
}

impl fmt::Debug for OrgSyncStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OrgSyncStore")
            .field("storage_dir", &self.storage_dir)
            .finish_non_exhaustive()
    }
}

impl OrgSyncStore {
    /// Load or initialize the encrypted `org_sync_vault.safi.enc` inside `storage_dir`.
    pub fn open_or_init(
        storage_dir: impl Into<PathBuf>,
        vault_key: VaultKey,
        default_device_id: &str,
        default_device_label: &str,
    ) -> Result<Self, String> {
        let dir = storage_dir.into();
        fs::create_dir_all(&dir).map_err(|_| "SF-SYS-023".to_owned())?;
        let vault_path = dir.join(SYNC_VAULT_FILENAME);

        let persisted = if vault_path.exists() {
            let sealed = fs::read(&vault_path).map_err(|_| "SF-SYS-024".to_owned())?;
            let mut plain =
                decrypt_vault_payload(&vault_key, &sealed).map_err(|_| "SF-SYS-024".to_owned())?;
            let loaded: PersistedSyncVault =
                serde_json::from_slice(&plain).map_err(|_| "SF-SYS-024".to_owned())?;
            plain.zeroize();
            loaded
        } else {
            let mut secret = [0u8; 32];
            fill_csprng(&mut secret)?;
            let pub_bytes = MontgomeryPoint::mul_base_clamped(secret).to_bytes();
            let pub_b64 = base64::engine::general_purpose::STANDARD.encode(pub_bytes);
            let fp = blake3::hash(&pub_bytes).to_hex()[..32].to_string();
            let dev_id = if default_device_id.trim().is_empty() {
                format!("dev-{}", &fp[..16])
            } else {
                default_device_id.trim().to_owned()
            };
            let dev_label = if default_device_label.trim().is_empty() {
                "SafiData Desktop".to_owned()
            } else {
                default_device_label.trim().to_owned()
            };
            PersistedSyncVault {
                device_id: dev_id,
                device_label: dev_label,
                device_x25519_secret: SecretKeyBytes32(secret),
                device_x25519_public_b64: pub_b64,
                device_key_fingerprint: fp,
                device_created_at: unix_secs_now(),
                active_organization_id: None,
                active_workspace_id: None,
                workspaces: BTreeMap::new(),
                offline_queue: VecDeque::new(),
            }
        };

        let store = Self {
            storage_dir: dir,
            vault_key,
            inner: Mutex::new(persisted),
        };
        store.flush_encrypted()?;
        Ok(store)
    }

    #[must_use]
    pub fn vault_file_path(&self) -> PathBuf {
        self.storage_dir.join(SYNC_VAULT_FILENAME)
    }

    fn flush_locked(&self, state: &PersistedSyncVault) -> Result<(), String> {
        let mut plain = serde_json::to_vec(state).map_err(|_| "SF-SYS-023".to_owned())?;
        let sealed =
            encrypt_vault_payload(&self.vault_key, &plain).map_err(|_| "SF-SYS-023".to_owned());
        plain.zeroize();
        let sealed = sealed?;
        write_atomic(&self.vault_file_path(), &sealed)
    }

    fn flush_encrypted(&self) -> Result<(), String> {
        let guard = self.inner.lock().map_err(|_| "SF-SYS-023".to_owned())?;
        self.flush_locked(&guard)
    }

    /// Return this installation's public X25519 sync identity (private key never exposed).
    pub fn public_device_identity(&self) -> Result<PublicDeviceSyncIdentity, String> {
        let guard = self.inner.lock().map_err(|_| "SF-SYS-023".to_owned())?;
        Ok(PublicDeviceSyncIdentity {
            device_id: guard.device_id.clone(),
            device_label: guard.device_label.clone(),
            x25519_public_key_b64: guard.device_x25519_public_b64.clone(),
            key_fingerprint: guard.device_key_fingerprint.clone(),
            created_at: guard.device_created_at,
        })
    }

    /// Rotate this installation's local X25519 device keypair (used after suspected device compromise).
    pub fn rotate_local_device_keypair(
        &self,
        new_device_id: Option<&str>,
    ) -> Result<PublicDeviceSyncIdentity, String> {
        let mut guard = self.inner.lock().map_err(|_| "SF-SYS-023".to_owned())?;
        let mut secret = [0u8; 32];
        fill_csprng(&mut secret)?;
        let pub_bytes = MontgomeryPoint::mul_base_clamped(secret).to_bytes();
        let pub_b64 = base64::engine::general_purpose::STANDARD.encode(pub_bytes);
        let fp = blake3::hash(&pub_bytes).to_hex()[..32].to_string();
        if let Some(id) = new_device_id.map(str::trim).filter(|s| !s.is_empty()) {
            guard.device_id = id.to_owned();
        }
        guard.device_x25519_secret = SecretKeyBytes32(secret);
        guard.device_x25519_public_b64 = pub_b64.clone();
        guard.device_key_fingerprint = fp.clone();
        guard.device_created_at = unix_secs_now();
        self.flush_locked(&guard)?;
        Ok(PublicDeviceSyncIdentity {
            device_id: guard.device_id.clone(),
            device_label: guard.device_label.clone(),
            x25519_public_key_b64: pub_b64,
            key_fingerprint: fp,
            created_at: guard.device_created_at,
        })
    }

    /// Select the active Shared Team Workspace (or pass `None` to return to Personal Workspace).
    pub fn select_active_workspace(
        &self,
        organization_id: Option<&str>,
        workspace_id: Option<&str>,
        active_key_epoch: u32,
    ) -> Result<DesktopWorkspaceSyncStatus, String> {
        let mut guard = self.inner.lock().map_err(|_| "SF-SYS-023".to_owned())?;
        match (
            organization_id.map(str::trim).filter(|s| !s.is_empty()),
            workspace_id.map(str::trim).filter(|s| !s.is_empty()),
        ) {
            (Some(org_id), Some(ws_id)) => {
                guard.active_organization_id = Some(org_id.to_owned());
                guard.active_workspace_id = Some(ws_id.to_owned());
                let entry = guard.workspaces.entry(ws_id.to_owned()).or_insert_with(|| {
                    WorkspaceVaultState {
                        organization_id: org_id.to_owned(),
                        workspace_id: ws_id.to_owned(),
                        active_key_epoch: active_key_epoch.max(1),
                        epoch_keys: BTreeMap::new(),
                        last_workspace_seq: 0,
                        resources: BTreeMap::new(),
                        conflicts: BTreeMap::new(),
                        last_synced_at: None,
                        last_error_code: None,
                    }
                });
                if active_key_epoch > entry.active_key_epoch {
                    entry.active_key_epoch = active_key_epoch;
                }
            }
            _ => {
                guard.active_organization_id = None;
                guard.active_workspace_id = None;
            }
        }
        self.flush_locked(&guard)?;
        Ok(Self::build_status_from_guard(&guard))
    }

    /// Bootstrap or wrap the current epoch's Workspace Key for all active member devices.
    /// If the epoch key does not exist locally yet (first Owner device initializing the org),
    /// generates a fresh 256-bit CSPRNG Workspace Key and stores it encrypted locally.
    pub fn ensure_and_wrap_epoch_key_for_devices(
        &self,
        organization_id: &str,
        workspace_id: &str,
        key_epoch: u32,
        devices: &[RemoteMemberDeviceKey],
    ) -> Result<Vec<WrappedKeyWireEntry>, String> {
        if key_epoch == 0 || devices.is_empty() {
            return Err("SF-AUTH-006".to_owned());
        }
        let mut guard = self.inner.lock().map_err(|_| "SF-SYS-023".to_owned())?;
        let ws_state = guard
            .workspaces
            .entry(workspace_id.trim().to_owned())
            .or_insert_with(|| WorkspaceVaultState {
                organization_id: organization_id.trim().to_owned(),
                workspace_id: workspace_id.trim().to_owned(),
                active_key_epoch: key_epoch,
                epoch_keys: BTreeMap::new(),
                last_workspace_seq: 0,
                resources: BTreeMap::new(),
                conflicts: BTreeMap::new(),
                last_synced_at: None,
                last_error_code: None,
            });
        if key_epoch > ws_state.active_key_epoch {
            ws_state.active_key_epoch = key_epoch;
        }
        if !ws_state.epoch_keys.contains_key(&key_epoch) {
            // Only generate a brand-new epoch 1 bootstrap key if NO epoch keys exist yet;
            // for epoch > 1, use `prepare_rotated_workspace_key`.
            if key_epoch != 1 && !ws_state.epoch_keys.is_empty() {
                return Err("SF-SYN-005".to_owned());
            }
            let mut fresh = [0u8; WORKSPACE_KEY_LEN];
            fill_csprng(&mut fresh)?;
            ws_state
                .epoch_keys
                .insert(key_epoch, SecretKeyBytes32(fresh));
        }
        let ws_key = ws_state
            .epoch_keys
            .get(&key_epoch)
            .map(|k| k.0)
            .ok_or_else(|| "SF-SYN-005".to_owned())?;

        let mut wrapped = Vec::with_capacity(devices.len());
        for dev in devices {
            wrapped.push(wrap_workspace_key_for_device(
                organization_id,
                workspace_id,
                key_epoch,
                &ws_key,
                dev,
            )?);
        }
        self.flush_locked(&guard)?;
        Ok(wrapped)
    }

    /// Generate a brand-new rotated 256-bit Workspace Key for `current_epoch + 1` and wrap it
    /// ONLY for the remaining authorized active member devices.
    pub fn prepare_rotated_workspace_key(
        &self,
        organization_id: &str,
        workspace_id: &str,
        current_epoch: u32,
        remaining_devices: &[RemoteMemberDeviceKey],
    ) -> Result<(u32, Vec<WrappedKeyWireEntry>), String> {
        if current_epoch == 0 || remaining_devices.is_empty() {
            return Err("SF-AUTH-006".to_owned());
        }
        let next_epoch = current_epoch + 1;
        let mut fresh_key = [0u8; WORKSPACE_KEY_LEN];
        fill_csprng(&mut fresh_key)?;

        let mut wrapped = Vec::with_capacity(remaining_devices.len());
        for dev in remaining_devices {
            wrapped.push(wrap_workspace_key_for_device(
                organization_id,
                workspace_id,
                next_epoch,
                &fresh_key,
                dev,
            )?);
        }

        let mut guard = self.inner.lock().map_err(|_| "SF-SYS-023".to_owned())?;
        let ws_state = guard
            .workspaces
            .entry(workspace_id.trim().to_owned())
            .or_insert_with(|| WorkspaceVaultState {
                organization_id: organization_id.trim().to_owned(),
                workspace_id: workspace_id.trim().to_owned(),
                active_key_epoch: next_epoch,
                epoch_keys: BTreeMap::new(),
                last_workspace_seq: 0,
                resources: BTreeMap::new(),
                conflicts: BTreeMap::new(),
                last_synced_at: None,
                last_error_code: None,
            });
        ws_state.active_key_epoch = next_epoch;
        ws_state
            .epoch_keys
            .insert(next_epoch, SecretKeyBytes32(fresh_key));
        // Clear any cached pre-rotation envelopes so queued offline edits re-encrypt at `next_epoch`!
        for item in &mut guard.offline_queue {
            if item.workspace_id == workspace_id.trim() {
                item.cached_envelope = None;
            }
        }
        self.flush_locked(&guard)?;
        Ok((next_epoch, wrapped))
    }

    /// Import and unwrap wrapped Workspace Keys fetched from `GET /v1/organizations/{id}/keys/my-wrapped`.
    pub fn ingest_wrapped_workspace_keys(
        &self,
        envelopes: &[RemoteWrappedKeyEnvelope],
    ) -> Result<usize, String> {
        let mut guard = self.inner.lock().map_err(|_| "SF-SYS-023".to_owned())?;
        let b64 = base64::engine::general_purpose::STANDARD;
        let pub_vec = b64
            .decode(guard.device_x25519_public_b64.trim())
            .map_err(|_| "SF-SYN-001".to_owned())?;
        if pub_vec.len() != 32 {
            return Err("SF-SYN-001".to_owned());
        }
        let mut my_pub = [0u8; 32];
        my_pub.copy_from_slice(&pub_vec);
        let my_secret = guard.device_x25519_secret.0;
        let my_device_id = guard.device_id.clone();

        let mut unlocked_count = 0usize;
        for env in envelopes {
            if env.recipient_device_id.trim() != my_device_id {
                continue;
            }
            let unwrapped = unwrap_workspace_key_with_device_secret(&my_secret, &my_pub, env)?;
            let ws_state = guard
                .workspaces
                .entry(env.workspace_id.trim().to_owned())
                .or_insert_with(|| WorkspaceVaultState {
                    organization_id: env.organization_id.trim().to_owned(),
                    workspace_id: env.workspace_id.trim().to_owned(),
                    active_key_epoch: env.key_epoch,
                    epoch_keys: BTreeMap::new(),
                    last_workspace_seq: 0,
                    resources: BTreeMap::new(),
                    conflicts: BTreeMap::new(),
                    last_synced_at: None,
                    last_error_code: None,
                });
            if env.key_epoch > ws_state.active_key_epoch {
                ws_state.active_key_epoch = env.key_epoch;
            }
            ws_state
                .epoch_keys
                .insert(env.key_epoch, SecretKeyBytes32(unwrapped));
            ws_state.last_error_code = None;
            unlocked_count += 1;
        }
        self.flush_locked(&guard)?;
        Ok(unlocked_count)
    }

    /// Save or update a shared resource locally (works 100% offline!) and enqueue an encrypted
    /// sync mutation with a durable idempotency key.
    pub fn stage_shared_resource_upsert(
        &self,
        organization_id: &str,
        workspace_id: &str,
        resource_kind: &str,
        resource_id: &str,
        title: &str,
        payload_json: &str,
    ) -> Result<SharedWorkspaceResource, String> {
        self.stage_mutation_internal(
            organization_id,
            workspace_id,
            resource_kind,
            resource_id,
            title,
            payload_json,
            false,
        )
    }

    /// Stage a deletion tombstone (`deleted = true`) for a shared resource.
    pub fn stage_shared_resource_delete(
        &self,
        organization_id: &str,
        workspace_id: &str,
        resource_kind: &str,
        resource_id: &str,
    ) -> Result<SharedWorkspaceResource, String> {
        self.stage_mutation_internal(
            organization_id,
            workspace_id,
            resource_kind,
            resource_id,
            "",
            "{\"deleted\":true}",
            true,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn stage_mutation_internal(
        &self,
        organization_id: &str,
        workspace_id: &str,
        resource_kind: &str,
        resource_id: &str,
        title: &str,
        payload_json: &str,
        deleted: bool,
    ) -> Result<SharedWorkspaceResource, String> {
        let ws_id = workspace_id.trim();
        let org_id = organization_id.trim();
        let kind = resource_kind.trim();
        let res_id = resource_id.trim();
        if ws_id.is_empty() || org_id.is_empty() || kind.is_empty() || res_id.is_empty() {
            return Err("SF-AUTH-006".to_owned());
        }

        let mut idem_bytes = [0u8; 16];
        fill_csprng(&mut idem_bytes)?;
        let idempotency_key = format!("sync-mut-{}", hex::encode(idem_bytes));
        let now = unix_secs_now();

        let mut guard = self.inner.lock().map_err(|_| "SF-SYS-023".to_owned())?;
        let device_id = guard.device_id.clone();
        let ws_state =
            guard
                .workspaces
                .entry(ws_id.to_owned())
                .or_insert_with(|| WorkspaceVaultState {
                    organization_id: org_id.to_owned(),
                    workspace_id: ws_id.to_owned(),
                    active_key_epoch: 1,
                    epoch_keys: BTreeMap::new(),
                    last_workspace_seq: 0,
                    resources: BTreeMap::new(),
                    conflicts: BTreeMap::new(),
                    last_synced_at: None,
                    last_error_code: None,
                });
        let map_key = resource_map_key(kind, res_id);
        let base_version = ws_state
            .resources
            .get(&map_key)
            .map_or(0, |existing| existing.version);
        let key_epoch = ws_state.active_key_epoch;

        let local_resource = SharedWorkspaceResource {
            workspace_id: ws_id.to_owned(),
            resource_kind: kind.to_owned(),
            resource_id: res_id.to_owned(),
            title: title.trim().to_owned(),
            payload_json: payload_json.to_owned(),
            version: base_version,
            key_epoch,
            deleted,
            dirty: true,
            last_author_user_id: None,
            origin_device_id: device_id,
            updated_at: now,
        };
        ws_state.resources.insert(map_key, local_resource.clone());

        // Coalesce any earlier unsent queued mutation for the exact same `(workspace_id, kind, id)`
        // so offline edits push cleanly from `base_version`!
        guard.offline_queue.retain(|q| {
            !(q.workspace_id == ws_id && q.resource_kind == kind && q.resource_id == res_id)
        });
        guard.offline_queue.push_back(QueuedOfflineMutation {
            idempotency_key,
            organization_id: org_id.to_owned(),
            workspace_id: ws_id.to_owned(),
            resource_kind: kind.to_owned(),
            resource_id: res_id.to_owned(),
            expected_version: base_version,
            deleted,
            title: title.trim().to_owned(),
            payload_json: payload_json.to_owned(),
            cached_envelope: None,
            created_at: now,
        });

        self.flush_locked(&guard)?;
        Ok(local_resource)
    }

    /// Peek and seal the next queued offline mutation for network transmission.
    /// Reuses the cached sealed envelope on network retry so `(nonce, ciphertext, idempotency_key)`
    /// remain byte-identical across dropped-connection retries!
    pub fn peek_next_outbound_envelope(
        &self,
        workspace_id: &str,
    ) -> Result<Option<OutboundEncryptedEnvelope>, String> {
        let ws_id = workspace_id.trim();
        let mut guard = self.inner.lock().map_err(|_| "SF-SYS-023".to_owned())?;
        let device_id = guard.device_id.clone();

        let (active_epoch, ws_key) = {
            let Some(ws_state) = guard.workspaces.get(ws_id) else {
                return Ok(None);
            };
            // Pause outbound pushes for a resource while it has an unresolved conflict.
            let has_queue_item = guard.offline_queue.iter().any(|q| {
                q.workspace_id == ws_id
                    && !ws_state
                        .conflicts
                        .contains_key(&resource_map_key(&q.resource_kind, &q.resource_id))
            });
            if !has_queue_item {
                return Ok(None);
            }
            let epoch = ws_state.active_key_epoch;
            let Some(secret) = ws_state.epoch_keys.get(&epoch) else {
                return Err("SF-SYN-005".to_owned());
            };
            (epoch, secret.0)
        };

        let mut result_env = None;
        for q in &mut guard.offline_queue {
            if q.workspace_id != ws_id {
                continue;
            }
            if let Some(ref cached) = q.cached_envelope {
                if cached.key_epoch == active_epoch && cached.expected_version == q.expected_version
                {
                    result_env = Some(cached.clone());
                    break;
                }
            }
            let sealed = seal_resource_payload(
                &q.organization_id,
                &q.workspace_id,
                active_epoch,
                &ws_key,
                &q.resource_kind,
                &q.resource_id,
                q.expected_version,
                q.deleted,
                &q.title,
                &q.payload_json,
                &device_id,
                &q.idempotency_key,
            )?;
            q.cached_envelope = Some(sealed.clone());
            result_env = Some(sealed);
            break;
        }

        if result_env.is_some() {
            self.flush_locked(&guard)?;
        }
        Ok(result_env)
    }

    /// Acknowledge a successful server `POST /v1/sync/envelopes` response, advancing the local
    /// resource version and dequeuing the mutation.
    pub fn mark_outbound_pushed(
        &self,
        idempotency_key: &str,
        committed_version: u64,
        committed_workspace_seq: u64,
        author_user_id: Option<&str>,
    ) -> Result<(), String> {
        let mut guard = self.inner.lock().map_err(|_| "SF-SYS-023".to_owned())?;
        let Some(pos) = guard
            .offline_queue
            .iter()
            .position(|q| q.idempotency_key == idempotency_key)
        else {
            return Ok(());
        };
        let Some(removed) = guard.offline_queue.remove(pos) else {
            return Ok(());
        };
        if let Some(ws_state) = guard.workspaces.get_mut(&removed.workspace_id) {
            let map_key = resource_map_key(&removed.resource_kind, &removed.resource_id);
            if let Some(res) = ws_state.resources.get_mut(&map_key) {
                res.version = committed_version;
                res.dirty = false;
                if let Some(uid) = author_user_id {
                    res.last_author_user_id = Some(uid.to_owned());
                }
            }
            if committed_workspace_seq > ws_state.last_workspace_seq
                && committed_workspace_seq == ws_state.last_workspace_seq + 1
            {
                ws_state.last_workspace_seq = committed_workspace_seq;
            }
            ws_state.last_synced_at = Some(unix_secs_now());
            ws_state.last_error_code = None;
        }
        self.flush_locked(&guard)
    }

    /// Record a network/server error without losing queued offline work.
    pub fn record_sync_error(&self, workspace_id: &str, error_code: &str) -> Result<(), String> {
        let mut guard = self.inner.lock().map_err(|_| "SF-SYS-023".to_owned())?;
        if let Some(ws_state) = guard.workspaces.get_mut(workspace_id.trim()) {
            ws_state.last_error_code = Some(error_code.trim().to_owned());
        }
        self.flush_locked(&guard)
    }

    /// Record a `409 SF-SYN-002` conflict by decrypting the conflicting remote envelope locally
    /// and surfacing both versions for explicit user resolution (`KeepLocal` vs `AcceptRemote`).
    pub fn record_push_conflict(
        &self,
        idempotency_key: &str,
        remote_envelope: &InboundEncryptedEnvelope,
    ) -> Result<SyncConflictItem, String> {
        let mut guard = self.inner.lock().map_err(|_| "SF-SYS-023".to_owned())?;
        let Some(ws_state) = guard.workspaces.get(&remote_envelope.workspace_id) else {
            return Err("SF-SYN-001".to_owned());
        };
        let Some(epoch_secret) = ws_state.epoch_keys.get(&remote_envelope.key_epoch) else {
            return Err("SF-SYN-005".to_owned());
        };
        let opened_remote = open_resource_payload(&epoch_secret.0, remote_envelope)?;

        let queued = guard
            .offline_queue
            .iter()
            .find(|q| q.idempotency_key == idempotency_key)
            .cloned();

        let (local_title, local_payload, local_base_ver) = match queued {
            Some(q) => (q.title, q.payload_json, q.expected_version),
            None => {
                let mk =
                    resource_map_key(&remote_envelope.resource_kind, &remote_envelope.resource_id);
                let existing = ws_state.resources.get(&mk);
                (
                    existing.map_or_else(String::new, |r| r.title.clone()),
                    existing.map_or_else(|| "{}".to_owned(), |r| r.payload_json.clone()),
                    existing.map_or(0, |r| r.version),
                )
            }
        };

        let conflict = SyncConflictItem {
            workspace_id: remote_envelope.workspace_id.clone(),
            resource_kind: remote_envelope.resource_kind.clone(),
            resource_id: remote_envelope.resource_id.clone(),
            local_title,
            local_payload_json: local_payload,
            local_base_version: local_base_ver,
            remote_title: opened_remote.title,
            remote_payload_json: opened_remote.payload_json,
            remote_version: remote_envelope.version,
            remote_deleted: remote_envelope.deleted,
            remote_user_id: remote_envelope.user_id.clone(),
            detected_at: unix_secs_now(),
        };

        let map_key =
            resource_map_key(&remote_envelope.resource_kind, &remote_envelope.resource_id);
        let ws_state_mut = guard
            .workspaces
            .get_mut(&remote_envelope.workspace_id)
            .ok_or_else(|| "SF-SYN-001".to_owned())?;
        ws_state_mut.conflicts.insert(map_key, conflict.clone());
        ws_state_mut.last_error_code = Some("SF-SYN-002".to_owned());
        self.flush_locked(&guard)?;
        Ok(conflict)
    }

    /// Resolve a detected conflict using the user's explicit choice (`KeepLocal` or `AcceptRemote`).
    pub fn resolve_conflict(
        &self,
        workspace_id: &str,
        resource_kind: &str,
        resource_id: &str,
        choice: ConflictResolutionChoice,
    ) -> Result<SharedWorkspaceResource, String> {
        let ws_id = workspace_id.trim();
        let kind = resource_kind.trim();
        let res_id = resource_id.trim();
        let map_key = resource_map_key(kind, res_id);

        let mut guard = self.inner.lock().map_err(|_| "SF-SYS-023".to_owned())?;
        let device_id = guard.device_id.clone();
        let org_id = guard
            .workspaces
            .get(ws_id)
            .map(|w| w.organization_id.clone())
            .ok_or_else(|| "SF-AUTH-006".to_owned())?;
        let active_epoch = guard
            .workspaces
            .get(ws_id)
            .map_or(1, |w| w.active_key_epoch);

        let conflict = guard
            .workspaces
            .get_mut(ws_id)
            .and_then(|w| w.conflicts.remove(&map_key))
            .ok_or_else(|| "SF-AUTH-006".to_owned())?;

        let resolved_resource = match choice {
            ConflictResolutionChoice::KeepLocal => {
                let mut idem_bytes = [0u8; 16];
                fill_csprng(&mut idem_bytes)?;
                let new_idem = format!("sync-mut-{}", hex::encode(idem_bytes));

                guard.offline_queue.retain(|q| {
                    !(q.workspace_id == ws_id && q.resource_kind == kind && q.resource_id == res_id)
                });
                guard.offline_queue.push_back(QueuedOfflineMutation {
                    idempotency_key: new_idem,
                    organization_id: org_id,
                    workspace_id: ws_id.to_owned(),
                    resource_kind: kind.to_owned(),
                    resource_id: res_id.to_owned(),
                    expected_version: conflict.remote_version,
                    deleted: false,
                    title: conflict.local_title.clone(),
                    payload_json: conflict.local_payload_json.clone(),
                    cached_envelope: None,
                    created_at: unix_secs_now(),
                });
                SharedWorkspaceResource {
                    workspace_id: ws_id.to_owned(),
                    resource_kind: kind.to_owned(),
                    resource_id: res_id.to_owned(),
                    title: conflict.local_title,
                    payload_json: conflict.local_payload_json,
                    version: conflict.remote_version,
                    key_epoch: active_epoch,
                    deleted: false,
                    dirty: true,
                    last_author_user_id: None,
                    origin_device_id: device_id,
                    updated_at: unix_secs_now(),
                }
            }
            ConflictResolutionChoice::AcceptRemote => {
                guard.offline_queue.retain(|q| {
                    !(q.workspace_id == ws_id && q.resource_kind == kind && q.resource_id == res_id)
                });
                SharedWorkspaceResource {
                    workspace_id: ws_id.to_owned(),
                    resource_kind: kind.to_owned(),
                    resource_id: res_id.to_owned(),
                    title: conflict.remote_title,
                    payload_json: conflict.remote_payload_json,
                    version: conflict.remote_version,
                    key_epoch: active_epoch,
                    deleted: conflict.remote_deleted,
                    dirty: false,
                    last_author_user_id: Some(conflict.remote_user_id),
                    origin_device_id: device_id,
                    updated_at: unix_secs_now(),
                }
            }
        };

        if let Some(ws_state) = guard.workspaces.get_mut(ws_id) {
            ws_state
                .resources
                .insert(map_key, resolved_resource.clone());
            if ws_state.conflicts.is_empty() {
                ws_state.last_error_code = None;
            }
        }
        self.flush_locked(&guard)?;
        Ok(resolved_resource)
    }

    /// Decrypt and apply a batch of incremental envelopes pulled from `GET /v1/sync/changes`,
    /// advancing `last_workspace_seq` monotonically so interrupted pulls resume cleanly.
    pub fn apply_pulled_changes(
        &self,
        workspace_id: &str,
        envelopes: &[InboundEncryptedEnvelope],
    ) -> Result<usize, String> {
        let ws_id = workspace_id.trim();
        let mut guard = self.inner.lock().map_err(|_| "SF-SYS-023".to_owned())?;
        let mut applied = 0usize;

        for env in envelopes {
            if env.workspace_id.trim() != ws_id {
                return Err("SF-SYN-005".to_owned());
            }
            let opened = {
                let ws_state = guard
                    .workspaces
                    .get(ws_id)
                    .ok_or_else(|| "SF-SYN-005".to_owned())?;
                let epoch_secret = ws_state
                    .epoch_keys
                    .get(&env.key_epoch)
                    .ok_or_else(|| "SF-SYN-005".to_owned())?;
                open_resource_payload(&epoch_secret.0, env)?
            };

            let map_key = resource_map_key(&env.resource_kind, &env.resource_id);
            let has_local_dirty = guard.offline_queue.iter().any(|q| {
                q.workspace_id == ws_id
                    && q.resource_kind == env.resource_kind
                    && q.resource_id == env.resource_id
            });

            let ws_state = guard
                .workspaces
                .get_mut(ws_id)
                .ok_or_else(|| "SF-SYN-005".to_owned())?;
            if env.key_epoch > ws_state.active_key_epoch {
                ws_state.active_key_epoch = env.key_epoch;
            }

            if has_local_dirty {
                if let Some(local_res) = ws_state.resources.get(&map_key).cloned() {
                    if env.version > local_res.version {
                        ws_state.conflicts.insert(
                            map_key,
                            SyncConflictItem {
                                workspace_id: ws_id.to_owned(),
                                resource_kind: env.resource_kind.clone(),
                                resource_id: env.resource_id.clone(),
                                local_title: local_res.title,
                                local_payload_json: local_res.payload_json,
                                local_base_version: local_res.version,
                                remote_title: opened.title,
                                remote_payload_json: opened.payload_json,
                                remote_version: env.version,
                                remote_deleted: env.deleted,
                                remote_user_id: env.user_id.clone(),
                                detected_at: unix_secs_now(),
                            },
                        );
                        ws_state.last_error_code = Some("SF-SYN-002".to_owned());
                    }
                }
            } else {
                ws_state.resources.insert(
                    map_key,
                    SharedWorkspaceResource {
                        workspace_id: ws_id.to_owned(),
                        resource_kind: env.resource_kind.clone(),
                        resource_id: env.resource_id.clone(),
                        title: opened.title,
                        payload_json: opened.payload_json,
                        version: env.version,
                        key_epoch: env.key_epoch,
                        deleted: env.deleted,
                        dirty: false,
                        last_author_user_id: Some(env.user_id.clone()),
                        origin_device_id: env.origin_device_id.clone(),
                        updated_at: env.updated_at,
                    },
                );
            }

            if env.workspace_seq > ws_state.last_workspace_seq {
                ws_state.last_workspace_seq = env.workspace_seq;
            }
            ws_state.last_synced_at = Some(unix_secs_now());
            applied += 1;
        }

        if let Some(ws_state) = guard.workspaces.get_mut(ws_id) {
            if ws_state.conflicts.is_empty() {
                ws_state.last_error_code = None;
            }
            if envelopes.is_empty() {
                ws_state.last_synced_at = Some(unix_secs_now());
            }
        }
        self.flush_locked(&guard)?;
        Ok(applied)
    }

    /// Return the incremental pull cursor (`since_seq`) for a workspace.
    pub fn workspace_cursor(&self, workspace_id: &str) -> Result<u64, String> {
        let guard = self.inner.lock().map_err(|_| "SF-SYS-023".to_owned())?;
        Ok(guard
            .workspaces
            .get(workspace_id.trim())
            .map_or(0, |w| w.last_workspace_seq))
    }

    /// List non-deleted shared resources decrypted inside the local vault for `workspace_id`.
    pub fn list_shared_resources(
        &self,
        workspace_id: &str,
        include_deleted: bool,
    ) -> Result<Vec<SharedWorkspaceResource>, String> {
        let guard = self.inner.lock().map_err(|_| "SF-SYS-023".to_owned())?;
        let Some(ws_state) = guard.workspaces.get(workspace_id.trim()) else {
            return Ok(Vec::new());
        };
        Ok(ws_state
            .resources
            .values()
            .filter(|r| include_deleted || !r.deleted)
            .cloned()
            .collect())
    }

    /// List unresolved sync conflicts for `workspace_id`.
    pub fn list_conflicts(&self, workspace_id: &str) -> Result<Vec<SyncConflictItem>, String> {
        let guard = self.inner.lock().map_err(|_| "SF-SYS-023".to_owned())?;
        let Some(ws_state) = guard.workspaces.get(workspace_id.trim()) else {
            return Ok(Vec::new());
        };
        Ok(ws_state.conflicts.values().cloned().collect())
    }

    /// Inspect live sync status for the currently selected workspace (or explicit `workspace_id`).
    pub fn status_for_workspace(
        &self,
        workspace_id: Option<&str>,
    ) -> Result<DesktopWorkspaceSyncStatus, String> {
        let guard = self.inner.lock().map_err(|_| "SF-SYS-023".to_owned())?;
        if let Some(ws_id) = workspace_id.map(str::trim).filter(|s| !s.is_empty()) {
            return Ok(Self::build_status_for_ws(&guard, Some(ws_id)));
        }
        Ok(Self::build_status_from_guard(&guard))
    }

    fn build_status_from_guard(guard: &PersistedSyncVault) -> DesktopWorkspaceSyncStatus {
        let active_ws = guard.active_workspace_id.as_deref();
        Self::build_status_for_ws(guard, active_ws)
    }

    fn build_status_for_ws(
        guard: &PersistedSyncVault,
        ws_id_opt: Option<&str>,
    ) -> DesktopWorkspaceSyncStatus {
        let device_identity = PublicDeviceSyncIdentity {
            device_id: guard.device_id.clone(),
            device_label: guard.device_label.clone(),
            x25519_public_key_b64: guard.device_x25519_public_b64.clone(),
            key_fingerprint: guard.device_key_fingerprint.clone(),
            created_at: guard.device_created_at,
        };

        let Some(ws_id) = ws_id_opt else {
            return DesktopWorkspaceSyncStatus {
                active_organization_id: None,
                active_workspace_id: None,
                device_identity,
                active_key_epoch: 1,
                unlocked_epochs: Vec::new(),
                last_workspace_seq: 0,
                pending_offline_count: 0,
                conflict_count: 0,
                resource_count: 0,
                health: SyncHealthState::Idle,
                last_synced_at: None,
                last_error_code: None,
            };
        };

        let pending_offline_count = guard
            .offline_queue
            .iter()
            .filter(|q| q.workspace_id == ws_id)
            .count();

        let Some(ws_state) = guard.workspaces.get(ws_id) else {
            return DesktopWorkspaceSyncStatus {
                active_organization_id: guard.active_organization_id.clone(),
                active_workspace_id: Some(ws_id.to_owned()),
                device_identity,
                active_key_epoch: 1,
                unlocked_epochs: Vec::new(),
                last_workspace_seq: 0,
                pending_offline_count,
                conflict_count: 0,
                resource_count: 0,
                health: SyncHealthState::KeyRequired,
                last_synced_at: None,
                last_error_code: None,
            };
        };

        let unlocked_epochs: Vec<u32> = ws_state.epoch_keys.keys().copied().collect();
        let conflict_count = ws_state.conflicts.len();
        let resource_count = ws_state.resources.values().filter(|r| !r.deleted).count();
        let has_active_key = ws_state.epoch_keys.contains_key(&ws_state.active_key_epoch);

        let health = if conflict_count > 0 {
            SyncHealthState::ConflictDetected
        } else if !has_active_key {
            SyncHealthState::KeyRequired
        } else if ws_state.last_error_code.is_some() {
            SyncHealthState::Error
        } else if pending_offline_count > 0 {
            SyncHealthState::OfflineQueued
        } else {
            SyncHealthState::Synced
        };

        DesktopWorkspaceSyncStatus {
            active_organization_id: Some(ws_state.organization_id.clone()),
            active_workspace_id: Some(ws_state.workspace_id.clone()),
            device_identity,
            active_key_epoch: ws_state.active_key_epoch,
            unlocked_epochs,
            last_workspace_seq: ws_state.last_workspace_seq,
            pending_offline_count,
            conflict_count,
            resource_count,
            health,
            last_synced_at: ws_state.last_synced_at,
            last_error_code: ws_state.last_error_code.clone(),
        }
    }
}

fn write_atomic(target: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|_| "SF-SYS-023".to_owned())?;
    }
    let mut rand_suffix = [0u8; 8];
    fill_csprng(&mut rand_suffix)?;
    let tmp = target.with_extension(format!("tmp.{}", hex::encode(rand_suffix)));
    {
        let mut f = fs::File::create(&tmp).map_err(|_| "SF-SYS-023".to_owned())?;
        f.write_all(bytes).map_err(|_| "SF-SYS-023".to_owned())?;
        f.sync_all().map_err(|_| "SF-SYS-023".to_owned())?;
    }
    fs::rename(&tmp, target).map_err(|_| "SF-SYS-023".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_result;

    fn make_vault_key() -> VaultKey {
        test_result(VaultKey::derive_from_passphrase(
            "SafiDesktopVaultTest#2026",
            b"0123456789abcdef",
        ))
    }

    #[test]
    fn e2ee_two_device_key_wrapping_offline_queue_conflict_and_key_rotation() {
        let dir_a = test_result(tempfile::tempdir());
        let dir_b = test_result(tempfile::tempdir());
        let dir_c = test_result(tempfile::tempdir());

        let store_a = test_result(OrgSyncStore::open_or_init(
            dir_a.path(),
            make_vault_key(),
            "dev-alice-1",
            "Alice Laptop",
        ));
        let store_b = test_result(OrgSyncStore::open_or_init(
            dir_b.path(),
            make_vault_key(),
            "dev-bob-1",
            "Bob Workstation",
        ));
        let store_c = test_result(OrgSyncStore::open_or_init(
            dir_c.path(),
            make_vault_key(),
            "dev-charlie-1",
            "Charlie Laptop",
        ));

        let id_a = test_result(store_a.public_device_identity());
        let id_b = test_result(store_b.public_device_identity());
        let id_c = test_result(store_c.public_device_identity());

        let org_id = "org-001";
        let ws_id = "ws-shared-001";
        test_result(store_a.select_active_workspace(Some(org_id), Some(ws_id), 1));
        test_result(store_b.select_active_workspace(Some(org_id), Some(ws_id), 1));
        test_result(store_c.select_active_workspace(Some(org_id), Some(ws_id), 1));

        // 1. Alice bootstraps Epoch 1 Workspace Key and wraps it for Alice, Bob, and Charlie.
        let devices_epoch_1 = vec![
            RemoteMemberDeviceKey {
                user_id: "user-alice".to_owned(),
                device_id: id_a.device_id.clone(),
                device_label: id_a.device_label.clone(),
                x25519_public_key_b64: id_a.x25519_public_key_b64.clone(),
                key_fingerprint: id_a.key_fingerprint.clone(),
            },
            RemoteMemberDeviceKey {
                user_id: "user-bob".to_owned(),
                device_id: id_b.device_id.clone(),
                device_label: id_b.device_label.clone(),
                x25519_public_key_b64: id_b.x25519_public_key_b64.clone(),
                key_fingerprint: id_b.key_fingerprint.clone(),
            },
            RemoteMemberDeviceKey {
                user_id: "user-charlie".to_owned(),
                device_id: id_c.device_id.clone(),
                device_label: id_c.device_label.clone(),
                x25519_public_key_b64: id_c.x25519_public_key_b64.clone(),
                key_fingerprint: id_c.key_fingerprint.clone(),
            },
        ];
        let wrapped_e1 = test_result(store_a.ensure_and_wrap_epoch_key_for_devices(
            org_id,
            ws_id,
            1,
            &devices_epoch_1,
        ));
        assert_eq!(wrapped_e1.len(), 3);

        // Bob & Charlie unwrap Epoch 1 Workspace Key using their local X25519 private keys.
        let bob_envelopes: Vec<RemoteWrappedKeyEnvelope> = wrapped_e1
            .iter()
            .filter(|w| w.recipient_user_id == "user-bob")
            .map(|w| RemoteWrappedKeyEnvelope {
                organization_id: org_id.to_owned(),
                workspace_id: ws_id.to_owned(),
                key_epoch: 1,
                recipient_user_id: w.recipient_user_id.clone(),
                recipient_device_id: w.recipient_device_id.clone(),
                sender_ephemeral_pub_b64: w.sender_ephemeral_pub_b64.clone(),
                wrapped_key_ciphertext_b64: w.wrapped_key_ciphertext_b64.clone(),
                wrapped_key_nonce_b64: w.wrapped_key_nonce_b64.clone(),
            })
            .collect();
        assert_eq!(
            test_result(store_b.ingest_wrapped_workspace_keys(&bob_envelopes)),
            1
        );

        let charlie_envelopes: Vec<RemoteWrappedKeyEnvelope> = wrapped_e1
            .iter()
            .filter(|w| w.recipient_user_id == "user-charlie")
            .map(|w| RemoteWrappedKeyEnvelope {
                organization_id: org_id.to_owned(),
                workspace_id: ws_id.to_owned(),
                key_epoch: 1,
                recipient_user_id: w.recipient_user_id.clone(),
                recipient_device_id: w.recipient_device_id.clone(),
                sender_ephemeral_pub_b64: w.sender_ephemeral_pub_b64.clone(),
                wrapped_key_ciphertext_b64: w.wrapped_key_ciphertext_b64.clone(),
                wrapped_key_nonce_b64: w.wrapped_key_nonce_b64.clone(),
            })
            .collect();
        assert_eq!(
            test_result(store_c.ingest_wrapped_workspace_keys(&charlie_envelopes)),
            1
        );

        // 2. Alice stages a secret recipe offline, restarts Desktop store, and verifies zero
        //    plaintext on disk AND stable idempotent outbound ciphertext across retries!
        let secret_json = "{\"steps\":[\"trim_whitespace\",\"mask_national_id\"],\"note\":\"ULTRA_SECRET_Q4_REVENUE\"}";
        test_result(store_a.stage_shared_resource_upsert(
            org_id,
            ws_id,
            "recipe",
            "rcp-q4",
            "Q4 Cleaning Recipe",
            secret_json,
        ));
        let disk_bytes = test_result(fs::read(store_a.vault_file_path()));
        let disk_lossy = String::from_utf8_lossy(&disk_bytes);
        assert!(!disk_lossy.contains("ULTRA_SECRET_Q4_REVENUE"));
        assert!(!disk_lossy.contains("Q4 Cleaning Recipe"));

        // Re-open Store A from disk (simulating Desktop cold restart while offline!)
        drop(store_a);
        let store_a = test_result(OrgSyncStore::open_or_init(
            dir_a.path(),
            make_vault_key(),
            "dev-alice-1",
            "Alice Laptop",
        ));
        let out_1 = test_result(store_a.peek_next_outbound_envelope(ws_id));
        let out_1 = test_result(out_1.ok_or("missing queued item"));
        assert!(!out_1.ciphertext_b64.contains("ULTRA_SECRET"));
        // Retry peek returns exact same idempotency_key + ciphertext + nonce!
        let out_retry = test_result(
            test_result(store_a.peek_next_outbound_envelope(ws_id)).ok_or("missing retry"),
        );
        assert_eq!(out_1.idempotency_key, out_retry.idempotency_key);
        assert_eq!(out_1.ciphertext_b64, out_retry.ciphertext_b64);
        assert_eq!(out_1.nonce_b64, out_retry.nonce_b64);

        test_result(store_a.mark_outbound_pushed(&out_1.idempotency_key, 1, 1, Some("user-alice")));

        // 3. Bob pulls and decrypts Alice's envelope!
        let inbound_v1 = InboundEncryptedEnvelope {
            id: "env-1".to_owned(),
            workspace_id: ws_id.to_owned(),
            user_id: "user-alice".to_owned(),
            resource_kind: out_1.resource_kind.clone(),
            resource_id: out_1.resource_id.clone(),
            version: 1,
            workspace_seq: 1,
            ciphertext_b64: out_1.ciphertext_b64.clone(),
            nonce_b64: out_1.nonce_b64.clone(),
            key_id: out_1.key_id.clone(),
            key_epoch: out_1.key_epoch,
            content_hash: out_1.content_hash.clone(),
            deleted: false,
            origin_device_id: out_1.origin_device_id.clone(),
            updated_at: 1_795_000_000,
        };
        assert_eq!(
            test_result(store_b.apply_pulled_changes(ws_id, std::slice::from_ref(&inbound_v1))),
            1
        );
        let bob_resources = test_result(store_b.list_shared_resources(ws_id, false));
        assert_eq!(bob_resources.len(), 1);
        assert_eq!(bob_resources[0].title, "Q4 Cleaning Recipe");
        assert_eq!(bob_resources[0].payload_json, secret_json);

        // Tampered ciphertext MUST be rejected on pull!
        let mut tampered = inbound_v1.clone();
        tampered.content_hash = "0".repeat(64);
        assert!(store_b.apply_pulled_changes(ws_id, &[tampered]).is_err());

        // 4. Conflict detection & resolution (`KeepLocal`):
        // Both Alice and Bob edit v1 concurrently; Alice commits v2, Bob gets conflict & chooses KeepLocal!
        test_result(store_a.stage_shared_resource_upsert(
            org_id,
            ws_id,
            "recipe",
            "rcp-q4",
            "Q4 Recipe (Alice v2)",
            "{\"owner\":\"alice-v2\"}",
        ));
        let out_alice_v2 =
            test_result(test_result(store_a.peek_next_outbound_envelope(ws_id)).ok_or("v2"));
        test_result(store_a.mark_outbound_pushed(
            &out_alice_v2.idempotency_key,
            2,
            2,
            Some("user-alice"),
        ));

        test_result(store_b.stage_shared_resource_upsert(
            org_id,
            ws_id,
            "recipe",
            "rcp-q4",
            "Q4 Recipe (Bob Concurrent)",
            "{\"owner\":\"bob-concurrent\"}",
        ));
        let out_bob_stale =
            test_result(test_result(store_b.peek_next_outbound_envelope(ws_id)).ok_or("bob stale"));
        assert_eq!(out_bob_stale.expected_version, 1);

        let inbound_alice_v2 = InboundEncryptedEnvelope {
            id: "env-2".to_owned(),
            workspace_id: ws_id.to_owned(),
            user_id: "user-alice".to_owned(),
            resource_kind: "recipe".to_owned(),
            resource_id: "rcp-q4".to_owned(),
            version: 2,
            workspace_seq: 2,
            ciphertext_b64: out_alice_v2.ciphertext_b64,
            nonce_b64: out_alice_v2.nonce_b64,
            key_id: out_alice_v2.key_id,
            key_epoch: out_alice_v2.key_epoch,
            content_hash: out_alice_v2.content_hash,
            deleted: false,
            origin_device_id: out_alice_v2.origin_device_id,
            updated_at: 1_795_000_010,
        };
        let conflict = test_result(
            store_b.record_push_conflict(&out_bob_stale.idempotency_key, &inbound_alice_v2),
        );
        assert_eq!(conflict.remote_version, 2);
        assert_eq!(conflict.remote_title, "Q4 Recipe (Alice v2)");
        assert_eq!(
            test_result(store_b.status_for_workspace(Some(ws_id))).health,
            SyncHealthState::ConflictDetected
        );

        let resolved = test_result(store_b.resolve_conflict(
            ws_id,
            "recipe",
            "rcp-q4",
            ConflictResolutionChoice::KeepLocal,
        ));
        assert_eq!(resolved.version, 2);
        let out_bob_v3 =
            test_result(test_result(store_b.peek_next_outbound_envelope(ws_id)).ok_or("bob v3"));
        assert_eq!(out_bob_v3.expected_version, 2);

        // 5. Charlie is removed -> Alice rotates Workspace Key to Epoch 2 for ONLY Alice & Bob!
        let remaining_devices = devices_epoch_1[..2].to_vec();
        let (new_epoch, wrapped_e2) = test_result(store_a.prepare_rotated_workspace_key(
            org_id,
            ws_id,
            1,
            &remaining_devices,
        ));
        assert_eq!(new_epoch, 2);
        assert_eq!(wrapped_e2.len(), 2);

        let bob_e2: Vec<RemoteWrappedKeyEnvelope> = wrapped_e2
            .iter()
            .filter(|w| w.recipient_user_id == "user-bob")
            .map(|w| RemoteWrappedKeyEnvelope {
                organization_id: org_id.to_owned(),
                workspace_id: ws_id.to_owned(),
                key_epoch: 2,
                recipient_user_id: w.recipient_user_id.clone(),
                recipient_device_id: w.recipient_device_id.clone(),
                sender_ephemeral_pub_b64: w.sender_ephemeral_pub_b64.clone(),
                wrapped_key_ciphertext_b64: w.wrapped_key_ciphertext_b64.clone(),
                wrapped_key_nonce_b64: w.wrapped_key_nonce_b64.clone(),
            })
            .collect();
        assert_eq!(
            test_result(store_b.ingest_wrapped_workspace_keys(&bob_e2)),
            1
        );

        // Alice encrypts a new post-rotation resource under Epoch 2: Bob decrypts it, Charlie CANNOT!
        test_result(store_a.stage_shared_resource_upsert(
            org_id,
            ws_id,
            "dashboard",
            "dash-post-rotation",
            "Post-Rotation Executive KPI",
            "{\"secret\":\"POST_ROTATION_EYES_ONLY\"}",
        ));
        let out_e2 =
            test_result(test_result(store_a.peek_next_outbound_envelope(ws_id)).ok_or("e2 env"));
        assert_eq!(out_e2.key_epoch, 2);

        let inbound_e2 = InboundEncryptedEnvelope {
            id: "env-3".to_owned(),
            workspace_id: ws_id.to_owned(),
            user_id: "user-alice".to_owned(),
            resource_kind: out_e2.resource_kind,
            resource_id: out_e2.resource_id,
            version: 1,
            workspace_seq: 3,
            ciphertext_b64: out_e2.ciphertext_b64,
            nonce_b64: out_e2.nonce_b64,
            key_id: out_e2.key_id,
            key_epoch: 2,
            content_hash: out_e2.content_hash,
            deleted: false,
            origin_device_id: out_e2.origin_device_id,
            updated_at: 1_795_000_020,
        };
        assert_eq!(
            test_result(store_b.apply_pulled_changes(ws_id, std::slice::from_ref(&inbound_e2))),
            1
        );
        // Charlie lacks Epoch 2 key AND even if Charlie tries to unwrap Bob's wrapped key, X25519 AEAD rejects!
        assert!(
            store_c
                .apply_pulled_changes(ws_id, std::slice::from_ref(&inbound_e2))
                .is_err()
        );
        let forged_for_charlie = vec![RemoteWrappedKeyEnvelope {
            organization_id: org_id.to_owned(),
            workspace_id: ws_id.to_owned(),
            key_epoch: 2,
            recipient_user_id: "user-charlie".to_owned(),
            recipient_device_id: id_c.device_id,
            sender_ephemeral_pub_b64: bob_e2[0].sender_ephemeral_pub_b64.clone(),
            wrapped_key_ciphertext_b64: bob_e2[0].wrapped_key_ciphertext_b64.clone(),
            wrapped_key_nonce_b64: bob_e2[0].wrapped_key_nonce_b64.clone(),
        }];
        assert!(
            store_c
                .ingest_wrapped_workspace_keys(&forged_for_charlie)
                .is_err()
        );
    }
}
