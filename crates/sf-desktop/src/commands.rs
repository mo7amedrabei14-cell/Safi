//! IPC command allowlist and serialized JSON payload ceiling for the desktop bridge (section 8.6).
//!
//! Every command invocation reaching the native boundary is checked by command name and by the
//! serialized size of its parsed JSON arguments (or raw `InvokeBody::Raw` buffer) **after** Tauri
//! parses the incoming IPC payload into `InvokeBody` (`serde_json::Value`) and **before** typed
//! command handler execution or `StudioStore` state mutation.
//!
//! **Scope & explicit non-claims**:
//! - **Protected**: Prevents oversized parsed JSON argument objects (including field names, quotes,
//!   separators, JSON string escapes, Unicode, numbers, booleans, and `null`) from reaching
//!   Studio command handlers or mutating dataset/project state.
//! - **Not protected / not claimed**: Does **not** bound raw wire transport bytes before Tauri
//!   parses the incoming WebView IPC message into `InvokeBody::Json(serde_json::Value)`.

use crate::error::{DesktopError, DesktopErrorCode};

/// Largest re-serialized JSON argument payload (or `InvokeBody::Raw` length) accepted by the
/// native command boundary (1 MiB, enforced after Tauri parses JSON into `serde_json::Value` and
/// before command execution/state mutation).
pub const MAX_IPC_BYTES: usize = 1_048_576;

/// Commands the shell exposes today.
pub const ALLOWED_COMMANDS: [&str; 83] = [
    "app_info",
    "take_pending_deep_link",
    "desktop_open_external_auth_url",
    "desktop_open_external_billing_url",
    "pick_input_file",
    "pick_output_file",
    "pick_capsule_output_file",
    "pick_deliverable_file",
    "pick_search_folder",
    "pick_safitemplate_open_file",
    "pick_safitemplate_save_file",
    "studio_set_auth_session",
    "studio_clear_auth_session",
    "studio_authorization_status",
    "studio_enter_offline_readonly",
    "studio_revalidate_session",
    "studio_open_file",
    "studio_select_region",
    "studio_workbook_details",
    "studio_state",
    "studio_rows_page",
    "studio_set_cell",
    "studio_set_cell_by_id",
    "studio_sort_rows",
    "studio_filter_rows",
    "studio_delete_row",
    "studio_insert_row",
    "studio_add_source_column",
    "studio_rename_column",
    "studio_delete_column",
    "studio_trim_column",
    "studio_define_metric",
    "studio_add_calculated_column",
    "studio_define_parameter",
    "studio_update_parameter",
    "studio_add_scenario",
    "studio_rename_scenario",
    "studio_activate_scenario",
    "studio_reset_scenario",
    "studio_set_scenario_parameter",
    "studio_compare_scenarios",
    "studio_apply_command",
    "studio_apply_anomaly_fix",
    "studio_undo",
    "studio_redo",
    "studio_grouped",
    "studio_chart_data",
    "studio_narrative",
    "studio_snapshot",
    "studio_restore",
    "studio_set_artifacts",
    "studio_save_project",
    "studio_list_projects",
    "studio_load_project",
    "studio_delete_project",
    "studio_list_templates",
    "studio_instantiate_template",
    "studio_export_capsule",
    "studio_export_deliverable",
    "desktop_search_index_folders",
    "desktop_search_query",
    "desktop_search_stats",
    "desktop_recipes_list",
    "desktop_recipes_sync_official",
    "desktop_recipes_save_as_new",
    "desktop_recipes_update_existing",
    "desktop_recipes_duplicate",
    "desktop_recipes_apply_to_project",
    "desktop_recipes_delete",
    "desktop_recipes_export_safitemplate",
    "desktop_recipes_import_safitemplate",
    "desktop_recipes_prepare_community_share",
    "desktop_workflow_memory_list",
    "desktop_workflow_memory_save_sheet",
    "desktop_workflow_memory_match_sheet",
    "desktop_workflow_memory_apply_replay",
    "desktop_workflow_memory_manage",
    "desktop_learning_profile_get",
    "desktop_learning_record_feedback",
    "desktop_learning_delete_rule",
    "desktop_learning_reset_profile",
    "desktop_decision_engine_evaluate",
    "desktop_decision_engine_apply",
];

