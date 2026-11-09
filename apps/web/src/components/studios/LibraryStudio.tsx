/**
 * `apps/web/src/components/studios/LibraryStudio.tsx`
 *
 * Template library: ready-made deterministic recipes plus real file exports
 * (`.safirecipe.json` and an HTML quality report carrying the BLAKE3 digest)
 * and interactive inline inspection.
 *
 * Ed25519 signing is a kernel responsibility; this page never fabricates a
 * signature and says so in the report footer.
 */
import type { ReactNode } from "react";
import { useMemo, useState } from "react";
import {
  BarChart3,
  Download,
  Eye,
  FileText,
  Home,
  Library,
  ListChecks,
  ShieldCheck,
  Sparkles,
  Zap
} from "lucide-react";
import type { SafiDockTabId, SafiLang } from "../layout/FloatingGlassDock";
import { downloadTextFile } from "../../lib/download";
import {
  RECIPE_STEPS,
  RECIPE_TEMPLATES,
  buildCanonicalRecipe,
  canonicalRecipeJson,
  recipeDigest
} from "../../lib/recipe";
import { buildRecipeReportHtml } from "../../lib/report";
import { MagneticButton } from "../ui/MagneticButton";
import { SpotlightCard } from "../ui/SpotlightCard";

export interface LibraryStudioProps {
  readonly lang?: SafiLang;
  readonly onSelectTab?: (tab: SafiDockTabId) => void;
}

