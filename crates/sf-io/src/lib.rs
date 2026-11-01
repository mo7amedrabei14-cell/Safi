//! Foundational Cargo workspace crate boundary for `sf-io` (`T1.00`).
//! CSV and XLSX streaming readers and safety limits are implemented in `T1.04` and `T1.05`.

/// Canonical Cargo crate identifier within the `safi` workspace.
pub const CRATE_ID: &str = "sf-io";

#[cfg(test)]
mod tests {
    use super::CRATE_ID;

    #[test]
    fn crate_id_matches_workspace_member() {
        assert_eq!(CRATE_ID, "sf-io");
    }
}
