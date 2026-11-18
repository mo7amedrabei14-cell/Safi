import React, { useEffect, useState } from "react";
import { Download, HelpCircle, Play, RotateCcw, SkipForward, X } from "lucide-react";
import type { ExportOptions } from "../../utils/export";

export interface UnifiedTooltipBadgeProps {
  readonly tooltipId: string;
  readonly text: string;
  readonly label?: string;
}

/**
 * Unified accessible tooltip badge usable via mouse hover OR keyboard click/focus (`data-tooltip-id`).
 */
export function UnifiedTooltipBadge({
  tooltipId,
  text,
  label = "?"
}: UnifiedTooltipBadgeProps): React.ReactElement {
  const [open, setOpen] = useState(false);
  return (
    <span className="studio-tooltip-anchor" data-tooltip-id={tooltipId}>
      <button
        type="button"
        className="studio-tooltip-trigger"
        aria-label={text}
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
        onBlur={() => setOpen(false)}
      >
        {label}
      </button>
      {open ? (
        <span className="studio-tooltip-popover" role="tooltip">
          {text}
        </span>
      ) : null}
    </span>
  );
}

export interface TutorialStepDef {
  readonly id: string;
  readonly targetSelector: string;
  readonly titleEn: string;
  readonly titleAr: string;
  readonly bodyEn: string;
  readonly bodyAr: string;
}

export const DESKTOP_TUTORIAL_STEPS: readonly TutorialStepDef[] = [
  {
    id: "step_import",
    targetSelector: '[data-tutorial-target="open-workbook"]',
    titleEn: "1. Open Any Spreadsheet or Workbook",
    titleAr: "١. افتح أي ملف بيانات",
    bodyEn:
      "Import .xlsx, .xlsm, .xls, .ods, .csv, or .tsv locally — SafiData never uploads your files.",
    bodyAr: "استورد ملفات Excel أو ODS أو CSV محليًا — بياناتك لا تغادر جهازك أبدًا."
  },
  {
    id: "step_workbook_explorer",
    targetSelector: '[data-tutorial-target="workbook-explorer"]',
    titleEn: "2. Inspect Multiple Sheets & Tables",
    titleAr: "٢. استكشف الأوراق والجداول",
    bodyEn:
      "Switch between visible or hidden sheets, review separated totals/banners, and audit formulas.",
    bodyAr: "تنقل بين الأوراق الظاهرة والمخفية وراجع الصفوف المفصولة وتدقيق المعادلات."
  },
  {
    id: "step_questions",
    targetSelector: '[data-tutorial-target="contextual-questions"]',
    titleEn: "3. Clarify Ambiguities",
    titleAr: "٣. أجب على الأسئلة التوضيحية",
    bodyEn:
      "Answer or skip column-role questions so KPIs and aggregations stay mathematically sound.",
    bodyAr: "أجب على الأسئلة المخصصة لتضمن دقة المؤشرات دون حساب خاطئ."
  },
  {
    id: "step_assistant",
    targetSelector: '[data-tutorial-target="grounded-assistant"]',
    titleEn: "4. Ask the Grounded Data Assistant",
    titleAr: "٤. اسأل المساعد الموثق",
    bodyEn:
      "Every answer cites real sheet ranges and verified aggregations computed deterministically offline.",
    bodyAr: "كل إجابة تستشهد بالنطاقات والأرقام المحسوبة فعليًا محليًا بدون تخمين."
  },
  {
    id: "step_dashboard",
    targetSelector: '[data-tutorial-target="dashboard-tab"]',
    titleEn: "5. Build Dashboards with 21 Chart Types",
    titleAr: "٥. صمّم لوحات بمخططات متنوعة (٢١ نوعًا)",
    bodyEn:
      "Create Bar, Column, Waterfall, Treemap, Heatmap, Gauge, Geographic, Funnel & more calculated in Rust.",
    bodyAr: "أنشئ جميع المخططات الـ ٢١ مع تصفية تفاعلية مترابطة."
  },
  {
    id: "step_slides_doc",
    targetSelector: '[data-tutorial-target="slides-doc-tabs"]',
    titleEn: "6. Edit Keynote Slides & Executive Report",
    titleAr: "٦. حرّر العرض التقديمي والتقرير التنفيذي",
    bodyEn:
      "Customize speaker notes, narrative sections, headers, and page breaks backed by verified tokens.",
    bodyAr: "عدّل ملاحظات المحاضر وفقرات التقرير مع تحديث حي من الأرقام الموثقة."
  },
  {
    id: "step_export",
    targetSelector: '[data-tutorial-target="export-deliverable-btn"]',
    titleEn: "7. Export Production Deliverables",
    titleAr: "٧. صدّر مخرجات احترافية حقيقية",
    bodyEn:
      "Export multi-sheet .xlsx, .pptx, .docx, multi-page .pdf, .png, .svg, .safi.html, and Power BI bundles.",
    bodyAr: "صدّر ملفات Excel وPowerPoint وWord وPDF متعددة الصفحات وصور SVG/PNG وPower BI."
  }
];

