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
const KEYSTORE_MAGIC_V2: &[u8; 4] = b"SFK2";
const RECOVERY_KEY_PREFIX: &str = "SFKR1:";
const KEYSTORE_VERIFY_PLAIN: &[u8] = b"safidata.vault.keystore.verified.v1";
const TAG_LEN: usize = 16;

/// Error returned by an OS-backed secret store (Windows Credential Manager / DPAPI,
/// macOS Keychain, or Linux Secret Service).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsKeystoreError {
    /// No secret stored for the requested `key_id`.
    NotFound,
    /// OS keystore daemon/service is unavailable on this host.
    Unavailable,
    /// OS keystore is currently locked by the user session.
    Locked,
    /// Storage I/O or backend failure.
    BackendError,
}

/// Platform OS keystore abstraction implemented by the native Tauri Desktop shell
/// (Windows Credential Manager, macOS Keychain, Linux Secret Service) while keeping
/// the air-gapped `sf-desktop` engine crate 100% network-free and `unsafe`-free.
pub trait OsKeystoreProvider: std::fmt::Debug {
    /// Human-readable backend identifier (e.g. `"windows_credential_manager"`, `"macos_keychain"`, `"linux_secret_service"`).
    fn backend_name(&self) -> &'static str;
    /// Persist the hex-encoded 32-byte master secret under `key_id` in the OS keystore.
    fn store_secret(&self, key_id: &str, secret_hex: &str) -> Result<(), OsKeystoreError>;
    /// Load the hex-encoded 32-byte master secret for `key_id` from the OS keystore.
    fn load_secret(&self, key_id: &str) -> Result<String, OsKeystoreError>;
    /// Delete a stored secret (used during cleanup/rotation).
    fn delete_secret(&self, key_id: &str) -> Result<(), OsKeystoreError>;
}

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
    /// Key-custody threat model & known limitation (honest audit note):
    /// - Format `SFK1` currently persists `salt` and `master_secret` in `storage_dir/.keystore/vault.master.key`
    ///   (with `0o700`/`0o600` permissions on Unix and standard user-profile directory ACLs on Windows).
    /// - This protects against accidental plaintext `.json` persistence, casual inspection/tampering
    ///   of `.safi.enc` files, and backup/sync exposure when `.keystore/` is excluded.
    /// - It does **NOT** yet bind `master_secret` to an OS keystore (Windows DPAPI / Credential Manager,
    ///   macOS Keychain, or Linux Secret Service), so an attacker who can read both `.keystore/vault.master.key`
    ///   and `.safi.enc` under the same OS user account or offline disk image can derive the vault key.
    /// - Because workspace policy forbids `unsafe_code` (`Cargo.toml`), OS keystore integration must be
    ///   introduced via an audited safe cross-platform wrapper (e.g. `SFK2` DPAPI / Keychain / Secret Service
    ///   envelope with verified round-trip migration and zero-data-loss fallback) in a dedicated platform release.
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

    /// Loads, initializes, or safely migrates (`SFK1 -> SFK2`) the installation [`VaultKey`]
    /// backed by an OS-protected keystore (`Windows Credential Manager` / `macOS Keychain` /
    /// `Linux Secret Service`) so **ZERO `master_secret` bytes are persisted on disk**.
    ///
    /// On-disk `SFK2` envelope format:
    /// `[SFK2: 4B][salt: 16B][key_id_len: 2B BE][key_id_utf8][verify_cipher]`
    ///
    /// Migration & safety guarantees (Section 3.14):
    /// - Existing `SFK1` keystore is read and verified first.
    /// - `master_secret` is stored in `os_keystore` and **read back & re-verified** against
    ///   `KEYSTORE_VERIFY_PLAIN` BEFORE `vault.master.key` is atomically replaced with `SFK2`.
    /// - If `os_keystore` is missing, unavailable, or locked during migration, `SFK1` remains
    ///   100% untouched on disk (zero data loss) and this method fails closed (`SF-SYS-024`).
    /// - Existing `SFK2` vaults fail closed (`SF-SYS-024`) if the OS keystore entry is missing,
    ///   locked, wrong, or the envelope is corrupted. Never falls back to plaintext.
    ///
    /// # Errors
    /// Returns `SF-SYS-023` or `SF-SYS-024` on OS keystore, filesystem, or verification failure.
    pub fn load_or_init_with_os_keystore(
        storage_dir: &Path,
        os_keystore: &dyn OsKeystoreProvider,
    ) -> Result<Self, &'static str> {
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
            if raw.len() >= 4 && &raw[0..4] == KEYSTORE_MAGIC_V2 {
                // SFK2: [SFK2: 4B][salt: 16B][key_id_len: 2B][key_id][verify_cipher]
                if raw.len() < 4 + 16 + 2 + 1 + NONCE_LEN + TAG_LEN {
                    return Err("SF-SYS-024: ملف مفتاح الخزنة المحلية (SFK2) تالف أو غير صالح");
                }
                let salt = &raw[4..20];
                let id_len = usize::from(u16::from_be_bytes([raw[20], raw[21]]));
                if id_len == 0 || raw.len() <= 22 + id_len + 1 + NONCE_LEN + TAG_LEN {
                    return Err("SF-SYS-024: معرّف مفتاح نظام التشغيل تالف أو غير صالح");
                }
                let key_id = std::str::from_utf8(&raw[22..22 + id_len])
                    .map_err(|_| "SF-SYS-024: معرّف مفتاح نظام التشغيل غير صالح")?;
                let verify_cipher = &raw[22 + id_len..];

                let mut secret_hex = os_keystore.load_secret(key_id).map_err(
                    |_| "SF-SYS-024: مخزن مفاتيح نظام التشغيل غير متاح أو مقفل أو المفتاح مفقود",
                )?;
                let derived_res = Self::derive_from_passphrase(&secret_hex, salt);
                secret_hex.zeroize();
                let derived = derived_res?;

                let verified = decrypt_vault_payload(&derived, verify_cipher).map_err(
                    |_| "SF-SYS-024: عدم تطابق مفتاح نظام التشغيل أو تلف الخزنة المحلية",
                )?;
                if verified != KEYSTORE_VERIFY_PLAIN {
                    return Err("SF-SYS-024: فشل التحقق من بصمة مفتاح الخزنة المحلية (SFK2)");
                }
                return Ok(derived);
            }

            if raw.len() >= 4 + 16 + 32 + 1 + NONCE_LEN + TAG_LEN && &raw[0..4] == KEYSTORE_MAGIC {
                // Verified zero-data-loss migration: SFK1 -> SFK2 (OS-backed)!
                let mut salt = [0u8; 16];
                salt.copy_from_slice(&raw[4..20]);
                let mut master_secret = [0u8; 32];
                master_secret.copy_from_slice(&raw[20..52]);
                let verify_cipher = &raw[52..];

                let mut secret_hex = hex::encode(master_secret);
                master_secret.zeroize();
                let derived = match Self::derive_from_passphrase(&secret_hex, &salt) {
                    Ok(k) => k,
                    Err(e) => {
                        secret_hex.zeroize();
                        return Err(e);
                    }
                };
                let verified = match decrypt_vault_payload(&derived, verify_cipher) {
                    Ok(v) => v,
                    Err(_) => {
                        secret_hex.zeroize();
                        return Err("SF-SYS-024: فشل التحقق من مفتاح الخزنة القديم قبل الترقية");
                    }
                };
                if verified != KEYSTORE_VERIFY_PLAIN {
                    secret_hex.zeroize();
                    return Err("SF-SYS-024: عدم تطابق بصمة المفتاح القديم قبل الترقية");
                }

                // Store into OS keystore and VERIFY round-trip BEFORE touching SFK1 on disk!
                let key_id = generate_os_key_id()?;
                if os_keystore.store_secret(&key_id, &secret_hex).is_err() {
                    secret_hex.zeroize();
                    // SFK1 remains untouched on disk (zero data loss rollback!).
                    return Err(
                        "SF-SYS-024: تعذّر حفظ المفتاح في مخزن نظام التشغيل؛ لم يُمسّ الملف القديم",
                    );
                }
                secret_hex.zeroize();

                let mut roundtrip_hex = match os_keystore.load_secret(&key_id) {
                    Ok(h) => h,
                    Err(_) => {
                        let _ = os_keystore.delete_secret(&key_id);
                        return Err(
                            "SF-SYS-024: فشل التحقق العكسي من مخزن نظام التشغيل قبل الترقية",
                        );
                    }
                };
                let re_derived = Self::derive_from_passphrase(&roundtrip_hex, &salt);
                roundtrip_hex.zeroize();
                let re_derived = match re_derived {
                    Ok(k) if k.as_bytes() == derived.as_bytes() => k,
                    _ => {
                        let _ = os_keystore.delete_secret(&key_id);
                        return Err("SF-SYS-024: عدم تطابق المفتاح المسترجع من مخزن نظام التشغيل");
                    }
                };

                let new_verify = encrypt_vault_payload(&re_derived, KEYSTORE_VERIFY_PLAIN)?;
                let sfk2 = build_sfk2_envelope(&salt, &key_id, &new_verify)?;
                if atomic_write_vault_file(&key_file, &sfk2).is_err() {
                    let _ = os_keystore.delete_secret(&key_id);
                    return Err("SF-SYS-023: تعذّر ترقية ملف الخزنة ذرياً إلى SFK2");
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = fs::set_permissions(&key_file, fs::Permissions::from_mode(0o600));
                }
                return Ok(re_derived);
            }

            return Err("SF-SYS-024: ملف مفتاح الخزنة المحلية تالف أو غير صالح");
        }

        // Fresh installation: generate CSPRNG salt + master_secret, store ONLY in OS keystore,
        // verify round-trip, and write SFK2 (containing zero secret bytes on disk).
        let rng = SystemRandom::new();
        let mut salt = [0u8; 16];
        let mut master_secret = [0u8; 32];
        rng.fill(&mut salt)
            .map_err(|_| "SF-SYS-023: تعذّر توليد ملح عشوائي للخزنة")?;
        rng.fill(&mut master_secret)
            .map_err(|_| "SF-SYS-023: تعذّر توليد المفتاح الرئيسي العشوائي للخزنة")?;

        let mut secret_hex = hex::encode(master_secret);
        master_secret.zeroize();
        let derived = match Self::derive_from_passphrase(&secret_hex, &salt) {
            Ok(k) => k,
            Err(e) => {
                secret_hex.zeroize();
                return Err(e);
            }
        };

        let key_id = generate_os_key_id()?;
        if os_keystore.store_secret(&key_id, &secret_hex).is_err() {
            secret_hex.zeroize();
            return Err("SF-SYS-024: مخزن مفاتيح نظام التشغيل غير متاح لحفظ المفتاح الجديد");
        }
        secret_hex.zeroize();

        let mut readback = match os_keystore.load_secret(&key_id) {
            Ok(h) => h,
            Err(_) => {
                let _ = os_keystore.delete_secret(&key_id);
                return Err("SF-SYS-024: فشل التحقق من قراءة المفتاح من مخزن نظام التشغيل");
            }
        };
        let check_key = Self::derive_from_passphrase(&readback, &salt);
        readback.zeroize();
        if check_key.map(|k| *k.as_bytes()).ok() != Some(*derived.as_bytes()) {
            let _ = os_keystore.delete_secret(&key_id);
            return Err("SF-SYS-024: عدم تطابق المفتاح المخزن في نظام التشغيل");
        }

        let verify_cipher = encrypt_vault_payload(&derived, KEYSTORE_VERIFY_PLAIN)?;
        let sfk2 = build_sfk2_envelope(&salt, &key_id, &verify_cipher)?;
        if atomic_write_vault_file(&key_file, &sfk2).is_err() {
            let _ = os_keystore.delete_secret(&key_id);
            return Err("SF-SYS-023: تعذّر حفظ ملف مفتاح الخزنة (SFK2) ذرياً");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&key_file, fs::Permissions::from_mode(0o600));
        }
        Ok(derived)
    }

    /// Exports an explicit user-controlled recovery key string (`SFKR1:<salt_hex>:<secret_hex>`)
    /// so the user can back up their local vault and restore encrypted `.safi.enc` files on another machine.
    ///
    /// # Errors
    /// Returns `SF-SYS-024` if the keystore is missing, corrupted, or the OS keyring is locked.
    pub fn export_recovery_key(
        storage_dir: &Path,
        os_keystore: &dyn OsKeystoreProvider,
    ) -> Result<String, &'static str> {
        let key_file = storage_dir.join(".keystore").join("vault.master.key");
        let raw = fs::read(&key_file).map_err(|_| "SF-SYS-024: ملف مفتاح الخزنة غير موجود")?;
        if raw.len() >= 22 && &raw[0..4] == KEYSTORE_MAGIC_V2 {
            let salt = &raw[4..20];
            let id_len = usize::from(u16::from_be_bytes([raw[20], raw[21]]));
            if id_len == 0 || raw.len() <= 22 + id_len {
                return Err("SF-SYS-024: ملف SFK2 تالف");
            }
            let key_id = std::str::from_utf8(&raw[22..22 + id_len])
                .map_err(|_| "SF-SYS-024: معرّف المفتاح غير صالح")?;
            let secret_hex = os_keystore
                .load_secret(key_id)
                .map_err(|_| "SF-SYS-024: تعذّر قراءة المفتاح من مخزن نظام التشغيل")?;
            return Ok(format!(
                "{RECOVERY_KEY_PREFIX}{}:{}",
                hex::encode(salt),
                secret_hex
            ));
        }
        if raw.len() >= 52 && &raw[0..4] == KEYSTORE_MAGIC {
            return Ok(format!(
                "{RECOVERY_KEY_PREFIX}{}:{}",
                hex::encode(&raw[4..20]),
                hex::encode(&raw[20..52])
            ));
        }
        Err("SF-SYS-024: تنسيق ملف الخزنة غير معروف")
    }

    /// Restores a local vault from a backed-up recovery key (`SFKR1:<salt_hex>:<secret_hex>`),
    /// storing the secret inside the target machine's OS keystore and writing `SFK2`.
    ///
    /// # Errors
    /// Returns `SF-SYS-024` if the recovery key is malformed or the OS keystore fails.
    pub fn restore_from_recovery_key(
        storage_dir: &Path,
        os_keystore: &dyn OsKeystoreProvider,
        recovery_key: &str,
    ) -> Result<Self, &'static str> {
        let body = recovery_key
            .trim()
            .strip_prefix(RECOVERY_KEY_PREFIX)
            .ok_or("SF-SYS-024: صيغة مفتاح الاسترداد غير صالحة")?;
        let (salt_hex, secret_hex) = body
            .split_once(':')
            .ok_or("SF-SYS-024: صيغة مفتاح الاسترداد ناقصة")?;
        let salt_bytes = hex::decode(salt_hex).map_err(|_| "SF-SYS-024: ملح الاسترداد غير صالح")?;
        let secret_bytes =
            hex::decode(secret_hex).map_err(|_| "SF-SYS-024: سر الاسترداد غير صالح")?;
        if salt_bytes.len() != 16 || secret_bytes.len() != 32 {
            return Err("SF-SYS-024: طول مفتاح الاسترداد غير صالح");
        }
        let derived = Self::derive_from_passphrase(secret_hex, &salt_bytes)?;
        let keystore_dir = storage_dir.join(".keystore");
        fs::create_dir_all(&keystore_dir).map_err(|_| "SF-SYS-023: تعذّر إنشاء مجلد الخزنة")?;
        let key_id = generate_os_key_id()?;
        os_keystore
            .store_secret(&key_id, secret_hex)
            .map_err(|_| "SF-SYS-024: تعذّر حفظ مفتاح الاسترداد في مخزن نظام التشغيل")?;
        let verify_cipher = encrypt_vault_payload(&derived, KEYSTORE_VERIFY_PLAIN)?;
        let sfk2 = build_sfk2_envelope(&salt_bytes, &key_id, &verify_cipher)?;
        let key_file = keystore_dir.join("vault.master.key");
        atomic_write_vault_file(&key_file, &sfk2)
            .map_err(|_| "SF-SYS-023: تعذّر كتابة ملف SFK2 المستعاد")?;
        Ok(derived)
    }
}

