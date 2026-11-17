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
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use sf_core::Decimal;
use sf_core::datagraph::{ColumnId, MetricId, RowId, ScenarioId, SourceId, VerifiedMetricToken};
use sf_core::error::SafiError;
use sf_core::limits::PlatformLimits;
use sf_core::table::Table;
use sf_io::{read_csv_stream, read_excel_stream};
use sf_recipe::datagraph::{
    AggFunc, ChangeOutcome, ChartType, DataGraph, RowQuery, datagraph_display,
};
use sf_recipe::studio_anomaly::{AnomalyFix, detect_anomalies};
use sf_recipe::studio_capsule::{
    CapsuleAsset, CapsuleChart, CapsuleDocBlock, CapsuleFreeFormBlock, CapsuleOptions,
    CapsuleSlide, export_capsule,
};
use sf_recipe::studio_dsl::{apply_command, parse_command, render_narrative};
use sf_recipe::studio_templates::{TEMPLATES, instantiate as instantiate_tpl, template_by_id};

use crate::commands::enforce_ipc_command;

/// Largest file the desktop app will import (`50 MiB`, matching the drop zone copy).
pub const MAX_STUDIO_IMPORT_BYTES: u64 = 50 * 1024 * 1024;

/// Code returned when a command names a project that is not open.
pub const UNKNOWN_PROJECT_CODE: &str = "SF-SYS-015";

/// Code returned when a capsule cannot be written next to the project.
pub const EXPORT_FAILED_CODE: &str = "SF-SYS-016";

/// Code returned when a Studio command runs without a valid authenticated session or bound device.
pub const AUTH_REQUIRED_CODE: &str = "SF-AUTH-001";

/// Code returned when the active Studio session has expired.
pub const AUTH_EXPIRED_CODE: &str = "SF-AUTH-002";

/// Code returned when the active Studio session or device installation has been revoked or replaced.
pub const AUTH_REVOKED_CODE: &str = "SF-AUTH-003";

/// Code returned when the authentication backend cannot be reached or returns a server error.
pub const AUTH_BACKEND_UNAVAILABLE_CODE: &str = "SF-AUTH-005";

/// Code returned when the account-service origin (`SAFI_API_BASE_URL`) is missing or invalid.
pub const AUTH_CONFIG_MISSING_CODE: &str = "SF-SYS-031";

const SESSION_TOKEN_PREFIX: &str = "sfs_";
const SESSION_RANDOM_HEX_LEN: usize = 64;

fn default_chart_type() -> String {
    "bar".to_owned()
}

/// Validates a Safi opaque session token (`sfs_<64 lowercase hex>`), matching `sf-server::auth::session`.
///
/// # Errors
/// Returns [`AUTH_REQUIRED_CODE`] when the token is empty or malformed.
pub fn parse_safi_session_token(raw: &str) -> Result<&str, String> {
    let trimmed = raw.trim();
    let Some(hex) = trimmed.strip_prefix(SESSION_TOKEN_PREFIX) else {
        return Err(AUTH_REQUIRED_CODE.to_owned());
    };
    if hex.len() != SESSION_RANDOM_HEX_LEN
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(AUTH_REQUIRED_CODE.to_owned());
    }
    Ok(trimmed)
}

/// Computes the `BLAKE3` hex digest of a validated `sfs_` session token.
///
/// # Errors
/// Returns [`AUTH_REQUIRED_CODE`] when `token` is not a valid `sfs_` token.
pub fn safi_session_token_hash(token: &str) -> Result<String, String> {
    let canonical = parse_safi_session_token(token)?;
    Ok(sf_recipe::blake3_hex(canonical.as_bytes()))
}

/// Validates a client device installation UUID (`8-4-4-4-12` hex, non-nil), matching `sf-server::auth::device`.
///
/// # Errors
/// Returns [`AUTH_REQUIRED_CODE`] when `raw` is not a non-nil canonical UUID.
pub fn parse_installation_uuid(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim().to_ascii_lowercase();
    if trimmed.len() != 36 {
        return Err(AUTH_REQUIRED_CODE.to_owned());
    }
    let parts: Vec<&str> = trimmed.split('-').collect();
    if parts.len() != 5
        || parts[0].len() != 8
        || parts[1].len() != 4
        || parts[2].len() != 4
        || parts[3].len() != 4
        || parts[4].len() != 12
        || !parts
            .iter()
            .all(|part| part.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err(AUTH_REQUIRED_CODE.to_owned());
    }
    if trimmed.bytes().all(|b| b == b'0' || b == b'-') {
        return Err(AUTH_REQUIRED_CODE.to_owned());
    }
    Ok(trimmed)
}

/// Computes a domain-separated `BLAKE3` fingerprint binding a workspace/account to one device installation.
///
/// # Errors
/// Returns [`AUTH_REQUIRED_CODE`] when `installation_id` is invalid or `workspace_id` is empty.
pub fn derive_device_installation_hash(
    workspace_id: &str,
    installation_id: &str,
) -> Result<String, String> {
    let ws = workspace_id.trim();
    if ws.is_empty() {
        return Err(AUTH_REQUIRED_CODE.to_owned());
    }
    let uuid = parse_installation_uuid(installation_id)?;
    let payload = format!("safidata.desktop.device.v1:{ws}:{uuid}");
    Ok(sf_recipe::blake3_hex(payload.as_bytes()))
}

/// Verified session claims returned by a [`StudioSessionVerifier`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedStudioSession {
    /// Server-issued session identifier.
    pub session_id: String,
    /// Bound workspace/account identifier.
    pub workspace_id: String,
    /// `BLAKE3` digest of the `sfs_` session token (raw token is never stored here).
    pub token_hash: String,
    /// Bound device installation fingerprint.
    pub installation_hash: String,
    /// Expiration timestamp in Unix milliseconds.
    pub expires_at_ms: u64,
}

/// Pluggable fail-closed session verifier enforced before every sensitive Studio IPC command.
pub trait StudioSessionVerifier: Send + Sync + std::fmt::Debug {
    /// Verifies `token` and `installation_id` at `now_ms`.
    ///
    /// # Errors
    /// Must fail closed with [`AUTH_REQUIRED_CODE`] (`SF-AUTH-001`) or [`AUTH_EXPIRED_CODE`] (`SF-AUTH-002`)
    /// whenever the token is malformed, unknown, revoked, expired, or bound to a revoked/different device.
    fn verify_session(
        &self,
        token: &str,
        installation_id: &str,
        now_ms: u64,
    ) -> Result<VerifiedStudioSession, String>;
}

#[derive(Clone)]
struct AuthoritySessionRecord {
    session_id: String,
    workspace_id: String,
    token_hash: String,
    installation_hash: String,
    expires_at_ms: u64,
    revoked: bool,
    device_revoked: bool,
}

impl std::fmt::Debug for AuthoritySessionRecord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AuthoritySessionRecord")
            .field("session_id", &self.session_id)
            .field("workspace_id", &self.workspace_id)
            .field("token_hash", &"[REDACTED]")
            .field("installation_hash", &"[REDACTED]")
            .field("expires_at_ms", &self.expires_at_ms)
            .field("revoked", &self.revoked)
            .field("device_revoked", &self.device_revoked)
            .finish()
    }
}

/// Authoritative session & single-device registry enforcing Safi's `sfs_` token hash, expiration,
/// server revocation, and single-device replacement policy (`P0.04` / `P0.05`).
#[derive(Debug, Default)]
pub struct StudioSessionAuthority {
    sessions_by_hash: Mutex<BTreeMap<String, AuthoritySessionRecord>>,
    active_device_by_workspace: Mutex<BTreeMap<String, String>>,
}

impl StudioSessionAuthority {
    /// Creates an empty authority (fail-closed until a verified session is registered).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a server-issued session bound to `(workspace_id, installation_id)`.
    ///
    /// Enforces the single-device policy: if `workspace_id` was previously bound to a different
    /// device installation, all previous sessions for that workspace on the old device are
    /// atomically marked `device_revoked = true` and `revoked = true`.
    ///
    /// # Errors
    /// Returns [`AUTH_REQUIRED_CODE`] when `token`, `installation_id`, `session_id`, or `workspace_id`
    /// is invalid.
    pub fn register_session(
        &self,
        session_id: &str,
        workspace_id: &str,
        token: &str,
        installation_id: &str,
        expires_at_ms: u64,
    ) -> Result<VerifiedStudioSession, String> {
        let sid = session_id.trim();
        let wid = workspace_id.trim();
        if sid.is_empty() || wid.is_empty() || expires_at_ms == 0 {
            return Err(AUTH_REQUIRED_CODE.to_owned());
        }
        let token_hash = safi_session_token_hash(token)?;
        let installation_hash = derive_device_installation_hash(wid, installation_id)?;

        let mut sessions = self
            .sessions_by_hash
            .lock()
            .map_err(|_| AUTH_REQUIRED_CODE.to_owned())?;
        let mut active_devices = self
            .active_device_by_workspace
            .lock()
            .map_err(|_| AUTH_REQUIRED_CODE.to_owned())?;

        // Single-device policy (P0.04): replace any previous active session/device for this workspace.
        for record in sessions.values_mut() {
            if record.workspace_id == wid {
                if record.installation_hash != installation_hash {
                    record.device_revoked = true;
                }
                record.revoked = true;
            }
        }
        let _ = active_devices.insert(wid.to_owned(), installation_hash.clone());
        let record = AuthoritySessionRecord {
            session_id: sid.to_owned(),
            workspace_id: wid.to_owned(),
            token_hash: token_hash.clone(),
            installation_hash: installation_hash.clone(),
            expires_at_ms,
            revoked: false,
            device_revoked: false,
        };
        let _ = sessions.insert(token_hash.clone(), record);
        Ok(VerifiedStudioSession {
            session_id: sid.to_owned(),
            workspace_id: wid.to_owned(),
            token_hash,
            installation_hash,
            expires_at_ms,
        })
    }

    /// Revokes a session by its `session_id` (simulating or mirroring server-side session revocation).
    pub fn revoke_session(&self, session_id: &str) {
        if let Ok(mut sessions) = self.sessions_by_hash.lock() {
            for record in sessions.values_mut() {
                if record.session_id == session_id {
                    record.revoked = true;
                }
            }
        }
    }

    /// Revokes a session by its raw `sfs_` token.
    pub fn revoke_token(&self, token: &str) {
        let Ok(hash) = safi_session_token_hash(token) else {
            return;
        };
        if let Ok(mut sessions) = self.sessions_by_hash.lock()
            && let Some(record) = sessions.get_mut(&hash)
        {
            record.revoked = true;
        }
    }

