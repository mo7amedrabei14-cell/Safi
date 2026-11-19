//! Secure Updater with Ed25519 manifest signature verification, Anti-Downgrade floor,
//! quarantine tracking, and Pending-Commit boot rollback (Section 9.8).

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpdateStatus {
    UpToDate,
    UpdateAvailable { version: u64, is_mandatory: bool },
    PendingVerification,
    Committed,
    RolledBackDueToFailure,
    RequiredUpdateBlocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseManifest {
    pub version: u64,
    pub min_required_version: u64,
    pub delta_checksum: String,
    pub signature_hex: String,
    pub is_mandatory: bool,
}

impl ReleaseManifest {
    /// Canonical byte payload signed by the release server's Ed25519 private key.
    #[must_use]
    pub fn canonical_signing_payload(&self) -> Vec<u8> {
        format!(
            "safidata-update-v1|{}|{}|{}|{}",
            self.version, self.min_required_version, self.delta_checksum, self.is_mandatory
        )
        .into_bytes()
    }
}

/// Verify an update manifest's Ed25519 signature against the pinned public verifying key.
pub fn verify_manifest_signature(
    manifest: &ReleaseManifest,
    verifying_key: &VerifyingKey,
) -> Result<(), &'static str> {
    let sig_bytes = hex::decode(&manifest.signature_hex)
        .map_err(|_| "SF-UPD-007: توقيع بيان التحديث تالف أو غير صالح")?;
    let signature = Signature::from_slice(&sig_bytes)
        .map_err(|_| "SF-UPD-007: طول توقيع Ed25519 لبيان التحديث غير صالح")?;
    verifying_key
        .verify(&manifest.canonical_signing_payload(), &signature)
        .map_err(|_| "SF-UPD-008: توقيع بيان التحديث غير موثوق أو مزوّر")
}

#[derive(Debug, Clone)]
pub struct SecureUpdater {
    pub current_version: u64,
    pub rollback_version: u64,
    pub quarantined_builds: HashSet<u64>,
    pub boot_state: UpdateStatus,
    verifying_key: Option<VerifyingKey>,
}

impl SecureUpdater {
    pub fn new(current_version: u64) -> Self {
        Self {
            current_version,
            rollback_version: current_version,
            quarantined_builds: HashSet::new(),
            boot_state: UpdateStatus::Committed,
            verifying_key: None,
        }
    }

    /// Construct a production updater that cryptographically verifies every manifest via Ed25519.
    pub fn with_verifying_key(current_version: u64, verifying_key: VerifyingKey) -> Self {
        Self {
            current_version,
            rollback_version: current_version,
            quarantined_builds: HashSet::new(),
            boot_state: UpdateStatus::Committed,
            verifying_key: Some(verifying_key),
        }
    }

    /// Check manifest cryptographic signature (when key is configured), anti-downgrade, and quarantine rules.
    pub fn evaluate_manifest(
        &self,
        manifest: &ReleaseManifest,
    ) -> Result<UpdateStatus, &'static str> {
        if let Some(ref verifying_key) = self.verifying_key {
            verify_manifest_signature(manifest, verifying_key)?;
        }
        if manifest.version < self.current_version {
            return Err(
                "SF-UPD-001: محاولة رجوع لإصدار أقدم مرفوضة سياسياً وتقنياً (Anti-Downgrade Floor)",
            );
        }
        if manifest.version == self.current_version {
            return Ok(UpdateStatus::UpToDate);
        }
        if self.quarantined_builds.contains(&manifest.version) {
            return Err(
                "SF-UPD-002: الإصدار المستهدف موضوع في قائمة الحظر المؤقت لتكرار فشل الإقلاع (Quarantined Build)",
            );
        }
        if manifest.version < manifest.min_required_version {
            return Ok(UpdateStatus::RequiredUpdateBlocked);
        }

