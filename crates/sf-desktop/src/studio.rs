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

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use sf_core::Decimal;
use sf_core::datagraph::{ColumnId, MetricId, RowId, ScenarioId, SourceId, VerifiedMetricToken};
use sf_core::error::SafiError;
use sf_core::limits::PlatformLimits;
use sf_core::table::Table;
use sf_io::{
    FormulaVerificationStatus, SheetVisibility, WorkbookIngestionReport, ingest_workbook_universal,
};
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

use crate::commands::{CommandAccessClass, classify_command_access, enforce_ipc_command};
use crate::local_search::{LocalSearchIndex, SearchQuery};
use crate::recipe_library::{
    LocalRecipeLibrary, LocalRecipeRecord, NameConflictPolicy, RecipeOriginClass,
};
use crate::vault::{
    VaultKey, decrypt_vault_payload, read_encrypted_or_migrate_legacy, write_encrypted_mandatory,
};
use crate::workflow_memory::{
    LearningCategory, LearningRuleScope, LearningSignalKind, LocalWorkflowMemoryStore,
    SavedCleaningOperation, SavedColumnDecision, SavedDashboardPreferences, SavedMetricDefinition,
    SavedQuestionDecision, SavedRecommendationFeedback, SheetBusinessDomain,
};

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

/// Code returned when an account has been suspended by the Platform Owner.
pub const AUTH_ACCOUNT_SUSPENDED_CODE: &str = "SF-AUTH-009";

/// Code returned when this device was automatically deactivated because another device signed in.
pub const AUTH_DEVICE_REPLACED_CODE: &str = "SF-AUTH-019";

/// Code returned when the cryptographic Ed25519 device proof is invalid.
pub const AUTH_DEVICE_PROOF_INVALID_CODE: &str = "SF-AUTH-020";

/// Code returned when a protected operation is attempted while offline in Read-Only mode.
pub const AUTH_OFFLINE_READ_ONLY_CODE: &str = "SF-AUTH-021";

/// Code returned when the online authorization lease expired or local clock rollback was detected.
pub const AUTH_LEASE_CLOCK_ROLLBACK_CODE: &str = "SF-AUTH-022";

/// Code returned when the account-service origin (`SAFI_API_BASE_URL`) is missing or invalid.
pub const AUTH_CONFIG_MISSING_CODE: &str = "SF-SYS-031";

/// Maximum tolerated backward clock jump (5 seconds) before failing closed with `SF-AUTH-022`.
const MAX_CLOCK_ROLLBACK_TOLERANCE_MS: u64 = 5_000;

const SESSION_TOKEN_PREFIX: &str = "sfs_";
const SESSION_RANDOM_HEX_LEN: usize = 64;

fn default_chart_type() -> String {
    "bar".to_owned()
}

const fn is_false(flag: &bool) -> bool {
    !*flag
}

const fn is_default_privacy(privacy: &StudioPrivacySettingsArtifact) -> bool {
    !privacy.external_ai_consent_granted
        && !privacy.mask_pii_on_cloud
        && privacy.consent_timestamp_ms.is_none()
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
    /// Presenter notes stored for presenter mode and PPTX speaker notes.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub presenter_notes: String,
    /// Visual slide theme id.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub theme: String,
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
    /// Whether to start a new page before this section in PDF/DOCX exports.
    #[serde(default, skip_serializing_if = "is_false")]
    pub page_break_before: bool,
    /// Running header text associated with this document section.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub header_text: String,
    /// Running footer text associated with this document section.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub footer_text: String,
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

/// Resolution state for a contextual clarification question in Desktop Studio.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StudioQuestionResolutionArtifact {
    /// Stable question identifier.
    pub question_id: String,
    /// Resolution action (`answered`, `skipped`, `deferred`, `custom`, `corrected`).
    pub action: String,
    /// Selected option or textual answer.
    pub answer: String,
    /// Optional custom explanation from user.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub custom_text: String,
    /// Epoch timestamp in milliseconds.
    pub updated_at_ms: u64,
}

/// Privacy & optional external AI consent record persisted locally per project.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StudioPrivacySettingsArtifact {
    /// Explicit opt-in consent before sending aggregated schema metadata to external AI providers.
    pub external_ai_consent_granted: bool,
    /// Mask/exclude raw personal identifiers even when consent is granted.
    pub mask_pii_on_cloud: bool,
    /// Epoch timestamp when consent state was last updated.
    pub consent_timestamp_ms: Option<u64>,
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
    /// Recorded contextual clarification question resolutions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub question_resolutions: Vec<StudioQuestionResolutionArtifact>,
    /// Explicit local/cloud privacy consent settings.
    #[serde(default, skip_serializing_if = "is_default_privacy")]
    pub privacy_settings: StudioPrivacySettingsArtifact,
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
/// native session, single-device, clock-rollback, and offline read-only gate.
#[derive(Debug)]
pub struct StudioStore {
    projects: Mutex<BTreeMap<String, DataGraph>>,
    artifacts: Mutex<BTreeMap<String, StudioWorkspaceArtifacts>>,
    ingestion_reports: Mutex<BTreeMap<String, WorkbookIngestionReport>>,
    active_regions: Mutex<BTreeMap<String, (String, String)>>,
    storage_dir: Mutex<Option<PathBuf>>,
    active_session: Mutex<Option<ActiveStudioSession>>,
    has_previously_authenticated: Mutex<bool>,
    offline_read_only: Mutex<bool>,
    last_online_verified_ms: Mutex<Option<u64>>,
    high_water_clock_ms: Mutex<u64>,
    vault_key: Mutex<Option<VaultKey>>,
    vault_key_explicitly_disabled: Mutex<bool>,
    authority: Arc<StudioSessionAuthority>,
    verifier: Arc<dyn StudioSessionVerifier>,
    clock_ms: Mutex<Option<u64>>,
    search_index: Mutex<Option<LocalSearchIndex>>,
    recipe_library: Mutex<Option<LocalRecipeLibrary>>,
    workflow_memory: Mutex<Option<LocalWorkflowMemoryStore>>,
    org_sync: Mutex<Option<Arc<crate::org_sync::OrgSyncStore>>>,
    ollama_probe: Mutex<Option<crate::decision_engine::OllamaRuntimeProbe>>,
}