    /// Revokes a device installation for `workspace_id` (simulating or mirroring server-side device revocation).
    pub fn revoke_device(&self, workspace_id: &str, installation_id: &str) {
        let Ok(inst_hash) = derive_device_installation_hash(workspace_id, installation_id) else {
            return;
        };
        if let Ok(mut sessions) = self.sessions_by_hash.lock() {
            for record in sessions.values_mut() {
                if record.workspace_id == workspace_id && record.installation_hash == inst_hash {
                    record.device_revoked = true;
                    record.revoked = true;
                }
            }
        }
        if let Ok(mut devices) = self.active_device_by_workspace.lock()
            && devices.get(workspace_id) == Some(&inst_hash)
        {
            let _ = devices.remove(workspace_id);
        }
    }
}

impl StudioSessionVerifier for StudioSessionAuthority {
    fn verify_session(
        &self,
        token: &str,
        installation_id: &str,
        now_ms: u64,
    ) -> Result<VerifiedStudioSession, String> {
        let token_hash = safi_session_token_hash(token)?;
        let _ = parse_installation_uuid(installation_id)?;
        let sessions = self
            .sessions_by_hash
            .lock()
            .map_err(|_| AUTH_REQUIRED_CODE.to_owned())?;
        let active_devices = self
            .active_device_by_workspace
            .lock()
            .map_err(|_| AUTH_REQUIRED_CODE.to_owned())?;
        let record = sessions
            .get(&token_hash)
            .ok_or_else(|| AUTH_REQUIRED_CODE.to_owned())?;
        if record.revoked || record.device_revoked {
            return Err(AUTH_REVOKED_CODE.to_owned());
        }
        let caller_inst_hash =
            derive_device_installation_hash(&record.workspace_id, installation_id)?;
        if caller_inst_hash != record.installation_hash {
            return Err(AUTH_REVOKED_CODE.to_owned());
        }
        if active_devices.get(&record.workspace_id) != Some(&record.installation_hash) {
            return Err(AUTH_REVOKED_CODE.to_owned());
        }
        if now_ms >= record.expires_at_ms {
            return Err(AUTH_EXPIRED_CODE.to_owned());
        }
        Ok(VerifiedStudioSession {
            session_id: record.session_id.clone(),
            workspace_id: record.workspace_id.clone(),
            token_hash: record.token_hash.clone(),
            installation_hash: record.installation_hash.clone(),
            expires_at_ms: record.expires_at_ms,
        })
    }
}

#[derive(Clone)]
struct ActiveStudioSession {
    token: String,
    installation_id: String,
    verified: VerifiedStudioSession,
}

impl std::fmt::Debug for ActiveStudioSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ActiveStudioSession")
            .field("token", &"[REDACTED]")
            .field("installation_id", &"[REDACTED]")
            .field("session_id", &self.verified.session_id)
            .field("workspace_id", &self.verified.workspace_id)
            .field("expires_at_ms", &self.verified.expires_at_ms)
            .finish()
    }
}

/// Chart layout spec persisted inside a Studio project bundle.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StudioChartArtifact {
    /// Stable chart id.
    pub id: String,
    /// Display title.
    pub title: String,
    /// Chart visual type (`bar`, `line`, `area`, `pie`, `donut`, `scatter`, `histogram`, `kpi`).
    #[serde(default = "default_chart_type")]
    pub chart_type: String,
    /// Aggregation (`SUM`, `AVG`, `COUNT`, `MIN`, `MAX`).
    pub function: String,
    /// Value column name (or Y column for `scatter`).
    pub column: String,
    /// Group-by column name (or X column for `scatter`).
    pub group_by: String,
}

/// Slide definition persisted inside a Studio project bundle.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StudioSlideArtifact {
    /// Stable slide id.
    pub id: String,
    /// Slide heading.
    pub title: String,
    /// Slide prose (may contain `{{metric:id}}`).
    pub body: String,
    /// Optional spotlight metric id.
    pub metric_id: Option<String>,
    /// Optional embedded chart index.
    pub chart_index: Option<usize>,
    /// Layout preset (`kpi_split`, `chart_focus`, `narrative`).
    pub layout: String,
}

/// Document block persisted inside a Studio project bundle.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StudioDocBlockArtifact {
    /// Stable block id.
    pub id: String,
    /// Block kind (`heading`, `paragraph`, `metric_callout`, `callout`).
    pub kind: String,
    /// Block title.
    pub title: String,
    /// Block content (may contain `{{metric:id}}`).
    pub content: String,
    /// Optional metric reference id.
    pub metric_id: Option<String>,
    /// Authorship attribution (`user` or `ai`).
    pub authored_by: String,
}

/// Positioned Free-Form Builder block persisted inside a Studio project bundle.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StudioFreeFormBlockArtifact {
    /// Stable block id.
    pub id: String,
    /// Block kind (`kpi`, `text`, `image`).
    pub kind: String,
    /// Block title.
    pub title: String,
    /// Block prose (may contain `{{metric:id}}`).
    pub body: String,
    /// Optional metric id.
    pub metric_id: Option<String>,
    /// Optional local asset id.
    pub asset_id: Option<String>,
    /// 1-based grid column (`1..=12`).
    pub col: u8,
    /// 1-based grid row (`1..=32`).
    pub row: u8,
    /// Column span (`1..=12`).
    pub col_span: u8,
    /// Row span (`1..=8`).
    pub row_span: u8,
    /// Layer ordering (`1..=50`).
    pub z_index: u16,
}

/// Local raster asset persisted inside a Studio project bundle.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StudioAssetArtifact {
    /// Stable asset id.
    pub id: String,
    /// Alt / display name.
    pub name: String,
    /// Validated raster `data:image/...` URI.
    pub data_uri: String,
}

/// Customized Living Template configuration per project.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StudioTemplateConfigArtifact {
    /// Selected template id from `studio_templates::TEMPLATES`.
    pub template_id: String,
    /// Optional custom deck/report title.
    pub title_override: Option<String>,
    /// Mapping from template role (`headline`, `primary_metric`, etc.) to `MetricId`.
    pub bindings: BTreeMap<String, String>,
    /// Editable section heading overrides keyed by section `headingKey`.
    pub section_overrides: BTreeMap<String, String>,
}

/// Complete Quad-Studio non-table creative artifacts for one project.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StudioWorkspaceArtifacts {
    /// Dashboard charts.
    pub charts: Vec<StudioChartArtifact>,
    /// Keynote slides.
    pub slides: Vec<StudioSlideArtifact>,
    /// Smart Verified Document blocks.
    pub doc_blocks: Vec<StudioDocBlockArtifact>,
    /// Free-Form canvas blocks.
    pub freeform_blocks: Vec<StudioFreeFormBlockArtifact>,
    /// Embedded raster assets.
    pub assets: Vec<StudioAssetArtifact>,
    /// Living Template selection and customizations.
    pub template_config: Option<StudioTemplateConfigArtifact>,
}

/// Integrity-checked project bundle persisted on local disk.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct StudioProjectBundle {
    schema_version: u32,
    project_id: String,
    display_name: String,
    updated_at_ms: u64,
    snapshot_json: String,
    artifacts: StudioWorkspaceArtifacts,
    checksum_blake3: String,
}

#[derive(serde::Serialize)]
struct BundleChecksumPayload<'a> {
    schema_version: u32,
    project_id: &'a str,
    display_name: &'a str,
    updated_at_ms: u64,
    snapshot_json: &'a str,
    artifacts: &'a StudioWorkspaceArtifacts,
}

fn bundle_checksum(
    schema_version: u32,
    project_id: &str,
    display_name: &str,
    updated_at_ms: u64,
    snapshot_json: &str,
    artifacts: &StudioWorkspaceArtifacts,
) -> Result<String, String> {
    let encoded = serde_json::to_vec(&BundleChecksumPayload {
        schema_version,
        project_id,
        display_name,
        updated_at_ms,
        snapshot_json,
        artifacts,
    })
    .map_err(|_| "SF-SYS-001".to_owned())?;
    Ok(sf_recipe::blake3_hex(&encoded))
}

fn validate_project_id(id: &str) -> Result<&str, String> {
    let trimmed = id.trim();
    if trimmed.is_empty()
        || trimmed.len() > 64
        || !trimmed
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
    {
        return Err("SF-SYS-001".to_owned());
    }
    Ok(trimmed)
}

fn validate_freeform_block(block: &StudioFreeFormBlockArtifact) -> Result<(), String> {
    if !(1..=12).contains(&block.col)
        || !(1..=32).contains(&block.row)
        || !(1..=12).contains(&block.col_span)
        || !(1..=8).contains(&block.row_span)
        || !(1..=50).contains(&block.z_index)
    {
        return Err("SF-CAL-002".to_owned());
    }
    Ok(())
}

fn validate_workspace_artifacts(artifacts: &StudioWorkspaceArtifacts) -> Result<(), String> {
    for chart in &artifacts.charts {
        if ChartType::from_name(&chart.chart_type).is_none()
            || AggFunc::from_name(&chart.function).is_none()
        {
            return Err("SF-CAL-002".to_owned());
        }
    }
    for block in &artifacts.freeform_blocks {
        validate_freeform_block(block)?;
    }
    Ok(())
}

fn system_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// Open Studio projects, keyed by the identifier the UI chose, guarded by a fail-closed
/// native session and single-device gate.
#[derive(Debug)]
pub struct StudioStore {
    projects: Mutex<BTreeMap<String, DataGraph>>,
    artifacts: Mutex<BTreeMap<String, StudioWorkspaceArtifacts>>,
    storage_dir: Mutex<Option<PathBuf>>,
    active_session: Mutex<Option<ActiveStudioSession>>,
    authority: Arc<StudioSessionAuthority>,
    verifier: Arc<dyn StudioSessionVerifier>,
    clock_ms: Mutex<Option<u64>>,
}

impl Default for StudioStore {
    fn default() -> Self {
        let authority = Arc::new(StudioSessionAuthority::new());
        let verifier: Arc<dyn StudioSessionVerifier> = authority.clone();
        Self {
            projects: Mutex::new(BTreeMap::new()),
            artifacts: Mutex::new(BTreeMap::new()),
            storage_dir: Mutex::new(None),
            active_session: Mutex::new(None),
            authority,
            verifier,
            clock_ms: Mutex::new(None),
        }
    }
}

impl StudioStore {
    /// Creates an empty store (fail-closed until a verified session is bound).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a store backed by an atomic multi-project folder inside `dir`.
    #[must_use]
    pub fn with_storage_dir(dir: PathBuf) -> Self {
        let authority = Arc::new(StudioSessionAuthority::new());
        let verifier: Arc<dyn StudioSessionVerifier> = authority.clone();
        Self {
            projects: Mutex::new(BTreeMap::new()),
            artifacts: Mutex::new(BTreeMap::new()),
            storage_dir: Mutex::new(Some(dir)),
            active_session: Mutex::new(None),
            authority,
            verifier,
            clock_ms: Mutex::new(None),
        }
    }

