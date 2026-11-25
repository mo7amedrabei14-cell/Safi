//! Production-grade environment configuration, fail-closed startup validation,
//! and secret-safe integration readiness matrix (`SF-SYS-032`).
//!
//! Security invariants:
//! - Secret values are never printed in `Debug`, logs, or `/ready` responses.
//! - Production mode (`SAFI_ENV=production`) fails closed when mandatory security
//!   or persistence configuration is absent or malformed.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// Deployment environment classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AppEnvironment {
    Development,
    Test,
    Staging,
    Production,
}

impl AppEnvironment {
    #[must_use]
    pub fn parse(raw: Option<&str>) -> Self {
        match raw.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("production") | Some("prod") => Self::Production,
            Some("staging") | Some("stage") => Self::Staging,
            Some("test") => Self::Test,
            _ => Self::Development,
        }
    }

    #[must_use]
    pub const fn is_strict(self) -> bool {
        matches!(self, Self::Production | Self::Staging)
    }
}

/// Readiness state of an external integration or cryptographic capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IntegrationStatus {
    Configured,
    NotConfigured,
    Pending,
}

/// Secret-safe readiness summary exposed by `/ready` and `--check-config`.
#[derive(Debug, Clone, Serialize)]
pub struct ReadinessReport {
    pub environment: AppEnvironment,
    pub ready: bool,
    pub missing_required: Vec<String>,
    pub integrations: BTreeMap<String, IntegrationStatus>,
}

/// Validated runtime configuration loaded from process environment variables.
#[derive(Clone)]
pub struct ServerConfig {
    pub environment: AppEnvironment,
    pub bind_addr: String,
    pub api_base_url: Option<String>,
    pub public_web_origin: Option<String>,
    pub database_url: Option<String>,
    pub valkey_url: Option<String>,
    pub totp_encryption_key: Option<[u8; 32]>,
    pub totp_key_version: i32,
    pub totp_issuer: String,
    pub license_signing_seed: Option<[u8; 32]>,
    pub update_signing_seed: Option<[u8; 32]>,
    pub token_signing_key: Option<[u8; 32]>,
    pub smtp_configured: bool,
    pub sms_configured: bool,
    pub whatsapp_configured: bool,
    pub oauth_google_configured: bool,
    pub oauth_apple_configured: bool,
    pub oauth_github_configured: bool,
    pub paddle_api_key: Option<String>,
    pub paddle_webhook_secret: Option<String>,
    pub paddle_api_base_url: String,
    pub paddle_price_individual_monthly: Option<String>,
    pub paddle_price_individual_annual: Option<String>,
    pub paddle_price_org_seat_annual: Option<String>,
    pub paddle_price_pay_per_file: Option<String>,
    pub paddle_price_credit_pack_25: Option<String>,
    pub paymob_api_key: Option<String>,
    pub paymob_public_key: Option<String>,
    pub paymob_hmac_secret: Option<String>,
    pub paymob_integration_id: Option<u64>,
    pub paymob_api_base_url: String,
    pub ai_provider_url: Option<String>,
    pub ai_api_key: Option<String>,
    pub ai_model: Option<String>,
    pub otlp_configured: bool,
    pub glitchtip_configured: bool,
}

impl fmt::Debug for ServerConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServerConfig")
            .field("environment", &self.environment)
            .field("bind_addr", &self.bind_addr)
            .field("api_base_url", &self.api_base_url)
            .field("public_web_origin", &self.public_web_origin)
            .field("database_configured", &self.database_url.is_some())
            .field("valkey_configured", &self.valkey_url.is_some())
            .field("totp_key_configured", &self.totp_encryption_key.is_some())
            .field("totp_key_version", &self.totp_key_version)
            .field(
                "license_key_configured",
                &self.license_signing_seed.is_some(),
            )
            .field("update_key_configured", &self.update_signing_seed.is_some())
            .field("paddle_configured", &self.paddle_configured())
            .field("paymob_configured", &self.paymob_configured())
            .finish()
    }
}

