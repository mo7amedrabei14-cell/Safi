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

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use sf_desktop::api_origin::{ApiOrigin, API_ORIGIN_ENV};
use sf_desktop::commands::{enforce_ipc_command, json_serialized_len};
use sf_desktop::deep_link::DeepLinkAction;
use sf_desktop::studio::{
    parse_installation_uuid, parse_safi_session_token, StudioSessionAuthority,
    StudioSessionVerifier, StudioStore, VerifiedStudioSession, AUTH_ACCOUNT_SUSPENDED_CODE,
    AUTH_BACKEND_UNAVAILABLE_CODE, AUTH_CONFIG_MISSING_CODE, AUTH_DEVICE_PROOF_INVALID_CODE,
    AUTH_DEVICE_REPLACED_CODE, AUTH_EXPIRED_CODE, AUTH_REQUIRED_CODE, AUTH_REVOKED_CODE,
};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;

/// A deep link that arrived before the webview could listen for it.
#[derive(Default)]
struct PendingDeepLink(Mutex<Option<DeepLinkAction>>);

#[derive(Debug, Clone)]
struct AuthServerEndpoint {
    host: String,
    port: u16,
    connect_addr: Option<SocketAddr>,
    tls_config: Arc<ClientConfig>,
}

/// Native fail-closed session verifier for the Tauri shell.
///
/// Resolves `SAFI_API_BASE_URL` compiled into the binary at build time and queries
/// `GET /v1/auth/sessions/current` over server-authenticated TLS (`rustls` + `webpki-roots`,
/// without client certificates) with `Authorization: Bearer <sfs_token>` and
/// `X-Safi-Installation-Id: <uuid>`.
/// Fails closed with `SF-SYS-031` when unconfigured and `SF-AUTH-005` when the auth server
/// is unreachable, never falling back to unauthenticated TCP or local mocks.
#[derive(Debug)]
struct DesktopShellSessionVerifier {
    authority: Arc<StudioSessionAuthority>,
    endpoint: Result<AuthServerEndpoint, String>,
}

impl DesktopShellSessionVerifier {
    fn new(authority: Arc<StudioSessionAuthority>) -> Self {
        Self::from_origin(authority, option_env!("SAFI_API_BASE_URL"))
    }

    fn from_origin(authority: Arc<StudioSessionAuthority>, configured: Option<&str>) -> Self {
        let mut root_store = RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        Self::from_origin_with_roots(authority, configured, root_store, None)
    }

    fn from_origin_with_roots(
        authority: Arc<StudioSessionAuthority>,
        configured: Option<&str>,
        root_store: RootCertStore,
        connect_addr: Option<SocketAddr>,
    ) -> Self {
        let endpoint = match configured
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ApiOrigin::parse)
        {
            Some(Ok(origin)) => Self::build_endpoint(&origin, root_store, connect_addr),
            _ => Err(AUTH_CONFIG_MISSING_CODE.to_owned()),
        };
        Self {
            authority,
            endpoint,
        }
    }

    fn build_endpoint(
        origin: &ApiOrigin,
        root_store: RootCertStore,
        connect_addr: Option<SocketAddr>,
    ) -> Result<AuthServerEndpoint, String> {
        let authority_part = origin
            .as_str()
            .strip_prefix("https://")
            .ok_or_else(|| AUTH_CONFIG_MISSING_CODE.to_owned())?;
        let (host, port) = match authority_part.rsplit_once(':') {
            Some((h, p)) => (
                h.to_owned(),
                p.parse::<u16>()
                    .map_err(|_| AUTH_CONFIG_MISSING_CODE.to_owned())?,
            ),
            None => (authority_part.to_owned(), 443),
        };
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let tls_config = ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|_| AUTH_CONFIG_MISSING_CODE.to_owned())?
            .with_root_certificates(root_store)
            .with_no_client_auth();
        Ok(AuthServerEndpoint {
            host,
            port,
            connect_addr,
            tls_config: Arc::new(tls_config),
        })
    }

    fn connect_tcp(endpoint: &AuthServerEndpoint) -> Result<TcpStream, String> {
        let timeout = Duration::from_secs(5);
        let stream = if let Some(addr) = endpoint.connect_addr {
            TcpStream::connect_timeout(&addr, timeout)
                .map_err(|_| AUTH_BACKEND_UNAVAILABLE_CODE.to_owned())?
        } else {
            let addrs: Vec<SocketAddr> = (endpoint.host.as_str(), endpoint.port)
                .to_socket_addrs()
                .map_err(|_| AUTH_BACKEND_UNAVAILABLE_CODE.to_owned())?
                .collect();
            let mut connected = None;
            for addr in addrs {
                if let Ok(candidate) = TcpStream::connect_timeout(&addr, timeout) {
                    connected = Some(candidate);
                    break;
                }
            }
            connected.ok_or_else(|| AUTH_BACKEND_UNAVAILABLE_CODE.to_owned())?
        };
        let _ = stream.set_read_timeout(Some(timeout));
        let _ = stream.set_write_timeout(Some(timeout));
        Ok(stream)
    }

    fn verify_via_https(
        &self,
        endpoint: &AuthServerEndpoint,
        token: &str,
        installation_id: &str,
        now_ms: u64,
    ) -> Result<VerifiedStudioSession, String> {
        let canonical_token = parse_safi_session_token(token)?;
        let canonical_inst = parse_installation_uuid(installation_id)?;
        let stream = Self::connect_tcp(endpoint)?;
        let server_name = ServerName::try_from(endpoint.host.clone())
            .map_err(|_| AUTH_CONFIG_MISSING_CODE.to_owned())?;
        let conn = ClientConnection::new(endpoint.tls_config.clone(), server_name)
            .map_err(|_| AUTH_BACKEND_UNAVAILABLE_CODE.to_owned())?;
        let mut tls = StreamOwned::new(conn, stream);

        let host_header = if endpoint.port == 443 {
            endpoint.host.clone()
        } else {
            format!("{}:{}", endpoint.host, endpoint.port)
        };
        let req = format!(
            "GET /v1/auth/sessions/current HTTP/1.1\r\nHost: {host_header}\r\nAuthorization: Bearer {canonical_token}\r\nX-Safi-Installation-Id: {canonical_inst}\r\nAccept: application/json\r\nAccept-Encoding: identity\r\nConnection: close\r\n\r\n"
        );
        tls.write_all(req.as_bytes())
            .map_err(|_| AUTH_BACKEND_UNAVAILABLE_CODE.to_owned())?;
        tls.flush()
            .map_err(|_| AUTH_BACKEND_UNAVAILABLE_CODE.to_owned())?;

        let mut buf = Vec::with_capacity(4096);
        let mut chunk = [0u8; 4096];
        loop {
            match tls.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    let remaining = (64_usize * 1024).saturating_sub(buf.len());
                    if remaining == 0 {
                        break;
                    }
                    let take = n.min(remaining);
                    buf.extend_from_slice(&chunk[..take]);
                    if take < n {
                        break;
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
                // Tolerate TLS close without close_notify once a full HTTP header block is already received.
                Err(_) if buf.windows(4).any(|w| w == b"\r\n\r\n") => break,
                Err(_) => return Err(AUTH_BACKEND_UNAVAILABLE_CODE.to_owned()),
            }
        }

        let response_text =
            std::str::from_utf8(&buf).map_err(|_| AUTH_BACKEND_UNAVAILABLE_CODE.to_owned())?;
        let (headers, raw_body) = response_text
            .split_once("\r\n\r\n")
            .ok_or_else(|| AUTH_BACKEND_UNAVAILABLE_CODE.to_owned())?;
        let status_line = headers
            .lines()
            .next()
            .ok_or_else(|| AUTH_BACKEND_UNAVAILABLE_CODE.to_owned())?;
        let status_code = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse::<u16>().ok())
            .ok_or_else(|| AUTH_BACKEND_UNAVAILABLE_CODE.to_owned())?;

        let is_chunked = headers.lines().any(|line| {
            line.split_once(':').is_some_and(|(name, value)| {
                name.trim().eq_ignore_ascii_case("transfer-encoding")
                    && value.to_ascii_lowercase().contains("chunked")
            })
        });
        let body = if is_chunked {
            decode_chunked_body(raw_body).ok_or_else(|| AUTH_BACKEND_UNAVAILABLE_CODE.to_owned())?
        } else {
            raw_body.trim().to_owned()
        };

        match status_code {
            200 => {
                let parsed: serde_json::Value = serde_json::from_str(&body)
                    .map_err(|_| AUTH_BACKEND_UNAVAILABLE_CODE.to_owned())?;
                let session_id = parsed["id"]
                    .as_str()
                    .ok_or_else(|| AUTH_BACKEND_UNAVAILABLE_CODE.to_owned())?;
                let workspace_id = parsed["workspace_id"]
                    .as_str()
                    .ok_or_else(|| AUTH_BACKEND_UNAVAILABLE_CODE.to_owned())?;
                let expires_at_secs = parsed["expires_at"]
                    .as_u64()
                    .ok_or_else(|| AUTH_BACKEND_UNAVAILABLE_CODE.to_owned())?;
                let expires_at_ms = expires_at_secs.saturating_mul(1_000);
                if now_ms >= expires_at_ms {
                    return Err(AUTH_EXPIRED_CODE.to_owned());
                }
                self.authority.register_session(
                    session_id,
                    workspace_id,
                    canonical_token,
                    &canonical_inst,
                    expires_at_ms,
                )
            }
            401 | 403 => {
                let code = serde_json::from_str::<serde_json::Value>(&body)
                    .ok()
                    .and_then(|v| v["code"].as_str().map(str::to_owned));
                match code.as_deref() {
                    Some("SF-AUTH-002") => Err(AUTH_EXPIRED_CODE.to_owned()),
                    Some("SF-AUTH-003") => Err(AUTH_REVOKED_CODE.to_owned()),
                    Some("SF-AUTH-009") => Err(AUTH_ACCOUNT_SUSPENDED_CODE.to_owned()),
                    Some("SF-AUTH-019") => Err(AUTH_DEVICE_REPLACED_CODE.to_owned()),
                    Some("SF-AUTH-020") => Err(AUTH_DEVICE_PROOF_INVALID_CODE.to_owned()),
                    _ => Err(AUTH_REQUIRED_CODE.to_owned()),
                }
            }
            _ => Err(AUTH_BACKEND_UNAVAILABLE_CODE.to_owned()),
        }
    }
}

