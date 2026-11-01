//! Foundational Cargo workspace crate boundary for `sf-profile` (`T1.00`).
//! Column type detection, confidence thresholds, and evidence generation are implemented in `T1.07`.

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