const TUTORIAL_STORAGE_KEY = "safidata.desktop.tutorial.completed.v2";

export interface InteractiveTutorialOverlayProps {
  readonly language: "en" | "ar";
  readonly open: boolean;
  readonly onClose: () => void;
}

export function InteractiveTutorialOverlay({
  language,
  open,
  onClose
}: InteractiveTutorialOverlayProps): React.ReactElement | null {
  const isAr = language === "ar";
  const [stepIndex, setStepIndex] = useState(0);
  const [availableSteps, setAvailableSteps] =
    useState<readonly TutorialStepDef[]>(DESKTOP_TUTORIAL_STEPS);

  useEffect(() => {
    if (!open || typeof document === "undefined") {
      return;
    }
    const visibleSteps = DESKTOP_TUTORIAL_STEPS.filter(
      (s) => document.querySelector(s.targetSelector) !== null
    );
    setAvailableSteps(visibleSteps.length > 0 ? visibleSteps : DESKTOP_TUTORIAL_STEPS);
    setStepIndex(0);
  }, [open]);

  if (!open || availableSteps.length === 0) {
    return null;
  }

  const clampedIdx = Math.min(stepIndex, availableSteps.length - 1);
  const activeStep = availableSteps[clampedIdx] ?? availableSteps[0];
  if (!activeStep) {
    return null;
  }

  function finishTutorial(): void {
    try {
      window.localStorage.setItem(TUTORIAL_STORAGE_KEY, "true");
    } catch {
      // Storage may be restricted in test harness; safe to ignore
    }
    onClose();
  }

  return (
    <div
      className="studio-modal-backdrop"
      role="dialog"
      aria-modal="true"
      aria-label={isAr ? "جولة تعريفية تفاعلية" : "Interactive Guided Tutorial"}
      data-testid="interactive-tutorial-dialog"
    >
      <div className="studio-modal-card">
        <div className="studio-panel-header">
          <h4>
            <HelpCircle size={16} /> {isAr ? activeStep.titleAr : activeStep.titleEn}
          </h4>
          <button
            type="button"
            className="studio-chip-btn"
            onClick={finishTutorial}
            aria-label={isAr ? "إغلاق" : "Close"}
          >
            <X size={14} />
          </button>
        </div>
        <p>{isAr ? activeStep.bodyAr : activeStep.bodyEn}</p>
        <div className="studio-muted-note">
          {isAr
            ? `خطوة ${clampedIdx + 1} من ${availableSteps.length}`
            : `Step ${clampedIdx + 1} of ${availableSteps.length}`}
        </div>
        <div className="studio-quick-templates">
          <button
            type="button"
            className="studio-chip-btn"
            onClick={finishTutorial}
            data-testid="tutorial-skip-btn"
          >
            <SkipForward size={14} /> {isAr ? "تخطي" : "Skip"}
          </button>
          <button
            type="button"
            className="studio-chip-btn"
            disabled={clampedIdx === 0}
            onClick={() => setStepIndex((i) => Math.max(0, i - 1))}
          >
            {isAr ? "السابق" : "Previous"}
          </button>
          {clampedIdx + 1 < availableSteps.length ? (
            <button
              type="button"
              className="studio-chip-btn is-active"
              data-testid="tutorial-next-btn"
              onClick={() => setStepIndex((i) => Math.min(availableSteps.length - 1, i + 1))}
            >
              <Play size={14} /> {isAr ? "التالي" : "Next"}
            </button>
          ) : (
            <button
              type="button"
              className="studio-chip-btn is-active"
              data-testid="tutorial-finish-btn"
              onClick={finishTutorial}
            >
              {isAr ? "إنهاء الجولة" : "Finish Tour"}
            </button>
          )}
          <button type="button" className="studio-chip-btn" onClick={() => setStepIndex(0)}>
            <RotateCcw size={13} /> {isAr ? "إعادة" : "Restart"}
          </button>
        </div>
      </div>
    </div>
  );
}

export interface DeliverableExportModalProps {
  readonly language: "en" | "ar";
  readonly open: boolean;
  readonly busy: boolean;
  readonly hiddenSheetCount: number;
  readonly lastStatus: string | null;
  readonly onClose: () => void;
  readonly onExportFormat: (
    format: ExportOptions["format"],
    includeHidden: boolean
  ) => Promise<void>;
}

