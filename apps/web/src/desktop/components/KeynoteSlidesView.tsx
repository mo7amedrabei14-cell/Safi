import React, { useMemo, useState } from "react";
import { ArrowDown, ArrowUp, Copy, Plus, Presentation, Trash2 } from "lucide-react";
import type { DesktopMetric, StudioSlideArtifact } from "../studio-bridge";
import { NarrativeTextRenderer, listMissingMetricRefs } from "./NarrativeTextRenderer";
import { VerifiedNumberChip } from "./VerifiedNumberChip";

export interface KeynoteSlidesViewProps {
  readonly language: "en" | "ar";
  readonly metrics: readonly DesktopMetric[];
  readonly slides: readonly StudioSlideArtifact[];
  readonly onUpdateSlides: (next: readonly StudioSlideArtifact[]) => void;
  readonly onInspectMetric?: (metric: DesktopMetric) => void;
}

export function KeynoteSlidesView({
  language,
  metrics,
  slides,
  onUpdateSlides,
  onInspectMetric
}: KeynoteSlidesViewProps): React.ReactElement {
  const isAr = language === "ar";
  const [presenterMode, setPresenterMode] = useState(false);
  const [presenterIdx, setPresenterIdx] = useState(0);

  const metricsById = useMemo(() => new Map(metrics.map((m) => [m.id, m])), [metrics]);
  const missingRefs = useMemo(
    () => slides.flatMap((s) => listMissingMetricRefs(s.body, metricsById)),
    [slides, metricsById]
  );

  function addSlide(): void {
    const firstMetric = metrics[0];
    const nextSlide: StudioSlideArtifact = {
      id: `slide_${Date.now()}_${slides.length + 1}`,
      title: isAr ? `شريحة ${slides.length + 1}` : `Slide ${slides.length + 1}`,
      body: firstMetric
        ? isAr
          ? `الأداء الموثق: {{metric:${firstMetric.id}}}`
          : `Verified result: {{metric:${firstMetric.id}}}`
        : isAr
          ? "أضف ملخصًا تنفيذيًا هنا."
          : "Add executive takeaway here.",
      metricId: firstMetric?.id ?? null,
      chartIndex: null,
      layout: "kpi_split"
    };
    onUpdateSlides([...slides, nextSlide]);
  }

  function updateSlide(id: string, patch: Partial<StudioSlideArtifact>): void {
    onUpdateSlides(slides.map((s) => (s.id === id ? { ...s, ...patch } : s)));
  }

  function moveSlide(idx: number, delta: -1 | 1): void {
    const target = idx + delta;
    if (target < 0 || target >= slides.length) {
      return;
    }
    const copy = [...slides];
    const [item] = copy.splice(idx, 1);
    if (item) {
      copy.splice(target, 0, item);
      onUpdateSlides(copy);
    }
  }

  function duplicateSlide(idx: number): void {
    const source = slides[idx];
    if (!source) {
      return;
    }
    const copy = [...slides];
    copy.splice(idx + 1, 0, {
      ...source,
      id: `slide_${Date.now()}_${copy.length + 1}`,
      title: `${source.title} (${isAr ? "نسخة" : "Copy"})`
    });
    onUpdateSlides(copy);
  }

  const activePresenterSlide = slides[Math.min(presenterIdx, Math.max(0, slides.length - 1))];

  return (
    <section className="studio-panel" data-testid="keynote-slides-view">
      <div className="studio-panel-header">
        <div>
          <h3>{isAr ? "محرر الشرائح الحيّة" : "Live Keynote Slides Editor"}</h3>
          <p className="studio-muted-note">
            {isAr
              ? "استخدم {{metric:ID}} لربط الأرقام الحية داخل الشرائح؛ تتحدث فورًا عند تغيّر البيانات."
              : "Use {{metric:ID}} tokens inside slide copy to embed verified Rust figures."}
          </p>
        </div>
        <div className="studio-quick-templates">
          <button
            type="button"
            className="studio-btn-secondary"
            disabled={slides.length === 0}
            onClick={() => setPresenterMode((v) => !v)}
          >
            <Presentation size={14} aria-hidden="true" />{" "}
            {presenterMode
              ? isAr
                ? "العودة للتحرير"
                : "Exit Presenter Preview"
              : isAr
                ? "معاينة العرض"
                : "Presenter Preview"}
          </button>
          <button type="button" className="studio-btn-primary" onClick={addSlide}>
            <Plus size={14} aria-hidden="true" /> {isAr ? "شريحة جديدة" : "Add Slide"}
          </button>
        </div>
      </div>

      {missingRefs.length > 0 ? (
        <div className="studio-alert-error" role="alert">
          {isAr
            ? `مراجع مؤشرات غير موجودة في الشرائح (${missingRefs.join(", ")}): التصدير محظور حتى التصحيح.`
            : `Unresolved metric references in slides (${missingRefs.join(", ")}): export is blocked until fixed.`}
        </div>
      ) : null}

      {presenterMode && activePresenterSlide ? (
        <div className="studio-subcard studio-presenter-stage">
          <div className="studio-panel-header">
            <h4>{activePresenterSlide.title}</h4>
            <div className="studio-quick-templates">
              <button
                type="button"
                className="studio-btn-secondary"
                disabled={presenterIdx === 0}
                onClick={() => setPresenterIdx((i) => Math.max(0, i - 1))}
              >
                {isAr ? "الشريحة السابقة" : "Prev Slide"}
              </button>
              <span>{`${presenterIdx + 1} / ${slides.length}`}</span>
              <button
                type="button"
                className="studio-btn-secondary"
                disabled={presenterIdx >= slides.length - 1}
                onClick={() => setPresenterIdx((i) => Math.min(slides.length - 1, i + 1))}
              >
                {isAr ? "الشريحة التالية" : "Next Slide"}
              </button>
            </div>
          </div>
          {activePresenterSlide.metricId && metricsById.has(activePresenterSlide.metricId) ? (
            <VerifiedNumberChip
              metric={metricsById.get(activePresenterSlide.metricId) ?? null}
              lang={language}
              {...(onInspectMetric ? { onInspect: onInspectMetric } : {})}
            />
          ) : null}
          <NarrativeTextRenderer
            template={activePresenterSlide.body}
            metricsById={metricsById}
            language={language}
            {...(onInspectMetric ? { onInspectMetric } : {})}
          />
        </div>
      ) : (
        <div className="studio-card-grid">
          {slides.map((slide, idx) => (
            <article key={slide.id} className="studio-subcard">
              <div className="studio-panel-header">
                <strong>{`${isAr ? "شريحة" : "Slide"} #${idx + 1}`}</strong>
                <div className="studio-quick-templates">
                  <button
                    type="button"
                    className="studio-chip-btn"
                    aria-label={isAr ? "نقل للأعلى" : "Move slide up"}
                    disabled={idx === 0}
                    onClick={() => moveSlide(idx, -1)}
                  >
                    <ArrowUp size={13} aria-hidden="true" />
                  </button>
                  <button
                    type="button"
                    className="studio-chip-btn"
                    aria-label={isAr ? "نقل للأسفل" : "Move slide down"}
                    disabled={idx === slides.length - 1}
                    onClick={() => moveSlide(idx, 1)}
                  >
                    <ArrowDown size={13} aria-hidden="true" />
                  </button>
                  <button
                    type="button"
                    className="studio-chip-btn"
                    aria-label={isAr ? "تكرار الشريحة" : "Duplicate slide"}
                    onClick={() => duplicateSlide(idx)}
                  >
                    <Copy size={13} aria-hidden="true" />
                  </button>
                  <button
                    type="button"
                    className="studio-chip-btn"
                    aria-label={isAr ? "حذف الشريحة" : "Delete slide"}
                    onClick={() => onUpdateSlides(slides.filter((s) => s.id !== slide.id))}
                  >
                    <Trash2 size={13} aria-hidden="true" />
                  </button>
                </div>
              </div>

              <label className="studio-field-stack">
                <span>{isAr ? "العنوان" : "Slide Title"}</span>
                <input
                  value={slide.title}
                  onChange={(e) => updateSlide(slide.id, { title: e.target.value })}
                />
              </label>

              <label className="studio-field-stack">
                <span>{isAr ? "المؤشر المميز" : "Spotlight Metric"}</span>
                <select
                  value={slide.metricId ?? ""}
                  onChange={(e) =>
                    updateSlide(slide.id, { metricId: e.target.value ? e.target.value : null })
                  }
                >
                  <option value="">{isAr ? "بدون مؤشر" : "None"}</option>
                  {metrics.map((m) => (
                    <option key={m.id} value={m.id}>
                      {`${m.name} (${m.id})`}
                    </option>
                  ))}
                </select>
              </label>

              <label className="studio-field-stack">
                <span>{isAr ? "محتوى الشريحة (يدعم {{metric:id}})" : "Body ({{metric:id}})"}</span>
                <textarea
                  rows={3}
                  value={slide.body}
                  onChange={(e) => updateSlide(slide.id, { body: e.target.value })}
                />
              </label>

              <div className="studio-preview-box">
                <NarrativeTextRenderer
                  template={slide.body}
                  metricsById={metricsById}
                  language={language}
                  {...(onInspectMetric ? { onInspectMetric } : {})}
                  onUnlinkMetric={(metricId, frozenText) =>
                    updateSlide(slide.id, {
                      body: slide.body.replaceAll(`{{metric:${metricId}}}`, frozenText)
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
