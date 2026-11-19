//! Strict immutable Platform Owner identity (by internal `user_id` UUID),
//! Friend VIP entitlements, and duplicate-account rules.
//!
//! Security invariant (Phase 04):
//! - Owner identity is NEVER hardcoded as an email address and NEVER checked by email.
//! - Authorization compares the authenticated `actor_user_id: Uuid` against the sealed
//!   `owner_user_id: Uuid` stored in `platform_owner`.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AuthError;

/// Minimum password length in Unicode scalar values (not bytes).
pub const MIN_PASSWORD_LENGTH: usize = 8;

/// Arabic rejection message required by the product constitution.
pub const PASSWORD_TOO_SHORT_MESSAGE_AR: &str = "كلمة المرور يجب ألا تقل عن 8 خانات";

/// Arabic rejection message for a duplicate account.
pub const DUPLICATE_ACCOUNT_MESSAGE_AR: &str = "هذا الحساب مسجل بالفعل، يرجى تسجيل الدخول";

/// Normalize an email exactly as duplicate-account checks expect.
#[must_use]
pub fn normalize_identity_email(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

/// Strict immutable single-owner identity check by internal `user_id` UUID.
/// Rejects nil UUIDs and never inspects email addresses.
#[must_use]
pub fn is_owner_id(actor_user_id: Uuid, sealed_owner_user_id: Uuid) -> bool {
    !actor_user_id.is_nil()
        && !sealed_owner_user_id.is_nil()
        && actor_user_id == sealed_owner_user_id
}

/// Owner-only actions from the business constitution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OwnerOnlyAction {
    /// Open the executive owner dashboard.
    AccessOwnerDashboard,
    /// Grant or revoke the Friend VIP role.
    GrantOrRevokeFriendRole,
    /// Cancel a subscription.
    CancelSubscription,
    /// Issue a refund.
    IssueRefund,
    /// Extend a trial.
    ExtendTrial,
    /// Suspend a user.
    SuspendUser,
    /// Toggle emergency maintenance mode.
    ToggleMaintenanceMode,
}

impl OwnerOnlyAction {
    /// Canonical action name used by the owner console.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AccessOwnerDashboard => "access_owner_dashboard",
            Self::GrantOrRevokeFriendRole => "grant_or_revoke_friend_role",
            Self::CancelSubscription => "cancel_subscription",
            Self::IssueRefund => "issue_refund",
            Self::ExtendTrial => "extend_trial",
            Self::SuspendUser => "suspend_user",
            Self::ToggleMaintenanceMode => "toggle_maintenance_mode",
        }
    }
}

/// Capability token proving the sealed platform owner authorized an action.
/// The private field makes it impossible to construct outside this module.
#[derive(Debug, Clone, Copy)]
pub struct OwnerGrant {
    _private: (),
}

/// Reject every non-owner before an owner-only action executes, and mint the
/// capability required by owner-scoped repositories.
pub fn authorize_owner_grant(
    actor_user_id: Uuid,
    sealed_owner_user_id: Uuid,
    action: OwnerOnlyAction,
) -> Result<OwnerGrant, AuthError> {
    authorize_owner(actor_user_id, sealed_owner_user_id, action)?;
    Ok(OwnerGrant { _private: () })
}

/// Reject every non-owner before an owner-only action executes (`SF-AUTH-013`).
pub fn authorize_owner(
    actor_user_id: Uuid,
    sealed_owner_user_id: Uuid,
    action: OwnerOnlyAction,
) -> Result<(), AuthError> {
    if is_owner_id(actor_user_id, sealed_owner_user_id) {
        Ok(())
    } else {
        tracing::warn!(
            action = action.as_str(),
            "owner-only action rejected for a non-owner account"
        );
        Err(AuthError::OwnerRequired)
    }
}

/// Full paid-plan entitlements granted by the Friend VIP role at zero cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FriendEntitlements {
    pub all_paid_features_open: bool,
    pub removes_watermark: bool,
    pub saved_recipes: bool,
    pub includes_premium_recipe_packs: bool,
    pub ai_calls_per_month: u32,
    pub price_usd: u32,
    pub price_egp: u32,
}

