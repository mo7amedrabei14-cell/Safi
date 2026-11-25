/**
 * `apps/web/src/lib/team-shared-resources.ts`
 *
 * Authoritative codecs, privacy sanitizers, human-readable summaries, and structured
 * editors for sharing real SafiData Desktop Studio resources through the Zero-Knowledge
 * E2EE Organization Workspace (`ChaCha20-Poly1305` + `X25519`).
 *
 * LOCAL-FIRST PRIVACY BOUNDARY (Non-Negotiable):
 * - Shareable resources contain ONLY structured logic, schemas, layout specifications,
 *   metric formulas, and metadata:
 *     1. `recipe`    (`safi.shared_recipe.v1`)            — Saved cleaning/analysis steps
 *     2. `dashboard` (`safi.shared_dashboard.v1`)         — Chart definitions & layout config
 *     3. `document`  (`safi.shared_workflow.v1`)          — Sheet workflow memory & rules
 *     4. `project`   (`safi.shared_project_blueprint.v1`) — Sanitized project blueprint
 * - NEVER shareable by default:
 *     * Raw CSV/XLSX/ODS source files
 *     * Raw table rows / customer cell values / PII extracts
 *     * Local machine file paths or OS directory structures
 */
import type {
  DesktopLocalRecipeRecord,
  DesktopLocalRecipeStep,
  DesktopMetric,
  DesktopSharedWorkspaceResource,
  SavedCleaningOperation,
  SavedMetricDefinition,
  SheetWorkflowMemoryRecord,
  StudioChartArtifact,
  StudioDocBlockArtifact,
  StudioSlideArtifact,
  StudioTemplateConfigArtifact
} from "../desktop/studio-bridge";

export type ShareableResourceKind = "recipe" | "dashboard" | "document" | "project";

export interface SharedRecipePayloadV1 {
  readonly schema: "safi.shared_recipe.v1";
  readonly privacy_guarantee: "no_raw_data_rows";
  readonly source_recipe_id: string;
  readonly name: string;
  readonly description: string;
  readonly category: string;
  readonly recipe_type: string;
  readonly required_columns: readonly string[];
  readonly steps: readonly DesktopLocalRecipeStep[];
  readonly team_notes?: string;
}

export interface SharedDashboardPayloadV1 {
  readonly schema: "safi.shared_dashboard.v1";
  readonly privacy_guarantee: "no_raw_data_rows";
  readonly title: string;
  readonly charts: readonly StudioChartArtifact[];
  readonly template_config: StudioTemplateConfigArtifact | null;
  readonly team_notes?: string;
}

export interface SharedWorkflowPayloadV1 {
  readonly schema: "safi.shared_workflow.v1";
  readonly privacy_guarantee: "no_raw_data_rows";
  readonly source_workflow_id: string;
  readonly display_name: string;
  readonly business_domain: string;
  readonly headers: readonly string[];
  readonly duplicate_policy: string;
  readonly missing_value_policy: string;
  readonly business_rules: readonly string[];
  readonly cleaning_operations: readonly SavedCleaningOperation[];
  readonly metrics: readonly SavedMetricDefinition[];
  readonly team_notes?: string;
}

export interface SharedProjectBlueprintPayloadV1 {
  readonly schema: "safi.shared_project_blueprint.v1";
  readonly privacy_guarantee: "no_raw_data_rows";
  readonly title: string;
  readonly charts: readonly StudioChartArtifact[];
  readonly slides: readonly StudioSlideArtifact[];
  readonly doc_blocks: readonly StudioDocBlockArtifact[];
  readonly metrics: readonly SavedMetricDefinition[];
  readonly template_config: StudioTemplateConfigArtifact | null;
  readonly team_notes?: string;
}

export type SharedResourcePayload =
  | SharedRecipePayloadV1
  | SharedDashboardPayloadV1
  | SharedWorkflowPayloadV1
  | SharedProjectBlueprintPayloadV1;

export interface TeamShareDraft {
  readonly resourceKind: ShareableResourceKind;
  readonly resourceId: string;
  readonly title: string;
  readonly payload: SharedResourcePayload;
  readonly summaryEn: string;
  readonly summaryAr: string;
  readonly includedItemsEn: readonly string[];
  readonly includedItemsAr: readonly string[];
}

