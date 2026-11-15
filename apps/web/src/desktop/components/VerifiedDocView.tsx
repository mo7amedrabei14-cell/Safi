import React, { useMemo } from "react";
import { ArrowDown, ArrowUp, Copy, FileText, Plus, Trash2 } from "lucide-react";
import type { DesktopMetric, StudioDocBlockArtifact } from "../studio-bridge";
import { NarrativeTextRenderer, listMissingMetricRefs } from "./NarrativeTextRenderer";

export interface VerifiedDocViewProps {
  readonly language: "en" | "ar";
  readonly metrics: readonly DesktopMetric[];
  readonly docBlocks: readonly StudioDocBlockArtifact[];
  readonly onUpdateDocBlocks: (next: readonly StudioDocBlockArtifact[]) => void;
  readonly onInspectMetric?: (metric: DesktopMetric) => void;
}

const BLOCK_KINDS: ReadonlyArray<{ kind: string; labelEn: string; labelAr: string }> = [
  { kind: "heading", labelEn: "Heading", labelAr: "عنوان" },
  { kind: "paragraph", labelEn: "Paragraph", labelAr: "فقرة" },
  { kind: "metric_callout", labelEn: "Metric Callout", labelAr: "بطاقة مؤشر" },
  { kind: "callout", labelEn: "Decision Box", labelAr: "صندوق قرار" }
];

