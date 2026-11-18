/**
 * Platform Governance Engine (`Phases 10, 11, 12`):
 * - Phase 10: Unified Bilingual Tooltip System, Contextual Help & Interactive Replayable Tutorials
 * - Phase 11: Privacy, Explicit External AI Consent Gate, PII Masking & Zero-Data Log Sanitizer
 * - Phase 12: Cancellable Chunked Processing, Virtual Windowing & Performance Benchmark Harness
 */

import { ingestWorkbookStructure } from "./workbook-ingestion";
import { buildSemanticWorkbookModel } from "./semantic-intelligence";

// ============================================================================
// PHASE 10: UNIFIED TOOLTIPS, CONTEXTUAL HELP & REPLAYABLE TUTORIALS
// ============================================================================

export interface SafiTooltipDescriptor {
  readonly tooltipId: string;
  readonly category:
    | "Ingestion"
    | "Epistemic"
    | "Confidence"
    | "DataQuality"
    | "Dashboard"
    | "Presentation"
    | "Document"
    | "Export"
    | "Privacy";
  readonly titleEn: string;
  readonly titleAr: string;
  readonly whatItDoesEn: string;
  readonly whatItDoesAr: string;
  readonly whenToUseEn: string;
  readonly whenToUseAr: string;
  readonly impactEn: string;
  readonly impactAr: string;
}