fn decode_chunked_body(raw: &str) -> Option<String> {
    let mut rest = raw.trim_start();
    let mut out = String::new();
    loop {
        let (size_line, after_size) = rest.split_once("\r\n")?;
        let hex_len = size_line.split(';').next()?.trim();
        let size = usize::from_str_radix(hex_len, 16).ok()?;
        if size == 0 {
            return Some(out);
        }
        if after_size.len() < size {
            return None;
        }
        out.push_str(&after_size[..size]);
        rest = after_size[size..].strip_prefix("\r\n")?;
    }
}

impl StudioSessionVerifier for DesktopShellSessionVerifier {
    fn verify_session(
        &self,
        token: &str,
        installation_id: &str,
        now_ms: u64,
    ) -> Result<VerifiedStudioSession, String> {
        let _ = API_ORIGIN_ENV;
        match &self.endpoint {
            Ok(endpoint) => self.verify_via_https(endpoint, token, installation_id, now_ms),
            Err(code) => Err(code.clone()),
        }
    }
}

/// Measures the serialized byte size of an incoming `InvokeBody` after Tauri has parsed JSON into
/// `InvokeBody::Json(serde_json::Value)` (or the byte slice length of `InvokeBody::Raw`), prior to
/// command handler deserialization/execution and state mutation.
///
/// Note: Because Tauri parses incoming JSON before `invoke_handler` receives `InvokeBody`, this
/// guards command handlers and `StudioStore` state rather than raw pre-parse wire transport bytes.
fn measure_tauri_invoke_body(body: &tauri::ipc::InvokeBody) -> usize {
    match body {
        tauri::ipc::InvokeBody::Json(value) => json_serialized_len(value),
        tauri::ipc::InvokeBody::Raw(bytes) => bytes.len(),
    }
}

/// Enforces the native IPC command allowlist and `MAX_IPC_BYTES` ceiling on the parsed
/// `InvokeBody` at the Tauri `invoke_handler` boundary before any command handler runs.
fn enforce_tauri_ipc_boundary(command: &str, body: &tauri::ipc::InvokeBody) -> Result<(), String> {
    enforce_ipc_command(command, measure_tauri_invoke_body(body))
}

/// Rejects unknown commands (`SF-SYS-010`) or oversized serialized payloads (`SF-SYS-007`) before
/// dispatching to any command closure in tests.
#[cfg(test)]
fn dispatch_tauri_invoke_with_guard<F>(
    command: &str,
    body: &tauri::ipc::InvokeBody,
    dispatcher: F,
) -> Result<bool, String>
where
    F: FnOnce() -> bool,
{
    enforce_tauri_ipc_boundary(command, body)?;
    Ok(dispatcher())
}

/// Non-sensitive build information for the about screen.
#[tauri::command]
fn app_info() -> Result<serde_json::Value, String> {
    enforce_ipc_command("app_info", sf_desktop::ipc_payload_len!())?;
    Ok(serde_json::json!({
        "product": "SafiData",
        "version": env!("CARGO_PKG_VERSION"),
        "shell": "tauri-2.2"
    }))
}

/// Hand the UI a deep link that arrived while it was still starting.
#[tauri::command]
fn take_pending_deep_link(
    state: tauri::State<'_, PendingDeepLink>,
) -> Result<Option<DeepLinkAction>, String> {
    enforce_ipc_command("take_pending_deep_link", sf_desktop::ipc_payload_len!())?;
    Ok(state.0.lock().ok().and_then(|mut slot| slot.take()))
}

/// Launches the OS default browser strictly for a validated SafiData PKCE `/auth/desktop` URL.
///
/// Rejects any URL whose origin does not match `SAFI_API_BASE_URL` (or loopback dev origin in
/// debug builds), whose path is not `/auth/desktop`, or whose query params contain anything
/// outside `flow`, `request_id`, and `state` (`SF-SYS-009`).
#[tauri::command]
fn desktop_open_external_auth_url(url: String) -> Result<serde_json::Value, String> {
    enforce_ipc_command(
        "desktop_open_external_auth_url",
        sf_desktop::ipc_payload_len!(("url", &url)),
    )?;
    let configured_origin = option_env!("SAFI_API_BASE_URL")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            if cfg!(debug_assertions) {
                if url.starts_with("http://127.0.0.1:") || url.starts_with("http://localhost:") {
                    url.find("/auth/desktop?").map(|idx| &url[..idx])
                } else {
                    None
                }
            } else {
                None
            }
        })
        .ok_or_else(|| AUTH_CONFIG_MISSING_CODE.to_owned())?;
    sf_desktop::content::validate_desktop_auth_external_url(&url, configured_origin)
        .map_err(|err| err.code().as_str().to_owned())?;

    launch_os_browser(&url).map_err(|_| "SF-SYS-009".to_owned())?;
    Ok(serde_json::json!({ "opened": true }))
}

fn launch_os_browser(validated_url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", validated_url])
            .spawn()?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(validated_url)
            .spawn()?;
        Ok(())
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(validated_url)
            .spawn()?;
        Ok(())
    }
}

/// Native folder picker for selecting local folders to index in the embedded Local Search engine.
#[tauri::command]
async fn pick_search_folder(
    app: tauri::AppHandle,
    store: tauri::State<'_, StudioStore>,
) -> Result<Option<String>, String> {
    store.gate_command(
        "pick_search_folder",
        sf_desktop::ipc_payload_len!(),
        Some(now_ms()),
    )?;
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .blocking_pick_folder()
            .and_then(|path| path.into_path().ok())
            .map(|path| path.display().to_string())
    })
    .await
    .map_err(|_| "SF-SYS-011".to_owned())
}

/// Native open dialog restricted to `.safitemplate` portable recipe files.
#[tauri::command]
async fn pick_safitemplate_open_file(
    app: tauri::AppHandle,
    store: tauri::State<'_, StudioStore>,
) -> Result<Option<String>, String> {
    store.gate_command(
        "pick_safitemplate_open_file",
        sf_desktop::ipc_payload_len!(),
        Some(now_ms()),
    )?;
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("SafiData Template", &["safitemplate"])
            .blocking_pick_file()
            .and_then(|path| path.into_path().ok())
            .map(|path| path.display().to_string())
    })
    .await
    .map_err(|_| "SF-SYS-011".to_owned())
}

