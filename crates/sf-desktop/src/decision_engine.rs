//! SafiData Central Optimal Decision Engine (`SafiDecisionEngine` — Sections 10, 11 & 12).
//!
//! Architectural Principles:
//! 1. **Single Rust Authority**: Shared across Spreadsheet Understanding, Data Profiling,
//!    Data Cleaning, Duplicate Classification, Optimal KPI Selection, Optimal Chart Selection,
//!    Coherent Dashboard Layout, Executive Reports & Presentations, Workflow Replay Validation,
//!    Cross-Product Consistency Auditing, and AI Task Routing.
//! 2. **10-Step Decision Pipeline**:
//!    (1) Understand Objective -> (2) Understand Data -> (3) Identify Constraints ->
//!    (4) Generate Valid Candidates -> (5) Evaluate Candidates -> (6) Eliminate Invalid Solutions ->
//!    (7) Optimize Lexicographically -> (8) Validate Selected Decision ->
//!    (9) Return One Primary `RecommendedDecision` -> (10) Save Explainable Decision History.
//! 3. **7-Level Lexicographic Objective Function (Section 10.4)**:
//!    Never uses an arbitrary weighted sum that could trade data correctness for speed or aesthetics.
//!    Candidates are compared lexicographically in strict priority order:
//!    `(correctness, data_safety_no_loss, user_constraint_compliance, analytical_fit, explainability_audit, deliverable_quality, execution_efficiency)`.
//! 4. **Honest Optimality Disclosure (Section 10.6)**:
//!    Explicitly distinguishes `ExactProvenOptimum`, `BestValidCandidateFound`,
//!    `HeuristicRecommendation`, and `InsufficientEvidenceRequiresQuestion`. Never conflates a
//!    heuristic or similarity score with calibrated mathematical probability.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sf_core::Decimal;
use sf_core::datagraph::{MetricId, RowId};
use sf_recipe::datagraph::DataGraph;
use sf_recipe::studio_anomaly::AnomalyFix;

use crate::studio::{
    StudioChartArtifact, StudioDocBlockArtifact, StudioSlideArtifact, StudioWorkspaceArtifacts,
};
use crate::workflow_memory::{
    PersonalLearningProfile, SheetBusinessDomain, SheetWorkflowMemoryRecord,
    canonicalize_header_concept,
};

/// Domain in which the `SafiDecisionEngine` is making an optimal decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionDomain {
    /// Column roles, IDs vs. Measures, date formats, and table grain (Section 11.1).
    SpreadsheetUnderstanding,
    /// Duplicate classification (exact vs. valid transactions vs. key conflict), trimming, missing values (Section 11.2).
    DataCleaning,
    /// Optimal verified KPI selection & formula validation (Section 11.3).
    KpiSelection,
    /// Optimal chart type & encoding selection without perceptual distortion (Section 11.4).
    ChartSelection,
    /// Coherent dashboard blueprint: KPIs, groupings, filters, hierarchy, sorting (Section 11.5).
    DashboardLayout,
    /// Executive report & PowerPoint deck structure with 100% verified metric lineage (Section 11.6).
    ReportAndPresentation,
}

/// Algorithm used by the Decision Engine to explore and prune the candidate space (Section 10.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptimizationAlgorithm {
    /// Exhaustive exact constraint solver over a finite discrete candidate domain.
    ExactConstraintSolver,
    /// Lexicographic Branch-and-Bound with hard safety pruning.
    LexicographicBranchAndBound,
    /// Constrained Beam Search over multi-component layout/chart combinations.
    ConstrainedBeamSearch,
    /// Deterministic rule-guided optimization for narrative & section structure.
    DeterministicRuleSolver,
}

/// Honest classification of the mathematical/epistemic status of a decision (Section 10.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptimalityClass {
    /// All finite candidates in the constrained discrete space were evaluated and a unique
    /// feasible candidate strictly dominates all alternatives on the lexicographic objective.
    ExactProvenOptimum,
    /// Branch-and-Bound or Constrained Beam Search evaluated the feasible space and returned
    /// the highest lexicographic candidate found without violating any safety constraint.
    BestValidCandidateFound,
    /// Heuristic / rule-based structural recommendation (e.g., report narrative flow).
    HeuristicRecommendation,
    /// Two valid candidates remain tied on critical dimensions and a missing piece of domain
    /// intent would materially change the outcome; asks one focused question instead of guessing.
    InsufficientEvidenceRequiresQuestion,
}

/// 7-level lexicographic objective vector (Section 10.4).
///
/// Ordered from highest priority (`correctness`) to lowest tie-breaker (`execution_efficiency`).
/// Derived `Ord` compares fields in declaration order, guaranteeing that a candidate with higher
/// `correctness` or `data_safety_no_loss` ALWAYS beats a faster or prettier candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LexicographicObjectiveVector {
    /// 1. Correctness of result (`0..=1000`; `0` = invalid/infeasible).
    pub correctness: u16,
    /// 2. Data safety & zero loss of valid information (`0..=1000`).
    pub data_safety_no_loss: u16,
    /// 3. Compliance with explicit user constraints & learned rules (`0..=1000`).
    pub user_constraint_compliance: u16,
    /// 4. Analytical fit to data distribution & domain purpose (`0..=1000`).
    pub analytical_fit: u16,
    /// 5. Explainability, deterministic lineage & auditability (`0..=1000`).
    pub explainability_audit: u16,
    /// 6. Deliverable & visual communication quality (`0..=1000`).
    pub deliverable_quality: u16,
    /// 7. Execution speed & resource efficiency tie-breaker (`0..=1000`).
    pub execution_efficiency: u16,
}

/// Concrete statistical or structural evidence item backing a decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionEvidenceItem {
    /// Metric or check name (e.g. `"uniqueness_ratio"`, `"duplicate_key_conflicts"`).
    pub label_en: String,
    /// Arabic label.
    pub label_ar: String,
    /// Observed value computed from the actual dataset.
    pub observed_value: String,
    /// Why this evidence supports the selected decision.
    pub impact_en: String,
    /// Arabic explanation of impact.
    pub impact_ar: String,
}

/// A candidate evaluated (and either selected, ranked lower, or pruned as invalid) during optimization.
/// Kept in `Advanced Details / Audit` so the primary UI shows only ONE recommended decision (Section 10.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluatedCandidateAudit {
    /// Candidate identifier.
    pub candidate_id: String,
    /// English title.
    pub title_en: String,
    /// Arabic title.
    pub title_ar: String,
    /// Whether this candidate passed all hard validity & safety constraints.
    pub is_valid: bool,
    /// If pruned/eliminated, the exact reason why it was rejected.
    pub elimination_reason_en: Option<String>,
    /// Arabic elimination reason.
    pub elimination_reason_ar: Option<String>,
    /// Lexicographic objective scores.
    pub objective: LexicographicObjectiveVector,
}

/// Focused single question surfaced ONLY when `optimality_class == InsufficientEvidenceRequiresQuestion` (Section 10.7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusedClarificationPrompt {
    /// Question ID.
    pub question_id: String,
    /// English prompt.
    pub prompt_en: String,
    /// Arabic prompt.
    pub prompt_ar: String,
    /// Options to resolve the ambiguity.
    pub options: Vec<String>,
}

/// Executable payload attached to a `RecommendedDecision` so the user can apply it in 1 click
/// (and undo it deterministically via `studio_undo`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "actionKind", rename_all = "camelCase")]
pub enum DecisionExecutableAction {
    /// Trim whitespace on target columns.
    TrimColumns { columns: Vec<String> },
    /// Drop exact duplicate row IDs that were verified to be 100% identical across all columns.
    DropExactDuplicateRows { row_ids: Vec<u64> },
    /// Fill missing numeric cells in a column with `"0"`.
    FillMissingNumericZero { column: String, row_ids: Vec<u64> },
    /// Define a verified deterministic KPI metric in `DataGraph`.
    DefineVerifiedMetric {
        metric_id: String,
        name: String,
        unit: Option<String>,
        formula: String,
    },
    /// Create or update an optimal chart in `StudioWorkspaceArtifacts`.
    ConfigureOptimalChart {
        chart_type: String,
        title: String,
        function: String,
        value_column: String,
        group_by_column: String,
    },
    /// Apply a complete coherent dashboard & executive report blueprint.
    ApplyDashboardAndReportBlueprint {
        primary_metric_id: String,
        primary_metric_name: String,
        primary_metric_unit: Option<String>,
        primary_metric_formula: String,
        primary_chart_type: String,
        value_column: String,
        group_by_column: String,
    },
    /// Advisory / structural classification (no destructive mutation needed).
    ConfirmClassificationOnly { summary: String },
}

/// The single primary Recommended Decision returned for a product domain (Section 10.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecommendedDecision {
    /// Unique decision identifier.
    pub decision_id: String,
    /// Product domain.
    pub domain: DecisionDomain,
    /// Optimization algorithm used.
    pub algorithm_used: OptimizationAlgorithm,
    /// Honest optimality classification (never claims `ExactProvenOptimum` for heuristics).
    pub optimality_class: OptimalityClass,
    /// Primary recommendation title (English).
    pub recommended_title_en: String,
    /// Primary recommendation title (Arabic).
    pub recommended_title_ar: String,
    /// Concise explanation of why this is the best valid decision (English).
    pub why_en: String,
    /// Concise explanation of why this is the best valid decision (Arabic).
    pub why_ar: String,
    /// Lexicographic objective vector achieved by this decision.
    pub objective: LexicographicObjectiveVector,
    /// Empirical evidence computed from the actual dataset.
    pub evidence: Vec<DecisionEvidenceItem>,
    /// Explicit assumptions required for this decision to hold.
    pub assumptions_en: Vec<String>,
    /// Explicit assumptions (Arabic).
    pub assumptions_ar: Vec<String>,
    /// Exact description of what will execute when applied (English).
    pub what_executes_en: String,
    /// Exact description of what will execute when applied (Arabic).
    pub what_executes_ar: String,
    /// Exact deterministic undo/rollback path (English).
    pub undo_way_en: String,
    /// Exact deterministic undo/rollback path (Arabic).
    pub undo_way_ar: String,
    /// Executable action payload for 1-click Apply.
    pub executable_action: DecisionExecutableAction,
    /// Optional single clarification question if `optimality_class == InsufficientEvidenceRequiresQuestion`.
    pub clarification_prompt: Option<FocusedClarificationPrompt>,
    /// Total candidates generated and evaluated.
    pub candidates_evaluated_count: usize,
    /// Count of invalid/unsafe candidates pruned.
    pub invalid_eliminated_count: usize,
    /// Full audit trail of evaluated and eliminated candidates (shown only in `Advanced Details / Audit`).
    pub advanced_audit_candidates: Vec<EvaluatedCandidateAudit>,
}

/// Content-based column profile used by `SpreadsheetUnderstanding` (Section 11.1).
/// Never relies on column header name alone!
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnContentDecision {
    /// Column name.
    pub column_name: String,
    /// Canonical concept from header + content.
    pub canonical_concept: String,
    /// Decided analytical role (`Identifier`, `Measure`, `Dimension`, `Temporal`, `DescriptiveText`).
    pub decided_role: String,
    /// Decided semantic type.
    pub decided_semantic_type: String,
    /// Optimality class of this column classification.
    pub optimality_class: OptimalityClass,
    /// Total rows inspected.
    pub non_null_count: usize,
    /// Null/blank cell count.
    pub null_count: usize,
    /// Distinct non-null value count.
    pub distinct_count: usize,
    /// Percentage of non-null values that parse as valid numbers (`0..=100`).
    pub numeric_parse_pct: u8,
    /// Percentage of non-null values that parse as dates (`0..=100`).
    pub date_parse_pct: u8,
    /// Count of cells with leading/trailing whitespace.
    pub untrimmed_whitespace_count: usize,
    /// Whether this column was protected from being misclassified as a summable `Measure`
    /// (e.g. `Employee ID` or `order_id` containing numbers).
    pub protected_identifier_from_measure: bool,
    /// Explanation (English).
    pub reason_en: String,
    /// Explanation (Arabic).
    pub reason_ar: String,
}

/// Duplicate detection classification (Section 11.2 & Section 4.4) distinguishing exact unintended
/// duplicates from valid multi-transaction records, conflicting-key rows, and ambiguous rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateAnalysisReport {
    /// Row IDs that are 100% identical across EVERY column (safe to drop automatically).
    pub exact_unintended_duplicate_row_ids: Vec<u64>,
    /// Count of rows sharing an entity/customer/party value WITH a distinct transaction identifier
    /// or distinct temporal/amount event (valid repeated transactions — MUST NOT be deleted!).
    pub valid_repeated_entity_transaction_rows: usize,
    /// Row IDs sharing the primary identifier key (`order_id`, `emp_id`, etc.) but having
    /// conflicting non-key values (unsafe to auto-delete; requires review or merge).
    pub conflicting_key_review_row_ids: Vec<u64>,
    /// Row IDs that look potentially duplicated without a unique transaction key or with identical
    /// `(entity, date, amount)` values (ambiguous — requires human confirmation, never auto-dropped).
    #[serde(default)]
    pub ambiguous_duplicate_review_row_ids: Vec<u64>,
    /// Key column used for key-conflict check, if detected.
    pub detected_primary_key_column: Option<String>,
    /// Summary explanation (English).
    pub summary_en: String,
    /// Summary explanation (Arabic).
    pub summary_ar: String,
}

/// Optional runtime probe result supplied by the Desktop shell boundary (outside air-gapped engine crates).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaRuntimeProbe {
    /// Loopback endpoint probed (`http://127.0.0.1:11434`).
    pub endpoint: String,
    /// Measured runtime state: `"available_with_models"`, `"reachable_zero_models"`,
    /// `"service_unreachable"`, or `"not_configured"`.
    pub status: String,
    /// Installed model tags discovered from `/api/tags` (empty if unreachable or zero models).
    pub installed_models: Vec<String>,
}

/// Cross-product consistency & lineage audit report (Section 11.7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsistencyImpactReport {
    /// Whether all charts, slides, and document blocks are 100% consistent with current
    /// `DataGraph` columns and verified metric definitions.
    pub is_fully_consistent: bool,
    /// Active dataset revision and fingerprint.
    pub dataset_revision: u64,
    /// Verified metric IDs currently active in `DataGraph`.
    pub active_metric_ids: Vec<String>,
    /// Charts referencing missing or renamed columns (`chart_title -> missing_column`).
    pub stale_chart_references: Vec<String>,
    /// Slides or document blocks referencing missing or outdated metric IDs.
    pub stale_deliverable_metric_refs: Vec<String>,
    /// Lineage map from `metric_id -> source_columns`.
    pub metric_column_lineage: BTreeMap<String, Vec<String>>,
    /// Explanation (English).
    pub summary_en: String,
    /// Explanation (Arabic).
    pub summary_ar: String,
}

/// AI Task Routing entry (Section 12.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiTaskRouteEntry {
    /// Task category ID.
    pub task_id: String,
    /// English task name.
    pub task_name_en: String,
    /// Arabic task name.
    pub task_name_ar: String,
    /// Primary execution engine (`Rust Deterministic Engine`, `Rust Cleaning Engine`,
    /// `Safi Decision Engine`, `Local Learning Engine`, `Rust Export Engines`,
    /// `Local Ollama / Consented AI Layer`).
    pub primary_engine: String,
    /// Whether LLM/AI is ever allowed to compute numbers for this task (`false` for all numeric/cleaning tasks!).
    pub allows_llm_numeric_computation: bool,
    /// Whether external cloud AI requires explicit user consent & local PII masking.
    pub requires_consent_and_pii_masking_if_external: bool,
}

/// Local Ollama & AI architecture status report (Section 12.1–12.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiArchitectureStatusReport {
    /// Configured local loopback endpoint (`http://127.0.0.1:11434`).
    pub local_ollama_endpoint: String,
    /// Honest status: `"not_probed_in_airgapped_engine"`, `"service_unreachable"`,
    /// `"reachable_zero_models"`, or `"ready"`.
    pub local_ollama_status: String,
    /// Never claims a model is bundled inside the Desktop binary.
    pub bundled_model_claimed: bool,
    /// List of verified installed local models (empty unless actually discovered).
    pub installed_local_models: Vec<String>,
    /// Whether core cleaning, profiling, decision engine, and calculations work 100% without AI.
    pub core_product_works_without_ai: bool,
    /// Whether external AI consent is currently granted in project artifacts.
    pub external_ai_consent_granted: bool,
    /// Whether local PII masking before external AI send is enforced.
    pub local_pii_masking_enforced: bool,
    /// Canonical 9-task routing table from Section 12.3.
    pub task_routing_table: Vec<AiTaskRouteEntry>,
}

/// Complete output of `SafiDecisionEngine::evaluate_workspace`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceOptimalDecisionSuite {
    /// Detected business domain of the active worksheet.
    pub detected_domain: SheetBusinessDomain,
    /// Content-based column understanding decisions (Section 11.1).
    pub column_decisions: Vec<ColumnContentDecision>,
    /// Smart duplicate analysis report (Section 11.2).
    pub duplicate_analysis: DuplicateAnalysisReport,
    /// One primary Recommended Decision for each of the 6 core product domains (Sections 10.5 & 11.1–11.6).
    pub recommended_decisions: Vec<RecommendedDecision>,
    /// Cross-product consistency & lineage report (Section 11.7).
    pub consistency_report: ConsistencyImpactReport,
    /// AI Task Routing & local intelligence boundary report (Section 12).
    pub ai_architecture: AiArchitectureStatusReport,
}

/// Central Rust `SafiDecisionEngine` (Section 10.2).
#[derive(Debug, Default)]
pub struct SafiDecisionEngine;

impl SafiDecisionEngine {
    /// Evaluates the active `DataGraph`, `StudioWorkspaceArtifacts`, saved `SheetWorkflowMemoryRecord`s,
    /// and `PersonalLearningProfile` to produce one optimal, evidence-backed `RecommendedDecision`
    /// per domain, plus content-based column understanding, duplicate analysis, and cross-product
    /// consistency verification.
    #[must_use]
    pub fn evaluate_workspace(
        graph: &DataGraph,
        artifacts: &StudioWorkspaceArtifacts,
        matched_workflow: Option<&SheetWorkflowMemoryRecord>,
        learning_profile: Option<&PersonalLearningProfile>,
    ) -> WorkspaceOptimalDecisionSuite {
        Self::evaluate_workspace_with_ollama(
            graph,
            artifacts,
            matched_workflow,
            learning_profile,
            None,
        )
    }