/// The Friend role is the paid plan for free: 400 AI calls, no watermark.
pub const FRIEND_ENTITLEMENTS: FriendEntitlements = FriendEntitlements {
    all_paid_features_open: true,
    removes_watermark: true,
    saved_recipes: true,
    includes_premium_recipe_packs: true,
    ai_calls_per_month: 400,
    price_usd: 0,
    price_egp: 0,
};

/// An active Friend VIP grant owned by the platform owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FriendGrant {
    pub target_user_id: Uuid,
    pub note: Option<String>,
    pub granted_by_user_id: Uuid,
    pub granted_at_unix: i64,
    pub entitlements: FriendEntitlements,
}

/// Grant the Friend role. Only the sealed owner may call this, and the owner
/// cannot grant the role to itself.
pub fn grant_friend_role(
    actor_user_id: Uuid,
    sealed_owner_user_id: Uuid,
    target_user_id: Uuid,
    note: Option<&str>,
    granted_at_unix: i64,
) -> Result<FriendGrant, AuthError> {
    authorize_owner(
        actor_user_id,
        sealed_owner_user_id,
        OwnerOnlyAction::GrantOrRevokeFriendRole,
    )?;
    if target_user_id.is_nil() || target_user_id == sealed_owner_user_id {
        return Err(AuthError::InvalidRequest);
    }
    let note = note
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    Ok(FriendGrant {
        target_user_id,
        note,
        granted_by_user_id: sealed_owner_user_id,
        granted_at_unix,
        entitlements: FRIEND_ENTITLEMENTS,
    })
}

/// Revoke the Friend role. Only the sealed owner may call this.
pub fn revoke_friend_role(
    actor_user_id: Uuid,
    sealed_owner_user_id: Uuid,
    target_user_id: Uuid,
) -> Result<Uuid, AuthError> {
    authorize_owner(
        actor_user_id,
        sealed_owner_user_id,
        OwnerOnlyAction::GrantOrRevokeFriendRole,
    )?;
    if target_user_id.is_nil() {
        return Err(AuthError::InvalidRequest);
    }
    Ok(target_user_id)
}

/// A stored account identity used for duplicate detection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExistingAccountIdentity {
    pub normalized_email: String,
    pub verified_phone: Option<String>,
}

/// Which unique account key collided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DuplicateAccountKey {
    /// The normalized email already belongs to an account.
    Email,
    /// The verified phone already belongs to an account.
    Phone,
}

/// Compare a candidate sign-up with existing accounts using the two constitution
/// unique keys: normalized email and verified phone.
#[must_use]
pub fn detect_duplicate_account(
    existing: &[ExistingAccountIdentity],
    candidate_email: &str,
    candidate_phone: Option<&str>,
) -> Option<DuplicateAccountKey> {
    let email = normalize_identity_email(candidate_email);
    let phone = candidate_phone
        .map(normalize_phone)
        .filter(|v| !v.is_empty());

    if existing
        .iter()
        .any(|account| normalize_identity_email(&account.normalized_email) == email)
    {
        return Some(DuplicateAccountKey::Email);
    }
    if let Some(phone) = phone {
        if existing.iter().any(|account| {
            account
                .verified_phone
                .as_deref()
                .map(normalize_phone)
                .is_some_and(|stored| !stored.is_empty() && stored == phone)
        }) {
            return Some(DuplicateAccountKey::Phone);
        }
    }
    None
}

