//! Strict `safi://` deep-link parser (sections 8.6 and 10.2).
//!
//! A deep link may be triggered by the operating system on behalf of any
//! website, so nothing about it is trusted. Only allowlisted actions and
//! per-action parameters pass, each exactly once, percent-decoded strictly, and
//! a file parameter must be an absolute spreadsheet path with no traversal.

use crate::error::{DesktopError, DesktopErrorCode};

/// Scheme registered with the operating system.
pub const DEEP_LINK_SCHEME: &str = "safi";

/// Longest accepted deep link.
pub const MAX_DEEP_LINK_LEN: usize = 2_048;

/// Longest accepted parameter value before decoding.
pub const MAX_PARAM_LEN: usize = 1_024;

const SPREADSHEET_EXTENSIONS: [&str; 3] = ["csv", "xls", "xlsx"];

/// A validated destination inside the app.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum DeepLinkAction {
    /// Open a spreadsheet that the operating system associated with SafiData.
    OpenFile {
        /// Absolute path inside the user's own filesystem.
        path: String,
    },
    /// Focus the settings screen on a specific section.
    Settings {
        /// The requested section.
        section: SettingsSection,
    },
}

/// Settings sections a deep link may target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingsSection {
    /// General preferences.
    General,
    /// Language and direction.
    Language,
    /// Privacy and data controls.
    Privacy,
    /// Update channel and checks.
    Updates,
}

fn malformed() -> DesktopError {
    DesktopError::new(DesktopErrorCode::DeepLinkMalformed)
}

fn action_denied() -> DesktopError {
    DesktopError::new(DesktopErrorCode::DeepLinkActionDenied)
}

fn parameter_denied() -> DesktopError {
    DesktopError::new(DesktopErrorCode::DeepLinkParameterDenied)
}

/// Parse and validate a `safi://` deep link.
///
/// # Errors
///
/// Returns `SF-SYS-002` for malformed links, `SF-SYS-003` for actions outside
/// the allowlist, and `SF-SYS-004` for parameters that fail their checks.
pub fn parse(raw: &str) -> Result<DeepLinkAction, DesktopError> {
    if raw.is_empty() || raw.len() > MAX_DEEP_LINK_LEN || raw != raw.trim() {
        return Err(malformed());
    }
    if raw
        .chars()
        .any(|character| character.is_control() || character.is_whitespace())
        || raw.contains('#')
        || raw.contains('@')
    {
        return Err(malformed());
    }

    let (scheme, rest) = raw.split_once("://").ok_or_else(malformed)?;
    if !scheme.eq_ignore_ascii_case(DEEP_LINK_SCHEME) || rest.is_empty() {
        return Err(malformed());
    }

    let (action, query) = match rest.split_once('?') {
        Some((action, query)) => (action, Some(query)),
        None => (rest, None),
    };
    if action.is_empty() || action.contains('/') || action.contains('\\') {
        return Err(malformed());
    }

    match action {
        "open" => Ok(DeepLinkAction::OpenFile {
            path: validate_file_path(&single_param(query, "file")?)?,
        }),
        "settings" => Ok(DeepLinkAction::Settings {
            section: match single_param(query, "section")?.as_str() {
                "general" => SettingsSection::General,
                "language" => SettingsSection::Language,
                "privacy" => SettingsSection::Privacy,
                "updates" => SettingsSection::Updates,
                _ => return Err(parameter_denied()),
            },
        }),
        _ => Err(action_denied()),
    }
}

fn single_param(query: Option<&str>, expected: &str) -> Result<String, DesktopError> {
    let query = query.ok_or_else(parameter_denied)?;
    if query.is_empty() {
        return Err(parameter_denied());
    }

    let mut found: Option<String> = None;
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').ok_or_else(parameter_denied)?;
        if key != expected || found.is_some() || value.is_empty() || value.len() > MAX_PARAM_LEN {
            return Err(parameter_denied());
        }
        found = Some(percent_decode(value)?);
    }

    found.ok_or_else(parameter_denied)
}

fn validate_file_path(path: &str) -> Result<String, DesktopError> {
    if path.is_empty() || path.len() > MAX_PARAM_LEN {
        return Err(parameter_denied());
    }
    if path.chars().any(char::is_control) {
        return Err(parameter_denied());
    }
    if path.starts_with("//") || path.starts_with("\\\\") {
        return Err(parameter_denied());
    }
    if path.split(['/', '\\']).any(|segment| segment == "..") {
        return Err(parameter_denied());
    }

    let bytes = path.as_bytes();
    let is_drive_path = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'/' || bytes[2] == b'\\');
    let is_posix_path = path.starts_with('/') && !path.starts_with("//");
    if !is_drive_path && !is_posix_path {
        return Err(parameter_denied());
    }

    let extension = path
        .rsplit(['/', '\\'])
        .next()
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, extension)| extension.to_ascii_lowercase());
    match extension.as_deref() {
        Some(extension) if SPREADSHEET_EXTENSIONS.contains(&extension) => Ok(path.to_owned()),
        _ => Err(parameter_denied()),
    }
}