const EXPORT_FORMATS: ReadonlyArray<{
  readonly format: ExportOptions["format"];
  readonly labelEn: string;
  readonly labelAr: string;
  readonly ext: string;
}> = [
  {
    format: "xlsx",
    labelEn: "Multi-Sheet Excel Workbook (.xlsx)",
    labelAr: "مصنف إكسل متعدد الأوراق (.xlsx)",
    ext: "xlsx"
  },
  {
    format: "pdf",
    labelEn: "Multi-Page Vector Report (.pdf)",
    labelAr: "تقرير PDF متعدد الصفحات (.pdf)",
    ext: "pdf"
  },
  {
    format: "docx",
    labelEn: "Word Executive Document (.docx)",
    labelAr: "مستند وورد تنفيذي (.docx)",
    ext: "docx"
  },
  {
    format: "pptx",
    labelEn: "PowerPoint Presentation (.pptx)",
    labelAr: "عرض بوربوينت (.pptx)",
    ext: "pptx"
  },
  {
    format: "safi.html",
    labelEn: "Offline Verified Capsule (.safi.html)",
    labelAr: "كبسولة تفاعلية موثقة (.safi.html)",
    ext: "safi.html"
  },
  {
    format: "png",
    labelEn: "High-Res Chart Snapshot (.png)",
    labelAr: "صورة المخطط (.png)",
    ext: "png"
  },
  { format: "svg", labelEn: "Vector Chart (.svg)", labelAr: "رسم متجهي (.svg)", ext: "svg" },
  {
    format: "csv",
    labelEn: "Formula-Safe UTF-8 CSV (.csv)",
    labelAr: "ملف CSV محمي (.csv)",
    ext: "csv"
  },
  {
    format: "powerbi",
    labelEn: "Power BI Connection & BIM Package (.zip)",
    labelAr: "حزمة Power BI والبيانات (.zip)",
    ext: "zip"
  }
];

export function DeliverableExportModal({
  language,
  open,
  busy,
  hiddenSheetCount,
  lastStatus,
  onClose,
  onExportFormat
}: DeliverableExportModalProps): React.ReactElement | null {
  const isAr = language === "ar";
  const [selectedFormat, setSelectedFormat] = useState<ExportOptions["format"]>("xlsx");
  const [includeHidden, setIncludeHidden] = useState(false);

  if (!open) return null;

  return (
    <div
      className="studio-modal-backdrop"
      role="dialog"
      aria-modal="true"
      aria-label={isAr ? "تصدير المخرجات الاحترافية" : "Export Production Deliverable"}
      data-testid="deliverable-export-modal"
    >
      <div className="studio-modal-card">
        <div className="studio-panel-header">
          <h4>
            <Download size={16} />{" "}
            {isAr ? "تصدير المخرجات الاحترافية" : "Export Multi-Format Deliverable"}
          </h4>
          <button
            type="button"
            className="studio-chip-btn"
            onClick={onClose}
            aria-label={isAr ? "إغلاق" : "Close"}
          >
            <X size={14} />
          </button>
        </div>

        <label className="studio-stacked-field">
          <span>{isAr ? "صيغة الملف:" : "Deliverable Format:"}</span>
          <select
            data-testid="export-format-select"
            value={selectedFormat}
            onChange={(ev) => setSelectedFormat(ev.target.value as ExportOptions["format"])}
          >
            {EXPORT_FORMATS.map((f) => (
              <option key={f.format} value={f.format}>
                {isAr ? f.labelAr : f.labelEn}
              </option>
            ))}
          </select>
        </label>

        {hiddenSheetCount > 0 ? (
          <label className="studio-checkbox-label">
            <input
              type="checkbox"
              data-testid="export-include-hidden-checkbox"
              checked={includeHidden}
              onChange={(ev) => setIncludeHidden(ev.target.checked)}
            />
            <span>
              {isAr
                ? `تضمين الأوراق المخفية (${hiddenSheetCount}) — وإلا يتم استبعادها والإفصاح عنها في التقرير`
                : `Include ${hiddenSheetCount} hidden worksheet(s) — otherwise omitted & explicitly disclosed`}
            </span>
          </label>
        ) : null}

        {lastStatus ? (
          <div
            className="studio-insight-warning"
            role="status"
            data-testid="export-deliverable-status"
          >
            {lastStatus}
          </div>
        ) : null}

        <div className="studio-quick-templates">
          <button
            type="button"
            className="studio-chip-btn is-active"
            disabled={busy}
            data-testid="confirm-export-deliverable-btn"
            onClick={() => void onExportFormat(selectedFormat, includeHidden)}
          >
            <Download size={14} /> {isAr ? "حفظ المخرج الذري الآن" : "Write Atomic Deliverable"}
          </button>
          <button type="button" className="studio-chip-btn" onClick={onClose}>
            {isAr ? "إغلاق" : "Close"}
          </button>
        </div>
      </div>
    </div>
  );
}
