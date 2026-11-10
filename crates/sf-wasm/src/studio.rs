//! Studio WebAssembly bindings (`STUDIO-PHASE-1` bridge).
//!
//! The whole Studio state lives inside Rust inside the worker: the browser holds only the
//! serialized state snapshot it received, so no view can compute or alter a metric locally.
//! Every method returns JSON produced from the engine, and every error is a registered `SF-*`
//! code so the UI never invents a message.

use std::collections::BTreeMap;
use std::io::Cursor;

use wasm_bindgen::prelude::*;

use sf_core::datagraph::{MetricId, ScenarioId, SourceId, VerifiedMetricToken};
use sf_core::error::SafiError;
use sf_core::limits::PlatformLimits;
use sf_core::table::Table;
use sf_core::value::Value;
use sf_io::read_excel_stream;
use sf_recipe::datagraph::{AggFunc, DataGraph, datagraph_display};
use sf_recipe::studio_anomaly::detect_anomalies;
use sf_recipe::studio_capsule::{CapsuleChart, CapsuleOptions, export_capsule};
use sf_recipe::studio_dsl::{apply_command, parse_command, render_narrative};

impl core::fmt::Debug for StudioSession {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("StudioSession")
            .field("revision", &self.graph.revision())
            .finish()
    }
}

fn js_error(error: &SafiError) -> JsValue {
    JsValue::from_str(&error.code().as_code_string())
}

fn now_ms(value: f64) -> u64 {
    if value.is_finite() && value > 0.0 {
        value as u64
    } else {
        0
    }
}

/// Maximum accepted import payload (`50 MiB`, matching the dropzone limit).
pub const MAX_STUDIO_IMPORT_BYTES: usize = 50 * 1024 * 1024;

/// One live Studio project.
#[wasm_bindgen]
pub struct StudioSession {
    graph: DataGraph,
}

fn state_json(graph: &DataGraph) -> Result<String, JsValue> {
    let mut columns = Vec::new();
    for column in graph.working().columns() {
        columns.push(serde_json::json!({
            "name": column.schema().name,
            "type": format!("{:?}", column.schema().column_type),
        }));
    }
    let mut rows: Vec<Vec<String>> = Vec::with_capacity(graph.working().row_count());
    for row in 0..graph.working().row_count() {
        let mut cells = Vec::with_capacity(graph.working().column_count());
        for column in graph.working().columns() {
            cells.push(
                column
                    .cell_at(row)
                    .map(|cell| cell.visible_text().to_owned())
                    .unwrap_or_default(),
            );
        }
        rows.push(cells);
    }
    let mut metrics = Vec::new();
    for id in graph.metric_ids() {
        let Some(token) = graph.token(&id) else {
            continue;
        };
        metrics.push(token_json(token));
    }
    let mut errors = Vec::new();
    for id in graph.metric_ids() {
        if let Some(error) = graph.metric_error(&id) {
            errors.push(serde_json::json!({
                "id": id.as_str(),
                "code": error.code().as_code_string(),
            }));
        }
    }
    let anomalies = detect_anomalies(graph.working())
        .into_iter()
        .map(|anomaly| {
            serde_json::json!({
                "kind": format!("{:?}", anomaly.kind),
                "messageKey": anomaly.kind.message_key(),
                "methodKey": anomaly.kind.method_key(),
                "suggestionKey": anomaly.kind.suggestion_key(),
                "column": anomaly.column,
                "rows": anomaly.rows,
            })
        })
        .collect::<Vec<_>>();
    let payload = serde_json::json!({
        "sourceId": graph.source_id().as_str(),
        "revision": graph.revision().get(),
        "datasetVersion": graph.dataset_version().get(),
        "originFingerprint": graph.origin_fingerprint(),
        "columns": columns,
        "rows": rows,
        "metrics": metrics,
        "errors": errors,
        "anomalies": anomalies,
        "calculated": graph.calculated_columns(),
        "lastRecomputed": graph.last_recomputed().iter().map(MetricId::as_str).collect::<Vec<_>>(),
        "activeScenario": graph.active_scenario().map(ScenarioId::as_str),
        "scenarios": graph.scenarios().keys().map(ScenarioId::as_str).collect::<Vec<_>>(),
        "parameters": graph
            .parameters()
            .iter()
            .map(|(name, value)| serde_json::json!({ "name": name, "value": value.to_string() }))
            .collect::<Vec<_>>(),
    });
    serde_json::to_string(&payload).map_err(|_| JsValue::from_str("SF-SYS-001"))
}

