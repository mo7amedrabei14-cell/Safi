import React, { useEffect, useState } from "react";
import { FolderOpen, LayoutTemplate, RotateCcw, Save, Trash2 } from "lucide-react";
import {
  deleteProjectBundle,
  instantiateTemplate,
  listProjectBundles,
  listTemplates,
  type DesktopInstantiatedSection,
  type DesktopMetric,
  type DesktopProjectBundleMeta,
  type DesktopTemplateSummary,
  type StudioDocBlockArtifact,
  type StudioSlideArtifact,
  type StudioTemplateConfigArtifact
} from "../studio-bridge";

const TEMPLATE_LABELS: Record<string, { en: string; ar: string }> = {
  "boardroom-keynote": { en: "Boardroom Keynote", ar: "عرض مجلس الإدارة" },
  "velocity-radar": { en: "Velocity Radar", ar: "رادار الأداء والنمو" },
  "impact-atlas": { en: "Impact Atlas", ar: "أطلس التأثير" },
  "forensic-xray": { en: "Forensic X-Ray", ar: "الفحص الجنائي العميق" },
  "time-machine": { en: "Time Machine", ar: "آلة الزمن والمقارنات" }
};

export interface TemplatesAndProjectsPanelProps {
  readonly language: "en" | "ar";
  readonly project: string;
  readonly projectLoaded: boolean;
  readonly metrics: readonly DesktopMetric[];
  readonly currentTemplateConfig: StudioTemplateConfigArtifact | null;
  readonly onApplyTemplateToWorkspace: (
    config: StudioTemplateConfigArtifact,
    generatedSlides: readonly StudioSlideArtifact[],
    generatedDocBlocks: readonly StudioDocBlockArtifact[]
  ) => void;
  readonly onSaveBundle: (savedId: string, displayName: string) => Promise<void>;
  readonly onLoadBundle: (savedId: string) => Promise<void>;
}

