/**
 * `apps/web/src/desktop/components/DesktopWorkflowMemoryPanel.tsx`
 *
 * Real Desktop UI for:
 * - Section 5: Persistent Per-Sheet Workflow Memory (filename-independent semantic matching,
 *   automatic replay on High confidence, partial reuse & targeted questions on Medium confidence,
 *   per-sheet isolation across multi-sheet workbooks, version history & rollback/restore,
 *   duplicate, rename, delete).
 * - Section 6: Continuous Local Learning & Explainable Personal Profile (record explicit
 *   corrections, accepted/rejected suggestions, preferred rules scoped to this sheet or all
 *   matching sheets, provenance explanations, undo/delete individual learned rules, or reset profile).
 * - Works in Offline Read-Only Mode for inspecting saved workflow memory and learned preferences
 *   while blocking protected mutations (`SF-AUTH-021`).
 */
import type { FormEvent, ReactNode } from "react";
import { useCallback, useEffect, useState } from "react";
import {
  BrainCircuit,
  CheckCircle2,
  CopyPlus,
  History,
  Play,
  RotateCcw,
  Save,
  Share2,
  Sparkles,
  Trash2
} from "lucide-react";
import type { SafiLang } from "../../components/layout/FloatingGlassDock";
import {
  applyDesktopWorkflowMemoryReplay,
  deleteDesktopLearningRule,
  listDesktopWorkflowMemories,
  manageDesktopWorkflowMemory,
  matchDesktopSheetWorkflowMemory,
  recordDesktopLearningFeedback,
  resetDesktopLearningProfile,
  saveDesktopSheetWorkflowMemory,
  type DesktopStudioState,
  type LearnedPreferenceRule,
  type PersonalLearningProfile,
  type SavedQuestionDecision,
  type SheetWorkflowMatchReport,
  type SheetWorkflowMemoryRecord,
  type StudioQuestionResolutionArtifact
} from "../studio-bridge";

export interface DesktopWorkflowMemoryPanelProps {
  readonly lang?: SafiLang;
  readonly projectId: string;
  readonly activeSheetName: string;
  readonly activeTableId: string;
  readonly state: DesktopStudioState | null;
  readonly questionResolutions: readonly StudioQuestionResolutionArtifact[];
  readonly readOnlyOffline?: boolean;
  readonly onStateUpdated: (nextState: DesktopStudioState) => void;
  readonly onReusedQuestionsApplied?: (
    reused: readonly SavedQuestionDecision[],
    suppressedTopics: readonly string[]
  ) => void;
  readonly onShareWorkflowToTeam?: (workflow: SheetWorkflowMemoryRecord) => void;
}

function inferTopicAndConceptFromQuestionId(
  questionId: string,
  headers: readonly string[]
): { readonly topic: string; readonly concept: string } {
  const lower = questionId.toLowerCase();
  if (lower.includes("date")) {
    return { topic: "date_granularity", concept: "temporal_date" };
  }
  if (lower.includes("currency") || lower.includes("unit")) {
    return { topic: "currency_unit", concept: "sales_currency_amount" };
  }
  if (lower.includes("id") || lower.includes("key")) {
    return { topic: "id_handling", concept: "record_identifier" };
  }
  if (lower.includes("kpi") || lower.includes("metric")) {
    return { topic: "primary_kpi", concept: "sales_currency_amount" };
  }
  const firstCol = headers[0] ?? "sheet_level";
  return { topic: `question_${questionId}`, concept: firstCol };
}