        Ok(UpdateStatus::UpdateAvailable {
            version: manifest.version,
            is_mandatory: manifest.is_mandatory,
        })
    }

    /// Stage an update with rollback slot and pending verification state.
    pub fn stage_update(&mut self, manifest: &ReleaseManifest) -> Result<(), &'static str> {
        let status = self.evaluate_manifest(manifest)?;
        match status {
            UpdateStatus::UpdateAvailable { .. } => {
                self.rollback_version = self.current_version;
                self.current_version = manifest.version;
                self.boot_state = UpdateStatus::PendingVerification;
                Ok(())
            }
            UpdateStatus::RequiredUpdateBlocked => {
                Err("SF-UPD-003: الإصدار الحالي أقل من الحد الأدنى الإجباري؛ شاشة 'تحديث مطلوب' مفعلة")
            }
            _ => Ok(()),
        }
    }

    /// Confirm boot health successfully; commits the update and raises anti-downgrade floor.
    pub fn confirm_boot_health(&mut self) -> Result<(), &'static str> {
        if self.boot_state != UpdateStatus::PendingVerification {
            return Err("SF-UPD-004: لا يوجد تحديث معلق بانتظار تأكيد الإقلاع");
        }
        self.boot_state = UpdateStatus::Committed;
        Ok(())
    }

    /// Handle boot failure during PendingVerification: auto-rollback to previous slot and quarantine.
    pub fn handle_boot_failure(&mut self) -> Result<(), &'static str> {
        if self.boot_state != UpdateStatus::PendingVerification {
            return Err("SF-UPD-005: حالة النظام لا تسمح بالتراجع الآمن الحالي");
        }
        let failed_version = self.current_version;
        self.quarantined_builds.insert(failed_version);
        self.current_version = self.rollback_version;
        self.boot_state = UpdateStatus::RolledBackDueToFailure;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    #[test]
    fn test_anti_downgrade_protection() {
        let updater = SecureUpdater::new(105);
        let manifest = ReleaseManifest {
            version: 100,
            min_required_version: 90,
            delta_checksum: "abc123sha256".to_string(),
            signature_hex: "valid_sig".to_string(),
            is_mandatory: false,
        };
        assert!(updater.evaluate_manifest(&manifest).is_err());
    }

    #[test]
    fn test_pending_commit_rollback_flow() {
        let mut updater = SecureUpdater::new(100);
        let manifest = ReleaseManifest {
            version: 101,
            min_required_version: 100,
            delta_checksum: "delta_checksum_101".to_string(),
            signature_hex: "valid_sig".to_string(),
            is_mandatory: true,
        };

        crate::test_result(updater.stage_update(&manifest));
        assert_eq!(updater.current_version, 101);
        assert_eq!(updater.rollback_version, 100);
        assert_eq!(updater.boot_state, UpdateStatus::PendingVerification);

        crate::test_result(updater.handle_boot_failure());
        assert_eq!(updater.current_version, 100);
        assert_eq!(updater.boot_state, UpdateStatus::RolledBackDueToFailure);
        assert!(updater.quarantined_builds.contains(&101));

        assert!(updater.stage_update(&manifest).is_err());
    }

    #[test]
    fn test_successful_boot_commit() {
        let mut updater = SecureUpdater::new(200);
        let manifest = ReleaseManifest {
            version: 201,
            min_required_version: 200,
            delta_checksum: "delta_201".to_string(),
            signature_hex: "valid_sig".to_string(),
            is_mandatory: false,
        };

        crate::test_result(updater.stage_update(&manifest));
        crate::test_result(updater.confirm_boot_health());
        assert_eq!(updater.boot_state, UpdateStatus::Committed);
        assert_eq!(updater.current_version, 201);
    }

    #[test]
    fn test_ed25519_signed_manifest_accepts_valid_and_rejects_forged() {
        let signing_key = SigningKey::from_bytes(&[77u8; 32]);
        let verifying_key = VerifyingKey::from(&signing_key);
        let updater = SecureUpdater::with_verifying_key(100, verifying_key);

        let mut manifest = ReleaseManifest {
            version: 102,
            min_required_version: 100,
            delta_checksum: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                .to_string(),
            signature_hex: String::new(),
            is_mandatory: false,
        };
        let sig = signing_key.sign(&manifest.canonical_signing_payload());
        manifest.signature_hex = hex::encode(sig.to_bytes());

        assert_eq!(
            crate::test_result(updater.evaluate_manifest(&manifest)),
            UpdateStatus::UpdateAvailable {
                version: 102,
                is_mandatory: false
            }
        );

        // Tamper checksum -> Ed25519 verification MUST reject.
        let mut tampered = manifest.clone();
        tampered.delta_checksum = "0".repeat(64);
        assert!(updater.evaluate_manifest(&tampered).is_err());

        // Fake string signature -> MUST reject.
        let mut fake = manifest;
        fake.signature_hex = "ed25519_verified_signature_manifest".to_string();
        assert!(updater.evaluate_manifest(&fake).is_err());
    }
}
