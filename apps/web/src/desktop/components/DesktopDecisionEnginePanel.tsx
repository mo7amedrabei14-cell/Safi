/**
 * `apps/web/src/desktop/components/DesktopDecisionEnginePanel.tsx`
 *
 * Real Desktop UI for the Central Rust `Safi Decision Engine` (Sections 10, 11, 12 & 13.1):
 * - Displays ONE primary Recommended Decision per product domain (Spreadsheet Understanding,
 *   Data Cleaning, Optimal KPI Selection, Optimal Chart Selection, Coherent Dashboard Blueprint,
 *   and Executive Report & Presentation).
 * - Explicitly discloses the honest Optimality Class (`ExactProvenOptimum`, `BestValidCandidateFound`,
 *   `HeuristicRecommendation`, `InsufficientEvidenceRequiresQuestion`) — never conflating a heuristic
 *   or similarity score with mathematical probability.
 * - Shows concrete Empirical Evidence, Assumptions, What Will Execute, 1-Click Apply, 1-Click Undo,
 *   and a collapsible `Advanced Details / Audit` view showing pruned/eliminated candidates and their
 *   7-level Lexicographic Objective Vector.
 * - Displays Cross-Product Consistency & Lineage verification (Section 11.7) and the 8-domain
 *   AI Task Routing & Local Ollama boundary table (Section 12).
 */
import type { ReactNode } from "react";
import { useCallback, useEffect, useState } from "react";
import {
  CheckCircle2,
  ChevronDown,
  ChevronUp,
  Cpu,
  Play,
  RefreshCw,
  ShieldAlert,
  ShieldCheck,
  Sparkles,
  Undo2
} from "lucide-react";
import type { SafiLang } from "../../components/layout/FloatingGlassDock";
import {
  applyDesktopDecisionEngineAction,
  evaluateDesktopDecisionEngine,
  undo,
  type DesktopStudioState,
  type OptimalityClass,
  type RecommendedDecision,
  type WorkspaceOptimalDecisionSuite
} from "../studio-bridge";

export interface DesktopDecisionEnginePanelProps {
  readonly lang?: SafiLang;
  readonly projectId: string;
  readonly state: DesktopStudioState | null;
  readonly readOnlyOffline?: boolean;
  readonly onStateUpdated: (nextState: DesktopStudioState) => void;
}

function formatOptimalityBadge(cls: OptimalityClass, lang: SafiLang): string {
  const isAr = lang === "ar";
  switch (cls) {
    case "exact_proven_optimum":
      return isAr ? "حل أمثل مبرهن (Exact Proven Optimum)" : "Exact Proven Optimum";
    case "best_valid_candidate_found":
      return isAr ? "أفضل مرشح صالح (Best Valid Candidate Found)" : "Best Valid Candidate Found";
    case "heuristic_recommendation":
      return isAr ? "توصية استدلالية (Heuristic Recommendation)" : "Heuristic Recommendation";
    case "insufficient_evidence_requires_question":
      return isAr
        ? "أدلة غير كافية — يتطلب سؤالًا توضيحيًا"
        : "Insufficient Evidence — Needs Clarification";
  }
}