export function DesktopWorkflowMemoryPanel({
  lang = "en",
  projectId,
  activeSheetName,
  activeTableId,
  state,
  questionResolutions,
  readOnlyOffline = false,
  onStateUpdated,
  onReusedQuestionsApplied,
  onShareWorkflowToTeam
}: DesktopWorkflowMemoryPanelProps): ReactNode {
  const L = useCallback((ar: string, en: string): string => (lang === "ar" ? ar : en), [lang]);
  const [workflows, setWorkflows] = useState<readonly SheetWorkflowMemoryRecord[]>([]);
  const [learningProfile, setLearningProfile] = useState<PersonalLearningProfile | null>(null);
  const [matchReport, setMatchReport] = useState<SheetWorkflowMatchReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  // Save current sheet workflow form state
  const [workflowName, setWorkflowName] = useState("");
  const [duplicatePolicy, setDuplicatePolicy] = useState("exact_key");
  const [missingValuePolicy, setMissingValuePolicy] = useState("fill_zero");
  const [businessRuleText, setBusinessRuleText] = useState("");
  const [observedDateFormat, setObservedDateFormat] = useState("YYYY-MM-DD");

  // Continuous learning feedback form state
  const [learnSignalKind, setLearnSignalKind] =
    useState<LearnedPreferenceRule["signalKind"]>("explicit_correction");
  const [learnCategory, setLearnCategory] =
    useState<LearnedPreferenceRule["category"]>("column_classification");
  const [learnScope, setLearnScope] = useState<LearnedPreferenceRule["scope"]>("matching_sheets");
  const [learnTargetConcept, setLearnTargetConcept] = useState("order_id");
  const [learnPreferredValue, setLearnPreferredValue] = useState("Identifier (Never Sum)");
  const [learnRejectedValue, setLearnRejectedValue] = useState("Measure (SUM)");
  const [learnExplanation, setLearnExplanation] = useState("");

  const activeHeaders = state ? state.columns.map((c) => c.name) : [];

  const refreshMemoryAndMatch = useCallback(async () => {
    try {
      const data = await listDesktopWorkflowMemories();
      setWorkflows(data.workflows);
      setLearningProfile(data.learningProfile);

      if (state && state.columns.length > 0) {
        const headers = state.columns.map((c) => c.name);
        const matchRes = await matchDesktopSheetWorkflowMemory({
          projectId,
          sheetName: activeSheetName,
          headers,
          observedDateFormat
        });
        setMatchReport(matchRes.report);
        if (matchRes.report.reusedQuestionResolutions.length > 0) {
          onReusedQuestionsApplied?.(
            matchRes.report.reusedQuestionResolutions,
            matchRes.report.suppressedQuestionTopics
          );
        }
      } else {
        setMatchReport(null);
      }
    } catch {
      // Ignore before session is bound.
    }
  }, [projectId, state, activeSheetName, observedDateFormat, onReusedQuestionsApplied]);

  useEffect(() => {
    void refreshMemoryAndMatch();
  }, [refreshMemoryAndMatch]);

  const handleSaveCurrentSheetWorkflow = async (event: FormEvent): Promise<void> => {
    event.preventDefault();
    if (!state || activeHeaders.length === 0) {
      setError(
        L(
          "افتح ورقة بيانات أولًا لحفظ ذاكرة مسار العمل.",
          "Open a worksheet first to save workflow memory."
        )
      );
      return;
    }
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const savedQuestions: SavedQuestionDecision[] = questionResolutions.map((qr) => {
        const { topic, concept } = inferTopicAndConceptFromQuestionId(qr.questionId, activeHeaders);
        return {
          questionId: qr.questionId,
          semanticTopic: topic,
          canonicalConcept: concept,
          action: qr.action,
          answer: qr.answer,
          customText: qr.customText ?? "",
          updatedAtMs: qr.updatedAtMs
        };
      });

      const savedMetrics = state.metrics.map((m) => ({
        id: m.id,
        name: m.name,
        unit: m.unit,
        formula: m.formula
      }));

      const savedOps = activeHeaders.slice(0, 2).map((col, idx) => ({
        stepId: `wf_trim_${idx + 1}`,
        kind: "trim_column",
        targetColumn: col,
        targetConcept: null,
        parameter: null
      }));

      const res = await saveDesktopSheetWorkflowMemory({
        workflowId: matchReport?.matchedWorkflowId ?? null,
        displayName: workflowName.trim() || `${activeSheetName} Workflow`,
        projectId,
        sheetName: activeSheetName,
        tableId: activeTableId,
        headers: activeHeaders,
        questionResolutions: savedQuestions,
        columnDecisions: activeHeaders.map((h) => ({
          headerName: h,
          canonicalConcept: "",
          confirmedRole: "",
          confirmedSemanticType: "",
          dateFormat: observedDateFormat,
          currencyOrUnit: null,
          isIdentifier: h.toLowerCase().includes("id") || h.includes("كود") || h.includes("رقم")
        })),
        duplicatePolicy,
        missingValuePolicy,
        businessRules: businessRuleText.trim() ? [businessRuleText.trim()] : [],
        cleaningOperations: savedOps,
        metrics: savedMetrics
      });
      setWorkflows(res.workflows);
      setNotice(
        L(
          `تم حفظ ذاكرة مسار العمل للورقة «${res.saved.displayName}» (الإصدار v${res.saved.workflowVersion} • المجال: ${res.saved.businessDomain}).`,
          `Saved per-sheet workflow memory "${res.saved.displayName}" (v${res.saved.workflowVersion} • domain: ${res.saved.businessDomain}).`
        )
      );
      await refreshMemoryAndMatch();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleApplyWorkflowReplay = async (workflowId: string): Promise<void> => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const nextState = await applyDesktopWorkflowMemoryReplay(projectId, workflowId);
      onStateUpdated(nextState);
      setNotice(
        L(
          "تم تطبيق مسار العمل المحفوظ تلقائيًا مع إعادة استخدام إجابات الأسئلة السابقة دون تكرارها.",
          "Replayed saved sheet workflow deterministically and reused prior question answers without repeating questions."
        )
      );
      await refreshMemoryAndMatch();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleManageWorkflow = async (
    action: "restore_version" | "duplicate" | "rename" | "delete",
    workflowId: string,
    targetVersion?: number,
    newName?: string
  ): Promise<void> => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const res = await manageDesktopWorkflowMemory({
        action,
        workflowId,
        ...(targetVersion !== undefined ? { targetVersion } : {}),
        ...(newName !== undefined ? { newName } : {})
      });
      setWorkflows(res.workflows);
      setNotice(
        L(
          `تم تنفيذ الإجراء (${action}) على مسار العمل بنجاح.`,
          `Completed workflow action: ${action}.`
        )
      );
      await refreshMemoryAndMatch();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleRecordLearning = async (event: FormEvent): Promise<void> => {
    event.preventDefault();
    if (!learnTargetConcept.trim() || !learnPreferredValue.trim()) {
      setError(
        L(
          "أدخل العمود/المفهوم والقيمة المفضلة.",
          "Enter target column/concept and preferred value."
        )
      );
      return;
    }
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const res = await recordDesktopLearningFeedback({
        signalKind: learnSignalKind,
        category: learnCategory,
        scope: learnScope,
        businessDomain: matchReport?.detectedDomain ?? "sales",
        targetConcept: learnTargetConcept.trim(),
        preferredValue: learnPreferredValue.trim(),
        rejectedValue: learnRejectedValue.trim() || null,
        userExplanation: learnExplanation.trim(),
        sourceSheetLabel: activeSheetName
      });
      setLearningProfile(res.learningProfile);
      setNotice(
        L(
          `تم تسجيل قاعدة التعلّم المحلي (${res.rule.ruleId}) بثقة ${res.rule.confidencePct}%.`,
          `Recorded local learning rule (${res.rule.ruleId}) at ${res.rule.confidencePct}% confidence.`
        )
      );
      await refreshMemoryAndMatch();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleDeleteRule = async (ruleId: string): Promise<void> => {
    setBusy(true);
    setError(null);
    try {
      const next = await deleteDesktopLearningRule(ruleId);
      setLearningProfile(next);
      setNotice(L("تم التراجع عن قاعدة التعلّم وحذفها.", "Removed learned preference rule."));
      await refreshMemoryAndMatch();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleResetLearningProfile = async (): Promise<void> => {
    setBusy(true);
    setError(null);
    try {
      const next = await resetDesktopLearningProfile();
      setLearningProfile(next);
      setNotice(L("تمت إعادة ضبط ملف التعلّم الشخصي بالكامل.", "Reset personal learning profile."));
      await refreshMemoryAndMatch();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section
      className="safi-panel"
      data-desktop-workflow-memory="true"
      style={{ display: "grid", gap: "18px", padding: "18px" }}
    >
      <div
        style={{
          display: "flex",
          flexWrap: "wrap",
          justifyContent: "space-between",
          alignItems: "center",
          gap: "12px"
        }}
      >
        <div>
          <h2 style={{ margin: 0, display: "flex", alignItems: "center", gap: "8px" }}>
            <BrainCircuit size={18} aria-hidden="true" />
            <span>
              {L(
                "ذاكرة مسار العمل لكل ورقة والتعلّم الشخصي المستمر",
                "Per-Sheet Workflow Memory & Continuous Local Learning"
              )}
            </span>
          </h2>
          <p style={{ margin: "4px 0 0", fontSize: "13px", opacity: 0.84 }}>
            {L(
              "يتذكّر صافي قراراتك وإجاباتك لكل ورقة بمعزل عن غيرها (حتى عند تغيير اسم الملف أو ترتيب الأعمدة)، ويعيد تطبيق مسار العمل دون تكرار نفس الأسئلة. جميع البيانات مشفّرة محليًا على جهازك.",
              "SafiData remembers your approved workflow per worksheet independently (even when filenames, sheet names, or column orders change) and replays decisions without repeating answered questions. 100% encrypted locally."
            )}
          </p>
        </div>
      </div>

      {error !== null ? (
        <div role="alert" data-workflow-memory-error="true" style={{ fontSize: "13px" }}>
          {error}
        </div>
      ) : null}

      {notice !== null ? (
        <div
          role="status"
          data-workflow-memory-notice="true"
          style={{ fontSize: "13px", fontWeight: 600 }}
        >
          {notice}
        </div>
      ) : null}

      {/* Active Sheet Semantic Recognition & Automatic Replay Banner */}
      {matchReport !== null ? (
        <div
          data-workflow-match-banner="true"
          data-match-tier={matchReport.matchTier}
          data-match-domain={matchReport.detectedDomain}
          data-match-confidence={matchReport.confidencePct}
          style={{
            padding: "14px",
            borderRadius: "10px",
            border: "1px solid var(--border-strong, rgba(59,130,246,0.4))",
            display: "grid",
            gap: "8px"
          }}
        >
          <div
            style={{
              display: "flex",
              flexWrap: "wrap",
              justifyContent: "space-between",
              alignItems: "center",
              gap: "8px"
            }}
          >
            <strong>
              {L("تحليل التطابق الدلالي للورقة الحالية:", "Active Worksheet Semantic Match:")}{" "}
              <code>{activeSheetName}</code> ({matchReport.detectedDomain.toUpperCase()} •{" "}
              {matchReport.confidencePct}% • {matchReport.matchTier.toUpperCase()})
            </strong>
            {matchReport.matchedWorkflowId !== null ? (
              <button
                type="button"
                className="safi-btn safi-btn-primary"
                disabled={readOnlyOffline || busy}
                data-workflow-replay-btn={matchReport.matchedWorkflowId}
                onClick={() =>
                  matchReport.matchedWorkflowId &&
                  void handleApplyWorkflowReplay(matchReport.matchedWorkflowId)
                }
              >
                <Play size={14} aria-hidden="true" />
                <span>
                  {matchReport.autoReplayEligible
                    ? L("تطبيق مسار العمل المتطابق تلقائيًا", "Auto-Replay Matched Workflow")
                    : L("تطبيق القرارات المتوافقة ومراجعة الجديد", "Apply Compatible Decisions")}
                </span>
              </button>
            ) : null}
          </div>

          <p style={{ margin: 0, fontSize: "13px" }}>
            {lang === "ar" ? matchReport.explanationAr : matchReport.explanationEn}
          </p>

          {matchReport.suppressedQuestionTopics.length > 0 ? (
            <div
              data-suppressed-questions="true"
              style={{
                fontSize: "12px",
                opacity: 0.9,
                display: "flex",
                alignItems: "center",
                gap: "6px"
              }}
            >
              <CheckCircle2 size={14} aria-hidden="true" />
              <span>
                {L(
                  `أسئلة تم تخطي تكرارها تلقائيًا بثقة عالية (${matchReport.suppressedQuestionTopics.length}): ${matchReport.suppressedQuestionTopics.join("، ")}`,
                  `Previously answered questions suppressed automatically (${matchReport.suppressedQuestionTopics.length}): ${matchReport.suppressedQuestionTopics.join(", ")}`
                )}
              </span>
            </div>
          ) : null}

          {matchReport.addedColumns.length > 0 || matchReport.schemaChangeNotices.length > 0 ? (
            <div data-schema-change-notices="true" style={{ fontSize: "12px" }}>
              <strong>
                {L(
                  "تغييرات هيكلية مكتشفة تحتاج تأكيدًا فقط:",
                  "Detected structural changes requiring targeted confirmation:"
                )}
              </strong>{" "}
              {matchReport.schemaChangeNotices.join(" | ")}
            </div>
          ) : null}

          {matchReport.learnedSuggestions.length > 0 ? (
            <div data-learned-suggestions-active="true" style={{ display: "grid", gap: "6px" }}>
              <strong style={{ fontSize: "12px" }}>
                {L(
                  "توصيات مستخلصة من ملف تعلّمك الشخصي:",
                  "Explainable Suggestions from Your Personal Learning Profile:"
                )}
              </strong>
              {matchReport.learnedSuggestions.map((sug) => (
                <div
                  key={sug.influencingRuleId}
                  data-learned-suggestion-rule={sug.influencingRuleId}
                  style={{
                    fontSize: "12px",
                    padding: "8px 10px",
                    borderRadius: "8px",
                    background: "rgba(59,130,246,0.08)"
                  }}
                >
                  <div>
                    <strong>{sug.targetHeaderOrConcept}</strong> → <code>{sug.suggestedValue}</code>{" "}
                    ({sug.confidencePct}%)
                  </div>
                  <div style={{ opacity: 0.85 }}>{lang === "ar" ? sug.whyAr : sug.whyEn}</div>
                </div>
              ))}
            </div>
          ) : null}
        </div>
      ) : null}

      {/* Save Current Sheet Workflow Memory Form */}
      <form
        onSubmit={(e) => void handleSaveCurrentSheetWorkflow(e)}
        data-save-sheet-workflow-form="true"
        style={{
          padding: "14px",
          borderRadius: "10px",
          border: "1px solid var(--border-subtle, rgba(148,163,184,0.28))",
          display: "grid",
          gap: "10px"
        }}
      >
        <h3
          style={{ margin: 0, fontSize: "15px", display: "flex", alignItems: "center", gap: "6px" }}
        >
          <Save size={15} aria-hidden="true" />
          <span>
            {L(
              `حفظ ذاكرة مسار العمل للورقة الحالية (${activeSheetName})`,
              `Save Workflow Memory for Active Sheet (${activeSheetName})`
            )}
          </span>
        </h3>
        <div
          style={{
            display: "grid",
            gap: "8px",
            gridTemplateColumns: "repeat(auto-fit, minmax(190px, 1fr))"
          }}
        >
          <input
            type="text"
            value={workflowName}
            onChange={(e) => setWorkflowName(e.target.value)}
            placeholder={L("اسم مسار عمل الورقة...", "Sheet workflow name...")}
            className="safi-input"
            data-workflow-name-input="true"
            disabled={readOnlyOffline || busy}
          />
          <select
            value={duplicatePolicy}
            onChange={(e) => setDuplicatePolicy(e.target.value)}
            className="safi-input"
            aria-label={L("سياسة التكرار", "Duplicate policy")}
            data-workflow-dup-policy="true"
            disabled={readOnlyOffline || busy}
          >
            <option value="exact_key">{L("دمج بالمفتاح الدقيق", "Exact key deduplication")}</option>
            <option value="fuzzy_merge">{L("دمج ذكي متشابه", "Fuzzy record merge")}</option>
            <option value="keep_all">{L("الإبقاء مع الوسم", "Keep all & flag")}</option>
          </select>
          <select
            value={missingValuePolicy}
            onChange={(e) => setMissingValuePolicy(e.target.value)}
            className="safi-input"
            aria-label={L("سياسة القيم المفقودة", "Missing value policy")}
            data-workflow-missing-policy="true"
            disabled={readOnlyOffline || busy}
          >
            <option value="fill_zero">
              {L("تعويض الأرقام بصفر", "Fill numeric missing with 0")}
            </option>
            <option value="fill_median">{L("تعويض بالوسيط", "Fill with median")}</option>
            <option value="flag_only">{L("وسم فقط دون تعديل", "Flag only")}</option>
          </select>
          <select
            value={observedDateFormat}
            onChange={(e) => setObservedDateFormat(e.target.value)}
            className="safi-input"
            aria-label={L("صيغة التاريخ المعتمدة", "Approved date format")}
            data-workflow-date-format="true"
            disabled={readOnlyOffline || busy}
          >
            <option value="YYYY-MM-DD">ISO (YYYY-MM-DD)</option>
            <option value="DD/MM/YYYY">Day-First (DD/MM/YYYY)</option>
            <option value="MM/DD/YYYY">Month-First (MM/DD/YYYY)</option>
          </select>
          <input
            type="text"
            value={businessRuleText}
            onChange={(e) => setBusinessRuleText(e.target.value)}
            placeholder={L("قاعدة عمل مخصصة (اختياري)...", "Custom business rule (optional)...")}
            className="safi-input"
            data-workflow-business-rule="true"
            disabled={readOnlyOffline || busy}
          />
        </div>
        <div>
          <button
            type="submit"
            className="safi-btn safi-btn-primary"
            disabled={readOnlyOffline || busy || state === null}
            data-save-workflow-memory-btn="true"
          >
            <Save size={14} aria-hidden="true" />
            <span>{L("حفظ مسار العمل للورقة", "Save Sheet Workflow Memory")}</span>
          </button>
        </div>
      </form>

      {/* Saved Per-Sheet Workflows List (with Version History, Restore, Duplicate, Delete) */}
      <div style={{ display: "grid", gap: "10px" }}>
        <h3
          style={{ margin: 0, fontSize: "15px", display: "flex", alignItems: "center", gap: "6px" }}
        >
          <History size={15} aria-hidden="true" />
          <span>
            {L("مسارات العمل المحفوظة لكل ورقة", "Saved Per-Sheet Workflow Memories")} (
            {workflows.length})
          </span>
        </h3>
        <div data-saved-workflows-list="true" style={{ display: "grid", gap: "10px" }}>
          {workflows.map((wf) => (
            <article
              key={wf.workflowId}
              data-saved-workflow-id={wf.workflowId}
              data-saved-workflow-domain={wf.businessDomain}
              style={{
                padding: "12px",
                borderRadius: "10px",
                border: "1px solid var(--border-subtle, rgba(148,163,184,0.28))",
                display: "flex",
                flexWrap: "wrap",
                justifyContent: "space-between",
                alignItems: "center",
                gap: "10px"
              }}
            >
              <div>
                <div style={{ fontWeight: 700, fontSize: "14px" }}>
                  {wf.displayName} <small>(v{wf.workflowVersion})</small>
                </div>
                <div style={{ fontSize: "12px", opacity: 0.8 }}>
                  {L("المجال:", "Domain:")} <strong>{wf.businessDomain}</strong> •{" "}
                  {L("آخر ورقة:", "Last sheet:")} {wf.lastSheetName} •{" "}
                  {L("الأسئلة المحفوظة:", "Questions:")} {wf.questionResolutions.length} •{" "}
                  {L("العمليات:", "Ops:")} {wf.cleaningOperations.length} •{" "}
                  {L("المؤشرات:", "Metrics:")} {wf.metrics.length} • {L("الإصدارات:", "Versions:")}{" "}
                  {wf.versions.length}
                </div>
              </div>
              <div style={{ display: "flex", flexWrap: "wrap", gap: "6px" }}>
                {state !== null ? (
                  <button
                    type="button"
                    className="safi-btn safi-btn-primary"
                    disabled={readOnlyOffline || busy}
                    data-replay-saved-workflow-btn={wf.workflowId}
                    onClick={() => void handleApplyWorkflowReplay(wf.workflowId)}
                  >
                    <Play size={13} aria-hidden="true" />
                    <span>{L("تطبيق", "Replay")}</span>
                  </button>
                ) : null}
                {wf.versions.length > 1 ? (
                  <button
                    type="button"
                    className="safi-btn safi-btn-ghost"
                    disabled={readOnlyOffline || busy}
                    data-restore-v1-workflow-btn={wf.workflowId}
                    onClick={() => void handleManageWorkflow("restore_version", wf.workflowId, 1)}
                  >
                    <RotateCcw size={13} aria-hidden="true" />
                    <span>{L("استعادة v1", "Restore v1")}</span>
                  </button>
                ) : null}
                <button
                  type="button"
                  className="safi-btn safi-btn-ghost"
                  disabled={readOnlyOffline || busy}
                  data-duplicate-workflow-btn={wf.workflowId}
                  onClick={() => void handleManageWorkflow("duplicate", wf.workflowId)}
                >
                  <CopyPlus size={13} aria-hidden="true" />
                  <span>{L("نسخ", "Duplicate")}</span>
                </button>
                {onShareWorkflowToTeam ? (
                  <button
                    type="button"
                    className="safi-btn safi-btn-primary"
                    disabled={readOnlyOffline || busy}
                    data-testid={`workflow-share-to-team-${wf.workflowId}`}
                    onClick={() => onShareWorkflowToTeam(wf)}
                  >
                    <Share2 size={13} aria-hidden="true" />
                    <span>{L("مشاركة مع الفريق", "Share to Team")}</span>
                  </button>
                ) : null}
                <button
                  type="button"
                  className="safi-btn safi-btn-ghost"
                  disabled={readOnlyOffline || busy}
                  data-delete-workflow-btn={wf.workflowId}
                  onClick={() => void handleManageWorkflow("delete", wf.workflowId)}
                >
                  <Trash2 size={13} aria-hidden="true" />
                </button>
              </div>
            </article>
          ))}
        </div>
      </div>

      {/* Continuous Local Learning & Personal Profile Editor */}
      <form
        onSubmit={(e) => void handleRecordLearning(e)}
        data-continuous-learning-form="true"
        style={{
          padding: "14px",
          borderRadius: "10px",
          border: "1px solid var(--border-subtle, rgba(148,163,184,0.28))",
          display: "grid",
          gap: "10px"
        }}
      >
        <div
          style={{
            display: "flex",
            flexWrap: "wrap",
            justifyContent: "space-between",
            alignItems: "center",
            gap: "8px"
          }}
        >
          <h3
            style={{
              margin: 0,
              fontSize: "15px",
              display: "flex",
              alignItems: "center",
              gap: "6px"
            }}
          >
            <Sparkles size={15} aria-hidden="true" />
            <span>
              {L(
                "ملف التعلّم الشخصي المستمر (Continuous Local Learning)",
                "Continuous Local Learning & Personal Profile"
              )}
            </span>
          </h3>
          <button
            type="button"
            className="safi-btn safi-btn-ghost"
            disabled={readOnlyOffline || busy}
            data-reset-learning-profile-btn="true"
            onClick={() => void handleResetLearningProfile()}
          >
            <RotateCcw size={13} aria-hidden="true" />
            <span>{L("إعادة ضبط التعلّم", "Reset Learning Profile")}</span>
          </button>
        </div>

        {/* Honest Longitudinal Learning Effectiveness Metrics (Section 3 & 8.4) */}
        {learningProfile?.effectivenessMetrics ? (
          <div
            data-learning-effectiveness-metrics="true"
            style={{
              display: "grid",
              gap: "6px",
              fontSize: "12px",
              padding: "10px 12px",
              borderRadius: "8px",
              background: "rgba(16, 185, 129, 0.08)"
            }}
          >
            <div style={{ display: "flex", flexWrap: "wrap", gap: "12px" }}>
              <span>
                <strong>{L("أسئلة أُعيد استخدامها:", "Questions Reused:")}</strong>{" "}
                {learningProfile.effectivenessMetrics.questionsReusedTotal ??
                  learningProfile.effectivenessMetrics.questionsAvoidedTotal}
              </span>
              <span>
                <strong>{L("خطوات موثّقة مُطبّقة:", "Verified Steps Applied:")}</strong>{" "}
                {learningProfile.effectivenessMetrics.verifiedStepsAppliedTotal ??
                  learningProfile.effectivenessMetrics.setupStepsSavedTotal}
              </span>
              <span>
                <strong>{L("إعادات تشغيل مكتملة:", "Completed Replays:")}</strong>{" "}
                {learningProfile.effectivenessMetrics.completedReplaysTotal ??
                  learningProfile.effectivenessMetrics.workflowsReplayedTotal}
              </span>
              <span>
                <strong>{L("التصحيحات / التراجعات:", "Corrections / Undos:")}</strong>{" "}
                {learningProfile.effectivenessMetrics.explicitCorrectionsRecorded +
                  learningProfile.effectivenessMetrics.repeatedCorrectionsCount}{" "}
                / {learningProfile.effectivenessMetrics.undoOrRollbackCount}
              </span>
              <span data-observed-retention-rate="true">
                <strong>
                  {L("معدل استبقاء القرارات المرصود:", "Observed Decision Retention Rate:")}
                </strong>{" "}
                {(learningProfile.effectivenessMetrics.observedEvaluationDenom ?? 0) === 0 ||
                learningProfile.effectivenessMetrics.observedRetentionRatePct === null ||
                learningProfile.effectivenessMetrics.observedRetentionRatePct === undefined
                  ? L("لا توجد أحداث تقييم بعد (0/0)", "No evaluation evidence yet (0/0)")
                  : `${learningProfile.effectivenessMetrics.observedRetentionRatePct}% (${learningProfile.effectivenessMetrics.observedRetentionNum ?? 0}/${learningProfile.effectivenessMetrics.observedEvaluationDenom ?? 0})`}
              </span>
            </div>
            <div style={{ fontSize: "11px", opacity: 0.78 }}>
              {L(
                learningProfile.effectivenessMetrics.metricDefinitionAr ??
                  "معدل استبقاء القرارات المرصود يقيس استمرار القرارات دون تصحيح أو تراجع، وليس دقة تحليلية مطلقة.",
                learningProfile.effectivenessMetrics.metricDefinitionEn ??
                  "Observed Decision Retention Rate tracks how often replayed/accepted decisions stayed unmodified; it is not ground-truth analytical accuracy."
              )}
            </div>
          </div>
        ) : null}

        <div
          style={{
            display: "grid",
            gap: "8px",
            gridTemplateColumns: "repeat(auto-fit, minmax(180px, 1fr))"
          }}
        >
          <select
            value={learnSignalKind}
            onChange={(e) =>
              setLearnSignalKind(e.target.value as LearnedPreferenceRule["signalKind"])
            }
            className="safi-input"
            data-learn-signal-select="true"
            disabled={readOnlyOffline || busy}
          >
            <option value="explicit_correction">
              {L("تصحيح صريح لتوصية", "Explicit Correction")}
            </option>
            <option value="preferred_rule">{L("قاعدة مفضلة دائمة", "Preferred Rule")}</option>
            <option value="explicit_acceptance">{L("قبول توصية", "Explicit Acceptance")}</option>
            <option value="explicit_rejection">{L("رفض توصية", "Explicit Rejection")}</option>
          </select>

          <select
            value={learnCategory}
            onChange={(e) => setLearnCategory(e.target.value as LearnedPreferenceRule["category"])}
            className="safi-input"
            data-learn-category-select="true"
            disabled={readOnlyOffline || busy}
          >
            <option value="column_classification">
              {L("تصنيف دور العمود", "Column Role Classification")}
            </option>
            <option value="date_format">
              {L("صيغة التاريخ المفضلة", "Preferred Date Format")}
            </option>
            <option value="currency_unit">
              {L("العملة / الوحدة المفضلة", "Preferred Currency / Unit")}
            </option>
            <option value="duplicate_handling">{L("معالجة التكرار", "Duplicate Handling")}</option>
            <option value="missing_value_treatment">
              {L("معالجة القيم المفقودة", "Missing Value Treatment")}
            </option>
            <option value="transformation_rule">
              {L("قاعدة تحويل وتنقية", "Transformation Rule")}
            </option>
          </select>

          <select
            value={learnScope}
            onChange={(e) => setLearnScope(e.target.value as LearnedPreferenceRule["scope"])}
            className="safi-input"
            data-learn-scope-select="true"
            disabled={readOnlyOffline || busy}
          >
            <option value="matching_sheets">
              {L("تطبيق على الأوراق المشابهة مستقبلًا", "Apply to matching sheets in future")}
            </option>
            <option value="this_sheet_only">
              {L("تطبيق على هذه الورقة فقط", "Apply to this sheet only")}
            </option>
          </select>

          <input
            type="text"
            value={learnTargetConcept}
            onChange={(e) => setLearnTargetConcept(e.target.value)}
            placeholder={L(
              "العمود أو المفهوم (مثل order_id)...",
              "Column or concept (e.g. order_id)..."
            )}
            className="safi-input"
            data-learn-target-input="true"
            disabled={readOnlyOffline || busy}
          />
          <input
            type="text"
            value={learnPreferredValue}
            onChange={(e) => setLearnPreferredValue(e.target.value)}
            placeholder={L("القرار الصحيح/المفضل...", "Preferred/corrected decision...")}
            className="safi-input"
            data-learn-preferred-input="true"
            disabled={readOnlyOffline || busy}
          />
          <input
            type="text"
            value={learnRejectedValue}
            onChange={(e) => setLearnRejectedValue(e.target.value)}
            placeholder={L("الاقتراح المرفوض (اختياري)...", "Rejected suggestion (optional)...")}
            className="safi-input"
            data-learn-rejected-input="true"
            disabled={readOnlyOffline || busy}
          />
          <input
            type="text"
            value={learnExplanation}
            onChange={(e) => setLearnExplanation(e.target.value)}
            placeholder={L("سبب التصحيح (للتفسير المستقبلي)...", "Why (for future explanation)...")}
            className="safi-input"
            data-learn-explanation-input="true"
            disabled={readOnlyOffline || busy}
          />
        </div>

        <div>
          <button
            type="submit"
            className="safi-btn safi-btn-primary"
            disabled={readOnlyOffline || busy}
            data-record-learning-btn="true"
          >
            <Sparkles size={14} aria-hidden="true" />
            <span>{L("حفظ التصحيح في ذاكرة التعلّم", "Record Correction / Preference")}</span>
          </button>
        </div>
      </form>

      {/* Learned Preferences List with Provenance & Undo */}
      <div data-learned-rules-list="true" style={{ display: "grid", gap: "8px" }}>
        {learningProfile?.rules.map((rule) => (
          <article
            key={rule.ruleId}
            data-learned-rule-id={rule.ruleId}
            style={{
              padding: "10px 12px",
              borderRadius: "8px",
              border: "1px solid var(--border-subtle, rgba(148,163,184,0.28))",
              display: "flex",
              flexWrap: "wrap",
              justifyContent: "space-between",
              alignItems: "center",
              gap: "8px",
              fontSize: "12px"
            }}
          >
            <div>
              <div>
                <strong>{rule.targetConcept}</strong> → <code>{rule.preferredValue}</code>{" "}
                {rule.rejectedValue ? (
                  <span style={{ opacity: 0.75 }}>
                    ({L("بدلًا من:", "instead of:")} <s>{rule.rejectedValue}</s>)
                  </span>
                ) : null}{" "}
                • <span>{rule.confidencePct}%</span> • <span>{rule.scope}</span>
              </div>
              <div style={{ opacity: 0.8 }}>
                {lang === "ar" ? rule.provenanceAr : rule.provenanceEn}
                {rule.userExplanation ? ` — "${rule.userExplanation}"` : ""}
              </div>
            </div>
            <button
              type="button"
              className="safi-btn safi-btn-ghost"
              disabled={readOnlyOffline || busy}
              data-delete-learned-rule-btn={rule.ruleId}
              onClick={() => void handleDeleteRule(rule.ruleId)}
            >
              <Trash2 size={13} aria-hidden="true" />
              <span>{L("تراجع / حذف", "Undo / Delete")}</span>
            </button>
          </article>
        ))}
      </div>
    </section>
  );
}
