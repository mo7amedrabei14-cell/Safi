/**
 * `apps/web/src/desktop/DesktopStudio.tsx`
 *
 * Quad-Studio workspace for the packaged desktop app. Execution runs exclusively inside
 * `crates/sf-desktop/src/studio.rs` behind the IPC bridge; this component never parses files,
 * aggregates numbers, or recomputes metrics in TypeScript.
 *
 * In a browser the runtime guard renders a notice instead of the workspace, which keeps the
 * public site a showcase.
 */
import type { KeyboardEvent, ReactNode } from "react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  ArrowDownUp,
  Ban,
  BarChart3,
  BrainCircuit,
  Download,
  FileSpreadsheet,
  FileText,
  FolderOpen,
  HelpCircle,
  Layers,
  LayoutTemplate,
  Plus,
  Presentation,
  Redo2,
  RefreshCw,
  Save,
  ShieldCheck,
  Trash2,
  Undo2,
  XCircle
} from "lucide-react";
import "./desktop-studio.css";
import type { SafiLang } from "../components/layout/FloatingGlassDock";
import type { ExportOptions } from "../utils/export";
import { exportStudioDeliverable } from "../utils/export";
import type { DesktopIntelligenceSnapshot } from "./desktop-intelligence-bridge";
import {
  buildDesktopIntelligenceSnapshotAsync,
  buildExportDashboardState,
  buildExportDocumentReport,
  buildExportPresentationDeck
} from "./desktop-intelligence-bridge";
import type {
  DesktopGroup,
  DesktopRowPage,
  DesktopStudioState,
  DesktopWorkbookDetails,
  StudioAssetArtifact,
  StudioChartArtifact,
  StudioDocBlockArtifact,
  StudioFreeFormBlockArtifact,
  StudioPrivacySettingsArtifact,
  StudioQuestionResolutionArtifact,
  StudioSlideArtifact,
  StudioTemplateConfigArtifact,
  StudioWorkspaceArtifacts
} from "./studio-bridge";
import {
  activateScenario,
  addCalculatedColumn,
  addScenario,
  addSourceColumn,
  applyAnomalyFix,
  applyDslCommand,
  compareScenarios,
  createRequestTracker,
  defineMetric,
  defineParameter,
  deleteColumn,
  deleteRow,
  exportCapsule,
  exportDeliverable,
  fetchRowsPage,
  grouped,
  insertRow,
  isDesktopRuntime,
  isNewerOrEqualRevision,
  loadProjectBundle,
  openProject,
  pickCapsuleOutputFile,
  pickDeliverableFile,
  pickInputFile,
  projectState,
  redo,
  renameColumn,
  renameScenario,
  resetScenario,
  restoreProject,
  saveProjectBundle,
  selectRegion,
  setArtifacts,
  setCellById,
  setScenarioParameter,
  snapshot,
  sortRows,
  trimColumn,
  undo,
  updateParameter,
  workbookDetails
} from "./studio-bridge";
import { AnomalyPanel } from "./components/AnomalyPanel";
import { CommandBarPanel } from "./components/CommandBarPanel";
import { DashboardChartsView } from "./components/DashboardChartsView";
import {
  DeliverableExportModal,
  InteractiveTutorialOverlay,
  UnifiedTooltipBadge
} from "./components/DesktopTutorialAndExport";
import { FreeFormBuilderView } from "./components/FreeFormBuilderView";
import { IntelligenceAndAssistantPanel } from "./components/IntelligenceAndAssistantPanel";
import { KeynoteSlidesView } from "./components/KeynoteSlidesView";
import { listMissingMetricRefs } from "./components/NarrativeTextRenderer";
import { TemplatesAndProjectsPanel } from "./components/TemplatesAndProjectsPanel";
import { VerifiedDocView } from "./components/VerifiedDocView";
import { VerifiedNumberChip } from "./components/VerifiedNumberChip";
import { WhatIfPanel } from "./components/WhatIfPanel";
import { WorkbookIntelligenceDrawer } from "./components/WorkbookIntelligenceDrawer";

type StudioMode =
  | "sheet"
  | "intelligence"
  | "dashboard"
  | "slides"
  | "doc"
  | "freeform"
  | "templates";
const STUDIO_MODES: readonly StudioMode[] = [
  "sheet",
  "intelligence",
  "dashboard",
  "slides",
  "doc",
  "freeform",
  "templates"
];
const DEFAULT_PRIVACY: StudioPrivacySettingsArtifact = {
  externalAiConsentGranted: false,
  maskPiiOnCloud: true,
  consentTimestampMs: null
};
const EMPTY_ARTIFACTS: StudioWorkspaceArtifacts = {
  charts: [],
  slides: [],
  docBlocks: [],
  freeformBlocks: [],
  assets: [],
  templateConfig: null,
  questionResolutions: [],
  privacySettings: DEFAULT_PRIVACY
};

