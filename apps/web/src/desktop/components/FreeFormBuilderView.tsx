import React, { useMemo, useState } from "react";
import { ArrowDown, ArrowLeft, ArrowRight, ArrowUp, Layers, Plus, Trash2 } from "lucide-react";
import type {
  DesktopMetric,
  StudioAssetArtifact,
  StudioFreeFormBlockArtifact
} from "../studio-bridge";
import { NarrativeTextRenderer } from "./NarrativeTextRenderer";
import { VerifiedNumberChip } from "./VerifiedNumberChip";

const ALLOWED_IMAGE_MIMES = new Set(["image/png", "image/jpeg", "image/webp"]);
const MAX_IMAGE_BYTES = 700_000;

function clamp(val: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, Math.trunc(val)));
}

function normalizeBlockBounds(block: StudioFreeFormBlockArtifact): StudioFreeFormBlockArtifact {
  return {
    ...block,
    col: clamp(block.col, 1, 12),
    row: clamp(block.row, 1, 32),
    colSpan: clamp(block.colSpan, 1, 12),
    rowSpan: clamp(block.rowSpan, 1, 8),
    zIndex: clamp(block.zIndex, 1, 50)
  };
}

export interface FreeFormBuilderViewProps {
  readonly language: "en" | "ar";
  readonly metrics: readonly DesktopMetric[];
  readonly freeformBlocks: readonly StudioFreeFormBlockArtifact[];
  readonly assets: readonly StudioAssetArtifact[];
  readonly onUpdateFreeForm: (
    blocks: readonly StudioFreeFormBlockArtifact[],
    assets: readonly StudioAssetArtifact[]
  ) => void;
  readonly onInspectMetric?: (metric: DesktopMetric) => void;
}

