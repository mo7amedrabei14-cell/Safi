//! `sf-recipe` — Recipe schema representation, steps, deterministic content hashing (`BLAKE3`),
//! and deterministic execution adhering to Specification Sections 4.9 and 19.5.
//!
//! Zero network dependencies (`AGENTS.md` Rule #7).

pub mod recipe;

pub use recipe::{RecipeOperation, RecipeStep, SafiRecipe};

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
