//! Session token format and hashing. Tokens are 256-bit random values issued
//! once; PostgreSQL stores only their BLAKE3 hash, and Valkey caches the
//! session record under that hash.

use std::fmt;

use serde::Serialize;
use uuid::Uuid;

use super::api_key::hash_secret;

/// Prefix of every session token.
pub const SESSION_TOKEN_PREFIX: &str = "sfs_";
const TOKEN_HEX_LEN: usize = 64;

/// A freshly issued session credential.
pub struct IssuedSessionToken {
    plaintext: String,
    token_hash: String,
}

impl IssuedSessionToken {
    /// One-time plaintext token returned to the client.
    #[must_use]
    pub fn plaintext(&self) -> &str {
        &self.plaintext
    }

    /// Hash persisted in PostgreSQL and used as the Valkey key.
    #[must_use]
    pub fn token_hash(&self) -> &str {
        &self.token_hash
    }
}

impl fmt::Debug for IssuedSessionToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("IssuedSessionToken")
            .field("plaintext", &"[REDACTED]")
            .field("token_hash", &"[REDACTED]")
            .finish()
    }
}

/// Generate a 256-bit session token and its storage hash.
#[must_use]
pub fn generate_session_token() -> IssuedSessionToken {
    let plaintext = format!(
        "{SESSION_TOKEN_PREFIX}{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let token_hash = hash_secret(&plaintext);
    IssuedSessionToken {
        plaintext,
        token_hash,
    }
}

/// Validate the presented token and return its hash for lookup.
#[must_use]
pub fn parse_session_token(raw: &str) -> Option<&str> {
    let secret = raw.strip_prefix(SESSION_TOKEN_PREFIX)?;
    if secret.len() != TOKEN_HEX_LEN {
        return None;
    }
    if !secret
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    Some(raw)
}

/// Hash a presented token for cache and database lookup.
#[must_use]
pub fn token_hash(raw: &str) -> String {
    hash_secret(raw)
}

/// Public description of a session, safe to return to its owner.
#[derive(Debug, Clone, Serialize)]
pub struct SessionView {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub device_name: String,
    pub expires_at: i64,
    pub created_at: i64,
    pub last_seen_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_tokens_are_prefixed_hashed_and_parseable() {
        let issued = generate_session_token();
        assert!(issued.plaintext().starts_with(SESSION_TOKEN_PREFIX));
        assert_eq!(issued.token_hash().len(), 64);
        assert_eq!(
            parse_session_token(issued.plaintext()),
            Some(issued.plaintext())
        );
        assert_eq!(token_hash(issued.plaintext()), issued.token_hash());
    }

    #[test]
    fn malformed_session_tokens_are_rejected() {
        assert!(parse_session_token("sfs_short").is_none());
        assert!(parse_session_token("sfk_live_abc").is_none());
        assert!(parse_session_token(&format!("sfs_{}", "Z".repeat(64))).is_none());
    }

    #[test]
    fn debug_never_exposes_the_token() {
        let issued = generate_session_token();
        let rendered = format!("{issued:?}");
        assert!(rendered.contains("[REDACTED]"));
        assert!(!rendered.contains(issued.plaintext()));
    }
}