    /// Creates a store backed by `dir` and a custom [`StudioSessionVerifier`].
    #[must_use]
    pub fn with_storage_dir_and_verifier(
        dir: PathBuf,
        authority: Arc<StudioSessionAuthority>,
        verifier: Arc<dyn StudioSessionVerifier>,
    ) -> Self {
        Self {
            projects: Mutex::new(BTreeMap::new()),
            artifacts: Mutex::new(BTreeMap::new()),
            storage_dir: Mutex::new(Some(dir)),
            active_session: Mutex::new(None),
            authority,
            verifier,
            clock_ms: Mutex::new(None),
        }
    }

    /// Returns the shared session authority used to register/revoke sessions and devices.
    #[must_use]
    pub fn session_authority(&self) -> Arc<StudioSessionAuthority> {
        self.authority.clone()
    }

    /// Overrides the internal clock used by read-only commands during deterministic tests.
    pub fn set_clock_ms(&self, now_ms: Option<u64>) {
        if let Ok(mut guard) = self.clock_ms.lock() {
            *guard = now_ms;
        }
    }

    fn effective_now_ms(&self, caller_now_ms: Option<u64>) -> u64 {
        if let Ok(guard) = self.clock_ms.lock()
            && let Some(pinned) = *guard
        {
            return pinned;
        }
        caller_now_ms.unwrap_or_else(system_now_ms)
    }

    /// Verifies and binds an authenticated Safi session (`sfs_...`) and device installation UUID
    /// into the native Tauri store.
    ///
    /// Never trusts a token merely because it arrived from React: the configured
    /// [`StudioSessionVerifier`] must confirm that the token exists, is unrevoked, is unexpired,
    /// and is bound to the authorized device for the workspace.
    ///
    /// # Errors
    /// Returns [`AUTH_REQUIRED_CODE`] (`SF-AUTH-001`), [`AUTH_EXPIRED_CODE`] (`SF-AUTH-002`),
    /// [`AUTH_REVOKED_CODE`] (`SF-AUTH-003`), [`AUTH_BACKEND_UNAVAILABLE_CODE`] (`SF-AUTH-005`),
    /// [`AUTH_CONFIG_MISSING_CODE`] (`SF-SYS-031`), or `SF-SYS-007` when the payload exceeds
    /// [`crate::commands::MAX_IPC_BYTES`].
    pub fn set_auth_session(
        &self,
        token: &str,
        installation_id: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len =
            crate::ipc_payload_len!(("token", token), ("installationId", installation_id));
        enforce_ipc_command("studio_set_auth_session", payload_len)?;
        let effective_now = self.effective_now_ms(Some(now_ms));
        let verified = match self
            .verifier
            .verify_session(token, installation_id, effective_now)
        {
            Ok(session) => session,
            Err(code) => {
                if let Ok(mut slot) = self.active_session.lock() {
                    *slot = None;
                }
                return Err(code);
            }
        };
        let mut slot = self
            .active_session
            .lock()
            .map_err(|_| AUTH_REQUIRED_CODE.to_owned())?;
        *slot = Some(ActiveStudioSession {
            token: token.trim().to_owned(),
            installation_id: installation_id.trim().to_ascii_lowercase(),
            verified: verified.clone(),
        });
        drop(slot);
        serde_json::to_string(&serde_json::json!({
            "authenticated": true,
            "sessionId": verified.session_id,
            "workspaceId": verified.workspace_id,
            "expiresAtMs": verified.expires_at_ms,
        }))
        .map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Immediately clears the native authenticated session context on sign-out.
    ///
    /// # Errors
    /// Returns `SF-SYS-010` or `SF-SYS-001` on internal failure.
    pub fn clear_auth_session(&self) -> Result<String, String> {
        enforce_ipc_command("studio_clear_auth_session", crate::ipc_payload_len!())?;
        let mut slot = self
            .active_session
            .lock()
            .map_err(|_| AUTH_REQUIRED_CODE.to_owned())?;
        *slot = None;
        drop(slot);
        serde_json::to_string(&serde_json::json!({
            "authenticated": false
        }))
        .map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Enforces the native command allowlist, `MAX_IPC_BYTES` canonical serialized-JSON argument
    /// ceiling (1 MiB checked prior to state mutation; does not cap pre-parse raw wire bytes inside
    /// WebView2/Tauri), and fail-closed session + single-device verification before any sensitive
    /// command executes.
    ///
    /// # Errors
    /// Returns `SF-SYS-010`, `SF-SYS-007`, [`AUTH_REQUIRED_CODE`] (`SF-AUTH-001`),
    /// [`AUTH_EXPIRED_CODE`] (`SF-AUTH-002`), [`AUTH_REVOKED_CODE`] (`SF-AUTH-003`),
    /// [`AUTH_BACKEND_UNAVAILABLE_CODE`] (`SF-AUTH-005`), or [`AUTH_CONFIG_MISSING_CODE`] (`SF-SYS-031`).
    pub fn gate_command(
        &self,
        command: &str,
        payload_len: usize,
        caller_now_ms: Option<u64>,
    ) -> Result<VerifiedStudioSession, String> {
        enforce_ipc_command(command, payload_len)?;
        let active = {
            let guard = self
                .active_session
                .lock()
                .map_err(|_| AUTH_REQUIRED_CODE.to_owned())?;
            guard.clone().ok_or_else(|| AUTH_REQUIRED_CODE.to_owned())?
        };
        let now_ms = self.effective_now_ms(caller_now_ms);
        match self
            .verifier
            .verify_session(&active.token, &active.installation_id, now_ms)
        {
            Ok(verified) => Ok(verified),
            Err(code) => {
                if let Ok(mut guard) = self.active_session.lock() {
                    *guard = None;
                }
                Err(code)
            }
        }
    }

    fn resolve_storage_dir(&self) -> Result<PathBuf, String> {
        let mut guard = self
            .storage_dir
            .lock()
            .map_err(|_| "SF-SYS-001".to_owned())?;
        if let Some(existing) = guard.as_ref() {
            fs::create_dir_all(existing).map_err(|_| EXPORT_FAILED_CODE.to_owned())?;
            return Ok(existing.clone());
        }
        let fallback = std::env::temp_dir().join("safi-studio-projects");
        fs::create_dir_all(&fallback).map_err(|_| EXPORT_FAILED_CODE.to_owned())?;
        *guard = Some(fallback.clone());
        Ok(fallback)
    }

    fn current_artifacts(&self, project: &str) -> StudioWorkspaceArtifacts {
        self.artifacts
            .lock()
            .ok()
            .and_then(|map| map.get(project).cloned())
            .unwrap_or_default()
    }

    fn state_unlocked(&self, project: &str) -> Result<String, String> {
        let artifacts = self.current_artifacts(project);
        let projects = self
            .projects
            .lock()
            .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
        let graph = projects
            .get(project)
            .ok_or_else(|| UNKNOWN_PROJECT_CODE.to_owned())?;
        state_json(graph, &artifacts)
    }

    fn snapshot_unlocked(&self, project: &str) -> Result<String, String> {
        let projects = self
            .projects
            .lock()
            .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
        let graph = projects
            .get(project)
            .ok_or_else(|| UNKNOWN_PROJECT_CODE.to_owned())?;
        graph.to_json().map_err(|error| code_of(&error))
    }

    fn list_project_bundles_unlocked(&self) -> Result<String, String> {
        let dir = self.resolve_storage_dir()?;
        let mut entries = Vec::new();
        let read_dir = fs::read_dir(&dir).map_err(|_| EXPORT_FAILED_CODE.to_owned())?;
        for item in read_dir {
            let Ok(entry) = item else { continue };
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let Ok(raw) = fs::read_to_string(&path) else {
                continue;
            };
            if let Ok(bundle) = serde_json::from_str::<StudioProjectBundle>(&raw) {
                let valid = bundle_checksum(
                    bundle.schema_version,
                    &bundle.project_id,
                    &bundle.display_name,
                    bundle.updated_at_ms,
                    &bundle.snapshot_json,
                    &bundle.artifacts,
                )
                .is_ok_and(|c| c == bundle.checksum_blake3);
                entries.push(serde_json::json!({
                    "id": bundle.project_id,
                    "displayName": bundle.display_name,
                    "updatedAtMs": bundle.updated_at_ms,
                    "validChecksum": valid,
                }));
            }
        }
        entries.sort_by(|a, b| {
            let left = a["updatedAtMs"].as_u64().unwrap_or(0);
            let right = b["updatedAtMs"].as_u64().unwrap_or(0);
            right.cmp(&left)
        });
        serde_json::to_string(&entries).map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Imports a CSV or Excel file from disk and replaces any project with the same id.
    ///
    /// # Errors
    /// Returns the `SF-*` code of the first failing step: unauthenticated (`SF-AUTH-001`),
    /// unsupported extension (`SF-IMP-002`), file too large (`SF-IMP-003`), or reader error.
    pub fn open_file(&self, project: &str, path: &Path, now_ms: u64) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(("project", project), ("path", path));
        self.gate_command("studio_open_file", payload_len, Some(now_ms))?;
        let table = read_table(path)?;
        let source = SourceId::new(project).map_err(|error| code_of(&error))?;
        let graph = DataGraph::new(source, table, now_ms).map_err(|error| code_of(&error))?;
        let mut projects = self
            .projects
            .lock()
            .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
        let _ = projects.insert(project.to_owned(), graph);
        drop(projects);
        self.state_unlocked(project)
    }

    /// Returns the state snapshot (metadata, columns, tokens, scenarios — never unbounded rows).
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`] or [`UNKNOWN_PROJECT_CODE`] when the project is not open.
    pub fn state(&self, project: &str) -> Result<String, String> {
        self.gate_command(
            "studio_state",
            crate::ipc_payload_len!(("project", project)),
            None,
        )?;
        self.state_unlocked(project)
    }

    /// Adds a new editable source column while preserving row and column provenance.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], or `SF-SYS-001`.
    pub fn add_source_column(
        &self,
        project: &str,
        name: &str,
        default_value: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(
            ("project", project),
            ("name", name),
            ("defaultValue", default_value),
        );
        self.gate_command("studio_add_source_column", payload_len, Some(now_ms))?;
        self.mutate(project, |graph| {
            graph
                .add_source_column(name, default_value, now_ms)
                .map(|(_, outcome)| outcome)
        })
    }

    /// Renames a working column cleanly while preserving its stable `ColumnId`.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], `SF-CAL-001`, or `SF-SYS-001`.
    pub fn rename_column(
        &self,
        project: &str,
        old_name: &str,
        new_name: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(
            ("project", project),
            ("oldName", old_name),
            ("newName", new_name),
        );
        self.gate_command("studio_rename_column", payload_len, Some(now_ms))?;
        self.mutate(project, |graph| {
            graph.rename_column(old_name, new_name, now_ms)
        })
    }

    /// Deletes a working column (refuses deleting the final remaining column).
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], `SF-CAL-001`, or `SF-CAL-002`.
    pub fn delete_column(&self, project: &str, name: &str, now_ms: u64) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(("project", project), ("name", name));
        self.gate_command("studio_delete_column", payload_len, Some(now_ms))?;
        self.mutate(project, |graph| graph.delete_column(name, now_ms))
    }

    /// Trims leading/trailing whitespace and collapses spaces across `column` atomically.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], or `SF-CAL-001`.
    pub fn trim_column(&self, project: &str, column: &str, now_ms: u64) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(("project", project), ("column", column));
        self.gate_command("studio_trim_column", payload_len, Some(now_ms))?;
        self.mutate(project, |graph| graph.trim_column(column, now_ms))
    }

    /// Updates the in-memory Quad-Studio creative artifacts (`charts`, `slides`, `docBlocks`,
    /// `freeformBlocks`, `assets`, `templateConfig`) for an open project.
    ///
    /// Validates chart types, aggregation functions, and Free-Form grid bounds (`col` `1..=12`,
    /// `row` `1..=32`, `colSpan` `1..=12`, `rowSpan` `1..=8`, `zIndex` `1..=50`).
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], or `SF-CAL-002`.
    pub fn set_artifacts(&self, project: &str, artifacts_json: &str) -> Result<String, String> {
        let payload_len =
            crate::ipc_payload_len!(("project", project), ("artifacts", artifacts_json));
        self.gate_command("studio_set_artifacts", payload_len, None)?;
        let parsed: StudioWorkspaceArtifacts =
            serde_json::from_str(artifacts_json).map_err(|_| "SF-CAL-002".to_owned())?;
        validate_workspace_artifacts(&parsed)?;
        {
            let projects = self
                .projects
                .lock()
                .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
            if !projects.contains_key(project) {
                return Err(UNKNOWN_PROJECT_CODE.to_owned());
            }
        }
        {
            let mut artifacts = self
                .artifacts
                .lock()
                .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
            let _ = artifacts.insert(project.to_owned(), parsed);
        }
        self.state_unlocked(project)
    }

    /// Saves the active project (origin, transforms, metrics, scenarios, and Quad-Studio artifacts)
    /// atomically to local storage with a `BLAKE3` tamper-detection checksum.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], `SF-SYS-001`, or [`EXPORT_FAILED_CODE`].
    pub fn save_project_bundle(
        &self,
        project: &str,
        saved_id: &str,
        display_name: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(
            ("project", project),
            ("savedId", saved_id),
            ("displayName", display_name),
        );
        self.gate_command("studio_save_project", payload_len, Some(now_ms))?;
        let safe_id = validate_project_id(saved_id)?;
        let snapshot_json = self.snapshot_unlocked(project)?;
        let artifacts = self.current_artifacts(project);
        let dir = self.resolve_storage_dir()?;
        let checksum_blake3 = bundle_checksum(
            1,
            safe_id,
            display_name.trim(),
            now_ms,
            &snapshot_json,
            &artifacts,
        )?;
        let bundle = StudioProjectBundle {
            schema_version: 1,
            project_id: safe_id.to_owned(),
            display_name: display_name.trim().to_owned(),
            updated_at_ms: now_ms,
            snapshot_json,
            artifacts,
            checksum_blake3,
        };
        let encoded = serde_json::to_string(&bundle).map_err(|_| "SF-SYS-001".to_owned())?;
        let target = dir.join(format!("{safe_id}.safi.json"));
        write_atomic(&target, &encoded)?;
        serde_json::to_string(&serde_json::json!({
            "id": bundle.project_id,
            "displayName": bundle.display_name,
            "updatedAtMs": bundle.updated_at_ms,
        }))
        .map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Lists saved local Studio project bundles without trusting corrupt payloads.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], `SF-SYS-001`, or [`EXPORT_FAILED_CODE`].
    pub fn list_project_bundles(&self) -> Result<String, String> {
        self.gate_command("studio_list_projects", crate::ipc_payload_len!(), None)?;
        self.list_project_bundles_unlocked()
    }

    /// Loads a saved `.safi.json` project bundle, checks `BLAKE3` integrity (`SF-CAL-008`),
    /// restores both `DataGraph` and creative artifacts into `project`, and returns its state.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], `SF-SYS-001` when missing/unreadable, or `SF-CAL-008` on checksum mismatch.
    pub fn load_project_bundle(
        &self,
        project: &str,
        saved_id: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(("project", project), ("savedId", saved_id));
        self.gate_command("studio_load_project", payload_len, Some(now_ms))?;
        let safe_id = validate_project_id(saved_id)?;
        let dir = self.resolve_storage_dir()?;
        let path = dir.join(format!("{safe_id}.safi.json"));
        let raw = fs::read_to_string(&path).map_err(|_| "SF-SYS-001".to_owned())?;
        let bundle: StudioProjectBundle =
            serde_json::from_str(&raw).map_err(|_| "SF-SYS-001".to_owned())?;
        let expected = bundle_checksum(
            bundle.schema_version,
            &bundle.project_id,
            &bundle.display_name,
            bundle.updated_at_ms,
            &bundle.snapshot_json,
            &bundle.artifacts,
        )?;
        if expected != bundle.checksum_blake3 {
            return Err("SF-CAL-008".to_owned());
        }
        validate_workspace_artifacts(&bundle.artifacts)?;
        let graph =
            DataGraph::from_json(&bundle.snapshot_json, now_ms).map_err(|error| code_of(&error))?;
        {
            let mut projects = self
                .projects
                .lock()
                .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
            let _ = projects.insert(project.to_owned(), graph);
        }
        {
            let mut artifacts = self
                .artifacts
                .lock()
                .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
            let _ = artifacts.insert(project.to_owned(), bundle.artifacts);
        }
        self.state_unlocked(project)
    }

    /// Deletes a saved project bundle from local storage.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], `SF-SYS-001`, or [`EXPORT_FAILED_CODE`].
    pub fn delete_project_bundle(&self, saved_id: &str) -> Result<String, String> {
        self.gate_command(
            "studio_delete_project",
            crate::ipc_payload_len!(("savedId", saved_id)),
            None,
        )?;
        let safe_id = validate_project_id(saved_id)?;
        let dir = self.resolve_storage_dir()?;
        let path = dir.join(format!("{safe_id}.safi.json"));
        if path.exists() {
            fs::remove_file(&path).map_err(|_| EXPORT_FAILED_CODE.to_owned())?;
        }
        self.list_project_bundles_unlocked()
    }

    /// Lists all 5 built-in Living Templates cleanly for the desktop gallery.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`] or `SF-SYS-001` on serialization error.
    pub fn list_templates(&self) -> Result<String, String> {
        self.gate_command("studio_list_templates", crate::ipc_payload_len!(), None)?;
        let list = TEMPLATES
            .iter()
            .map(|tpl| {
                let sections = tpl
                    .sections
                    .iter()
                    .map(|sec| {
                        serde_json::json!({
                            "headingKey": sec.heading_key,
                            "role": sec.role,
                        })
                    })
                    .collect::<Vec<_>>();
                serde_json::json!({
                    "id": tpl.id,
                    "nameKey": tpl.name_key,
                    "sections": sections,
                })
            })
            .collect::<Vec<_>>();
        serde_json::to_string(&list).map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Binds one of the 5 Living Templates to real verified metrics (`SF-CAL-006` if missing).
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], `SF-CAL-002`, or `SF-CAL-006`.
    pub fn instantiate_template(
        &self,
        project: &str,
        template_id: &str,
        bindings_json: &str,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(
            ("project", project),
            ("templateId", template_id),
            ("bindings", bindings_json),
        );
        self.gate_command("studio_instantiate_template", payload_len, None)?;
        let tpl = template_by_id(template_id).ok_or_else(|| "SF-SYS-001".to_owned())?;
        let raw_bindings: BTreeMap<String, String> =
            serde_json::from_str(bindings_json).map_err(|_| "SF-CAL-002".to_owned())?;
        let mut typed = BTreeMap::new();
        for (role, metric_id) in raw_bindings {
            let parsed_id = MetricId::new(&metric_id).map_err(|error| code_of(&error))?;
            let _ = typed.insert(role, parsed_id);
        }
        let projects = self
            .projects
            .lock()
            .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
        let graph = projects
            .get(project)
            .ok_or_else(|| UNKNOWN_PROJECT_CODE.to_owned())?;
        let instantiated = instantiate_tpl(tpl, graph, &typed).map_err(|error| code_of(&error))?;
        let payload = instantiated
            .into_iter()
            .map(|sec| {
                serde_json::json!({
                    "headingKey": sec.heading_key,
                    "role": sec.role,
                    "token": token_json(&sec.token),
                })
            })
            .collect::<Vec<_>>();
        serde_json::to_string(&payload).map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Queries a bounded viewport page of working rows without transferring the full table.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], `SF-SYS-001` for malformed query JSON, or `SF-CAL-001` for unknown columns.
    pub fn rows_page(&self, project: &str, query_json: &str) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(("project", project), ("query", query_json));
        self.gate_command("studio_rows_page", payload_len, None)?;
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
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], or the engine's own `SF-CAL-*` code.
    pub fn set_cell(
        &self,
        project: &str,
        row: usize,
        column: &str,
        raw: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(
            ("project", project),
            ("row", row),
            ("column", column),
            ("value", raw),
        );
        self.gate_command("studio_set_cell", payload_len, Some(now_ms))?;
        self.mutate(project, |graph| graph.set_cell(row, column, raw, now_ms))
    }

