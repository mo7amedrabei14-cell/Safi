import React, { useMemo, useState } from "react";
import {
  DEFAULT_AI_PROVIDER,
  interpretPromptWithRealAi,
  previewDslCommand,
  validateUntrustedDslPayload,
  type AiProviderConfig,
  type AiSchemaContext,
  type StudioDslCommand,
} from "../ai-interpreter";

export interface CommandBarPanelProps {
  readonly language: "en" | "ar";
  readonly projectLoaded: boolean;
  readonly schema: AiSchemaContext;
  readonly onExecuteDsl: (dslJson: string) => Promise<void>;
}

const SAMPLE_DSL_COMMANDS: ReadonlyArray<{ labelEn: string; labelAr: string; json: string }> = [
  {
    labelEn: "Define SUM metric",
    labelAr: "تعريف مؤشر مجموع",
    json: JSON.stringify(
      {
        op: "define_metric",
        id: "total_amount",
        name: "Total Amount",
        unit: "EGP",
        formula: "SUM(amount)",
      },
      null,
      2
    ),
  },
  {
    labelEn: "Add calculated column",
    labelAr: "إضافة عمود محسوب",
    json: JSON.stringify(
      {
        op: "add_calculated_column",
        name: "double_amount",
        formula: "amount * 2",
      },
      null,
      2
    ),
  },
  {
    labelEn: "Sort descending",
    labelAr: "فرز تنازلي",
    json: JSON.stringify(
      {
        op: "sort_rows",
        column: "amount",
        ascending: false,
      },
      null,
      2
    ),
  },
];

