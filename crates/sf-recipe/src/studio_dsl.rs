//! Studio command DSL and narrative references (`STUDIO-PHASE-2`).
//!
//! The AI layer never computes, never sets a metric value, and never invents a number. It may only
//! emit a [`StudioCommand`] JSON payload; Rust validates the payload, checks that every referenced
//! column and parameter exists, and executes it through [`DataGraph`]. Anything else is rejected
//! with a registered `SF-*` code and leaves the project unchanged.
//!
//! Narrative text uses `{{metric:ID}}` placeholders. Rendering resolves them from the graph's
//! tokens; a placeholder with no token is reported as missing and is never replaced by a guess.

use std::collections::BTreeSet;
use std::str::FromStr;

use rust_decimal::Decimal;
use serde::Deserialize;
use sf_core::datagraph::{MetricId, ScenarioId};
use sf_core::error::{ErrorCode, ErrorSeverity, SafiError};

use crate::datagraph::{ChangeOutcome, DataGraph};

/// Placeholder prefix used by narrative text.
pub const METRIC_REFERENCE_PREFIX: &str = "{{metric:";

/// Placeholder suffix used by narrative text.
pub const METRIC_REFERENCE_SUFFIX: &str = "}}";

fn invalid_command() -> SafiError {
    SafiError::new(
        ErrorCode::SF_CAL_002,
        ErrorSeverity::Error,
        "InvalidStudioCommand",
        "studio.invalid_command",
    )
}

fn unknown_metric() -> SafiError {
    SafiError::new(
        ErrorCode::SF_CAL_006,
        ErrorSeverity::Error,
        "UnknownMetric",
        "studio.unknown_metric",
    )
}

/// A validated Studio command produced by a user prompt or the AI interpreter.
///
/// `deny_unknown_fields` is deliberate: an unexpected field means the payload is not a Studio
/// command, and silently ignoring it would execute something the author did not intend.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum StudioCommand {
    /// Define a metric from a formula.
    DefineMetric {
        /// Stable metric identifier.
        id: String,
        /// Display name.
        name: String,
        /// Optional unit.
        unit: Option<String>,
        /// Formula source text.
        formula: String,
    },
    /// Replace a metric formula (bumps its formula version).
    UpdateMetricFormula {
        /// Existing metric identifier.
        id: String,
        /// New formula source text.
        formula: String,
    },
    /// Replace one cell of the working table.
    SetCell {
        /// Zero-based row.
        row: usize,
        /// Column name.
        column: String,
        /// New raw text.
        value: String,
    },
    /// Append a calculated column.
    AddCalculatedColumn {
        /// New column name.
        name: String,
        /// Per-row formula source text.
        formula: String,
    },
    /// Register a What-If parameter.
    DefineParameter {
        /// Parameter name used inside formulas.
        name: String,
        /// Default value as an exact decimal string.
        value: String,
    },
    /// Activate a scenario, or `null` to return to the defaults.
    ActivateScenario {
        /// Scenario identifier, or `null`.
        id: Option<String>,
    },
}

/// Parses a Studio command from JSON.
///
/// # Errors
/// Returns `SF-CAL-002` for malformed JSON, an unknown `op`, or an unexpected field.
pub fn parse_command(json: &str) -> Result<StudioCommand, SafiError> {
    if json.len() > 4_096 {
        return Err(invalid_command());
    }
    serde_json::from_str(json).map_err(|_| invalid_command())
}

/// Applies a validated command to the graph.
///
/// # Errors
/// Propagates the graph's `SF-CAL-*` validation errors and rejects decimal strings that are not
/// exact numbers.
pub fn apply_command(
    graph: &mut DataGraph,
    command: &StudioCommand,
    now_ms: u64,
) -> Result<ChangeOutcome, SafiError> {
    match command {
        StudioCommand::DefineMetric {
            id,
            name,
            unit,
            formula,
        } => graph.define_metric(MetricId::new(id)?, name, unit.as_deref(), formula, now_ms),
        StudioCommand::UpdateMetricFormula { id, formula } => {
            graph.update_metric_formula(&MetricId::new(id)?, formula, now_ms)
        }
        StudioCommand::SetCell { row, column, value } => {
            graph.set_cell(*row, column, value, now_ms)
        }
        StudioCommand::AddCalculatedColumn { name, formula } => {
            graph.add_calculated_column(name, formula, now_ms)
        }
        StudioCommand::DefineParameter { name, value } => {
            let value = Decimal::from_str(value).map_err(|_| invalid_command())?;
            graph.define_parameter(name, value, now_ms)?;
            Ok(plain_outcome(graph))
        }
        StudioCommand::ActivateScenario { id } => {
            let scenario = id.as_ref().map(ScenarioId::new).transpose()?;
            graph.activate_scenario(scenario.as_ref(), now_ms)?;
            Ok(plain_outcome(graph))
        }
    }
}

