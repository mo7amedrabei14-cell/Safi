//! Foundational Cargo workspace crate boundary for `sf-wasm` (`T1.00`).
//! WebAssembly Worker bindings and panic boundary hooks are implemented in `T1.11`.

/// Canonical Cargo crate identifier within the `safi` workspace.
pub const CRATE_ID: &str = "sf-wasm";

#[cfg(test)]
mod tests {
    use super::CRATE_ID;

    #[test]
    fn crate_id_matches_workspace_member() {
        assert_eq!(CRATE_ID, "sf-wasm");
    }
}