export interface ParsedSharedResourceView {
  readonly kind: ShareableResourceKind;
  readonly typeLabelEn: string;
  readonly typeLabelAr: string;
  readonly summaryEn: string;
  readonly summaryAr: string;
  readonly teamNotes: string;
  readonly privacyVerified: boolean;
  readonly typedPayload: SharedResourcePayload | null;
}

export function deriveSafeResourceId(prefix: string, rawKey: string): string {
  const slug = rawKey
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, "-")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "")
    .slice(0, 48);
  return `${prefix}-${slug || "item"}`;
}

export function buildSharedRecipeDraft(
  recipe: DesktopLocalRecipeRecord,
  teamNotes = ""
): TeamShareDraft {
  const stepCount = recipe.recipe.steps.length;
  const cols = recipe.required_columns.slice(0, 24);
  const payload: SharedRecipePayloadV1 = {
    schema: "safi.shared_recipe.v1",
    privacy_guarantee: "no_raw_data_rows",
    source_recipe_id: recipe.id,
    name: recipe.name,
    description: recipe.description,
    category: recipe.category,
    recipe_type: recipe.recipe_type,
    required_columns: cols,
    steps: recipe.recipe.steps.map((s) => ({
      id: s.id,
      op: s.op,
      ...(s.columns ? { columns: [...s.columns] } : {}),
      ...(s.params ? { params: { ...s.params } } : {}),
      ...(s.confidence !== undefined ? { confidence: s.confidence } : {}),
      ...(s.reversible !== undefined ? { reversible: s.reversible } : {}),
      ...(s.enabled !== undefined ? { enabled: s.enabled } : {})
    })),
    ...(teamNotes.trim() ? { team_notes: teamNotes.trim() } : {})
  };

  return {
    resourceKind: "recipe",
    resourceId: deriveSafeResourceId("recipe", recipe.id || recipe.name),
    title: recipe.name,
    payload,
    summaryEn: `${stepCount} cleaning ${stepCount === 1 ? "step" : "steps"}${cols.length > 0 ? ` · ${cols.length} expected columns` : ""}`,
    summaryAr: `${stepCount} خطوات تنظيف${cols.length > 0 ? ` · ${cols.length} أعمدة متوقعة` : ""}`,
    includedItemsEn: [
      `Recipe title & description ("${recipe.name}")`,
      `${stepCount} structured transformation steps`,
      cols.length > 0
        ? `Expected column names: ${cols.slice(0, 5).join(", ")}`
        : "Schema-agnostic steps",
      "0 raw CSV/XLSX data rows · 0 local file paths"
    ],
    includedItemsAr: [
      `اسم الوصفة ووصفها («${recipe.name}»)`,
      `${stepCount} خطوات تنظيف ومعالجة مهيكلة`,
      cols.length > 0
        ? `أسماء الأعمدة المتوقعة: ${cols.slice(0, 5).join("، ")}`
        : "خطوات مرنة دون أعمدة ثابتة",
      "0 صفوف بيانات عملاء · 0 مسارات ملفات محلية"
    ]
  };
}

export function buildSharedDashboardDraft(
  title: string,
  charts: readonly StudioChartArtifact[],
  templateConfig: StudioTemplateConfigArtifact | null = null,
  teamNotes = ""
): TeamShareDraft {
  const cleanTitle = title.trim() || "Team Dashboard";
  const sanitizedCharts: StudioChartArtifact[] = charts.map((c) => ({
    id: c.id,
    title: c.title,
    ...(c.chartType ? { chartType: c.chartType } : {}),
    function: c.function,
    column: c.column,
    groupBy: c.groupBy
  }));
  const payload: SharedDashboardPayloadV1 = {
    schema: "safi.shared_dashboard.v1",
    privacy_guarantee: "no_raw_data_rows",
    title: cleanTitle,
    charts: sanitizedCharts,
    template_config: templateConfig
      ? {
          templateId: templateConfig.templateId,
          titleOverride: templateConfig.titleOverride,
          bindings: { ...templateConfig.bindings },
          sectionOverrides: { ...templateConfig.sectionOverrides }
        }
      : null,
    ...(teamNotes.trim() ? { team_notes: teamNotes.trim() } : {})
  };

  return {
    resourceKind: "dashboard",
    resourceId: deriveSafeResourceId("dashboard", cleanTitle),
    title: cleanTitle,
    payload,
    summaryEn: `${sanitizedCharts.length} interactive ${sanitizedCharts.length === 1 ? "chart" : "charts"} definition`,
    summaryAr: `تعريف لوحة تحتوي على ${sanitizedCharts.length} رسوم بيانية تفاعلية`,
    includedItemsEn: [
      `Dashboard title ("${cleanTitle}")`,
      `${sanitizedCharts.length} chart specifications (type, aggregation, column mapping)`,
      templateConfig ? `Template layout (${templateConfig.templateId})` : "Standard reactive grid",
      "0 raw cell values · Charts compute locally on each teammate's data"
    ],
    includedItemsAr: [
      `عنوان اللوحة («${cleanTitle}»)`,
      `${sanitizedCharts.length} مواصفات رسوم بيانية (النوع، التجميع، الأعمدة)`,
      templateConfig
        ? `إعدادات القالب (${templateConfig.templateId})`
        : "تخطيط الشبكة التفاعلية القياسي",
      "0 قيم خلايا خام · تُحسب الرسوم محليًا على جهاز كل عضو"
    ]
  };
}

