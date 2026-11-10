//! Stable installation identity for the one-active-device account policy.
//!
//! The raw installation UUID is accepted only for the login request. PostgreSQL
//! stores a user-scoped, domain-separated BLAKE3 digest; the raw UUID is never
//! persisted or logged.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AuthError;

/// Platforms supported by the account/device registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DevicePlatform {
    Windows,
    Linux,
    Macos,
    Web,
}

impl DevicePlatform {
    /// Canonical value stored in PostgreSQL.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::Macos => "macos",
            Self::Web => "web",
        }
    }
}

/// Server-derived device data used only while issuing a session.
#[derive(Clone, Copy)]
pub struct DeviceRegistration {
    pub installation_key_hash: [u8; 32],
    pub platform: DevicePlatform,
}

impl std::fmt::Debug for DeviceRegistration {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DeviceRegistration")
            .field("installation_key_hash", &"[REDACTED]")
            .field("platform", &self.platform)
            .finish()
    }
}

impl DeviceRegistration {
    /// Derive a stable, user-scoped installation digest without retaining the
    /// caller-supplied UUID. The same installation maps to the same digest for
    /// one account, but cannot be correlated across different accounts.
    pub fn from_installation_id(
        user_id: Uuid,
        installation_id: Uuid,
        platform: DevicePlatform,
    ) -> Result<Self, AuthError> {
        if user_id.is_nil() || installation_id.is_nil() {
            return Err(AuthError::InvalidRequest);
        }

        let mut hasher =
            blake3::Hasher::new_derive_key("com.safidata.installation-key.v1");
        hasher.update(user_id.as_bytes());
        hasher.update(installation_id.as_bytes());

        Ok(Self {
            installation_key_hash: *hasher.finalize().as_bytes(),
            platform,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installation_digest_is_stable_for_the_same_user_and_installation() {
        let user_id = Uuid::now_v7();
        let installation_id = Uuid::now_v7();
        let first = DeviceRegistration::from_installation_id(
            user_id,
            installation_id,
            DevicePlatform::Web,
        )
        .unwrap();
        let second = DeviceRegistration::from_installation_id(
            user_id,
            installation_id,
            DevicePlatform::Windows,
        )
        .unwrap();

        assert_eq!(first.installation_key_hash, second.installation_key_hash);
        assert_ne!(first.platform, second.platform);
    }

    #[test]
    fn installation_digest_is_scoped_to_the_account() {
        let installation_id = Uuid::now_v7();
        let first = DeviceRegistration::from_installation_id(
            Uuid::now_v7(),
            installation_id,
            DevicePlatform::Web,
        )
        .unwrap();
        let second = DeviceRegistration::from_installation_id(
            Uuid::now_v7(),
            installation_id,
            DevicePlatform::Web,
        )
        .unwrap();

        assert_ne!(first.installation_key_hash, second.installation_key_hash);
    }

    #[test]
    fn nil_identifiers_are_rejected() {
        assert_eq!(
            DeviceRegistration::from_installation_id(
                Uuid::now_v7(),
                Uuid::nil(),
                DevicePlatform::Web,
            )
            .unwrap_err(),
            AuthError::InvalidRequest
        );
    }

    #[test]
    fn platform_names_match_the_database_contract() {
        assert_eq!(DevicePlatform::Windows.as_str(), "windows");
        assert_eq!(DevicePlatform::Linux.as_str(), "linux");
        assert_eq!(DevicePlatform::Macos.as_str(), "macos");
        assert_eq!(DevicePlatform::Web.as_str(), "web");
    }
}