fn non_empty_var(getter: &impl Fn(&str) -> Option<String>, key: &str) -> Option<String> {
    getter(key).and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed.starts_with("CHANGE_ME_") {
            None
        } else {
            Some(trimmed.to_owned())
        }
    })
}

/// Decode a 64-character hex string into a fixed 32-byte secret key.
pub fn parse_hex_32(raw: &str) -> Result<[u8; 32], &'static str> {
    let bytes = hex::decode(raw.trim()).map_err(|_| "SF-SYS-032: invalid hex key material")?;
    if bytes.len() != 32 {
        return Err("SF-SYS-032: key material must be exactly 32 bytes (64 hex chars)");
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn is_reserved_public_host(host: &str) -> bool {
    matches!(
        host,
        "invalid"
            | "test"
            | "example"
            | "local"
            | "internal"
            | "example.com"
            | "example.org"
            | "example.net"
    ) || host.ends_with(".invalid")
        || host.ends_with(".test")
        || host.ends_with(".example")
        || host.ends_with(".local")
        || host.ends_with(".internal")
        || host.ends_with(".arpa")
        || host.ends_with(".example.com")
        || host.ends_with(".example.org")
        || host.ends_with(".example.net")
}

/// Strictly validate a public origin environment variable (`SAFI_PUBLIC_WEB_ORIGIN` or
/// `SAFI_API_BASE_URL`).
///
/// - Rejects any path, query string, fragment, or user-info (`@`).
/// - In strict (`staging` / `production`) environments: requires `https://`, a multi-label valid
///   DNS host, non-localhost, non-IP literal, and non-reserved placeholder domain.
/// - In `development` / `test`: permits valid `https://` origins OR explicit loopback `http://127.0.0.1:<port>`
///   / `http://localhost:<port>`.
pub fn validate_public_origin(var_name: &str, raw: &str, strict: bool) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(format!("SF-SYS-032: {var_name} must not be empty"));
    }
    let lower = trimmed.to_ascii_lowercase();
    let (scheme, rest) = if let Some(r) = lower.strip_prefix("https://") {
        ("https", r)
    } else if !strict {
        if let Some(r) = lower.strip_prefix("http://") {
            ("http", r)
        } else {
            return Err(format!("SF-SYS-032: {var_name} must start with https://"));
        }
    } else {
        return Err(format!(
            "SF-SYS-032: {var_name} must use https:// in staging/production"
        ));
    };

    if rest.is_empty()
        || rest
            .chars()
            .any(|c| matches!(c, '/' | '?' | '#' | '@' | '\\') || c.is_whitespace())
    {
        return Err(format!(
            "SF-SYS-032: {var_name} must be an origin without path, query, fragment, or credentials"
        ));
    }

    let (host, port_raw) = match rest.rsplit_once(':') {
        Some((h, p)) => (h, Some(p)),
        None => (rest, None),
    };
    if host.is_empty() || host.starts_with('[') || host.contains(':') {
        return Err(format!("SF-SYS-032: {var_name} has an invalid host"));
    }

    let port = match port_raw {
        Some(p) => {
            if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
                return Err(format!("SF-SYS-032: {var_name} has an invalid port"));
            }
            let parsed: u16 = p
                .parse()
                .map_err(|_| format!("SF-SYS-032: {var_name} port out of range"))?;
            if parsed == 0 {
                return Err(format!("SF-SYS-032: {var_name} port out of range"));
            }
            if (scheme == "https" && parsed == 443) || (scheme == "http" && parsed == 80) {
                None
            } else {
                Some(parsed)
            }
        }
        None => None,
    };

    if scheme == "http" {
        if host != "127.0.0.1" && host != "localhost" {
            return Err(format!(
                "SF-SYS-032: http:// {var_name} is only permitted for loopback dev/test"
            ));
        }
    } else {
        if host == "localhost" || host.ends_with(".localhost") {
            return Err(format!(
                "SF-SYS-032: {var_name} must not point at localhost"
            ));
        }
        if host.chars().all(|c| c.is_ascii_digit() || c == '.') {
            return Err(format!("SF-SYS-032: {var_name} must not be an IP literal"));
        }
        let valid_labels = host.contains('.')
            && host.split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && !label.starts_with('-')
                    && !label.ends_with('-')
                    && label
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            });
        if !valid_labels {
            return Err(format!(
                "SF-SYS-032: {var_name} must use a valid multi-label DNS host"
            ));
        }
        if strict && is_reserved_public_host(host) {
            return Err(format!(
                "SF-SYS-032: {var_name} must not use a reserved placeholder domain in staging/production"
            ));
        }
    }

    Ok(match port {
        Some(p) => format!("{scheme}://{host}:{p}"),
        None => format!("{scheme}://{host}"),
    })
}