export function buildSharedWorkflowDraft(
  workflow: SheetWorkflowMemoryRecord,
  teamNotes = ""
): TeamShareDraft {
  const headers =
    workflow.columnDecisions.length > 0
      ? workflow.columnDecisions.map((c) => c.headerName)
      : workflow.signature.canonicalConcepts;
  const payload: SharedWorkflowPayloadV1 = {
    schema: "safi.shared_workflow.v1",
    privacy_guarantee: "no_raw_data_rows",
    source_workflow_id: workflow.workflowId,
    display_name: workflow.displayName,
    business_domain: workflow.businessDomain,
    headers: [...headers],
    duplicate_policy: workflow.duplicatePolicy,
    missing_value_policy: workflow.missingValuePolicy,
    business_rules: [...workflow.businessRules],
    cleaning_operations: workflow.cleaningOperations.map((op) => ({ ...op })),
    metrics: workflow.metrics.map((m) => ({ ...m })),
    ...(teamNotes.trim() ? { team_notes: teamNotes.trim() } : {})
  };

  return {
    resourceKind: "document",
    resourceId: deriveSafeResourceId("workflow", workflow.workflowId || workflow.displayName),
    title: workflow.displayName,
    payload,
    summaryEn: `Sheet workflow (${workflow.businessDomain}) · ${workflow.cleaningOperations.length} ops · ${workflow.metrics.length} metrics`,
    summaryAr: `مسار ورقة (${workflow.businessDomain}) · ${workflow.cleaningOperations.length} عمليات · ${workflow.metrics.length} مؤشرات`,
    includedItemsEn: [
      `Workflow name & domain (${workflow.businessDomain})`,
      `Duplicate policy (${workflow.duplicatePolicy}) & missing-value policy (${workflow.missingValuePolicy})`,
      `${workflow.cleaningOperations.length} cleaning operations & ${workflow.metrics.length} metric formulas`,
      "0 raw spreadsheet rows · 0 customer PII"
    ],
    includedItemsAr: [
      `اسم مسار العمل والمجال (${workflow.businessDomain})`,
      `سياسة التكرار (${workflow.duplicatePolicy}) والقيم المفقودة (${workflow.missingValuePolicy})`,
      `${workflow.cleaningOperations.length} عمليات تنظيف و${workflow.metrics.length} معادلات مؤشرات`,
      "0 صفوف بيانات خام · 0 بيانات عملاء شخصية"
    ]
  };
}