export function LibraryStudio({ lang = "en", onSelectTab }: LibraryStudioProps = {}): ReactNode {
  const isAr = lang === "ar";
  const [exported, setExported] = useState<string | null>(null);
  const [previewTemplateId, setPreviewTemplateId] = useState<string>(
    RECIPE_TEMPLATES[0]?.id ?? "hr-payroll"
  );

  const templates = useMemo(
    () =>
      RECIPE_TEMPLATES.map((template) => ({
        ...template,
        enabledSteps: RECIPE_STEPS.filter((step) => template.toggles[step.id]).map(
          (step) => step.labelAr
        ),
        digest: recipeDigest(template.nameAr, template.toggles),
        canonicalJson: canonicalRecipeJson(template.nameAr, template.toggles)
      })),
    []
  );

  const activePreview = templates.find((item) => item.id === previewTemplateId) ?? templates[0];

  return (
    <section
      className="safi-studio"
      aria-labelledby="safi-library-title"
      dir={isAr ? "rtl" : "ltr"}
    >
      {/* Apple Dynamic Island Template Vault & App Switcher Bar */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: "12px",
          flexWrap: "wrap"
        }}
      >
        <div className="safi-dynamic-island" style={{ cursor: "default" }}>
          <span className="safi-island-dot" />
          <span style={{ fontSize: "12.5px", fontWeight: 800, color: "#F8FAFC" }}>
            {isAr
              ? "خزنة القوالب الموثّقة • BLAKE3 + JCS"
              : "Verified Template Vault • BLAKE3 + JCS"}
          </span>
          <span
            className="safi-mono"
            dir="ltr"
            style={{ fontSize: "11.5px", color: "#34D399", fontWeight: 800 }}
          >
            {templates.length} Verified Templates • 100% Auditable
          </span>
        </div>

        {onSelectTab ? (
          <div style={{ display: "flex", gap: "6px", flexWrap: "wrap" }}>
            <button
              type="button"
              onClick={() => onSelectTab("home")}
              className="safi-btn safi-btn-secondary"
              style={{ minHeight: "36px", padding: "4px 12px", fontSize: "12px" }}
            >
              <Home size={13} /> {isAr ? "الرئيسية" : "Website"}
            </button>
            <button
              type="button"
              onClick={() => onSelectTab("cleaning")}
              className="safi-btn safi-btn-secondary"
              style={{ minHeight: "36px", padding: "4px 12px", fontSize: "12px" }}
            >
              <Sparkles size={13} /> {isAr ? "التنظيف وPII" : "Cleaning"}
            </button>
            <button
              type="button"
              onClick={() => onSelectTab("recipes")}
              className="safi-btn safi-btn-secondary"
              style={{ minHeight: "36px", padding: "4px 12px", fontSize: "12px" }}
            >
              <ListChecks size={13} /> {isAr ? "الوصفات" : "Recipes"}
            </button>
            <button
              type="button"
              onClick={() => onSelectTab("charts")}
              className="safi-btn safi-btn-secondary"
              style={{ minHeight: "36px", padding: "4px 12px", fontSize: "12px" }}
            >
              <BarChart3 size={13} /> {isAr ? "الرسوم البيانية" : "Charts"}
            </button>
            <button
              type="button"
              onClick={() => onSelectTab("owner")}
              className="safi-btn safi-btn-secondary"
              style={{ minHeight: "36px", padding: "4px 12px", fontSize: "12px" }}
            >
              <ShieldCheck size={13} /> {isAr ? "لوحة المالك" : "Owner"}
            </button>
          </div>
        ) : null}
      </div>

      {/* Open Unboxed Header Sitting Directly on the Interactive Background */}
      <header
        className="safi-studio-header"
        style={{
          background: "transparent",
          border: "none",
          boxShadow: "none",
          padding: "8px 4px"
        }}
      >
        <span className="safi-eyebrow">مكتبة القوالب وتقارير الجودة • Section 6.5 & 15</span>
        <h1 id="safi-library-title" className="safi-studio-title safi-rb-shiny-text">
          المكتبة
        </h1>
        <p className="safi-studio-lead">
          {isAr
            ? "وصفات جاهزة قابلة للتدقيق: كل قالب يحمل بصمة BLAKE3 ثابتة، ويمكنك معاينة الوصفة مباشرة أو تنزيلها أو تصدير تقرير الجودة الخاص بها كملفات جديدة."
            : "Audit-ready deterministic templates: every template carries a stable BLAKE3 digest, live before/after sandbox preview, and 1-click recipe & HTML quality report export."}
        </p>
      </header>

      {exported !== null && (
        <p className="safi-status safi-status--ok" role="status">
          ✓ تم إنشاء ملف جديد بنجاح: <span className="safi-mono">{exported}</span>
        </p>
      )}

      <div className="safi-template-grid">
        {templates.map((template) => {
          const isPreviewed = activePreview?.id === template.id;
          return (
            <SpotlightCard key={template.id} padding={24}>
              <article data-template-id={template.id}>
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    gap: "8px",
                    marginBottom: "8px"
                  }}
                >
                  <h2 className="safi-card-title" style={{ marginBottom: 0 }}>
                    <Library size={16} aria-hidden="true" /> {template.nameAr}
                  </h2>
                  {isPreviewed && (
                    <span className="safi-badge safi-badge--accent">قيد المعاينة</span>
                  )}
                </div>
                <p className="safi-hint">{template.descriptionAr}</p>

                <div className="safi-chip-row">
                  {template.enabledSteps.map((label) => (
                    <span key={label} className="safi-chip">
                      {label}
                    </span>
                  ))}
                </div>

                <p className="safi-mini-label">بصمة الوصفة (BLAKE3)</p>
                <p className="safi-mono safi-digest-line" dir="ltr">
                  {template.digest.slice(0, 24)}…
                </p>

                <div className="safi-actions">
                  <MagneticButton
                    onClick={() => {
                      const fileName = `recipe-${template.id}.safirecipe.json`;
                      if (downloadTextFile(fileName, template.canonicalJson, "application/json")) {
                        setExported(fileName);
                      }
                    }}
                    ariaLabel={`تنزيل وصفة ${template.nameAr}`}
                  >
                    <Download size={16} aria-hidden="true" />
                    تنزيل الوصفة
                  </MagneticButton>
                  <MagneticButton
                    variant="glass"
                    onClick={() => {
                      const html = buildRecipeReportHtml({
                        recipe: buildCanonicalRecipe(template.nameAr, template.toggles),
                        digest: template.digest,
                        generatedAt: new Date().toISOString()
                      });
                      const fileName = `quality-report-${template.id}.html`;
                      if (downloadTextFile(fileName, html, "text/html")) {
                        setExported(fileName);
                      }
                    }}
                    ariaLabel={`تصدير تقرير جودة ${template.nameAr}`}
                  >
                    <FileText size={16} aria-hidden="true" />
                    تقرير الجودة
                  </MagneticButton>
                  <button
                    type="button"
                    className="safi-btn safi-btn-secondary"
                    style={{ padding: "6px 12px", fontSize: "12px" }}
                    onClick={() => setPreviewTemplateId(template.id)}
                  >
                    <Eye size={14} aria-hidden="true" />
                    معاينة حية
                  </button>
                </div>
              </article>
            </SpotlightCard>
          );
        })}
      </div>

      {activePreview !== undefined && (
        <SpotlightCard padding={24}>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              flexWrap: "wrap",
              gap: "12px",
              marginBottom: "14px"
            }}
          >
            <div>
              <h2 className="safi-card-title" style={{ marginBottom: "4px" }}>
                <Eye size={16} aria-hidden="true" /> معاينة القالب الحي: {activePreview.nameAr}
              </h2>
              <p className="safi-hint" style={{ margin: 0 }}>
                تمثيل JCS القانوني وبصمة BLAKE3 الكاملة ({activePreview.enabledSteps.length} خطوات
                مفعّلة)
              </p>
            </div>
            <div style={{ display: "flex", alignItems: "center", gap: "8px", flexWrap: "wrap" }}>
              <span className="safi-mono safi-badge safi-badge--ok" dir="ltr">
                BLAKE3: {activePreview.digest.slice(0, 16)}…
              </span>
              {onSelectTab ? (
                <button
                  type="button"
                  onClick={() => onSelectTab("recipes")}
                  className="safi-btn safi-btn-primary"
                  style={{ minHeight: "34px", padding: "4px 14px", fontSize: "12px" }}
                >
                  <Zap size={13} /> {isAr ? "تخصيص في محرك الوصفات" : "Customize in Recipes"}
                </button>
              ) : null}
            </div>
          </div>

          <div
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(auto-fit, minmax(240px, 1fr))",
              gap: "12px",
              marginBottom: "14px"
            }}
          >
            <div
              style={{
                padding: "12px 14px",
                borderRadius: "14px",
                background: "var(--bg-elevated)",
                border: "1px solid var(--border-subtle)"
              }}
            >
              <span
                style={{
                  fontSize: "11px",
                  color: "var(--text-muted)",
                  display: "block",
                  marginBottom: "4px"
                }}
              >
                {isAr ? "عينة البيانات الخام قبل القالب" : "Sample Raw Input"}
              </span>
              <span
                style={{
                  fontSize: "13px",
                  color: "var(--text-secondary)",
                  textDecoration: "line-through"
                }}
              >
                أَحْمَدُ السيّد ـــ ١٥٠٠.٥٠ ج.م • ٠١٠١٢٣٤٥٦٧٨
              </span>
            </div>

            <div
              style={{
                padding: "12px 14px",
                borderRadius: "14px",
                background: "var(--accent-tint)",
                border: "1px solid var(--border-strong)"
              }}
            >
              <span
                style={{
                  fontSize: "11px",
                  color: "var(--accent-primary)",
                  fontWeight: 700,
                  display: "block",
                  marginBottom: "4px"
                }}
              >
                {isAr ? "المخرج المنقّى بعد تطبيق القالب" : "Purified Output After Template"}
              </span>
              <span style={{ fontSize: "13.5px", fontWeight: 800, color: "var(--text-primary)" }}>
                ✓ أحمد السيد | 1500.50 | 010••••5678
              </span>
            </div>
          </div>

          <pre className="safi-code" dir="ltr">
            {activePreview.canonicalJson}
          </pre>
        </SpotlightCard>
      )}

      <SpotlightCard padding={24}>
        <h2 className="safi-card-title">
          <ShieldCheck size={16} aria-hidden="true" /> عن التوقيع الرقمي
        </h2>
        <p className="safi-hint">
          التقرير المصدَّر يحمل بصمة تحقق BLAKE3 يطابقها أي طرف دون مفاتيح. أما التوقيع الرقمي
          Ed25519 فيُنتَج داخل نواة سطح المكتب بمفتاح الترخيص المحلي عند التصدير النهائي من التطبيق،
          ولا تدّعي هذه الصفحة أنها وقّعت شيئًا.
        </p>
      </SpotlightCard>
    </section>
  );
}