impl Default for StudioStore {
    fn default() -> Self {
        let authority = Arc::new(StudioSessionAuthority::new());
        let verifier: Arc<dyn StudioSessionVerifier> = authority.clone();
        Self {
            projects: Mutex::new(BTreeMap::new()),
            artifacts: Mutex::new(BTreeMap::new()),
            ingestion_reports: Mutex::new(BTreeMap::new()),
            active_regions: Mutex::new(BTreeMap::new()),
            storage_dir: Mutex::new(None),
            active_session: Mutex::new(None),
            has_previously_authenticated: Mutex::new(false),
            offline_read_only: Mutex::new(false),
            last_online_verified_ms: Mutex::new(None),
            high_water_clock_ms: Mutex::new(0),
            vault_key: Mutex::new(None),
            vault_key_explicitly_disabled: Mutex::new(false),
            authority,
            verifier,
            clock_ms: Mutex::new(None),
            search_index: Mutex::new(None),
            recipe_library: Mutex::new(None),
            workflow_memory: Mutex::new(None),
            org_sync: Mutex::new(None),
            ollama_probe: Mutex::new(None),
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
        let has_prior_auth = dir.join("prior_auth.marker").is_file();
        let init_key = VaultKey::load_or_init_for_storage_dir(&dir).ok();
        Self {
            projects: Mutex::new(BTreeMap::new()),
            artifacts: Mutex::new(BTreeMap::new()),
            ingestion_reports: Mutex::new(BTreeMap::new()),
            active_regions: Mutex::new(BTreeMap::new()),
            storage_dir: Mutex::new(Some(dir)),
            active_session: Mutex::new(None),
            has_previously_authenticated: Mutex::new(has_prior_auth),
            offline_read_only: Mutex::new(false),
            last_online_verified_ms: Mutex::new(None),
            high_water_clock_ms: Mutex::new(0),
            vault_key: Mutex::new(init_key),
            vault_key_explicitly_disabled: Mutex::new(false),
            authority,
            verifier,
            clock_ms: Mutex::new(None),
            search_index: Mutex::new(None),
            recipe_library: Mutex::new(None),
            workflow_memory: Mutex::new(None),
            org_sync: Mutex::new(None),
            ollama_probe: Mutex::new(None),
        }
    }

    /// Creates a store backed by `dir` and a custom [`StudioSessionVerifier`].
    #[must_use]
    pub fn with_storage_dir_and_verifier(
        dir: PathBuf,
        authority: Arc<StudioSessionAuthority>,
        verifier: Arc<dyn StudioSessionVerifier>,
    ) -> Self {
        let has_prior_auth = dir.join("prior_auth.marker").is_file();
        let init_key = VaultKey::load_or_init_for_storage_dir(&dir).ok();
        Self {
            projects: Mutex::new(BTreeMap::new()),
            artifacts: Mutex::new(BTreeMap::new()),
            ingestion_reports: Mutex::new(BTreeMap::new()),
            active_regions: Mutex::new(BTreeMap::new()),
            storage_dir: Mutex::new(Some(dir)),
            active_session: Mutex::new(None),
            has_previously_authenticated: Mutex::new(has_prior_auth),
            offline_read_only: Mutex::new(false),
            last_online_verified_ms: Mutex::new(None),
            high_water_clock_ms: Mutex::new(0),
            vault_key: Mutex::new(init_key),
            vault_key_explicitly_disabled: Mutex::new(false),
            authority,
            verifier,
            clock_ms: Mutex::new(None),
            search_index: Mutex::new(None),
            recipe_library: Mutex::new(None),
            workflow_memory: Mutex::new(None),
            org_sync: Mutex::new(None),
            ollama_probe: Mutex::new(None),
        }
    }

    /// Updates the runtime Ollama loopback probe result captured by the Desktop shell boundary
    /// (outside the zero-network engine crate).
    pub fn set_ollama_runtime_probe(
        &self,
        probe: Option<crate::decision_engine::OllamaRuntimeProbe>,
    ) {
        if let Ok(mut guard) = self.ollama_probe.lock() {
            *guard = probe;
        }
    }

    /// Configures a local Argon2id + ChaCha20-Poly1305 vault key for encrypting
    /// saved `.safi.enc` local project bundles and workflow/learning stores at rest.
    /// Passing `None` explicitly disables the vault key (causing persistence operations
    /// to fail closed with `SF-SYS-024`).
    pub fn set_local_vault_key(&self, key: Option<VaultKey>) {
        if let Ok(mut disabled) = self.vault_key_explicitly_disabled.lock() {
            *disabled = key.is_none();
        }
        if let Ok(mut guard) = self.vault_key.lock() {
            *guard = key;
        }
    }

    /// Ensures the local `VaultKey` is loaded, initialized, or migrated (`SFK1 -> SFK2`)
    /// using the native OS keystore (`Windows Credential Manager`, `macOS Keychain`, or `Linux Secret Service`).
    ///
    /// # Errors
    /// Returns `"SF-SYS-024"` if the OS keystore is unavailable/locked or verification fails.
    pub fn ensure_local_vault_key_with_os_keystore(
        &self,
        os_keystore: &dyn crate::vault::OsKeystoreProvider,
    ) -> Result<VaultKey, String> {
        let explicitly_disabled = self
            .vault_key_explicitly_disabled
            .lock()
            .map(|g| *g)
            .unwrap_or(false);
        if explicitly_disabled {
            return Err("SF-SYS-024".to_string());
        }
        let dir = self.resolve_storage_dir()?;
        let loaded = VaultKey::load_or_init_with_os_keystore(&dir, os_keystore)
            .map_err(|_| "SF-SYS-024".to_string())?;
        if let Ok(mut guard) = self.vault_key.lock() {
            *guard = Some(loaded.clone());
        }
        Ok(loaded)
    }

    /// Ensures the local `VaultKey` is loaded or initialized from the storage keystore,
    /// failing closed with `SF-SYS-024` if the key is missing or explicitly disabled.
    pub fn ensure_local_vault_key(&self) -> Result<VaultKey, String> {
        let explicitly_disabled = self
            .vault_key_explicitly_disabled
            .lock()
            .map(|g| *g)
            .unwrap_or(false);
        if explicitly_disabled {
            return Err("SF-SYS-024".to_string());
        }
        if let Ok(guard) = self.vault_key.lock() {
            if let Some(ref key) = *guard {
                return Ok(key.clone());
            }
        }
        let storage_dir = self
            .storage_dir
            .lock()
            .map_err(|_| "SF-SYS-001".to_string())?
            .clone();
        if let Some(dir) = storage_dir {
            let key =
                VaultKey::load_or_init_for_storage_dir(&dir).map_err(|code| code.to_string())?;
            if let Ok(mut guard) = self.vault_key.lock() {
                *guard = Some(key.clone());
            }
            Ok(key)
        } else {
            // In-memory store without a storage directory derives an ephemeral per-store key
            let salt = [0x53u8; 16];
            let key = VaultKey::derive_from_passphrase("safi-ephemeral-in-memory-vault-v1", &salt)
                .map_err(|code| code.to_string())?;
            if let Ok(mut guard) = self.vault_key.lock() {
                *guard = Some(key.clone());
            }
            Ok(key)
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
        if let Ok(mut ro) = self.offline_read_only.lock() {
            *ro = false;
        }
        if let Ok(mut prior) = self.has_previously_authenticated.lock() {
            *prior = true;
        }
        if let Ok(dir_guard) = self.storage_dir.lock()
            && let Some(dir) = dir_guard.as_ref()
            && fs::create_dir_all(dir).is_ok()
        {
            let _ = fs::write(dir.join("prior_auth.marker"), b"v1\n");
        }
        if let Ok(mut ts) = self.last_online_verified_ms.lock() {
            *ts = Some(effective_now);
        }
        if let Ok(mut hw) = self.high_water_clock_ms.lock() {
            *hw = effective_now;
        }
        serde_json::to_string(&serde_json::json!({
            "authenticated": true,
            "mode": "online_authorized",
            "canExecuteProtectedOps": true,
            "hasPreviouslyAuthenticated": true,
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
        if let Ok(mut ro) = self.offline_read_only.lock() {
            *ro = false;
        }
        if let Ok(mut hw) = self.high_water_clock_ms.lock() {
            *hw = 0;
        }
        let has_prior = self
            .has_previously_authenticated
            .lock()
            .map(|g| *g)
            .unwrap_or(false);
        serde_json::to_string(&serde_json::json!({
            "authenticated": false,
            "mode": "unauthenticated",
            "canExecuteProtectedOps": false,
            "hasPreviouslyAuthenticated": has_prior,
        }))
        .map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Explicitly enters Offline Read-Only Mode so a previously authenticated user can open and
    /// inspect saved local projects, datasets, dashboards, and reports without internet access
    /// while strictly blocking all protected operations (`SF-AUTH-021`).
    ///
    /// Fresh unauthenticated installations MUST NOT bypass online authentication via Offline Mode
    /// and fail closed with [`AUTH_REQUIRED_CODE`] (`SF-AUTH-001`).
    ///
    /// # Errors
    /// Returns [`AUTH_REQUIRED_CODE`] (`SF-AUTH-001`) on a fresh unauthenticated installation,
    /// or `SF-SYS-010` / `SF-SYS-001` on internal failure.
    pub fn enter_offline_read_only(&self) -> Result<String, String> {
        enforce_ipc_command("studio_enter_offline_readonly", crate::ipc_payload_len!())?;
        let has_prior = self
            .has_previously_authenticated
            .lock()
            .map(|g| *g)
            .unwrap_or(false);
        let has_active = self
            .active_session
            .lock()
            .map(|g| g.is_some())
            .unwrap_or(false);
        if !has_prior && !has_active {
            return Err(AUTH_REQUIRED_CODE.to_owned());
        }
        if let Ok(mut ro) = self.offline_read_only.lock() {
            *ro = true;
        }
        self.authorization_status()
    }

    /// Returns the current native authorization state (`online_authorized`, `offline_read_only`,
    /// or `unauthenticated`) for the Desktop UI banner and capability gating.
    ///
    /// # Errors
    /// Returns `SF-SYS-010` or `SF-SYS-001` on internal failure.
    pub fn authorization_status(&self) -> Result<String, String> {
        enforce_ipc_command("studio_authorization_status", crate::ipc_payload_len!())?;
        let read_only = self.offline_read_only.lock().map(|g| *g).unwrap_or(false);
        let has_prior = self
            .has_previously_authenticated
            .lock()
            .map(|g| *g)
            .unwrap_or(false);
        let session = self
            .active_session
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|s| s.verified.clone()));
        let last_verified = self.last_online_verified_ms.lock().ok().and_then(|g| *g);
        let (mode, can_execute) = if read_only {
            ("offline_read_only", false)
        } else if session.is_some() {
            ("online_authorized", true)
        } else {
            ("unauthenticated", false)
        };
        serde_json::to_string(&serde_json::json!({
            "mode": mode,
            "readOnlyOffline": read_only,
            "canExecuteProtectedOps": can_execute,
            "hasPreviouslyAuthenticated": has_prior || session.is_some(),
            "sessionId": session.as_ref().map(|s| &s.session_id),
            "workspaceId": session.as_ref().map(|s| &s.workspace_id),
            "lastOnlineVerifiedMs": last_verified,
        }))
        .map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Revalidates the preserved session against the online server when internet returns,
    /// checking session validity, active sole device, and account status before leaving
    /// Offline Read-Only Mode.
    ///
    /// # Errors
    /// Returns `SF-AUTH-001`, `SF-AUTH-002`, `SF-AUTH-003`, `SF-AUTH-005`, `SF-AUTH-009`,
    /// or `SF-AUTH-019` on validation failure.
    pub fn revalidate_session(&self, now_ms: u64) -> Result<String, String> {
        enforce_ipc_command("studio_revalidate_session", crate::ipc_payload_len!())?;
        let active = {
            let guard = self
                .active_session
                .lock()
                .map_err(|_| AUTH_REQUIRED_CODE.to_owned())?;
            guard.clone().ok_or_else(|| AUTH_REQUIRED_CODE.to_owned())?
        };
        let effective_now = self.effective_now_ms(Some(now_ms));
        match self
            .verifier
            .verify_session(&active.token, &active.installation_id, effective_now)
        {
            Ok(verified) => {
                if let Ok(mut guard) = self.active_session.lock() {
                    *guard = Some(ActiveStudioSession {
                        token: active.token,
                        installation_id: active.installation_id,
                        verified,
                    });
                }
                if let Ok(mut ro) = self.offline_read_only.lock() {
                    *ro = false;
                }
                if let Ok(mut ts) = self.last_online_verified_ms.lock() {
                    *ts = Some(effective_now);
                }
                if let Ok(mut hw) = self.high_water_clock_ms.lock() {
                    *hw = effective_now;
                }
                self.authorization_status()
            }
            Err(code) if code == AUTH_BACKEND_UNAVAILABLE_CODE => {
                // Still offline: stay cleanly in Offline Read-Only Mode without losing credentials.
                if let Ok(mut ro) = self.offline_read_only.lock() {
                    *ro = true;
                }
                Err(code)
            }
            Err(code) => {
                // Device replaced (SF-AUTH-019), revoked (SF-AUTH-003), suspended (SF-AUTH-009),
                // or expired (SF-AUTH-002): wipe active session immediately.
                if let Ok(mut guard) = self.active_session.lock() {
                    *guard = None;
                }
                if let Ok(mut ro) = self.offline_read_only.lock() {
                    *ro = false;
                }
                Err(code)
            }
        }
    }

    /// Enforces the native command allowlist, `MAX_IPC_BYTES` canonical serialized-JSON argument
    /// ceiling, Offline Read-Only vs Protected Online separation, monotonic local clock rollback
    /// protection (`SF-AUTH-022`), and fail-closed session + single-device verification.
    ///
    /// # Errors
    /// Returns `SF-SYS-010`, `SF-SYS-007`, [`AUTH_REQUIRED_CODE`] (`SF-AUTH-001`),
    /// [`AUTH_EXPIRED_CODE`] (`SF-AUTH-002`), [`AUTH_REVOKED_CODE`] (`SF-AUTH-003`),
    /// [`AUTH_BACKEND_UNAVAILABLE_CODE`] (`SF-AUTH-005`), [`AUTH_DEVICE_REPLACED_CODE`] (`SF-AUTH-019`),
    /// [`AUTH_OFFLINE_READ_ONLY_CODE`] (`SF-AUTH-021`), [`AUTH_LEASE_CLOCK_ROLLBACK_CODE`] (`SF-AUTH-022`),
    /// or [`AUTH_CONFIG_MISSING_CODE`] (`SF-SYS-031`).
    pub fn gate_command(
        &self,
        command: &str,
        payload_len: usize,
        caller_now_ms: Option<u64>,
    ) -> Result<VerifiedStudioSession, String> {
        enforce_ipc_command(command, payload_len)?;
        let access_class = classify_command_access(command);
        let is_offline_ro = self.offline_read_only.lock().map(|g| *g).unwrap_or(false);

        if is_offline_ro {
            if access_class == CommandAccessClass::ReadOnlyOffline {
                return Ok(VerifiedStudioSession {
                    session_id: "offline-readonly".to_owned(),
                    workspace_id: "local-offline".to_owned(),
                    token_hash: String::new(),
                    installation_hash: String::new(),
                    expires_at_ms: 0,
                });
            }
            return Err(AUTH_OFFLINE_READ_ONLY_CODE.to_owned());
        }

        let active = {
            let guard = self
                .active_session
                .lock()
                .map_err(|_| AUTH_REQUIRED_CODE.to_owned())?;
            guard.clone().ok_or_else(|| AUTH_REQUIRED_CODE.to_owned())?
        };
        let now_ms = self.effective_now_ms(caller_now_ms);

        // Monotonic local clock rollback guard: prevent extending an online session by setting
        // the OS clock backwards.
        {
            let mut hw = self
                .high_water_clock_ms
                .lock()
                .map_err(|_| AUTH_LEASE_CLOCK_ROLLBACK_CODE.to_owned())?;
            if *hw > 0 && now_ms.saturating_add(MAX_CLOCK_ROLLBACK_TOLERANCE_MS) < *hw {
                return Err(AUTH_LEASE_CLOCK_ROLLBACK_CODE.to_owned());
            }
            if now_ms > *hw {
                *hw = now_ms;
            }
        }

        match self
            .verifier
            .verify_session(&active.token, &active.installation_id, now_ms)
        {
            Ok(verified) => {
                if let Ok(mut ts) = self.last_online_verified_ms.lock() {
                    *ts = Some(now_ms);
                }
                Ok(verified)
            }
            Err(code) if code == AUTH_BACKEND_UNAVAILABLE_CODE => {
                // Network dropped while authenticated: preserve credentials for automatic
                // reconnection revalidation, and fail closed with SF-AUTH-005 until the app
                // explicitly transitions into Offline Read-Only Mode (`enter_offline_read_only`).
                Err(code)
            }
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
        let report = self
            .ingestion_reports
            .lock()
            .ok()
            .and_then(|map| map.get(project).cloned());
        let active_region = self
            .active_regions
            .lock()
            .ok()
            .and_then(|map| map.get(project).cloned());
        let projects = self
            .projects
            .lock()
            .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
        let graph = projects
            .get(project)
            .ok_or_else(|| UNKNOWN_PROJECT_CODE.to_owned())?;
        state_json(graph, &artifacts, report.as_ref(), active_region.as_ref())
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
        let key = self.ensure_local_vault_key()?;
        let mut entries = Vec::new();
        let read_dir = fs::read_dir(&dir).map_err(|_| EXPORT_FAILED_CODE.to_owned())?;
        for item in read_dir {
            let Ok(entry) = item else { continue };
            let path = entry.path();
            let file_name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            // Ignore any temporary crash-recovery files (`.tmp`) and internal keystore/store files.
            if file_name.ends_with(".tmp")
                || file_name.starts_with("workflow_memory.")
                || file_name.starts_with("learning_profile.")
                || file_name.starts_with("recipe_library.")
                || file_name.starts_with("local_search_index.")
            {
                continue;
            }
            let raw_json = if file_name.ends_with(".safi.enc") {
                let Ok(cipher) = fs::read(&path) else {
                    continue;
                };
                let Ok(plain) = decrypt_vault_payload(&key, &cipher) else {
                    continue;
                };
                let Ok(text) = String::from_utf8(plain) else {
                    continue;
                };
                text
            } else if file_name.ends_with(".safi.json") {
                let Ok(raw) = fs::read_to_string(&path) else {
                    continue;
                };
                raw
            } else {
                continue;
            };
            if let Ok(bundle) = serde_json::from_str::<StudioProjectBundle>(&raw_json) {
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
                    "encrypted": file_name.ends_with(".safi.enc"),
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

    /// Imports a CSV, TSV, XLSX, XLSM, XLSB, XLS, or ODS file via the Universal Workbook
    /// Intelligence Engine and replaces any project with the same id.
    ///
    /// # Errors
    /// Returns the `SF-*` code of the first failing step: unauthenticated (`SF-AUTH-001`),
    /// unsupported extension (`SF-IMP-002`), file too large (`SF-IMP-003`), or reader error.
    pub fn open_file(&self, project: &str, path: &Path, now_ms: u64) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(("project", project), ("path", path));
        self.gate_command("studio_open_file", payload_len, Some(now_ms))?;
        let (report, table, sheet_name, table_id) = ingest_workbook_for_desktop(path)?;
        let source = SourceId::new(project).map_err(|error| code_of(&error))?;
        let graph = DataGraph::new(source, table, now_ms).map_err(|error| code_of(&error))?;
        {
            let mut projects = self
                .projects
                .lock()
                .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
            let _ = projects.insert(project.to_owned(), graph);
        }
        if let Ok(mut reports) = self.ingestion_reports.lock() {
            let _ = reports.insert(project.to_owned(), report);
        }
        if let Ok(mut regions) = self.active_regions.lock() {
            let _ = regions.insert(project.to_owned(), (sheet_name, table_id));
        }
        self.state_unlocked(project)
    }

    /// Switches the active worksheet and detected table region for a multi-sheet/multi-table workbook.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], or `SF-IMP-001` when the region is missing.
    pub fn select_region(
        &self,
        project: &str,
        sheet_name: &str,
        table_id: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(
            ("project", project),
            ("sheetName", sheet_name),
            ("tableId", table_id),
        );
        self.gate_command("studio_select_region", payload_len, Some(now_ms))?;
        let table = {
            let reports = self
                .ingestion_reports
                .lock()
                .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
            let report = reports
                .get(project)
                .ok_or_else(|| UNKNOWN_PROJECT_CODE.to_owned())?;
            report
                .core_table_by_sheet_and_table(sheet_name, table_id)
                .map_err(|error| code_of(&error))?
                .ok_or_else(|| "SF-IMP-001".to_owned())?
        };
        let source = SourceId::new(project).map_err(|error| code_of(&error))?;
        let graph = DataGraph::new(source, table, now_ms).map_err(|error| code_of(&error))?;
        {
            let mut projects = self
                .projects
                .lock()
                .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
            let _ = projects.insert(project.to_owned(), graph);
        }
        if let Ok(mut regions) = self.active_regions.lock() {
            let _ = regions.insert(
                project.to_owned(),
                (sheet_name.to_owned(), table_id.to_owned()),
            );
        }
        self.state_unlocked(project)
    }

    /// Returns complete multi-sheet, formula verification, separated-rows, and embedded-object
    /// metadata from the Universal Workbook Intelligence Engine for `project`.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], or `SF-SYS-001`.
    pub fn workbook_details(&self, project: &str) -> Result<String, String> {
        self.gate_command(
            "studio_workbook_details",
            crate::ipc_payload_len!(("project", project)),
            None,
        )?;
        let report = self
            .ingestion_reports
            .lock()
            .ok()
            .and_then(|map| map.get(project).cloned());
        let active_region = self
            .active_regions
            .lock()
            .ok()
            .and_then(|map| map.get(project).cloned());
        let projects = self
            .projects
            .lock()
            .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
        let graph = projects
            .get(project)
            .ok_or_else(|| UNKNOWN_PROJECT_CODE.to_owned())?;
        let payload = workbook_details_json(graph, report.as_ref(), active_region.as_ref());
        serde_json::to_string(&payload).map_err(|_| "SF-SYS-001".to_owned())
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
        let key = self.ensure_local_vault_key()?;
        let enc_target = dir.join(format!("{safe_id}.safi.enc"));
        let legacy_json_target = dir.join(format!("{safe_id}.safi.json"));
        write_encrypted_mandatory(
            &enc_target,
            Some(&legacy_json_target),
            Some(&key),
            encoded.as_bytes(),
        )?;
        serde_json::to_string(&serde_json::json!({
            "id": bundle.project_id,
            "displayName": bundle.display_name,
            "updatedAtMs": bundle.updated_at_ms,
            "encrypted": true,
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

    /// Loads a saved encrypted `.safi.enc` (or migrates a legacy `.safi.json`) project bundle,
    /// checks `BLAKE3` integrity (`SF-CAL-008`), restores both `DataGraph` and creative artifacts
    /// into `project`, and returns its state.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], `SF-SYS-001` when missing/unreadable, `SF-SYS-024` on vault
    /// decryption failure, or `SF-CAL-008` on checksum mismatch.
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
        let key = self.ensure_local_vault_key()?;
        let enc_path = dir.join(format!("{safe_id}.safi.enc"));
        let json_path = dir.join(format!("{safe_id}.safi.json"));
        let raw_bytes = read_encrypted_or_migrate_legacy(&enc_path, Some(&json_path), Some(&key))?
            .ok_or_else(|| "SF-SYS-001".to_owned())?;
        let raw = String::from_utf8(raw_bytes).map_err(|_| "SF-SYS-024".to_owned())?;
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
        let enc_path = dir.join(format!("{safe_id}.safi.enc"));
        if enc_path.exists() {
            fs::remove_file(&enc_path).map_err(|_| EXPORT_FAILED_CODE.to_owned())?;
        }
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

    /// Computes a full chart series in Rust for any of the 21 supported Dashboard chart types.
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
        self.chart_data_filtered(project, chart_type, function, column, group_by, None, None)
    }

    /// Computes a chart series in Rust optionally filtered by `filter_column == filter_value`.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], `SF-CAL-001`, `SF-CAL-002`, or `SF-CAL-004`.
    #[allow(clippy::too_many_arguments)]
    pub fn chart_data_filtered(
        &self,
        project: &str,
        chart_type: &str,
        function: &str,
        column: &str,
        group_by: &str,
        filter_column: Option<&str>,
        filter_value: Option<&str>,
    ) -> Result<String, String> {
        let f_col = filter_column.unwrap_or("");
        let f_val = filter_value.unwrap_or("");
        let payload_len = crate::ipc_payload_len!(
            ("project", project),
            ("chartType", chart_type),
            ("function", function),
            ("column", column),
            ("groupBy", group_by),
            ("filterColumn", f_col),
            ("filterValue", f_val),
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
            .compute_chart_series_filtered(kind, agg, column, group_by, filter_column, filter_value)
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

    /// Writes any generated binary (base64) or UTF-8 text deliverable atomically to disk
    /// (`temp -> fsync -> rename`) without modifying any source workbook.
    ///
    /// # Errors
    /// [`AUTH_REQUIRED_CODE`], [`UNKNOWN_PROJECT_CODE`], `SF-SYS-001` on invalid base64,
    /// or [`EXPORT_FAILED_CODE`].
    pub fn export_deliverable(
        &self,
        project: &str,
        target: &Path,
        is_base64: bool,
        payload: &str,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(
            ("project", project),
            ("target", target),
            ("payload", payload),
        );
        self.gate_command("studio_export_deliverable", payload_len, None)?;
        {
            let projects = self
                .projects
                .lock()
                .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
            if !projects.contains_key(project) {
                return Err(UNKNOWN_PROJECT_CODE.to_owned());
            }
        }
        let bytes = if is_base64 {
            BASE64_STANDARD
                .decode(payload.trim())
                .map_err(|_| "SF-SYS-001".to_owned())?
        } else {
            payload.as_bytes().to_vec()
        };
        write_atomic_bytes(target, &bytes)?;
        Ok(target.display().to_string())
    }

    fn ensure_search_index_loaded(&self) -> Result<(), String> {
        let key = self.ensure_local_vault_key()?;
        let mut slot = self
            .search_index
            .lock()
            .map_err(|_| "SF-SYS-001".to_owned())?;
        if slot.is_none() {
            let dir = self.resolve_storage_dir()?;
            let loaded = LocalSearchIndex::load_from_dir(&dir, Some(&key))?;
            *slot = Some(loaded);
        }
        Ok(())
    }

    fn ensure_recipe_library_loaded(&self) -> Result<(), String> {
        let key = self.ensure_local_vault_key()?;
        let mut slot = self
            .recipe_library
            .lock()
            .map_err(|_| "SF-SYS-001".to_owned())?;
        if slot.is_none() {
            let dir = self.resolve_storage_dir()?;
            let loaded = LocalRecipeLibrary::open(dir, Some(key))?;
            *slot = Some(loaded);
        }
        Ok(())
    }

    /// Indexes or incrementally updates one or more user-selected local folders into the
    /// on-device `LocalSearchIndex` (strictly local, zero cloud upload).
    ///
    /// # Errors
    /// Returns auth/offline errors or `SF-SYS-001` / `SF-IMP-002`.
    pub fn desktop_search_index_folders(
        &self,
        folders_json: &str,
        project_id: Option<&str>,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len =
            crate::ipc_payload_len!(("folders", folders_json), ("projectId", project_id),);
        self.gate_command("desktop_search_index_folders", payload_len, Some(now_ms))?;
        let raw_folders: Vec<String> =
            serde_json::from_str(folders_json).map_err(|_| "SF-SYS-001".to_owned())?;
        if raw_folders.is_empty() || raw_folders.len() > 32 {
            return Err("SF-SYS-001".to_owned());
        }
        let mut roots = Vec::with_capacity(raw_folders.len());
        for raw in raw_folders {
            let path = PathBuf::from(raw.trim());
            if path.as_os_str().is_empty()
                || path
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err("SF-IMP-002".to_owned());
            }
            roots.push(path);
        }

        self.ensure_search_index_loaded()?;
        let dir = self.resolve_storage_dir()?;
        let key = self.ensure_local_vault_key()?;
        let mut slot = self
            .search_index
            .lock()
            .map_err(|_| "SF-SYS-001".to_owned())?;
        let Some(index) = slot.as_mut() else {
            return Err("SF-SYS-001".to_owned());
        };
        let summary = index.index_folders(&roots, project_id, None, |_| {})?;
        index.save_to_dir(&dir, Some(&key))?;
        let stats = index.stats_snapshot();
        serde_json::to_string(&serde_json::json!({
            "summary": summary,
            "stats": stats,
        }))
        .map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Executes a local BM25 full-text search query (strictly Read-Only; works offline).
    ///
    /// # Errors
    /// Returns `SF-AUTH-001` or `SF-SYS-001`.
    pub fn desktop_search_query(&self, query_json: &str) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(("query", query_json));
        self.gate_command("desktop_search_query", payload_len, None)?;
        let query: SearchQuery =
            serde_json::from_str(query_json).map_err(|_| "SF-SYS-001".to_owned())?;
        self.ensure_search_index_loaded()?;
        let slot = self
            .search_index
            .lock()
            .map_err(|_| "SF-SYS-001".to_owned())?;
        let Some(index) = slot.as_ref() else {
            return Err("SF-SYS-001".to_owned());
        };
        let hits = index.search_readonly(&query);
        serde_json::to_string(&serde_json::json!({
            "hits": hits,
            "stats": index.stats_snapshot(),
        }))
        .map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Returns aggregate metadata of the local full-text search index (works offline).
    ///
    /// # Errors
    /// Returns `SF-AUTH-001` or `SF-SYS-001`.
    pub fn desktop_search_stats(&self) -> Result<String, String> {
        self.gate_command("desktop_search_stats", crate::ipc_payload_len!(), None)?;
        self.ensure_search_index_loaded()?;
        let slot = self
            .search_index
            .lock()
            .map_err(|_| "SF-SYS-001".to_owned())?;
        let Some(index) = slot.as_ref() else {
            return Err("SF-SYS-001".to_owned());
        };
        serde_json::to_string(&index.stats_snapshot()).map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Lists both cached Official Templates and user-owned local My Recipes (works offline).
    ///
    /// # Errors
    /// Returns `SF-AUTH-001` or `SF-SYS-001`.
    pub fn desktop_recipes_list(&self) -> Result<String, String> {
        self.gate_command("desktop_recipes_list", crate::ipc_payload_len!(), None)?;
        self.ensure_recipe_library_loaded()?;
        let slot = self
            .recipe_library
            .lock()
            .map_err(|_| "SF-SYS-001".to_owned())?;
        let Some(lib) = slot.as_ref() else {
            return Err("SF-SYS-001".to_owned());
        };
        serde_json::to_string(&serde_json::json!({
            "officialTemplates": lib.list_official_templates(),
            "myRecipes": lib.list_my_recipes(),
        }))
        .map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Synchronizes Official Templates fetched from `GET /v1/templates/official` into the local cache.
    ///
    /// # Errors
    /// Returns auth/offline errors or `SF-RCP-001`.
    pub fn desktop_recipes_sync_official(
        &self,
        templates_json: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(("templates", templates_json));
        self.gate_command("desktop_recipes_sync_official", payload_len, Some(now_ms))?;
        let raw_list: Vec<serde_json::Value> =
            serde_json::from_str(templates_json).map_err(|_| "SF-RCP-001".to_owned())?;
        let mut incoming = Vec::with_capacity(raw_list.len());
        for item in raw_list {
            let id = item
                .get("id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_owned();
            let name = item
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_owned();
            let description = item
                .get("description")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let category = item
                .get("category")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("general")
                .to_owned();
            let recipe_type = item
                .get("recipe_type")
                .or_else(|| item.get("recipeType"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("cleaning")
                .to_owned();
            let required_columns: Vec<String> = item
                .get("required_columns")
                .or_else(|| item.get("requiredColumns"))
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
            let steps: Vec<sf_recipe::recipe::RecipeStep> = item
                .get("steps")
                .and_then(parse_recipe_steps_flexible)
                .unwrap_or_default();
            if id.is_empty() || name.is_empty() || steps.is_empty() {
                continue;
            }
            let is_community = item
                .get("origin")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|o| o == "published_community_template");
            let mut recipe = sf_recipe::recipe::SafiRecipe {
                schema_version: 1,
                id: id.clone(),
                name: name.clone(),
                created_with: Some("1.0.0".to_owned()),
                region: None,
                column_rules: None,
                steps,
                content_hash: None,
            };
            recipe.content_hash = recipe.compute_content_hash().ok();
            incoming.push(LocalRecipeRecord {
                id,
                name,
                description,
                category,
                recipe_type,
                origin: if is_community {
                    RecipeOriginClass::PublishedCommunityTemplate
                } else {
                    RecipeOriginClass::OfficialTemplate
                },
                source_template_id: None,
                required_columns,
                recipe,
                created_at_ms: now_ms,
                updated_at_ms: now_ms,
            });
        }
        self.ensure_recipe_library_loaded()?;
        let mut slot = self
            .recipe_library
            .lock()
            .map_err(|_| "SF-SYS-001".to_owned())?;
        let Some(lib) = slot.as_mut() else {
            return Err("SF-SYS-001".to_owned());
        };
        let count = lib.sync_official_templates(incoming)?;
        serde_json::to_string(&serde_json::json!({
            "syncedCount": count,
            "officialTemplates": lib.list_official_templates(),
            "myRecipes": lib.list_my_recipes(),
        }))
        .map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Customizes an Official Template (or personal recipe) and saves an independent user-owned
    /// recipe into `My Recipes` (`Save as New Recipe`) without mutating the original Official Template.
    ///
    /// # Errors
    /// Returns auth/offline errors, `SF-RCP-001`, or `"DUPLICATE_RECIPE_NAME"`.
    pub fn desktop_recipes_save_as_new(
        &self,
        payload_json: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(("payload", payload_json));
        self.gate_command("desktop_recipes_save_as_new", payload_len, Some(now_ms))?;
        let body: serde_json::Value =
            serde_json::from_str(payload_json).map_err(|_| "SF-RCP-001".to_owned())?;
        let source_template_id = body
            .get("sourceTemplateId")
            .and_then(serde_json::Value::as_str);
        let name = body
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let description = body
            .get("description")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let category = body
            .get("category")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("custom");
        let recipe_type = body
            .get("recipeType")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("cleaning");
        let conflict_policy = parse_conflict_policy(
            body.get("conflictPolicy")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("fail_on_duplicate"),
        );
        let custom_steps: Option<Vec<sf_recipe::recipe::RecipeStep>> =
            body.get("steps").and_then(parse_recipe_steps_flexible);
        let required_columns: Option<Vec<String>> = body
            .get("requiredColumns")
            .and_then(|v| serde_json::from_value(v.clone()).ok());

        self.ensure_recipe_library_loaded()?;
        let mut slot = self
            .recipe_library
            .lock()
            .map_err(|_| "SF-SYS-001".to_owned())?;
        let Some(lib) = slot.as_mut() else {
            return Err("SF-SYS-001".to_owned());
        };

        let mut working_copy = if let Some(src_id) = source_template_id {
            if lib.get_official_template(src_id).is_some() {
                lib.clone_official_for_editing(src_id, now_ms)?
            } else if let Some(existing) = lib.get_my_recipe(src_id).cloned() {
                existing
            } else {
                return Err("SF-RCP-001".to_owned());
            }
        } else {
            let steps = custom_steps
                .clone()
                .ok_or_else(|| "SF-RCP-001".to_owned())?;
            LocalRecipeRecord {
                id: String::new(),
                name: name.trim().to_owned(),
                description: description.trim().to_owned(),
                category: category.trim().to_owned(),
                recipe_type: recipe_type.trim().to_owned(),
                origin: RecipeOriginClass::MyRecipe,
                source_template_id: None,
                required_columns: required_columns.clone().unwrap_or_default(),
                recipe: sf_recipe::recipe::SafiRecipe {
                    schema_version: 1,
                    id: String::new(),
                    name: name.trim().to_owned(),
                    created_with: Some("1.0.0".to_owned()),
                    region: None,
                    column_rules: None,
                    steps,
                    content_hash: None,
                },
                created_at_ms: now_ms,
                updated_at_ms: now_ms,
            }
        };

        if let Some(steps) = custom_steps {
            LocalRecipeLibrary::customize_working_copy_steps(&mut working_copy, steps, now_ms)?;
        }
        if let Some(cols) = required_columns {
            working_copy.required_columns = cols;
        }
        working_copy.recipe_type = recipe_type.trim().to_owned();

        let saved = lib.save_as_new_recipe(
            working_copy,
            name,
            description,
            category,
            conflict_policy,
            now_ms,
        )?;
        serde_json::to_string(&serde_json::json!({
            "saved": saved,
            "officialTemplates": lib.list_official_templates(),
            "myRecipes": lib.list_my_recipes(),
        }))
        .map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Deletes a user-owned recipe from `My Recipes` when explicitly confirmed (never deletes Official Templates).
    ///
    /// # Errors
    /// Returns auth/offline errors or `SF-RCP-001`.
    pub fn desktop_recipes_delete(
        &self,
        recipe_id: &str,
        confirmed: bool,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len =
            crate::ipc_payload_len!(("recipeId", recipe_id), ("confirmed", confirmed),);
        self.gate_command("desktop_recipes_delete", payload_len, Some(now_ms))?;
        self.ensure_recipe_library_loaded()?;
        let mut slot = self
            .recipe_library
            .lock()
            .map_err(|_| "SF-SYS-001".to_owned())?;
        let Some(lib) = slot.as_mut() else {
            return Err("SF-SYS-001".to_owned());
        };
        lib.delete_my_recipe(recipe_id, confirmed)?;
        serde_json::to_string(&serde_json::json!({
            "deleted": recipe_id,
            "myRecipes": lib.list_my_recipes(),
        }))
        .map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Exports a user-owned recipe to a portable `.safitemplate` file after server entitlement verification.
    ///
    /// # Errors
    /// Returns auth/offline errors, `SF-RCP-005`, `SF-RCP-006`, or `SF-RCP-001`.
    pub fn desktop_recipes_export_safitemplate(
        &self,
        recipe_id: &str,
        target_path: &Path,
        premium_entitlement_verified: bool,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(
            ("recipeId", recipe_id),
            ("targetPath", target_path),
            ("premiumEntitlementVerified", premium_entitlement_verified),
        );
        self.gate_command(
            "desktop_recipes_export_safitemplate",
            payload_len,
            Some(now_ms),
        )?;
        self.ensure_recipe_library_loaded()?;
        let slot = self
            .recipe_library
            .lock()
            .map_err(|_| "SF-SYS-001".to_owned())?;
        let Some(lib) = slot.as_ref() else {
            return Err("SF-SYS-001".to_owned());
        };
        let exported =
            lib.export_safitemplate(recipe_id, target_path, premium_entitlement_verified)?;
        serde_json::to_string(&serde_json::json!({
            "path": target_path.display().to_string(),
            "portable": exported,
        }))
        .map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Imports a validated `.safitemplate` file into `My Recipes` after server entitlement verification.
    ///
    /// # Errors
    /// Returns auth/offline errors, `SF-RCP-005`, `SF-RCP-006`, `SF-CAL-008`, `SF-RCP-001`, or `"DUPLICATE_RECIPE_NAME"`.
    pub fn desktop_recipes_import_safitemplate(
        &self,
        source_path: &Path,
        premium_entitlement_verified: bool,
        conflict_policy: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(
            ("sourcePath", source_path),
            ("premiumEntitlementVerified", premium_entitlement_verified),
            ("conflictPolicy", conflict_policy),
        );
        self.gate_command(
            "desktop_recipes_import_safitemplate",
            payload_len,
            Some(now_ms),
        )?;
        self.ensure_recipe_library_loaded()?;
        let mut slot = self
            .recipe_library
            .lock()
            .map_err(|_| "SF-SYS-001".to_owned())?;
        let Some(lib) = slot.as_mut() else {
            return Err("SF-SYS-001".to_owned());
        };
        let imported = lib.import_safitemplate(
            source_path,
            premium_entitlement_verified,
            parse_conflict_policy(conflict_policy),
            now_ms,
        )?;
        serde_json::to_string(&serde_json::json!({
            "imported": imported,
            "myRecipes": lib.list_my_recipes(),
        }))
        .map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Prepares a sanitized `Share with SafiData` community contribution preview & payload with
    /// secret/PII scanning and mandatory consent validation (zero dataset rows).
    ///
    /// # Errors
    /// Returns auth/offline errors, `SF-RCP-006`, or `SF-RCP-001`.
    pub fn desktop_recipes_prepare_community_share(
        &self,
        recipe_id: &str,
        consent_accepted: bool,
        consent_terms_version: &str,
        show_contributor_attribution: bool,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(
            ("recipeId", recipe_id),
            ("consentAccepted", consent_accepted),
            ("consentTermsVersion", consent_terms_version),
            ("showContributorAttribution", show_contributor_attribution),
        );
        self.gate_command(
            "desktop_recipes_prepare_community_share",
            payload_len,
            Some(now_ms),
        )?;
        self.ensure_recipe_library_loaded()?;
        let slot = self
            .recipe_library
            .lock()
            .map_err(|_| "SF-SYS-001".to_owned())?;
        let Some(lib) = slot.as_ref() else {
            return Err("SF-SYS-001".to_owned());
        };
        let payload = lib.prepare_community_contribution(
            recipe_id,
            consent_accepted,
            consent_terms_version,
            show_contributor_attribution,
        )?;
        serde_json::to_string(&payload).map_err(|_| "SF-SYS-001".to_owned())
    }

    fn ensure_workflow_memory_loaded(&self) -> Result<(), String> {
        let key = self.ensure_local_vault_key()?;
        let mut slot = self
            .workflow_memory
            .lock()
            .map_err(|_| "SF-SYS-001".to_owned())?;
        if slot.is_none() {
            let dir = self.resolve_storage_dir()?;
            let loaded = LocalWorkflowMemoryStore::open(dir, Some(key))?;
            *slot = Some(loaded);
        }
        Ok(())
    }

    /// Updates an existing user-owned recipe in place (never allowed on Official Templates).
    ///
    /// # Errors
    /// Returns auth/offline errors or `SF-RCP-001` / `"DUPLICATE_RECIPE_NAME"`.
    pub fn desktop_recipes_update_existing(
        &self,
        payload_json: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(("payload", payload_json));
        self.gate_command("desktop_recipes_update_existing", payload_len, Some(now_ms))?;
        let body: serde_json::Value =
            serde_json::from_str(payload_json).map_err(|_| "SF-RCP-001".to_owned())?;
        let recipe_id = body
            .get("recipeId")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .trim();
        if recipe_id.is_empty() {
            return Err("SF-RCP-001".to_owned());
        }
        let new_name = body
            .get("name")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let new_description = body.get("description").and_then(serde_json::Value::as_str);
        let new_steps: Vec<sf_recipe::recipe::RecipeStep> = body
            .get("steps")
            .and_then(parse_recipe_steps_flexible)
            .ok_or_else(|| "SF-RCP-001".to_owned())?;

        self.ensure_recipe_library_loaded()?;
        let mut slot = self
            .recipe_library
            .lock()
            .map_err(|_| "SF-SYS-001".to_owned())?;
        let Some(lib) = slot.as_mut() else {
            return Err("SF-SYS-001".to_owned());
        };
        if let Some(name_str) = new_name {
            let current_name = lib.get_my_recipe(recipe_id).map(|r| r.name.clone());
            if current_name.as_deref() != Some(name_str) {
                let _ = lib.rename_my_recipe(recipe_id, name_str, now_ms)?;
            }
        }
        let updated = lib.update_my_recipe(recipe_id, new_steps, new_description, now_ms)?;
        serde_json::to_string(&serde_json::json!({
            "updated": updated,
            "officialTemplates": lib.list_official_templates(),
            "myRecipes": lib.list_my_recipes(),
        }))
        .map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Duplicates an existing user-owned recipe under a unique generated copy name.
    ///
    /// # Errors
    /// Returns auth/offline errors or `SF-RCP-001`.
    pub fn desktop_recipes_duplicate(
        &self,
        recipe_id: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(("recipeId", recipe_id));
        self.gate_command("desktop_recipes_duplicate", payload_len, Some(now_ms))?;
        self.ensure_recipe_library_loaded()?;
        let mut slot = self
            .recipe_library
            .lock()
            .map_err(|_| "SF-SYS-001".to_owned())?;
        let Some(lib) = slot.as_mut() else {
            return Err("SF-SYS-001".to_owned());
        };
        let duplicated = lib.duplicate_my_recipe(recipe_id, now_ms)?;
        serde_json::to_string(&serde_json::json!({
            "duplicated": duplicated,
            "myRecipes": lib.list_my_recipes(),
        }))
        .map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Deterministically applies a saved or official recipe onto an open Studio project dataset.
    ///
    /// # Errors
    /// Returns auth/offline errors, `SF-RCP-001`, or `SF-SYS-015`.
    pub fn desktop_recipes_apply_to_project(
        &self,
        project: &str,
        recipe_id: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(("project", project), ("recipeId", recipe_id));
        self.gate_command(
            "desktop_recipes_apply_to_project",
            payload_len,
            Some(now_ms),
        )?;
        self.ensure_recipe_library_loaded()?;
        let record = {
            let slot = self
                .recipe_library
                .lock()
                .map_err(|_| "SF-SYS-001".to_owned())?;
            let Some(lib) = slot.as_ref() else {
                return Err("SF-SYS-001".to_owned());
            };
            lib.get_my_recipe(recipe_id)
                .or_else(|| lib.get_official_template(recipe_id))
                .cloned()
                .ok_or_else(|| "SF-RCP-001".to_owned())?
        };

        let col_names: Vec<String> = {
            let projects = self
                .projects
                .lock()
                .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
            let graph = projects
                .get(project)
                .ok_or_else(|| UNKNOWN_PROJECT_CODE.to_owned())?;
            graph
                .working()
                .columns()
                .iter()
                .map(|c| c.schema().name.clone())
                .collect()
        };

        for step in &record.recipe.steps {
            let cols = step.columns.clone().unwrap_or_default();
            match step.op {
                sf_recipe::recipe::RecipeOperation::NormalizeText
                | sf_recipe::recipe::RecipeOperation::NormalizePhone
                | sf_recipe::recipe::RecipeOperation::ParseDate
                | sf_recipe::recipe::RecipeOperation::ParseNumber => {
                    let targets: Vec<String> =
                        if cols.is_empty() || cols.iter().any(|c| c == "all" || c == "*") {
                            col_names.clone()
                        } else {
                            cols.into_iter().filter(|c| col_names.contains(c)).collect()
                        };
                    for col in targets {
                        let _ = self.mutate(project, |graph| graph.trim_column(&col, now_ms));
                    }
                }
                sf_recipe::recipe::RecipeOperation::Rename => {
                    let from = step
                        .params
                        .get("from")
                        .and_then(serde_json::Value::as_str)
                        .or_else(|| cols.first().map(String::as_str));
                    let to = step.params.get("to").and_then(serde_json::Value::as_str);
                    if let (Some(from_col), Some(to_col)) = (from, to) {
                        if col_names.contains(&from_col.to_owned()) {
                            let _ = self.mutate(project, |graph| {
                                graph.rename_column(from_col, to_col, now_ms)
                            });
                        }
                    }
                }
                _ => {
                    for col in cols {
                        if col_names.contains(&col) {
                            let _ = self.mutate(project, |graph| graph.trim_column(&col, now_ms));
                        }
                    }
                }
            }
        }
        self.state_unlocked(project)
    }

    /// Lists all saved per-sheet workflow memory records and the personal learning profile (works offline).
    ///
    /// # Errors
    /// Returns `SF-AUTH-001` or `SF-SYS-001`.
    pub fn desktop_workflow_memory_list(&self) -> Result<String, String> {
        self.gate_command(
            "desktop_workflow_memory_list",
            crate::ipc_payload_len!(),
            None,
        )?;
        self.ensure_workflow_memory_loaded()?;
        let slot = self
            .workflow_memory
            .lock()
            .map_err(|_| "SF-SYS-001".to_owned())?;
        let Some(store) = slot.as_ref() else {
            return Err("SF-SYS-001".to_owned());
        };
        serde_json::to_string(&serde_json::json!({
            "workflows": store.list_sheet_workflows(),
            "learningProfile": store.learning_profile(),
        }))
        .map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Saves or updates the complete per-sheet workflow memory for a worksheet/table.
    ///
    /// # Errors
    /// Returns auth/offline errors or `SF-SYS-001`.
    pub fn desktop_workflow_memory_save_sheet(
        &self,
        payload_json: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(("payload", payload_json));
        self.gate_command(
            "desktop_workflow_memory_save_sheet",
            payload_len,
            Some(now_ms),
        )?;
        let body: serde_json::Value =
            serde_json::from_str(payload_json).map_err(|_| "SF-SYS-001".to_owned())?;

        let workflow_id = body.get("workflowId").and_then(serde_json::Value::as_str);
        let display_name = body
            .get("displayName")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let project_id = body
            .get("projectId")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("main");
        let sheet_name = body
            .get("sheetName")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Sheet1");
        let table_id = body
            .get("tableId")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("tbl_1");
        let headers: Vec<String> = body
            .get("headers")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        let question_resolutions: Vec<SavedQuestionDecision> = body
            .get("questionResolutions")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        let column_decisions: Vec<SavedColumnDecision> = body
            .get("columnDecisions")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        let duplicate_policy = body
            .get("duplicatePolicy")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("exact_key");
        let missing_value_policy = body
            .get("missingValuePolicy")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("flag_only");
        let business_rules: Vec<String> = body
            .get("businessRules")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        let cleaning_operations: Vec<SavedCleaningOperation> = body
            .get("cleaningOperations")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        let metrics: Vec<SavedMetricDefinition> = body
            .get("metrics")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        let dashboard_preferences: SavedDashboardPreferences = body
            .get("dashboardPreferences")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        let recommendation_feedback: Vec<SavedRecommendationFeedback> = body
            .get("recommendationFeedback")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        let active_table = self
            .projects
            .lock()
            .ok()
            .and_then(|map| map.get(project_id).map(|g| g.working().clone()));

        self.ensure_workflow_memory_loaded()?;
        let mut slot = self
            .workflow_memory
            .lock()
            .map_err(|_| "SF-SYS-001".to_owned())?;
        let Some(store) = slot.as_mut() else {
            return Err("SF-SYS-001".to_owned());
        };

        let saved = store.save_sheet_workflow_with_table(
            workflow_id,
            display_name,
            project_id,
            sheet_name,
            table_id,
            &headers,
            active_table.as_ref(),
            question_resolutions,
            column_decisions,
            duplicate_policy,
            missing_value_policy,
            business_rules,
            cleaning_operations,
            metrics,
            dashboard_preferences,
            recommendation_feedback,
            now_ms,
        )?;
        serde_json::to_string(&serde_json::json!({
            "saved": saved,
            "workflows": store.list_sheet_workflows(),
        }))
        .map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Matches an incoming or active worksheet against saved per-sheet workflow memories and
    /// learned personal preferences (works in Offline Read-Only Mode).
    ///
    /// # Errors
    /// Returns `SF-AUTH-001` or `SF-SYS-001`.
    pub fn desktop_workflow_memory_match_sheet(
        &self,
        payload_json: &str,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(("payload", payload_json));
        self.gate_command("desktop_workflow_memory_match_sheet", payload_len, None)?;
        let body: serde_json::Value =
            serde_json::from_str(payload_json).map_err(|_| "SF-SYS-001".to_owned())?;
        let project_id = body
            .get("projectId")
            .or_else(|| body.get("project"))
            .and_then(serde_json::Value::as_str);
        let sheet_name = body
            .get("sheetName")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Sheet1");
        let headers: Vec<String> = body
            .get("headers")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        let observed_date_format = body
            .get("observedDateFormat")
            .and_then(serde_json::Value::as_str);

        let active_table = project_id.and_then(|pid| {
            self.projects.lock().ok().and_then(|map| {
                let g = map.get(pid)?;
                let working = g.working();
                let working_headers: Vec<String> = working
                    .columns()
                    .iter()
                    .map(|c| c.schema().name.clone())
                    .collect();
                if working_headers == headers {
                    Some(working.clone())
                } else {
                    None
                }
            })
        });

        self.ensure_workflow_memory_loaded()?;
        let slot = self
            .workflow_memory
            .lock()
            .map_err(|_| "SF-SYS-001".to_owned())?;
        let Some(store) = slot.as_ref() else {
            return Err("SF-SYS-001".to_owned());
        };
        let report = store.match_incoming_sheet_with_table(
            sheet_name,
            &headers,
            observed_date_format,
            active_table.as_ref(),
        );
        let matched_record = report
            .matched_workflow_id
            .as_deref()
            .and_then(|id| store.get_sheet_workflow(id))
            .cloned();
        serde_json::to_string(&serde_json::json!({
            "report": report,
            "matchedWorkflow": matched_record,
        }))
        .map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Deterministically and atomically replays a saved sheet workflow's cleaning operations,
    /// metrics, and question resolutions onto an open Studio project dataset.
    ///
    /// Executes against an isolated staged clone of `DataGraph` and `StudioWorkspaceArtifacts`,
    /// preflight-validating confidence tier, required column mapping, every operation, and every
    /// metric. If any step fails or is unsupported, returns an explicit error code and leaves the
    /// active project state completely untouched.
    ///
    /// # Errors
    /// Returns auth/offline errors, `SF-RCP-001` when eligibility/preflight/operation fails,
    /// `SF-CAL-*` on formula/column failure, `SF-SYS-015`, or `SF-SYS-001`.
    pub fn desktop_workflow_memory_apply_replay(
        &self,
        project: &str,
        workflow_id: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len =
            crate::ipc_payload_len!(("project", project), ("workflowId", workflow_id));
        self.gate_command(
            "desktop_workflow_memory_apply_replay",
            payload_len,
            Some(now_ms),
        )?;
        self.ensure_workflow_memory_loaded()?;

        // Stage an isolated clone of the active DataGraph and StudioWorkspaceArtifacts for atomic replay.
        let snapshot_json = self.snapshot_unlocked(project)?;
        let mut staged_graph =
            DataGraph::from_json(&snapshot_json, now_ms).map_err(|error| code_of(&error))?;
        let mut staged_artifacts = self.current_artifacts(project);

        let current_headers: Vec<String> = staged_graph
            .working()
            .columns()
            .iter()
            .map(|c| c.schema().name.clone())
            .collect();
        let active_sheet_name = self
            .active_regions
            .lock()
            .ok()
            .and_then(|map| map.get(project).map(|(s, _)| s.clone()))
            .unwrap_or_else(|| "Sheet1".to_owned());

        let (workflow, match_report) = {
            let slot = self
                .workflow_memory
                .lock()
                .map_err(|_| "SF-SYS-001".to_owned())?;
            let Some(store) = slot.as_ref() else {
                return Err("SF-SYS-001".to_owned());
            };
            let wf = store
                .get_sheet_workflow(workflow_id)
                .cloned()
                .ok_or_else(|| "SF-SYS-001".to_owned())?;
            let rep = store.match_specific_workflow_with_table(
                workflow_id,
                &active_sheet_name,
                &current_headers,
                None,
                Some(staged_graph.working()),
            )?;
            (wf, rep)
        };

        // Strictly enforce at the Rust boundary: full workflow replay is allowed ONLY for High
        // confidence when `auto_replay_eligible` is true, with zero missing required columns and
        // zero content-type drift. Medium (partial/shifted schema) and Low MUST fail closed (`SF-RCP-001`).
        if match_report.match_tier != crate::workflow_memory::WorkflowMatchTier::High
            || !match_report.auto_replay_eligible
            || !match_report.missing_required_columns.is_empty()
            || match_report
                .schema_change_notices
                .iter()
                .any(|n| n.contains("Content type mismatch"))
        {
            return Err("SF-RCP-001".to_owned());
        }

        let resolve_col = |saved_col: &str| -> String {
            match_report
                .column_mapping
                .get(saved_col)
                .cloned()
                .unwrap_or_else(|| saved_col.to_owned())
        };

        let mut applied_ops = 0usize;
        for op in &workflow.cleaning_operations {
            match op.kind.as_str() {
                "trim_column" => {
                    let col = op
                        .target_column
                        .as_deref()
                        .ok_or_else(|| "SF-RCP-001".to_owned())?;
                    let mapped = resolve_col(col);
                    let _ = staged_graph
                        .trim_column(&mapped, now_ms)
                        .map_err(|error| code_of(&error))?;
                    applied_ops += 1;
                }
                "rename_column" => {
                    let from = op
                        .target_column
                        .as_deref()
                        .ok_or_else(|| "SF-RCP-001".to_owned())?;
                    let to = op
                        .parameter
                        .as_deref()
                        .ok_or_else(|| "SF-RCP-001".to_owned())?;
                    let mapped = resolve_col(from);
                    let _ = staged_graph
                        .rename_column(&mapped, to, now_ms)
                        .map_err(|error| code_of(&error))?;
                    applied_ops += 1;
                }
                "dsl_command" => {
                    let cmd_str = op
                        .parameter
                        .as_deref()
                        .ok_or_else(|| "SF-RCP-001".to_owned())?;
                    let parsed = parse_command(cmd_str).map_err(|error| code_of(&error))?;
                    let _ = apply_command(&mut staged_graph, &parsed, now_ms)
                        .map_err(|error| code_of(&error))?;
                    applied_ops += 1;
                }
                "fill_missing" => {
                    let col = op
                        .target_column
                        .as_deref()
                        .ok_or_else(|| "SF-RCP-001".to_owned())?;
                    let replacement = op.parameter.as_deref().unwrap_or("0");
                    let mapped = resolve_col(col);
                    let target_col = staged_graph
                        .working()
                        .columns()
                        .iter()
                        .find(|c| c.schema().name == mapped)
                        .ok_or_else(|| "SF-CAL-001".to_owned())?;
                    let mut blank_row_ids = Vec::new();
                    for r_idx in 0..staged_graph.working().row_count() {
                        let is_blank = target_col
                            .cell_at(r_idx)
                            .is_none_or(|c| c.raw_text().trim().is_empty());
                        if is_blank && let Some(rid) = staged_graph.row_id_at(r_idx) {
                            blank_row_ids.push(rid.get());
                        }
                    }
                    if !blank_row_ids.is_empty() {
                        let _ = staged_graph
                            .apply_anomaly_fix(
                                AnomalyFix::FillMissing {
                                    column: mapped,
                                    row_ids: blank_row_ids,
                                    replacement: replacement.to_owned(),
                                },
                                now_ms,
                            )
                            .map_err(|error| code_of(&error))?;
                    }
                    applied_ops += 1;
                }
                "drop_duplicate_rows" => {
                    let mut seen = std::collections::BTreeSet::new();
                    let mut dup_row_ids = Vec::new();
                    let rc = staged_graph.working().row_count();
                    let cols = staged_graph.working().columns();
                    for r_idx in 0..rc {
                        let key: Vec<String> = cols
                            .iter()
                            .map(|c| {
                                c.cell_at(r_idx)
                                    .map_or_else(String::new, |cell| cell.raw_text().to_owned())
                            })
                            .collect();
                        if !seen.insert(key)
                            && let Some(rid) = staged_graph.row_id_at(r_idx)
                        {
                            dup_row_ids.push(rid.get());
                        }
                    }
                    if !dup_row_ids.is_empty() {
                        let _ = staged_graph
                            .apply_anomaly_fix(
                                AnomalyFix::DropDuplicateRows {
                                    row_ids: dup_row_ids,
                                },
                                now_ms,
                            )
                            .map_err(|error| code_of(&error))?;
                    }
                    applied_ops += 1;
                }
                _ => {
                    return Err("SF-RCP-001".to_owned());
                }
            }
        }

        let mut applied_metrics = 0usize;
        for m in &workflow.metrics {
            // Translate any mapped column names in the formula (e.g. `SUM(amount)` -> `SUM(المبيعات)`).
            let mut mapped_formula = m.formula.clone();
            for (saved_h, incoming_h) in &match_report.column_mapping {
                if saved_h != incoming_h {
                    mapped_formula =
                        mapped_formula.replace(&format!("({saved_h})"), &format!("({incoming_h})"));
                }
            }
            let mid = MetricId::new(&m.id).map_err(|error| code_of(&error))?;
            let _ = staged_graph
                .define_metric(mid, &m.name, m.unit.as_deref(), &mapped_formula, now_ms)
                .map_err(|error| code_of(&error))?;
            applied_metrics += 1;
        }

        // Update staged question resolutions.
        let reused_count = match_report.reused_question_resolutions.len();
        for rq in &match_report.reused_question_resolutions {
            if let Some(existing) = staged_artifacts
                .question_resolutions
                .iter_mut()
                .find(|q| q.question_id == rq.question_id)
            {
                existing.action = rq.action.clone();
                existing.answer = rq.answer.clone();
                existing.custom_text = rq.custom_text.clone();
                existing.updated_at_ms = now_ms;
            } else {
                staged_artifacts
                    .question_resolutions
                    .push(StudioQuestionResolutionArtifact {
                        question_id: rq.question_id.clone(),
                        action: rq.action.clone(),
                        answer: rq.answer.clone(),
                        custom_text: rq.custom_text.clone(),
                        updated_at_ms: now_ms,
                    });
            }
        }

        // Persist execution telemetry BEFORE committing live project state (propagate any vault/disk error!).
        {
            let mut slot = self
                .workflow_memory
                .lock()
                .map_err(|_| "SF-SYS-001".to_owned())?;
            let Some(store) = slot.as_mut() else {
                return Err("SF-SYS-001".to_owned());
            };
            let _ = store.record_workflow_execution(
                workflow_id,
                applied_ops,
                applied_metrics,
                reused_count,
                now_ms,
            )?;
        }

        // Atomic commit: replace active project DataGraph and artifacts only after every step succeeded!
        {
            let mut projects = self
                .projects
                .lock()
                .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
            let _ = projects.insert(project.to_owned(), staged_graph);
        }
        {
            let mut art_map = self.artifacts.lock().map_err(|_| "SF-SYS-001".to_owned())?;
            let _ = art_map.insert(project.to_owned(), staged_artifacts);
        }

        self.state_unlocked(project)
    }

    /// Manages saved per-sheet workflows (`restore_version`, `duplicate`, `rename`, `delete`).
    ///
    /// # Errors
    /// Returns auth/offline errors or `SF-SYS-001`.
    pub fn desktop_workflow_memory_manage(
        &self,
        payload_json: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(("payload", payload_json));
        self.gate_command("desktop_workflow_memory_manage", payload_len, Some(now_ms))?;
        let body: serde_json::Value =
            serde_json::from_str(payload_json).map_err(|_| "SF-SYS-001".to_owned())?;
        let action = body
            .get("action")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let workflow_id = body
            .get("workflowId")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();

        self.ensure_workflow_memory_loaded()?;
        let mut slot = self
            .workflow_memory
            .lock()
            .map_err(|_| "SF-SYS-001".to_owned())?;
        let Some(store) = slot.as_mut() else {
            return Err("SF-SYS-001".to_owned());
        };

        let result_record = match action {
            "restore_version" => {
                let target_version = body
                    .get("targetVersion")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|v| u32::try_from(v).ok())
                    .ok_or_else(|| "SF-SYS-001".to_owned())?;
                Some(store.restore_workflow_version(workflow_id, target_version, now_ms)?)
            }
            "duplicate" => {
                let new_name = body.get("newName").and_then(serde_json::Value::as_str);
                Some(store.duplicate_workflow(workflow_id, new_name, now_ms)?)
            }
            "rename" => {
                let new_name = body
                    .get("newName")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                Some(store.rename_workflow(workflow_id, new_name, now_ms)?)
            }
            "delete" => {
                store.delete_workflow(workflow_id)?;
                None
            }
            _ => return Err("SF-SYS-001".to_owned()),
        };

        serde_json::to_string(&serde_json::json!({
            "workflow": result_record,
            "workflows": store.list_sheet_workflows(),
        }))
        .map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Returns the user's local [`PersonalLearningProfile`] (works in Offline Read-Only Mode).
    ///
    /// # Errors
    /// Returns `SF-AUTH-001` or `SF-SYS-001`.
    pub fn desktop_learning_profile_get(&self) -> Result<String, String> {
        self.gate_command(
            "desktop_learning_profile_get",
            crate::ipc_payload_len!(),
            None,
        )?;
        self.ensure_workflow_memory_loaded()?;
        let slot = self
            .workflow_memory
            .lock()
            .map_err(|_| "SF-SYS-001".to_owned())?;
        let Some(store) = slot.as_ref() else {
            return Err("SF-SYS-001".to_owned());
        };
        serde_json::to_string(store.learning_profile()).map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Records explicit user learning feedback into [`PersonalLearningProfile`].
    ///
    /// # Errors
    /// Returns auth/offline errors or `SF-SYS-001`.
    pub fn desktop_learning_record_feedback(
        &self,
        payload_json: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(("payload", payload_json));
        self.gate_command(
            "desktop_learning_record_feedback",
            payload_len,
            Some(now_ms),
        )?;
        let body: serde_json::Value =
            serde_json::from_str(payload_json).map_err(|_| "SF-SYS-001".to_owned())?;

        let signal_kind: LearningSignalKind = body
            .get("signalKind")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or(LearningSignalKind::ExplicitCorrection);
        let category: LearningCategory = body
            .get("category")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or(LearningCategory::ColumnClassification);
        let scope: LearningRuleScope = body
            .get("scope")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or(LearningRuleScope::MatchingSheets);
        let bound_hash = body
            .get("boundStructuralHash")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let business_domain: Option<SheetBusinessDomain> = body
            .get("businessDomain")
            .and_then(|v| serde_json::from_value(v.clone()).ok());
        let target_concept = body
            .get("targetConcept")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let preferred_value = body
            .get("preferredValue")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let rejected_value = body
            .get("rejectedValue")
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .map(str::to_owned);
        let user_explanation = body
            .get("userExplanation")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let source_sheet_label = body
            .get("sourceSheetLabel")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("Worksheet");

        self.ensure_workflow_memory_loaded()?;
        let mut slot = self
            .workflow_memory
            .lock()
            .map_err(|_| "SF-SYS-001".to_owned())?;
        let Some(store) = slot.as_mut() else {
            return Err("SF-SYS-001".to_owned());
        };
        let rule = store.record_learning_feedback(
            signal_kind,
            category,
            scope,
            bound_hash,
            business_domain,
            target_concept,
            preferred_value,
            rejected_value,
            user_explanation,
            source_sheet_label,
            now_ms,
        )?;
        serde_json::to_string(&serde_json::json!({
            "rule": rule,
            "learningProfile": store.learning_profile(),
        }))
        .map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Removes (undoes) a learned preference rule from [`PersonalLearningProfile`].
    ///
    /// # Errors
    /// Returns auth/offline errors or `SF-SYS-001`.
    pub fn desktop_learning_delete_rule(
        &self,
        rule_id: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(("ruleId", rule_id));
        self.gate_command("desktop_learning_delete_rule", payload_len, Some(now_ms))?;
        self.ensure_workflow_memory_loaded()?;
        let mut slot = self
            .workflow_memory
            .lock()
            .map_err(|_| "SF-SYS-001".to_owned())?;
        let Some(store) = slot.as_mut() else {
            return Err("SF-SYS-001".to_owned());
        };
        store.delete_learned_rule(rule_id, now_ms)?;
        serde_json::to_string(store.learning_profile()).map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Resets the local [`PersonalLearningProfile`].
    ///
    /// # Errors
    /// Returns auth/offline errors or `SF-SYS-001`.
    pub fn desktop_learning_reset_profile(&self, now_ms: u64) -> Result<String, String> {
        self.gate_command(
            "desktop_learning_reset_profile",
            crate::ipc_payload_len!(),
            Some(now_ms),
        )?;
        self.ensure_workflow_memory_loaded()?;
        let mut slot = self
            .workflow_memory
            .lock()
            .map_err(|_| "SF-SYS-001".to_owned())?;
        let Some(store) = slot.as_mut() else {
            return Err("SF-SYS-001".to_owned());
        };
        let profile = store.reset_learning_profile(now_ms)?;
        serde_json::to_string(&profile).map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Evaluates the central Rust [`crate::decision_engine::SafiDecisionEngine`] on the active
    /// Studio project dataset, artifacts, workflow memory, and personal learning profile.
    ///
    /// Works in Offline Read-Only Mode (`CommandAccessClass::ReadOnlyOffline`).
    ///
    /// # Errors
    /// Returns `SF-AUTH-001`, `SF-SYS-015`, or `SF-SYS-001`.
    pub fn desktop_decision_engine_evaluate(&self, project: &str) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(("project", project));
        self.gate_command("desktop_decision_engine_evaluate", payload_len, None)?;
        self.ensure_workflow_memory_loaded()?;

        let projects = self
            .projects
            .lock()
            .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
        let graph = projects
            .get(project)
            .ok_or_else(|| UNKNOWN_PROJECT_CODE.to_owned())?;

        let artifacts = self
            .artifacts
            .lock()
            .ok()
            .and_then(|map| map.get(project).cloned())
            .unwrap_or_default();

        let headers: Vec<String> = graph
            .working()
            .columns()
            .iter()
            .map(|c| c.schema().name.clone())
            .collect();
        let active_sheet_name = self
            .active_regions
            .lock()
            .ok()
            .and_then(|map| map.get(project).map(|(s, _)| s.clone()))
            .unwrap_or_else(|| "Sheet1".to_owned());

        let (matched_wf, learning_prof) = {
            let slot = self
                .workflow_memory
                .lock()
                .map_err(|_| "SF-SYS-001".to_owned())?;
            if let Some(store) = slot.as_ref() {
                let rep = store.match_incoming_sheet_with_table(
                    &active_sheet_name,
                    &headers,
                    None,
                    Some(graph.working()),
                );
                let wf = rep
                    .matched_workflow_id
                    .as_deref()
                    .and_then(|id| store.get_sheet_workflow(id))
                    .cloned();
                (wf, Some(store.learning_profile().clone()))
            } else {
                (None, None)
            }
        };

        let ollama_probe = self.ollama_probe.lock().ok().and_then(|g| g.clone());
        let suite = crate::decision_engine::SafiDecisionEngine::evaluate_workspace_with_ollama(
            graph,
            &artifacts,
            matched_wf.as_ref(),
            learning_prof.as_ref(),
            ollama_probe.as_ref(),
        );
        serde_json::to_string(&suite).map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Executes the recommended action for `decision_id` from [`crate::decision_engine::SafiDecisionEngine`]
    /// atomically on the open Studio project dataset and artifacts, returning the updated project state.
    ///
    /// # Errors
    /// Returns auth/offline errors, `SF-SYS-015`, or `SF-SYS-001`.
    pub fn desktop_decision_engine_apply(
        &self,
        project: &str,
        decision_id: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len =
            crate::ipc_payload_len!(("project", project), ("decisionId", decision_id));
        self.gate_command("desktop_decision_engine_apply", payload_len, Some(now_ms))?;
        self.ensure_workflow_memory_loaded()?;

        let suite_json = self.desktop_decision_engine_evaluate(project)?;
        let suite: crate::decision_engine::WorkspaceOptimalDecisionSuite =
            serde_json::from_str(&suite_json).map_err(|_| "SF-SYS-001".to_owned())?;
        let target_decision = suite
            .recommended_decisions
            .iter()
            .find(|d| d.decision_id == decision_id)
            .ok_or_else(|| "SF-SYS-001".to_owned())?;

        {
            let mut projects = self
                .projects
                .lock()
                .map_err(|_| UNKNOWN_PROJECT_CODE.to_owned())?;
            let graph = projects
                .get_mut(project)
                .ok_or_else(|| UNKNOWN_PROJECT_CODE.to_owned())?;
            let mut art_map = self.artifacts.lock().map_err(|_| "SF-SYS-001".to_owned())?;
            let artifacts = art_map.entry(project.to_owned()).or_default();
            let _ = crate::decision_engine::execute_recommended_decision_action(
                graph,
                artifacts,
                &target_decision.executable_action,
                now_ms,
            )?;
        }

        self.state_unlocked(project)
    }

    fn ensure_org_sync_loaded(&self) -> Result<Arc<crate::org_sync::OrgSyncStore>, String> {
        let key = self.ensure_local_vault_key()?;
        let mut slot = self.org_sync.lock().map_err(|_| "SF-SYS-001".to_owned())?;
        if let Some(ref existing) = *slot {
            return Ok(existing.clone());
        }
        let dir = self.resolve_storage_dir()?;
        let default_dev_id = self
            .active_session
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|s| s.installation_id.clone()))
            .unwrap_or_default();
        let store = Arc::new(crate::org_sync::OrgSyncStore::open_or_init(
            dir,
            key,
            &default_dev_id,
            "SafiData Desktop",
        )?);
        *slot = Some(store.clone());
        Ok(store)
    }

    /// Return current Organization & E2EE Sync status for `workspace_id` (or active workspace).
    pub fn desktop_sync_status(&self, workspace_id: Option<&str>) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(("workspaceId", workspace_id));
        self.gate_command("desktop_sync_status", payload_len, None)?;
        let sync = self.ensure_org_sync_loaded()?;
        let status = sync.status_for_workspace(workspace_id)?;
        serde_json::to_string(&status).map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Switch active Desktop workspace between Personal (`None`) and Shared Team Workspace.
    pub fn desktop_sync_select_workspace(
        &self,
        organization_id: Option<&str>,
        workspace_id: Option<&str>,
        active_key_epoch: u32,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(
            ("organizationId", organization_id),
            ("workspaceId", workspace_id),
            ("activeKeyEpoch", active_key_epoch)
        );
        self.gate_command("desktop_sync_select_workspace", payload_len, None)?;
        let sync = self.ensure_org_sync_loaded()?;
        let status =
            sync.select_active_workspace(organization_id, workspace_id, active_key_epoch)?;
        serde_json::to_string(&status).map_err(|_| "SF-SYS-001".to_owned())
    }

    /// List decrypted shared resources stored in the local encrypted vault for `workspace_id`.
    pub fn desktop_sync_list_resources(
        &self,
        workspace_id: &str,
        include_deleted: bool,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(
            ("workspaceId", workspace_id),
            ("includeDeleted", include_deleted)
        );
        self.gate_command("desktop_sync_list_resources", payload_len, None)?;
        let sync = self.ensure_org_sync_loaded()?;
        let resources = sync.list_shared_resources(workspace_id, include_deleted)?;
        serde_json::to_string(&resources).map_err(|_| "SF-SYS-001".to_owned())
    }

    /// List unresolved E2EE sync conflicts for `workspace_id`.
    pub fn desktop_sync_list_conflicts(&self, workspace_id: &str) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(("workspaceId", workspace_id));
        self.gate_command("desktop_sync_list_conflicts", payload_len, None)?;
        let sync = self.ensure_org_sync_loaded()?;
        let conflicts = sync.list_conflicts(workspace_id)?;
        serde_json::to_string(&conflicts).map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Rotate this installation's local X25519 device keypair.
    pub fn desktop_sync_rotate_device_key(
        &self,
        new_device_id: Option<&str>,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(("newDeviceId", new_device_id));
        self.gate_command("desktop_sync_rotate_device_key", payload_len, Some(now_ms))?;
        let sync = self.ensure_org_sync_loaded()?;
        let pub_id = sync.rotate_local_device_keypair(new_device_id)?;
        serde_json::to_string(&pub_id).map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Bootstrap/wrap the active epoch's 256-bit Workspace Key for authorized member devices.
    pub fn desktop_sync_wrap_epoch_keys(
        &self,
        organization_id: &str,
        workspace_id: &str,
        key_epoch: u32,
        devices_json: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(
            ("organizationId", organization_id),
            ("workspaceId", workspace_id),
            ("keyEpoch", key_epoch),
            ("devicesJson", devices_json)
        );
        self.gate_command("desktop_sync_wrap_epoch_keys", payload_len, Some(now_ms))?;
        let devices: Vec<crate::org_sync::RemoteMemberDeviceKey> =
            serde_json::from_str(devices_json).map_err(|_| "SF-AUTH-006".to_owned())?;
        let sync = self.ensure_org_sync_loaded()?;
        let wrapped = sync.ensure_and_wrap_epoch_key_for_devices(
            organization_id,
            workspace_id,
            key_epoch,
            &devices,
        )?;
        serde_json::to_string(&wrapped).map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Generate a rotated `current_epoch + 1` Workspace Key and wrap it ONLY for remaining devices.
    pub fn desktop_sync_prepare_key_rotation(
        &self,
        organization_id: &str,
        workspace_id: &str,
        current_epoch: u32,
        remaining_devices_json: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(
            ("organizationId", organization_id),
            ("workspaceId", workspace_id),
            ("currentEpoch", current_epoch),
            ("remainingDevicesJson", remaining_devices_json)
        );
        self.gate_command(
            "desktop_sync_prepare_key_rotation",
            payload_len,
            Some(now_ms),
        )?;
        let devices: Vec<crate::org_sync::RemoteMemberDeviceKey> =
            serde_json::from_str(remaining_devices_json).map_err(|_| "SF-AUTH-006".to_owned())?;
        let sync = self.ensure_org_sync_loaded()?;
        let (next_epoch, entries) = sync.prepare_rotated_workspace_key(
            organization_id,
            workspace_id,
            current_epoch,
            &devices,
        )?;
        serde_json::to_string(&serde_json::json!({
            "nextKeyEpoch": next_epoch,
            "entries": entries
        }))
        .map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Unwrap wrapped Workspace Keys fetched from `/v1/organizations/{id}/keys/my-wrapped`.
    pub fn desktop_sync_ingest_wrapped_keys(
        &self,
        envelopes_json: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(("envelopesJson", envelopes_json));
        self.gate_command(
            "desktop_sync_ingest_wrapped_keys",
            payload_len,
            Some(now_ms),
        )?;
        let envelopes: Vec<crate::org_sync::RemoteWrappedKeyEnvelope> =
            serde_json::from_str(envelopes_json).map_err(|_| "SF-SYN-001".to_owned())?;
        let sync = self.ensure_org_sync_loaded()?;
        let unlocked = sync.ingest_wrapped_workspace_keys(&envelopes)?;
        serde_json::to_string(&serde_json::json!({ "unlockedCount": unlocked }))
            .map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Stage a local shared resource upsert into the encrypted offline queue.
    #[allow(clippy::too_many_arguments)]
    pub fn desktop_sync_stage_upsert(
        &self,
        organization_id: &str,
        workspace_id: &str,
        resource_kind: &str,
        resource_id: &str,
        title: &str,
        payload_json: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(
            ("organizationId", organization_id),
            ("workspaceId", workspace_id),
            ("resourceKind", resource_kind),
            ("resourceId", resource_id),
            ("title", title),
            ("payloadJson", payload_json)
        );
        self.gate_command("desktop_sync_stage_upsert", payload_len, Some(now_ms))?;
        let sync = self.ensure_org_sync_loaded()?;
        let staged = sync.stage_shared_resource_upsert(
            organization_id,
            workspace_id,
            resource_kind,
            resource_id,
            title,
            payload_json,
        )?;
        serde_json::to_string(&staged).map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Stage a deletion tombstone (`deleted = true`) into the encrypted offline queue.
    pub fn desktop_sync_stage_delete(
        &self,
        organization_id: &str,
        workspace_id: &str,
        resource_kind: &str,
        resource_id: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(
            ("organizationId", organization_id),
            ("workspaceId", workspace_id),
            ("resourceKind", resource_kind),
            ("resourceId", resource_id)
        );
        self.gate_command("desktop_sync_stage_delete", payload_len, Some(now_ms))?;
        let sync = self.ensure_org_sync_loaded()?;
        let staged = sync.stage_shared_resource_delete(
            organization_id,
            workspace_id,
            resource_kind,
            resource_id,
        )?;
        serde_json::to_string(&staged).map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Seal and return the next queued encrypted envelope for `workspace_id` (`null` if queue empty).
    pub fn desktop_sync_peek_outbound(
        &self,
        workspace_id: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(("workspaceId", workspace_id));
        self.gate_command("desktop_sync_peek_outbound", payload_len, Some(now_ms))?;
        let sync = self.ensure_org_sync_loaded()?;
        let next = sync.peek_next_outbound_envelope(workspace_id)?;
        serde_json::to_string(&next).map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Record outcome (`committed`, `conflict`, or `error`) of an outbound encrypted push.
    pub fn desktop_sync_complete_outbound(
        &self,
        outcome_json: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(("outcomeJson", outcome_json));
        self.gate_command("desktop_sync_complete_outbound", payload_len, Some(now_ms))?;
        let parsed: serde_json::Value =
            serde_json::from_str(outcome_json).map_err(|_| "SF-AUTH-006".to_owned())?;
        let kind = parsed
            .get("outcome")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let sync = self.ensure_org_sync_loaded()?;
        match kind {
            "committed" => {
                let idem = parsed
                    .get("idempotencyKey")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let ver = parsed
                    .get("version")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(1);
                let seq = parsed
                    .get("workspaceSeq")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(1);
                let author = parsed
                    .get("authorUserId")
                    .and_then(serde_json::Value::as_str);
                sync.mark_outbound_pushed(idem, ver, seq, author)?;
                Ok(r#"{"status":"committed"}"#.to_owned())
            }
            "conflict" => {
                let idem = parsed
                    .get("idempotencyKey")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let remote_val = parsed
                    .get("remoteEnvelope")
                    .cloned()
                    .ok_or_else(|| "SF-SYN-001".to_owned())?;
                let remote_env: crate::org_sync::InboundEncryptedEnvelope =
                    serde_json::from_value(remote_val).map_err(|_| "SF-SYN-001".to_owned())?;
                let conflict = sync.record_push_conflict(idem, &remote_env)?;
                serde_json::to_string(&conflict).map_err(|_| "SF-SYS-001".to_owned())
            }
            "error" => {
                let ws_id = parsed
                    .get("workspaceId")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                let code = parsed
                    .get("errorCode")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("SF-AUTH-005");
                sync.record_sync_error(ws_id, code)?;
                Ok(r#"{"status":"error_recorded"}"#.to_owned())
            }
            _ => Err("SF-AUTH-006".to_owned()),
        }
    }

    /// Verify BLAKE3 integrity, decrypt, and apply pulled encrypted envelopes for `workspace_id`.
    pub fn desktop_sync_apply_pulled(
        &self,
        workspace_id: &str,
        envelopes_json: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(
            ("workspaceId", workspace_id),
            ("envelopesJson", envelopes_json)
        );
        self.gate_command("desktop_sync_apply_pulled", payload_len, Some(now_ms))?;
        let envelopes: Vec<crate::org_sync::InboundEncryptedEnvelope> =
            serde_json::from_str(envelopes_json).map_err(|_| "SF-SYN-001".to_owned())?;
        let sync = self.ensure_org_sync_loaded()?;
        let applied = sync.apply_pulled_changes(workspace_id, &envelopes)?;
        let status = sync.status_for_workspace(Some(workspace_id))?;
        serde_json::to_string(&serde_json::json!({
            "appliedCount": applied,
            "status": status
        }))
        .map_err(|_| "SF-SYS-001".to_owned())
    }

    /// Resolve a detected conflict (`keep_local` vs `accept_remote`).
    pub fn desktop_sync_resolve_conflict(
        &self,
        workspace_id: &str,
        resource_kind: &str,
        resource_id: &str,
        choice: &str,
        now_ms: u64,
    ) -> Result<String, String> {
        let payload_len = crate::ipc_payload_len!(
            ("workspaceId", workspace_id),
            ("resourceKind", resource_kind),
            ("resourceId", resource_id),
            ("choice", choice)
        );
        self.gate_command("desktop_sync_resolve_conflict", payload_len, Some(now_ms))?;
        let parsed_choice = match choice.trim() {
            "keep_local" => crate::org_sync::ConflictResolutionChoice::KeepLocal,
            "accept_remote" => crate::org_sync::ConflictResolutionChoice::AcceptRemote,
            _ => return Err("SF-AUTH-006".to_owned()),
        };
        let sync = self.ensure_org_sync_loaded()?;
        let resolved =
            sync.resolve_conflict(workspace_id, resource_kind, resource_id, parsed_choice)?;
        serde_json::to_string(&resolved).map_err(|_| "SF-SYS-001".to_owned())
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

/// Reads any supported workbook or delimited file via the Universal Workbook Intelligence Engine
/// and selects its primary analytical table `(report, primary_table, sheet_name, table_id)`.
fn ingest_workbook_for_desktop(
    path: &Path,
) -> Result<(WorkbookIngestionReport, Table, String, String), String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| "SF-IMP-002".to_owned())?;
    if !matches!(
        extension.as_str(),
        "csv" | "tsv" | "xlsx" | "xlsm" | "xlsb" | "xls" | "ods"
    ) {
        return Err("SF-IMP-002".to_owned());
    }
    let metadata = fs::metadata(path).map_err(|_| "SF-IMP-001".to_owned())?;
    if metadata.len() > MAX_STUDIO_IMPORT_BYTES {
        return Err("SF-IMP-003".to_owned());
    }
    let bytes = fs::read(path).map_err(|_| "SF-IMP-001".to_owned())?;
    let hint = path.file_name().and_then(|value| value.to_str());
    let report =
        ingest_workbook_universal(Cursor::new(bytes), PlatformLimits::DESKTOP_CLI, hint, None)
            .map_err(|error| code_of(&error))?;
    let primary_region = report
        .worksheets
        .iter()
        .filter(|ws| ws.visibility == SheetVisibility::Visible)
        .flat_map(|ws| ws.tables.iter())
        .max_by_key(|t| (t.data_rows.len(), t.headers.len()))
        .or_else(|| {
            report
                .worksheets
                .iter()
                .flat_map(|ws| ws.tables.iter())
                .max_by_key(|t| (t.data_rows.len(), t.headers.len()))
        })
        .ok_or_else(|| "SF-IMP-001".to_owned())?;
    let table = primary_region
        .to_core_table()
        .map_err(|error| code_of(&error))?;
    Ok((
        report.clone(),
        table,
        primary_region.sheet_name.clone(),
        primary_region.table_id.clone(),
    ))
}

fn workbook_summary_json(
    report: Option<&WorkbookIngestionReport>,
    active_region: Option<&(String, String)>,
) -> serde_json::Value {
    let Some(rep) = report else {
        return serde_json::Value::Null;
    };
    let sheets: Vec<serde_json::Value> = rep
        .worksheets
        .iter()
        .map(|ws| {
            let tables: Vec<serde_json::Value> = ws
                .tables
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "tableId": t.table_id,
                        "title": t.title,
                        "sheetName": t.sheet_name,
                        "headerCount": t.headers.len(),
                        "rowCount": t.data_rows.len(),
                        "separatedRowCount": t.separated_rows.len(),
                        "confidencePct": t.confidence_pct,
                    })
                })
                .collect();
            serde_json::json!({
                "name": ws.name,
                "index": ws.index,
                "visibility": format!("{:?}", ws.visibility),
                "classification": format!("{:?}", ws.classification),
                "classificationConfidencePct": ws.classification_confidence_pct,
                "tables": tables,
                "formulaCount": ws.formula_count,
                "verifiedFormulaCount": ws.verified_formula_count,
            })
        })
        .collect();
    serde_json::json!({
        "detectedFormat": format!("{:?}", rep.detected_format),
        "supportLevel": format!("{:?}", rep.support_level),
        "dateSystem": format!("{:?}", rep.date_system),
        "macrosNeutralized": rep.macros_neutralized,
        "totalSheets": rep.total_sheets,
        "visibleSheets": rep.visible_sheets,
        "hiddenSheets": rep.hidden_sheets,
        "totalTablesExtracted": rep.total_tables_extracted,
        "totalAnalyticalRows": rep.total_analytical_rows,
        "totalFormulasDetected": rep.total_formulas_detected,
        "totalFormulasVerified": rep.total_formulas_verified,
        "warningsCount": rep.warnings.len(),
        "activeSheet": active_region.map(|r| r.0.as_str()),
        "activeTableId": active_region.map(|r| r.1.as_str()),
        "sheets": sheets,
    })
}

fn workbook_details_json(
    graph: &DataGraph,
    report: Option<&WorkbookIngestionReport>,
    active_region: Option<&(String, String)>,
) -> serde_json::Value {
    let Some(rep) = report else {
        let headers: Vec<String> = graph
            .working()
            .columns()
            .iter()
            .map(|c| c.schema().name.clone())
            .collect();
        return serde_json::json!({
            "detectedFormat": "Unknown",
            "supportLevel": "FullSupport",
            "dateSystem": "Standard1900",
            "macrosNeutralized": false,
            "totalSheets": 1,
            "visibleSheets": 1,
            "hiddenSheets": 0,
            "totalTablesExtracted": 1,
            "totalAnalyticalRows": graph.working().row_count(),
            "totalFormulasDetected": 0,
            "totalFormulasVerified": 0,
            "activeSheet": "Sheet1",
            "activeTableId": "tbl_1",
            "definedNames": [],
            "embeddedObjects": [],
            "warnings": [],
            "worksheets": [{
                "name": "Sheet1",
                "index": 0,
                "visibility": "Visible",
                "classification": "MainDataset",
                "classificationConfidencePct": 90,
                "classificationReason": "Restored project snapshot",
                "rawRowCount": graph.working().row_count() + 1,
                "rawColCount": headers.len(),
                "mergedRanges": [],
                "hasFrozenPanes": false,
                "isProtected": false,
                "formulaCount": 0,
                "verifiedFormulaCount": 0,
                "tables": [{
                    "tableId": "tbl_1",
                    "title": graph.working().name(),
                    "sheetName": "Sheet1",
                    "headers": headers,
                    "rowCount": graph.working().row_count(),
                    "separatedRows": [],
                    "confidencePct": 95,
                }],
                "formulaSamples": [],
            }],
        });
    };

    let worksheets: Vec<serde_json::Value> = rep
        .worksheets
        .iter()
        .map(|ws| {
            let tables: Vec<serde_json::Value> = ws
                .tables
                .iter()
                .map(|t| {
                    let separated: Vec<serde_json::Value> = t
                        .separated_rows
                        .iter()
                        .take(50)
                        .map(|sr| {
                            serde_json::json!({
                                "sheetRow": sr.sheet_row,
                                "kind": format!("{:?}", sr.kind),
                                "cells": sr.values,
                            })
                        })
                        .collect();
                    serde_json::json!({
                        "tableId": t.table_id,
                        "title": t.title,
                        "sheetName": t.sheet_name,
                        "headers": t.headers,
                        "rowCount": t.data_rows.len(),
                        "separatedRows": separated,
                        "confidencePct": t.confidence_pct,
                    })
                })
                .collect();

            let mut unsupported_count = 0usize;
            let mut error_count = 0usize;
            let mut volatile_count = 0usize;
            let mut external_count = 0usize;
            let mut formula_samples = Vec::new();
            for cell in &ws.cell_metadata {
                match cell.formula_status {
                    FormulaVerificationStatus::UnsupportedFormula => unsupported_count += 1,
                    FormulaVerificationStatus::FormulaError => error_count += 1,
                    FormulaVerificationStatus::VolatileFormula => volatile_count += 1,
                    FormulaVerificationStatus::ExternalWorkbookReference => external_count += 1,
                    _ => {}
                }
                let interesting =
                    cell.formula.is_some() || cell.comment.is_some() || cell.hyperlink.is_some();
                if interesting && formula_samples.len() < 120 {
                    formula_samples.push(serde_json::json!({
                        "address": cell.address,
                        "row": cell.row,
                        "col": cell.col,
                        "rawValue": cell.raw_value,
                        "formula": cell.formula,
                        "cachedValue": cell.cached_value,
                        "recalculatedValue": cell.recalculated_value,
                        "formulaStatus": format!("{:?}", cell.formula_status),
                        "dataType": format!("{:?}", cell.data_type),
                        "numberFormat": cell.number_format,
                        "comment": cell.comment,
                        "hyperlink": cell.hyperlink,
                    }));
                }
            }

            serde_json::json!({
                "name": ws.name,
                "index": ws.index,
                "visibility": format!("{:?}", ws.visibility),
                "classification": format!("{:?}", ws.classification),
                "classificationConfidencePct": ws.classification_confidence_pct,
                "classificationReason": ws.classification_reason,
                "rawRowCount": ws.raw_row_count,
                "rawColCount": ws.raw_col_count,
                "mergedRanges": ws.merged_ranges,
                "hasFrozenPanes": ws.has_frozen_panes,
                "autoFilterRange": ws.auto_filter_range,
                "printArea": ws.print_area,
                "isProtected": ws.is_protected,
                "formulaCount": ws.formula_count,
                "verifiedFormulaCount": ws.verified_formula_count,
                "unsupportedFormulaCount": unsupported_count,
                "formulaErrorCount": error_count,
                "volatileFormulaCount": volatile_count,
                "externalRefCount": external_count,
                "tables": tables,
                "formulaSamples": formula_samples,
            })
        })
        .collect();

    let defined_names: Vec<serde_json::Value> = rep
        .defined_names
        .iter()
        .map(|dn| {
            serde_json::json!({
                "name": dn.name,
                "refersTo": dn.refers_to,
                "isBuiltIn": dn.is_built_in,
            })
        })
        .collect();

    let embedded_objects: Vec<serde_json::Value> = rep
        .embedded_objects
        .iter()
        .map(|obj| {
            serde_json::json!({
                "kind": format!("{:?}", obj.kind),
                "location": obj.location,
                "handling": format!("{:?}", obj.handling),
                "summary": obj.summary,
            })
        })
        .collect();

    serde_json::json!({
        "detectedFormat": format!("{:?}", rep.detected_format),
        "supportLevel": format!("{:?}", rep.support_level),
        "dateSystem": format!("{:?}", rep.date_system),
        "macrosNeutralized": rep.macros_neutralized,
        "totalSheets": rep.total_sheets,
        "visibleSheets": rep.visible_sheets,
        "hiddenSheets": rep.hidden_sheets,
        "totalTablesExtracted": rep.total_tables_extracted,
        "totalAnalyticalRows": rep.total_analytical_rows,
        "totalFormulasDetected": rep.total_formulas_detected,
        "totalFormulasVerified": rep.total_formulas_verified,
        "activeSheet": active_region.map(|r| r.0.as_str()),
        "activeTableId": active_region.map(|r| r.1.as_str()),
        "definedNames": defined_names,
        "embeddedObjects": embedded_objects,
        "warnings": rep.warnings,
        "worksheets": worksheets,
    })
}

/// Serializes the metadata & provenance state the desktop UI renders without dumping all rows.
fn state_json(
    graph: &DataGraph,
    artifacts: &StudioWorkspaceArtifacts,
    report: Option<&WorkbookIngestionReport>,
    active_region: Option<&(String, String)>,
) -> Result<String, String> {
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
        "workbookSummary": workbook_summary_json(report, active_region),
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
    write_atomic_bytes(target, contents.as_bytes())
}

fn write_atomic_bytes(target: &Path, contents: &[u8]) -> Result<(), String> {
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
        file.write_all(contents)?;
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

fn parse_conflict_policy(raw: &str) -> NameConflictPolicy {
    match raw.trim().to_ascii_lowercase().as_str() {
        "rename" => NameConflictPolicy::Rename,
        "skip" => NameConflictPolicy::Skip,
        "confirmed_replace" => NameConflictPolicy::ConfirmedReplace,
        _ => NameConflictPolicy::FailOnDuplicate,
    }
}

fn parse_recipe_steps_flexible(
    value: &serde_json::Value,
) -> Option<Vec<sf_recipe::recipe::RecipeStep>> {
    if let Ok(canonical) =
        serde_json::from_value::<Vec<sf_recipe::recipe::RecipeStep>>(value.clone())
    {
        return Some(canonical);
    }
    let arr = value.as_array()?;
    let mut out = Vec::with_capacity(arr.len());
    for (idx, item) in arr.iter().enumerate() {
        if let Ok(single) = serde_json::from_value::<sf_recipe::recipe::RecipeStep>(item.clone()) {
            out.push(single);
            continue;
        }
        let id = item
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("step_{}", idx + 1));
        let op_field = item.get("op")?;
        let (op_type, nested_cols) = if let Some(s) = op_field.as_str() {
            (s, None)
        } else if let Some(obj) = op_field.as_object() {
            let t = obj
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("normalize_text");
            let c: Option<Vec<String>> = obj
                .get("columns")
                .and_then(|v| serde_json::from_value(v.clone()).ok());
            (t, c)
        } else {
            ("normalize_text", None)
        };
        let mapped_op = match op_type {
            "dedupe" | "deduplicate_rows" => sf_recipe::recipe::RecipeOperation::Dedupe,
            "fill_blank" => sf_recipe::recipe::RecipeOperation::FillBlank,
            "parse_date" => sf_recipe::recipe::RecipeOperation::ParseDate,
            "parse_number" => sf_recipe::recipe::RecipeOperation::ParseNumber,
            "normalize_phone" => sf_recipe::recipe::RecipeOperation::NormalizePhone,
            "rename" | "rename_column" => sf_recipe::recipe::RecipeOperation::Rename,
            "split" => sf_recipe::recipe::RecipeOperation::Split,
            "merge" => sf_recipe::recipe::RecipeOperation::Merge,
            _ => sf_recipe::recipe::RecipeOperation::NormalizeText,
        };
        let columns: Option<Vec<String>> = item
            .get("columns")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .or(nested_cols);
        let params = item
            .get("params")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({ "trim": true }));
        let confidence = item.get("confidence").and_then(serde_json::Value::as_f64);
        let reversible = item
            .get("reversible")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true);
        out.push(sf_recipe::recipe::RecipeStep {
            id,
            op: mapped_op,
            columns,
            params,
            confidence,
            reversible,
        });
    }
    Some(out)
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

        // Tamper encrypted bundle on disk -> SF-SYS-024 AEAD tamper rejection (and no plaintext .safi.json exists!)
        assert!(!storage.join("q4_board.safi.json").exists());
        let saved_path = storage.join("q4_board.safi.enc");
        let mut corrupted = test_result(fs::read(&saved_path));
        if let Some(last) = corrupted.last_mut() {
            *last ^= 0x5A;
        }
        test_result(fs::write(&saved_path, corrupted));
        assert_eq!(
            store.load_project_bundle("p3", "q4_board", 4_000),
            Err("SF-SYS-024".to_owned())
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

    #[test]
    fn all_21_chart_types_and_region_switching_and_deliverable_export_work() {
        let directory = test_result(tempfile::tempdir());
        let storage = directory.path().join("projects");
        let store = authenticated_store_with_dir(storage);
        let csv_path = directory.path().join("metrics21.csv");
        test_result(fs::write(
            &csv_path,
            "region,amount,units\nCairo,100,10\nGiza,200,20\nAlex,300,30\nCairo,150,15\n",
        ));
        let opened = json(&test_result(store.open_file("p21", &csv_path, 1_000)));
        assert_eq!(
            opened["workbookSummary"]["totalSheets"],
            serde_json::json!(1)
        );

        for kind in [
            "bar",
            "column",
            "line",
            "area",
            "stacked",
            "combo",
            "pie",
            "donut",
            "scatter",
            "bubble",
            "histogram",
            "boxplot",
            "heatmap",
            "treemap",
            "waterfall",
            "funnel",
            "gauge",
            "kpi",
            "table",
            "pivotsummary",
            "geographic",
        ] {
            let group_col = if kind == "scatter" || kind == "bubble" {
                "units"
            } else {
                "region"
            };
            let func = if kind == "histogram" { "COUNT" } else { "SUM" };
            let raw = test_result(store.chart_data("p21", kind, func, "amount", group_col));
            let parsed = json(&raw);
            assert_eq!(parsed["chartType"], serde_json::json!(kind));
            assert!(
                parsed["points"]
                    .as_array()
                    .is_some_and(|pts| !pts.is_empty()),
                "chart {kind} must return computed points"
            );
        }

        let details = json(&test_result(store.workbook_details("p21")));
        assert_eq!(details["detectedFormat"], serde_json::json!("Csv"));
        assert_eq!(details["totalTablesExtracted"], serde_json::json!(1));

        let out_bin = directory.path().join("export.pdf");
        let payload_b64 = BASE64_STANDARD.encode(b"%PDF-1.7\n1 0 obj\n<<>>\nendobj\n");
        test_result(store.export_deliverable("p21", &out_bin, true, &payload_b64));
        let written = test_result(fs::read(&out_bin));
        assert!(written.starts_with(b"%PDF-1.7"));
    }

    #[derive(Debug)]
    struct ToggleOfflineVerifier {
        authority: Arc<StudioSessionAuthority>,
        offline: Mutex<bool>,
    }

    impl StudioSessionVerifier for ToggleOfflineVerifier {
        fn verify_session(
            &self,
            token: &str,
            installation_id: &str,
            now_ms: u64,
        ) -> Result<VerifiedStudioSession, String> {
            let is_offline = self.offline.lock().map(|g| *g).unwrap_or(false);
            if is_offline {
                return Err(AUTH_BACKEND_UNAVAILABLE_CODE.to_owned());
            }
            self.authority
                .verify_session(token, installation_id, now_ms)
        }
    }

    #[test]
    fn offline_readonly_preserves_saved_projects_blocks_protected_ops_and_revalidates_on_reconnect()
    {
        let dir = test_result(tempfile::tempdir());
        let storage = dir.path().join("projects");
        let csv_path = dir.path().join("sales.csv");
        let capsule_path = dir.path().join("out.safi.html");
        let export_path = dir.path().join("out.csv");
        test_result(fs::write(&csv_path, CSV));

        let authority = Arc::new(StudioSessionAuthority::new());
        let verifier = Arc::new(ToggleOfflineVerifier {
            authority: authority.clone(),
            offline: Mutex::new(false),
        });
        let store = StudioStore::with_storage_dir_and_verifier(
            storage.clone(),
            authority.clone(),
            verifier.clone(),
        );
        store.set_clock_ms(Some(10_000));

        // 1. Online: sign in, import dataset, save project locally with encrypted vault key.
        let salt = [0x99u8; 16];
        let vault_key = test_result(VaultKey::derive_from_passphrase(
            "local-project-vault-key",
            &salt,
        ));
        store.set_local_vault_key(Some(vault_key));
        test_result(authority.register_session(
            "sess_offline_1",
            "ws_offline",
            VALID_TOKEN,
            INSTALLATION_ID,
            200_000,
        ));
        test_result(store.set_auth_session(VALID_TOKEN, INSTALLATION_ID, 10_000));
        test_result(store.open_file("p_saved", &csv_path, 10_500));
        let saved_meta = json(&test_result(
            store.save_project_bundle("p_saved", "q1_sales", "Q1 Sales", 11_000),
        ));
        assert_eq!(saved_meta["encrypted"], serde_json::json!(true));
        // Prove zero plaintext exists on disk in encrypted `.safi.enc`.
        let enc_bytes = test_result(fs::read(storage.join("q1_sales.safi.enc")));
        assert!(!enc_bytes.windows(5).any(|w| w == b"Cairo"));

        // Leave a simulated crash `.tmp` file in storage dir; it must never break loading.
        test_result(fs::write(
            storage.join("q1_sales.safi.enc.tmp"),
            b"corrupt-partial-write",
        ));

        // 2. Go offline: enter Offline Read-Only Mode.
        if let Ok(mut flag) = verifier.offline.lock() {
            *flag = true;
        }
        let status_off = json(&test_result(store.enter_offline_read_only()));
        assert_eq!(status_off["mode"], serde_json::json!("offline_read_only"));
        assert_eq!(
            status_off["canExecuteProtectedOps"],
            serde_json::json!(false)
        );

        // 3. Offline Read-Only: customer CAN list, load, view state, and paginate saved local project.
        let listed = json(&test_result(store.list_project_bundles()));
        assert_eq!(listed.as_array().map(Vec::len), Some(1));
        let loaded = json(&test_result(
            store.load_project_bundle("p_ro", "q1_sales", 12_000),
        ));
        assert_eq!(loaded["rowCount"], serde_json::json!(3));
        let st = json(&test_result(store.state("p_ro")));
        assert_eq!(st["rowCount"], serde_json::json!(3));
        let page = json(&test_result(store.rows_page("p_ro", "{}")));
        assert_eq!(page["rows"].as_array().map(Vec::len), Some(3));

        // 4. Offline Read-Only: EVERY protected operation MUST fail closed with SF-AUTH-021!
        for (name, res) in [
            ("open_file", store.open_file("p_ro", &csv_path, 12_500)),
            (
                "set_cell",
                store.set_cell("p_ro", 0, "amount", "999", 12_500),
            ),
            (
                "define_metric",
                store.define_metric("p_ro", "m_off", "Off", None, "SUM(amount)", 12_500),
            ),
            ("grouped", store.grouped("p_ro", "SUM", "amount", "region")),
            (
                "chart_data",
                store.chart_data("p_ro", "bar", "SUM", "amount", "region"),
            ),
            ("narrative", store.narrative("p_ro", "Summary")),
            ("set_artifacts", store.set_artifacts("p_ro", "{}")),
            (
                "save_project",
                store.save_project_bundle("p_ro", "q1_sales", "Mutated", 12_500),
            ),
            ("delete_project", store.delete_project_bundle("q1_sales")),
            (
                "export_capsule",
                store.export_capsule("p_ro", &capsule_path, CAPSULE),
            ),
            (
                "export_deliverable",
                store.export_deliverable("p_ro", &export_path, false, "a,b\n1,2\n"),
            ),
        ] {
            assert_eq!(
                res,
                Err(AUTH_OFFLINE_READ_ONLY_CODE.to_owned()),
                "offline protected command {name} must fail closed with SF-AUTH-021"
            );
        }

        // Local saved project remains completely intact after blocked offline attempts.
        assert!(!export_path.exists());
        assert!(!capsule_path.exists());

        // 5. Reconnect with still-valid session -> revalidate restores online mode & protected ops.
        if let Ok(mut flag) = verifier.offline.lock() {
            *flag = false;
        }
        let status_back = json(&test_result(store.revalidate_session(15_000)));
        assert_eq!(status_back["mode"], serde_json::json!("online_authorized"));
        assert_eq!(
            status_back["canExecuteProtectedOps"],
            serde_json::json!(true)
        );
        test_result(store.set_cell("p_ro", 0, "amount", "125", 16_000));

        // 6. Local clock rollback attack (>5s backwards during active session) -> SF-AUTH-022!
        store.set_clock_ms(Some(50_000));
        test_result(store.state("p_ro"));
        store.set_clock_ms(Some(40_000)); // Rolled back 10s!
        assert_eq!(
            store.set_cell("p_ro", 0, "amount", "200", 40_000),
            Err(AUTH_LEASE_CLOCK_ROLLBACK_CODE.to_owned())
        );

        // 7. Go offline again, have Device B take over account on server, then reconnect ->
        //    Device A MUST be rejected on reconnect and wiped cleanly!
        store.set_clock_ms(Some(55_000));
        if let Ok(mut flag) = verifier.offline.lock() {
            *flag = true;
        }
        test_result(store.enter_offline_read_only());
        test_result(authority.register_session(
            "sess_device_b",
            "ws_offline",
            SECOND_DEVICE_TOKEN,
            OTHER_INSTALLATION_ID,
            200_000,
        ));
        if let Ok(mut flag) = verifier.offline.lock() {
            *flag = false;
        }
        assert_eq!(
            store.revalidate_session(60_000),
            Err(AUTH_REVOKED_CODE.to_owned())
        );
        let status_revoked = json(&test_result(store.authorization_status()));
        assert_eq!(status_revoked["mode"], serde_json::json!("unauthenticated"));
    }

    #[test]
    fn fresh_unauthenticated_install_cannot_bypass_login_into_offline_read_only_mode() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!(
            "safi-fresh-no-offline-bypass-{}-{nonce}",
            std::process::id()
        ));
        test_result(fs::create_dir_all(&dir));
        let fresh_store = StudioStore::with_storage_dir(dir.clone());

        // Fresh unauthenticated installation has hasPreviouslyAuthenticated = false
        let initial_status = json(&test_result(fresh_store.authorization_status()));
        assert_eq!(initial_status["mode"], serde_json::json!("unauthenticated"));
        assert_eq!(
            initial_status["hasPreviouslyAuthenticated"],
            serde_json::json!(false)
        );
        // Attempting to bypass login via Offline Read-Only Mode MUST fail closed with SF-AUTH-001!
        assert_eq!(
            fresh_store.enter_offline_read_only(),
            Err(AUTH_REQUIRED_CODE.to_owned())
        );

        // Once the user completes a verified online login once, prior_auth.marker is persisted.
        test_result(fresh_store.session_authority().register_session(
            "sess_first_login",
            "ws_fresh",
            VALID_TOKEN,
            INSTALLATION_ID,
            200_000,
        ));
        test_result(fresh_store.set_auth_session(VALID_TOKEN, INSTALLATION_ID, 10_000));
        assert!(dir.join("prior_auth.marker").is_file());

        // After restarting Desktop offline, the previously authenticated user CAN inspect saved local projects read-only!
        let restarted_store = StudioStore::with_storage_dir(dir.clone());
        let restarted_status = json(&test_result(restarted_store.authorization_status()));
        assert_eq!(
            restarted_status["hasPreviouslyAuthenticated"],
            serde_json::json!(true)
        );
        let offline_status = json(&test_result(restarted_store.enter_offline_read_only()));
        assert_eq!(
            offline_status["mode"],
            serde_json::json!("offline_read_only")
        );
        assert_eq!(
            offline_status["canExecuteProtectedOps"],
            serde_json::json!(false)
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn studio_store_writes_only_encrypted_files_and_atomic_workflow_replay_rolls_back_on_error() {
        let dir = test_result(tempfile::tempdir());
        let storage = dir.path().join("prod-storage");
        let csv_path = dir.path().join("sales_nov.csv");
        test_result(fs::write(
            &csv_path,
            "sale_date,region,customer,amount\n2026-11-01,  Cairo  ,ACME,100\n2026-11-02,Giza,Beta,250\n",
        ));

        // 1. Production StudioStore initializes VaultKey automatically and NEVER writes plaintext .json files.
        let store = authenticated_store_with_dir(storage.clone());
        test_result(store.open_file("p_enc", &csv_path, 1_000));
        // Explicitly set untrimmed whitespace in row 0 `region` so we can prove atomic rollback on failed replay!
        test_result(store.set_cell("p_enc", 0, "region", "  Cairo  ", 1_050));
        test_result(store.save_project_bundle("p_enc", "nov_sales", "Nov Sales", 1_100));
        test_result(store.desktop_workflow_memory_save_sheet(
            r#"{
                "displayName": "Valid Sales Workflow",
                "projectId": "p_enc",
                "sheetName": "Nov_Sales",
                "tableId": "tbl_1",
                "headers": ["sale_date", "region", "customer", "amount"],
                "cleaningOperations": [
                    {"stepId": "s1", "kind": "trim_column", "targetColumn": "region"}
                ],
                "metrics": [
                    {"id": "total_sales", "name": "Total Sales", "unit": "USD", "formula": "SUM(amount)"}
                ]
            }"#,
            1_200,
        ));
        test_result(store.desktop_learning_record_feedback(
            r#"{
                "signalKind": "explicit_acceptance",
                "category": "currency_unit",
                "scope": "matching_sheets",
                "businessDomain": "sales",
                "targetConcept": "sales_currency_amount",
                "preferredValue": "USD",
                "sourceSheetLabel": "Nov_Sales"
            }"#,
            1_250,
        ));
        let folders_json = test_result(serde_json::to_string(&vec![
            dir.path().to_string_lossy().into_owned(),
        ]));
        let _ =
            test_result(store.desktop_search_index_folders(&folders_json, Some("p_enc"), 1_300));
        let _ = test_result(store.desktop_recipes_list());

        // Inspect storage directory: ZERO .json files may exist!
        let entries: Vec<String> = test_result(fs::read_dir(&storage))
            .filter_map(Result::ok)
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        assert!(
            !entries.iter().any(|name| name.ends_with(".json")),
            "Production writes must NEVER create plaintext .json files, found: {entries:?}"
        );
        assert!(storage.join("nov_sales.safi.enc").is_file());
        assert!(storage.join("workflow_memory.safi.enc").is_file());
        assert!(storage.join("learning_profile.safi.enc").is_file());
        assert!(storage.join("local_search_index.safi.enc").is_file());

        // 2. Atomic workflow replay: a workflow with a valid op followed by an unsupported op MUST fail closed
        //    with SF-RCP-001 and leave `region` untrimmed (zero partial mutation!).
        let bad_wf_resp = json(&test_result(store.desktop_workflow_memory_save_sheet(
            r#"{
                "workflowId": "wfm_broken_op_test",
                "displayName": "Broken Op Workflow",
                "projectId": "p_enc",
                "sheetName": "Nov_Sales",
                "tableId": "tbl_1",
                "headers": ["sale_date", "region", "customer", "amount"],
                "cleaningOperations": [
                    {"stepId": "s1", "kind": "trim_column", "targetColumn": "region"},
                    {"stepId": "s2", "kind": "unsupported_magic_op", "targetColumn": "amount"}
                ],
                "metrics": []
            }"#,
            1_400,
        )));
        let bad_wf_id = bad_wf_resp["saved"]["workflowId"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let bad_replay = store.desktop_workflow_memory_apply_replay("p_enc", &bad_wf_id, 1_500);
        assert_eq!(bad_replay, Err("SF-RCP-001".to_owned()));
        // Verify row 0 `region` is STILL untrimmed `"  Cairo  "` (atomic rollback!).
        let page_after_failed = json(&test_result(store.rows_page("p_enc", "{}")));
        assert_eq!(
            page_after_failed["rows"][0]["cells"][1]["visible"],
            serde_json::json!("  Cairo  ")
        );

        // 3. Valid High-confidence workflow replays cleanly and atomically!
        let list_resp = json(&test_result(store.desktop_workflow_memory_list()));
        let valid_wf_id = list_resp["workflows"]
            .as_array()
            .and_then(|arr| {
                arr.iter()
                    .find(|w| w["displayName"] == "Valid Sales Workflow")
                    .and_then(|w| w["workflowId"].as_str())
            })
            .unwrap_or_default()
            .to_owned();
        let replayed_state = json(&test_result(store.desktop_workflow_memory_apply_replay(
            "p_enc",
            &valid_wf_id,
            1_600,
        )));
        assert_eq!(
            replayed_state["metrics"][0]["value"],
            serde_json::json!("350")
        );
        let page_after_good = json(&test_result(store.rows_page("p_enc", "{}")));
        assert_eq!(
            page_after_good["rows"][0]["cells"][1]["visible"],
            serde_json::json!("Cairo")
        );
    }

    #[test]
    fn workflow_replay_ipc_enforces_high_only_confidence_and_rejects_medium_and_low() {
        let dir = test_result(tempfile::tempdir());
        let storage = dir.path().join("replay-confidence-storage");
        let store = authenticated_store_with_dir(storage);

        // 1. Open baseline Sales CSV and save a High-confidence workflow.
        let sales_base = dir.path().join("sales_base.csv");
        test_result(fs::write(
            &sales_base,
            "sale_date,region,customer,amount\n2026-11-01,  Cairo  ,ACME,100\n2026-11-02,Giza,Beta,250\n",
        ));
        test_result(store.open_file("p_base", &sales_base, 1_000));
        let saved_resp = json(&test_result(store.desktop_workflow_memory_save_sheet(
            r#"{
                "workflowId": "wfm_sales_strict_high",
                "displayName": "Monthly Sales Strict High",
                "projectId": "p_base",
                "sheetName": "Sales_Base",
                "tableId": "tbl_sales",
                "headers": ["sale_date", "region", "customer", "amount"],
                "cleaningOperations": [
                    {"stepId": "s1", "kind": "trim_column", "targetColumn": "region"}
                ],
                "metrics": [
                    {"id": "total_sales", "name": "Total Sales", "unit": "USD", "formula": "SUM(amount)"}
                ]
            }"#,
            1_100,
        )));
        let wf_id = saved_resp["saved"]["workflowId"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        assert_eq!(wf_id, "wfm_sales_strict_high");

        // 2. Medium-confidence incoming sheet (same 4 columns + 1 extra column `discount_code`):
        //    match_sheet must classify as Medium / autoReplayEligible=false AND direct IPC replay MUST fail closed!
        let sales_medium = dir.path().join("sales_medium_extra_col.csv");
        test_result(fs::write(
            &sales_medium,
            "sale_date,region,customer,amount,discount_code\n2026-12-01,  Alex  ,Gamma,300,PROMO10\n",
        ));
        test_result(store.open_file("p_medium", &sales_medium, 1_200));
        test_result(store.set_cell("p_medium", 0, "region", "  Alex  ", 1_250));
        let medium_match = json(&test_result(store.desktop_workflow_memory_match_sheet(
            r#"{"projectId":"p_medium","sheetName":"Sales_Dec","headers":["sale_date","region","customer","amount","discount_code"]}"#,
        )));
        assert_eq!(
            medium_match["report"]["matchTier"],
            serde_json::json!("medium")
        );
        assert_eq!(
            medium_match["report"]["autoReplayEligible"],
            serde_json::json!(false)
        );
        // Direct IPC attempt to force replay on Medium confidence MUST be rejected with SF-RCP-001.
        let medium_replay = store.desktop_workflow_memory_apply_replay("p_medium", &wf_id, 1_300);
        assert_eq!(medium_replay, Err("SF-RCP-001".to_owned()));
        // Verify p_medium table was NOT mutated at all.
        let medium_page = json(&test_result(store.rows_page("p_medium", "{}")));
        assert_eq!(
            medium_page["rows"][0]["cells"][1]["visible"],
            serde_json::json!("  Alex  ")
        );

        // 3. Low-confidence incoming sheet (completely different HR domain & headers):
        //    Direct IPC replay MUST fail closed with SF-RCP-001 and leave HR sheet untouched.
        let hr_low = dir.path().join("hr_payroll.csv");
        test_result(fs::write(
            &hr_low,
            "employee_id,department,base_salary,bonus\nE-01,  Engineering  ,5000,750\n",
        ));
        test_result(store.open_file("p_low", &hr_low, 1_400));
        test_result(store.set_cell("p_low", 0, "department", "  Engineering  ", 1_450));
        let low_match = json(&test_result(store.desktop_workflow_memory_match_sheet(
            r#"{"projectId":"p_low","sheetName":"HR_Payroll","headers":["employee_id","department","base_salary","bonus"]}"#,
        )));
        assert_eq!(low_match["report"]["matchTier"], serde_json::json!("low"));
        assert_eq!(
            low_match["report"]["autoReplayEligible"],
            serde_json::json!(false)
        );
        let low_replay = store.desktop_workflow_memory_apply_replay("p_low", &wf_id, 1_500);
        assert_eq!(low_replay, Err("SF-RCP-001".to_owned()));
        let low_page = json(&test_result(store.rows_page("p_low", "{}")));
        assert_eq!(
            low_page["rows"][0]["cells"][1]["visible"],
            serde_json::json!("  Engineering  ")
        );

        // 4. Exact High-confidence incoming sheet replays cleanly and atomically.
        let sales_high = dir.path().join("sales_dec_high.csv");
        test_result(fs::write(
            &sales_high,
            "sale_date,region,customer,amount\n2026-12-05,  Luxor  ,Delta,420\n",
        ));
        test_result(store.open_file("p_high", &sales_high, 1_600));
        test_result(store.set_cell("p_high", 0, "region", "  Luxor  ", 1_650));
        let high_match = json(&test_result(store.desktop_workflow_memory_match_sheet(
            r#"{"projectId":"p_high","sheetName":"Sales_Dec","headers":["sale_date","region","customer","amount"]}"#,
        )));
        assert_eq!(high_match["report"]["matchTier"], serde_json::json!("high"));
        assert_eq!(
            high_match["report"]["autoReplayEligible"],
            serde_json::json!(true)
        );
        let high_state = json(&test_result(
            store.desktop_workflow_memory_apply_replay("p_high", &wf_id, 1_700),
        ));
        assert_eq!(high_state["metrics"][0]["value"], serde_json::json!("420"));
        let high_page = json(&test_result(store.rows_page("p_high", "{}")));
        assert_eq!(
            high_page["rows"][0]["cells"][1]["visible"],
            serde_json::json!("Luxor")
        );
    }
}