/// Access classification separating offline read-only viewing of saved local projects
/// from protected operations that strictly require live online server authorization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandAccessClass {
    /// Allowed locally in Offline Read-Only Mode to inspect previously saved projects,
    /// datasets, dashboards, and reports without mutating or exporting anything.
    ReadOnlyOffline,
    /// Strictly requires a valid online server authorization lease + active sole device +
    /// active subscription. Fails closed offline with `SF-AUTH-021`.
    ProtectedOnline,
}

/// Classify a desktop IPC command as read-only offline vs protected online.
#[must_use]
pub fn classify_command_access(command: &str) -> CommandAccessClass {
    match command {
        "app_info"
        | "take_pending_deep_link"
        | "desktop_open_external_auth_url"
        | "studio_authorization_status"
        | "studio_enter_offline_readonly"
        | "studio_revalidate_session"
        | "studio_list_projects"
        | "studio_load_project"
        | "studio_state"
        | "studio_rows_page"
        | "studio_workbook_details"
        | "studio_list_templates"
        | "desktop_search_query"
        | "desktop_search_stats"
        | "desktop_recipes_list"
        | "desktop_workflow_memory_list"
        | "desktop_workflow_memory_match_sheet"
        | "desktop_learning_profile_get"
        | "desktop_decision_engine_evaluate" => CommandAccessClass::ReadOnlyOffline,
        _ => CommandAccessClass::ProtectedOnline,
    }
}

/// Authorise an IPC command by exact name.
///
/// # Errors
///
/// Returns [`DesktopErrorCode::CommandDenied`] for anything outside
/// [`ALLOWED_COMMANDS`].
pub fn authorize_command(command: &str) -> Result<(), DesktopError> {
    if ALLOWED_COMMANDS.contains(&command) {
        Ok(())
    } else {
        Err(DesktopError::new(DesktopErrorCode::CommandDenied))
    }
}

/// Reject a payload larger than [`MAX_IPC_BYTES`].
///
/// # Errors
///
/// Returns [`DesktopErrorCode::PayloadTooLarge`] once the ceiling is exceeded.
pub fn validate_ipc_payload_len(len: usize) -> Result<(), DesktopError> {
    if len <= MAX_IPC_BYTES {
        Ok(())
    } else {
        Err(DesktopError::new(DesktopErrorCode::PayloadTooLarge))
    }
}

/// Enforces both [`authorize_command`] (`SF-SYS-010`) and [`validate_ipc_payload_len`] (`SF-SYS-007`)
/// for an incoming Tauri IPC invocation.
///
/// # Errors
///
/// Returns the canonical `SF-SYS-*` error code string when the command name or payload size is
/// rejected.
pub fn enforce_ipc_command(command: &str, payload_len: usize) -> Result<(), String> {
    authorize_command(command).map_err(|err| err.code().as_str().to_owned())?;
    validate_ipc_payload_len(payload_len).map_err(|err| err.code().as_str().to_owned())?;
    Ok(())
}

/// Zero-allocation byte counter implementing [`std::io::Write`] for `serde_json`.
#[derive(Default)]
struct JsonByteCounter {
    bytes: usize,
}

impl std::io::Write for JsonByteCounter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.bytes = self.bytes.saturating_add(buf.len());
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Computes the serialized JSON byte length of `value` without heap allocation.
///
/// Fails closed to `usize::MAX` if serialization ever fails so malformed values can never bypass
/// [`MAX_IPC_BYTES`].
#[must_use]
pub fn json_serialized_len<T: serde::Serialize + ?Sized>(value: &T) -> usize {
    let mut counter = JsonByteCounter::default();
    match serde_json::to_writer(&mut counter, value) {
        Ok(()) => counter.bytes,
        Err(_) => usize::MAX,
    }
}

/// Serialized JSON byte length (or conservative upper bound for non-UTF-8 OS paths) of an IPC
/// parameter toward [`MAX_IPC_BYTES`].
pub trait IpcJsonValueLen {
    /// Serialized JSON byte length of this parameter, including quotes, escapes, literals (`null`,
    /// `true`, `false`), and decimal numeric formatting.
    fn ipc_json_value_len(&self) -> usize;
}

