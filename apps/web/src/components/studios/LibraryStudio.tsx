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
import { Download, Eye, FileText, Library, ShieldCheck } from "lucide-react";
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

export function LibraryStudio(): ReactNode {
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

  const activePreview =
    templates.find((item) => item.id === previewTemplateId) ?? templates[0];

  return (
    <section className="safi-studio" aria-labelledby="safi-library-title" dir="rtl">
      <header className="safi-studio-header">
        <span className="safi-eyebrow">مكتبة القوالب وتقارير الجودة • Section 6.5 & 15</span>
        <h1 id="safi-library-title" className="safi-studio-title">
          المكتبة
        </h1>
        <p className="safi-studio-lead">
          وصفات جاهزة قابلة للتدقيق: كل قالب يحمل بصمة BLAKE3 ثابتة، ويمكنك معاينة الوصفة مباشرة أو
          تنزيلها أو تصدير تقرير الجودة الخاص بها كملفات جديدة.
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
                      if (
                        downloadTextFile(
                          fileName,
                          template.canonicalJson,
                          "application/json"
                        )
                      ) {
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
              marginBottom: "12px"
            }}
          >
            <div>
              <h2 className="safi-card-title" style={{ marginBottom: "4px" }}>
                <Eye size={16} aria-hidden="true" /> معاينة القالب الحي: {activePreview.nameAr}
              </h2>
              <p className="safi-hint" style={{ margin: 0 }}>
                تمثيل JCS القانوني وبصمة BLAKE3 الكاملة ({activePreview.enabledSteps.length} خطوات مفعّلة)
              </p>
            </div>
            <span className="safi-mono safi-badge safi-badge--ok" dir="ltr">
              BLAKE3: {activePreview.digest.slice(0, 16)}…
            </span>
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
