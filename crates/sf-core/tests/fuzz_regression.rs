//! Fuzzing targets and regression tests for SafiData core parsers, PII detectors, and desktop security components (Section 18.9).
//!
//! Zero network dependencies (`AGENTS.md` Rule #7).

#[cfg(test)]
mod fuzz_regression_tests {
    use sf_core::pii::PiiProtector;

    #[test]
    fn test_fuzz_pii_edge_cases_and_corrupt_inputs() {
        let secret = b"fuzz_secret_key_safi_2026";

        // Corpus of extreme / malformed inputs that might cause crashes or panics in parsers
        let corpus = vec![
            "",
            "   ",
            "a",
            "123456789012345678901234567890",
            "2990101010123", // too short for NID
            "299010101012345", // too long for NID
            "49901010101234", // starts with 4 (invalid NID prefix)
            "00000000000", // invalid phone
            "+20 010 1234 5678",
            "not-an-email",
            "@safidata.com",
            "mohamed@@safidata.com",
            "مرحباً بك يا محمد",
            "🎉🔥🚀",
            "\0\0\0\x12",
            "SELECT * FROM users; --",
        ];

        for input in corpus {
            // Ensure no panic occurs under any malformed or malicious input
            let res = PiiProtector::protect(input, secret);
            for m in res {
                assert!(!m.redacted_text.is_empty());
                assert!(!m.tokenized_value.is_empty());
            }
        }
    }

    #[test]
    fn test_fuzz_deterministic_token_stability() {
        let secret = b"stable_secret_key_2026";
        let inputs = vec!["01012345678", "29901010101234", "test@safidata.com", "عربي"];
        for input in inputs {
            let t1 = PiiProtector::compute_deterministic_token(input, secret);
            let t2 = PiiProtector::compute_deterministic_token(input, secret);
            assert_eq!(t1, t2);
            assert!(t1.starts_with("tok_"));
        }
    }
}