/// Strictly validate `SAFI_PUBLIC_WEB_ORIGIN` (e.g. `https://safidata.app`).
pub fn validate_public_web_origin(raw: &str, strict: bool) -> Result<String, String> {
    validate_public_origin("SAFI_PUBLIC_WEB_ORIGIN", raw, strict)
}

/// Strictly validate `SAFI_API_BASE_URL` (e.g. `https://api.safidata.app`).
pub fn validate_api_base_url(raw: &str, strict: bool) -> Result<String, String> {
    validate_public_origin("SAFI_API_BASE_URL", raw, strict)
}

impl ServerConfig {
    /// Load configuration from real OS environment variables.
    pub fn from_env() -> Result<Self, String> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    /// Load configuration from a deterministic lookup function (used in unit/security tests).
    pub fn from_lookup<F>(lookup: F) -> Result<Self, String>
    where
        F: Fn(&str) -> Option<String>,
    {
        let environment = AppEnvironment::parse(lookup("SAFI_ENV").as_deref());
        let bind_addr =
            non_empty_var(&lookup, "SAFI_BIND_ADDR").unwrap_or_else(|| "127.0.0.1:8080".to_owned());
        let api_base_url = match non_empty_var(&lookup, "SAFI_API_BASE_URL") {
            Some(raw) => Some(validate_api_base_url(&raw, environment.is_strict())?),
            None => None,
        };
        let public_web_origin = match non_empty_var(&lookup, "SAFI_PUBLIC_WEB_ORIGIN") {
            Some(raw) => Some(validate_public_web_origin(&raw, environment.is_strict())?),
            None => None,
        };
        let database_url = non_empty_var(&lookup, "DATABASE_URL");
        let valkey_url = non_empty_var(&lookup, "SAFI_VALKEY_URL");

        let totp_encryption_key = match non_empty_var(&lookup, "SAFI_TOTP_ENCRYPTION_KEY") {
            Some(raw) => {
                Some(parse_hex_32(&raw).map_err(|e| format!("SAFI_TOTP_ENCRYPTION_KEY: {e}"))?)
            }
            None => None,
        };
        let totp_key_version = non_empty_var(&lookup, "SAFI_TOTP_KEY_VERSION")
            .and_then(|v| v.parse::<i32>().ok())
            .filter(|v| *v >= 1)
            .unwrap_or(1);
        let totp_issuer = non_empty_var(&lookup, "SAFI_OWNER_TOTP_ISSUER")
            .unwrap_or_else(|| "SafiData".to_owned());

        let license_signing_seed = match non_empty_var(&lookup, "SAFI_LICENSE_SIGNING_PRIVATE_KEY")
        {
            Some(raw) => Some(
                parse_hex_32(&raw).map_err(|e| format!("SAFI_LICENSE_SIGNING_PRIVATE_KEY: {e}"))?,
            ),
            None => None,
        };
        let update_signing_seed = match non_empty_var(&lookup, "SAFI_UPDATE_SIGNING_PRIVATE_KEY") {
            Some(raw) => Some(
                parse_hex_32(&raw).map_err(|e| format!("SAFI_UPDATE_SIGNING_PRIVATE_KEY: {e}"))?,
            ),
            None => None,
        };
        let token_signing_key = match non_empty_var(&lookup, "SAFI_TOKEN_SIGNING_KEY") {
            Some(raw) => {
                Some(parse_hex_32(&raw).map_err(|e| format!("SAFI_TOKEN_SIGNING_KEY: {e}"))?)
            }
            None => None,
        };

        let smtp_configured = non_empty_var(&lookup, "SAFI_SMTP_HOST").is_some()
            && non_empty_var(&lookup, "SAFI_SMTP_FROM").is_some();
        let sms_configured = non_empty_var(&lookup, "SAFI_SMS_PROVIDER").is_some()
            && non_empty_var(&lookup, "SAFI_SMS_API_KEY").is_some();
        let whatsapp_configured = non_empty_var(&lookup, "SAFI_WHATSAPP_PROVIDER").is_some()
            && non_empty_var(&lookup, "SAFI_WHATSAPP_API_KEY").is_some();

        let oauth_google_configured = non_empty_var(&lookup, "SAFI_OAUTH_GOOGLE_CLIENT_ID")
            .is_some()
            && non_empty_var(&lookup, "SAFI_OAUTH_GOOGLE_CLIENT_SECRET").is_some()
            && non_empty_var(&lookup, "SAFI_OAUTH_GOOGLE_REDIRECT_URI").is_some();
        let oauth_apple_configured = non_empty_var(&lookup, "SAFI_OAUTH_APPLE_CLIENT_ID").is_some()
            && non_empty_var(&lookup, "SAFI_OAUTH_APPLE_TEAM_ID").is_some()
            && non_empty_var(&lookup, "SAFI_OAUTH_APPLE_KEY_ID").is_some()
            && non_empty_var(&lookup, "SAFI_OAUTH_APPLE_PRIVATE_KEY_PEM").is_some();
        let oauth_github_configured = non_empty_var(&lookup, "SAFI_OAUTH_GITHUB_CLIENT_ID")
            .is_some()
            && non_empty_var(&lookup, "SAFI_OAUTH_GITHUB_CLIENT_SECRET").is_some()
            && non_empty_var(&lookup, "SAFI_OAUTH_GITHUB_REDIRECT_URI").is_some();

        let paddle_api_key = non_empty_var(&lookup, "SAFI_PADDLE_API_KEY");
        let paddle_webhook_secret = non_empty_var(&lookup, "SAFI_PADDLE_WEBHOOK_SECRET");
        let paddle_api_base_url = non_empty_var(&lookup, "SAFI_PADDLE_API_BASE_URL")
            .unwrap_or_else(|| "https://api.paddle.com".to_owned());
        let paddle_price_individual_monthly =
            non_empty_var(&lookup, "SAFI_PADDLE_PRICE_INDIVIDUAL_MONTHLY");
        let paddle_price_individual_annual =
            non_empty_var(&lookup, "SAFI_PADDLE_PRICE_INDIVIDUAL_ANNUAL");
        let paddle_price_org_seat_annual =
            non_empty_var(&lookup, "SAFI_PADDLE_PRICE_ORG_SEAT_ANNUAL");
        let paddle_price_pay_per_file = non_empty_var(&lookup, "SAFI_PADDLE_PRICE_PAY_PER_FILE");
        let paddle_price_credit_pack_25 =
            non_empty_var(&lookup, "SAFI_PADDLE_PRICE_CREDIT_PACK_25");
        let paymob_api_key = non_empty_var(&lookup, "SAFI_PAYMOB_API_KEY");
        let paymob_public_key = non_empty_var(&lookup, "SAFI_PAYMOB_PUBLIC_KEY");
        let paymob_hmac_secret = non_empty_var(&lookup, "SAFI_PAYMOB_HMAC_SECRET");
        let paymob_integration_id = non_empty_var(&lookup, "SAFI_PAYMOB_INTEGRATION_ID")
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|v| *v > 0);
        let paymob_api_base_url = non_empty_var(&lookup, "SAFI_PAYMOB_API_BASE_URL")
            .unwrap_or_else(|| "https://accept.paymob.com".to_owned());

