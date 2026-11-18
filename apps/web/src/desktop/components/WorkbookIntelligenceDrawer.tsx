import React, { useState } from "react";
import {
  AlertTriangle,
  CheckCircle2,
  Eye,
  EyeOff,
  FileSpreadsheet,
  Layers,
  Lock,
  ShieldAlert,
  Table as TableIcon
} from "lucide-react";
import type { DesktopWorkbookDetails } from "../studio-bridge";

export interface WorkbookIntelligenceDrawerProps {
  readonly language: "en" | "ar";
  readonly details: DesktopWorkbookDetails | null;
  readonly busy: boolean;
  readonly onSelectRegion: (sheetName: string, tableId: string) => Promise<void>;
}

export function WorkbookIntelligenceDrawer({
  language,
  details,
  busy,
  onSelectRegion
}: WorkbookIntelligenceDrawerProps): React.ReactElement | null {
  const isAr = language === "ar";
  const [selectedSheetName, setSelectedSheetName] = useState<string | null>(null);
  const [showHiddenSheets, setShowHiddenSheets] = useState(false);

  if (!details) {
    return null;
  }

  const activeSheetName =
    selectedSheetName ?? details.activeSheet ?? details.worksheets[0]?.name ?? "";
  const activeSheet =
    details.worksheets.find((w) => w.name === activeSheetName) ?? details.worksheets[0];

  const visibleOrAllowed = details.worksheets.filter(
    (w) => showHiddenSheets || w.visibility === "Visible" || w.name === details.activeSheet
  );

  return (
    <section
      className="studio-panel"
      data-testid="workbook-intelligence-drawer"
      data-tutorial-target="workbook-explorer"
    >
      <div className="studio-panel-header">
        <div>
          <h3>
            <FileSpreadsheet size={16} />{" "}
            {isAr ? "مستكشف المصنف والميتاداتا" : "Workbook Explorer & Audit"}
          </h3>
          <p className="studio-muted-note">
            {isAr
              ? `التنسيق: ${details.detectedFormat} • نظام التاريخ: ${details.dateSystem} • الأوراق: ${details.totalSheets} (${details.hiddenSheets} مخفية) • المعادلات المفحوصة: ${details.totalFormulasVerified}/${details.totalFormulasDetected}`
              : `Format: ${details.detectedFormat} · Date System: ${details.dateSystem} · Sheets: ${details.totalSheets} (${details.hiddenSheets} hidden) · Formulas Verified: ${details.totalFormulasVerified}/${details.totalFormulasDetected}`}
          </p>
        </div>
        <div className="studio-quick-templates">
          <button
            type="button"
            className="studio-chip-btn"
            data-testid="toggle-hidden-sheets-btn"
            onClick={() => setShowHiddenSheets((v) => !v)}
          >
            {showHiddenSheets ? <Eye size={14} /> : <EyeOff size={14} />}{" "}
            {isAr
              ? showHiddenSheets
                ? "إخفاء الأوراق المخفية"
                : `عرض المخفية (${details.hiddenSheets})`
              : showHiddenSheets
                ? "Hide Hidden Sheets"
                : `Show Hidden (${details.hiddenSheets})`}
          </button>
        </div>
      </div>

      {details.macrosNeutralized ? (
        <div className="studio-alert-error" role="status" data-testid="macros-neutralized-banner">
          <ShieldAlert size={15} />{" "}
          {isAr
            ? "تم اكتشاف مشروع ماكرو VBA وتحييده أمنيًا (لا يتم تنفيذ أي ماكرو)."
            : "VBA Macro Project (vbaProject.bin) was detected and safely neutralized by security policy."}
        </div>
      ) : null}

      {details.warnings.length > 0 ? (
        <div className="studio-insight-warning" data-testid="workbook-warnings-box">
          <strong>
            <AlertTriangle size={14} /> {isAr ? "تنبيهات المصنف:" : "Workbook Disclosures:"}
          </strong>
          <ul>
            {details.warnings.map((w, idx) => (
              <li key={`${idx}_${w.slice(0, 16)}`}>{w}</li>
            ))}
          </ul>
        </div>
      ) : null}

      <div
        className="studio-sheet-tabs"
        role="tablist"
        aria-label={isAr ? "أوراق العمل" : "Worksheets"}
      >
        {visibleOrAllowed.map((ws) => {
          const isSelected = ws.name === activeSheet?.name;
          const isCurrentActive = ws.name === details.activeSheet;
          return (
            <button
              key={ws.name}
              type="button"
              role="tab"
              aria-selected={isSelected}
              className={`studio-chip-btn ${isSelected ? "is-active" : ""}`}
              data-testid={`sheet-tab-${ws.index}`}
              onClick={() => setSelectedSheetName(ws.name)}
            >
              <Layers size={13} />
              <span>{ws.name}</span>
              {ws.visibility !== "Visible" ? (
                <span className="studio-badge-tag">{ws.visibility}</span>
              ) : null}
              {isCurrentActive ? (
                <CheckCircle2 size={12} aria-label={isAr ? "نشطة" : "Active"} />
              ) : null}
              {ws.isProtected ? <Lock size={12} /> : null}
            </button>
          );
        })}
      </div>

      {activeSheet ? (
        <div className="studio-sheet-detail-card">
          <div className="studio-sheet-meta-row">
            <span>
              <strong>{isAr ? "التصنيف:" : "Classification:"}</strong> {activeSheet.classification}{" "}
              ({activeSheet.classificationConfidencePct}%)
            </span>
            <span>
              <strong>{isAr ? "الأبعاد:" : "Grid:"}</strong> {activeSheet.rawRowCount} ×{" "}
              {activeSheet.rawColCount}
            </span>
            <span>
              <strong>{isAr ? "التجميد والدمج:" : "Layout:"}</strong>{" "}
              {activeSheet.hasFrozenPanes
                ? isAr
                  ? "مجمّد"
                  : "Frozen Panes"
                : isAr
                  ? "عادي"
                  : "Standard"}{" "}
              · {activeSheet.mergedRanges.length} {isAr ? "خلايا مدمجة" : "merges"}
            </span>
          </div>

          <div className="studio-table-regions-list">
            {activeSheet.tables.map((tbl) => {
              const isRegionActive =
                activeSheet.name === details.activeSheet && tbl.tableId === details.activeTableId;
              return (
                <div
                  key={`${activeSheet.name}_${tbl.tableId}`}
                  className={`studio-region-card ${isRegionActive ? "is-active" : ""}`}
                >
                  <div>
                    <strong>
                      <TableIcon size={14} /> {tbl.title || tbl.tableId}
                    </strong>
                    <div className="studio-muted-note">
                      {isAr
                        ? `${tbl.rowCount} صف تحليلي · ${tbl.headers.length} أعمدة · ثقة ${tbl.confidencePct}% · صفوف مفصولة: ${tbl.separatedRows.length}`
                        : `${tbl.rowCount} analytical rows · ${tbl.headers.length} columns · ${tbl.confidencePct}% confidence · ${tbl.separatedRows.length} separated total/banner rows`}
                    </div>
                  </div>
                  <button
                    type="button"
                    disabled={busy || isRegionActive}
                    className="studio-chip-btn"
                    data-testid={`activate-region-${activeSheet.index}-${tbl.tableId}`}
                    onClick={() => void onSelectRegion(activeSheet.name, tbl.tableId)}
                  >
                    {isRegionActive
                      ? isAr
                        ? "الجدول النشط"
                        : "Active in Grid"
                      : isAr
                        ? "تحميل في الشبكة"
                        : "Switch Active Table"}
                  </button>
                </div>
              );
            })}
          </div>

          {activeSheet.formulaSamples.length > 0 ? (
            <details className="studio-formula-audit">
              <summary>
                {isAr
                  ? `تدقيق المعادلات والروابط (${activeSheet.formulaSamples.length})`
                  : `Formula, Link & Comment Audit (${activeSheet.formulaSamples.length})`}
              </summary>
              <ul className="studio-audit-list">
                {activeSheet.formulaSamples.slice(0, 20).map((f) => (
                  <li key={`${activeSheet.name}_${f.address}`}>
                    <code>{f.address}</code> ·{" "}
                    {f.formula ? <code>={f.formula}</code> : <span>{f.dataType}</span>} ·{" "}
                    <span className="studio-badge-tag">{f.formulaStatus}</span>
                    {f.comment ? <span> · 💬 {f.comment}</span> : null}
                    {f.hyperlink ? <span> · 🔗 {f.hyperlink}</span> : null}
                  </li>
                ))}
              </ul>
            </details>
          ) : null}
        </div>
      ) : null}

      {details.definedNames.length > 0 || details.embeddedObjects.length > 0 ? (
        <div className="studio-muted-note">
          {isAr
            ? `النطاقات المعرّفة: ${details.definedNames.length} · الكائنات المضمنة: ${details.embeddedObjects.length}`
            : `Defined Names: ${details.definedNames.length} · Embedded Objects: ${details.embeddedObjects.length}`}
        </div>
      ) : null}
    </section>
  );
}
