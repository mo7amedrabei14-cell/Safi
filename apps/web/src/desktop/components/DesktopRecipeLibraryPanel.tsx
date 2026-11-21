/**
 * `apps/web/src/desktop/components/DesktopRecipeLibraryPanel.tsx`
 *
 * Real Desktop UI for:
 * 1. Official Templates (synced from real `GET /v1/templates/official`, read-only locally).
 * 2. Customize & Save as New Recipe (`desktop_recipes_save_as_new` — NEVER mutates Official Template).
 * 3. Premium `.safitemplate` Export / Import gated by REAL Backend entitlement check
 *    (`POST /v1/templates/entitlement/export-import` -> `SF-RCP-005` for Free/Trial).
 * 4. `Share with SafiData` Community Contribution (local preview + PII/secret scan + mandatory
 *    explicit consent -> REAL Backend `POST /v1/community/submissions`).
 */
import type { FormEvent, ReactNode } from "react";
import { useCallback, useEffect, useState } from "react";
import {
  ArrowDown,
  ArrowUp,
  Copy,
  CopyPlus,
  Download,
  Edit3,
  Play,
  RefreshCw,
  Send,
  ShieldAlert,
  Sparkles,
  Trash2,
  Upload
} from "lucide-react";
import type { SafiLang } from "../../components/layout/FloatingGlassDock";
import { SafiAuthApiError } from "../../lib/auth-api";
import { ownerApi, type OwnerCommunitySubmission } from "../../lib/owner-api";
import {
  applyDesktopRecipeToProject,
  deleteDesktopRecipe,
  duplicateDesktopRecipe,
  exportDesktopSafiTemplate,
  importDesktopSafiTemplate,
  listDesktopRecipes,
  pickSafiTemplateOpenFile,
  pickSafiTemplateSaveFile,
  prepareDesktopCommunityShare,
  saveDesktopRecipeAsNew,
  syncDesktopOfficialTemplates,
  updateDesktopExistingRecipe,
  type DesktopCommunitySharePayload,
  type DesktopLocalRecipeRecord,
  type DesktopLocalRecipeStep,
  type DesktopStudioState
} from "../studio-bridge";

export interface DesktopRecipeLibraryPanelProps {
  readonly lang?: SafiLang;
  readonly sessionToken?: string | null;
  readonly readOnlyOffline?: boolean;
  readonly activeProjectId?: string | null;
  readonly onStateUpdated?: (state: DesktopStudioState) => void;
}

const DEFAULT_CUSTOM_STEPS: readonly DesktopLocalRecipeStep[] = [
  {
    id: "step_trim",
    op: "normalize_text",
    columns: ["all"],
    params: { trim: true },
    confidence: 1.0,
    reversible: true
  },
  {
    id: "step_dedup",
    op: "dedupe",
    columns: ["all"],
    params: {},
    confidence: 1.0,
    reversible: true
  }
];