        let ai_provider_url = non_empty_var(&lookup, "SAFI_AI_PROVIDER_URL");
        let ai_api_key = non_empty_var(&lookup, "SAFI_AI_API_KEY");
        let ai_model = non_empty_var(&lookup, "SAFI_AI_MODEL");

        let otlp_configured = non_empty_var(&lookup, "SAFI_OTLP_ENDPOINT").is_some();
        let glitchtip_configured = non_empty_var(&lookup, "SAFI_GLITCHTIP_DSN").is_some();

        Ok(Self {
            environment,
            bind_addr,
            api_base_url,
            public_web_origin,
            database_url,
            valkey_url,
            totp_encryption_key,
            totp_key_version,
            totp_issuer,
            license_signing_seed,
            update_signing_seed,
            token_signing_key,
            smtp_configured,
            sms_configured,
            whatsapp_configured,
            oauth_google_configured,
            oauth_apple_configured,
            oauth_github_configured,
            paddle_api_key,
            paddle_webhook_secret,
            paddle_api_base_url,
            paddle_price_individual_monthly,
            paddle_price_individual_annual,
            paddle_price_org_seat_annual,
            paddle_price_pay_per_file,
            paddle_price_credit_pack_25,
            paymob_api_key,
            paymob_public_key,
            paymob_hmac_secret,
            paymob_integration_id,
            paymob_api_base_url,
            ai_provider_url,
            ai_api_key,
            ai_model,
            otlp_configured,
            glitchtip_configured,
        })
    }

    #[must_use]
    pub fn paddle_configured(&self) -> bool {
        self.paddle_api_key.is_some() && self.paddle_webhook_secret.is_some()
    }

    #[must_use]
    pub fn paymob_configured(&self) -> bool {
        self.paymob_api_key.is_some()
            && self.paymob_public_key.is_some()
            && self.paymob_hmac_secret.is_some()
            && self.paymob_integration_id.is_some()
    }

    #[must_use]
    pub fn ai_configured(&self) -> bool {
        self.ai_provider_url.is_some() && self.ai_api_key.is_some() && self.ai_model.is_some()
    }

    /// Resolve the approved API origin (`SAFI_API_BASE_URL`) used for server-side webhook
    /// callbacks and API metadata.
    ///
    /// - Never falls back to `SAFI_PUBLIC_WEB_ORIGIN` or `localhost` in strict staging/production.
    pub fn resolved_api_base_url(&self) -> Result<&str, crate::error::AuthError> {
        if let Some(origin) = self.api_base_url.as_deref() {
            return Ok(origin);
        }
        if self.environment.is_strict() {
            return Err(crate::error::AuthError::BackendUnavailable);
        }
        Ok("https://api.safidata.app")
    }

    /// Resolve the approved public website origin used for browser billing/auth redirects.
    ///
    /// - Never falls back to `SAFI_API_BASE_URL` in strict staging/production builds.
    /// - In non-strict local tests/dev, uses `public_web_origin` first (defaulting to
    ///   `https://safidata.app` when omitted).
    pub fn resolved_public_web_origin(&self) -> Result<&str, crate::error::AuthError> {
        if let Some(origin) = self.public_web_origin.as_deref() {
            return Ok(origin);
        }
        if self.environment.is_strict() {
            return Err(crate::error::AuthError::BackendUnavailable);
        }
        Ok("https://safidata.app")
    }

    /// Build a secret-safe readiness report showing every integration's status.
    #[must_use]
    pub fn readiness_report(&self) -> ReadinessReport {
        let mut missing_required = Vec::new();
        if self.environment.is_strict() {
            if self.api_base_url.is_none() {
                missing_required.push("SAFI_API_BASE_URL".to_owned());
            }
            if self.public_web_origin.is_none() {
                missing_required.push("SAFI_PUBLIC_WEB_ORIGIN".to_owned());
            }
            if self.database_url.is_none() {
                missing_required.push("DATABASE_URL".to_owned());
            }
            if self.valkey_url.is_none() {
                missing_required.push("SAFI_VALKEY_URL".to_owned());
            }
            if self.totp_encryption_key.is_none() {
                missing_required.push("SAFI_TOTP_ENCRYPTION_KEY".to_owned());
            }
            if self.license_signing_seed.is_none() {
                missing_required.push("SAFI_LICENSE_SIGNING_PRIVATE_KEY".to_owned());
            }
            if self.update_signing_seed.is_none() {
                missing_required.push("SAFI_UPDATE_SIGNING_PRIVATE_KEY".to_owned());
            }
            if self.token_signing_key.is_none() {
                missing_required.push("SAFI_TOKEN_SIGNING_KEY".to_owned());
            }
        }

        let flag = |ok: bool| {
            if ok {
                IntegrationStatus::Configured
            } else {
                IntegrationStatus::NotConfigured
            }
        };

        let mut integrations = BTreeMap::new();
        integrations.insert("api_base_url".to_owned(), flag(self.api_base_url.is_some()));
        integrations.insert(
            "public_web_origin".to_owned(),
            flag(self.public_web_origin.is_some()),
        );
        integrations.insert("postgresql".to_owned(), flag(self.database_url.is_some()));
        integrations.insert("valkey".to_owned(), flag(self.valkey_url.is_some()));
        integrations.insert(
            "owner_totp_encryption".to_owned(),
            flag(self.totp_encryption_key.is_some()),
        );
        integrations.insert(
            "ed25519_license_signing".to_owned(),
            flag(self.license_signing_seed.is_some()),
        );
        integrations.insert(
            "ed25519_update_signing".to_owned(),
            flag(self.update_signing_seed.is_some()),
        );
        integrations.insert("smtp_email".to_owned(), flag(self.smtp_configured));
        integrations.insert("sms_phone_otp".to_owned(), flag(self.sms_configured));
        integrations.insert(
            "whatsapp_phone_otp".to_owned(),
            flag(self.whatsapp_configured),
        );
        integrations.insert(
            "oauth_google".to_owned(),
            flag(self.oauth_google_configured),
        );
        integrations.insert("oauth_apple".to_owned(), flag(self.oauth_apple_configured));
        integrations.insert(
            "oauth_github".to_owned(),
            flag(self.oauth_github_configured),
        );
        integrations.insert("billing_paddle".to_owned(), flag(self.paddle_configured()));
        integrations.insert("billing_paymob".to_owned(), flag(self.paymob_configured()));
        integrations.insert("ai_provider".to_owned(), flag(self.ai_configured()));
        integrations.insert("observability_otlp".to_owned(), flag(self.otlp_configured));
        integrations.insert(
            "error_tracking_glitchtip".to_owned(),
            flag(self.glitchtip_configured),
        );

        ReadinessReport {
            environment: self.environment,
            ready: missing_required.is_empty(),
            missing_required,
            integrations,
        }
    }

    /// Enforce fail-closed startup in staging/production environments (`SF-SYS-032`).
    pub fn validate_for_startup(&self) -> Result<ReadinessReport, String> {
        let report = self.readiness_report();
        if !report.ready {
            return Err(format!(
                "SF-SYS-032: missing required configuration for {:?} environment: {}",
                self.environment,
                report.missing_required.join(", ")
            ));
        }
        Ok(report)
    }

    /// Deterministic test configuration with non-production test keys.
    #[must_use]
    pub fn for_tests() -> Self {
        Self {
            environment: AppEnvironment::Test,
            bind_addr: "127.0.0.1:0".to_owned(),
            api_base_url: Some("http://127.0.0.1:8080".to_owned()),
            public_web_origin: Some("https://safidata.app".to_owned()),
            database_url: None,
            valkey_url: None,
            totp_encryption_key: Some([0x42u8; 32]),
            totp_key_version: 1,
            totp_issuer: "SafiData-Test".to_owned(),
            license_signing_seed: Some([0x11u8; 32]),
            update_signing_seed: Some([0x22u8; 32]),
            token_signing_key: Some([0x33u8; 32]),
            smtp_configured: false,
            sms_configured: false,
            whatsapp_configured: false,
            oauth_google_configured: false,
            oauth_apple_configured: false,
            oauth_github_configured: false,
            paddle_api_key: None,
            paddle_webhook_secret: None,
            paddle_api_base_url: "https://api.paddle.com".to_owned(),
            paddle_price_individual_monthly: None,
            paddle_price_individual_annual: None,
            paddle_price_org_seat_annual: None,
            paddle_price_pay_per_file: None,
            paddle_price_credit_pack_25: None,
            paymob_api_key: None,
            paymob_public_key: None,
            paymob_hmac_secret: None,
            paymob_integration_id: None,
            paymob_api_base_url: "https://accept.paymob.com".to_owned(),
            ai_provider_url: None,
            ai_api_key: None,
            ai_model: None,
            otlp_configured: false,
            glitchtip_configured: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn production_fails_closed_when_required_vars_missing() {
        let env = HashMap::from([("SAFI_ENV".to_owned(), "production".to_owned())]);
        let cfg = ServerConfig::from_lookup(|k| env.get(k).cloned()).expect("parse");
        let err = cfg
            .validate_for_startup()
            .expect_err("production must fail closed");
        assert!(err.contains("SF-SYS-032"));
        assert!(err.contains("SAFI_API_BASE_URL"));
        assert!(err.contains("SAFI_PUBLIC_WEB_ORIGIN"));
        assert!(err.contains("DATABASE_URL"));
        assert!(err.contains("SAFI_VALKEY_URL"));
        assert!(err.contains("SAFI_TOTP_ENCRYPTION_KEY"));
    }

    #[test]
    fn strict_env_rejects_malformed_http_localhost_and_reserved_public_web_origin() {
        for bad in [
            "http://safidata.app",
            "https://safidata.app/billing",
            "https://user@safidata.app",
            "https://localhost",
            "https://127.0.0.1",
            "https://safidata.example",
            "https://billing.test",
            "not-a-url",
        ] {
            let env = HashMap::from([
                ("SAFI_ENV".to_owned(), "production".to_owned()),
                ("SAFI_PUBLIC_WEB_ORIGIN".to_owned(), bad.to_owned()),
            ]);
            let err = ServerConfig::from_lookup(|k| env.get(k).cloned())
                .expect_err(&format!("must reject bad origin: {bad}"));
            assert!(err.contains("SF-SYS-032"), "bad={bad} err={err}");

            let env_api = HashMap::from([
                ("SAFI_ENV".to_owned(), "production".to_owned()),
                ("SAFI_API_BASE_URL".to_owned(), bad.to_owned()),
            ]);
            let err_api = ServerConfig::from_lookup(|k| env_api.get(k).cloned())
                .expect_err(&format!("must reject bad api origin: {bad}"));
            assert!(err_api.contains("SF-SYS-032"), "bad={bad} err={err_api}");
        }
    }

    #[test]
    fn changing_public_web_origin_never_alters_api_base_url_and_vice_versa() {
        let hex32 = "bb".repeat(32);
        let make_cfg = |api: &str, web: &str| {
            let env = HashMap::from([
                ("SAFI_ENV".to_owned(), "production".to_owned()),
                ("SAFI_API_BASE_URL".to_owned(), api.to_owned()),
                ("SAFI_PUBLIC_WEB_ORIGIN".to_owned(), web.to_owned()),
                (
                    "DATABASE_URL".to_owned(),
                    "postgres://safi:pw@127.0.0.1:6432/safi".to_owned(),
                ),
                (
                    "SAFI_VALKEY_URL".to_owned(),
                    "redis://127.0.0.1:6379/0".to_owned(),
                ),
                ("SAFI_TOTP_ENCRYPTION_KEY".to_owned(), hex32.clone()),
                ("SAFI_LICENSE_SIGNING_PRIVATE_KEY".to_owned(), hex32.clone()),
                ("SAFI_UPDATE_SIGNING_PRIVATE_KEY".to_owned(), hex32.clone()),
                ("SAFI_TOKEN_SIGNING_KEY".to_owned(), hex32.clone()),
            ]);
            ServerConfig::from_lookup(|k| env.get(k).cloned()).expect("valid config")
        };

        let cfg_initial = make_cfg("https://api.safidata.app", "https://safidata.app");
        let cfg_web_changed = make_cfg("https://api.safidata.app", "https://portal.safidata.io");
        let cfg_api_changed = make_cfg("https://gateway.safidata.io", "https://safidata.app");

        assert_eq!(
            cfg_initial.resolved_api_base_url().expect("api"),
            cfg_web_changed.resolved_api_base_url().expect("api")
        );
        assert_eq!(
            cfg_web_changed.resolved_public_web_origin().expect("web"),
            "https://portal.safidata.io"
        );
        assert_eq!(
            cfg_initial.resolved_public_web_origin().expect("web"),
            cfg_api_changed.resolved_public_web_origin().expect("web")
        );
        assert_eq!(
            cfg_api_changed.resolved_api_base_url().expect("api"),
            "https://gateway.safidata.io"
        );
    }

    #[test]
    fn placeholder_change_me_values_are_treated_as_not_configured() {
        let env = HashMap::from([
            ("SAFI_ENV".to_owned(), "production".to_owned()),
            ("DATABASE_URL".to_owned(), "CHANGE_ME_DB_URL".to_owned()),
            (
                "SAFI_PUBLIC_WEB_ORIGIN".to_owned(),
                "CHANGE_ME_WEB_ORIGIN".to_owned(),
            ),
            (
                "SAFI_TOTP_ENCRYPTION_KEY".to_owned(),
                "CHANGE_ME_64_HEX_BYTES".to_owned(),
            ),
        ]);
        let cfg = ServerConfig::from_lookup(|k| env.get(k).cloned()).expect("parse");
        assert!(cfg.database_url.is_none());
        assert!(cfg.public_web_origin.is_none());
        assert!(cfg.totp_encryption_key.is_none());
        assert!(cfg.validate_for_startup().is_err());
    }

    #[test]
    fn production_passes_when_all_mandatory_security_keys_are_valid() {
        let hex32 = "aa".repeat(32);
        let env = HashMap::from([
            ("SAFI_ENV".to_owned(), "production".to_owned()),
            (
                "SAFI_API_BASE_URL".to_owned(),
                "https://api.safidata.app".to_owned(),
            ),
            (
                "SAFI_PUBLIC_WEB_ORIGIN".to_owned(),
                "https://SAFIDATA.APP:443".to_owned(),
            ),
            (
                "DATABASE_URL".to_owned(),
                "postgres://safi:pw@127.0.0.1:6432/safi".to_owned(),
            ),
            (
                "SAFI_VALKEY_URL".to_owned(),
                "redis://127.0.0.1:6379/0".to_owned(),
            ),
            ("SAFI_TOTP_ENCRYPTION_KEY".to_owned(), hex32.clone()),
            ("SAFI_LICENSE_SIGNING_PRIVATE_KEY".to_owned(), hex32.clone()),
            ("SAFI_UPDATE_SIGNING_PRIVATE_KEY".to_owned(), hex32.clone()),
            ("SAFI_TOKEN_SIGNING_KEY".to_owned(), hex32),
        ]);
        let cfg = ServerConfig::from_lookup(|k| env.get(k).cloned()).expect("parse");
        assert_eq!(
            cfg.api_base_url.as_deref(),
            Some("https://api.safidata.app")
        );
        assert_eq!(
            cfg.public_web_origin.as_deref(),
            Some("https://safidata.app")
        );
        assert_eq!(
            cfg.resolved_public_web_origin().expect("resolved"),
            "https://safidata.app"
        );
        let report = cfg.validate_for_startup().expect("valid production config");
        assert!(report.ready);
        assert_eq!(
            report.integrations.get("public_web_origin"),
            Some(&IntegrationStatus::Configured)
        );
        assert_eq!(
            report.integrations.get("billing_paddle"),
            Some(&IntegrationStatus::NotConfigured)
        );
        assert_eq!(
            report.integrations.get("owner_totp_encryption"),
            Some(&IntegrationStatus::Configured)
        );
        // Debug output must never leak database credentials or hex keys.
        let dbg = format!("{cfg:?}");
        assert!(!dbg.contains("aaaa"));
        assert!(!dbg.contains("postgres://"));
    }
}
