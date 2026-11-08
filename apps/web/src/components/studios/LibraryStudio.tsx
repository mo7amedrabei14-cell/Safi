/**
 * `apps/web/src/components/studios/LibraryStudio.tsx`
 *
 * Template library: ready-made deterministic recipes plus real file exports
 * (`.safirecipe.json` and an HTML quality report carrying the BLAKE3 digest).
 *
 * Ed25519 signing is a kernel responsibility; this page never fabricates a
 * signature and says so in the report footer.
 */
import type { ReactNode } from "react";
import { useMemo, useState } from "react";
import { Download, FileText, Library, ShieldCheck } from "lucide-react";
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

  const templates = useMemo(
    () =>
      RECIPE_TEMPLATES.map((template) => ({
        ...template,
        enabledSteps: RECIPE_STEPS.filter((step) => template.toggles[step.id]).map(
          (step) => step.labelAr
        ),
        digest: recipeDigest(template.nameAr, template.toggles)
      })),
    []
  );

  return (
    <section className="safi-studio" aria-labelledby="safi-library-title" dir="rtl">
      <header className="safi-studio-header">
        <span className="safi-eyebrow">مكتبة القوالب وتقارير الجودة</span>
        <h1 id="safi-library-title" className="safi-studio-title">
          المكتبة
        </h1>
        <p className="safi-studio-lead">
          وصفات جاهزة قابلة للتدقيق: كل قالب يحمل بصمة BLAKE3 ثابتة، ويمكنك تنزيل الوصفة أو تقرير
          الجودة الخاص بها كملفات جديدة.
        </p>
      </header>

      {exported !== null && (
        <p className="safi-status safi-status--ok" role="status">
          تم إنشاء ملف جديد: <span className="safi-mono">{exported}</span>
        </p>
      )}

      <div className="safi-template-grid">
        {templates.map((template) => (
          <SpotlightCard key={template.id} padding={22}>
            <article data-template-id={template.id}>
              <h2 className="safi-card-title">
                <Library size={16} aria-hidden="true" /> {template.nameAr}
              </h2>
              <p className="safi-hint">{template.descriptionAr}</p>

              <div className="safi-chip-row">
                {template.enabledSteps.map((label) => (
                  <span key={label} className="safi-chip">
                    {label}
                  </span>
                ))}
              </div>

              <p className="safi-mini-label">بصمة الوصفة</p>
              <p className="safi-mono safi-digest-line" dir="ltr">
                {template.digest.slice(0, 24)}…
              </p>

              <div className="safi-actions">
                <MagneticButton
                  onClick={() => {
                    const fileName = `recipe-${template.id}.safirecipe.json`;
                    const content = canonicalRecipeJson(template.nameAr, template.toggles);
                    if (downloadTextFile(fileName, content, "application/json")) {
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
              </div>
            </article>
          </SpotlightCard>
        ))}
      </div>

      <SpotlightCard padding={22}>
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