export function DesktopDecisionEnginePanel({
  lang = "en",
  projectId,
  state,
  readOnlyOffline = false,
  onStateUpdated
}: DesktopDecisionEnginePanelProps): ReactNode {
  const L = useCallback((ar: string, en: string): string => (lang === "ar" ? ar : en), [lang]);
  const [suite, setSuite] = useState<WorkspaceOptimalDecisionSuite | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [expandedAudits, setExpandedAudits] = useState<Readonly<Record<string, boolean>>>({});

  const refreshEvaluation = useCallback(async () => {
    if (state === null) {
      setSuite(null);
      return;
    }
    try {
      const evaluated = await evaluateDesktopDecisionEngine(projectId);
      setSuite(evaluated);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }, [projectId, state]);

  useEffect(() => {
    void refreshEvaluation();
  }, [refreshEvaluation, state?.revision]);

  const handleApplyDecision = async (decision: RecommendedDecision): Promise<void> => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const nextState = await applyDesktopDecisionEngineAction(projectId, decision.decisionId);
      onStateUpdated(nextState);
      const refreshed = await evaluateDesktopDecisionEngine(projectId);
      setSuite(refreshed);
      setNotice(
        L(
          `تم تطبيق القرار الموصى به بنجاح: ${decision.recommendedTitleAr}`,
          `Applied Recommended Decision: ${decision.recommendedTitleEn}`
        )
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleUndoLast = async (): Promise<void> => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const nextState = await undo(projectId);
      onStateUpdated(nextState);
      const refreshed = await evaluateDesktopDecisionEngine(projectId);
      setSuite(refreshed);
      setNotice(
        L(
          "تم التراجع عن آخر عملية واستعادة النسخة السابقة من البيانات حتميًا.",
          "Reverted last operation and restored previous DataGraph revision deterministically."
        )
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const toggleAudit = (decisionId: string): void => {
    setExpandedAudits((prev) => ({
      ...prev,
      [decisionId]: !prev[decisionId]
    }));
  };

  return (
    <section
      className="safi-panel"
      data-desktop-decision-engine="true"
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
            <Cpu size={18} aria-hidden="true" />
            <span>
              {L(
                "محرك القرار الأمثل المركزي (Safi Optimal Decision Engine)",
                "Safi Optimal Decision Engine (Rust Lexicographic Solver)"
              )}
            </span>
          </h2>
          <p style={{ margin: "4px 0 0", fontSize: "13px", opacity: 0.85 }}>
            {L(
              "يختبر البدائل الصالحة، ويستبعد الحلول الضارة أو المضللة، ويختار قرارًا رئيسيًا واحدًا لكل مجال وفق دالة هدف معجمية من 7 مستويات تعطي الأولوية القصوى لصحة وسلامة البيانات.",
              "Evaluates valid candidates, prunes unsafe or misleading options, and selects ONE primary evidence-backed Recommended Decision per domain using a 7-level Lexicographic Objective Function prioritizing data correctness and zero data loss."
            )}
          </p>
        </div>

        <div style={{ display: "flex", gap: "8px" }}>
          <button
            type="button"
            className="safi-btn safi-btn-ghost"
            onClick={() => void handleUndoLast()}
            disabled={readOnlyOffline || busy || state === null}
            data-decision-engine-undo-btn="true"
          >
            <Undo2 size={14} aria-hidden="true" />
            <span>{L("تراجع عن القرار", "Undo")}</span>
          </button>
          <button
            type="button"
            className="safi-btn safi-btn-ghost"
            onClick={() => void refreshEvaluation()}
            disabled={busy || state === null}
            data-decision-engine-refresh-btn="true"
          >
            <RefreshCw size={14} aria-hidden="true" />
            <span>{L("إعادة التقييم", "Re-Evaluate")}</span>
          </button>
        </div>
      </div>

      {error !== null ? (
        <div
          role="alert"
          data-decision-engine-error="true"
          style={{ display: "flex", alignItems: "center", gap: "8px", fontSize: "13px" }}
        >
          <ShieldAlert size={16} aria-hidden="true" />
          <span>{error}</span>
        </div>
      ) : null}

      {notice !== null ? (
        <div
          role="status"
          data-decision-engine-notice="true"
          style={{ fontSize: "13px", fontWeight: 600 }}
        >
          {notice}
        </div>
      ) : null}

      {state === null || suite === null ? (
        <p style={{ margin: 0, fontSize: "13px", opacity: 0.78 }}>
          {L(
            "افتح ملف بيانات أولًا ليقوم محرك القرار الأمثل في Rust بتقييم الهيكل والتنقية والمؤشرات والرسوم.",
            "Open a dataset first so the Rust Safi Decision Engine can evaluate schema, cleaning, KPIs, and charts."
          )}
        </p>
      ) : (
        <>
          {/* Cross-Product Consistency & Lineage Banner (Section 11.7) */}
          <div
            data-consistency-report="true"
            data-consistency-ok={suite.consistencyReport.isFullyConsistent ? "true" : "false"}
            style={{
              padding: "12px 14px",
              borderRadius: "10px",
              border: "1px solid var(--border-strong, rgba(16,185,129,0.4))",
              background: suite.consistencyReport.isFullyConsistent
                ? "rgba(16, 185, 129, 0.08)"
                : "rgba(245, 158, 11, 0.1)",
              display: "grid",
              gap: "4px",
              fontSize: "12.5px"
            }}
          >
            <div style={{ display: "flex", alignItems: "center", gap: "6px", fontWeight: 700 }}>
              <ShieldCheck size={15} aria-hidden="true" />
              <span>
                {L(
                  "اتساق المخرجات وسلسلة النسب عبر المنتج (Cross-Product Consistency):",
                  "Cross-Product Lineage & Consistency Gate:"
                )}
              </span>
            </div>
            <div>
              {lang === "ar"
                ? suite.consistencyReport.summaryAr
                : suite.consistencyReport.summaryEn}
            </div>
          </div>

          {/* Primary Recommended Decisions (One per domain — Section 10.5 & 13.1) */}
          <div data-recommended-decisions-list="true" style={{ display: "grid", gap: "14px" }}>
            {suite.recommendedDecisions.map((dec) => {
              const auditOpen = Boolean(expandedAudits[dec.decisionId]);
              return (
                <article
                  key={dec.decisionId}
                  data-recommended-decision-card={dec.decisionId}
                  data-decision-domain={dec.domain}
                  data-optimality-class={dec.optimalityClass}
                  style={{
                    padding: "16px",
                    borderRadius: "12px",
                    border: "1px solid var(--border-subtle, rgba(148,163,184,0.3))",
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
                    <div
                      style={{
                        display: "flex",
                        flexWrap: "wrap",
                        gap: "6px",
                        alignItems: "center"
                      }}
                    >
                      <span
                        style={{
                          fontSize: "11px",
                          fontWeight: 700,
                          padding: "3px 8px",
                          borderRadius: "999px",
                          background: "rgba(59, 130, 246, 0.14)"
                        }}
                      >
                        {dec.domain.toUpperCase()}
                      </span>
                      <span
                        data-optimality-badge={dec.optimalityClass}
                        style={{
                          fontSize: "11px",
                          fontWeight: 700,
                          padding: "3px 8px",
                          borderRadius: "999px",
                          background: "rgba(16, 185, 129, 0.14)"
                        }}
                      >
                        {formatOptimalityBadge(dec.optimalityClass, lang)}
                      </span>
                      <span style={{ fontSize: "11px", opacity: 0.72 }}>
                        {L("الخوارزمية:", "Solver:")} <code>{dec.algorithmUsed}</code>
                      </span>
                    </div>

                    <button
                      type="button"
                      className="safi-btn safi-btn-primary"
                      disabled={readOnlyOffline || busy}
                      data-apply-recommended-decision={dec.decisionId}
                      onClick={() => void handleApplyDecision(dec)}
                    >
                      <Play size={13} aria-hidden="true" />
                      <span>{L("تطبيق القرار الموصى به", "Apply Recommended Decision")}</span>
                    </button>
                  </div>

                  <h3 style={{ margin: 0, fontSize: "15.5px", fontWeight: 800 }}>
                    {lang === "ar" ? dec.recommendedTitleAr : dec.recommendedTitleEn}
                  </h3>

                  <p style={{ margin: 0, fontSize: "13px", opacity: 0.9 }}>
                    <strong>{L("السبب:", "Why:")}</strong> {lang === "ar" ? dec.whyAr : dec.whyEn}
                  </p>

                  {/* Evidence Items */}
                  <div
                    data-decision-evidence={dec.decisionId}
                    style={{
                      display: "grid",
                      gap: "6px",
                      gridTemplateColumns: "repeat(auto-fit, minmax(220px, 1fr))"
                    }}
                  >
                    {dec.evidence.map((ev, idx) => (
                      <div
                        key={idx}
                        style={{
                          padding: "8px 10px",
                          borderRadius: "8px",
                          background: "rgba(148, 163, 184, 0.08)",
                          fontSize: "12px"
                        }}
                      >
                        <div>
                          <strong>{lang === "ar" ? ev.labelAr : ev.labelEn}:</strong>{" "}
                          <code>{ev.observedValue}</code>
                        </div>
                        <div style={{ opacity: 0.82 }}>
                          {lang === "ar" ? ev.impactAr : ev.impactEn}
                        </div>
                      </div>
                    ))}
                  </div>

                  {/* Assumptions, What Executes & Undo Path */}
                  <div style={{ fontSize: "12px", opacity: 0.85, display: "grid", gap: "3px" }}>
                    <div>
                      <strong>{L("الافتراضات:", "Assumptions:")}</strong>{" "}
                      {(lang === "ar" ? dec.assumptionsAr : dec.assumptionsEn).join(" • ")}
                    </div>
                    <div>
                      <strong>{L("ما سيتم تنفيذه:", "What executes:")}</strong>{" "}
                      {lang === "ar" ? dec.whatExecutesAr : dec.whatExecutesEn}
                    </div>
                    <div>
                      <strong>{L("طريقة التراجع:", "How to undo:")}</strong>{" "}
                      {lang === "ar" ? dec.undoWayAr : dec.undoWayEn}
                    </div>
                  </div>

                  {/* Focused Single Clarification Question when Evidence is Insufficient (Section 10.7) */}
                  {dec.clarificationPrompt ? (
                    <div
                      data-decision-clarification-prompt={dec.clarificationPrompt.questionId}
                      style={{
                        padding: "10px 12px",
                        borderRadius: "8px",
                        border: "1px solid rgba(245, 158, 11, 0.45)",
                        background: "rgba(245, 158, 11, 0.08)",
                        fontSize: "12.5px"
                      }}
                    >
                      <strong>
                        {L("سؤال توضيحي واحد لحسم الغموض:", "Focused Clarification Needed:")}
                      </strong>{" "}
                      {lang === "ar"
                        ? dec.clarificationPrompt.promptAr
                        : dec.clarificationPrompt.promptEn}
                    </div>
                  ) : null}

                  {/* Advanced Details / Audit Toggle (Section 10.5 & 13.1) */}
                  <div>
                    <button
                      type="button"
                      className="safi-btn safi-btn-ghost"
                      data-toggle-decision-audit={dec.decisionId}
                      onClick={() => toggleAudit(dec.decisionId)}
                      style={{ fontSize: "12px" }}
                    >
                      {auditOpen ? (
                        <ChevronUp size={13} aria-hidden="true" />
                      ) : (
                        <ChevronDown size={13} aria-hidden="true" />
                      )}
                      <span>
                        {L(
                          `تفاصيل التدقيق والبدائل المستبعدة (${dec.candidatesEvaluatedCount} مرشح • ${dec.invalidEliminatedCount} مستبعد)`,
                          `Advanced Details / Audit (${dec.candidatesEvaluatedCount} evaluated • ${dec.invalidEliminatedCount} eliminated)`
                        )}
                      </span>
                    </button>

                    {auditOpen ? (
                      <div
                        data-decision-audit-drawer={dec.decisionId}
                        style={{
                          marginTop: "8px",
                          padding: "10px",
                          borderRadius: "8px",
                          background: "rgba(15, 23, 42, 0.06)",
                          display: "grid",
                          gap: "6px",
                          fontSize: "11.5px"
                        }}
                      >
                        <div>
                          <strong>
                            {L(
                              "متجه دالة الهدف المعجمية (7 مستويات حسب الأولوية):",
                              "7-Level Lexicographic Objective Vector (Priority Order):"
                            )}
                          </strong>{" "}
                          <code>
                            [1.Correctness={dec.objective.correctness}, 2.Safety=
                            {dec.objective.dataSafetyNoLoss}, 3.Constraints=
                            {dec.objective.userConstraintCompliance}, 4.Fit=
                            {dec.objective.analyticalFit}, 5.Audit=
                            {dec.objective.explainabilityAudit}, 6.Quality=
                            {dec.objective.deliverableQuality}, 7.Efficiency=
                            {dec.objective.executionEfficiency}]
                          </code>
                        </div>
                        {dec.advancedAuditCandidates.map((cand) => (
                          <div
                            key={cand.candidateId}
                            data-audit-candidate={cand.candidateId}
                            data-audit-candidate-valid={cand.isValid ? "true" : "false"}
                            style={{
                              padding: "6px 8px",
                              borderRadius: "6px",
                              borderLeft: cand.isValid ? "3px solid #10b981" : "3px solid #ef4444"
                            }}
                          >
                            <div>
                              <strong>{lang === "ar" ? cand.titleAr : cand.titleEn}</strong> —{" "}
                              {cand.isValid
                                ? L("صالح ومختار", "VALID & SELECTED")
                                : L("مستبعد (PRUNED)", "ELIMINATED (PRUNED)")}
                            </div>
                            {!cand.isValid &&
                            (cand.eliminationReasonAr || cand.eliminationReasonEn) ? (
                              <div style={{ opacity: 0.88 }}>
                                {lang === "ar"
                                  ? cand.eliminationReasonAr
                                  : cand.eliminationReasonEn}
                              </div>
                            ) : null}
                          </div>
                        ))}
                      </div>
                    ) : null}
                  </div>
                </article>
              );
            })}
          </div>

          {/* AI Task Routing & Local Intelligence Boundary Table (Section 12) */}
          <div
            data-ai-task-routing="true"
            style={{
              padding: "14px",
              borderRadius: "10px",
              border: "1px solid var(--border-subtle, rgba(148,163,184,0.28))",
              display: "grid",
              gap: "8px"
            }}
          >
            <h3
              style={{
                margin: 0,
                fontSize: "14.5px",
                display: "flex",
                alignItems: "center",
                gap: "6px"
              }}
            >
              <Sparkles size={15} aria-hidden="true" />
              <span>
                {L(
                  "توجيه المهام بين المحرك الحتمي ومحرك القرار والذكاء المحلي (AI Task Routing)",
                  "Deterministic vs. Decision Engine vs. AI Task Routing Matrix"
                )}
              </span>
            </h3>
            <p style={{ margin: 0, fontSize: "12px", opacity: 0.84 }}>
              {L(
                `حالة Ollama المحلي (${suite.aiArchitecture.localOllamaEndpoint}): لا يُشترط أي نموذج لغوي لتشغيل التنقية أو الحسابات أو محرك القرار الأمثل. جميع العمليات الحسابية تُنفَّذ في Rust بنسبة 100%.`,
                `Local Ollama (${suite.aiArchitecture.localOllamaEndpoint}): Core cleaning, calculations, and the Optimal Decision Engine run 100% in deterministic Rust without requiring any LLM.`
              )}
            </p>
            <div
              style={{
                display: "grid",
                gap: "6px",
                gridTemplateColumns: "repeat(auto-fit, minmax(240px, 1fr))"
              }}
            >
              {suite.aiArchitecture.taskRoutingTable.map((route) => (
                <div
                  key={route.taskId}
                  data-ai-route-task={route.taskId}
                  style={{
                    padding: "8px 10px",
                    borderRadius: "8px",
                    background: "rgba(148, 163, 184, 0.07)",
                    fontSize: "11.5px"
                  }}
                >
                  <div style={{ fontWeight: 700 }}>
                    {lang === "ar" ? route.taskNameAr : route.taskNameEn}
                  </div>
                  <div style={{ opacity: 0.85 }}>{route.primaryEngine}</div>
                  <div
                    style={{
                      fontSize: "10.5px",
                      opacity: 0.72,
                      display: "flex",
                      alignItems: "center",
                      gap: "4px"
                    }}
                  >
                    <CheckCircle2 size={11} aria-hidden="true" />
                    <span>{L("حسابات LLM ممنوعة (0%)", "LLM Numeric Computation: FORBIDDEN")}</span>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </>
      )}
    </section>
  );
}
