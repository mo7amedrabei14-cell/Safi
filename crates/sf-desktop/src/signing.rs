//! Windows Authenticode & Binary Signature Verification (Section 10.2).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignatureStatus {
    Valid,
    MissingTimestamp,
    ExpiredCertificate,
    TamperedOrInvalid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinarySignatureInfo {
    pub is_signed: bool,
    pub has_rfc3161_timestamp: bool,
    pub certificate_valid: bool,
    pub sha256_hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignatureVerifier;

impl SignatureVerifier {
    pub fn verify(info: &BinarySignatureInfo) -> Result<SignatureStatus, &'static str> {
        if !info.is_signed {
            return Err("SF-UPD-007: الملف التنفيذي غير موقّع رقميًا (Authenticode مفقود)");
        }
        if !info.certificate_valid {
            return Err("SF-UPD-008: شهادة التوقيع الرقمي منتهية أو غير موثوقة");
        }
        if !info.has_rfc3161_timestamp {
            return Ok(SignatureStatus::MissingTimestamp);
        }
        if info.sha256_hash.len() != 64 || !info.sha256_hash.chars().all(|c| c.is_ascii_hexdigit())
        {
            return Err("SF-UPD-006: بصمة SHA-256 للملف غير صالحة أو متلاعب بها");
        }

        Ok(SignatureStatus::Valid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_signature() {
        let info = BinarySignatureInfo {
            is_signed: true,
            has_rfc3161_timestamp: true,
            certificate_valid: true,
            sha256_hash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                .to_string(),
        };
        assert_eq!(
            SignatureVerifier::verify(&info).unwrap(),
            SignatureStatus::Valid
        );
    }

    #[test]
    fn test_missing_signature() {
        let info = BinarySignatureInfo {
            is_signed: false,
            has_rfc3161_timestamp: false,
            certificate_valid: false,
            sha256_hash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                .to_string(),
        };
        assert!(SignatureVerifier::verify(&info).is_err());
    }

    #[test]
    fn test_invalid_certificate() {
        let info = BinarySignatureInfo {
            is_signed: true,
            has_rfc3161_timestamp: true,
            certificate_valid: false,
            sha256_hash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                .to_string(),
        };
        assert!(SignatureVerifier::verify(&info).is_err());
    }

    #[test]
    fn test_missing_timestamp() {
        let info = BinarySignatureInfo {
            is_signed: true,
            has_rfc3161_timestamp: false,
            certificate_valid: true,
            sha256_hash: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                .to_string(),
        };
        assert_eq!(
            SignatureVerifier::verify(&info).unwrap(),
            SignatureStatus::MissingTimestamp
        );
    }
}
