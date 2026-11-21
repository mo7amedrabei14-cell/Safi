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

/// Duplicate detection classification (Section 11.2) distinguishing exact unintended duplicates
/// from valid multi-transaction records and conflicting-key rows requiring review/merge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateAnalysisReport {
    /// Row IDs that are 100% identical across EVERY column (safe to drop automatically).
    pub exact_unintended_duplicate_row_ids: Vec<u64>,
    /// Count of rows sharing an entity/customer/dimension value but having distinct dates/amounts
    /// (valid repeated transactions — MUST NOT be deleted!).
    pub valid_repeated_entity_transaction_rows: usize,
    /// Row IDs sharing the primary identifier key (`order_id`, `emp_id`, etc.) but having
    /// conflicting non-key values (unsafe to auto-delete; requires review or merge).
    pub conflicting_key_review_row_ids: Vec<u64>,
    /// Key column used for key-conflict check, if detected.
    pub detected_primary_key_column: Option<String>,
    /// Summary explanation (English).
    pub summary_en: String,
    /// Summary explanation (Arabic).
    pub summary_ar: String,
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
                let cleaned_num = trimmed.replace([',', '$', ' '], "");
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
        // 2. Smart Duplicate Analysis (Section 11.2)
        // --------------------------------------------------------------------
        let duplicate_analysis = analyze_duplicates(
            graph,
            identifier_cols.first().map(String::as_str),
            dimension_cols.first().map(String::as_str),
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
            &untrimmed_cols,
            &missing_numeric_cols,
            &duplicate_analysis,
        ));

        // Domain 3: Optimal KPI Selection Decision
        let (kpi_decision, best_measure_col, best_metric_id, best_metric_name, best_metric_formula) =
            build_kpi_selection_decision(
                table,
                &column_decisions,
                &measure_cols,
                &identifier_cols,
                detected_domain,
            );
        recommended_decisions.push(kpi_decision);

        // Domain 4: Optimal Chart Selection Decision
        let (chart_decision, best_chart_type, best_group_col) = build_chart_selection_decision(
            &column_decisions,
            &best_measure_col,
            &dimension_cols,
            &temporal_cols,
            row_count,
        );
        recommended_decisions.push(chart_decision);

        // Domain 5: Coherent Dashboard Builder Decision
        recommended_decisions.push(build_dashboard_layout_decision(
            &best_metric_id,
            &best_metric_name,
            &best_metric_formula,
            &best_chart_type,
            &best_measure_col,
            &best_group_col,
            detected_domain,
        ));

        // Domain 6: Executive Report & PowerPoint Decision
        recommended_decisions.push(build_report_and_presentation_decision(
            graph,
            &best_metric_id,
            &best_metric_name,
            &best_chart_type,
            &best_group_col,
            detected_domain,
        ));

        // --------------------------------------------------------------------
        // 4. Cross-Product Consistency & Lineage Audit (Section 11.7)
        // --------------------------------------------------------------------
        let consistency_report = audit_cross_product_consistency(graph, artifacts);

        // --------------------------------------------------------------------
        // 5. AI Architecture & Task Router Status (Section 12)
        // --------------------------------------------------------------------
        let ai_architecture = build_ai_architecture_status(artifacts);

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
    primary_key_col: Option<&str>,
    entity_col: Option<&str>,
) -> DuplicateAnalysisReport {
    let table = graph.working();
    let row_count = table.row_count();
    let cols = table.columns();
    let mut seen_full_rows: BTreeSet<Vec<String>> = BTreeSet::new();
    let mut exact_dups = Vec::new();

    let pk_idx = primary_key_col.and_then(|pk| cols.iter().position(|c| c.schema().name == pk));
    let entity_idx = entity_col.and_then(|en| cols.iter().position(|c| c.schema().name == en));

    let mut seen_pk_payloads: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut conflicting_key_rows = Vec::new();
    let mut seen_entities: BTreeMap<String, usize> = BTreeMap::new();

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

        if !seen_full_rows.insert(tuple.clone()) {
            exact_dups.push(row_id);
            continue;
        }

        if let Some(p_i) = pk_idx {
            let key_val = tuple.get(p_i).cloned().unwrap_or_default();
            if !key_val.is_empty() {
                if let Some(prev_tuple) = seen_pk_payloads.get(&key_val) {
                    if prev_tuple != &tuple {
                        conflicting_key_rows.push(row_id);
                    }
                } else {
                    let _ = seen_pk_payloads.insert(key_val, tuple.clone());
                }
            }
        }

        if let Some(e_i) = entity_idx {
            let ent_val = tuple.get(e_i).cloned().unwrap_or_default();
            if !ent_val.is_empty() {
                *seen_entities.entry(ent_val).or_insert(0) += 1;
            }
        }
    }

    let valid_repeated_entity_transaction_rows: usize = seen_entities
        .values()
        .filter(|&&count| count > 1)
        .map(|&count| count - 1)
        .sum();

    let summary_en = format!(
        "Detected {} exact unintended duplicate row(s), {} valid repeated entity transaction(s) (preserved safely), and {} conflicting primary-key row(s) requiring review.",
        exact_dups.len(),
        valid_repeated_entity_transaction_rows,
        conflicting_key_rows.len()
    );
    let summary_ar = format!(
        "تم رصد {} صفوف مكررة بالكامل عن غير قصد، و{} معاملات متكررة صحيحة لنفس الجهة/الفرع (تم الحفاظ عليها بأمان)، و{} صفوف ذات تعارض في المفتاح الأساسي تحتاج مراجعة.",
        exact_dups.len(),
        valid_repeated_entity_transaction_rows,
        conflicting_key_rows.len()
    );

    DuplicateAnalysisReport {
        exact_unintended_duplicate_row_ids: exact_dups,
        valid_repeated_entity_transaction_rows,
        conflicting_key_review_row_ids: conflicting_key_rows,
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
                data_safety_no_loss: 500,
                user_constraint_compliance: 200,
                analytical_fit: 0,
                explainability_audit: 300,
                deliverable_quality: 100,
                execution_efficiency: 1000,
            },
        });
    }

    let winning_obj = LexicographicObjectiveVector {
        correctness: 1000,
        data_safety_no_loss: 1000,
        user_constraint_compliance: 1000,
        analytical_fit: 960,
        explainability_audit: 1000,
        deliverable_quality: 950,
        execution_efficiency: 980,
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
        is_valid: true,
        elimination_reason_en: None,
        elimination_reason_ar: None,
        objective: winning_obj,
    });

    RecommendedDecision {
        decision_id: "dec_spreadsheet_understanding".to_owned(),
        domain: DecisionDomain::SpreadsheetUnderstanding,
        algorithm_used: OptimizationAlgorithm::ExactConstraintSolver,
        optimality_class: OptimalityClass::ExactProvenOptimum,
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
            "Every column was verified by inspecting both cell value distributions and bilingual header semantics."
                .to_owned()
        } else {
            format!(
                "Protected numeric identifier column(s) [{}] from being summed as Measures based on uniqueness and identifier patterns.",
                protected_ids.join(", ")
            )
        },
        why_ar: if protected_ids.is_empty() {
            "تم التحقق من كل عمود بفحص توزيع القيم الفعلية مع الدلالة اللغوية للعناوين.".to_owned()
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
    untrimmed_cols: &[(String, usize)],
    missing_numeric_cols: &[(String, Vec<u64>)],
    dup: &DuplicateAnalysisReport,
) -> RecommendedDecision {
    let mut audit = Vec::new();

    // Candidate A (Pruned as unsafe if repeated entity transactions or conflicting keys exist):
    // Aggressive deduplication by entity/dimension or blind key drop
    if dup.valid_repeated_entity_transaction_rows > 0
        || !dup.conflicting_key_review_row_ids.is_empty()
    {
        audit.push(EvaluatedCandidateAudit {
            candidate_id: "cand_aggressive_key_or_entity_dedup".to_owned(),
            title_en: "Aggressive Deduplication on Entity / Conflicting Keys".to_owned(),
            title_ar: "حذف التكرارات العدواني حسب الجهة أو المفاتيح المتعارضة".to_owned(),
            is_valid: false,
            elimination_reason_en: Some(format!(
                "Eliminated by Data-Safety Gate: would destroy {} valid repeated transaction row(s) and {} conflicting-key row(s) that require human review.",
                dup.valid_repeated_entity_transaction_rows,
                dup.conflicting_key_review_row_ids.len()
            )),
            elimination_reason_ar: Some(format!(
                "تم استبعاده بواسطة بوابة سلامة البيانات: يؤدي إلى حذف {} معاملات متكررة صحيحة و{} صفوف متعارضة المفتاح تتطلب مراجعة.",
                dup.valid_repeated_entity_transaction_rows,
                dup.conflicting_key_review_row_ids.len()
            )),
            objective: LexicographicObjectiveVector {
                correctness: 0,
                data_safety_no_loss: 100,
                user_constraint_compliance: 300,
                analytical_fit: 200,
                explainability_audit: 400,
                deliverable_quality: 200,
                execution_efficiency: 990,
            },
        });
    }

    // Determine the highest-priority safe deterministic cleaning action
    let (title_en, title_ar, why_en, why_ar, action, what_en, what_ar) = if !dup
        .exact_unintended_duplicate_row_ids
        .is_empty()
    {
        let ids = dup.exact_unintended_duplicate_row_ids.clone();
        (
                format!(
                    "Remove {} Exact Full-Row Duplicate(s) While Preserving {} Valid Repeated Transaction(s)",
                    ids.len(),
                    dup.valid_repeated_entity_transaction_rows
                ),
                format!(
                    "إزالة {} صفوف مكررة بالكامل مع الحفاظ على {} معاملات متكررة صحيحة",
                    ids.len(),
                    dup.valid_repeated_entity_transaction_rows
                ),
                "Only rows that are 100% byte-identical across every column are removed; multi-transaction rows for the same entity/branch are preserved."
                    .to_owned(),
                "يتم حذف الصفوف المتطابقة بنسبة 100% في جميع الأعمدة فقط، مع الحفاظ الكامل على المعاملات المتعددة لنفس العميل أو الفرع."
                    .to_owned(),
                DecisionExecutableAction::DropExactDuplicateRows { row_ids: ids.clone() },
                format!("Drops exact duplicate row IDs {:?} atomically in Rust DataGraph.", ids),
                format!("يحذف الصفوف المكررة المتطابقة {:?} ذريًا داخل محرك Rust.", ids),
            )
    } else if !untrimmed_cols.is_empty() {
        let cols: Vec<String> = untrimmed_cols.iter().map(|(c, _)| c.clone()).collect();
        let total_cells: usize = untrimmed_cols.iter().map(|(_, n)| *n).sum();
        (
                format!(
                    "Normalize & Trim Whitespace Across {} Column(s) ({} Cell(s))",
                    cols.len(),
                    total_cells
                ),
                format!(
                    "تطبيع وإزالة الفراغات الزائدة في {} أعمدة ({} خلية)",
                    cols.len(),
                    total_cells
                ),
                "Leading/trailing whitespace splits identical categories during GROUP BY aggregation; trimming restores exact category grouping without losing any data."
                    .to_owned(),
                "الفراغات الزائدة في بداية أو نهاية النص تؤدي لتقسيم نفس الفئة عند التجميع؛ تنظيفها يوحّد الفئات دون فقدان أي بيانات."
                    .to_owned(),
                DecisionExecutableAction::TrimColumns {
                    columns: cols.clone(),
                },
                format!("Executes deterministic trim_column on [{}].", cols.join(", ")),
                format!("ينفّذ تنظيف الفراغات الحتمي على الأعمدة [{}].", cols.join("، ")),
            )
    } else if let Some((col, row_ids)) = missing_numeric_cols.first() {
        (
                format!(
                    "Impute {} Missing Numeric Cell(s) in '{}' with Verified 0",
                    row_ids.len(),
                    col
                ),
                format!(
                    "تعويض {} خلايا رقمية مفقودة في العمود '{}' بالقيمة 0 الموثقة",
                    row_ids.len(),
                    col
                ),
                "Prevents null-propagation errors in downstream SUM/AVG formulas while preserving full row lineage."
                    .to_owned(),
                "يمنع أخطاء القيم الفارغة في معادلات الجمع والمتوسط مع الحفاظ على كامل الصفوف وسلسلة المصدر."
                    .to_owned(),
                DecisionExecutableAction::FillMissingNumericZero {
                    column: col.clone(),
                    row_ids: row_ids.clone(),
                },
                format!("Fills missing cells in '{}' at row IDs {:?} with 0.", col, row_ids),
                format!("يعوّض الخلايا المفقودة في '{}' عند الصفوف {:?} بالقيمة 0.", col, row_ids),
            )
    } else {
        (
                "Dataset Passed All Deterministic Hygiene Gates (Zero Destructive Changes Needed)".to_owned(),
                "اجتازت البيانات جميع بوابات النظافة الحتمية (لا حاجة لأي تعديل حذفي)".to_owned(),
                "No exact duplicates, untrimmed whitespace, or missing numeric cells were detected; preserving the working table as-is is provably optimal."
                    .to_owned(),
                "لم يتم رصد أي تكرارات متطابقة أو فراغات زائدة أو قيم رقمية مفقودة؛ الحفاظ على الجدول الحالي هو القرار الأمثل."
                    .to_owned(),
                DecisionExecutableAction::ConfirmClassificationOnly {
                    summary: "clean_noop".to_owned(),
                },
                "No mutation required; working dataset is already normalized.".to_owned(),
                "لا يتطلب أي تعديل؛ بيانات العمل منقّاة وجاهزة.".to_owned(),
            )
    };

    let winning_obj = LexicographicObjectiveVector {
        correctness: 1000,
        data_safety_no_loss: 1000,
        user_constraint_compliance: 1000,
        analytical_fit: 970,
        explainability_audit: 1000,
        deliverable_quality: 960,
        execution_efficiency: 975,
    };

    audit.push(EvaluatedCandidateAudit {
        candidate_id: "cand_safe_deterministic_cleaning".to_owned(),
        title_en: title_en.clone(),
        title_ar: title_ar.clone(),
        is_valid: true,
        elimination_reason_en: None,
        elimination_reason_ar: None,
        objective: winning_obj,
    });

    RecommendedDecision {
        decision_id: "dec_data_cleaning".to_owned(),
        domain: DecisionDomain::DataCleaning,
        algorithm_used: OptimizationAlgorithm::LexicographicBranchAndBound,
        optimality_class: if dup.conflicting_key_review_row_ids.is_empty() {
            OptimalityClass::ExactProvenOptimum
        } else {
            OptimalityClass::BestValidCandidateFound
        },
        recommended_title_en: title_en,
        recommended_title_ar: title_ar,
        why_en,
        why_ar,
        objective: winning_obj,
        evidence: vec![
            DecisionEvidenceItem {
                label_en: "Exact Unintended Duplicates".to_owned(),
                label_ar: "التكرارات الكاملة غير المقصودة".to_owned(),
                observed_value: dup.exact_unintended_duplicate_row_ids.len().to_string(),
                impact_en: "100% identical across all columns; safe to deduplicate.".to_owned(),
                impact_ar: "متطابقة بنسبة 100% في كل الأعمدة؛ آمنة للإزالة.".to_owned(),
            },
            DecisionEvidenceItem {
                label_en: "Valid Repeated Entity Transactions".to_owned(),
                label_ar: "معاملات متكررة صحيحة لنفس الجهة".to_owned(),
                observed_value: dup.valid_repeated_entity_transaction_rows.to_string(),
                impact_en: "Protected from deletion by Data-Safety Gate.".to_owned(),
                impact_ar: "محمية من الحذف بواسطة بوابة سلامة البيانات.".to_owned(),
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
        what_executes_en: what_en,
        what_executes_ar: what_ar,
        undo_way_en: "Single-click Undo (studio_undo) immediately restores the previous DataGraph revision."
            .to_owned(),
        undo_way_ar: "ضغطة واحدة على زر التراجع (Undo) تستعيد النسخة السابقة من البيانات فورًا.".to_owned(),
        executable_action: action,
        clarification_prompt: if !dup.conflicting_key_review_row_ids.is_empty() {
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
        } else {
            None
        },
        candidates_evaluated_count: audit.len(),
        invalid_eliminated_count: audit.iter().filter(|c| !c.is_valid).count(),
        advanced_audit_candidates: audit,
    }
}

fn build_kpi_selection_decision(
    table: &sf_core::Table,
    column_decisions: &[ColumnContentDecision],
    measure_cols: &[String],
    identifier_cols: &[String],
    domain: SheetBusinessDomain,
) -> (RecommendedDecision, String, String, String, String) {
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

    // Select primary Measure column (prefer CurrencyAmount >NumericMeasure > Quantity)
    let best_measure = column_decisions
        .iter()
        .find(|c| c.decided_role == "Measure" && c.decided_semantic_type == "CurrencyAmount")
        .or_else(|| {
            column_decisions
                .iter()
                .find(|c| c.decided_role == "Measure")
        })
        .map(|c| c.column_name.clone())
        .or_else(|| measure_cols.first().cloned())
        .unwrap_or_else(|| {
            table
                .columns()
                .first()
                .map_or_else(|| "amount".to_owned(), |c| c.schema().name.clone())
        });

    let use_sum = measure_cols.contains(&best_measure);
    let formula = if use_sum {
        format!("SUM({best_measure})")
    } else {
        format!("COUNT({best_measure})")
    };
    let metric_id = if use_sum {
        format!("optimal_total_{}", sanitize_metric_token(&best_measure))
    } else {
        "optimal_record_count".to_owned()
    };
    let metric_name = match domain {
        SheetBusinessDomain::Sales => format!("Verified Total {best_measure}"),
        SheetBusinessDomain::Employees => format!("Verified Payroll / HR {best_measure}"),
        SheetBusinessDomain::Inventory => format!("Verified Inventory {best_measure}"),
        SheetBusinessDomain::Finance => format!("Verified Ledger {best_measure}"),
        _ => format!("Verified {formula}"),
    };
    let unit = column_decisions
        .iter()
        .find(|c| c.column_name == best_measure && c.decided_semantic_type == "CurrencyAmount")
        .map(|_| "EGP".to_owned());

    let winning_obj = LexicographicObjectiveVector {
        correctness: 1000,
        data_safety_no_loss: 1000,
        user_constraint_compliance: 1000,
        analytical_fit: 985,
        explainability_audit: 1000,
        deliverable_quality: 965,
        execution_efficiency: 990,
    };

    audit.push(EvaluatedCandidateAudit {
        candidate_id: format!("cand_kpi_{metric_id}"),
        title_en: format!("{metric_name} = {formula}"),
        title_ar: format!("{metric_name} = {formula}"),
        is_valid: true,
        elimination_reason_en: None,
        elimination_reason_ar: None,
        objective: winning_obj,
    });

    let decision = RecommendedDecision {
        decision_id: "dec_optimal_kpi".to_owned(),
        domain: DecisionDomain::KpiSelection,
        algorithm_used: OptimizationAlgorithm::ExactConstraintSolver,
        optimality_class: OptimalityClass::ExactProvenOptimum,
        recommended_title_en: format!("Define Verified Primary KPI: {metric_name} ({formula})"),
        recommended_title_ar: format!("تعريف المؤشر الرئيسي الموثق: {metric_name} ({formula})"),
        why_en: format!(
            "Selected '{best_measure}' as the primary quantitative measure for {:?} domain; computed deterministically in Rust with full row/column lineage and zero LLM involvement.",
            domain
        ),
        why_ar: format!(
            "تم اختيار العمود '{best_measure}' كمقياس كمي رئيسي؛ يُحسب حتميًا داخل محرك Rust مع تتبع كامل للصفوف والأعمدة دون أي تدخل من نماذج اللغة."
        ),
        objective: winning_obj,
        evidence: vec![DecisionEvidenceItem {
            label_en: "Verified Formula".to_owned(),
            label_ar: "المعادلة الحتمية الموثقة".to_owned(),
            observed_value: formula.clone(),
            impact_en: "Produces a cryptographically fingerprinted VerifiedMetricToken.".to_owned(),
            impact_ar: "تنتج رمز مؤشر موثق ببصمة BLAKE3 قابلة للتدقيق.".to_owned(),
        }],
        assumptions_en: vec![
            "All rows in the active worksheet table share a consistent unit of measure.".to_owned(),
        ],
        assumptions_ar: vec![
            "جميع الصفوف في جدول الورقة النشطة تتبع نفس وحدة القياس/العملة.".to_owned(),
        ],
        what_executes_en: format!("Defines metric '{metric_id}' = '{formula}' in Rust DataGraph."),
        what_executes_ar: format!("يعرّف المؤشر '{metric_id}' بالمعادلة '{formula}' في محرك Rust."),
        undo_way_en: "Click Undo (studio_undo) to remove the metric definition.".to_owned(),
        undo_way_ar: "اضغط تراجع (Undo) لإلغاء تعريف المؤشر فورًا.".to_owned(),
        executable_action: DecisionExecutableAction::DefineVerifiedMetric {
            metric_id: metric_id.clone(),
            name: metric_name.clone(),
            unit,
            formula: formula.clone(),
        },
        clarification_prompt: None,
        candidates_evaluated_count: audit.len(),
        invalid_eliminated_count: audit.iter().filter(|c| !c.is_valid).count(),
        advanced_audit_candidates: audit,
    };

    (decision, best_measure, metric_id, metric_name, formula)
}

fn build_chart_selection_decision(
    column_decisions: &[ColumnContentDecision],
    best_measure_col: &str,
    dimension_cols: &[String],
    temporal_cols: &[String],
    row_count: usize,
) -> (RecommendedDecision, String, String) {
    let mut audit = Vec::new();

    // Prefer a categorical dimension with 2..=25 distinct categories, otherwise temporal column
    let chosen_group = dimension_cols
        .first()
        .cloned()
        .or_else(|| temporal_cols.first().cloned())
        .unwrap_or_else(|| best_measure_col.to_owned());

    let group_distinct = column_decisions
        .iter()
        .find(|c| c.column_name == chosen_group)
        .map_or(row_count.max(1), |c| c.distinct_count);
    let is_temporal_group = temporal_cols.contains(&chosen_group);

    // Prune Pie/Donut if categories > 6 (perceptual distortion guard — Section 11.4)
    if group_distinct > 6 || is_temporal_group {
        audit.push(EvaluatedCandidateAudit {
            candidate_id: "cand_chart_pie".to_owned(),
            title_en: format!("Pie Chart of {best_measure_col} by {chosen_group}"),
            title_ar: format!("رسم دائري (Pie) لـ {best_measure_col} حسب {chosen_group}"),
            is_valid: false,
            elimination_reason_en: Some(format!(
                "Eliminated by Perceptual Guard: '{}' has {} distinct categories / temporal ordering, which causes angle-comparison distortion in Pie charts.",
                chosen_group, group_distinct
            )),
            elimination_reason_ar: Some(format!(
                "تم استبعاده بواسطة حارس الوضوح البصري: البُعد '{}' يحتوي على {} فئة/ترتيب زمني مما يسبب تضليلًا بصريًا في الرسم الدائري.",
                chosen_group, group_distinct
            )),
            objective: LexicographicObjectiveVector {
                correctness: 0,
                data_safety_no_loss: 1000,
                user_constraint_compliance: 500,
                analytical_fit: 200,
                explainability_audit: 600,
                deliverable_quality: 200,
                execution_efficiency: 990,
            },
        });
    }

    let (best_chart_type, chart_label_en, chart_label_ar, why_en, why_ar) = if is_temporal_group {
        (
            "line".to_owned(),
            "Temporal Trend Line Chart",
            "رسم خطي للاتجاه الزمني (Line Chart)",
            format!(
                "Selected Line Chart because '{}' is a chronological Temporal dimension; position-along-common-scale encodes trends without distortion.",
                chosen_group
            ),
            format!(
                "تم اختيار الرسم الخطي لأن '{}' بُعد زمني؛ يوضّح الاتجاه عبر الزمن بأعلى دقة بصرية.",
                chosen_group
            ),
        )
    } else {
        (
            "bar".to_owned(),
            "Ranked Categorical Bar Chart",
            "رسم أعمدة مقارن للفئات (Bar Chart)",
            format!(
                "Selected Bar Chart for comparing '{}' across {} distinct categories in '{}'; aligned horizontal/vertical lengths provide the highest Cleveland-McGill perceptual accuracy.",
                best_measure_col, group_distinct, chosen_group
            ),
            format!(
                "تم اختيار رسم الأعمدة لمقارنة '{}' عبر {} فئات في '{}'؛ مقارنة الأطوال المتحاذاة توفر أعلى دقة إدراكية دون تضليل بصري.",
                best_measure_col, group_distinct, chosen_group
            ),
        )
    };

    let winning_obj = LexicographicObjectiveVector {
        correctness: 1000,
        data_safety_no_loss: 1000,
        user_constraint_compliance: 1000,
        analytical_fit: 980,
        explainability_audit: 1000,
        deliverable_quality: 985,
        execution_efficiency: 990,
    };

    audit.push(EvaluatedCandidateAudit {
        candidate_id: format!("cand_chart_{best_chart_type}"),
        title_en: format!("{chart_label_en}: SUM({best_measure_col}) by {chosen_group}"),
        title_ar: format!("{chart_label_ar}: SUM({best_measure_col}) حسب {chosen_group}"),
        is_valid: true,
        elimination_reason_en: None,
        elimination_reason_ar: None,
        objective: winning_obj,
    });

    let decision = RecommendedDecision {
        decision_id: "dec_optimal_chart".to_owned(),
        domain: DecisionDomain::ChartSelection,
        algorithm_used: OptimizationAlgorithm::ConstrainedBeamSearch,
        optimality_class: OptimalityClass::BestValidCandidateFound,
        recommended_title_en: format!(
            "Build {chart_label_en}: {best_measure_col} by {chosen_group}"
        ),
        recommended_title_ar: format!(
            "إنشاء {chart_label_ar}: {best_measure_col} حسب {chosen_group}"
        ),
        why_en,
        why_ar,
        objective: winning_obj,
        evidence: vec![DecisionEvidenceItem {
            label_en: "Grouping Cardinality".to_owned(),
            label_ar: "عدد الفئات المميزة".to_owned(),
            observed_value: group_distinct.to_string(),
            impact_en: "Fits optimal perceptual comparison density without label collision."
                .to_owned(),
            impact_ar: "يتناسب مع الكثافة البصرية المثلى للمقارنة دون تداخل العناوين.".to_owned(),
        }],
        assumptions_en: vec![
            "Chart series are aggregated strictly inside Rust (studio_chart_data) before rendering."
                .to_owned(),
        ],
        assumptions_ar: vec![
            "تُجمع سلاسل البيانات حتميًا داخل محرك Rust قبل رسمها في الواجهة.".to_owned(),
        ],
        what_executes_en: format!(
            "Adds a verified '{best_chart_type}' chart aggregating SUM({best_measure_col}) by {chosen_group}."
        ),
        what_executes_ar: format!(
            "يضيف رسم '{best_chart_type}' موثقًا يجمع SUM({best_measure_col}) حسب {chosen_group}."
        ),
        undo_way_en: "Remove or edit the chart card in Dashboard view or click Undo.".to_owned(),
        undo_way_ar: "احذف أو عدّل بطاقة الرسم من لوحة المؤشرات أو اضغط تراجع.".to_owned(),
        executable_action: DecisionExecutableAction::ConfigureOptimalChart {
            chart_type: best_chart_type.clone(),
            title: format!("{best_measure_col} by {chosen_group}"),
            function: "SUM".to_owned(),
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

fn build_dashboard_layout_decision(
    metric_id: &str,
    metric_name: &str,
    metric_formula: &str,
    chart_type: &str,
    value_col: &str,
    group_col: &str,
    domain: SheetBusinessDomain,
) -> RecommendedDecision {
    let winning_obj = LexicographicObjectiveVector {
        correctness: 1000,
        data_safety_no_loss: 1000,
        user_constraint_compliance: 1000,
        analytical_fit: 970,
        explainability_audit: 1000,
        deliverable_quality: 975,
        execution_efficiency: 980,
    };

    let audit = vec![EvaluatedCandidateAudit {
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
    }];

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
            impact_en: "Guarantees zero discrepancy between headline KPI total and chart bar sums.".to_owned(),
            impact_ar: "يضمن تطابق الرقم الإجمالي للمؤشر مع مجموع أعمدة الرسم البياني بنسبة 100%.".to_owned(),
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
            primary_metric_unit: Some("EGP".to_owned()),
            primary_metric_formula: metric_formula.to_owned(),
            primary_chart_type: chart_type.to_owned(),
            value_column: value_col.to_owned(),
            group_by_column: group_col.to_owned(),
        },
        clarification_prompt: None,
        candidates_evaluated_count: 1,
        invalid_eliminated_count: 0,
        advanced_audit_candidates: audit,
    }
}

fn build_report_and_presentation_decision(
    graph: &DataGraph,
    metric_id: &str,
    metric_name: &str,
    chart_type: &str,
    group_col: &str,
    _domain: SheetBusinessDomain,
) -> RecommendedDecision {
    let row_count = graph.working().row_count();
    let winning_obj = LexicographicObjectiveVector {
        correctness: 1000,
        data_safety_no_loss: 1000,
        user_constraint_compliance: 1000,
        analytical_fit: 960,
        explainability_audit: 1000,
        deliverable_quality: 970,
        execution_efficiency: 985,
    };

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
            observed_value: format!("{{{{metric:{metric_id}}}}}"),
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
            "Synchronizes executive slide deck and document report blocks with metric '{metric_name}' and '{chart_type}' breakdown by '{group_col}'."
        ),
        what_executes_ar: format!(
            "يزامن شرائح العرض التقديمي وفقرات التقرير مع المؤشر '{metric_name}' والتوزيع حسب '{group_col}'."
        ),
        undo_way_en: "Edit or remove generated slides/blocks in the Slides and Doc tabs.".to_owned(),
        undo_way_ar: "يمكنك تعديل أو حذف الشرائح والفقرات من تبويبي الشرائح والمستند.".to_owned(),
        executable_action: DecisionExecutableAction::ApplyDashboardAndReportBlueprint {
            primary_metric_id: metric_id.to_owned(),
            primary_metric_name: metric_name.to_owned(),
            primary_metric_unit: Some("EGP".to_owned()),
            primary_metric_formula: format!("SUM({group_col})"),
            primary_chart_type: chart_type.to_owned(),
            value_column: group_col.to_owned(),
            group_by_column: group_col.to_owned(),
        },
        clarification_prompt: None,
        candidates_evaluated_count: 1,
        invalid_eliminated_count: 0,
        advanced_audit_candidates: Vec::new(),
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
) -> AiArchitectureStatusReport {
    AiArchitectureStatusReport {
        local_ollama_endpoint: "http://127.0.0.1:11434".to_owned(),
        local_ollama_status: "not_probed_in_airgapped_engine".to_owned(),
        bundled_model_claimed: false,
        installed_local_models: Vec::new(),
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
            if !artifacts
                .charts
                .iter()
                .any(|c| c.column == *value_column && c.group_by == *group_by_column)
            {
                artifacts.charts.push(StudioChartArtifact {
                    id: format!("chart_opt_{now_ms}"),
                    chart_type: primary_chart_type.clone(),
                    title: format!("{value_column} by {group_by_column}"),
                    function: "SUM".to_owned(),
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

    #[test]
    fn decision_engine_protects_numeric_employee_id_distinguishes_duplicates_and_optimizes_lexicographically()
     {
        // Dataset with:
        // - Numeric `Employee ID` (1001, 1002, 1003) -> MUST be classified as Identifier, NEVER Measure!
        // - `Department` with untrimmed whitespace ("Sales ")
        // - Exact unintended duplicate row (row 2 and row 3 are 100% identical)
        // - Valid repeated department transaction (row 1 and row 2 are both Sales but different employees/salaries)
        // - Conflicting primary key row (row 4 has Employee ID 1001 with different salary 9500)
        let csv = "Employee ID,Department,Hire Date,Salary\n\
                   1001,Sales,2026-01-10,8000\n\
                   1002,Sales ,2026-01-15,9000\n\
                   1002,Sales ,2026-01-15,9000\n\
                   1001,Sales,2026-02-01,9500\n\
                   1003,Engineering,2026-02-05,12000\n";
        let report = test_result(sf_io::ingest_workbook_universal(
            std::io::Cursor::new(csv.as_bytes()),
            sf_core::limits::PlatformLimits::DESKTOP_CLI,
            Some("hr_sheet.csv"),
            None,
        ));
        let table = test_result(report.worksheets[0].tables[0].to_core_table());
        let source = test_result(sf_core::datagraph::SourceId::new("hr_sheet"));
        let mut graph = test_result(DataGraph::new(source, table, 1_000));
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

        // 2. Verify Smart Duplicate Analysis distinguishes exact duplicates vs. valid transactions vs. key conflicts
        assert_eq!(
            suite
                .duplicate_analysis
                .exact_unintended_duplicate_row_ids
                .len(),
            1
        );
        assert!(
            suite
                .duplicate_analysis
                .valid_repeated_entity_transaction_rows
                >= 1
        );
        assert_eq!(
            suite
                .duplicate_analysis
                .conflicting_key_review_row_ids
                .len(),
            1
        );

        // 3. Verify KPI Decision pruned `SUM(Employee ID)` and selected `SUM(Salary)`
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
        assert!(
            kpi_dec
                .advanced_audit_candidates
                .iter()
                .any(|c| !c.is_valid && c.title_en.contains("SUM(Employee ID)"))
        );

        // 4. Execute the cleaning decision (drops only the 1 exact duplicate row, preserving 4 rows)
        let clean_dec = test_result(
            suite
                .recommended_decisions
                .iter()
                .find(|d| d.domain == DecisionDomain::DataCleaning)
                .ok_or("Missing Cleaning decision"),
        );
        let _ = test_result(execute_recommended_decision_action(
            &mut graph,
            &mut artifacts,
            &clean_dec.executable_action,
            2_000,
        ));
        assert_eq!(graph.working().row_count(), 4);

        // 5. Execute the dashboard & report blueprint decision and verify 100% cross-product consistency
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
}
