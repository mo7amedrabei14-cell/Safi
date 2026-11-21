//! Persistent Per-Sheet Workflow Memory & Continuous Local Learning Engine (Sections 5 & 6).
//!
//! Architecture & Privacy Guarantees:
//! - Strictly local on the customer's device (`workflow_memory.safi.enc` and
//!   `learning_profile.safi.enc`), encrypted at rest with `ChaCha20-Poly1305` via `crate::vault`.
//! - Zero network access (`tools/check-no-network.sh`) and zero automatic upload to central PostgreSQL.
//! - Never logs file names, sheet names, column names, or cell values (`AGENTS.md` Rule 4).
//! - Keeps 5 local storage domains strictly separate:
//!   1. General user preferences & learned rules (`PersonalLearningProfile`)
//!   2. Per-workbook shared context (`WorkbookContextMemory`)
//!   3. Per-sheet / per-table independent workflow memory (`SheetWorkflowMemoryRecord`)
//!   4. Individual project state (`StudioProjectBundle`)
//!   5. Official & personal recipes (`LocalRecipeLibrary`)

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sf_arabic::{NormOptions, normalize_text};

use crate::vault::{VaultKey, decrypt_vault_payload, encrypt_vault_payload};

/// Current schema version for persisted workflow memory and learning profiles.
pub const WORKFLOW_MEMORY_SCHEMA_VERSION: u32 = 1;

/// High-confidence threshold (`>= 80%`) for safe automatic workflow replay when required columns exist.
pub const HIGH_CONFIDENCE_REPLAY_THRESHOLD_PCT: u8 = 80;
/// Medium-confidence threshold (`55%..79%`) for partial reuse with explicit user confirmation.
pub const MEDIUM_CONFIDENCE_SUGGEST_THRESHOLD_PCT: u8 = 55;

/// Semantic business domain of a worksheet or table (enforces strict per-sheet isolation so a
/// Sales workflow never applies to an Employees or Inventory sheet in the same workbook).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SheetBusinessDomain {
    /// Revenue, orders, invoices, customers, regional sales.
    Sales,
    /// HR, payroll, employees, salaries, departments, attendance.
    Employees,
    /// Stock, SKU, warehouse, reorder levels, quantities.
    Inventory,
    /// General ledger, expenses, budgets, cost centers, P&L, tax.
    Finance,
    /// Branch operations, tickets, SLA, logistics.
    BranchOperations,
    /// General tabular dataset when domain signals are mixed.
    #[default]
    General,
}

/// Confidence tier of a sheet workflow match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowMatchTier {
    /// `>= 80%` confidence and all required columns present: safe to auto-replay deterministic workflow.
    High,
    /// `55%..79%` confidence or structural evolution (added column / format shift): reuse safe decisions
    /// and ask only about new or ambiguous columns.
    Medium,
    /// `< 55%` confidence: do not auto-apply; ask necessary clarification questions.
    Low,
}

/// Signature of a single column inside a sheet's semantic identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnSemanticDescriptor {
    /// Original header name as seen in the worksheet.
    pub header_name: String,
    /// Normalized bilingual token key.
    pub normalized_header: String,
    /// Canonical language-independent business concept (e.g. `temporal_date`, `currency_amount`).
    pub canonical_concept: String,
    /// Analytical role (`Measure`, `Dimension`, `Identifier`, `Temporal`, `DescriptiveText`).
    pub analytical_role: String,
    /// Semantic type (`CurrencyAmount`, `CalendarDate`, `PersonName`, `Quantity`, etc.).
    pub semantic_type: String,
    /// Whether the user or engine marked this column as required for the workflow.
    pub required: bool,
}

/// Filename-independent and sheet-name-independent semantic & structural signature of a worksheet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetSemanticSignature {
    /// Classified business purpose of this sheet.
    pub business_domain: SheetBusinessDomain,
    /// Column semantic descriptors in declaration order.
    pub columns: Vec<ColumnSemanticDescriptor>,
    /// Sorted canonical concepts present in the sheet (order-invariant).
    pub canonical_concepts: Vec<String>,
    /// Deterministic structural fingerprint of `(business_domain, sorted canonical_concepts, roles)`.
    pub structural_hash: String,
}

/// Persisted resolution for a contextual clarification question, anchored to both `question_id`
/// and a sheet-rename-resilient `(semantic_topic, canonical_concept)` key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedQuestionDecision {
    /// Original question ID (`q_date_granularity_Sales_tbl_1`, etc.).
    pub question_id: String,
    /// Rename-invariant topic (`date_granularity`, `currency_unit`, `primary_kpi`, `id_handling`, `column_role`).
    pub semantic_topic: String,
    /// Canonical column concept this question applies to (or `"sheet_level"`).
    pub canonical_concept: String,
    /// Action taken (`answered`, `skipped`, `deferred`, `custom`, `corrected`).
    pub action: String,
    /// Selected option ID or answer value.
    pub answer: String,
    /// Optional custom explanation from the user.
    #[serde(default)]
    pub custom_text: String,
    /// Timestamp when the user made this decision.
    pub updated_at_ms: u64,
}

/// User-confirmed column interpretation and formatting decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedColumnDecision {
    /// Original or mapped header name.
    pub header_name: String,
    /// Canonical concept key for cross-file matching.
    pub canonical_concept: String,
    /// Confirmed analytical role (`Measure`, `Dimension`, `Identifier`, `Temporal`, `DescriptiveText`).
    pub confirmed_role: String,
    /// Confirmed semantic type (`CurrencyAmount`, `CalendarDate`, `IdentifierCode`, etc.).
    pub confirmed_semantic_type: String,
    /// Preferred date format (e.g. `YYYY-MM-DD`, `DD/MM/YYYY`) when temporal.
    #[serde(default)]
    pub date_format: Option<String>,
    /// Preferred currency or unit code (e.g. `EGP`, `USD`, `KG`) when measure.
    #[serde(default)]
    pub currency_or_unit: Option<String>,
    /// Whether this column is an identifier/key field (never summed or averaged).
    pub is_identifier: bool,
}

/// Ordered deterministic cleaning/transformation step stored inside a sheet workflow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedCleaningOperation {
    /// Step identifier.
    pub step_id: String,
    /// Operation kind (`trim_column`, `rename_column`, `fill_missing`, `drop_duplicate_rows`,
    /// `blank_invalid_dates`, `calculated_column`, `dsl_command`).
    pub kind: String,
    /// Target column header or canonical concept.
    #[serde(default)]
    pub target_column: Option<String>,
    /// Canonical concept of the target column so renamed headers still map safely.
    #[serde(default)]
    pub target_concept: Option<String>,
    /// Secondary parameter (new column name, replacement value, formula, or DSL command).
    #[serde(default)]
    pub parameter: Option<String>,
}

/// Saved verified metric definition belonging to a sheet workflow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedMetricDefinition {
    /// Metric identifier (`total_revenue`, `avg_order_value`).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Optional unit (`EGP`, `USD`, `%`).
    pub unit: Option<String>,
    /// Formula (`SUM(amount)`, `AVG(revenue)`).
    pub formula: String,
}

/// Dashboard, chart, and deliverable preferences associated with a sheet workflow.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedDashboardPreferences {
    /// Preferred chart types (`["bar", "line", "waterfall"]`).
    #[serde(default)]
    pub preferred_chart_types: Vec<String>,
    /// Preferred grouping dimension concept.
    #[serde(default)]
    pub preferred_group_by_concept: Option<String>,
    /// Preferred report template ID.
    #[serde(default)]
    pub preferred_template_id: Option<String>,
    /// Preferred deliverable export format (`xlsx`, `pdf`, `pptx`, `docx`, `safi.html`).
    #[serde(default)]
    pub preferred_export_format: Option<String>,
}

/// Accepted or rejected recommendation record inside a sheet workflow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedRecommendationFeedback {
    /// Recommendation identifier or topic.
    pub recommendation_id: String,
    /// Whether the user accepted (`true`) or rejected (`false`) it.
    pub accepted: bool,
    /// Optional user note or correction reason.
    #[serde(default)]
    pub note: String,
    /// Timestamp in Unix ms.
    pub timestamp_ms: u64,
}

/// Audit entry in a sheet workflow's decision history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowDecisionLogEntry {
    /// Workflow version number when recorded.
    pub version: u32,
    /// Action summary (`created`, `updated_questions`, `updated_columns`, `restored_version`).
    pub summary: String,
    /// Timestamp in Unix ms.
    pub timestamp_ms: u64,
}

/// Execution metadata from the last time a workflow was applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowExecutionSummary {
    /// Timestamp of execution.
    pub executed_at_ms: u64,
    /// Number of operations applied.
    pub applied_operations: usize,
    /// Number of metrics defined.
    pub applied_metrics: usize,
    /// Number of prior question answers reused without re-asking.
    pub reused_questions: usize,
}

/// Immutable snapshot of a workflow version for non-destructive rollback/restore (Section 5.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetWorkflowVersionSnapshot {
    /// Version number (`1, 2, ...`).
    pub workflow_version: u32,
    /// Timestamp when this version was committed.
    pub saved_at_ms: u64,
    /// Signature at this version.
    pub signature: SheetSemanticSignature,
    /// Question decisions at this version.
    pub question_resolutions: Vec<SavedQuestionDecision>,
    /// Column interpretation decisions at this version.
    pub column_decisions: Vec<SavedColumnDecision>,
    /// Duplicate-detection policy (`exact_key`, `fuzzy_merge`, `keep_all`).
    pub duplicate_policy: String,
    /// Missing-value policy (`flag_only`, `fill_zero`, `fill_median`, `drop_rows`).
    pub missing_value_policy: String,
    /// Explicit business rules supplied by the user.
    pub business_rules: Vec<String>,
    /// Ordered deterministic cleaning operations.
    pub cleaning_operations: Vec<SavedCleaningOperation>,
    /// Verified metric definitions.
    pub metrics: Vec<SavedMetricDefinition>,
    /// Dashboard & reporting preferences.
    pub dashboard_preferences: SavedDashboardPreferences,
}