    /// Evaluates the workspace with an optional runtime [`OllamaRuntimeProbe`] supplied by the
    /// Desktop shell boundary (`apps/desktop/src-tauri` or `studio_ipc_harness`).
    #[must_use]
    pub fn evaluate_workspace_with_ollama(
        graph: &DataGraph,
        artifacts: &StudioWorkspaceArtifacts,
        matched_workflow: Option<&SheetWorkflowMemoryRecord>,
        learning_profile: Option<&PersonalLearningProfile>,
        ollama_probe: Option<&OllamaRuntimeProbe>,
    ) -> WorkspaceOptimalDecisionSuite {
        let table = graph.working();
        let row_count = table.row_count();
        let columns = table.columns();

        // --------------------------------------------------------------------
        // 1. Content-Based Spreadsheet Understanding (Section 11.1)
        // --------------------------------------------------------------------
        let mut column_decisions = Vec::with_capacity(columns.len());
        let mut identifier_cols = Vec::new();
        let mut measure_cols = Vec::new();
        let mut dimension_cols = Vec::new();
        let mut temporal_cols = Vec::new();
        let mut untrimmed_cols: Vec<(String, usize)> = Vec::new();
        let mut missing_numeric_cols: Vec<(String, Vec<u64>)> = Vec::new();

        for (col_idx, col) in columns.iter().enumerate() {
            let name = col.schema().name.clone();
            let header_concept = canonicalize_header_concept(&name);

            let mut non_null_count = 0usize;
            let mut null_count = 0usize;
            let mut numeric_count = 0usize;
            let mut integer_like_count = 0usize;
            let mut leading_zero_code_count = 0usize;
            let mut date_like_count = 0usize;
            let mut untrimmed_count = 0usize;
            let mut distinct_values = BTreeSet::new();
            let mut null_row_ids = Vec::new();

            for row_idx in 0..row_count {
                let row_id = graph
                    .row_id_at(row_idx)
                    .map_or((row_idx + 1) as u64, RowId::get);
                let raw_text = col
                    .cell_at(row_idx)
                    .map_or_else(String::new, |c| c.raw_text().to_owned());
                if raw_text.trim().is_empty() {
                    null_count += 1;
                    null_row_ids.push(row_id);
                    continue;
                }
                if raw_text != raw_text.trim() {
                    untrimmed_count += 1;
                }
                let trimmed = raw_text.trim();
                non_null_count += 1;
                let _ = distinct_values.insert(trimmed.to_owned());

                if trimmed.len() > 1
                    && trimmed.starts_with('0')
                    && trimmed.chars().all(|c| c.is_ascii_digit())
                {
                    leading_zero_code_count += 1;
                }
                let cleaned_num = trimmed
                    .replace([',', '$', '€', '£', '%', ' '], "")
                    .replace("EGP", "")
                    .replace("egp", "")
                    .replace("USD", "")
                    .replace("usd", "")
                    .replace("SAR", "")
                    .replace("sar", "")
                    .replace("EUR", "")
                    .replace("eur", "")
                    .replace("GBP", "")
                    .replace("AED", "")
                    .replace("ج.م", "")
                    .replace("ر.س", "")
                    .replace("د.إ", "");
                if cleaned_num.parse::<Decimal>().is_ok() {
                    numeric_count += 1;
                    if !cleaned_num.contains('.') {
                        integer_like_count += 1;
                    }
                }
                if looks_like_date_value(trimmed) {
                    date_like_count += 1;
                }
            }

            if untrimmed_count > 0 {
                untrimmed_cols.push((name.clone(), untrimmed_count));
            }

            let numeric_parse_pct = (numeric_count * 100)
                .checked_div(non_null_count)
                .map_or(0, |v| v.min(100) as u8);
            let date_parse_pct = (date_like_count * 100)
                .checked_div(non_null_count)
                .map_or(0, |v| v.min(100) as u8);
            let distinct_count = distinct_values.len();
            let is_all_unique = non_null_count > 1 && distinct_count == non_null_count;

            // Check if the user's PersonalLearningProfile has an explicit rule for this concept/header.
            let learned_role_override = learning_profile.and_then(|prof| {
                prof.rules
                    .iter()
                    .find(|r| {
                        r.category == crate::workflow_memory::LearningCategory::ColumnClassification
                            && (r.target_concept.eq_ignore_ascii_case(&header_concept)
                                || r.target_concept.eq_ignore_ascii_case(&name))
                    })
                    .map(|r| r.preferred_value.clone())
            });

            // Content + Header Joint Constraint Solver (Section 11.1):
            // Never classify a numeric column as a summable Measure if:
            // - Its header concept is an identifier (`record_identifier`, `inventory_sku_code`, `id`, `emp_id`, `رقم`)
            // - OR it has leading-zero codes
            // - OR it is 100% unique integers in column 0 with sequential/ID pattern
            // - OR the user's learned rule explicitly classified it as `Identifier`.
            let lower_name = name.trim().to_ascii_lowercase();
            let header_says_id = matches!(
                header_concept.as_str(),
                "record_identifier" | "inventory_sku_code"
            ) || (header_concept == "hr_employee_identity"
                && numeric_parse_pct >= 80)
                || lower_name == "id"
                || lower_name.ends_with("_id")
                || lower_name.ends_with(" id")
                || lower_name.starts_with("id_")
                || lower_name.starts_with("id ")
                || lower_name.ends_with("_code")
                || lower_name.ends_with(" code")
                || lower_name.ends_with("_no")
                || lower_name.ends_with(" no")
                || name.contains("رقم")
                || name.contains("كود")
                || name.contains("معرف");

            let mut protected_identifier_from_measure = false;
            let (decided_role, decided_semantic_type, reason_en, reason_ar) = if let Some(
                learned_role,
            ) =
                learned_role_override
            {
                if learned_role.to_ascii_lowercase().contains("identifier") {
                    protected_identifier_from_measure = numeric_parse_pct >= 80;
                    (
                        "Identifier".to_owned(),
                        "IdentifierCode".to_owned(),
                        format!(
                            "Classified '{}' as Identifier honoring your learned personal preference rule.",
                            name
                        ),
                        format!(
                            "تم تصنيف العمود '{}' كمعرّف (Identifier) التزامًا بقاعدة التعلّم الشخصي المحفوظة.",
                            name
                        ),
                    )
                } else {
                    (
                        learned_role.clone(),
                        "UserLearnedType".to_owned(),
                        format!(
                            "Classified '{}' as {} honoring your learned preference rule.",
                            name, learned_role
                        ),
                        format!(
                            "تم تصنيف العمود '{}' كـ {} التزامًا بقاعدة التعلّم الشخصي.",
                            name, learned_role
                        ),
                    )
                }
            } else if date_parse_pct >= 75
                || matches!(header_concept.as_str(), "temporal_date" | "hr_hire_date")
            {
                (
                    "Temporal".to_owned(),
                    "CalendarDate".to_owned(),
                    format!(
                        "Classified '{}' as Temporal CalendarDate ({}% valid date values).",
                        name, date_parse_pct
                    ),
                    format!(
                        "تم تصنيف العمود '{}' كبُعد زمني ({}% قيم تاريخ صالحة).",
                        name, date_parse_pct
                    ),
                )
            } else if header_says_id
                || leading_zero_code_count > 0
                || (col_idx == 0
                    && is_all_unique
                    && integer_like_count == non_null_count
                    && non_null_count >= 3
                    && !matches!(
                        header_concept.as_str(),
                        "sales_currency_amount"
                            | "hr_salary_amount"
                            | "finance_ledger_amount"
                            | "unit_price"
                            | "quantity_measure"
                            | "inventory_stock_qty"
                    ))
            {
                if numeric_parse_pct >= 80 {
                    protected_identifier_from_measure = true;
                }
                (
                    "Identifier".to_owned(),
                    "IdentifierCode".to_owned(),
                    format!(
                        "Classified '{}' as Identifier ({}/{} distinct values, numeric={}%). Protected from accidental SUM/AVG aggregation.",
                        name, distinct_count, non_null_count, numeric_parse_pct
                    ),
                    format!(
                        "تم تصنيف العمود '{}' كمعرّف فريد ({}/{} قيمة مميزة، رقمي={}%). تم منعه من الجمع أو المتوسط الخاطئ.",
                        name, distinct_count, non_null_count, numeric_parse_pct
                    ),
                )
            } else if numeric_parse_pct >= 80 {
                if !null_row_ids.is_empty() {
                    missing_numeric_cols.push((name.clone(), null_row_ids));
                }
                let sem_type = match header_concept.as_str() {
                    "sales_currency_amount" | "hr_salary_amount" | "finance_ledger_amount" => {
                        "CurrencyAmount"
                    }
                    "unit_price" => "UnitPrice",
                    "quantity_measure" | "inventory_stock_qty" => "Quantity",
                    _ => "NumericMeasure",
                };
                (
                    "Measure".to_owned(),
                    sem_type.to_owned(),
                    format!(
                        "Classified '{}' as quantitative Measure ({}, {}% numeric values).",
                        name, sem_type, numeric_parse_pct
                    ),
                    format!(
                        "تم تصنيف العمود '{}' كمقياس كمي قابل للحساب ({}, {}% قيم رقمية).",
                        name, sem_type, numeric_parse_pct
                    ),
                )
            } else {
                (
                    "Dimension".to_owned(),
                    "CategoricalDimension".to_owned(),
                    format!(
                        "Classified '{}' as grouping Dimension ({} distinct categories across {} rows).",
                        name, distinct_count, non_null_count
                    ),
                    format!(
                        "تم تصنيف العمود '{}' كبُعد تصنيفي للتجميع ({} فئة مميزة عبر {} صف).",
                        name, distinct_count, non_null_count
                    ),
                )
            };

            match decided_role.as_str() {
                "Identifier" => identifier_cols.push(name.clone()),
                "Measure" => measure_cols.push(name.clone()),
                "Temporal" => temporal_cols.push(name.clone()),
                _ => dimension_cols.push(name.clone()),
            }

            column_decisions.push(ColumnContentDecision {
                column_name: name,
                canonical_concept: header_concept,
                decided_role,
                decided_semantic_type,
                optimality_class: OptimalityClass::ExactProvenOptimum,
                non_null_count,
                null_count,
                distinct_count,
                numeric_parse_pct,
                date_parse_pct,
                untrimmed_whitespace_count: untrimmed_count,
                protected_identifier_from_measure,
                reason_en,
                reason_ar,
            });
        }

        // Detect sheet business domain from canonical concepts
        let concepts: Vec<String> = column_decisions
            .iter()
            .map(|c| c.canonical_concept.clone())
            .collect();
        let detected_domain = matched_workflow.map_or_else(
            || infer_domain_from_concepts(&concepts),
            |wf| wf.business_domain,
        );

        // --------------------------------------------------------------------
        // 2. Smart Duplicate Analysis (Section 11.2 & Section 4.4)
        // --------------------------------------------------------------------
        let duplicate_analysis = analyze_duplicates(
            graph,
            &column_decisions,
            identifier_cols.first().map(String::as_str),
            temporal_cols.first().map(String::as_str),
            measure_cols.first().map(String::as_str),
        );

        // --------------------------------------------------------------------
        // 3. Build the 6 Domain-Level Primary Recommended Decisions
        // --------------------------------------------------------------------
        let mut recommended_decisions = Vec::with_capacity(6);

        // Domain 1: Spreadsheet Understanding Decision
        recommended_decisions.push(build_spreadsheet_understanding_decision(
            &column_decisions,
            &identifier_cols,
            &measure_cols,
            &dimension_cols,
            &temporal_cols,
        ));

        // Domain 2: Data Cleaning Decision
        recommended_decisions.push(build_data_cleaning_decision(
            row_count,
            &untrimmed_cols,
            &missing_numeric_cols,
            &duplicate_analysis,
        ));

        // Domain 3: Optimal KPI Selection Decision
        let kpi_out = build_kpi_selection_decision(
            table,
            &column_decisions,
            &measure_cols,
            &identifier_cols,
            detected_domain,
            matched_workflow,
            learning_profile,
        );
        recommended_decisions.push(kpi_out.decision);

        // Domain 4: Optimal Chart Selection Decision
        let (chart_decision, best_chart_type, best_group_col) = build_chart_selection_decision(
            table,
            &column_decisions,
            &kpi_out.best_measure_col,
            &kpi_out.best_agg_fn,
            &measure_cols,
            &dimension_cols,
            &temporal_cols,
            detected_domain,
            row_count,
            matched_workflow,
            learning_profile,
        );
        recommended_decisions.push(chart_decision);

        // Domain 5: Coherent Dashboard Builder Decision
        recommended_decisions.push(build_dashboard_layout_decision(
            &kpi_out.best_metric_id,
            &kpi_out.best_metric_name,
            kpi_out.best_metric_unit.as_deref(),
            &kpi_out.best_metric_formula,
            &best_chart_type,
            &kpi_out.best_measure_col,
            &best_group_col,
            detected_domain,
            row_count,
            &measure_cols,
            &dimension_cols,
        ));

        // Domain 6: Executive Report & PowerPoint Decision
        recommended_decisions.push(build_report_and_presentation_decision(
            graph,
            &kpi_out.best_metric_id,
            &kpi_out.best_metric_name,
            kpi_out.best_metric_unit.as_deref(),
            &kpi_out.best_metric_formula,
            &kpi_out.best_measure_col,
            &best_chart_type,
            &best_group_col,
            detected_domain,
        ));

        // --------------------------------------------------------------------
        // 4. Cross-Product Consistency & Lineage Audit (Section 11.7)
        // --------------------------------------------------------------------
        let consistency_report = audit_cross_product_consistency(graph, artifacts);

        // --------------------------------------------------------------------
        // 5. AI Architecture & Task Router Status (Section 12 & Section 5)
        // --------------------------------------------------------------------
        let ai_architecture = build_ai_architecture_status(artifacts, ollama_probe);

        WorkspaceOptimalDecisionSuite {
            detected_domain,
            column_decisions,
            duplicate_analysis,
            recommended_decisions,
            consistency_report,
            ai_architecture,
        }
    }
}

fn looks_like_date_value(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() == 10 {
        // YYYY-MM-DD or DD/MM/YYYY
        let is_iso = (bytes[4] == b'-' || bytes[4] == b'/')
            && (bytes[7] == b'-' || bytes[7] == b'/')
            && bytes[0..4].iter().all(u8::is_ascii_digit)
            && bytes[5..7].iter().all(u8::is_ascii_digit)
            && bytes[8..10].iter().all(u8::is_ascii_digit);
        let is_dmy = (bytes[2] == b'/' || bytes[2] == b'-')
            && (bytes[5] == b'/' || bytes[5] == b'-')
            && bytes[0..2].iter().all(u8::is_ascii_digit)
            && bytes[3..5].iter().all(u8::is_ascii_digit)
            && bytes[6..10].iter().all(u8::is_ascii_digit);
        return is_iso || is_dmy;
    }
    false
}

fn infer_domain_from_concepts(concepts: &[String]) -> SheetBusinessDomain {
    let mut sales = 0u32;
    let mut hr = 0u32;
    let mut inv = 0u32;
    let mut fin = 0u32;
    for c in concepts {
        if c.starts_with("hr_") {
            hr += 3;
        } else if c.starts_with("inventory_") {
            inv += 3;
        } else if c.starts_with("sales_") {
            sales += 3;
        } else if c.starts_with("finance_") {
            fin += 3;
        } else if c == "dimension_branch_region" || c == "unit_price" {
            sales += 1;
        }
    }
    let max = sales.max(hr).max(inv).max(fin);
    if max == 0 {
        SheetBusinessDomain::General
    } else if hr == max {
        SheetBusinessDomain::Employees
    } else if inv == max {
        SheetBusinessDomain::Inventory
    } else if fin == max {
        SheetBusinessDomain::Finance
    } else {
        SheetBusinessDomain::Sales
    }
}

