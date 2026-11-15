//! SafiData desktop shell (Tauri 2.2).
//!
//! The shell is deliberately thin. Every security decision lives in
//! `sf-desktop`, is unit-tested there, and is enforced here:
//!
//! * `on_navigation` sends every URL through the local-only content policy.
//! * `safi://` deep links are parsed against an action/parameter allowlist
//!   before they reach the webview or the operating system.
//! * Only the commands in `sf_desktop::commands::ALLOWED_COMMANDS` are exposed.
//! * The window runs with a strict CSP and no devtools in production.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;

use sf_desktop::deep_link::DeepLinkAction;
use sf_desktop::studio::StudioStore;
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

/// A deep link that arrived before the webview could listen for it.
#[derive(Default)]
struct PendingDeepLink(Mutex<Option<DeepLinkAction>>);

/// Non-sensitive build information for the about screen.
#[tauri::command]
fn app_info() -> serde_json::Value {
    serde_json::json!({
        "product": "SafiData",
        "version": env!("CARGO_PKG_VERSION"),
        "shell": "tauri-2.2"
    })
}

/// Hand the UI a deep link that arrived while it was still starting.
#[tauri::command]
fn take_pending_deep_link(state: tauri::State<'_, PendingDeepLink>) -> Option<DeepLinkAction> {
    state.0.lock().ok().and_then(|mut slot| slot.take())
}

/// Native open dialog restricted to spreadsheets.
#[tauri::command]
async fn pick_input_file(app: tauri::AppHandle) -> Result<Option<String>, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("Spreadsheets", &["csv", "xlsx", "xls"])
            .blocking_pick_file()
            .and_then(|path| path.into_path().ok())
            .map(|path| path.display().to_string())
    })
    .await
    .map_err(|_| "SF-SYS-011")
}

/// Native save dialog restricted to new CSV exports.
#[tauri::command]
async fn pick_output_file(app: tauri::AppHandle) -> Result<Option<String>, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("CSV", &["csv"])
            .set_file_name("cleaned.csv")
            .blocking_save_file()
            .and_then(|path| path.into_path().ok())
            .map(|path| path.display().to_string())
    })
    .await
    .map_err(|_| "SF-SYS-011")
}

/// Native save dialog restricted to `.safi.html` offline capsule exports.
#[tauri::command]
async fn pick_capsule_output_file(app: tauri::AppHandle) -> Result<Option<String>, &'static str> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("Safi Capsule", &["html"])
            .set_file_name("report.safi.html")
            .blocking_save_file()
            .and_then(|path| path.into_path().ok())
            .map(|path| path.display().to_string())
    })
    .await
    .map_err(|_| "SF-SYS-011")
}

/// Current wall clock in milliseconds; the engine takes time as an argument so it stays
/// replayable in tests.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// Every studio command below delegates to `sf_desktop::studio`, where the engine actually
/// runs. The webview only ever receives JSON snapshots.

#[tauri::command]
fn studio_open_file(
    store: tauri::State<'_, StudioStore>,
    project: String,
    path: String,
) -> Result<String, String> {
    store.open_file(&project, std::path::Path::new(&path), now_ms())
}

#[tauri::command]
fn studio_state(store: tauri::State<'_, StudioStore>, project: String) -> Result<String, String> {
    store.state(&project)
}

#[tauri::command]
fn studio_rows_page(
    store: tauri::State<'_, StudioStore>,
    project: String,
    query: String,
) -> Result<String, String> {
    store.rows_page(&project, &query)
}

#[tauri::command]
fn studio_set_cell(
    store: tauri::State<'_, StudioStore>,
    project: String,
    row: usize,
    column: String,
    value: String,
) -> Result<String, String> {
    store.set_cell(&project, row, &column, &value, now_ms())
}

#[tauri::command]
fn studio_set_cell_by_id(
    store: tauri::State<'_, StudioStore>,
    project: String,
    row_id: u64,
    column: String,
    value: String,
    expected_revision: Option<u64>,
) -> Result<String, String> {
    store.set_cell_by_id(
        &project,
        row_id,
        &column,
        &value,
        expected_revision,
        now_ms(),
    )
}