/// Complete per-sheet / per-table workflow memory record (Section 5.1 & 5.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetWorkflowMemoryRecord {
    /// Stable workflow UUID/ID (`wfm_...`).
    pub workflow_id: String,
    /// Human-readable workflow title (e.g. `"Monthly Sales Sheet Workflow"`).
    pub display_name: String,
    /// Associated project identifier.
    pub project_id: String,
    /// Last seen worksheet name (informational only; matching never depends on filename/sheet name).
    pub last_sheet_name: String,
    /// Last seen table region ID.
    pub last_table_id: String,
    /// Classified business domain (`Sales`, `Employees`, `Inventory`, etc.).
    pub business_domain: SheetBusinessDomain,
    /// Envelope schema version.
    pub schema_version: u32,
    /// Monotonically increasing workflow version (`1, 2, ...`).
    pub workflow_version: u32,
    /// Creation timestamp in Unix ms.
    pub created_at_ms: u64,
    /// Last update timestamp in Unix ms.
    pub updated_at_ms: u64,
    /// Semantic & structural signature used for filename-independent matching.
    pub signature: SheetSemanticSignature,
    /// Answered, skipped, deferred, and custom clarification question decisions.
    pub question_resolutions: Vec<SavedQuestionDecision>,
    /// User-confirmed column role, semantic type, date/currency/unit, and key decisions.
    pub column_decisions: Vec<SavedColumnDecision>,
    /// Duplicate-detection policy.
    pub duplicate_policy: String,
    /// Missing-value handling policy.
    pub missing_value_policy: String,
    /// Explicit business rules supplied by the user.
    pub business_rules: Vec<String>,
    /// Ordered deterministic cleaning & transformation steps.
    pub cleaning_operations: Vec<SavedCleaningOperation>,
    /// Verified metric definitions and formulas.
    pub metrics: Vec<SavedMetricDefinition>,
    /// Dashboard and reporting preferences.
    pub dashboard_preferences: SavedDashboardPreferences,
    /// Accepted and rejected recommendation history.
    pub recommendation_feedback: Vec<SavedRecommendationFeedback>,
    /// Versioned decision audit log.
    pub decision_history: Vec<WorkflowDecisionLogEntry>,
    /// Prior version snapshots for one-click review and restore.
    pub versions: Vec<SheetWorkflowVersionSnapshot>,
    /// Validation status (`"verified"`, `"provisional"`).
    pub validation_status: String,
    /// Last successful execution summary.
    pub last_execution: Option<WorkflowExecutionSummary>,
}

/// Shared workbook-level context (kept distinct from per-sheet workflow memory per Section 5.2 & 6.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbookContextMemory {
    /// Workbook structural signature (sorted domain set + sheet count).
    pub workbook_signature: String,
    /// Default currency code across the workbook.
    pub default_currency: Option<String>,
    /// Default locale (`en` or `ar`).
    pub preferred_locale: String,
    /// Sheet workflow IDs associated with sheets in this workbook family.
    pub linked_sheet_workflow_ids: Vec<String>,
    /// Last updated timestamp in Unix ms.
    pub updated_at_ms: u64,
}

/// Result of matching an incoming worksheet against local workflow memory (Section 5.3 & 5.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetWorkflowMatchReport {
    /// Matched workflow ID, if any candidate scored `>= 40%` in the same business domain.
    pub matched_workflow_id: Option<String>,
    /// Matched workflow display name.
    pub matched_workflow_name: Option<String>,
    /// Matched workflow version.
    pub matched_workflow_version: Option<u32>,
    /// Inferred business domain of the incoming sheet.
    pub detected_domain: SheetBusinessDomain,
    /// Confidence score (`0..=100`).
    pub confidence_pct: u8,
    /// Confidence tier (`High`, `Medium`, `Low`).
    pub match_tier: WorkflowMatchTier,
    /// Whether this workflow is safe to replay automatically without prompting (`High` tier & zero missing required columns).
    pub auto_replay_eligible: bool,
    /// Map of `saved_header -> incoming_header` (handles renamed headers & reordered columns).
    pub column_mapping: BTreeMap<String, String>,
    /// New columns present in the incoming sheet that were not in the saved workflow.
    pub added_columns: Vec<String>,
    /// Columns from the saved workflow that are missing in the incoming sheet.
    pub missing_required_columns: Vec<String>,
    /// Detected format or schema shifts requiring targeted clarification (e.g. date format change).
    pub schema_change_notices: Vec<String>,
    /// Previously answered question decisions that apply directly and MUST NOT be re-asked.
    pub reused_question_resolutions: Vec<SavedQuestionDecision>,
    /// Question topics that are already resolved with high confidence and should be suppressed.
    pub suppressed_question_topics: Vec<String>,
    /// Learned preference suggestions applicable to this sheet from `PersonalLearningProfile`.
    pub learned_suggestions: Vec<ExplainableLearnedSuggestion>,
    /// Human-readable English explanation of why this match was suggested.
    pub explanation_en: String,
    /// Human-readable Arabic explanation of why this match was suggested.
    pub explanation_ar: String,
}

// ============================================================================
// Section 6 — Continuous Local Learning & Personal Profile Types
// ============================================================================

/// Distinguishes how a preference signal was acquired (Section 6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningSignalKind {
    /// User explicitly corrected an automated classification or recommendation.
    ExplicitCorrection,
    /// User explicitly accepted a recommendation.
    ExplicitAcceptance,
    /// User explicitly rejected a recommendation.
    ExplicitRejection,
    /// User marked a choice as a preferred rule for future sheets.
    PreferredRule,
    /// User made the same repeatable choice across multiple sheets.
    RepeatedChoice,
}

/// Category of learned user preference (Section 6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningCategory {
    /// Column semantic type or analytical role classification override.
    ColumnClassification,
    /// Preferred date parsing/output format (`YYYY-MM-DD`, `DD/MM/YYYY`).
    DateFormat,
    /// Preferred currency or measurement unit (`EGP`, `USD`, `SAR`).
    CurrencyUnit,
    /// Preferred duplicate-row policy (`exact_key`, `fuzzy_merge`, `keep_all`).
    DuplicateHandling,
    /// Preferred missing-value policy (`fill_zero`, `fill_median`, `flag_only`).
    MissingValueTreatment,
    /// Preferred metric formula or KPI convention.
    MetricFormula,
    /// Preferred dashboard chart type or layout.
    ChartLayout,
    /// Preferred deliverable export format (`xlsx`, `pdf`, `pptx`, `docx`).
    ExportFormat,
    /// Transformation rule accepted/corrected by the user.
    TransformationRule,
}

/// Scope of a learned rule (Section 6.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningRuleScope {
    /// Applies only to one specific sheet structural hash.
    ThisSheetOnly,
    /// Applies to all future compatible sheets in the matching business domain/concept.
    MatchingSheets,
}

/// Single explainable, reversible learned preference rule inside [`PersonalLearningProfile`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearnedPreferenceRule {
    /// Stable rule ID (`lrn_...`).
    pub rule_id: String,
    /// Signal strength/origin (`explicit_correction`, `preferred_rule`, etc.).
    pub signal_kind: LearningSignalKind,
    /// Category of learned behavior.
    pub category: LearningCategory,
    /// Rule scope (`this_sheet_only` vs `matching_sheets`).
    pub scope: LearningRuleScope,
    /// Optional sheet structural hash when `scope == ThisSheetOnly`.
    #[serde(default)]
    pub bound_structural_hash: Option<String>,
    /// Optional business domain filter.
    #[serde(default)]
    pub business_domain: Option<SheetBusinessDomain>,
    /// Canonical column concept or topic key (e.g. `currency_amount`, `temporal_date`, `customer_code`).
    pub target_concept: String,
    /// Learned preferred value/action.
    pub preferred_value: String,
    /// Rejected value/action (when user corrected or rejected a suggestion).
    #[serde(default)]
    pub rejected_value: Option<String>,
    /// User's custom explanation of the correction.
    #[serde(default)]
    pub user_explanation: String,
    /// Confidence score (`0..=100`). Explicit corrections/rules carry `95%`; repeated choices scale with evidence.
    pub confidence_pct: u8,
    /// Number of times the user confirmed or repeated this preference.
    pub evidence_count: u32,
    /// English provenance explanation (`"Learned from explicit correction on Sales sheet"`).
    pub provenance_en: String,
    /// Arabic provenance explanation.
    pub provenance_ar: String,
    /// Creation timestamp in Unix ms.
    pub created_at_ms: u64,
    /// Last update timestamp in Unix ms.
    pub updated_at_ms: u64,
}

/// Explainable suggestion generated from [`PersonalLearningProfile`] for a worksheet (Section 6.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainableLearnedSuggestion {
    /// ID of the [`LearnedPreferenceRule`] that produced this suggestion.
    pub influencing_rule_id: String,
    /// Category of suggestion.
    pub category: LearningCategory,
    /// Target column header or concept in the current sheet.
    pub target_header_or_concept: String,
    /// Suggested value or action.
    pub suggested_value: String,
    /// Rejected value avoided because of prior user correction.
    pub avoided_value: Option<String>,
    /// Confidence percentage (`0..=100`).
    pub confidence_pct: u8,
    /// Whether this can be auto-applied (`true` only for non-destructive high-confidence rules) or requires confirmation.
    pub requires_confirmation: bool,
    /// English explanation of why this suggestion was made and which prior decision influenced it.
    pub why_en: String,
    /// Arabic explanation of why this suggestion was made and which prior decision influenced it.
    pub why_ar: String,
}

/// Versioned, encrypted, explainable, and resettable Personal Learning Profile (Section 6.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalLearningProfile {
    /// Profile schema version.
    pub schema_version: u32,
    /// Monotonically increasing profile revision.
    pub profile_version: u32,
    /// General user preferences (default currency, date format, export format, UI density).
    pub general_preferences: BTreeMap<String, String>,
    /// Active learned preference rules keyed by `rule_id`.
    pub rules: Vec<LearnedPreferenceRule>,
    /// Last updated timestamp in Unix ms.
    pub updated_at_ms: u64,
}

