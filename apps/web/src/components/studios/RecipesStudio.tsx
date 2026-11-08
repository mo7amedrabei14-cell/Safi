/**
 * `apps/web/src/components/studios/RecipesStudio.tsx`
 *
 * Deterministic Recipe Builder + Graphify DAG + WCAG 2.2 2.5.7 Click-to-Move Reordering.
 */
import type { ChangeEvent, ReactNode } from "react";
import { useMemo, useState } from "react";
import { ArrowDown, ArrowUp, CheckCircle2, Download, FileCode2, FolderOpen, XCircle } from "lucide-react";
import { downloadTextFile } from "../../lib/download";
import {
  DEFAULT_RECIPE_TOGGLES,
  RECIPE_STEPS,
  canonicalRecipeJson,
  buildCanonicalRecipe,
  enabledStepCount,
  recipeDigest,
  validateWatchFolder
} from "../../lib/recipe";
import type { RecipeStepId, RecipeToggles } from "../../lib/recipe";
import { buildRecipeReportHtml } from "../../lib/report";
import { DecryptedMonoText } from "../ui/DecryptedMonoText";
import { GraphifyLineageCanvas } from "../ui/GraphifyLineageCanvas";
import { MagneticButton } from "../ui/MagneticButton";
import { SpotlightCard } from "../ui/SpotlightCard";

export const DEFAULT_WATCH_FOLDER = "C:\\Safi\\Output";