/// Native save dialog restricted to `.safitemplate` portable recipe files.
#[tauri::command]
async fn pick_safitemplate_save_file(
    app: tauri::AppHandle,
    store: tauri::State<'_, StudioStore>,
    default_name: Option<String>,
) -> Result<Option<String>, String> {
    let name = default_name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "recipe.safitemplate".to_owned());
    store.gate_command(
        "pick_safitemplate_save_file",
        sf_desktop::ipc_payload_len!(("defaultName", &name)),
        Some(now_ms()),
    )?;
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter("SafiData Template", &["safitemplate"])
            .set_file_name(&name)
            .blocking_save_file()
            .and_then(|path| path.into_path().ok())
            .map(|path| path.display().to_string())
    })
    .await
    .map_err(|_| "SF-SYS-011".to_owned())
}

/// Native open dialog restricted to spreadsheets (requires an authenticated Studio session).
#[tauri::command]
async fn pick_input_file(
    app: tauri::AppHandle,
    store: tauri::State<'_, StudioStore>,
) -> Result<Option<String>, String> {
    store.gate_command(
        "pick_input_file",
        sf_desktop::ipc_payload_len!(),
        Some(now_ms()),
    )?;
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter(
                "Spreadsheets",
                &["csv", "tsv", "xlsx", "xlsm", "xlsb", "xls", "ods"],
            )
            .blocking_pick_file()
            .and_then(|path| path.into_path().ok())
            .map(|path| path.display().to_string())
    })
    .await
    .map_err(|_| "SF-SYS-011".to_owned())
}

/// Native save dialog restricted to new CSV exports (requires an authenticated Studio session).
#[tauri::command]
async fn pick_output_file(
    app: tauri::AppHandle,
    store: tauri::State<'_, StudioStore>,
) -> Result<Option<String>, String> {
    store.gate_command(
        "pick_output_file",
        sf_desktop::ipc_payload_len!(),
        Some(now_ms()),
    )?;
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
    .map_err(|_| "SF-SYS-011".to_owned())
}

/// Native save dialog for `.safi.html` offline capsule exports (requires an authenticated Studio session).
#[tauri::command]
async fn pick_capsule_output_file(
    app: tauri::AppHandle,
    store: tauri::State<'_, StudioStore>,
) -> Result<Option<String>, String> {
    store.gate_command(
        "pick_capsule_output_file",
        sf_desktop::ipc_payload_len!(),
        Some(now_ms()),
    )?;
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
    .map_err(|_| "SF-SYS-011".to_owned())
}

/// Native save dialog for desktop deliverables (`.xlsx`, `.pptx`, `.docx`, `.pdf`, `.png`, `.svg`, `.csv`, `.zip`).
#[tauri::command]
async fn pick_deliverable_file(
    app: tauri::AppHandle,
    store: tauri::State<'_, StudioStore>,
    default_name: Option<String>,
    extension: Option<String>,
) -> Result<Option<String>, String> {
    let ext = extension.unwrap_or_else(|| "xlsx".to_owned());
    let name = default_name.unwrap_or_else(|| format!("safidata_deliverable.{ext}"));
    store.gate_command(
        "pick_deliverable_file",
        sf_desktop::ipc_payload_len!(("extension", &ext), ("defaultName", &name)),
        Some(now_ms()),
    )?;
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter(
                "SafiData Deliverable",
                &[
                    ext.as_str(),
                    "xlsx",
                    "pptx",
                    "docx",
                    "pdf",
                    "png",
                    "svg",
                    "csv",
                    "zip",
                ],
            )
            .set_file_name(&name)
            .blocking_save_file()
            .and_then(|path| path.into_path().ok())
            .map(|path| path.display().to_string())
    })
    .await
    .map_err(|_| "SF-SYS-011".to_owned())
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
fn studio_set_auth_session(
    store: tauri::State<'_, StudioStore>,
    token: String,
    installation_id: String,
) -> Result<String, String> {
    store.set_auth_session(&token, &installation_id, now_ms())
}

#[tauri::command]
fn studio_clear_auth_session(store: tauri::State<'_, StudioStore>) -> Result<String, String> {
    store.clear_auth_session()
}

#[tauri::command]
fn studio_authorization_status(store: tauri::State<'_, StudioStore>) -> Result<String, String> {
    store.authorization_status()
}

#[tauri::command]
fn studio_enter_offline_readonly(store: tauri::State<'_, StudioStore>) -> Result<String, String> {
    store.enter_offline_read_only()
}

#[tauri::command]
fn studio_revalidate_session(store: tauri::State<'_, StudioStore>) -> Result<String, String> {
    store.revalidate_session(now_ms())
}

#[tauri::command]
fn studio_open_file(
    store: tauri::State<'_, StudioStore>,
    project: String,
    path: String,
) -> Result<String, String> {
    store.open_file(&project, std::path::Path::new(&path), now_ms())
}

#[tauri::command]
fn studio_select_region(
    store: tauri::State<'_, StudioStore>,
    project: String,
    sheet_name: String,
    table_id: String,
) -> Result<String, String> {
    store.select_region(&project, &sheet_name, &table_id, now_ms())
}