impl IpcJsonValueLen for str {
    fn ipc_json_value_len(&self) -> usize {
        json_serialized_len(self)
    }
}

impl IpcJsonValueLen for String {
    fn ipc_json_value_len(&self) -> usize {
        json_serialized_len(self)
    }
}

impl IpcJsonValueLen for std::path::Path {
    fn ipc_json_value_len(&self) -> usize {
        match self.to_str() {
            Some(utf8) => json_serialized_len(utf8),
            None => {
                // Non-UTF-8 OS path: fail conservatively above any escaped `\u00XX` expansion.
                let lossy_len = json_serialized_len(&*self.to_string_lossy());
                let upper_bound = self.as_os_str().len().saturating_mul(6).saturating_add(2);
                lossy_len.max(upper_bound)
            }
        }
    }
}

impl IpcJsonValueLen for std::path::PathBuf {
    fn ipc_json_value_len(&self) -> usize {
        self.as_path().ipc_json_value_len()
    }
}

impl IpcJsonValueLen for usize {
    fn ipc_json_value_len(&self) -> usize {
        json_serialized_len(self)
    }
}

impl IpcJsonValueLen for u64 {
    fn ipc_json_value_len(&self) -> usize {
        json_serialized_len(self)
    }
}

impl IpcJsonValueLen for bool {
    fn ipc_json_value_len(&self) -> usize {
        json_serialized_len(self)
    }
}

impl<T: IpcJsonValueLen + ?Sized> IpcJsonValueLen for &T {
    fn ipc_json_value_len(&self) -> usize {
        (**self).ipc_json_value_len()
    }
}

impl<T: IpcJsonValueLen> IpcJsonValueLen for Option<T> {
    fn ipc_json_value_len(&self) -> usize {
        match self {
            Some(value) => value.ipc_json_value_len(),
            None => json_serialized_len(&None::<()>),
        }
    }
}

/// Computes the serialized JSON object byte length `{"k1":v1,"k2":v2}` for `fields`, where each
/// entry supplies `(field_name, serialized_value_len)`.
///
/// Accounts for `{`, `}`, quoted & escaped field names, `:`, separating `,`, and each serialized
/// value size. Returns `2` (`{}`) when `fields` is empty.
#[must_use]
pub fn measure_ipc_json_object(fields: &[(&str, usize)]) -> usize {
    if fields.is_empty() {
        return 2;
    }
    let mut total = 2usize.saturating_add(fields.len().saturating_sub(1));
    for (key, value_len) in fields {
        let entry = json_serialized_len(key)
            .saturating_add(1)
            .saturating_add(*value_len);
        total = total.saturating_add(entry);
    }
    total
}