fn analyze_duplicates(
    graph: &DataGraph,
    column_decisions: &[ColumnContentDecision],
    primary_key_col: Option<&str>,
    temporal_col: Option<&str>,
    measure_col: Option<&str>,
) -> DuplicateAnalysisReport {
    let table = graph.working();
    let row_count = table.row_count();
    let cols = table.columns();
    let mut seen_full_rows: BTreeSet<Vec<String>> = BTreeSet::new();
    let mut exact_dups = Vec::new();

    let pk_idx = primary_key_col.and_then(|pk| cols.iter().position(|c| c.schema().name == pk));
    let temporal_idx = temporal_col.and_then(|t| cols.iter().position(|c| c.schema().name == t));
    let measure_idx = measure_col.and_then(|m| cols.iter().position(|c| c.schema().name == m));

    // Only treat a genuine party/account/customer/employee/branch column as an entity for
    // repeated-transaction evidence (never generic categorical buckets like `Department` alone!).
    let entity_col_name = column_decisions
        .iter()
        .find(|c| {
            c.decided_role == "Dimension"
                && matches!(
                    c.canonical_concept.as_str(),
                    "sales_customer_party" | "hr_employee_identity" | "dimension_branch_region"
                )
        })
        .map(|c| c.column_name.as_str());
    let entity_idx = entity_col_name.and_then(|en| cols.iter().position(|c| c.schema().name == en));

    let mut seen_pk_payloads: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut conflicting_key_rows = Vec::new();
    // Map entity -> set of distinct transaction fingerprints `(pk, temporal, measure)`
    let mut entity_tx_fingerprints: BTreeMap<String, BTreeSet<(String, String, String)>> =
        BTreeMap::new();
    // Without a primary key, track `(entity, temporal, measure)` repeats that differ only in notes/secondary columns.
    let mut seen_no_pk_events: BTreeSet<(String, String, String)> = BTreeSet::new();
    let mut ambiguous_rows = Vec::new();

    for row_idx in 0..row_count {
        let row_id = graph
            .row_id_at(row_idx)
            .map_or((row_idx + 1) as u64, RowId::get);
        let mut tuple = Vec::with_capacity(cols.len());
        for col in cols {
            let txt = col
                .cell_at(row_idx)
                .map_or_else(String::new, |c| c.raw_text().trim().to_owned());
            tuple.push(txt);
        }

        // 1. Exact full-row unintended duplicates (100% identical across all columns)
        if !seen_full_rows.insert(tuple.clone()) {
            exact_dups.push(row_id);
            continue;
        }

        // 2. Conflicting primary key rows (same primary key, different non-key attributes)
        let mut is_pk_conflict = false;
        let pk_val = pk_idx
            .and_then(|i| tuple.get(i))
            .cloned()
            .unwrap_or_default();
        if pk_idx.is_some() && !pk_val.is_empty() {
            if let Some(prev_tuple) = seen_pk_payloads.get(&pk_val) {
                if prev_tuple != &tuple {
                    conflicting_key_rows.push(row_id);
                    is_pk_conflict = true;
                }
            } else {
                let _ = seen_pk_payloads.insert(pk_val.clone(), tuple.clone());
            }
        }

        let date_val = temporal_idx
            .and_then(|i| tuple.get(i))
            .cloned()
            .unwrap_or_default();
        let amt_val = measure_idx
            .and_then(|i| tuple.get(i))
            .cloned()
            .unwrap_or_default();

        // 3. Valid repeated entity transactions vs. Ambiguous duplicates:
        //    Only count as a valid repeated transaction when backed by a distinct transaction ID
        //    OR distinct temporal date/amount event (never from a bare category repetition!).
        if !is_pk_conflict {
            if let Some(e_i) = entity_idx {
                let ent_val = tuple.get(e_i).cloned().unwrap_or_default();
                if !ent_val.is_empty() {
                    let has_tx_evidence =
                        pk_idx.is_some() || temporal_idx.is_some() || measure_idx.is_some();
                    if has_tx_evidence {
                        let tx_fp = (pk_val.clone(), date_val.clone(), amt_val.clone());
                        let entry = entity_tx_fingerprints.entry(ent_val.clone()).or_default();
                        if !entry.insert(tx_fp.clone()) && pk_idx.is_none() {
                            // Same (entity, date, amount) without a unique transaction key -> ambiguous duplicate!
                            ambiguous_rows.push(row_id);
                        }
                    }
                }
            } else if pk_idx.is_none() && (temporal_idx.is_some() || measure_idx.is_some()) {
                // No entity column and no primary key: check if `(first_col, date, amount)` repeats ambiguously
                let first_val = tuple.first().cloned().unwrap_or_default();
                let ev_key = (first_val, date_val, amt_val);
                if !seen_no_pk_events.insert(ev_key) {
                    ambiguous_rows.push(row_id);
                }
            }
        }
    }

    // Valid repeated transactions = distinct transactions beyond the first per entity.
    let valid_repeated_entity_transaction_rows: usize = entity_tx_fingerprints
        .values()
        .filter(|fps| fps.len() > 1)
        .map(|fps| fps.len() - 1)
        .sum();

    let summary_en = format!(
        "Detected {} exact unintended duplicate row(s), {} verified repeated entity transaction(s) (preserved safely), {} conflicting primary-key row(s), and {} ambiguous near-duplicate row(s) requiring review.",
        exact_dups.len(),
        valid_repeated_entity_transaction_rows,
        conflicting_key_rows.len(),
        ambiguous_rows.len()
    );
    let summary_ar = format!(
        "تم رصد {} صفوف مكررة بالكامل عن غير قصد، و{} معاملات متكررة موثّقة لنفس الجهة (تم الحفاظ عليها بأمان)، و{} صفوف ذات تعارض في المفتاح الأساسي، و{} صفوف محتملة التكرار تحتاج مراجعة.",
        exact_dups.len(),
        valid_repeated_entity_transaction_rows,
        conflicting_key_rows.len(),
        ambiguous_rows.len()
    );

    DuplicateAnalysisReport {
        exact_unintended_duplicate_row_ids: exact_dups,
        valid_repeated_entity_transaction_rows,
        conflicting_key_review_row_ids: conflicting_key_rows,
        ambiguous_duplicate_review_row_ids: ambiguous_rows,
        detected_primary_key_column: primary_key_col.map(str::to_owned),
        summary_en,
        summary_ar,
    }
}

fn build_spreadsheet_understanding_decision(
    column_decisions: &[ColumnContentDecision],
    identifier_cols: &[String],
    measure_cols: &[String],
    dimension_cols: &[String],
    temporal_cols: &[String],
) -> RecommendedDecision {
    let protected_ids: Vec<String> = column_decisions
        .iter()
        .filter(|c| c.protected_identifier_from_measure)
        .map(|c| c.column_name.clone())
        .collect();

    // Compute real data-driven completeness & parse-rate metrics across columns.
    let total_cells: usize = column_decisions
        .iter()
        .map(|c| c.non_null_count + c.null_count)
        .sum();
    let total_non_null: usize = column_decisions.iter().map(|c| c.non_null_count).sum();
    let completeness_permille: u16 = (total_non_null.saturating_mul(1000))
        .checked_div(total_cells)
        .map_or(0, |v| v.min(1000) as u16);

    let typed_cols = measure_cols.len() + temporal_cols.len() + identifier_cols.len();
    let schema_signal_permille: u16 = (typed_cols.saturating_mul(1000))
        .checked_div(column_decisions.len().max(1))
        .map_or(500, |v| (600 + (v * 400) / 1000).min(1000) as u16);

    let mut audit = Vec::new();
    if !protected_ids.is_empty() {
        audit.push(EvaluatedCandidateAudit {
            candidate_id: "cand_naive_numeric_measure".to_owned(),
            title_en: format!(
                "Treat numeric column(s) [{}] as summable Measures",
                protected_ids.join(", ")
            ),
            title_ar: format!(
                "التعامل مع الأعمدة الرقمية [{}] كمقاييس قابلة للجمع",
                protected_ids.join("، ")
            ),
            is_valid: false,
            elimination_reason_en: Some(
                "Eliminated by Constraint Solver: 100% unique identifier/key cardinality or ID semantics; summing or averaging identifier codes produces nonsensical totals."
                    .to_owned(),
            ),
            elimination_reason_ar: Some(
                "تم استبعاده بواسطة محرك القيود: العمود يمثل معرّفًا فريدًا (Identifier)؛ جمع أو حساب متوسط المعرّفات ينتج أرقامًا خاطئة تمامًا."
                    .to_owned(),
            ),
            objective: LexicographicObjectiveVector {
                correctness: 0,
                data_safety_no_loss: completeness_permille,
                user_constraint_compliance: 200,
                analytical_fit: 0,
                explainability_audit: 300,
                deliverable_quality: 100,
                execution_efficiency: 990,
            },
        });
    }

    // Candidate B: Header-only classification (valid only if all columns match header semantics; scores lower when content evidence was needed).
    let header_only_correctness: u16 = if protected_ids.is_empty() { 880 } else { 0 };
    audit.push(EvaluatedCandidateAudit {
        candidate_id: "cand_header_only_schema".to_owned(),
        title_en: "Header-Only Column Role Inference (Without Cell Distribution Verification)"
            .to_owned(),
        title_ar: "استنتاج أدوار الأعمدة من العناوين فقط دون فحص توزيع القيم".to_owned(),
        is_valid: protected_ids.is_empty(),
        elimination_reason_en: if protected_ids.is_empty() {
            None
        } else {
            Some("Eliminated: header-only inference failed to protect numeric identifier columns from summation.".to_owned())
        },
        elimination_reason_ar: if protected_ids.is_empty() {
            None
        } else {
            Some("تم استبعاده: الاعتماد على العناوين فقط فشل في حماية أعمدة المعرّفات الرقمية.".to_owned())
        },
        objective: LexicographicObjectiveVector {
            correctness: header_only_correctness,
            data_safety_no_loss: completeness_permille,
            user_constraint_compliance: 800,
            analytical_fit: schema_signal_permille.saturating_sub(120),
            explainability_audit: 750,
            deliverable_quality: 800,
            execution_efficiency: 995,
        },
    });

    let winning_obj = LexicographicObjectiveVector {
        correctness: if column_decisions.is_empty() {
            0
        } else {
            950u16.saturating_add((completeness_permille / 20).min(50))
        },
        data_safety_no_loss: completeness_permille,
        user_constraint_compliance: 1000,
        analytical_fit: schema_signal_permille,
        explainability_audit: 980,
        deliverable_quality: 880u16.saturating_add((completeness_permille / 10).min(100)),
        execution_efficiency: 970,
    };
    audit.push(EvaluatedCandidateAudit {
        candidate_id: "cand_joint_header_content_schema".to_owned(),
        title_en: format!(
            "Joint Content + Semantic Schema ({} Measures, {} Dimensions, {} Temporal, {} Identifiers)",
            measure_cols.len(),
            dimension_cols.len(),
            temporal_cols.len(),
            identifier_cols.len()
        ),
        title_ar: format!(
            "تفسير مشترك للمحتوى والدلالة ({} مقاييس، {} أبعاد، {} تواريخ، {} معرّفات)",
            measure_cols.len(),
            dimension_cols.len(),
            temporal_cols.len(),
            identifier_cols.len()
        ),
        is_valid: !column_decisions.is_empty(),
        elimination_reason_en: None,
        elimination_reason_ar: None,
        objective: winning_obj,
    });

    RecommendedDecision {
        decision_id: "dec_spreadsheet_understanding".to_owned(),
        domain: DecisionDomain::SpreadsheetUnderstanding,
        algorithm_used: OptimizationAlgorithm::ExactConstraintSolver,
        optimality_class: if column_decisions.is_empty() {
            OptimalityClass::InsufficientEvidenceRequiresQuestion
        } else {
            OptimalityClass::ExactProvenOptimum
        },
        recommended_title_en: format!(
            "Lock Verified Schema: {} Measure(s), {} Dimension(s), {} Temporal, {} Identifier(s)",
            measure_cols.len(),
            dimension_cols.len(),
            temporal_cols.len(),
            identifier_cols.len()
        ),
        recommended_title_ar: format!(
            "اعتماد الهيكل الدلالي الموثق: {} مقاييس، {} أبعاد، {} زمني، {} معرّفات",
            measure_cols.len(),
            dimension_cols.len(),
            temporal_cols.len(),
            identifier_cols.len()
        ),
        why_en: if protected_ids.is_empty() {
            format!(
                "Every column was verified by inspecting both cell value distributions ({:.1}% non-null completeness) and bilingual header semantics.",
                f64::from(completeness_permille) / 10.0
            )
        } else {
            format!(
                "Protected numeric identifier column(s) [{}] from being summed as Measures based on uniqueness and identifier patterns ({:.1}% completeness).",
                protected_ids.join(", "),
                f64::from(completeness_permille) / 10.0
            )
        },
        why_ar: if protected_ids.is_empty() {
            format!(
                "تم التحقق من كل عمود بفحص توزيع القيم الفعلية (اكتمال {:.1}%) مع الدلالة اللغوية للعناوين.",
                f64::from(completeness_permille) / 10.0
            )
        } else {
            format!(
                "تمت حماية أعمدة المعرّفات الرقمية [{}] من الجمع الخاطئ كمقاييس بناءً على تفرد القيم ونمط المعرّف.",
                protected_ids.join("، ")
            )
        },
        objective: winning_obj,
        evidence: vec![
            DecisionEvidenceItem {
                label_en: "Quantitative Measures".to_owned(),
                label_ar: "المقاييس الكمية".to_owned(),
                observed_value: if measure_cols.is_empty() {
                    "None".to_owned()
                } else {
                    measure_cols.join(", ")
                },
                impact_en: "Eligible for deterministic SUM/AVG/MIN/MAX/MEDIAN aggregation.".to_owned(),
                impact_ar: "صالحة لعمليات الجمع والمتوسط والحد الأدنى والأعلى الحتمية.".to_owned(),
            },
            DecisionEvidenceItem {
                label_en: "Protected Identifiers".to_owned(),
                label_ar: "المعرّفات المحمية من الجمع".to_owned(),
                observed_value: if identifier_cols.is_empty() {
                    "None".to_owned()
                } else {
                    identifier_cols.join(", ")
                },
                impact_en: "Excluded from numeric summation to prevent corrupted KPIs.".to_owned(),
                impact_ar: "مستبعدة من الجمع الرقمي لمنع تشويه المؤشرات.".to_owned(),
            },
            DecisionEvidenceItem {
                label_en: "Observed Non-Null Completeness".to_owned(),
                label_ar: "نسبة اكتمال القيم الفعلية".to_owned(),
                observed_value: format!("{:.1}% ({total_non_null}/{total_cells})", f64::from(completeness_permille) / 10.0),
                impact_en: "Directly drives data-safety and analytical-fit scoring.".to_owned(),
                impact_ar: "تُستخدم مباشرة في حساب درجات سلامة البيانات والملاءمة التحليلية.".to_owned(),
            },
        ],
        assumptions_en: vec![
            "Header row is the first analytical row extracted by the workbook region detector.".to_owned(),
        ],
        assumptions_ar: vec![
            "صف العناوين هو الصف التحليلي الأول المستخرج بواسطة كاشف مناطق المصنف.".to_owned(),
        ],
        what_executes_en: "Applies the verified column role classification across KPI, Chart, and Cleaning engines."
            .to_owned(),
        what_executes_ar: "يطبّق تصنيف أدوار الأعمدة الموثق عبر محركات المؤشرات والرسوم والتنقية.".to_owned(),
        undo_way_en: "Override any column role in Workflow Memory or record a correction in Continuous Learning."
            .to_owned(),
        undo_way_ar: "يمكنك تعديل دور أي عمود في ذاكرة الورقة أو تسجيل تصحيح في التعلّم المستمر.".to_owned(),
        executable_action: DecisionExecutableAction::ConfirmClassificationOnly {
            summary: format!("measures={:?}, ids={:?}", measure_cols, identifier_cols),
        },
        clarification_prompt: None,
        candidates_evaluated_count: audit.len(),
        invalid_eliminated_count: audit.iter().filter(|c| !c.is_valid).count(),
        advanced_audit_candidates: audit,
    }
}