fn generate_os_key_id() -> Result<String, &'static str> {
    let rng = SystemRandom::new();
    let mut id_bytes = [0u8; 16];
    rng.fill(&mut id_bytes)
        .map_err(|_| "SF-SYS-023: تعذّر توليد معرّف مفتاح نظام التشغيل")?;
    Ok(format!("safi-vault-{}", hex::encode(id_bytes)))
}

fn build_sfk2_envelope(
    salt: &[u8],
    key_id: &str,
    verify_cipher: &[u8],
) -> Result<Vec<u8>, &'static str> {
    let id_bytes = key_id.as_bytes();
    let id_len = u16::try_from(id_bytes.len()).map_err(|_| "SF-SYS-023: معرّف المفتاح طويل جداً")?;
    let mut out = Vec::with_capacity(4 + 16 + 2 + id_bytes.len() + verify_cipher.len());
    out.extend_from_slice(KEYSTORE_MAGIC_V2);
    out.extend_from_slice(salt);
    out.extend_from_slice(&id_len.to_be_bytes());
    out.extend_from_slice(id_bytes);
    out.extend_from_slice(verify_cipher);
    Ok(out)
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

    #[derive(Debug, Default)]
    struct MockOsKeystore {
        secrets: std::sync::Mutex<std::collections::HashMap<String, String>>,
        mode: std::sync::Mutex<Option<OsKeystoreError>>,
    }

    impl MockOsKeystore {
        fn set_error(&self, err: Option<OsKeystoreError>) {
            if let Ok(mut g) = self.mode.lock() {
                *g = err;
            }
        }

        fn overwrite_all_with(&self, wrong_secret_hex: &str) {
            if let Ok(mut g) = self.secrets.lock() {
                for v in g.values_mut() {
                    *v = wrong_secret_hex.to_owned();
                }
            }
        }

        fn clear_all(&self) {
            if let Ok(mut g) = self.secrets.lock() {
                g.clear();
            }
        }
    }

    impl OsKeystoreProvider for MockOsKeystore {
        fn backend_name(&self) -> &'static str {
            "mock_os_keystore"
        }

        fn store_secret(&self, key_id: &str, secret_hex: &str) -> Result<(), OsKeystoreError> {
            if let Ok(g) = self.mode.lock()
                && let Some(err) = *g
            {
                return Err(err);
            }
            if let Ok(mut map) = self.secrets.lock() {
                let _ = map.insert(key_id.to_owned(), secret_hex.to_owned());
                return Ok(());
            }
            Err(OsKeystoreError::BackendError)
        }

        fn load_secret(&self, key_id: &str) -> Result<String, OsKeystoreError> {
            if let Ok(g) = self.mode.lock()
                && let Some(err) = *g
            {
                return Err(err);
            }
            if let Ok(map) = self.secrets.lock() {
                return map.get(key_id).cloned().ok_or(OsKeystoreError::NotFound);
            }
            Err(OsKeystoreError::BackendError)
        }

        fn delete_secret(&self, key_id: &str) -> Result<(), OsKeystoreError> {
            if let Ok(mut map) = self.secrets.lock() {
                let _ = map.remove(key_id);
                return Ok(());
            }
            Err(OsKeystoreError::BackendError)
        }
    }

    #[test]
    fn test_sfk2_os_keystore_never_writes_master_secret_to_disk() {
        let dir = temp_vault_test_dir("sfk2_zero_secret_on_disk");
        let os_ks = MockOsKeystore::default();
        let key1 = crate::test_result(VaultKey::load_or_init_with_os_keystore(&dir, &os_ks));
        let key2 = crate::test_result(VaultKey::load_or_init_with_os_keystore(&dir, &os_ks));
        assert_eq!(key1.as_bytes(), key2.as_bytes());

        let key_file = dir.join(".keystore").join("vault.master.key");
        let on_disk = crate::test_result(fs::read(&key_file));
        assert_eq!(&on_disk[0..4], KEYSTORE_MAGIC_V2);

        // Prove the 32-byte raw master_secret stored in OS keystore is COMPLETELY ABSENT from disk!
        let stored_hex = os_ks
            .secrets
            .lock()
            .ok()
            .and_then(|m| m.values().next().cloned())
            .unwrap_or_default();
        let stored_bytes = crate::test_result(hex::decode(&stored_hex));
        assert_eq!(stored_bytes.len(), 32);
        assert!(!on_disk.windows(32).any(|w| w == stored_bytes.as_slice()));
        assert!(!String::from_utf8_lossy(&on_disk).contains(&stored_hex));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_sfk1_to_sfk2_verified_migration_rollback_and_existing_file_preservation() {
        let dir = temp_vault_test_dir("sfk1_to_sfk2_migration");
        // 1. Initialize legacy SFK1 vault and encrypt a real customer file.
        let legacy_key = crate::test_result(VaultKey::load_or_init_for_storage_dir(&dir));
        let enc_path = dir.join("customer_q4.safi.enc");
        let payload = br#"{"sheet":"Q4_Sales","total":987654}"#;
        crate::test_result(write_encrypted_mandatory(
            &enc_path,
            None,
            Some(&legacy_key),
            payload,
        ));
        let key_file = dir.join(".keystore").join("vault.master.key");
        let original_sfk1_bytes = crate::test_result(fs::read(&key_file));
        assert_eq!(&original_sfk1_bytes[0..4], KEYSTORE_MAGIC);

        // 2. Simulate locked OS keyring during migration -> MUST fail closed AND leave SFK1 untouched!
        let os_ks = MockOsKeystore::default();
        os_ks.set_error(Some(OsKeystoreError::Locked));
        assert!(VaultKey::load_or_init_with_os_keystore(&dir, &os_ks).is_err());
        assert_eq!(crate::test_result(fs::read(&key_file)), original_sfk1_bytes);

        // 3. Unlock OS keyring -> migration upgrades SFK1 -> SFK2 seamlessly and existing .safi.enc stays 100% readable!
        os_ks.set_error(None);
        let migrated_key =
            crate::test_result(VaultKey::load_or_init_with_os_keystore(&dir, &os_ks));
        assert_eq!(migrated_key.as_bytes(), legacy_key.as_bytes());
        assert_eq!(
            &crate::test_result(fs::read(&key_file))[0..4],
            KEYSTORE_MAGIC_V2
        );

        let decrypted = crate::test_result(read_encrypted_or_migrate_legacy(
            &enc_path,
            None,
            Some(&migrated_key),
        ));
        assert_eq!(decrypted.as_deref(), Some(payload.as_slice()));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_sfk2_corrupted_wrong_key_missing_locked_and_backup_restore() {
        let dir = temp_vault_test_dir("sfk2_failure_modes");
        let os_ks = MockOsKeystore::default();
        let key = crate::test_result(VaultKey::load_or_init_with_os_keystore(&dir, &os_ks));
        let enc_path = dir.join("project.safi.enc");
        let secret_doc = br#"{"confidential":"audit-2026"}"#;
        crate::test_result(write_encrypted_mandatory(
            &enc_path,
            None,
            Some(&key),
            secret_doc,
        ));

        // Export recovery key before testing failure modes.
        let recovery_key = crate::test_result(VaultKey::export_recovery_key(&dir, &os_ks));
        assert!(recovery_key.starts_with(RECOVERY_KEY_PREFIX));

        // 1. Locked OS keyring -> fails closed.
        os_ks.set_error(Some(OsKeystoreError::Locked));
        assert!(VaultKey::load_or_init_with_os_keystore(&dir, &os_ks).is_err());

        // 2. Unavailable OS keyring -> fails closed.
        os_ks.set_error(Some(OsKeystoreError::Unavailable));
        assert!(VaultKey::load_or_init_with_os_keystore(&dir, &os_ks).is_err());

        // 3. Wrong key stored in OS keyring -> fails closed on AEAD verification!
        os_ks.set_error(None);
        os_ks.overwrite_all_with(&"ff".repeat(32));
        assert!(VaultKey::load_or_init_with_os_keystore(&dir, &os_ks).is_err());

        // 4. Missing key in OS keyring -> fails closed!
        os_ks.clear_all();
        assert!(VaultKey::load_or_init_with_os_keystore(&dir, &os_ks).is_err());

        // 5. Corrupted vault.master.key on disk -> fails closed!
        let key_file = dir.join(".keystore").join("vault.master.key");
        crate::test_result(fs::write(&key_file, b"SFK2corrupt"));
        assert!(VaultKey::load_or_init_with_os_keystore(&dir, &os_ks).is_err());

        // 6. Backup & Restore onto a brand-new machine OS keyring unlocks the exact original .safi.enc file!
        let new_machine_ks = MockOsKeystore::default();
        let restored_key = crate::test_result(VaultKey::restore_from_recovery_key(
            &dir,
            &new_machine_ks,
            &recovery_key,
        ));
        assert_eq!(restored_key.as_bytes(), key.as_bytes());
        let unlocked = crate::test_result(read_encrypted_or_migrate_legacy(
            &enc_path,
            None,
            Some(&restored_key),
        ));
        assert_eq!(unlocked.as_deref(), Some(secret_doc.as_slice()));

        let _ = fs::remove_dir_all(&dir);
    }
}
