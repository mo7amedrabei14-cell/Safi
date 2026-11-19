//! Organizations & Teams management (Phase 11):
//! - Minimum 3 seats enforced on creation and seat adjustment (`SF-SYN-004`).
//! - Role-based access control (`owner`, `admin`, `member`, `viewer`).
//! - Invitations, acceptance, member removal, and SSO/SCIM readiness (`SF-SYN-006`).

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::auth::unix_now;
use crate::entitlements::ORG_MIN_SEATS;
use crate::error::AuthError;

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
    pub const fn can_manage_members(self) -> bool {
        matches!(self, Self::Owner | Self::Admin)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Organization {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub purchased_seats: u32,
    pub owner_user_id: Uuid,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationMember {
    pub organization_id: Uuid,
    pub user_id: Uuid,
    pub role: OrgRole,
    pub joined_at: i64,
    pub removed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationInvitation {
    pub id: Uuid,
    pub organization_id: Uuid,
    pub invited_email: String,
    pub role: OrgRole,
    pub token_hash: String,
    pub invited_by: Uuid,
    pub expires_at: i64,
    pub accepted_at: Option<i64>,
    pub revoked_at: Option<i64>,
}

#[derive(Debug, Default)]
struct OrgStore {
    orgs: HashMap<Uuid, Organization>,
    members: Vec<OrganizationMember>,
    invitations: Vec<OrganizationInvitation>,
}

/// Thread-safe organization management service.
#[derive(Debug, Clone, Default)]
pub struct OrganizationService {
    store: Arc<RwLock<OrgStore>>,
}

impl OrganizationService {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an organization enforcing minimum 3 seats (`SF-SYN-004`).
    pub async fn create_org(
        &self,
        workspace_id: Uuid,
        owner_user_id: Uuid,
        name: &str,
        seats: u32,
    ) -> Result<Organization, AuthError> {
        let trimmed = name.trim();
        if trimmed.is_empty() || trimmed.len() > 120 {
            return Err(AuthError::InvalidRequest);
        }
        if seats < ORG_MIN_SEATS {
            return Err(AuthError::OrgMinSeatsRequired);
        }
        let now = unix_now();
        let org = Organization {
            id: Uuid::now_v7(),
            workspace_id,
            name: trimmed.to_owned(),
            purchased_seats: seats,
            owner_user_id,
            created_at: now,
        };
        let mut guard = self.store.write().await;
        guard.orgs.insert(org.id, org.clone());
        guard.members.push(OrganizationMember {
            organization_id: org.id,
            user_id: owner_user_id,
            role: OrgRole::Owner,
            joined_at: now,
            removed_at: None,
        });
        Ok(org)
    }

    /// Invite a member (requires Owner or Admin role and available seat capacity).
    pub async fn invite_member(
        &self,
        organization_id: Uuid,
        actor_user_id: Uuid,
        invited_email: &str,
        role: OrgRole,
    ) -> Result<(OrganizationInvitation, String), AuthError> {
        if role == OrgRole::Owner || !invited_email.contains('@') {
            return Err(AuthError::InvalidRequest);
        }
        let mut guard = self.store.write().await;
        let org = guard
            .orgs
            .get(&organization_id)
            .cloned()
            .ok_or(AuthError::InvalidRequest)?;

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

        let active_seats = guard
            .members
            .iter()
            .filter(|m| m.organization_id == organization_id && m.removed_at.is_none())
            .count() as u32;
        if active_seats >= org.purchased_seats {
            return Err(AuthError::QuotaExceeded);
        }

        let raw_token = format!("sfi_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let token_hash = blake3::hash(raw_token.as_bytes()).to_hex().to_string();
        let now = unix_now();
        let invite = OrganizationInvitation {
            id: Uuid::now_v7(),
            organization_id,
            invited_email: invited_email.trim().to_ascii_lowercase(),
            role,
            token_hash,
            invited_by: actor_user_id,
            expires_at: now + 7 * 24 * 60 * 60,
            accepted_at: None,
            revoked_at: None,
        };
        guard.invitations.push(invite.clone());
        Ok((invite, raw_token))
    }

    /// Accept a valid non-expired invitation token and join the organization.
    pub async fn accept_invitation(
        &self,
        raw_token: &str,
        joining_user_id: Uuid,
    ) -> Result<OrganizationMember, AuthError> {
        let token_hash = blake3::hash(raw_token.trim().as_bytes())
            .to_hex()
            .to_string();
        let now = unix_now();
        let mut guard = self.store.write().await;
        let invite_idx = guard
            .invitations
            .iter()
            .position(|i| {
                i.token_hash == token_hash
                    && i.accepted_at.is_none()
                    && i.revoked_at.is_none()
                    && now < i.expires_at
            })
            .ok_or(AuthError::InvalidVerificationToken)?;

        let org_id = guard.invitations[invite_idx].organization_id;
        let role = guard.invitations[invite_idx].role;
        guard.invitations[invite_idx].accepted_at = Some(now);

        let member = OrganizationMember {
            organization_id: org_id,
            user_id: joining_user_id,
            role,
            joined_at: now,
            removed_at: None,
        };
        guard.members.push(member.clone());
        Ok(member)
    }

    /// Remove a member (requires Owner or Admin; cannot remove the org Owner).
    pub async fn remove_member(
        &self,
        organization_id: Uuid,
        actor_user_id: Uuid,
        target_user_id: Uuid,
    ) -> Result<(), AuthError> {
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

        let target = guard
            .members
            .iter_mut()
            .find(|m| {
                m.organization_id == organization_id
                    && m.user_id == target_user_id
                    && m.removed_at.is_none()
            })
            .ok_or(AuthError::InvalidRequest)?;
        if target.role == OrgRole::Owner {
            return Err(AuthError::OrgPermissionDenied);
        }
        target.removed_at = Some(unix_now());
        Ok(())
    }

    /// List active members of an organization.
    pub async fn list_members(&self, organization_id: Uuid) -> Vec<OrganizationMember> {
        self.store
            .read()
            .await
            .members
            .iter()
            .filter(|m| m.organization_id == organization_id && m.removed_at.is_none())
            .cloned()
            .collect()
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
        let (_invite, token) = svc
            .invite_member(org.id, owner, "analyst@company.example", OrgRole::Member)
            .await
            .expect("invite");

        svc.accept_invitation(&token, member).await.expect("accept");
        assert_eq!(svc.list_members(org.id).await.len(), 2);

        // Regular member cannot invite others.
        assert_eq!(
            svc.invite_member(org.id, member, "other@company.example", OrgRole::Viewer)
                .await
                .unwrap_err(),
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