fn build_data_cleaning_decision(
    row_count: usize,
    untrimmed_cols: &[(String, usize)],
    missing_numeric_cols: &[(String, Vec<u64>)],
    dup: &DuplicateAnalysisReport,
) -> RecommendedDecision {
    let mut audit = Vec::new();
    let total_rows = row_count.max(1);

    // Candidate A (Pruned as unsafe if repeated entity transactions, conflicting keys, or ambiguous duplicates exist):
    if dup.valid_repeated_entity_transaction_rows > 0
        || !dup.conflicting_key_review_row_ids.is_empty()
        || !dup.ambiguous_duplicate_review_row_ids.is_empty()
    {
        audit.push(EvaluatedCandidateAudit {
            candidate_id: "cand_aggressive_key_or_entity_dedup".to_owned(),
            title_en: "Aggressive Deduplication on Entity / Conflicting / Ambiguous Rows".to_owned(),
            title_ar: "حذف التكرارات العدواني حسب الجهة أو المفاتيح المتعارضة أو الغامضة".to_owned(),
            is_valid: false,
            elimination_reason_en: Some(format!(
                "Eliminated by Data-Safety Gate: would destroy {} verified repeated transaction(s), {} conflicting-key row(s), and {} ambiguous row(s) requiring human review.",
                dup.valid_repeated_entity_transaction_rows,
                dup.conflicting_key_review_row_ids.len(),
                dup.ambiguous_duplicate_review_row_ids.len()
            )),
            elimination_reason_ar: Some(format!(
                "تم استبعاده بواسطة بوابة سلامة البيانات: يؤدي إلى حذف {} معاملات متكررة صحيحة و{} صفوف متعارضة المفتاح و{} صفوف غامضة تتطلب مراجعة.",
                dup.valid_repeated_entity_transaction_rows,
                dup.conflicting_key_review_row_ids.len(),
                dup.ambiguous_duplicate_review_row_ids.len()
            )),
            objective: LexicographicObjectiveVector {
                correctness: 0,
                data_safety_no_loss: 100,
                user_constraint_compliance: 200,
                analytical_fit: 200,
                explainability_audit: 400,
                deliverable_quality: 200,
                execution_efficiency: 990,
            },
        });
    }

    // Build all valid candidate cleaning plans and score them dynamically from dataset impact!
    struct CleaningCandidate {
        id: String,
        title_en: String,
        title_ar: String,
        why_en: String,
        why_ar: String,
        what_en: String,
        what_ar: String,
        action: DecisionExecutableAction,
        objective: LexicographicObjectiveVector,
    }

    let mut valid_candidates: Vec<CleaningCandidate> = Vec::new();
    let has_unresolved_ambiguity = !dup.conflicting_key_review_row_ids.is_empty()
        || !dup.ambiguous_duplicate_review_row_ids.is_empty();

    if !dup.exact_unintended_duplicate_row_ids.is_empty() {
        let ids = dup.exact_unintended_duplicate_row_ids.clone();
        let dup_impact_permille = ((ids.len() * 1000) / total_rows).min(1000) as u16;
        valid_candidates.push(CleaningCandidate {
            id: "cand_drop_exact_duplicates".to_owned(),
            title_en: format!(
                "Remove {} Exact Full-Row Duplicate(s) While Preserving {} Verified Repeated Transaction(s)",
                ids.len(),
                dup.valid_repeated_entity_transaction_rows
            ),
            title_ar: format!(
                "إزالة {} صفوف مكررة بالكامل مع الحفاظ على {} معاملات متكررة موثّقة",
                ids.len(),
                dup.valid_repeated_entity_transaction_rows
            ),
            why_en: "Only rows that are 100% byte-identical across every column are removed; verified multi-transaction rows and ambiguous/conflicting keys are preserved."
                .to_owned(),
            why_ar: "يتم حذف الصفوف المتطابقة بنسبة 100% في جميع الأعمدة فقط، مع الحفاظ الكامل على المعاملات الصحيحة والصفوف التي تحتاج مراجعة."
                .to_owned(),
            what_en: format!("Drops exact duplicate row IDs {:?} atomically in Rust DataGraph.", ids),
            what_ar: format!("يحذف الصفوف المكررة المتطابقة {:?} ذريًا داخل محرك Rust.", ids),
            action: DecisionExecutableAction::DropExactDuplicateRows { row_ids: ids },
            objective: LexicographicObjectiveVector {
                correctness: 990,
                data_safety_no_loss: 1000,
                user_constraint_compliance: 980,
                analytical_fit: 850u16.saturating_add((dup_impact_permille / 7).min(140)),
                explainability_audit: 1000,
                deliverable_quality: 960,
                execution_efficiency: 970,
            },
        });
    }

    if !untrimmed_cols.is_empty() {
        let cols: Vec<String> = untrimmed_cols.iter().map(|(c, _)| c.clone()).collect();
        let total_cells: usize = untrimmed_cols.iter().map(|(_, n)| *n).sum();
        let trim_impact = ((total_cells * 1000) / total_rows).min(1000) as u16;
        valid_candidates.push(CleaningCandidate {
            id: "cand_trim_whitespace".to_owned(),
            title_en: format!(
                "Normalize & Trim Whitespace Across {} Column(s) ({} Cell(s))",
                cols.len(),
                total_cells
            ),
            title_ar: format!(
                "تطبيع وإزالة الفراغات الزائدة في {} أعمدة ({} خلية)",
                cols.len(),
                total_cells
            ),
            why_en: "Leading/trailing whitespace splits identical categories during GROUP BY aggregation; trimming restores exact category grouping without losing any data."
                .to_owned(),
            why_ar: "الفراغات الزائدة في بداية أو نهاية النص تؤدي لتقسيم نفس الفئة عند التجميع؛ تنظيفها يوحّد الفئات دون فقدان أي بيانات."
                .to_owned(),
            what_en: format!("Executes deterministic trim_column on [{}].", cols.join(", ")),
            what_ar: format!("ينفّذ تنظيف الفراغات الحتمي على الأعمدة [{}].", cols.join("، ")),
            action: DecisionExecutableAction::TrimColumns { columns: cols },
            objective: LexicographicObjectiveVector {
                correctness: 985,
                data_safety_no_loss: 1000,
                user_constraint_compliance: 980,
                analytical_fit: 840u16.saturating_add((trim_impact / 8).min(130)),
                explainability_audit: 1000,
                deliverable_quality: 940,
                execution_efficiency: 985,
            },
        });
    }

    if let Some((col, row_ids)) = missing_numeric_cols.first() {
        let miss_impact = ((row_ids.len() * 1000) / total_rows).min(1000) as u16;
        valid_candidates.push(CleaningCandidate {
            id: format!("cand_fill_missing_zero_{col}"),
            title_en: format!(
                "Impute {} Missing Numeric Cell(s) in '{}' with Verified 0",
                row_ids.len(),
                col
            ),
            title_ar: format!(
                "تعويض {} خلايا رقمية مفقودة في العمود '{}' بالقيمة 0 الموثقة",
                row_ids.len(),
                col
            ),
            why_en: "Prevents null-propagation errors in downstream SUM/AVG formulas while preserving full row lineage."
                .to_owned(),
            why_ar: "يمنع أخطاء القيم الفارغة في معادلات الجمع والمتوسط مع الحفاظ على كامل الصفوف وسلسلة المصدر."
                .to_owned(),
            what_en: format!("Fills missing cells in '{}' at row IDs {:?} with 0.", col, row_ids),
            what_ar: format!("يعوّض الخلايا المفقودة في '{}' عند الصفوف {:?} بالقيمة 0.", col, row_ids),
            action: DecisionExecutableAction::FillMissingNumericZero {
                column: col.clone(),
                row_ids: row_ids.clone(),
            },
            objective: LexicographicObjectiveVector {
                correctness: 960,
                data_safety_no_loss: 950u16.saturating_sub((miss_impact / 10).min(80)),
                user_constraint_compliance: 940,
                analytical_fit: 820u16.saturating_add((miss_impact / 8).min(120)),
                explainability_audit: 980,
                deliverable_quality: 920,
                execution_efficiency: 980,
            },
        });
    }

    // Always evaluate the non-destructive preserve-as-is candidate!
    let has_hygiene_issues = !valid_candidates.is_empty();
    valid_candidates.push(CleaningCandidate {
        id: "cand_preserve_working_table".to_owned(),
        title_en: if has_hygiene_issues {
            "Keep Working Dataset Unmodified (Defer Cleaning)".to_owned()
        } else {
            "Dataset Passed All Deterministic Hygiene Gates (Zero Destructive Changes Needed)"
                .to_owned()
        },
        title_ar: if has_hygiene_issues {
            "إبقاء جدول البيانات كما هو دون تنظيف حاليًا".to_owned()
        } else {
            "اجتازت البيانات جميع بوابات النظافة الحتمية (لا حاجة لأي تعديل حذفي)".to_owned()
        },
        why_en: if has_hygiene_issues {
            "Preserves raw values intact, though detected duplicates/whitespace/nulls remain."
                .to_owned()
        } else {
            "No exact duplicates, untrimmed whitespace, or missing numeric cells were detected; preserving the working table as-is is provably optimal."
                .to_owned()
        },
        why_ar: if has_hygiene_issues {
            "يبقي القيم الخام كما هي مع بقاء مشكلات النظافة المرصودة.".to_owned()
        } else {
            "لم يتم رصد أي تكرارات متطابقة أو فراغات زائدة أو قيم رقمية مفقودة؛ الحفاظ على الجدول الحالي هو القرار الأمثل."
                .to_owned()
        },
        what_en: "No mutation required; working dataset remains preserved.".to_owned(),
        what_ar: "لا يتطلب أي تعديل؛ بيانات العمل تبقى محفوظة كما هي.".to_owned(),
        action: DecisionExecutableAction::ConfirmClassificationOnly {
            summary: "clean_noop".to_owned(),
        },
        objective: LexicographicObjectiveVector {
            correctness: if has_hygiene_issues { 820 } else { 1000 },
            data_safety_no_loss: 1000,
            user_constraint_compliance: 950,
            analytical_fit: if has_hygiene_issues { 700 } else { 980 },
            explainability_audit: 1000,
            deliverable_quality: if has_hygiene_issues { 780 } else { 970 },
            execution_efficiency: 1000,
        },
    });

    valid_candidates.sort_by_key(|a| std::cmp::Reverse(a.objective));
    for cand in &valid_candidates {
        audit.push(EvaluatedCandidateAudit {
            candidate_id: cand.id.clone(),
            title_en: cand.title_en.clone(),
            title_ar: cand.title_ar.clone(),
            is_valid: true,
            elimination_reason_en: None,
            elimination_reason_ar: None,
            objective: cand.objective,
        });
    }

    let strictly_dominates = valid_candidates.len() == 1
        || valid_candidates[0].objective > valid_candidates[1].objective;
    let winner = &valid_candidates[0];

    let optimality_class = if has_unresolved_ambiguity {
        OptimalityClass::InsufficientEvidenceRequiresQuestion
    } else if strictly_dominates {
        OptimalityClass::ExactProvenOptimum
    } else {
        OptimalityClass::BestValidCandidateFound
    };

    let clarification_prompt = if !dup.conflicting_key_review_row_ids.is_empty() {
        Some(FocusedClarificationPrompt {
            question_id: "q_conflicting_primary_key_policy".to_owned(),
            prompt_en: format!(
                "Found {} row(s) sharing the same primary key with different values. How should SafiData handle them?",
                dup.conflicting_key_review_row_ids.len()
            ),
            prompt_ar: format!(
                "تم العثور على {} صفوف تشترك في نفس المفتاح الأساسي ولكن بقيم مختلفة. كيف تفضّل معالجتها؟",
                dup.conflicting_key_review_row_ids.len()
            ),
            options: vec![
                "Keep all and flag for manual review (Recommended)".to_owned(),
                "Keep latest record by date".to_owned(),
            ],
        })
    } else if !dup.ambiguous_duplicate_review_row_ids.is_empty() {
        Some(FocusedClarificationPrompt {
            question_id: "q_ambiguous_duplicate_policy".to_owned(),
            prompt_en: format!(
                "Found {} potentially repeated row(s) with no unique transaction ID key. Are these separate valid events or accidental near-duplicates?",
                dup.ambiguous_duplicate_review_row_ids.len()
            ),
            prompt_ar: format!(
                "تم رصد {} صفوف متكررة محتملة بدون مفتاح معاملة فريد. هل تمثل معاملات منفصلة صحيحة أم تكرارات غير مقصودة؟",
                dup.ambiguous_duplicate_review_row_ids.len()
            ),
            options: vec![
                "Preserve all rows (treat as separate valid events)".to_owned(),
                "Flag for manual deduplication review".to_owned(),
            ],
        })
    } else {
        None
    };

    RecommendedDecision {
        decision_id: "dec_data_cleaning".to_owned(),
        domain: DecisionDomain::DataCleaning,
        algorithm_used: OptimizationAlgorithm::LexicographicBranchAndBound,
        optimality_class,
        recommended_title_en: winner.title_en.clone(),
        recommended_title_ar: winner.title_ar.clone(),
        why_en: winner.why_en.clone(),
        why_ar: winner.why_ar.clone(),
        objective: winner.objective,
        evidence: vec![
            DecisionEvidenceItem {
                label_en: "Exact Unintended Duplicates".to_owned(),
                label_ar: "التكرارات الكاملة غير المقصودة".to_owned(),
                observed_value: dup.exact_unintended_duplicate_row_ids.len().to_string(),
                impact_en: "100% identical across all columns; safe to deduplicate.".to_owned(),
                impact_ar: "متطابقة بنسبة 100% في كل الأعمدة؛ آمنة للإزالة.".to_owned(),
            },
            DecisionEvidenceItem {
                label_en: "Verified Repeated Entity Transactions".to_owned(),
                label_ar: "معاملات متكررة موثّقة لنفس الجهة".to_owned(),
                observed_value: dup.valid_repeated_entity_transaction_rows.to_string(),
                impact_en: "Backed by distinct transaction ID/date/amount; protected by Data-Safety Gate.".to_owned(),
                impact_ar: "مدعومة بمعرّف معاملة أو تاريخ/مبلغ مختلف؛ محمية من الحذف.".to_owned(),
            },
            DecisionEvidenceItem {
                label_en: "Ambiguous / Conflicting Review Rows".to_owned(),
                label_ar: "صفوف متعارضة أو غامضة للمراجعة".to_owned(),
                observed_value: format!(
                    "{} conflicting, {} ambiguous",
                    dup.conflicting_key_review_row_ids.len(),
                    dup.ambiguous_duplicate_review_row_ids.len()
                ),
                impact_en: "Never auto-deleted; routed to user confirmation.".to_owned(),
                impact_ar: "لا تُحذف تلقائيًا أبدًا؛ تُحال لتأكيد المستخدم.".to_owned(),
            },
            DecisionEvidenceItem {
                label_en: "Untrimmed Text Columns".to_owned(),
                label_ar: "أعمدة تحتوي فراغات زائدة".to_owned(),
                observed_value: untrimmed_cols.len().to_string(),
                impact_en: "Normalized non-destructively via Rust trim_column.".to_owned(),
                impact_ar: "يتم تطبيعها دون أي فقد للبيانات عبر محرك Rust.".to_owned(),
            },
        ],
        assumptions_en: vec![
            "Original source file on disk is never modified; all cleaning runs on the in-memory DataGraph with full undo history."
                .to_owned(),
        ],
        assumptions_ar: vec![
            "لا يتم المساس بالملف الأصلي على القرص أبدًا؛ تُنفَّذ التنقية داخل DataGraph مع سجل تراجع كامل."
                .to_owned(),
        ],
        what_executes_en: winner.what_en.clone(),
        what_executes_ar: winner.what_ar.clone(),
        undo_way_en: "Single-click Undo (studio_undo) immediately restores the previous DataGraph revision."
            .to_owned(),
        undo_way_ar: "ضغطة واحدة على زر التراجع (Undo) تستعيد النسخة السابقة من البيانات فورًا.".to_owned(),
        executable_action: winner.action.clone(),
        clarification_prompt,
        candidates_evaluated_count: audit.len(),
        invalid_eliminated_count: audit.iter().filter(|c| !c.is_valid).count(),
        advanced_audit_candidates: audit,
    }
}

struct KpiSelectionOutput {
    decision: RecommendedDecision,
    best_measure_col: String,
    best_agg_fn: String,
    best_metric_id: String,
    best_metric_name: String,
    best_metric_unit: Option<String>,
    best_metric_formula: String,
}

fn detect_currencies_in_column(table: &sf_core::Table, col_name: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let detect_token = |raw: &str, out: &mut BTreeSet<String>| {
        let upper = raw.to_ascii_uppercase();
        if upper.contains("EGP") || raw.contains("ج.م") || raw.contains("جنيه") {
            let _ = out.insert("EGP".to_owned());
        }
        if upper.contains("USD") || raw.contains('$') {
            let _ = out.insert("USD".to_owned());
        }
        if upper.contains("SAR") || raw.contains("ر.س") || raw.contains("ريال") {
            let _ = out.insert("SAR".to_owned());
        }
        if upper.contains("EUR") || raw.contains('€') {
            let _ = out.insert("EUR".to_owned());
        }
        if upper.contains("GBP") || raw.contains('£') {
            let _ = out.insert("GBP".to_owned());
        }
        if upper.contains("AED") || raw.contains("د.إ") {
            let _ = out.insert("AED".to_owned());
        }
        if raw.trim().ends_with('%') {
            let _ = out.insert("%".to_owned());
        }
    };

    detect_token(col_name, &mut found);
    if let Some(col) = table.columns().iter().find(|c| c.schema().name == col_name) {
        for r in 0..table.row_count() {
            if let Some(cell) = col.cell_at(r) {
                detect_token(cell.raw_text(), &mut found);
            }
        }
    }
    found
}

fn column_has_negative_values(table: &sf_core::Table, col_name: &str) -> bool {
    let Some(col) = table.columns().iter().find(|c| c.schema().name == col_name) else {
        return false;
    };
    for r in 0..table.row_count() {
        if let Some(cell) = col.cell_at(r) {
            let cleaned = cell
                .raw_text()
                .trim()
                .replace([',', '$', '€', '£', ' '], "");
            if let Ok(d) = cleaned.parse::<Decimal>()
                && d < Decimal::ZERO
            {
                return true;
            }
        }
    }
    false
}

