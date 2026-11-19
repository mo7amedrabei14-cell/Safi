//! Local Encrypted Vault using audited Argon2id key derivation and ChaCha20-Poly1305 AEAD
//! with automatic key zeroization on drop (Section 8.3, H4, AGENTS.md Rule 5).

use argon2::{Algorithm, Argon2, Params, Version};
use ring::aead::{Aad, CHACHA20_POLY1305, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use ring::rand::{SecureRandom, SystemRandom};
use zeroize::Zeroize;

const VAULT_ENVELOPE_VERSION: u8 = 1;
const VAULT_AAD: &[u8] = b"safidata.vault.v1";
const TAG_LEN: usize = 16;

/// Argon2id parameters: 19 MiB memory, 2 iterations, 1 lane, 32-byte key (OWASP interactive floor).
const ARGON2_M_COST_KIB: u32 = 19 * 1024;
const ARGON2_T_COST: u32 = 2;
const ARGON2_P_COST: u32 = 1;

#[derive(Clone)]
pub struct VaultKey {
    bytes: [u8; 32],
}

impl std::fmt::Debug for VaultKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VaultKey")
            .field("bytes", &"[REDACTED]")
            .finish()
    }
}

impl VaultKey {
    /// Derive a 256-bit vault encryption key from a passphrase and >= 16-byte salt using Argon2id.
    pub fn derive_from_passphrase(passphrase: &str, salt: &[u8]) -> Result<Self, &'static str> {
        if passphrase.chars().count() < 8 {
            return Err("SF-SYS-020: كلمة سر الخزنة قصيرة جداً (8 أحرف على الأقل)");
        }
        if salt.len() < 16 {
            return Err("SF-SYS-021: ملح التشفير (Salt) غير صالح أو قصير");
        }

        let params = Params::new(ARGON2_M_COST_KIB, ARGON2_T_COST, ARGON2_P_COST, Some(32))
            .map_err(|_| "SF-SYS-021: معاملات اشتقاق المفتاح غير صالحة")?;
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let mut derived = [0u8; 32];
        argon2
            .hash_password_into(passphrase.as_bytes(), salt, &mut derived)
            .map_err(|_| "SF-SYS-021: تعذّر اشتقاق مفتاح الخزنة")?;

        Ok(Self { bytes: derived })
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }
}

impl Drop for VaultKey {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

/// Encrypt a vault payload with ChaCha20-Poly1305 AEAD and a cryptographically random 96-bit nonce.
/// Wire format: `[version: 1 byte][nonce: 12 bytes][ciphertext || Poly1305 tag: len + 16 bytes]`.
pub fn encrypt_vault_payload(key: &VaultKey, plaintext: &[u8]) -> Result<Vec<u8>, &'static str> {
    if plaintext.is_empty() {
        return Err("SF-SYS-022: بيانات الخزنة فارغة");
    }

    let rng = SystemRandom::new();
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rng.fill(&mut nonce_bytes)
        .map_err(|_| "SF-SYS-023: تعذّر توليد رقم عشوائي للتشفير")?;

    let unbound = UnboundKey::new(&CHACHA20_POLY1305, key.as_bytes())
        .map_err(|_| "SF-SYS-024: مفتاح التشفير غير صالح")?;
    let sealing_key = LessSafeKey::new(unbound);

    let mut in_out = plaintext.to_vec();
    sealing_key
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce_bytes),
            Aad::from(VAULT_AAD),
            &mut in_out,
        )
        .map_err(|_| "SF-SYS-023: فشل تشفير بيانات الخزنة")?;

    let mut envelope = Vec::with_capacity(1 + NONCE_LEN + in_out.len());
    envelope.push(VAULT_ENVELOPE_VERSION);
    envelope.extend_from_slice(&nonce_bytes);
    envelope.extend_from_slice(&in_out);
    Ok(envelope)
}

/// Authenticate and decrypt a vault payload encrypted by [`encrypt_vault_payload`].
pub fn decrypt_vault_payload(key: &VaultKey, ciphertext: &[u8]) -> Result<Vec<u8>, &'static str> {
    if ciphertext.len() <= 1 + NONCE_LEN + TAG_LEN {
        return Err("SF-SYS-023: حمولة الخزنة تالفة أو غير صالحة");
    }
    if ciphertext[0] != VAULT_ENVELOPE_VERSION {
        return Err("SF-SYS-023: إصدار تشفير الخزنة غير مدعوم");
    }

    let mut nonce_bytes = [0u8; NONCE_LEN];
    nonce_bytes.copy_from_slice(&ciphertext[1..1 + NONCE_LEN]);
    let mut encrypted = ciphertext[1 + NONCE_LEN..].to_vec();

    let unbound = UnboundKey::new(&CHACHA20_POLY1305, key.as_bytes())
        .map_err(|_| "SF-SYS-024: مفتاح فك التشفير غير صالح")?;
    let opening_key = LessSafeKey::new(unbound);

    let plaintext = opening_key
        .open_in_place(
            Nonce::assume_unique_for_key(nonce_bytes),
            Aad::from(VAULT_AAD),
            &mut encrypted,
        )
        .map_err(|_| "SF-SYS-024: مفتاح فك التشفير غير صحيح أو البيانات تعرضت للتلاعب")?;

    Ok(plaintext.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vault_key_derivation_and_encryption() {
        let salt = [1u8; 16];
        let key = crate::test_result(VaultKey::derive_from_passphrase(
            "secure-master-pass",
            &salt,
        ));
        let plain = b"SafiData Secret Local Storage Data";

        let cipher1 = crate::test_result(encrypt_vault_payload(&key, plain));
        let cipher2 = crate::test_result(encrypt_vault_payload(&key, plain));
        // Random nonce guarantees semantic security (IND-CPA) and no key/plaintext leak.
        assert_ne!(cipher1, cipher2);
        assert!(!cipher1.windows(plain.len()).any(|w| w == plain));
        assert!(!cipher1.windows(32).any(|w| w == key.as_bytes()));

        let decrypted = crate::test_result(decrypt_vault_payload(&key, &cipher1));
        assert_eq!(decrypted, plain);
    }

    #[test]
    fn test_vault_wrong_key_and_tampering_rejection() {
        let salt = [1u8; 16];
        let key1 = crate::test_result(VaultKey::derive_from_passphrase(
            "secure-master-pass-1",
            &salt,
        ));
        let key2 = crate::test_result(VaultKey::derive_from_passphrase(
            "secure-master-pass-2",
            &salt,
        ));
        let plain = b"Confidential Data";

        let mut cipher = crate::test_result(encrypt_vault_payload(&key1, plain));
        assert!(decrypt_vault_payload(&key2, &cipher).is_err());

        // Flip one bit in the ciphertext -> Poly1305 authentication must fail.
        let last_idx = cipher.len() - 1;
        cipher[last_idx] ^= 0x01;
        assert!(decrypt_vault_payload(&key1, &cipher).is_err());
    }

    #[test]
    fn test_vault_debug_redacts_key_material() {
        let salt = [9u8; 16];
        let key = crate::test_result(VaultKey::derive_from_passphrase(
            "another-strong-passphrase",
            &salt,
        ));
        let debug = format!("{key:?}");
        assert!(debug.contains("[REDACTED]"));
    }
}