fn plain_outcome(graph: &DataGraph) -> ChangeOutcome {
    ChangeOutcome {
        revision: graph.revision(),
        recomputed: graph.last_recomputed().to_vec(),
        rows: graph.last_rows().to_vec(),
        columns: graph.last_columns().to_vec(),
    }
}

/// A resolved metric reference inside narrative text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NarrativeReference {
    /// Metric identifier the placeholder points at.
    pub metric_id: MetricId,
    /// Exact value as a display string, taken from the verified token.
    pub value: String,
    /// Optional unit.
    pub unit: Option<String>,
    /// Token fingerprint, so the document can be re-verified later.
    pub fingerprint: String,
}

/// Result of rendering narrative text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedNarrative {
    /// Text with every resolvable placeholder replaced.
    pub text: String,
    /// References that were resolved, in order of appearance.
    pub references: Vec<NarrativeReference>,
    /// Placeholders whose metric does not exist; export must be blocked while this is non-empty.
    pub missing: Vec<String>,
}

/// Lists metric identifiers referenced by narrative text, without resolving them.
#[must_use]
pub fn metric_references(template: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = template;
    while let Some(start) = rest.find(METRIC_REFERENCE_PREFIX) {
        let after_prefix = &rest[start + METRIC_REFERENCE_PREFIX.len()..];
        let Some(end) = after_prefix.find(METRIC_REFERENCE_SUFFIX) else {
            break;
        };
        let id = after_prefix[..end].trim();
        if !id.is_empty() {
            let _ = found.insert(id.to_owned());
        }
        rest = &after_prefix[end + METRIC_REFERENCE_SUFFIX.len()..];
    }
    found
}

/// Returns `true` when the text contains every reference it mentions.
#[must_use]
pub fn references_resolve(graph: &DataGraph, template: &str) -> bool {
    metric_references(template).iter().all(|id| {
        MetricId::new(id)
            .ok()
            .and_then(|id| graph.token(&id).cloned())
            .is_some()
    })
}

/// Renders narrative text, resolving `{{metric:ID}}` from verified tokens only.
///
/// No number is invented: a placeholder with no token is reported in [`RenderedNarrative::missing`]
/// and left untouched so an export can refuse to proceed.
///
/// # Errors
/// Returns `SF-CAL-006` when a referenced identifier is not a valid Studio identifier.
pub fn render_narrative(template: &str, graph: &DataGraph) -> Result<RenderedNarrative, SafiError> {
    let mut text = String::with_capacity(template.len());
    let mut references = Vec::new();
    let mut missing = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find(METRIC_REFERENCE_PREFIX) {
        text.push_str(&rest[..start]);
        let after_prefix = &rest[start + METRIC_REFERENCE_PREFIX.len()..];
        let Some(end) = after_prefix.find(METRIC_REFERENCE_SUFFIX) else {
            text.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let raw_id = after_prefix[..end].trim();
        let placeholder = &rest[start..start + METRIC_REFERENCE_PREFIX.len() + end + 2];
        let id = MetricId::new(raw_id)?;
        match graph.token(&id) {
            Some(token) => {
                text.push_str(&crate::datagraph_display(&token.value));
                references.push(NarrativeReference {
                    metric_id: id,
                    value: crate::datagraph_display(&token.value),
                    unit: token.unit.clone(),
                    fingerprint: token.fingerprint().to_owned(),
                });
            }
            None => {
                text.push_str(placeholder);
                missing.push(raw_id.to_owned());
            }
        }
        rest = &after_prefix[end + METRIC_REFERENCE_SUFFIX.len()..];
    }
    text.push_str(rest);
    Ok(RenderedNarrative {
        text,
        references,
        missing,
    })
}

/// Renders text and fails when a reference cannot be resolved, which is the export gate.
///
/// # Errors
/// `SF-CAL-006` when any placeholder has no verified token.
pub fn render_narrative_strict(
    template: &str,
    graph: &DataGraph,
) -> Result<RenderedNarrative, SafiError> {
    let rendered = render_narrative(template, graph)?;
    if rendered.missing.is_empty() {
        Ok(rendered)
    } else {
        Err(unknown_metric())
    }
}
