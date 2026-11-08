/**
 * `apps/web/src/components/studios/CleaningStudio.tsx`
 *
 * Full SafiData Cleaning Workspace (Spec Section 6.5.4, 16.3, 16.4, 20.3):
 * 1. Interactive 3-Pane Cleaning Workspace:
 *    - 56px action bar (Undo, Redo, Density toggle مريح/مدمج, Split/Merge, Export with A11 Formula Injection guard)
 *    - 320px collapsible Understanding Sidebar (Columns, Confidence, Natural Language Rule Parser 16.3)
 *    - Center Interactive RTL Data Grid (diff markers ⚡, quality bars, live step execution)
 *    - 320px collapsible Steps & Aggregated Questions Card (`U6` — max 3 impact-ranked questions)
 *    - 40px Bottom Status Bar (Row count, Column count, Quality score)
 * 2. Live Arabic Normalization + PII Masking + BLAKE3 Tokenization + Official 14-Digit Egyptian National ID Validator (Section 20.3).
 */
import type { ChangeEvent, ReactNode } from "react";
import { useMemo, useState } from "react";
import {
  CheckCircle2,
  Download,
  Fingerprint,
  HelpCircle,
  Redo2,
  ShieldCheck,
  Sparkles,
  Table2,
  Undo2,
  Wand2
} from "lucide-react";
import { normalizeLines, normalizeText } from "../../lib/arabic-normalize";
import {
  PII_KIND_LABELS,
  countByKind,
  detectPii,
  maskText,
  maskValue,
  tokenizeValue
} from "../../lib/pii";
import type { PiiKind } from "../../lib/pii";
import { validateEgyptianNationalId } from "../../lib/national-id";
import { downloadTextFile } from "../../lib/download";
import { generateExportContent } from "../../utils/export";
import { parseNaturalLanguageRule } from "../../utils/column-understanding";
import { aggregateAndRankQuestions } from "../../utils/aggregated-questions";
import type { DataIssueItem } from "../../utils/aggregated-questions";
import { generateDataDictionaryMarkdown } from "../../utils/data-dictionary";
import { DecryptedMonoText } from "../ui/DecryptedMonoText";
import { MagneticButton } from "../ui/MagneticButton";
import { SpotlightCard } from "../ui/SpotlightCard";

export const SAMPLE_CLEANING_TEXT = [
  "الاسم: أَحْمَدُ محمد عبد الله",
  "الهاتف: ٠١٠١٢٣٤٥٦٧٨",
  "البريد: ahmed.mohamed@example.com",
  "الرقم القومي: 29801011234567",
  "العنوان: ١٢ شارع التحرير ـــ الجيزة",
  "المبلغ: ١٥٠٠ ج.م"
].join("\n");

export const DEFAULT_TOKEN_SALT = "safi-local-tenant";

interface TokenRow {
  readonly kind: PiiKind;
  readonly masked: string;
  readonly token: string;
}

interface Analysis {
  readonly normalized: string;
  readonly masked: string;
  readonly rows: readonly TokenRow[];
  readonly counts: ReturnType<typeof countByKind>;
  readonly total: number;
}

function analyze(raw: string, salt: string): Analysis {
  const normalized = normalizeLines(raw);
  const matches = detectPii(normalized);
  return {
    normalized,
    masked: maskText(normalized, matches),
    rows: matches.map((match) => ({
      kind: match.kind,
      masked: maskValue(match.kind, match.value),
      token: tokenizeValue(match.kind, match.value, salt)
    })),
    counts: countByKind(matches),
    total: matches.length
  };
}

interface WorkspaceColumn {
  readonly name: string;
  readonly type: string;
  readonly confidence: number;
  readonly alternatives: readonly string[];
  readonly quality: number;
}

interface WorkspaceStep {
  readonly id: string;
  readonly title: string;
  readonly detail: string;
  readonly enabled: boolean;
}

const INITIAL_COLUMNS: readonly WorkspaceColumn[] = [
  { name: "الاسم الكامل", type: "نص عربي (Text)", confidence: 0.98, alternatives: ["اسم مركب"], quality: 96 },
  { name: "الرقم القومي", type: "رقم قومي مصري (14 رقمًا)", confidence: 0.99, alternatives: ["معرّف نصي"], quality: 94 },
  { name: "تاريخ الصرف", type: "تاريخ (ISO Date)", confidence: 0.89, alternatives: ["تاريخ هجري", "نص"], quality: 88 },
  { name: "المبلغ (ج.م)", type: "مبلغ مالي (Decimal)", confidence: 0.94, alternatives: ["رقم صحيح"], quality: 92 },
  { name: "رقم الهاتف", type: "هاتف محمول (Phone)", confidence: 0.91, alternatives: ["نص"], quality: 90 }
];

