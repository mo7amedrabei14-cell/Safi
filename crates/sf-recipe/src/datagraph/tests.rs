//! Acceptance tests for `STUDIO-PHASE-1`: one source of truth, verified tokens, incremental
//! recomputation, What-If, and undo/redo that never touches the original data.
//!
//! All data here is synthetic and local (`AGENTS.md` Rule 4).

use std::collections::BTreeMap;
use std::str::FromStr;

use rust_decimal::Decimal;
use sf_core::column::{Column, ColumnSchema, ColumnType, ConfidenceScore};
use sf_core::datagraph::{
    MetricId, ScenarioId, SourceId, VerificationOutcome, VerifiedMetricToken,
};
use sf_core::error::{ErrorCode, SafiError};
use sf_core::table::Table;
use sf_core::value::{CellValue, Value};

use super::{DataGraph, cell_number, tables_equal};

fn table(columns: &[(&str, &[&str])]) -> Result<Table, SafiError> {
    let columns = columns
        .iter()
        .enumerate()
        .map(|(index, (name, values))| {
            Column::new(
                ColumnSchema::new(index, *name, ColumnType::Unknown, ConfidenceScore::ONE),
                values
                    .iter()
                    .map(|value| CellValue::from_raw(*value))
                    .collect(),
            )
        })
        .collect();
    Table::new("tbl_sales", "sales", columns)
}

fn sales() -> Result<Table, SafiError> {
    table(&[
        ("region", &["Cairo", "Giza", "Cairo"]),
        ("amount", &["100", "200", "50.5"]),
        ("quantity", &["2", "1", "4"]),
    ])
}

fn graph() -> Result<DataGraph, SafiError> {
    DataGraph::new(SourceId::new("sales")?, sales()?, 10)
}

fn metric(id: &str) -> Result<MetricId, SafiError> {
    MetricId::new(id)
}

fn decimal(amount: &str) -> Result<Decimal, SafiError> {
    Decimal::from_str(amount).map_err(|_| missing())
}

fn missing() -> SafiError {
    SafiError::new(
        ErrorCode::SF_CAL_006,
        sf_core::error::ErrorSeverity::Error,
        "UnknownMetric",
        "studio.unknown_metric",
    )
}

fn token(graph: &DataGraph, id: &MetricId) -> Result<VerifiedMetricToken, SafiError> {
    graph.token(id).cloned().ok_or_else(missing)
}

fn value_of(token: &VerifiedMetricToken) -> Result<Decimal, SafiError> {
    match &token.value {
        Value::Decimal(decimal) | Value::Percentage(decimal) => Ok(*decimal),
        Value::Integer(integer) => Ok(Decimal::from(*integer)),
        _ => Err(missing()),
    }
}

#[test]
fn editing_a_cell_recomputes_only_dependent_metrics() -> Result<(), SafiError> {
    let mut graph = graph()?;
    let sum_amount = metric("sum_amount")?;
    let sum_quantity = metric("sum_quantity")?;
    graph.define_metric(
        sum_amount.clone(),
        "Total amount",
        Some("EGP"),
        "SUM(amount)",
        10,
    )?;
    graph.define_metric(
        sum_quantity.clone(),
        "Total quantity",
        None,
        "SUM(quantity)",
        10,
    )?;

    assert_eq!(value_of(&token(&graph, &sum_amount)?)?, decimal("350.5")?);
    assert_eq!(value_of(&token(&graph, &sum_quantity)?)?, decimal("7")?);

    let untouched = token(&graph, &sum_quantity)?;
    let outcome = graph.set_cell(0, "amount", "150", 20)?;

    assert_eq!(outcome.recomputed, vec![sum_amount.clone()]);
    assert_eq!(value_of(&token(&graph, &sum_amount)?)?, decimal("400.5")?);
    let still_untouched = token(&graph, &sum_quantity)?;
    assert_eq!(untouched.fingerprint(), still_untouched.fingerprint());
    assert_eq!(untouched.computed_at_ms, 10);
    assert_eq!(still_untouched.computed_at_ms, 10);
    assert!(still_untouched.is_verifiable());
    Ok(())
}

#[test]
fn the_original_dataset_survives_every_edit_and_undo() -> Result<(), SafiError> {
    let mut graph = graph()?;
    let origin = graph.origin().clone();
    graph.set_cell(0, "amount", "999", 20)?;
    assert!(graph.origin_unchanged());
    assert!(tables_equal(graph.origin(), &origin));
    assert_eq!(
        graph.working().cell(0, 1).map(CellValue::raw_text),
        Some("999")
    );

    graph.undo(30)?;
    assert!(tables_equal(graph.working(), &origin));
    assert!(graph.origin_unchanged());

    graph.redo(40)?;
    assert_eq!(
        graph.working().cell(0, 1).map(CellValue::raw_text),
        Some("999")
    );
    assert!(graph.origin_unchanged());
    Ok(())
}

