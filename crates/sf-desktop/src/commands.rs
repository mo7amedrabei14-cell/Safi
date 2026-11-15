//! IPC command allowlist and payload ceiling for the desktop bridge (section 8.6).
//!
//! Every message between the webview and the native core is checked by name and
//! size before it runs. Anything not listed here is refused; the list grows only
//! when a later task adds a command with strict input types.

use crate::error::{DesktopError, DesktopErrorCode};

/// Largest IPC payload the shell accepts (1 MiB).
pub const MAX_IPC_BYTES: usize = 1_048_576;

/// Commands the shell exposes today.
pub const ALLOWED_COMMANDS: [&str; 32] = [
    "app_info",
    "take_pending_deep_link",
    "pick_input_file",
    "pick_output_file",
    "pick_capsule_output_file",
    "studio_open_file",
    "studio_state",
    "studio_rows_page",
    "studio_set_cell",
    "studio_set_cell_by_id",
    "studio_sort_rows",
    "studio_filter_rows",
    "studio_delete_row",
    "studio_insert_row",
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
    "studio_undo",
    "studio_redo",
    "studio_grouped",
    "studio_narrative",
    "studio_snapshot",
    "studio_restore",
    "studio_export_capsule",
];

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
    }
}