    /// Replaces one cell using its stable [`RowId`] and optional optimistic revision guard.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], `SF-CAL-007` on stale `expected_revision`, or `SF-CAL-*`.
    pub fn set_cell_by_id(
        &self,
        project: &str,
        row_id: u64,
        column: &str,
        raw: &str,
        expected_revision: Option<u64>,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(
            ("project", project),
            ("rowId", row_id),
            ("column", column),
            ("value", raw),
            ("expectedRevision", expected_revision),
        );
        self.gate_command("studio_set_cell_by_id", payload_len, Some(now_ms))?;
        self.mutate_at_revision(project, expected_revision, |graph| {
            graph.set_cell_by_id(RowId::from_raw(row_id), column, raw, now_ms)
        })
    }

    /// Sorts working rows without altering stable row identities.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], or `SF-CAL-001`.
    pub fn sort_rows(
        &self,
        project: &str,
        column: &str,
        ascending: bool,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(
            ("project", project),
            ("column", column),
            ("ascending", ascending),
        );
        self.gate_command("studio_sort_rows", payload_len, Some(now_ms))?;
        self.mutate(project, |graph| graph.sort_rows(column, ascending, now_ms))
    }

    /// Filters working rows on a column while preserving stable row identities.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], or `SF-CAL-001`.
    pub fn filter_rows(
        &self,
        project: &str,
        column: &str,
        query: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len =
            crate::ipc_payload_len!(("project", project), ("column", column), ("query", query),);
        self.gate_command("studio_filter_rows", payload_len, Some(now_ms))?;
        self.mutate(project, |graph| graph.filter_rows(column, query, now_ms))
    }