#[test]
fn replay_reproduces_the_working_table_and_is_deterministic() -> Result<(), SafiError> {
    let mut first = graph()?;
    let mut second = graph()?;
    let sum = metric("sum_amount")?;
    for graph in [&mut first, &mut second] {
        graph.add_calculated_column("total", "amount * quantity", 10)?;
        graph.set_cell(1, "amount", "220", 10)?;
        graph.define_metric(sum.clone(), "Total", Some("EGP"), "SUM(total)", 10)?;
    }
    assert!(tables_equal(first.working(), &first.replay()?));
    assert!(tables_equal(first.working(), second.working()));
    let left = token(&first, &sum)?;
    let right = token(&second, &sum)?;
    assert_eq!(left.fingerprint(), right.fingerprint());
    assert_eq!(left.value, right.value);
    Ok(())
}

#[test]
fn a_new_dataset_version_invalidates_previous_verification() -> Result<(), SafiError> {
    let mut graph = graph()?;
    let sum = metric("sum_amount")?;
    graph.define_metric(sum.clone(), "Total amount", Some("EGP"), "SUM(amount)", 10)?;
    let before = token(&graph, &sum)?;

    let updated = table(&[
        ("region", &["Cairo", "Giza", "Cairo"]),
        ("amount", &["1000", "200", "50.5"]),
        ("quantity", &["2", "1", "4"]),
    ])?;
    graph.replace_source(updated, 50)?;
    let after = token(&graph, &sum)?;

    assert_eq!(before.source_version.get(), 0);
    assert_eq!(after.source_version.get(), 1);
    assert_eq!(value_of(&after)?, decimal("1250.5")?);
    assert!(matches!(
        before.verify_against(&after),
        VerificationOutcome::Mismatch { .. }
    ));
    assert_eq!(graph.dataset_version().get(), 1);
    Ok(())
}

#[test]
fn a_new_formula_version_invalidates_previous_verification() -> Result<(), SafiError> {
    let mut graph = graph()?;
    let sum = metric("sum_amount")?;
    graph.define_metric(sum.clone(), "Total amount", Some("EGP"), "SUM(amount)", 10)?;
    let before = token(&graph, &sum)?;

    graph.update_metric_formula(&sum, "SUM(amount) * 2", 60)?;
    let after = token(&graph, &sum)?;

    assert_eq!(before.formula_version, 1);
    assert_eq!(after.formula_version, 2);
    assert_eq!(value_of(&after)?, decimal("701")?);
    assert_eq!(
        graph.verify(&before)?,
        VerificationOutcome::Mismatch {
            expected: before.fingerprint().to_owned(),
            actual: after.fingerprint().to_owned(),
        }
    );
    graph.verify_strict(&after)?;
    assert_eq!(
        graph.verify_strict(&before).err().map(|error| error.code()),
        Some(ErrorCode::SF_CAL_008)
    );
    Ok(())
}

#[test]
fn a_displayed_number_that_no_longer_matches_is_detected() -> Result<(), SafiError> {
    let mut graph = graph()?;
    let sum = metric("sum_amount")?;
    graph.define_metric(sum.clone(), "Total amount", Some("EGP"), "SUM(amount)", 10)?;
    let honest = token(&graph, &sum)?;

    let mut tampered = honest.clone();
    tampered.value = Value::Decimal(decimal("9999")?);
    match graph.verify(&tampered)? {
        VerificationOutcome::Mismatch { expected, actual } => {
            assert_eq!(expected, honest.fingerprint());
            assert_eq!(actual, honest.fingerprint());
            assert_ne!(tampered.value, honest.value);
        }
        VerificationOutcome::Verified => panic!("a tampered value must not verify"),
    }
    assert_eq!(
        graph
            .verify_strict(&tampered)
            .err()
            .map(|error| error.code()),
        Some(ErrorCode::SF_CAL_008)
    );
    Ok(())
}

#[test]
fn every_reported_row_is_reachable_in_the_working_table() -> Result<(), SafiError> {
    let mut graph = graph()?;
    let sum = metric("sum_amount")?;
    let count = metric("count_region")?;
    graph.define_metric(sum.clone(), "Total amount", Some("EGP"), "SUM(amount)", 10)?;
    graph.define_metric(count.clone(), "Rows", None, "COUNT(region)", 10)?;

    let sum_token = token(&graph, &sum)?;
    assert_eq!(sum_token.row_refs, vec![0, 1, 2]);
    let mut total = Decimal::ZERO;
    for row in &sum_token.row_refs {
        let cell = graph.working().cell(*row as usize, 1).ok_or_else(missing)?;
        total += cell_number(cell).ok_or_else(missing)?;
    }
    assert_eq!(total, value_of(&sum_token)?);

    let count_token = token(&graph, &count)?;
    assert_eq!(value_of(&count_token)?, decimal("3")?);
    assert_eq!(count_token.row_refs, vec![0, 1, 2]);
    Ok(())
}