fn token_json(token: &VerifiedMetricToken) -> serde_json::Value {
    serde_json::json!({
        "id": token.metric_id.as_str(),
        "name": token.metric_name,
        "value": datagraph_display(&token.value),
        "unit": token.unit,
        "formula": token.formula,
        "formulaVersion": token.formula_version,
        "sourceVersion": token.source_version.get(),
        "fingerprint": token.fingerprint(),
        "status": format!("{:?}", token.status),
        "rows": token.row_refs,
        "columns": token.column_refs,
        "engineVersion": token.engine_version,
        "computedAtMs": token.computed_at_ms,
    })
}

#[wasm_bindgen]
impl StudioSession {
    /// Creates a session over an imported table (JSON produced by the import functions below).
    #[wasm_bindgen(constructor)]
    pub fn new(table_json: &str, clock_ms: f64) -> Result<Self, JsValue> {
        let table: Table =
            serde_json::from_str(table_json).map_err(|_| JsValue::from_str("SF-IMP-001"))?;
        let graph = DataGraph::new(
            SourceId::new("source").map_err(|error| js_error(&error))?,
            table,
            now_ms(clock_ms),
        )
        .map_err(|error| js_error(&error))?;
        Ok(Self { graph })
    }

    /// Restores a saved project and re-derives every metric before returning it.
    pub fn restore(snapshot_json: &str, clock_ms: f64) -> Result<Self, JsValue> {
        let graph = DataGraph::from_json(snapshot_json, now_ms(clock_ms))
            .map_err(|error| js_error(&error))?;
        Ok(Self { graph })
    }

    /// Returns the full UI state: table, tokens, errors, anomalies, scenarios.
    pub fn state_json(&self) -> Result<String, JsValue> {
        state_json(&self.graph)
    }

    /// Returns the persisted project JSON.
    pub fn snapshot_json(&self) -> Result<String, JsValue> {
        self.graph.to_json().map_err(|error| js_error(&error))
    }

    /// Replaces one cell and recomputes only dependent metrics.
    pub fn set_cell(
        &mut self,
        row: usize,
        column: &str,
        raw: &str,
        clock_ms: f64,
    ) -> Result<String, JsValue> {
        self.graph
            .set_cell(row, column, raw, now_ms(clock_ms))
            .map_err(|error| js_error(&error))?;
        state_json(&self.graph)
    }

    /// Appends a calculated column.
    pub fn add_calculated_column(
        &mut self,
        name: &str,
        formula: &str,
        clock_ms: f64,
    ) -> Result<String, JsValue> {
        self.graph
            .add_calculated_column(name, formula, now_ms(clock_ms))
            .map_err(|error| js_error(&error))?;
        state_json(&self.graph)
    }

    /// Defines a metric and computes its verified token.
    pub fn define_metric(
        &mut self,
        id: &str,
        name: &str,
        unit: Option<String>,
        formula: &str,
        clock_ms: f64,
    ) -> Result<String, JsValue> {
        let id = MetricId::new(id).map_err(|error| js_error(&error))?;
        self.graph
            .define_metric(id, name, unit.as_deref(), formula, now_ms(clock_ms))
            .map_err(|error| js_error(&error))?;
        state_json(&self.graph)
    }

    /// Registers a What-If parameter.
    pub fn define_parameter(
        &mut self,
        name: &str,
        value: &str,
        clock_ms: f64,
    ) -> Result<String, JsValue> {
        let value: sf_core::Decimal = value.parse().map_err(|_| JsValue::from_str("SF-CAL-002"))?;
        self.graph
            .define_parameter(name, value, now_ms(clock_ms))
            .map_err(|error| js_error(&error))?;
        state_json(&self.graph)
    }

    /// Adds a scenario from a JSON object of parameter overrides.
    pub fn add_scenario(
        &mut self,
        id: &str,
        name: &str,
        parameters_json: &str,
        clock_ms: f64,
    ) -> Result<String, JsValue> {
        let raw: BTreeMap<String, String> =
            serde_json::from_str(parameters_json).map_err(|_| JsValue::from_str("SF-CAL-002"))?;
        let mut parameters = BTreeMap::new();
        for (key, value) in raw {
            let parsed = value.parse().map_err(|_| JsValue::from_str("SF-CAL-002"))?;
            let _ = parameters.insert(key, parsed);
        }
        self.graph
            .add_scenario(
                ScenarioId::new(id).map_err(|error| js_error(&error))?,
                name,
                parameters,
            )
            .map_err(|error| js_error(&error))?;
        self.graph
            .activate_scenario(
                Some(&ScenarioId::new(id).map_err(|error| js_error(&error))?),
                now_ms(clock_ms),
            )
            .map_err(|error| js_error(&error))?;
        state_json(&self.graph)
    }

    /// Activates a scenario, or `None` to return to the defaults.
    pub fn activate_scenario(
        &mut self,
        id: Option<String>,
        clock_ms: f64,
    ) -> Result<String, JsValue> {
        let scenario = match id {
            Some(id) => Some(ScenarioId::new(id).map_err(|error| js_error(&error))?),
            None => None,
        };
        self.graph
            .activate_scenario(scenario.as_ref(), now_ms(clock_ms))
            .map_err(|error| js_error(&error))?;
        state_json(&self.graph)
    }

