//! Foundational Cargo workspace crate boundary for `sf-recipe` (`T1.00`).
//! Deterministic recipe serialization and BLAKE3 content hashing are implemented in `T1.09`.

/// Canonical Cargo crate identifier within the `safi` workspace.
pub const CRATE_ID: &str = "sf-recipe";

#[cfg(test)]
mod tests {
    use super::CRATE_ID;

    #[test]
    fn crate_id_matches_workspace_member() {
        assert_eq!(CRATE_ID, "sf-recipe");
    }
}