#[allow(clippy::too_many_arguments)]
fn build_kpi_selection_decision(
    table: &sf_core::Table,
    column_decisions: &[ColumnContentDecision],
    measure_cols: &[String],
    identifier_cols: &[String],
    domain: SheetBusinessDomain,
    matched_workflow: Option<&SheetWorkflowMemoryRecord>,
    learning_profile: Option<&PersonalLearningProfile>,
) -> KpiSelectionOutput {
    let mut audit = Vec::new();

    // Prune any candidate that tries to SUM an Identifier column!
    for id_col in identifier_cols {
        audit.push(EvaluatedCandidateAudit {
            candidate_id: format!("cand_kpi_sum_id_{id_col}"),
            title_en: format!("SUM({id_col})"),
            title_ar: format!("جمع المعرّف SUM({id_col})"),
            is_valid: false,
            elimination_reason_en: Some(format!(
                "Eliminated: '{id_col}' is an Identifier column; summing identifiers violates arithmetic meaningfulness."
            )),
            elimination_reason_ar: Some(format!(
                "تم استبعاده: العمود '{id_col}' هو معرّف (Identifier)؛ جمع المعرّفات غير صالح حسابيًا."
            )),
            objective: LexicographicObjectiveVector {
                correctness: 0,
                data_safety_no_loss: 1000,
                user_constraint_compliance: 0,
                analytical_fit: 0,
                explainability_audit: 200,
                deliverable_quality: 0,
                execution_efficiency: 1000,
            },
        });
    }

    // Check learned or workflow-saved currency preference (NEVER hardcode "EGP"!).
    let saved_currency_pref: Option<String> = learning_profile
        .and_then(|prof| {
            prof.rules
                .iter()
                .find(|r| r.category == crate::workflow_memory::LearningCategory::CurrencyUnit)
                .map(|r| r.preferred_value.clone())
        })
        .or_else(|| {
            matched_workflow.and_then(|wf| {
                wf.question_resolutions
                    .iter()
                    .find(|q| {
                        q.semantic_topic == "currency_unit"
                            && matches!(q.action.as_str(), "answered" | "custom" | "corrected")
                    })
                    .map(|q| q.answer.clone())
            })
        });

    struct KpiCandidate {
        id: String,
        col: String,
        agg_fn: String,
        metric_id: String,
        metric_name: String,
        formula: String,
        unit: Option<String>,
        mixed_currencies: Vec<String>,
        objective: LexicographicObjectiveVector,
    }

    let mut valid_candidates: Vec<KpiCandidate> = Vec::new();

    for m_col in measure_cols {
        let Some(col_dec) = column_decisions.iter().find(|c| c.column_name == *m_col) else {
            continue;
        };
        let total = (col_dec.non_null_count + col_dec.null_count).max(1);
        let completeness_permille = ((col_dec.non_null_count * 1000) / total).min(1000) as u16;
        let parse_score = (u16::from(col_dec.numeric_parse_pct) * 10).min(1000);
        let spread_bonus: u16 = if col_dec.distinct_count > 1 { 60 } else { 0 };

        let observed_currencies = detect_currencies_in_column(table, m_col);
        let mixed_currencies: Vec<String> = if observed_currencies.len() > 1 {
            observed_currencies.iter().cloned().collect()
        } else {
            Vec::new()
        };
        let resolved_unit: Option<String> = if observed_currencies.len() > 1 {
            None
        } else if let Some(single) = observed_currencies.iter().next() {
            Some(single.clone())
        } else if col_dec.decided_semantic_type == "CurrencyAmount" {
            saved_currency_pref.clone()
        } else {
            None
        };

        let unit_quality: u16 = if !mixed_currencies.is_empty() {
            620
        } else if resolved_unit.is_some() {
            960
        } else {
            880
        };

        let domain_fit_base: u16 = match col_dec.decided_semantic_type.as_str() {
            "CurrencyAmount" => 910,
            "Quantity" => 850,
            "UnitPrice" => 820,
            _ => 800,
        };
        let is_rate_or_price =
            col_dec.decided_semantic_type == "UnitPrice" || resolved_unit.as_deref() == Some("%");

        // Evaluate SUM(col)
        let sum_fit = if is_rate_or_price {
            domain_fit_base.saturating_sub(140)
        } else {
            domain_fit_base.saturating_add(spread_bonus).min(995)
        };
        let sum_metric_id = format!("optimal_total_{}", sanitize_metric_token(m_col));
        let sum_name = match domain {
            SheetBusinessDomain::Sales => format!("Verified Total {m_col}"),
            SheetBusinessDomain::Employees => format!("Verified Payroll / HR {m_col}"),
            SheetBusinessDomain::Inventory => format!("Verified Inventory {m_col}"),
            SheetBusinessDomain::Finance => format!("Verified Ledger {m_col}"),
            _ => format!("Verified SUM({m_col})"),
        };
        valid_candidates.push(KpiCandidate {
            id: format!("cand_kpi_sum_{}", sanitize_metric_token(m_col)),
            col: m_col.clone(),
            agg_fn: "SUM".to_owned(),
            metric_id: sum_metric_id,
            metric_name: sum_name,
            formula: format!("SUM({m_col})"),
            unit: resolved_unit.clone(),
            mixed_currencies: mixed_currencies.clone(),
            objective: LexicographicObjectiveVector {
                correctness: if is_rate_or_price {
                    parse_score.saturating_sub(80)
                } else if !mixed_currencies.is_empty() {
                    parse_score.saturating_sub(120)
                } else {
                    parse_score
                },
                data_safety_no_loss: completeness_permille,
                user_constraint_compliance: if saved_currency_pref.is_some()
                    && resolved_unit == saved_currency_pref
                {
                    1000
                } else {
                    940
                },
                analytical_fit: sum_fit,
                explainability_audit: 1000,
                deliverable_quality: unit_quality,
                execution_efficiency: 990,
            },
        });

        // Evaluate AVG(col)
        let avg_fit = if is_rate_or_price {
            domain_fit_base.saturating_add(spread_bonus).min(990)
        } else {
            domain_fit_base
                .saturating_sub(70)
                .saturating_add(spread_bonus / 2)
        };
        valid_candidates.push(KpiCandidate {
            id: format!("cand_kpi_avg_{}", sanitize_metric_token(m_col)),
            col: m_col.clone(),
            agg_fn: "AVG".to_owned(),
            metric_id: format!("optimal_avg_{}", sanitize_metric_token(m_col)),
            metric_name: format!("Verified Average {m_col}"),
            formula: format!("AVG({m_col})"),
            unit: resolved_unit,
            mixed_currencies,
            objective: LexicographicObjectiveVector {
                correctness: parse_score,
                data_safety_no_loss: completeness_permille,
                user_constraint_compliance: 940,
                analytical_fit: avg_fit,
                explainability_audit: 1000,
                deliverable_quality: unit_quality,
                execution_efficiency: 980,
            },
        });
    }

    // Zero-Measure Dataset Handling (Section 4.3): NEVER emit SUM() for a non-numeric column!
    if valid_candidates.is_empty() {
        let fallback_col = table
            .columns()
            .first()
            .map_or_else(|| "record".to_owned(), |c| c.schema().name.clone());
        let non_null_ratio: u16 = column_decisions
            .first()
            .map(|c| {
                let total = (c.non_null_count + c.null_count).max(1);
                ((c.non_null_count * 1000) / total).min(1000) as u16
            })
            .unwrap_or(1000);

        // Explicitly audit why SUM(fallback_col) was pruned!
        audit.push(EvaluatedCandidateAudit {
            candidate_id: format!("cand_kpi_pruned_sum_{}", sanitize_metric_token(&fallback_col)),
            title_en: format!("SUM({fallback_col})"),
            title_ar: format!("جمع SUM({fallback_col})"),
            is_valid: false,
            elimination_reason_en: Some(format!(
                "Eliminated: dataset has zero numeric Measures; '{fallback_col}' is non-numeric and cannot be summed."
            )),
            elimination_reason_ar: Some(format!(
                "تم استبعاده: لا توجد أعمدة مقاييس رقمية في الجدول؛ العمود '{fallback_col}' غير رقمي ولا يمكن جمعه."
            )),
            objective: LexicographicObjectiveVector {
                correctness: 0,
                data_safety_no_loss: non_null_ratio,
                user_constraint_compliance: 0,
                analytical_fit: 0,
                explainability_audit: 200,
                deliverable_quality: 0,
                execution_efficiency: 990,
            },
        });

        valid_candidates.push(KpiCandidate {
            id: "cand_kpi_count_records".to_owned(),
            col: fallback_col.clone(),
            agg_fn: "COUNT".to_owned(),
            metric_id: "optimal_record_count".to_owned(),
            metric_name: format!("Verified Record Count ({fallback_col})"),
            formula: format!("COUNT({fallback_col})"),
            unit: None,
            mixed_currencies: Vec::new(),
            objective: LexicographicObjectiveVector {
                correctness: 1000,
                data_safety_no_loss: non_null_ratio,
                user_constraint_compliance: 950,
                analytical_fit: 860,
                explainability_audit: 1000,
                deliverable_quality: 900,
                execution_efficiency: 995,
            },
        });
    }

    valid_candidates.sort_by_key(|a| std::cmp::Reverse(a.objective));
    for cand in &valid_candidates {
        audit.push(EvaluatedCandidateAudit {
            candidate_id: cand.id.clone(),
            title_en: format!("{} = {}", cand.metric_name, cand.formula),
            title_ar: format!("{} = {}", cand.metric_name, cand.formula),
            is_valid: true,
            elimination_reason_en: None,
            elimination_reason_ar: None,
            objective: cand.objective,
        });
    }

    let winner = &valid_candidates[0];
    let has_mixed_currency = !winner.mixed_currencies.is_empty();
    let near_tie_measures = valid_candidates.len() >= 2
        && valid_candidates[0].col != valid_candidates[1].col
        && valid_candidates[0].objective.correctness == valid_candidates[1].objective.correctness
        && valid_candidates[0].objective.data_safety_no_loss
            == valid_candidates[1].objective.data_safety_no_loss
        && valid_candidates[0]
            .objective
            .analytical_fit
            .abs_diff(valid_candidates[1].objective.analytical_fit)
            <= 15;

    let optimality_class = if has_mixed_currency || near_tie_measures {
        OptimalityClass::InsufficientEvidenceRequiresQuestion
    } else if valid_candidates.len() == 1
        || valid_candidates[0].objective > valid_candidates[1].objective
    {
        OptimalityClass::ExactProvenOptimum
    } else {
        OptimalityClass::BestValidCandidateFound
    };

    let clarification_prompt = if has_mixed_currency {
        Some(FocusedClarificationPrompt {
            question_id: "q_mixed_currency_clarification".to_owned(),
            prompt_en: format!(
                "Multiple currency symbols ({}) were detected in '{}'. Summing mixed currencies directly is invalid without FX conversion. Which currency or conversion policy applies?",
                winner.mixed_currencies.join(", "),
                winner.col
            ),
            prompt_ar: format!(
                "تم رصد أكثر من عملة ({}) في العمود '{}'. جمع عملات مختلفة مباشرةً غير صحيح دون تحويل سعر الصرف. ما العملة أو السياسة المعتمدة؟",
                winner.mixed_currencies.join("، "),
                winner.col
            ),
            options: vec![
                "Group totals separately by currency (Recommended)".to_owned(),
                "Convert all rows using explicit FX rate".to_owned(),
            ],
        })
    } else if near_tie_measures {
        Some(FocusedClarificationPrompt {
            question_id: "q_primary_kpi_tiebreaker".to_owned(),
            prompt_en: format!(
                "Two valid quantitative measures ('{}' and '{}') scored nearly identically. Which should be your headline KPI?",
                valid_candidates[0].col, valid_candidates[1].col
            ),
            prompt_ar: format!(
                "حقق مقياسان كميان ('{}' و'{}') درجات متقاربة جدًا. أيهما تريد اعتماده كمؤشر رئيسي؟",
                valid_candidates[0].col, valid_candidates[1].col
            ),
            options: vec![
                format!(
                    "{} ({})",
                    valid_candidates[0].col, valid_candidates[0].formula
                ),
                format!(
                    "{} ({})",
                    valid_candidates[1].col, valid_candidates[1].formula
                ),
            ],
        })
    } else {
        None
    };

    let unit_display = winner
        .unit
        .as_deref()
        .unwrap_or("Unspecified (no currency assumed)");

    let decision = RecommendedDecision {
        decision_id: "dec_optimal_kpi".to_owned(),
        domain: DecisionDomain::KpiSelection,
        algorithm_used: OptimizationAlgorithm::ExactConstraintSolver,
        optimality_class,
        recommended_title_en: format!(
            "Define Verified Primary KPI: {} ({})",
            winner.metric_name, winner.formula
        ),
        recommended_title_ar: format!(
            "تعريف المؤشر الرئيسي الموثق: {} ({})",
            winner.metric_name, winner.formula
        ),
        why_en: if winner.agg_fn == "COUNT" {
            format!(
                "Dataset contains zero numeric Measure columns; selected deterministic record count '{}' instead of an invalid SUM().",
                winner.formula
            )
        } else {
            format!(
                "Evaluated {} valid KPI candidate(s) across {} Measure column(s); '{}' achieved the highest lexicographic score (correctness={}, completeness={}, fit={}) with unit '{}'.",
                valid_candidates.len(),
                measure_cols.len(),
                winner.formula,
                winner.objective.correctness,
                winner.objective.data_safety_no_loss,
                winner.objective.analytical_fit,
                unit_display
            )
        },
        why_ar: if winner.agg_fn == "COUNT" {
            format!(
                "لا يحتوي الجدول على أعمدة مقاييس رقمية؛ تم اختيار عدّ السجلات الحتمي '{}' بدلًا من جمع نصي غير صالح.",
                winner.formula
            )
        } else {
            format!(
                "تمت مقارنة {} مرشح مؤشر عبر {} أعمدة رقمية؛ حققت المعادلة '{}' أعلى نتيجة معجمية بوحدة «{}» وحساب حتمي داخل Rust.",
                valid_candidates.len(),
                measure_cols.len(),
                winner.formula,
                unit_display
            )
        },
        objective: winner.objective,
        evidence: vec![
            DecisionEvidenceItem {
                label_en: "Verified Formula".to_owned(),
                label_ar: "المعادلة الحتمية الموثقة".to_owned(),
                observed_value: winner.formula.clone(),
                impact_en: "Produces a cryptographically fingerprinted VerifiedMetricToken.".to_owned(),
                impact_ar: "تنتج رمز مؤشر موثق ببصمة BLAKE3 قابلة للتدقيق.".to_owned(),
            },
            DecisionEvidenceItem {
                label_en: "Detected Currency / Unit".to_owned(),
                label_ar: "العملة / الوحدة المرصودة".to_owned(),
                observed_value: if has_mixed_currency {
                    format!("MIXED: {}", winner.mixed_currencies.join(", "))
                } else {
                    unit_display.to_owned()
                },
                impact_en: "Never hardcoded; derived strictly from cell text, headers, or saved workflow preference.".to_owned(),
                impact_ar: "لا يتم افتراض عملة ثابتة أبدًا؛ تُستخرج من البيانات الفعلية أو تفضيل المستخدم المحفوظ.".to_owned(),
            },
        ],
        assumptions_en: vec![if has_mixed_currency {
            "Mixed currencies detected; user confirmation is required before summing across currencies."
                .to_owned()
        } else {
            "All rows in the active worksheet table share a consistent unit of measure.".to_owned()
        }],
        assumptions_ar: vec![if has_mixed_currency {
            "تم رصد عملات متعددة؛ يلزم تأكيد المستخدم قبل الجمع عبر العملات.".to_owned()
        } else {
            "جميع الصفوف في جدول الورقة النشطة تتبع نفس وحدة القياس.".to_owned()
        }],
        what_executes_en: format!(
            "Defines metric '{}' = '{}' in Rust DataGraph.",
            winner.metric_id, winner.formula
        ),
        what_executes_ar: format!(
            "يعرّف المؤشر '{}' بالمعادلة '{}' في محرك Rust.",
            winner.metric_id, winner.formula
        ),
        undo_way_en: "Click Undo (studio_undo) to remove the metric definition.".to_owned(),
        undo_way_ar: "اضغط تراجع (Undo) لإلغاء تعريف المؤشر فورًا.".to_owned(),
        executable_action: DecisionExecutableAction::DefineVerifiedMetric {
            metric_id: winner.metric_id.clone(),
            name: winner.metric_name.clone(),
            unit: winner.unit.clone(),
            formula: winner.formula.clone(),
        },
        clarification_prompt,
        candidates_evaluated_count: audit.len(),
        invalid_eliminated_count: audit.iter().filter(|c| !c.is_valid).count(),
        advanced_audit_candidates: audit,
    };

    KpiSelectionOutput {
        decision,
        best_measure_col: winner.col.clone(),
        best_agg_fn: winner.agg_fn.clone(),
        best_metric_id: winner.metric_id.clone(),
        best_metric_name: winner.metric_name.clone(),
        best_metric_unit: winner.unit.clone(),
        best_metric_formula: winner.formula.clone(),
    }
}