export function VerifiedDocView({
  language,
  metrics,
  docBlocks,
  onUpdateDocBlocks,
  onInspectMetric
}: VerifiedDocViewProps): React.ReactElement {
  const isAr = language === "ar";
  const metricsById = useMemo(() => new Map(metrics.map((m) => [m.id, m])), [metrics]);
  const missingRefs = useMemo(
    () => docBlocks.flatMap((b) => listMissingMetricRefs(b.content, metricsById)),
    [docBlocks, metricsById]
  );

  function addBlock(kind: string): void {
    const first = metrics[0];
    const nextBlock: StudioDocBlockArtifact = {
      id: `doc_${Date.now()}_${docBlocks.length + 1}`,
      kind,
      title:
        kind === "callout"
          ? isAr
            ? "قرار تنفيذي"
            : "Decision"
          : isAr
            ? `كتلة ${docBlocks.length + 1}`
            : `Section ${docBlocks.length + 1}`,
      content:
        first && (kind === "metric_callout" || kind === "paragraph")
          ? isAr
            ? `النتيجة الموثقة: {{metric:${first.id}}}`
            : `Verified figure: {{metric:${first.id}}}`
          : isAr
            ? "اكتب التحليل هنا..."
            : "Write verified prose here...",
      metricId: first?.id ?? null,
      authoredBy: "user"
    };
    onUpdateDocBlocks([...docBlocks, nextBlock]);
  }

  function updateBlock(id: string, patch: Partial<StudioDocBlockArtifact>): void {
    onUpdateDocBlocks(docBlocks.map((b) => (b.id === id ? { ...b, ...patch } : b)));
  }

  function moveBlock(idx: number, delta: -1 | 1): void {
    const target = idx + delta;
    if (target < 0 || target >= docBlocks.length) {
      return;
    }
    const copy = [...docBlocks];
    const [item] = copy.splice(idx, 1);
    if (item) {
      copy.splice(target, 0, item);
      onUpdateDocBlocks(copy);
    }
  }

  function duplicateBlock(idx: number): void {
    const src = docBlocks[idx];
    if (!src) {
      return;
    }
    const copy = [...docBlocks];
    copy.splice(idx + 1, 0, {
      ...src,
      id: `doc_${Date.now()}_${copy.length + 1}`
    });
    onUpdateDocBlocks(copy);
  }

  return (
    <section className="studio-panel" data-testid="verified-doc-view">
      <div className="studio-panel-header">
        <div>
          <h3>{isAr ? "محرر المستندات الموثّقة" : "Smart Verified Document Editor"}</h3>
          <p className="studio-muted-note">
            {isAr
              ? "يميز المستند بين نص المستخدم ومسودات الذكاء الاصطناعي، ويمنع التصدير إذا بقي أي {{metric:id}} مفقودًا."
              : "Distinguishes user-authored prose from AI drafts and blocks export if any {{metric:id}} is unresolved."}
          </p>
        </div>
        <div className="studio-quick-templates">
          {BLOCK_KINDS.map((entry) => (
            <button
              key={entry.kind}
              type="button"
              className="studio-btn-secondary"
              onClick={() => addBlock(entry.kind)}
            >
              <Plus size={13} aria-hidden="true" /> {isAr ? entry.labelAr : entry.labelEn}
            </button>
          ))}
        </div>
      </div>

      {missingRefs.length > 0 ? (
        <div className="studio-alert-error" role="alert" data-testid="doc-export-blocker">
          {isAr
            ? `التصدير محظور: مراجع غير محلولة (${missingRefs.join(", ")})`
            : `Export blocked: unresolved metric references (${missingRefs.join(", ")})`}
        </div>
      ) : null}

      {docBlocks.length === 0 ? (
        <p className="studio-muted-note">
          <FileText size={14} aria-hidden="true" />{" "}
          {isAr
            ? "أضف كتل مستند (عنوان، فقرة، بطاقة مؤشر، صندوق قرار) من الأزرار أعلاه."
            : "Add document blocks above to build your verified narrative report."}
        </p>
      ) : (
        <div className="studio-doc-blocks">
          {docBlocks.map((block, idx) => (
            <article key={block.id} className="studio-subcard">
              <div className="studio-panel-header">
                <div className="studio-quick-templates">
                  <strong>{`${isAr ? "كتلة" : "Block"} #${idx + 1} (${block.kind})`}</strong>
                  <span
                    className={
                      block.authoredBy === "ai" ? "studio-badge-warn" : "studio-badge-muted"
                    }
                  >
                    {block.authoredBy === "ai"
                      ? isAr
                        ? "مسودة ذكاء اصطناعي"
                        : "AI-Assisted Draft"
                      : isAr
                        ? "تحرير بشري"
                        : "User Prose"}
                  </span>
                </div>
                <div className="studio-quick-templates">
                  <button
                    type="button"
                    className="studio-chip-btn"
                    aria-label={isAr ? "تبديل مصدر الكتلة" : "Toggle authorship attribution"}
                    onClick={() =>
                      updateBlock(block.id, {
                        authoredBy: block.authoredBy === "ai" ? "user" : "ai"
                      })
                    }
                  >
                    {isAr ? "المصدر" : "Author"}
                  </button>
                  <button
                    type="button"
                    className="studio-chip-btn"
                    aria-label={isAr ? "نقل للأعلى" : "Move block up"}
                    disabled={idx === 0}
                    onClick={() => moveBlock(idx, -1)}
                  >
                    <ArrowUp size={13} aria-hidden="true" />
                  </button>
                  <button
                    type="button"
                    className="studio-chip-btn"
                    aria-label={isAr ? "نقل للأسفل" : "Move block down"}
                    disabled={idx === docBlocks.length - 1}
                    onClick={() => moveBlock(idx, 1)}
                  >
                    <ArrowDown size={13} aria-hidden="true" />
                  </button>
                  <button
                    type="button"
                    className="studio-chip-btn"
                    aria-label={isAr ? "تكرار الكتلة" : "Duplicate block"}
                    onClick={() => duplicateBlock(idx)}
                  >
                    <Copy size={13} aria-hidden="true" />
                  </button>
                  <button
                    type="button"
                    className="studio-chip-btn"
                    aria-label={isAr ? "حذف الكتلة" : "Delete block"}
                    onClick={() => onUpdateDocBlocks(docBlocks.filter((b) => b.id !== block.id))}
                  >
                    <Trash2 size={13} aria-hidden="true" />
                  </button>
                </div>
              </div>

              <div className="studio-inline-fields">
                <label>
                  <span>{isAr ? "العنوان" : "Title"}</span>
                  <input
                    value={block.title}
                    onChange={(e) => updateBlock(block.id, { title: e.target.value })}
                  />
                </label>
                {metrics.length > 0 ? (
                  <label>
                    <span>{isAr ? "إدراج مرجع مؤشر" : "Insert Metric Token"}</span>
                    <select
                      value=""
                      onChange={(e) => {
                        if (e.target.value) {
                          updateBlock(block.id, {
                            content: `${block.content} {{metric:${e.target.value}}}`.trim()
                          });
                        }
                      }}
                    >
                      <option value="">{isAr ? "+ اختر مؤشرًا" : "+ Insert metric..."}</option>
                      {metrics.map((m) => (
                        <option key={m.id} value={m.id}>
                          {`${m.name} ({{metric:${m.id}}})`}
                        </option>
                      ))}
                    </select>
                  </label>
                ) : null}
              </div>

              <textarea
                rows={3}
                value={block.content}
                onChange={(e) => updateBlock(block.id, { content: e.target.value })}
                aria-label={isAr ? "محتوى الكتلة" : "Block content"}
              />

              <div className="studio-preview-box">
                <NarrativeTextRenderer
                  template={block.content}
                  metricsById={metricsById}
                  language={language}
                  {...(onInspectMetric ? { onInspectMetric } : {})}
                  onUnlinkMetric={(metricId, frozenText) =>
                    updateBlock(block.id, {
                      content: block.content.replaceAll(`{{metric:${metricId}}}`, frozenText)
                    })
                  }
                />
              </div>
            </article>
          ))}
        </div>
      )}
    </section>
  );
}
