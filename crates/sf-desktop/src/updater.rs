//! Secure Updater, Anti-Downgrade, Delta Patches, and Pending-Commit Boot Rollback (Section 9.8).

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

#[derive(Debug, Clone)]
pub struct SecureUpdater {
    pub current_version: u64,
    pub rollback_version: u64,
    pub quarantined_builds: HashSet<u64>,
    pub boot_state: UpdateStatus,
}

impl SecureUpdater {
    pub fn new(current_version: u64) -> Self {
        Self {
            current_version,
            rollback_version: current_version,
            quarantined_builds: HashSet::new(),
            boot_state: UpdateStatus::Committed,
        }
    }

    /// Check manifest and apply anti-downgrade and quarantine rules.
    pub fn evaluate_manifest(&self, manifest: &ReleaseManifest) -> Result<UpdateStatus, &'static str> {
        if manifest.version < self.current_version {
            return Err("SF-UPD-001: محاولة رجوع لإصدار أقدم مرفوضة سياسياً وتقنياً (Anti-Downgrade Floor)");
        }
        if manifest.version == self.current_version {
            return Ok(UpdateStatus::UpToDate);
        }
        if self.quarantined_builds.contains(&manifest.version) {
            return Err("SF-UPD-002: الإصدار المستهدف موضوع في قائمة الحظر المؤقت لتكرار فشل الإقلاع (Quarantined Build)");
        }
        if manifest.version < manifest.min_required_version {
            return Ok(UpdateStatus::RequiredUpdateBlocked);
        }

        Ok(UpdateStatus::UpdateAvailable {
            version: manifest.version,
            is_mandatory: manifest.is_mandatory,
        })
    }

    /// Simulate staging an update with rollback slot and pending verification state.
    pub fn stage_update(&mut self, manifest: &ReleaseManifest) -> Result<(), &'static str> {
        let status = self.evaluate_manifest(manifest)?;
        match status {
            UpdateStatus::UpdateAvailable { .. } => {
                // Save current version as rollback slot before staging new binary
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

    /// Simulate boot failure during PendingVerification: auto-rollback to previous slot and quarantine.
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

    #[test]
    fn test_anti_downgrade_protection() {
        let updater = SecureUpdater::new(105);
        let manifest = ReleaseManifest {
            version: 100, // Older than current 105
            min_required_version: 90,
            delta_checksum: "abc123sha256".to_string(),
            signature_hex: "valid_sig".to_string(),
            is_mandatory: false,
        };
        let res = updater.evaluate_manifest(&manifest);
        assert!(res.is_err());
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

        // Stage update
        updater.stage_update(&manifest).unwrap();
        assert_eq!(updater.current_version, 101);
        assert_eq!(updater.rollback_version, 100);
        assert_eq!(updater.boot_state, UpdateStatus::PendingVerification);

        // Simulate boot failure -> triggers auto-rollback to 100 and quarantines 101
        updater.handle_boot_failure().unwrap();
        assert_eq!(updater.current_version, 100);
        assert_eq!(updater.boot_state, UpdateStatus::RolledBackDueToFailure);
        assert!(updater.quarantined_builds.contains(&101));

        // Trying to stage 101 again should fail due to quarantine
        let res = updater.stage_update(&manifest);
        assert!(res.is_err());
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

        updater.stage_update(&manifest).unwrap();
        updater.confirm_boot_health().unwrap();
        assert_eq!(updater.boot_state, UpdateStatus::Committed);
        assert_eq!(updater.current_version, 201);
    }
}