    /// Undoes the last change (the original dataset is never touched).
    pub fn undo(&mut self, clock_ms: f64) -> Result<String, JsValue> {
        self.graph
            .undo(now_ms(clock_ms))
            .map_err(|error| js_error(&error))?;
        state_json(&self.graph)
    }

    /// Redoes the last undone change.
    pub fn redo(&mut self, clock_ms: f64) -> Result<String, JsValue> {
        self.graph
            .redo(now_ms(clock_ms))
            .map_err(|error| js_error(&error))?;
        state_json(&self.graph)
    }

    /// Executes a validated Studio DSL command (the only path an AI interpreter may use).
    pub fn apply_command(&mut self, json: &str, clock_ms: f64) -> Result<String, JsValue> {
        let command = parse_command(json).map_err(|error| js_error(&error))?;
        apply_command(&mut self.graph, &command, now_ms(clock_ms))
            .map_err(|error| js_error(&error))?;
        state_json(&self.graph)
    }

    /// Groups and aggregates in Rust for the dashboard and the capsule.
    pub fn grouped(&self, function: &str, column: &str, group_by: &str) -> Result<String, JsValue> {
        let function =
            AggFunc::from_name(function).ok_or_else(|| JsValue::from_str("SF-CAL-002"))?;
        let groups = self
            .graph
            .grouped_aggregate(function, column, group_by)
            .map_err(|error| js_error(&error))?;
        let payload = groups
            .into_iter()
            .map(|group| {
                serde_json::json!({
                    "label": group.label,
                    "value": group.value.to_string(),
                })
            })
            .collect::<Vec<_>>();
        serde_json::to_string(&payload).map_err(|_| JsValue::from_str("SF-SYS-001"))
    }

    /// Resolves `{{metric:ID}}` placeholders from verified tokens only.
    pub fn narrative(&self, template: &str) -> Result<String, JsValue> {
        let rendered = render_narrative(template, &self.graph).map_err(|error| js_error(&error))?;
        serde_json::to_string(&serde_json::json!({
            "text": rendered.text,
            "missing": rendered.missing,
            "references": rendered.references.iter().map(|reference| serde_json::json!({
                "id": reference.metric_id.as_str(),
                "value": reference.value,
                "fingerprint": reference.fingerprint,
            })).collect::<Vec<_>>(),
        }))
        .map_err(|_| JsValue::from_str("SF-SYS-001"))
    }

    /// Exports the self-contained `.safi.html` capsule (no JavaScript inside).
    pub fn capsule(&self, options_json: &str) -> Result<String, JsValue> {
        let options: CapsuleInput =
            serde_json::from_str(options_json).map_err(|_| JsValue::from_str("SF-CAL-002"))?;
        let charts = options
            .charts
            .into_iter()
            .map(|chart| CapsuleChart {
                title: chart.title,
                function: AggFunc::from_name(&chart.function).unwrap_or(AggFunc::Sum),
                column: chart.column,
                group_by: chart.group_by,
            })
            .collect();
        export_capsule(
            &self.graph,
            &CapsuleOptions {
                title: options.title,
                created_at: options.created_at,
                charts,
            },
        )
        .map_err(|error| js_error(&error))
    }
}

#[derive(serde::Deserialize)]
struct CapsuleInput {
    title: String,
    created_at: String,
    charts: Vec<CapsuleChartInput>,
}

#[derive(serde::Deserialize)]
struct CapsuleChartInput {
    title: String,
    function: String,
    column: String,
    group_by: String,
}

/// Imports CSV bytes into a table JSON the session can open.
#[wasm_bindgen]
pub fn studio_import_csv(data: &[u8], sheet_name: &str) -> Result<String, JsValue> {
    if data.len() > MAX_STUDIO_IMPORT_BYTES {
        return Err(JsValue::from_str("SF-IMP-003"));
    }
    let sheet = sf_io::read_csv_stream(data, PlatformLimits::WEB_PWA, sheet_name, "tbl_1", "table")
        .map_err(|error| js_error(&error))?;
    let table = sheet
        .primary_table()
        .ok_or_else(|| JsValue::from_str("SF-IMP-001"))?;
    serde_json::to_string(table).map_err(|_| JsValue::from_str("SF-SYS-001"))
}

/// Imports the first table of the first sheet of an XLSX/ODS workbook.
#[wasm_bindgen]
pub fn studio_import_excel(data: &[u8]) -> Result<String, JsValue> {
    if data.len() > MAX_STUDIO_IMPORT_BYTES {
        return Err(JsValue::from_str("SF-IMP-003"));
    }
    let sheets = read_excel_stream(
        Cursor::new(data),
        data.len() as u64,
        PlatformLimits::WEB_PWA,
    )
    .map_err(|error| js_error(&error))?;
    let table = sheets
        .first()
        .and_then(|sheet| sheet.primary_table())
        .ok_or_else(|| JsValue::from_str("SF-IMP-001"))?;
    serde_json::to_string(table).map_err(|_| JsValue::from_str("SF-SYS-001"))
}

