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
use std::str::FromStr;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use sf_core::Decimal;
use sf_core::datagraph::{ColumnId, MetricId, RowId, ScenarioId, SourceId, VerifiedMetricToken};
use sf_core::error::SafiError;
use sf_core::limits::PlatformLimits;
use sf_core::table::Table;
use sf_io::{read_csv_stream, read_excel_stream};
use sf_recipe::datagraph::{AggFunc, ChangeOutcome, DataGraph, RowQuery, datagraph_display};
use sf_recipe::studio_anomaly::{AnomalyFix, detect_anomalies};
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

    /// Returns the state snapshot (metadata, columns, tokens, scenarios — never unbounded rows).
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

    /// Queries a bounded viewport page of working rows without transferring the full table.
    ///
    /// # Errors
    /// [`UNKNOWN_PROJECT_CODE`], `SF-SYS-001` for malformed query JSON, or `SF-CAL-001` for unknown columns.
    pub fn rows_page(&self, project: &str, query_json: &str) -> Result<String, String> {
        let query: RowQuery = if query_json.trim().is_empty() {
            RowQuery::default()
        } else {
            serde_json::from_str(query_json).map_err(|_| "SF-SYS-001".to_owned())?
        };
        let projects = self
            .projects
            .lock()
            .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
        let graph = projects
            .get(project)
            .ok_or_else(|| UNKNOWN_PROJECT_CODE.to_owned())?;
        let page = graph.rows_page(&query).map_err(|error| code_of(&error))?;
        serde_json::to_string(&page).map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Replaces one cell by positional display row and returns the updated state.
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

    /// Replaces one cell using its stable [`RowId`] and optional optimistic revision guard.
    ///
    /// # Errors
    /// [`UNKNOWN_PROJECT_CODE`], `SF-CAL-007` on stale `expected_revision`, or `SF-CAL-*`.
    pub fn set_cell_by_id(
        &self,
        project: &str,
        row_id: u64,
        column: &str,
        raw: &str,
        expected_revision: Option<u64>,
        now_ms: u64,
    ) -> Result<String, String> {
        self.mutate_at_revision(project, expected_revision, |graph| {
            graph.set_cell_by_id(RowId::from_raw(row_id), column, raw, now_ms)
        })
    }

    /// Sorts working rows without altering stable row identities.
    ///
    /// # Errors
    /// [`UNKNOWN_PROJECT_CODE`] or `SF-CAL-001`.
    pub fn sort_rows(
        &self,
        project: &str,
        column: &str,
        ascending: bool,
        now_ms: u64,
    ) -> Result<String, String> {
        self.mutate(project, |graph| graph.sort_rows(column, ascending, now_ms))
    }

    /// Filters working rows on a column while preserving stable row identities.
    ///
    /// # Errors
    /// [`UNKNOWN_PROJECT_CODE`] or `SF-CAL-001`.
    pub fn filter_rows(
        &self,
        project: &str,
        column: &str,
        query: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        self.mutate(project, |graph| graph.filter_rows(column, query, now_ms))
    }

    /// Deletes a working row by stable row identity.
    ///
    /// # Errors
    /// [`UNKNOWN_PROJECT_CODE`] or `SF-SYS-001`.
    pub fn delete_row(&self, project: &str, row_id: u64, now_ms: u64) -> Result<String, String> {
        self.mutate(project, |graph| {
            graph.delete_row(RowId::from_raw(row_id), now_ms)
        })
    }

    /// Inserts a row with a freshly allocated stable row identity.
    ///
    /// # Errors
    /// [`UNKNOWN_PROJECT_CODE`], `SF-SYS-001`, or `SF-CAL-001`.
    pub fn insert_row(
        &self,
        project: &str,
        after_row_id: Option<u64>,
        values_json: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let values: BTreeMap<String, String> = if values_json.trim().is_empty() {
            BTreeMap::new()
        } else {
            serde_json::from_str(values_json).map_err(|_| "SF-SYS-001".to_owned())?
        };
        let after = after_row_id.map(RowId::from_raw);
        self.mutate(project, |graph| {
            graph
                .insert_row(after, values, now_ms)
                .map(|(_, outcome)| outcome)
        })
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

    /// Registers a baseline What-If parameter assumption.
    ///
    /// # Errors
    /// [`UNKNOWN_PROJECT_CODE`] or `SF-SYS-001`.
    pub fn define_parameter(
        &self,
        project: &str,
        name: &str,
        value_str: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let value = Decimal::from_str(value_str.trim()).map_err(|_| "SF-CAL-002".to_owned())?;
        self.mutate_unit(project, None, |graph| {
            graph.define_parameter(name, value, now_ms)
        })
    }

    /// Updates a baseline What-If assumption value and recomputes dependent metrics.
    ///
    /// # Errors
    /// [`UNKNOWN_PROJECT_CODE`], `SF-CAL-002`, or `SF-CAL-005`.
    pub fn update_parameter(
        &self,
        project: &str,
        name: &str,
        value_str: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let value = Decimal::from_str(value_str.trim()).map_err(|_| "SF-CAL-002".to_owned())?;
        self.mutate(project, |graph| graph.update_parameter(name, value, now_ms))
    }

    /// Creates a What-If scenario with optional parameter overrides (`{name: "decimal"}`).
    ///
    /// # Errors
    /// [`UNKNOWN_PROJECT_CODE`], `SF-SYS-001`, or `SF-CAL-002`.
    pub fn add_scenario(
        &self,
        project: &str,
        id: &str,
        name: &str,
        params_json: &str,
    ) -> Result<String, String> {
        let scenario_id = ScenarioId::new(id).map_err(|error| code_of(&error))?;
        let raw_params: BTreeMap<String, String> = if params_json.trim().is_empty() {
            BTreeMap::new()
        } else {
            serde_json::from_str(params_json).map_err(|_| "SF-CAL-002".to_owned())?
        };
        let mut parsed = BTreeMap::new();
        for (key, val) in raw_params {
            let decimal = Decimal::from_str(val.trim()).map_err(|_| "SF-CAL-002".to_owned())?;
            parsed.insert(key, decimal);
        }
        self.mutate_unit(project, None, |graph| {
            graph.add_scenario(scenario_id, name, parsed)
        })
    }

    /// Renames an existing What-If scenario.
    ///
    /// # Errors
    /// [`UNKNOWN_PROJECT_CODE`] or `SF-SYS-001`.
    pub fn rename_scenario(
        &self,
        project: &str,
        id: &str,
        new_name: &str,
    ) -> Result<String, String> {
        let scenario_id = ScenarioId::new(id).map_err(|error| code_of(&error))?;
        self.mutate_unit(project, None, |graph| {
            graph.rename_scenario(&scenario_id, new_name)
        })
    }

    /// Activates a scenario (`Some(id)`) or clears to baseline (`None`) and recomputes metrics.
    ///
    /// # Errors
    /// [`UNKNOWN_PROJECT_CODE`] or `SF-SYS-001`.
    pub fn activate_scenario(
        &self,
        project: &str,
        id: Option<&str>,
        now_ms: u64,
    ) -> Result<String, String> {
        let scenario_id = match id {
            Some(raw) if !raw.trim().is_empty() => {
                Some(ScenarioId::new(raw).map_err(|error| code_of(&error))?)
            }
            _ => None,
        };
        self.mutate_unit(project, None, |graph| {
            graph.activate_scenario(scenario_id.as_ref(), now_ms)
        })
    }

    /// Resets active scenario to baseline assumptions.
    ///
    /// # Errors
    /// [`UNKNOWN_PROJECT_CODE`] or `SF-SYS-001`.
    pub fn reset_scenario(&self, project: &str, now_ms: u64) -> Result<String, String> {
        self.mutate_unit(project, None, |graph| graph.reset_scenario(now_ms))
    }

    /// Sets a numeric What-If parameter override inside a scenario and recomputes active metrics.
    ///
    /// # Errors
    /// [`UNKNOWN_PROJECT_CODE`], `SF-CAL-002`, `SF-CAL-005`, or `SF-SYS-001`.
    pub fn set_scenario_parameter(
        &self,
        project: &str,
        scenario_id: &str,
        parameter: &str,
        value_str: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let id = ScenarioId::new(scenario_id).map_err(|error| code_of(&error))?;
        let value = Decimal::from_str(value_str.trim()).map_err(|_| "SF-CAL-002".to_owned())?;
        self.mutate(project, |graph| {
            graph.set_scenario_parameter(&id, parameter, value, now_ms)
        })
    }

    /// Compares two scenarios across all metrics without mutating active project state.
    ///
    /// # Errors
    /// [`UNKNOWN_PROJECT_CODE`], `SF-SYS-001`, or evaluation codes.
    pub fn compare_scenarios(
        &self,
        project: &str,
        left: &str,
        right: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let left_id = ScenarioId::new(left).map_err(|error| code_of(&error))?;
        let right_id = ScenarioId::new(right).map_err(|error| code_of(&error))?;
        let projects = self
            .projects
            .lock()
            .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
        let graph = projects
            .get(project)
            .ok_or_else(|| UNKNOWN_PROJECT_CODE.to_owned())?;
        let comparisons = graph
            .compare_scenarios(&left_id, &right_id, now_ms)
            .map_err(|error| code_of(&error))?;
        let payload = comparisons
            .into_iter()
            .map(|entry| {
                serde_json::json!({
                    "metricId": entry.metric_id.as_str(),
                    "metricName": entry.metric_name,
                    "left": entry.left.normalize().to_string(),
                    "right": entry.right.normalize().to_string(),
                })
            })
            .collect::<Vec<_>>();
        serde_json::to_string(&payload).map_err(|_| "SF-SYS-001".to_owned())
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

    /// Applies an explicit, user-confirmed [`AnomalyFix`] batch atomically as one undoable step.
    ///
    /// # Errors
    /// [`UNKNOWN_PROJECT_CODE`], `SF-CAL-001`, `SF-CAL-002`, `SF-CAL-007`, or `SF-SYS-001`.
    pub fn apply_anomaly_fix(
        &self,
        project: &str,
        fix_json: &str,
        expected_revision: Option<u64>,
        now_ms: u64,
    ) -> Result<String, String> {
        let fix: AnomalyFix =
            serde_json::from_str(fix_json).map_err(|_| "SF-CAL-002".to_owned())?;
        self.mutate_at_revision(project, expected_revision, |graph| {
            graph.apply_anomaly_fix(fix, now_ms)
        })
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
                let status = graph
                    .token(&reference.metric_id)
                    .map(|token| format!("{:?}", token.status))
                    .unwrap_or_else(|| "Missing".to_owned());
                serde_json::json!({
                    "id": reference.metric_id.as_str(),
                    "value": reference.value,
                    "unit": reference.unit,
                    "status": status,
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
            let mut charts = Vec::with_capacity(options.charts.len());
            for chart in options.charts {
                let function =
                    AggFunc::from_name(&chart.function).ok_or_else(|| "SF-CAL-002".to_owned())?;
                charts.push(CapsuleChart {
                    title: chart.title,
                    function,
                    column: chart.column,
                    group_by: chart.group_by,
                });
            }
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
        self.mutate_at_revision(project, None, action)
    }

    fn mutate_at_revision(
        &self,
        project: &str,
        expected_revision: Option<u64>,
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
            if let Some(expected) = expected_revision
                && graph.revision().get() != expected
            {
                return Err("SF-CAL-007".to_owned());
            }
            action(graph).map_err(|error| code_of(&error))?;
        }
        self.state(project)
    }

    fn mutate_unit(
        &self,
        project: &str,
        expected_revision: Option<u64>,
        action: impl FnOnce(&mut DataGraph) -> Result<(), SafiError>,
    ) -> Result<String, String> {
        {
            let mut projects = self
                .projects
                .lock()
                .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
            let graph = projects
                .get_mut(project)
                .ok_or_else(|| UNKNOWN_PROJECT_CODE.to_owned())?;
            if let Some(expected) = expected_revision
                && graph.revision().get() != expected
            {
                return Err("SF-CAL-007".to_owned());
            }
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

/// Serializes the metadata & provenance state the desktop UI renders without dumping all rows.
fn state_json(graph: &DataGraph) -> Result<String, String> {
    let columns = graph
        .working()
        .columns()
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let prov = graph.column_provenance().get(index);
            serde_json::json!({
                "id": prov.map_or((index as u64).saturating_add(1), |entry| entry.column_id.get()),
                "name": column.schema().name,
                "type": format!("{:?}", column.schema().column_type),
                "originColumnIndex": prov.and_then(|entry| entry.origin_column_index),
            })
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
            let row_ids: Vec<u64> = anomaly
                .rows
                .iter()
                .filter_map(|&row_index| usize::try_from(row_index).ok())
                .filter_map(|idx| graph.row_id_at(idx).map(RowId::get))
                .collect();
            serde_json::json!({
                "kind": format!("{:?}", anomaly.kind),
                "messageKey": anomaly.kind.message_key(),
                "methodKey": anomaly.kind.method_key(),
                "suggestionKey": anomaly.kind.suggestion_key(),
                "column": anomaly.column,
                "rows": anomaly.rows,
                "rowIds": row_ids,
            })
        })
        .collect::<Vec<_>>();

    let scenarios = graph
        .scenarios()
        .values()
        .map(|scenario| {
            let params: BTreeMap<&String, String> = scenario
                .parameters()
                .iter()
                .map(|(key, val)| (key, val.normalize().to_string()))
                .collect();
            serde_json::json!({
                "id": scenario.id().as_str(),
                "name": scenario.name(),
                "parameters": params,
            })
        })
        .collect::<Vec<_>>();

    let payload = serde_json::json!({
        "sourceId": graph.source_id().as_str(),
        "revision": graph.revision().get(),
        "datasetVersion": graph.dataset_version().get(),
        "originFingerprint": graph.origin_fingerprint(),
        "rowCount": graph.working().row_count(),
        "columns": columns,
        "metrics": metrics,
        "errors": errors,
        "anomalies": anomalies,
        "calculated": graph.calculated_columns(),
        "lastRecomputed": graph.last_recomputed().iter().map(MetricId::as_str).collect::<Vec<_>>(),
        "activeScenario": graph.active_scenario().map(ScenarioId::as_str),
        "scenarios": scenarios,
        "parameters": graph
            .parameters()
            .iter()
            .map(|(name, value)| serde_json::json!({ "name": name, "value": value.normalize().to_string() }))
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
        "sourceId": token.source_id.as_str(),
        "sourceVersion": token.source_version.get(),
        "sourceFingerprint": token.source_fingerprint,
        "fingerprint": token.fingerprint(),
        "status": format!("{:?}", token.status),
        "rows": token.row_refs,
        "sourceRowIds": token.source_row_ids.iter().copied().map(RowId::get).collect::<Vec<_>>(),
        "columns": token.column_refs,
        "columnIds": token.column_ids.iter().copied().map(ColumnId::get).collect::<Vec<_>>(),
        "transformationTrace": token.transformation_trace,
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
        assert_eq!(state["rowCount"], serde_json::json!(3));
        assert!(state.get("rows").is_none());

        let page = json(&test_result(store.rows_page("p1", "{}")));
        assert_eq!(page["total_rows"], serde_json::json!(3));
        assert_eq!(page["rows"][0]["row_id"], serde_json::json!(1));
        assert_eq!(
            page["rows"][0]["cells"][0]["visible"],
            serde_json::json!("Cairo")
        );
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

    #[test]
    fn large_dataset_pagination_is_bounded_and_state_stays_compact() {
        let directory = test_result(tempfile::tempdir());
        let path = directory.path().join("large.csv");
        let mut csv = String::from("region,amount\n");
        for i in 1..=600 {
            let region = if i % 2 == 0 { "Cairo" } else { "Giza" };
            csv.push_str(&format!("{region},{i}\n"));
        }
        test_result(fs::write(&path, &csv));
        let store = StudioStore::new();
        let state_raw = test_result(store.open_file("big", &path, 1_000));
        let parsed = json(&state_raw);
        assert_eq!(parsed["rowCount"], serde_json::json!(600));
        assert!(parsed.get("rows").is_none());

        let page = json(&test_result(store.rows_page(
            "big",
            r#"{"offset":100,"limit":999,"search":"Cairo","sort_column":"amount","ascending":false,"filter_column":null,"filter_value":null}"#,
        )));
        assert_eq!(page["total_rows"], serde_json::json!(600));
        assert_eq!(page["filtered_rows"], serde_json::json!(300));
        assert_eq!(page["limit"], serde_json::json!(200));
        assert_eq!(page["rows"].as_array().map(Vec::len), Some(200));
    }

    #[test]
    fn revision_guard_what_if_scenarios_and_strict_chart_aggregation_work() {
        let directory = test_result(tempfile::tempdir());
        let (store, _) = open_csv(directory.path());
        test_result(store.define_parameter("p1", "tax", "1.1", 1_500));
        test_result(store.define_metric(
            "p1",
            "proj",
            "Projected",
            Some("EGP"),
            "SUM(amount) * tax",
            2_000,
        ));
        let rev = state(&store)["revision"].as_u64().unwrap_or(0);

        // Stale revision must be rejected with SF-CAL-007
        assert_eq!(
            store.set_cell_by_id("p1", 1, "amount", "500", Some(rev.saturating_sub(1)), 2_100),
            Err("SF-CAL-007".to_owned())
        );

        // Add, rename, activate, and update scenario parameter
        test_result(store.add_scenario("p1", "s1", "Optimistic", r#"{"tax":"2"}"#));
        test_result(store.rename_scenario("p1", "s1", "Optimistic Q4"));
        test_result(store.activate_scenario("p1", Some("s1"), 2_200));
        let active_state = state(&store);
        assert_eq!(
            active_state["metrics"][0]["value"],
            serde_json::json!("350")
        );
        assert_eq!(
            active_state["scenarios"][0]["name"],
            serde_json::json!("Optimistic Q4")
        );

        // Invalid aggregation in capsule must return SF-CAL-002 rather than falling back to SUM
        let bad_capsule = r#"{"title":"Capsule","created_at":"2026-11-10T00:00:00Z","charts":[{"title":"Bad","function":"MEDIAN","column":"amount","group_by":"region"}]}"#;
        let target = directory.path().join("bad.safi.html");
        assert_eq!(
            store.export_capsule("p1", &target, bad_capsule),
            Err("SF-CAL-002".to_owned())
        );
    }

    #[test]
    fn anomaly_detection_reports_row_ids_and_confirmed_fix_undoes_in_one_step() {
        let directory = test_result(tempfile::tempdir());
        let path = directory.path().join("anom.csv");
        test_result(fs::write(
            &path,
            "region,amount\nCairo,100\n,50\nCairo,100\n",
        ));
        let store = StudioStore::new();
        let initial = json(&test_result(store.open_file("a1", &path, 1_000)));
        assert_eq!(initial["anomalies"].as_array().map(Vec::len), Some(2));
        let rev = initial["revision"].as_u64();
        let missing = &initial["anomalies"][0];
        assert_eq!(missing["rowIds"], serde_json::json!([2]));

        let after_fill = json(&test_result(store.apply_anomaly_fix(
            "a1",
            r#"{"action":"fill_missing","column":"region","row_ids":[2],"replacement":"Alex"}"#,
            rev,
            2_000,
        )));
        assert_eq!(after_fill["anomalies"].as_array().map(Vec::len), Some(1));

        let after_undo = json(&test_result(store.undo("a1", 3_000)));
        assert_eq!(after_undo["anomalies"].as_array().map(Vec::len), Some(2));
    }
}