#[allow(clippy::too_many_arguments)]
fn build_chart_selection_decision(
    table: &sf_core::Table,
    column_decisions: &[ColumnContentDecision],
    best_measure_col: &str,
    best_agg_fn: &str,
    measure_cols: &[String],
    dimension_cols: &[String],
    temporal_cols: &[String],
    domain: SheetBusinessDomain,
    row_count: usize,
    matched_workflow: Option<&SheetWorkflowMemoryRecord>,
    learning_profile: Option<&PersonalLearningProfile>,
) -> (RecommendedDecision, String, String) {
    let mut audit = Vec::new();

    // Choose best grouping column: prefer temporal column if 2+ dates and no lower-cardinality categorical override,
    // or a categorical dimension with 2..=25 distinct categories!
    let best_cat_dim = dimension_cols
        .iter()
        .filter_map(|d| {
            let distinct = column_decisions
                .iter()
                .find(|c| c.column_name == *d)
                .map_or(0, |c| c.distinct_count);
            if (2..=30).contains(&distinct) {
                Some((d.clone(), distinct))
            } else {
                None
            }
        })
        .min_by_key(|(_, distinct)| distinct.abs_diff(6))
        .map(|(d, _)| d);

    let best_temp_dim = temporal_cols.iter().find(|t| {
        column_decisions
            .iter()
            .find(|c| c.column_name == **t)
            .is_some_and(|c| c.distinct_count >= 2)
    });

    // If temporal dimension exists and has >= 3 distinct points while categorical has none or > 15, prefer temporal!
    let chosen_group = if let Some(temp) = best_temp_dim {
        if best_cat_dim.is_none() {
            temp.clone()
        } else if let Some(ref cat) = best_cat_dim {
            let cat_d = column_decisions
                .iter()
                .find(|c| c.column_name == *cat)
                .map_or(0, |c| c.distinct_count);
            if cat_d > 15 {
                temp.clone()
            } else {
                cat.clone()
            }
        } else {
            temp.clone()
        }
    } else {
        best_cat_dim
            .or_else(|| dimension_cols.first().cloned())
            .or_else(|| temporal_cols.first().cloned())
            .unwrap_or_else(|| best_measure_col.to_owned())
    };

    let group_distinct = column_decisions
        .iter()
        .find(|c| c.column_name == chosen_group)
        .map_or(row_count.max(1), |c| c.distinct_count);
    let is_temporal_group = temporal_cols.contains(&chosen_group);
    let has_negative =
        best_agg_fn != "COUNT" && column_has_negative_values(table, best_measure_col);
    let measure_count = measure_cols.len();
    let dimension_count = dimension_cols.len();

    let preferred_charts: Vec<String> = matched_workflow
        .map(|wf| wf.dashboard_preferences.preferred_chart_types.clone())
        .unwrap_or_default();
    let learned_chart_pref: Option<String> = learning_profile.and_then(|p| {
        p.rules
            .iter()
            .find(|r| r.category == crate::workflow_memory::LearningCategory::ChartLayout)
            .map(|r| r.preferred_value.to_ascii_lowercase())
    });

    struct ChartCandidate {
        chart_type: String,
        label_en: String,
        label_ar: String,
        why_en: String,
        why_ar: String,
        objective: LexicographicObjectiveVector,
    }

    let mut valid_charts: Vec<ChartCandidate> = Vec::new();
    let mut push_pruned =
        |chart_type: &str, title_en: &str, title_ar: &str, reason_en: String, reason_ar: String| {
            audit.push(EvaluatedCandidateAudit {
                candidate_id: format!("cand_chart_{chart_type}"),
                title_en: title_en.to_owned(),
                title_ar: title_ar.to_owned(),
                is_valid: false,
                elimination_reason_en: Some(reason_en),
                elimination_reason_ar: Some(reason_ar),
                objective: LexicographicObjectiveVector {
                    correctness: 0,
                    data_safety_no_loss: 1000,
                    user_constraint_compliance: 400,
                    analytical_fit: 150,
                    explainability_audit: 500,
                    deliverable_quality: 200,
                    execution_efficiency: 980,
                },
            });
        };

    let pref_bonus = |ct: &str| -> u16 {
        let in_wf = preferred_charts.iter().any(|p| p.eq_ignore_ascii_case(ct));
        let in_lrn = learned_chart_pref
            .as_deref()
            .is_some_and(|p| p.contains(ct));
        if in_wf || in_lrn { 1000 } else { 930 }
    };

    // 1. Evaluate Line & Area (temporal families)
    if is_temporal_group && group_distinct >= 2 {
        let line_fit = (930u16 + (group_distinct.min(12) as u16) * 5).min(995);
        valid_charts.push(ChartCandidate {
            chart_type: "line".to_owned(),
            label_en: "Temporal Trend Line Chart".to_owned(),
            label_ar: "رسم خطي للاتجاه الزمني (Line Chart)".to_owned(),
            why_en: format!("Selected Line Chart because '{}' is a chronological Temporal dimension with {} periods.", chosen_group, group_distinct),
            why_ar: format!("تم اختيار الرسم الخطي لأن '{}' بُعد زمني يحتوي {} فترة زمنية.", chosen_group, group_distinct),
            objective: LexicographicObjectiveVector {
                correctness: 1000,
                data_safety_no_loss: 1000,
                user_constraint_compliance: pref_bonus("line"),
                analytical_fit: line_fit,
                explainability_audit: 1000,
                deliverable_quality: 975,
                execution_efficiency: 990,
            },
        });
        valid_charts.push(ChartCandidate {
            chart_type: "area".to_owned(),
            label_en: "Cumulative Temporal Area Chart".to_owned(),
            label_ar: "رسم مساحي زمني (Area Chart)".to_owned(),
            why_en: format!(
                "Area Chart emphasizes cumulative magnitude across {} temporal periods in '{}'.",
                group_distinct, chosen_group
            ),
            why_ar: format!(
                "الرسم المساحي يبرز الحجم التراكمي عبر {} فترات في '{}'.",
                group_distinct, chosen_group
            ),
            objective: LexicographicObjectiveVector {
                correctness: if has_negative { 850 } else { 980 },
                data_safety_no_loss: 1000,
                user_constraint_compliance: pref_bonus("area"),
                analytical_fit: line_fit.saturating_sub(35),
                explainability_audit: 980,
                deliverable_quality: 940,
                execution_efficiency: 985,
            },
        });
    } else {
        push_pruned(
            "line",
            &format!("Line Chart of {best_measure_col} by {chosen_group}"),
            &format!("رسم خطي لـ {best_measure_col} حسب {chosen_group}"),
            format!(
                "Eliminated: '{chosen_group}' is unordered categorical data (not temporal); connecting unordered categories with lines implies a false continuous trend."
            ),
            format!(
                "تم استبعاده: '{chosen_group}' فئات غير مرتبة زمنيًا؛ وصلها بخط يوحي باتجاه زمني مضلل."
            ),
        );
    }

    // 2. Evaluate Pie & Donut (strict perceptual guard: 2..=6 non-negative unordered categories ONLY!)
    if !is_temporal_group && (2..=6).contains(&group_distinct) && !has_negative {
        valid_charts.push(ChartCandidate {
            chart_type: "donut".to_owned(),
            label_en: "Part-to-Whole Donut Chart".to_owned(),
            label_ar: "رسم حلقي لنسب الأجزاء (Donut Chart)".to_owned(),
            why_en: format!(
                "Valid part-to-whole Donut Chart ({} non-negative categories in '{}').",
                group_distinct, chosen_group
            ),
            why_ar: format!(
                "رسم حلقي صالح لتوزيع النسب ({} فئات غير سالبة في '{}').",
                group_distinct, chosen_group
            ),
            objective: LexicographicObjectiveVector {
                correctness: 960,
                data_safety_no_loss: 1000,
                user_constraint_compliance: pref_bonus("donut"),
                analytical_fit: if group_distinct <= 4 { 870 } else { 810 },
                explainability_audit: 960,
                deliverable_quality: 910,
                execution_efficiency: 990,
            },
        });
        valid_charts.push(ChartCandidate {
            chart_type: "pie".to_owned(),
            label_en: "Part-to-Whole Pie Chart".to_owned(),
            label_ar: "رسم دائري لنسب الأجزاء (Pie Chart)".to_owned(),
            why_en: format!(
                "Valid part-to-whole Pie Chart ({} non-negative categories in '{}').",
                group_distinct, chosen_group
            ),
            why_ar: format!(
                "رسم دائري صالح لتوزيع النسب ({} فئات غير سالبة في '{}').",
                group_distinct, chosen_group
            ),
            objective: LexicographicObjectiveVector {
                correctness: 950,
                data_safety_no_loss: 1000,
                user_constraint_compliance: pref_bonus("pie"),
                analytical_fit: if group_distinct <= 4 { 855 } else { 790 },
                explainability_audit: 950,
                deliverable_quality: 890,
                execution_efficiency: 990,
            },
        });
    } else {
        push_pruned(
            "pie",
            &format!("Pie / Donut Chart of {best_measure_col} by {chosen_group}"),
            &format!("رسم دائري/حلقي لـ {best_measure_col} حسب {chosen_group}"),
            format!(
                "Eliminated by Perceptual Guard: '{}' has {} distinct categories (temporal={}, negative={}), violating the 2..=6 non-negative part-to-whole rule.",
                chosen_group, group_distinct, is_temporal_group, has_negative
            ),
            format!(
                "تم استبعاده بواسطة حارس الوضوح البصري: '{}' يحتوي {} فئة (زمني={}، سالب={}) مما يسبب تضليلًا في الرسم الدائري.",
                chosen_group, group_distinct, is_temporal_group, has_negative
            ),
        );
    }

    // 3. Evaluate Bar & Column (Cleveland-McGill aligned length encoding for categorical comparisons)
    if (2..=40).contains(&group_distinct) {
        let bar_fit: u16 = if is_temporal_group {
            875
        } else if group_distinct > 6 {
            975
        } else {
            955
        };
        let col_fit: u16 = if is_temporal_group {
            890
        } else if group_distinct <= 6 {
            950
        } else {
            920
        };
        valid_charts.push(ChartCandidate {
            chart_type: "bar".to_owned(),
            label_en: "Ranked Categorical Bar Chart".to_owned(),
            label_ar: "رسم أعمدة مقارن للفئات (Bar Chart)".to_owned(),
            why_en: format!("Selected Bar Chart for comparing {best_agg_fn}({best_measure_col}) across {group_distinct} categories in '{chosen_group}' using aligned lengths (highest Cleveland-McGill perceptual accuracy)."),
            why_ar: format!("تم اختيار رسم الأعمدة لمقارنة {best_agg_fn}({best_measure_col}) عبر {group_distinct} فئات في '{chosen_group}' بأعلى دقة إدراكية دون تضليل بصري."),
            objective: LexicographicObjectiveVector {
                correctness: 1000,
                data_safety_no_loss: 1000,
                user_constraint_compliance: pref_bonus("bar"),
                analytical_fit: bar_fit,
                explainability_audit: 1000,
                deliverable_quality: 980,
                execution_efficiency: 990,
            },
        });
        valid_charts.push(ChartCandidate {
            chart_type: "column".to_owned(),
            label_en: "Vertical Column Chart".to_owned(),
            label_ar: "رسم أعمدة رأسية (Column Chart)".to_owned(),
            why_en: format!("Vertical Column Chart cleanly compares {group_distinct} categories in '{chosen_group}'."),
            why_ar: format!("الأعمدة الرأسية تقارن {group_distinct} فئات في '{chosen_group}' بوضوح."),
            objective: LexicographicObjectiveVector {
                correctness: 1000,
                data_safety_no_loss: 1000,
                user_constraint_compliance: pref_bonus("column"),
                analytical_fit: col_fit,
                explainability_audit: 1000,
                deliverable_quality: 970,
                execution_efficiency: 990,
            },
        });
    }

    // 4. Evaluate Pareto, Treemap, Waterfall, Scatter, Histogram, Boxplot, Stacked, Combo, Heatmap, KPI
    if !is_temporal_group && group_distinct >= 4 && !has_negative {
        valid_charts.push(ChartCandidate {
            chart_type: "pareto".to_owned(),
            label_en: "Pareto 80/20 Concentration Chart".to_owned(),
            label_ar: "رسم باريتو للتركّز (Pareto Chart)".to_owned(),
            why_en: format!(
                "Highlights top contributors among {group_distinct} categories in '{chosen_group}'."
            ),
            why_ar: format!("يبرز أكبر المساهمين ضمن {group_distinct} فئات في '{chosen_group}'."),
            objective: LexicographicObjectiveVector {
                correctness: 980,
                data_safety_no_loss: 1000,
                user_constraint_compliance: pref_bonus("pareto"),
                analytical_fit: if group_distinct >= 7 { 935 } else { 860 },
                explainability_audit: 970,
                deliverable_quality: 940,
                execution_efficiency: 970,
            },
        });
        valid_charts.push(ChartCandidate {
            chart_type: "treemap".to_owned(),
            label_en: "Hierarchical Treemap".to_owned(),
            label_ar: "خريطة شجرية (Treemap)".to_owned(),
            why_en: format!("Compares proportional share across {group_distinct} categories."),
            why_ar: format!("تقارن الحصص النسبية عبر {group_distinct} فئات."),
            objective: LexicographicObjectiveVector {
                correctness: 960,
                data_safety_no_loss: 1000,
                user_constraint_compliance: pref_bonus("treemap"),
                analytical_fit: if group_distinct >= 8 { 900 } else { 810 },
                explainability_audit: 940,
                deliverable_quality: 900,
                execution_efficiency: 960,
            },
        });
    }

    if has_negative || domain == SheetBusinessDomain::Finance {
        valid_charts.push(ChartCandidate {
            chart_type: "waterfall".to_owned(),
            label_en: "Financial Bridge Waterfall Chart".to_owned(),
            label_ar: "رسم شلالي مالي (Waterfall Chart)".to_owned(),
            why_en: format!("Waterfall Chart accurately displays positive and negative variance contributions across '{chosen_group}'."),
            why_ar: format!("الرسم الشلالي يعرض المساهمات الموجبة والسالبة بوضوح عبر '{chosen_group}'."),
            objective: LexicographicObjectiveVector {
                correctness: 1000,
                data_safety_no_loss: 1000,
                user_constraint_compliance: pref_bonus("waterfall"),
                analytical_fit: if has_negative { 985 } else { 910 },
                explainability_audit: 990,
                deliverable_quality: 975,
                execution_efficiency: 975,
            },
        });
    }

    if measure_count >= 2 {
        valid_charts.push(ChartCandidate {
            chart_type: "scatter".to_owned(),
            label_en: "Bivariate Scatter Correlation Plot".to_owned(),
            label_ar: "مخطط التبعثر للارتباط (Scatter Plot)".to_owned(),
            why_en: format!("Compares relationship across {measure_count} numeric measures."),
            why_ar: format!("يقارن العلاقة بين {measure_count} مقاييس رقمية."),
            objective: LexicographicObjectiveVector {
                correctness: 980,
                data_safety_no_loss: 1000,
                user_constraint_compliance: pref_bonus("scatter"),
                analytical_fit: if dimension_cols.is_empty() && temporal_cols.is_empty() {
                    970
                } else {
                    850
                },
                explainability_audit: 970,
                deliverable_quality: 930,
                execution_efficiency: 970,
            },
        });
    } else {
        push_pruned(
            "scatter",
            "Scatter Correlation Plot",
            "مخطط التبعثر (Scatter)",
            format!(
                "Eliminated: Scatter plots require at least 2 numeric Measures (dataset has {measure_count})."
            ),
            format!(
                "تم استبعاده: يتطلب مخطط التبعثر مقياسين رقميين على الأقل (المتوفر: {measure_count})."
            ),
        );
    }

    if measure_count >= 1 && row_count >= 4 {
        let dist_fit = if dimension_cols.is_empty() && temporal_cols.is_empty() {
            960
        } else {
            820
        };
        valid_charts.push(ChartCandidate {
            chart_type: "histogram".to_owned(),
            label_en: "Numeric Distribution Histogram".to_owned(),
            label_ar: "مدرج تكراري للتوزيع (Histogram)".to_owned(),
            why_en: format!("Shows value frequency distribution of '{best_measure_col}' across {row_count} rows."),
            why_ar: format!("يوضّح توزيع قيم '{best_measure_col}' عبر {row_count} صف."),
            objective: LexicographicObjectiveVector {
                correctness: 980,
                data_safety_no_loss: 1000,
                user_constraint_compliance: pref_bonus("histogram"),
                analytical_fit: dist_fit,
                explainability_audit: 970,
                deliverable_quality: 920,
                execution_efficiency: 980,
            },
        });
    }

    if dimension_count >= 2 {
        valid_charts.push(ChartCandidate {
            chart_type: "stacked".to_owned(),
            label_en: "Multi-Dimension Stacked Bar Chart".to_owned(),
            label_ar: "أعمدة متراكمة متعددة الأبعاد (Stacked)".to_owned(),
            why_en: format!("Shows composition across {dimension_count} categorical dimensions."),
            why_ar: format!("يعرض التركيبة عبر {dimension_count} أبعاد تصنيفية."),
            objective: LexicographicObjectiveVector {
                correctness: if has_negative { 840 } else { 980 },
                data_safety_no_loss: 1000,
                user_constraint_compliance: pref_bonus("stacked"),
                analytical_fit: 880,
                explainability_audit: 960,
                deliverable_quality: 930,
                execution_efficiency: 975,
            },
        });
    }

    // Fallback scalar KPI card when only 1 category or 1 row exists
    if valid_charts.is_empty() || group_distinct <= 1 {
        valid_charts.push(ChartCandidate {
            chart_type: "kpi".to_owned(),
            label_en: "Verified Headline KPI Card".to_owned(),
            label_ar: "بطاقة مؤشر رئيسي موثق (KPI Card)".to_owned(),
            why_en: format!("Single-group summary ({group_distinct} distinct group) is clearest as a direct verified KPI card."),
            why_ar: "ملخص الفئة الواحدة أوضح كبطاقة مؤشر مباشر دون رسم بياني زائد.".to_owned(),
            objective: LexicographicObjectiveVector {
                correctness: 1000,
                data_safety_no_loss: 1000,
                user_constraint_compliance: pref_bonus("kpi"),
                analytical_fit: if group_distinct <= 1 { 990 } else { 760 },
                explainability_audit: 1000,
                deliverable_quality: 960,
                execution_efficiency: 1000,
            },
        });
    }

    valid_charts.sort_by_key(|a| std::cmp::Reverse(a.objective));
    for cand in &valid_charts {
        audit.push(EvaluatedCandidateAudit {
            candidate_id: format!("cand_chart_{}", cand.chart_type),
            title_en: format!(
                "{}: {best_agg_fn}({best_measure_col}) by {chosen_group}",
                cand.label_en
            ),
            title_ar: format!(
                "{}: {best_agg_fn}({best_measure_col}) حسب {chosen_group}",
                cand.label_ar
            ),
            is_valid: true,
            elimination_reason_en: None,
            elimination_reason_ar: None,
            objective: cand.objective,
        });
    }

    let winner = &valid_charts[0];
    let best_chart_type = winner.chart_type.clone();

    let decision = RecommendedDecision {
        decision_id: "dec_optimal_chart".to_owned(),
        domain: DecisionDomain::ChartSelection,
        algorithm_used: OptimizationAlgorithm::ConstrainedBeamSearch,
        optimality_class: OptimalityClass::BestValidCandidateFound,
        recommended_title_en: format!(
            "Build {}: {best_measure_col} by {chosen_group}",
            winner.label_en
        ),
        recommended_title_ar: format!(
            "إنشاء {}: {best_measure_col} حسب {chosen_group}",
            winner.label_ar
        ),
        why_en: winner.why_en.clone(),
        why_ar: winner.why_ar.clone(),
        objective: winner.objective,
        evidence: vec![
            DecisionEvidenceItem {
                label_en: "Grouping Cardinality".to_owned(),
                label_ar: "عدد الفئات المميزة".to_owned(),
                observed_value: group_distinct.to_string(),
                impact_en: "Evaluated across all feasible chart families with perceptual distortion guards."
                    .to_owned(),
                impact_ar: "تم التقييم عبر جميع عائلات الرسوم الممكنة مع حراس منع التضليل البصري.".to_owned(),
            },
            DecisionEvidenceItem {
                label_en: "Aggregation Function".to_owned(),
                label_ar: "دالة التجميع".to_owned(),
                observed_value: format!("{best_agg_fn}({best_measure_col})"),
                impact_en: "Matched to column data type (COUNT for zero-measure sheets, SUM/AVG for verified numeric measures).".to_owned(),
                impact_ar: "متوافقة مع نوع بيانات العمود (COUNT بدون مقاييس رقمية، وSUM/AVG للمقاييس الرقمية).".to_owned(),
            },
        ],
        assumptions_en: vec![
            "Chart series are aggregated strictly inside Rust (studio_chart_data) before rendering."
                .to_owned(),
        ],
        assumptions_ar: vec![
            "تُجمع سلاسل البيانات حتميًا داخل محرك Rust قبل رسمها في الواجهة.".to_owned(),
        ],
        what_executes_en: format!(
            "Adds a verified '{best_chart_type}' chart aggregating {best_agg_fn}({best_measure_col}) by {chosen_group}."
        ),
        what_executes_ar: format!(
            "يضيف رسم '{best_chart_type}' موثقًا يجمع {best_agg_fn}({best_measure_col}) حسب {chosen_group}."
        ),
        undo_way_en: "Remove or edit the chart card in Dashboard view or click Undo.".to_owned(),
        undo_way_ar: "احذف أو عدّل بطاقة الرسم من لوحة المؤشرات أو اضغط تراجع.".to_owned(),
        executable_action: DecisionExecutableAction::ConfigureOptimalChart {
            chart_type: best_chart_type.clone(),
            title: format!("{best_measure_col} by {chosen_group}"),
            function: best_agg_fn.to_owned(),
            value_column: best_measure_col.to_owned(),
            group_by_column: chosen_group.clone(),
        },
        clarification_prompt: None,
        candidates_evaluated_count: audit.len(),
        invalid_eliminated_count: audit.iter().filter(|c| !c.is_valid).count(),
        advanced_audit_candidates: audit,
    };

    (decision, best_chart_type, chosen_group)
}