#[tauri::command]
fn studio_workbook_details(
    store: tauri::State<'_, StudioStore>,
    project: String,
) -> Result<String, String> {
    store.workbook_details(&project)
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
#[allow(clippy::too_many_arguments)]
fn studio_chart_data(
    store: tauri::State<'_, StudioStore>,
    project: String,
    chart_type: String,
    function: String,
    column: String,
    group_by: String,
    filter_column: Option<String>,
    filter_value: Option<String>,
) -> Result<String, String> {
    store.chart_data_filtered(
        &project,
        &chart_type,
        &function,
        &column,
        &group_by,
        filter_column.as_deref(),
        filter_value.as_deref(),
    )
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
fn studio_add_source_column(
    store: tauri::State<'_, StudioStore>,
    project: String,
    name: String,
    default_value: String,
) -> Result<String, String> {
    store.add_source_column(&project, &name, &default_value, now_ms())
}

#[tauri::command]
fn studio_rename_column(
    store: tauri::State<'_, StudioStore>,
    project: String,
    old_name: String,
    new_name: String,
) -> Result<String, String> {
    store.rename_column(&project, &old_name, &new_name, now_ms())
}

#[tauri::command]
fn studio_delete_column(
    store: tauri::State<'_, StudioStore>,
    project: String,
    name: String,
) -> Result<String, String> {
    store.delete_column(&project, &name, now_ms())
}

#[tauri::command]
fn studio_trim_column(
    store: tauri::State<'_, StudioStore>,
    project: String,
    column: String,
) -> Result<String, String> {
    store.trim_column(&project, &column, now_ms())
}

#[tauri::command]
fn studio_set_artifacts(
    store: tauri::State<'_, StudioStore>,
    project: String,
    artifacts: String,
) -> Result<String, String> {
    store.set_artifacts(&project, &artifacts)
}

#[tauri::command]
fn studio_save_project(
    store: tauri::State<'_, StudioStore>,
    project: String,
    saved_id: String,
    display_name: String,
) -> Result<String, String> {
    store.save_project_bundle(&project, &saved_id, &display_name, now_ms())
}

#[tauri::command]
fn studio_list_projects(store: tauri::State<'_, StudioStore>) -> Result<String, String> {
    store.list_project_bundles()
}

#[tauri::command]
fn studio_load_project(
    store: tauri::State<'_, StudioStore>,
    project: String,
    saved_id: String,
) -> Result<String, String> {
    store.load_project_bundle(&project, &saved_id, now_ms())
}

#[tauri::command]
fn studio_delete_project(
    store: tauri::State<'_, StudioStore>,
    saved_id: String,
) -> Result<String, String> {
    store.delete_project_bundle(&saved_id)
}

#[tauri::command]
fn studio_list_templates(store: tauri::State<'_, StudioStore>) -> Result<String, String> {
    store.list_templates()
}

#[tauri::command]
fn studio_instantiate_template(
    store: tauri::State<'_, StudioStore>,
    project: String,
    template_id: String,
    bindings: String,
) -> Result<String, String> {
    store.instantiate_template(&project, &template_id, &bindings)
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

#[tauri::command]
fn studio_export_deliverable(
    store: tauri::State<'_, StudioStore>,
    project: String,
    target: String,
    is_base64: bool,
    payload: String,
) -> Result<String, String> {
    store.export_deliverable(&project, std::path::Path::new(&target), is_base64, &payload)
}

#[tauri::command]
fn desktop_search_index_folders(
    store: tauri::State<'_, StudioStore>,
    folders: String,
    project_id: Option<String>,
) -> Result<String, String> {
    store.desktop_search_index_folders(&folders, project_id.as_deref(), now_ms())
}

#[tauri::command]
fn desktop_search_query(
    store: tauri::State<'_, StudioStore>,
    query: String,
) -> Result<String, String> {
    store.desktop_search_query(&query)
}

#[tauri::command]
fn desktop_search_stats(store: tauri::State<'_, StudioStore>) -> Result<String, String> {
    store.desktop_search_stats()
}

#[tauri::command]
fn desktop_recipes_list(store: tauri::State<'_, StudioStore>) -> Result<String, String> {
    store.desktop_recipes_list()
}

#[tauri::command]
fn desktop_recipes_sync_official(
    store: tauri::State<'_, StudioStore>,
    templates: String,
) -> Result<String, String> {
    store.desktop_recipes_sync_official(&templates, now_ms())
}

#[tauri::command]
fn desktop_recipes_save_as_new(
    store: tauri::State<'_, StudioStore>,
    payload: String,
) -> Result<String, String> {
    store.desktop_recipes_save_as_new(&payload, now_ms())
}

#[tauri::command]
fn desktop_recipes_delete(
    store: tauri::State<'_, StudioStore>,
    recipe_id: String,
    confirmed: bool,
) -> Result<String, String> {
    store.desktop_recipes_delete(&recipe_id, confirmed, now_ms())
}

#[tauri::command]
fn desktop_recipes_export_safitemplate(
    store: tauri::State<'_, StudioStore>,
    recipe_id: String,
    target_path: String,
    premium_entitlement_verified: bool,
) -> Result<String, String> {
    store.desktop_recipes_export_safitemplate(
        &recipe_id,
        std::path::Path::new(&target_path),
        premium_entitlement_verified,
        now_ms(),
    )
}

#[tauri::command]
fn desktop_recipes_import_safitemplate(
    store: tauri::State<'_, StudioStore>,
    source_path: String,
    premium_entitlement_verified: bool,
    conflict_policy: Option<String>,
) -> Result<String, String> {
    store.desktop_recipes_import_safitemplate(
        std::path::Path::new(&source_path),
        premium_entitlement_verified,
        conflict_policy.as_deref().unwrap_or("fail_on_duplicate"),
        now_ms(),
    )
}

#[tauri::command]
fn desktop_recipes_prepare_community_share(
    store: tauri::State<'_, StudioStore>,
    recipe_id: String,
    consent_accepted: bool,
    consent_terms_version: String,
    show_contributor_attribution: bool,
) -> Result<String, String> {
    store.desktop_recipes_prepare_community_share(
        &recipe_id,
        consent_accepted,
        &consent_terms_version,
        show_contributor_attribution,
        now_ms(),
    )
}

#[tauri::command]
fn desktop_recipes_update_existing(
    store: tauri::State<'_, StudioStore>,
    payload: String,
) -> Result<String, String> {
    store.desktop_recipes_update_existing(&payload, now_ms())
}

#[tauri::command]
fn desktop_recipes_duplicate(
    store: tauri::State<'_, StudioStore>,
    recipe_id: String,
) -> Result<String, String> {
    store.desktop_recipes_duplicate(&recipe_id, now_ms())
}

#[tauri::command]
fn desktop_recipes_apply_to_project(
    store: tauri::State<'_, StudioStore>,
    project: String,
    recipe_id: String,
) -> Result<String, String> {
    store.desktop_recipes_apply_to_project(&project, &recipe_id, now_ms())
}

#[tauri::command]
fn desktop_workflow_memory_list(store: tauri::State<'_, StudioStore>) -> Result<String, String> {
    store.desktop_workflow_memory_list()
}

#[tauri::command]
fn desktop_workflow_memory_save_sheet(
    store: tauri::State<'_, StudioStore>,
    payload: String,
) -> Result<String, String> {
    store.desktop_workflow_memory_save_sheet(&payload, now_ms())
}

#[tauri::command]
fn desktop_workflow_memory_match_sheet(
    store: tauri::State<'_, StudioStore>,
    payload: String,
) -> Result<String, String> {
    store.desktop_workflow_memory_match_sheet(&payload)
}

#[tauri::command]
fn desktop_workflow_memory_apply_replay(
    store: tauri::State<'_, StudioStore>,
    project: String,
    workflow_id: String,
) -> Result<String, String> {
    store.desktop_workflow_memory_apply_replay(&project, &workflow_id, now_ms())
}

#[tauri::command]
fn desktop_workflow_memory_manage(
    store: tauri::State<'_, StudioStore>,
    payload: String,
) -> Result<String, String> {
    store.desktop_workflow_memory_manage(&payload, now_ms())
}

#[tauri::command]
fn desktop_learning_profile_get(store: tauri::State<'_, StudioStore>) -> Result<String, String> {
    store.desktop_learning_profile_get()
}

#[tauri::command]
fn desktop_learning_record_feedback(
    store: tauri::State<'_, StudioStore>,
    payload: String,
) -> Result<String, String> {
    store.desktop_learning_record_feedback(&payload, now_ms())
}

#[tauri::command]
fn desktop_learning_delete_rule(
    store: tauri::State<'_, StudioStore>,
    rule_id: String,
) -> Result<String, String> {
    store.desktop_learning_delete_rule(&rule_id, now_ms())
}

#[tauri::command]
fn desktop_learning_reset_profile(store: tauri::State<'_, StudioStore>) -> Result<String, String> {
    store.desktop_learning_reset_profile(now_ms())
}

fn studio_generated_handler(
) -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        app_info,
        take_pending_deep_link,
        desktop_open_external_auth_url,
        pick_input_file,
        pick_output_file,
        pick_capsule_output_file,
        pick_deliverable_file,
        pick_search_folder,
        pick_safitemplate_open_file,
        pick_safitemplate_save_file,
        studio_set_auth_session,
        studio_clear_auth_session,
        studio_authorization_status,
        studio_enter_offline_readonly,
        studio_revalidate_session,
        studio_open_file,
        studio_select_region,
        studio_workbook_details,
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
        studio_chart_data,
        studio_narrative,
        studio_snapshot,
        studio_restore,
        studio_add_source_column,
        studio_rename_column,
        studio_delete_column,
        studio_trim_column,
        studio_set_artifacts,
        studio_save_project,
        studio_list_projects,
        studio_load_project,
        studio_delete_project,
        studio_list_templates,
        studio_instantiate_template,
        studio_export_capsule,
        studio_export_deliverable,
        desktop_search_index_folders,
        desktop_search_query,
        desktop_search_stats,
        desktop_recipes_list,
        desktop_recipes_sync_official,
        desktop_recipes_save_as_new,
        desktop_recipes_update_existing,
        desktop_recipes_duplicate,
        desktop_recipes_apply_to_project,
        desktop_recipes_delete,
        desktop_recipes_export_safitemplate,
        desktop_recipes_import_safitemplate,
        desktop_recipes_prepare_community_share,
        desktop_workflow_memory_list,
        desktop_workflow_memory_save_sheet,
        desktop_workflow_memory_match_sheet,
        desktop_workflow_memory_apply_replay,
        desktop_workflow_memory_manage,
        desktop_learning_profile_get,
        desktop_learning_record_feedback,
        desktop_learning_delete_rule,
        desktop_learning_reset_profile
    ]
}

fn main() {
    if std::env::args().nth(1).as_deref() == Some("--print-build-origin") {
        let payload = serde_json::json!({
            "apiOrigin": option_env!("SAFI_API_BASE_URL").unwrap_or("")
        });
        println!("{payload}");
        return;
    }

    let generated_handler = studio_generated_handler();

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            focus_main_window(app);
            for argument in argv.iter().skip(1) {
                queue_deep_link(app, argument);
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(move |invoke: tauri::ipc::Invoke<tauri::Wry>| -> bool {
            if let Err(code) =
                enforce_tauri_ipc_boundary(invoke.message.command(), invoke.message.payload())
            {
                invoke.resolver.reject(code);
                return true;
            }
            generated_handler(invoke)
        })
        .setup(|app| {
            app.manage(PendingDeepLink::default());
            let storage_dir = app
                .path()
                .app_local_data_dir()
                .map(|dir| dir.join("studio-projects"))
                .unwrap_or_else(|_| std::env::temp_dir().join("safi-studio-projects"));
            let authority = Arc::new(StudioSessionAuthority::new());
            let verifier: Arc<dyn StudioSessionVerifier> =
                Arc::new(DesktopShellSessionVerifier::new(authority.clone()));
            app.manage(StudioStore::with_storage_dir_and_verifier(
                storage_dir,
                authority,
                verifier,
            ));
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

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use ring::rand::SystemRandom;
    use ring::signature::{EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_ASN1_SIGNING};
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
    use rustls::{RootCertStore, ServerConfig, ServerConnection, StreamOwned};
    use sf_desktop::studio::{
        StudioSessionAuthority, StudioSessionVerifier, StudioStore, AUTH_BACKEND_UNAVAILABLE_CODE,
        AUTH_CONFIG_MISSING_CODE, AUTH_EXPIRED_CODE, AUTH_REQUIRED_CODE, AUTH_REVOKED_CODE,
    };
    use sf_server::auth::cache::InMemoryAuthCache;
    use sf_server::auth::device::{DevicePlatform, DeviceRegistration};
    use sf_server::auth::repository::{
        InMemoryApiKeyRepository, InMemorySessionRepository, SessionRecord, SessionRepository,
    };
    use sf_server::auth::service::AuthService;
    use sf_server::auth::session::{generate_session_token, SessionView};
    use sf_server::auth::{unix_now, AuthMethod, Principal, Scope};
    use sf_server::{app, AppState};
    use tower::ServiceExt;
    use uuid::Uuid;

    use super::{decode_chunked_body, DesktopShellSessionVerifier};

    const CSV: &str = "region,amount\nCairo,100\nGiza,50\n";

    fn der_len(len: usize) -> Vec<u8> {
        if len < 128 {
            vec![len as u8]
        } else if len < 256 {
            vec![0x81, len as u8]
        } else {
            vec![0x82, (len >> 8) as u8, len as u8]
        }
    }

    fn der_tlv(tag: u8, body: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(1 + 4 + body.len());
        out.push(tag);
        out.extend_from_slice(&der_len(body.len()));
        out.extend_from_slice(body);
        out
    }

    fn der_seq(items: &[&[u8]]) -> Vec<u8> {
        let mut body = Vec::new();
        for item in items {
            body.extend_from_slice(item);
        }
        der_tlv(0x30, &body)
    }

    fn der_set(items: &[&[u8]]) -> Vec<u8> {
        let mut body = Vec::new();
        for item in items {
            body.extend_from_slice(item);
        }
        der_tlv(0x31, &body)
    }

    fn der_oid(bytes: &[u8]) -> Vec<u8> {
        der_tlv(0x06, bytes)
    }

    fn der_bit_string(bytes: &[u8]) -> Vec<u8> {
        let mut body = Vec::with_capacity(1 + bytes.len());
        body.push(0x00);
        body.extend_from_slice(bytes);
        der_tlv(0x03, &body)
    }

    fn der_cn(name: &str) -> Vec<u8> {
        let attr = der_seq(&[
            &der_oid(&[0x55, 0x04, 0x03]),
            &der_tlv(0x0c, name.as_bytes()),
        ]);
        let rdn = der_set(&[&attr]);
        der_seq(&[&rdn])
    }

    fn sign_x509_v3(
        serial: u8,
        issuer_cn: &str,
        subject_cn: &str,
        pub_key: &[u8],
        extensions: &[Vec<u8>],
        signer: &EcdsaKeyPair,
        rng: &SystemRandom,
    ) -> Vec<u8> {
        let version = der_tlv(0xa0, &der_tlv(0x02, &[2]));
        let serial_num = der_tlv(0x02, &[serial]);
        let sig_alg = der_seq(&[&der_oid(&[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x04, 0x03, 0x02])]);
        let issuer = der_cn(issuer_cn);
        let validity = der_seq(&[
            &der_tlv(0x17, b"200101000000Z"),
            &der_tlv(0x17, b"491231235959Z"),
        ]);
        let subject = der_cn(subject_cn);
        let spki = der_seq(&[
            &der_seq(&[
                &der_oid(&[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01]),
                &der_oid(&[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07]),
            ]),
            &der_bit_string(pub_key),
        ]);
        let ext_refs: Vec<&[u8]> = extensions.iter().map(Vec::as_slice).collect();
        let ext_seq = der_seq(&ext_refs);
        let ext_explicit = der_tlv(0xa3, &ext_seq);

        let tbs = der_seq(&[
            &version,
            &serial_num,
            &sig_alg,
            &issuer,
            &validity,
            &subject,
            &spki,
            &ext_explicit,
        ]);
        let sig = signer
            .sign(rng, &tbs)
            .expect("ECDSA test cert signing must succeed");
        der_seq(&[&tbs, &sig_alg, &der_bit_string(sig.as_ref())])
    }

    fn der_ext(oid: &[u8], critical: bool, value_der: &[u8]) -> Vec<u8> {
        let oid_part = der_oid(oid);
        let oct_part = der_tlv(0x04, value_der);
        if critical {
            der_seq(&[&oid_part, &[0x01, 0x01, 0xff], &oct_part])
        } else {
            der_seq(&[&oid_part, &oct_part])
        }
    }

    fn build_test_tls_material(san_dns: &str) -> (RootCertStore, Arc<ServerConfig>) {
        let rng = SystemRandom::new();
        let ca_pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng)
            .expect("CA key generation must succeed");
        let ca_key =
            EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, ca_pkcs8.as_ref(), &rng)
                .expect("CA key parse must succeed");
        let ca_exts = vec![
            der_ext(&[0x55, 0x1d, 0x13], true, &der_seq(&[&[0x01, 0x01, 0xff]])),
            der_ext(&[0x55, 0x1d, 0x0f], true, &[0x03, 0x02, 0x02, 0x04]),
        ];
        let ca_der = sign_x509_v3(
            1,
            "Safi Test Root CA",
            "Safi Test Root CA",
            ca_key.public_key().as_ref(),
            &ca_exts,
            &ca_key,
            &rng,
        );

        let leaf_pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng)
            .expect("leaf key generation must succeed");
        let leaf_key =
            EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, leaf_pkcs8.as_ref(), &rng)
                .expect("leaf key parse must succeed");
        let eku = der_seq(&[&der_oid(&[0x2b, 0x06, 0x01, 0x05, 0x05, 0x07, 0x03, 0x01])]);
        let san = der_seq(&[&der_tlv(0x82, san_dns.as_bytes())]);
        let leaf_exts = vec![
            der_ext(&[0x55, 0x1d, 0x13], true, &der_seq(&[])),
            der_ext(&[0x55, 0x1d, 0x0f], true, &[0x03, 0x02, 0x07, 0x80]),
            der_ext(&[0x55, 0x1d, 0x25], false, &eku),
            der_ext(&[0x55, 0x1d, 0x11], false, &san),
        ];
        let leaf_der = sign_x509_v3(
            2,
            "Safi Test Root CA",
            san_dns,
            leaf_key.public_key().as_ref(),
            &leaf_exts,
            &ca_key,
            &rng,
        );

        let mut root_store = RootCertStore::empty();
        root_store
            .add(CertificateDer::from(ca_der.clone()))
            .expect("CA cert must load into RootCertStore");

        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let server_cfg = ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .expect("TLS versions must configure")
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(leaf_der), CertificateDer::from(ca_der)],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(leaf_pkcs8.as_ref().to_vec())),
            )
            .expect("ServerConfig cert/key must load");

        (root_store, Arc::new(server_cfg))
    }

    struct TlsAuthTestHarness {
        addr: std::net::SocketAddr,
        root_store: RootCertStore,
        auth: Arc<AuthService>,
        sessions_repo: Arc<InMemorySessionRepository>,
        rt: Arc<tokio::runtime::Runtime>,
        stop: Arc<AtomicBool>,
        force_unavailable: Arc<AtomicBool>,
        handle: Option<std::thread::JoinHandle<()>>,
    }

    impl Drop for TlsAuthTestHarness {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            let _ = std::net::TcpStream::connect(self.addr);
            if let Some(handle) = self.handle.take() {
                let _ = handle.join();
            }
        }
    }

    impl TlsAuthTestHarness {
        fn start(host: &str) -> Self {
            let (root_store, server_cfg) = build_test_tls_material(host);
            let rt = Arc::new(
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("tokio test runtime must start"),
            );
            let api_keys = Arc::new(rt.block_on(InMemoryApiKeyRepository::new(vec![])));
            let sessions_repo = Arc::new(InMemorySessionRepository::new());
            let cache = Arc::new(InMemoryAuthCache::new());
            let state = AppState::new(AuthService::new(api_keys, sessions_repo.clone(), cache));
            let auth = state.auth.clone();
            let router = app(state);

            let listener = TcpListener::bind("127.0.0.1:0").expect("ephemeral bind must succeed");
            let addr = listener.local_addr().expect("local addr must exist");
            let stop = Arc::new(AtomicBool::new(false));
            let force_unavailable = Arc::new(AtomicBool::new(false));

            let stop_c = stop.clone();
            let unavail_c = force_unavailable.clone();
            let rt_c = rt.clone();
            let handle = std::thread::spawn(move || {
                for stream_res in listener.incoming() {
                    if stop_c.load(Ordering::SeqCst) {
                        break;
                    }
                    let Ok(stream) = stream_res else { continue };
                    let Ok(conn) = ServerConnection::new(server_cfg.clone()) else {
                        continue;
                    };
                    let mut tls = StreamOwned::new(conn, stream);
                    let mut buf = [0u8; 4096];
                    let mut raw = Vec::new();
                    loop {
                        match tls.read(&mut buf) {
                            Ok(0) => break,
                            Ok(n) => {
                                raw.extend_from_slice(&buf[..n]);
                                if raw.windows(4).any(|w| w == b"\r\n\r\n") {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    if stop_c.load(Ordering::SeqCst) {
                        break;
                    }
                    if unavail_c.load(Ordering::SeqCst) {
                        let _ = tls.write_all(
                            b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        );
                        let _ = tls.flush();
                        continue;
                    }
                    let req_text = String::from_utf8_lossy(&raw);
                    let mut lines = req_text.lines();
                    let first_line = lines.next().unwrap_or("");
                    let mut parts = first_line.split_whitespace();
                    let method = parts.next().unwrap_or("GET");
                    let uri = parts.next().unwrap_or("/");
                    let mut builder = Request::builder().method(method).uri(uri);
                    for line in lines {
                        if line.trim().is_empty() {
                            break;
                        }
                        if let Some((name, val)) = line.split_once(':') {
                            builder = builder.header(name.trim(), val.trim());
                        }
                    }
                    let axum_req = builder.body(Body::empty()).expect("HTTP request valid");
                    let resp = rt_c
                        .block_on(router.clone().oneshot(axum_req))
                        .expect("router response");
                    let status = resp.status();
                    let ct = resp
                        .headers()
                        .get("content-type")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("application/json")
                        .to_owned();
                    let body_bytes = rt_c
                        .block_on(to_bytes(resp.into_body(), 1024 * 1024))
                        .expect("body bytes");
                    let head = format!(
                        "HTTP/1.1 {} {}\r\nContent-Type: {ct}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        status.as_u16(),
                        status.canonical_reason().unwrap_or("OK"),
                        body_bytes.len()
                    );
                    let _ = tls.write_all(head.as_bytes());
                    let _ = tls.write_all(&body_bytes);
                    let _ = tls.flush();
                }
            });

            Self {
                addr,
                root_store,
                auth,
                sessions_repo,
                rt,
                stop,
                force_unavailable,
                handle: Some(handle),
            }
        }

        fn build_store(&self, storage_dir: std::path::PathBuf, host: &str) -> StudioStore {
            let authority = Arc::new(StudioSessionAuthority::new());
            let origin = format!("https://{host}");
            let verifier: Arc<dyn StudioSessionVerifier> =
                Arc::new(DesktopShellSessionVerifier::from_origin_with_roots(
                    authority.clone(),
                    Some(&origin),
                    self.root_store.clone(),
                    Some(self.addr),
                ));
            StudioStore::with_storage_dir_and_verifier(storage_dir, authority, verifier)
        }

        fn issue_session(
            &self,
            user_id: Uuid,
            workspace_id: Uuid,
            installation_id: Uuid,
        ) -> (String, SessionView) {
            let principal = Principal {
                user_id,
                workspace_id,
                scopes: vec![Scope::Read, Scope::Write],
                auth_method: AuthMethod::Password,
            };
            let reg = DeviceRegistration::from_installation_id(
                user_id,
                installation_id,
                DevicePlatform::Windows,
            )
            .expect("device registration");
            let issued = self
                .rt
                .block_on(
                    self.auth
                        .create_session_for_device(&principal, "Safi Desktop", reg),
                )
                .expect("session creation");
            (issued.token, issued.session)
        }
    }

    #[test]
    fn desktop_shell_verifier_covers_all_eleven_production_auth_cases() {
        let host = "api.safidata.invalid";
        let harness = TlsAuthTestHarness::start(host);
        let dir = tempfile::tempdir().expect("tempdir");
        let csv_path = dir.path().join("sales.csv");
        std::fs::write(&csv_path, CSV).expect("csv write");
        let projects_dir = dir.path().join("projects");

        let user_id = Uuid::now_v7();
        let workspace_id = Uuid::now_v7();
        let inst_a = Uuid::parse_str("11111111-2222-4333-8444-555555555555").expect("uuid");
        let inst_b = Uuid::parse_str("99999999-8888-4777-8666-555555555555").expect("uuid");
        let inst_a_str = inst_a.to_string();
        let inst_b_str = inst_b.to_string();
        let forged_token = "sfs_ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
        let now = (unix_now() as u64).saturating_mul(1_000);

        let store = harness.build_store(projects_dir.clone(), host);

        // 1. No session -> SF-AUTH-001
        assert_eq!(
            store.open_file("p1", &csv_path, now),
            Err(AUTH_REQUIRED_CODE.to_owned())
        );

        // 2. Invalid / forged token -> SF-AUTH-001
        assert_eq!(
            store.set_auth_session("not-valid", &inst_a_str, now),
            Err(AUTH_REQUIRED_CODE.to_owned())
        );
        assert_eq!(
            store.set_auth_session(forged_token, &inst_a_str, now),
            Err(AUTH_REQUIRED_CODE.to_owned())
        );

        // 3. Expired session on sf-server -> SF-AUTH-002
        let expired_tok = generate_session_token();
        let exp_record = SessionRecord {
            id: Uuid::now_v7(),
            workspace_id,
            user_id,
            api_key_id: None,
            token_hash: expired_tok.token_hash().to_owned(),
            device_name: "Safi Desktop".to_owned(),
            scopes: vec![Scope::Read, Scope::Write],
            created_at: unix_now() - 120,
            last_seen_at: unix_now() - 120,
            expires_at: unix_now() - 1,
            revoked: false,
            device_id: None,
            device_installation_key_hash: None,
            device_revoked: false,
            account_disabled: false,
        };
        let exp_dev =
            DeviceRegistration::from_installation_id(user_id, inst_a, DevicePlatform::Windows)
                .expect("device");
        harness
            .rt
            .block_on(
                harness
                    .sessions_repo
                    .replace_active(&exp_record, Some(&exp_dev)),
            )
            .expect("insert expired session");
        assert_eq!(
            store.set_auth_session(expired_tok.plaintext(), &inst_a_str, now),
            Err(AUTH_EXPIRED_CODE.to_owned())
        );

        // 6. Valid session on real TLS sf-server + sensitive IPC command execution
        let (valid_token_a, session_a) = harness.issue_session(user_id, workspace_id, inst_a);
        assert!(store
            .set_auth_session(&valid_token_a, &inst_a_str, now)
            .is_ok());
        assert!(store.open_file("p1", &csv_path, now).is_ok());
        assert!(store
            .save_project_bundle("p1", "bundle1", "Bundle 1", now)
            .is_ok());

        // 4. Revoked session -> SF-AUTH-003 on next IPC command and clears active session fail-closed
        let tok_hash = sf_server::auth::session::token_hash(
            sf_server::auth::session::parse_session_token(&valid_token_a).expect("parse"),
        );
        harness
            .rt
            .block_on(harness.auth.revoke_session(session_a.id, &tok_hash))
            .expect("revoke");
        assert_eq!(store.state("p1"), Err(AUTH_REVOKED_CODE.to_owned()));
        assert_eq!(store.state("p1"), Err(AUTH_REQUIRED_CODE.to_owned()));

        // 5. Replaced device -> old token/device rejected with SF-AUTH-003 when Device B signs in
        let (tok_dev_a, _) = harness.issue_session(user_id, workspace_id, inst_a);
        assert!(store.set_auth_session(&tok_dev_a, &inst_a_str, now).is_ok());
        let (tok_dev_b, _) = harness.issue_session(user_id, workspace_id, inst_b);
        assert_eq!(store.state("p1"), Err(AUTH_REVOKED_CODE.to_owned()));
        // Even trying to re-bind Device B's token with Device A's installation UUID fails with SF-AUTH-003
        assert_eq!(
            store.set_auth_session(&tok_dev_b, &inst_a_str, now),
            Err(AUTH_REVOKED_CODE.to_owned())
        );
        // Device B itself succeeds
        assert!(store.set_auth_session(&tok_dev_b, &inst_b_str, now).is_ok());
        assert!(store.state("p1").is_ok());

        // 7. Logout clears native session immediately -> SF-AUTH-001
        assert!(store.clear_auth_session().is_ok());
        assert_eq!(store.state("p1"), Err(AUTH_REQUIRED_CODE.to_owned()));

        // 8. App restart: fresh StudioStore starts unauthenticated (SF-AUTH-001),
        // verifies against TLS sf-server upon re-bind, and restores saved bundle cleanly.
        drop(store);
        let restarted_store = harness.build_store(projects_dir, host);
        assert_eq!(
            restarted_store.load_project_bundle("p1", "bundle1", now),
            Err(AUTH_REQUIRED_CODE.to_owned())
        );
        assert!(restarted_store
            .set_auth_session(&tok_dev_b, &inst_b_str, now)
            .is_ok());
        assert!(restarted_store
            .load_project_bundle("p1", "bundle1", now)
            .is_ok());

        // 9. Server unavailable (503 + closed port) -> SF-AUTH-005 fail-closed
        harness.force_unavailable.store(true, Ordering::SeqCst);
        assert_eq!(
            restarted_store.state("p1"),
            Err(AUTH_BACKEND_UNAVAILABLE_CODE.to_owned())
        );
        drop(restarted_store);
        let unreachable_port_addr = harness.addr;
        drop(harness);
        let store_down = StudioStore::with_storage_dir_and_verifier(
            dir.path().join("projects2"),
            Arc::new(StudioSessionAuthority::new()),
            Arc::new(DesktopShellSessionVerifier::from_origin_with_roots(
                Arc::new(StudioSessionAuthority::new()),
                Some("https://api.safidata.invalid"),
                RootCertStore::empty(),
                Some(unreachable_port_addr),
            )),
        );
        assert_eq!(
            store_down.set_auth_session(&tok_dev_b, &inst_b_str, now),
            Err(AUTH_BACKEND_UNAVAILABLE_CODE.to_owned())
        );

        // 10. Missing / invalid configuration -> SF-SYS-031 fail-closed
        let missing_auth = Arc::new(StudioSessionAuthority::new());
        let missing_verifier = DesktopShellSessionVerifier::from_origin(missing_auth.clone(), None);
        assert_eq!(
            missing_verifier.verify_session(&tok_dev_b, &inst_b_str, now),
            Err(AUTH_CONFIG_MISSING_CODE.to_owned())
        );
        let insecure_verifier =
            DesktopShellSessionVerifier::from_origin(missing_auth, Some("http://127.0.0.1:8080"));
        assert_eq!(
            insecure_verifier.verify_session(&tok_dev_b, &inst_b_str, now),
            Err(AUTH_CONFIG_MISSING_CODE.to_owned())
        );

        // 11. Installed packaged desktop startup without manual runtime env vars:
        // `DesktopShellSessionVerifier::new` never checks `SAFI_DESKTOP_AUTH_LOOPBACK_ADDR`
        // and uses the compile-time baked `option_env!("SAFI_API_BASE_URL")` (or fails closed
        // with SF-SYS-031 in unconfigured dev builds).
        let compiled_origin = option_env!("SAFI_API_BASE_URL");
        let startup_verifier =
            DesktopShellSessionVerifier::new(Arc::new(StudioSessionAuthority::new()));
        if compiled_origin.is_some() {
            assert!(startup_verifier.endpoint.is_ok());
        } else {
            assert_eq!(
                startup_verifier.verify_session(&tok_dev_b, &inst_b_str, now),
                Err(AUTH_CONFIG_MISSING_CODE.to_owned())
            );
        }

        // Verify chunked Transfer-Encoding decoder helper
        assert_eq!(
            decode_chunked_body("5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n"),
            Some("hello world".to_owned())
        );
    }

    #[test]
    fn tauri_ipc_boundary_rejects_oversized_serialized_json_before_studio_command_executes() {
        use std::sync::atomic::AtomicBool;
        use tauri::ipc::InvokeBody;

        use super::{dispatch_tauri_invoke_with_guard, measure_tauri_invoke_body};
        use sf_desktop::commands::MAX_IPC_BYTES;

        // 1. Unknown command never reaches handler
        let called = Arc::new(AtomicBool::new(false));
        let called_c = called.clone();
        let unknown_res = dispatch_tauri_invoke_with_guard(
            "shell_exec",
            &InvokeBody::Json(serde_json::json!({})),
            move || {
                called_c.store(true, Ordering::SeqCst);
                true
            },
        );
        assert_eq!(unknown_res, Err("SF-SYS-010".to_owned()));
        assert!(!called.load(Ordering::SeqCst));

        // 2. Parsed `InvokeBody::Json(serde_json::Value)` whose raw string chars are below MAX_IPC_BYTES,
        // but JSON keys, quotes, boolean/numeric literals, null, and escaped control chars push the
        // re-serialized JSON size to MAX_IPC_BYTES + 1 -> rejected with SF-SYS-007 BEFORE command
        // handler deserialization/execution or state mutation (not a raw pre-parse wire transport cap).
        let template_obj = serde_json::json!({
            "project": "p1",
            "row": 42usize,
            "column": "amount",
            "ascending": false,
            "expectedRevision": serde_json::Value::Null,
            "value": ""
        });
        let base_len = serde_json::to_vec(&template_obj)
            .expect("template JSON must serialize")
            .len();
        let exact_filler = "a".repeat(MAX_IPC_BYTES - base_len);
        let exact_body = InvokeBody::Json(serde_json::json!({
            "project": "p1",
            "row": 42usize,
            "column": "amount",
            "ascending": false,
            "expectedRevision": serde_json::Value::Null,
            "value": exact_filler
        }));
        assert_eq!(measure_tauri_invoke_body(&exact_body), MAX_IPC_BYTES);

        let dispatched_exact = Arc::new(AtomicBool::new(false));
        let dispatched_exact_c = dispatched_exact.clone();
        assert_eq!(
            dispatch_tauri_invoke_with_guard("studio_set_cell", &exact_body, move || {
                dispatched_exact_c.store(true, Ordering::SeqCst);
                true
            }),
            Ok(true)
        );
        assert!(dispatched_exact.load(Ordering::SeqCst));

        // Escaping one quote `"` adds 1 extra serialized byte (`\"`) without changing field count.
        let over_by_escape = format!("{}\"", "a".repeat(MAX_IPC_BYTES - base_len - 1));
        let over_body = InvokeBody::Json(serde_json::json!({
            "project": "p1",
            "row": 42usize,
            "column": "amount",
            "ascending": false,
            "expectedRevision": serde_json::Value::Null,
            "value": over_by_escape
        }));
        assert_eq!(measure_tauri_invoke_body(&over_body), MAX_IPC_BYTES + 1);

        let dispatched_over = Arc::new(AtomicBool::new(false));
        let dispatched_over_c = dispatched_over.clone();
        let over_res = dispatch_tauri_invoke_with_guard("studio_set_cell", &over_body, move || {
            dispatched_over_c.store(true, Ordering::SeqCst);
            true
        });
        assert_eq!(over_res, Err("SF-SYS-007".to_owned()));
        assert!(!dispatched_over.load(Ordering::SeqCst));

        // 3. Raw binary body > MAX_IPC_BYTES is also rejected before dispatch
        let dispatched_raw = Arc::new(AtomicBool::new(false));
        let dispatched_raw_c = dispatched_raw.clone();
        let raw_over = dispatch_tauri_invoke_with_guard(
            "studio_state",
            &InvokeBody::Raw(vec![b'x'; MAX_IPC_BYTES + 1]),
            move || {
                dispatched_raw_c.store(true, Ordering::SeqCst);
                true
            },
        );
        assert_eq!(raw_over, Err("SF-SYS-007".to_owned()));
        assert!(!dispatched_raw.load(Ordering::SeqCst));
    }

    #[test]
    #[ignore = "Manual performance benchmark for native TLS verification (run with --ignored --nocapture)"]
    fn benchmark_native_session_verifier_performance() {
        use std::time::Instant;

        let harness = TlsAuthTestHarness::start("api.safidata.invalid");
        let now_ms = u64::try_from(unix_now()).unwrap_or(1_700_000_000) * 1_000;
        let user_id = Uuid::new_v4();
        let workspace_id = Uuid::new_v4();
        let inst = Uuid::new_v4();
        let inst_str = inst.to_string();
        let (tok, session) = harness.issue_session(user_id, workspace_id, inst);

        let dir = tempfile::tempdir().expect("tempdir");
        let csv_path = dir.path().join("bench.csv");
        let mut csv = String::from("region,amount\n");
        for i in 0..100 {
            csv.push_str(&format!("Cairo,{}\n", 100 + i));
        }
        std::fs::write(&csv_path, csv).expect("csv write");

        // 1. In-memory baseline StudioStore (no per-command TLS roundtrip)
        let baseline_store = StudioStore::with_storage_dir(dir.path().join("baseline_projects"));
        baseline_store.set_clock_ms(Some(now_ms));
        baseline_store
            .session_authority()
            .register_session(
                &session.id.to_string(),
                &workspace_id.to_string(),
                &tok,
                &inst_str,
                session.expires_at as u64 * 1_000,
            )
            .expect("register baseline session");
        baseline_store
            .set_auth_session(&tok, &inst_str, now_ms)
            .expect("bind baseline auth");
        baseline_store
            .open_file("p_base", &csv_path, now_ms)
            .expect("open baseline csv");

        let t0 = Instant::now();
        for _ in 0..50 {
            baseline_store
                .rows_page("p_base", r#"{"offset":0,"limit":25}"#)
                .expect("baseline rows_page");
        }
        let baseline_rows_50 = t0.elapsed();

        let base_state = baseline_store.state("p_base").expect("baseline state");
        let base_rev = serde_json::from_str::<serde_json::Value>(&base_state)
            .expect("parse baseline state")["revision"]
            .as_u64()
            .expect("baseline revision");
        let t1 = Instant::now();
        for i in 0..50u64 {
            baseline_store
                .set_cell_by_id(
                    "p_base",
                    1,
                    "amount",
                    &format!("{}", 200 + i),
                    Some(base_rev + i),
                    now_ms,
                )
                .expect("baseline set_cell_by_id");
        }
        let baseline_edit_50 = t1.elapsed();

        // 2. Native DesktopShellSessionVerifier over localhost TLS (full TCP+TLS handshake + HTTP Close per command)
        let authority = Arc::new(StudioSessionAuthority::new());
        let verifier = Arc::new(DesktopShellSessionVerifier::from_origin_with_roots(
            authority.clone(),
            Some("https://api.safidata.invalid"),
            harness.root_store.clone(),
            Some(harness.addr),
        ));
        let tls_store = StudioStore::with_storage_dir_and_verifier(
            dir.path().join("tls_projects"),
            authority,
            verifier.clone(),
        );
        tls_store.set_clock_ms(Some(now_ms));
        tls_store
            .set_auth_session(&tok, &inst_str, now_ms)
            .expect("bind tls auth");
        tls_store
            .open_file("p_tls", &csv_path, now_ms)
            .expect("open tls csv");

        let mut verify_samples_us = Vec::with_capacity(50);
        for _ in 0..50 {
            let s = Instant::now();
            verifier
                .verify_session(&tok, &inst_str, now_ms)
                .expect("verify_session");
            verify_samples_us.push(s.elapsed().as_micros());
        }
        verify_samples_us.sort_unstable();

        let t2 = Instant::now();
        for _ in 0..50 {
            tls_store
                .rows_page("p_tls", r#"{"offset":0,"limit":25}"#)
                .expect("tls rows_page");
        }
        let tls_rows_50 = t2.elapsed();

        let tls_state = tls_store.state("p_tls").expect("tls state");
        let tls_rev = serde_json::from_str::<serde_json::Value>(&tls_state)
            .expect("parse tls state")["revision"]
            .as_u64()
            .expect("tls revision");
        let t3 = Instant::now();
        for i in 0..50u64 {
            tls_store
                .set_cell_by_id(
                    "p_tls",
                    1,
                    "amount",
                    &format!("{}", 200 + i),
                    Some(tls_rev + i),
                    now_ms,
                )
                .expect("tls set_cell_by_id");
        }
        let tls_edit_50 = t3.elapsed();

        // 3. Remote HTTPS endpoint RTT measurement (real public TLS endpoint -> DNS+TCP+TLS+HTTP round-trip)
        let mut system_roots = RootCertStore::empty();
        system_roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let remote_verifier = DesktopShellSessionVerifier::from_origin_with_roots(
            Arc::new(StudioSessionAuthority::new()),
            Some("https://api.github.com"),
            system_roots,
            None,
        );
        let t_remote = Instant::now();
        let remote_res = remote_verifier.verify_session(&tok, &inst_str, now_ms);
        let remote_elapsed = t_remote.elapsed();

        // 4. Offline / network interruption behavior
        drop(harness);
        let t_drop = Instant::now();
        let drop_res = tls_store.rows_page("p_tls", r#"{"offset":0,"limit":25}"#);
        let drop_elapsed = t_drop.elapsed();
        let post_drop_res = tls_store.state("p_tls");

        println!("=== NATIVE VERIFIER BENCHMARK REPORT ===");
        println!("verify_session_local_50_min_us={}", verify_samples_us[0]);
        println!(
            "verify_session_local_50_median_us={}",
            verify_samples_us[25]
        );
        println!("verify_session_local_50_p95_us={}", verify_samples_us[47]);
        println!(
            "baseline_rows_page_50_ms={:.3} baseline_set_cell_50_ms={:.3}",
            baseline_rows_50.as_secs_f64() * 1000.0,
            baseline_edit_50.as_secs_f64() * 1000.0
        );
        println!(
            "local_tls_rows_page_50_ms={:.3} local_tls_set_cell_50_ms={:.3}",
            tls_rows_50.as_secs_f64() * 1000.0,
            tls_edit_50.as_secs_f64() * 1000.0
        );
        println!(
            "remote_https_probe_host=api.github.com elapsed_ms={:.3} result={remote_res:?}",
            remote_elapsed.as_secs_f64() * 1000.0
        );
        println!(
            "network_drop_elapsed_ms={:.3} first_call={drop_res:?} subsequent_call={post_drop_res:?}",
            drop_elapsed.as_secs_f64() * 1000.0
        );
        assert_eq!(drop_res, Err(AUTH_BACKEND_UNAVAILABLE_CODE.to_owned()));
        assert_eq!(post_drop_res, Err(AUTH_REQUIRED_CODE.to_owned()));
    }
}