export function RecipesStudio(): ReactNode {
  const [name, setName] = useState("تنقية أسماء العملاء");
  const [toggles, setToggles] = useState<RecipeToggles>(DEFAULT_RECIPE_TOGGLES);
  const [stepOrder, setStepOrder] = useState<readonly RecipeStepId[]>(
    RECIPE_STEPS.map((s) => s.id)
  );
  const [folder, setFolder] = useState(DEFAULT_WATCH_FOLDER);
  const [exported, setExported] = useState<string | null>(null);

  const canonicalJson = useMemo(() => canonicalRecipeJson(name, toggles), [name, toggles]);
  const digest = useMemo(() => recipeDigest(name, toggles), [name, toggles]);
  const folderCheck = useMemo(() => validateWatchFolder(folder), [folder]);
  const enabled = enabledStepCount(toggles);

  const orderedSteps = useMemo(
    () =>
      stepOrder
        .map((id) => RECIPE_STEPS.find((s) => s.id === id))
        .filter((s): s is (typeof RECIPE_STEPS)[number] => s !== undefined),
    [stepOrder]
  );

  const toggleStep = (id: RecipeStepId): void => {
    setToggles((current) => ({ ...current, [id]: !current[id] }));
  };

  const moveStep = (index: number, delta: -1 | 1): void => {
    const target = index + delta;
    if (target < 0 || target >= stepOrder.length) return;
    const copy = [...stepOrder];
    const currentItem = copy[index];
    const targetItem = copy[target];
    if (currentItem === undefined || targetItem === undefined) return;
    copy[index] = targetItem;
    copy[target] = currentItem;
    setStepOrder(copy);
  };

  const handleName = (event: ChangeEvent<HTMLInputElement>): void => setName(event.target.value);
  const handleFolder = (event: ChangeEvent<HTMLInputElement>): void =>
    setFolder(event.target.value);

  const exportRecipe = (): void => {
    const fileName = `recipe-${digest.slice(0, 12)}.safirecipe.json`;
    if (downloadTextFile(fileName, canonicalJson, "application/json")) {
      setExported(fileName);
    }
  };

  const exportReport = (): void => {
    const html = buildRecipeReportHtml({
      recipe: buildCanonicalRecipe(name, toggles),
      digest,
      generatedAt: new Date().toISOString()
    });
    const fileName = `quality-report-${digest.slice(0, 12)}.html`;
    if (downloadTextFile(fileName, html, "text/html")) {
      setExported(fileName);
    }
  };

  return (
    <section className="safi-studio" aria-labelledby="safi-recipes-title" dir="rtl">
      <header className="safi-studio-header">
        <span className="safi-eyebrow">محرك الوصفات الحتمية</span>
        <h1 id="safi-recipes-title" className="safi-studio-title">
          الوصفات
        </h1>
        <p className="safi-studio-lead">
          ركّب الوصفة خطوة بخطوة مع بدائل التحريك بالنقر (WCAG 2.2 2.5.7). نفس الاختيارات تُنتج
          دائمًا نفس الملف ونفس البصمة، ما يجعل كل تصدير قابلًا للتدقيق.
        </p>
      </header>

      <div className="safi-grid-2">
        <SpotlightCard padding={24}>
          <label className="safi-field-label" htmlFor="safi-recipe-name">
            اسم الوصفة
          </label>
          <input
            id="safi-recipe-name"
            className="safi-input"
            value={name}
            onChange={handleName}
            maxLength={80}
          />

          <fieldset className="safi-fieldset">
            <legend className="safi-field-label">
              خطوات التنظيف (مع أزرار نقل لأعلى/لأسفل كبديل للسحب 2.5.7)
            </legend>
            {orderedSteps.map((step, idx) => (
              <div
                key={step.id}
                className="safi-toggle-row"
                style={{ justifyContent: "space-between", alignItems: "center" }}
              >
                <label
                  htmlFor={`safi-step-${step.id}`}
                  style={{ display: "flex", alignItems: "flex-start", gap: "10px", cursor: "pointer", flex: 1 }}
                >
                  <input
                    id={`safi-step-${step.id}`}
                    type="checkbox"
                    checked={toggles[step.id]}
                    onChange={() => toggleStep(step.id)}
                  />
                  <span>
                    <span className="safi-toggle-label">{step.labelAr}</span>
                    <span className="safi-toggle-detail">{step.detailAr}</span>
                  </span>
                </label>

                <div style={{ display: "flex", gap: "4px", flexShrink: 0 }}>
                  <button
                    type="button"
                    onClick={() => moveStep(idx, -1)}
                    disabled={idx === 0}
                    aria-label={`انقل ${step.labelAr} لأعلى`}
                    style={{ minHeight: "34px", minWidth: "34px", padding: "4px" }}
                  >
                    <ArrowUp size={13} aria-hidden="true" />
                  </button>
                  <button
                    type="button"
                    onClick={() => moveStep(idx, 1)}
                    disabled={idx === orderedSteps.length - 1}
                    aria-label={`انقل ${step.labelAr} لأسفل`}
                    style={{ minHeight: "34px", minWidth: "34px", padding: "4px" }}
                  >
                    <ArrowDown size={13} aria-hidden="true" />
                  </button>
                </div>
              </div>
            ))}
          </fieldset>
        </SpotlightCard>

        <SpotlightCard padding={24}>
          <h2 className="safi-card-title">
            <FileCode2 size={16} aria-hidden="true" /> الوصفة القانونية
          </h2>
          <p className="safi-hint" data-testid="safi-recipe-stats">
            خطوات مفعّلة: {enabled} من {RECIPE_STEPS.length}
          </p>
          <pre className="safi-pre safi-pre--json" dir="ltr" data-testid="safi-recipe-json">
            {canonicalJson}
          </pre>

          <span className="safi-mini-label">بصمة BLAKE3 للوصفة</span>
          <p className="safi-digest-line">
            <DecryptedMonoText value={digest} />
          </p>

          <label className="safi-field-label" htmlFor="safi-watch-folder">
            مجلد F1 المراقَب (مخرجات الوصفة)
          </label>
          <input
            id="safi-watch-folder"
            className="safi-input safi-mono"
            value={folder}
            onChange={handleFolder}
            dir="ltr"
            spellCheck={false}
          />
          <p
            className={
              folderCheck.ok ? "safi-status safi-status--ok" : "safi-status safi-status--bad"
            }
            role="status"
            data-testid="safi-folder-status"
          >
            {folderCheck.ok ? (
              <>
                <CheckCircle2 size={14} aria-hidden="true" /> مسار مطلق صالح:{" "}
                <span className="safi-mono">{folderCheck.normalized}</span>
              </>
            ) : (
              <>
                <XCircle size={14} aria-hidden="true" /> {folderCheck.code} ·{" "}
                {folderCheck.messageAr}
              </>
            )}
          </p>

          <div className="safi-actions">
            <MagneticButton
              onClick={exportRecipe}
              disabled={!folderCheck.ok}
              ariaLabel="تنزيل ملف الوصفة"
            >
              <Download size={16} aria-hidden="true" />
              تنزيل الوصفة
            </MagneticButton>
            <MagneticButton
              variant="glass"
              onClick={exportReport}
              ariaLabel="تصدير تقرير جودة الوصفة"
            >
              <FileCode2 size={16} aria-hidden="true" />
              تصدير تقرير الجودة
            </MagneticButton>
          </div>

          {exported !== null && (
            <p className="safi-hint" role="status">
              تم إنشاء ملف جديد: <span className="safi-mono">{exported}</span>
            </p>
          )}

          <p className="safi-hint">
            <FolderOpen size={13} aria-hidden="true" /> الكتابة الذرية (ملف مؤقت ← fsync ← إعادة
            تسمية) والمراقبة الفعلية للمجلد تُنفَّذ في نواة سطح المكتب، وترفض الروابط الرمزية
            والمسارات الخارجية عن المجلد الهدف.
          </p>
        </SpotlightCard>
      </div>

      <GraphifyLineageCanvas titleAr="مخطط Graphify التفاعلي لمسار تنفيذ الوصفة الحتمية (DAG)" />
    </section>
  );
}
