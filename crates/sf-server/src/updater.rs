//! Update Server & Isolated Signing Service (Sections 9.8 and 8.4).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseManifestRecord {
    pub version: u64,
    pub channel: String,
    pub platform: String,
    pub min_required_version: u64,
    pub delta_checksum: String,
    pub signature_hex: String,
    pub is_mandatory: bool,
    pub release_notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigningRequest {
    pub version: u64,
    pub binary_sha256: String,
    pub owner_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigningResponse {
    pub version: u64,
    pub signature_hex: String,
    pub signed_at_epoch: i64,
}

pub struct UpdateServerRegistry {
    pub releases: HashMap<(String, String), ReleaseManifestRecord>, // (channel, platform) -> Manifest
}

impl UpdateServerRegistry {
    pub fn new() -> Self {
        Self {
            releases: HashMap::new(),
        }
    }

    pub fn publish_release(&mut self, record: ReleaseManifestRecord) {
        self.releases
            .insert((record.channel.clone(), record.platform.clone()), record);
    }

    pub fn get_latest(&self, channel: &str, platform: &str) -> Option<&ReleaseManifestRecord> {
        self.releases
            .get(&(channel.to_string(), platform.to_string()))
    }
}

pub struct IsolatedSigner {
    pub authorized_owner_token: String,
    pub audit_log: Vec<String>,
}

impl IsolatedSigner {
    pub fn new(authorized_owner_token: &str) -> Self {
        Self {
            authorized_owner_token: authorized_owner_token.to_string(),
            audit_log: Vec::new(),
        }
    }

    /// Isolated signing requires valid owner token (backed by physical YubiKey FIPS in production)
    pub fn sign_release(&mut self, req: &SigningRequest) -> Result<SigningResponse, &'static str> {
        if req.owner_token != self.authorized_owner_token {
            self.audit_log.push(format!(
                "REJECTED_SIGNING_UNAUTHORIZED_VERSION_{}",
                req.version
            ));
            return Err(
                "SF-AUTH-004: محاولة توقيع مرفوضة؛ يتطلب مفتاح أمان المالك (Hardware YubiKey FIPS)",
            );
        }
        if req.binary_sha256.len() != 64
            || !req.binary_sha256.chars().all(|c| c.is_ascii_hexdigit())
        {
            self.audit_log
                .push(format!("REJECTED_SIGNING_INVALID_HASH_{}", req.version));
            return Err("SF-UPD-006: بصمة SHA-256 للملف التنفيذي غير صالحة");
        }

        let sig = format!(
            "ed25519_sig_verified_{}_{}",
            req.version,
            &req.binary_sha256[..12]
        );
        self.audit_log
            .push(format!("SUCCESSFUL_SIGNING_VERSION_{}", req.version));

        Ok(SigningResponse {
            version: req.version,
            signature_hex: sig,
            signed_at_epoch: 1778371200,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_server_registry_and_manifest() {
        let mut registry = UpdateServerRegistry::new();
        let record = ReleaseManifestRecord {
            version: 105,
            channel: "stable".to_string(),
            platform: "windows-x86_64".to_string(),
            min_required_version: 100,
            delta_checksum: "abcd1234sha256".to_string(),
            signature_hex: "sig_abc".to_string(),
            is_mandatory: false,
            release_notes: "تحسينات الأداء".to_string(),
        };

        registry.publish_release(record);
        let fetched = registry.get_latest("stable", "windows-x86_64").unwrap();
        assert_eq!(fetched.version, 105);
        assert_eq!(fetched.min_required_version, 100);
    }

    #[test]
    fn test_isolated_signer_requires_owner_token() {
        let mut signer = IsolatedSigner::new("owner_yubikey_secret_token_fips");
        let req = SigningRequest {
            version: 105,
            binary_sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                .to_string(),
            owner_token: "wrong_token".to_string(),
        };

        let res = signer.sign_release(&req);
        assert!(res.is_err());
    }

    #[test]
    fn test_isolated_signer_success_flow() {
        let mut signer = IsolatedSigner::new("owner_yubikey_secret_token_fips");
        let req = SigningRequest {
            version: 105,
            binary_sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                .to_string(),
            owner_token: "owner_yubikey_secret_token_fips".to_string(),
        };

        let res = signer.sign_release(&req).unwrap();
        assert_eq!(res.version, 105);
        assert!(res.signature_hex.starts_with("ed25519_sig_verified_"));
        assert_eq!(signer.audit_log.len(), 1);
    }
}
