//! Production Organizations, Shared Team Workspaces, Verified-Email Invitations,
//! 4-Role Server RBAC (`owner`, `admin`, `member`, `viewer`), and X25519 Device Key Directory.
//!
//! Security & durability invariants:
//! - Minimum 3 seats enforced on creation and seat adjustment (`SF-SYN-004`).
//! - Authoritative PostgreSQL 18 persistence via `organizations_pg` when `PgControlPlaneStore`
//!   is attached; strict environments (`Staging`/`Production`) fail closed (`SF-AUTH-005`) if
//!   PostgreSQL is absent.
//! - Invitation tokens (`sfi_*`) are CSPRNG 256-bit secrets stored ONLY as `BLAKE3` digests.
//! - Accepting or declining an invitation requires the authenticated caller's verified,
//!   normalized account email to match `invited_email_norm`; token possession alone is rejected
//!   (`403 SF-SYN-005`).
//! - Server-side 4-role RBAC prevents self-promotion, protects the last Owner, scopes `Admin`
//!   role/member management strictly to `Member`/`Viewer`, blocks `Viewer` from shared resource
//!   writes, and restricts subscription/ownership-transfer/org-deletion to `Owner` only.

use std::collections::HashMap;
use std::sync::Arc;

use base64::Engine;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::auth::unix_now;
use crate::entitlements::ORG_MIN_SEATS;
use crate::error::AuthError;
use crate::organizations_pg;
use crate::pg_store::PgControlPlaneStore;

pub const DEFAULT_INVITATION_TTL_SECS: i64 = 7 * 24 * 60 * 60;
const X25519_PUBLIC_KEY_BYTES: usize = 32;
const WRAPPED_WORKSPACE_KEY_BYTES: usize = 48; // 32B key + 16B Poly1305 tag
const WRAPPED_KEY_NONCE_BYTES: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrgRole {
    Owner,
    Admin,
    Member,
    Viewer,
}