export function FreeFormBuilderView({
  language,
  metrics,
  freeformBlocks,
  assets,
  onUpdateFreeForm,
  onInspectMetric
}: FreeFormBuilderViewProps): React.ReactElement {
  const isAr = language === "ar";
  const [uploadError, setUploadError] = useState<string | null>(null);
  const metricsById = useMemo(() => new Map(metrics.map((m) => [m.id, m])), [metrics]);
  const assetsById = useMemo(() => new Map(assets.map((a) => [a.id, a])), [assets]);

  function addBlock(kind: "kpi" | "text" | "image"): void {
    const firstMetric = metrics[0];
    const firstAsset = assets[0];
    const next: StudioFreeFormBlockArtifact = {
      id: `ff_${Date.now()}_${freeformBlocks.length + 1}`,
      kind,
      title:
        kind === "kpi"
          ? (firstMetric?.name ?? "KPI")
          : kind === "image"
            ? isAr
              ? "صورة محلية"
              : "Local Visual"
            : isAr
              ? "بطاقة نصية"
              : "Canvas Note",
      body:
        kind === "text" && firstMetric
          ? `{{metric:${firstMetric.id}}}`
          : isAr
            ? "كتلة حرة متصلة بالبيانات"
            : "Canvas block",
      metricId: kind === "kpi" ? (firstMetric?.id ?? null) : null,
      assetId: kind === "image" ? (firstAsset?.id ?? null) : null,
      col: 1,
      row: Math.min(30, freeformBlocks.length * 2 + 1),
      colSpan: kind === "kpi" ? 4 : 6,
      rowSpan: 2,
      zIndex: freeformBlocks.length + 1
    };
    onUpdateFreeForm([...freeformBlocks, next], assets);
  }

  function updateBlock(id: string, patch: Partial<StudioFreeFormBlockArtifact>): void {
    onUpdateFreeForm(
      freeformBlocks.map((b) => (b.id === id ? normalizeBlockBounds({ ...b, ...patch }) : b)),
      assets
    );
  }

  function handleAssetFile(file: File | null): void {
    setUploadError(null);
    if (!file) {
      return;
    }
    if (!ALLOWED_IMAGE_MIMES.has(file.type)) {
      setUploadError(
        isAr
          ? "صيغة الصورة غير مدعومة (يُسمح فقط بـ PNG / JPEG / WEBP لحظر SVG التنفيذية)."
          : "Unsupported image type (PNG / JPEG / WEBP only; SVG is blocked for safety)."
      );
      return;
    }
    if (file.size > MAX_IMAGE_BYTES) {
      setUploadError(
        isAr ? "حجم الصورة يتجاوز الحد الآمن (700 كيلوبايت)." : "Image exceeds 700 KB limit."
      );
      return;
    }
    const reader = new FileReader();
    reader.onload = () => {
      if (typeof reader.result === "string") {
        const nextAsset: StudioAssetArtifact = {
          id: `asset_${Date.now()}`,
          name: file.name.slice(0, 64),
          dataUri: reader.result
        };
        onUpdateFreeForm(freeformBlocks, [...assets, nextAsset]);
      }
    };
    reader.readAsDataURL(file);
  }

  return (
    <section className="studio-panel" data-testid="freeform-builder-view">
      <div className="studio-panel-header">
        <div>
          <h3>{isAr ? "منصة البناء الحر (Free-Form)" : "Custom Free-Form Canvas Builder"}</h3>
          <p className="studio-muted-note">
            {isAr
              ? "مخطط شبكة مرن من 12 عمودًا مع أزرار تحريك/تغيير حجم/طبقات بديلة للسحب بالكامل بلوحة المفاتيح."
              : "12-column free-form canvas with full keyboard & click move/resize/z-index controls."}
          </p>
        </div>
        <div className="studio-quick-templates">
          <button type="button" className="studio-btn-secondary" onClick={() => addBlock("kpi")}>
            <Plus size={13} aria-hidden="true" /> {isAr ? "بطاقة KPI" : "KPI Card"}
          </button>
          <button type="button" className="studio-btn-secondary" onClick={() => addBlock("text")}>
            <Plus size={13} aria-hidden="true" /> {isAr ? "نص موثق" : "Narrative Card"}
          </button>
          <button type="button" className="studio-btn-secondary" onClick={() => addBlock("image")}>
            <Plus size={13} aria-hidden="true" /> {isAr ? "صورة" : "Image Block"}
          </button>
          <label className="studio-btn-secondary studio-file-label">
            <span>{isAr ? "رفع صورة محلية (PNG/JPEG)" : "Add Local Image (PNG/JPEG)"}</span>
            <input
              type="file"
              accept="image/png,image/jpeg,image/webp"
              onChange={(e) => handleAssetFile(e.target.files?.[0] ?? null)}
            />
          </label>
        </div>
      </div>

      {uploadError ? (
        <div className="studio-alert-error" role="alert">
          {uploadError}
        </div>
      ) : null}

      <div className="studio-freeform-canvas">
        {freeformBlocks.map((block) => {
          const metric = block.metricId ? (metricsById.get(block.metricId) ?? null) : null;
          const asset = block.assetId ? (assetsById.get(block.assetId) ?? null) : null;
          return (
            <article
              key={block.id}
              className="studio-subcard studio-ff-item"
              tabIndex={0}
              data-grid-col={clamp(block.col, 1, 12)}
              data-grid-row={clamp(block.row, 1, 32)}
              data-grid-col-span={clamp(block.colSpan, 1, 12)}
              data-grid-row-span={clamp(block.rowSpan, 1, 8)}
              onKeyDown={(e) => {
                if (e.target !== e.currentTarget) return;
                if (e.shiftKey && e.key === "ArrowDown") {
                  e.preventDefault();
                  updateBlock(block.id, { rowSpan: block.rowSpan + 1 });
                } else if (e.shiftKey && e.key === "ArrowUp") {
                  e.preventDefault();
                  updateBlock(block.id, { rowSpan: block.rowSpan - 1 });
                } else if (e.shiftKey && e.key === "ArrowRight") {
                  e.preventDefault();
                  updateBlock(block.id, { colSpan: block.colSpan + 1 });
                } else if (e.shiftKey && e.key === "ArrowLeft") {
                  e.preventDefault();
                  updateBlock(block.id, { colSpan: block.colSpan - 1 });
                }
              }}
              style={{
                gridColumn: `${clamp(block.col, 1, 12)} / span ${clamp(block.colSpan, 1, 12)}`,
                gridRow: `${clamp(block.row, 1, 32)} / span ${clamp(block.rowSpan, 1, 8)}`,
                zIndex: clamp(block.zIndex, 1, 50)
              }}
            >
              <div className="studio-panel-header">
                <input
                  value={block.title}
                  onChange={(e) => updateBlock(block.id, { title: e.target.value })}
                  aria-label={isAr ? "عنوان الكتلة" : "Canvas block title"}
                />
                <div className="studio-quick-templates">
                  <button
                    type="button"
                    className="studio-chip-btn"
                    title={isAr ? "يسار" : "Move left"}
                    aria-label={isAr ? "تحريك لليسار" : "Move left"}
                    onClick={() => updateBlock(block.id, { col: Math.max(1, block.col - 1) })}
                  >
                    <ArrowLeft size={12} aria-hidden="true" />
                  </button>
                  <button
                    type="button"
                    className="studio-chip-btn"
                    title={isAr ? "يمين" : "Move right"}
                    aria-label={isAr ? "تحريك لليمين" : "Move right"}
                    onClick={() => updateBlock(block.id, { col: Math.min(12, block.col + 1) })}
                  >
                    <ArrowRight size={12} aria-hidden="true" />
                  </button>
                  <button
                    type="button"
                    className="studio-chip-btn"
                    aria-label={isAr ? "تحريك للأعلى" : "Move up"}
                    onClick={() => updateBlock(block.id, { row: Math.max(1, block.row - 1) })}
                  >
                    <ArrowUp size={12} aria-hidden="true" />
                  </button>
                  <button
                    type="button"
                    className="studio-chip-btn"
                    aria-label={isAr ? "تحريك للأسفل" : "Move down"}
                    onClick={() => updateBlock(block.id, { row: Math.min(32, block.row + 1) })}
                  >
                    <ArrowDown size={12} aria-hidden="true" />
                  </button>
                  <button
                    type="button"
                    className="studio-chip-btn"
                    aria-label={isAr ? "تغيير العرض" : "Cycle width"}
                    onClick={() =>
                      updateBlock(block.id, {
                        colSpan: block.colSpan >= 12 ? 4 : block.colSpan + 2
                      })
                    }
                  >
                    {`W:${block.colSpan}`}
                  </button>
                  <button
                    type="button"
                    className="studio-chip-btn"
                    aria-label={isAr ? "تقليل الارتفاع" : "Decrease height"}
                    disabled={block.rowSpan <= 1}
                    onClick={() =>
                      updateBlock(block.id, { rowSpan: Math.max(1, block.rowSpan - 1) })
                    }
                  >
                    {`-H`}
                  </button>
                  <button
                    type="button"
                    className="studio-chip-btn"
                    aria-label={isAr ? "تغيير الارتفاع" : "Cycle height"}
                    onClick={() =>
                      updateBlock(block.id, {
                        rowSpan: block.rowSpan >= 8 ? 1 : block.rowSpan + 1
                      })
                    }
                  >
                    {`H:${block.rowSpan}`}
                  </button>
                  <button
                    type="button"
                    className="studio-chip-btn"
                    aria-label={isAr ? "زيادة الارتفاع" : "Increase height"}
                    disabled={block.rowSpan >= 8}
                    onClick={() =>
                      updateBlock(block.id, { rowSpan: Math.min(8, block.rowSpan + 1) })
                    }
                  >
                    {`+H`}
                  </button>
                  <button
                    type="button"
                    className="studio-chip-btn"
                    onClick={() => updateBlock(block.id, { zIndex: block.zIndex + 1 })}
                  >
                    <Layers size={12} aria-hidden="true" /> {`Z:${block.zIndex}`}
                  </button>
                  <button
                    type="button"
                    className="studio-chip-btn"
                    aria-label={isAr ? "حذف" : "Delete block"}
                    onClick={() =>
                      onUpdateFreeForm(
                        freeformBlocks.filter((b) => b.id !== block.id),
                        assets
                      )
                    }
                  >
                    <Trash2 size={12} aria-hidden="true" />
                  </button>
                </div>
              </div>

              {metric ? (
                <VerifiedNumberChip
                  metric={metric}
                  lang={language}
                  {...(onInspectMetric ? { onInspect: onInspectMetric } : {})}
                />
              ) : null}

              {block.kind === "image" ? (
                <div className="studio-field-stack">
                  <select
                    value={block.assetId ?? ""}
                    onChange={(e) =>
                      updateBlock(block.id, { assetId: e.target.value ? e.target.value : null })
                    }
                  >
                    <option value="">{isAr ? "اختر صورة مرفوعة" : "Pick uploaded image..."}</option>
                    {assets.map((a) => (
                      <option key={a.id} value={a.id}>
                        {a.name}
                      </option>
                    ))}
                  </select>
                  {asset ? (
                    <img className="studio-canvas-img" src={asset.dataUri} alt={asset.name} />
                  ) : null}
                </div>
              ) : (
                <div className="studio-field-stack">
                  <input
                    value={block.body}
                    onChange={(e) => updateBlock(block.id, { body: e.target.value })}
                    aria-label={isAr ? "نص الكتلة" : "Canvas block body"}
                  />
                  <NarrativeTextRenderer
                    template={block.body}
                    metricsById={metricsById}
                    language={language}
                    {...(onInspectMetric ? { onInspectMetric } : {})}
                  />
                </div>
              )}
            </article>
          );
        })}
      </div>
    </section>
  );
}