export function DesktopRecipeLibraryPanel({
  lang = "en",
  sessionToken = null,
  readOnlyOffline = false,
  activeProjectId = null,
  onStateUpdated
}: DesktopRecipeLibraryPanelProps): ReactNode {
  const L = useCallback((ar: string, en: string): string => (lang === "ar" ? ar : en), [lang]);
  const [officialTemplates, setOfficialTemplates] = useState<readonly DesktopLocalRecipeRecord[]>(
    []
  );
  const [myRecipes, setMyRecipes] = useState<readonly DesktopLocalRecipeRecord[]>([]);
  const [mySubmissions, setMySubmissions] = useState<readonly OwnerCommunitySubmission[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  // Customize & Save as New Recipe / Edit Existing Recipe form state
  const [sourceTemplateId, setSourceTemplateId] = useState<string | null>(null);
  const [editingRecipeId, setEditingRecipeId] = useState<string | null>(null);
  const [draftName, setDraftName] = useState("");
  const [draftDescription, setDraftDescription] = useState("");
  const [draftCategory, setDraftCategory] = useState("finance");
  const [draftSteps, setDraftSteps] =
    useState<readonly DesktopLocalRecipeStep[]>(DEFAULT_CUSTOM_STEPS);
  const [conflictPolicy, setConflictPolicy] = useState<
    "fail_on_duplicate" | "rename" | "confirmed_replace"
  >("fail_on_duplicate");

  // `.safitemplate` path override (for keyboard accessibility & automated testing without modal hang)
  const [templateFilePath, setTemplateFilePath] = useState("");

  // `Share with SafiData` community contribution state
  const [shareRecipeId, setShareRecipeId] = useState<string | null>(null);
  const [sharePreview, setSharePreview] = useState<DesktopCommunitySharePayload | null>(null);
  const [shareConsent, setShareConsent] = useState(false);
  const [shareAttribution, setShareAttribution] = useState(true);

  const loadLocal = useCallback(async () => {
    try {
      const data = await listDesktopRecipes();
      setOfficialTemplates(data.officialTemplates);
      setMyRecipes(data.myRecipes);
    } catch {
      // Ignore before session is bound.
    }
  }, []);

  useEffect(() => {
    void loadLocal();
  }, [loadLocal]);

  const handleSyncOfficial = async (): Promise<void> => {
    if (!sessionToken) {
      setError(
        L("يجب تسجيل الدخول لمزامنة القوالب الرسمية.", "Sign in to sync official templates.")
      );
      return;
    }
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const serverTemplates = await ownerApi.listUserOfficialTemplates(sessionToken);
      const synced = await syncDesktopOfficialTemplates(
        serverTemplates.map((tpl) => {
          const currentVer =
            tpl.versions.find((v) => v.version_number === tpl.current_version) ??
            tpl.versions[tpl.versions.length - 1];
          return {
            id: tpl.id,
            name: tpl.name,
            description: tpl.description,
            category: tpl.category,
            recipe_type: tpl.recipe_type ?? "cleaning",
            origin: tpl.origin ?? "official_template",
            required_columns: tpl.required_columns ?? currentVer?.required_columns ?? [],
            steps: tpl.steps ?? currentVer?.steps ?? []
          };
        })
      );
      setOfficialTemplates(synced.officialTemplates);
      setMyRecipes(synced.myRecipes);
      try {
        const subs = await ownerApi.listMyCommunitySubmissions(sessionToken);
        setMySubmissions(subs);
      } catch {
        // Non-fatal if community list is empty.
      }
      setNotice(
        L(
          `تمت مزامنة ${synced.syncedCount} قالبًا رسميًا بنجاح.`,
          `Synced ${synced.syncedCount} official templates from SafiData.`
        )
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const beginCustomize = (tpl: DesktopLocalRecipeRecord): void => {
    setEditingRecipeId(null);
    setSourceTemplateId(tpl.id);
    setDraftName(`${tpl.name} — Custom`);
    setDraftDescription(tpl.description);
    setDraftCategory(tpl.category || "custom");
    setDraftSteps(tpl.recipe.steps.length > 0 ? tpl.recipe.steps : DEFAULT_CUSTOM_STEPS);
    setError(null);
    setNotice(
      L(
        `تم فتح نسخة عمل قابلة للتعديل من "${tpl.name}" دون المساس بالقالب الرسمي الأصلي.`,
        `Opened editable working copy of "${tpl.name}". Original official template remains immutable.`
      )
    );
  };

  const beginEditMyRecipe = (rec: DesktopLocalRecipeRecord): void => {
    setEditingRecipeId(rec.id);
    setSourceTemplateId(rec.source_template_id ?? null);
    setDraftName(rec.name);
    setDraftDescription(rec.description);
    setDraftCategory(rec.category || "custom");
    setDraftSteps(rec.recipe.steps.length > 0 ? rec.recipe.steps : DEFAULT_CUSTOM_STEPS);
    setError(null);
    setNotice(
      L(
        `جاري تعديل وصفتك الشخصية "${rec.name}" مباشرةً.`,
        `Editing your personal recipe "${rec.name}" in place.`
      )
    );
  };

  const addNormalizeStep = (): void => {
    setDraftSteps((prev) => [
      ...prev,
      {
        id: `step_ar_${prev.length + 1}`,
        op: "normalize_text",
        columns: ["all"],
        params: { trim: true, normalize_arabic: true },
        confidence: 1.0,
        reversible: true
      }
    ]);
  };

  const removeStepAt = (idx: number): void => {
    setDraftSteps((prev) => (prev.length <= 1 ? prev : prev.filter((_, i) => i !== idx)));
  };

  const moveStep = (idx: number, direction: -1 | 1): void => {
    setDraftSteps((prev) => {
      const target = idx + direction;
      if (target < 0 || target >= prev.length) {
        return prev;
      }
      const next = [...prev];
      const current = next[idx];
      const other = next[target];
      if (!current || !other) {
        return prev;
      }
      next[idx] = other;
      next[target] = current;
      return next;
    });
  };

  const handleSaveAsNew = async (event: FormEvent): Promise<void> => {
    event.preventDefault();
    if (draftName.trim().length === 0) {
      setError(L("أدخل اسمًا للوصفة الجديدة.", "Enter a name for the new recipe."));
      return;
    }
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const res = await saveDesktopRecipeAsNew({
        sourceTemplateId,
        name: draftName.trim(),
        description: draftDescription.trim() || "Custom SafiData Desktop recipe",
        category: draftCategory.trim() || "custom",
        recipeType: "cleaning",
        steps: draftSteps,
        conflictPolicy
      });
      setOfficialTemplates(res.officialTemplates);
      setMyRecipes(res.myRecipes);
      setEditingRecipeId(null);
      if (res.saved !== null) {
        setNotice(
          L(
            `تم حفظ "${res.saved.name}" في وصفاتي (My Recipes) كنسخة مستقلة.`,
            `Saved "${res.saved.name}" to My Recipes as an independent recipe.`
          )
        );
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleUpdateExisting = async (): Promise<void> => {
    if (!editingRecipeId) {
      return;
    }
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const res = await updateDesktopExistingRecipe({
        recipeId: editingRecipeId,
        name: draftName.trim(),
        description: draftDescription.trim() || "Updated SafiData recipe",
        steps: draftSteps
      });
      setOfficialTemplates(res.officialTemplates);
      setMyRecipes(res.myRecipes);
      setEditingRecipeId(null);
      if (res.updated !== null) {
        setNotice(
          L(
            `تم تحديث "${res.updated.name}" في وصفاتي بنجاح.`,
            `Updated "${res.updated.name}" in My Recipes.`
          )
        );
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleDuplicateMyRecipe = async (recipeId: string): Promise<void> => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const res = await duplicateDesktopRecipe(recipeId);
      setMyRecipes(res.myRecipes);
      if (res.duplicated !== null) {
        setNotice(
          L(
            `تم إنشاء نسخة مستقلة: "${res.duplicated.name}".`,
            `Created independent duplicate: "${res.duplicated.name}".`
          )
        );
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleApplyToActiveProject = async (recipe: DesktopLocalRecipeRecord): Promise<void> => {
    if (!activeProjectId) {
      return;
    }
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const updatedState = await applyDesktopRecipeToProject(activeProjectId, recipe.id);
      onStateUpdated?.(updatedState);
      setNotice(
        L(
          `تم تطبيق الوصفة "${recipe.name}" على ورقة العمل المفتوحة (${updatedState.rowCount} صف).`,
          `Applied recipe "${recipe.name}" to the active Studio sheet (${updatedState.rowCount} rows).`
        )
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const verifyPremiumEntitlement = async (): Promise<boolean> => {
    if (!sessionToken) {
      setError("SF-AUTH-001");
      return false;
    }
    try {
      const ent = await ownerApi.checkTemplateExportImportEntitlement(sessionToken);
      if (!ent.allowed) {
        setError("SF-RCP-005");
        return false;
      }
      return true;
    } catch (err) {
      if (err instanceof SafiAuthApiError) {
        setError(err.code ?? `HTTP ${err.status}`);
      } else {
        setError("SF-RCP-005");
      }
      return false;
    }
  };

  const handleExportSafiTemplate = async (recipe: DesktopLocalRecipeRecord): Promise<void> => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const entitled = await verifyPremiumEntitlement();
      if (!entitled) {
        return;
      }
      const target =
        templateFilePath.trim().length > 0
          ? templateFilePath.trim()
          : await pickSafiTemplateSaveFile(`${recipe.id}.safitemplate`);
      if (target === null || target.trim().length === 0) {
        return;
      }
      const res = await exportDesktopSafiTemplate(recipe.id, target, true);
      setTemplateFilePath(res.path);
      setNotice(
        L(
          `تم تصدير القالب المحمول إلى: ${res.path}`,
          `Exported portable .safitemplate to: ${res.path}`
        )
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleImportSafiTemplate = async (): Promise<void> => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const entitled = await verifyPremiumEntitlement();
      if (!entitled) {
        return;
      }
      const source =
        templateFilePath.trim().length > 0
          ? templateFilePath.trim()
          : await pickSafiTemplateOpenFile();
      if (source === null || source.trim().length === 0) {
        return;
      }
      const res = await importDesktopSafiTemplate(source, true, "rename");
      setMyRecipes(res.myRecipes);
      if (res.imported !== null) {
        setNotice(
          L(
            `تم استيراد "${res.imported.name}" بنجاح إلى وصفاتي.`,
            `Imported "${res.imported.name}" into My Recipes.`
          )
        );
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleOpenSharePreview = async (recipeId: string): Promise<void> => {
    setBusy(true);
    setError(null);
    setNotice(null);
    setShareConsent(false);
    try {
      const preview = await prepareDesktopCommunityShare(
        recipeId,
        true,
        "2026-11-v1",
        shareAttribution
      );
      setShareRecipeId(recipeId);
      setSharePreview(preview);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleSubmitCommunityShare = async (): Promise<void> => {
    if (!sessionToken || shareRecipeId === null || sharePreview === null) {
      return;
    }
    if (!shareConsent) {
      setError(
        L(
          "يجب تأكيد الموافقة الصريحة على شروط المساهمة قبل الإرسال (SF-RCP-006).",
          "Explicit contribution consent is required before submitting (SF-RCP-006)."
        )
      );
      return;
    }
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const payload = await prepareDesktopCommunityShare(
        shareRecipeId,
        true,
        "2026-11-v1",
        shareAttribution
      );
      const submitted = await ownerApi.submitCommunityTemplate(sessionToken, {
        name: payload.name,
        description: payload.description,
        recipe_type: payload.recipe_type,
        schema_version: payload.schema_version,
        app_version: payload.app_version,
        required_columns: payload.required_columns,
        steps: payload.steps,
        consent_accepted: true,
        consent_terms_version: payload.consent_terms_version,
        show_contributor_attribution: payload.show_contributor_attribution,
        pii_warnings: payload.pii_warnings
      });
      setShareRecipeId(null);
      setSharePreview(null);
      setShareConsent(false);
      setMySubmissions((prev) => [submitted, ...prev]);
      setNotice(
        L(
          `تم إرسال المساهمة للمراجعة بنجاح (${submitted.id}) — الحالة: ${submitted.status}`,
          `Community contribution submitted for Owner review (${submitted.id}) — status: ${submitted.status}`
        )
      );
    } catch (err) {
      if (err instanceof SafiAuthApiError) {
        setError(err.code ?? `HTTP ${err.status}`);
      } else {
        setError(err instanceof Error ? err.message : String(err));
      }
    } finally {
      setBusy(false);
    }
  };

  const handleDeleteRecipe = async (recipeId: string): Promise<void> => {
    setBusy(true);
    setError(null);
    try {
      const res = await deleteDesktopRecipe(recipeId, true);
      setMyRecipes(res.myRecipes);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section
      className="safi-panel"
      data-desktop-recipe-library="true"
      style={{ display: "grid", gap: "18px", padding: "18px" }}
    >
      <div
        style={{
          display: "flex",
          flexWrap: "wrap",
          justifyContent: "space-between",
          alignItems: "center",
          gap: "12px"
        }}
      >
        <div>
          <h2 style={{ margin: 0, display: "flex", alignItems: "center", gap: "8px" }}>
            <Sparkles size={18} aria-hidden="true" />
            <span>{L("مكتبة القوالب الرسمية والوصفات", "Official Templates & My Recipes")}</span>
          </h2>
          <p style={{ margin: "4px 0 0", fontSize: "13px", opacity: 0.82 }}>
            {L(
              "خصّص أي قالب رسمي واحفظه كوصفة جديدة مستقلة دون المساس بالأصل، وصدّر/استورد ملفات .safitemplate للحسابات المدفوعة، أو شارك وصفتك المنقّاة مع صافي.",
              "Customize any Official Template via Save as New Recipe without mutating the original, export/import portable .safitemplate files on Paid plans, or Share with SafiData."
            )}
          </p>
        </div>

        <button
          type="button"
          className="safi-btn safi-btn-primary"
          onClick={() => void handleSyncOfficial()}
          disabled={readOnlyOffline || busy || !sessionToken}
          data-recipes-sync-official-btn="true"
        >
          <RefreshCw size={15} aria-hidden="true" />
          <span>{L("مزامنة القوالب الرسمية", "Sync Official Templates")}</span>
        </button>
      </div>

      {error !== null ? (
        <div
          role="alert"
          data-recipes-error="true"
          style={{ display: "flex", alignItems: "center", gap: "8px", fontSize: "13px" }}
        >
          <ShieldAlert size={16} aria-hidden="true" />
          <span>{error}</span>
        </div>
      ) : null}

      {notice !== null ? (
        <div role="status" data-recipes-notice="true" style={{ fontSize: "13px", fontWeight: 600 }}>
          {notice}
        </div>
      ) : null}

      {/* Official Templates (Immutable locally) */}
      <div style={{ display: "grid", gap: "10px" }}>
        <h3 style={{ margin: 0, fontSize: "15px" }}>
          {L("القوالب الرسمية", "Official Templates")} ({officialTemplates.length})
        </h3>
        <div
          data-official-templates-list="true"
          style={{
            display: "grid",
            gap: "10px",
            gridTemplateColumns: "repeat(auto-fit, minmax(260px, 1fr))"
          }}
        >
          {officialTemplates.map((tpl) => (
            <article
              key={tpl.id}
              data-official-template-id={tpl.id}
              style={{
                padding: "12px",
                borderRadius: "10px",
                border: "1px solid var(--border-subtle, rgba(148,163,184,0.28))",
                display: "grid",
                gap: "6px"
              }}
            >
              <div style={{ fontWeight: 700, fontSize: "14px" }}>{tpl.name}</div>
              <div style={{ fontSize: "12px", opacity: 0.8 }}>{tpl.description}</div>
              <div style={{ fontSize: "11px", opacity: 0.7 }}>
                {tpl.origin} • {tpl.recipe.steps.length} {L("خطوات", "steps")}
              </div>
              <div style={{ display: "flex", flexWrap: "wrap", gap: "6px" }}>
                <button
                  type="button"
                  className="safi-btn safi-btn-ghost"
                  onClick={() => beginCustomize(tpl)}
                  disabled={readOnlyOffline || busy}
                  data-customize-official-btn={tpl.id}
                >
                  <CopyPlus size={14} aria-hidden="true" />
                  <span>{L("تخصيص وحفظ كوصفة جديدة", "Customize & Save as New")}</span>
                </button>
                {activeProjectId ? (
                  <button
                    type="button"
                    className="safi-btn safi-btn-ghost"
                    onClick={() => void handleApplyToActiveProject(tpl)}
                    disabled={readOnlyOffline || busy}
                    data-apply-recipe-btn={tpl.id}
                  >
                    <Play size={14} aria-hidden="true" />
                    <span>{L("تطبيق على الورقة", "Apply")}</span>
                  </button>
                ) : null}
              </div>
            </article>
          ))}
        </div>
      </div>

      {/* Customize & Save as New Recipe Editor */}
      <form
        onSubmit={(e) => void handleSaveAsNew(e)}
        data-save-as-new-form="true"
        style={{
          padding: "14px",
          borderRadius: "10px",
          border: "1px solid var(--border-subtle, rgba(148,163,184,0.28))",
          display: "grid",
          gap: "10px"
        }}
      >
        <h3 style={{ margin: 0, fontSize: "15px" }}>
          {L("حفظ كوصفة جديدة (Save as New Recipe)", "Save as New Recipe")}
        </h3>
        <div
          style={{
            display: "grid",
            gap: "8px",
            gridTemplateColumns: "repeat(auto-fit, minmax(200px, 1fr))"
          }}
        >
          <input
            type="text"
            value={draftName}
            onChange={(e) => setDraftName(e.target.value)}
            placeholder={L("اسم الوصفة الجديدة", "New recipe name")}
            className="safi-input"
            data-recipe-draft-name="true"
            disabled={readOnlyOffline || busy}
          />
          <input
            type="text"
            value={draftDescription}
            onChange={(e) => setDraftDescription(e.target.value)}
            placeholder={L("وصف مختصر", "Short description")}
            className="safi-input"
            data-recipe-draft-desc="true"
            disabled={readOnlyOffline || busy}
          />
          <select
            value={conflictPolicy}
            onChange={(e) =>
              setConflictPolicy(
                e.target.value as "fail_on_duplicate" | "rename" | "confirmed_replace"
              )
            }
            className="safi-input"
            aria-label={L("سياسة تكرار الاسم", "Duplicate name policy")}
            disabled={readOnlyOffline || busy}
          >
            <option value="fail_on_duplicate">
              {L("منع الاستبدال الصامت", "Fail on duplicate")}
            </option>
            <option value="rename">{L("إعادة تسمية تلقائية", "Auto-rename copy")}</option>
            <option value="confirmed_replace">{L("استبدال مؤكد", "Confirmed replace")}</option>
          </select>
        </div>
        <div style={{ display: "grid", gap: "4px" }}>
          {draftSteps.map((step, idx) => (
            <div
              key={step.id}
              data-recipe-draft-step={step.id}
              style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                fontSize: "12px",
                padding: "4px 8px",
                borderRadius: "6px",
                background: "rgba(148,163,184,0.08)"
              }}
            >
              <span>
                #{idx + 1} <strong>{step.id}</strong> —{" "}
                {typeof step.op === "string"
                  ? step.op
                  : ((step.op as { type?: string }).type ?? "normalize_text")}
              </span>
              <span style={{ display: "flex", gap: "4px" }}>
                <button
                  type="button"
                  className="safi-btn safi-btn-ghost"
                  onClick={() => moveStep(idx, -1)}
                  disabled={readOnlyOffline || busy || idx === 0}
                  aria-label={L("تحريك لأعلى", "Move step up")}
                >
                  <ArrowUp size={12} aria-hidden="true" />
                </button>
                <button
                  type="button"
                  className="safi-btn safi-btn-ghost"
                  onClick={() => moveStep(idx, 1)}
                  disabled={readOnlyOffline || busy || idx === draftSteps.length - 1}
                  aria-label={L("تحريك لأسفل", "Move step down")}
                >
                  <ArrowDown size={12} aria-hidden="true" />
                </button>
                <button
                  type="button"
                  className="safi-btn safi-btn-ghost"
                  onClick={() => removeStepAt(idx)}
                  disabled={readOnlyOffline || busy || draftSteps.length <= 1}
                  aria-label={L("حذف الخطوة", "Remove step")}
                >
                  <Trash2 size={12} aria-hidden="true" />
                </button>
              </span>
            </div>
          ))}
        </div>
        <div style={{ display: "flex", flexWrap: "wrap", gap: "8px", alignItems: "center" }}>
          <span style={{ fontSize: "12px", opacity: 0.8 }}>
            {L("الخطوات الحالية:", "Steps:")} {draftSteps.length}
          </span>
          <button
            type="button"
            className="safi-btn safi-btn-ghost"
            onClick={addNormalizeStep}
            disabled={readOnlyOffline || busy}
            data-recipe-add-step-btn="true"
          >
            {L("+ إضافة خطوة تطبيع عربي", "+ Add Arabic Normalize Step")}
          </button>
          {editingRecipeId !== null ? (
            <button
              type="button"
              className="safi-btn safi-btn-primary"
              onClick={() => void handleUpdateExisting()}
              disabled={readOnlyOffline || busy}
              data-recipe-update-existing-btn="true"
            >
              {L("حفظ التعديلات على الوصفة الحالية", "Update Existing Recipe")}
            </button>
          ) : null}
          <button
            type="submit"
            className="safi-btn safi-btn-primary"
            disabled={readOnlyOffline || busy}
            data-recipe-save-new-btn="true"
          >
            {L("حفظ كوصفة جديدة", "Save as New Recipe")}
          </button>
        </div>
      </form>

      {/* Portable `.safitemplate` Premium Export / Import bar */}
      <div
        style={{
          display: "flex",
          flexWrap: "wrap",
          gap: "8px",
          alignItems: "center",
          padding: "12px",
          borderRadius: "10px",
          border: "1px solid var(--border-subtle, rgba(148,163,184,0.28))"
        }}
      >
        <input
          type="text"
          value={templateFilePath}
          onChange={(e) => setTemplateFilePath(e.target.value)}
          placeholder={L(
            "مسار ملف .safitemplate (اختياري؛ يُفتح الحوار الأصلي إن تُرك فارغًا)",
            ".safitemplate path (optional; uses native file picker if blank)"
          )}
          className="safi-input"
          data-safitemplate-path-input="true"
          style={{ flex: "1 1 260px" }}
          disabled={readOnlyOffline || busy}
        />
        <button
          type="button"
          className="safi-btn safi-btn-ghost"
          onClick={() => void handleImportSafiTemplate()}
          disabled={readOnlyOffline || busy}
          data-safitemplate-import-btn="true"
        >
          <Upload size={14} aria-hidden="true" />
          <span>{L("استيراد .safitemplate", "Import .safitemplate")}</span>
        </button>
      </div>

      {/* My Recipes */}
      <div style={{ display: "grid", gap: "10px" }}>
        <h3 style={{ margin: 0, fontSize: "15px" }}>
          {L("وصفاتي المحلية (My Recipes)", "My Recipes")} ({myRecipes.length})
        </h3>
        <div data-my-recipes-list="true" style={{ display: "grid", gap: "10px" }}>
          {myRecipes.map((rec) => (
            <article
              key={rec.id}
              data-my-recipe-id={rec.id}
              style={{
                padding: "12px",
                borderRadius: "10px",
                border: "1px solid var(--border-subtle, rgba(148,163,184,0.28))",
                display: "flex",
                flexWrap: "wrap",
                justifyContent: "space-between",
                alignItems: "center",
                gap: "10px"
              }}
            >
              <div>
                <div style={{ fontWeight: 700, fontSize: "14px" }}>{rec.name}</div>
                <div style={{ fontSize: "12px", opacity: 0.78 }}>
                  {rec.origin} • {rec.recipe.steps.length} {L("خطوات", "steps")}
                  {rec.source_template_id
                    ? ` • ${L("مشتق من:", "from:")} ${rec.source_template_id}`
                    : ""}
                </div>
              </div>
              <div style={{ display: "flex", flexWrap: "wrap", gap: "6px" }}>
                {activeProjectId ? (
                  <button
                    type="button"
                    className="safi-btn safi-btn-primary"
                    onClick={() => void handleApplyToActiveProject(rec)}
                    disabled={readOnlyOffline || busy}
                    data-apply-recipe-btn={rec.id}
                  >
                    <Play size={14} aria-hidden="true" />
                    <span>{L("تطبيق على الورقة", "Apply")}</span>
                  </button>
                ) : null}
                <button
                  type="button"
                  className="safi-btn safi-btn-ghost"
                  onClick={() => beginEditMyRecipe(rec)}
                  disabled={readOnlyOffline || busy}
                  data-edit-my-recipe-btn={rec.id}
                >
                  <Edit3 size={14} aria-hidden="true" />
                  <span>{L("تعديل", "Edit")}</span>
                </button>
                <button
                  type="button"
                  className="safi-btn safi-btn-ghost"
                  onClick={() => void handleDuplicateMyRecipe(rec.id)}
                  disabled={readOnlyOffline || busy}
                  data-duplicate-recipe-btn={rec.id}
                >
                  <Copy size={14} aria-hidden="true" />
                  <span>{L("تكرار", "Duplicate")}</span>
                </button>
                <button
                  type="button"
                  className="safi-btn safi-btn-ghost"
                  onClick={() => void handleExportSafiTemplate(rec)}
                  disabled={readOnlyOffline || busy}
                  data-export-safitemplate-btn={rec.id}
                >
                  <Download size={14} aria-hidden="true" />
                  <span>{L("تصدير .safitemplate", "Export .safitemplate")}</span>
                </button>
                <button
                  type="button"
                  className="safi-btn safi-btn-ghost"
                  onClick={() => void handleOpenSharePreview(rec.id)}
                  disabled={readOnlyOffline || busy}
                  data-share-safidata-btn={rec.id}
                >
                  <Send size={14} aria-hidden="true" />
                  <span>{L("مشاركة مع صافي", "Share with SafiData")}</span>
                </button>
                <button
                  type="button"
                  className="safi-btn safi-btn-ghost"
                  onClick={() => void handleDeleteRecipe(rec.id)}
                  disabled={readOnlyOffline || busy}
                  data-delete-recipe-btn={rec.id}
                >
                  <Trash2 size={14} aria-hidden="true" />
                </button>
              </div>
            </article>
          ))}
        </div>
      </div>

      {/* Share with SafiData Preview & Consent Modal */}
      {sharePreview !== null ? (
        <div
          data-share-preview-modal="true"
          style={{
            padding: "16px",
            borderRadius: "12px",
            border: "2px solid var(--accent-primary, #10b981)",
            display: "grid",
            gap: "10px"
          }}
        >
          <h3 style={{ margin: 0, fontSize: "15px" }}>
            {L(
              "معاينة المساهمة قبل الإرسال — Share with SafiData",
              "Preview Before Sharing — Share with SafiData"
            )}
          </h3>
          <p style={{ margin: 0, fontSize: "12px", opacity: 0.85 }}>
            {L(
              "سيتم إرسال تعريف الوصفة التصريحي فقط للمراجعة من قِبل المالك. لا يتم إرسال أي صفوف أو ملفات عملاء إطلاقًا.",
              "Only the sanitized declarative recipe definition is submitted for Owner review. ZERO customer rows or files are ever uploaded."
            )}
          </p>
          <pre
            data-share-preview-json="true"
            style={{
              maxHeight: "180px",
              overflow: "auto",
              fontSize: "11px",
              padding: "10px",
              borderRadius: "8px",
              background: "rgba(15,23,42,0.12)"
            }}
          >
            {JSON.stringify(sharePreview, null, 2)}
          </pre>
          <label style={{ display: "flex", alignItems: "center", gap: "8px", fontSize: "13px" }}>
            <input
              type="checkbox"
              checked={shareConsent}
              onChange={(e) => setShareConsent(e.target.checked)}
              data-share-consent-checkbox="true"
            />
            <span>
              {L(
                "أوافق صراحةً على مشاركة هذه الوصفة التصريحية للمراجعة والنشر كقالب مجتمعي رسمي (2026-11-v1).",
                "I explicitly consent to share this declarative recipe for Owner review and optional official publication (2026-11-v1)."
              )}
            </span>
          </label>
          <label style={{ display: "flex", alignItems: "center", gap: "8px", fontSize: "12px" }}>
            <input
              type="checkbox"
              checked={shareAttribution}
              onChange={(e) => setShareAttribution(e.target.checked)}
            />
            <span>
              {L("إظهار اسم المساهم عند النشر", "Show contributor attribution if published")}
            </span>
          </label>
          <div style={{ display: "flex", gap: "8px" }}>
            <button
              type="button"
              className="safi-btn safi-btn-primary"
              onClick={() => void handleSubmitCommunityShare()}
              disabled={busy || !shareConsent}
              data-share-submit-btn="true"
            >
              <Send size={14} aria-hidden="true" />
              <span>{L("إرسال المساهمة للمراجعة", "Submit Contribution")}</span>
            </button>
            <button
              type="button"
              className="safi-btn safi-btn-ghost"
              onClick={() => {
                setSharePreview(null);
                setShareRecipeId(null);
              }}
            >
              {L("إلغاء", "Cancel")}
            </button>
          </div>
        </div>
      ) : null}

      {/* My Submitted Community Contributions */}
      {mySubmissions.length > 0 ? (
        <div data-my-community-submissions="true" style={{ display: "grid", gap: "6px" }}>
          <h3 style={{ margin: 0, fontSize: "14px" }}>
            {L("مساهماتي المرسلة للمراجعة", "My Community Submissions")} ({mySubmissions.length})
          </h3>
          {mySubmissions.map((sub) => (
            <div key={sub.id} style={{ fontSize: "12px", opacity: 0.88 }}>
              <strong>{sub.name}</strong> — {sub.status} ({sub.id})
            </div>
          ))}
        </div>
      ) : null}
    </section>
  );
}
