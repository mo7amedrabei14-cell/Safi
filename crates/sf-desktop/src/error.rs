//! Stable desktop error codes (AGENTS.md rule 9).
//!
//! The type carries only a code. Arabic and English wording lives in
//! `errors.toml`, so nothing in the shell needs to duplicate user-facing text.
//!
//! Desktop rejections use the existing `SYS` domain (`SF-SYS-002`..`SF-SYS-011`)
//! because Specification §15.4 locks the canonical domain list at fifteen
//! entries; the wording in `errors.toml` carries the desktop-specific meaning.

use std::fmt;

/// Every rejection the desktop shell can produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DesktopErrorCode {
    /// `SF-SYS-002`: the deep link is structurally malformed.
    DeepLinkMalformed,
    /// `SF-SYS-003`: the deep-link action is not allowlisted.
    DeepLinkActionDenied,
    /// `SF-SYS-004`: a deep-link parameter failed its allowlist or value checks.
    DeepLinkParameterDenied,
    /// `SF-SYS-005`: navigation tried to leave the app's local origin.
    RemoteContentBlocked,
    /// `SF-SYS-006`: a Tauri permission is outside the allowlist.
    PermissionDenied,
    /// `SF-SYS-007`: an IPC payload exceeded the bridge limit.
    PayloadTooLarge,
    /// `SF-SYS-008`: the shell configuration failed conformance.
    InvalidConfiguration,
    /// `SF-SYS-009`: an external link is not a safe public `https` URL.
    ExternalLinkBlocked,
    /// `SF-SYS-010`: an IPC command is outside the allowlist.
    CommandDenied,
    /// `SF-SYS-011`: the native file dialog could not be opened.
    DialogUnavailable,
}

impl DesktopErrorCode {
    /// The code as written in `errors.toml`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DeepLinkMalformed => "SF-SYS-002",
            Self::DeepLinkActionDenied => "SF-SYS-003",
            Self::DeepLinkParameterDenied => "SF-SYS-004",
            Self::RemoteContentBlocked => "SF-SYS-005",
            Self::PermissionDenied => "SF-SYS-006",
            Self::PayloadTooLarge => "SF-SYS-007",
            Self::InvalidConfiguration => "SF-SYS-008",
            Self::ExternalLinkBlocked => "SF-SYS-009",
            Self::CommandDenied => "SF-SYS-010",
            Self::DialogUnavailable => "SF-SYS-011",
        }
    }
}

/// A coded desktop rejection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopError {
    code: DesktopErrorCode,
}

impl DesktopError {
    /// Build an error from its code.
    #[must_use]
    pub const fn new(code: DesktopErrorCode) -> Self {
        Self { code }
    }

    /// The structured code.
    #[must_use]
    pub const fn code(self) -> DesktopErrorCode {
        self.code
    }

    /// The code as written in `errors.toml`.
    #[must_use]
    pub const fn code_str(self) -> &'static str {
        self.code.as_str()
    }
}

impl fmt::Display for DesktopError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code.as_str())
    }
}

impl std::error::Error for DesktopError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_code_is_reported_in_the_documented_format() {
        let codes = [
            DesktopErrorCode::DeepLinkMalformed,
            DesktopErrorCode::DeepLinkActionDenied,
            DesktopErrorCode::DeepLinkParameterDenied,
            DesktopErrorCode::RemoteContentBlocked,
            DesktopErrorCode::PermissionDenied,
            DesktopErrorCode::PayloadTooLarge,
            DesktopErrorCode::InvalidConfiguration,
            DesktopErrorCode::ExternalLinkBlocked,
            DesktopErrorCode::CommandDenied,
            DesktopErrorCode::DialogUnavailable,
        ];

        for (index, code) in codes.iter().enumerate() {
            let expected = format!("SF-SYS-{:03}", index + 2);
            assert_eq!(code.as_str(), expected);
        }

        let error = DesktopError::new(DesktopErrorCode::RemoteContentBlocked);
        assert_eq!(error.to_string(), "SF-SYS-005");
        assert_eq!(error.code_str(), "SF-SYS-005");
    }
}
