//! API-key authentication and high-performance session management (Section 9.1, 9.3).

pub mod account;
pub mod api_key;
pub mod cache;
pub mod middleware;
pub mod repository;
pub mod service;
pub mod session;

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Permission scope carried by an API key or a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// Read workspace metadata and reports.
    Read,
    /// Create or modify workspace data.
    Write,
    /// Manage keys, sessions, and members.
    Admin,
}

impl Scope {
    /// Parse one canonical scope name.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "read" => Some(Self::Read),
            "write" => Some(Self::Write),
            "admin" => Some(Self::Admin),
            _ => None,
        }
    }

    /// Canonical stored name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Admin => "admin",
        }
    }

    /// Whether holding `self` satisfies a requirement for `required`.
    #[must_use]
    pub const fn allows(self, required: Self) -> bool {
        match required {
            Self::Read => true,
            Self::Write => matches!(self, Self::Write | Self::Admin),
            Self::Admin => matches!(self, Self::Admin),
        }
    }
}

/// Parse a stored scope array, dropping unknown values and duplicates.
#[must_use]
pub fn parse_scopes(values: &[String]) -> Vec<Scope> {
    let mut scopes = Vec::new();
    for value in values {
        if let Some(scope) = Scope::parse(value) {
            if !scopes.contains(&scope) {
                scopes.push(scope);
            }
        }
    }
    scopes
}

/// Serialize scopes back to their canonical storage form.
#[must_use]
pub fn scopes_to_strings(scopes: &[Scope]) -> Vec<String> {
    scopes
        .iter()
        .map(|scope| scope.as_str().to_owned())
        .collect()
}

/// How the caller proved its identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum AuthMethod {
    /// Long-lived, scoped API key.
    ApiKey { key_id: Uuid },
    /// Short-lived session issued from an API key.
    Session { session_id: Uuid },
    /// Email/password sign-in before a new session is issued.
    Password,
}

/// Authenticated caller, always bound to exactly one workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Principal {
    pub user_id: Uuid,
    pub workspace_id: Uuid,
    pub scopes: Vec<Scope>,
    pub auth_method: AuthMethod,
}

impl Principal {
    /// Whether this caller may perform an action requiring `required`.
    #[must_use]
    pub fn has_scope(&self, required: Scope) -> bool {
        self.scopes.iter().any(|scope| scope.allows(required))
    }
}

/// Evidence attached to the request after the authentication middleware runs.
#[derive(Debug, Clone)]
pub struct AuthenticatedSession {
    pub principal: Principal,
    pub session_id: Uuid,
    pub device_name: String,
    pub expires_at: i64,
    /// Hash of the presented token, used to evict the exact Valkey entry on revoke.
    pub token_hash: String,
}

/// Current UNIX time in seconds.
#[must_use]
pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64)
}
