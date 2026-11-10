//! Studio session commands for the desktop app (`STUDIO-PHASE-1`).
//!
//! Execution lives here, never in the webview: the browser build ships only the showcase
//! site, so every number the user sees is computed and verified by Rust behind these IPC
//! commands. Each project is an immutable origin plus a transformation log, and every
//! mutation returns the full state the UI should render.
//!
//! Nothing in this module logs a value, a file name or a column name (AGENTS.md rule 4).

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use sf_core::datagraph::{MetricId, ScenarioId, SourceId, VerifiedMetricToken};
use sf_core::error::SafiError;
use sf_core::limits::PlatformLimits;
use sf_core::table::Table;
use sf_io::{read_csv_stream, read_excel_stream};
use sf_recipe::datagraph::{AggFunc, ChangeOutcome, DataGraph, datagraph_display};
use sf_recipe::studio_anomaly::detect_anomalies;
use sf_recipe::studio_capsule::{CapsuleChart, CapsuleOptions, export_capsule};
use sf_recipe::studio_dsl::{apply_command, parse_command, render_narrative};

/// Largest file the desktop app will import (`50 MiB`, matching the drop zone copy).
pub const MAX_STUDIO_IMPORT_BYTES: u64 = 50 * 1024 * 1024;

/// Code returned when a command names a project that is not open.
pub const UNKNOWN_PROJECT_CODE: &str = "SF-SYS-015";

/// Code returned when a capsule cannot be written next to the project.
pub const EXPORT_FAILED_CODE: &str = "SF-SYS-016";

/// Open Studio projects, keyed by the identifier the UI chose.
#[derive(Debug, Default)]
pub struct StudioStore {
    projects: Mutex<BTreeMap<String, DataGraph>>,
}

impl StudioStore {
    /// Creates an empty store (the app keeps exactly one per window).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Imports a CSV or Excel file from disk and replaces any project with the same id.
    ///
    /// # Errors
    /// Returns the `SF-*` code of the first failing step: the path is rejected when its
    /// extension is unsupported (`SF-IMP-002`), the file is too large (`SF-IMP-003`), or the
    /// reader rejects the contents.
    pub fn open_file(&self, project: &str, path: &Path, now_ms: u64) -> Result<String, String> {
        let table = read_table(path)?;
        let source = SourceId::new(project).map_err(|error| code_of(&error))?;
        let graph = DataGraph::new(source, table, now_ms).map_err(|error| code_of(&error))?;
        let mut projects = self
            .projects
            .lock()
            .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
        let _ = projects.insert(project.to_owned(), graph);
        drop(projects);
        self.state(project)
    }

    /// Returns the state snapshot the UI renders.
    ///
    /// # Errors
    /// [`UNKNOWN_PROJECT_CODE`] when the project is not open.
    pub fn state(&self, project: &str) -> Result<String, String> {
        let projects = self
            .projects
            .lock()
            .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
        let graph = projects
            .get(project)
            .ok_or_else(|| UNKNOWN_PROJECT_CODE.to_owned())?;
        state_json(graph)
    }

    /// Replaces one cell and returns the recomputed state.
    ///
    /// # Errors
    /// [`UNKNOWN_PROJECT_CODE`] or the engine's own `SF-CAL-*` code.
    pub fn set_cell(
        &self,
        project: &str,
        row: usize,
        column: &str,
        raw: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        self.mutate(project, |graph| graph.set_cell(row, column, raw, now_ms))
    }

    /// Adds a calculated column.
    ///
    /// # Errors
    /// [`UNKNOWN_PROJECT_CODE`] or the engine's own `SF-CAL-*` code.
    pub fn add_calculated_column(
        &self,
        project: &str,
        name: &str,
        formula: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        self.mutate(project, |graph| {
            graph.add_calculated_column(name, formula, now_ms)
        })
    }

    /// Defines a metric and computes its verified token.
    ///
    /// # Errors
    /// [`UNKNOWN_PROJECT_CODE`] or the engine's own `SF-CAL-*` code.
    pub fn define_metric(
        &self,
        project: &str,
        id: &str,
        name: &str,
        unit: Option<&str>,
        formula: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let metric = MetricId::new(id).map_err(|error| code_of(&error))?;
        self.mutate(project, |graph| {
            graph.define_metric(metric, name, unit, formula, now_ms)
        })
    }

    /// Executes a validated Studio DSL command.
    ///
    /// # Errors
    /// [`UNKNOWN_PROJECT_CODE`] or the engine's own `SF-CAL-*` code.
    pub fn apply_command(
        &self,
        project: &str,
        command_json: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let command = parse_command(command_json).map_err(|error| code_of(&error))?;
        self.mutate(project, |graph| apply_command(graph, &command, now_ms))
    }

