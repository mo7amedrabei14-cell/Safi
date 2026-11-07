//! Explicit Tauri capability allowlist (section 8.6).
//!
//! Tauri plugins are deny-by-default, but a generated capability file can drift.
//! The shell therefore accepts only these four permissions; adding anything else
//! requires a code change here, a test update, and owner review.

use crate::error::{DesktopError, DesktopErrorCode};

/// The complete permission set granted to the main window.
pub const ALLOWED_PERMISSIONS: [&str; 4] = [
    "core:default",
    "dialog:allow-open",
    "dialog:allow-save",
    "deep-link:default",
];

/// Authorise one capability permission.
///
/// # Errors
///
/// Returns [`DesktopErrorCode::PermissionDenied`] for every permission outside
/// [`ALLOWED_PERMISSIONS`].
pub fn authorize_permission(permission: &str) -> Result<(), DesktopError> {
    if ALLOWED_PERMISSIONS.contains(&permission) {
        Ok(())
    } else {
        Err(DesktopError::new(DesktopErrorCode::PermissionDenied))
    }
}

/// Authorise a whole capability permission list.
///
/// # Errors
///
/// Returns the first [`DesktopErrorCode::PermissionDenied`] it encounters.
pub fn authorize_permissions<'a>(
    permissions: impl IntoIterator<Item = &'a str>,
) -> Result<(), DesktopError> {
    for permission in permissions {
        authorize_permission(permission)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grants_exactly_the_four_documented_permissions() {
        assert_eq!(ALLOWED_PERMISSIONS.len(), 4);
        assert!(authorize_permissions(ALLOWED_PERMISSIONS).is_ok());
    }

    #[test]
    fn refuses_every_plugin_that_could_reach_the_network_or_the_filesystem() {
        for permission in [
            "",
            "dialog:default",
            "dialog:allow-ask",
            "fs:default",
            "fs:allow-read-file",
            "shell:allow-execute",
            "http:default",
            "process:allow-restart",
            "updater:default",
            "opener:default",
            "clipboard-manager:allow-read-text",
            "os:default",
            "core:allow-all",
        ] {
            assert_eq!(
                authorize_permission(permission).map_err(DesktopError::code),
                Err(DesktopErrorCode::PermissionDenied),
                "expected permission to be denied: {permission:?}"
            );
        }
    }
}