impl Default for PersonalLearningProfile {
    fn default() -> Self {
        Self {
            schema_version: WORKFLOW_MEMORY_SCHEMA_VERSION,
            profile_version: 1,
            general_preferences: BTreeMap::new(),
            rules: Vec::new(),
            updated_at_ms: 0,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedWorkflowStore {
    schema_version: u32,
    workbook_contexts: BTreeMap<String, WorkbookContextMemory>,
    sheet_workflows: BTreeMap<String, SheetWorkflowMemoryRecord>,
}

/// Local encrypted manager for Per-Sheet Workflow Memory and Continuous Local Learning.
#[derive(Debug)]
pub struct LocalWorkflowMemoryStore {
    storage_dir: PathBuf,
    vault_key: Option<VaultKey>,
    workbook_contexts: BTreeMap<String, WorkbookContextMemory>,
    sheet_workflows: BTreeMap<String, SheetWorkflowMemoryRecord>,
    learning_profile: PersonalLearningProfile,
}

impl LocalWorkflowMemoryStore {
    /// Opens or initializes the encrypted workflow memory store and personal learning profile in `storage_dir`.
    ///
    /// # Errors
    /// Returns `SF-SYS-001`, `SF-SYS-016`, or `SF-SYS-024` if existing files cannot be read or decrypted.
    pub fn open(storage_dir: PathBuf, vault_key: Option<VaultKey>) -> Result<Self, String> {
        fs::create_dir_all(&storage_dir).map_err(|_| "SF-SYS-016".to_owned())?;

        let wf_raw = read_optional_encrypted_or_plain(
            &storage_dir.join("workflow_memory.safi.enc"),
            &storage_dir.join("workflow_memory.json"),
            vault_key.as_ref(),
        )?;
        let persisted_wf: PersistedWorkflowStore = match wf_raw {
            Some(raw) => serde_json::from_str(&raw).map_err(|_| "SF-SYS-001".to_owned())?,
            None => PersistedWorkflowStore {
                schema_version: WORKFLOW_MEMORY_SCHEMA_VERSION,
                workbook_contexts: BTreeMap::new(),
                sheet_workflows: BTreeMap::new(),
            },
        };

        let lp_raw = read_optional_encrypted_or_plain(
            &storage_dir.join("learning_profile.safi.enc"),
            &storage_dir.join("learning_profile.json"),
            vault_key.as_ref(),
        )?;
        let learning_profile: PersonalLearningProfile = match lp_raw {
            Some(raw) => serde_json::from_str(&raw).map_err(|_| "SF-SYS-001".to_owned())?,
            None => PersonalLearningProfile::default(),
        };

        Ok(Self {
            storage_dir,
            vault_key,
            workbook_contexts: persisted_wf.workbook_contexts,
            sheet_workflows: persisted_wf.sheet_workflows,
            learning_profile,
        })
    }

    /// Updates the vault key used for encrypting persisted workflow memory and learning profile files.
    pub fn set_vault_key(&mut self, key: Option<VaultKey>) {
        self.vault_key = key;
    }

    /// Lists all saved per-sheet workflow memory records sorted by most recently updated.
    #[must_use]
    pub fn list_sheet_workflows(&self) -> Vec<SheetWorkflowMemoryRecord> {
        let mut list: Vec<SheetWorkflowMemoryRecord> =
            self.sheet_workflows.values().cloned().collect();
        list.sort_by(|a, b| {
            b.updated_at_ms
                .cmp(&a.updated_at_ms)
                .then_with(|| a.workflow_id.cmp(&b.workflow_id))
        });
        list
    }

    /// Gets a single saved sheet workflow by ID.
    #[must_use]
    pub fn get_sheet_workflow(&self, workflow_id: &str) -> Option<&SheetWorkflowMemoryRecord> {
        self.sheet_workflows.get(workflow_id)
    }

    /// Returns a snapshot of the user's [`PersonalLearningProfile`].
    #[must_use]
    pub fn learning_profile(&self) -> &PersonalLearningProfile {
        &self.learning_profile
    }

    /// Saves or updates a per-sheet workflow memory record, preserving version snapshots so previous
    /// versions can always be reviewed or restored without corrupting history (Section 5.1 & 5.5).
    ///
    /// Never overwrites a different sheet's workflow: if `workflow_id` is omitted, a new stable ID
    /// is derived from `(project_id, sheet_name, signature.structural_hash)` or allocated fresh.
    ///
    /// # Errors
    /// Returns `SF-SYS-001` or `SF-SYS-023` on validation or persistence failure.
    #[allow(clippy::too_many_arguments)]
    pub fn save_sheet_workflow(
        &mut self,
        workflow_id: Option<&str>,
        display_name: &str,
        project_id: &str,
        sheet_name: &str,
        table_id: &str,
        headers: &[String],
        question_resolutions: Vec<SavedQuestionDecision>,
        column_decisions: Vec<SavedColumnDecision>,
        duplicate_policy: &str,
        missing_value_policy: &str,
        business_rules: Vec<String>,
        cleaning_operations: Vec<SavedCleaningOperation>,
        metrics: Vec<SavedMetricDefinition>,
        dashboard_preferences: SavedDashboardPreferences,
        recommendation_feedback: Vec<SavedRecommendationFeedback>,
        now_ms: u64,
    ) -> Result<SheetWorkflowMemoryRecord, String> {
        if headers.is_empty() {
            return Err("SF-SYS-001".to_owned());
        }
        let signature = build_sheet_semantic_signature(sheet_name, headers, &column_decisions);
        let resolved_id = workflow_id
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| {
                let seed = format!(
                    "{}:{}:{}:{}",
                    project_id.trim(),
                    sheet_name.trim(),
                    table_id.trim(),
                    signature.structural_hash
                );
                format!("wfm_{}", &sf_recipe::blake3_hex(seed.as_bytes())[..16])
            });

        let clean_name = if display_name.trim().is_empty() {
            format!("{} ({:?})", sheet_name.trim(), signature.business_domain)
        } else {
            display_name.trim().to_owned()
        };

        let has_provisional = question_resolutions
            .iter()
            .any(|q| matches!(q.action.as_str(), "skipped" | "deferred"));
        let validation_status = if has_provisional {
            "provisional".to_owned()
        } else {
            "verified".to_owned()
        };

        let record = if let Some(existing) = self.sheet_workflows.get(&resolved_id).cloned() {
            let next_version = existing.workflow_version.saturating_add(1);
            let mut versions = existing.versions;
            versions.push(SheetWorkflowVersionSnapshot {
                workflow_version: next_version,
                saved_at_ms: now_ms,
                signature: signature.clone(),
                question_resolutions: question_resolutions.clone(),
                column_decisions: column_decisions.clone(),
                duplicate_policy: duplicate_policy.to_owned(),
                missing_value_policy: missing_value_policy.to_owned(),
                business_rules: business_rules.clone(),
                cleaning_operations: cleaning_operations.clone(),
                metrics: metrics.clone(),
                dashboard_preferences: dashboard_preferences.clone(),
            });
            let mut history = existing.decision_history;
            history.push(WorkflowDecisionLogEntry {
                version: next_version,
                summary: format!(
                    "Updated workflow v{next_version}: {} questions, {} ops, {} metrics",
                    question_resolutions.len(),
                    cleaning_operations.len(),
                    metrics.len()
                ),
                timestamp_ms: now_ms,
            });
            SheetWorkflowMemoryRecord {
                workflow_id: resolved_id.clone(),
                display_name: clean_name,
                project_id: project_id.trim().to_owned(),
                last_sheet_name: sheet_name.trim().to_owned(),
                last_table_id: table_id.trim().to_owned(),
                business_domain: signature.business_domain,
                schema_version: WORKFLOW_MEMORY_SCHEMA_VERSION,
                workflow_version: next_version,
                created_at_ms: existing.created_at_ms,
                updated_at_ms: now_ms,
                signature,
                question_resolutions,
                column_decisions,
                duplicate_policy: duplicate_policy.to_owned(),
                missing_value_policy: missing_value_policy.to_owned(),
                business_rules,
                cleaning_operations,
                metrics,
                dashboard_preferences,
                recommendation_feedback,
                decision_history: history,
                versions,
                validation_status,
                last_execution: existing.last_execution,
            }
        } else {
            let initial_snapshot = SheetWorkflowVersionSnapshot {
                workflow_version: 1,
                saved_at_ms: now_ms,
                signature: signature.clone(),
                question_resolutions: question_resolutions.clone(),
                column_decisions: column_decisions.clone(),
                duplicate_policy: duplicate_policy.to_owned(),
                missing_value_policy: missing_value_policy.to_owned(),
                business_rules: business_rules.clone(),
                cleaning_operations: cleaning_operations.clone(),
                metrics: metrics.clone(),
                dashboard_preferences: dashboard_preferences.clone(),
            };
            SheetWorkflowMemoryRecord {
                workflow_id: resolved_id.clone(),
                display_name: clean_name,
                project_id: project_id.trim().to_owned(),
                last_sheet_name: sheet_name.trim().to_owned(),
                last_table_id: table_id.trim().to_owned(),
                business_domain: signature.business_domain,
                schema_version: WORKFLOW_MEMORY_SCHEMA_VERSION,
                workflow_version: 1,
                created_at_ms: now_ms,
                updated_at_ms: now_ms,
                signature,
                question_resolutions,
                column_decisions,
                duplicate_policy: duplicate_policy.to_owned(),
                missing_value_policy: missing_value_policy.to_owned(),
                business_rules,
                cleaning_operations,
                metrics,
                dashboard_preferences,
                recommendation_feedback,
                decision_history: vec![WorkflowDecisionLogEntry {
                    version: 1,
                    summary: "Created initial sheet workflow memory".to_owned(),
                    timestamp_ms: now_ms,
                }],
                versions: vec![initial_snapshot],
                validation_status,
                last_execution: None,
            }
        };

        let _ = self.sheet_workflows.insert(resolved_id, record.clone());
        self.persist_workflows()?;
        Ok(record)
    }

    /// Matches an incoming worksheet (regardless of filename or sheet name) against all saved
    /// per-sheet workflows and the user's continuous learning profile (Section 5.3 & 5.4).
    #[must_use]
    pub fn match_incoming_sheet(
        &self,
        sheet_name: &str,
        headers: &[String],
        observed_date_format: Option<&str>,
    ) -> SheetWorkflowMatchReport {
        let incoming_sig = build_sheet_semantic_signature(sheet_name, headers, &[]);
        let learned_suggestions = self.suggest_from_learning(sheet_name, &incoming_sig);

        let mut best_score: u8 = 0;
        let mut best_record: Option<&SheetWorkflowMemoryRecord> = None;
        let mut best_mapping: BTreeMap<String, String> = BTreeMap::new();
        let mut best_added: Vec<String> = Vec::new();
        let mut best_missing_req: Vec<String> = Vec::new();

        for candidate in self.sheet_workflows.values() {
            // Hard domain isolation: NEVER cross-apply Sales workflow to Employees or Inventory!
            if candidate.business_domain != incoming_sig.business_domain
                && candidate.business_domain != SheetBusinessDomain::General
                && incoming_sig.business_domain != SheetBusinessDomain::General
            {
                continue;
            }

            let (score, mapping, added, missing_req) =
                score_signature_compatibility(&candidate.signature, &incoming_sig);
            if score > best_score
                || (score == best_score
                    && best_record.is_some_and(|prev| candidate.updated_at_ms > prev.updated_at_ms))
            {
                best_score = score;
                best_record = Some(candidate);
                best_mapping = mapping;
                best_added = added;
                best_missing_req = missing_req;
            }
        }

        let Some(matched) = best_record.filter(|_| best_score >= 40) else {
            return SheetWorkflowMatchReport {
                matched_workflow_id: None,
                matched_workflow_name: None,
                matched_workflow_version: None,
                detected_domain: incoming_sig.business_domain,
                confidence_pct: best_score,
                match_tier: WorkflowMatchTier::Low,
                auto_replay_eligible: false,
                column_mapping: BTreeMap::new(),
                added_columns: headers.to_vec(),
                missing_required_columns: Vec::new(),
                schema_change_notices: Vec::new(),
                reused_question_resolutions: Vec::new(),
                suppressed_question_topics: Vec::new(),
                learned_suggestions,
                explanation_en: "No prior saved workflow matched this worksheet structure."
                    .to_owned(),
                explanation_ar: "لم يتم العثور على مسار عمل محفوظ يطابق هيكل هذه الورقة."
                    .to_owned(),
            };
        };

        let mut schema_notices = Vec::new();
        if !best_added.is_empty() {
            schema_notices.push(format!("New column(s) detected: {}", best_added.join(", ")));
        }
        if !best_missing_req.is_empty() {
            schema_notices.push(format!(
                "Missing required column(s): {}",
                best_missing_req.join(", ")
            ));
        }
        if let Some(obs_fmt) = observed_date_format {
            for col_dec in &matched.column_decisions {
                if let Some(saved_fmt) = col_dec.date_format.as_deref()
                    && !saved_fmt.is_empty()
                    && !saved_fmt.eq_ignore_ascii_case(obs_fmt)
                {
                    schema_notices.push(format!(
                        "Date format shift on '{}': previously {}, now {}",
                        col_dec.header_name, saved_fmt, obs_fmt
                    ));
                }
            }
        }

        let has_schema_shift =
            !best_added.is_empty() || !best_missing_req.is_empty() || !schema_notices.is_empty();

        // Clamp confidence to Medium if there is a structural schema change (new column or date shift)
        // so SafiData reuses safe prior decisions and asks ONLY about the new/changed elements!
        let effective_confidence =
            if has_schema_shift && best_score >= HIGH_CONFIDENCE_REPLAY_THRESHOLD_PCT {
                76
            } else {
                best_score
            };

        let match_tier = if effective_confidence >= HIGH_CONFIDENCE_REPLAY_THRESHOLD_PCT
            && best_missing_req.is_empty()
            && !has_schema_shift
        {
            WorkflowMatchTier::High
        } else if effective_confidence >= MEDIUM_CONFIDENCE_SUGGEST_THRESHOLD_PCT {
            WorkflowMatchTier::Medium
        } else {
            WorkflowMatchTier::Low
        };

        let auto_replay_eligible = match_tier == WorkflowMatchTier::High;

        // Collect all answered/custom/corrected question decisions whose concept exists in the incoming sheet.
        let incoming_concepts: BTreeSet<&str> = incoming_sig
            .canonical_concepts
            .iter()
            .map(String::as_str)
            .collect();
        let mut reused_questions = Vec::new();
        let mut suppressed_topics = Vec::new();

        for q in &matched.question_resolutions {
            let concept_present = q.canonical_concept == "sheet_level"
                || incoming_concepts.contains(q.canonical_concept.as_str());
            let answered_confidently =
                matches!(q.action.as_str(), "answered" | "custom" | "corrected");
            // If date format shifted, do not suppress date format questions!
            let date_shifted = q.semantic_topic.contains("date")
                && schema_notices
                    .iter()
                    .any(|n| n.starts_with("Date format shift"));
            if concept_present && answered_confidently && !date_shifted {
                reused_questions.push(q.clone());
                if !suppressed_topics.contains(&q.semantic_topic) {
                    suppressed_topics.push(q.semantic_topic.clone());
                }
            }
        }

        let explanation_en = match match_tier {
            WorkflowMatchTier::High => format!(
                "Recognized '{}' (v{}, {}% semantic match across {} columns). Ready to automatically replay {} cleaning step(s), {} metric(s), and reuse {} answered question(s).",
                matched.display_name,
                matched.workflow_version,
                effective_confidence,
                best_mapping.len(),
                matched.cleaning_operations.len(),
                matched.metrics.len(),
                reused_questions.len()
            ),
            WorkflowMatchTier::Medium => format!(
                "Partial semantic match with '{}' (v{}, {}% match). Reusing {} prior decision(s); asking only about structural changes ({}).",
                matched.display_name,
                matched.workflow_version,
                effective_confidence,
                reused_questions.len(),
                if schema_notices.is_empty() {
                    "column variations".to_owned()
                } else {
                    schema_notices.join("; ")
                }
            ),
            WorkflowMatchTier::Low => format!(
                "Low similarity ({}%) with '{}'; manual confirmation required.",
                effective_confidence, matched.display_name
            ),
        };

        let explanation_ar = match match_tier {
            WorkflowMatchTier::High => format!(
                "تم التعرّف تلقائيًا على نمط الورقة «{}» (الإصدار v{}، تطابق دلالي {}% عبر {} أعمدة). تمت إعادة استخدام {} إجابات سابقة وتجهيز {} خطوات تنظيف و{} مؤشرات دون تكرار الأسئلة.",
                matched.display_name,
                matched.workflow_version,
                effective_confidence,
                best_mapping.len(),
                reused_questions.len(),
                matched.cleaning_operations.len(),
                matched.metrics.len()
            ),
            WorkflowMatchTier::Medium => format!(
                "تطابق دلالي جزئي مع «{}» (الإصدار v{}، نسبة {}%). تم الحفاظ على {} قرارات سابقة آمنة، ويتم السؤال فقط عن التغييرات الجديدة ({}).",
                matched.display_name,
                matched.workflow_version,
                effective_confidence,
                reused_questions.len(),
                if schema_notices.is_empty() {
                    "اختلافات الأعمدة".to_owned()
                } else {
                    schema_notices.join("؛ ")
                }
            ),
            WorkflowMatchTier::Low => format!(
                "تشابه منخفض ({}%) مع «{}»؛ يتطلب مراجعة المستخدم.",
                effective_confidence, matched.display_name
            ),
        };

        SheetWorkflowMatchReport {
            matched_workflow_id: Some(matched.workflow_id.clone()),
            matched_workflow_name: Some(matched.display_name.clone()),
            matched_workflow_version: Some(matched.workflow_version),
            detected_domain: incoming_sig.business_domain,
            confidence_pct: effective_confidence,
            match_tier,
            auto_replay_eligible,
            column_mapping: best_mapping,
            added_columns: best_added,
            missing_required_columns: best_missing_req,
            schema_change_notices: schema_notices,
            reused_question_resolutions: reused_questions,
            suppressed_question_topics: suppressed_topics,
            learned_suggestions,
            explanation_en,
            explanation_ar,
        }
    }

    /// Records that a saved workflow was executed on a sheet.
    ///
    /// # Errors
    /// Returns `SF-SYS-001` if `workflow_id` is not found.
    pub fn record_workflow_execution(
        &mut self,
        workflow_id: &str,
        applied_operations: usize,
        applied_metrics: usize,
        reused_questions: usize,
        now_ms: u64,
    ) -> Result<SheetWorkflowMemoryRecord, String> {
        let rec = self
            .sheet_workflows
            .get_mut(workflow_id)
            .ok_or_else(|| "SF-SYS-001".to_owned())?;
        rec.last_execution = Some(WorkflowExecutionSummary {
            executed_at_ms: now_ms,
            applied_operations,
            applied_metrics,
            reused_questions,
        });
        let cloned = rec.clone();
        self.persist_workflows()?;
        Ok(cloned)
    }

    /// Restores a workflow to a previous version snapshot non-destructively (creates a new version
    /// entry reflecting the restored state so version history is never lost — Section 5.5).
    ///
    /// # Errors
    /// Returns `SF-SYS-001` if `workflow_id` or `target_version` does not exist.
    pub fn restore_workflow_version(
        &mut self,
        workflow_id: &str,
        target_version: u32,
        now_ms: u64,
    ) -> Result<SheetWorkflowMemoryRecord, String> {
        let rec = self
            .sheet_workflows
            .get_mut(workflow_id)
            .ok_or_else(|| "SF-SYS-001".to_owned())?;
        let snap = rec
            .versions
            .iter()
            .find(|v| v.workflow_version == target_version)
            .cloned()
            .ok_or_else(|| "SF-SYS-001".to_owned())?;

        let next_ver = rec.workflow_version.saturating_add(1);
        rec.workflow_version = next_ver;
        rec.updated_at_ms = now_ms;
        rec.signature = snap.signature.clone();
        rec.question_resolutions = snap.question_resolutions.clone();
        rec.column_decisions = snap.column_decisions.clone();
        rec.duplicate_policy = snap.duplicate_policy.clone();
        rec.missing_value_policy = snap.missing_value_policy.clone();
        rec.business_rules = snap.business_rules.clone();
        rec.cleaning_operations = snap.cleaning_operations.clone();
        rec.metrics = snap.metrics.clone();
        rec.dashboard_preferences = snap.dashboard_preferences.clone();
        rec.decision_history.push(WorkflowDecisionLogEntry {
            version: next_ver,
            summary: format!("Restored workflow from v{target_version} as v{next_ver}"),
            timestamp_ms: now_ms,
        });
        rec.versions.push(SheetWorkflowVersionSnapshot {
            workflow_version: next_ver,
            saved_at_ms: now_ms,
            ..snap
        });

        let cloned = rec.clone();
        self.persist_workflows()?;
        Ok(cloned)
    }

    /// Duplicates an existing sheet workflow under a new independent ID and display name (Section 5.5).
    ///
    /// # Errors
    /// Returns `SF-SYS-001` if `workflow_id` is missing.
    pub fn duplicate_workflow(
        &mut self,
        workflow_id: &str,
        new_display_name: Option<&str>,
        now_ms: u64,
    ) -> Result<SheetWorkflowMemoryRecord, String> {
        let orig = self
            .sheet_workflows
            .get(workflow_id)
            .cloned()
            .ok_or_else(|| "SF-SYS-001".to_owned())?;
        let copy_name = new_display_name
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{} (Copy)", orig.display_name));
        let new_id = format!(
            "wfm_{}_{}",
            now_ms,
            &sf_recipe::blake3_hex(format!("{workflow_id}:{copy_name}:{now_ms}").as_bytes())[..10]
        );
        let mut copy = orig;
        copy.workflow_id = new_id.clone();
        copy.display_name = copy_name;
        copy.workflow_version = 1;
        copy.created_at_ms = now_ms;
        copy.updated_at_ms = now_ms;
        copy.decision_history = vec![WorkflowDecisionLogEntry {
            version: 1,
            summary: format!("Duplicated from workflow {workflow_id}"),
            timestamp_ms: now_ms,
        }];
        let _ = self.sheet_workflows.insert(new_id, copy.clone());
        self.persist_workflows()?;
        Ok(copy)
    }

    /// Renames a saved sheet workflow (Section 5.5).
    ///
    /// # Errors
    /// Returns `SF-SYS-001` if `workflow_id` is missing or `new_name` is empty.
    pub fn rename_workflow(
        &mut self,
        workflow_id: &str,
        new_name: &str,
        now_ms: u64,
    ) -> Result<SheetWorkflowMemoryRecord, String> {
        let trimmed = new_name.trim();
        if trimmed.is_empty() || trimmed.len() > 160 {
            return Err("SF-SYS-001".to_owned());
        }
        let rec = self
            .sheet_workflows
            .get_mut(workflow_id)
            .ok_or_else(|| "SF-SYS-001".to_owned())?;
        rec.display_name = trimmed.to_owned();
        rec.updated_at_ms = now_ms;
        let cloned = rec.clone();
        self.persist_workflows()?;
        Ok(cloned)
    }

    /// Deletes a saved sheet workflow (Section 5.5).
    ///
    /// # Errors
    /// Returns `SF-SYS-001` if `workflow_id` does not exist.
    pub fn delete_workflow(&mut self, workflow_id: &str) -> Result<(), String> {
        if self.sheet_workflows.remove(workflow_id).is_none() {
            return Err("SF-SYS-001".to_owned());
        }
        self.persist_workflows()
    }

    // ========================================================================
    // Section 6 — Continuous Local Learning Operations
    // ========================================================================

    /// Records explicit user feedback (correction, acceptance, rejection, or preferred rule)
    /// into the local [`PersonalLearningProfile`] (Section 6.1, 6.2, 6.3).
    ///
    /// # Errors
    /// Returns `SF-SYS-001` or `SF-SYS-023` on validation or persistence error.
    #[allow(clippy::too_many_arguments)]
    pub fn record_learning_feedback(
        &mut self,
        signal_kind: LearningSignalKind,
        category: LearningCategory,
        scope: LearningRuleScope,
        bound_structural_hash: Option<String>,
        business_domain: Option<SheetBusinessDomain>,
        target_concept: &str,
        preferred_value: &str,
        rejected_value: Option<String>,
        user_explanation: &str,
        source_sheet_label: &str,
        now_ms: u64,
    ) -> Result<LearnedPreferenceRule, String> {
        let concept = canonicalize_header_concept(target_concept);
        let pref = preferred_value.trim();
        if concept.is_empty() || pref.is_empty() {
            return Err("SF-SYS-001".to_owned());
        }

        // Check if an existing rule matches (category, scope, business_domain, target_concept, bound_hash).
        if let Some(existing) = self.learning_profile.rules.iter_mut().find(|r| {
            r.category == category
                && r.scope == scope
                && r.business_domain == business_domain
                && r.target_concept == concept
                && r.bound_structural_hash == bound_structural_hash
        }) {
            existing.signal_kind = signal_kind;
            existing.preferred_value = pref.to_owned();
            existing.rejected_value = rejected_value;
            if !user_explanation.trim().is_empty() {
                existing.user_explanation = user_explanation.trim().to_owned();
            }
            existing.evidence_count = existing.evidence_count.saturating_add(1);
            existing.confidence_pct =
                compute_learning_confidence(signal_kind, existing.evidence_count);
            existing.updated_at_ms = now_ms;
            existing.provenance_en = format!(
                "Updated from {:?} on '{}' ({} confirmation(s))",
                signal_kind, source_sheet_label, existing.evidence_count
            );
            existing.provenance_ar = format!(
                "تم التحديث بناءً على تفضيل المستخدم في الورقة «{}» ({} مرات تأكيد)",
                source_sheet_label, existing.evidence_count
            );
            let updated_rule = existing.clone();
            self.learning_profile.profile_version =
                self.learning_profile.profile_version.saturating_add(1);
            self.learning_profile.updated_at_ms = now_ms;
            self.persist_learning_profile()?;
            return Ok(updated_rule);
        }

        let confidence_pct = compute_learning_confidence(signal_kind, 1);
        let rule_id = format!(
            "lrn_{}_{}",
            now_ms,
            &sf_recipe::blake3_hex(
                format!(
                    "{concept}:{pref}:{now_ms}:{}",
                    self.learning_profile.rules.len()
                )
                .as_bytes()
            )[..10]
        );
        let rule = LearnedPreferenceRule {
            rule_id,
            signal_kind,
            category,
            scope,
            bound_structural_hash,
            business_domain,
            target_concept: concept,
            preferred_value: pref.to_owned(),
            rejected_value,
            user_explanation: user_explanation.trim().to_owned(),
            confidence_pct,
            evidence_count: 1,
            provenance_en: format!(
                "Learned via {:?} on worksheet '{}'",
                signal_kind, source_sheet_label
            ),
            provenance_ar: format!(
                "تم تعلّم القاعدة من تصحيح/تفضيل صريح في الورقة «{}»",
                source_sheet_label
            ),
            created_at_ms: now_ms,
            updated_at_ms: now_ms,
        };
        self.learning_profile.rules.push(rule.clone());
        self.learning_profile.profile_version =
            self.learning_profile.profile_version.saturating_add(1);
        self.learning_profile.updated_at_ms = now_ms;
        self.persist_learning_profile()?;
        Ok(rule)
    }

    /// Generates explainable suggestions for a worksheet based on the user's learned preferences (Section 6.3).
    #[must_use]
    pub fn suggest_from_learning(
        &self,
        _sheet_name: &str,
        signature: &SheetSemanticSignature,
    ) -> Vec<ExplainableLearnedSuggestion> {
        let mut out = Vec::new();
        for rule in &self.learning_profile.rules {
            match rule.scope {
                LearningRuleScope::ThisSheetOnly => {
                    if rule.bound_structural_hash.as_deref()
                        != Some(signature.structural_hash.as_str())
                    {
                        continue;
                    }
                }
                LearningRuleScope::MatchingSheets => {
                    if let Some(dom) = rule.business_domain
                        && dom != signature.business_domain
                        && dom != SheetBusinessDomain::General
                    {
                        continue;
                    }
                }
            }

            // Check if the rule targets a column concept present in the sheet or is a sheet-wide preference.
            let matched_col = signature
                .columns
                .iter()
                .find(|c| c.canonical_concept == rule.target_concept)
                .map(|c| c.header_name.clone());
            let is_sheet_wide = matches!(
                rule.category,
                LearningCategory::DateFormat
                    | LearningCategory::CurrencyUnit
                    | LearningCategory::DuplicateHandling
                    | LearningCategory::MissingValueTreatment
                    | LearningCategory::ChartLayout
                    | LearningCategory::ExportFormat
            );
            if matched_col.is_none() && !is_sheet_wide {
                continue;
            }
            let target_label = matched_col.unwrap_or_else(|| rule.target_concept.clone());
            let requires_confirmation = rule.confidence_pct < 85
                || matches!(
                    rule.category,
                    LearningCategory::DuplicateHandling | LearningCategory::MissingValueTreatment
                );

            let avoided_note_en = rule
                .rejected_value
                .as_deref()
                .map(|rej| format!(" (avoiding previously rejected '{rej}')"))
                .unwrap_or_default();
            let avoided_note_ar = rule
                .rejected_value
                .as_deref()
                .map(|rej| format!(" (مع تجنّب الخيار المرفوض سابقًا «{rej}»)"))
                .unwrap_or_default();

            out.push(ExplainableLearnedSuggestion {
                influencing_rule_id: rule.rule_id.clone(),
                category: rule.category,
                target_header_or_concept: target_label.clone(),
                suggested_value: rule.preferred_value.clone(),
                avoided_value: rule.rejected_value.clone(),
                confidence_pct: rule.confidence_pct,
                requires_confirmation,
                why_en: format!(
                    "Suggested '{}' for '{}'{} — {} ({}% confidence).",
                    rule.preferred_value,
                    target_label,
                    avoided_note_en,
                    rule.provenance_en,
                    rule.confidence_pct
                ),
                why_ar: format!(
                    "تم اقتراح «{}» للعنصر «{}»{} — {} (ثقة {}%).",
                    rule.preferred_value,
                    target_label,
                    avoided_note_ar,
                    rule.provenance_ar,
                    rule.confidence_pct
                ),
            });
        }
        out
    }

    /// Removes (undoes) a single learned preference rule by ID (Section 6.3 & 6.5 Scenario D).
    ///
    /// # Errors
    /// Returns `SF-SYS-001` if `rule_id` does not exist.
    pub fn delete_learned_rule(&mut self, rule_id: &str, now_ms: u64) -> Result<(), String> {
        let before = self.learning_profile.rules.len();
        self.learning_profile.rules.retain(|r| r.rule_id != rule_id);
        if self.learning_profile.rules.len() == before {
            return Err("SF-SYS-001".to_owned());
        }
        self.learning_profile.profile_version =
            self.learning_profile.profile_version.saturating_add(1);
        self.learning_profile.updated_at_ms = now_ms;
        self.persist_learning_profile()
    }

    /// Completely resets the local [`PersonalLearningProfile`] while leaving saved per-sheet
    /// workflows and projects intact (Section 6.2).
    ///
    /// # Errors
    /// Returns `SF-SYS-016` or `SF-SYS-023` on disk error.
    pub fn reset_learning_profile(
        &mut self,
        now_ms: u64,
    ) -> Result<PersonalLearningProfile, String> {
        self.learning_profile = PersonalLearningProfile {
            schema_version: WORKFLOW_MEMORY_SCHEMA_VERSION,
            profile_version: self.learning_profile.profile_version.saturating_add(1),
            general_preferences: BTreeMap::new(),
            rules: Vec::new(),
            updated_at_ms: now_ms,
        };
        self.persist_learning_profile()?;
        Ok(self.learning_profile.clone())
    }

    fn persist_workflows(&self) -> Result<(), String> {
        let data = PersistedWorkflowStore {
            schema_version: WORKFLOW_MEMORY_SCHEMA_VERSION,
            workbook_contexts: self.workbook_contexts.clone(),
            sheet_workflows: self.sheet_workflows.clone(),
        };
        let encoded = serde_json::to_vec_pretty(&data).map_err(|_| "SF-SYS-001".to_owned())?;
        write_encrypted_or_plain(
            &self.storage_dir.join("workflow_memory.safi.enc"),
            &self.storage_dir.join("workflow_memory.json"),
            self.vault_key.as_ref(),
            &encoded,
        )
    }

    fn persist_learning_profile(&self) -> Result<(), String> {
        let encoded = serde_json::to_vec_pretty(&self.learning_profile)
            .map_err(|_| "SF-SYS-001".to_owned())?;
        write_encrypted_or_plain(
            &self.storage_dir.join("learning_profile.safi.enc"),
            &self.storage_dir.join("learning_profile.json"),
            self.vault_key.as_ref(),
            &encoded,
        )
    }
}

const fn compute_learning_confidence(signal: LearningSignalKind, evidence_count: u32) -> u8 {
    match signal {
        LearningSignalKind::ExplicitCorrection | LearningSignalKind::PreferredRule => 95,
        LearningSignalKind::ExplicitAcceptance | LearningSignalKind::ExplicitRejection => {
            if evidence_count >= 2 {
                90
            } else {
                82
            }
        }
        LearningSignalKind::RepeatedChoice => {
            if evidence_count >= 3 {
                86
            } else if evidence_count == 2 {
                74
            } else {
                60
            }
        }
    }
}

/// Builds a filename-independent semantic & structural signature for a worksheet.
#[must_use]
pub fn build_sheet_semantic_signature(
    sheet_name: &str,
    headers: &[String],
    overrides: &[SavedColumnDecision],
) -> SheetSemanticSignature {
    let mut columns = Vec::with_capacity(headers.len());
    let mut concept_set = BTreeSet::new();

    for h in headers {
        let norm = normalize_identifier_token(h);
        let default_concept = canonicalize_header_concept(h);
        let override_opt = overrides.iter().find(|o| {
            normalize_identifier_token(&o.header_name) == norm
                || o.canonical_concept == default_concept
        });

        let canonical_concept = override_opt
            .map(|o| o.canonical_concept.clone())
            .filter(|c| !c.is_empty())
            .unwrap_or(default_concept);
        let (default_role, default_sem) = infer_role_and_type_from_concept(&canonical_concept);
        let analytical_role = override_opt
            .map(|o| o.confirmed_role.clone())
            .filter(|r| !r.is_empty())
            .unwrap_or_else(|| default_role.to_owned());
        let semantic_type = override_opt
            .map(|o| o.confirmed_semantic_type.clone())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| default_sem.to_owned());

        let _ = concept_set.insert(canonical_concept.clone());
        columns.push(ColumnSemanticDescriptor {
            header_name: h.clone(),
            normalized_header: norm,
            canonical_concept,
            analytical_role,
            semantic_type,
            required: true,
        });
    }

    let canonical_concepts: Vec<String> = concept_set.into_iter().collect();
    let business_domain = classify_sheet_business_domain(sheet_name, &canonical_concepts);
    let structural_payload = format!(
        "domain:{:?}|concepts:{}",
        business_domain,
        canonical_concepts.join(",")
    );
    let structural_hash = sf_recipe::blake3_hex(structural_payload.as_bytes())[..24].to_owned();

    SheetSemanticSignature {
        business_domain,
        columns,
        canonical_concepts,
        structural_hash,
    }
}

fn normalize_identifier_token(raw: &str) -> String {
    let normalized = normalize_text(raw.trim(), NormOptions::default());
    normalized
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

/// Maps bilingual Arabic and English column headers to a canonical business concept so renamed
/// headers (`sale_date` -> `invoice_date` -> `تاريخ البيع`) match with high confidence.
#[must_use]
pub fn canonicalize_header_concept(raw: &str) -> String {
    let token = normalize_identifier_token(raw);
    if token.is_empty() {
        return "unknown_column".to_owned();
    }

    let has_any = |needles: &[&str]| needles.iter().any(|n| token == *n || token.contains(n));

    if has_any(&["hire_date", "join_date", "تاريخ_التعيين", "تاريخ_الالتحاق"])
    {
        return "hr_hire_date".to_owned();
    }
    if has_any(&[
        "date",
        "invoice_date",
        "sale_date",
        "order_date",
        "txn_date",
        "month",
        "period",
        "تاريخ",
        "التاريخ",
        "الشهر",
        "الفتره",
    ]) {
        return "temporal_date".to_owned();
    }
    if has_any(&[
        "salary",
        "wage",
        "payroll",
        "basic_pay",
        "net_pay",
        "الراتب",
        "راتب",
        "الاجر",
        "المرتب",
    ]) {
        return "hr_salary_amount".to_owned();
    }
    if has_any(&[
        "employee",
        "emp_id",
        "emp_name",
        "staff",
        "worker",
        "الموظف",
        "موظف",
        "العامل",
        "اسم_الموظف",
    ]) {
        return "hr_employee_identity".to_owned();
    }
    if has_any(&[
        "department",
        "dept",
        "job_title",
        "position",
        "القسم",
        "الاداره",
        "المسمى_الوظيفي",
        "الوظيفه",
    ]) {
        return "hr_department_role".to_owned();
    }
    if has_any(&[
        "sku",
        "item_code",
        "product_code",
        "barcode",
        "كود_الصنف",
        "رمز_الصنف",
        "باركود",
    ]) {
        return "inventory_sku_code".to_owned();
    }
    if has_any(&[
        "stock",
        "on_hand",
        "reorder",
        "inventory_qty",
        "available_qty",
        "المخزون",
        "رصيد_المخزون",
        "الكميه_المتاحه",
        "حد_الطلب",
    ]) {
        return "inventory_stock_qty".to_owned();
    }
    if has_any(&[
        "warehouse",
        "bin",
        "depot",
        "storage_loc",
        "المخزن",
        "المستودع",
        "موقع_التخزين",
    ]) {
        return "inventory_warehouse".to_owned();
    }
    if has_any(&[
        "revenue",
        "sales",
        "amount",
        "total_sales",
        "gross_sales",
        "net_sales",
        "invoice_total",
        "المبلغ",
        "المبيعات",
        "الايرادات",
        "اجمالي_المبيعات",
        "القيمه",
    ]) {
        return "sales_currency_amount".to_owned();
    }
    if has_any(&[
        "cost",
        "expense",
        "budget",
        "variance",
        "ledger",
        "debit",
        "credit",
        "التكلفه",
        "المصروفات",
        "الميزانيه",
        "مدين",
        "دائن",
    ]) {
        return "finance_ledger_amount".to_owned();
    }
    if has_any(&["price", "unit_price", "rate", "سعر", "السعر", "سعر_الوحده"]) {
        return "unit_price".to_owned();
    }
    if has_any(&["qty", "quantity", "units", "count", "الكميه", "العدد"]) {
        return "quantity_measure".to_owned();
    }
    if has_any(&[
        "region",
        "branch",
        "city",
        "store",
        "location",
        "territory",
        "المنطقه",
        "الفرع",
        "المدينه",
        "المعرض",
        "المحافظه",
    ]) {
        return "dimension_branch_region".to_owned();
    }
    if has_any(&[
        "customer",
        "client",
        "buyer",
        "account_name",
        "العميل",
        "عميل",
        "المشتري",
    ]) {
        return "sales_customer_party".to_owned();
    }
    if has_any(&[
        "product",
        "item",
        "category",
        "brand",
        "المنتج",
        "الصنف",
        "الفئه",
    ]) {
        return "dimension_product_item".to_owned();
    }
    if has_any(&[
        "id",
        "code",
        "ref",
        "order_id",
        "invoice_id",
        "معرف",
        "رقم",
        "كود",
    ]) {
        return "record_identifier".to_owned();
    }

    token
}

fn infer_role_and_type_from_concept(concept: &str) -> (&'static str, &'static str) {
    match concept {
        "temporal_date" | "hr_hire_date" => ("Temporal", "CalendarDate"),
        "sales_currency_amount" | "hr_salary_amount" | "finance_ledger_amount" => {
            ("Measure", "CurrencyAmount")
        }
        "unit_price" => ("Measure", "UnitPrice"),
        "quantity_measure" | "inventory_stock_qty" => ("Measure", "Quantity"),
        "hr_employee_identity" => ("Dimension", "PersonName"),
        "hr_department_role" | "dimension_product_item" => ("Dimension", "CategoryOrStatus"),
        "dimension_branch_region" | "inventory_warehouse" => ("Dimension", "GeographicLocation"),
        "sales_customer_party" => ("Dimension", "OrganizationOrBranch"),
        "inventory_sku_code" | "record_identifier" => ("Identifier", "IdentifierCode"),
        _ => ("Dimension", "GeneralDimension"),
    }
}

fn classify_sheet_business_domain(sheet_name: &str, concepts: &[String]) -> SheetBusinessDomain {
    let mut sales_votes = 0u32;
    let mut hr_votes = 0u32;
    let mut inv_votes = 0u32;
    let mut fin_votes = 0u32;

    for c in concepts {
        if c.starts_with("hr_") {
            hr_votes += 3;
        } else if c.starts_with("inventory_") {
            inv_votes += 3;
        } else if c.starts_with("sales_") {
            sales_votes += 3;
        } else if c.starts_with("finance_") {
            fin_votes += 3;
        } else if c == "unit_price" || c == "dimension_product_item" {
            sales_votes += 1;
            inv_votes += 1;
        } else if c == "dimension_branch_region" {
            sales_votes += 1;
        }
    }

    // Secondary weak signal from sheet name only when column concepts are tied.
    let sheet_norm = normalize_identifier_token(sheet_name);
    if hr_votes == 0 && inv_votes == 0 && sales_votes == 0 && fin_votes == 0 {
        if sheet_norm.contains("emp")
            || sheet_norm.contains("hr")
            || sheet_norm.contains("payroll")
            || sheet_norm.contains("موظف")
            || sheet_norm.contains("رواتب")
        {
            return SheetBusinessDomain::Employees;
        }
        if sheet_norm.contains("inv")
            || sheet_norm.contains("stock")
            || sheet_norm.contains("مخزون")
            || sheet_norm.contains("مستودع")
        {
            return SheetBusinessDomain::Inventory;
        }
        if sheet_norm.contains("sale")
            || sheet_norm.contains("rev")
            || sheet_norm.contains("مبيعات")
        {
            return SheetBusinessDomain::Sales;
        }
    }

    let max_votes = sales_votes.max(hr_votes).max(inv_votes).max(fin_votes);
    if max_votes == 0 {
        SheetBusinessDomain::General
    } else if hr_votes == max_votes && hr_votes > sales_votes && hr_votes > inv_votes {
        SheetBusinessDomain::Employees
    } else if inv_votes == max_votes && inv_votes > sales_votes && inv_votes > hr_votes {
        SheetBusinessDomain::Inventory
    } else if fin_votes == max_votes && fin_votes > sales_votes {
        SheetBusinessDomain::Finance
    } else {
        SheetBusinessDomain::Sales
    }
}

fn score_signature_compatibility(
    saved: &SheetSemanticSignature,
    incoming: &SheetSemanticSignature,
) -> (u8, BTreeMap<String, String>, Vec<String>, Vec<String>) {
    let mut mapping = BTreeMap::new();
    let mut used_incoming = BTreeSet::new();
    let mut missing_required = Vec::new();

    // Pass 1: exact normalized header match.
    for s_col in &saved.columns {
        if let Some((idx, i_col)) = incoming.columns.iter().enumerate().find(|(idx, ic)| {
            !used_incoming.contains(idx) && ic.normalized_header == s_col.normalized_header
        }) {
            let _ = used_incoming.insert(idx);
            let _ = mapping.insert(s_col.header_name.clone(), i_col.header_name.clone());
        }
    }

    // Pass 2: canonical concept + role match (handles renamed headers & Arabic/English aliases).
    for s_col in &saved.columns {
        if mapping.contains_key(&s_col.header_name) {
            continue;
        }
        if let Some((idx, i_col)) = incoming.columns.iter().enumerate().find(|(idx, ic)| {
            !used_incoming.contains(idx) && ic.canonical_concept == s_col.canonical_concept
        }) {
            let _ = used_incoming.insert(idx);
            let _ = mapping.insert(s_col.header_name.clone(), i_col.header_name.clone());
        } else if s_col.required {
            missing_required.push(s_col.header_name.clone());
        }
    }

    let added_columns: Vec<String> = incoming
        .columns
        .iter()
        .enumerate()
        .filter(|(idx, _)| !used_incoming.contains(idx))
        .map(|(_, c)| c.header_name.clone())
        .collect();

    let saved_len = saved.columns.len().max(1);
    let incoming_len = incoming.columns.len().max(1);
    let matched_count = mapping.len();

    // Dice similarity over columns `(2 * matched) / (saved_len + incoming_len)`.
    let dice = (matched_count.saturating_mul(200)) / (saved_len.saturating_add(incoming_len));
    let mut score = u8::try_from(dice.min(100)).unwrap_or(100);

    if saved.business_domain == incoming.business_domain
        && saved.business_domain != SheetBusinessDomain::General
        && matched_count >= 2
    {
        score = score.saturating_add(6).min(100);
    }
    if !missing_required.is_empty() {
        score = score.saturating_sub(18);
    }

    (score, mapping, added_columns, missing_required)
}

fn read_optional_encrypted_or_plain(
    enc_path: &Path,
    json_path: &Path,
    vault_key: Option<&VaultKey>,
) -> Result<Option<String>, String> {
    if enc_path.exists() {
        let key = vault_key.ok_or_else(|| "SF-SYS-024".to_owned())?;
        let cipher = fs::read(enc_path).map_err(|_| "SF-SYS-001".to_owned())?;
        let plain = decrypt_vault_payload(key, &cipher).map_err(|_| "SF-SYS-024".to_owned())?;
        let text = String::from_utf8(plain).map_err(|_| "SF-SYS-024".to_owned())?;
        Ok(Some(text))
    } else if json_path.exists() {
        let text = fs::read_to_string(json_path).map_err(|_| "SF-SYS-001".to_owned())?;
        Ok(Some(text))
    } else {
        Ok(None)
    }
}

fn write_encrypted_or_plain(
    enc_path: &Path,
    json_path: &Path,
    vault_key: Option<&VaultKey>,
    plain_bytes: &[u8],
) -> Result<(), String> {
    if let Some(key) = vault_key {
        let cipher =
            encrypt_vault_payload(key, plain_bytes).map_err(|_| "SF-SYS-023".to_owned())?;
        write_atomic_bytes(enc_path, &cipher)?;
        if json_path.exists() {
            let _ = fs::remove_file(json_path);
        }
        Ok(())
    } else {
        write_atomic_bytes(json_path, plain_bytes)
    }
}

fn write_atomic_bytes(target: &Path, contents: &[u8]) -> Result<(), String> {
    let dir = target.parent().ok_or_else(|| "SF-SYS-016".to_owned())?;
    fs::create_dir_all(dir).map_err(|_| "SF-SYS-016".to_owned())?;
    let name = target
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("safi_wf");
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = dir.join(format!(".{name}.{stamp}.tmp"));
    let write_res = (|| -> std::io::Result<()> {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(contents)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, target)
    })();
    if write_res.is_err() {
        let _ = fs::remove_file(&tmp);
        return Err("SF-SYS-016".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_test_dir(tag: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("safi_wfm_{tag}_{stamp}"))
    }

    #[test]
    fn scenario_a_b_c_d_e_workflow_memory_and_continuous_learning_end_to_end() {
        let dir = temp_test_dir("scenarios_abcde");
        let salt = [0x42u8; 16];
        let Ok(vault_key) = VaultKey::derive_from_passphrase("SafiLocalVault#2026", &salt) else {
            panic!("vault key derivation must succeed");
        };

        // ====================================================================
        // SCENARIO A — Monthly Sales Workbook (save, restart, renamed file & reordered Arabic/English headers)
        // ====================================================================
        let sales_wf_id = {
            let mut store =
                match LocalWorkflowMemoryStore::open(dir.clone(), Some(vault_key.clone())) {
                    Ok(s) => s,
                    Err(e) => panic!("open store failed: {e}"),
                };

            let nov_headers = vec![
                "sale_date".to_owned(),
                "region".to_owned(),
                "customer".to_owned(),
                "amount".to_owned(),
            ];
            let saved = match store.save_sheet_workflow(
                None,
                "Monthly Sales Pipeline",
                "proj_nov_sales",
                "November_Sales",
                "tbl_1",
                &nov_headers,
                vec![
                    SavedQuestionDecision {
                        question_id: "q_date_granularity_November_Sales_tbl_1".to_owned(),
                        semantic_topic: "date_granularity".to_owned(),
                        canonical_concept: "temporal_date".to_owned(),
                        action: "answered".to_owned(),
                        answer: "monthly".to_owned(),
                        custom_text: String::new(),
                        updated_at_ms: 1_000,
                    },
                    SavedQuestionDecision {
                        question_id: "q_currency_unit_November_Sales_tbl_1".to_owned(),
                        semantic_topic: "currency_unit".to_owned(),
                        canonical_concept: "sales_currency_amount".to_owned(),
                        action: "answered".to_owned(),
                        answer: "EGP".to_owned(),
                        custom_text: String::new(),
                        updated_at_ms: 1_000,
                    },
                ],
                vec![SavedColumnDecision {
                    header_name: "sale_date".to_owned(),
                    canonical_concept: "temporal_date".to_owned(),
                    confirmed_role: "Temporal".to_owned(),
                    confirmed_semantic_type: "CalendarDate".to_owned(),
                    date_format: Some("YYYY-MM-DD".to_owned()),
                    currency_or_unit: None,
                    is_identifier: false,
                }],
                "exact_key",
                "fill_zero",
                vec!["Exclude cancelled orders".to_owned()],
                vec![SavedCleaningOperation {
                    step_id: "op_1".to_owned(),
                    kind: "trim_column".to_owned(),
                    target_column: Some("region".to_owned()),
                    target_concept: Some("dimension_branch_region".to_owned()),
                    parameter: None,
                }],
                vec![SavedMetricDefinition {
                    id: "total_sales_egp".to_owned(),
                    name: "Total Sales (EGP)".to_owned(),
                    unit: Some("EGP".to_owned()),
                    formula: "SUM(amount)".to_owned(),
                }],
                SavedDashboardPreferences {
                    preferred_chart_types: vec!["bar".to_owned(), "waterfall".to_owned()],
                    preferred_group_by_concept: Some("dimension_branch_region".to_owned()),
                    preferred_template_id: Some("executive_brief".to_owned()),
                    preferred_export_format: Some("pdf".to_owned()),
                },
                vec![],
                1_000,
            ) {
                Ok(r) => r,
                Err(e) => panic!("save sales workflow failed: {e}"),
            };
            assert_eq!(saved.business_domain, SheetBusinessDomain::Sales);
            saved.workflow_id
        };

        // Close & restart application (re-open encrypted store from disk)
        let mut store = match LocalWorkflowMemoryStore::open(dir.clone(), Some(vault_key.clone())) {
            Ok(s) => s,
            Err(e) => panic!("reopen store after restart failed: {e}"),
        };
        assert!(
            dir.join("workflow_memory.safi.enc").is_file(),
            "workflow memory must be encrypted at rest"
        );

        // Next month: user imports `Sales_Report_Final.xlsx` with sheet `" مبيعات ديسمبر "` and reordered columns!
        let dec_headers = vec![
            "الفرع".to_owned(),        // alias of `region`
            "المبيعات".to_owned(),     // alias of `amount`
            "invoice_date".to_owned(), // alias of `sale_date`
            "العميل".to_owned(),       // alias of `customer`
        ];
        let match_dec =
            store.match_incoming_sheet("مبيعات ديسمبر", &dec_headers, Some("YYYY-MM-DD"));
        assert_eq!(
            match_dec.matched_workflow_id.as_deref(),
            Some(sales_wf_id.as_str())
        );
        assert_eq!(match_dec.match_tier, WorkflowMatchTier::High);
        assert!(match_dec.auto_replay_eligible);
        assert_eq!(match_dec.reused_question_resolutions.len(), 2);
        assert!(
            match_dec
                .suppressed_question_topics
                .contains(&"date_granularity".to_owned())
        );
        assert!(
            match_dec
                .suppressed_question_topics
                .contains(&"currency_unit".to_owned())
        );

        // ====================================================================
        // SCENARIO B — Multiple Sheets in One Workbook (Sales vs Employees vs Inventory isolation)
        // ====================================================================
        let emp_headers = vec![
            "employee_id".to_owned(),
            "اسم_الموظف".to_owned(),
            "القسم".to_owned(),
            "الراتب".to_owned(),
            "hire_date".to_owned(),
        ];
        let inv_headers = vec![
            "sku".to_owned(),
            "الصنف".to_owned(),
            "المخزن".to_owned(),
            "رصيد_المخزون".to_owned(),
        ];

        // Before saving Employees or Inventory workflows, matching them MUST NOT match the Sales workflow!
        let emp_before = store.match_incoming_sheet("Sheet_A", &emp_headers, None);
        assert_eq!(emp_before.detected_domain, SheetBusinessDomain::Employees);
        assert!(
            emp_before.matched_workflow_id.is_none(),
            "Sales workflow must NEVER cross-contaminate Employees sheet"
        );

        let inv_before = store.match_incoming_sheet("Sheet_B", &inv_headers, None);
        assert_eq!(inv_before.detected_domain, SheetBusinessDomain::Inventory);
        assert!(
            inv_before.matched_workflow_id.is_none(),
            "Sales workflow must NEVER cross-contaminate Inventory sheet"
        );

        // Save independent Employees and Inventory workflows
        let emp_wf = match store.save_sheet_workflow(
            None,
            "HR Payroll Workflow",
            "proj_enterprise",
            "Employees_2026",
            "tbl_emp",
            &emp_headers,
            vec![],
            vec![],
            "exact_key",
            "flag_only",
            vec![],
            vec![],
            vec![],
            SavedDashboardPreferences::default(),
            vec![],
            2_000,
        ) {
            Ok(w) => w,
            Err(e) => panic!("save emp wf: {e}"),
        };
        let inv_wf = match store.save_sheet_workflow(
            None,
            "Warehouse Stock Workflow",
            "proj_enterprise",
            "Inventory_2026",
            "tbl_inv",
            &inv_headers,
            vec![],
            vec![],
            "exact_key",
            "fill_zero",
            vec![],
            vec![],
            vec![],
            SavedDashboardPreferences::default(),
            vec![],
            3_000,
        ) {
            Ok(w) => w,
            Err(e) => panic!("save inv wf: {e}"),
        };

        // Rename and reorder sheets: each sheet still matches ONLY its own workflow!
        assert_eq!(
            store
                .match_incoming_sheet("Renamed_Tab_99", &emp_headers, None)
                .matched_workflow_id
                .as_deref(),
            Some(emp_wf.workflow_id.as_str())
        );
        assert_eq!(
            store
                .match_incoming_sheet("Renamed_Tab_01", &inv_headers, None)
                .matched_workflow_id
                .as_deref(),
            Some(inv_wf.workflow_id.as_str())
        );

        // ====================================================================
        // SCENARIO C — Structural Change (New Column Added + Date Format Shift)
        // ====================================================================
        let evolved_sales_headers = vec![
            "sale_date".to_owned(),
            "region".to_owned(),
            "customer".to_owned(),
            "amount".to_owned(),
            "discount_code".to_owned(), // newly added column
        ];
        let match_evolved =
            store.match_incoming_sheet("Jan_Sales", &evolved_sales_headers, Some("DD/MM/YYYY"));
        assert_eq!(
            match_evolved.matched_workflow_id.as_deref(),
            Some(sales_wf_id.as_str())
        );
        assert_eq!(
            match_evolved.match_tier,
            WorkflowMatchTier::Medium,
            "Structural change must downgrade tier to Medium and ask only about new/changed elements"
        );
        assert!(!match_evolved.auto_replay_eligible);
        assert_eq!(
            match_evolved.added_columns,
            vec!["discount_code".to_owned()]
        );
        // Currency decision is still reused, while date_granularity is re-opened because date format shifted!
        assert!(
            match_evolved
                .suppressed_question_topics
                .contains(&"currency_unit".to_owned())
        );
        assert!(
            !match_evolved
                .suppressed_question_topics
                .contains(&"date_granularity".to_owned())
        );

        // ====================================================================
        // SCENARIO D — User Correction, Continuous Learning & Undo/Delete Learned Rule
        // ====================================================================
        let learned_rule = match store.record_learning_feedback(
            LearningSignalKind::ExplicitCorrection,
            LearningCategory::ColumnClassification,
            LearningRuleScope::MatchingSheets,
            None,
            Some(SheetBusinessDomain::Sales),
            "record_identifier",
            "Identifier (Never Sum)",
            Some("Measure (SUM)".to_owned()),
            "Order IDs are numeric codes and must never be summed",
            "Jan_Sales",
            4_000,
        ) {
            Ok(r) => r,
            Err(e) => panic!("record learning feedback: {e}"),
        };
        assert_eq!(learned_rule.confidence_pct, 95);

        let sales_with_id = vec![
            "sale_date".to_owned(),
            "order_id".to_owned(),
            "region".to_owned(),
            "amount".to_owned(),
        ];
        let match_with_learned = store.match_incoming_sheet("Feb_Sales", &sales_with_id, None);
        assert_eq!(match_with_learned.learned_suggestions.len(), 1);
        assert_eq!(
            match_with_learned.learned_suggestions[0].influencing_rule_id,
            learned_rule.rule_id
        );
        assert_eq!(
            match_with_learned.learned_suggestions[0]
                .avoided_value
                .as_deref(),
            Some("Measure (SUM)")
        );

        // Undo / delete the learned preference rule
        if let Err(e) = store.delete_learned_rule(&learned_rule.rule_id, 5_000) {
            panic!("delete learned rule failed: {e}");
        }
        let match_after_undo = store.match_incoming_sheet("Feb_Sales", &sales_with_id, None);
        assert!(
            match_after_undo.learned_suggestions.is_empty(),
            "Deleted learned preference must no longer suggest"
        );

        // ====================================================================
        // SCENARIO E — Versioning, Rollback & Encrypted Persistence Integrity
        // ====================================================================
        let updated_sales = match store.save_sheet_workflow(
            Some(&sales_wf_id),
            "Monthly Sales Pipeline v2",
            "proj_nov_sales",
            "Feb_Sales",
            "tbl_1",
            &evolved_sales_headers,
            vec![],
            vec![],
            "fuzzy_merge",
            "fill_median",
            vec![],
            vec![],
            vec![],
            SavedDashboardPreferences::default(),
            vec![],
            6_000,
        ) {
            Ok(u) => u,
            Err(e) => panic!("update v2: {e}"),
        };
        assert_eq!(updated_sales.workflow_version, 2);
        assert_eq!(updated_sales.duplicate_policy, "fuzzy_merge");

        let restored = match store.restore_workflow_version(&sales_wf_id, 1, 7_000) {
            Ok(r) => r,
            Err(e) => panic!("restore v1: {e}"),
        };
        assert_eq!(restored.workflow_version, 3);
        assert_eq!(restored.duplicate_policy, "exact_key");
        assert_eq!(restored.question_resolutions.len(), 2);

        let _ = fs::remove_dir_all(&dir);
    }
}
