//! Strict owner identity, Friend VIP entitlements, and duplicate-account rules.
//!
//! `config/business.json` is the launch constitution. The values below mirror
//! it and are covered by tests so the server can never drift from the owner's
//! locked decisions.

use serde::{Deserialize, Serialize};

use crate::error::AuthError;

/// The one and only owner account, already normalized.
pub const OWNER_EMAIL: &str = "mo7amed.rabei14@gmail.com";

/// Minimum password length in Unicode scalar values (not bytes).
pub const MIN_PASSWORD_LENGTH: usize = 8;

/// Arabic rejection message required by the product constitution.
pub const PASSWORD_TOO_SHORT_MESSAGE_AR: &str = "كلمة المرور يجب ألا تقل عن 8 خانات";

/// Arabic rejection message for a duplicate account.
pub const DUPLICATE_ACCOUNT_MESSAGE_AR: &str = "هذا الحساب مسجل بالفعل، يرجى تسجيل الدخول";

/// Normalize an email exactly as the identity checks expect.
#[must_use]
pub fn normalize_identity_email(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

/// Strict single-owner identity check.
#[must_use]
pub fn is_owner(email: &str) -> bool {
    normalize_identity_email(email) == OWNER_EMAIL
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

/// Capability token proving the owner authorized an action. The private field
/// makes it impossible to construct outside this module, so cross-workspace
/// database access cannot be requested without passing the strict owner check.
#[derive(Debug, Clone, Copy)]
pub struct OwnerGrant {
    _private: (),
}

/// Reject every non-owner before an owner-only action executes, and mint the
/// capability required by owner-scoped repositories.
pub fn authorize_owner_grant(
    actor_email: &str,
    action: OwnerOnlyAction,
) -> Result<OwnerGrant, AuthError> {
    authorize_owner(actor_email, action)?;
    Ok(OwnerGrant { _private: () })
}

/// Reject every non-owner before an owner-only action executes.
pub fn authorize_owner(actor_email: &str, action: OwnerOnlyAction) -> Result<(), AuthError> {
    if is_owner(actor_email) {
        Ok(())
    } else {
        tracing::warn!(
            action = action.as_str(),
            "owner-only action rejected for a non-owner account"
        );
        Err(AuthError::InsufficientScope)
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
    pub email: String,
    pub note: Option<String>,
    pub granted_by: String,
    pub granted_at_unix: i64,
    pub entitlements: FriendEntitlements,
}

/// Grant the Friend role. Only the owner may call this, and the owner cannot
/// grant the role to itself.
pub fn grant_friend_role(
    actor_email: &str,
    friend_email: &str,
    note: Option<&str>,
    granted_at_unix: i64,
) -> Result<FriendGrant, AuthError> {
    authorize_owner(actor_email, OwnerOnlyAction::GrantOrRevokeFriendRole)?;
    let email = normalize_identity_email(friend_email);
    if email.is_empty() || email.len() > 254 || !email.contains('@') || is_owner(&email) {
        return Err(AuthError::InvalidRequest);
    }
    let note = note
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    Ok(FriendGrant {
        email,
        note,
        granted_by: OWNER_EMAIL.to_owned(),
        granted_at_unix,
        entitlements: FRIEND_ENTITLEMENTS,
    })
}

/// Revoke the Friend role. Only the owner may call this.
pub fn revoke_friend_role(actor_email: &str, friend_email: &str) -> Result<String, AuthError> {
    authorize_owner(actor_email, OwnerOnlyAction::GrantOrRevokeFriendRole)?;
    let email = normalize_identity_email(friend_email);
    if email.is_empty() || !email.contains('@') {
        return Err(AuthError::InvalidRequest);
    }
    Ok(email)
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
    fn owner_identity_is_strict_after_trim_and_lowercase() {
        assert!(is_owner("mo7amed.rabei14@gmail.com"));
        assert!(is_owner("  MO7amed.Rabei14@Gmail.com  "));
        assert!(!is_owner("mo7amed.rabei14@gmail.com.eg"));
        assert!(!is_owner("mo7amed.rabei14+1@gmail.com"));
        assert!(!is_owner("owner@safi.local"));
        assert!(!is_owner(""));
    }

    #[test]
    fn every_owner_action_rejects_a_non_owner() {
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
                authorize_owner("friend@example.test", action).unwrap_err(),
                AuthError::InsufficientScope
            );
            assert!(authorize_owner(OWNER_EMAIL, action).is_ok());
        }
    }

    #[test]
    fn owner_grant_is_minted_only_for_the_owner() {
        assert!(authorize_owner_grant(OWNER_EMAIL, OwnerOnlyAction::SuspendUser).is_ok());
        assert_eq!(
            authorize_owner_grant("friend@example.test", OwnerOnlyAction::SuspendUser).unwrap_err(),
            AuthError::InsufficientScope
        );
    }

    #[test]
    fn friend_role_grants_the_full_paid_plan_for_free() {
        let grant = grant_friend_role(
            " MO7amed.Rabei14@Gmail.com ",
            " Friend@Example.test ",
            Some("  Ahmed  "),
            1_780_000_000,
        )
        .unwrap();

        assert_eq!(grant.email, "friend@example.test");
        assert_eq!(grant.note.as_deref(), Some("Ahmed"));
        assert_eq!(grant.granted_by, OWNER_EMAIL);
        assert_eq!(grant.entitlements, FRIEND_ENTITLEMENTS);
        assert!(grant.entitlements.all_paid_features_open);
        assert!(grant.entitlements.removes_watermark);
        assert!(grant.entitlements.saved_recipes);
        assert!(grant.entitlements.includes_premium_recipe_packs);
        assert_eq!(grant.entitlements.ai_calls_per_month, 400);
        assert_eq!(grant.entitlements.price_usd, 0);
        assert_eq!(grant.entitlements.price_egp, 0);
    }

    #[test]
    fn only_the_owner_can_grant_or_revoke_friend() {
        assert_eq!(
            grant_friend_role("friend@example.test", "other@example.test", None, 0).unwrap_err(),
            AuthError::InsufficientScope
        );
        assert_eq!(
            revoke_friend_role("friend@example.test", "other@example.test").unwrap_err(),
            AuthError::InsufficientScope
        );
        assert_eq!(
            revoke_friend_role(OWNER_EMAIL, " Friend@Example.test ").unwrap(),
            "friend@example.test"
        );
    }

    #[test]
    fn friend_grants_reject_malformed_and_owner_addresses() {
        assert_eq!(
            grant_friend_role(OWNER_EMAIL, "  ", None, 0).unwrap_err(),
            AuthError::InvalidRequest
        );
        assert_eq!(
            grant_friend_role(OWNER_EMAIL, "not-an-email", None, 0).unwrap_err(),
            AuthError::InvalidRequest
        );
        assert_eq!(
            grant_friend_role(OWNER_EMAIL, OWNER_EMAIL, None, 0).unwrap_err(),
            AuthError::InvalidRequest
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
        assert_eq!(
            detect_duplicate_account(&existing, "phone-less@example.test", Some("201001234567")),
            Some(DuplicateAccountKey::Email)
        );
    }

    #[test]
    fn password_floor_is_eight_characters() {
        assert_eq!(MIN_PASSWORD_LENGTH, 8);
        assert_eq!(
            PASSWORD_TOO_SHORT_MESSAGE_AR,
            "كلمة المرور يجب ألا تقل عن 8 خانات"
        );
        assert_eq!(
            DUPLICATE_ACCOUNT_MESSAGE_AR,
            "هذا الحساب مسجل بالفعل، يرجى تسجيل الدخول"
        );
    }
}
