//! Real Ed25519 Release Manifest Signing & Offline Desktop License Issuance (Phase 15).
//!
//! Security invariants:
//! - Uses audited `ed25519-dalek` SigningKey/VerifyingKey exclusively.
//! - Private signing keys exist ONLY server-side via `SAFI_UPDATE_SIGNING_PRIVATE_KEY` and
//!   `SAFI_LICENSE_SIGNING_PRIVATE_KEY`; public verifying keys are exported safely for Desktop.
//! - Zero hardcoded fake signatures (`"ed25519_verified_signature_manifest"` removed).

use std::fmt;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::error::AuthError;

/// Release manifest published by the update service and verified by `sf-desktop::updater`.
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

impl ReleaseManifestRecord {
    /// Canonical byte representation matching `sf-desktop::updater::ReleaseManifest::canonical_signing_payload`.
    #[must_use]
    pub fn canonical_signing_payload(&self) -> Vec<u8> {
        format!(
            "safidata-update-v1|{}|{}|{}|{}",
            self.version, self.min_required_version, self.delta_checksum, self.is_mandatory
        )
        .into_bytes()
    }
}

/// Offline Desktop license payload matching `sf-desktop::license::LicensePayload`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssuedLicensePayload {
    pub license_id: String,
    pub user_id: String,
    pub tier: String,
    pub issued_at_secs: u64,
    pub expires_at_secs: u64,
    pub device_fingerprint: String,
}

/// Ed25519-signed offline license bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedLicenseBundle {
    pub payload: IssuedLicensePayload,
    pub signature_hex: String,
    pub verifying_key_hex: String,
}

/// Server-side Ed25519 signer for both Desktop update manifests and offline licenses.
#[derive(Clone)]
pub struct ReleaseAndLicenseSigner {
    update_signing_key: Option<SigningKey>,
    license_signing_key: Option<SigningKey>,
}

impl fmt::Debug for ReleaseAndLicenseSigner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReleaseAndLicenseSigner")
            .field("update_key_configured", &self.update_signing_key.is_some())
            .field(
                "license_key_configured",
                &self.license_signing_key.is_some(),
            )
            .finish()
    }
}

impl ReleaseAndLicenseSigner {
    #[must_use]
    pub fn new(update_seed: Option<[u8; 32]>, license_seed: Option<[u8; 32]>) -> Self {
        Self {
            update_signing_key: update_seed.map(|s| SigningKey::from_bytes(&s)),
            license_signing_key: license_seed.map(|s| SigningKey::from_bytes(&s)),
        }
    }

    #[must_use]
    pub fn update_verifying_key_hex(&self) -> Option<String> {
        self.update_signing_key
            .as_ref()
            .map(|k| hex::encode(VerifyingKey::from(k).to_bytes()))
    }

    #[must_use]
    pub fn license_verifying_key_hex(&self) -> Option<String> {
        self.license_signing_key
            .as_ref()
            .map(|k| hex::encode(VerifyingKey::from(k).to_bytes()))
    }

    /// Sign a Desktop update manifest with the server's Ed25519 private key.
    pub fn sign_manifest(
        &self,
        mut manifest: ReleaseManifestRecord,
    ) -> Result<ReleaseManifestRecord, AuthError> {
        let key = self
            .update_signing_key
            .as_ref()
            .ok_or(AuthError::BackendUnavailable)?;
        let sig = key.sign(&manifest.canonical_signing_payload());
        manifest.signature_hex = hex::encode(sig.to_bytes());
        Ok(manifest)
    }

    /// Verify a signed manifest against the corresponding Ed25519 verifying key.
    pub fn verify_manifest(&self, manifest: &ReleaseManifestRecord) -> Result<(), AuthError> {
        let key = self
            .update_signing_key
            .as_ref()
            .ok_or(AuthError::BackendUnavailable)?;
        let verifying_key = VerifyingKey::from(key);
        let sig_bytes =
            hex::decode(&manifest.signature_hex).map_err(|_| AuthError::InvalidCredentials)?;
        let signature =
            Signature::from_slice(&sig_bytes).map_err(|_| AuthError::InvalidCredentials)?;
        verifying_key
            .verify(&manifest.canonical_signing_payload(), &signature)
            .map_err(|_| AuthError::InvalidCredentials)
    }

    /// Sign an offline Desktop license payload with the server's Ed25519 license private key.
    pub fn sign_license(
        &self,
        payload: IssuedLicensePayload,
    ) -> Result<SignedLicenseBundle, AuthError> {
        let key = self
            .license_signing_key
            .as_ref()
            .ok_or(AuthError::BackendUnavailable)?;
        let payload_bytes = serde_json::to_vec(&payload).map_err(|_| AuthError::InvalidRequest)?;
        let sig = key.sign(&payload_bytes);
        Ok(SignedLicenseBundle {
            payload,
            signature_hex: hex::encode(sig.to_bytes()),
            verifying_key_hex: hex::encode(VerifyingKey::from(key).to_bytes()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_ed25519_manifest_and_license_signing_rejects_tampering() {
        let signer = ReleaseAndLicenseSigner::new(Some([0x31u8; 32]), Some([0x32u8; 32]));
        let unsigned = ReleaseManifestRecord {
            version: 105,
            channel: "stable".to_owned(),
            platform: "windows-x86_64".to_owned(),
            min_required_version: 100,
            delta_checksum: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                .to_owned(),
            signature_hex: String::new(),
            is_mandatory: false,
            release_notes: "SafiData Desktop 1.0 release".to_owned(),
        };
        let signed = signer.sign_manifest(unsigned).expect("sign manifest");
        assert_eq!(signed.signature_hex.len(), 128); // 64 bytes hex
        assert!(signer.verify_manifest(&signed).is_ok());

        let mut tampered = signed;
        tampered.version = 999;
        assert!(signer.verify_manifest(&tampered).is_err());

        let lic = signer
            .sign_license(IssuedLicensePayload {
                license_id: "lic_001".to_owned(),
                user_id: "usr_001".to_owned(),
                tier: "Pro".to_owned(),
                issued_at_secs: 1_795_000_000,
                expires_at_secs: 1_797_592_000,
                device_fingerprint: "dev_fp_01".to_owned(),
            })
            .expect("sign license");
        assert_eq!(lic.signature_hex.len(), 128);
        assert_eq!(lic.verifying_key_hex.len(), 64);
    }

    #[test]
    fn unconfigured_signing_key_fails_closed() {
        let signer = ReleaseAndLicenseSigner::new(None, None);
        let unsigned = ReleaseManifestRecord {
            version: 105,
            channel: "stable".to_owned(),
            platform: "windows-x86_64".to_owned(),
            min_required_version: 100,
            delta_checksum: "0".repeat(64),
            signature_hex: String::new(),
            is_mandatory: false,
            release_notes: String::new(),
        };
        assert_eq!(
            signer.sign_manifest(unsigned).unwrap_err(),
            AuthError::BackendUnavailable
        );
    }
}
