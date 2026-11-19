//! Cryptographic Desktop Device Identity (Ed25519) with encrypted-at-rest local key storage.
//!
//! Each SafiData Desktop installation generates a unique Ed25519 keypair bound to its
//! installation ID. The private key is encrypted at rest via [`crate::vault`] (Argon2id +
//! ChaCha20-Poly1305) and zeroized on drop. On every online authorization / heartbeat request,
//! the desktop signs the canonical proof `safidata-device-proof-v1|{token_hash}|{timestamp_secs}`
//! so a stolen bearer token alone cannot be replayed from another computer.

use std::fs;
use std::io::Write;
use std::path::Path;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use ring::rand::{SecureRandom, SystemRandom};
use zeroize::Zeroize;

use crate::vault::{VaultKey, decrypt_vault_payload, encrypt_vault_payload};

/// Cryptographic Ed25519 device identity kept zeroized in memory.
pub struct DeviceKeypair {
    seed: [u8; 32],
    public_key_hex: String,
}

impl std::fmt::Debug for DeviceKeypair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceKeypair")
            .field("public_key_hex", &self.public_key_hex)
            .field("seed", &"[REDACTED]")
            .finish()
    }
}

impl Drop for DeviceKeypair {
    fn drop(&mut self) {
        self.seed.zeroize();
    }
}

