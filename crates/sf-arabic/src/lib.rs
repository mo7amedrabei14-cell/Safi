//! `sf-arabic` — Arabic text normalization, cleaning, and matching engine
//! adhering to Specification Section 4.4.
//!
//! Rule #1: The original text is always preserved; normalized form is used for matching only.
//! Zero network dependencies (`AGENTS.md` Rule #7).

pub mod normalizer;

pub use normalizer::{ArabicNormalizer, NormOptions, normalize_text};

/// Canonical Cargo crate identifier within the `safi` workspace.
pub const CRATE_ID: &str = "sf-arabic";

#[cfg(test)]
mod tests {
    use super::CRATE_ID;

    #[test]
    fn crate_id_matches_workspace_member() {
        assert_eq!(CRATE_ID, "sf-arabic");
    }
}
