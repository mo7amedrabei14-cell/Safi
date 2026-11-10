//! `sf-recipe` — Recipe schema representation, steps, deterministic content hashing (`BLAKE3`),
//! and deterministic execution adhering to Specification Sections 4.9 and 19.5.
//!
//! Zero network dependencies (`AGENTS.md` Rule #7).

pub mod datagraph;
pub mod recipe;
pub mod studio_anomaly;
pub mod studio_capsule;
pub mod studio_dsl;
pub mod studio_templates;

pub use datagraph::{
    AggFunc, BinaryOp, ChangeOutcome, DataGraph, Formula, GroupedValue, MetricSpec,
    PROJECT_SCHEMA_VERSION, ProjectSnapshot, Scenario, ScenarioComparison, Transformation,
    datagraph_display, parse_formula, tables_equal,
};
pub use recipe::{RecipeOperation, RecipeStep, SafiRecipe};
pub use studio_anomaly::{Anomaly, AnomalyKind, detect_anomalies};
pub use studio_capsule::{CapsuleChart, CapsuleOptions, export_capsule};
pub use studio_dsl::{
    NarrativeReference, RenderedNarrative, StudioCommand, apply_command, metric_references,
    parse_command, references_resolve, render_narrative, render_narrative_strict,
};
pub use studio_templates::{InstantiatedSection, LivingTemplate, TEMPLATES, template_by_id};

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