/// Returns the display form of a metric value for tests and tooling.
#[wasm_bindgen]
pub fn studio_display_value(value: &str) -> String {
    datagraph_display(&Value::Text(value.to_owned()))
}

// Test code must fail fast: a broken engine fixture has to abort the test, so the
// panic-on-error restriction (which targets shipped code) is lifted for this module only.
#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod studio_bridge_tests {
    use super::*;

    const CSV: &[u8] = b"region,amount
Cairo,100
Giza,50
Cairo,25
";

    fn session() -> StudioSession {
        let table = studio_import_csv(CSV, "Sheet1").expect("csv import");
        StudioSession::new(&table, 1_000.0).expect("session")
    }

    fn snapshot_of(session: &StudioSession) -> serde_json::Value {
        serde_json::from_str(&session.state_json().expect("state")).expect("json")
    }

    #[test]
    fn imports_csv_into_a_live_table() {
        let state = snapshot_of(&session());
        assert_eq!(state["columns"][0]["name"], "region");
        assert_eq!(state["rows"].as_array().map(Vec::len), Some(3));
        assert_eq!(state["rows"][0][0], "Cairo");
        assert_eq!(state["revision"], 0);
    }

    #[test]
    fn editing_a_cell_updates_the_verified_metric_and_its_fingerprint() {
        let mut session = session();
        session
            .define_metric(
                "total",
                "Total",
                Some("EGP".to_owned()),
                "SUM(amount)",
                1_000.0,
            )
            .expect("metric");
        let before = snapshot_of(&session);
        assert_eq!(before["metrics"][0]["value"], "175");
        assert_eq!(before["metrics"][0]["status"], "Verified");

        session.set_cell(0, "amount", "200", 2_000.0).expect("edit");
        let after = snapshot_of(&session);
        assert_eq!(after["metrics"][0]["value"], "275");
        assert_ne!(
            after["metrics"][0]["fingerprint"],
            before["metrics"][0]["fingerprint"]
        );
        assert_eq!(after["lastRecomputed"][0], "total");
    }

    #[test]
    fn undo_restores_the_previous_value() {
        let mut session = session();
        session
            .define_metric("total", "Total", None, "SUM(amount)", 1_000.0)
            .expect("metric");
        session.set_cell(0, "amount", "200", 2_000.0).expect("edit");
        session.undo(3_000.0).expect("undo");
        assert_eq!(snapshot_of(&session)["metrics"][0]["value"], "175");
    }

    #[test]
    fn snapshot_roundtrip_preserves_the_project() {
        let mut session = session();
        session
            .define_metric("total", "Total", None, "SUM(amount)", 1_000.0)
            .expect("metric");
        let snapshot = session.snapshot_json().expect("snapshot");
        let restored = StudioSession::restore(&snapshot, 3_000.0).expect("restore");
        let restored_state = snapshot_of(&restored);
        assert_eq!(restored_state["metrics"][0]["value"], "175");
        assert_eq!(
            restored_state["originFingerprint"],
            snapshot_of(&session)["originFingerprint"]
        );
    }

    #[test]
    fn grouped_aggregation_is_computed_in_rust() {
        let groups: serde_json::Value = serde_json::from_str(
            &session()
                .grouped("SUM", "amount", "region")
                .expect("groups"),
        )
        .expect("json");
        assert_eq!(groups.as_array().map(Vec::len), Some(2));
    }

    #[test]
    fn capsule_contains_no_javascript() {
        let html = session()
            .capsule(
                r#"{"title":"Capsule","created_at":"2026-11-10T00:00:00Z",
                    "charts":[{"title":"By region","function":"SUM","column":"amount","group_by":"region"}]}"#,
            )
            .expect("capsule");
        assert!(!html.contains("<script"));
        assert!(!html.contains("http://"));
        assert!(!html.contains("https://"));
    }

    #[test]
    fn narrative_uses_only_verified_tokens() {
        let mut session = session();
        session
            .define_metric(
                "total",
                "Total",
                Some("EGP".to_owned()),
                "SUM(amount)",
                1_000.0,
            )
            .expect("metric");
        let rendered: serde_json::Value = serde_json::from_str(
            &session
                .narrative("الإجمالي {{metric:total}}")
                .expect("narrative"),
        )
        .expect("json");
        assert_eq!(rendered["references"][0]["id"], "total");
        assert!(
            rendered["text"]
                .as_str()
                .unwrap_or_default()
                .contains("175")
        );
    }
}