/// Phone comparison ignores formatting characters and the Egyptian country or
/// trunk prefix, so `+20 100 123 4567` and `01001234567` collide correctly.
#[must_use]
pub fn normalize_phone(raw: &str) -> String {
    let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    if digits.len() == 12 && digits.starts_with("20") {
        return digits[2..].to_owned();
    }
    if digits.len() == 11 && digits.starts_with('0') {
        return digits[1..].to_owned();
    }
    digits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_identity_uses_immutable_uuid_and_rejects_nil_and_other_ids() {
        let owner_id = Uuid::now_v7();
        let other_id = Uuid::now_v7();
        assert!(is_owner_id(owner_id, owner_id));
        assert!(!is_owner_id(other_id, owner_id));
        assert!(!is_owner_id(Uuid::nil(), Uuid::nil()));
        assert!(!is_owner_id(Uuid::nil(), owner_id));
    }

    #[test]
    fn every_owner_action_rejects_a_non_owner() {
        let owner_id = Uuid::now_v7();
        let regular_id = Uuid::now_v7();
        for action in [
            OwnerOnlyAction::AccessOwnerDashboard,
            OwnerOnlyAction::GrantOrRevokeFriendRole,
            OwnerOnlyAction::CancelSubscription,
            OwnerOnlyAction::IssueRefund,
            OwnerOnlyAction::ExtendTrial,
            OwnerOnlyAction::SuspendUser,
            OwnerOnlyAction::ToggleMaintenanceMode,
        ] {
            assert_eq!(
                authorize_owner(regular_id, owner_id, action).unwrap_err(),
                AuthError::OwnerRequired
            );
            assert!(authorize_owner(owner_id, owner_id, action).is_ok());
        }
    }

    #[test]
    fn owner_grant_is_minted_only_for_the_sealed_owner_id() {
        let owner_id = Uuid::now_v7();
        let regular_id = Uuid::now_v7();
        assert!(authorize_owner_grant(owner_id, owner_id, OwnerOnlyAction::SuspendUser).is_ok());
        assert_eq!(
            authorize_owner_grant(regular_id, owner_id, OwnerOnlyAction::SuspendUser).unwrap_err(),
            AuthError::OwnerRequired
        );
    }

    #[test]
    fn friend_role_grants_full_paid_plan_by_uuid_only() {
        let owner_id = Uuid::now_v7();
        let friend_id = Uuid::now_v7();
        let grant = grant_friend_role(
            owner_id,
            owner_id,
            friend_id,
            Some("  VIP  "),
            1_780_000_000,
        )
        .expect("grant");

        assert_eq!(grant.target_user_id, friend_id);
        assert_eq!(grant.note.as_deref(), Some("VIP"));
        assert_eq!(grant.granted_by_user_id, owner_id);
        assert_eq!(grant.entitlements, FRIEND_ENTITLEMENTS);
        assert_eq!(grant.entitlements.ai_calls_per_month, 400);
        assert_eq!(grant.entitlements.price_usd, 0);
        assert_eq!(grant.entitlements.price_egp, 0);

        // Non-owner cannot grant or revoke; owner cannot self-grant.
        assert_eq!(
            grant_friend_role(friend_id, owner_id, Uuid::now_v7(), None, 0).unwrap_err(),
            AuthError::OwnerRequired
        );
        assert_eq!(
            grant_friend_role(owner_id, owner_id, owner_id, None, 0).unwrap_err(),
            AuthError::InvalidRequest
        );
        assert_eq!(
            revoke_friend_role(owner_id, owner_id, friend_id).expect("revoke"),
            friend_id
        );
    }

    #[test]
    fn duplicate_detection_uses_normalized_email_and_verified_phone() {
        let existing = vec![
            ExistingAccountIdentity {
                normalized_email: "user@example.test".to_owned(),
                verified_phone: Some("+20 100 123 4567".to_owned()),
            },
            ExistingAccountIdentity {
                normalized_email: "phone-less@example.test".to_owned(),
                verified_phone: None,
            },
        ];

        assert_eq!(
            detect_duplicate_account(&existing, " USER@Example.Test ", None),
            Some(DuplicateAccountKey::Email)
        );
        assert_eq!(
            detect_duplicate_account(&existing, "new@example.test", Some("201001234567")),
            Some(DuplicateAccountKey::Phone)
        );
        assert_eq!(
            detect_duplicate_account(&existing, "new@example.test", Some("201009999999")),
            None
        );
    }
}