function uint8ToBase64(bytes: Uint8Array): string {
  let binary = "";
  const chunk = 8192;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

const PROJECT = "main";
const SAVED_KEY = "safi.desktop.studio.snapshot.v1";
const AGGREGATES = ["SUM", "AVG", "COUNT", "MIN", "MAX"] as const;
const PAGE_SIZES = [25, 50, 100, 200] as const;
type Aggregate = (typeof AGGREGATES)[number];

export interface DesktopStudioProps {
  readonly lang?: SafiLang;
}

export function DesktopStudio({ lang = "en" }: DesktopStudioProps): ReactNode {
  const L = useCallback((ar: string, en: string): string => (lang === "ar" ? ar : en), [lang]);
  const desktop = isDesktopRuntime();
  const [state, setState] = useState<DesktopStudioState | null>(null);
  const [rowPage, setRowPage] = useState<DesktopRowPage | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [recalculating, setRecalculating] = useState(false);

  const [metricId, setMetricId] = useState("total_amount");
  const [metricName, setMetricName] = useState("Total amount");
  const [metricUnit, setMetricUnit] = useState("EGP");
  const [metricFormula, setMetricFormula] = useState("SUM(amount)");
  const [columnName, setColumnName] = useState("line_total");
  const [columnFormula, setColumnFormula] = useState("amount * quantity");

  const [aggregate, setAggregate] = useState<Aggregate>("SUM");
  const [valueColumn, setValueColumn] = useState("");
  const [groupBy, setGroupBy] = useState("");
  const [groups, setGroups] = useState<readonly DesktopGroup[]>([]);

  // Paginated viewport query state (UI only holds one bounded page <= 200 rows)
  const [pageOffset, setPageOffset] = useState(0);
  const [pageLimit, setPageLimit] = useState<number>(50);
  const [searchQuery, setSearchQuery] = useState("");
  const [filterColumn, setFilterColumn] = useState("");
  const [filterValue, setFilterValue] = useState("");
  const [sortColumn, setSortColumn] = useState<string | null>(null);
  const [sortAscending, setSortAscending] = useState(true);
  const [mode, setMode] = useState<StudioMode>("sheet");
  const [newColumnName, setNewColumnName] = useState("");
  const [renameSource, setRenameSource] = useState("");
  const [renameTarget, setRenameTarget] = useState("");
  const [trimTarget, setTrimTarget] = useState("");
  const [wbDetails, setWbDetails] = useState<DesktopWorkbookDetails | null>(null);
  const [intelSnapshot, setIntelSnapshot] = useState<DesktopIntelligenceSnapshot | null>(null);
  const [analysisProgress, setAnalysisProgress] = useState<{ pct: number; phase: string } | null>(
    null
  );
  const [tutorialOpen, setTutorialOpen] = useState(false);
  const [exportModalOpen, setExportModalOpen] = useState(false);
  const [exportStatus, setExportStatus] = useState<string | null>(null);
  const analysisAbortRef = useRef<AbortController | null>(null);

  const stateRef = useRef<DesktopStudioState | null>(null);
  stateRef.current = state;

  const stateTracker = useRef(createRequestTracker());
  const rowsTracker = useRef(createRequestTracker());
  const chartTracker = useRef(createRequestTracker());

  const run = useCallback(async (action: () => Promise<DesktopStudioState>): Promise<void> => {
    const seq = stateTracker.current.next();
    setBusy(true);
    setRecalculating(true);
    try {
      const next = await action();
      if (!stateTracker.current.isCurrent(seq)) {
        return;
      }
      if (isNewerOrEqualRevision(next, stateRef.current)) {
        setState(next);
        setError(null);
      }
    } catch (cause) {
      if (stateTracker.current.isCurrent(seq)) {
        setError(cause instanceof Error ? cause.message : String(cause));
      }
    } finally {
      if (stateTracker.current.isCurrent(seq)) {
        setBusy(false);
        setRecalculating(false);
      }
    }
  }, []);

  // Automatically load paginated rows whenever revision or query parameters change.
  const loadRows = useCallback(async (): Promise<void> => {
    if (!desktop || stateRef.current === null) {
      return;
    }
    const seq = rowsTracker.current.next();
    const expectedRevision = stateRef.current.revision;
    try {
      const page = await fetchRowsPage(PROJECT, {
        offset: pageOffset,
        limit: pageLimit,
        search: searchQuery.trim().length > 0 ? searchQuery : null,
        sort_column: sortColumn,
        ascending: sortAscending,
        filter_column: filterColumn.length > 0 ? filterColumn : null,
        filter_value: filterValue.trim().length > 0 ? filterValue : null
      });
      if (!rowsTracker.current.isCurrent(seq)) {
        return;
      }
      if (stateRef.current !== null && page.revision < stateRef.current.revision) {
        return;
      }
      if (page.revision >= expectedRevision) {
        setRowPage(page);
      }
    } catch (cause) {
      if (rowsTracker.current.isCurrent(seq)) {
        setError(cause instanceof Error ? cause.message : String(cause));
      }
    }
  }, [
    desktop,
    filterColumn,
    filterValue,
    pageLimit,
    pageOffset,
    searchQuery,
    sortAscending,
    sortColumn
  ]);

  // Automatically recompute chart grouping in Rust whenever data or chart spec changes.
  const loadChart = useCallback(async (): Promise<void> => {
    if (!desktop || stateRef.current === null || groupBy.length === 0 || valueColumn.length === 0) {
      return;
    }
    const seq = chartTracker.current.next();
    try {
      const nextGroups = await grouped(PROJECT, aggregate, valueColumn, groupBy);
      if (chartTracker.current.isCurrent(seq)) {
        setGroups(nextGroups);
      }
    } catch (cause) {
      if (chartTracker.current.isCurrent(seq)) {
        setError(cause instanceof Error ? cause.message : String(cause));
      }
    }
  }, [aggregate, desktop, groupBy, valueColumn]);

  useEffect(() => {
    if (state !== null) {
      void loadRows();
    }
  }, [state, state?.revision, loadRows]);

  useEffect(() => {
    if (state !== null) {
      void loadChart();
    }
  }, [state, state?.revision, loadChart]);

  // Fetch workbook intelligence metadata & run cooperative cancellable semantic/verified pipeline.
  useEffect(() => {
    if (!desktop || state === null) {
      return;
    }
    const controller = new AbortController();
    analysisAbortRef.current?.abort();
    analysisAbortRef.current = controller;

    void (async () => {
      try {
        setAnalysisProgress({ pct: 10, phase: "Reading workbook intelligence" });
        const det = await workbookDetails(PROJECT).catch(() => null);
        if (controller.signal.aborted) return;
        setWbDetails(det);
        const snap = await buildDesktopIntelligenceSnapshotAsync({
          state,
          details: det,
          page: rowPage,
          savedResolutions: state.artifacts?.questionResolutions ?? [],
          signal: controller.signal,
          onProgress: (pct, phase) => {
            if (!controller.signal.aborted) {
              setAnalysisProgress({ pct, phase });
            }
          }
        });
        if (!controller.signal.aborted) {
          setIntelSnapshot(snap);
          setAnalysisProgress(null);
        }
      } catch (cause) {
        if (cause instanceof DOMException && cause.name === "AbortError") {
          setAnalysisProgress(null);
          return;
        }
        setAnalysisProgress(null);
      }
    })();

    return () => {
      controller.abort();
    };
  }, [desktop, state, state?.revision, rowPage]);

  const handleSelectRegion = useCallback(
    async (sheetName: string, tableId: string): Promise<void> => {
      const seq = stateTracker.current.next();
      setBusy(true);
      setRecalculating(true);
      try {
        const next = await selectRegion(PROJECT, sheetName, tableId);
        if (!stateTracker.current.isCurrent(seq)) {
          return;
        }
        // Switching regions starts a fresh DataGraph at revision 1, so bypass
        // isNewerOrEqualRevision (which would otherwise drop lower revisions).
        setState(next);
        setRowPage(null);
        setPageOffset(0);
        setSortColumn(null);
        setSearchQuery("");
        setFilterValue("");
        setGroups([]);
        const first = next.columns[0]?.name ?? "";
        setGroupBy(first);
        setValueColumn(next.columns[1]?.name ?? first);
        setError(null);
      } catch (cause) {
        if (stateTracker.current.isCurrent(seq)) {
          setError(cause instanceof Error ? cause.message : String(cause));
        }
      } finally {
        if (stateTracker.current.isCurrent(seq)) {
          setBusy(false);
          setRecalculating(false);
        }
      }
    },
    []
  );

  const openFile = useCallback(async (): Promise<void> => {
    const seq = stateTracker.current.next();
    setBusy(true);
    setRecalculating(true);
    try {
      const path = await pickInputFile();
      if (path === null || !stateTracker.current.isCurrent(seq)) {
        return;
      }
      const next = await openProject(PROJECT, path);
      if (!stateTracker.current.isCurrent(seq)) {
        return;
      }
      setState(next);
      setRowPage(null);
      setPageOffset(0);
      setSortColumn(null);
      setSearchQuery("");
      setFilterValue("");
      setGroups([]);
      const first = next.columns[0]?.name ?? "";
      setGroupBy(first);
      setValueColumn(next.columns[1]?.name ?? first);
      setError(null);
    } catch (cause) {
      if (stateTracker.current.isCurrent(seq)) {
        setError(cause instanceof Error ? cause.message : String(cause));
      }
    } finally {
      if (stateTracker.current.isCurrent(seq)) {
        setBusy(false);
        setRecalculating(false);
      }
    }
  }, []);

  const save = useCallback(async (): Promise<void> => {
    try {
      window.localStorage.setItem(SAVED_KEY, await snapshot(PROJECT));
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }, []);

  const restore = useCallback(async (): Promise<void> => {
    const saved = window.localStorage.getItem(SAVED_KEY);
    if (saved === null) {
      setError(L("لا يوجد مشروع محفوظ على هذا الجهاز.", "No saved project on this device."));
      return;
    }
    await run(async () => {
      const next = await restoreProject(PROJECT, saved);
      const first = next.columns[0]?.name ?? "";
      if (first.length > 0 && groupBy.length === 0) {
        setGroupBy(first);
        setValueColumn(next.columns[1]?.name ?? first);
      }
      return next;
    });
  }, [L, groupBy.length, run]);

  const artifacts = state?.artifacts ?? EMPTY_ARTIFACTS;
  const metricsById = useMemo(
    () => new Map((state?.metrics ?? []).map((metric) => [metric.id, metric])),
    [state?.metrics]
  );

  const persistArtifacts = useCallback(
    async (nextArtifacts: StudioWorkspaceArtifacts): Promise<void> => {
      await run(() => setArtifacts(PROJECT, nextArtifacts));
    },
    [run]
  );

  const handleTabKeyDown = useCallback(
    (event: KeyboardEvent<HTMLDivElement>): void => {
      const idx = STUDIO_MODES.indexOf(mode);
      if (idx < 0) {
        return;
      }
      if (event.key === "ArrowRight") {
        event.preventDefault();
        setMode(STUDIO_MODES[(idx + 1) % STUDIO_MODES.length] ?? "sheet");
      } else if (event.key === "ArrowLeft") {
        event.preventDefault();
        setMode(STUDIO_MODES[(idx - 1 + STUDIO_MODES.length) % STUDIO_MODES.length] ?? "sheet");
      }
    },
    [mode]
  );

  const writeCapsule = useCallback(async (): Promise<void> => {
    const unresolved = [
      ...artifacts.slides.flatMap((s) => listMissingMetricRefs(s.body, metricsById)),
      ...artifacts.docBlocks.flatMap((b) => listMissingMetricRefs(b.content, metricsById)),
      ...artifacts.freeformBlocks.flatMap((f) => listMissingMetricRefs(f.body, metricsById))
    ];
    if (unresolved.length > 0) {
      setError(
        L(
          `[SF-CAL-006] تعذر التصدير لوجود مراجع مؤشرات مفقودة: ${Array.from(new Set(unresolved)).join(", ")}`,
          `[SF-CAL-006] Export blocked by unresolved metric references: ${Array.from(new Set(unresolved)).join(", ")}`
        )
      );
      return;
    }

    setBusy(true);
    try {
      const target = await pickCapsuleOutputFile();
      if (target === null) {
        return;
      }
      const savedCharts = artifacts.charts.map((c) => ({
        title: c.title,
        chart_type: c.chartType ?? "bar",
        function: c.function,
        column: c.column,
        group_by: c.groupBy
      }));
      const fallbackCharts =
        groupBy.length > 0 && valueColumn.length > 0
          ? [
              {
                title: `${aggregate} ${valueColumn} / ${groupBy}`,
                function: aggregate,
                column: valueColumn,
                group_by: groupBy
              }
            ]
          : [];
      const charts = savedCharts.length > 0 ? savedCharts : fallbackCharts;
      await exportCapsule(
        PROJECT,
        target,
        JSON.stringify({
          title: metricName.length > 0 ? metricName : "Safi capsule",
          created_at: new Date().toISOString(),
          charts,
          slides: artifacts.slides,
          doc_blocks: artifacts.docBlocks,
          freeform_blocks: artifacts.freeformBlocks,
          assets: artifacts.assets,
          template_name: artifacts.templateConfig?.templateId ?? null
        })
      );
      setError(null);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  }, [L, aggregate, artifacts, groupBy, metricName, metricsById, valueColumn]);

  const handleExportDeliverable = useCallback(
    async (format: ExportOptions["format"], includeHidden: boolean): Promise<void> => {
      if (!state) return;
      if (format === "safi.html" && (!artifacts.slides || artifacts.slides.length === 0)) {
        await writeCapsule();
        setExportStatus(
          L(
            "تم تصدير الكبسولة التفاعلية الموثقة عبر محرك Rust.",
            "Verified .safi.html capsule written atomically via Rust."
          )
        );
        return;
      }

      if (!intelSnapshot) {
        setError(
          L(
            "الرجاء الانتظار لحين اكتمال بناء النموذج الدلالي.",
            "Please wait for semantic workbook analysis to complete."
          )
        );
        return;
      }

      setBusy(true);
      try {
        const dashState = buildExportDashboardState(
          intelSnapshot.semanticModel,
          intelSnapshot.verifiedReport,
          state
        );
        const presState = buildExportPresentationDeck(
          intelSnapshot.semanticModel,
          intelSnapshot.verifiedReport,
          artifacts.slides
        );
        const docState = buildExportDocumentReport(
          intelSnapshot.semanticModel,
          intelSnapshot.verifiedReport,
          artifacts.docBlocks
        );
        const result = exportStudioDeliverable({
          format,
          model: intelSnapshot.semanticModel,
          dashboard: dashState,
          presentation: presState,
          document: docState,
          rtl: lang === "ar",
          includeHiddenSheets: includeHidden
        });
        const ext = format === "powerbi" ? "zip" : format;
        const target = await pickDeliverableFile(result.filename, ext);
        if (!target) {
          return;
        }
        const isBinary = format !== "csv" && format !== "svg" && format !== "safi.html";
        const payload = isBinary ? uint8ToBase64(result.binaryBytes) : result.content;
        const writtenPath = await exportDeliverable(PROJECT, target, isBinary, payload);
        setExportStatus(
          L(
            `تم الحفظ الذري في: ${writtenPath} (${result.sizeBytes.toLocaleString("en-US")} بايت)`,
            `Written atomically to: ${writtenPath} (${result.sizeBytes.toLocaleString("en-US")} bytes)`
          )
        );
        setError(null);
      } catch (cause) {
        setError(cause instanceof Error ? cause.message : String(cause));
      } finally {
        setBusy(false);
      }
    },
    [L, artifacts.docBlocks, artifacts.slides, intelSnapshot, lang, state, writeCapsule]
  );

  if (!desktop) {
    return (
      <section className="safi-desktop-studio safi-desktop-studio--locked" role="note">
        <FileSpreadsheet size={28} aria-hidden="true" />
        <h1>{L("الاستوديو يعمل داخل التطبيق", "The Studio runs inside the app")}</h1>
        <p>
          {L(
            "هذه الشاشة تنفّذ المحرك محلياً داخل تطبيق سطح المكتب فقط. حمّل التطبيق لتفتح الملفات وتحسب المؤشرات.",
            "This screen executes the engine locally inside the desktop app only. Download the app to open files and compute metrics."
          )}
        </p>
      </section>
    );
  }

  return (
    <section className="safi-desktop-studio" aria-busy={busy}>
      <header className="safi-desktop-studio__head">
        <h1>{L("الاستوديو الرباعي", "Quad-Studio")}</h1>
        <p>
          {L(
            "كل رقم هنا محسوب في محرك Rust على جهازك ومربوط ببصمة تحقق.",
            "Every number here is computed by the Rust engine on your machine and bound to a verifiable fingerprint."
          )}
        </p>
      </header>

      <div className="safi-desktop-studio__toolbar">
        <button
          type="button"
          onClick={() => void openFile()}
          disabled={busy}
          data-tutorial-target="open-workbook"
        >
          <FileSpreadsheet size={16} aria-hidden="true" />
          {L("افتح ملفاً", "Open a file")}
        </button>
        <UnifiedTooltipBadge
          tooltipId="tip_open_workbook"
          text={L(
            "يستورد ملفات .xlsx/.xlsm/.xls/.ods/.csv/.tsv محليًا ويحلل الأوراق الظاهرة والمخفية.",
            "Imports .xlsx/.xlsm/.xls/.ods/.csv/.tsv locally and audits visible & hidden sheets."
          )}
        />
        <button type="button" onClick={() => void save()} disabled={state === null}>
          <Save size={16} aria-hidden="true" /> {L("حفظ", "Save")}
        </button>
        <button type="button" onClick={() => void restore()} disabled={busy}>
          {L("استعادة", "Restore")}
        </button>
        <button
          type="button"
          onClick={() => void run(() => undo(PROJECT))}
          disabled={state === null || busy}
        >
          <Undo2 size={16} aria-hidden="true" /> {L("تراجع", "Undo")}
        </button>
        <button
          type="button"
          onClick={() => void run(() => redo(PROJECT))}
          disabled={state === null || busy}
        >
          <Redo2 size={16} aria-hidden="true" /> {L("إعادة", "Redo")}
        </button>
        <button type="button" onClick={() => void writeCapsule()} disabled={state === null || busy}>
          <Download size={16} aria-hidden="true" />
          {L("تصدير كبسولة (.safi.html)", "Export capsule (.safi.html)")}
        </button>
        <button
          type="button"
          onClick={() => setExportModalOpen(true)}
          disabled={state === null || busy}
          data-testid="open-export-deliverable-btn"
          data-tutorial-target="export-deliverable-btn"
        >
          <Download size={16} aria-hidden="true" />
          {L("تصدير المخرجات (XLSX / PDF / PPTX / DOCX)", "Export Deliverables")}
        </button>
        <button type="button" onClick={() => setTutorialOpen(true)} data-testid="open-tutorial-btn">
          <HelpCircle size={16} aria-hidden="true" /> {L("الجولة التعليمية", "Tutorial")}
        </button>
        {analysisProgress ? (
          <button
            type="button"
            className="studio-chip-btn"
            data-testid="cancel-analysis-btn"
            onClick={() => analysisAbortRef.current?.abort()}
          >
            <Ban size={14} aria-hidden="true" /> {analysisProgress.phase} ({analysisProgress.pct}%)
            · {L("إلغاء", "Cancel")}
          </button>
        ) : null}
        {state !== null ? (
          <button
            type="button"
            onClick={() => void run(() => projectState(PROJECT))}
            disabled={busy}
          >
            <RefreshCw size={16} aria-hidden="true" /> {L("تحديث", "Refresh")}
          </button>
        ) : null}
      </div>

      <InteractiveTutorialOverlay
        language={lang}
        open={tutorialOpen}
        onClose={() => setTutorialOpen(false)}
      />

      <DeliverableExportModal
        language={lang}
        open={exportModalOpen}
        busy={busy}
        hiddenSheetCount={wbDetails?.hiddenSheets ?? 0}
        lastStatus={exportStatus}
        onClose={() => setExportModalOpen(false)}
        onExportFormat={handleExportDeliverable}
      />

      {recalculating ? (
        <p className="safi-desktop-studio__note" role="status" aria-live="polite">
          {L(
            "جارٍ إعادة الحساب في Rust — النتائج السابقة موسومة كقديمة.",
            "Recalculating in Rust — previous values are marked Stale."
          )}
        </p>
      ) : null}

      {error !== null ? (
        <p className="safi-desktop-studio__error" role="alert">
          <XCircle size={16} aria-hidden="true" /> {error}
        </p>
      ) : null}

      {state === null ? (
        <p className="safi-desktop-studio__empty">
          {L(
            "ابدأ بفتح ملف CSV أو Excel. لا يخرج أي جزء من بياناتك من جهازك.",
            "Start by opening a CSV or Excel file. No part of your data leaves this machine."
          )}
        </p>
      ) : (
        <>
          <dl className="safi-desktop-studio__meta">
            <div>
              <dt>{L("الإصدار", "Revision")}</dt>
              <dd>{state.revision}</dd>
            </div>
            <div>
              <dt>{L("نسخة البيانات", "Dataset version")}</dt>
              <dd>{state.datasetVersion}</dd>
            </div>
            <div>
              <dt>{L("صفوف", "Rows")}</dt>
              <dd>{state.rowCount}</dd>
            </div>
            <div>
              <dt>{L("أعمدة", "Columns")}</dt>
              <dd>{state.columns.length}</dd>
            </div>
          </dl>

          <h2>{L("المؤشرات الموثقة", "Verified metrics")}</h2>
          <div className="safi-desktop-studio__chips">
            {state.metrics.map((metric) => (
              <VerifiedNumberChip
                key={metric.id}
                metric={metric}
                lang={lang}
                recalculating={recalculating}
              />
            ))}
            {state.errors.map((metricErr) => (
              <VerifiedNumberChip
                key={`err-${metricErr.id}`}
                metric={null}
                fallbackLabel={metricErr.id}
                errorCode={metricErr.code}
                lang={lang}
              />
            ))}
            {state.metrics.length === 0 && state.errors.length === 0 ? (
              <p className="safi-desktop-studio__empty">{L("لا مؤشرات بعد.", "No metrics yet.")}</p>
            ) : null}
          </div>

          <h2>{L("تعريف مؤشر", "Define a metric")}</h2>
          <form
            className="safi-desktop-studio__form"
            onSubmit={(event) => {
              event.preventDefault();
              void run(() =>
                defineMetric(
                  PROJECT,
                  metricId,
                  metricName,
                  metricUnit.length > 0 ? metricUnit : null,
                  metricFormula
                )
              );
            }}
          >
            <label>
              {L("المعرّف", "Id")}
              <input value={metricId} onChange={(event) => setMetricId(event.target.value)} />
            </label>
            <label>
              {L("الاسم", "Name")}
              <input value={metricName} onChange={(event) => setMetricName(event.target.value)} />
            </label>
            <label>
              {L("الوحدة", "Unit")}
              <input value={metricUnit} onChange={(event) => setMetricUnit(event.target.value)} />
            </label>
            <label>
              {L("المعادلة", "Formula")}
              <input
                value={metricFormula}
                onChange={(event) => setMetricFormula(event.target.value)}
                placeholder="SUM(amount) / COUNT(region)"
              />
            </label>
            <button type="submit" disabled={busy}>
              <ShieldCheck size={16} aria-hidden="true" /> {L("أضف المؤشر", "Add metric")}
            </button>
          </form>

          <h2>{L("عمود محسوب", "Calculated column")}</h2>
          <form
            className="safi-desktop-studio__form"
            onSubmit={(event) => {
              event.preventDefault();
              void run(() => addCalculatedColumn(PROJECT, columnName, columnFormula));
            }}
          >
            <label>
              {L("الاسم", "Name")}
              <input value={columnName} onChange={(event) => setColumnName(event.target.value)} />
            </label>
            <label>
              {L("المعادلة", "Formula")}
              <input
                value={columnFormula}
                onChange={(event) => setColumnFormula(event.target.value)}
              />
            </label>
            <button type="submit" disabled={busy}>
              <BarChart3 size={16} aria-hidden="true" /> {L("أضف العمود", "Add column")}
            </button>
          </form>

          <WhatIfPanel
            state={state}
            lang={lang}
            busy={busy}
            onDefineParameter={(name, value) => run(() => defineParameter(PROJECT, name, value))}
            onUpdateParameter={(name, value) => run(() => updateParameter(PROJECT, name, value))}
            onAddScenario={(id, name) => run(() => addScenario(PROJECT, id, name, {}))}
            onRenameScenario={(id, newName) => run(() => renameScenario(PROJECT, id, newName))}
            onActivateScenario={(id) => run(() => activateScenario(PROJECT, id))}
            onResetScenario={() => run(() => resetScenario(PROJECT))}
            onSetScenarioParameter={(sid, p, v) =>
              run(() => setScenarioParameter(PROJECT, sid, p, v))
            }
            onCompareScenarios={(left, right) => compareScenarios(PROJECT, left, right)}
          />

          <WorkbookIntelligenceDrawer
            language={lang}
            details={wbDetails}
            busy={busy}
            onSelectRegion={handleSelectRegion}
          />

          <div
            className="studio-tabs"
            role="tablist"
            aria-label={L("أوضاع الاستوديو الرباعي", "Quad-Studio workspaces")}
            onKeyDown={handleTabKeyDown}
            data-tutorial-target="slides-doc-tabs"
          >
            <button
              type="button"
              role="tab"
              aria-selected={mode === "sheet"}
              className="studio-tab-btn"
              onClick={() => setMode("sheet")}
            >
              <FileSpreadsheet size={15} aria-hidden="true" /> {L("الجدول", "Sheet")}
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={mode === "intelligence"}
              className="studio-tab-btn"
              data-testid="tab-intelligence"
              onClick={() => setMode("intelligence")}
            >
              <BrainCircuit size={15} aria-hidden="true" />{" "}
              {L("الذكاء والتحقق", "Intelligence & Q&A")} (
              {intelSnapshot?.questions.filter((q) => q.status === "Pending").length ?? 0})
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={mode === "dashboard"}
              className="studio-tab-btn"
              data-tutorial-target="dashboard-tab"
              onClick={() => setMode("dashboard")}
            >
              <BarChart3 size={15} aria-hidden="true" /> {L("اللوحة", "Dashboard")} (
              {artifacts.charts.length})
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={mode === "slides"}
              className="studio-tab-btn"
              onClick={() => setMode("slides")}
            >
              <Presentation size={15} aria-hidden="true" /> {L("الشرائح", "Slides")} (
              {artifacts.slides.length})
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={mode === "doc"}
              className="studio-tab-btn"
              onClick={() => setMode("doc")}
            >
              <FileText size={15} aria-hidden="true" /> {L("المستند", "Doc")} (
              {artifacts.docBlocks.length})
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={mode === "freeform"}
              className="studio-tab-btn"
              onClick={() => setMode("freeform")}
            >
              <Layers size={15} aria-hidden="true" /> {L("اللوح الحر", "Canvas")} (
              {artifacts.freeformBlocks.length})
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={mode === "templates"}
              className="studio-tab-btn"
              onClick={() => setMode("templates")}
            >
              <LayoutTemplate size={15} aria-hidden="true" />{" "}
              {L("القوالب والمشاريع", "Templates & Projects")}
            </button>
          </div>

          {mode === "intelligence" ? (
            <div role="tabpanel" aria-label={L("الذكاء والتحقق", "Intelligence & Q&A")}>
              <IntelligenceAndAssistantPanel
                language={lang}
                model={intelSnapshot?.semanticModel ?? null}
                report={intelSnapshot?.verifiedReport ?? null}
                questions={intelSnapshot?.questions ?? []}
                privacySettings={artifacts.privacySettings ?? DEFAULT_PRIVACY}
                onResolveQuestion={(res: StudioQuestionResolutionArtifact) => {
                  const prev = artifacts.questionResolutions ?? [];
                  const next = [...prev.filter((r) => r.questionId !== res.questionId), res];
                  void persistArtifacts({ ...artifacts, questionResolutions: next });
                }}
                onUpdatePrivacy={(nextPrivacy) => {
                  void persistArtifacts({ ...artifacts, privacySettings: nextPrivacy });
                }}
              />
            </div>
          ) : null}

          {mode === "dashboard" ? (
            <div role="tabpanel" aria-label={L("اللوحة", "Dashboard")}>
              <DashboardChartsView
                language={lang}
                project={PROJECT}
                revision={state.revision}
                activeScenario={state.activeScenario}
                columns={state.columns}
                metrics={state.metrics}
                charts={artifacts.charts}
                activeFilterColumn={filterColumn}
                activeFilterValue={filterValue}
                onUpdateCharts={(nextCharts: readonly StudioChartArtifact[]) => {
                  void persistArtifacts({ ...artifacts, charts: nextCharts });
                }}
                onCrossFilter={(col, label) => {
                  setFilterColumn(col);
                  setFilterValue(label);
                  setPageOffset(0);
                }}
                onOpenFilteredSheet={() => {
                  setMode("sheet");
                }}
              />
            </div>
          ) : null}

          {mode === "slides" ? (
            <div role="tabpanel" aria-label={L("الشرائح", "Slides")}>
              <KeynoteSlidesView
                language={lang}
                metrics={state.metrics}
                slides={artifacts.slides}
                onUpdateSlides={(nextSlides: readonly StudioSlideArtifact[]) => {
                  void persistArtifacts({ ...artifacts, slides: nextSlides });
                }}
                onAutoGenerateDeck={() => {
                  if (!intelSnapshot) return;
                  const deck = buildExportPresentationDeck(
                    intelSnapshot.semanticModel,
                    intelSnapshot.verifiedReport,
                    []
                  );
                  const generated: StudioSlideArtifact[] = deck.slides.map((s) => ({
                    id: s.slideId,
                    title: s.title,
                    body: [...s.bullets, s.subtitle].filter(Boolean).join("\n"),
                    metricId: state.metrics[0]?.id ?? null,
                    chartIndex: null,
                    layout: "kpi_split",
                    presenterNotes: s.presenterNotes
                  }));
                  void persistArtifacts({ ...artifacts, slides: generated });
                }}
              />
            </div>
          ) : null}

          {mode === "doc" ? (
            <div role="tabpanel" aria-label={L("المستند", "Doc")}>
              <VerifiedDocView
                language={lang}
                metrics={state.metrics}
                docBlocks={artifacts.docBlocks}
                onUpdateDocBlocks={(nextBlocks: readonly StudioDocBlockArtifact[]) => {
                  void persistArtifacts({ ...artifacts, docBlocks: nextBlocks });
                }}
                onAutoGenerateReport={() => {
                  if (!intelSnapshot) return;
                  const docRep = buildExportDocumentReport(
                    intelSnapshot.semanticModel,
                    intelSnapshot.verifiedReport,
                    []
                  );
                  const generated: StudioDocBlockArtifact[] = docRep.sections.map((sec) => ({
                    id: sec.sectionId,
                    kind: sec.kind === "KeyFindings" ? "callout" : "paragraph",
                    title: sec.heading,
                    content: [...sec.paragraphs, ...sec.bulletItems.map((b) => `• ${b}`)].join(
                      "\n"
                    ),
                    metricId: state.metrics[0]?.id ?? null,
                    authoredBy: "user",
                    pageBreakBefore: sec.pageBreakBefore,
                    headerText: docRep.headerText,
                    footerText: docRep.footerText
                  }));
                  void persistArtifacts({ ...artifacts, docBlocks: generated });
                }}
              />
            </div>
          ) : null}

          {mode === "freeform" ? (
            <div role="tabpanel" aria-label={L("اللوح الحر", "Canvas")}>
              <FreeFormBuilderView
                language={lang}
                metrics={state.metrics}
                freeformBlocks={artifacts.freeformBlocks}
                assets={artifacts.assets}
                onUpdateFreeForm={(
                  nextBlocks: readonly StudioFreeFormBlockArtifact[],
                  nextAssets: readonly StudioAssetArtifact[]
                ) => {
                  void persistArtifacts({
                    ...artifacts,
                    freeformBlocks: nextBlocks,
                    assets: nextAssets
                  });
                }}
              />
            </div>
          ) : null}

          {mode === "templates" ? (
            <div role="tabpanel" aria-label={L("القوالب والمشاريع", "Templates & Projects")}>
              <TemplatesAndProjectsPanel
                language={lang}
                project={PROJECT}
                projectLoaded={state !== null}
                metrics={state.metrics}
                currentTemplateConfig={artifacts.templateConfig}
                onApplyTemplateToWorkspace={(
                  templateConfig: StudioTemplateConfigArtifact,
                  generatedSlides: readonly StudioSlideArtifact[],
                  generatedDocBlocks: readonly StudioDocBlockArtifact[]
                ) => {
                  void persistArtifacts({
                    ...artifacts,
                    slides: generatedSlides,
                    docBlocks: generatedDocBlocks,
                    templateConfig
                  });
                }}
                onSaveBundle={async (savedId: string, displayName: string) => {
                  await saveProjectBundle(PROJECT, savedId, displayName);
                }}
                onLoadBundle={async (savedId: string) => {
                  await run(() => loadProjectBundle(PROJECT, savedId));
                }}
              />
            </div>
          ) : null}

          {mode === "sheet" ? (
            <>
              <div className="safi-desktop-studio__panel-head">
                <h2>{L("الجدول الحي (مقسّم الصفحات)", "Live sheet (paginated)")}</h2>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => void run(() => insertRow(PROJECT, null, {}))}
                >
                  <Plus size={14} aria-hidden="true" /> {L("أضف صفاً", "Insert row")}
                </button>
              </div>

              <div className="safi-desktop-studio__form">
                <label>
                  {L("إضافة عمود مصدر", "Add source column")}
                  <input
                    value={newColumnName}
                    onChange={(event) => setNewColumnName(event.target.value)}
                    placeholder={L("اسم العمود...", "Column name...")}
                  />
                </label>
                <button
                  type="button"
                  disabled={busy || newColumnName.trim().length === 0}
                  onClick={() => {
                    const name = newColumnName.trim();
                    setNewColumnName("");
                    void run(() => addSourceColumn(PROJECT, name));
                  }}
                >
                  <FolderOpen size={14} aria-hidden="true" /> {L("إضافة عمود", "Add column")}
                </button>
                <label>
                  {L("العمود المستهدف", "Target column")}
                  <select
                    value={renameSource}
                    onChange={(event) => setRenameSource(event.target.value)}
                  >
                    <option value="">{L("اختر عموداً...", "Pick column...")}</option>
                    {state.columns.map((col) => (
                      <option key={col.id} value={col.name}>
                        {col.name}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  {L("الاسم الجديد", "New name")}
                  <input
                    value={renameTarget}
                    onChange={(event) => setRenameTarget(event.target.value)}
                    placeholder={L("اسم بديل...", "Renamed column...")}
                  />
                </label>
                <button
                  type="button"
                  disabled={busy || renameSource.length === 0 || renameTarget.trim().length === 0}
                  onClick={() => {
                    const from = renameSource;
                    const to = renameTarget.trim();
                    setRenameTarget("");
                    void run(() => renameColumn(PROJECT, from, to));
                  }}
                >
                  {L("إعادة تسمية", "Rename column")}
                </button>
                <button
                  type="button"
                  disabled={busy || renameSource.length === 0}
                  onClick={() => {
                    const target = renameSource;
                    setRenameSource("");
                    void run(() => deleteColumn(PROJECT, target));
                  }}
                >
                  {L("حذف العمود", "Delete column")}
                </button>
                <label>
                  {L("تنظيف الفراغات", "Trim whitespace")}
                  <select
                    value={trimTarget}
                    onChange={(event) => setTrimTarget(event.target.value)}
                  >
                    <option value="">{L("اختر عموداً...", "Pick column...")}</option>
                    {state.columns.map((col) => (
                      <option key={col.id} value={col.name}>
                        {col.name}
                      </option>
                    ))}
                  </select>
                </label>
                <button
                  type="button"
                  disabled={busy || trimTarget.length === 0}
                  onClick={() => void run(() => trimColumn(PROJECT, trimTarget))}
                >
                  {L("تنظيف العمود", "Trim column")}
                </button>
              </div>

              <div className="safi-desktop-studio__form">
                <label>
                  {L("بحث", "Search")}
                  <input
                    value={searchQuery}
                    onChange={(event) => {
                      setSearchQuery(event.target.value);
                      setPageOffset(0);
                    }}
                    placeholder={L("ابحث في الأعمدة...", "Search columns...")}
                  />
                </label>
                <label>
                  {L("تصفية العمود", "Filter column")}
                  <select
                    value={filterColumn}
                    onChange={(event) => {
                      setFilterColumn(event.target.value);
                      setPageOffset(0);
                    }}
                  >
                    <option value="">{L("الكل", "All")}</option>
                    {state.columns.map((col) => (
                      <option key={col.id} value={col.name}>
                        {col.name}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  {L("قيمة التصفية", "Filter value")}
                  <input
                    value={filterValue}
                    onChange={(event) => {
                      setFilterValue(event.target.value);
                      setPageOffset(0);
                    }}
                  />
                </label>
                <label>
                  {L("حجم الصفحة", "Page size")}
                  <select
                    value={pageLimit}
                    onChange={(event) => {
                      setPageLimit(Number(event.target.value));
                      setPageOffset(0);
                    }}
                  >
                    {PAGE_SIZES.map((size) => (
                      <option key={size} value={size}>
                        {size}
                      </option>
                    ))}
                  </select>
                </label>
              </div>

              <div className="safi-desktop-studio__table-wrap">
                <table className="safi-desktop-studio__table">
                  <thead>
                    <tr>
                      <th scope="col">{L("هوية الصف", "Row ID")}</th>
                      {state.columns.map((column) => (
                        <th key={column.id} scope="col">
                          <button
                            type="button"
                            className="safi-desktop-studio__col-sort"
                            onClick={() => {
                              if (sortColumn === column.name) {
                                const nextAsc = !sortAscending;
                                setSortAscending(nextAsc);
                                void run(() => sortRows(PROJECT, column.name, nextAsc));
                              } else {
                                setSortColumn(column.name);
                                setSortAscending(true);
                                void run(() => sortRows(PROJECT, column.name, true));
                              }
                            }}
                          >
                            <span>{column.name}</span>
                            <ArrowDownUp size={12} aria-hidden="true" />
                          </button>
                          {state.calculated.includes(column.name) ? (
                            <span className="safi-desktop-studio__calc">
                              {" "}
                              · {L("محسوب", "computed")}
                            </span>
                          ) : null}
                        </th>
                      ))}
                      <th scope="col">{L("إجراء", "Action")}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {(rowPage?.rows ?? []).map((rowItem) => (
                      <tr key={rowItem.row_id}>
                        <td>
                          <span className="safi-desktop-studio__rowid">
                            r{rowItem.row_id}
                            {rowItem.origin_row_id !== null
                              ? ` (src r${rowItem.origin_row_id})`
                              : ` (${L("جديد", "new")})`}
                          </span>
                        </td>
                        {rowItem.cells.map((cell, columnIndex) => {
                          const columnLabel = state.columns[columnIndex]?.name ?? "";
                          const editable = !state.calculated.includes(columnLabel);
                          return (
                            <td
                              key={`${rowItem.row_id}-${columnLabel}`}
                              data-changed={cell.changed ? "true" : undefined}
                            >
                              {editable ? (
                                <input
                                  key={`${rowItem.row_id}-${columnLabel}-${state.revision}`}
                                  defaultValue={cell.raw}
                                  aria-label={`${columnLabel} r${rowItem.row_id}`}
                                  onBlur={(event) => {
                                    if (event.target.value !== cell.raw) {
                                      const nextVal = event.target.value;
                                      void run(() =>
                                        setCellById(
                                          PROJECT,
                                          rowItem.row_id,
                                          columnLabel,
                                          nextVal,
                                          state.revision
                                        )
                                      );
                                    }
                                  }}
                                  onKeyDown={(event) => {
                                    if (event.key === "Enter") {
                                      event.currentTarget.blur();
                                    }
                                  }}
                                />
                              ) : (
                                <span>{cell.visible}</span>
                              )}
                            </td>
                          );
                        })}
                        <td>
                          <button
                            type="button"
                            aria-label={`${L("حذف", "Delete")} r${rowItem.row_id}`}
                            disabled={busy}
                            onClick={() => void run(() => deleteRow(PROJECT, rowItem.row_id))}
                          >
                            <Trash2 size={14} aria-hidden="true" />
                          </button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>

              {rowPage !== null ? (
                <div className="safi-desktop-studio__pager">
                  <button
                    type="button"
                    disabled={pageOffset === 0}
                    onClick={() => setPageOffset(Math.max(0, pageOffset - pageLimit))}
                  >
                    {L("السابق", "Previous")}
                  </button>
                  <span>
                    {rowPage.filtered_rows === 0
                      ? L("0 صفوف مطابقة", "0 matching rows")
                      : `${rowPage.offset + 1}–${rowPage.offset + rowPage.rows.length} / ${rowPage.filtered_rows} (${L("الإجمالي", "total")}: ${rowPage.total_rows})`}
                  </span>
                  <button
                    type="button"
                    disabled={rowPage.offset + rowPage.rows.length >= rowPage.filtered_rows}
                    onClick={() => setPageOffset(pageOffset + pageLimit)}
                  >
                    {L("التالي", "Next")}
                  </button>
                </div>
              ) : null}

              <h2>{L("اللوحة (تحديث تفاعلي)", "Dashboard (reactive)")}</h2>
              <div className="safi-desktop-studio__form">
                <label>
                  {L("التجميع", "Aggregate")}
                  <select
                    value={aggregate}
                    onChange={(event) => setAggregate(event.target.value as Aggregate)}
                  >
                    {AGGREGATES.map((name) => (
                      <option key={name} value={name}>
                        {name}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  {L("العمود", "Column")}
                  <select
                    value={valueColumn}
                    onChange={(event) => setValueColumn(event.target.value)}
                  >
                    {state.columns.map((column) => (
                      <option key={column.id} value={column.name}>
                        {column.name}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  {L("التجميع حسب", "Group by")}
                  <select value={groupBy} onChange={(event) => setGroupBy(event.target.value)}>
                    {state.columns.map((column) => (
                      <option key={column.id} value={column.name}>
                        {column.name}
                      </option>
                    ))}
                  </select>
                </label>
                <button type="button" onClick={() => void loadChart()} disabled={busy}>
                  <BarChart3 size={16} aria-hidden="true" /> {L("حدّث الرسم", "Refresh chart")}
                </button>
              </div>
              <div className="safi-desktop-studio__bars">
                {groups.map((group) => {
                  const numeric = Number(group.value);
                  const maximum = Math.max(
                    ...groups.map((candidate) => Math.abs(Number(candidate.value))),
                    1
                  );
                  const width = Number.isFinite(numeric)
                    ? Math.round((Math.abs(numeric) / maximum) * 100)
                    : 0;
                  return (
                    <div key={group.label} className="safi-desktop-studio__bar">
                      <span>{group.label}</span>
                      <div className="safi-desktop-studio__track">
                        <div className="safi-desktop-studio__fill" style={{ width: `${width}%` }} />
                      </div>
                      <strong>{group.value}</strong>
                    </div>
                  );
                })}
              </div>
              {groups.length > 0 ? (
                <p className="safi-desktop-studio__note">
                  {L(
                    "القيم مجمّعة في Rust وتُحدَّث تلقائياً عند كل تعديل؛ الواجهة ترسم النسبة فقط.",
                    "Values are aggregated in Rust and update automatically on every edit; the UI only draws ratios."
                  )}
                </p>
              ) : null}

              <h2>{L("لوحة الأوامر والذكاء الاصطناعي", "Command Bar & AI Interpreter")}</h2>
              <CommandBarPanel
                language={lang}
                projectLoaded={state !== null}
                schema={{
                  columns: state.columns.map((c) => ({ name: c.name, type: c.type })),
                  metricIds: state.metrics.map((m) => m.id),
                  parameterNames: state.parameters.map((p) => p.name)
                }}
                onExecuteDsl={(dslJson) => run(() => applyDslCommand(PROJECT, dslJson))}
              />

              <h2>{L("جودة البيانات", "Data quality")}</h2>
              <AnomalyPanel
                language={lang}
                anomalies={state.anomalies}
                isMutating={busy}
                onApplyFix={(fix) => run(() => applyAnomalyFix(PROJECT, fix, state.revision))}
                onFocusRowId={(rowId) => {
                  setSearchQuery(String(rowId));
                  setPageOffset(0);
                }}
              />
            </>
          ) : null}
        </>
      )}
    </section>
  );
}