/// Computes the serialized JSON object byte size across all named IPC parameters of a command.
#[macro_export]
macro_rules! ipc_payload_len {
    () => {
        $crate::commands::measure_ipc_json_object(&[])
    };
    ($(($name:literal, $field:expr)),+ $(,)?) => {
        $crate::commands::measure_ipc_json_object(&[
            $(($name, $crate::commands::IpcJsonValueLen::ipc_json_value_len(&$field))),+
        ])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_only_the_documented_commands() {
        for command in ALLOWED_COMMANDS {
            assert!(authorize_command(command).is_ok());
        }

        for command in [
            "",
            "app_info ",
            "APP_INFO",
            "shell_exec",
            "fs_read",
            "http_request",
            "process_spawn",
            "open_url",
        ] {
            assert_eq!(
                authorize_command(command).map_err(DesktopError::code),
                Err(DesktopErrorCode::CommandDenied),
                "expected command to be denied: {command:?}"
            );
            assert_eq!(
                enforce_ipc_command(command, 0),
                Err("SF-SYS-010".to_owned())
            );
        }
    }

    #[test]
    fn enforces_the_one_mib_payload_ceiling() {
        assert!(validate_ipc_payload_len(0).is_ok());
        assert!(validate_ipc_payload_len(MAX_IPC_BYTES).is_ok());
        assert_eq!(
            validate_ipc_payload_len(MAX_IPC_BYTES + 1).map_err(DesktopError::code),
            Err(DesktopErrorCode::PayloadTooLarge)
        );
        assert_eq!(enforce_ipc_command("app_info", MAX_IPC_BYTES), Ok(()));
        assert_eq!(
            enforce_ipc_command("app_info", MAX_IPC_BYTES + 1),
            Err("SF-SYS-007".to_owned())
        );
    }

    #[test]
    fn matches_serialized_json_bytes_across_unicode_escapes_numbers_bools_and_optionals() {
        assert_eq!(crate::ipc_payload_len!(), b"{}".len());

        let path = std::path::Path::new("C:\\Users\\Safi\\صافي.csv");
        let text = "صافي \"quote\"\n\u{0000}\u{001f}\\tail 📊";
        let opt_some: Option<u64> = Some(18_446_744_073_709_551_615);
        let opt_none: Option<u64> = None;
        let measured = crate::ipc_payload_len!(
            ("project", "p1"),
            ("path", path),
            ("text", text),
            ("row", 12_345usize),
            ("rowId", 99u64),
            ("ascending", false),
            ("optSome", opt_some),
            ("optNone", opt_none),
            ("unit", Some("ج.م")),
        );

        let serialized = crate::test_result(serde_json::to_vec(&serde_json::json!({
            "project": "p1",
            "path": path.to_string_lossy(),
            "text": text,
            "row": 12_345usize,
            "rowId": 99u64,
            "ascending": false,
            "optSome": opt_some,
            "optNone": opt_none,
            "unit": Some("ج.م"),
        })));
        assert_eq!(measured, serialized.len());

        // Control chars and quotes expand in JSON and count toward the 1 MiB ceiling even when
        // raw Rust byte length is far below MAX_IPC_BYTES.
        // Construct an object of exact MAX_IPC_BYTES serialized length:
        // {"project":"p1","raw":"..."} has fixed overhead 24 bytes -> 1_048_576 - 24 = 1_048_552.
        let fixed = crate::test_result(serde_json::to_vec(&serde_json::json!({
            "project": "p1",
            "raw": ""
        })))
        .len();
        let filler = "a".repeat(MAX_IPC_BYTES - fixed);
        let exact_len = crate::ipc_payload_len!(("project", "p1"), ("raw", filler.as_str()));
        assert_eq!(exact_len, MAX_IPC_BYTES);
        assert_eq!(enforce_ipc_command("studio_state", exact_len), Ok(()));

        // Escaping one ASCII byte with `"` adds 1 byte (`\"` = 2 bytes) -> MAX_IPC_BYTES + 1
        let escaped_over = format!("{}\"", "a".repeat(MAX_IPC_BYTES - fixed - 1));
        let escaped_len =
            crate::ipc_payload_len!(("project", "p1"), ("raw", escaped_over.as_str()));
        assert_eq!(escaped_over.len(), MAX_IPC_BYTES - fixed); // raw Rust len did not change!
        assert_eq!(escaped_len, MAX_IPC_BYTES + 1);
        assert_eq!(
            enforce_ipc_command("studio_sort_rows", escaped_len),
            Err("SF-SYS-007".to_owned())
        );

        // Switching Option::None (`null`, 4 bytes) to Option::Some(10000) (`10000`, 5 bytes) pushes
        // an exact boundary payload over by 1 byte -> SF-SYS-007.
        let base_overhead = crate::test_result(serde_json::to_vec(&serde_json::json!({
            "project": "p1",
            "expectedRevision": null,
            "raw": ""
        })))
        .len();
        let base_opt_none = crate::ipc_payload_len!(
            ("project", "p1"),
            ("expectedRevision", None::<u64>),
            ("raw", "a".repeat(MAX_IPC_BYTES - base_overhead))
        );
        assert_eq!(base_opt_none, MAX_IPC_BYTES);

        let over_opt_some = crate::ipc_payload_len!(
            ("project", "p1"),
            ("expectedRevision", Some(10_000u64)),
            ("raw", "a".repeat(MAX_IPC_BYTES - base_overhead))
        );
        assert_eq!(over_opt_some, MAX_IPC_BYTES + 1);
        assert_eq!(
            enforce_ipc_command("studio_set_cell_by_id", over_opt_some),
            Err("SF-SYS-007".to_owned())
        );
    }
}
