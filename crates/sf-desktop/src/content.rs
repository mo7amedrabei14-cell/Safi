//! Local-only content policy for the desktop webview (section 8.6).
//!
//! The shell may only navigate inside its own bundled origin. Any remote
//! `http(s)` host, `file:`, `data:`, `javascript:` or `blob:` URL is refused,
//! and so are path-traversal sequences in any encoding.

use crate::error::{DesktopError, DesktopErrorCode};

/// Longest URL the shell accepts before rejecting it outright.
pub const MAX_URL_LEN: usize = 2_048;

const APP_ORIGINS: [&str; 3] = [
    "tauri://localhost",
    "http://tauri.localhost",
    "https://tauri.localhost",
];

const FORBIDDEN_SEQUENCES: [&str; 5] = ["..", "%2e", "%2f", "%5c", "%00"];

const FORBIDDEN_SCHEMES: [&str; 4] = ["javascript:", "data:", "file:", "vbscript:"];

fn deny(code: DesktopErrorCode) -> DesktopError {
    DesktopError::new(code)
}

/// Reject navigation that leaves the app's own local origin.
///
/// # Errors
///
/// Returns [`DesktopErrorCode::RemoteContentBlocked`] for anything that is not
/// one of the three documented Tauri origins or that smuggles traversal bytes.
pub fn validate_app_navigation(raw: &str) -> Result<(), DesktopError> {
    let code = DesktopErrorCode::RemoteContentBlocked;
    validate_shape(raw, code)?;

    let lower = raw.to_ascii_lowercase();
    if contains_any(&lower, &FORBIDDEN_SCHEMES, &FORBIDDEN_SEQUENCES) {
        return Err(deny(code));
    }

    for origin in APP_ORIGINS {
        if let Some(rest) = lower.strip_prefix(origin) {
            if rest.is_empty()
                || rest.starts_with('/')
                || rest.starts_with('?')
                || rest.starts_with('#')
            {
                return Ok(());
            }
        }
    }

    Err(deny(code))
}

/// Accept only a public `https` link with no credentials, port, IP or loopback host.
///
/// # Errors
///
/// Returns [`DesktopErrorCode::ExternalLinkBlocked`] when the link could reach a
/// local service, a non-TLS endpoint, or a host the shell cannot vouch for.
pub fn validate_external_link(raw: &str) -> Result<(), DesktopError> {
    let code = DesktopErrorCode::ExternalLinkBlocked;
    validate_shape(raw, code)?;

    let lower = raw.to_ascii_lowercase();
    if FORBIDDEN_SCHEMES
        .iter()
        .any(|scheme| lower.contains(scheme))
    {
        return Err(deny(code));
    }

    let rest = lower.strip_prefix("https://").ok_or_else(|| deny(code))?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();

    if authority.is_empty()
        || authority.contains(':')
        || authority.contains('[')
        || authority == "localhost"
        || authority.ends_with(".local")
        || authority.starts_with('.')
        || authority.ends_with('.')
        || !authority.contains('.')
        || authority
            .chars()
            .all(|character| character.is_ascii_digit() || character == '.')
    {
        return Err(deny(code));
    }

    Ok(())
}

/// Validates a system-browser Desktop PKCE sign-in/sign-up URL before the Tauri shell opens it
/// in the user's default OS browser.
///
/// Only permits exact-origin `/auth/desktop?flow=signin|signup&request_id=...&state=...` URLs
/// matching `allowed_origin` (either the compiled HTTPS `SAFI_API_BASE_URL` or a loopback
/// development/test origin when explicitly passed). Rejects arbitrary URLs, schemes, path
/// traversal, extra query parameters, or shell metacharacters.
///
/// # Errors
/// Returns [`DesktopErrorCode::ExternalLinkBlocked`] (`SF-SYS-009`) on any violation.
pub fn validate_desktop_auth_external_url(
    raw: &str,
    allowed_origin: &str,
) -> Result<(), DesktopError> {
    let code = DesktopErrorCode::ExternalLinkBlocked;
    validate_shape(raw, code)?;
    let lower = raw.to_ascii_lowercase();
    if contains_any(&lower, &FORBIDDEN_SCHEMES, &FORBIDDEN_SEQUENCES) {
        return Err(deny(code));
    }
    let origin = allowed_origin.trim().trim_end_matches('/');
    if origin.is_empty() {
        return Err(deny(code));
    }
    let is_https = origin.starts_with("https://");
    let is_loopback_http =
        origin.starts_with("http://127.0.0.1:") || origin.starts_with("http://localhost:");
    if !is_https && !is_loopback_http {
        return Err(deny(code));
    }
    let prefix = format!("{origin}/auth/desktop?");
    let Some(query) = raw.strip_prefix(&prefix) else {
        return Err(deny(code));
    };
    if query.contains('#') {
        return Err(deny(code));
    }

    let mut flow_ok = false;
    let mut request_id_ok = false;
    let mut state_ok = false;

    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            return Err(deny(code));
        };
        let is_token_chars = |s: &str| {
            s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        };
        match key {
            "flow" => {
                if flow_ok || !matches!(value, "signin" | "signup") {
                    return Err(deny(code));
                }
                flow_ok = true;
            }
            "request_id" => {
                if request_id_ok || !(8..=128).contains(&value.len()) || !is_token_chars(value) {
                    return Err(deny(code));
                }
                request_id_ok = true;
            }
            "state" => {
                if state_ok || !(16..=256).contains(&value.len()) || !is_token_chars(value) {
                    return Err(deny(code));
                }
                state_ok = true;
            }
            _ => return Err(deny(code)),
        }
    }

    if flow_ok && request_id_ok && state_ok {
        Ok(())
    } else {
        Err(deny(code))
    }
}