#[test]
fn calculated_columns_follow_their_inputs_and_metrics_follow_them() -> Result<(), SafiError> {
    let mut graph = graph()?;
    graph.add_calculated_column("total", "amount * quantity", 10)?;
    let sum_total = metric("sum_total")?;
    graph.define_metric(sum_total.clone(), "Total", Some("EGP"), "SUM(total)", 10)?;
    assert_eq!(value_of(&token(&graph, &sum_total)?)?, decimal("602")?);

    let outcome = graph.set_cell(0, "quantity", "3", 70)?;
    assert_eq!(outcome.recomputed, vec![sum_total.clone()]);
    assert_eq!(value_of(&token(&graph, &sum_total)?)?, decimal("702")?);
    assert_eq!(
        graph.working().cell(0, 3).and_then(cell_number),
        Some(decimal("300")?)
    );
    assert_eq!(
        graph.set_cell(0, "total", "1", 80).err().map(|e| e.code()),
        Some(ErrorCode::SF_CAL_004)
    );
    Ok(())
}

#[test]
fn what_if_scenarios_change_assumptions_without_touching_data() -> Result<(), SafiError> {
    let mut graph = graph()?;
    let projected = metric("projected")?;
    graph.define_parameter("growth", Decimal::ONE, 10)?;
    graph.define_metric(
        projected.clone(),
        "Projected amount",
        Some("EGP"),
        "SUM(amount) * growth",
        10,
    )?;
    assert_eq!(value_of(&token(&graph, &projected)?)?, decimal("350.5")?);

    let doubled = ScenarioId::new("doubled")?;
    let mut parameters = BTreeMap::new();
    let _ = parameters.insert("growth".to_owned(), decimal("2")?);
    graph.add_scenario(doubled.clone(), "Doubled growth", parameters)?;
    graph.activate_scenario(Some(&doubled), 90)?;

    assert_eq!(value_of(&token(&graph, &projected)?)?, decimal("701")?);
    assert!(graph.origin_unchanged());
    assert!(tables_equal(graph.working(), &graph.replay()?));

    graph.reset_scenario(95)?;
    assert_eq!(value_of(&token(&graph, &projected)?)?, decimal("350.5")?);
    Ok(())
}

#[test]
fn scenario_comparison_reports_both_values() -> Result<(), SafiError> {
    let mut graph = graph()?;
    let projected = metric("projected")?;
    graph.define_parameter("growth", Decimal::ONE, 10)?;
    graph.define_metric(
        projected.clone(),
        "Projected",
        Some("EGP"),
        "SUM(amount) * growth",
        10,
    )?;
    let base = ScenarioId::new("base")?;
    let doubled = ScenarioId::new("doubled")?;
    graph.add_scenario(base.clone(), "Base", BTreeMap::new())?;
    let mut parameters = BTreeMap::new();
    let _ = parameters.insert("growth".to_owned(), decimal("2")?);
    graph.add_scenario(doubled.clone(), "Doubled", parameters)?;

    let comparison = graph.compare_scenarios(&base, &doubled, 100)?;
    assert_eq!(comparison.len(), 1);
    let first = comparison.first().ok_or_else(missing)?;
    assert_eq!(first.metric_id, projected);
    assert_eq!(first.left, decimal("350.5")?);
    assert_eq!(first.right, decimal("701")?);
    Ok(())
}

#[test]
fn invalid_formulas_report_registered_sf_codes() -> Result<(), SafiError> {
    let mut graph = graph()?;
    assert_eq!(
        graph
            .define_metric(metric("bad_column")?, "Bad", None, "SUM(unknown)", 10)
            .err()
            .map(|error| error.code()),
        Some(ErrorCode::SF_CAL_001)
    );
    assert_eq!(
        graph
            .define_metric(metric("bad_syntax")?, "Bad", None, "SUM(", 10)
            .err()
            .map(|error| error.code()),
        Some(ErrorCode::SF_CAL_002)
    );
    let bad_type = metric("bad_type")?;
    graph.define_metric(bad_type.clone(), "Bad", None, "SUM(region)", 10)?;
    assert_eq!(
        graph.metric_error(&bad_type).map(|error| error.code()),
        Some(ErrorCode::SF_CAL_004)
    );
    assert!(graph.token(&bad_type).is_none());
    graph.define_metric(metric("zero")?, "Zero", None, "SUM(amount) / 0", 10)?;
    assert_eq!(
        graph
            .metric_error(&metric("zero")?)
            .map(|error| error.code()),
        Some(ErrorCode::SF_CAL_003)
    );
    graph.define_metric(metric("rate")?, "Rate", None, "SUM(amount) * rate", 10)?;
    assert_eq!(
        graph
            .metric_error(&metric("rate")?)
            .map(|error| error.code()),
        Some(ErrorCode::SF_CAL_005)
    );
    Ok(())
}
