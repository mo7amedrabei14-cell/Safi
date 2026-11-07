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
            pick_output_file
        ])
        .setup(|app| {
            app.manage(PendingDeepLink::default());
            tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title("صافي البيانات | SafiData")
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