export function CommandBarPanel({
  language,
  projectLoaded,
  schema,
  onExecuteDsl,
}: CommandBarPanelProps): React.ReactElement {
  const isAr = language === "ar";
  const [promptInput, setPromptInput] = useState("");
  const [dslInput, setDslInput] = useState("");
  const [provider, setProvider] = useState<AiProviderConfig>(DEFAULT_AI_PROVIDER);
  const [showProviderConfig, setShowProviderConfig] = useState(false);
  const [isProcessing, setIsProcessing] = useState(false);
  const [aiSummary, setAiSummary] = useState<string | null>(null);
  const [blockerBanner, setBlockerBanner] = useState<{ code: string; message: string } | null>(
    null
  );
  const [execStatus, setExecStatus] = useState<string | null>(null);
  const [execError, setExecError] = useState<string | null>(null);

  const dslValidation = useMemo(() => {
    if (!dslInput.trim()) {
      return null;
    }
    return validateUntrustedDslPayload(dslInput);
  }, [dslInput]);

  const proposedPreview = useMemo(() => {
    if (!dslValidation || !dslValidation.ok) {
      return null;
    }
    return previewDslCommand(dslValidation.command, language);
  }, [dslValidation, language]);

  async function handleAskAi(): Promise<void> {
    if (!promptInput.trim() || !projectLoaded) {
      return;
    }
    setIsProcessing(true);
    setBlockerBanner(null);
    setAiSummary(null);
    setExecStatus(null);
    setExecError(null);
    try {
      const result = await interpretPromptWithRealAi(promptInput, schema, provider);
      if (!result.ok) {
        setBlockerBanner({ code: result.code, message: result.blockerDetail });
        return;
      }
      setAiSummary(`${result.model}: ${result.suggestedSummary}`);
      setDslInput(result.dslJson);
    } finally {
      setIsProcessing(false);
    }
  }

  async function handleApproveAndExecute(): Promise<void> {
    if (!projectLoaded || !dslValidation || !dslValidation.ok) {
      return;
    }
    setIsProcessing(true);
    setExecError(null);
    setExecStatus(null);
    try {
      const commandObj: StudioDslCommand = dslValidation.command;
      await onExecuteDsl(JSON.stringify(commandObj));
      setExecStatus(
        isAr
          ? `تم التنفيذ والتحقق بواسطة محرك Rust (${commandObj.op}).`
          : `Approved & executed in Rust (${commandObj.op}).`
      );
    } catch (err) {
      setExecError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsProcessing(false);
    }
  }

  return (
    <section
      className="studio-panel studio-command-bar"
      aria-label={isAr ? "شريط الأوامر والذكاء الاصطناعي" : "Studio Command Bar & AI Interpreter"}
    >
      <div className="studio-panel-header">
        <div>
          <h3>{isAr ? "شريط الأوامر المهيكلة والذكاء الاصطناعي" : "Command Bar & AI Interpreter"}</h3>
          <p className="studio-muted-note">
            {isAr
              ? "الذكاء الاصطناعي يقترح أوامر DSL فقط ولا يحسب أي رقم؛ التنفيذ يتطلب موافقتك الصريحة ويُتحقق منه في Rust."
              : "AI only proposes structured DSL commands and never computes numbers; execution requires explicit approval and Rust validation."}
          </p>
        </div>
        <button
          type="button"
          className="studio-btn-secondary"
          onClick={() => setShowProviderConfig((v) => !v)}
        >
          {isAr
            ? provider.kind === "unconfigured"
              ? "مزود الذكاء: غير مهيأ"
              : "إعدادات مزود الذكاء"
            : provider.kind === "unconfigured"
              ? "AI Provider: Unconfigured"
              : "AI Provider Settings"}
        </button>
      </div>

      {showProviderConfig ? (
        <div className="studio-ai-provider-box" data-testid="ai-provider-settings">
          <div className="studio-inline-fields">
            <label>
              <span>{isAr ? "النوع" : "Provider"}</span>
              <select
                value={provider.kind}
                onChange={(e) =>
                  setProvider((prev) => ({
                    ...prev,
                    kind: e.target.value as AiProviderConfig["kind"],
                  }))
                }
              >
                <option value="unconfigured">
                  {isAr ? "بدون مزود (وضع عدم الاتصال الافتراضي)" : "Unconfigured (Offline Default)"}
                </option>
                <option value="openai_compatible">
                  {isAr ? "خادم محلي/معتمد متوافق مع OpenAI" : "Local/Approved OpenAI-Compatible"}
                </option>
              </select>
            </label>
            <label>
              <span>{isAr ? "عنوان الخادم" : "Endpoint URL"}</span>
              <input
                type="url"
                value={provider.endpointUrl}
                placeholder="http://127.0.0.1:11434/v1/chat/completions"
                onChange={(e) =>
                  setProvider((prev) => ({ ...prev, endpointUrl: e.target.value }))
                }
              />
            </label>
            <label>
              <span>{isAr ? "اسم النموذج" : "Model Name"}</span>
              <input
                type="text"
                value={provider.modelName}
                placeholder="qwen2.5:7b"
                onChange={(e) =>
                  setProvider((prev) => ({ ...prev, modelName: e.target.value }))
                }
              />
            </label>
          </div>
          <label className="studio-checkbox-label">
            <input
              type="checkbox"
              checked={provider.consentedToSchemaContext}
              onChange={(e) =>
                setProvider((prev) => ({
                  ...prev,
                  consentedToSchemaContext: e.target.checked,
                }))
              }
            />
            <span>
              {isAr
                ? "أوافق صراحةً على إرسال أسماء الأعمدة والمؤشرات فقط (بدون قيم الصفوف أو أسماء الملفات)"
                : "I explicitly consent to sending column/metric schema names only (zero row data or filenames)"}
            </span>
          </label>
        </div>
      ) : null}

      <div className="studio-command-nl-row">
        <input
          type="text"
          className="studio-input-grow"
          value={promptInput}
          disabled={!projectLoaded || isProcessing}
          onChange={(e) => setPromptInput(e.target.value)}
          placeholder={
            isAr
              ? "اكتب طلبك بالعربية أو الإنجليزية (مثال: احسب إجمالي الإيرادات حسب المنطقة)..."
              : "Ask in Arabic or English (e.g., Analyze total revenue by region)..."
          }
          aria-label={isAr ? "طلب لغة طبيعية" : "Natural language request"}
        />
        <button
          type="button"
          className="studio-btn-secondary"
          disabled={!projectLoaded || isProcessing || !promptInput.trim()}
          onClick={() => {
            void handleAskAi();
          }}
        >
          {isProcessing
            ? isAr
              ? "جارٍ المعالجة..."
              : "Processing..."
            : isAr
              ? "ترجمة عبر الذكاء الاصطناعي"
              : "Translate with AI"}
        </button>
      </div>

      {blockerBanner ? (
        <div className="studio-alert-warn" role="status" data-testid="ai-blocker-banner">
          <strong>{blockerBanner.code}</strong>: {blockerBanner.message}
        </div>
      ) : null}

      {aiSummary ? (
        <div className="studio-alert-info" role="status">
          {aiSummary}
        </div>
      ) : null}

      <div className="studio-dsl-area">
        <div className="studio-dsl-header">
          <span>
            {isAr
              ? "أمر DSL المهيكل (يخضع للتحقق الصارم قبل التنفيذ)"
              : "Structured Studio DSL (strictly validated before execution)"}
          </span>
          <div className="studio-quick-templates">
            {SAMPLE_DSL_COMMANDS.map((tpl) => (
              <button
                key={tpl.labelEn}
                type="button"
                className="studio-chip-btn"
                disabled={!projectLoaded}
                onClick={() => setDslInput(tpl.json)}
              >
                {isAr ? tpl.labelAr : tpl.labelEn}
              </button>
            ))}
          </div>
        </div>
        <textarea
          className="studio-dsl-editor"
          rows={5}
          value={dslInput}
          disabled={!projectLoaded || isProcessing}
          onChange={(e) => setDslInput(e.target.value)}
          placeholder={
            isAr
              ? '{"op":"define_metric","id":"rev","name":"Revenue","unit":"EGP","formula":"SUM(amount)"}'
              : '{"op":"define_metric","id":"rev","name":"Revenue","unit":"EGP","formula":"SUM(amount)"}'
          }
          aria-label={isAr ? "محرر أوامر DSL" : "Structured DSL JSON editor"}
        />
      </div>

      {dslValidation && !dslValidation.ok ? (
        <div className="studio-alert-error" role="alert">
          <strong>{dslValidation.code}</strong>: {dslValidation.reason}
        </div>
      ) : null}

      {proposedPreview ? (
        <div className="studio-preview-box" data-testid="dsl-action-preview">
          <div className="studio-preview-title">
            {isAr ? "معاينة الإجراء المقترح قبل التنفيذ:" : "Proposed action preview (unapplied):"}
          </div>
          <div>{proposedPreview}</div>
        </div>
      ) : null}

      <div className="studio-command-actions">
        <button
          type="button"
          className="studio-btn-primary"
          disabled={!projectLoaded || isProcessing || !dslValidation || !dslValidation.ok}
          onClick={() => {
            void handleApproveAndExecute();
          }}
        >
          {isAr ? "اعتماد وتنفيذ في Rust" : "Approve & Execute in Rust"}
        </button>
        {dslInput ? (
          <button
            type="button"
            className="studio-btn-secondary"
            disabled={isProcessing}
            onClick={() => {
              setDslInput("");
              setExecStatus(null);
              setExecError(null);
            }}
          >
            {isAr ? "مسح المسودة" : "Clear Draft"}
          </button>
        ) : null}
      </div>

      {execStatus ? (
        <div className="studio-alert-success" role="status">
          {execStatus}
        </div>
      ) : null}
      {execError ? (
        <div className="studio-alert-error" role="alert">
          {execError}
        </div>
      ) : null}
    </section>
  );
}
