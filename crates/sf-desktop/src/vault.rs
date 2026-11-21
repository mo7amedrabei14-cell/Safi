//! Local Encrypted Vault using audited Argon2id key derivation and ChaCha20-Poly1305 AEAD
//! with automatic key zeroization on drop (Section 8.3, H4, AGENTS.md Rule 5).

use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use argon2::{Algorithm, Argon2, Params, Version};
use ring::aead::{Aad, CHACHA20_POLY1305, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use ring::rand::{SecureRandom, SystemRandom};
use zeroize::Zeroize;

const VAULT_ENVELOPE_VERSION: u8 = 1;
const VAULT_AAD: &[u8] = b"safidata.vault.v1";
const KEYSTORE_MAGIC: &[u8; 4] = b"SFK1";
const KEYSTORE_VERIFY_PLAIN: &[u8] = b"safidata.vault.keystore.verified.v1";
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

    /// Loads or initializes a cryptographically random, Argon2id-derived installation [`VaultKey`]
    /// inside `storage_dir/.keystore/vault.master.key`.
    ///
    /// Security properties:
    /// - Never uses a predictable machine fingerprint, hostname, or static constant as a secret.
    /// - Generates 32 bytes of OS CSPRNG entropy (`SystemRandom`) + 16 bytes of CSPRNG salt.
    /// - Derives the 256-bit `VaultKey` via `Argon2id` and seals a `ChaCha20-Poly1305` verification
    ///   tag so corrupted or tampered keystore files fail closed immediately (`SF-SYS-024`).
    /// - Zeroizes intermediate secret buffers before returning.
    ///
    /// # Errors
    /// Returns `SF-SYS-023` or `SF-SYS-024` if CSPRNG, filesystem, or verification fails.
    pub fn load_or_init_for_storage_dir(storage_dir: &Path) -> Result<Self, &'static str> {
        let keystore_dir = storage_dir.join(".keystore");
        fs::create_dir_all(&keystore_dir)
            .map_err(|_| "SF-SYS-023: تعذّر إنشاء مجلد مفاتيح الخزنة المحلية")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&keystore_dir, fs::Permissions::from_mode(0o700));
        }

        let key_file = keystore_dir.join("vault.master.key");
        if key_file.exists() {
            let raw =
                fs::read(&key_file).map_err(|_| "SF-SYS-024: تعذّر قراءة مفتاح الخزنة المحلية")?;
            // Envelope: [SFK1: 4B][salt: 16B][master_secret: 32B][verify_cipher: >= 30B]
            if raw.len() < 4 + 16 + 32 + 1 + NONCE_LEN + TAG_LEN || &raw[0..4] != KEYSTORE_MAGIC {
                return Err("SF-SYS-024: ملف مفتاح الخزنة المحلية تالف أو غير صالح");
            }
            let salt = &raw[4..20];
            let mut master_secret = [0u8; 32];
            master_secret.copy_from_slice(&raw[20..52]);
            let verify_cipher = &raw[52..];

            let mut secret_hex = hex::encode(master_secret);
            master_secret.zeroize();
            let derived_res = Self::derive_from_passphrase(&secret_hex, salt);
            secret_hex.zeroize();
            let derived = derived_res?;

            let verified = decrypt_vault_payload(&derived, verify_cipher)
                .map_err(|_| "SF-SYS-024: فشل التحقق من سلامة مفتاح الخزنة المحلية")?;
            if verified != KEYSTORE_VERIFY_PLAIN {
                return Err("SF-SYS-024: عدم تطابق بصمة التحقق لمفتاح الخزنة المحلية");
            }
            return Ok(derived);
        }

        let rng = SystemRandom::new();
        let mut salt = [0u8; 16];
        let mut master_secret = [0u8; 32];
        rng.fill(&mut salt)
            .map_err(|_| "SF-SYS-023: تعذّر توليد ملح عشوائي للخزنة")?;
        rng.fill(&mut master_secret)
            .map_err(|_| "SF-SYS-023: تعذّر توليد المفتاح الرئيسي العشوائي للخزنة")?;

        let mut secret_hex = hex::encode(master_secret);
        let derived_res = Self::derive_from_passphrase(&secret_hex, &salt);
        secret_hex.zeroize();
        let derived = derived_res?;

        let verify_cipher = encrypt_vault_payload(&derived, KEYSTORE_VERIFY_PLAIN)?;
        let mut envelope = Vec::with_capacity(4 + 16 + 32 + verify_cipher.len());
        envelope.extend_from_slice(KEYSTORE_MAGIC);
        envelope.extend_from_slice(&salt);
        envelope.extend_from_slice(&master_secret);
        master_secret.zeroize();
        envelope.extend_from_slice(&verify_cipher);

        atomic_write_vault_file(&key_file, &envelope)
            .map_err(|_| "SF-SYS-023: تعذّر حفظ مفتاح الخزنة المحلية ذرياً")?;
        envelope.zeroize();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&key_file, fs::Permissions::from_mode(0o600));
        }

        Ok(derived)
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