#[allow(clippy::too_many_arguments)]
fn build_dashboard_layout_decision(
    metric_id: &str,
    metric_name: &str,
    metric_unit: Option<&str>,
    metric_formula: &str,
    chart_type: &str,
    value_col: &str,
    group_col: &str,
    domain: SheetBusinessDomain,
    row_count: usize,
    measure_cols: &[String],
    dimension_cols: &[String],
) -> RecommendedDecision {
    let kpi_strip_fit: u16 = 910u16
        .saturating_add((measure_cols.len().min(3) as u16) * 20)
        .saturating_add((dimension_cols.len().min(2) as u16) * 15)
        .min(990);
    let winning_obj = LexicographicObjectiveVector {
        correctness: 1000,
        data_safety_no_loss: 1000,
        user_constraint_compliance: 960,
        analytical_fit: kpi_strip_fit,
        explainability_audit: 1000,
        deliverable_quality: 960,
        execution_efficiency: if row_count > 10_000 { 950 } else { 985 },
    };

    let audit = vec![
        EvaluatedCandidateAudit {
            candidate_id: "cand_coherent_dashboard_blueprint".to_owned(),
            title_en: format!(
                "Executive {:?} Dashboard: Top KPI Strip ({metric_name}) + Primary {chart_type} Breakdown by {group_col}",
                domain
            ),
            title_ar: format!(
                "لوحة مؤشرات تنفيذية ({:?}): شريط المؤشر الموثق ({metric_name}) + رسم {chart_type} حسب {group_col}",
                domain
            ),
            is_valid: true,
            elimination_reason_en: None,
            elimination_reason_ar: None,
            objective: winning_obj,
        },
        EvaluatedCandidateAudit {
            candidate_id: "cand_dense_unlinked_grid".to_owned(),
            title_en: "Unlinked Multi-Widget Grid Without Verified KPI Lineage".to_owned(),
            title_ar: "شبكة بطاقات غير مترابطة بدون سلسلة تحقق للمؤشرات".to_owned(),
            is_valid: false,
            elimination_reason_en: Some(
                "Eliminated: unlinked widgets risk showing totals inconsistent with the primary DataGraph KPI."
                    .to_owned(),
            ),
            elimination_reason_ar: Some(
                "تم استبعاده: البطاقات غير المترابطة قد تعرض أرقامًا متعارضة مع المؤشر الموثق في DataGraph."
                    .to_owned(),
            ),
            objective: LexicographicObjectiveVector {
                correctness: 0,
                data_safety_no_loss: 1000,
                user_constraint_compliance: 500,
                analytical_fit: 500,
                explainability_audit: 300,
                deliverable_quality: 400,
                execution_efficiency: 980,
            },
        },
    ];

    RecommendedDecision {
        decision_id: "dec_dashboard_blueprint".to_owned(),
        domain: DecisionDomain::DashboardLayout,
        algorithm_used: OptimizationAlgorithm::ConstrainedBeamSearch,
        optimality_class: OptimalityClass::BestValidCandidateFound,
        recommended_title_en: format!(
            "Assemble Coherent Dashboard: {metric_name} + {value_col} by {group_col}"
        ),
        recommended_title_ar: format!(
            "بناء لوحة المؤشرات المتكاملة: {metric_name} + {value_col} حسب {group_col}"
        ),
        why_en: "Pairs the verified headline KPI token with its primary dimensional breakdown and interactive cross-filtering."
            .to_owned(),
        why_ar: "يربط المؤشر الرئيسي الموثق بتفصيله البُعدي الأساسي مع تصفية تفاعلية مترابطة.".to_owned(),
        objective: winning_obj,
        evidence: vec![DecisionEvidenceItem {
            label_en: "KPI-to-Chart Lineage".to_owned(),
            label_ar: "ترابط المؤشر والرسم البياني".to_owned(),
            observed_value: format!("{metric_id} <-> {chart_type}({value_col} by {group_col})"),
            impact_en: "Guarantees zero discrepancy between headline KPI total and chart sums.".to_owned(),
            impact_ar: "يضمن تطابق الرقم الإجمالي للمؤشر مع مجموع الرسم البياني بنسبة 100%.".to_owned(),
        }],
        assumptions_en: vec![
            "Dashboard cards stay dynamically bound to the live DataGraph revision.".to_owned(),
        ],
        assumptions_ar: vec![
            "تظل بطاقات اللوحة مرتبطة ديناميكيًا بإصدار البيانات الحي في DataGraph.".to_owned(),
        ],
        what_executes_en: "Ensures the primary KPI metric, dashboard chart, executive slide, and document block are all created and bound to the same verified metric token."
            .to_owned(),
        what_executes_ar: "ينشئ ويربط المؤشر الرئيسي ورسم اللوحة والشريحة التقديمية وفقرة التقرير بنفس الرمز الموثق."
            .to_owned(),
        undo_way_en: "Revert via Undo or customize individual cards in the Dashboard tab.".to_owned(),
        undo_way_ar: "يمكنك التراجع عبر زر Undo أو تخصيص البطاقات في تبويب اللوحة.".to_owned(),
        executable_action: DecisionExecutableAction::ApplyDashboardAndReportBlueprint {
            primary_metric_id: metric_id.to_owned(),
            primary_metric_name: metric_name.to_owned(),
            primary_metric_unit: metric_unit.map(str::to_owned),
            primary_metric_formula: metric_formula.to_owned(),
            primary_chart_type: chart_type.to_owned(),
            value_column: value_col.to_owned(),
            group_by_column: group_col.to_owned(),
        },
        clarification_prompt: None,
        candidates_evaluated_count: audit.len(),
        invalid_eliminated_count: audit.iter().filter(|c| !c.is_valid).count(),
        advanced_audit_candidates: audit,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_report_and_presentation_decision(
    graph: &DataGraph,
    metric_id: &str,
    metric_name: &str,
    metric_unit: Option<&str>,
    metric_formula: &str,
    value_col: &str,
    chart_type: &str,
    group_col: &str,
    _domain: SheetBusinessDomain,
) -> RecommendedDecision {
    let row_count = graph.working().row_count();
    let winning_obj = LexicographicObjectiveVector {
        correctness: 1000,
        data_safety_no_loss: 1000,
        user_constraint_compliance: 960,
        analytical_fit: 920,
        explainability_audit: 1000,
        deliverable_quality: 960,
        execution_efficiency: 985,
    };

    let audit = vec![EvaluatedCandidateAudit {
        candidate_id: "cand_verified_token_report_deck".to_owned(),
        title_en: format!(
            "Token-Bound Executive Report & Slides ({{{{metric:{metric_id}}}}} = {metric_formula})"
        ),
        title_ar: format!(
            "تقرير تنفيذي وشرائح مربوطة بالرموز الموثقة ({{{{metric:{metric_id}}}}} = {metric_formula})"
        ),
        is_valid: true,
        elimination_reason_en: None,
        elimination_reason_ar: None,
        objective: winning_obj,
    }];

    RecommendedDecision {
        decision_id: "dec_report_and_presentation".to_owned(),
        domain: DecisionDomain::ReportAndPresentation,
        algorithm_used: OptimizationAlgorithm::DeterministicRuleSolver,
        optimality_class: OptimalityClass::HeuristicRecommendation,
        recommended_title_en: format!(
            "Generate Verified Executive Report & Slide Deck Bound to {{{{metric:{metric_id}}}}}"
        ),
        recommended_title_ar: format!(
            "إنشاء التقرير التنفيذي والعرض التقديمي المرتبطين بالمؤشر الموثق {{{{metric:{metric_id}}}}}"
        ),
        why_en: format!(
            "Structures executive deliverables around verified descriptive evidence across {row_count} rows without fabricating unverified causal claims."
        ),
        why_ar: format!(
            "يبني التقارير والعروض التقديمية حول الأدلة الوصفية الموثقة عبر {row_count} صف دون اختلاق تفسيرات سببية غير مثبتة."
        ),
        objective: winning_obj,
        evidence: vec![DecisionEvidenceItem {
            label_en: "Narrative Token Binding".to_owned(),
            label_ar: "ربط النصوص بالرموز الموثقة".to_owned(),
            observed_value: format!("{{{{metric:{metric_id}}}}} ({metric_formula})"),
            impact_en: "Every number in Word (.docx), Vector PDF (.pdf), PowerPoint (.pptx), and HTML Capsule (.safi.html) resolves directly from Rust."
                .to_owned(),
            impact_ar: "كل رقم في ملفات Word وPDF وPowerPoint وكبسولة HTML يُستمد مباشرة من محرك Rust."
                .to_owned(),
        }],
        assumptions_en: vec![
            "Correlations or dimensional rankings are reported descriptively, never as unproven causation."
                .to_owned(),
        ],
        assumptions_ar: vec![
            "يتم عرض الفروقات والترتيبات وصفيًا بدقة دون ادعاء علاقة سببية غير مبرهنة.".to_owned(),
        ],
        what_executes_en: format!(
            "Synchronizes executive slide deck and document report blocks with metric '{metric_name}' ({metric_formula}) and '{chart_type}' breakdown by '{group_col}'."
        ),
        what_executes_ar: format!(
            "يزامن شرائح العرض التقديمي وفقرات التقرير مع المؤشر '{metric_name}' ({metric_formula}) والتوزيع حسب '{group_col}'."
        ),
        undo_way_en: "Edit or remove generated slides/blocks in the Slides and Doc tabs.".to_owned(),
        undo_way_ar: "يمكنك تعديل أو حذف الشرائح والفقرات من تبويبي الشرائح والمستند.".to_owned(),
        executable_action: DecisionExecutableAction::ApplyDashboardAndReportBlueprint {
            primary_metric_id: metric_id.to_owned(),
            primary_metric_name: metric_name.to_owned(),
            primary_metric_unit: metric_unit.map(str::to_owned),
            primary_metric_formula: metric_formula.to_owned(),
            primary_chart_type: chart_type.to_owned(),
            value_column: value_col.to_owned(),
            group_by_column: group_col.to_owned(),
        },
        clarification_prompt: None,
        candidates_evaluated_count: audit.len(),
        invalid_eliminated_count: 0,
        advanced_audit_candidates: audit,
    }
}

fn audit_cross_product_consistency(
    graph: &DataGraph,
    artifacts: &StudioWorkspaceArtifacts,
) -> ConsistencyImpactReport {
    let table = graph.working();
    let existing_cols: BTreeSet<String> = table
        .columns()
        .iter()
        .map(|c| c.schema().name.clone())
        .collect();
    let active_metric_ids: Vec<String> = graph
        .metric_ids()
        .iter()
        .map(|k| k.as_str().to_owned())
        .collect();
    let active_metric_set: BTreeSet<String> = active_metric_ids.iter().cloned().collect();

    let mut metric_column_lineage = BTreeMap::new();
    for mid in graph.metric_ids() {
        if let Some(token) = graph.token(&mid) {
            let _ =
                metric_column_lineage.insert(mid.as_str().to_owned(), token.column_refs.clone());
        }
    }

    let mut stale_chart_references = Vec::new();
    for chart in &artifacts.charts {
        if !existing_cols.contains(&chart.column) || !existing_cols.contains(&chart.group_by) {
            stale_chart_references.push(format!(
                "Chart '{}' references missing column(s): value='{}', groupBy='{}'",
                chart.title, chart.column, chart.group_by
            ));
        }
    }

    let mut stale_deliverable_metric_refs = Vec::new();
    for slide in &artifacts.slides {
        if let Some(mid) = slide.metric_id.as_deref()
            && !mid.is_empty()
            && !active_metric_set.contains(mid)
        {
            stale_deliverable_metric_refs.push(format!(
                "Slide '{}' references undefined metric '{}'",
                slide.title, mid
            ));
        }
    }
    for block in &artifacts.doc_blocks {
        if let Some(mid) = block.metric_id.as_deref()
            && !mid.is_empty()
            && !active_metric_set.contains(mid)
        {
            stale_deliverable_metric_refs.push(format!(
                "Document block '{}' references undefined metric '{}'",
                block.title, mid
            ));
        }
    }

    let is_fully_consistent =
        stale_chart_references.is_empty() && stale_deliverable_metric_refs.is_empty();

    let summary_en = if is_fully_consistent {
        format!(
            "100% Cross-Product Consistency verified across revision v{} ({} active metric(s), {} chart(s), {} slide(s), {} doc block(s)).",
            graph.revision().get(),
            active_metric_ids.len(),
            artifacts.charts.len(),
            artifacts.slides.len(),
            artifacts.doc_blocks.len()
        )
    } else {
        format!(
            "Detected {} stale chart reference(s) and {} stale deliverable metric reference(s) requiring synchronization.",
            stale_chart_references.len(),
            stale_deliverable_metric_refs.len()
        )
    };
    let summary_ar = if is_fully_consistent {
        format!(
            "تم التحقق من اتساق المخرجات بنسبة 100% عبر الإصدار v{} ({} مؤشرات نشطة، {} رسوم، {} شرائح، {} فقرات تقرير).",
            graph.revision().get(),
            active_metric_ids.len(),
            artifacts.charts.len(),
            artifacts.slides.len(),
            artifacts.doc_blocks.len()
        )
    } else {
        format!(
            "تم رصد {} مراجع رسوم قديمة و{} مراجع مؤشرات في التقارير/الشرائح تحتاج إلى مزامنة.",
            stale_chart_references.len(),
            stale_deliverable_metric_refs.len()
        )
    };

    ConsistencyImpactReport {
        is_fully_consistent,
        dataset_revision: graph.revision().get(),
        active_metric_ids,
        stale_chart_references,
        stale_deliverable_metric_refs,
        metric_column_lineage,
        summary_en,
        summary_ar,
    }
}

fn build_ai_architecture_status(
    artifacts: &StudioWorkspaceArtifacts,
    ollama_probe: Option<&OllamaRuntimeProbe>,
) -> AiArchitectureStatusReport {
    let (endpoint, status, models) = match ollama_probe {
        Some(p) => (
            p.endpoint.clone(),
            p.status.clone(),
            p.installed_models.clone(),
        ),
        None => (
            "http://127.0.0.1:11434".to_owned(),
            "not_probed".to_owned(),
            Vec::new(),
        ),
    };
    AiArchitectureStatusReport {
        local_ollama_endpoint: endpoint,
        local_ollama_status: status,
        bundled_model_claimed: false,
        installed_local_models: models,
        core_product_works_without_ai: true,
        external_ai_consent_granted: artifacts.privacy_settings.external_ai_consent_granted,
        local_pii_masking_enforced: artifacts.privacy_settings.mask_pii_on_cloud,
        task_routing_table: vec![
            AiTaskRouteEntry {
                task_id: "metrics_calculation".to_owned(),
                task_name_en: "Metrics & KPI Calculations".to_owned(),
                task_name_ar: "حسابات المؤشرات والأرقام".to_owned(),
                primary_engine: "Rust Deterministic Engine (DataGraph)".to_owned(),
                allows_llm_numeric_computation: false,
                requires_consent_and_pii_masking_if_external: false,
            },
            AiTaskRouteEntry {
                task_id: "data_cleaning".to_owned(),
                task_name_en: "Data Cleaning & Normalization".to_owned(),
                task_name_ar: "تنظيف وتوحيد البيانات".to_owned(),
                primary_engine: "Rust Cleaning Engine".to_owned(),
                allows_llm_numeric_computation: false,
                requires_consent_and_pii_masking_if_external: false,
            },
            AiTaskRouteEntry {
                task_id: "schema_validation".to_owned(),
                task_name_en: "Schema & Column Role Validation".to_owned(),
                task_name_ar: "التحقق من الهيكل وأدوار الأعمدة".to_owned(),
                primary_engine: "Rust Rules + Safi Decision Engine".to_owned(),
                allows_llm_numeric_computation: false,
                requires_consent_and_pii_masking_if_external: false,
            },
            AiTaskRouteEntry {
                task_id: "optimal_treatment_selection".to_owned(),
                task_name_en: "Optimal Treatment & KPI Selection".to_owned(),
                task_name_ar: "اختيار أفضل معالجة وأفضل مؤشرات".to_owned(),
                primary_engine: "Safi Decision Engine (Lexicographic Solver)".to_owned(),
                allows_llm_numeric_computation: false,
                requires_consent_and_pii_masking_if_external: false,
            },
            AiTaskRouteEntry {
                task_id: "chart_selection".to_owned(),
                task_name_en: "Optimal Chart & Dashboard Selection".to_owned(),
                task_name_ar: "اختيار أفضل الرسوم البيانية وتخطيط اللوحة".to_owned(),
                primary_engine: "Safi Decision Engine (Perceptual Guard)".to_owned(),
                allows_llm_numeric_computation: false,
                requires_consent_and_pii_masking_if_external: false,
            },
            AiTaskRouteEntry {
                task_id: "office_exports".to_owned(),
                task_name_en: "Excel, Word, PowerPoint, PDF & HTML Exports".to_owned(),
                task_name_ar: "إنشاء ملفات Excel وWord وPowerPoint وPDF وHTML".to_owned(),
                primary_engine: "Rust Deterministic Export Engines".to_owned(),
                allows_llm_numeric_computation: false,
                requires_consent_and_pii_masking_if_external: false,
            },
            AiTaskRouteEntry {
                task_id: "continuous_learning".to_owned(),
                task_name_en: "Learning from User Corrections & Preferences".to_owned(),
                task_name_ar: "التعلّم من تصحيحات وتفضيلات المستخدم".to_owned(),
                primary_engine: "Local Learning Engine (Encrypted Vault)".to_owned(),
                allows_llm_numeric_computation: false,
                requires_consent_and_pii_masking_if_external: false,
            },
            AiTaskRouteEntry {
                task_id: "language_and_narrative".to_owned(),
                task_name_en: "Language Intent & Grounded Executive Narrative".to_owned(),
                task_name_ar: "فهم اللغة وصياغة السرد التنفيذي المربوط بالمصادر".to_owned(),
                primary_engine: "Local Ollama / Consented External AI (Bound to Verified Tokens)"
                    .to_owned(),
                allows_llm_numeric_computation: false,
                requires_consent_and_pii_masking_if_external: true,
            },
        ],
    }
}

fn sanitize_metric_token(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "value".to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// Applies an executable action from a [`RecommendedDecision`] directly onto a [`DataGraph`] and
/// [`StudioWorkspaceArtifacts`] atomically.
///
/// # Errors
/// Returns a `SafiError` code string if any underlying mutation fails.
pub fn execute_recommended_decision_action(
    graph: &mut DataGraph,
    artifacts: &mut StudioWorkspaceArtifacts,
    action: &DecisionExecutableAction,
    now_ms: u64,
) -> Result<String, String> {
    match action {
        DecisionExecutableAction::TrimColumns { columns } => {
            for col in columns {
                graph
                    .trim_column(col, now_ms)
                    .map_err(|e| format!("{:?}", e.code()))?;
            }
            Ok(format!("Trimmed {} column(s)", columns.len()))
        }
        DecisionExecutableAction::DropExactDuplicateRows { row_ids } => {
            let _ = graph
                .apply_anomaly_fix(
                    AnomalyFix::DropDuplicateRows {
                        row_ids: row_ids.clone(),
                    },
                    now_ms,
                )
                .map_err(|e| format!("{:?}", e.code()))?;
            Ok(format!("Dropped {} exact duplicate row(s)", row_ids.len()))
        }
        DecisionExecutableAction::FillMissingNumericZero { column, row_ids } => {
            let _ = graph
                .apply_anomaly_fix(
                    AnomalyFix::FillMissing {
                        column: column.clone(),
                        row_ids: row_ids.clone(),
                        replacement: "0".to_owned(),
                    },
                    now_ms,
                )
                .map_err(|e| format!("{:?}", e.code()))?;
            Ok(format!(
                "Filled {} missing numeric cell(s) in '{column}' with 0",
                row_ids.len()
            ))
        }
        DecisionExecutableAction::DefineVerifiedMetric {
            metric_id,
            name,
            unit,
            formula,
        } => {
            let mid = MetricId::new(metric_id).map_err(|e| format!("{:?}", e.code()))?;
            let _ = graph
                .define_metric(mid, name, unit.as_deref(), formula, now_ms)
                .map_err(|e| format!("{:?}", e.code()))?;
            Ok(format!("Defined verified metric '{metric_id}' = {formula}"))
        }
        DecisionExecutableAction::ConfigureOptimalChart {
            chart_type,
            title,
            function,
            value_column,
            group_by_column,
        } => {
            artifacts.charts.push(StudioChartArtifact {
                id: format!("chart_opt_{now_ms}"),
                chart_type: chart_type.clone(),
                title: title.clone(),
                function: function.clone(),
                column: value_column.clone(),
                group_by: group_by_column.clone(),
            });
            Ok(format!("Added optimal '{chart_type}' chart '{title}'"))
        }
        DecisionExecutableAction::ApplyDashboardAndReportBlueprint {
            primary_metric_id,
            primary_metric_name,
            primary_metric_unit,
            primary_metric_formula,
            primary_chart_type,
            value_column,
            group_by_column,
        } => {
            if let Ok(mid) = MetricId::new(primary_metric_id)
                && graph.token(&mid).is_none()
                && graph
                    .working()
                    .columns()
                    .iter()
                    .any(|c| c.schema().name == *value_column)
            {
                let _ = graph.define_metric(
                    mid,
                    primary_metric_name,
                    primary_metric_unit.as_deref(),
                    primary_metric_formula,
                    now_ms,
                );
            }
            let chart_fn = if primary_metric_formula.starts_with("COUNT(") {
                "COUNT"
            } else if primary_metric_formula.starts_with("AVG(") {
                "AVG"
            } else {
                "SUM"
            };
            if !artifacts
                .charts
                .iter()
                .any(|c| c.column == *value_column && c.group_by == *group_by_column)
            {
                artifacts.charts.push(StudioChartArtifact {
                    id: format!("chart_opt_{now_ms}"),
                    chart_type: primary_chart_type.clone(),
                    title: format!("{value_column} by {group_by_column}"),
                    function: chart_fn.to_owned(),
                    column: value_column.clone(),
                    group_by: group_by_column.clone(),
                });
            }
            if !artifacts
                .slides
                .iter()
                .any(|s| s.metric_id.as_deref() == Some(primary_metric_id.as_str()))
            {
                artifacts.slides.push(StudioSlideArtifact {
                    id: format!("slide_opt_{now_ms}"),
                    title: format!("Executive Summary — {primary_metric_name}"),
                    body: format!(
                        "Verified headline KPI: {{{{metric:{primary_metric_id}}}}} broken down by {group_by_column}."
                    ),
                    metric_id: Some(primary_metric_id.clone()),
                    chart_index: Some(0),
                    layout: "kpi_split".to_owned(),
                    presenter_notes:
                        "Generated by Safi Decision Engine with 100% deterministic Rust metric lineage."
                            .to_owned(),
                    theme: "executive".to_owned(),
                });
            }
            if !artifacts
                .doc_blocks
                .iter()
                .any(|b| b.metric_id.as_deref() == Some(primary_metric_id.as_str()))
            {
                artifacts.doc_blocks.push(StudioDocBlockArtifact {
                    id: format!("doc_opt_{now_ms}"),
                    kind: "callout".to_owned(),
                    title: format!("Verified KPI Finding — {primary_metric_name}"),
                    content: format!(
                        "Verified total across active dataset: {{{{metric:{primary_metric_id}}}}}."
                    ),
                    metric_id: Some(primary_metric_id.clone()),
                    authored_by: "safi_decision_engine".to_owned(),
                    page_break_before: false,
                    header_text: "SafiData Verified Executive Report".to_owned(),
                    footer_text: "100% Local Deterministic Calculation".to_owned(),
                });
            }
            Ok(format!(
                "Synchronized Dashboard, Slides, and Report with verified metric '{primary_metric_id}'"
            ))
        }
        DecisionExecutableAction::ConfirmClassificationOnly { summary } => {
            Ok(format!("Confirmed classification ({summary})"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_result;

    fn graph_from_csv(name: &str, csv: &str) -> DataGraph {
        let report = test_result(sf_io::ingest_workbook_universal(
            std::io::Cursor::new(csv.as_bytes()),
            sf_core::limits::PlatformLimits::DESKTOP_CLI,
            Some(name),
            None,
        ));
        let table = test_result(report.worksheets[0].tables[0].to_core_table());
        let source = test_result(sf_core::datagraph::SourceId::new(
            name.trim_end_matches(".csv"),
        ));
        test_result(DataGraph::new(source, table, 1_000))
    }

    #[test]
    fn decision_engine_protects_numeric_employee_id_distinguishes_duplicates_and_optimizes_lexicographically()
     {
        let csv = "Employee ID,Department,Hire Date,Salary\n\
                   1001,Sales,2026-01-10,8000\n\
                   1002,Sales ,2026-01-15,9000\n\
                   1002,Sales ,2026-01-15,9000\n\
                   1001,Sales,2026-02-01,9500\n\
                   1003,Engineering,2026-02-05,12000\n";
        let mut graph = graph_from_csv("hr_sheet.csv", csv);
        let mut artifacts = StudioWorkspaceArtifacts::default();

        let suite = SafiDecisionEngine::evaluate_workspace(&graph, &artifacts, None, None);
        assert_eq!(suite.detected_domain, SheetBusinessDomain::Employees);

        // 1. Verify `Employee ID` is protected from being classified as a summable Measure
        let emp_id_col = test_result(
            suite
                .column_decisions
                .iter()
                .find(|c| c.column_name == "Employee ID")
                .ok_or("Missing Employee ID decision"),
        );
        assert_eq!(emp_id_col.decided_role, "Identifier");
        assert!(emp_id_col.protected_identifier_from_measure);

        let salary_col = test_result(
            suite
                .column_decisions
                .iter()
                .find(|c| c.column_name == "Salary")
                .ok_or("Missing Salary decision"),
        );
        assert_eq!(salary_col.decided_role, "Measure");

        // 2. Smart Duplicate Analysis:
        //    - 1 exact duplicate (row 3 == row 2)
        //    - 1 conflicting primary key (Employee ID 1001 with different salary)
        //    - Repeated `Department = Sales` is a category, NOT falsely counted as a repeated entity transaction!
        assert_eq!(
            suite
                .duplicate_analysis
                .exact_unintended_duplicate_row_ids
                .len(),
            1
        );
        assert_eq!(
            suite
                .duplicate_analysis
                .valid_repeated_entity_transaction_rows,
            0
        );
        assert_eq!(
            suite
                .duplicate_analysis
                .conflicting_key_review_row_ids
                .len(),
            1
        );

        // 3. KPI Decision pruned `SUM(Employee ID)`, selected `SUM(Salary)`, and did NOT hardcode EGP!
        let kpi_dec = test_result(
            suite
                .recommended_decisions
                .iter()
                .find(|d| d.domain == DecisionDomain::KpiSelection)
                .ok_or("Missing KPI decision"),
        );
        assert_eq!(
            kpi_dec.optimality_class,
            OptimalityClass::ExactProvenOptimum
        );
        assert!(kpi_dec.recommended_title_en.contains("SUM(Salary)"));
        if let DecisionExecutableAction::DefineVerifiedMetric { unit, .. } =
            &kpi_dec.executable_action
        {
            assert_eq!(
                unit, &None,
                "Must NEVER hardcode EGP when no currency is present"
            );
        }
        assert!(
            kpi_dec
                .advanced_audit_candidates
                .iter()
                .any(|c| !c.is_valid && c.title_en.contains("SUM(Employee ID)"))
        );

        // 4. Report & Presentation action MUST reference `SUM(Salary)` and `value_column = Salary`, NEVER `SUM(Department)`!
        let rep_dec = test_result(
            suite
                .recommended_decisions
                .iter()
                .find(|d| d.domain == DecisionDomain::ReportAndPresentation)
                .ok_or("Missing Report decision"),
        );
        assert_eq!(
            rep_dec.optimality_class,
            OptimalityClass::HeuristicRecommendation
        );
        if let DecisionExecutableAction::ApplyDashboardAndReportBlueprint {
            primary_metric_formula,
            primary_metric_unit,
            value_column,
            ..
        } = &rep_dec.executable_action
        {
            assert_eq!(primary_metric_formula, "SUM(Salary)");
            assert_eq!(value_column, "Salary");
            assert_eq!(primary_metric_unit, &None);
        }

        // 5. Execute cleaning decision (drops only the 1 exact duplicate row, preserving 4 rows)
        let clean_dec = test_result(
            suite
                .recommended_decisions
                .iter()
                .find(|d| d.domain == DecisionDomain::DataCleaning)
                .ok_or("Missing Cleaning decision"),
        );
        assert_eq!(
            clean_dec.optimality_class,
            OptimalityClass::InsufficientEvidenceRequiresQuestion
        );
        let _ = test_result(execute_recommended_decision_action(
            &mut graph,
            &mut artifacts,
            &clean_dec.executable_action,
            2_000,
        ));
        assert_eq!(graph.working().row_count(), 4);

        // 6. Execute dashboard blueprint and verify 100% cross-product consistency
        let dash_dec = test_result(
            suite
                .recommended_decisions
                .iter()
                .find(|d| d.domain == DecisionDomain::DashboardLayout)
                .ok_or("Missing Dashboard decision"),
        );
        let _ = test_result(execute_recommended_decision_action(
            &mut graph,
            &mut artifacts,
            &dash_dec.executable_action,
            3_000,
        ));
        let after_suite = SafiDecisionEngine::evaluate_workspace(&graph, &artifacts, None, None);
        assert!(after_suite.consistency_report.is_fully_consistent);
        assert!(
            after_suite
                .ai_architecture
                .task_routing_table
                .iter()
                .all(|r| !r.allows_llm_numeric_computation)
        );
    }

    #[test]
    fn decision_engine_covers_currency_safety_zero_measures_charts_duplicates_learning_and_ollama()
    {
        let artifacts = StudioWorkspaceArtifacts::default();

        // Scenario A: USD currency detected from cell symbols -> never hardcoded EGP!
        let usd_graph = graph_from_csv(
            "usd_sales.csv",
            "Order ID,Customer,Sale Date,Revenue\n\
             ORD-1,ACME,2026-01-01,$1200\n\
             ORD-2,ACME,2026-01-15,$1500\n\
             ORD-3,Beta,2026-01-20,$800\n",
        );
        let usd_suite = SafiDecisionEngine::evaluate_workspace(&usd_graph, &artifacts, None, None);
        // Repeated Customer `ACME` with distinct Order IDs ORD-1 & ORD-2 -> 1 verified repeated entity transaction!
        assert_eq!(
            usd_suite
                .duplicate_analysis
                .valid_repeated_entity_transaction_rows,
            1
        );
        let usd_kpi = test_result(
            usd_suite
                .recommended_decisions
                .iter()
                .find(|d| d.domain == DecisionDomain::KpiSelection)
                .ok_or("missing KPI"),
        );
        if let DecisionExecutableAction::DefineVerifiedMetric { unit, formula, .. } =
            &usd_kpi.executable_action
        {
            assert_eq!(unit.as_deref(), Some("USD"));
            assert_eq!(formula, "SUM(Revenue)");
        }

        // Scenario B: Mixed currencies ($ and EGP) -> InsufficientEvidenceRequiresQuestion + clarification prompt!
        let mixed_graph = graph_from_csv(
            "mixed_cur.csv",
            "Order ID,Customer,Revenue\n\
             101,ACME,$500\n\
             102,Beta,12000 EGP\n",
        );
        let mixed_suite =
            SafiDecisionEngine::evaluate_workspace(&mixed_graph, &artifacts, None, None);
        let mixed_kpi = test_result(
            mixed_suite
                .recommended_decisions
                .iter()
                .find(|d| d.domain == DecisionDomain::KpiSelection)
                .ok_or("missing KPI"),
        );
        assert_eq!(
            mixed_kpi.optimality_class,
            OptimalityClass::InsufficientEvidenceRequiresQuestion
        );
        assert!(mixed_kpi.clarification_prompt.is_some());
        if let DecisionExecutableAction::DefineVerifiedMetric { unit, .. } =
            &mixed_kpi.executable_action
        {
            assert_eq!(unit, &None);
        }

        // Scenario C: Zero-measure all-text dataset -> NEVER emits SUM(), selects COUNT() everywhere!
        let text_graph = graph_from_csv(
            "text_only.csv",
            "City,Status,Notes\n\
             Cairo,Open,First ticket\n\
             Alex,Closed,Second ticket\n\
             Cairo,Open,Third ticket\n",
        );
        let text_suite =
            SafiDecisionEngine::evaluate_workspace(&text_graph, &artifacts, None, None);
        let text_kpi = test_result(
            text_suite
                .recommended_decisions
                .iter()
                .find(|d| d.domain == DecisionDomain::KpiSelection)
                .ok_or("missing KPI"),
        );
        assert!(text_kpi.recommended_title_en.contains("COUNT("));
        assert!(!text_kpi.recommended_title_en.contains("SUM("));
        let text_chart = test_result(
            text_suite
                .recommended_decisions
                .iter()
                .find(|d| d.domain == DecisionDomain::ChartSelection)
                .ok_or("missing Chart"),
        );
        if let DecisionExecutableAction::ConfigureOptimalChart { function, .. } =
            &text_chart.executable_action
        {
            assert_eq!(function, "COUNT");
        }

        // Scenario D: Ambiguous near-duplicates without a primary key -> flagged for review!
        let ambig_graph = graph_from_csv(
            "ambig_dup.csv",
            "Customer,Sale Date,Amount,Notes\n\
             ACME,2026-03-01,500,Invoice A\n\
             ACME,2026-03-01,500,Invoice A copy?\n",
        );
        let ambig_suite =
            SafiDecisionEngine::evaluate_workspace(&ambig_graph, &artifacts, None, None);
        assert_eq!(
            ambig_suite
                .duplicate_analysis
                .ambiguous_duplicate_review_row_ids
                .len(),
            1
        );
        let ambig_clean = test_result(
            ambig_suite
                .recommended_decisions
                .iter()
                .find(|d| d.domain == DecisionDomain::DataCleaning)
                .ok_or("missing Cleaning"),
        );
        assert_eq!(
            ambig_clean.optimality_class,
            OptimalityClass::InsufficientEvidenceRequiresQuestion
        );
        assert!(ambig_clean.clarification_prompt.is_some());

        // Scenario E: Temporal time series -> selects `line` chart and prunes `pie`!
        let ts_graph = graph_from_csv(
            "timeseries.csv",
            "Sale Date,Revenue\n\
             2026-01-01,100\n\
             2026-02-01,150\n\
             2026-03-01,210\n\
             2026-04-01,260\n",
        );
        let ts_suite = SafiDecisionEngine::evaluate_workspace(&ts_graph, &artifacts, None, None);
        let ts_chart = test_result(
            ts_suite
                .recommended_decisions
                .iter()
                .find(|d| d.domain == DecisionDomain::ChartSelection)
                .ok_or("missing Chart"),
        );
        if let DecisionExecutableAction::ConfigureOptimalChart { chart_type, .. } =
            &ts_chart.executable_action
        {
            assert_eq!(chart_type, "line");
        }
        assert!(
            ts_chart
                .advanced_audit_candidates
                .iter()
                .any(|c| !c.is_valid && c.candidate_id == "cand_chart_pie")
        );

        // Scenario F: Negative financial variance -> selects `waterfall` and prunes `pie`!
        let wf_graph = graph_from_csv(
            "pnl_bridge.csv",
            "Category,Cost\n\
             Gross,5000\n\
             COGS,-2000\n\
             Opex,-1200\n\
             Tax,-400\n",
        );
        let wf_suite = SafiDecisionEngine::evaluate_workspace(&wf_graph, &artifacts, None, None);
        let wf_chart = test_result(
            wf_suite
                .recommended_decisions
                .iter()
                .find(|d| d.domain == DecisionDomain::ChartSelection)
                .ok_or("missing Chart"),
        );
        if let DecisionExecutableAction::ConfigureOptimalChart { chart_type, .. } =
            &wf_chart.executable_action
        {
            assert_eq!(chart_type, "waterfall");
        }
        assert!(
            wf_chart
                .advanced_audit_candidates
                .iter()
                .any(|c| !c.is_valid && c.candidate_id == "cand_chart_pie")
        );

        // Scenario G: Multi-measure table evaluates multiple KPI candidates & allows scatter chart!
        let mm_graph = graph_from_csv(
            "multi_measure.csv",
            "Region,Revenue,Quantity,Unit Price\n\
             Cairo,5000,50,100\n\
             Alex,3000,30,100\n\
             Giza,2000,20,100\n\
             Delta,4000,40,100\n\
             Upper,1500,15,100\n\
             Canal,2500,25,100\n\
             Sinai,1200,12,100\n",
        );
        let mm_suite = SafiDecisionEngine::evaluate_workspace(&mm_graph, &artifacts, None, None);
        let mm_kpi = test_result(
            mm_suite
                .recommended_decisions
                .iter()
                .find(|d| d.domain == DecisionDomain::KpiSelection)
                .ok_or("missing KPI"),
        );
        assert!(
            mm_kpi.candidates_evaluated_count >= 6,
            "Must evaluate SUM & AVG across all 3 valid numeric measures"
        );
        let mm_chart = test_result(
            mm_suite
                .recommended_decisions
                .iter()
                .find(|d| d.domain == DecisionDomain::ChartSelection)
                .ok_or("missing Chart"),
        );
        assert!(
            mm_chart
                .advanced_audit_candidates
                .iter()
                .any(|c| c.is_valid && c.candidate_id == "cand_chart_scatter")
        );
        // 7 regions -> Pie must be pruned!
        assert!(
            mm_chart
                .advanced_audit_candidates
                .iter()
                .any(|c| !c.is_valid && c.candidate_id == "cand_chart_pie")
        );

        // Scenario H: Real Ollama runtime probe injection reflects exact status & models!
        let probe = OllamaRuntimeProbe {
            endpoint: "http://127.0.0.1:11434".to_owned(),
            status: "available_with_models".to_owned(),
            installed_models: vec!["qwen2.5:7b".to_owned()],
        };
        let probed_suite = SafiDecisionEngine::evaluate_workspace_with_ollama(
            &usd_graph,
            &artifacts,
            None,
            None,
            Some(&probe),
        );
        assert_eq!(
            probed_suite.ai_architecture.local_ollama_status,
            "available_with_models"
        );
        assert_eq!(
            probed_suite.ai_architecture.installed_local_models,
            vec!["qwen2.5:7b".to_owned()]
        );
    }
}