export function TemplatesAndProjectsPanel({
  language,
  project,
  projectLoaded,
  metrics,
  currentTemplateConfig,
  onApplyTemplateToWorkspace,
  onSaveBundle,
  onLoadBundle
}: TemplatesAndProjectsPanelProps): React.ReactElement {
  const isAr = language === "ar";
  const [templates, setTemplates] = useState<readonly DesktopTemplateSummary[]>([]);
  const [selectedTplId, setSelectedTplId] = useState(
    currentTemplateConfig?.templateId ?? "boardroom-keynote"
  );
  const [titleOverride, setTitleOverride] = useState(currentTemplateConfig?.titleOverride ?? "");
  const [bindings, setBindings] = useState<Record<string, string>>({
    ...(currentTemplateConfig?.bindings ?? {})
  });
  const [sectionOverrides, setSectionOverrides] = useState<Record<string, string>>({
    ...(currentTemplateConfig?.sectionOverrides ?? {})
  });
  const [instantiatedPreview, setInstantiatedPreview] = useState<
    readonly DesktopInstantiatedSection[]
  >([]);
  const [tplError, setTplError] = useState<string | null>(null);

  const [bundles, setBundles] = useState<readonly DesktopProjectBundleMeta[]>([]);
  const [saveId, setSaveId] = useState("studio_q4");
  const [saveName, setSaveName] = useState(isAr ? "تحليل الربع الرابع" : "Q4 Studio Analysis");
  const [projectStatus, setProjectStatus] = useState<string | null>(null);

  useEffect(() => {
    void listTemplates()
      .then(setTemplates)
      .catch(() => {});
    void listProjectBundles()
      .then(setBundles)
      .catch(() => {});
  }, []);

  const activeTemplate = templates.find((t) => t.id === selectedTplId) ?? templates[0] ?? null;

  async function handleInstantiateTemplate(): Promise<void> {
    if (!activeTemplate || !projectLoaded) {
      return;
    }
    setTplError(null);
    try {
      const sections = await instantiateTemplate(project, activeTemplate.id, bindings);
      setInstantiatedPreview(sections);
      const slides: StudioSlideArtifact[] = sections.map((sec, i) => ({
        id: `tpl_slide_${i + 1}`,
        title: sectionOverrides[sec.headingKey] || sec.headingKey,
        body: `${sec.role}: {{metric:${sec.token.id}}}`,
        metricId: sec.token.id,
        chartIndex: null,
        layout: "kpi_split"
      }));
      const docs: StudioDocBlockArtifact[] = sections.map((sec, i) => ({
        id: `tpl_doc_${i + 1}`,
        kind: "metric_callout",
        title: sectionOverrides[sec.headingKey] || sec.headingKey,
        content: `${sec.token.name}: {{metric:${sec.token.id}}}`,
        metricId: sec.token.id,
        authoredBy: "user"
      }));
      onApplyTemplateToWorkspace(
        {
          templateId: activeTemplate.id,
          titleOverride: titleOverride.trim() || null,
          bindings,
          sectionOverrides
        },
        slides,
        docs
      );
    } catch (err) {
      setTplError(err instanceof Error ? err.message : String(err));
    }
  }

  function resetTemplateDefaults(): void {
    setTitleOverride("");
    setBindings({});
    setSectionOverrides({});
    setInstantiatedPreview([]);
    setTplError(null);
  }

  return (
    <section className="studio-panel" data-testid="templates-projects-panel">
      <div className="studio-card-grid">
        {/* Saved Multi-Project Bundles */}
        <article className="studio-subcard">
          <div className="studio-panel-header">
            <h3>
              <FolderOpen size={15} aria-hidden="true" />{" "}
              {isAr ? "مشاريعي المحفوظة محليًا (BLAKE3)" : "Local Multi-Project Manager (BLAKE3)"}
            </h3>
          </div>
          <div className="studio-inline-fields">
            <label>
              <span>{isAr ? "معرّف المشروع" : "Project ID"}</span>
              <input value={saveId} onChange={(e) => setSaveId(e.target.value)} />
            </label>
            <label>
              <span>{isAr ? "اسم العرض" : "Display Name"}</span>
              <input value={saveName} onChange={(e) => setSaveName(e.target.value)} />
            </label>
            <div className="studio-inline-action-end">
              <button
                type="button"
                className="studio-btn-primary"
                disabled={!projectLoaded}
                onClick={() => {
                  setProjectStatus(null);
                  void onSaveBundle(saveId, saveName)
                    .then(() => listProjectBundles().then(setBundles))
                    .then(() =>
                      setProjectStatus(
                        isAr
                          ? "تم الحفظ الذري على القرص مع بصمة BLAKE3."
                          : "Saved atomically with BLAKE3."
                      )
                    )
                    .catch((e) => setProjectStatus(e instanceof Error ? e.message : String(e)));
                }}
              >
                <Save size={13} aria-hidden="true" /> {isAr ? "حفظ المشروع" : "Save Project"}
              </button>
            </div>
          </div>

          {projectStatus ? <div className="studio-alert-info">{projectStatus}</div> : null}

          <ul className="studio-anomaly-list">
            {bundles.map((bundle) => (
              <li key={bundle.id} className="studio-panel-header">
                <div>
                  <strong>{bundle.displayName || bundle.id}</strong>{" "}
                  <span
                    className={bundle.validChecksum ? "studio-badge-muted" : "studio-badge-warn"}
                  >
                    {bundle.validChecksum
                      ? isAr
                        ? "BLAKE3 سليم"
                        : "BLAKE3 OK"
                      : isAr
                        ? "ملف تالف"
                        : "CHECKSUM FAIL"}
                  </span>
                </div>
                <div className="studio-quick-templates">
                  <button
                    type="button"
                    className="studio-btn-secondary"
                    onClick={() => {
                      setProjectStatus(null);
                      void onLoadBundle(bundle.id).catch((e) =>
                        setProjectStatus(e instanceof Error ? e.message : String(e))
                      );
                    }}
                  >
                    {isAr ? "فتح" : "Open"}
                  </button>
                  <button
                    type="button"
                    className="studio-chip-btn"
                    aria-label={isAr ? "حذف المشروع" : "Delete project"}
                    onClick={() => void deleteProjectBundle(bundle.id).then(setBundles)}
                  >
                    <Trash2 size={13} aria-hidden="true" />
                  </button>
                </div>
              </li>
            ))}
          </ul>
        </article>

        {/* All 5 Living Templates Gallery */}
        <article className="studio-subcard">
          <div className="studio-panel-header">
            <h3>
              <LayoutTemplate size={15} aria-hidden="true" />{" "}
              {isAr ? "معرض القوالب الحيّة الخمسة" : "Living Templates Gallery (5 Built-In)"}
            </h3>
            <button type="button" className="studio-chip-btn" onClick={resetTemplateDefaults}>
              <RotateCcw size={12} aria-hidden="true" />{" "}
              {isAr ? "إعادة للافتراضي" : "Reset Template"}
            </button>
          </div>

          <div className="studio-quick-templates">
            {templates.map((tpl) => {
              const label = TEMPLATE_LABELS[tpl.id];
              return (
                <button
                  key={tpl.id}
                  type="button"
                  className={
                    tpl.id === (activeTemplate?.id ?? selectedTplId)
                      ? "studio-btn-primary"
                      : "studio-btn-secondary"
                  }
                  onClick={() => {
                    setSelectedTplId(tpl.id);
                    setTplError(null);
                  }}
                >
                  {label ? (isAr ? label.ar : label.en) : tpl.id}
                </button>
              );
            })}
          </div>

          {activeTemplate ? (
            <div className="studio-field-stack">
              <label className="studio-field-stack">
                <span>{isAr ? "عنوان مخصص للقالب" : "Custom Template Title"}</span>
                <input
                  value={titleOverride}
                  placeholder={TEMPLATE_LABELS[activeTemplate.id]?.en ?? activeTemplate.id}
                  onChange={(e) => setTitleOverride(e.target.value)}
                />
              </label>

              {activeTemplate.sections.map((sec) => (
                <div key={`${sec.headingKey}-${sec.role}`} className="studio-inline-fields">
                  <label>
                    <span>{`${isAr ? "دور" : "Role"}: ${sec.role}`}</span>
                    <select
                      value={bindings[sec.role] ?? ""}
                      onChange={(e) =>
                        setBindings((prev) => ({ ...prev, [sec.role]: e.target.value }))
                      }
                    >
                      <option value="">{isAr ? "-- اربط بمؤشر --" : "-- Bind metric --"}</option>
                      {metrics.map((m) => (
                        <option key={m.id} value={m.id}>
                          {`${m.name} (${m.id})`}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label>
                    <span>{isAr ? "عنوان القسم" : "Section Heading"}</span>
                    <input
                      value={sectionOverrides[sec.headingKey] ?? sec.headingKey}
                      onChange={(e) =>
                        setSectionOverrides((prev) => ({
                          ...prev,
                          [sec.headingKey]: e.target.value
                        }))
                      }
                    />
                  </label>
                </div>
              ))}

              <button
                type="button"
                className="studio-btn-primary"
                disabled={!projectLoaded}
                onClick={() => void handleInstantiateTemplate()}
              >
                {isAr
                  ? "تطبيق القالب وربطه بالمؤشرات الموثقة"
                  : "Instantiate Template with Verified Metrics"}
              </button>
            </div>
          ) : null}

          {tplError ? (
            <div className="studio-alert-error" role="alert">
              {tplError}
            </div>
          ) : null}

          {instantiatedPreview.length > 0 ? (
            <div className="studio-alert-success">
              {isAr
                ? `تم توليد ${instantiatedPreview.length} أقسام موثقة وإدراجها في الشرائح والمستند.`
                : `Generated ${instantiatedPreview.length} verified sections into Slides and Verified Doc.`}
            </div>
          ) : null}
        </article>
      </div>
    </section>
  );
}