/// Atomically writes an encrypted vault file (`temp -> sync_all -> rename`) and verifies that
/// the written ciphertext decrypts back to `plain_bytes` before removing any legacy plaintext file.
///
/// Fails closed with `"SF-SYS-024"` if `vault_key` is `None`. Never writes plaintext JSON!
///
/// # Errors
/// Returns `"SF-SYS-024"` when `vault_key` is missing, `"SF-SYS-023"` on encryption/verification
/// failure, or `"SF-SYS-016"` on disk write failure.
pub fn write_encrypted_mandatory(
    enc_path: &Path,
    legacy_plain_path: Option<&Path>,
    vault_key: Option<&VaultKey>,
    plain_bytes: &[u8],
) -> Result<(), String> {
    let key = vault_key.ok_or_else(|| "SF-SYS-024".to_owned())?;
    let cipher = encrypt_vault_payload(key, plain_bytes).map_err(|_| "SF-SYS-023".to_owned())?;
    atomic_write_vault_file(enc_path, &cipher)?;

    // Read-back verification before touching any legacy plaintext file.
    let on_disk = fs::read(enc_path).map_err(|_| "SF-SYS-023".to_owned())?;
    let verified = decrypt_vault_payload(key, &on_disk).map_err(|_| "SF-SYS-024".to_owned())?;
    if verified.as_slice() != plain_bytes {
        return Err("SF-SYS-023".to_owned());
    }

    if let Some(plain_path) = legacy_plain_path
        && plain_path != enc_path
        && plain_path.exists()
    {
        let _ = fs::remove_file(plain_path);
    }
    Ok(())
}

/// Reads an encrypted `.safi.enc` file, or safely migrates a legacy plaintext `.json` file into
/// encrypted `.safi.enc` storage (verifying the encrypted output on disk before deleting the
/// plaintext source so migration failures never lose user data).
///
/// Fails closed with `"SF-SYS-024"` if `vault_key` is `None`.
///
/// # Errors
/// Returns `"SF-SYS-024"` when `vault_key` is missing, wrong, or ciphertext is corrupted;
/// `"SF-SYS-023"` if legacy plaintext migration fails (leaving the plaintext file intact);
/// or `"SF-SYS-001"` on read error.
pub fn read_encrypted_or_migrate_legacy(
    enc_path: &Path,
    legacy_plain_path: Option<&Path>,
    vault_key: Option<&VaultKey>,
) -> Result<Option<Vec<u8>>, String> {
    let key = vault_key.ok_or_else(|| "SF-SYS-024".to_owned())?;
    if enc_path.exists() {
        let cipher = fs::read(enc_path).map_err(|_| "SF-SYS-001".to_owned())?;
        let plain = decrypt_vault_payload(key, &cipher).map_err(|_| "SF-SYS-024".to_owned())?;
        return Ok(Some(plain));
    }
    if let Some(plain_path) = legacy_plain_path
        && plain_path.exists()
    {
        let plain_bytes = fs::read(plain_path).map_err(|_| "SF-SYS-001".to_owned())?;
        if plain_bytes.is_empty() {
            return Err("SF-SYS-001".to_owned());
        }
        // Migrate to encrypted file and verify decryption BEFORE removing the legacy plaintext file.
        // If write_encrypted_mandatory fails, legacy_plain_path remains untouched on disk!
        write_encrypted_mandatory(enc_path, Some(plain_path), Some(key), &plain_bytes)
            .map_err(|_| "SF-SYS-023".to_owned())?;
        return Ok(Some(plain_bytes));
    }
    Ok(None)
}