fn percent_decode(input: &str) -> Result<String, DesktopError> {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes
                .get(index + 1)
                .copied()
                .and_then(hex_value)
                .ok_or_else(parameter_denied)?;
            let low = bytes
                .get(index + 2)
                .copied()
                .and_then(hex_value)
                .ok_or_else(parameter_denied)?;
            decoded.push(high * 16 + low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }

    String::from_utf8(decoded).map_err(|_| parameter_denied())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_an_associated_spreadsheet_including_an_arabic_path() {
        let action = parse(
            "safi://open?file=C%3A%5CUsers%5C%D9%85%D8%AD%D9%85%D8%AF%5C%D8%A8%D9%8A%D8%A7%D9%86%D8%A7%D8%AA.xlsx",
        );
        assert_eq!(
            action,
            Ok(DeepLinkAction::OpenFile {
                path: "C:\\Users\\محمد\\بيانات.xlsx".to_owned()
            })
        );

        assert_eq!(
            parse("safi://open?file=%2Fhome%2Fowner%2Fsales.CSV"),
            Ok(DeepLinkAction::OpenFile {
                path: "/home/owner/sales.CSV".to_owned()
            })
        );
    }

    #[test]
    fn accepts_allowlisted_settings_sections() {
        for (raw, expected) in [
            ("safi://settings?section=general", SettingsSection::General),
            (
                "safi://settings?section=language",
                SettingsSection::Language,
            ),
            ("safi://settings?section=privacy", SettingsSection::Privacy),
            ("safi://settings?section=updates", SettingsSection::Updates),
        ] {
            assert_eq!(
                parse(raw),
                Ok(DeepLinkAction::Settings { section: expected }),
                "expected settings section to pass: {raw}"
            );
        }
    }

    #[test]
    fn rejects_unknown_actions_and_schemes() {
        for raw in [
            "https://evil.example/open?file=C:/x.csv",
            "safi2://open?file=C:/x.csv",
            "safi://delete?file=C:/x.csv",
            "safi://open/extra?file=C:/x.csv",
            "safi://OPEN?file=C:/x.csv",
            "safi://",
            "safi:/open?file=C:/x.csv",
            "safi://open#fragment",
        ] {
            assert!(
                parse(raw).is_err(),
                "expected deep link to be rejected: {raw}"
            );
        }
    }

    #[test]
    fn rejects_parameters_outside_the_per_action_allowlist() {
        for raw in [
            "safi://open",
            "safi://open?",
            "safi://open?path=C:/x.csv",
            "safi://open?file=C:/x.csv&extra=1",
            "safi://open?file=C:/x.csv&file=C:/y.csv",
            "safi://open?file=",
            "safi://open?file",
            "safi://open?file=&file=C:/x.csv",
            "safi://settings?section=root",
            "safi://settings?section=Language",
            "safi://settings?file=C:/x.csv",
            "safi://settings?section=general&section=language",
        ] {
            assert_eq!(
                parse(raw).map_err(DesktopError::code),
                Err(DesktopErrorCode::DeepLinkParameterDenied),
                "expected parameter to be denied: {raw}"
            );
        }
    }

    #[test]
    fn rejects_traversal_relative_unc_and_non_spreadsheet_paths() {
        for raw in [
            "safi://open?file=C%3A%2Fx%2F..%2F..%2Fsecret.csv",
            "safi://open?file=C%3A%5Cx%5C..%5Csecret.csv",
            "safi://open?file=relative.csv",
            "safi://open?file=..%2Fsecret.csv",
            "safi://open?file=%2F%2Fserver%2Fshare%2Fx.csv",
            "safi://open?file=%5C%5Cserver%5Cshare%5Cx.csv",
            "safi://open?file=C%3A%2Fx.txt",
            "safi://open?file=C%3A%2Fx",
            "safi://open?file=C%3A%2F.png",
            "safi://open?file=C%3A%2Fx.csv%00.exe",
            "safi://open?file=C%3A%2Fx%2.csv",
            "safi://open?file=C%3A%2F%ZZ.csv",
        ] {
            assert!(parse(raw).is_err(), "expected path to be rejected: {raw}");
        }
    }

    #[test]
    fn rejects_malformed_or_oversized_links() {
        let oversized = format!("safi://open?file={}", "a".repeat(MAX_PARAM_LEN));
        for raw in [
            "",
            " safi://open?file=C:/x.csv",
            "safi://open?file=C:/x.csv ",
            "safi://user@open?file=C:/x.csv",
            "safi://open?file=C:/x.csv\n",
            oversized.as_str(),
        ] {
            assert!(
                parse(raw).is_err(),
                "expected malformed deep link to be rejected: {raw:?}"
            );
        }
    }
}
