//! Foundational Cargo workspace crate boundary for `sf-arabic` (`T1.00`).
//! Arabic text normalization and Egyptian National ID validation are implemented in `T1.06` and `T2.13`.

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