fn atomic_write_vault_file(target: &Path, contents: &[u8]) -> Result<(), String> {
    let dir = target.parent().ok_or_else(|| "SF-SYS-016".to_owned())?;
    fs::create_dir_all(dir).map_err(|_| "SF-SYS-016".to_owned())?;
    let name = target
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("safi_vault");
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = dir.join(format!(".{name}.{stamp}.tmp"));
    let write_res = (|| -> std::io::Result<()> {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(contents)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, target)
    })();
    if write_res.is_err() {
        let _ = fs::remove_file(&tmp);
        return Err("SF-SYS-016".to_owned());
    }
    Ok(())
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

    fn temp_vault_test_dir(tag: &str) -> std::path::PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("safi_vault_test_{tag}_{stamp}"));
        let _ = fs::create_dir_all(&dir);
        dir
    }

    #[test]
    fn test_missing_key_fail_closed_behavior() {
        let dir = temp_vault_test_dir("missing_key");
        let enc_path = dir.join("workflow_memory.safi.enc");
        let plain_path = dir.join("workflow_memory.json");

        let write_res =
            write_encrypted_mandatory(&enc_path, Some(&plain_path), None, b"{\"secret\":42}");
        assert_eq!(write_res, Err("SF-SYS-024".to_owned()));
        assert!(!enc_path.exists());
        assert!(!plain_path.exists());

        let read_res = read_encrypted_or_migrate_legacy(&enc_path, Some(&plain_path), None);
        assert_eq!(read_res, Err("SF-SYS-024".to_owned()));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_encrypted_write_read_round_trip_and_keystore_persistence() {
        let dir = temp_vault_test_dir("round_trip");
        let key1 = crate::test_result(VaultKey::load_or_init_for_storage_dir(&dir));
        let key2 = crate::test_result(VaultKey::load_or_init_for_storage_dir(&dir));
        assert_eq!(key1.as_bytes(), key2.as_bytes());

        let enc_path = dir.join("workflow_memory.safi.enc");
        let plain_path = dir.join("workflow_memory.json");
        let payload = br#"{"project":"Q4_Payroll","sensitive_formula":"SUM(Net_Pay)"}"#;

        crate::test_result(write_encrypted_mandatory(
            &enc_path,
            Some(&plain_path),
            Some(&key1),
            payload,
        ));
        assert!(enc_path.exists());
        assert!(!plain_path.exists());

        let raw_on_disk = crate::test_result(fs::read(&enc_path));
        assert!(!String::from_utf8_lossy(&raw_on_disk).contains("Q4_Payroll"));

        let loaded = crate::test_result(read_encrypted_or_migrate_legacy(
            &enc_path,
            Some(&plain_path),
            Some(&key2),
        ));
        assert_eq!(loaded.as_deref(), Some(payload.as_slice()));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_wrong_key_and_corrupted_ciphertext_rejection_on_disk() {
        let dir = temp_vault_test_dir("wrong_key_corrupt");
        let key_good = crate::test_result(VaultKey::derive_from_passphrase("good-key", &[3u8; 16]));
        let key_wrong =
            crate::test_result(VaultKey::derive_from_passphrase("wrong-key", &[3u8; 16]));

        let enc_path = dir.join("learning_profile.safi.enc");
        let payload = br#"{"domain":"Finance","confirmations":5}"#;
        crate::test_result(write_encrypted_mandatory(
            &enc_path,
            None,
            Some(&key_good),
            payload,
        ));

        // Wrong key must fail closed with SF-SYS-024
        let wrong_res = read_encrypted_or_migrate_legacy(&enc_path, None, Some(&key_wrong));
        assert_eq!(wrong_res, Err("SF-SYS-024".to_owned()));

        // Corrupted ciphertext must fail closed with SF-SYS-024
        let mut raw = crate::test_result(fs::read(&enc_path));
        let last = raw.len() - 1;
        raw[last] ^= 0xFF;
        crate::test_result(fs::write(&enc_path, &raw));

        let corrupt_res = read_encrypted_or_migrate_legacy(&enc_path, None, Some(&key_good));
        assert_eq!(corrupt_res, Err("SF-SYS-024".to_owned()));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_legacy_plaintext_migration_and_zero_data_loss_on_failure() {
        let dir = temp_vault_test_dir("legacy_migration");
        let key = crate::test_result(VaultKey::load_or_init_for_storage_dir(&dir));
        let enc_path = dir.join("recipe_library.safi.enc");
        let plain_path = dir.join("recipe_library.json");
        let legacy_json = br#"[{"id":"rcp-1","name":"Legacy Recipe"}]"#;

        crate::test_result(fs::write(&plain_path, legacy_json));

        // 1. Simulate migration failure: make enc_path a directory so atomic rename into enc_path fails!
        crate::test_result(fs::create_dir_all(&enc_path));
        let failed_migration =
            read_encrypted_or_migrate_legacy(&enc_path, Some(&plain_path), Some(&key));
        assert!(failed_migration.is_err());
        // Legacy plaintext file MUST still exist and contain the exact original bytes!
        assert!(plain_path.exists());
        assert_eq!(
            crate::test_result(fs::read(&plain_path)).as_slice(),
            legacy_json
        );

        // 2. Remove the obstructing directory and run migration again -> must succeed, verify, and remove plaintext.
        let _ = fs::remove_dir_all(&enc_path);
        let migrated = crate::test_result(read_encrypted_or_migrate_legacy(
            &enc_path,
            Some(&plain_path),
            Some(&key),
        ));
        assert_eq!(migrated.as_deref(), Some(legacy_json.as_slice()));
        assert!(enc_path.exists());
        assert!(!plain_path.exists());

        // Subsequent read loads directly from encrypted .safi.enc
        let subsequent = crate::test_result(read_encrypted_or_migrate_legacy(
            &enc_path,
            Some(&plain_path),
            Some(&key),
        ));
        assert_eq!(subsequent.as_deref(), Some(legacy_json.as_slice()));

        let _ = fs::remove_dir_all(&dir);
    }
}