export const UNIFIED_TOOLTIP_CATALOG: Readonly<Record<string, SafiTooltipDescriptor>> = {
  "epistemic.ConfirmedFact": {
    tooltipId: "epistemic.ConfirmedFact",
    category: "Epistemic",
    titleEn: "Confirmed Fact",
    titleAr: "حقيقة مؤكدة",
    whatItDoesEn:
      "Marks an exact mathematical total, count, or formula result verified directly from the workbook.",
    whatItDoesAr: "يشير إلى نتيجة حسابية دقيقة أو معادلة تم التحقق منها مباشرة من ملف البيانات.",
    whenToUseEn: "Use for executive reporting, financial disclosures, and board KPIs.",
    whenToUseAr: "يُستخدم في التقارير التنفيذية والإفصاحات المالية ومؤشرات الأداء المعتمدة.",
    impactEn: "Guaranteed 100% traceable to source worksheet cells and formulas.",
    impactAr: "موثوق وقابل للتتبع بنسبة 100% إلى الخلايا والمعادلات الأصلية."
  },
  "epistemic.DataSupportedObservation": {
    tooltipId: "epistemic.DataSupportedObservation",
    category: "Epistemic",
    titleEn: "Data-Supported Observation",
    titleAr: "ملاحظة مدعومة بالبيانات",
    whatItDoesEn:
      "Highlights a statistical pattern, concentration, correlation, or trend observed in the dataset.",
    whatItDoesAr:
      "يوضح نمطًا إحصائيًا أو تركزًا أو ارتباطًا أو اتجاهًا زمنيًا مرصودًا في البيانات.",
    whenToUseEn:
      "Use to identify trends, anomalies, and co-movements across categories or periods.",
    whenToUseAr: "يُستخدم لاكتشاف الاتجاهات والقيم المتطرفة والارتباطات بين الفئات أو الفترات.",
    impactEn: "Reflects empirical association without claiming unverified external causation.",
    impactAr: "يعكس ارتباطًا رقميًا فعليًا دون ادعاء أسباب خارجية غير مثبتة."
  },
  "epistemic.PotentialExplanation": {
    tooltipId: "epistemic.PotentialExplanation",
    category: "Epistemic",
    titleEn: "Potential Explanation (Hypothesis)",
    titleAr: "تفسير محتمل (فرضية)",
    whatItDoesEn:
      "Presents a plausible domain hypothesis consistent with the data that requires operational verification.",
    whatItDoesAr: "يعرض فرضية محتملة متسقة مع حركة الأرقام ولكنها تحتاج إلى تحقق تشغيلي.",
    whenToUseEn: "Use as a starting point for root-cause investigation with operational teams.",
    whenToUseAr: "يُستخدم كنقطة انطلاق للتحقق من الأسباب الجذرية مع فرق العمل.",
    impactEn:
      "Clearly separated from confirmed facts so hypotheses are never presented as certainties.",
    impactAr: "مفصول بوضوح عن الحقائق المؤكدة لمنع الخلط بين الفرضيات واليقينيات."
  },
  "epistemic.UnresolvedQuestion": {
    tooltipId: "epistemic.UnresolvedQuestion",
    category: "Epistemic",
    titleEn: "Unresolved Clarification Question",
    titleAr: "سؤال توضيحي غير محسوم",
    whatItDoesEn:
      "Indicates an analytical ambiguity (such as an unconfirmed column role or skipped question).",
    whatItDoesAr: "يشير إلى غموض تحليلي (مثل دور عمود غير مؤكد أو سؤال تم تخطيه).",
    whenToUseEn:
      "Answer or confirm the question to upgrade dependent metrics from provisional to verified.",
    whenToUseAr: "أجب عن السؤال لترقية المؤشرات المعتمدة عليه من مؤقتة إلى مؤكدة.",
    impactEn: "Dependent KPIs and charts carry a Provisional badge until resolved.",
    impactAr: "تحمل المؤشرات المعتمدة عليه شارة (مؤقت) حتى يتم حسمه."
  },
  "epistemic.UnsupportedConclusion": {
    tooltipId: "epistemic.UnsupportedConclusion",
    category: "Epistemic",
    titleEn: "Epistemic Boundary (Cannot Be Concluded)",
    titleAr: "حدود الاستنتاج (لا يمكن الجزم به)",
    whatItDoesEn:
      "Explicitly states causal or external claims that cannot be proven from this workbook alone.",
    whatItDoesAr:
      "يوضح صراحة الاستنتاجات السببية أو الخارجية التي لا يمكن إثباتها من هذا الملف وحده.",
    whenToUseEn: "Include in methodology and executive briefings to prevent over-interpretation.",
    whenToUseAr: "يُدرج في قسم المنهجية والعروض التنفيذية لمنع التفسير المبالغ فيه.",
    impactEn: "Protects decision-makers from unsupported causal leaps.",
    impactAr: "يحمي متخذي القرار من القفز إلى استنتاجات غير مدعومة."
  },
  "confidence.High": {
    tooltipId: "confidence.High",
    category: "Confidence",
    titleEn: "High Confidence (≥ 85%)",
    titleAr: "ثقة عالية (≥ 85%)",
    whatItDoesEn: "Indicates unambiguous data types, clean headers, and verified formula lineage.",
    whatItDoesAr: "يشير إلى وضوح نوع البيانات وعناوين الأعمدة وتطابق المعادلات.",
    whenToUseEn: "Safe for automated dashboarding and executive exports.",
    whenToUseAr: "آمن للتوليد التلقائي للوحات المؤشرات والتقارير التنفيذية.",
    impactEn: "Applied automatically without requiring manual clarification.",
    impactAr: "يُطبّق تلقائيًا دون الحاجة إلى تدخل يدوي."
  },
  "confidence.Low": {
    tooltipId: "confidence.Low",
    category: "Confidence",
    titleEn: "Low / Provisional Confidence (< 65%)",
    titleAr: "ثقة منخفضة / مؤقتة (< 65%)",
    whatItDoesEn:
      "Signals ambiguous column semantics, partial key overlap, or skipped clarification questions.",
    whatItDoesAr: "ينبه إلى غموض في دلالة العمود أو تطابق جزئي في المفاتيح أو سؤال تم تخطيه.",
    whenToUseEn: "Review in the Contextual Questions panel to confirm the intended meaning.",
    whenToUseAr: "راجعه في لوحة الأسئلة السياقية لتأكيد المعنى المقصود.",
    impactEn: "Flags dependent numbers as Provisional across Dashboard, Slides, and Document.",
    impactAr: "يميز الأرقام المعتمدة عليه كأرقام مؤقتة في جميع الاستوديوهات."
  },
  "studio.CrossFilter": {
    tooltipId: "studio.CrossFilter",
    category: "Dashboard",
    titleEn: "Interactive Cross-Filtering & Drill-Down",
    titleAr: "التصفية التفاعلية المتقاطعة والتعمق",
    whatItDoesEn:
      "Clicking any bar, slice, or category filters sibling charts and drills down into sub-dimensions.",
    whatItDoesAr: "النقر على أي فئة يصفي الرسوم البيانية الأخرى ويتعمق في الأبعاد الفرعية.",
    whenToUseEn:
      "Use during live exploration to isolate a specific branch, product, or time period.",
    whenToUseAr: "يُستخدم أثناء الاستكشاف المباشر لعزل فرع أو منتج أو فترة زمنية محددة.",
    impactEn: "Recomputes all dashboard visuals instantaneously from the canonical semantic model.",
    impactAr: "يعيد حساب جميع الرسوم البيانية فورًا من النموذج الدلالي الموحد."
  },
  "export.GenuineOoxml": {
    tooltipId: "export.GenuineOoxml",
    category: "Export",
    titleEn: "Genuine OOXML & Vector Export",
    titleAr: "تصدير ملفات أوفيس ومتجهات حقيقية",
    whatItDoesEn:
      "Packages real .xlsx, .pptx, .docx, .pdf, .png, .svg, .csv, .safi.html, and Power BI bundles locally.",
    whatItDoesAr: "ينشئ ملفات Excel وPowerPoint وWord وPDF وPNG وSVG وPower BI حقيقية محليًا.",
    whenToUseEn:
      "Use when sharing deliverables with stakeholders or importing into Microsoft Office / Power BI.",
    whenToUseAr: "يُستخدم عند مشاركة المخرجات مع الإدارة أو فتحها في برامج أوفيس وPower BI.",
    impactEn: "Writes new atomic files without ever modifying your original source workbook.",
    impactAr: "ينشئ ملفات جديدة بالكامل دون المساس بملفك الأصلي على الإطلاق."
  }
};

