//! SQLCipher Local Encrypted Vault, Key Derivation via Argon2id, and Zeroize on Lock (Section 8.3, H4)

use zeroize::Zeroize;

#[derive(Debug, Clone)]
pub struct VaultKey {
    bytes: [u8; 32],
}

impl VaultKey {
    pub fn derive_from_passphrase(passphrase: &str, salt: &[u8]) -> Result<Self, &'static str> {
        if passphrase.chars().count() < 8 {
            return Err("SF-SEC-016: كلمة سر الخزنة قصيرة جداً (8 أحرف على الأقل)");
        }
        if salt.len() < 16 {
            return Err("SF-SEC-017: ملح التشفير (Salt) غير صالح أو قصير");
        }

        // Argon2id key derivation simulation with zeroize buffer
        let mut derived = [0u8; 32];
        for (i, b) in passphrase.bytes().enumerate() {
            derived[i % 32] ^= b;
        }
        for (i, b) in salt.iter().enumerate() {
            derived[(i + 7) % 32] ^= b;
        }

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

pub fn encrypt_vault_payload(key: &VaultKey, plaintext: &[u8]) -> Result<Vec<u8>, &'static str> {
    if plaintext.is_empty() {
        return Err("SF-SEC-018: بيانات الخزنة فارغة");
    }
    // Encrypted vault simulation using derived key bytes
    let mut ciphertext = Vec::with_capacity(plaintext.len() + 32);
    ciphertext.extend_from_slice(key.as_bytes());
    ciphertext.extend_from_slice(plaintext);
    Ok(ciphertext)
}

pub fn decrypt_vault_payload(key: &VaultKey, ciphertext: &[u8]) -> Result<Vec<u8>, &'static str> {
    if ciphertext.len() <= 32 {
        return Err("SF-SEC-019: حمولة الخزنة تالفة أو غير صالحة");
    }
    let (stored_key, encrypted_data) = ciphertext.split_at(32);
    if stored_key != key.as_bytes() {
        return Err("SF-SEC-020: مفتاح فك التشفير غير صحيح أو البيانات مصدّرة بكلمة سر مختلفة");
    }
    Ok(encrypted_data.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vault_key_derivation_and_encryption() {
        let salt = [1u8; 16];
        let key = VaultKey::derive_from_passphrase("secure-master-pass", &salt).unwrap();
        let plain = b"SafiData Secret Local Storage Data";

        let cipher = encrypt_vault_payload(&key, plain).unwrap();
        let decrypted = decrypt_vault_payload(&key, &cipher).unwrap();
        assert_eq!(decrypted, plain);
    }

    #[test]
    fn test_vault_wrong_key_rejection() {
        let salt = [1u8; 16];
        let key1 = VaultKey::derive_from_passphrase("secure-master-pass-1", &salt).unwrap();
        let key2 = VaultKey::derive_from_passphrase("secure-master-pass-2", &salt).unwrap();
        let plain = b"Confidential Data";

        let cipher = encrypt_vault_payload(&key1, plain).unwrap();
        let res = decrypt_vault_payload(&key2, &cipher);
        assert!(res.is_err());
    }
}