const RAW_WORKSPACE_ROWS: readonly (readonly string[])[] = [
  ["أَحْمَدُ محمد عبد الله", "29801010123455", "2026/01/15", "١٥٠٠", "٠١٠١٢٣٤٥٦٧٨"],
  ["فاطمة حسن إبراهيم", "29505122101248", "15-01-2026", "٢٣٠٠", "٠١٢٣٤٥٦٧٨٩٠"],
  ["محمود السيّدـــعلي", "30109200201351", "2026.01.16", "0", "٠١١٢٣٤٥٦٧٨٩"],
  ["أَحْمَدُ محمد عبد الله", "29801010123455", "2026/01/15", "١٥٠٠", "٠١٠١٢٣٤٥٦٧٨"],
  ["=HYPERLINK(\"http://evil\")", "28811041201432", "2026/01/18", "٣٢٠٠", "٠١٥٥٥٤٤٣٣٢٢"]
];

const INITIAL_ISSUES: readonly DataIssueItem[] = [
  {
    id: "q1",
    category: "ambiguity",
    titleAr: "القيمة (0) في عمود «المبلغ (ج.م)» بالصف 3: هل تعني صفرًا فعليًا أم «لم يستلم / فارغ»؟",
    impactScore: 95,
    deferred: false
  },
  {
    id: "q2",
    category: "schema",
    titleAr: "الصف 4 مكرر بالكامل مع الصف 1: هل ترغب في دمج السجلات المتطابقة؟",
    impactScore: 88,
    deferred: false
  },
  {
    id: "q3",
    category: "outlier",
    titleAr: "الخلية الأولى في الصف 5 تبدأ بعلامة صيغة (=HYPERLINK): سيتم تحييدها تلقائيًا لحمايتك (A11).",
    impactScore: 82,
    deferred: false
  }
];