export function getTooltipDescriptor(
  tooltipId: string,
  lang: "en" | "ar" = "en"
): {
  readonly id: string;
  readonly title: string;
  readonly whatItDoes: string;
  readonly whenToUse: string;
  readonly impact: string;
} | null {
  const entry = UNIFIED_TOOLTIP_CATALOG[tooltipId];
  if (!entry) return null;
  const isAr = lang === "ar";
  return {
    id: entry.tooltipId,
    title: isAr ? entry.titleAr : entry.titleEn,
    whatItDoes: isAr ? entry.whatItDoesAr : entry.whatItDoesEn,
    whenToUse: isAr ? entry.whenToUseAr : entry.whenToUseEn,
    impact: isAr ? entry.impactAr : entry.impactEn
  };
}

export type TutorialStepId =
  | "step_1_import"
  | "step_2_sheets_tables"
  | "step_3_quality_cleaning"
  | "step_4_questions_assistant"
  | "step_5_dashboard_studio"
  | "step_6_presentation_studio"
  | "step_7_document_studio"
  | "step_8_professional_exports";

export interface TutorialStepDefinition {
  readonly stepId: TutorialStepId;
  readonly stepNumber: number;
  readonly targetSelector: string;
  readonly titleEn: string;
  readonly titleAr: string;
  readonly bodyEn: string;
  readonly bodyAr: string;
}

