//! Strict validation of the account-service origin baked into desktop builds.
//!
//! An installed shell must never fall back to a local address. A missing or
//! malformed `SAFI_API_BASE_URL` is an error, and a valid origin is injected
//! into both the web bundle and the Tauri `connect-src` policy.

/// Environment variable read at desktop build time.
pub const API_ORIGIN_ENV: &str = "SAFI_API_BASE_URL";

const HTTPS_SCHEME: &str = "https://";

/// Why an account-service origin was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiOriginError {
    /// The value was empty or whitespace only.
    Missing,
    /// The scheme was not exactly `https`.
    NotHttps,
    /// A path, query, fragment, or user-info component was present.
    NotAnOrigin,
    /// The host was missing, single-label, or not a valid DNS name.
    InvalidHost,
    /// The host was an IP literal, which cannot carry a trusted certificate name.
    IpAddress,
    /// The host pointed at the local machine.
    Localhost,
    /// The port was not a number in `1..=65535`.
    InvalidPort,
}

impl ApiOriginError {
    /// Stable, non-sensitive explanation for build logs.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::Missing => "the API origin is empty",
            Self::NotHttps => "the API origin must start with https://",
            Self::NotAnOrigin => {
                "the API origin must not contain a path, query, fragment, or credentials"
            }
            Self::InvalidHost => "the API origin must use a valid multi-label DNS host",
            Self::IpAddress => "the API origin must not be an IP address",
            Self::Localhost => "the API origin must not point at localhost",
            Self::InvalidPort => "the API origin port must be between 1 and 65535",
        }
    }
}

/// A normalized `https://host[:port]` origin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiOrigin {
    origin: String,
}

impl ApiOrigin {
    /// Parse and normalize a configured origin.
    ///
    /// The scheme and host are lowercased, the default port `443` is removed,
    /// and every other component is rejected rather than repaired.
    pub fn parse(raw: &str) -> Result<Self, ApiOriginError> {
        let value = raw.trim();
        if value.is_empty() {
            return Err(ApiOriginError::Missing);
        }
        let scheme = value
            .get(..HTTPS_SCHEME.len())
            .ok_or(ApiOriginError::NotHttps)?;
        if !scheme.eq_ignore_ascii_case(HTTPS_SCHEME) {
            return Err(ApiOriginError::NotHttps);
        }
        let rest = &value[HTTPS_SCHEME.len()..];
        if rest.is_empty()
            || rest
                .chars()
                .any(|character| matches!(character, '/' | '?' | '#' | '@'))
        {
            return Err(ApiOriginError::NotAnOrigin);
        }
        if rest.starts_with('[') {
            return Err(ApiOriginError::InvalidHost);
        }

        let (host_raw, port_raw) = match rest.rsplit_once(':') {
            Some((host, port)) => {
                if port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err(ApiOriginError::InvalidPort);
                }
                (host, Some(port))
            }
            None => (rest, None),
        };

        if host_raw.is_empty() || host_raw.starts_with('[') || host_raw.contains(':') {
            return Err(ApiOriginError::InvalidHost);
        }
        let host = host_raw.to_ascii_lowercase();
        if host == "localhost" || host.ends_with(".localhost") {
            return Err(ApiOriginError::Localhost);
        }
        if host
            .chars()
            .all(|character| character.is_ascii_digit() || character == '.')
        {
            return Err(ApiOriginError::IpAddress);
        }
        if !host.contains('.') || !valid_dns_labels(&host) {
            return Err(ApiOriginError::InvalidHost);
        }

        let port = match port_raw {
            Some(port) => {
                let parsed: u32 = port.parse().map_err(|_| ApiOriginError::InvalidPort)?;
                if parsed == 0 || parsed > 65_535 {
                    return Err(ApiOriginError::InvalidPort);
                }
                (parsed != 443).then_some(parsed)
            }
            None => None,
        };

        let origin = match port {
            Some(port) => format!("https://{host}:{port}"),
            None => format!("https://{host}"),
        };
        Ok(Self { origin })
    }

    /// The normalized origin, without a trailing slash.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.origin
    }
}

fn valid_dns_labels(host: &str) -> bool {
    host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-')
    })
}