#[tauri::command]
fn studio_sort_rows(
    store: tauri::State<'_, StudioStore>,
    project: String,
    column: String,
    ascending: bool,
) -> Result<String, String> {
    store.sort_rows(&project, &column, ascending, now_ms())
}

#[tauri::command]
fn studio_filter_rows(
    store: tauri::State<'_, StudioStore>,
    project: String,
    column: String,
    query: String,
) -> Result<String, String> {
    store.filter_rows(&project, &column, &query, now_ms())
}

#[tauri::command]
fn studio_delete_row(
    store: tauri::State<'_, StudioStore>,
    project: String,
    row_id: u64,
) -> Result<String, String> {
    store.delete_row(&project, row_id, now_ms())
}

#[tauri::command]
fn studio_insert_row(
    store: tauri::State<'_, StudioStore>,
    project: String,
    after_row_id: Option<u64>,
    values: String,
) -> Result<String, String> {
    store.insert_row(&project, after_row_id, &values, now_ms())
}

#[tauri::command]
fn studio_define_parameter(
    store: tauri::State<'_, StudioStore>,
    project: String,
    name: String,
    value: String,
) -> Result<String, String> {
    store.define_parameter(&project, &name, &value, now_ms())
}

#[tauri::command]
fn studio_update_parameter(
    store: tauri::State<'_, StudioStore>,
    project: String,
    name: String,
    value: String,
) -> Result<String, String> {
    store.update_parameter(&project, &name, &value, now_ms())
}

#[tauri::command]
fn studio_add_scenario(
    store: tauri::State<'_, StudioStore>,
    project: String,
    id: String,
    name: String,
    parameters: String,
) -> Result<String, String> {
    store.add_scenario(&project, &id, &name, &parameters)
}

#[tauri::command]
fn studio_rename_scenario(
    store: tauri::State<'_, StudioStore>,
    project: String,
    id: String,
    new_name: String,
) -> Result<String, String> {
    store.rename_scenario(&project, &id, &new_name)
}

#[tauri::command]
fn studio_activate_scenario(
    store: tauri::State<'_, StudioStore>,
    project: String,
    id: Option<String>,
) -> Result<String, String> {
    store.activate_scenario(&project, id.as_deref(), now_ms())
}

#[tauri::command]
fn studio_reset_scenario(
    store: tauri::State<'_, StudioStore>,
    project: String,
) -> Result<String, String> {
    store.reset_scenario(&project, now_ms())
}

#[tauri::command]
fn studio_set_scenario_parameter(
    store: tauri::State<'_, StudioStore>,
    project: String,
    scenario_id: String,
    parameter: String,
    value: String,
) -> Result<String, String> {
    store.set_scenario_parameter(&project, &scenario_id, &parameter, &value, now_ms())
}

#[tauri::command]
fn studio_compare_scenarios(
    store: tauri::State<'_, StudioStore>,
    project: String,
    left: String,
    right: String,
) -> Result<String, String> {
    store.compare_scenarios(&project, &left, &right, now_ms())
}

#[tauri::command]
fn studio_define_metric(
    store: tauri::State<'_, StudioStore>,
    project: String,
    id: String,
    name: String,
    unit: Option<String>,
    formula: String,
) -> Result<String, String> {
    store.define_metric(&project, &id, &name, unit.as_deref(), &formula, now_ms())
}

#[tauri::command]
fn studio_add_calculated_column(
    store: tauri::State<'_, StudioStore>,
    project: String,
    name: String,
    formula: String,
) -> Result<String, String> {
    store.add_calculated_column(&project, &name, &formula, now_ms())
}

#[tauri::command]
fn studio_apply_command(
    store: tauri::State<'_, StudioStore>,
    project: String,
    command: String,
) -> Result<String, String> {
    store.apply_command(&project, &command, now_ms())
}