export const INTERACTIVE_TUTORIAL_STEPS: readonly TutorialStepDefinition[] = [
  {
    stepId: "step_1_import",
    stepNumber: 1,
    targetSelector: "[data-tutorial-target='import']",
    titleEn: "1. Universal Workbook Import",
    titleAr: "1. استيراد ملفات إكسل والبيانات الشامل",
    bodyEn:
      "Open XLSX, XLS, XLSM, XLSB, CSV, or ODS files locally. Macros are neutralized automatically and original files are never modified.",
    bodyAr:
      "افتح ملفات XLSX وXLS وXLSM وXLSB وCSV وODS محليًا. يتم تحييد الماكرو تلقائيًا ولا يتم تعديل ملفك الأصلي أبدًا."
  },
  {
    stepId: "step_2_sheets_tables",
    stepNumber: 2,
    targetSelector: "[data-tutorial-target='sheets-tables']",
    titleEn: "2. Multi-Sheet & Multi-Table Discovery",
    titleAr: "2. اكتشاف الأوراق والجداول المتعددة",
    bodyEn:
      "Review visible and hidden worksheets, separated title banners, multi-row headers, and isolated subtotal/grand-total rows.",
    bodyAr:
      "راجع الأوراق المرئية والمخفية، وعناوين الجداول متعددة الصفوف، وصفوف الإجماليات المفصولة تلقائيًا لمنع ازدواجية الحساب."
  },
  {
    stepId: "step_3_quality_cleaning",
    stepNumber: 3,
    targetSelector: "[data-tutorial-target='quality-cleaning']",
    titleEn: "3. Data Quality & Reversible Cleaning",
    titleAr: "3. جودة البيانات والتنظيف القابل للتراجع",
    bodyEn:
      "Inspect Confirmed Issues, Potential Issues, and Outliers, and preview reversible cleaning actions before applying.",
    bodyAr:
      "افحص المشكلات المؤكدة والمحتملة والقيم المتطرفة، وعاين خطوات التنظيف القابلة للتراجع قبل تطبيقها."
  },
  {
    stepId: "step_4_questions_assistant",
    stepNumber: 4,
    targetSelector: "[data-tutorial-target='questions-assistant']",
    titleEn: "4. Contextual Questions & Data Assistant",
    titleAr: "4. الأسئلة السياقية والمساعد التحليلي",
    bodyEn:
      "Answer, Skip, or Defer high-impact clarification questions and ask grounded natural-language questions citing exact cells.",
    bodyAr:
      "أجب أو تخطَّ أو أجّل أسئلة التوضيح الجوهرية، واطرح أسئلة تحليلية مدعومة بمصادر الخلايا والمعادلات."
  },
  {
    stepId: "step_5_dashboard_studio",
    stepNumber: 5,
    targetSelector: "[data-tutorial-target='dashboard-studio']",
    titleEn: "5. Dashboard Studio (21 Chart Types)",
    titleAr: "5. استوديو لوحات المؤشرات (21 نوع رسم بياني)",
    bodyEn:
      "Customize KPIs and 21 chart types, add calculated metrics, cross-filter across visuals, and undo/redo any layout change.",
    bodyAr:
      "خصّص بطاقات الأداء و21 نوعًا من الرسوم البيانية، وأضف مقاييس محسوبة وتصفية متقاطعة مع تراجع وإعادة كاملة."
  },
  {
    stepId: "step_6_presentation_studio",
    stepNumber: 6,
    targetSelector: "[data-tutorial-target='presentation-studio']",
    titleEn: "6. Presentation Studio & Fullscreen Mode",
    titleAr: "6. استوديو العروض التقديمية ووضع العرض الكامل",
    bodyEn:
      "Edit the 10-slide executive deck, customize layouts and speaker notes, and present fullscreen with keyboard navigation.",
    bodyAr:
      "حرّر العرض التنفيذي المكوّن من 10 شرائح وملاحظات المتحدث، واعرضه بملء الشاشة باستخدام لوحة المفاتيح."
  },
  {
    stepId: "step_7_document_studio",
    stepNumber: 7,
    targetSelector: "[data-tutorial-target='document-studio']",
    titleEn: "7. Document Studio (Paginated Reports)",
    titleAr: "7. استوديو المستندات والتقارير المرقّمة",
    bodyEn:
      "Edit all 13 structured report sections, headers, footers, page breaks, and epistemic findings.",
    bodyAr:
      "حرّر أقسام التقرير الـ 13، والترويسة والتذييل وفواصل الصفحات والنتائج المصنّفة معرفيًا."
  },
  {
    stepId: "step_8_professional_exports",
    stepNumber: 8,
    targetSelector: "[data-tutorial-target='exports']",
    titleEn: "8. Genuine Office, PDF & Power BI Exports",
    titleAr: "8. التصدير الاحترافي إلى أوفيس وPDF وPower BI",
    bodyEn:
      "Export genuine .xlsx, .pptx, .docx, .pdf, .png, .svg, .csv, .safi.html, and Power BI packages with identical verified numbers.",
    bodyAr:
      "صدّر ملفات Excel وPowerPoint وWord وPDF وPNG وSVG وPower BI حقيقية بأرقام موحّدة ومؤكدة."
  }
];

export interface TutorialSessionState {
  readonly isOpen: boolean;
  readonly currentStepIndex: number;
  readonly completed: boolean;
  readonly skipped: boolean;
  readonly replayCount: number;
}

export function createInitialTutorialState(firstRun = true): TutorialSessionState {
  return {
    isOpen: firstRun,
    currentStepIndex: 0,
    completed: false,
    skipped: false,
    replayCount: 0
  };
}

