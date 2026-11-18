import React, { useMemo, useState } from "react";
import {
  BrainCircuit,
  HelpCircle,
  MessageSquare,
  ShieldCheck,
  ShieldOff,
  Sparkles
} from "lucide-react";
import type {
  ContextualClarificationQuestion,
  EpistemicCategory,
  VerifiedAnalysisReport
} from "../../utils/verified-analysis-engine";
import { answerAnalyticalQuestion } from "../../utils/verified-analysis-engine";
import type { SemanticWorkbookModel } from "../../utils/semantic-intelligence";
import type {
  StudioPrivacySettingsArtifact,
  StudioQuestionResolutionArtifact
} from "../studio-bridge";

export interface IntelligenceAndAssistantPanelProps {
  readonly language: "en" | "ar";
  readonly model: SemanticWorkbookModel | null;
  readonly report: VerifiedAnalysisReport | null;
  readonly questions: readonly ContextualClarificationQuestion[];
  readonly privacySettings: StudioPrivacySettingsArtifact;
  readonly onResolveQuestion: (resolution: StudioQuestionResolutionArtifact) => void;
  readonly onUpdatePrivacy: (next: StudioPrivacySettingsArtifact) => void;
}

const EPISTEMIC_LABELS: Readonly<
  Record<EpistemicCategory, { readonly en: string; readonly ar: string }>
> = {
  ConfirmedFact: { en: "Confirmed Fact", ar: "حقيقة مؤكدة" },
  DataSupportedObservation: { en: "Data-Supported Observation", ar: "ملاحظة مدعومة بالبيانات" },
  PotentialExplanation: { en: "Potential Explanation (Hypothesis)", ar: "تفسير محتمل (فرضية)" },
  UnresolvedQuestion: { en: "Unresolved Question", ar: "سؤال غير محسوم" },
  UnsupportedConclusion: { en: "Unsupported Conclusion (Disclosed Gap)", ar: "استنتاج غير مدعوم" }
};

