//! The five living Studio templates (`STUDIO-PHASE-4`).
//!
//! A template is a declarative skeleton of *roles*, not a picture. Instantiating it binds each role
//! to a real metric id supplied by the project and copies that metric's verified token, so the
//! rendered section always carries a value that Rust computed and a fingerprint that can be
//! re-checked. A role with no binding fails loudly instead of showing a zero.

use std::collections::BTreeMap;

use sf_core::datagraph::{MetricId, VerifiedMetricToken};
use sf_core::error::{ErrorCode, ErrorSeverity, SafiError};

/// One editable section of a template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TemplateSection {
    /// i18n key of the heading.
    pub heading_key: &'static str,
    /// Role the project must bind to a metric.
    pub role: &'static str,
}

/// A living template definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LivingTemplate {
    /// Stable template identifier.
    pub id: &'static str,
    /// i18n key of the template name.
    pub name_key: &'static str,
    /// Ordered sections.
    pub sections: &'static [TemplateSection],
}

const fn section(heading_key: &'static str, role: &'static str) -> TemplateSection {
    TemplateSection { heading_key, role }
}

const BOARDROOM: &[TemplateSection] = &[
    section("studio.template.summary", "headline"),
    section("studio.template.kpis", "primary_metric"),
    section("studio.template.secondary", "secondary_metric"),
    section("studio.template.recommendations", "growth"),
    section("studio.template.risks", "risk"),
];

const VELOCITY: &[TemplateSection] = &[
    section("studio.template.trend", "trend"),
    section("studio.template.comparison", "primary_metric"),
    section("studio.template.growth", "growth"),
    section("studio.template.distribution", "distribution"),
    section("studio.template.sales", "secondary_metric"),
];

const IMPACT: &[TemplateSection] = &[
    section("studio.template.coverage", "coverage"),
    section("studio.template.spatial", "distribution"),
    section("studio.template.operations", "primary_metric"),
    section("studio.template.impact", "secondary_metric"),
];

const FORENSIC: &[TemplateSection] = &[
    section("studio.template.quality", "quality"),
    section("studio.template.outliers", "outliers"),
    section("studio.template.contradictions", "contradictions"),
    section("studio.template.cleaning_log", "cleaning"),
    section("studio.template.provenance", "primary_metric"),
];

const TIME_MACHINE: &[TemplateSection] = &[
    section("studio.template.timeline", "trend"),
    section("studio.template.period_comparison", "growth"),
    section("studio.template.scenarios", "scenario"),
    section("studio.template.forecast", "forecast"),
];

/// The five shipped templates.
pub const TEMPLATES: [LivingTemplate; 5] = [
    LivingTemplate {
        id: "boardroom-keynote",
        name_key: "studio.template.boardroom",
        sections: BOARDROOM,
    },
    LivingTemplate {
        id: "velocity-radar",
        name_key: "studio.template.velocity",
        sections: VELOCITY,
    },
    LivingTemplate {
        id: "impact-atlas",
        name_key: "studio.template.impact",
        sections: IMPACT,
    },
    LivingTemplate {
        id: "forensic-xray",
        name_key: "studio.template.forensic",
        sections: FORENSIC,
    },
    LivingTemplate {
        id: "time-machine",
        name_key: "studio.template.time_machine",
        sections: TIME_MACHINE,
    },
];

/// Looks a template up by its stable identifier.
#[must_use]
pub fn template_by_id(id: &str) -> Option<&'static LivingTemplate> {
    TEMPLATES.iter().find(|template| template.id == id)
}

/// A template section bound to a real verified token.
#[derive(Debug, Clone, PartialEq)]
pub struct InstantiatedSection {
    /// i18n key of the heading.
    pub heading_key: &'static str,
    /// Role that was bound.
    pub role: &'static str,
    /// Verified token the section must render.
    pub token: VerifiedMetricToken,
}

fn unknown_metric() -> SafiError {
    SafiError::new(
        ErrorCode::SF_CAL_006,
        ErrorSeverity::Error,
        "UnknownMetric",
        "studio.unknown_metric",
    )
}

/// Binds a template's roles to the project's metrics.
///
/// # Errors
/// `SF-CAL-006` when a role has no binding or the bound metric has no verified token.
pub fn instantiate(
    template: &LivingTemplate,
    graph: &crate::datagraph::DataGraph,
    bindings: &BTreeMap<String, MetricId>,
) -> Result<Vec<InstantiatedSection>, SafiError> {
    let mut sections = Vec::with_capacity(template.sections.len());
    for section in template.sections {
        let id = bindings.get(section.role).ok_or_else(unknown_metric)?;
        let token = graph.token(id).cloned().ok_or_else(unknown_metric)?;
        sections.push(InstantiatedSection {
            heading_key: section.heading_key,
            role: section.role,
            token,
        });
    }
    Ok(sections)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use sf_core::column::{Column, ColumnSchema, ColumnType, ConfidenceScore};
    use sf_core::datagraph::{MetricId, SourceId};
    use sf_core::error::SafiError;
    use sf_core::table::Table;
    use sf_core::value::CellValue;

    use super::{TEMPLATES, instantiate, template_by_id};
    use crate::datagraph::DataGraph;

    fn graph() -> Result<DataGraph, SafiError> {
        let columns = [
            ("region", ["Cairo", "Giza", "Cairo"].as_slice()),
            ("amount", ["100", "200", "50"].as_slice()),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (name, values))| {
            Column::new(
                ColumnSchema::new(index, name, ColumnType::Unknown, ConfidenceScore::ONE),
                values
                    .iter()
                    .map(|value| CellValue::from_raw(*value))
                    .collect(),
            )
        })
        .collect();
        let table = Table::new("tbl_sales", "sales", columns)?;
        let mut graph = DataGraph::new(SourceId::new("sales")?, table, 10)?;
        graph.define_metric(
            MetricId::new("total")?,
            "Total amount",
            Some("EGP"),
            "SUM(amount)",
            10,
        )?;
        Ok(graph)
    }

    #[test]
    fn the_five_templates_exist_with_stable_unique_ids() {
        assert_eq!(TEMPLATES.len(), 5);
        for template in &TEMPLATES {
            assert!(!template.sections.is_empty());
            assert!(template_by_id(template.id).is_some());
        }
        assert!(template_by_id("unknown-template").is_none());
    }

    #[test]
    fn instantiation_binds_real_verified_tokens() -> Result<(), SafiError> {
        let graph = graph()?;
        let template = template_by_id("boardroom-keynote").ok_or_else(super::unknown_metric)?;
        let mut bindings = BTreeMap::new();
        for section in template.sections {
            let _ = bindings.insert(section.role.to_owned(), MetricId::new("total")?);
        }
        let sections = instantiate(template, &graph, &bindings)?;
        assert_eq!(sections.len(), template.sections.len());
        let token = graph
            .token(&MetricId::new("total")?)
            .ok_or_else(super::unknown_metric)?;
        for section in &sections {
            assert_eq!(section.token.fingerprint(), token.fingerprint());
            assert!(section.token.is_verifiable());
        }
        Ok(())
    }

    #[test]
    fn a_missing_binding_fails_instead_of_showing_a_zero() -> Result<(), SafiError> {
        let graph = graph()?;
        let template = template_by_id("forensic-xray").ok_or_else(super::unknown_metric)?;
        let bindings = BTreeMap::new();
        let error = instantiate(template, &graph, &bindings).err();
        assert_eq!(
            error.map(|error| error.code()),
            Some(sf_core::error::ErrorCode::SF_CAL_006)
        );
        Ok(())
    }
}