impl DeviceKeypair {
    /// Generate a fresh Ed25519 device identity using the OS CSPRNG.
    ///
    /// # Errors
    /// Returns `SF-SYS-023` if OS randomness fails.
    pub fn generate() -> Result<Self, &'static str> {
        let rng = SystemRandom::new();
        let mut seed = [0u8; 32];
        rng.fill(&mut seed)
            .map_err(|_| "SF-SYS-023: failed to generate cryptographic device identity")?;
        Ok(Self::from_seed(seed))
    }

    /// Construct a [`DeviceKeypair`] from a 32-byte Ed25519 seed.
    #[must_use]
    pub fn from_seed(seed: [u8; 32]) -> Self {
        let signing = SigningKey::from_bytes(&seed);
        let public_key_hex = hex::encode(VerifyingKey::from(&signing).to_bytes());
        Self {
            seed,
            public_key_hex,
        }
    }

    /// Hex-encoded 32-byte Ed25519 public key sent to SafiData Server during device login.
    #[must_use]
    pub fn public_key_hex(&self) -> &str {
        &self.public_key_hex
    }

    /// Sign the canonical online authorization proof matching `sf-server::device_authority`.
    #[must_use]
    pub fn sign_device_proof(&self, session_token_hash: &str, timestamp_secs: i64) -> String {
        let signing = SigningKey::from_bytes(&self.seed);
        let canonical = format!("safidata-device-proof-v1|{session_token_hash}|{timestamp_secs}");
        let sig = signing.sign(canonical.as_bytes());
        hex::encode(sig.to_bytes())
    }

    /// Verify a canonical proof against a hex-encoded Ed25519 public key.
    ///
    /// # Errors
    /// Returns `SF-AUTH-020` if the public key, signature, or proof verification fails.
    pub fn verify_device_proof(
        public_key_hex: &str,
        session_token_hash: &str,
        timestamp_secs: i64,
        signature_hex: &str,
    ) -> Result<(), &'static str> {
        let pk_bytes = hex::decode(public_key_hex.trim()).map_err(|_| "SF-AUTH-020")?;
        let pk_arr: [u8; 32] = pk_bytes.try_into().map_err(|_| "SF-AUTH-020")?;
        let vk = VerifyingKey::from_bytes(&pk_arr).map_err(|_| "SF-AUTH-020")?;
        let sig_bytes = hex::decode(signature_hex.trim()).map_err(|_| "SF-AUTH-020")?;
        let sig = Signature::from_slice(&sig_bytes).map_err(|_| "SF-AUTH-020")?;
        let canonical = format!("safidata-device-proof-v1|{session_token_hash}|{timestamp_secs}");
        vk.verify(canonical.as_bytes(), &sig)
            .map_err(|_| "SF-AUTH-020")
    }

    /// Persist the Ed25519 seed encrypted at rest (`ChaCha20-Poly1305`) using atomic temp -> `sync_all` -> rename.
    ///
    /// # Errors
    /// Returns an error if encryption or atomic persistence fails.
    pub fn seal_to_disk(&self, path: &Path, vault_key: &VaultKey) -> Result<(), &'static str> {
        let encrypted = encrypt_vault_payload(vault_key, &self.seed)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|_| "SF-SYS-023: failed to create key directory")?;
        }
        let tmp = path.with_extension("key.tmp");
        {
            let mut file =
                fs::File::create(&tmp).map_err(|_| "SF-SYS-023: failed to create temp key file")?;
            file.write_all(&encrypted)
                .map_err(|_| "SF-SYS-023: failed to write encrypted device key")?;
            file.sync_all()
                .map_err(|_| "SF-SYS-023: failed to sync encrypted device key")?;
        }
        fs::rename(&tmp, path).map_err(|_| "SF-SYS-023: failed to commit encrypted device key")?;
        Ok(())
    }

    /// Load and decrypt a sealed Ed25519 device keypair from disk.
    ///
    /// # Errors
    /// Returns an error if the file is unreadable, tampered, or the key is incorrect.
    pub fn load_from_disk(path: &Path, vault_key: &VaultKey) -> Result<Self, &'static str> {
        let ciphertext =
            fs::read(path).map_err(|_| "SF-SYS-023: failed to read encrypted device key")?;
        let mut plain = decrypt_vault_payload(vault_key, &ciphertext)?;
        if plain.len() != 32 {
            plain.zeroize();
            return Err("SF-SYS-024: decrypted device key has invalid length");
        }
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&plain);
        plain.zeroize();
        Ok(Self::from_seed(seed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_keypair_signs_verifies_and_seals_encrypted_at_rest() {
        let dir = crate::test_result(tempfile::tempdir());
        let key_path = dir.path().join("device.key.enc");
        let salt = [0x42u8; 16];
        let vault_key = crate::test_result(VaultKey::derive_from_passphrase(
            "desktop-device-vault-pass",
            &salt,
        ));

        let kp = crate::test_result(DeviceKeypair::generate());
        let token_hash = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let ts = 1_795_000_000i64;
        let sig = kp.sign_device_proof(token_hash, ts);

        // Valid proof verifies.
        assert!(
            DeviceKeypair::verify_device_proof(kp.public_key_hex(), token_hash, ts, &sig).is_ok()
        );
        // Different token hash or wrong timestamp fails closed with SF-AUTH-020.
        assert_eq!(
            DeviceKeypair::verify_device_proof(kp.public_key_hex(), "bbbb", ts, &sig),
            Err("SF-AUTH-020")
        );
        assert_eq!(
            DeviceKeypair::verify_device_proof(kp.public_key_hex(), token_hash, ts + 1, &sig),
            Err("SF-AUTH-020")
        );
        // Different keypair fails closed.
        let other = crate::test_result(DeviceKeypair::generate());
        assert_eq!(
            DeviceKeypair::verify_device_proof(other.public_key_hex(), token_hash, ts, &sig),
            Err("SF-AUTH-020")
        );

        // Seal to disk and verify zero plaintext seed bytes exist on disk.
        crate::test_result(kp.seal_to_disk(&key_path, &vault_key));
        let raw = crate::test_result(fs::read(&key_path));
        assert!(!raw.windows(32).any(|w| w == kp.seed));

        // Load back and verify identical public key & signature verification.
        let loaded = crate::test_result(DeviceKeypair::load_from_disk(&key_path, &vault_key));
        assert_eq!(loaded.public_key_hex(), kp.public_key_hex());
        let sig2 = loaded.sign_device_proof(token_hash, ts);
        assert!(
            DeviceKeypair::verify_device_proof(kp.public_key_hex(), token_hash, ts, &sig2).is_ok()
        );

        // Wrong vault key fails closed.
        let wrong_vault = crate::test_result(VaultKey::derive_from_passphrase(
            "wrong-vault-passphrase",
            &salt,
        ));
        assert!(DeviceKeypair::load_from_disk(&key_path, &wrong_vault).is_err());
    }
}
