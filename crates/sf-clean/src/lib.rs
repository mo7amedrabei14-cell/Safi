//! `sf-clean` — Five cleaning operations, delta-based undo/redo, and data cleaning engine
//! adhering to Specification Sections 4.5, 4.6, 4.7, and 4.10.
//!
//! Zero network dependencies (`AGENTS.md` Rule #7).

pub mod cleaner;
pub mod delta;
#[cfg(test)]
mod tests;

pub use cleaner::{
    BlankFillStrategy, CleanEngine, CleanOperation, CleanOptions, DateAmbiguityHandling,
    DeduplicateKeep,
};
pub use delta::{CellDelta, CleanDelta};

/// Canonical Cargo crate identifier within the `safi` workspace.
pub const CRATE_ID: &str = "sf-clean";