fn validate_shape(raw: &str, code: DesktopErrorCode) -> Result<(), DesktopError> {
    if raw.is_empty() || raw.len() > MAX_URL_LEN || raw != raw.trim() {
        return Err(deny(code));
    }
    if raw
        .chars()
        .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(deny(code));
    }
    if raw.contains('\\') || raw.contains('@') {
        return Err(deny(code));
    }
    Ok(())
}

fn contains_any(lower: &str, schemes: &[&str], sequences: &[&str]) -> bool {
    schemes.iter().any(|value| lower.contains(value))
        || sequences.iter().any(|value| lower.contains(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_three_bundled_tauri_origins() {
        for url in [
            "tauri://localhost",
            "tauri://localhost/index.html",
            "tauri://localhost/#/library?tab=recent",
            "http://tauri.localhost/index.html",
            "https://tauri.localhost/index.html",
        ] {
            assert!(
                validate_app_navigation(url).is_ok(),
                "expected local origin to pass: {url}"
            );
        }
    }

    #[test]
    fn rejects_remote_and_non_http_content() {
        for url in [
            "https://evil.example/index.html",
            "http://safidata.app/",
            "file:///C:/Windows/System32/calc.exe",
            "data:text/html,<script>alert(1)</script>",
            "javascript:alert(1)",
            "vbscript:msgbox(1)",
            "blob:https://evil.example/123",
            "//evil.example/index.html",
            "tauri://evil.example/index.html",
            "tauri://localhost.evil.example/",
            "about:blank",
            "mailto:owner@safidata.app",
        ] {
            assert_eq!(
                validate_app_navigation(url).map_err(DesktopError::code),
                Err(DesktopErrorCode::RemoteContentBlocked),
                "expected remote content to be blocked: {url}"
            );
        }
    }

    #[test]
    fn rejects_path_traversal_in_every_encoding() {
        for url in [
            "tauri://localhost/../../etc/passwd",
            "tauri://localhost/%2e%2e/%2e%2e/etc/passwd",
            "tauri://localhost/%2E%2E/secret",
            "tauri://localhost/%2f%2fevil.example",
            "tauri://localhost/%5c%5cevil.example",
            "tauri://localhost/%00/index.html",
        ] {
            assert!(
                validate_app_navigation(url).is_err(),
                "expected traversal to be blocked: {url}"
            );
        }
    }

    #[test]
    fn rejects_confusing_or_oversized_input() {
        let oversized = format!("tauri://localhost/{}", "a".repeat(MAX_URL_LEN));
        for url in [
            "",
            " tauri://localhost/index.html",
            "tauri://localhost/index.html ",
            "tauri://user@localhost/index.html",
            "tauri://localhost\\index.html",
            "tauri://localhost/index.html\n",
            oversized.as_str(),
        ] {
            assert!(
                validate_app_navigation(url).is_err(),
                "expected malformed URL to be blocked: {url:?}"
            );
        }
    }

    #[test]
    fn accepts_only_public_https_external_links() {
        assert!(validate_external_link("https://safidata.app/docs").is_ok());
        assert!(validate_external_link("https://docs.safidata.app/ar?x=1").is_ok());

        for url in [
            "http://safidata.app/docs",
            "https://localhost/admin",
            "https://127.0.0.1/admin",
            "https://10.0.0.5/admin",
            "https://printer.local/admin",
            "https://user:pass@safidata.app/",
            "https://safidata.app:8443/",
            "https://[::1]/",
            "javascript:alert(1)",
            "https://safidata",
            "https://.safidata.app/",
            "https://safidata.app./",
        ] {
            assert_eq!(
                validate_external_link(url).map_err(DesktopError::code),
                Err(DesktopErrorCode::ExternalLinkBlocked),
                "expected unsafe external link to be blocked: {url}"
            );
        }
    }

    #[test]
    fn validates_desktop_pkce_external_auth_urls_strictly() {
        assert!(validate_desktop_auth_external_url(
            "https://api.safidata.app/auth/desktop?flow=signin&request_id=req_12345678&state=state_1234567890abcdef",
            "https://api.safidata.app"
        )
        .is_ok());
        assert!(validate_desktop_auth_external_url(
            "http://127.0.0.1:4173/auth/desktop?flow=signup&request_id=req_87654321&state=state_1234567890abcdef",
            "http://127.0.0.1:4173"
        )
        .is_ok());

        for bad in [
            "https://evil.example/auth/desktop?flow=signin&request_id=req_12345678&state=state_1234567890abcdef",
            "https://api.safidata.app/owner?flow=signin&request_id=req_12345678&state=state_1234567890abcdef",
            "https://api.safidata.app/auth/desktop?flow=admin&request_id=req_12345678&state=state_1234567890abcdef",
            "https://api.safidata.app/auth/desktop?flow=signin&request_id=short&state=state_1234567890abcdef",
            "https://api.safidata.app/auth/desktop?flow=signin&request_id=req_12345678&state=short",
            "https://api.safidata.app/auth/desktop?flow=signin&request_id=req_12345678&state=state_1234567890abcdef&extra=1",
            "https://api.safidata.app/auth/desktop?flow=signin&request_id=req_12345678&state=state_1234567890abcdef#frag",
            "javascript:alert(1)",
        ] {
            assert_eq!(
                validate_desktop_auth_external_url(bad, "https://api.safidata.app")
                    .map_err(DesktopError::code),
                Err(DesktopErrorCode::ExternalLinkBlocked),
                "expected invalid desktop auth URL to be blocked: {bad}"
            );
        }
    }
}