#[tauri::command]
fn studio_apply_anomaly_fix(
    store: tauri::State<'_, StudioStore>,
    project: String,
    fix: String,
    expected_revision: Option<u64>,
) -> Result<String, String> {
    store.apply_anomaly_fix(&project, &fix, expected_revision, now_ms())
}

#[tauri::command]
fn studio_undo(store: tauri::State<'_, StudioStore>, project: String) -> Result<String, String> {
    store.undo(&project, now_ms())
}

#[tauri::command]
fn studio_redo(store: tauri::State<'_, StudioStore>, project: String) -> Result<String, String> {
    store.redo(&project, now_ms())
}

#[tauri::command]
fn studio_grouped(
    store: tauri::State<'_, StudioStore>,
    project: String,
    function: String,
    column: String,
    group_by: String,
) -> Result<String, String> {
    store.grouped(&project, &function, &column, &group_by)
}

#[tauri::command]
fn studio_narrative(
    store: tauri::State<'_, StudioStore>,
    project: String,
    template: String,
) -> Result<String, String> {
    store.narrative(&project, &template)
}

#[tauri::command]
fn studio_snapshot(
    store: tauri::State<'_, StudioStore>,
    project: String,
) -> Result<String, String> {
    store.snapshot(&project)
}

#[tauri::command]
fn studio_restore(
    store: tauri::State<'_, StudioStore>,
    project: String,
    snapshot: String,
) -> Result<String, String> {
    store.restore(&project, &snapshot, now_ms())
}

#[tauri::command]
fn studio_export_capsule(
    store: tauri::State<'_, StudioStore>,
    project: String,
    target: String,
    options: String,
) -> Result<String, String> {
    store.export_capsule(&project, std::path::Path::new(&target), &options)
}

fn main() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            focus_main_window(app);
            for argument in argv.iter().skip(1) {
                queue_deep_link(app, argument);
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            app_info,
            take_pending_deep_link,
            pick_input_file,
            pick_output_file,
            pick_capsule_output_file,
            studio_open_file,
            studio_state,
            studio_rows_page,
            studio_set_cell,
            studio_set_cell_by_id,
            studio_sort_rows,
            studio_filter_rows,
            studio_delete_row,
            studio_insert_row,
            studio_define_metric,
            studio_add_calculated_column,
            studio_define_parameter,
            studio_update_parameter,
            studio_add_scenario,
            studio_rename_scenario,
            studio_activate_scenario,
            studio_reset_scenario,
            studio_set_scenario_parameter,
            studio_compare_scenarios,
            studio_apply_command,
            studio_apply_anomaly_fix,
            studio_undo,
            studio_redo,
            studio_grouped,
            studio_narrative,
            studio_snapshot,
            studio_restore,
            studio_export_capsule
        ])
        .setup(|app| {
            app.manage(PendingDeepLink::default());
            app.manage(StudioStore::new());
            tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title("SafiData")
            .inner_size(1200.0, 800.0)
            .min_inner_size(960.0, 600.0)
            .resizable(true)
            .devtools(false)
            .on_navigation(|url| sf_desktop::content::validate_app_navigation(url.as_str()).is_ok())
            .build()?;
            for argument in std::env::args().skip(1) {
                queue_deep_link(app.handle(), &argument);
            }
            Ok(())
        })
        .run(tauri::generate_context!());

    if result.is_err() {
        // A graphical app has no console to report to; exit without a window.
        std::process::exit(1);
    }
}

fn focus_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_focus();
    }
}

/// Validate a deep link, remember it for the UI, and notify a live webview.
///
/// Invalid links are dropped here: nothing outside the allowlist reaches the
/// webview, the filesystem, or the operating system.
fn queue_deep_link(app: &tauri::AppHandle, raw: &str) {
    if !raw.to_ascii_lowercase().starts_with("safi://") {
        return;
    }
    let Ok(action) = sf_desktop::deep_link::parse(raw) else {
        return;
    };
    if let Some(state) = app.try_state::<PendingDeepLink>() {
        if let Ok(mut slot) = state.0.lock() {
            *slot = Some(action.clone());
        }
    }
    let _ = app.emit("safi://deep-link", action);
}
