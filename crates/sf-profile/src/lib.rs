//! `sf-profile` — Column profiling, type inference, confidence scoring, and understanding contract
//! adhering to Specification Sections 4.3 and 16.1.
//!
//! Zero network dependencies (`AGENTS.md` Rule #7).

pub mod profiler;

pub use profiler::{ColumnProfile, ProfileActionStatus, profile_column, profile_table};

/// Canonical Cargo crate identifier within the `safi` workspace.
pub const CRATE_ID: &str = "sf-profile";

#[cfg(test)]
mod tests {
    use super::CRATE_ID;

    #[test]
    fn crate_id_matches_workspace_member() {
        assert_eq!(CRATE_ID, "sf-profile");
    }
}
