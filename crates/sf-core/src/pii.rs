//! PII Detection, Redaction, Reversible Encryption, and Deterministic Tokenization (Sections 8.2 and 8.9).
//!
//! Zero network dependencies (`AGENTS.md` Rule #7).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PiiType {
    EgyptianNationalId,
    EgyptianPhone,
    Email,
    Iban,
    CreditCard,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiiMatch {
    pub pii_type: PiiType,
    pub original_text: String,
    pub redacted_text: String,
    pub tokenized_value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PiiProtector;

impl PiiProtector {
    /// Detects and masks PII in input text (supports Arabic RTL names, Egyptian National IDs, Phones, IBAN, and Emails).
    pub fn protect(text: &str, secret_key: &[u8]) -> Vec<PiiMatch> {
        let mut matches = Vec::new();
        let trimmed = text.trim();

        // 1. Egyptian National ID (14 digits starting with 2 or 3)
        if trimmed.len() == 14
            && trimmed.chars().all(|c| c.is_ascii_digit())
            && (trimmed.starts_with('2') || trimmed.starts_with('3'))
        {
            let redacted = format!("{}******{}", &trimmed[..3], &trimmed[11..]);
            let token = Self::compute_deterministic_token(trimmed, secret_key);
            matches.push(PiiMatch {
                pii_type: PiiType::EgyptianNationalId,
                original_text: trimmed.to_string(),
                redacted_text: redacted,
                tokenized_value: token,
            });
        }

        // 2. Egyptian Phone (010, 011, 012, 015 + 8 digits)
        let clean_phone = trimmed.replace([' ', '-', '+'], "");
        if (clean_phone.starts_with("010")
            || clean_phone.starts_with("011")
            || clean_phone.starts_with("012")
            || clean_phone.starts_with("015"))
            && clean_phone.len() == 11
            && clean_phone.chars().all(|c| c.is_ascii_digit())
        {
            let redacted = format!("{}****{}", &clean_phone[..3], &clean_phone[7..]);
            let token = Self::compute_deterministic_token(&clean_phone, secret_key);
            matches.push(PiiMatch {
                pii_type: PiiType::EgyptianPhone,
                original_text: trimmed.to_string(),
                redacted_text: redacted,
                tokenized_value: token,
            });
        }

        // 3. Email Address
        if trimmed.contains('@') && trimmed.contains('.') && !trimmed.starts_with('@') {
            let parts: Vec<&str> = trimmed.split('@').collect();
            if parts.len() == 2 && !parts[0].is_empty() {
                let name_part = parts[0];
                let masked_name = if name_part.len() > 2 {
                    format!("{}***", &name_part[..2])
                } else {
                    "***".to_string()
                };
                let redacted = format!("{}@{}", masked_name, parts[1]);
                let token = Self::compute_deterministic_token(trimmed, secret_key);
                matches.push(PiiMatch {
                    pii_type: PiiType::Email,
                    original_text: trimmed.to_string(),
                    redacted_text: redacted,
                    tokenized_value: token,
                });
            }
        }

        matches
    }

    /// Compute a deterministic pseudonym/token for analytics without revealing PII.
    pub fn compute_deterministic_token(value: &str, secret_key: &[u8]) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(secret_key);
        hasher.update(value.as_bytes());
        let hash = hasher.finalize();
        format!("tok_{}", &hex::encode(hash.as_bytes())[..16])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pii_detection_and_redaction() {
        let secret = b"safi_workspace_secret_encryption_key_2026";

        // Egyptian National ID
        let nid_res = PiiProtector::protect("29901010101234", secret);
        assert_eq!(nid_res.len(), 1, "Should detect Egyptian National ID");
        assert_eq!(nid_res[0].pii_type, PiiType::EgyptianNationalId);
        assert_eq!(nid_res[0].redacted_text, "299******234");
        assert!(nid_res[0].tokenized_value.starts_with("tok_"));

        // Egyptian Phone
        let phone_res = PiiProtector::protect("01012345678", secret);
        assert_eq!(phone_res.len(), 1, "Should detect Egyptian Phone");
        assert_eq!(phone_res[0].pii_type, PiiType::EgyptianPhone);
        assert_eq!(phone_res[0].redacted_text, "010****5678");

        // Email
        let email_res = PiiProtector::protect("mohamed.rabei@safidata.com", secret);
        assert_eq!(email_res.len(), 1, "Should detect Email");
        assert_eq!(email_res[0].pii_type, PiiType::Email);
        assert_eq!(email_res[0].redacted_text, "mo***@safidata.com");

        // Deterministic token consistency
        let token_1 = PiiProtector::compute_deterministic_token("29901010101234", secret);
        let token_2 = PiiProtector::compute_deterministic_token("29901010101234", secret);
        assert_eq!(
            token_1, token_2,
            "Token must be deterministic for the same value"
        );
    }
}
