//! Stdio JSON-RPC harness for the real Rust `StudioStore` + `StudioSessionAuthority`.
//!
//! Used by `tools/verify-studio-desktop-e2e.mjs` so headless Chromium executes every
//! `window.__TAURI_INTERNALS__.invoke(command, args)` call against the real compiled Rust
//! `DataGraph`, `rust_decimal` evaluator, single-device session gate, atomic project
//! bundle persistence, and `.safi.html` capsule exporter — with zero mocks or stubs.

#![allow(clippy::print_stdout)]

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use sf_desktop::studio::StudioStore;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn str_arg<'a>(args: &'a Value, primary: &str, fallback: &str) -> &'a str {
    args.get(primary)
        .or_else(|| args.get(fallback))
        .and_then(Value::as_str)
        .unwrap_or("")
}

fn opt_str_arg<'a>(args: &'a Value, primary: &str, fallback: &str) -> Option<&'a str> {
    args.get(primary)
        .or_else(|| args.get(fallback))
        .and_then(Value::as_str)
}

fn u64_arg(args: &Value, primary: &str, fallback: &str) -> u64 {
    args.get(primary)
        .or_else(|| args.get(fallback))
        .and_then(Value::as_u64)
        .unwrap_or(0)
}

fn opt_u64_arg(args: &Value, primary: &str, fallback: &str) -> Option<u64> {
    args.get(primary)
        .or_else(|| args.get(fallback))
        .and_then(Value::as_u64)
}