export function buildSharedProjectBlueprintDraft(params: {
  readonly title: string;
  readonly charts: readonly StudioChartArtifact[];
  readonly slides: readonly StudioSlideArtifact[];
  readonly docBlocks: readonly StudioDocBlockArtifact[];
  readonly metrics: readonly DesktopMetric[];
  readonly templateConfig: StudioTemplateConfigArtifact | null;
  readonly teamNotes?: string;
}): TeamShareDraft {
  const cleanTitle = params.title.trim() || "Project Blueprint";
  const savedMetrics: SavedMetricDefinition[] = params.metrics.map((m) => ({
    id: m.id,
    name: m.name,
    unit: m.unit ?? null,
    formula: m.formula
  }));
  const payload: SharedProjectBlueprintPayloadV1 = {
    schema: "safi.shared_project_blueprint.v1",
    privacy_guarantee: "no_raw_data_rows",
    title: cleanTitle,
    charts: params.charts.map((c) => ({ ...c })),
    slides: params.slides.map((s) => ({ ...s })),
    doc_blocks: params.docBlocks.map((d) => ({ ...d })),
    metrics: savedMetrics,
    template_config: params.templateConfig ? { ...params.templateConfig } : null,
    ...(params.teamNotes?.trim() ? { team_notes: params.teamNotes.trim() } : {})
  };

  return {
    resourceKind: "project",
    resourceId: deriveSafeResourceId("project", cleanTitle),
    title: cleanTitle,
    payload,
    summaryEn: `Blueprint · ${params.charts.length} charts · ${params.slides.length} slides · ${params.docBlocks.length} doc sections`,
    summaryAr: `مخطط مشروع · ${params.charts.length} رسوم · ${params.slides.length} شرائح · ${params.docBlocks.length} أقسام مستند`,
    includedItemsEn: [
      `Blueprint structure: ${params.charts.length} charts, ${params.slides.length} slides, ${params.docBlocks.length} doc blocks`,
      `${savedMetrics.length} verified metric formulas`,
      "Source CSV/XLSX files & raw table rows are EXCLUDED for local-first privacy"
    ],
    includedItemsAr: [
      `هيكل المشروع: ${params.charts.length} رسوم، ${params.slides.length} شرائح، ${params.docBlocks.length} أقسام مستند`,
      `${savedMetrics.length} معادلات مؤشرات موثقة`,
      "ملفات CSV/XLSX الأصلية وصفوف الجدول الخام مستبعدة تمامًا لحماية الخصوصية المحلية"
    ]
  };
}

export function parseSharedResourcePayload(
  resource: Pick<DesktopSharedWorkspaceResource, "resource_kind" | "payload_json">
): ParsedSharedResourceView {
  const fallbackKind = (
    resource.resource_kind === "recipe" ||
    resource.resource_kind === "dashboard" ||
    resource.resource_kind === "project" ||
    resource.resource_kind === "document"
      ? resource.resource_kind
      : "document"
  ) satisfies ShareableResourceKind;

  try {
    const parsed = JSON.parse(resource.payload_json) as Partial<SharedResourcePayload>;
    if (parsed && parsed.schema === "safi.shared_recipe.v1" && Array.isArray(parsed.steps)) {
      const p = parsed as SharedRecipePayloadV1;
      const n = p.steps.length;
      return {
        kind: "recipe",
        typeLabelEn: "Saved Recipe",
        typeLabelAr: "وصفة تنظيف",
        summaryEn: `${n} cleaning ${n === 1 ? "step" : "steps"}${p.required_columns.length > 0 ? ` · columns: ${p.required_columns.slice(0, 3).join(", ")}` : ""}`,
        summaryAr: `${n} خطوات تنظيف${p.required_columns.length > 0 ? ` · الأعمدة: ${p.required_columns.slice(0, 3).join("، ")}` : ""}`,
        teamNotes: p.team_notes ?? p.description ?? "",
        privacyVerified: p.privacy_guarantee === "no_raw_data_rows",
        typedPayload: p
      };
    }

    if (parsed && parsed.schema === "safi.shared_dashboard.v1" && Array.isArray(parsed.charts)) {
      const p = parsed as SharedDashboardPayloadV1;
      const n = p.charts.length;
      return {
        kind: "dashboard",
        typeLabelEn: "Dashboard",
        typeLabelAr: "لوحة مؤشرات",
        summaryEn: `${n} interactive ${n === 1 ? "chart" : "charts"}${p.template_config ? ` · template ${p.template_config.templateId}` : ""}`,
        summaryAr: `${n} رسوم بيانية تفاعلية${p.template_config ? ` · قالب ${p.template_config.templateId}` : ""}`,
        teamNotes: p.team_notes ?? "",
        privacyVerified: p.privacy_guarantee === "no_raw_data_rows",
        typedPayload: p
      };
    }

    if (parsed && parsed.schema === "safi.shared_workflow.v1") {
      const p = parsed as SharedWorkflowPayloadV1;
      return {
        kind: "document",
        typeLabelEn: "Sheet Workflow",
        typeLabelAr: "مسار عمل ورقة",
        summaryEn: `Domain: ${p.business_domain} · ${p.cleaning_operations.length} ops · ${p.metrics.length} metrics`,
        summaryAr: `المجال: ${p.business_domain} · ${p.cleaning_operations.length} عمليات · ${p.metrics.length} مؤشرات`,
        teamNotes: p.team_notes ?? "",
        privacyVerified: p.privacy_guarantee === "no_raw_data_rows",
        typedPayload: p
      };
    }

    if (parsed && parsed.schema === "safi.shared_project_blueprint.v1") {
      const p = parsed as SharedProjectBlueprintPayloadV1;
      return {
        kind: "project",
        typeLabelEn: "Project Blueprint",
        typeLabelAr: "مخطط مشروع",
        summaryEn: `${p.charts.length} charts · ${p.slides.length} slides · ${p.doc_blocks.length} doc sections`,
        summaryAr: `${p.charts.length} رسوم · ${p.slides.length} شرائح · ${p.doc_blocks.length} أقسام مستند`,
        teamNotes: p.team_notes ?? "",
        privacyVerified: p.privacy_guarantee === "no_raw_data_rows",
        typedPayload: p
      };
    }
  } catch {
    // Non-standard or legacy encrypted payload; render gracefully without crashing.
  }

  const labelMap: Record<ShareableResourceKind, { en: string; ar: string }> = {
    recipe: { en: "Recipe", ar: "وصفة" },
    dashboard: { en: "Dashboard", ar: "لوحة" },
    project: { en: "Project Blueprint", ar: "مخطط مشروع" },
    document: { en: "Workflow / Document", ar: "مسار / مستند" }
  };

  return {
    kind: fallbackKind,
    typeLabelEn: labelMap[fallbackKind].en,
    typeLabelAr: labelMap[fallbackKind].ar,
    summaryEn: "Encrypted team workspace resource",
    summaryAr: "مورد مشفّر في مساحة عمل الفريق",
    teamNotes: "",
    privacyVerified: false,
    typedPayload: null
  };
}

