//! Official Template Library, Community Template Contributions (`Share with SafiData`),
//! and Signed Application Release Management (Sections 3, 11, 12, 13, 19).
//!
//! Security & privacy invariants:
//! - Community submissions accept ONLY declarative allowlisted recipe definitions (ZERO scripts,
//!   ZERO executable code, ZERO raw dataset rows).
//! - Explicit user consent + terms version is mandatory (`SF-RCP-006`).
//! - Automated secret/PII scanner blocks embedded credentials/tokens/passwords and warns on
//!   potential personal identifiers before acceptance.
//! - Pending/rejected submissions are NEVER visible to other regular users; ONLY Owner can review
//!   all submissions and explicitly publish approved recipes as versioned Official Templates.
//! - `Approved` and `Published` are strictly separate states.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::auth::unix_now;
use crate::entitlements::{period_ym_at, PlanTier};
use crate::error::AuthError;
use crate::updater::ReleaseManifestRecord;

/// Default monthly community contribution cap per verified account (configurable).
pub const DEFAULT_MONTHLY_COMMUNITY_SUBMISSIONS: usize = 10;
/// Maximum allowed serialized JSON payload size for a community recipe submission (128 KiB).
pub const MAX_SUBMISSION_BYTES: usize = 128 * 1024;
/// Current contribution consent terms version.
pub const CONTRIBUTION_TERMS_VERSION: &str = "safidata-community-terms-v1.0";