fn dispatch(
    store: &mut StudioStore,
    storage_dir: &Path,
    csv_path: &str,
    capsule_path: &str,
    command: &str,
    args: &Value,
) -> Result<String, String> {
    let ts = now_ms();
    match command {
        "__harness_register_session" => {
            let token = str_arg(args, "token", "token");
            let session_id = str_arg(args, "sessionId", "session_id");
            let workspace_id = str_arg(args, "workspaceId", "workspace_id");
            let installation_id = str_arg(args, "installationId", "installation_id");
            let expires_at_ms = u64_arg(args, "expiresAtMs", "expires_at_ms");
            store.session_authority().register_session(
                if session_id.is_empty() {
                    "sess-e2e-1"
                } else {
                    session_id
                },
                if workspace_id.is_empty() {
                    "ws-e2e-1"
                } else {
                    workspace_id
                },
                token,
                installation_id,
                if expires_at_ms == 0 {
                    ts.saturating_add(3_600_000)
                } else {
                    expires_at_ms
                },
            )?;
            Ok(r#"{"registered":true}"#.to_owned())
        }
        "__harness_revoke_session" => {
            let token = str_arg(args, "token", "token");
            store.session_authority().revoke_session(token);
            Ok(r#"{"revoked":true}"#.to_owned())
        }
        "__harness_reset_store_keep_bundles" => {
            let authority = store.session_authority();
            *store = StudioStore::with_storage_dir_and_verifier(
                storage_dir.to_path_buf(),
                authority.clone(),
                authority,
            );
            Ok(r#"{"reset":true}"#.to_owned())
        }
        "pick_input_file" => {
            store.gate_command("pick_input_file", 0, Some(ts))?;
            serde_json::to_string(&csv_path).map_err(|_| "SF-SYS-001".to_owned())
        }
        "pick_capsule_output_file" => {
            store.gate_command("pick_capsule_output_file", 0, Some(ts))?;
            serde_json::to_string(&capsule_path).map_err(|_| "SF-SYS-001".to_owned())
        }
        "pick_deliverable_file" => {
            store.gate_command("pick_deliverable_file", 0, Some(ts))?;
            let ext = str_arg(args, "extension", "default_extension");
            let ext_clean = if ext.is_empty() { "xlsx" } else { ext };
            let out_path = Path::new(capsule_path)
                .parent()
                .map_or_else(std::env::temp_dir, Path::to_path_buf)
                .join(format!("safi_deliverable.{ext_clean}"));
            serde_json::to_string(&out_path.display().to_string())
                .map_err(|_| "SF-SYS-001".to_owned())
        }
        "studio_select_region" => store.select_region(
            str_arg(args, "project", "project"),
            str_arg(args, "sheetName", "sheet_name"),
            str_arg(args, "tableId", "table_id"),
            ts,
        ),
        "studio_workbook_details" => store.workbook_details(str_arg(args, "project", "project")),
        "studio_set_auth_session" => store.set_auth_session(
            str_arg(args, "token", "token"),
            str_arg(args, "installationId", "installation_id"),
            ts,
        ),
        "studio_clear_auth_session" => store.clear_auth_session(),
        "studio_open_file" => store.open_file(
            str_arg(args, "project", "project"),
            Path::new(str_arg(args, "path", "path")),
            ts,
        ),
        "studio_state" => store.state(str_arg(args, "project", "project")),
        "studio_rows_page" => store.rows_page(
            str_arg(args, "project", "project"),
            str_arg(args, "query", "query"),
        ),
        "studio_set_cell" => store.set_cell(
            str_arg(args, "project", "project"),
            usize::try_from(u64_arg(args, "row", "row")).unwrap_or(0),
            str_arg(args, "column", "column"),
            str_arg(args, "value", "value"),
            ts,
        ),
        "studio_set_cell_by_id" => store.set_cell_by_id(
            str_arg(args, "project", "project"),
            u64_arg(args, "rowId", "row_id"),
            str_arg(args, "column", "column"),
            str_arg(args, "value", "value"),
            opt_u64_arg(args, "expectedRevision", "expected_revision"),
            ts,
        ),
        "studio_sort_rows" => store.sort_rows(
            str_arg(args, "project", "project"),
            str_arg(args, "column", "column"),
            args.get("ascending")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            ts,
        ),
        "studio_filter_rows" => store.filter_rows(
            str_arg(args, "project", "project"),
            str_arg(args, "column", "column"),
            str_arg(args, "query", "query"),
            ts,
        ),
        "studio_delete_row" => store.delete_row(
            str_arg(args, "project", "project"),
            u64_arg(args, "rowId", "row_id"),
            ts,
        ),
        "studio_insert_row" => store.insert_row(
            str_arg(args, "project", "project"),
            opt_u64_arg(args, "afterRowId", "after_row_id"),
            str_arg(args, "values", "values"),
            ts,
        ),
        "studio_define_parameter" => store.define_parameter(
            str_arg(args, "project", "project"),
            str_arg(args, "name", "name"),
            str_arg(args, "value", "value"),
            ts,
        ),
        "studio_update_parameter" => store.update_parameter(
            str_arg(args, "project", "project"),
            str_arg(args, "name", "name"),
            str_arg(args, "value", "value"),
            ts,
        ),
        "studio_add_scenario" => store.add_scenario(
            str_arg(args, "project", "project"),
            str_arg(args, "id", "id"),
            str_arg(args, "name", "name"),
            str_arg(args, "parameters", "parameters"),
        ),
        "studio_rename_scenario" => store.rename_scenario(
            str_arg(args, "project", "project"),
            str_arg(args, "id", "id"),
            str_arg(args, "newName", "new_name"),
        ),
        "studio_activate_scenario" => store.activate_scenario(
            str_arg(args, "project", "project"),
            opt_str_arg(args, "id", "id"),
            ts,
        ),
        "studio_reset_scenario" => store.reset_scenario(str_arg(args, "project", "project"), ts),
        "studio_set_scenario_parameter" => store.set_scenario_parameter(
            str_arg(args, "project", "project"),
            str_arg(args, "scenarioId", "scenario_id"),
            str_arg(args, "parameter", "parameter"),
            str_arg(args, "value", "value"),
            ts,
        ),
        "studio_compare_scenarios" => store.compare_scenarios(
            str_arg(args, "project", "project"),
            str_arg(args, "left", "left"),
            str_arg(args, "right", "right"),
            ts,
        ),
        "studio_define_metric" => store.define_metric(
            str_arg(args, "project", "project"),
            str_arg(args, "id", "id"),
            str_arg(args, "name", "name"),
            opt_str_arg(args, "unit", "unit"),
            str_arg(args, "formula", "formula"),
            ts,
        ),
        "studio_add_calculated_column" => store.add_calculated_column(
            str_arg(args, "project", "project"),
            str_arg(args, "name", "name"),
            str_arg(args, "formula", "formula"),
            ts,
        ),
        "studio_apply_command" => store.apply_command(
            str_arg(args, "project", "project"),
            str_arg(args, "command", "command"),
            ts,
        ),
        "studio_apply_anomaly_fix" => store.apply_anomaly_fix(
            str_arg(args, "project", "project"),
            str_arg(args, "fix", "fix"),
            opt_u64_arg(args, "expectedRevision", "expected_revision"),
            ts,
        ),
        "studio_undo" => store.undo(str_arg(args, "project", "project"), ts),
        "studio_redo" => store.redo(str_arg(args, "project", "project"), ts),
        "studio_grouped" => store.grouped(
            str_arg(args, "project", "project"),
            str_arg(args, "function", "function"),
            str_arg(args, "column", "column"),
            str_arg(args, "groupBy", "group_by"),
        ),
        "studio_chart_data" => store.chart_data_filtered(
            str_arg(args, "project", "project"),
            str_arg(args, "chartType", "chart_type"),
            str_arg(args, "function", "function"),
            str_arg(args, "column", "column"),
            str_arg(args, "groupBy", "group_by"),
            opt_str_arg(args, "filterColumn", "filter_column"),
            opt_str_arg(args, "filterValue", "filter_value"),
        ),
        "studio_narrative" => store.narrative(
            str_arg(args, "project", "project"),
            str_arg(args, "template", "template"),
        ),
        "studio_snapshot" => store.snapshot(str_arg(args, "project", "project")),
        "studio_restore" => store.restore(
            str_arg(args, "project", "project"),
            str_arg(args, "snapshot", "snapshot"),
            ts,
        ),
        "studio_add_source_column" => store.add_source_column(
            str_arg(args, "project", "project"),
            str_arg(args, "name", "name"),
            str_arg(args, "defaultValue", "default_value"),
            ts,
        ),
        "studio_rename_column" => store.rename_column(
            str_arg(args, "project", "project"),
            str_arg(args, "oldName", "old_name"),
            str_arg(args, "newName", "new_name"),
            ts,
        ),
        "studio_delete_column" => store.delete_column(
            str_arg(args, "project", "project"),
            str_arg(args, "name", "name"),
            ts,
        ),
        "studio_trim_column" => store.trim_column(
            str_arg(args, "project", "project"),
            str_arg(args, "column", "column"),
            ts,
        ),
        "studio_set_artifacts" => store.set_artifacts(
            str_arg(args, "project", "project"),
            str_arg(args, "artifacts", "artifacts"),
        ),
        "studio_save_project" => store.save_project_bundle(
            str_arg(args, "project", "project"),
            str_arg(args, "savedId", "saved_id"),
            str_arg(args, "displayName", "display_name"),
            ts,
        ),
        "studio_list_projects" => store.list_project_bundles(),
        "studio_load_project" => store.load_project_bundle(
            str_arg(args, "project", "project"),
            str_arg(args, "savedId", "saved_id"),
            ts,
        ),
        "studio_delete_project" => {
            store.delete_project_bundle(str_arg(args, "savedId", "saved_id"))
        }
        "studio_list_templates" => store.list_templates(),
        "studio_instantiate_template" => store.instantiate_template(
            str_arg(args, "project", "project"),
            str_arg(args, "templateId", "template_id"),
            str_arg(args, "bindings", "bindings"),
        ),
        "studio_export_capsule" => store.export_capsule(
            str_arg(args, "project", "project"),
            Path::new(str_arg(args, "target", "target")),
            str_arg(args, "options", "options"),
        ),
        "studio_export_deliverable" => store.export_deliverable(
            str_arg(args, "project", "project"),
            Path::new(str_arg(args, "target", "target")),
            args.get("isBase64")
                .or_else(|| args.get("is_base64"))
                .and_then(Value::as_bool)
                .unwrap_or(false),
            str_arg(args, "payload", "payload"),
        ),
        other => store
            .gate_command(other, 0, Some(ts))
            .map(|_| "null".to_owned()),
    }
}

fn main() {
    let storage_dir = std::env::var("HARNESS_STORAGE_DIR").map_or_else(
        |_| std::env::temp_dir().join("safi-e2e-bundles"),
        PathBuf::from,
    );
    let csv_path = std::env::var("HARNESS_CSV_PATH").unwrap_or_default();
    let capsule_path = std::env::var("HARNESS_CAPSULE_PATH").unwrap_or_default();

    let mut store = StudioStore::with_storage_dir(storage_dir.clone());

    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let _ = writeln!(stdout, r#"{{"ready":true}}"#);
    let _ = stdout.flush();

    for line_res in stdin.lock().lines() {
        let Ok(line) = line_res else { break };
        if line.trim().is_empty() {
            continue;
        }
        let parsed: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(err) => {
                let _ = writeln!(
                    stdout,
                    "{}",
                    serde_json::json!({ "id": 0, "ok": false, "error": err.to_string() })
                );
                let _ = stdout.flush();
                continue;
            }
        };
        let id = parsed.get("id").and_then(Value::as_u64).unwrap_or(0);
        let command = parsed.get("command").and_then(Value::as_str).unwrap_or("");
        let args = parsed.get("args").cloned().unwrap_or(Value::Null);

        let response = match dispatch(
            &mut store,
            &storage_dir,
            &csv_path,
            &capsule_path,
            command,
            &args,
        ) {
            Ok(result) => serde_json::json!({
                "id": id,
                "ok": true,
                "result": result,
            }),
            Err(error) => serde_json::json!({
                "id": id,
                "ok": false,
                "error": error,
            }),
        };
        let _ = writeln!(stdout, "{response}");
        let _ = stdout.flush();
    }
}