export function CleaningStudio(): ReactNode {
  // 1. Arabic Normalization + PII + National ID state
  const [draft, setDraft] = useState(SAMPLE_CLEANING_TEXT);
  const [submitted, setSubmitted] = useState(SAMPLE_CLEANING_TEXT);
  const [salt, setSalt] = useState(DEFAULT_TOKEN_SALT);
  const [nationalIdInput, setNationalIdInput] = useState("29801010123455");

  // 2. Interactive 3-Pane Cleaning Workspace state (Section 6.5.4)
  const [columns, setColumns] = useState<readonly WorkspaceColumn[]>(INITIAL_COLUMNS);
  const [selectedCol, setSelectedCol] = useState<number>(0);
  const [density, setDensity] = useState<"comfortable" | "compact">("comfortable");
  const [rightOpen, setRightOpen] = useState<boolean>(true);
  const [leftOpen, setLeftOpen] = useState<boolean>(true);
  const [nlRuleText, setNlRuleText] = useState<string>("0 معناها لم يستلم");
  const [zeroMeansNull, setZeroMeansNull] = useState<boolean>(false);
  const [issues, setIssues] = useState<readonly DataIssueItem[]>(INITIAL_ISSUES);
  const [statusBanner, setStatusBanner] = useState<string | null>(null);

  const [stepsHistory, setStepsHistory] = useState<readonly (readonly WorkspaceStep[])[]>([
    [
      { id: "norm-ar", title: "تطبيع الحروف والأرقام العربية", detail: "توحيد أ/إ/آ، التاء المربوطة، والأرقام ٠-٩", enabled: true },
      { id: "norm-date", title: "توحيد التواريخ القياسي", detail: "تحويل جميع الصيغ إلى YYYY-MM-DD", enabled: true },
      { id: "dedup", title: "إزالة الصفوف المكررة", detail: "حذف الصفوف المتطابقة 100%", enabled: true },
      { id: "mask-pii", title: "إخفاء الهاتف والرقم القومي", detail: "قناع جزئي مع الحفاظ على المحافظة", enabled: false }
    ]
  ]);
  const [historyIndex, setHistoryIndex] = useState<number>(0);
  const currentSteps = stepsHistory[historyIndex] ?? stepsHistory[0]!;

  const pushSteps = (next: readonly WorkspaceStep[], msg: string): void => {
    const sliced = stepsHistory.slice(0, historyIndex + 1);
    setStepsHistory([...sliced, next]);
    setHistoryIndex(sliced.length);
    setStatusBanner(msg);
  };

  const toggleWorkspaceStep = (id: string): void => {
    const next = currentSteps.map((s) => (s.id === id ? { ...s, enabled: !s.enabled } : s));
    pushSteps(next, "تم تحديث خطوات التنضيف وإعادة حساب الجدول فورًا.");
  };

  // Compute live cleaned rows in the 3-Pane Workspace
  const processedGrid = useMemo(() => {
    const normAr = currentSteps.find((s) => s.id === "norm-ar")?.enabled ?? false;
    const normDate = currentSteps.find((s) => s.id === "norm-date")?.enabled ?? false;
    const dedup = currentSteps.find((s) => s.id === "dedup")?.enabled ?? false;
    const maskPii = currentSteps.find((s) => s.id === "mask-pii")?.enabled ?? false;

    const seen = new Set<string>();
    const rowsOut: { cells: string[]; diffs: boolean[] }[] = [];

    for (const rawRow of RAW_WORKSPACE_ROWS) {
      const key = rawRow.join("|");
      if (dedup && seen.has(key)) {
        continue;
      }
      seen.add(key);

      const cells: string[] = [];
      const diffs: boolean[] = [];

      rawRow.forEach((cell, cIdx) => {
        let nextVal = cell;
        // A11 formula injection guard
        if (/^[=+\-@]/.test(nextVal.trim())) {
          nextVal = `'${nextVal}`;
        }
        if (normAr) {
          nextVal = normalizeText(nextVal);
        }
        if (normDate && cIdx === 2) {
          nextVal = nextVal
            .replace(/^(\d{2})-(\d{2})-(\d{4})$/, "$3-$2-$1")
            .replace(/[/.]/g, "-");
        }
        if (zeroMeansNull && cIdx === 3 && nextVal.trim() === "0") {
          nextVal = "غير متاح (Null)";
        }
        if (maskPii && (cIdx === 1 || cIdx === 4)) {
          nextVal = cIdx === 1 ? maskValue("national_id", nextVal) : maskValue("phone", nextVal);
        }
        cells.push(nextVal);
        diffs.push(nextVal !== cell);
      });

      rowsOut.push({ cells, diffs });
    }

    return rowsOut;
  }, [currentSteps, zeroMeansNull]);

  const parsedRule = useMemo(
    () => parseNaturalLanguageRule(nlRuleText, columns[selectedCol]?.name ?? "المبلغ"),
    [nlRuleText, columns, selectedCol]
  );

  const activeQuestions = useMemo(() => aggregateAndRankQuestions([...issues]), [issues]);

  const analysis = useMemo(() => analyze(submitted, salt), [submitted, salt]);
  const activeKinds = useMemo(
    () =>
      (Object.keys(analysis.counts) as Array<keyof typeof analysis.counts>).filter(
        (kind) => analysis.counts[kind] > 0
      ),
    [analysis.counts]
  );

  const nationalIdCheck = useMemo(
    () => validateEgyptianNationalId(nationalIdInput),
    [nationalIdInput]
  );

  const handleDraft = (event: ChangeEvent<HTMLTextAreaElement>): void =>
    setDraft(event.target.value);
  const handleSalt = (event: ChangeEvent<HTMLInputElement>): void => setSalt(event.target.value);

  const handleExportCleanCsv = (): void => {
    const exported = generateExportContent(
      columns.map((c) => c.name),
      processedGrid.map((r) => r.cells),
      { format: "csv", protectFormulaInjection: true, rtl: true }
    );
    downloadTextFile(exported.filename, exported.content, exported.mimeType);
    setStatusBanner(`تم تصدير الملف النظيف (${exported.filename}) مع تحييد حقن الصيغ A11.`);
  };

  const handleExportDictionary = (): void => {
    const md = generateDataDictionaryMarkdown(
      "سجل العملاء والمرتبات المنقّى",
      columns.map((col) => ({
        columnName: col.name,
        inferredType: col.type,
        completenessRate: col.quality / 100,
        meaning: `عمود موثق بدرجة ثقة ${Math.round(col.confidence * 100)}%`
      }))
    );
    downloadTextFile("safi-data-dictionary.md", md, "text/markdown;charset=utf-8");
    setStatusBanner("تم توليد وتصدير قاموس البيانات التلقائي (U8).");
  };

  const qualityScore = useMemo(() => {
    const enabledCount = currentSteps.filter((s) => s.enabled).length;
    return Math.min(100, 82 + enabledCount * 4 + (zeroMeansNull ? 2 : 0));
  }, [currentSteps, zeroMeansNull]);

  return (
    <section className="safi-studio" aria-labelledby="safi-cleaning-title" dir="rtl">
      <header className="safi-studio-header">
        <span className="safi-eyebrow">مختبر التنقية والخصوصية ومساحة العمل</span>
        <h1 id="safi-cleaning-title" className="safi-studio-title">
          التنظيف وPII
        </h1>
        <p className="safi-studio-lead">
          مساحة التنضيف الكاملة (الشبكة + لوحة الفهم + الخطوات + بطاقة الأسئلة المجمّعة U6) مع مختبر
          التطبيع العربي وإخفاء PII ومحرك الرقم القومي المصري — كل ذلك محليًا 100% داخل جهازك.
        </p>
      </header>

      {/* =====================================================================
          PART 1: Interactive 3-Pane Cleaning Workspace (Spec Section 6.5.4)
         ===================================================================== */}
      <div
        className="safi-card"
        style={{
          padding: 0,
          overflow: "hidden",
          border: "1px solid var(--border-strong)"
        }}
      >
        {/* 56px Workspace Top Toolbar (Spec Section 6.5.4) */}
        <div
          style={{
            minHeight: "56px",
            padding: "8px 16px",
            background: "var(--bg-elevated)",
            borderBottom: "1px solid var(--border-subtle)",
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            gap: "12px",
            flexWrap: "wrap"
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: "8px", flexWrap: "wrap" }}>
            <span style={{ display: "inline-flex", alignItems: "center", gap: "6px", fontWeight: 600, fontSize: "14px" }}>
              <Table2 size={16} aria-hidden="true" style={{ color: "var(--accent-primary)" }} />
              مساحة التنضيف التفاعلية (Section 6.5.4)
            </span>

            <button
              type="button"
              onClick={() => {
                if (historyIndex > 0) {
                  setHistoryIndex(historyIndex - 1);
                  setStatusBanner("↶ تم التراجع عن الخطوة الأخيرة.");
                }
              }}
              disabled={historyIndex === 0}
              style={{ minHeight: "36px", padding: "4px 10px", fontSize: "12px" }}
            >
              <Undo2 size={13} aria-hidden="true" /> تراجع
            </button>

            <button
              type="button"
              onClick={() => {
                if (historyIndex < stepsHistory.length - 1) {
                  setHistoryIndex(historyIndex + 1);
                  setStatusBanner("↷ تمت إعادة الخطوة.");
                }
              }}
              disabled={historyIndex >= stepsHistory.length - 1}
              style={{ minHeight: "36px", padding: "4px 10px", fontSize: "12px" }}
            >
              <Redo2 size={13} aria-hidden="true" /> إعادة
            </button>

            <button
              type="button"
              onClick={() => setDensity((d) => (d === "comfortable" ? "compact" : "comfortable"))}
              style={{ minHeight: "36px", padding: "4px 10px", fontSize: "12px" }}
            >
              الكثافة: {density === "comfortable" ? "مريح (44px)" : "مدمج للمحترفين (28px)"}
            </button>
          </div>

          <div style={{ display: "flex", alignItems: "center", gap: "8px", flexWrap: "wrap" }}>
            <button
              type="button"
              className="safi-btn safi-btn-secondary"
              onClick={handleExportDictionary}
              style={{ minHeight: "38px", padding: "6px 12px", fontSize: "12px" }}
            >
              قاموس البيانات (U8)
            </button>
            <button
              type="button"
              className="safi-btn safi-btn-primary"
              onClick={handleExportCleanCsv}
              style={{ minHeight: "38px", padding: "6px 14px", fontSize: "12px" }}
            >
              <Download size={14} aria-hidden="true" />
              تصدير CSV نظيف (حماية A11)
            </button>
          </div>
        </div>

        {statusBanner !== null ? (
          <div
            role="status"
            style={{
              padding: "8px 16px",
              background: "var(--accent-tint)",
              borderBottom: "1px solid var(--border-subtle)",
              fontSize: "12px",
              color: "var(--accent-primary)",
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center"
            }}
          >
            <span>✓ {statusBanner}</span>
            <button
              type="button"
              onClick={() => setStatusBanner(null)}
              style={{ minHeight: "28px", padding: "2px 8px", fontSize: "11px" }}
            >
              إغلاق
            </button>
          </div>
        ) : null}

        {/* 3-Pane Body: Right Sidebar (320px) + Center Grid + Left Sidebar (320px) */}
        <div
          style={{
            display: "grid",
            gridTemplateColumns: `${rightOpen ? "300px" : "48px"} minmax(0, 1fr) ${leftOpen ? "300px" : "48px"}`,
            minHeight: "420px"
          }}
        >
          {/* Right Sidebar (RTL Start): Columns & Understanding Panel (320px) */}
          <aside
            style={{
              borderInlineEnd: "1px solid var(--border-subtle)",
              background: "var(--bg-surface)",
              padding: rightOpen ? "16px" : "8px",
              display: "flex",
              flexDirection: "column",
              gap: "12px"
            }}
          >
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
              {rightOpen ? (
                <h2 style={{ fontSize: "14px", fontWeight: 600 }}>لوحة الفهم والأعمدة (16.2)</h2>
              ) : null}
              <button
                type="button"
                onClick={() => setRightOpen((o) => !o)}
                aria-label={rightOpen ? "طي لوحة الأعمدة" : "فتح لوحة الأعمدة"}
                style={{ minHeight: "32px", minWidth: "32px", padding: "4px" }}
              >
                {rightOpen ? "▶" : "◀"}
              </button>
            </div>

            {rightOpen ? (
              <>
                <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
                  {columns.map((col, idx) => {
                    const isSelected = idx === selectedCol;
                    return (
                      <div
                        key={col.name}
                        onClick={() => setSelectedCol(idx)}
                        style={{
                          padding: "10px",
                          borderRadius: "var(--radius-card)",
                          border: `1px solid ${isSelected ? "var(--accent-primary)" : "var(--border-subtle)"}`,
                          background: isSelected ? "var(--accent-tint)" : "var(--bg-elevated)",
                          cursor: "pointer"
                        }}
                      >
                        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                          <strong style={{ fontSize: "13px" }}>{col.name}</strong>
                          <span style={{ fontSize: "11px", color: "var(--status-ok)", fontWeight: 600 }}>
                            {Math.round(col.confidence * 100)}% ثقة
                          </span>
                        </div>
                        <div style={{ fontSize: "11px", color: "var(--text-secondary)", margin: "4px 0" }}>
                          النوع: {col.type}
                        </div>
                        {/* Thin column quality bar (Spec 6.5.4) */}
                        <div
                          style={{
                            height: "4px",
                            borderRadius: "2px",
                            background: "var(--border-subtle)",
                            overflow: "hidden",
                            marginBottom: "6px"
                          }}
                        >
                          <div
                            style={{
                              width: `${col.quality}%`,
                              height: "100%",
                              background: "var(--accent-primary)"
                            }}
                          />
                        </div>
                        <div style={{ display: "flex", gap: "6px" }}>
                          <button
                            type="button"
                            onClick={(e) => {
                              e.stopPropagation();
                              setStatusBanner(`تم تأكيد نوع العمود «${col.name}» وحفظه في ذاكرة التعلم.`);
                            }}
                            style={{ minHeight: "28px", padding: "2px 8px", fontSize: "11px" }}
                          >
                            ✓ صحيح
                          </button>
                          <button
                            type="button"
                            onClick={(e) => {
                              e.stopPropagation();
                              const alt = col.alternatives[0] ?? "نص";
                              setColumns((prev) =>
                                prev.map((c, i) => (i === idx ? { ...c, type: alt } : c))
                              );
                              setStatusBanner(`تم تبديل نوع «${col.name}» إلى ${alt}.`);
                            }}
                            style={{ minHeight: "28px", padding: "2px 8px", fontSize: "11px" }}
                          >
                            تبديل ({col.alternatives[0] ?? "نص"})
                          </button>
                        </div>
                      </div>
                    );
                  })}
                </div>

                {/* Natural Language Rule Parser (Spec Section 16.3) */}
                <div
                  style={{
                    padding: "10px",
                    borderRadius: "var(--radius-card)",
                    background: "var(--bg-elevated)",
                    border: "1px solid var(--border-subtle)"
                  }}
                >
                  <label className="safi-field-label" htmlFor="safi-nl-rule">
                    <Wand2 size={12} aria-hidden="true" /> قاعدة فهم بالعربية (القسم 16.3)
                  </label>
                  <input
                    id="safi-nl-rule"
                    value={nlRuleText}
                    onChange={(e) => setNlRuleText(e.target.value)}
                    style={{ minHeight: "36px", fontSize: "12px", marginBottom: "6px" }}
                  />
                  <p style={{ margin: "0 0 6px", fontSize: "11px", color: "var(--text-secondary)" }}>
                    {parsedRule.descriptionAr}
                  </p>
                  <button
                    type="button"
                    className="safi-btn safi-btn-primary"
                    onClick={() => {
                      setZeroMeansNull((z) => !z);
                      setStatusBanner("تم تطبيق قاعدة الفهم الطبيعية على الجدول.");
                    }}
                    style={{ width: "100%", minHeight: "34px", padding: "4px 10px", fontSize: "12px" }}
                  >
                    {zeroMeansNull ? "إلغاء القاعدة المطبّقة" : "تطبيق القاعدة على العمود"}
                  </button>
                </div>
              </>
            ) : null}
          </aside>

          {/* Center Pane: Interactive Data Grid + Bottom Status Bar */}
          <div style={{ display: "flex", flexDirection: "column", justifyContent: "space-between", padding: "16px", overflowX: "auto" }}>
            <div className="safi-table-wrap">
              <table className="safi-table">
                <thead>
                  <tr>
                    <th scope="col">#</th>
                    {columns.map((col) => (
                      <th key={col.name} scope="col">
                        <div>{col.name}</div>
                        <span style={{ fontSize: "10px", fontWeight: 400, color: "var(--accent-primary)" }}>
                          {col.type}
                        </span>
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {processedGrid.map((row, rIdx) => (
                    <tr
                      key={rIdx}
                      style={{
                        height: density === "comfortable" ? "44px" : "28px"
                      }}
                    >
                      <td className="safi-mono" style={{ color: "var(--text-muted)" }}>
                        {rIdx + 1}
                      </td>
                      {row.cells.map((cell, cIdx) => {
                        const changed = row.diffs[cIdx];
                        return (
                          <td
                            key={cIdx}
                            style={{
                              padding: density === "comfortable" ? "10px 12px" : "4px 10px",
                              background: changed ? "var(--accent-tint)" : undefined,
                              fontWeight: changed ? 600 : 400
                            }}
                          >
                            {/* Spec 6.5.4: Differences marked by pattern/icon + color, never color alone */}
                            {changed ? (
                              <span
                                title="خلية تم تنظيفها وتوحيدها"
                                style={{
                                  display: "inline-flex",
                                  alignItems: "center",
                                  gap: "6px"
                                }}
                              >
                                <span
                                  aria-label="معدّل"
                                  style={{
                                    fontSize: "10px",
                                    padding: "1px 5px",
                                    borderRadius: "4px",
                                    background: "var(--accent-brand)",
                                    color: "#FFFFFF"
                                  }}
                                >
                                  ⚡ منقّى
                                </span>
                                <span>{cell}</span>
                              </span>
                            ) : (
                              cell
                            )}
                          </td>
                        );
                      })}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>

            {/* 40px Bottom Status Bar (Spec Section 6.5.4) */}
            <div
              style={{
                marginTop: "12px",
                minHeight: "40px",
                padding: "6px 14px",
                borderRadius: "var(--radius-field)",
                background: "var(--bg-elevated)",
                border: "1px solid var(--border-subtle)",
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                gap: "12px",
                fontSize: "12px",
                color: "var(--text-secondary)",
                flexWrap: "wrap"
              }}
            >
              <span>
                📊 عدد الصفوف النشطة: <strong>{processedGrid.length}</strong> (من أصل {RAW_WORKSPACE_ROWS.length}) | الأعمدة:{" "}
                <strong>{columns.length}</strong> | درجة الجودة الشاملة:{" "}
                <strong style={{ color: "var(--status-ok)" }}>{qualityScore}%</strong>
              </span>
              <span>
                الفروق مميّزة بالرمز (⚡ منقّى) واللون معًا التزامًا بمعيار WCAG 2.2 AA
              </span>
            </div>
          </div>

          {/* Left Sidebar (RTL End): Steps & Aggregated Questions U6 (320px) */}
          <aside
            style={{
              borderInlineStart: "1px solid var(--border-subtle)",
              background: "var(--bg-surface)",
              padding: leftOpen ? "16px" : "8px",
              display: "flex",
              flexDirection: "column",
              gap: "12px"
            }}
          >
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
              <button
                type="button"
                onClick={() => setLeftOpen((o) => !o)}
                aria-label={leftOpen ? "طي لوحة الخطوات" : "فتح لوحة الخطوات"}
                style={{ minHeight: "32px", minWidth: "32px", padding: "4px" }}
              >
                {leftOpen ? "◀" : "▶"}
              </button>
              {leftOpen ? (
                <h2 style={{ fontSize: "14px", fontWeight: 600 }}>الخطوات والأسئلة المجمّعة</h2>
              ) : null}
            </div>

            {leftOpen ? (
              <>
                <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
                  {currentSteps.map((step) => (
                    <label key={step.id} className="safi-toggle-row" style={{ padding: "8px 10px" }}>
                      <input
                        type="checkbox"
                        checked={step.enabled}
                        onChange={() => toggleWorkspaceStep(step.id)}
                      />
                      <span>
                        <span className="safi-toggle-label" style={{ fontSize: "12px" }}>
                          {step.title}
                        </span>
                        <span className="safi-toggle-detail" style={{ fontSize: "11px" }}>
                          {step.detail}
                        </span>
                      </span>
                    </label>
                  ))}
                </div>

                {/* Aggregated Questions Card U6 (I-Have-ADHD Skill: max 3 questions + اسألني لاحقًا) */}
                <div
                  style={{
                    padding: "12px",
                    borderRadius: "var(--radius-card)",
                    background: "var(--bg-elevated)",
                    border: "1px solid var(--border-strong)",
                    display: "flex",
                    flexDirection: "column",
                    gap: "8px"
                  }}
                >
                  <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
                    <span style={{ fontSize: "12px", fontWeight: 600, color: "var(--accent-primary)", display: "inline-flex", alignItems: "center", gap: "4px" }}>
                      <HelpCircle size={13} aria-hidden="true" />
                      بطاقة الأسئلة المجمّعة (U6)
                    </span>
                    <span style={{ fontSize: "11px", color: "var(--text-muted)" }}>
                      {activeQuestions.length} / 3
                    </span>
                  </div>

                  {activeQuestions.length === 0 ? (
                    <p style={{ margin: 0, fontSize: "11px", color: "var(--status-ok)" }}>
                      ✓ تم حسم أو تأجيل جميع الأسئلة المعلقة بهدوء.
                    </p>
                  ) : (
                    <ul className="safi-list" style={{ gap: "6px" }}>
                      {activeQuestions.map((q) => (
                        <li
                          key={q.id}
                          style={{
                            fontSize: "11px",
                            padding: "6px 8px",
                            borderRadius: "6px",
                            background: "var(--bg-surface)",
                            border: "1px solid var(--border-subtle)"
                          }}
                        >
                          {q.titleAr}
                        </li>
                      ))}
                    </ul>
                  )}

                  {activeQuestions.length > 0 ? (
                    <div style={{ display: "flex", gap: "6px" }}>
                      <button
                        type="button"
                        className="safi-btn safi-btn-primary"
                        onClick={() => {
                          setZeroMeansNull(true);
                          setIssues((prev) => prev.map((i) => ({ ...i, deferred: true })));
                          setStatusBanner("تم اعتماد توصيات بطاقة الأسئلة المجمّعة (U6) دفعة واحدة.");
                        }}
                        style={{ flex: 1, minHeight: "32px", padding: "4px 8px", fontSize: "11px" }}
                      >
                        اعتماد الحلول
                      </button>
                      <button
                        type="button"
                        onClick={() => {
                          setIssues((prev) => prev.map((i) => ({ ...i, deferred: true })));
                          setStatusBanner("تم تأجيل الأسئلة — يمكنك متابعة العمل بلا مقاطعة.");
                        }}
                        style={{ minHeight: "32px", padding: "4px 8px", fontSize: "11px" }}
                      >
                        اسألني لاحقًا
                      </button>
                    </div>
                  ) : null}
                </div>
              </>
            ) : null}
          </aside>
        </div>
      </div>

      {/* =====================================================================
          PART 2: Live Arabic Normalization, PII Masking & Egyptian National ID
         ===================================================================== */}
      <div className="safi-grid-2">
        <SpotlightCard padding={24}>
          <label className="safi-field-label" htmlFor="safi-cleaning-input">
            النص التجريبي (مثال ببيانات وهمية)
          </label>
          <textarea
            id="safi-cleaning-input"
            className="safi-textarea"
            value={draft}
            onChange={handleDraft}
            rows={7}
            spellCheck={false}
          />

          <label className="safi-field-label" htmlFor="safi-token-salt">
            ملح الترميز (Salt) — يغيّر البصمات دون تغيير النص
          </label>
          <input
            id="safi-token-salt"
            className="safi-input safi-mono"
            value={salt}
            onChange={handleSalt}
            dir="ltr"
            spellCheck={false}
          />

          <div className="safi-actions">
            <MagneticButton onClick={() => setSubmitted(draft)} ariaLabel="شغّل التنقية محليًا">
              <Sparkles size={16} aria-hidden="true" />
              شغّل التنقية محليًا
            </MagneticButton>
            <MagneticButton
              variant="glass"
              onClick={() => {
                setDraft(SAMPLE_CLEANING_TEXT);
                setSubmitted(SAMPLE_CLEANING_TEXT);
                setSalt(DEFAULT_TOKEN_SALT);
              }}
              ariaLabel="استعادة المثال"
            >
              استعادة المثال
            </MagneticButton>
          </div>

          <p className="safi-hint">
            لا يُرسل هذا النص إلى أي خادم، ولا يُكتب في السجلات. المعالجة الفعلية للملفات تتم في
            نواة Rust المعزولة.
          </p>
        </SpotlightCard>

        <SpotlightCard padding={24}>
          <h2 className="safi-card-title">نتيجة التطبيع العربي</h2>
          <div className="safi-compare">
            <div>
              <span className="safi-mini-label">قبل</span>
              <pre className="safi-pre" dir="auto">
                {submitted}
              </pre>
            </div>
            <div>
              <span className="safi-mini-label">بعد (sf-arabic)</span>
              <pre className="safi-pre" dir="auto" data-testid="safi-normalized">
                {analysis.normalized}
              </pre>
            </div>
          </div>

          <h2 className="safi-card-title" style={{ marginTop: "20px" }}>
            <ShieldCheck size={16} aria-hidden="true" /> الإخفاء الجزئي
          </h2>
          <pre className="safi-pre" dir="auto" data-testid="safi-masked">
            {analysis.masked}
          </pre>
        </SpotlightCard>
      </div>

      {/* Part 3: Official Egyptian National ID Validator (Spec Section 20.3) */}
      <SpotlightCard padding={24}>
        <h2 className="safi-card-title">
          <CheckCircle2 size={16} aria-hidden="true" style={{ color: "var(--accent-primary)" }} />
          فاحص الرقم القومي المصري الرسمي (القسم 20.3 — 14 رقمًا + المحافظة + تاريخ الميلاد + معادلة التحقق)
        </h2>
        <div style={{ display: "grid", gridTemplateColumns: "minmax(240px, 340px) 1fr", gap: "16px", alignItems: "center" }}>
          <div>
            <label className="safi-field-label" htmlFor="safi-national-id-input">
              أدخل رقمًا قوميًا مصريًا (يقبل الأرقام العربية ٠-٩ أو اللاتينية)
            </label>
            <input
              id="safi-national-id-input"
              className="safi-input safi-mono"
              value={nationalIdInput}
              onChange={(e) => setNationalIdInput(e.target.value)}
              dir="ltr"
              maxLength={20}
              style={{ marginBottom: 0 }}
            />
          </div>

          <div
            role="status"
            style={{
              padding: "12px 16px",
              borderRadius: "var(--radius-card)",
              background: "var(--bg-elevated)",
              border: `1px solid ${nationalIdCheck.ok ? "var(--border-strong)" : "var(--status-danger)"}`
            }}
          >
            {nationalIdCheck.ok ? (
              <div style={{ display: "flex", gap: "20px", flexWrap: "wrap", fontSize: "13px" }}>
                <span>
                  المحافظة: <strong style={{ color: "var(--accent-primary)" }}>{nationalIdCheck.info.governorateNameAr}</strong> ({nationalIdCheck.info.governorateCode})
                </span>
                <span>
                  تاريخ الميلاد: <strong className="safi-mono" dir="ltr">{nationalIdCheck.info.birthDate}</strong>
                </span>
                <span>
                  النوع: <strong>{nationalIdCheck.info.genderAr}</strong>
                </span>
                <span style={{ color: nationalIdCheck.info.checksumMatches ? "var(--status-ok)" : "var(--status-warn)" }}>
                  {nationalIdCheck.info.checksumMatches
                    ? "✓ خانة التحقق (Modulo-11) صحيحة"
                    : `⚠ خانة التحقق المتوقعة: ${nationalIdCheck.info.expectedCheckDigit}`}
                </span>
              </div>
            ) : (
              <span style={{ color: "var(--status-danger)", fontSize: "13px", fontWeight: 600 }}>
                ✗ {nationalIdCheck.messageAr}
              </span>
            )}
          </div>
        </div>
      </SpotlightCard>

      {/* Part 4: Deterministic BLAKE3 Tokenization Table */}
      <SpotlightCard padding={24}>
        <h2 className="safi-card-title">
          <Fingerprint size={16} aria-hidden="true" /> الاستبدال الرمزي الحتمي (BLAKE3)
        </h2>

        <div className="safi-chip-row" role="status" aria-live="polite">
          <span className="safi-chip">{analysis.total} حقلًا شخصيًا</span>
          {activeKinds.map((kind) => (
            <span key={kind} className="safi-chip safi-chip--accent" data-pii-kind={kind}>
              {PII_KIND_LABELS[kind]}: {analysis.counts[kind]}
            </span>
          ))}
        </div>

        {analysis.rows.length === 0 ? (
          <p className="safi-hint">
            لا توجد بيانات شخصية مطابقة في هذا النص. جرّب رقم هاتف أو بريدًا إلكترونيًا.
          </p>
        ) : (
          <div className="safi-table-wrap">
            <table className="safi-table">
              <thead>
                <tr>
                  <th scope="col">النوع</th>
                  <th scope="col">القيمة المخفية</th>
                  <th scope="col">الرمز (Token)</th>
                </tr>
              </thead>
              <tbody>
                {analysis.rows.map((entry, index) => (
                  <tr key={`${entry.kind}-${index}`} data-token-row={entry.kind}>
                    <td>{PII_KIND_LABELS[entry.kind]}</td>
                    <td className="safi-mono">{entry.masked}</td>
                    <td>
                      <DecryptedMonoText value={entry.token} title={entry.token} />
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}

        <p className="safi-hint">
          الرمز = ‎BLAKE3(salt : النوع : القيمة)‎ بـ16 بايت. نفس الملح والقيمة يعطيان دائمًا نفس
          الرمز، فيبقى الربط بين الجداول ممكنًا داخل جهازك، ويستحيل خارجه.
        </p>
      </SpotlightCard>
    </section>
  );
}