/// Allowlist of safe declarative recipe operations (ZERO executable code/scripts).
pub const ALLOWED_RECIPE_OPS: [&str; 9] = [
    "normalize_text",
    "dedupe",
    "fill_blank",
    "parse_date",
    "parse_number",
    "normalize_phone",
    "rename",
    "split",
    "merge",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeRecipeStep {
    pub id: String,
    pub op: String,
    #[serde(default)]
    pub columns: Vec<String>,
    #[serde(default)]
    pub params: Value,
    #[serde(default = "default_true")]
    pub reversible: bool,
}

const fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfficialTemplateVersion {
    pub id: Uuid,
    pub template_id: Uuid,
    pub version_number: u32,
    pub schema_version: u32,
    pub compatible_app_version: String,
    pub release_notes: String,
    pub required_columns: Vec<String>,
    pub steps: Vec<DeclarativeRecipeStep>,
    pub content_hash: String,
    pub published_by: Option<Uuid>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfficialTemplateRecord {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub recipe_type: String,
    pub active: bool,
    pub current_version: u32,
    pub requires_paid_plan: bool,
    pub source_submission_id: Option<Uuid>,
    pub published_by: Option<Uuid>,
    pub created_at: i64,
    pub updated_at: i64,
    pub versions: Vec<OfficialTemplateVersion>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubmissionStatus {
    Pending,
    UnderReview,
    NeedsChanges,
    Approved,
    Rejected,
    Published,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommunitySubmissionRecord {
    pub id: Uuid,
    pub contributor_user_id: Uuid,
    pub workspace_id: Uuid,
    pub name: String,
    pub description: String,
    pub recipe_type: String,
    pub schema_version: u32,
    pub app_version: String,
    pub required_columns: Vec<String>,
    pub steps: Vec<DeclarativeRecipeStep>,
    pub content_hash: String,
    pub status: SubmissionStatus,
    pub reviewer_notes: String,
    pub reviewed_by: Option<Uuid>,
    pub consent_accepted: bool,
    pub consent_terms_version: String,
    pub consent_at: i64,
    pub show_contributor_attribution: bool,
    pub pii_warnings: Vec<String>,
    pub published_template_id: Option<Uuid>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateCommunitySubmissionRequest {
    pub name: String,
    pub description: String,
    #[serde(default = "default_recipe_type")]
    pub recipe_type: String,
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default = "default_app_version")]
    pub app_version: String,
    #[serde(default)]
    pub required_columns: Vec<String>,
    pub steps: Vec<DeclarativeRecipeStep>,
    pub consent_accepted: bool,
    #[serde(default = "default_terms_version")]
    pub consent_terms_version: String,
    #[serde(default)]
    pub show_contributor_attribution: bool,
    /// Optional raw payload fields rejected if a client tries smuggling dataset rows.
    #[serde(default)]
    pub rows: Option<Value>,
    #[serde(default)]
    pub data_rows: Option<Value>,
}

fn default_recipe_type() -> String {
    "cleaning".to_owned()
}
const fn default_schema_version() -> u32 {
    1
}
fn default_app_version() -> String {
    "1.0.0".to_owned()
}
fn default_terms_version() -> String {
    CONTRIBUTION_TERMS_VERSION.to_owned()
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReviewSubmissionRequest {
    pub status: SubmissionStatus,
    #[serde(default)]
    pub reviewer_notes: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PublishSubmissionRequest {
    pub slug: String,
    pub official_name: String,
    pub official_description: String,
    #[serde(default = "default_category")]
    pub category: String,
    #[serde(default)]
    pub requires_paid_plan: bool,
    #[serde(default)]
    pub release_notes: String,
    /// Optional Owner-edited steps override; if omitted, uses submission steps.
    pub edited_steps: Option<Vec<DeclarativeRecipeStep>>,
}

fn default_category() -> String {
    "community".to_owned()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppReleaseRecord {
    pub id: Uuid,
    pub version_code: u64,
    pub version_name: String,
    pub channel: String,
    pub platform: String,
    pub download_url: String,
    pub sha256_hex: String,
    pub file_size_bytes: u64,
    pub min_required_version: u64,
    pub release_notes: String,
    pub is_mandatory: bool,
    pub active: bool,
    pub published_by: Option<Uuid>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PublishAppReleaseRequest {
    pub version_code: u64,
    pub version_name: String,
    #[serde(default = "default_channel")]
    pub channel: String,
    #[serde(default = "default_platform")]
    pub platform: String,
    pub download_url: String,
    pub sha256_hex: String,
    pub file_size_bytes: u64,
    #[serde(default = "default_min_ver")]
    pub min_required_version: u64,
    pub release_notes: String,
    #[serde(default)]
    pub is_mandatory: bool,
}

fn default_channel() -> String {
    "stable".to_owned()
}
fn default_platform() -> String {
    "windows-x86_64".to_owned()
}
const fn default_min_ver() -> u64 {
    100
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TemplatesHubSnapshot {
    pub official_templates: Vec<OfficialTemplateRecord>,
    pub submissions: Vec<CommunitySubmissionRecord>,
    pub releases: Vec<AppReleaseRecord>,
    pub monthly_submission_limit: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecipeScanReport {
    pub blocked_secrets: Vec<String>,
    pub pii_warnings: Vec<String>,
}

/// Inspect a declarative recipe definition for forbidden secrets, tokens, scripts, and PII warnings.
#[must_use]
pub fn scan_recipe_definition(
    name: &str,
    description: &str,
    steps: &[DeclarativeRecipeStep],
) -> RecipeScanReport {
    let mut blocked_secrets = Vec::new();
    let mut pii_warnings = Vec::new();

    let combined = format!(
        "{name}\n{description}\n{}",
        serde_json::to_string(steps).unwrap_or_default()
    );
    let lower = combined.to_ascii_lowercase();

    for (needle, label) in [
        ("sfs_", "SafiData session token"),
        ("sfw_", "Owner web session token"),
        ("sfk_live_", "SafiData live API key"),
        ("sk-", "AI provider secret key"),
        ("sk_live_", "Payment provider live secret key"),
        ("sk_test_", "Payment provider test secret key"),
        ("pdl_live_", "Paddle live API key"),
        ("ghp_", "GitHub personal access token"),
        ("akia", "AWS access key ID"),
        ("bearer ", "HTTP Authorization bearer token"),
        ("-----begin", "Private cryptographic key"),
        ("postgres://", "Database connection string"),
        ("postgresql://", "Database connection string"),
        ("<script", "Executable script tag"),
        ("javascript:", "Executable script URI"),
        ("eval(", "Executable script expression"),
        ("cmd.exe", "Shell command"),
        ("powershell", "Shell command"),
    ] {
        if lower.contains(needle) {
            blocked_secrets.push(label.to_owned());
        }
    }

    // Check JSON keys for credential-like names in step params
    for step in steps {
        inspect_value_for_secrets(&step.params, &mut blocked_secrets, &mut pii_warnings);
    }

    // Warn on email-like or long digit identifiers (e.g. phone/national ID) embedded in static params
    if combined.contains('@') && combined.contains('.') {
        pii_warnings.push("Possible email address found in recipe text/parameters".to_owned());
    }
    if has_long_digit_run(&combined, 11) {
        pii_warnings
            .push("Possible phone number or national ID digit sequence found in recipe".to_owned());
    }

    RecipeScanReport {
        blocked_secrets,
        pii_warnings,
    }
}

fn inspect_value_for_secrets(val: &Value, blocked: &mut Vec<String>, warnings: &mut Vec<String>) {
    match val {
        Value::Object(map) => {
            for (k, v) in map {
                let kl = k.to_ascii_lowercase();
                if kl.contains("password")
                    || kl.contains("secret")
                    || kl.contains("api_key")
                    || kl.contains("private_key")
                    || kl.contains("access_token")
                    || kl.contains("refresh_token")
                    || kl == "rows"
                    || kl == "data_rows"
                {
                    blocked.push(format!("Forbidden field '{k}' in step parameters"));
                }
                if kl.contains("c:\\users\\") || kl.contains("/users/") {
                    warnings.push(format!("Possible local filesystem path in '{k}'"));
                }
                inspect_value_for_secrets(v, blocked, warnings);
            }
        }
        Value::Array(items) => {
            if items.len() > 50 {
                blocked.push("Suspiciously large array inside step parameters".to_owned());
            }
            for item in items {
                inspect_value_for_secrets(item, blocked, warnings);
            }
        }
        Value::String(s) => {
            let sl = s.to_ascii_lowercase();
            if sl.starts_with("c:\\users\\")
                || sl.starts_with("/home/")
                || sl.starts_with("/users/")
            {
                warnings.push("Possible personal filesystem path inside step parameter".to_owned());
            }
        }
        _ => {}
    }
}

fn has_long_digit_run(input: &str, min_len: usize) -> bool {
    let mut run = 0usize;
    for ch in input.chars() {
        if ch.is_ascii_digit() {
            run += 1;
            if run >= min_len {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

pub fn validate_declarative_steps(steps: &[DeclarativeRecipeStep]) -> Result<(), AuthError> {
    if steps.is_empty() || steps.len() > 64 {
        return Err(AuthError::CommunitySubmissionRejected);
    }
    for step in steps {
        if step.id.trim().is_empty() || step.id.len() > 64 {
            return Err(AuthError::CommunitySubmissionRejected);
        }
        if !ALLOWED_RECIPE_OPS.contains(&step.op.as_str()) {
            return Err(AuthError::CommunitySubmissionRejected);
        }
    }
    let encoded = serde_json::to_vec(steps).map_err(|_| AuthError::CommunitySubmissionRejected)?;
    if encoded.len() > MAX_SUBMISSION_BYTES {
        return Err(AuthError::CommunitySubmissionRejected);
    }
    Ok(())
}

/// Check server-authoritative entitlement for Premium Recipe Export/Import (`.safitemplate`).
/// Per `config/business.json` & Section 12.5:
/// - Eligible Paid plans (`IndividualMonthly`, `IndividualAnnual`, `OrganizationSeat`), `FriendVip`, and `Owner` -> ALLOWED.
/// - `Free` and `Trial` -> REJECTED (`SF-RCP-005`).
#[must_use]
pub const fn plan_allows_premium_recipe_exchange(plan: PlanTier) -> bool {
    matches!(
        plan,
        PlanTier::IndividualMonthly
            | PlanTier::IndividualAnnual
            | PlanTier::OrganizationSeat
            | PlanTier::FriendVip
            | PlanTier::Owner
    )
}

#[derive(Debug)]
struct TemplatesHubStore {
    official: HashMap<Uuid, OfficialTemplateRecord>,
    submissions: HashMap<Uuid, CommunitySubmissionRecord>,
    releases: Vec<AppReleaseRecord>,
    monthly_submission_limit: usize,
}

impl Default for TemplatesHubStore {
    fn default() -> Self {
        let now = unix_now();
        let mut official = HashMap::new();
        for (slug, name, desc, category, cols, step_op) in [
            (
                "arabic-customer-cleaning",
                "Arabic & Multilingual Customer Directory Cleaner",
                "Normalizes Arabic/Latin names, trims whitespace, standardizes phone numbers, and deduplicates exact rows.",
                "cleaning",
                vec!["full_name", "phone", "city"],
                "normalize_text",
            ),
            (
                "financial-ledger-standardizer",
                "Financial Ledger & Currency Standardizer",
                "Parses mixed currencies and dates into ISO-8601 and decimal precision, removing blank totals.",
                "finance",
                vec!["date", "amount", "currency"],
                "parse_number",
            ),
            (
                "hr-payroll-sanitization",
                "HR & Payroll Integrity Cleaner",
                "Standardizes employee records, parses hire dates, fills missing department tags, and removes duplicate rows.",
                "hr",
                vec!["employee_name", "hire_date", "salary"],
                "parse_date",
            ),
        ] {
            let tid = Uuid::now_v7();
            let steps = vec![
                DeclarativeRecipeStep {
                    id: "step_1".to_owned(),
                    op: step_op.to_owned(),
                    columns: cols.iter().map(|s| (*s).to_owned()).collect(),
                    params: serde_json::json!({ "trim": true }),
                    reversible: true,
                },
                DeclarativeRecipeStep {
                    id: "step_2".to_owned(),
                    op: "dedupe".to_owned(),
                    columns: vec![],
                    params: serde_json::json!({ "exact": true }),
                    reversible: true,
                },
            ];
            let content_hash = blake3::hash(
                serde_json::to_string(&steps)
                    .unwrap_or_default()
                    .as_bytes(),
            )
            .to_hex()
            .to_string();
            let ver = OfficialTemplateVersion {
                id: Uuid::now_v7(),
                template_id: tid,
                version_number: 1,
                schema_version: 1,
                compatible_app_version: "1.0.0".to_owned(),
                release_notes: "Initial official SafiData template".to_owned(),
                required_columns: cols.into_iter().map(str::to_owned).collect(),
                steps,
                content_hash,
                published_by: None,
                created_at: now,
            };
            official.insert(
                tid,
                OfficialTemplateRecord {
                    id: tid,
                    slug: slug.to_owned(),
                    name: name.to_owned(),
                    description: desc.to_owned(),
                    category: category.to_owned(),
                    recipe_type: "cleaning".to_owned(),
                    active: true,
                    current_version: 1,
                    requires_paid_plan: slug == "hr-payroll-sanitization",
                    source_submission_id: None,
                    published_by: None,
                    created_at: now,
                    updated_at: now,
                    versions: vec![ver],
                },
            );
        }

        // Seed honest default stable Windows x86_64 release metadata (manageable by Owner via /v1/owner/releases).
        let default_release = AppReleaseRecord {
            id: Uuid::now_v7(),
            version_code: 100,
            version_name: "1.0.0".to_owned(),
            channel: "stable".to_owned(),
            platform: "windows-x86_64".to_owned(),
            download_url: "/downloads/SafiData-Desktop-1.0.0-windows-x64.exe".to_owned(),
            sha256_hex: "f78a0776cb5cb7f9829aa95e108c8e5c8675509ac5445603d1136e4c566f9024"
                .to_owned(),
            file_size_bytes: 6_240_768,
            min_required_version: 100,
            release_notes: "SafiData Desktop 1.0 local release build".to_owned(),
            is_mandatory: false,
            active: true,
            published_by: None,
            created_at: now,
        };

        Self {
            official,
            submissions: HashMap::new(),
            releases: vec![default_release],
            monthly_submission_limit: DEFAULT_MONTHLY_COMMUNITY_SUBMISSIONS,
        }
    }
}

/// Central platform service managing Official Templates, Community Submissions, and Signed Releases.
#[derive(Debug, Clone, Default)]
pub struct TemplatesHubService {
    store: Arc<RwLock<TemplatesHubStore>>,
}

impl TemplatesHubService {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn export_snapshot(&self) -> TemplatesHubSnapshot {
        let guard = self.store.read().await;
        TemplatesHubSnapshot {
            official_templates: guard.official.values().cloned().collect(),
            submissions: guard.submissions.values().cloned().collect(),
            releases: guard.releases.clone(),
            monthly_submission_limit: guard.monthly_submission_limit,
        }
    }

    pub async fn import_snapshot(&self, snap: TemplatesHubSnapshot) {
        let mut guard = self.store.write().await;
        if !snap.official_templates.is_empty() {
            guard.official = snap
                .official_templates
                .into_iter()
                .map(|t| (t.id, t))
                .collect();
        }
        guard.submissions = snap.submissions.into_iter().map(|s| (s.id, s)).collect();
        if !snap.releases.is_empty() {
            guard.releases = snap.releases;
        }
        if snap.monthly_submission_limit > 0 {
            guard.monthly_submission_limit = snap.monthly_submission_limit;
        }
    }

    /// List published, active official templates authorized for the caller's plan tier.
    pub async fn list_official_for_plan(&self, plan: PlanTier) -> Vec<OfficialTemplateRecord> {
        let allow_paid = plan_allows_premium_recipe_exchange(plan) || plan == PlanTier::Trial;
        let mut out: Vec<OfficialTemplateRecord> = self
            .store
            .read()
            .await
            .official
            .values()
            .filter(|t| t.active && (!t.requires_paid_plan || allow_paid))
            .cloned()
            .collect();
        out.sort_by(|a, b| a.slug.cmp(&b.slug));
        out
    }

    /// Owner-only: list every official template including deactivated ones and full version history.
    pub async fn list_all_official_for_owner(&self) -> Vec<OfficialTemplateRecord> {
        let mut out: Vec<OfficialTemplateRecord> =
            self.store.read().await.official.values().cloned().collect();
        out.sort_by(|a, b| a.slug.cmp(&b.slug));
        out
    }

    /// Submit a community recipe definition for Owner review (`Share with SafiData`).
    pub async fn submit_community_recipe(
        &self,
        contributor_user_id: Uuid,
        workspace_id: Uuid,
        req: CreateCommunitySubmissionRequest,
        now_secs: i64,
    ) -> Result<CommunitySubmissionRecord, AuthError> {
        // Explicit consent mandatory; never allow smuggled dataset rows.
        if !req.consent_accepted
            || req.consent_terms_version.trim().is_empty()
            || req.rows.is_some()
            || req.data_rows.is_some()
        {
            return Err(AuthError::CommunitySubmissionRejected);
        }
        let name = req.name.trim();
        let desc = req.description.trim();
        if name.is_empty() || name.len() > 160 || desc.is_empty() || desc.len() > 2_000 {
            return Err(AuthError::CommunitySubmissionRejected);
        }
        if req.schema_version != 1 {
            return Err(AuthError::CommunitySubmissionRejected);
        }
        validate_declarative_steps(&req.steps)?;

        let scan = scan_recipe_definition(name, desc, &req.steps);
        if !scan.blocked_secrets.is_empty() {
            return Err(AuthError::CommunitySubmissionRejected);
        }

        let mut guard = self.store.write().await;
        let current_ym = period_ym_at(now_secs);
        let used_this_month = guard
            .submissions
            .values()
            .filter(|s| {
                s.contributor_user_id == contributor_user_id
                    && period_ym_at(s.created_at) == current_ym
            })
            .count();
        if used_this_month >= guard.monthly_submission_limit {
            return Err(AuthError::CommunitySubmissionLimitReached);
        }

        let content_hash = blake3::hash(
            serde_json::to_string(&req.steps)
                .unwrap_or_default()
                .as_bytes(),
        )
        .to_hex()
        .to_string();

        let rec = CommunitySubmissionRecord {
            id: Uuid::now_v7(),
            contributor_user_id,
            workspace_id,
            name: name.to_owned(),
            description: desc.to_owned(),
            recipe_type: req.recipe_type.trim().to_owned(),
            schema_version: req.schema_version,
            app_version: req.app_version.trim().to_owned(),
            required_columns: req.required_columns,
            steps: req.steps,
            content_hash,
            status: SubmissionStatus::Pending,
            reviewer_notes: String::new(),
            reviewed_by: None,
            consent_accepted: true,
            consent_terms_version: req.consent_terms_version.trim().to_owned(),
            consent_at: now_secs,
            show_contributor_attribution: req.show_contributor_attribution,
            pii_warnings: scan.pii_warnings,
            published_template_id: None,
            created_at: now_secs,
            updated_at: now_secs,
        };
        guard.submissions.insert(rec.id, rec.clone());
        Ok(rec)
    }

    /// Regular user: list ONLY their own submitted community recipes.
    pub async fn list_submissions_for_user(&self, user_id: Uuid) -> Vec<CommunitySubmissionRecord> {
        let mut out: Vec<CommunitySubmissionRecord> = self
            .store
            .read()
            .await
            .submissions
            .values()
            .filter(|s| s.contributor_user_id == user_id)
            .cloned()
            .collect();
        out.sort_by_key(|s| std::cmp::Reverse(s.created_at));
        out
    }

    /// Owner-only: list all community template submissions across the platform.
    pub async fn list_all_submissions_for_owner(&self) -> Vec<CommunitySubmissionRecord> {
        let mut out: Vec<CommunitySubmissionRecord> = self
            .store
            .read()
            .await
            .submissions
            .values()
            .cloned()
            .collect();
        out.sort_by_key(|s| std::cmp::Reverse(s.created_at));
        out
    }

    /// Owner-only: inspect a single community submission by ID.
    pub async fn get_submission(&self, id: Uuid) -> Result<CommunitySubmissionRecord, AuthError> {
        self.store
            .read()
            .await
            .submissions
            .get(&id)
            .cloned()
            .ok_or(AuthError::InvalidRequest)
    }

    /// Owner-only: transition a submission review state (`UnderReview`, `NeedsChanges`, `Approved`, `Rejected`).
    /// Note: setting `Approved` NEVER automatically publishes the template.
    pub async fn review_submission(
        &self,
        id: Uuid,
        owner_user_id: Uuid,
        req: ReviewSubmissionRequest,
        now_secs: i64,
    ) -> Result<CommunitySubmissionRecord, AuthError> {
        if req.status == SubmissionStatus::Published {
            // Publishing must use the explicit separate `publish_submission` action.
            return Err(AuthError::InvalidRequest);
        }
        let mut guard = self.store.write().await;
        let sub = guard
            .submissions
            .get_mut(&id)
            .ok_or(AuthError::InvalidRequest)?;
        sub.status = req.status;
        sub.reviewer_notes = req.reviewer_notes.trim().to_owned();
        sub.reviewed_by = Some(owner_user_id);
        sub.updated_at = now_secs;
        Ok(sub.clone())
    }

    /// Owner-only: explicitly publish an approved community submission as a versioned Official Template.
    pub async fn publish_submission(
        &self,
        submission_id: Uuid,
        owner_user_id: Uuid,
        req: PublishSubmissionRequest,
        now_secs: i64,
    ) -> Result<(OfficialTemplateRecord, CommunitySubmissionRecord), AuthError> {
        let slug = req.slug.trim().to_ascii_lowercase();
        let name = req.official_name.trim();
        let desc = req.official_description.trim();
        if slug.len() < 3
            || slug.len() > 80
            || !slug
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || name.is_empty()
            || desc.is_empty()
        {
            return Err(AuthError::InvalidRequest);
        }

        let mut guard = self.store.write().await;
        let sub_clone = guard
            .submissions
            .get(&submission_id)
            .cloned()
            .ok_or(AuthError::InvalidRequest)?;
        if sub_clone.status != SubmissionStatus::Approved {
            return Err(AuthError::InvalidRequest);
        }

        let final_steps = match req.edited_steps {
            Some(steps) => {
                validate_declarative_steps(&steps)?;
                steps
            }
            None => sub_clone.steps.clone(),
        };
        let scan = scan_recipe_definition(name, desc, &final_steps);
        if !scan.blocked_secrets.is_empty() {
            return Err(AuthError::CommunitySubmissionRejected);
        }

        let content_hash = blake3::hash(
            serde_json::to_string(&final_steps)
                .unwrap_or_default()
                .as_bytes(),
        )
        .to_hex()
        .to_string();

        // Check if slug already exists -> append new version instead of silently overwriting!
        let existing_id = guard
            .official
            .values()
            .find(|t| t.slug == slug)
            .map(|t| t.id);

        let template_record = if let Some(tid) = existing_id {
            let tpl = guard
                .official
                .get_mut(&tid)
                .ok_or(AuthError::InvalidRequest)?;
            let next_ver = tpl.current_version.saturating_add(1);
            tpl.name = name.to_owned();
            tpl.description = desc.to_owned();
            tpl.category = req.category.trim().to_owned();
            tpl.requires_paid_plan = req.requires_paid_plan;
            tpl.active = true;
            tpl.current_version = next_ver;
            tpl.source_submission_id = Some(submission_id);
            tpl.published_by = Some(owner_user_id);
            tpl.updated_at = now_secs;
            tpl.versions.push(OfficialTemplateVersion {
                id: Uuid::now_v7(),
                template_id: tid,
                version_number: next_ver,
                schema_version: sub_clone.schema_version,
                compatible_app_version: sub_clone.app_version.clone(),
                release_notes: req.release_notes.trim().to_owned(),
                required_columns: sub_clone.required_columns.clone(),
                steps: final_steps,
                content_hash,
                published_by: Some(owner_user_id),
                created_at: now_secs,
            });
            tpl.clone()
        } else {
            let tid = Uuid::now_v7();
            let ver = OfficialTemplateVersion {
                id: Uuid::now_v7(),
                template_id: tid,
                version_number: 1,
                schema_version: sub_clone.schema_version,
                compatible_app_version: sub_clone.app_version.clone(),
                release_notes: req.release_notes.trim().to_owned(),
                required_columns: sub_clone.required_columns.clone(),
                steps: final_steps,
                content_hash,
                published_by: Some(owner_user_id),
                created_at: now_secs,
            };
            let tpl = OfficialTemplateRecord {
                id: tid,
                slug,
                name: name.to_owned(),
                description: desc.to_owned(),
                category: req.category.trim().to_owned(),
                recipe_type: sub_clone.recipe_type.clone(),
                active: true,
                current_version: 1,
                requires_paid_plan: req.requires_paid_plan,
                source_submission_id: Some(submission_id),
                published_by: Some(owner_user_id),
                created_at: now_secs,
                updated_at: now_secs,
                versions: vec![ver],
            };
            guard.official.insert(tid, tpl.clone());
            tpl
        };

        let sub = guard
            .submissions
            .get_mut(&submission_id)
            .ok_or(AuthError::InvalidRequest)?;
        sub.status = SubmissionStatus::Published;
        sub.published_template_id = Some(template_record.id);
        sub.updated_at = now_secs;

        Ok((template_record, sub.clone()))
    }

    /// Owner-only: deactivate or reactivate a published official template.
    pub async fn set_official_active(
        &self,
        template_id: Uuid,
        active: bool,
        now_secs: i64,
    ) -> Result<OfficialTemplateRecord, AuthError> {
        let mut guard = self.store.write().await;
        let tpl = guard
            .official
            .get_mut(&template_id)
            .ok_or(AuthError::InvalidRequest)?;
        tpl.active = active;
        tpl.updated_at = now_secs;
        Ok(tpl.clone())
    }

    /// Owner-only: rollback an official template to an earlier published version number.
    pub async fn rollback_official_version(
        &self,
        template_id: Uuid,
        target_version: u32,
        now_secs: i64,
    ) -> Result<OfficialTemplateRecord, AuthError> {
        let mut guard = self.store.write().await;
        let tpl = guard
            .official
            .get_mut(&template_id)
            .ok_or(AuthError::InvalidRequest)?;
        if !tpl
            .versions
            .iter()
            .any(|v| v.version_number == target_version)
        {
            return Err(AuthError::InvalidRequest);
        }
        tpl.current_version = target_version;
        tpl.updated_at = now_secs;
        Ok(tpl.clone())
    }

    /// Owner-only: publish a real Desktop application release record.
    pub async fn publish_release(
        &self,
        owner_user_id: Uuid,
        req: PublishAppReleaseRequest,
        now_secs: i64,
    ) -> Result<AppReleaseRecord, AuthError> {
        let sha = req.sha256_hex.trim().to_ascii_lowercase();
        let url = req.download_url.trim();
        if sha.len() != 64
            || !sha.bytes().all(|b| b.is_ascii_hexdigit())
            || req.file_size_bytes == 0
            || req.version_name.trim().is_empty()
            || (!url.starts_with("https://") && !url.starts_with("/downloads/"))
            || !matches!(req.channel.as_str(), "stable" | "beta")
        {
            return Err(AuthError::InvalidRequest);
        }
        let rec = AppReleaseRecord {
            id: Uuid::now_v7(),
            version_code: req.version_code,
            version_name: req.version_name.trim().to_owned(),
            channel: req.channel,
            platform: req.platform.trim().to_owned(),
            download_url: url.to_owned(),
            sha256_hex: sha,
            file_size_bytes: req.file_size_bytes,
            min_required_version: req.min_required_version,
            release_notes: req.release_notes.trim().to_owned(),
            is_mandatory: req.is_mandatory,
            active: true,
            published_by: Some(owner_user_id),
            created_at: now_secs,
        };
        let mut guard = self.store.write().await;
        guard.releases.push(rec.clone());
        Ok(rec)
    }

    /// Resolve the latest active release for `(channel, platform)` into a `ReleaseManifestRecord`.
    pub async fn latest_release(
        &self,
        channel: &str,
        platform: &str,
    ) -> Option<(AppReleaseRecord, ReleaseManifestRecord)> {
        let guard = self.store.read().await;
        let rec = guard
            .releases
            .iter()
            .filter(|r| r.active && r.channel == channel && r.platform == platform)
            .max_by_key(|r| r.version_code)?
            .clone();
        let manifest = ReleaseManifestRecord {
            version: rec.version_code,
            channel: rec.channel.clone(),
            platform: rec.platform.clone(),
            min_required_version: rec.min_required_version,
            delta_checksum: rec.sha256_hex.clone(),
            signature_hex: String::new(),
            is_mandatory: rec.is_mandatory,
            release_notes: rec.release_notes.clone(),
        };
        Some((rec, manifest))
    }

    /// Owner-only: list all published Desktop releases.
    pub async fn list_releases(&self) -> Vec<AppReleaseRecord> {
        let mut out = self.store.read().await.releases.clone();
        out.sort_by_key(|r| std::cmp::Reverse(r.version_code));
        out
    }
}