export function advanceTutorialState(
  state: TutorialSessionState,
  action:
    | { readonly type: "NEXT" }
    | { readonly type: "PREV" }
    | { readonly type: "JUMP_TO"; readonly index: number }
    | { readonly type: "SKIP" }
    | { readonly type: "REPLAY" }
    | { readonly type: "RESET" }
): TutorialSessionState {
  const maxIdx = INTERACTIVE_TUTORIAL_STEPS.length - 1;
  switch (action.type) {
    case "NEXT":
      if (state.currentStepIndex >= maxIdx) {
        return { ...state, isOpen: false, completed: true };
      }
      return { ...state, currentStepIndex: state.currentStepIndex + 1 };
    case "PREV":
      return { ...state, currentStepIndex: Math.max(0, state.currentStepIndex - 1) };
    case "JUMP_TO":
      return {
        ...state,
        isOpen: true,
        currentStepIndex: Math.max(0, Math.min(maxIdx, action.index))
      };
    case "SKIP":
      return { ...state, isOpen: false, skipped: true };
    case "REPLAY":
      return {
        isOpen: true,
        currentStepIndex: 0,
        completed: false,
        skipped: false,
        replayCount: state.replayCount + 1
      };
    case "RESET":
      return createInitialTutorialState(true);
  }
}

// ============================================================================
// PHASE 11: PRIVACY, CONSENT GATE & SECURITY ENFORCEMENT
// ============================================================================

export interface ExternalAiConsentState {
  readonly mode: "StrictlyLocal" | "CloudAssistedWithConsent";
  readonly userGrantedConsent: boolean;
  readonly consentTimestampIso?: string | undefined;
  readonly disclosedPayloadScope: readonly ("SchemaSummaryOnly" | "AggregatedStatsOnly")[];
  readonly maskPiiBeforeTransmission: true;
}

export function createDefaultPrivacyConsentState(): ExternalAiConsentState {
  return {
    mode: "StrictlyLocal",
    userGrantedConsent: false,
    disclosedPayloadScope: ["SchemaSummaryOnly"],
    maskPiiBeforeTransmission: true
  };
}

/**
 * Masks sensitive PII values (Email, Phone, National ID, Credit Card, IBAN) deterministically.
 */
export function maskPiiValue(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed) return raw;

  // Email
  if (/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(trimmed)) {
    const [user, domain] = trimmed.split("@");
    const prefix = (user ?? "").slice(0, 1);
    return `${prefix}***@${domain ?? "masked.local"}`;
  }

  // IBAN
  if (/^[A-Z]{2}\d{2}[A-Z0-9]{11,30}$/i.test(trimmed.replace(/\s+/g, ""))) {
    const clean = trimmed.replace(/\s+/g, "");
    return `${clean.slice(0, 4)}********${clean.slice(-4)}`;
  }

  // Phone / National ID / Card (8 to 16 digits)
  const digitsOnly = trimmed.replace(/[\s\-()]/g, "");
  if (/^\+?\d{8,16}$/.test(digitsOnly)) {
    return `${digitsOnly.slice(0, 3)}******${digitsOnly.slice(-2)}`;
  }

  return raw;
}

/**
 * Validates that an external AI request is permitted by the user's explicit consent state
 * and returns a sanitized, PII-free schema payload (never raw cell values!).
 */
export function prepareSanitizedExternalAiPayload(
  consent: ExternalAiConsentState,
  columnTypes: readonly { readonly index: number; readonly role: string; readonly type: string }[]
): {
  readonly allowed: boolean;
  readonly reason: string;
  readonly sanitizedPayload?: string | undefined;
} {
  if (consent.mode === "StrictlyLocal" || !consent.userGrantedConsent) {
    return {
      allowed: false,
      reason:
        "External AI transmission blocked: SafiData is operating in Strictly Local mode without explicit user consent."
    };
  }

  return {
    allowed: true,
    reason: "Explicit user consent verified; transmitting anonymized column role metadata only.",
    sanitizedPayload: JSON.stringify({
      scope: consent.disclosedPayloadScope,
      columns: columnTypes.map((c) => ({
        colIndex: c.index,
        analyticalRole: c.role,
        semanticType: c.type
      }))
    })
  };
}

/**
 * Strips any potential cell values, filenames, or column names from diagnostic log records (`AGENTS.md` Rule 4).
 */
export function buildPrivacySafeLogEntry(params: {
  readonly eventCode: string;
  readonly rowCount: number;
  readonly colCount: number;
  readonly durationMs: number;
}): Readonly<Record<string, string | number>> {
  return {
    eventCode: params.eventCode.replace(/[^A-Za-z0-9_.-]/g, ""),
    rowCount: Math.max(0, Math.floor(params.rowCount)),
    colCount: Math.max(0, Math.floor(params.colCount)),
    durationMs: Math.max(0, Number(params.durationMs.toFixed(2)))
  };
}