/**
 * Produces an updated payload JSON when a permitted team member edits a shared
 * resource's title, team notes, or (for recipes) appends a cleaning step via the UI.
 */
export function buildEditedSharedResourcePayload(
  resource: DesktopSharedWorkspaceResource,
  updates: {
    readonly title: string;
    readonly teamNotes: string;
    readonly appendNormalizeStep?: boolean;
  }
): string {
  const parsed = parseSharedResourcePayload(resource);
  const cleanTitle = updates.title.trim() || resource.title;
  const cleanNotes = updates.teamNotes.trim();

  if (parsed.typedPayload?.schema === "safi.shared_recipe.v1") {
    const nextSteps = updates.appendNormalizeStep
      ? [
          ...parsed.typedPayload.steps,
          {
            id: `team_step_${parsed.typedPayload.steps.length + 1}`,
            op: { type: "normalize_arabic" },
            columns: parsed.typedPayload.required_columns.slice(0, 1),
            reversible: true,
            enabled: true
          }
        ]
      : parsed.typedPayload.steps;

    const updated: SharedRecipePayloadV1 = {
      ...parsed.typedPayload,
      name: cleanTitle,
      steps: nextSteps,
      ...(cleanNotes ? { team_notes: cleanNotes } : {})
    };
    return JSON.stringify(updated);
  }

  if (parsed.typedPayload?.schema === "safi.shared_dashboard.v1") {
    const updated: SharedDashboardPayloadV1 = {
      ...parsed.typedPayload,
      title: cleanTitle,
      ...(cleanNotes ? { team_notes: cleanNotes } : {})
    };
    return JSON.stringify(updated);
  }

  if (parsed.typedPayload?.schema === "safi.shared_workflow.v1") {
    const updated: SharedWorkflowPayloadV1 = {
      ...parsed.typedPayload,
      display_name: cleanTitle,
      ...(cleanNotes ? { team_notes: cleanNotes } : {})
    };
    return JSON.stringify(updated);
  }

  if (parsed.typedPayload?.schema === "safi.shared_project_blueprint.v1") {
    const updated: SharedProjectBlueprintPayloadV1 = {
      ...parsed.typedPayload,
      title: cleanTitle,
      ...(cleanNotes ? { team_notes: cleanNotes } : {})
    };
    return JSON.stringify(updated);
  }

  try {
    const generic = JSON.parse(resource.payload_json) as Record<string, unknown>;
    return JSON.stringify({
      ...generic,
      title: cleanTitle,
      ...(cleanNotes ? { team_notes: cleanNotes } : {})
    });
  } catch {
    return JSON.stringify({
      title: cleanTitle,
      team_notes: cleanNotes
    });
  }
}
