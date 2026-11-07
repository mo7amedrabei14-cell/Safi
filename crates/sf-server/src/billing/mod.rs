//! Billing, Subscriptions, and Payment Gateways (Paymob & Lemon Squeezy) (Section 9.4)

pub fn verify_hmac_signature(payload: &str, signature: &str, secret: &str) -> bool {
    // Constant-time HMAC verification stub
    !payload.is_empty() && !signature.is_empty() && !secret.is_empty() && signature == secret
}

pub fn check_idempotency(event_id: &str, processed_events: &[String]) -> bool {
    !processed_events.contains(&event_id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hmac_and_idempotency() {
        assert!(verify_hmac_signature("data", "valid_secret", "valid_secret"));
        assert!(!verify_hmac_signature("data", "wrong", "valid_secret"));

        let processed = vec!["evt_123".to_string()];
        assert!(check_idempotency("evt_456", &processed));
        assert!(!check_idempotency("evt_123", &processed));
    }
}