impl OrgRole {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Admin => "admin",
            Self::Member => "member",
            Self::Viewer => "viewer",
        }
    }

    pub fn parse_str(raw: &str) -> Result<Self, AuthError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "owner" => Ok(Self::Owner),
            "admin" => Ok(Self::Admin),
            "member" => Ok(Self::Member),
            "viewer" => Ok(Self::Viewer),
            _ => Err(AuthError::InvalidRequest),
        }
    }

    /// View shared workspace, member list, and synced resources.
    #[must_use]
    pub const fn can_view_workspace(self) -> bool {
        matches!(
            self,
            Self::Owner | Self::Admin | Self::Member | Self::Viewer
        )
    }

    /// Manage members and send/revoke invitations.
    #[must_use]
    pub const fn can_manage_members(self) -> bool {
        matches!(self, Self::Owner | Self::Admin)
    }

    /// Send or revoke organization invitations.
    #[must_use]
    pub const fn can_send_invitations(self) -> bool {
        matches!(self, Self::Owner | Self::Admin)
    }

    /// Create/modify/delete shared workspace resources (push sync envelopes, create/revoke share links).
    /// `Viewer` is strictly read-only.
    #[must_use]
    pub const fn can_modify_shared_resources(self) -> bool {
        matches!(self, Self::Owner | Self::Admin | Self::Member)
    }

    /// Manage organization subscription and seat purchases (`Owner` only).
    #[must_use]
    pub const fn can_manage_subscription(self) -> bool {
        matches!(self, Self::Owner)
    }

    /// Transfer organization ownership or delete organization (`Owner` only).
    #[must_use]
    pub const fn is_owner(self) -> bool {
        matches!(self, Self::Owner)
    }

    /// Policy-enforced role change check:
    /// - Nobody can assign `Owner` via role change (must use `transfer_ownership`).
    /// - Nobody can modify their own role (prevents self-promotion & accidental last-Owner self-demotion).
    /// - `Owner` can change any non-Owner member between `Admin`, `Member`, and `Viewer`.
    /// - `Admin` can ONLY change `Member` <-> `Viewer` (never touching `Owner` or `Admin`, and never promoting to `Admin` or `Owner`).
    #[must_use]
    pub fn can_change_role_of(
        self,
        actor_user_id: Uuid,
        target_user_id: Uuid,
        target_current_role: Self,
        new_role: Self,
    ) -> bool {
        if actor_user_id == target_user_id
            || new_role == Self::Owner
            || target_current_role == Self::Owner
        {
            return false;
        }
        match self {
            Self::Owner => matches!(new_role, Self::Admin | Self::Member | Self::Viewer),
            Self::Admin => {
                matches!(target_current_role, Self::Member | Self::Viewer)
                    && matches!(new_role, Self::Member | Self::Viewer)
            }
            Self::Member | Self::Viewer => false,
        }
    }

    /// Policy-enforced member removal check:
    /// - `Owner` can never be removed without `transfer_ownership`.
    /// - `Owner` can remove any `Admin`, `Member`, or `Viewer`.
    /// - `Admin` can ONLY remove `Member` or `Viewer` (never `Owner` or `Admin`).
    #[must_use]
    pub fn can_remove_target(
        self,
        actor_user_id: Uuid,
        target_user_id: Uuid,
        target_role: Self,
    ) -> bool {
        if target_role == Self::Owner || actor_user_id == target_user_id {
            return false;
        }
        match self {
            Self::Owner => true,
            Self::Admin => matches!(target_role, Self::Member | Self::Viewer),
            Self::Member | Self::Viewer => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InvitationStatus {
    Pending,
    Accepted,
    Declined,
    Revoked,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Organization {
    pub id: Uuid,
    pub workspace_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub billing_workspace_id: Option<Uuid>,
    pub name: String,
    pub purchased_seats: u32,
    #[serde(default = "default_billing_cycle")]
    pub billing_cycle: String,
    pub owner_user_id: Uuid,
    #[serde(default = "default_key_epoch")]
    pub active_key_epoch: u32,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<i64>,
}

fn default_billing_cycle() -> String {
    "annual_per_seat".to_owned()
}

const fn default_key_epoch() -> u32 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationMember {
    pub organization_id: Uuid,
    #[serde(default)]
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub role: OrgRole,
    pub joined_at: i64,
    pub removed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationInvitation {
    pub id: Uuid,
    pub organization_id: Uuid,
    #[serde(default)]
    pub workspace_id: Uuid,
    pub invited_email: String,
    pub role: OrgRole,
    #[serde(skip_serializing)]
    pub token_hash: String,
    pub invited_by: Uuid,
    pub expires_at: i64,
    pub accepted_at: Option<i64>,
    #[serde(default)]
    pub declined_at: Option<i64>,
    pub revoked_at: Option<i64>,
    #[serde(default)]
    pub created_at: i64,
}

impl OrganizationInvitation {
    #[must_use]
    pub fn status_at(&self, now_secs: i64) -> InvitationStatus {
        if self.accepted_at.is_some() {
            InvitationStatus::Accepted
        } else if self.declined_at.is_some() {
            InvitationStatus::Declined
        } else if self.revoked_at.is_some() {
            InvitationStatus::Revoked
        } else if now_secs >= self.expires_at {
            InvitationStatus::Expired
        } else {
            InvitationStatus::Pending
        }
    }

    #[must_use]
    pub fn to_view(&self, now_secs: i64) -> OrganizationInvitationView {
        OrganizationInvitationView {
            id: self.id,
            organization_id: self.organization_id,
            workspace_id: self.workspace_id,
            invited_email: self.invited_email.clone(),
            role: self.role,
            invited_by: self.invited_by,
            status: self.status_at(now_secs),
            expires_at: self.expires_at,
            accepted_at: self.accepted_at,
            declined_at: self.declined_at,
            revoked_at: self.revoked_at,
            created_at: self.created_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationInvitationView {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub workspace_id: Uuid,
    pub invited_email: String,
    pub role: OrgRole,
    pub invited_by: Uuid,
    pub status: InvitationStatus,
    pub expires_at: i64,
    pub accepted_at: Option<i64>,
    pub declined_at: Option<i64>,
    pub revoked_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingUserInvitationView {
    pub invitation_id: Uuid,
    pub organization_id: Uuid,
    pub organization_name: String,
    pub workspace_id: Uuid,
    pub invited_email: String,
    pub role: OrgRole,
    pub expires_at: i64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationMembershipSummary {
    pub organization: Organization,
    pub my_role: OrgRole,
    pub active_members_count: u32,
    pub pending_invitations_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberDevicePublicKey {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub device_id: String,
    pub device_label: String,
    pub x25519_public_key_b64: String,
    pub key_fingerprint: String,
    pub created_at: i64,
    pub revoked_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WrappedKeyEntryInput {
    pub recipient_user_id: Uuid,
    pub recipient_device_id: String,
    pub sender_ephemeral_pub_b64: String,
    pub wrapped_key_ciphertext_b64: String,
    pub wrapped_key_nonce_b64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WrappedWorkspaceKeyRecord {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub workspace_id: Uuid,
    pub key_epoch: u32,
    pub recipient_user_id: Uuid,
    pub recipient_device_id: String,
    pub sender_user_id: Uuid,
    pub sender_ephemeral_pub_b64: String,
    pub wrapped_key_ciphertext_b64: String,
    pub wrapped_key_nonce_b64: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OrganizationSnapshot {
    pub orgs: Vec<Organization>,
    pub members: Vec<OrganizationMember>,
    pub invitations: Vec<OrganizationInvitation>,
    pub device_keys: Vec<MemberDevicePublicKey>,
    pub wrapped_keys: Vec<WrappedWorkspaceKeyRecord>,
}

#[derive(Debug, Default)]
struct OrgStore {
    orgs: HashMap<Uuid, Organization>,
    members: Vec<OrganizationMember>,
    invitations: Vec<OrganizationInvitation>,
    device_keys: Vec<MemberDevicePublicKey>,
    wrapped_keys: Vec<WrappedWorkspaceKeyRecord>,
}

/// Normalize an email address consistently across account registration and invitations.
pub fn normalize_email(raw: &str) -> Result<String, AuthError> {
    let norm = raw.trim().to_ascii_lowercase();
    let Some((local, domain)) = norm.split_once('@') else {
        return Err(AuthError::InvalidRequest);
    };
    if local.is_empty()
        || domain.is_empty()
        || !domain.contains('.')
        || domain.starts_with('.')
        || domain.ends_with('.')
        || norm.len() > 254
        || norm.chars().any(char::is_whitespace)
    {
        return Err(AuthError::InvalidRequest);
    }
    Ok(norm)
}

/// Generate a cryptographically random 256-bit `sfi_<hex>` single-use invitation token.
pub fn generate_invitation_token() -> Result<(String, String), AuthError> {
    let rng = SystemRandom::new();
    let mut secret = [0u8; 32];
    rng.fill(&mut secret)
        .map_err(|_| AuthError::BackendUnavailable)?;
    let raw_token = format!("sfi_{}", hex::encode(secret));
    let token_hash = blake3::hash(raw_token.as_bytes()).to_hex().to_string();
    Ok((raw_token, token_hash))
}

pub(crate) fn validate_x25519_public_key_b64(
    raw_b64: &str,
) -> Result<(Vec<u8>, String), AuthError> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let decoded = b64
        .decode(raw_b64.trim())
        .map_err(|_| AuthError::SyncEnvelopeInvalid)?;
    if decoded.len() != X25519_PUBLIC_KEY_BYTES || decoded.iter().all(|b| *b == 0) {
        return Err(AuthError::SyncEnvelopeInvalid);
    }
    let fingerprint = blake3::hash(&decoded).to_hex()[..32].to_string();
    Ok((decoded, fingerprint))
}

pub(crate) struct ValidatedWrappedKeyBytes {
    pub recipient_user_id: Uuid,
    pub recipient_device_id: String,
    pub sender_ephemeral_pub: Vec<u8>,
    pub wrapped_key_ciphertext: Vec<u8>,
    pub wrapped_key_nonce: Vec<u8>,
}

pub(crate) fn validate_wrapped_key_entry(
    entry: &WrappedKeyEntryInput,
) -> Result<ValidatedWrappedKeyBytes, AuthError> {
    let device_id = entry.recipient_device_id.trim();
    if device_id.is_empty() || device_id.len() > 128 {
        return Err(AuthError::InvalidRequest);
    }
    let b64 = base64::engine::general_purpose::STANDARD;
    let eph = b64
        .decode(entry.sender_ephemeral_pub_b64.trim())
        .map_err(|_| AuthError::SyncEnvelopeInvalid)?;
    let ct = b64
        .decode(entry.wrapped_key_ciphertext_b64.trim())
        .map_err(|_| AuthError::SyncEnvelopeInvalid)?;
    let nonce = b64
        .decode(entry.wrapped_key_nonce_b64.trim())
        .map_err(|_| AuthError::SyncEnvelopeInvalid)?;
    if eph.len() != X25519_PUBLIC_KEY_BYTES
        || eph.iter().all(|b| *b == 0)
        || ct.len() != WRAPPED_WORKSPACE_KEY_BYTES
        || nonce.len() != WRAPPED_KEY_NONCE_BYTES
    {
        return Err(AuthError::SyncEnvelopeInvalid);
    }
    Ok(ValidatedWrappedKeyBytes {
        recipient_user_id: entry.recipient_user_id,
        recipient_device_id: device_id.to_owned(),
        sender_ephemeral_pub: eph,
        wrapped_key_ciphertext: ct,
        wrapped_key_nonce: nonce,
    })
}

/// Authoritative Organization, RBAC, Invitation, and Key-Directory service.
#[derive(Debug, Clone, Default)]
pub struct OrganizationService {
    store: Arc<RwLock<OrgStore>>,
    pg_store: Option<PgControlPlaneStore>,
    require_durable_pg: bool,
}

impl OrganizationService {
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

    pub async fn export_snapshot(&self) -> OrganizationSnapshot {
        let guard = self.store.read().await;
        OrganizationSnapshot {
            orgs: guard.orgs.values().cloned().collect(),
            members: guard.members.clone(),
            invitations: guard.invitations.clone(),
            device_keys: guard.device_keys.clone(),
            wrapped_keys: guard.wrapped_keys.clone(),
        }
    }

    pub async fn import_snapshot(&self, snap: OrganizationSnapshot) {
        let mut guard = self.store.write().await;
        guard.orgs = snap.orgs.into_iter().map(|o| (o.id, o)).collect();
        guard.members = snap.members;
        guard.invitations = snap.invitations;
        guard.device_keys = snap.device_keys;
        guard.wrapped_keys = snap.wrapped_keys;
    }

    /// Create an organization with its own dedicated shared team workspace, bound to the Owner's
    /// paid seat entitlement (`SF-SYN-004` / `SF-LIC-005`).
    pub async fn create_organization_with_shared_workspace(
        &self,
        billing_workspace_id: Uuid,
        owner_user_id: Uuid,
        name: &str,
        requested_seats: u32,
        max_paid_seats: u32,
        billing_cycle: &str,
    ) -> Result<Organization, AuthError> {
        self.ensure_durable_ready()?;
        let trimmed = name.trim();
        if trimmed.is_empty() || trimmed.len() > 120 {
            return Err(AuthError::InvalidRequest);
        }
        if requested_seats < ORG_MIN_SEATS || max_paid_seats < ORG_MIN_SEATS {
            return Err(AuthError::OrgMinSeatsRequired);
        }
        if requested_seats > max_paid_seats {
            return Err(AuthError::QuotaExceeded);
        }
        let cycle = match billing_cycle.trim() {
            "monthly_per_seat" => "monthly_per_seat",
            _ => "annual_per_seat",
        };
        if let Some(pg) = &self.pg_store {
            return organizations_pg::create_organization_pg(
                pg,
                billing_workspace_id,
                owner_user_id,
                trimmed,
                requested_seats,
                cycle,
            )
            .await;
        }

        let now = unix_now();
        let shared_workspace_id = Uuid::now_v7();
        let org = Organization {
            id: Uuid::now_v7(),
            workspace_id: shared_workspace_id,
            billing_workspace_id: Some(billing_workspace_id),
            name: trimmed.to_owned(),
            purchased_seats: requested_seats,
            billing_cycle: cycle.to_owned(),
            owner_user_id,
            active_key_epoch: 1,
            created_at: now,
            deleted_at: None,
        };
        let mut guard = self.store.write().await;
        guard.orgs.insert(org.id, org.clone());
        guard.members.push(OrganizationMember {
            organization_id: org.id,
            workspace_id: shared_workspace_id,
            user_id: owner_user_id,
            email: None,
            display_name: None,
            role: OrgRole::Owner,
            joined_at: now,
            removed_at: None,
        });
        Ok(org)
    }

    /// Create an organization enforcing minimum 3 seats (`SF-SYN-004`).
    pub async fn create_org(
        &self,
        workspace_id: Uuid,
        owner_user_id: Uuid,
        name: &str,
        seats: u32,
    ) -> Result<Organization, AuthError> {
        self.create_organization_with_shared_workspace(
            workspace_id,
            owner_user_id,
            name,
            seats,
            seats.max(ORG_MIN_SEATS),
            "annual_per_seat",
        )
        .await
    }

    /// Look up an active organization by ID if the caller is an active member (or platform owner).
    pub async fn get_organization_for_actor(
        &self,
        organization_id: Uuid,
        actor_user_id: Uuid,
        is_platform_owner: bool,
    ) -> Result<(Organization, Option<OrgRole>, u32, u32), AuthError> {
        self.ensure_durable_ready()?;
        let now = unix_now();
        if let Some(pg) = &self.pg_store {
            return organizations_pg::get_organization_for_actor_pg(
                pg,
                organization_id,
                actor_user_id,
                is_platform_owner,
                now,
            )
            .await;
        }
        let guard = self.store.read().await;
        let org = guard
            .orgs
            .get(&organization_id)
            .filter(|o| o.deleted_at.is_none())
            .cloned()
            .ok_or(AuthError::OrgPermissionDenied)?;
        let my_role = guard
            .members
            .iter()
            .find(|m| {
                m.organization_id == organization_id
                    && m.user_id == actor_user_id
                    && m.removed_at.is_none()
            })
            .map(|m| m.role);
        if my_role.is_none() && !is_platform_owner {
            return Err(AuthError::OrgPermissionDenied);
        }
        let active_seats = guard
            .members
            .iter()
            .filter(|m| m.organization_id == organization_id && m.removed_at.is_none())
            .count() as u32;
        let pending_invites = guard
            .invitations
            .iter()
            .filter(|i| {
                i.organization_id == organization_id
                    && i.status_at(now) == InvitationStatus::Pending
            })
            .count() as u32;
        Ok((org, my_role, active_seats, pending_invites))
    }

    /// Look up an active organization by its shared `workspace_id`.
    pub async fn get_organization_by_workspace(
        &self,
        workspace_id: Uuid,
    ) -> Result<Option<Organization>, AuthError> {
        self.ensure_durable_ready()?;
        if let Some(pg) = &self.pg_store {
            return organizations_pg::get_organization_by_workspace_pg(pg, workspace_id).await;
        }
        let guard = self.store.read().await;
        Ok(guard
            .orgs
            .values()
            .find(|o| o.workspace_id == workspace_id && o.deleted_at.is_none())
            .cloned())
    }

    /// Verify that `actor_user_id` is an active member of the organization owning `workspace_id`.
    pub async fn require_workspace_member_role(
        &self,
        workspace_id: Uuid,
        actor_user_id: Uuid,
    ) -> Result<(Organization, OrgRole), AuthError> {
        self.ensure_durable_ready()?;
        if let Some(pg) = &self.pg_store {
            return organizations_pg::require_workspace_member_role_pg(
                pg,
                workspace_id,
                actor_user_id,
            )
            .await;
        }
        let guard = self.store.read().await;
        let org = guard
            .orgs
            .values()
            .find(|o| o.workspace_id == workspace_id && o.deleted_at.is_none())
            .cloned()
            .ok_or(AuthError::OrgPermissionDenied)?;
        let role = guard
            .members
            .iter()
            .find(|m| {
                m.organization_id == org.id && m.user_id == actor_user_id && m.removed_at.is_none()
            })
            .map(|m| m.role)
            .ok_or(AuthError::OrgPermissionDenied)?;
        Ok((org, role))
    }

    /// List organizations the caller belongs to plus pending unexpired invitations addressed to
    /// their verified email.
    pub async fn list_user_organizations_and_invites(
        &self,
        user_id: Uuid,
        verified_email_norm: Option<&str>,
        now_secs: i64,
    ) -> Result<
        (
            Vec<OrganizationMembershipSummary>,
            Vec<PendingUserInvitationView>,
        ),
        AuthError,
    > {
        self.ensure_durable_ready()?;
        if let Some(pg) = &self.pg_store {
            return organizations_pg::list_user_organizations_and_invites_pg(
                pg,
                user_id,
                verified_email_norm,
                now_secs,
            )
            .await;
        }
        let guard = self.store.read().await;
        let mut orgs = Vec::new();
        for m in &guard.members {
            if m.user_id == user_id && m.removed_at.is_none() {
                if let Some(org) = guard
                    .orgs
                    .get(&m.organization_id)
                    .filter(|o| o.deleted_at.is_none())
                {
                    let active_members_count = guard
                        .members
                        .iter()
                        .filter(|x| x.organization_id == org.id && x.removed_at.is_none())
                        .count() as u32;
                    let pending_invitations_count = guard
                        .invitations
                        .iter()
                        .filter(|i| {
                            i.organization_id == org.id
                                && i.status_at(now_secs) == InvitationStatus::Pending
                        })
                        .count() as u32;
                    orgs.push(OrganizationMembershipSummary {
                        organization: org.clone(),
                        my_role: m.role,
                        active_members_count,
                        pending_invitations_count,
                    });
                }
            }
        }
        orgs.sort_by_key(|s| s.organization.created_at);

        let mut pending = Vec::new();
        if let Some(email) = verified_email_norm {
            let norm = email.trim().to_ascii_lowercase();
            for inv in &guard.invitations {
                if inv.invited_email == norm && inv.status_at(now_secs) == InvitationStatus::Pending
                {
                    if let Some(org) = guard
                        .orgs
                        .get(&inv.organization_id)
                        .filter(|o| o.deleted_at.is_none())
                    {
                        pending.push(PendingUserInvitationView {
                            invitation_id: inv.id,
                            organization_id: org.id,
                            organization_name: org.name.clone(),
                            workspace_id: org.workspace_id,
                            invited_email: inv.invited_email.clone(),
                            role: inv.role,
                            expires_at: inv.expires_at,
                            created_at: inv.created_at,
                        });
                    }
                }
            }
        }
        Ok((orgs, pending))
    }

    /// Synchronize `purchased_seats` when the Owner upgrades their Organization subscription.
    pub async fn sync_purchased_seats(
        &self,
        organization_id: Uuid,
        new_paid_seats: u32,
    ) -> Result<(), AuthError> {
        self.ensure_durable_ready()?;
        if new_paid_seats < ORG_MIN_SEATS {
            return Err(AuthError::OrgMinSeatsRequired);
        }
        if let Some(pg) = &self.pg_store {
            return organizations_pg::sync_purchased_seats_pg(pg, organization_id, new_paid_seats)
                .await;
        }
        let mut guard = self.store.write().await;
        if let Some(org) = guard.orgs.get_mut(&organization_id) {
            org.purchased_seats = new_paid_seats;
        }
        Ok(())
    }

    /// Invite a member with full RBAC, duplicate detection, and paid-seat cap enforcement.
    #[allow(clippy::too_many_arguments)]
    pub async fn invite_member_checked(
        &self,
        organization_id: Uuid,
        actor_user_id: Uuid,
        invited_email: &str,
        role: OrgRole,
        max_paid_seats: Option<u32>,
        existing_member_emails: &HashMap<Uuid, String>,
        ttl_secs: i64,
        now_secs: i64,
    ) -> Result<(OrganizationInvitation, String), AuthError> {
        self.ensure_durable_ready()?;
        if role == OrgRole::Owner || ttl_secs <= 0 || ttl_secs > 30 * 86_400 {
            return Err(AuthError::InvalidRequest);
        }
        let email_norm = normalize_email(invited_email)?;
        let (raw_token, token_hash) = generate_invitation_token()?;

        if let Some(pg) = &self.pg_store {
            let invite = organizations_pg::invite_member_pg(
                pg,
                organization_id,
                actor_user_id,
                &email_norm,
                role,
                &token_hash,
                max_paid_seats,
                ttl_secs,
                now_secs,
            )
            .await?;
            return Ok((invite, raw_token));
        }

        let mut guard = self.store.write().await;
        let org = guard
            .orgs
            .get(&organization_id)
            .filter(|o| o.deleted_at.is_none())
            .cloned()
            .ok_or(AuthError::OrgPermissionDenied)?;

        let actor_role = guard
            .members
            .iter()
            .find(|m| {
                m.organization_id == organization_id
                    && m.user_id == actor_user_id
                    && m.removed_at.is_none()
            })
            .map(|m| m.role)
            .ok_or(AuthError::OrgPermissionDenied)?;
        if !actor_role.can_send_invitations() {
            return Err(AuthError::OrgPermissionDenied);
        }
        // Policy: Admin cannot invite another Admin — only Owner can appoint Admins.
        if actor_role == OrgRole::Admin && role == OrgRole::Admin {
            return Err(AuthError::OrgPermissionDenied);
        }

        // Prevent inviting a user who is already an active member of this organization.
        for m in guard
            .members
            .iter()
            .filter(|m| m.organization_id == organization_id && m.removed_at.is_none())
        {
            let m_email = m
                .email
                .as_deref()
                .or_else(|| existing_member_emails.get(&m.user_id).map(String::as_str));
            if let Some(existing_em) = m_email {
                if existing_em.trim().eq_ignore_ascii_case(&email_norm) {
                    return Err(AuthError::SyncVersionConflict);
                }
            }
        }

        // Auto-archive expired invitations for this email, and reject active duplicate invitations.
        for inv in &mut guard.invitations {
            if inv.organization_id == organization_id
                && inv.invited_email == email_norm
                && inv.accepted_at.is_none()
                && inv.revoked_at.is_none()
                && inv.declined_at.is_none()
            {
                if now_secs >= inv.expires_at {
                    inv.revoked_at = Some(inv.expires_at);
                } else {
                    return Err(AuthError::SyncVersionConflict);
                }
            }
        }

        let seat_cap = match max_paid_seats {
            Some(paid) => org.purchased_seats.min(paid),
            None => org.purchased_seats,
        };
        let active_seats = guard
            .members
            .iter()
            .filter(|m| m.organization_id == organization_id && m.removed_at.is_none())
            .count() as u32;
        let pending_seats = guard
            .invitations
            .iter()
            .filter(|i| {
                i.organization_id == organization_id
                    && i.status_at(now_secs) == InvitationStatus::Pending
            })
            .count() as u32;
        if active_seats + pending_seats >= seat_cap {
            return Err(AuthError::QuotaExceeded);
        }

        let invite = OrganizationInvitation {
            id: Uuid::now_v7(),
            organization_id,
            workspace_id: org.workspace_id,
            invited_email: email_norm,
            role,
            token_hash,
            invited_by: actor_user_id,
            expires_at: now_secs + ttl_secs,
            accepted_at: None,
            declined_at: None,
            revoked_at: None,
            created_at: now_secs,
        };
        guard.invitations.push(invite.clone());
        Ok((invite, raw_token))
    }

    /// Convenience wrapper preserving existing unit-test signature.
    pub async fn invite_member(
        &self,
        organization_id: Uuid,
        actor_user_id: Uuid,
        invited_email: &str,
        role: OrgRole,
    ) -> Result<(OrganizationInvitation, String), AuthError> {
        self.invite_member_checked(
            organization_id,
            actor_user_id,
            invited_email,
            role,
            None,
            &HashMap::new(),
            DEFAULT_INVITATION_TTL_SECS,
            unix_now(),
        )
        .await
    }

    /// List organization invitations (Owner & Admin only).
    pub async fn list_invitations(
        &self,
        organization_id: Uuid,
        actor_user_id: Uuid,
        now_secs: i64,
    ) -> Result<Vec<OrganizationInvitationView>, AuthError> {
        self.ensure_durable_ready()?;
        if let Some(pg) = &self.pg_store {
            return organizations_pg::list_invitations_pg(
                pg,
                organization_id,
                actor_user_id,
                now_secs,
            )
            .await;
        }
        let guard = self.store.read().await;
        let _org = guard
            .orgs
            .get(&organization_id)
            .filter(|o| o.deleted_at.is_none())
            .ok_or(AuthError::OrgPermissionDenied)?;
        let actor_role = guard
            .members
            .iter()
            .find(|m| {
                m.organization_id == organization_id
                    && m.user_id == actor_user_id
                    && m.removed_at.is_none()
            })
            .map(|m| m.role)
            .ok_or(AuthError::OrgPermissionDenied)?;
        if !actor_role.can_send_invitations() {
            return Err(AuthError::OrgPermissionDenied);
        }
        let mut views: Vec<OrganizationInvitationView> = guard
            .invitations
            .iter()
            .filter(|i| i.organization_id == organization_id)
            .map(|i| i.to_view(now_secs))
            .collect();
        views.sort_by_key(|a| std::cmp::Reverse(a.created_at));
        Ok(views)
    }

    /// Revoke a pending invitation (Owner & Admin only).
    pub async fn revoke_invitation(
        &self,
        organization_id: Uuid,
        invitation_id: Uuid,
        actor_user_id: Uuid,
        now_secs: i64,
    ) -> Result<OrganizationInvitationView, AuthError> {
        self.ensure_durable_ready()?;
        if let Some(pg) = &self.pg_store {
            return organizations_pg::revoke_invitation_pg(
                pg,
                organization_id,
                invitation_id,
                actor_user_id,
                now_secs,
            )
            .await;
        }
        let mut guard = self.store.write().await;
        let actor_role = guard
            .members
            .iter()
            .find(|m| {
                m.organization_id == organization_id
                    && m.user_id == actor_user_id
                    && m.removed_at.is_none()
            })
            .map(|m| m.role)
            .ok_or(AuthError::OrgPermissionDenied)?;
        if !actor_role.can_send_invitations() {
            return Err(AuthError::OrgPermissionDenied);
        }
        let inv = guard
            .invitations
            .iter_mut()
            .find(|i| i.id == invitation_id && i.organization_id == organization_id)
            .ok_or(AuthError::InvalidRequest)?;
        if inv.accepted_at.is_some() || inv.declined_at.is_some() || inv.revoked_at.is_some() {
            return Err(AuthError::InvalidVerificationToken);
        }
        inv.revoked_at = Some(now_secs);
        Ok(inv.to_view(now_secs))
    }

    /// Accept an invitation enforcing single-use, expiry, paid seat cap, and MANDATORY verified
    /// email match (`joining_verified_email_norm == invite.invited_email_norm`).
    #[allow(clippy::too_many_arguments)]
    pub async fn accept_invitation_verified(
        &self,
        raw_token: Option<&str>,
        invitation_id: Option<Uuid>,
        joining_user_id: Uuid,
        joining_verified_email: &str,
        joining_display_name: Option<&str>,
        max_paid_seats: Option<u32>,
        now_secs: i64,
    ) -> Result<OrganizationMember, AuthError> {
        self.ensure_durable_ready()?;
        let caller_email_norm = normalize_email(joining_verified_email)?;
        let token_hash = raw_token
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(|t| blake3::hash(t.as_bytes()).to_hex().to_string());
        if token_hash.is_none() && invitation_id.is_none() {
            return Err(AuthError::InvalidRequest);
        }

        if let Some(pg) = &self.pg_store {
            return organizations_pg::accept_invitation_pg(
                pg,
                token_hash.as_deref(),
                invitation_id,
                joining_user_id,
                &caller_email_norm,
                max_paid_seats,
                now_secs,
            )
            .await;
        }

        let mut guard = self.store.write().await;
        let invite_idx = guard
            .invitations
            .iter()
            .position(|i| match (&token_hash, invitation_id) {
                (Some(th), _) => &i.token_hash == th,
                (None, Some(id)) => i.id == id,
                (None, None) => false,
            })
            .ok_or(AuthError::InvalidVerificationToken)?;

        // Mandatory security check BEFORE checking or mutating anything else:
        // caller's verified normalized email MUST match the invited email!
        if guard.invitations[invite_idx].invited_email != caller_email_norm {
            return Err(AuthError::OrgPermissionDenied);
        }
        let inv = &guard.invitations[invite_idx];
        if inv.accepted_at.is_some()
            || inv.revoked_at.is_some()
            || inv.declined_at.is_some()
            || now_secs >= inv.expires_at
        {
            return Err(AuthError::InvalidVerificationToken);
        }

        let org_id = inv.organization_id;
        let role = inv.role;
        let org = guard
            .orgs
            .get(&org_id)
            .filter(|o| o.deleted_at.is_none())
            .cloned()
            .ok_or(AuthError::OrgPermissionDenied)?;

        let seat_cap = match max_paid_seats {
            Some(paid) => org.purchased_seats.min(paid),
            None => org.purchased_seats,
        };
        let already_active = guard.members.iter().any(|m| {
            m.organization_id == org_id && m.user_id == joining_user_id && m.removed_at.is_none()
        });
        if already_active {
            return Err(AuthError::SyncVersionConflict);
        }
        let active_seats = guard
            .members
            .iter()
            .filter(|m| m.organization_id == org_id && m.removed_at.is_none())
            .count() as u32;
        if active_seats >= seat_cap {
            return Err(AuthError::QuotaExceeded);
        }

        guard.invitations[invite_idx].accepted_at = Some(now_secs);
        if let Some(existing) = guard
            .members
            .iter_mut()
            .find(|m| m.organization_id == org_id && m.user_id == joining_user_id)
        {
            existing.role = role;
            existing.email = Some(caller_email_norm);
            existing.display_name = joining_display_name.map(str::to_owned);
            existing.joined_at = now_secs;
            existing.removed_at = None;
            return Ok(existing.clone());
        }

        let member = OrganizationMember {
            organization_id: org_id,
            workspace_id: org.workspace_id,
            user_id: joining_user_id,
            email: Some(caller_email_norm),
            display_name: joining_display_name.map(str::to_owned),
            role,
            joined_at: now_secs,
            removed_at: None,
        };
        guard.members.push(member.clone());
        Ok(member)
    }

    /// Legacy unit-test helper (for tests that construct `OrganizationService::new()` directly
    /// without account-service context). Production HTTP routes ALWAYS call `accept_invitation_verified`.
    pub async fn accept_invitation(
        &self,
        raw_token: &str,
        joining_user_id: Uuid,
    ) -> Result<OrganizationMember, AuthError> {
        let token_hash = blake3::hash(raw_token.trim().as_bytes())
            .to_hex()
            .to_string();
        let email = {
            let guard = self.store.read().await;
            guard
                .invitations
                .iter()
                .find(|i| i.token_hash == token_hash)
                .map(|i| i.invited_email.clone())
                .ok_or(AuthError::InvalidVerificationToken)?
        };
        self.accept_invitation_verified(
            Some(raw_token),
            None,
            joining_user_id,
            &email,
            None,
            None,
            unix_now(),
        )
        .await
    }

    /// Decline a pending invitation addressed to the caller's verified email.
    pub async fn decline_invitation_verified(
        &self,
        raw_token: Option<&str>,
        invitation_id: Option<Uuid>,
        user_id: Uuid,
        verified_email: &str,
        now_secs: i64,
    ) -> Result<OrganizationInvitationView, AuthError> {
        self.ensure_durable_ready()?;
        let caller_email_norm = normalize_email(verified_email)?;
        let token_hash = raw_token
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(|t| blake3::hash(t.as_bytes()).to_hex().to_string());
        if token_hash.is_none() && invitation_id.is_none() {
            return Err(AuthError::InvalidRequest);
        }

        if let Some(pg) = &self.pg_store {
            return organizations_pg::decline_invitation_pg(
                pg,
                token_hash.as_deref(),
                invitation_id,
                user_id,
                &caller_email_norm,
                now_secs,
            )
            .await;
        }

        let mut guard = self.store.write().await;
        let inv = guard
            .invitations
            .iter_mut()
            .find(|i| match (&token_hash, invitation_id) {
                (Some(th), _) => &i.token_hash == th,
                (None, Some(id)) => i.id == id,
                (None, None) => false,
            })
            .ok_or(AuthError::InvalidVerificationToken)?;
        if inv.invited_email != caller_email_norm {
            return Err(AuthError::OrgPermissionDenied);
        }
        if inv.accepted_at.is_some()
            || inv.revoked_at.is_some()
            || inv.declined_at.is_some()
            || now_secs >= inv.expires_at
        {
            return Err(AuthError::InvalidVerificationToken);
        }
        inv.declined_at = Some(now_secs);
        Ok(inv.to_view(now_secs))
    }

    /// Change a member's role enforcing the 4-role RBAC policy.
    pub async fn change_member_role(
        &self,
        organization_id: Uuid,
        actor_user_id: Uuid,
        target_user_id: Uuid,
        new_role: OrgRole,
        now_secs: i64,
    ) -> Result<OrganizationMember, AuthError> {
        self.ensure_durable_ready()?;
        if let Some(pg) = &self.pg_store {
            return organizations_pg::change_member_role_pg(
                pg,
                organization_id,
                actor_user_id,
                target_user_id,
                new_role,
                now_secs,
            )
            .await;
        }

        let mut guard = self.store.write().await;
        if !guard
            .orgs
            .get(&organization_id)
            .is_some_and(|o| o.deleted_at.is_none())
        {
            return Err(AuthError::OrgPermissionDenied);
        }
        let actor_role = guard
            .members
            .iter()
            .find(|m| {
                m.organization_id == organization_id
                    && m.user_id == actor_user_id
                    && m.removed_at.is_none()
            })
            .map(|m| m.role)
            .ok_or(AuthError::OrgPermissionDenied)?;

        let target = guard
            .members
            .iter_mut()
            .find(|m| {
                m.organization_id == organization_id
                    && m.user_id == target_user_id
                    && m.removed_at.is_none()
            })
            .ok_or(AuthError::InvalidRequest)?;

        if !actor_role.can_change_role_of(actor_user_id, target_user_id, target.role, new_role) {
            return Err(AuthError::OrgPermissionDenied);
        }
        target.role = new_role;
        Ok(target.clone())
    }

    /// Remove a member, enforcing 4-role RBAC and revoking all device & wrapped keys for that member.
    pub async fn remove_member(
        &self,
        organization_id: Uuid,
        actor_user_id: Uuid,
        target_user_id: Uuid,
    ) -> Result<(), AuthError> {
        self.ensure_durable_ready()?;
        let now = unix_now();
        if let Some(pg) = &self.pg_store {
            return organizations_pg::remove_member_pg(
                pg,
                organization_id,
                actor_user_id,
                target_user_id,
                now,
            )
            .await;
        }

        let mut guard = self.store.write().await;
        if !guard
            .orgs
            .get(&organization_id)
            .is_some_and(|o| o.deleted_at.is_none())
        {
            return Err(AuthError::OrgPermissionDenied);
        }
        let actor_role = guard
            .members
            .iter()
            .find(|m| {
                m.organization_id == organization_id
                    && m.user_id == actor_user_id
                    && m.removed_at.is_none()
            })
            .map(|m| m.role)
            .ok_or(AuthError::OrgPermissionDenied)?;

        let target_idx = guard
            .members
            .iter()
            .position(|m| {
                m.organization_id == organization_id
                    && m.user_id == target_user_id
                    && m.removed_at.is_none()
            })
            .ok_or(AuthError::InvalidRequest)?;
        let target_role = guard.members[target_idx].role;
        if !actor_role.can_remove_target(actor_user_id, target_user_id, target_role) {
            return Err(AuthError::OrgPermissionDenied);
        }
        guard.members[target_idx].removed_at = Some(now);
        for dk in &mut guard.device_keys {
            if dk.organization_id == organization_id
                && dk.user_id == target_user_id
                && dk.revoked_at.is_none()
            {
                dk.revoked_at = Some(now);
            }
        }
        guard.wrapped_keys.retain(|wk| {
            !(wk.organization_id == organization_id && wk.recipient_user_id == target_user_id)
        });
        Ok(())
    }

    /// Atomically transfer ownership from the current `Owner` to another active member,
    /// demoting the previous `Owner` to `Admin` so the organization never has zero Owners.
    pub async fn transfer_ownership(
        &self,
        organization_id: Uuid,
        current_owner_user_id: Uuid,
        new_owner_user_id: Uuid,
        now_secs: i64,
    ) -> Result<Organization, AuthError> {
        self.ensure_durable_ready()?;
        if current_owner_user_id == new_owner_user_id {
            return Err(AuthError::InvalidRequest);
        }
        if let Some(pg) = &self.pg_store {
            return organizations_pg::transfer_ownership_pg(
                pg,
                organization_id,
                current_owner_user_id,
                new_owner_user_id,
                now_secs,
            )
            .await;
        }

        let mut guard = self.store.write().await;
        let actor_is_owner = guard.members.iter().any(|m| {
            m.organization_id == organization_id
                && m.user_id == current_owner_user_id
                && m.removed_at.is_none()
                && m.role == OrgRole::Owner
        });
        if !actor_is_owner {
            return Err(AuthError::OrgPermissionDenied);
        }
        let target_exists = guard.members.iter().any(|m| {
            m.organization_id == organization_id
                && m.user_id == new_owner_user_id
                && m.removed_at.is_none()
        });
        if !target_exists {
            return Err(AuthError::InvalidRequest);
        }
        let org = guard
            .orgs
            .get_mut(&organization_id)
            .filter(|o| o.deleted_at.is_none())
            .ok_or(AuthError::OrgPermissionDenied)?;
        org.owner_user_id = new_owner_user_id;
        let updated_org = org.clone();

        for m in &mut guard.members {
            if m.organization_id == organization_id && m.removed_at.is_none() {
                if m.user_id == current_owner_user_id {
                    m.role = OrgRole::Admin;
                } else if m.user_id == new_owner_user_id {
                    m.role = OrgRole::Owner;
                }
            }
        }
        Ok(updated_org)
    }

    /// Soft-delete an organization (`Owner` ONLY), revoking all pending invites and key material.
    pub async fn delete_organization(
        &self,
        organization_id: Uuid,
        actor_user_id: Uuid,
        now_secs: i64,
    ) -> Result<(), AuthError> {
        self.ensure_durable_ready()?;
        if let Some(pg) = &self.pg_store {
            return organizations_pg::delete_organization_pg(
                pg,
                organization_id,
                actor_user_id,
                now_secs,
            )
            .await;
        }
        let mut guard = self.store.write().await;
        let actor_is_owner = guard.members.iter().any(|m| {
            m.organization_id == organization_id
                && m.user_id == actor_user_id
                && m.removed_at.is_none()
                && m.role == OrgRole::Owner
        });
        if !actor_is_owner {
            return Err(AuthError::OrgPermissionDenied);
        }
        let org = guard
            .orgs
            .get_mut(&organization_id)
            .filter(|o| o.deleted_at.is_none())
            .ok_or(AuthError::OrgPermissionDenied)?;
        org.deleted_at = Some(now_secs);
        for inv in &mut guard.invitations {
            if inv.organization_id == organization_id
                && inv.accepted_at.is_none()
                && inv.declined_at.is_none()
                && inv.revoked_at.is_none()
            {
                inv.revoked_at = Some(now_secs);
            }
        }
        for dk in &mut guard.device_keys {
            if dk.organization_id == organization_id && dk.revoked_at.is_none() {
                dk.revoked_at = Some(now_secs);
            }
        }
        guard
            .wrapped_keys
            .retain(|wk| wk.organization_id != organization_id);
        Ok(())
    }

    /// List all organizations with active seat counts for the Owner Control Center.
    pub async fn list_all_orgs(&self) -> Vec<(Organization, u32)> {
        if let Some(pg) = &self.pg_store {
            if let Ok(rows) = organizations_pg::list_all_orgs_pg(pg).await {
                return rows;
            }
        }
        let guard = self.store.read().await;
        guard
            .orgs
            .values()
            .filter(|org| org.deleted_at.is_none())
            .map(|org| {
                let active_members = guard
                    .members
                    .iter()
                    .filter(|m| m.organization_id == org.id && m.removed_at.is_none())
                    .count() as u32;
                (org.clone(), active_members)
            })
            .collect()
    }

    /// List active members of an organization.
    pub async fn list_members(&self, organization_id: Uuid) -> Vec<OrganizationMember> {
        if let Some(pg) = &self.pg_store {
            if let Ok(rows) = organizations_pg::list_members_pg(pg, organization_id).await {
                return rows;
            }
        }
        let guard = self.store.read().await;
        if !guard
            .orgs
            .get(&organization_id)
            .is_some_and(|o| o.deleted_at.is_none())
        {
            return Vec::new();
        }
        guard
            .members
            .iter()
            .filter(|m| m.organization_id == organization_id && m.removed_at.is_none())
            .cloned()
            .collect()
    }

    /// Register or refresh an X25519 device public key for an active organization member.
    pub async fn register_device_public_key(
        &self,
        organization_id: Uuid,
        user_id: Uuid,
        device_id: &str,
        device_label: &str,
        x25519_public_key_b64: &str,
        now_secs: i64,
    ) -> Result<MemberDevicePublicKey, AuthError> {
        self.ensure_durable_ready()?;
        let dev_id = device_id.trim();
        let dev_label = if device_label.trim().is_empty() {
            "Desktop"
        } else {
            device_label.trim()
        };
        if dev_id.is_empty() || dev_id.len() > 128 || dev_label.len() > 120 {
            return Err(AuthError::InvalidRequest);
        }
        let (pub_bytes, fingerprint) = validate_x25519_public_key_b64(x25519_public_key_b64)?;

        if let Some(pg) = &self.pg_store {
            return organizations_pg::register_device_public_key_pg(
                pg,
                organization_id,
                user_id,
                dev_id,
                dev_label,
                &pub_bytes,
                &fingerprint,
                now_secs,
            )
            .await;
        }

        let mut guard = self.store.write().await;
        let org = guard
            .orgs
            .get(&organization_id)
            .filter(|o| o.deleted_at.is_none())
            .cloned()
            .ok_or(AuthError::OrgPermissionDenied)?;
        let is_member = guard.members.iter().any(|m| {
            m.organization_id == organization_id && m.user_id == user_id && m.removed_at.is_none()
        });
        if !is_member {
            return Err(AuthError::OrgPermissionDenied);
        }
        let canonical_b64 = base64::engine::general_purpose::STANDARD.encode(&pub_bytes);
        if let Some(existing) = guard.device_keys.iter_mut().find(|d| {
            d.organization_id == organization_id && d.user_id == user_id && d.device_id == dev_id
        }) {
            existing.device_label = dev_label.to_owned();
            existing.x25519_public_key_b64 = canonical_b64;
            existing.key_fingerprint = fingerprint;
            existing.created_at = now_secs;
            existing.revoked_at = None;
            return Ok(existing.clone());
        }
        let record = MemberDevicePublicKey {
            id: Uuid::now_v7(),
            organization_id,
            workspace_id: org.workspace_id,
            user_id,
            device_id: dev_id.to_owned(),
            device_label: dev_label.to_owned(),
            x25519_public_key_b64: canonical_b64,
            key_fingerprint: fingerprint,
            created_at: now_secs,
            revoked_at: None,
        };
        guard.device_keys.push(record.clone());
        Ok(record)
    }

    /// Revoke a registered device key (lost device self-revocation or Owner/Admin management).
    pub async fn revoke_device_public_key(
        &self,
        organization_id: Uuid,
        actor_user_id: Uuid,
        target_user_id: Uuid,
        device_id: &str,
        now_secs: i64,
    ) -> Result<(), AuthError> {
        self.ensure_durable_ready()?;
        let dev_id = device_id.trim();
        if dev_id.is_empty() {
            return Err(AuthError::InvalidRequest);
        }
        if let Some(pg) = &self.pg_store {
            return organizations_pg::revoke_device_public_key_pg(
                pg,
                organization_id,
                actor_user_id,
                target_user_id,
                dev_id,
                now_secs,
            )
            .await;
        }
        let mut guard = self.store.write().await;
        let actor_role = guard
            .members
            .iter()
            .find(|m| {
                m.organization_id == organization_id
                    && m.user_id == actor_user_id
                    && m.removed_at.is_none()
            })
            .map(|m| m.role)
            .ok_or(AuthError::OrgPermissionDenied)?;
        if actor_user_id != target_user_id {
            let target_role = guard
                .members
                .iter()
                .find(|m| {
                    m.organization_id == organization_id
                        && m.user_id == target_user_id
                        && m.removed_at.is_none()
                })
                .map(|m| m.role)
                .ok_or(AuthError::InvalidRequest)?;
            if !actor_role.can_remove_target(actor_user_id, target_user_id, target_role) {
                return Err(AuthError::OrgPermissionDenied);
            }
        }
        let mut found = false;
        for dk in &mut guard.device_keys {
            if dk.organization_id == organization_id
                && dk.user_id == target_user_id
                && dk.device_id == dev_id
                && dk.revoked_at.is_none()
            {
                dk.revoked_at = Some(now_secs);
                found = true;
            }
        }
        if !found {
            return Err(AuthError::InvalidRequest);
        }
        guard.wrapped_keys.retain(|wk| {
            !(wk.organization_id == organization_id
                && wk.recipient_user_id == target_user_id
                && wk.recipient_device_id == dev_id)
        });
        Ok(())
    }

    /// List active X25519 device public keys for all active members of an organization.
    pub async fn list_active_member_device_keys(
        &self,
        organization_id: Uuid,
        actor_user_id: Uuid,
    ) -> Result<Vec<MemberDevicePublicKey>, AuthError> {
        self.ensure_durable_ready()?;
        if let Some(pg) = &self.pg_store {
            return organizations_pg::list_active_member_device_keys_pg(
                pg,
                organization_id,
                actor_user_id,
            )
            .await;
        }
        let guard = self.store.read().await;
        if !guard
            .orgs
            .get(&organization_id)
            .is_some_and(|o| o.deleted_at.is_none())
        {
            return Err(AuthError::OrgPermissionDenied);
        }
        let active_user_ids: Vec<Uuid> = guard
            .members
            .iter()
            .filter(|m| m.organization_id == organization_id && m.removed_at.is_none())
            .map(|m| m.user_id)
            .collect();
        if !active_user_ids.contains(&actor_user_id) {
            return Err(AuthError::OrgPermissionDenied);
        }
        Ok(guard
            .device_keys
            .iter()
            .filter(|dk| {
                dk.organization_id == organization_id
                    && dk.revoked_at.is_none()
                    && active_user_ids.contains(&dk.user_id)
            })
            .cloned()
            .collect())
    }

    /// Publish wrapped workspace keys for authorized active member devices (`Owner`/`Admin`/`Member`).
    pub async fn publish_wrapped_workspace_keys(
        &self,
        organization_id: Uuid,
        actor_user_id: Uuid,
        key_epoch: u32,
        entries: &[WrappedKeyEntryInput],
        now_secs: i64,
    ) -> Result<usize, AuthError> {
        self.ensure_durable_ready()?;
        if key_epoch == 0 || entries.is_empty() || entries.len() > 500 {
            return Err(AuthError::InvalidRequest);
        }
        let validated: Vec<ValidatedWrappedKeyBytes> = entries
            .iter()
            .map(validate_wrapped_key_entry)
            .collect::<Result<Vec<_>, _>>()?;

        if let Some(pg) = &self.pg_store {
            return organizations_pg::publish_wrapped_workspace_keys_pg(
                pg,
                organization_id,
                actor_user_id,
                key_epoch,
                &validated,
                now_secs,
            )
            .await;
        }

        let mut guard = self.store.write().await;
        let org = guard
            .orgs
            .get(&organization_id)
            .filter(|o| o.deleted_at.is_none())
            .cloned()
            .ok_or(AuthError::OrgPermissionDenied)?;
        let actor_role = guard
            .members
            .iter()
            .find(|m| {
                m.organization_id == organization_id
                    && m.user_id == actor_user_id
                    && m.removed_at.is_none()
            })
            .map(|m| m.role)
            .ok_or(AuthError::OrgPermissionDenied)?;
        if !actor_role.can_modify_shared_resources() {
            return Err(AuthError::OrgPermissionDenied);
        }
        if key_epoch > org.active_key_epoch {
            return Err(AuthError::SyncVersionConflict);
        }

        for v in &validated {
            let recipient_active = guard.members.iter().any(|m| {
                m.organization_id == organization_id
                    && m.user_id == v.recipient_user_id
                    && m.removed_at.is_none()
            });
            let device_active = guard.device_keys.iter().any(|dk| {
                dk.organization_id == organization_id
                    && dk.user_id == v.recipient_user_id
                    && dk.device_id == v.recipient_device_id
                    && dk.revoked_at.is_none()
            });
            if !recipient_active || !device_active {
                return Err(AuthError::OrgPermissionDenied);
            }
        }

        let b64 = base64::engine::general_purpose::STANDARD;
        for v in &validated {
            guard.wrapped_keys.retain(|wk| {
                !(wk.organization_id == organization_id
                    && wk.key_epoch == key_epoch
                    && wk.recipient_user_id == v.recipient_user_id
                    && wk.recipient_device_id == v.recipient_device_id)
            });
            guard.wrapped_keys.push(WrappedWorkspaceKeyRecord {
                id: Uuid::now_v7(),
                organization_id,
                workspace_id: org.workspace_id,
                key_epoch,
                recipient_user_id: v.recipient_user_id,
                recipient_device_id: v.recipient_device_id.clone(),
                sender_user_id: actor_user_id,
                sender_ephemeral_pub_b64: b64.encode(&v.sender_ephemeral_pub),
                wrapped_key_ciphertext_b64: b64.encode(&v.wrapped_key_ciphertext),
                wrapped_key_nonce_b64: b64.encode(&v.wrapped_key_nonce),
                created_at: now_secs,
            });
        }
        Ok(validated.len())
    }

    /// Rotate the workspace key to `expected_current_epoch + 1` (`Owner` or `Admin` ONLY),
    /// distributing wrapped keys strictly to active remaining member devices.
    pub async fn rotate_workspace_key(
        &self,
        organization_id: Uuid,
        actor_user_id: Uuid,
        expected_current_epoch: u32,
        entries: &[WrappedKeyEntryInput],
        now_secs: i64,
    ) -> Result<u32, AuthError> {
        self.ensure_durable_ready()?;
        if expected_current_epoch == 0 || entries.is_empty() || entries.len() > 500 {
            return Err(AuthError::InvalidRequest);
        }
        let validated: Vec<ValidatedWrappedKeyBytes> = entries
            .iter()
            .map(validate_wrapped_key_entry)
            .collect::<Result<Vec<_>, _>>()?;

        if let Some(pg) = &self.pg_store {
            return organizations_pg::rotate_workspace_key_pg(
                pg,
                organization_id,
                actor_user_id,
                expected_current_epoch,
                &validated,
                now_secs,
            )
            .await;
        }

        let mut guard = self.store.write().await;
        let actor_role = guard
            .members
            .iter()
            .find(|m| {
                m.organization_id == organization_id
                    && m.user_id == actor_user_id
                    && m.removed_at.is_none()
            })
            .map(|m| m.role)
            .ok_or(AuthError::OrgPermissionDenied)?;
        if !actor_role.can_manage_members() {
            return Err(AuthError::OrgPermissionDenied);
        }
        let current_epoch = guard
            .orgs
            .get(&organization_id)
            .filter(|o| o.deleted_at.is_none())
            .map(|o| (o.active_key_epoch, o.workspace_id))
            .ok_or(AuthError::OrgPermissionDenied)?;
        if current_epoch.0 != expected_current_epoch {
            return Err(AuthError::SyncVersionConflict);
        }
        for v in &validated {
            let recipient_active = guard.members.iter().any(|m| {
                m.organization_id == organization_id
                    && m.user_id == v.recipient_user_id
                    && m.removed_at.is_none()
            });
            let device_active = guard.device_keys.iter().any(|dk| {
                dk.organization_id == organization_id
                    && dk.user_id == v.recipient_user_id
                    && dk.device_id == v.recipient_device_id
                    && dk.revoked_at.is_none()
            });
            if !recipient_active || !device_active {
                return Err(AuthError::OrgPermissionDenied);
            }
        }
        let new_epoch = expected_current_epoch + 1;
        if let Some(org) = guard.orgs.get_mut(&organization_id) {
            org.active_key_epoch = new_epoch;
        }
        let b64 = base64::engine::general_purpose::STANDARD;
        for v in &validated {
            guard.wrapped_keys.push(WrappedWorkspaceKeyRecord {
                id: Uuid::now_v7(),
                organization_id,
                workspace_id: current_epoch.1,
                key_epoch: new_epoch,
                recipient_user_id: v.recipient_user_id,
                recipient_device_id: v.recipient_device_id.clone(),
                sender_user_id: actor_user_id,
                sender_ephemeral_pub_b64: b64.encode(&v.sender_ephemeral_pub),
                wrapped_key_ciphertext_b64: b64.encode(&v.wrapped_key_ciphertext),
                wrapped_key_nonce_b64: b64.encode(&v.wrapped_key_nonce),
                created_at: now_secs,
            });
        }
        Ok(new_epoch)
    }

    /// Fetch caller's active wrapped workspace keys across epochs (`Owner`/`Admin`/`Member`/`Viewer`).
    pub async fn list_my_wrapped_keys(
        &self,
        organization_id: Uuid,
        user_id: Uuid,
        device_id: Option<&str>,
    ) -> Result<Vec<WrappedWorkspaceKeyRecord>, AuthError> {
        self.ensure_durable_ready()?;
        if let Some(pg) = &self.pg_store {
            return organizations_pg::list_my_wrapped_keys_pg(
                pg,
                organization_id,
                user_id,
                device_id,
            )
            .await;
        }
        let guard = self.store.read().await;
        if !guard
            .orgs
            .get(&organization_id)
            .is_some_and(|o| o.deleted_at.is_none())
        {
            return Err(AuthError::OrgPermissionDenied);
        }
        let is_member = guard.members.iter().any(|m| {
            m.organization_id == organization_id && m.user_id == user_id && m.removed_at.is_none()
        });
        if !is_member {
            return Err(AuthError::OrgPermissionDenied);
        }
        let dev_filter = device_id.map(str::trim).filter(|s| !s.is_empty());
        let mut out: Vec<WrappedWorkspaceKeyRecord> = guard
            .wrapped_keys
            .iter()
            .filter(|wk| {
                wk.organization_id == organization_id
                    && wk.recipient_user_id == user_id
                    && dev_filter.is_none_or(|d| wk.recipient_device_id == d)
            })
            .cloned()
            .collect();
        out.sort_by_key(|wk| wk.key_epoch);
        Ok(out)
    }

    /// Enterprise SSO/SCIM adapter readiness: explicitly returns `SF-SYN-006` (`NOT_CONFIGURED`).
    pub fn check_sso_scim_readiness(&self) -> Result<(), AuthError> {
        Err(AuthError::OrgSsoNotConfigured)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn organization_enforces_min_three_seats_roles_and_sso_status() {
        let svc = OrganizationService::new();
        let ws = Uuid::now_v7();
        let owner = Uuid::now_v7();
        let member = Uuid::now_v7();

        assert_eq!(
            svc.create_org(ws, owner, "Small Team", 2)
                .await
                .unwrap_err(),
            AuthError::OrgMinSeatsRequired
        );

        let org = svc
            .create_org(ws, owner, "Safi Analytics Org", 3)
            .await
            .expect("create org");
        assert_ne!(org.workspace_id, ws);
        let (_invite, token) = svc
            .invite_member(org.id, owner, "analyst@company.example", OrgRole::Member)
            .await
            .expect("invite");

        // Wrong verified email MUST be rejected even with valid token!
        assert_eq!(
            svc.accept_invitation_verified(
                Some(&token),
                None,
                member,
                "intruder@company.example",
                None,
                None,
                unix_now(),
            )
            .await
            .unwrap_err(),
            AuthError::OrgPermissionDenied
        );

        // Matching verified email succeeds once and cannot be reused.
        svc.accept_invitation_verified(
            Some(&token),
            None,
            member,
            "ANALYST@company.example",
            Some("Analyst"),
            None,
            unix_now(),
        )
        .await
        .expect("accept");
        assert_eq!(
            svc.accept_invitation_verified(
                Some(&token),
                None,
                Uuid::now_v7(),
                "analyst@company.example",
                None,
                None,
                unix_now(),
            )
            .await
            .unwrap_err(),
            AuthError::InvalidVerificationToken
        );
        assert_eq!(svc.list_members(org.id).await.len(), 2);

        // Regular member cannot invite others or promote themselves.
        assert_eq!(
            svc.invite_member(org.id, member, "other@company.example", OrgRole::Viewer)
                .await
                .unwrap_err(),
            AuthError::OrgPermissionDenied
        );
        assert_eq!(
            svc.change_member_role(org.id, member, member, OrgRole::Admin, unix_now())
                .await
                .unwrap_err(),
            AuthError::OrgPermissionDenied
        );

        // Cannot remove the last Owner.
        assert_eq!(
            svc.remove_member(org.id, owner, owner).await.unwrap_err(),
            AuthError::OrgPermissionDenied
        );

        svc.remove_member(org.id, owner, member)
            .await
            .expect("remove");
        assert_eq!(svc.list_members(org.id).await.len(), 1);
        assert_eq!(
            svc.check_sso_scim_readiness().unwrap_err(),
            AuthError::OrgSsoNotConfigured
        );
    }
}
