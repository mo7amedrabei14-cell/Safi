//! Offline Desktop Licensing via Ed25519 Signatures, Grace Period, and Clock Rollback Detection (Section 9.4, AGENTS.md Rule 5).

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LicenseTier {
    Free,
    Pro,
    Team,
    Enterprise,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicensePayload {
    pub license_id: String,
    pub user_id: String,
    pub tier: LicenseTier,
    pub issued_at_secs: u64,
    pub expires_at_secs: u64,
    pub device_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedLicense {
    pub payload: LicensePayload,
    pub signature_hex: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicenseStatus {
    Active,
    GracePeriod { days_remaining: u64 },
    Expired,
    Invalid,
}

#[derive(Debug, Clone)]
pub struct LicenseVerifier {
    verifying_key: VerifyingKey,
}

impl LicenseVerifier {
    pub fn new(verifying_key: VerifyingKey) -> Self {
        Self { verifying_key }
    }

    pub fn verify(
        &self,
        signed: &SignedLicense,
        current_time_secs: u64,
        last_seen_time_secs: u64,
        device_fingerprint: &str,
    ) -> Result<LicenseStatus, &'static str> {
        // 1. Clock rollback detection
        if current_time_secs < last_seen_time_secs {
            return Err("SF-LIC-001: تم رصد إرجاع ساعة النظام للوراء بشكل مشبوه");
        }

        // 2. Decode Ed25519 signature
        let sig_bytes = hex::decode(&signed.signature_hex)
            .map_err(|_| "SF-LIC-002: توقيع الترخيص تالف أو غير صالح")?;
        let signature = Signature::from_slice(&sig_bytes)
            .map_err(|_| "SF-LIC-002: بصمة توقيع الترخيص غير صالحة")?;

        // 3. Serialize payload for verification
        let payload_bytes = serde_json::to_vec(&signed.payload)
            .map_err(|_| "SF-LIC-002: تعذّر قراءة بيانات الترخيص")?;

        // 4. Verify cryptographic signature
        self.verifying_key
            .verify(&payload_bytes, &signature)
            .map_err(|_| "SF-LIC-003: توقيع الترخيص الرقمي غير مطابق أو مزوّر")?;

        // 5. Verify device fingerprint binding
        if signed.payload.device_fingerprint != device_fingerprint {
            return Err("SF-LIC-004: الترخيص مرتبط بجهاز مختلف");
        }

        // 6. Check expiration and grace period
        let expires = signed.payload.expires_at_secs;
        if current_time_secs <= expires {
            return Ok(LicenseStatus::Active);
        }

        // Enterprise tier has no expiration/offline restriction,
        // but for standard Pro/Team, apply 30-day grace period (2592000 seconds)
        let grace_period_secs = match signed.payload.tier {
            LicenseTier::Enterprise => u64::MAX,
            _ => 30 * 24 * 60 * 60,
        };

        let grace_deadline = expires.saturating_add(grace_period_secs);
        if current_time_secs <= grace_deadline {
            let remaining_secs = grace_deadline.saturating_sub(current_time_secs);
            let days_remaining = remaining_secs / (24 * 60 * 60);
            return Ok(LicenseStatus::GracePeriod { days_remaining });
        }

        Ok(LicenseStatus::Expired)
    }
}

/// Helper to generate signed licenses for testing and issuance
pub fn generate_test_license(
    signing_key: &SigningKey,
    license_id: &str,
    user_id: &str,
    tier: LicenseTier,
    issued_at_secs: u64,
    expires_at_secs: u64,
    device_fingerprint: &str,
) -> SignedLicense {
    let payload = LicensePayload {
        license_id: license_id.to_string(),
        user_id: user_id.to_string(),
        tier,
        issued_at_secs,
        expires_at_secs,
        device_fingerprint: device_fingerprint.to_string(),
    };
    let payload_bytes = serde_json::to_vec(&payload).unwrap_or_else(|error| {
        panic!("Failed to serialize license payload for signing: {error}")
    });
    let signature = signing_key.sign(&payload_bytes);
    SignedLicense {
        payload,
        signature_hex: hex::encode(signature.to_bytes()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_active_license() {
        let secret = [42u8; 32];
        let signing_key = SigningKey::from_bytes(&secret);
        let verifying_key = VerifyingKey::from(&signing_key);
        let verifier = LicenseVerifier::new(verifying_key);

        let now = 1_000_000_000;
        let signed = generate_test_license(
            &signing_key,
            "lic_123",
            "usr_456",
            LicenseTier::Pro,
            now - 1000,
            now + 86400,
            "dev_abc",
        );

        let status = verifier.verify(&signed, now, now - 500, "dev_abc").unwrap();
        assert_eq!(status, LicenseStatus::Active);
    }

    #[test]
    fn test_grace_period_offline() {
        let secret = [43u8; 32];
        let signing_key = SigningKey::from_bytes(&secret);
        let verifying_key = VerifyingKey::from(&signing_key);
        let verifier = LicenseVerifier::new(verifying_key);

        let issued = 1_000_000_000;
        let expires = issued + 86400; // expired 1 day ago
        let now = expires + 5 * 24 * 3600; // 5 days into 30-day grace period

        let signed = generate_test_license(
            &signing_key,
            "lic_123",
            "usr_456",
            LicenseTier::Pro,
            issued,
            expires,
            "dev_abc",
        );

        let status = verifier
            .verify(&signed, now, now - 1000, "dev_abc")
            .unwrap();
        match status {
            LicenseStatus::GracePeriod { days_remaining } => {
                assert!(days_remaining > 0);
            }
            _ => panic!("Expected GracePeriod status"),
        }
    }

    #[test]
    fn test_expired_after_grace_period() {
        let secret = [47u8; 32];
        let signing_key = SigningKey::from_bytes(&secret);
        let verifying_key = VerifyingKey::from(&signing_key);
        let verifier = LicenseVerifier::new(verifying_key);

        let issued = 1_000_000_000;
        let expires = issued + 86400;
        let now = expires + 35 * 24 * 3600; // 35 days past expiration (exceeds 30-day grace period)

        let signed = generate_test_license(
            &signing_key,
            "lic_123",
            "usr_456",
            LicenseTier::Pro,
            issued,
            expires,
            "dev_abc",
        );

        let status = verifier
            .verify(&signed, now, now - 1000, "dev_abc")
            .unwrap();
        assert_eq!(status, LicenseStatus::Expired);
    }

    #[test]
    fn test_enterprise_never_expires_offline() {
        let secret = [48u8; 32];
        let signing_key = SigningKey::from_bytes(&secret);
        let verifying_key = VerifyingKey::from(&signing_key);
        let verifier = LicenseVerifier::new(verifying_key);

        let issued = 1_000_000_000;
        let expires = issued + 86400;
        let now = expires + 365 * 24 * 3600; // 1 year past expiration

        let signed = generate_test_license(
            &signing_key,
            "lic_123",
            "usr_456",
            LicenseTier::Enterprise,
            issued,
            expires,
            "dev_abc",
        );

        // Enterprise tier bypasses expiration offline
        let status = verifier
            .verify(&signed, now, now - 1000, "dev_abc")
            .unwrap();
        match status {
            LicenseStatus::GracePeriod { .. } => {}
            _ => panic!("Expected GracePeriod status for Enterprise"),
        }
    }

    #[test]
    fn test_clock_rollback_rejection() {
        let secret = [44u8; 32];
        let signing_key = SigningKey::from_bytes(&secret);
        let verifying_key = VerifyingKey::from(&signing_key);
        let verifier = LicenseVerifier::new(verifying_key);

        let now = 1_000_000_000;
        let last_seen = now + 5000;

        let signed = generate_test_license(
            &signing_key,
            "lic_123",
            "usr_456",
            LicenseTier::Pro,
            now - 1000,
            now + 86400,
            "dev_abc",
        );

        let res = verifier.verify(&signed, now, last_seen, "dev_abc");
        assert!(res.is_err());
    }

    #[test]
    fn test_forged_signature_rejection() {
        let secret1 = [45u8; 32];
        let secret2 = [46u8; 32];
        let signing_key1 = SigningKey::from_bytes(&secret1);
        let signing_key2 = SigningKey::from_bytes(&secret2);
        let verifying_key1 = VerifyingKey::from(&signing_key1);
        let verifier = LicenseVerifier::new(verifying_key1);

        let now = 1_000_000_000;
        let signed = generate_test_license(
            &signing_key2, // Signed with different key
            "lic_123",
            "usr_456",
            LicenseTier::Pro,
            now - 1000,
            now + 86400,
            "dev_abc",
        );

        let res = verifier.verify(&signed, now, now - 100, "dev_abc");
        assert!(res.is_err());
    }

    #[test]
    fn test_wrong_device_fingerprint_rejection() {
        let secret = [49u8; 32];
        let signing_key = SigningKey::from_bytes(&secret);
        let verifying_key = VerifyingKey::from(&signing_key);
        let verifier = LicenseVerifier::new(verifying_key);

        let now = 1_000_000_000;
        let signed = generate_test_license(
            &signing_key,
            "lic_123",
            "usr_456",
            LicenseTier::Pro,
            now - 1000,
            now + 86400,
            "dev_abc", // Bound to dev_abc
        );

        let res = verifier.verify(&signed, now, now - 100, "dev_xyz"); // Checked on dev_xyz
        assert!(res.is_err());
    }
}
