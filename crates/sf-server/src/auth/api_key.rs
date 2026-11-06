//! API key format, generation, and verification (Section 9.1 item 11, 9.3).
//!
//! Format: `sfk_live_<16 hex prefix>_<64 hex secret>`. Only the BLAKE3 hash of
//! the secret is ever persisted; the plaintext key is returned once to its owner
//! and is never logged or stored by the server.

use std::fmt;

use subtle::ConstantTimeEq;
use uuid::Uuid;

/// Prefix of every live API key.
pub const API_KEY_PREFIX: &str = "sfk_live_";
const PREFIX_HEX_LEN: usize = 16;
const SECRET_HEX_LEN: usize = 64;

/// A freshly generated key. `plaintext` is shown once and then dropped.
pub struct GeneratedApiKey {
    plaintext: String,
    key_prefix: String,
    secret_hash: String,
}

impl GeneratedApiKey {
    /// One-time plaintext credential.
    #[must_use]
    pub fn plaintext(&self) -> &str {
        &self.plaintext
    }

    /// Indexed lookup prefix (not secret on its own).
    #[must_use]
    pub fn key_prefix(&self) -> &str {
        &self.key_prefix
    }

    /// BLAKE3 hash of the secret, hex encoded.
    #[must_use]
    pub fn secret_hash(&self) -> &str {
        &self.secret_hash
    }
}

impl fmt::Debug for GeneratedApiKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GeneratedApiKey")
            .field("key_prefix", &self.key_prefix)
            .field("plaintext", &"[REDACTED]")
            .field("secret_hash", &"[REDACTED]")
            .finish()
    }
}

/// Generate a high-entropy API key with an indexed prefix and hashed secret.
#[must_use]
pub fn generate_api_key() -> GeneratedApiKey {
    let key_prefix = Uuid::new_v4().simple().to_string()[..PREFIX_HEX_LEN].to_owned();
    let secret = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let secret_hash = hash_secret(&secret);
    let plaintext = format!("{API_KEY_PREFIX}{key_prefix}_{secret}");
    GeneratedApiKey {
        plaintext,
        key_prefix,
        secret_hash,
    }
}

/// BLAKE3 hash of a secret, hex encoded (never logged).
#[must_use]
pub fn hash_secret(secret: &str) -> String {
    blake3::hash(secret.as_bytes()).to_hex().to_string()
}

/// Split a presented key into `(prefix, secret)`; rejects malformed input early.
#[must_use]
pub fn parse_api_key(raw: &str) -> Option<(&str, &str)> {
    let rest = raw.strip_prefix(API_KEY_PREFIX)?;
    let (key_prefix, secret) = rest.split_once('_')?;
    if key_prefix.len() != PREFIX_HEX_LEN || secret.len() != SECRET_HEX_LEN {
        return None;
    }
    if !is_lower_hex(key_prefix) || !is_lower_hex(secret) {
        return None;
    }
    Some((key_prefix, secret))
}

/// Constant-time verification of a presented secret against a stored hash.
#[must_use]
pub fn verify_secret(secret: &str, expected_hash: &str) -> bool {
    let computed = hash_secret(secret);
    computed.as_bytes().ct_eq(expected_hash.as_bytes()).into()
}

fn is_lower_hex(value: &str) -> bool {
    value
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_keys_have_canonical_shape_and_unique_prefixes() {
        let first = generate_api_key();
        let second = generate_api_key();

        assert!(first.plaintext().starts_with(API_KEY_PREFIX));
        assert_eq!(first.key_prefix().len(), PREFIX_HEX_LEN);
        assert_ne!(first.key_prefix(), second.key_prefix());

        let (prefix, secret) = parse_api_key(first.plaintext()).expect("generated key must parse");
        assert_eq!(prefix, first.key_prefix());
        assert!(verify_secret(secret, first.secret_hash()));
    }

    #[test]
    fn tampered_or_malformed_keys_are_rejected() {
        let key = generate_api_key();
        let (_, secret) = parse_api_key(key.plaintext()).expect("generated key must parse");

        assert!(parse_api_key("sfk_live_nothex_short").is_none());
        assert!(parse_api_key("bearer abc").is_none());
        assert!(!verify_secret(&format!("{secret}0"), key.secret_hash()));
    }

    #[test]
    fn debug_never_exposes_the_secret() {
        let key = generate_api_key();
        let rendered = format!("{key:?}");
        assert!(rendered.contains("[REDACTED]"));
        assert!(!rendered.contains(key.plaintext()));
    }
}