    /// Undoes the last change.
    ///
    /// # Errors
    /// [`UNKNOWN_PROJECT_CODE`] or the engine's own `SF-CAL-*` code.
    pub fn undo(&self, project: &str, now_ms: u64) -> Result<String, String> {
        self.mutate(project, |graph| graph.undo(now_ms))
    }

    /// Redoes the last undone change.
    ///
    /// # Errors
    /// [`UNKNOWN_PROJECT_CODE`] or the engine's own `SF-CAL-*` code.
    pub fn redo(&self, project: &str, now_ms: u64) -> Result<String, String> {
        self.mutate(project, |graph| graph.redo(now_ms))
    }

    /// Groups and aggregates in Rust for the dashboard.
    ///
    /// # Errors
    /// [`UNKNOWN_PROJECT_CODE`] or the engine's own `SF-CAL-*` code.
    pub fn grouped(
        &self,
        project: &str,
        function: &str,
        column: &str,
        group_by: &str,
    ) -> Result<String, String> {
        let function = AggFunc::from_name(function).ok_or_else(|| "SF-CAL-002".to_owned())?;
        let projects = self
            .projects
            .lock()
            .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
        let graph = projects
            .get(project)
            .ok_or_else(|| UNKNOWN_PROJECT_CODE.to_owned())?;
        let groups = graph
            .grouped_aggregate(function, column, group_by)
            .map_err(|error| code_of(&error))?;
        let payload = groups
            .into_iter()
            .map(|group| serde_json::json!({ "label": group.label, "value": group.value.to_string() }))
            .collect::<Vec<_>>();
        serde_json::to_string(&payload).map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Fills `{{metric:...}}` placeholders from verified tokens only.
    ///
    /// # Errors
    /// [`UNKNOWN_PROJECT_CODE`] or the engine's own `SF-CAL-*` code.
    pub fn narrative(&self, project: &str, template: &str) -> Result<String, String> {
        let projects = self
            .projects
            .lock()
            .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
        let graph = projects
            .get(project)
            .ok_or_else(|| UNKNOWN_PROJECT_CODE.to_owned())?;
        let rendered = render_narrative(template, graph).map_err(|error| code_of(&error))?;
        let references = rendered
            .references
            .iter()
            .map(|reference| {
                serde_json::json!({
                    "id": reference.metric_id.as_str(),
                    "value": reference.value,
                    "fingerprint": reference.fingerprint,
                })
            })
            .collect::<Vec<_>>();
        serde_json::to_string(&serde_json::json!({
            "text": rendered.text,
            "missing": rendered.missing,
            "references": references,
        }))
        .map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Serializes the project so the app can store it locally.
    ///
    /// # Errors
    /// [`UNKNOWN_PROJECT_CODE`] or `SF-SYS-001`.
    pub fn snapshot(&self, project: &str) -> Result<String, String> {
        let projects = self
            .projects
            .lock()
            .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
        let graph = projects
            .get(project)
            .ok_or_else(|| UNKNOWN_PROJECT_CODE.to_owned())?;
        graph.to_json().map_err(|error| code_of(&error))
    }

    /// Restores a saved project and re-derives every metric before returning it.
    ///
    /// # Errors
    /// `SF-SYS-001` for an unreadable file and `SF-CAL-008` when the stored data no longer
    /// matches its own fingerprint.
    pub fn restore(
        &self,
        project: &str,
        snapshot_json: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let graph = DataGraph::from_json(snapshot_json, now_ms).map_err(|error| code_of(&error))?;
        let mut projects = self
            .projects
            .lock()
            .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
        let _ = projects.insert(project.to_owned(), graph);
        drop(projects);
        self.state(project)
    }

    /// Writes the self-contained `.safi.html` capsule next to the project.
    ///
    /// The export is a new file written atomically; the original data is never modified.
    ///
    /// # Errors
    /// [`UNKNOWN_PROJECT_CODE`], `SF-CAL-002` for malformed options, or [`EXPORT_FAILED_CODE`]
    /// when the file cannot be written.
    pub fn export_capsule(
        &self,
        project: &str,
        target: &Path,
        options_json: &str,
    ) -> Result<String, String> {
        let options: CapsuleInput =
            serde_json::from_str(options_json).map_err(|_| "SF-CAL-002".to_owned())?;
        let html = {
            let projects = self
                .projects
                .lock()
                .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
            let graph = projects
                .get(project)
                .ok_or_else(|| UNKNOWN_PROJECT_CODE.to_owned())?;
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
                graph,
                &CapsuleOptions {
                    title: options.title,
                    created_at: options.created_at,
                    charts,
                },
            )
            .map_err(|error| code_of(&error))?
        };
        write_atomic(target, &html)?;
        Ok(target.display().to_string())
    }

    fn mutate(
        &self,
        project: &str,
        action: impl FnOnce(&mut DataGraph) -> Result<ChangeOutcome, SafiError>,
    ) -> Result<String, String> {
        {
            let mut projects = self
                .projects
                .lock()
                .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
            let graph = projects
                .get_mut(project)
                .ok_or_else(|| UNKNOWN_PROJECT_CODE.to_owned())?;
            action(graph).map_err(|error| code_of(&error))?;
        }
        self.state(project)
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

fn code_of(error: &SafiError) -> String {
    error.code().as_code_string()
}

/// Reads a spreadsheet from disk with the desktop platform limits.
fn read_table(path: &Path) -> Result<Table, String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| "SF-IMP-002".to_owned())?;
    let is_csv = extension == "csv";
    if !is_csv && !matches!(extension.as_str(), "xlsx" | "xlsm" | "xls" | "ods") {
        return Err("SF-IMP-002".to_owned());
    }
    let metadata = fs::metadata(path).map_err(|_| "SF-IMP-001".to_owned())?;
    if metadata.len() > MAX_STUDIO_IMPORT_BYTES {
        return Err("SF-IMP-003".to_owned());
    }
    let bytes = fs::read(path).map_err(|_| "SF-IMP-001".to_owned())?;
    let sheet_name = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("Sheet1")
        .to_owned();
    let sheet = if is_csv {
        read_csv_stream(
            Cursor::new(&bytes),
            PlatformLimits::DESKTOP_CLI,
            &sheet_name,
            "tbl_1",
            "table",
        )
        .map_err(|error| code_of(&error))?
    } else {
        read_excel_stream(
            Cursor::new(&bytes),
            bytes.len() as u64,
            PlatformLimits::DESKTOP_CLI,
        )
        .map_err(|error| code_of(&error))?
        .into_iter()
        .next()
        .ok_or_else(|| "SF-IMP-001".to_owned())?
    };
    sheet
        .primary_table()
        .cloned()
        .ok_or_else(|| "SF-IMP-001".to_owned())
}

/// Serializes the state the desktop UI renders.
fn state_json(graph: &DataGraph) -> Result<String, String> {
    let columns = graph
        .working()
        .columns()
        .iter()
        .map(|column| {
            serde_json::json!({
                "name": column.schema().name,
                "type": format!("{:?}", column.schema().column_type),
            })
        })
        .collect::<Vec<_>>();

    let rows = (0..graph.working().row_count())
        .map(|row| {
            graph
                .working()
                .columns()
                .iter()
                .map(|column| {
                    column
                        .cell_at(row)
                        .map(|cell| cell.visible_text().to_owned())
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    let metrics = graph
        .metric_ids()
        .iter()
        .filter_map(|id| graph.token(id).map(token_json))
        .collect::<Vec<_>>();

    let errors = graph
        .metric_ids()
        .iter()
        .filter_map(|id| {
            graph.metric_error(id).map(|error| {
                serde_json::json!({ "id": id.as_str(), "code": error.code().as_code_string() })
            })
        })
        .collect::<Vec<_>>();

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
    serde_json::to_string(&payload).map_err(|_| "SF-SYS-001".to_owned())
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

/// Writes a new file via a temporary sibling, `fsync`, then rename (AGENTS.md rule 13).
fn write_atomic(target: &Path, contents: &str) -> Result<(), String> {
    let directory: PathBuf = target
        .parent()
        .ok_or_else(|| EXPORT_FAILED_CODE.to_owned())?
        .to_path_buf();
    let name = target
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| EXPORT_FAILED_CODE.to_owned())?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let temporary = directory.join(format!(".{name}.{stamp}.tmp"));
    let write = || -> std::io::Result<()> {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, target)
    };
    if write().is_err() {
        let _ = fs::remove_file(&temporary);
        return Err(EXPORT_FAILED_CODE.to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_result;

    const CSV: &str = "region,amount\nCairo,100\nGiza,50\nCairo,25\n";
    const CAPSULE: &str = r#"{"title":"Capsule","created_at":"2026-11-10T00:00:00Z","charts":[{"title":"By region","function":"SUM","column":"amount","group_by":"region"}]}"#;

    fn open_csv(dir: &Path) -> (StudioStore, PathBuf) {
        let path = dir.join("input.csv");
        test_result(fs::write(&path, CSV));
        let store = StudioStore::new();
        test_result(store.open_file("p1", &path, 1_000));
        (store, path)
    }

    fn json(raw: &str) -> serde_json::Value {
        test_result(serde_json::from_str(raw))
    }

    fn state(store: &StudioStore) -> serde_json::Value {
        json(&test_result(store.state("p1")))
    }

    #[test]
    fn opening_a_csv_creates_a_live_project() {
        let directory = test_result(tempfile::tempdir());
        let (store, _) = open_csv(directory.path());
        let state = state(&store);
        assert_eq!(state["columns"][0]["name"], serde_json::json!("region"));
        assert_eq!(state["rows"].as_array().map(Vec::len), Some(3));
        assert_eq!(state["rows"][0][0], serde_json::json!("Cairo"));
    }

    #[test]
    fn editing_a_cell_updates_only_the_dependent_metric() {
        let directory = test_result(tempfile::tempdir());
        let (store, _) = open_csv(directory.path());
        test_result(store.define_metric("p1", "total", "Total", Some("EGP"), "SUM(amount)", 2_000));
        let before = state(&store);
        assert_eq!(before["metrics"][0]["value"], serde_json::json!("175"));
        assert_eq!(
            before["metrics"][0]["status"],
            serde_json::json!("Verified")
        );

        test_result(store.set_cell("p1", 0, "amount", "200", 3_000));
        let after = state(&store);
        assert_eq!(after["metrics"][0]["value"], serde_json::json!("275"));
        assert_ne!(
            after["metrics"][0]["fingerprint"],
            before["metrics"][0]["fingerprint"]
        );
    }

    #[test]
    fn undo_restores_the_previous_value() {
        let directory = test_result(tempfile::tempdir());
        let (store, _) = open_csv(directory.path());
        test_result(store.define_metric("p1", "total", "Total", None, "SUM(amount)", 2_000));
        test_result(store.set_cell("p1", 0, "amount", "200", 3_000));
        test_result(store.undo("p1", 4_000));
        assert_eq!(
            state(&store)["metrics"][0]["value"],
            serde_json::json!("175")
        );
    }

    #[test]
    fn snapshot_roundtrip_keeps_the_verified_value() {
        let directory = test_result(tempfile::tempdir());
        let (store, _) = open_csv(directory.path());
        test_result(store.define_metric("p1", "total", "Total", None, "SUM(amount)", 2_000));
        let snapshot = test_result(store.snapshot("p1"));
        let restored = StudioStore::new();
        test_result(restored.restore("p2", &snapshot, 3_000));
        let state = json(&test_result(restored.state("p2")));
        assert_eq!(state["metrics"][0]["value"], serde_json::json!("175"));
    }

    #[test]
    fn capsule_export_writes_a_new_file_and_never_touches_the_original() {
        let directory = test_result(tempfile::tempdir());
        let (store, csv_path) = open_csv(directory.path());
        let before = test_result(fs::read(&csv_path));
        let target = directory.path().join("capsule.safi.html");
        test_result(store.export_capsule("p1", &target, CAPSULE));
        let after = test_result(fs::read(&csv_path));
        assert_eq!(before, after, "the imported file must stay byte-identical");

        let html = String::from_utf8_lossy(&test_result(fs::read(&target))).to_string();
        assert!(html.contains("<html"));
        assert!(!html.contains("<script"));
        assert!(!html.contains("http://"));
        assert!(!html.contains("https://"));
    }

    #[test]
    fn grouping_is_computed_in_rust() {
        let directory = test_result(tempfile::tempdir());
        let (store, _) = open_csv(directory.path());
        let groups = json(&test_result(store.grouped("p1", "SUM", "amount", "region")));
        assert_eq!(groups.as_array().map(Vec::len), Some(2));
    }

    #[test]
    fn an_unknown_project_is_reported_with_its_code() {
        let store = StudioStore::new();
        assert_eq!(store.state("missing"), Err(UNKNOWN_PROJECT_CODE.to_owned()));
    }

    #[test]
    fn unsupported_extensions_are_rejected_before_reading() {
        let directory = test_result(tempfile::tempdir());
        let path = directory.path().join("notes.txt");
        test_result(fs::write(&path, "not a spreadsheet"));
        let store = StudioStore::new();
        assert_eq!(
            store.open_file("p1", &path, 1_000),
            Err("SF-IMP-002".to_owned())
        );
    }
}
