//! Foundational Cargo workspace crate boundary for `sf-report` (`T1.00`).
//! Quality summaries and embedded Typst RTL vector PDF generation are implemented in `T1.10`.

/// Canonical Cargo crate identifier within the `safi` workspace.
pub const CRATE_ID: &str = "sf-report";

#[cfg(test)]
mod tests {
    use super::CRATE_ID;

    #[test]
    fn crate_id_matches_workspace_member() {
        assert_eq!(CRATE_ID, "sf-report");
    }
}
