//! Foundational Cargo workspace crate boundary for `sf-core` (`T1.00`).
//! Domain types and error models are implemented in `T1.02` and `T1.03`.

/// Canonical Cargo crate identifier within the `safi` workspace.
pub const CRATE_ID: &str = "sf-core";

#[cfg(test)]
mod tests {
    use super::CRATE_ID;

    #[test]
    fn crate_id_matches_workspace_member() {
        assert_eq!(CRATE_ID, "sf-core");
    }
}