// ============================================================================
// PHASE 12: PERFORMANCE, CHUNKED PROCESSING, CANCELLATION & BENCHMARKS
// ============================================================================

export function computeVirtualGridWindow(params: {
  readonly totalRows: number;
  readonly rowHeightPx: number;
  readonly scrollTopPx: number;
  readonly viewportHeightPx: number;
  readonly overscanRows?: number | undefined;
}): {
  readonly startRow: number;
  readonly endRow: number;
  readonly visibleRowCount: number;
  readonly topSpacerPx: number;
  readonly bottomSpacerPx: number;
} {
  const { totalRows, rowHeightPx, scrollTopPx, viewportHeightPx } = params;
  const overscan = params.overscanRows ?? 5;
  if (totalRows <= 0 || rowHeightPx <= 0) {
    return { startRow: 0, endRow: 0, visibleRowCount: 0, topSpacerPx: 0, bottomSpacerPx: 0 };
  }
  const firstVisible = Math.floor(Math.max(0, scrollTopPx) / rowHeightPx);
  const visibleCount = Math.ceil(Math.max(1, viewportHeightPx) / rowHeightPx);
  const startRow = Math.max(0, firstVisible - overscan);
  const endRow = Math.min(totalRows - 1, firstVisible + visibleCount + overscan);
  const renderedCount = Math.max(0, endRow - startRow + 1);
  const topSpacerPx = startRow * rowHeightPx;
  const bottomSpacerPx = Math.max(0, (totalRows - 1 - endRow) * rowHeightPx);

  return {
    startRow,
    endRow,
    visibleRowCount: renderedCount,
    topSpacerPx,
    bottomSpacerPx
  };
}

export function processRowsInChunks<T, R>(params: {
  readonly rows: readonly T[];
  readonly chunkSize: number;
  readonly mapRow: (row: T, index: number) => R;
  readonly signal?: AbortSignal | undefined;
  readonly onProgress?: ((processed: number, total: number, pct: number) => void) | undefined;
}): R[] {
  const { rows, mapRow, signal, onProgress } = params;
  const chunkSize = Math.max(1, Math.floor(params.chunkSize));
  const out: R[] = [];

  for (let i = 0; i < rows.length; i += chunkSize) {
    if (signal?.aborted) {
      throw new Error("SF-IMP-002: ChunkedProcessingCancelled");
    }
    const end = Math.min(rows.length, i + chunkSize);
    for (let j = i; j < end; j += 1) {
      out.push(mapRow(rows[j]!, j));
    }
    const pct = rows.length > 0 ? Math.round((end / rows.length) * 100) : 100;
    onProgress?.(end, rows.length, pct);
  }
  return out;
}

export function runIngestionAndProfilingBenchmark(rowCount = 1000): {
  readonly rowCount: number;
  readonly elapsedMs: number;
  readonly rowsPerSecond: number;
  readonly tablesExtracted: number;
  readonly kpisGenerated: number;
} {
  const start = performance.now();
  const rows: string[][] = [["Transaction_ID", "Branch", "Date", "Revenue_EGP", "Cost_EGP"]];
  const branches = ["Cairo", "Alexandria", "Giza", "Mansoura"];
  for (let i = 1; i <= rowCount; i += 1) {
    rows.push([
      `TX-${String(i).padStart(5, "0")}`,
      branches[i % branches.length]!,
      `2026-01-${String((i % 28) + 1).padStart(2, "0")}`,
      String(1000 + (i % 50) * 25),
      String(600 + (i % 30) * 15)
    ]);
  }

  const ingestion = ingestWorkbookStructure([{ name: "Benchmark_Sheet", grid: rows }]);
  const model = buildSemanticWorkbookModel(ingestion);
  const elapsedMs = Math.max(0.1, Number((performance.now() - start).toFixed(2)));
  const rowsPerSecond = Math.round((rowCount / elapsedMs) * 1000);

  return {
    rowCount,
    elapsedMs,
    rowsPerSecond,
    tablesExtracted: model.tables.length,
    kpisGenerated: model.canonicalKpis.length
  };
}
