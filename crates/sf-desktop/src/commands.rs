//! IPC command allowlist and payload ceiling for the desktop bridge (section 8.6).
//!
//! Every message between the webview and the native core is checked by name and
//! size before it runs. Anything not listed here is refused; the list grows only
//! when a later task adds a command with strict input types.

use crate::error::{DesktopError, DesktopErrorCode};

/// Largest IPC payload the shell accepts (1 MiB).
pub const MAX_IPC_BYTES: usize = 1_048_576;

/// Commands the shell exposes today.
pub const ALLOWED_COMMANDS: [&str; 47] = [
    "app_info",
    "take_pending_deep_link",
    "pick_input_file",
    "pick_output_file",
    "pick_capsule_output_file",
    "studio_set_auth_session",
    "studio_clear_auth_session",
    "studio_open_file",
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

/// Uniform byte-size accounting for a single IPC command parameter.
pub trait IpcFieldLen {
    /// Byte contribution of this parameter toward [`MAX_IPC_BYTES`].
    fn ipc_field_len(&self) -> usize;
}

impl IpcFieldLen for str {
    fn ipc_field_len(&self) -> usize {
        self.len()
    }
}

impl IpcFieldLen for String {
    fn ipc_field_len(&self) -> usize {
        self.len()
    }
}

impl IpcFieldLen for std::path::Path {
    fn ipc_field_len(&self) -> usize {
        self.as_os_str().len()
    }
}

impl IpcFieldLen for std::path::PathBuf {
    fn ipc_field_len(&self) -> usize {
        self.as_os_str().len()
    }
}

impl IpcFieldLen for usize {
    fn ipc_field_len(&self) -> usize {
        std::mem::size_of::<usize>()
    }
}

impl IpcFieldLen for u64 {
    fn ipc_field_len(&self) -> usize {
        std::mem::size_of::<u64>()
    }
}

impl IpcFieldLen for bool {
    fn ipc_field_len(&self) -> usize {
        std::mem::size_of::<bool>()
    }
}

impl<T: IpcFieldLen + ?Sized> IpcFieldLen for &T {
    fn ipc_field_len(&self) -> usize {
        (**self).ipc_field_len()
    }
}

impl<T: IpcFieldLen> IpcFieldLen for Option<T> {
    fn ipc_field_len(&self) -> usize {
        self.as_ref().map_or(0, IpcFieldLen::ipc_field_len)
    }
}

/// Saturating sum of IPC parameter lengths.
#[must_use]
pub fn measure_ipc_payload(lengths: &[usize]) -> usize {
    lengths.iter().copied().fold(0usize, usize::saturating_add)
}

/// Computes the total IPC payload byte length across all arguments of a command.
#[macro_export]
macro_rules! ipc_payload_len {
    () => {
        0usize
    };
    ($($field:expr),+ $(,)?) => {
        $crate::commands::measure_ipc_payload(&[
            $($crate::commands::IpcFieldLen::ipc_field_len(&$field)),+
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
    fn measures_all_ipc_parameter_types_and_enforces_exact_boundary() {
        let path = std::path::Path::new("input.csv");
        let opt_some: Option<u64> = Some(42);
        let opt_none: Option<u64> = None;
        let measured = crate::ipc_payload_len!(
            "p1",
            path,
            3usize,
            99u64,
            true,
            opt_some,
            opt_none,
            Some("EGP")
        );
        assert_eq!(
            measured,
            2 + "input.csv".len()
                + std::mem::size_of::<usize>()
                + std::mem::size_of::<u64>()
                + std::mem::size_of::<bool>()
                + std::mem::size_of::<u64>()
                + 3
        );

        let exact_filler = "a".repeat(MAX_IPC_BYTES - std::mem::size_of::<u64>());
        let exact_len = crate::ipc_payload_len!(exact_filler.as_str(), 1u64);
        assert_eq!(exact_len, MAX_IPC_BYTES);
        assert_eq!(enforce_ipc_command("studio_delete_row", exact_len), Ok(()));

        let over_len = crate::ipc_payload_len!(exact_filler.as_str(), 1u64, true);
        assert_eq!(over_len, MAX_IPC_BYTES + 1);
        assert_eq!(
            enforce_ipc_command("studio_sort_rows", over_len),
            Err("SF-SYS-007".to_owned())
        );
    }
}