export function IntelligenceAndAssistantPanel({
  language,
  model,
  report,
  questions,
  privacySettings,
  onResolveQuestion,
  onUpdatePrivacy
}: IntelligenceAndAssistantPanelProps): React.ReactElement {
  const isAr = language === "ar";
  const [customDraftById, setCustomDraftById] = useState<Readonly<Record<string, string>>>({});
  const [assistantQuery, setAssistantQuery] = useState("");
  const [qaHistory, setQaHistory] = useState<
    ReadonlyArray<{ readonly q: string; readonly a: string; readonly provenance: string }>
  >([]);
  const [selectedEpistemic, setSelectedEpistemic] = useState<EpistemicCategory | "ALL">("ALL");

  const filteredInsights = useMemo(() => {
    if (!report) return [];
    const list =
      selectedEpistemic === "ALL"
        ? report.insights
        : (report.insightsByEpistemicCategory[selectedEpistemic] ?? []);
    return list;
  }, [report, selectedEpistemic]);

  const pendingQuestionCount = useMemo(
    () => questions.filter((q) => q.status === "Pending").length,
    [questions]
  );

  if (!model || !report) {
    return (
      <section className="studio-panel" data-testid="intelligence-assistant-panel">
        <h3>
          {isAr ? "محرك الذكاء الدلالي والتحقق" : "Semantic Intelligence & Verified Assistant"}
        </h3>
        <p className="studio-muted-note">
          {isAr
            ? "افتح ملف بيانات أولًا لتفعيل الأسئلة التوضيحية، الاستنتاجات المعرفية، والمساعد الموثق."
            : "Open a workbook or CSV to activate semantic profiling, contextual questions, and grounded Q&A."}
        </p>
      </section>
    );
  }

  function handleAskAssistant(e: React.FormEvent): void {
    e.preventDefault();
    if (!assistantQuery.trim() || !model || !report) return;
    const ans = answerAnalyticalQuestion(assistantQuery.trim(), model, report);
    const cites = ans.citations.map((c) => `${c.sheetName}:${c.columnNames.join(",")}`).join(" · ");
    setQaHistory((prev) => [
      {
        q: assistantQuery.trim(),
        a: ans.answerText,
        provenance: `${ans.intent} | ${ans.confidenceTier}${cites ? ` | ${cites}` : ""}`
      },
      ...prev
    ]);
    setAssistantQuery("");
  }

  return (
    <div className="studio-intelligence-grid" data-testid="intelligence-assistant-panel">
      {/* 1. Privacy & Optional External AI Explicit Consent Gate */}
      <section className="studio-panel" data-testid="privacy-consent-panel">
        <div className="studio-panel-header">
          <div>
            <h3>
              {privacySettings.externalAiConsentGranted ? (
                <ShieldOff size={16} />
              ) : (
                <ShieldCheck size={16} />
              )}{" "}
              {isAr ? "حماية البيانات وضوابط الذكاء الاصطناعي" : "Local Privacy & AI Consent Gate"}
            </h3>
            <p className="studio-muted-note">
              {privacySettings.externalAiConsentGranted
                ? isAr
                  ? "موافقة الإرسال الخارجي مفعّلة: يتم إخفاء المعرّفات الشخصية وإرسال ملخص المخطط فقط عند الطلب."
                  : "External schema-only consent enabled: personal columns masked before any cloud request."
                : isAr
                  ? "وضع عدم الاتصال الصارم مفعّل افتراضيًا: لا تخرج أي بيانات أو أسماء أعمدة من جهازك."
                  : "Strict Offline Local-Only Mode (Default): zero raw rows, file names, or column values ever leave this device."}
            </p>
          </div>
          <div className="studio-quick-templates">
            <label className="studio-checkbox-label">
              <input
                type="checkbox"
                data-testid="consent-cloud-ai-checkbox"
                checked={privacySettings.externalAiConsentGranted}
                onChange={(ev) =>
                  onUpdatePrivacy({
                    ...privacySettings,
                    externalAiConsentGranted: ev.target.checked,
                    consentTimestampMs: Date.now()
                  })
                }
              />
              <span>
                {isAr
                  ? "السماح بملخصات المخطط الخارجية (بموافقة صريحة)"
                  : "Enable optional cloud schema assistance (Explicit Opt-In)"}
              </span>
            </label>
            <label className="studio-checkbox-label">
              <input
                type="checkbox"
                checked={privacySettings.maskPiiOnCloud}
                onChange={(ev) =>
                  onUpdatePrivacy({
                    ...privacySettings,
                    maskPiiOnCloud: ev.target.checked
                  })
                }
              />
              <span>
                {isAr ? "إخفاء الأعمدة الحساسة تلقائيًا" : "Mask PII / IDs automatically"}
              </span>
            </label>
          </div>
        </div>
      </section>

      {/* 2. Contextual Clarification Questions */}
      <section className="studio-panel" data-tutorial-target="contextual-questions">
        <div className="studio-panel-header">
          <div>
            <h3>
              <HelpCircle size={16} />{" "}
              {isAr ? "الأسئلة التوضيحية المخصصة للبيانات" : "Contextual Clarification Questions"}
            </h3>
            <p className="studio-muted-note">
              {isAr
                ? `مكتمل: ${questions.length - pendingQuestionCount}/${questions.length} • النتائج المؤقتة تظهر بعلامة تحذير عند التخطي أو التأجيل.`
                : `Resolved: ${questions.length - pendingQuestionCount}/${questions.length} · Skipped or deferred items flag dependent metrics as Provisional.`}
            </p>
          </div>
        </div>

        {questions.length === 0 ? (
          <p className="studio-muted-note">
            {isAr
              ? "لا توجد غموضات دلالية في هذا الجدول؛ كافة الأدوار والأعمدة مصنفة بثقة عالية."
              : "Zero semantic ambiguities detected; all column roles and links are high-confidence."}
          </p>
        ) : (
          <div className="studio-cards-list">
            {questions.map((q) => {
              const isProvisional = q.status === "Skipped" || q.status === "Deferred";
              return (
                <article
                  key={q.questionId}
                  className="studio-question-card"
                  data-testid={`question-card-${q.questionId}`}
                >
                  <div className="studio-region-card">
                    <strong>{q.prompt}</strong>
                    <span className="studio-badge-tag">
                      {q.status} {isProvisional ? "(Provisional)" : ""}
                    </span>
                  </div>
                  <p className="studio-muted-note">{q.whyItMatters}</p>
                  <div className="studio-quick-templates">
                    {q.options.map((opt) => (
                      <button
                        key={opt.optionId}
                        type="button"
                        className={`studio-chip-btn ${q.selectedOptionId === opt.optionId ? "is-active" : ""}`}
                        data-testid={`answer-q-${q.questionId}-${opt.optionId}`}
                        onClick={() =>
                          onResolveQuestion({
                            questionId: q.questionId,
                            action: q.status === "Answered" ? "corrected" : "answered",
                            answer: opt.optionId,
                            updatedAtMs: Date.now()
                          })
                        }
                      >
                        {opt.label}
                      </button>
                    ))}
                    <button
                      type="button"
                      className="studio-chip-btn"
                      data-testid={`skip-q-${q.questionId}`}
                      onClick={() =>
                        onResolveQuestion({
                          questionId: q.questionId,
                          action: "skipped",
                          answer: "skipped",
                          updatedAtMs: Date.now()
                        })
                      }
                    >
                      {isAr ? "تخطي (مؤقت)" : "Skip (Provisional)"}
                    </button>
                    <button
                      type="button"
                      className="studio-chip-btn"
                      onClick={() =>
                        onResolveQuestion({
                          questionId: q.questionId,
                          action: "deferred",
                          answer: "deferred",
                          updatedAtMs: Date.now()
                        })
                      }
                    >
                      {isAr ? "تأجيل" : "Defer"}
                    </button>
                  </div>
                  <div className="studio-inline-input-row">
                    <input
                      type="text"
                      value={customDraftById[q.questionId] ?? ""}
                      placeholder={
                        isAr ? "أو اكتب توضيحًا مخصصًا..." : "Or enter custom clarification..."
                      }
                      onChange={(ev) =>
                        setCustomDraftById((prev) => ({
                          ...prev,
                          [q.questionId]: ev.target.value
                        }))
                      }
                    />
                    <button
                      type="button"
                      className="studio-chip-btn"
                      onClick={() => {
                        const text = (customDraftById[q.questionId] ?? "").trim();
                        if (!text) return;
                        onResolveQuestion({
                          questionId: q.questionId,
                          action: "custom",
                          answer: text,
                          customText: text,
                          updatedAtMs: Date.now()
                        });
                      }}
                    >
                      {isAr ? "تطبيق المخصص" : "Apply Custom"}
                    </button>
                  </div>
                </article>
              );
            })}
          </div>
        )}
      </section>

      {/* 3. Epistemic Verified Insights (5 categories) */}
      <section className="studio-panel" data-testid="epistemic-insights-panel">
        <div className="studio-panel-header">
          <div>
            <h3>
              <BrainCircuit size={16} />{" "}
              {isAr ? "الاستنتاجات والتحقق المعرفي" : "Epistemic Verified Insights"}
            </h3>
            <p className="studio-muted-note">
              {isAr
                ? "تمييز صارم بين الحقائق، الملاحظات، الفرضيات، الفجوات غير المدعومة، وما يحتاج توضيح."
                : "Strict epistemic separation between Facts, Observations, Hypotheses, Unsupported Gaps & Open Clarifications."}
            </p>
          </div>
          <div className="studio-quick-templates">
            <button
              type="button"
              className={`studio-chip-btn ${selectedEpistemic === "ALL" ? "is-active" : ""}`}
              onClick={() => setSelectedEpistemic("ALL")}
            >
              {isAr ? `الكل (${report.insights.length})` : `All (${report.insights.length})`}
            </button>
            {(Object.keys(EPISTEMIC_LABELS) as EpistemicCategory[]).map((cat) => (
              <button
                key={cat}
                type="button"
                className={`studio-chip-btn ${selectedEpistemic === cat ? "is-active" : ""}`}
                onClick={() => setSelectedEpistemic(cat)}
              >
                {isAr ? EPISTEMIC_LABELS[cat].ar : EPISTEMIC_LABELS[cat].en} (
                {(report.insightsByEpistemicCategory[cat] ?? []).length})
              </button>
            ))}
          </div>
        </div>

        <div className="studio-cards-list">
          {filteredInsights.slice(0, 12).map((ins) => (
            <article key={ins.insightId} className="studio-insight-card">
              <div className="studio-region-card">
                <strong>{ins.title}</strong>
                <span className="studio-badge-tag">
                  {isAr
                    ? EPISTEMIC_LABELS[ins.epistemicCategory].ar
                    : EPISTEMIC_LABELS[ins.epistemicCategory].en}{" "}
                  · {ins.confidencePct}%
                </span>
              </div>
              <p>{ins.statement}</p>
              <div className="studio-muted-note">
                {ins.citation.sheetName} ({ins.citation.a1Range}) · {ins.citation.formulaOrMethod}
                {ins.provisional ? ` · ⚠ ${ins.provisionalReason ?? "Provisional"}` : ""}
              </div>
            </article>
          ))}
        </div>
      </section>

      {/* 4. Grounded Deterministic Data Assistant */}
      <section
        className="studio-panel"
        data-testid="grounded-assistant-panel"
        data-tutorial-target="grounded-assistant"
      >
        <div className="studio-panel-header">
          <div>
            <h3>
              <MessageSquare size={16} />{" "}
              {isAr
                ? "المساعد التحليلي الموثق (محلي بالكامل)"
                : "Grounded Data Assistant (Deterministic Local Q&A)"}
            </h3>
            <p className="studio-muted-note">
              {isAr
                ? "يجيب فقط من المؤشرات والإحصائيات المحسوبة فعليًا داخل المصنف مع ذكر المصدر — لا يخمّن أبدًا."
                : "Answers strictly from verified workbook metrics & distributions with cell/range citations — zero fabrication."}
            </p>
          </div>
        </div>

        <form className="studio-inline-input-row" onSubmit={handleAskAssistant}>
          <input
            type="text"
            data-testid="assistant-question-input"
            value={assistantQuery}
            onChange={(ev) => setAssistantQuery(ev.target.value)}
            placeholder={
              isAr
                ? "اسأل: ما ملخص البيانات؟ أو أي فرع حقق أعلى قيمة؟ أو التوقعات؟"
                : "Ask: What does this workbook contain? / Which category performed best? / What is the quality score?"
            }
          />
          <button type="submit" className="studio-chip-btn" data-testid="assistant-ask-btn">
            <Sparkles size={14} /> {isAr ? "اسأل" : "Ask"}
          </button>
        </form>

        {qaHistory.length > 0 ? (
          <div className="studio-cards-list">
            {qaHistory.map((item, idx) => (
              <div
                key={`${idx}_${item.q.slice(0, 12)}`}
                className="studio-insight-card"
                data-testid={`assistant-answer-${idx}`}
              >
                <strong>Q: {item.q}</strong>
                <p>{item.a}</p>
                <span className="studio-muted-note">{item.provenance}</span>
              </div>
            ))}
          </div>
        ) : null}
      </section>
    </div>
  );
}