    /// Deletes a working row by stable row identity.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], or `SF-SYS-001`.
    pub fn delete_row(&self, project: &str, row_id: u64, now_ms: u64) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(("project", project), ("rowId", row_id));
        self.gate_command("studio_delete_row", payload_len, Some(now_ms))?;
        self.mutate(project, |graph| {
            graph.delete_row(RowId::from_raw(row_id), now_ms)
        })
    }

    /// Inserts a row with a freshly allocated stable row identity.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], `SF-SYS-001`, or `SF-CAL-001`.
    pub fn insert_row(
        &self,
        project: &str,
        after_row_id: Option<u64>,
        values_json: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(
            ("project", project),
            ("afterRowId", after_row_id),
            ("values", values_json),
        );
        self.gate_command("studio_insert_row", payload_len, Some(now_ms))?;
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
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], or the engine's own `SF-CAL-*` code.
    pub fn add_calculated_column(
        &self,
        project: &str,
        name: &str,
        formula: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len =
            crate::ipc_payload_len!(("project", project), ("name", name), ("formula", formula),);
        self.gate_command("studio_add_calculated_column", payload_len, Some(now_ms))?;
        self.mutate(project, |graph| {
            graph.add_calculated_column(name, formula, now_ms)
        })
    }

    /// Registers a baseline What-If parameter assumption.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], or `SF-SYS-001`.
    pub fn define_parameter(
        &self,
        project: &str,
        name: &str,
        value_str: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len =
            crate::ipc_payload_len!(("project", project), ("name", name), ("value", value_str),);
        self.gate_command("studio_define_parameter", payload_len, Some(now_ms))?;
        let value = Decimal::from_str(value_str.trim()).map_err(|_| "SF-CAL-002".to_owned())?;
        self.mutate_unit(project, None, |graph| {
            graph.define_parameter(name, value, now_ms)
        })
    }

    /// Updates a baseline What-If assumption value and recomputes dependent metrics.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], `SF-CAL-002`, or `SF-CAL-005`.
    pub fn update_parameter(
        &self,
        project: &str,
        name: &str,
        value_str: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len =
            crate::ipc_payload_len!(("project", project), ("name", name), ("value", value_str),);
        self.gate_command("studio_update_parameter", payload_len, Some(now_ms))?;
        let value = Decimal::from_str(value_str.trim()).map_err(|_| "SF-CAL-002".to_owned())?;
        self.mutate(project, |graph| graph.update_parameter(name, value, now_ms))
    }

    /// Creates a What-If scenario with optional parameter overrides (`{name: "decimal"}`).
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], `SF-SYS-001`, or `SF-CAL-002`.
    pub fn add_scenario(
        &self,
        project: &str,
        id: &str,
        name: &str,
        params_json: &str,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(
            ("project", project),
            ("id", id),
            ("name", name),
            ("parameters", params_json),
        );
        self.gate_command("studio_add_scenario", payload_len, None)?;
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
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], or `SF-SYS-001`.
    pub fn rename_scenario(
        &self,
        project: &str,
        id: &str,
        new_name: &str,
    ) -> Result<String, String> {
        let payload_len =
            crate::ipc_payload_len!(("project", project), ("id", id), ("newName", new_name),);
        self.gate_command("studio_rename_scenario", payload_len, None)?;
        let scenario_id = ScenarioId::new(id).map_err(|error| code_of(&error))?;
        self.mutate_unit(project, None, |graph| {
            graph.rename_scenario(&scenario_id, new_name)
        })
    }

    /// Activates a scenario (`Some(id)`) or clears to baseline (`None`) and recomputes metrics.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], or `SF-SYS-001`.
    pub fn activate_scenario(
        &self,
        project: &str,
        id: Option<&str>,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(("project", project), ("id", id));
        self.gate_command("studio_activate_scenario", payload_len, Some(now_ms))?;
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
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], or `SF-SYS-001`.
    pub fn reset_scenario(&self, project: &str, now_ms: u64) -> Result<String, String> {
        self.gate_command(
            "studio_reset_scenario",
            crate::ipc_payload_len!(("project", project)),
            Some(now_ms),
        )?;
        self.mutate_unit(project, None, |graph| graph.reset_scenario(now_ms))
    }

    /// Sets a numeric What-If parameter override inside a scenario and recomputes active metrics.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], `SF-CAL-002`, `SF-CAL-005`, or `SF-SYS-001`.
    pub fn set_scenario_parameter(
        &self,
        project: &str,
        scenario_id: &str,
        parameter: &str,
        value_str: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(
            ("project", project),
            ("scenarioId", scenario_id),
            ("parameter", parameter),
            ("value", value_str),
        );
        self.gate_command("studio_set_scenario_parameter", payload_len, Some(now_ms))?;
        let id = ScenarioId::new(scenario_id).map_err(|error| code_of(&error))?;
        let value = Decimal::from_str(value_str.trim()).map_err(|_| "SF-CAL-002".to_owned())?;
        self.mutate(project, |graph| {
            graph.set_scenario_parameter(&id, parameter, value, now_ms)
        })
    }

    /// Compares two scenarios across all metrics without mutating active project state.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], `SF-SYS-001`, or evaluation codes.
    pub fn compare_scenarios(
        &self,
        project: &str,
        left: &str,
        right: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len =
            crate::ipc_payload_len!(("project", project), ("left", left), ("right", right));
        self.gate_command("studio_compare_scenarios", payload_len, Some(now_ms))?;
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
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], or the engine's own `SF-CAL-*` code.
    pub fn define_metric(
        &self,
        project: &str,
        id: &str,
        name: &str,
        unit: Option<&str>,
        formula: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(
            ("project", project),
            ("id", id),
            ("name", name),
            ("unit", unit),
            ("formula", formula),
        );
        self.gate_command("studio_define_metric", payload_len, Some(now_ms))?;
        let metric = MetricId::new(id).map_err(|error| code_of(&error))?;
        self.mutate(project, |graph| {
            graph.define_metric(metric, name, unit, formula, now_ms)
        })
    }

    /// Executes a validated Studio DSL command.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], or the engine's own `SF-CAL-*` code.
    pub fn apply_command(
        &self,
        project: &str,
        command_json: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(("project", project), ("command", command_json));
        self.gate_command("studio_apply_command", payload_len, Some(now_ms))?;
        let command = parse_command(command_json).map_err(|error| code_of(&error))?;
        self.mutate(project, |graph| apply_command(graph, &command, now_ms))
    }

    /// Applies an explicit, user-confirmed [`AnomalyFix`] batch atomically as one undoable step.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], `SF-CAL-001`, `SF-CAL-002`, `SF-CAL-007`, or `SF-SYS-001`.
    pub fn apply_anomaly_fix(
        &self,
        project: &str,
        fix_json: &str,
        expected_revision: Option<u64>,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(
            ("project", project),
            ("fix", fix_json),
            ("expectedRevision", expected_revision),
        );
        self.gate_command("studio_apply_anomaly_fix", payload_len, Some(now_ms))?;
        let fix: AnomalyFix =
            serde_json::from_str(fix_json).map_err(|_| "SF-CAL-002".to_owned())?;
        self.mutate_at_revision(project, expected_revision, |graph| {
            graph.apply_anomaly_fix(fix, now_ms)
        })
    }

    /// Undoes the last change.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], or the engine's own `SF-CAL-*` code.
    pub fn undo(&self, project: &str, now_ms: u64) -> Result<String, String> {
        self.gate_command(
            "studio_undo",
            crate::ipc_payload_len!(("project", project)),
            Some(now_ms),
        )?;
        self.mutate(project, |graph| graph.undo(now_ms))
    }

    /// Redoes the last undone change.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], or the engine's own `SF-CAL-*` code.
    pub fn redo(&self, project: &str, now_ms: u64) -> Result<String, String> {
        self.gate_command(
            "studio_redo",
            crate::ipc_payload_len!(("project", project)),
            Some(now_ms),
        )?;
        self.mutate(project, |graph| graph.redo(now_ms))
    }

    /// Groups and aggregates in Rust for the dashboard.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], or the engine's own `SF-CAL-*` code.
    pub fn grouped(
        &self,
        project: &str,
        function: &str,
        column: &str,
        group_by: &str,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(
            ("project", project),
            ("function", function),
            ("column", column),
            ("groupBy", group_by),
        );
        self.gate_command("studio_grouped", payload_len, None)?;
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

    /// Computes a full chart series in Rust for any of the 8 supported Dashboard chart types
    /// (`bar`, `line`, `area`, `pie`, `donut`, `scatter`, `histogram`, `kpi`).
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], `SF-CAL-001`, `SF-CAL-002`, or `SF-CAL-004`.
    pub fn chart_data(
        &self,
        project: &str,
        chart_type: &str,
        function: &str,
        column: &str,
        group_by: &str,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(
            ("project", project),
            ("chartType", chart_type),
            ("function", function),
            ("column", column),
            ("groupBy", group_by),
        );
        self.gate_command("studio_chart_data", payload_len, None)?;
        let kind = ChartType::from_name(chart_type).ok_or_else(|| "SF-CAL-002".to_owned())?;
        let agg = AggFunc::from_name(function).ok_or_else(|| "SF-CAL-002".to_owned())?;
        let projects = self
            .projects
            .lock()
            .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
        let graph = projects
            .get(project)
            .ok_or_else(|| UNKNOWN_PROJECT_CODE.to_owned())?;
        let series = graph
            .compute_chart_series(kind, agg, column, group_by)
            .map_err(|error| code_of(&error))?;
        serde_json::to_string(&series).map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Fills `{{metric:...}}` placeholders from verified tokens only.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], or the engine's own `SF-CAL-*` code.
    pub fn narrative(&self, project: &str, template: &str) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(("project", project), ("template", template));
        self.gate_command("studio_narrative", payload_len, None)?;
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
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], or `SF-SYS-001`.
    pub fn snapshot(&self, project: &str) -> Result<String, String> {
        self.gate_command(
            "studio_snapshot",
            crate::ipc_payload_len!(("project", project)),
            None,
        )?;
        self.snapshot_unlocked(project)
    }

    /// Restores a saved project and re-derives every metric before returning it.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], `SF-SYS-001` for an unreadable file, and `SF-CAL-008` when the
    /// stored data no longer matches its own fingerprint.
    pub fn restore(
        &self,
        project: &str,
        snapshot_json: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len =
            crate::ipc_payload_len!(("project", project), ("snapshot", snapshot_json));
        self.gate_command("studio_restore", payload_len, Some(now_ms))?;
        let graph = DataGraph::from_json(snapshot_json, now_ms).map_err(|error| code_of(&error))?;
        let mut projects = self
            .projects
            .lock()
            .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
        let _ = projects.insert(project.to_owned(), graph);
        drop(projects);
        self.state_unlocked(project)
    }

    /// Writes the self-contained `.safi.html` capsule next to the project.
    ///
    /// The export is a new file written atomically; the original data is never modified.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], `SF-CAL-002` for malformed options, or
    /// [`EXPORT_FAILED_CODE`] when the file cannot be written.
    pub fn export_capsule(
        &self,
        project: &str,
        target: &Path,
        options_json: &str,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(
            ("project", project),
            ("target", target),
            ("options", options_json),
        );
        self.gate_command("studio_export_capsule", payload_len, None)?;
        let options: CapsuleInput =
            serde_json::from_str(options_json).map_err(|_| "SF-CAL-002".to_owned())?;
        let stored_artifacts = self.current_artifacts(project);
        let html = {
            let projects = self
                .projects
                .lock()
                .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
            let graph = projects
                .get(project)
                .ok_or_else(|| UNKNOWN_PROJECT_CODE.to_owned())?;
            let chart_inputs = if options.charts.is_empty() {
                stored_artifacts
                    .charts
                    .iter()
                    .map(|c| CapsuleChartInput {
                        title: c.title.clone(),
                        chart_type: c.chart_type.clone(),
                        function: c.function.clone(),
                        column: c.column.clone(),
                        group_by: c.group_by.clone(),
                    })
                    .collect::<Vec<_>>()
            } else {
                options.charts
            };
            let mut charts = Vec::with_capacity(chart_inputs.len());
            for chart in chart_inputs {
                let chart_type = ChartType::from_name(&chart.chart_type)
                    .ok_or_else(|| "SF-CAL-002".to_owned())?;
                let function =
                    AggFunc::from_name(&chart.function).ok_or_else(|| "SF-CAL-002".to_owned())?;
                charts.push(CapsuleChart {
                    title: chart.title,
                    chart_type,
                    function,
                    column: chart.column,
                    group_by: chart.group_by,
                });
            }
            let slides_source = if options.slides.is_empty() {
                stored_artifacts
                    .slides
                    .into_iter()
                    .map(|s| CapsuleSlide {
                        title: s.title,
                        body: s.body,
                        metric_id: s.metric_id,
                    })
                    .collect()
            } else {
                options
                    .slides
                    .into_iter()
                    .map(|s| CapsuleSlide {
                        title: s.title,
                        body: s.body,
                        metric_id: s.metric_id,
                    })
                    .collect()
            };
            let doc_source = if options.doc_blocks.is_empty() {
                stored_artifacts
                    .doc_blocks
                    .into_iter()
                    .map(|b| CapsuleDocBlock {
                        kind: b.kind,
                        title: b.title,
                        content: b.content,
                        authored_by: b.authored_by,
                    })
                    .collect()
            } else {
                options
                    .doc_blocks
                    .into_iter()
                    .map(|b| CapsuleDocBlock {
                        kind: b.kind,
                        title: b.title,
                        content: b.content,
                        authored_by: b.authored_by,
                    })
                    .collect()
            };
            let raw_ff = if options.freeform_blocks.is_empty() {
                stored_artifacts.freeform_blocks
            } else {
                options.freeform_blocks
            };
            let mut ff_source = Vec::with_capacity(raw_ff.len());
            for f in raw_ff {
                validate_freeform_block(&f)?;
                ff_source.push(CapsuleFreeFormBlock {
                    kind: f.kind,
                    title: f.title,
                    body: f.body,
                    metric_id: f.metric_id,
                    asset_id: f.asset_id,
                    col: f.col,
                    row: f.row,
                    col_span: f.col_span,
                    row_span: f.row_span,
                    z_index: f.z_index,
                });
            }
            let assets_source = if options.assets.is_empty() {
                stored_artifacts
                    .assets
                    .into_iter()
                    .map(|a| CapsuleAsset {
                        id: a.id,
                        name: a.name,
                        data_uri: a.data_uri,
                    })
                    .collect()
            } else {
                options
                    .assets
                    .into_iter()
                    .map(|a| CapsuleAsset {
                        id: a.id,
                        name: a.name,
                        data_uri: a.data_uri,
                    })
                    .collect()
            };
            export_capsule(
                graph,
                &CapsuleOptions {
                    title: options.title,
                    created_at: options.created_at,
                    charts,
                    slides: slides_source,
                    doc_blocks: doc_source,
                    freeform_blocks: ff_source,
                    assets: assets_source,
                    template_name: options.template_name.or_else(|| {
                        stored_artifacts
                            .template_config
                            .map(|cfg| cfg.title_override.unwrap_or(cfg.template_id))
                    }),
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
        self.state_unlocked(project)
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
        self.state_unlocked(project)
    }
}

#[derive(serde::Deserialize)]
struct CapsuleInput {
    title: String,
    created_at: String,
    charts: Vec<CapsuleChartInput>,
    #[serde(default)]
    slides: Vec<StudioSlideArtifact>,
    #[serde(default)]
    doc_blocks: Vec<StudioDocBlockArtifact>,
    #[serde(default)]
    freeform_blocks: Vec<StudioFreeFormBlockArtifact>,
    #[serde(default)]
    assets: Vec<StudioAssetArtifact>,
    #[serde(default)]
    template_name: Option<String>,
}

#[derive(serde::Deserialize)]
struct CapsuleChartInput {
    title: String,
    #[serde(default = "default_chart_type", alias = "chartType")]
    chart_type: String,
    function: String,
    column: String,
    #[serde(alias = "groupBy")]
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
fn state_json(graph: &DataGraph, artifacts: &StudioWorkspaceArtifacts) -> Result<String, String> {
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
        "artifacts": artifacts,
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
    use crate::commands::MAX_IPC_BYTES;
    use crate::test_result;

    const CSV: &str = "region,amount\nCairo,100\nGiza,50\nCairo,25\n";
    const CAPSULE: &str = r#"{"title":"Capsule","created_at":"2026-11-10T00:00:00Z","charts":[{"title":"By region","function":"SUM","column":"amount","group_by":"region"}]}"#;
    const VALID_TOKEN: &str =
        "sfs_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const FORGED_TOKEN: &str =
        "sfs_fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
    const SECOND_DEVICE_TOKEN: &str =
        "sfs_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const INSTALLATION_ID: &str = "11111111-2222-4333-8444-555555555555";
    const OTHER_INSTALLATION_ID: &str = "99999999-8888-4777-8666-555555555555";

    fn authenticated_store() -> StudioStore {
        let store = StudioStore::new();
        store.set_clock_ms(Some(1_000));
        test_result(store.session_authority().register_session(
            "sess_1",
            "ws_1",
            VALID_TOKEN,
            INSTALLATION_ID,
            1_000_000,
        ));
        test_result(store.set_auth_session(VALID_TOKEN, INSTALLATION_ID, 1_000));
        store
    }

    fn authenticated_store_with_dir(dir: PathBuf) -> StudioStore {
        let store = StudioStore::with_storage_dir(dir);
        store.set_clock_ms(Some(1_000));
        test_result(store.session_authority().register_session(
            "sess_1",
            "ws_1",
            VALID_TOKEN,
            INSTALLATION_ID,
            1_000_000,
        ));
        test_result(store.set_auth_session(VALID_TOKEN, INSTALLATION_ID, 1_000));
        store
    }

    fn open_csv(dir: &Path) -> (StudioStore, PathBuf) {
        let path = dir.join("input.csv");
        test_result(fs::write(&path, CSV));
        let store = authenticated_store();
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
        let restored = authenticated_store();
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
        let store = authenticated_store();
        assert_eq!(store.state("missing"), Err(UNKNOWN_PROJECT_CODE.to_owned()));
    }

    #[test]
    fn unsupported_extensions_are_rejected_before_reading() {
        let directory = test_result(tempfile::tempdir());
        let path = directory.path().join("notes.txt");
        test_result(fs::write(&path, "not a spreadsheet"));
        let store = authenticated_store();
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
        let store = authenticated_store();
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
        let store = authenticated_store();
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

    #[test]
    fn multi_project_persistence_templates_column_ops_and_full_capsule_work() {
        let directory = test_result(tempfile::tempdir());
        let storage = directory.path().join("projects-storage");
        let store = authenticated_store_with_dir(storage.clone());
        let csv_path = directory.path().join("sales.csv");
        test_result(fs::write(
            &csv_path,
            "region,amount\n  Cairo  ,100\nGiza,200\n",
        ));
        test_result(store.open_file("p1", &csv_path, 1_000));
        test_result(store.trim_column("p1", "region", 1_100));
        test_result(store.add_source_column("p1", "notes", "ok", 1_200));
        test_result(store.rename_column("p1", "notes", "remarks", 1_300));
        test_result(store.define_metric(
            "p1",
            "total_rev",
            "Total Revenue",
            Some("EGP"),
            "SUM(amount)",
            1_400,
        ));

        // Templates listing & strict instantiation
        let tpls = json(&test_result(store.list_templates()));
        assert_eq!(tpls.as_array().map(Vec::len), Some(5));
        assert_eq!(
            store.instantiate_template("p1", "boardroom-keynote", "{}"),
            Err("SF-CAL-006".to_owned())
        );
        let inst = json(&test_result(store.instantiate_template(
            "p1",
            "boardroom-keynote",
            r#"{"headline":"total_rev","primary_metric":"total_rev","secondary_metric":"total_rev","growth":"total_rev","risk":"total_rev"}"#,
        )));
        assert_eq!(inst.as_array().map(Vec::len), Some(5));

        // Creative artifacts + persistent save/list/load/tamper check
        let artifacts_json = r#"{
            "charts":[{"id":"c1","title":"By Region","chartType":"donut","function":"SUM","column":"amount","groupBy":"region"}],
            "slides":[{"id":"s1","title":"Overview","body":"Revenue {{metric:total_rev}}","metricId":"total_rev","layout":"kpi_split"}],
            "docBlocks":[{"id":"d1","kind":"callout","title":"Decision","content":"Confirmed at {{metric:total_rev}}","authoredBy":"user"}],
            "freeformBlocks":[{"id":"f1","kind":"kpi","title":"KPI","body":"{{metric:total_rev}}","metricId":"total_rev","col":1,"row":1,"colSpan":6,"rowSpan":2,"zIndex":1}],
            "assets":[],
            "templateConfig":{"templateId":"boardroom-keynote","titleOverride":"Q4 Board Deck","bindings":{"headline":"total_rev"},"sectionOverrides":{}}
        }"#;
        let updated = json(&test_result(store.set_artifacts("p1", artifacts_json)));
        assert_eq!(
            updated["artifacts"]["slides"].as_array().map(Vec::len),
            Some(1)
        );
        assert_eq!(
            updated["artifacts"]["charts"][0]["chartType"],
            serde_json::json!("donut")
        );

        test_result(store.save_project_bundle("p1", "q4_board", "Q4 Board", 2_000));
        let listed = json(&test_result(store.list_project_bundles()));
        assert_eq!(listed.as_array().map(Vec::len), Some(1));
        assert_eq!(listed[0]["validChecksum"], serde_json::json!(true));

        let reloaded = json(&test_result(
            store.load_project_bundle("p2", "q4_board", 3_000),
        ));
        assert_eq!(reloaded["metrics"][0]["value"], serde_json::json!("300"));
        assert_eq!(
            reloaded["artifacts"]["docBlocks"].as_array().map(Vec::len),
            Some(1)
        );
        assert_eq!(
            reloaded["artifacts"]["charts"][0]["chartType"],
            serde_json::json!("donut")
        );

        // Full upgraded capsule uses stored artifacts + strict CSP without unsafe-inline
        let out_capsule = directory.path().join("full.safi.html");
        test_result(store.export_capsule(
            "p2",
            &out_capsule,
            r#"{"title":"Full Q4","created_at":"2026-11-15T00:00:00Z","charts":[]}"#,
        ));
        let html = test_result(fs::read_to_string(&out_capsule));
        assert!(html.contains("Q4 Board Deck"));
        assert!(html.contains("Confirmed at 300"));
        assert!(html.contains(r#"data-chart-type="donut""#));
        assert!(!html.contains("unsafe-inline"));

        // Tamper bundle on disk -> SF-CAL-008 checksum rejection
        let saved_path = storage.join("q4_board.safi.json");
        let corrupted =
            test_result(fs::read_to_string(&saved_path)).replace("Q4 Board", "Tampered");
        test_result(fs::write(&saved_path, corrupted));
        assert_eq!(
            store.load_project_bundle("p3", "q4_board", 4_000),
            Err("SF-CAL-008".to_owned())
        );

        // Delete column & delete bundle
        test_result(store.delete_column("p1", "remarks", 5_000));
        test_result(store.delete_project_bundle("q4_board"));
        let empty_list = json(&test_result(store.list_project_bundles()));
        assert_eq!(empty_list.as_array().map(Vec::len), Some(0));
    }

    fn assert_all_sensitive_commands_fail_with(
        store: &StudioStore,
        csv_path: &Path,
        capsule_path: &Path,
        expected_code: &str,
    ) {
        let checks: Vec<(&str, Result<String, String>)> = vec![
            (
                "pick_input_file",
                store
                    .gate_command("pick_input_file", 0, None)
                    .map(|_| "ok".to_owned()),
            ),
            (
                "pick_output_file",
                store
                    .gate_command("pick_output_file", 0, None)
                    .map(|_| "ok".to_owned()),
            ),
            (
                "pick_capsule_output_file",
                store
                    .gate_command("pick_capsule_output_file", 0, None)
                    .map(|_| "ok".to_owned()),
            ),
            ("studio_open_file", store.open_file("p1", csv_path, 1_000)),
            ("studio_state", store.state("p1")),
            ("studio_rows_page", store.rows_page("p1", "{}")),
            (
                "studio_set_cell",
                store.set_cell("p1", 0, "amount", "10", 1_000),
            ),
            (
                "studio_set_cell_by_id",
                store.set_cell_by_id("p1", 1, "amount", "10", None, 1_000),
            ),
            (
                "studio_sort_rows",
                store.sort_rows("p1", "amount", true, 1_000),
            ),
            (
                "studio_filter_rows",
                store.filter_rows("p1", "region", "Cairo", 1_000),
            ),
            ("studio_delete_row", store.delete_row("p1", 1, 1_000)),
            (
                "studio_insert_row",
                store.insert_row("p1", None, "{}", 1_000),
            ),
            (
                "studio_define_metric",
                store.define_metric("p1", "m1", "Metric", None, "SUM(amount)", 1_000),
            ),
            (
                "studio_add_calculated_column",
                store.add_calculated_column("p1", "double", "amount * 2", 1_000),
            ),
            (
                "studio_define_parameter",
                store.define_parameter("p1", "tax", "1.1", 1_000),
            ),
            (
                "studio_update_parameter",
                store.update_parameter("p1", "tax", "1.2", 1_000),
            ),
            (
                "studio_add_scenario",
                store.add_scenario("p1", "s1", "Base", "{}"),
            ),
            (
                "studio_rename_scenario",
                store.rename_scenario("p1", "s1", "Renamed"),
            ),
            (
                "studio_activate_scenario",
                store.activate_scenario("p1", Some("s1"), 1_000),
            ),
            ("studio_reset_scenario", store.reset_scenario("p1", 1_000)),
            (
                "studio_set_scenario_parameter",
                store.set_scenario_parameter("p1", "s1", "tax", "1.5", 1_000),
            ),
            (
                "studio_compare_scenarios",
                store.compare_scenarios("p1", "s1", "s1", 1_000),
            ),
            (
                "studio_apply_command",
                store.apply_command("p1", r#"{"op":"undo"}"#, 1_000),
            ),
            (
                "studio_apply_anomaly_fix",
                store.apply_anomaly_fix(
                    "p1",
                    r#"{"action":"drop_rows","row_ids":[1]}"#,
                    None,
                    1_000,
                ),
            ),
            ("studio_undo", store.undo("p1", 1_000)),
            ("studio_redo", store.redo("p1", 1_000)),
            (
                "studio_grouped",
                store.grouped("p1", "SUM", "amount", "region"),
            ),
            (
                "studio_chart_data",
                store.chart_data("p1", "bar", "SUM", "amount", "region"),
            ),
            ("studio_narrative", store.narrative("p1", "Hello")),
            ("studio_snapshot", store.snapshot("p1")),
            ("studio_restore", store.restore("p1", "{}", 1_000)),
            (
                "studio_add_source_column",
                store.add_source_column("p1", "col", "v", 1_000),
            ),
            (
                "studio_rename_column",
                store.rename_column("p1", "region", "city", 1_000),
            ),
            (
                "studio_delete_column",
                store.delete_column("p1", "region", 1_000),
            ),
            (
                "studio_trim_column",
                store.trim_column("p1", "region", 1_000),
            ),
            ("studio_set_artifacts", store.set_artifacts("p1", "{}")),
            (
                "studio_save_project",
                store.save_project_bundle("p1", "saved1", "Saved", 1_000),
            ),
            ("studio_list_projects", store.list_project_bundles()),
            (
                "studio_load_project",
                store.load_project_bundle("p1", "saved1", 1_000),
            ),
            (
                "studio_delete_project",
                store.delete_project_bundle("saved1"),
            ),
            ("studio_list_templates", store.list_templates()),
            (
                "studio_instantiate_template",
                store.instantiate_template("p1", "boardroom-keynote", "{}"),
            ),
            (
                "studio_export_capsule",
                store.export_capsule("p1", capsule_path, CAPSULE),
            ),
        ];
        assert_eq!(checks.len(), 43, "all 43 sensitive commands must be tested");
        for (name, outcome) in checks {
            assert_eq!(
                outcome,
                Err(expected_code.to_owned()),
                "command {name} must fail with {expected_code}"
            );
        }
    }

    #[test]
    fn native_auth_gate_enforces_session_device_expiry_revocation_logout_and_ipc_limits() {
        let directory = test_result(tempfile::tempdir());
        let csv_path = directory.path().join("input.csv");
        let capsule_path = directory.path().join("out.safi.html");
        test_result(fs::write(&csv_path, CSV));

        let store = StudioStore::with_storage_dir(directory.path().join("projects"));
        store.set_clock_ms(Some(1_000));

        // 1. Calling any of the 43 sensitive Studio commands without a session -> SF-AUTH-001
        assert_all_sensitive_commands_fail_with(
            &store,
            &csv_path,
            &capsule_path,
            AUTH_REQUIRED_CODE,
        );

        // 2. Malformed token or malformed installation UUID -> SF-AUTH-001
        assert_eq!(
            store.set_auth_session("not-a-sfs-token", INSTALLATION_ID, 1_000),
            Err(AUTH_REQUIRED_CODE.to_owned())
        );
        assert_eq!(
            store.set_auth_session(VALID_TOKEN, "not-a-uuid", 1_000),
            Err(AUTH_REQUIRED_CODE.to_owned())
        );

        // 3. Forged/unregistered token sent from React without server authority -> SF-AUTH-001
        assert_eq!(
            store.set_auth_session(FORGED_TOKEN, INSTALLATION_ID, 1_000),
            Err(AUTH_REQUIRED_CODE.to_owned())
        );
        assert_all_sensitive_commands_fail_with(
            &store,
            &csv_path,
            &capsule_path,
            AUTH_REQUIRED_CODE,
        );

        // 4. Register valid session in authority and authenticate
        test_result(store.session_authority().register_session(
            "sess_valid",
            "ws_alpha",
            VALID_TOKEN,
            INSTALLATION_ID,
            10_000,
        ));
        // Wrong device installation ID must be rejected with SF-AUTH-003 (AUTH_REVOKED_CODE)
        assert_eq!(
            store.set_auth_session(VALID_TOKEN, OTHER_INSTALLATION_ID, 1_000),
            Err(AUTH_REVOKED_CODE.to_owned())
        );
        // Correct token + device succeeds
        let auth_json = json(&test_result(store.set_auth_session(
            VALID_TOKEN,
            INSTALLATION_ID,
            1_000,
        )));
        assert_eq!(auth_json["authenticated"], serde_json::json!(true));
        let debug_repr = format!("{store:?}");
        assert!(!debug_repr.contains(VALID_TOKEN));
        assert!(!debug_repr.contains(INSTALLATION_ID));
        assert!(debug_repr.contains("[REDACTED]"));

        // Authorized commands now succeed
        test_result(store.open_file("p1", &csv_path, 1_500));
        test_result(store.define_metric("p1", "m1", "Total", None, "SUM(amount)", 1_600));

        // 5. Exact MAX_IPC_BYTES (1_048_576) serialized JSON succeeds, while MAX_IPC_BYTES + 1
        // (1_048_577) fails with SF-SYS-007 without modifying dataset state or revision.
        let narrative_overhead = test_result(serde_json::to_vec(&serde_json::json!({
            "project": "p1",
            "template": ""
        })))
        .len();
        let exact_narrative = "x".repeat(MAX_IPC_BYTES - narrative_overhead);
        test_result(store.narrative("p1", &exact_narrative));
        let oversized = "x".repeat(MAX_IPC_BYTES - narrative_overhead + 1);
        assert_eq!(
            store.narrative("p1", &oversized),
            Err("SF-SYS-007".to_owned())
        );

        let rev_before = state(&store)["revision"].as_u64();
        assert!(rev_before.is_some());
        // Verify JSON-serialized accounting across non-string parameters (row: usize, rowId: u64,
        // expectedRevision: Option<u64>, ascending: bool), quotes/control-char escapes, and Unicode:
        let set_cell_overhead = test_result(serde_json::to_vec(&serde_json::json!({
            "project": "p1",
            "row": 0usize,
            "column": "amount",
            "value": ""
        })))
        .len();
        let set_cell_filler = "1".repeat(MAX_IPC_BYTES - set_cell_overhead);
        assert_ne!(
            store.set_cell("p1", 0, "amount", &set_cell_filler, 1_650),
            Err("SF-SYS-007".to_owned())
        );
        let set_cell_over = format!("{set_cell_filler}0");
        assert_eq!(
            store.set_cell("p1", 0, "amount", &set_cell_over, 1_650),
            Err("SF-SYS-007".to_owned())
        );
        // Raw `.len()` smaller than filler, but control char `\u{0000}` expands to `\u0000` (6 bytes in JSON):
        let rev_before_escape_reject = state(&store)["revision"].as_u64();
        assert!(rev_before_escape_reject.is_some());
        let escaped_value = format!(
            "{}\u{0000}",
            "1".repeat(MAX_IPC_BYTES - set_cell_overhead - 5)
        );
        assert!(escaped_value.len() < MAX_IPC_BYTES - set_cell_overhead);
        assert_eq!(
            store.set_cell("p1", 0, "amount", &escaped_value, 1_651),
            Err("SF-SYS-007".to_owned())
        );
        // Must reject before mutating dataset revision or cell value:
        assert_eq!(state(&store)["revision"].as_u64(), rev_before_escape_reject);
        assert_ne!(rev_before, Some(0));

        let set_cell_by_id_overhead_none = test_result(serde_json::to_vec(&serde_json::json!({
            "project": "p1",
            "rowId": 1u64,
            "column": "amount",
            "value": "",
            "expectedRevision": None::<u64>
        })))
        .len();
        let set_cell_by_id_over_none = "1".repeat(MAX_IPC_BYTES - set_cell_by_id_overhead_none + 1);
        assert_eq!(
            store.set_cell_by_id("p1", 1, "amount", &set_cell_by_id_over_none, None, 1_650),
            Err("SF-SYS-007".to_owned())
        );

        // Switching Option::None (`null`, 4 bytes) to Option::Some(10000) (`10000`, 5 bytes) pushes exact payload over:
        let exact_with_none = "1".repeat(MAX_IPC_BYTES - set_cell_by_id_overhead_none);
        assert_eq!(
            store.set_cell_by_id("p1", 1, "amount", &exact_with_none, Some(10_000), 1_650),
            Err("SF-SYS-007".to_owned())
        );
        assert_eq!(state(&store)["revision"].as_u64(), rev_before_escape_reject);

        let sort_overhead_true = test_result(serde_json::to_vec(&serde_json::json!({
            "project": "p1",
            "column": "",
            "ascending": true
        })))
        .len();
        let sort_col_exact = "c".repeat(MAX_IPC_BYTES - sort_overhead_true);
        assert_ne!(
            store.sort_rows("p1", &sort_col_exact, true, 1_650),
            Err("SF-SYS-007".to_owned())
        );
        // Changing `true` (4 bytes) -> `false` (5 bytes) exceeds MAX_IPC_BYTES by 1 byte:
        assert_eq!(
            store.sort_rows("p1", &sort_col_exact, false, 1_650),
            Err("SF-SYS-007".to_owned())
        );

        // 6. Session expiration (clock >= 10_000) -> first call fails with SF-AUTH-002 and clears session fail-closed
        store.set_clock_ms(Some(10_000));
        assert_eq!(store.state("p1"), Err(AUTH_EXPIRED_CODE.to_owned()));
        // Once expired, active_session was cleared fail-closed -> subsequent calls return SF-AUTH-001
        assert_all_sensitive_commands_fail_with(
            &store,
            &csv_path,
            &capsule_path,
            AUTH_REQUIRED_CODE,
        );

        // 7. Re-register and test server-side session revocation -> SF-AUTH-003 then fail-closed SF-AUTH-001
        store.set_clock_ms(Some(2_000));
        test_result(store.session_authority().register_session(
            "sess_2",
            "ws_alpha",
            VALID_TOKEN,
            INSTALLATION_ID,
            50_000,
        ));
        test_result(store.set_auth_session(VALID_TOKEN, INSTALLATION_ID, 2_000));
        test_result(store.state("p1"));
        store.session_authority().revoke_session("sess_2");
        assert_eq!(store.state("p1"), Err(AUTH_REVOKED_CODE.to_owned()));
        assert_all_sensitive_commands_fail_with(
            &store,
            &csv_path,
            &capsule_path,
            AUTH_REQUIRED_CODE,
        );

        // 8. Re-register and test single-device replacement / device revocation -> SF-AUTH-003 then fail-closed SF-AUTH-001
        test_result(store.session_authority().register_session(
            "sess_3",
            "ws_alpha",
            VALID_TOKEN,
            INSTALLATION_ID,
            50_000,
        ));
        test_result(store.set_auth_session(VALID_TOKEN, INSTALLATION_ID, 2_000));
        test_result(store.state("p1"));
        // Another device logs into the same workspace -> first device's session is automatically revoked
        test_result(store.session_authority().register_session(
            "sess_4",
            "ws_alpha",
            SECOND_DEVICE_TOKEN,
            OTHER_INSTALLATION_ID,
            50_000,
        ));
        assert_eq!(store.state("p1"), Err(AUTH_REVOKED_CODE.to_owned()));
        assert_all_sensitive_commands_fail_with(
            &store,
            &csv_path,
            &capsule_path,
            AUTH_REQUIRED_CODE,
        );

        // 9. Explicit sign-out (clear_auth_session) immediately blocks subsequent commands
        test_result(store.set_auth_session(SECOND_DEVICE_TOKEN, OTHER_INSTALLATION_ID, 2_000));
        test_result(store.state("p1"));
        test_result(store.clear_auth_session());
        assert_all_sensitive_commands_fail_with(
            &store,
            &csv_path,
            &capsule_path,
            AUTH_REQUIRED_CODE,
        );
    }

    #[test]
    fn all_eight_chart_types_and_freeform_grid_bounds_are_enforced_and_persisted() {
        let directory = test_result(tempfile::tempdir());
        let storage = directory.path().join("projects");
        let store = authenticated_store_with_dir(storage);
        let csv_path = directory.path().join("metrics.csv");
        test_result(fs::write(
            &csv_path,
            "region,amount,units\nCairo,100,10\nGiza,200,20\nAlex,300,30\nCairo,150,15\n",
        ));
        test_result(store.open_file("p1", &csv_path, 1_000));

        for kind in [
            "bar",
            "line",
            "area",
            "pie",
            "donut",
            "scatter",
            "histogram",
            "kpi",
        ] {
            let group_col = if kind == "scatter" { "units" } else { "region" };
            let raw = test_result(store.chart_data("p1", kind, "SUM", "amount", group_col));
            let parsed = json(&raw);
            assert_eq!(parsed["chartType"], serde_json::json!(kind));
            assert!(
                parsed["points"]
                    .as_array()
                    .is_some_and(|pts| !pts.is_empty()),
                "chart {kind} must return computed points from Rust"
            );
        }

        // Unsuitable column for scatter/histogram must fail with SF-CAL-004
        assert_eq!(
            store.chart_data("p1", "scatter", "SUM", "amount", "region"),
            Err("SF-CAL-004".to_owned())
        );
        assert_eq!(
            store.chart_data("p1", "histogram", "COUNT", "region", "region"),
            Err("SF-CAL-004".to_owned())
        );

        // Out-of-bounds Free-Form rowSpan (0 or 9) or colSpan (13) must fail with SF-CAL-002
        let invalid_row_span = r#"{
            "charts":[],
            "slides":[],
            "docBlocks":[],
            "freeformBlocks":[{"id":"f1","kind":"text","title":"Bad","body":"x","col":1,"row":1,"colSpan":4,"rowSpan":9,"zIndex":1}],
            "assets":[]
        }"#;
        assert_eq!(
            store.set_artifacts("p1", invalid_row_span),
            Err("SF-CAL-002".to_owned())
        );

        // Valid 8 chart types + Free-Form rowSpan=6 persist, reload, and export to capsule
        let valid_artifacts = r#"{
            "charts":[
                {"id":"c1","title":"Bar","chartType":"bar","function":"SUM","column":"amount","groupBy":"region"},
                {"id":"c2","title":"Line","chartType":"line","function":"SUM","column":"amount","groupBy":"region"},
                {"id":"c3","title":"Area","chartType":"area","function":"SUM","column":"amount","groupBy":"region"},
                {"id":"c4","title":"Pie","chartType":"pie","function":"SUM","column":"amount","groupBy":"region"},
                {"id":"c5","title":"Donut","chartType":"donut","function":"SUM","column":"amount","groupBy":"region"},
                {"id":"c6","title":"Scatter","chartType":"scatter","function":"SUM","column":"amount","groupBy":"units"},
                {"id":"c7","title":"Histogram","chartType":"histogram","function":"COUNT","column":"amount","groupBy":"region"},
                {"id":"c8","title":"KPI","chartType":"kpi","function":"SUM","column":"amount","groupBy":"region"}
            ],
            "slides":[],
            "docBlocks":[],
            "freeformBlocks":[{"id":"f1","kind":"text","title":"Tall Card","body":"Height 6","col":2,"row":3,"colSpan":5,"rowSpan":6,"zIndex":4}],
            "assets":[]
        }"#;
        test_result(store.set_artifacts("p1", valid_artifacts));
        test_result(store.save_project_bundle("p1", "eight_charts", "Eight Charts", 2_000));
        let loaded = json(&test_result(store.load_project_bundle(
            "p2",
            "eight_charts",
            3_000,
        )));
        assert_eq!(
            loaded["artifacts"]["charts"].as_array().map(Vec::len),
            Some(8)
        );
        assert_eq!(
            loaded["artifacts"]["freeformBlocks"][0]["rowSpan"],
            serde_json::json!(6)
        );

        let capsule_out = directory.path().join("eight.safi.html");
        test_result(store.export_capsule(
            "p2",
            &capsule_out,
            r#"{"title":"All 8","created_at":"2026-11-15T00:00:00Z","charts":[]}"#,
        ));
        let html = test_result(fs::read_to_string(&capsule_out));
        for kind in [
            "bar",
            "line",
            "area",
            "pie",
            "donut",
            "scatter",
            "histogram",
            "kpi",
        ] {
            assert!(
                html.contains(&format!(r#"data-chart-type="{kind}""#)),
                "exported capsule must contain rendered chart type {kind}"
            );
        }
        assert!(html.contains(r#"data-grid-row-span="6""#));
        assert!(html.contains("grid-row:3 / span 6"));
    }
}
