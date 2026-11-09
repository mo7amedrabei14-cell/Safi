//! Fuzzing and regression fuzz tests for file parsers, zip bombs, deep links, and IPC commands (Section 18.9).
//!
//! Zero network dependencies (`AGENTS.md` Rule #7).

#[cfg(test)]
mod deep_link_fuzz_tests {
    use sf_desktop::deep_link::parse;

    #[test]
    fn test_fuzz_malformed_and_malicious_deep_links() {
        let fuzz_corpus = vec![
            "",
            "   ",
            "safi://",
            "http://safi://open?file=C:/test.csv",
            "safi://open?file=C:/test.exe",
            "safi://open?file=../../../etc/passwd",
            "safi://unknown_action?foo=bar",
            "safi://open?file=C:/a/b/../c/secret.txt",
            "safi://open?file=%00C:/test.csv",
            "safi://settings?section=admin",
            "safi://open?file=\\\\evil-server\\share\\data.csv",
            "safi://open?file=C:/test.csv&malicious=payload",
            "safi://settings?section=general&section=updates",
            "safi://open?file=C:/test.CSV%00.exe",
        ];

        for link in fuzz_corpus {
            let res = parse(link);
            // Every malicious or malformed link MUST be rejected safely without panic
            assert!(res.is_err(), "Expected deep link to be rejected: {link}");
        }
    }
}