/// Add the configured origin to the `connect-src` directive and leave every
/// other directive untouched. Applying the same origin twice is idempotent.
pub fn apply_csp_origin(csp: &str, origin: &ApiOrigin) -> Result<String, ApiOriginError> {
    let needle = origin.as_str();
    let directives: Vec<&str> = csp
        .split(';')
        .map(str::trim)
        .filter(|directive| !directive.is_empty())
        .collect();
    if directives.is_empty() {
        return Err(ApiOriginError::NotAnOrigin);
    }

    let mut result = Vec::with_capacity(directives.len() + 1);
    let mut seen_connect_src = false;
    for directive in directives {
        let mut parts = directive.split_whitespace();
        let name = parts.next().unwrap_or_default();
        if name.eq_ignore_ascii_case("connect-src") {
            if seen_connect_src {
                continue;
            }
            let mut sources: Vec<&str> = parts.collect();
            if !sources.contains(&needle) {
                sources.push(needle);
            }
            result.push(format!("connect-src {}", sources.join(" ")));
            seen_connect_src = true;
        } else {
            result.push(directive.to_owned());
        }
    }
    if !seen_connect_src {
        result.push(format!("connect-src 'self' {needle}"));
    }
    Ok(format!("{};", result.join("; ")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_result;

    fn parse(raw: &str) -> Result<String, ApiOriginError> {
        ApiOrigin::parse(raw).map(|origin| origin.as_str().to_owned())
    }

    fn reject(raw: &str) -> ApiOriginError {
        match ApiOrigin::parse(raw) {
            Ok(origin) => panic!("expected {raw} to be rejected, got {}", origin.as_str()),
            Err(error) => error,
        }
    }

    #[test]
    fn accepts_and_normalizes_https_origins() {
        assert_eq!(
            test_result(parse("https://API.SafiData.app")),
            "https://api.safidata.app"
        );
        assert_eq!(
            test_result(parse("  HTTPS://api.safidata.app:443  ")),
            "https://api.safidata.app"
        );
        assert_eq!(
            test_result(parse("https://api.safidata.app:8443")),
            "https://api.safidata.app:8443"
        );
    }

    #[test]
    fn rejects_missing_and_non_https_values() {
        assert_eq!(parse(""), Err(ApiOriginError::Missing));
        assert_eq!(parse("   "), Err(ApiOriginError::Missing));
        assert_eq!(
            parse("http://api.safidata.app"),
            Err(ApiOriginError::NotHttps)
        );
        assert_eq!(
            parse("ftp://api.safidata.app"),
            Err(ApiOriginError::NotHttps)
        );
        assert_eq!(parse("https://"), Err(ApiOriginError::NotAnOrigin));
    }

    #[test]
    fn rejects_components_that_are_not_an_origin() {
        for value in [
            "https://api.safidata.app/",
            "https://api.safidata.app/v1",
            "https://api.safidata.app?x=1",
            "https://api.safidata.app#fragment",
            "https://user:pass@api.safidata.app",
        ] {
            assert_eq!(parse(value), Err(ApiOriginError::NotAnOrigin), "{value}");
        }
    }

    #[test]
    fn rejects_ip_addresses_and_localhost() {
        for value in [
            "https://127.0.0.1",
            "https://10.0.0.5:8443",
            "https://localhost",
            "https://api.localhost",
            "https://localhost:8443",
            "https://[::1]",
        ] {
            let error = reject(value);
            assert!(
                matches!(
                    error,
                    ApiOriginError::IpAddress
                        | ApiOriginError::Localhost
                        | ApiOriginError::InvalidHost
                ),
                "{value} produced {error:?}"
            );
        }
    }

    #[test]
    fn rejects_wildcards_and_invalid_dns_labels() {
        for value in [
            "https://*.safidata.app",
            "https://api..safidata.app",
            "https://-api.safidata.app",
            "https://api-.safidata.app",
            "https://api_safidata.app",
            "https://safidata",
            "https://api.safidata.app.",
        ] {
            assert_eq!(parse(value), Err(ApiOriginError::InvalidHost), "{value}");
        }
    }

    #[test]
    fn rejects_invalid_ports() {
        for value in [
            "https://api.safidata.app:0",
            "https://api.safidata.app:65536",
            "https://api.safidata.app:",
            "https://api.safidata.app:abc",
        ] {
            assert_eq!(parse(value), Err(ApiOriginError::InvalidPort), "{value}");
        }
    }

    #[test]
    fn injects_the_origin_into_connect_src_only() {
        let origin = test_result(ApiOrigin::parse("https://api.safidata.app"));
        let csp =
            "default-src 'self'; connect-src 'self' ipc: http://ipc.localhost; object-src 'none'";
        let merged = test_result(apply_csp_origin(csp, &origin));
        assert!(
            merged
                .contains("connect-src 'self' ipc: http://ipc.localhost https://api.safidata.app")
        );
        assert!(merged.contains("default-src 'self'"));
        assert!(merged.contains("object-src 'none'"));
        assert!(!merged.contains("https: "));
        assert_eq!(test_result(apply_csp_origin(&merged, &origin)), merged);
    }

    #[test]
    fn appends_connect_src_when_absent() {
        let origin = test_result(ApiOrigin::parse("https://api.safidata.app"));
        let merged = test_result(apply_csp_origin("default-src 'self'", &origin));
        assert_eq!(
            merged,
            "default-src 'self'; connect-src 'self' https://api.safidata.app;"
        );
    }

    #[test]
    fn rejects_an_empty_policy() {
        let origin = test_result(ApiOrigin::parse("https://api.safidata.app"));
        assert_eq!(
            apply_csp_origin("  ;; ", &origin),
            Err(ApiOriginError::NotAnOrigin)
        );
    }
}
