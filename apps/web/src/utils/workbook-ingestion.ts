/**
 * Universal Workbook Ingestion & Structure Detection Engine (`Phase 1`).
 *
 * Mirrors `crates/sf-io/src/workbook_intelligence.rs` for the web/PWA and desktop UI layers:
 * - Format Compatibility Matrix (`XLSX`, `XLS`, `XLSM`, `XLSB`, `CSV`, `ODS`)
 * - Worksheet visibility (`Visible`, `Hidden`, `VeryHidden`) & purpose classification
 * - Multi-table & irregular structure segmentation (title banners, multi-row/merged headers,
 *   repeated headers, subtotal/grand-total separation, footnotes)
 * - Complete cell-level metadata (raw value, formula expression, cached value, recalculated
 *   verification, number format, 1900/1904 date system, comments, hyperlinks, leading zeros)
 * - Embedded object inventory & cancellation/progress tracking
 */

export type WorkbookFormat = "Xlsx" | "Xls" | "Xlsm" | "Xlsb" | "Csv" | "Ods" | "Unknown";

export type FormatSupportLevel =
  | "FullSupport"
  | "SupportedWithMacroNeutralization"
  | "SupportedTabularAndFormulas"
  | "Unsupported";

export interface FormatCompatibilityEntry {
  readonly format: WorkbookFormat;
  readonly extensions: readonly string[];
  readonly supportLevel: FormatSupportLevel;
  readonly supportedFeatures: readonly string[];
  readonly limitations: readonly string[];
}

export type SheetVisibility = "Visible" | "Hidden" | "VeryHidden";

export type SheetClassification =
  | "MainDataset"
  | "ReferenceData"
  | "LookupTable"
  | "Summary"
  | "Report"
  | "Configuration"
  | "Instructions"
  | "SupportingCalculations"
  | "PivotOutput"
  | "Unknown";

export type WorkbookDateSystem = "Standard1900" | "Macintosh1904";

export type IngestedCellDataType =
  | "Empty"
  | "Text"
  | "ZeroPreservedText"
  | "Integer"
  | "Decimal"
  | "Currency"
  | "Percentage"
  | "Boolean"
  | "Date"
  | "DateTime"
  | "Time"
  | "FormulaError";

export type FormulaVerificationStatus =
  | "LiteralValue"
  | "VerifiedCachedAndRecalculated"
  | "CachedResultOnly"
  | "RecalculatedLocally"
  | "VolatileFormula"
  | "ExternalWorkbookReference"
  | "FormulaError"
  | "UnverifiedFormula";

export interface IngestedCellMetadata {
  readonly sheetName: string;
  readonly address: string;
  readonly row: number;
  readonly col: number;
  readonly rawValue: string;
  readonly formula?: string | undefined;
  readonly cachedValue?: string | undefined;
  readonly recalculatedValue?: string | undefined;
  readonly formulaStatus: FormulaVerificationStatus;
  readonly dataType: IngestedCellDataType;
  readonly numberFormat?: string | undefined;
  readonly mergedFrom?: string | undefined;
  readonly hyperlink?: string | undefined;
  readonly comment?: string | undefined;
}

export interface SheetRange {
  readonly startRow: number;
  readonly startCol: number;
  readonly endRow: number;
  readonly endCol: number;
}

export type SeparatedRowKind =
  | "Title"
  | "Subtitle"
  | "RepeatedHeader"
  | "Subtotal"
  | "GrandTotal"
  | "Footnote";

export interface SeparatedSheetRow {
  readonly sheetRow: number;
  readonly kind: SeparatedRowKind;
  readonly values: readonly string[];
}

export interface DetectedTableRegion {
  readonly tableId: string;
  readonly title: string;
  readonly sheetName: string;
  readonly bounds: SheetRange;
  readonly a1Range: string;
  readonly headerRows: readonly number[];
  readonly headers: readonly string[];
  readonly dataRows: readonly (readonly string[])[];
  readonly sourceRowIndices: readonly number[];
  readonly separatedRows: readonly SeparatedSheetRow[];
  readonly confidencePct: number;
}

export type EmbeddedObjectKind =
  | "Chart"
  | "Image"
  | "CommentOrNote"
  | "Hyperlink"
  | "DrawingOrShape"
  | "PivotTable"
  | "ExcelTable"
  | "VbaMacroProject";

export type EmbeddedObjectHandling =
  | "Extracted"
  | "Summarized"
  | "NeutralizedBySecurityPolicy"
  | "Unsupported";

export interface EmbeddedObjectRecord {
  readonly kind: EmbeddedObjectKind;
  readonly location: string;
  readonly handling: EmbeddedObjectHandling;
  readonly summary: string;
}

export interface DefinedNameRecord {
  readonly name: string;
  readonly refersTo: string;
  readonly isBuiltIn: boolean;
}

export interface WorksheetIntelligence {
  readonly name: string;
  readonly index: number;
  readonly visibility: SheetVisibility;
  readonly classification: SheetClassification;
  readonly classificationConfidencePct: number;
  readonly classificationReason: string;
  readonly usedRange?: SheetRange | undefined;
  readonly rawRowCount: number;
  readonly rawColCount: number;
  readonly mergedRanges: readonly string[];
  readonly hasFrozenPanes: boolean;
  readonly autoFilterRange?: string | undefined;
  readonly printArea?: string | undefined;
  readonly isProtected: boolean;
  readonly tables: readonly DetectedTableRegion[];
  readonly cellMetadata: readonly IngestedCellMetadata[];
  readonly formulaCount: number;
  readonly verifiedFormulaCount: number;
}

export interface IngestionProgressUpdate {
  readonly phase:
    | "Preflight"
    | "SheetDiscovery"
    | "TableSegmentation"
    | "FormulaVerification"
    | "Completed";
  readonly sheetsProcessed: number;
  readonly totalSheets: number;
  readonly percentComplete: number;
}

export interface WorkbookIngestionReport {
  readonly detectedFormat: WorkbookFormat;
  readonly supportLevel: FormatSupportLevel;
  readonly dateSystem: WorkbookDateSystem;
  readonly macrosNeutralized: boolean;
  readonly totalSheets: number;
  readonly visibleSheets: number;
  readonly hiddenSheets: number;
  readonly totalTablesExtracted: number;
  readonly totalAnalyticalRows: number;
  readonly totalFormulasDetected: number;
  readonly totalFormulasVerified: number;
  readonly definedNames: readonly DefinedNameRecord[];
  readonly embeddedObjects: readonly EmbeddedObjectRecord[];
  readonly worksheets: readonly WorksheetIntelligence[];
  readonly warnings: readonly string[];
  readonly progressLog: readonly IngestionProgressUpdate[];
}

export interface RawSheetInput {
  readonly name: string;
  readonly visibility?: SheetVisibility | undefined;
  readonly grid: readonly (readonly string[])[];
  readonly formulas?: Readonly<Record<string, string>> | undefined;
  readonly numberFormats?: Readonly<Record<string, string>> | undefined;
  readonly comments?: Readonly<Record<string, string>> | undefined;
  readonly hyperlinks?: Readonly<Record<string, string>> | undefined;
  readonly mergedRanges?: readonly string[] | undefined;
  readonly hasFrozenPanes?: boolean | undefined;
  readonly autoFilterRange?: string | undefined;
  readonly printArea?: string | undefined;
  readonly isProtected?: boolean | undefined;
  readonly hasPivotTable?: boolean | undefined;
}

export interface WorkbookIngestionOptions {
  readonly filename?: string | undefined;
  readonly formatHint?: WorkbookFormat | undefined;
  readonly dateSystem?: WorkbookDateSystem | undefined;
  readonly hasVbaMacros?: boolean | undefined;
  readonly definedNames?: readonly DefinedNameRecord[] | undefined;
  readonly embeddedObjects?: readonly EmbeddedObjectRecord[] | undefined;
  readonly maxRows?: number | undefined;
  readonly signal?: AbortSignal | undefined;
  readonly onProgress?: ((update: IngestionProgressUpdate) => void) | undefined;
}

const EMPTY_SENTINELS = new Set([
  "",
  "null",
  "nil",
  "none",
  "n/a",
  "na",
  "#n/a",
  "-",
  "--",
  "—",
  "غير متوفر",
  "لا يوجد",
  "فارغ"
]);

export function getFormatCompatibilityMatrix(): readonly FormatCompatibilityEntry[] {
  return [
    {
      format: "Xlsx",
      extensions: ["xlsx"],
      supportLevel: "FullSupport",
      supportedFeatures: [
        "Visible, Hidden, and VeryHidden worksheets",
        "Multi-table & irregular header segmentation",
        "Cell values, formulas, cached values & local recalculation",
        "Number formats, 1900/1904 date systems, merges, comments, hyperlinks",
        "Defined names, Excel tables, embedded object inventory"
      ],
      limitations: ["Password-encrypted workbooks require decryption before import"]
    },
    {
      format: "Xlsm",
      extensions: ["xlsm"],
      supportLevel: "SupportedWithMacroNeutralization",
      supportedFeatures: [
        "All XLSX worksheet, table, formula, and metadata features",
        "Automatic VBA project detection and macro neutralization audit"
      ],
      limitations: ["VBA macros (vbaProject.bin) are never executed by security policy"]
    },
    {
      format: "Xls",
      extensions: ["xls"],
      supportLevel: "SupportedTabularAndFormulas",
      supportedFeatures: [
        "Multi-sheet BIFF8 ingestion, cell values, formulas, defined names",
        "Multi-table segmentation and subtotal separation"
      ],
      limitations: [
        "Legacy BIFF5/encrypted XLS streams and embedded OLE drawings are reported as unsupported"
      ]
    },
    {
      format: "Xlsb",
      extensions: ["xlsb"],
      supportLevel: "SupportedTabularAndFormulas",
      supportedFeatures: [
        "Binary OOXML sheets, cell values, formulas, defined names, multi-table detection"
      ],
      limitations: ["VBA macros neutralized; binary drawing styles summarized"]
    },
    {
      format: "Csv",
      extensions: ["csv", "tsv", "txt"],
      supportLevel: "FullSupport",
      supportedFeatures: [
        "UTF-8/UTF-16/Windows-1256 Arabic encoding detection",
        "Auto-delimiter detection, leading-zero preservation, multi-region segmentation"
      ],
      limitations: ["Single-sheet flat text format (no native formulas or multiple worksheets)"]
    },
    {
      format: "Ods",
      extensions: ["ods"],
      supportLevel: "SupportedTabularAndFormulas",
      supportedFeatures: [
        "OpenDocument multi-sheet values, formulas, defined names, multi-table detection"
      ],
      limitations: ["ODF chart/drawing objects are inventoried at summary level"]
    }
  ];
}

export function coordsToA1(row: number, col: number): string {
  let c = Math.max(0, Math.floor(col)) + 1;
  let colStr = "";
  while (c > 0) {
    const rem = (c - 1) % 26;
    colStr = String.fromCharCode(65 + rem) + colStr;
    c = Math.floor((c - 1) / 26);
  }
  return `${colStr}${Math.max(0, Math.floor(row)) + 1}`;
}

export function parseA1Coord(ref: string): { row: number; col: number } | null {
  const cleaned = ref.trim().replace(/\$/g, "");
  const match = /^([A-Za-z]{1,3})(\d{1,7})$/.exec(cleaned);
  if (!match) return null;
  const colPart = (match[1] ?? "").toUpperCase();
  const rowNum = Number.parseInt(match[2] ?? "0", 10);
  if (!Number.isFinite(rowNum) || rowNum <= 0) return null;
  let colNum = 0;
  for (let i = 0; i < colPart.length; i += 1) {
    colNum = colNum * 26 + (colPart.charCodeAt(i) - 64);
  }
  return { row: rowNum - 1, col: colNum - 1 };
}

export function parseA1Range(ref: string): SheetRange | null {
  const parts = ref.split(":");
  if (parts.length === 0 || parts.length > 2) return null;
  const start = parseA1Coord(parts[0] ?? "");
  if (!start) return null;
  const end = parts[1] ? parseA1Coord(parts[1]) : start;
  if (!end) return null;
  return {
    startRow: Math.min(start.row, end.row),
    startCol: Math.min(start.col, end.col),
    endRow: Math.max(start.row, end.row),
    endCol: Math.max(start.col, end.col)
  };
}

export function formatA1Range(range: SheetRange): string {
  return `${coordsToA1(range.startRow, range.startCol)}:${coordsToA1(range.endRow, range.endCol)}`;
}

export function detectWorkbookFormatFromFilename(filename?: string): WorkbookFormat {
  if (!filename) return "Xlsx";
  const ext = filename.split(".").pop()?.toLowerCase() ?? "";
  switch (ext) {
    case "xlsx":
      return "Xlsx";
    case "xlsm":
      return "Xlsm";
    case "xls":
      return "Xls";
    case "xlsb":
      return "Xlsb";
    case "csv":
    case "tsv":
    case "txt":
      return "Csv";
    case "ods":
      return "Ods";
    default:
      return "Unknown";
  }
}

export function classifyCellType(rawValue: string, numberFormat?: string): IngestedCellDataType {
  const trimmed = rawValue.trim();
  if (EMPTY_SENTINELS.has(trimmed.toLowerCase())) {
    return "Empty";
  }
  if (
    trimmed.startsWith("#") &&
    (trimmed.endsWith("!") || trimmed.endsWith("?") || trimmed.toUpperCase() === "#N/A")
  ) {
    return "FormulaError";
  }
  if (
    (trimmed.length > 1 &&
      trimmed.startsWith("0") &&
      !trimmed.startsWith("0.") &&
      /^\d+$/.test(trimmed)) ||
    /^\+\d{5,15}$/.test(trimmed)
  ) {
    return "ZeroPreservedText";
  }
  if (trimmed.toLowerCase() === "true" || trimmed.toLowerCase() === "false") {
    return "Boolean";
  }
  if (trimmed.endsWith("%") && Number.isFinite(Number(trimmed.slice(0, -1).trim()))) {
    return "Percentage";
  }
  const fmtLower = (numberFormat ?? "").toLowerCase();
  if (fmtLower.includes("%") && Number.isFinite(Number(trimmed.replace(/,/g, "")))) {
    return "Percentage";
  }
  const currencyCleaned = trimmed
    .replace(/^[$€£]\s*/, "")
    .replace(/\s*(EGP|SAR|AED|USD|EUR|ج\.م|ر\.س)$/i, "")
    .replace(/,/g, "");
  if (
    (currencyCleaned.length !== trimmed.replace(/,/g, "").length ||
      /[$€£]|egp|sar|aed|usd|ج\.م|ر\.س/i.test(fmtLower)) &&
    currencyCleaned.length > 0 &&
    Number.isFinite(Number(currencyCleaned))
  ) {
    return "Currency";
  }
  if (/^-?\d+$/.test(trimmed)) {
    return "Integer";
  }
  const numCandidate = trimmed.replace(/,/g, "");
  if (/^-?\d+(\.\d+)?$/.test(numCandidate) && Number.isFinite(Number(numCandidate))) {
    return "Decimal";
  }
  if (
    /^\d{4}[-/]\d{1,2}[-/]\d{1,2}([ T]\d{1,2}:\d{2}(:\d{2})?)?$/.test(trimmed) ||
    /^\d{1,2}[-/]\d{1,2}[-/]\d{4}([ T]\d{1,2}:\d{2}(:\d{2})?)?$/.test(trimmed)
  ) {
    return trimmed.includes(":") ? "DateTime" : "Date";
  }
  if (/^\d{1,2}:\d{2}(:\d{2})?$/.test(trimmed)) {
    return "Time";
  }
  return "Text";
}

export function evaluateSimpleFormula(
  formula: string,
  numericLookup: ReadonlyMap<string, number>
): string | undefined {
  const expr = formula.trim().replace(/^=/, "").trim();
  const upper = expr.toUpperCase();
  if (upper.includes("!") || upper.includes("[")) {
    return undefined;
  }

  for (const fn of ["SUM", "AVERAGE", "MIN", "MAX", "COUNT"] as const) {
    const prefix = `${fn}(`;
    if (upper.startsWith(prefix) && upper.endsWith(")")) {
      const inner = upper.slice(prefix.length, -1).trim();
      const range = parseA1Range(inner);
      if (!range) return undefined;
      const vals: number[] = [];
      for (let r = range.startRow; r <= range.endRow; r += 1) {
        for (let c = range.startCol; c <= range.endCol; c += 1) {
          const v = numericLookup.get(`${r},${c}`);
          if (v !== undefined && Number.isFinite(v)) {
            vals.push(v);
          }
        }
      }
      if (vals.length === 0 && fn !== "COUNT") return "0";
      let res = 0;
      switch (fn) {
        case "SUM":
          res = vals.reduce((a, b) => a + b, 0);
          break;
        case "AVERAGE":
          res = vals.reduce((a, b) => a + b, 0) / vals.length;
          break;
        case "MIN":
          res = Math.min(...vals);
          break;
        case "MAX":
          res = Math.max(...vals);
          break;
        case "COUNT":
          res = vals.length;
          break;
      }
      return Number.isInteger(res) ? String(res) : Number(res.toFixed(6)).toString();
    }
  }

  for (const op of ["+", "-", "*", "/"] as const) {
    const idx = upper.indexOf(op);
    if (idx > 0) {
      const leftStr = upper.slice(0, idx).trim();
      const rightStr = upper.slice(idx + 1).trim();
      const leftCoord = parseA1Coord(leftStr);
      const rightCoord = parseA1Coord(rightStr);
      const lVal = leftCoord
        ? numericLookup.get(`${leftCoord.row},${leftCoord.col}`)
        : Number(leftStr);
      const rVal = rightCoord
        ? numericLookup.get(`${rightCoord.row},${rightCoord.col}`)
        : Number(rightStr);
      if (
        lVal === undefined ||
        rVal === undefined ||
        !Number.isFinite(lVal) ||
        !Number.isFinite(rVal)
      ) {
        return undefined;
      }
      if (op === "/" && Math.abs(rVal) < 1e-12) {
        return "#DIV/0!";
      }
      const res =
        op === "+"
          ? lVal + rVal
          : op === "-"
            ? lVal - rVal
            : op === "*"
              ? lVal * rVal
              : lVal / rVal;
      return Number.isInteger(res) ? String(res) : Number(res.toFixed(6)).toString();
    }
  }
  return undefined;
}

function classifyFormulaVerification(
  formula: string,
  cachedValue: string | undefined,
  recalculatedValue: string | undefined,
  dataType: IngestedCellDataType
): FormulaVerificationStatus {
  if (
    dataType === "FormulaError" ||
    (cachedValue && cachedValue.startsWith("#")) ||
    (recalculatedValue && recalculatedValue.startsWith("#"))
  ) {
    return "FormulaError";
  }
  const upper = formula.toUpperCase();
  if (upper.includes("[") && upper.includes("]")) {
    return "ExternalWorkbookReference";
  }
  if (/(NOW|TODAY|RAND|RANDBETWEEN|OFFSET|INDIRECT)\s*\(/.test(upper)) {
    return "VolatileFormula";
  }
  if (cachedValue !== undefined && recalculatedValue !== undefined) {
    const cNum = Number(cachedValue.replace(/,/g, ""));
    const rNum = Number(recalculatedValue.replace(/,/g, ""));
    if (
      cachedValue === recalculatedValue ||
      (Number.isFinite(cNum) && Number.isFinite(rNum) && Math.abs(cNum - rNum) < 1e-4)
    ) {
      return "VerifiedCachedAndRecalculated";
    }
    return "CachedResultOnly";
  }
  if (cachedValue === undefined && recalculatedValue !== undefined) {
    return "RecalculatedLocally";
  }
  if (cachedValue !== undefined) {
    return "CachedResultOnly";
  }
  return "UnverifiedFormula";
}

function detectTotalRowKind(rowVals: readonly string[]): SeparatedRowKind | null {
  const firstText = rowVals
    .find((s) => s.trim().length > 0)
    ?.trim()
    .toLowerCase();
  if (!firstText) return null;
  if (
    firstText === "grand total" ||
    firstText === "total" ||
    firstText.startsWith("grand total") ||
    firstText.startsWith("total ") ||
    firstText === "الإجمالي العام" ||
    firstText === "الإجمالي" ||
    firstText === "الاجمالي" ||
    firstText === "المجموع الكلي" ||
    firstText === "المجموع"
  ) {
    return "GrandTotal";
  }
  if (
    firstText.startsWith("subtotal") ||
    firstText.startsWith("sub-total") ||
    firstText.includes("subtotal") ||
    firstText.startsWith("إجمالي فرعي") ||
    firstText.startsWith("اجمالي فرعي") ||
    firstText.startsWith("مجموع فرعي")
  ) {
    return "Subtotal";
  }
  return null;
}

function isFootnoteText(text: string): boolean {
  const lower = text.trim().toLowerCase();
  return (
    lower.startsWith("*") ||
    lower.startsWith("note:") ||
    lower.startsWith("notes:") ||
    lower.startsWith("source:") ||
    lower.startsWith("ملاحظة:") ||
    lower.startsWith("المصدر:")
  );
}

/**
 * Segments a worksheet grid into one or more structured table regions, separating
 * banner titles, multi-row headers, repeated headers, subtotals, grand totals, and footnotes.
 */
export function segmentWorksheetTables(
  sheetName: string,
  grid: readonly (readonly string[])[],
  mergedRanges: readonly string[] = []
): DetectedTableRegion[] {
  if (grid.length === 0) return [];

  const maxColsGlobal = Math.max(0, ...grid.map((r) => r.length));
  const normalizedGrid: string[][] = grid.map((r) =>
    Array.from({ length: maxColsGlobal }, (_, idx) => (r[idx] ?? "").trim())
  );

  // Propagate horizontal merged header values across their spans
  for (const mergeStr of mergedRanges) {
    const range = parseA1Range(mergeStr);
    if (!range) continue;
    const originVal = normalizedGrid[range.startRow]?.[range.startCol] ?? "";
    if (!originVal) continue;
    for (let c = range.startCol + 1; c <= range.endCol; c += 1) {
      const rowRef = normalizedGrid[range.startRow];
      if (rowRef && !rowRef[c]) {
        rowRef[c] = originVal;
      }
    }
  }

  // 1. Split into vertical row bands separated by blank rows
  const rowBands: Array<{ start: number; end: number }> = [];
  let bandStart: number | null = null;
  for (let r = 0; r < normalizedGrid.length; r += 1) {
    const hasData = (normalizedGrid[r] ?? []).some((cell) => cell.length > 0);
    if (bandStart === null && hasData) {
      bandStart = r;
    } else if (bandStart !== null && !hasData) {
      rowBands.push({ start: bandStart, end: r - 1 });
      bandStart = null;
    }
  }
  if (bandStart !== null) {
    rowBands.push({ start: bandStart, end: normalizedGrid.length - 1 });
  }

  const pendingBannerTitles: Array<{ row: number; text: string }> = [];
  const tables: DetectedTableRegion[] = [];

  for (const band of rowBands) {
    const bandRows = normalizedGrid.slice(band.start, band.end + 1);
    const maxNonEmptyInBand = Math.max(
      0,
      ...bandRows.map((r) => r.filter((c) => c.length > 0).length)
    );

    if (bandRows.length <= 2 && maxNonEmptyInBand <= 1) {
      bandRows.forEach((r, offset) => {
        const text = r.find((c) => c.length > 0);
        if (text) {
          pendingBannerTitles.push({ row: band.start + offset, text });
        }
      });
      continue;
    }

    // 2. Split horizontally by empty columns inside this row band
    const colBands: Array<{ start: number; end: number }> = [];
    let cStart: number | null = null;
    for (let c = 0; c < maxColsGlobal; c += 1) {
      const colHasData = bandRows.some((r) => (r[c] ?? "").length > 0);
      if (cStart === null && colHasData) {
        cStart = c;
      } else if (cStart !== null && !colHasData) {
        colBands.push({ start: cStart, end: c - 1 });
        cStart = null;
      }
    }
    if (cStart !== null) {
      colBands.push({ start: cStart, end: maxColsGlobal - 1 });
    }

    for (const colBand of colBands) {
      const region = buildTableRegionFromBlock(
        sheetName,
        normalizedGrid,
        band.start,
        band.end,
        colBand.start,
        colBand.end,
        tables.length + 1,
        pendingBannerTitles
      );
      if (region) {
        tables.push(region);
      }
    }
  }

  return tables;
}

function buildTableRegionFromBlock(
  sheetName: string,
  grid: readonly (readonly string[])[],
  rStart: number,
  rEnd: number,
  cStart: number,
  cEnd: number,
  tableSeq: number,
  pendingBannerTitles: Array<{ row: number; text: string }>
): DetectedTableRegion | null {
  const width = cEnd - cStart + 1;
  if (width <= 0) return null;

  const separatedRows: SeparatedSheetRow[] = [];
  while (pendingBannerTitles.length > 0) {
    const item = pendingBannerTitles.shift();
    if (item) {
      separatedRows.push({
        sheetRow: item.row,
        kind: separatedRows.length === 0 ? "Title" : "Subtitle",
        values: [item.text]
      });
    }
  }

  const sliceRow = (rowIdx: number): string[] => {
    const row = grid[rowIdx] ?? [];
    const out: string[] = [];
    for (let c = cStart; c <= cEnd; c += 1) {
      out.push((row[c] ?? "").trim());
    }
    return out;
  };

  let curR = rStart;
  while (curR < rEnd) {
    const vals = sliceRow(curR).filter((v) => v.length > 0);
    const distinct = new Set(vals);
    if (distinct.size === 1 && width > 1) {
      const hasTitle = separatedRows.some((s) => s.kind === "Title");
      separatedRows.push({
        sheetRow: curR,
        kind: hasTitle ? "Subtitle" : "Title",
        values: [vals[0] ?? ""]
      });
      curR += 1;
    } else {
      break;
    }
  }

  if (curR > rEnd) return null;

  const firstRow = sliceRow(curR);
  const headerRows: number[] = [curR];
  let headers: string[] = [...firstRow];

  if (curR + 1 < rEnd) {
    const secondRow = sliceRow(curR + 1);
    const thirdRow = sliceRow(curR + 2);
    const secondAllText = secondRow
      .filter((s) => s.length > 0)
      .every((s) => !Number.isFinite(Number(s.replace(/,/g, ""))));
    const thirdHasNumbers = thirdRow
      .filter((s) => s.length > 0)
      .some((s) => Number.isFinite(Number(s.replace(/,/g, ""))));
    const nonEmptyFirst = firstRow.filter((s) => s.length > 0);
    const firstHasDuplicates = nonEmptyFirst.length > new Set(nonEmptyFirst).size;

    if (secondAllText && thirdHasNumbers && firstHasDuplicates) {
      headerRows.push(curR + 1);
      headers = firstRow.map((top, idx) => {
        const sub = secondRow[idx] ?? "";
        if (top && sub && top !== sub) return `${top} — ${sub}`;
        if (top) return top;
        if (sub) return sub;
        return `Column_${idx + 1}`;
      });
      curR += 2;
    } else {
      curR += 1;
    }
  } else {
    curR += 1;
  }

  const seenHeaders = new Map<string, number>();
  headers = headers.map((h, idx) => {
    const base = h.trim().length > 0 ? h.trim() : `Column_${idx + 1}`;
    const count = (seenHeaders.get(base) ?? 0) + 1;
    seenHeaders.set(base, count);
    return count > 1 ? `${base}_${count}` : base;
  });

  const dataRows: string[][] = [];
  const sourceRowIndices: number[] = [];

  for (let r = curR; r <= rEnd; r += 1) {
    const rowVals = sliceRow(r);
    const nonEmpty = rowVals.filter((v) => v.length > 0);
    if (nonEmpty.length === 0) continue;

    const headerMatchCount = rowVals.filter(
      (v, idx) => v.length > 0 && v.toLowerCase() === (headers[idx] ?? "").toLowerCase()
    ).length;
    if (headerMatchCount >= Math.max(2, Math.floor(headers.length / 2))) {
      separatedRows.push({
        sheetRow: r,
        kind: "RepeatedHeader",
        values: rowVals
      });
      continue;
    }

    const totalKind = detectTotalRowKind(rowVals);
    if (totalKind) {
      separatedRows.push({
        sheetRow: r,
        kind: totalKind,
        values: rowVals
      });
      continue;
    }

    if (r === rEnd && nonEmpty.length === 1 && width > 1 && isFootnoteText(nonEmpty[0] ?? "")) {
      separatedRows.push({
        sheetRow: r,
        kind: "Footnote",
        values: [nonEmpty[0] ?? ""]
      });
      continue;
    }

    dataRows.push(rowVals);
    sourceRowIndices.push(r);
  }

  const titleRow = separatedRows.find((s) => s.kind === "Title");
  const title =
    titleRow?.values[0] ?? (tableSeq === 1 ? sheetName : `${sheetName} (Table ${tableSeq})`);
  const bounds: SheetRange = {
    startRow: headerRows[0] ?? rStart,
    startCol: cStart,
    endRow: rEnd,
    endCol: cEnd
  };

  return {
    tableId: `tbl_${tableSeq}`,
    title,
    sheetName,
    bounds,
    a1Range: formatA1Range(bounds),
    headerRows,
    headers,
    dataRows,
    sourceRowIndices,
    separatedRows,
    confidencePct: dataRows.length > 0 && headers.length >= 2 ? 95 : dataRows.length > 0 ? 80 : 60
  };
}

export function classifyWorksheetPurpose(
  sheetName: string,
  visibility: SheetVisibility,
  tables: readonly DetectedTableRegion[],
  formulaCount: number,
  hasPivotTable: boolean,
  analyticalRows: number
): { classification: SheetClassification; confidencePct: number; reason: string } {
  const lower = sheetName.toLowerCase();
  if (analyticalRows === 0 && tables.length === 0) {
    return {
      classification: "Unknown",
      confidencePct: 90,
      reason: "Worksheet has no structured tabular records"
    };
  }
  if (/readme|instruction|guide|ارشادات|إرشادات|تعليمات/.test(lower)) {
    return {
      classification: "Instructions",
      confidencePct: 94,
      reason: "Sheet name and structure match instructions/documentation"
    };
  }
  if (/config|setting|param|إعدادات|ثوابت/.test(lower)) {
    return {
      classification: "Configuration",
      confidencePct: 92,
      reason: "Sheet name matches configuration/parameters"
    };
  }
  if (hasPivotTable || /pivot|محوري/.test(lower)) {
    return {
      classification: "PivotOutput",
      confidencePct: 93,
      reason: "Contains pivot structure or pivot nomenclature"
    };
  }
  if (/lookup|mapping|code|أكواد|ترميز/.test(lower)) {
    return {
      classification: "LookupTable",
      confidencePct: 90,
      reason: "Compact code/description mapping sheet"
    };
  }
  if (/ref|master|مرجع/.test(lower)) {
    return {
      classification: "ReferenceData",
      confidencePct: 88,
      reason: "Reference/master entity table"
    };
  }
  if (/summary|dashboard|kpi|ملخص|مؤشرات/.test(lower)) {
    return {
      classification: "Summary",
      confidencePct: 92,
      reason: "Aggregated summary/KPI sheet"
    };
  }
  if (/report|تقرير/.test(lower)) {
    return {
      classification: "Report",
      confidencePct: 89,
      reason: "Formatted report worksheet"
    };
  }
  if (visibility !== "Visible" && formulaCount > 0) {
    return {
      classification: "SupportingCalculations",
      confidencePct: 87,
      reason: "Hidden worksheet containing intermediate formulas"
    };
  }
  const colCount = tables[0]?.headers.length ?? 0;
  if (colCount <= 3 && analyticalRows <= 50 && formulaCount === 0) {
    return {
      classification: "LookupTable",
      confidencePct: 82,
      reason: "Small 2-3 column reference dictionary"
    };
  }
  return {
    classification: "MainDataset",
    confidencePct: 91,
    reason: "Primary analytical dataset"
  };
}

/**
 * Ingests a multi-sheet workbook structure into a canonical `WorkbookIngestionReport`.
 */
export function ingestWorkbookStructure(
  sheets: readonly RawSheetInput[],
  options: WorkbookIngestionOptions = {}
): WorkbookIngestionReport {
  if (options.signal?.aborted) {
    throw new Error("SF-IMP-002: WorkbookIngestionCancelled");
  }

  const detectedFormat = options.formatHint ?? detectWorkbookFormatFromFilename(options.filename);
  const progressLog: IngestionProgressUpdate[] = [];
  const emitProgress = (update: IngestionProgressUpdate): void => {
    progressLog.push(update);
    options.onProgress?.(update);
  };

  emitProgress({
    phase: "Preflight",
    sheetsProcessed: 0,
    totalSheets: sheets.length,
    percentComplete: 10
  });

  const warnings: string[] = [];
  const embeddedObjects: EmbeddedObjectRecord[] = [...(options.embeddedObjects ?? [])];
  const macrosNeutralized = Boolean(options.hasVbaMacros || detectedFormat === "Xlsm");

  if (macrosNeutralized) {
    warnings.push(
      "Workbook contains a VBA macro project (vbaProject.bin); macros were neutralized and never executed."
    );
    if (!embeddedObjects.some((o) => o.kind === "VbaMacroProject")) {
      embeddedObjects.push({
        kind: "VbaMacroProject",
        location: "xl/vbaProject.bin",
        handling: "NeutralizedBySecurityPolicy",
        summary: "VBA macro binary detected and neutralized by local security policy"
      });
    }
  }

  emitProgress({
    phase: "SheetDiscovery",
    sheetsProcessed: 0,
    totalSheets: sheets.length,
    percentComplete: 25
  });

  const worksheets: WorksheetIntelligence[] = [];
  let totalTablesExtracted = 0;
  let totalAnalyticalRows = 0;
  let totalFormulasDetected = 0;
  let totalFormulasVerified = 0;
  const maxRows = options.maxRows ?? 1_000_000;

  sheets.forEach((sheet, idx) => {
    if (options.signal?.aborted) {
      throw new Error("SF-IMP-002: WorkbookIngestionCancelled");
    }

    const visibility = sheet.visibility ?? "Visible";
    const mergedRanges = sheet.mergedRanges ?? [];
    const maxCols = Math.max(0, ...sheet.grid.map((r) => r.length));
    const workingGrid: string[][] = sheet.grid.map((r) =>
      Array.from({ length: maxCols }, (_, c) => (r[c] ?? "").trim())
    );

    const numericLookup = new Map<string, number>();
    workingGrid.forEach((row, rIdx) => {
      row.forEach((val, cIdx) => {
        const cleaned = val.replace(/,/g, "");
        if (cleaned.length > 0 && Number.isFinite(Number(cleaned))) {
          numericLookup.set(`${rIdx},${cIdx}`, Number(cleaned));
        }
      });
    });

    const mergeOriginByCoord = new Map<string, string>();
    for (const mStr of mergedRanges) {
      const m = parseA1Range(mStr);
      if (!m) continue;
      const originAddr = coordsToA1(m.startRow, m.startCol);
      for (let r = m.startRow; r <= m.endRow; r += 1) {
        for (let c = m.startCol; c <= m.endCol; c += 1) {
          if (r !== m.startRow || c !== m.startCol) {
            mergeOriginByCoord.set(`${r},${c}`, originAddr);
          }
        }
      }
    }

    const cellMetadata: IngestedCellMetadata[] = [];
    let formulaCount = 0;
    let verifiedFormulaCount = 0;

    for (let r = 0; r < workingGrid.length; r += 1) {
      for (let c = 0; c < maxCols; c += 1) {
        const addr = coordsToA1(r, c);
        const rawVal = workingGrid[r]?.[c] ?? "";
        const formula = sheet.formulas?.[addr];
        const numberFormat = sheet.numberFormats?.[addr];
        const comment = sheet.comments?.[addr];
        const hyperlink = sheet.hyperlinks?.[addr];
        const mergedFrom = mergeOriginByCoord.get(`${r},${c}`);

        if (!rawVal && !formula && !comment && !hyperlink && !mergedFrom) {
          continue;
        }

        if (comment) {
          embeddedObjects.push({
            kind: "CommentOrNote",
            location: `${sheet.name}!${addr}`,
            handling: "Extracted",
            summary: `Comment at ${sheet.name}!${addr}`
          });
        }
        if (hyperlink) {
          embeddedObjects.push({
            kind: "Hyperlink",
            location: `${sheet.name}!${addr}`,
            handling: "Extracted",
            summary: `Hyperlink at ${sheet.name}!${addr} -> ${hyperlink}`
          });
        }

        const dataType = classifyCellType(rawVal, numberFormat);
        let cachedValue: string | undefined;
        let recalculatedValue: string | undefined;
        let formulaStatus: FormulaVerificationStatus = "LiteralValue";

        if (formula && formula.trim().length > 0) {
          formulaCount += 1;
          cachedValue = rawVal.length > 0 ? rawVal : undefined;
          recalculatedValue = evaluateSimpleFormula(formula, numericLookup);
          formulaStatus = classifyFormulaVerification(
            formula,
            cachedValue,
            recalculatedValue,
            dataType
          );
          if (
            formulaStatus === "VerifiedCachedAndRecalculated" ||
            formulaStatus === "RecalculatedLocally"
          ) {
            verifiedFormulaCount += 1;
          }
          if (!rawVal && recalculatedValue !== undefined && workingGrid[r]) {
            workingGrid[r]![c] = recalculatedValue;
          }
        }

        cellMetadata.push({
          sheetName: sheet.name,
          address: addr,
          row: r,
          col: c,
          rawValue: workingGrid[r]?.[c] ?? rawVal,
          formula,
          cachedValue,
          recalculatedValue,
          formulaStatus,
          dataType,
          numberFormat,
          mergedFrom,
          hyperlink,
          comment
        });
      }
    }

    const tables = segmentWorksheetTables(sheet.name, workingGrid, mergedRanges);
    const sheetAnalyticalRows = tables.reduce((acc, t) => acc + t.dataRows.length, 0);
    totalTablesExtracted += tables.length;
    totalAnalyticalRows += sheetAnalyticalRows;
    totalFormulasDetected += formulaCount;
    totalFormulasVerified += verifiedFormulaCount;

    if (totalAnalyticalRows > maxRows) {
      throw new Error("SF-IMP-003: PlatformRowLimitExceeded");
    }

    const { classification, confidencePct, reason } = classifyWorksheetPurpose(
      sheet.name,
      visibility,
      tables,
      formulaCount,
      Boolean(sheet.hasPivotTable),
      sheetAnalyticalRows
    );

    const usedRange: SheetRange | undefined =
      workingGrid.length > 0 && maxCols > 0
        ? {
            startRow: 0,
            startCol: 0,
            endRow: workingGrid.length - 1,
            endCol: maxCols - 1
          }
        : undefined;

    worksheets.push({
      name: sheet.name,
      index: idx,
      visibility,
      classification,
      classificationConfidencePct: confidencePct,
      classificationReason: reason,
      usedRange,
      rawRowCount: workingGrid.length,
      rawColCount: maxCols,
      mergedRanges,
      hasFrozenPanes: Boolean(sheet.hasFrozenPanes),
      autoFilterRange: sheet.autoFilterRange,
      printArea: sheet.printArea,
      isProtected: Boolean(sheet.isProtected),
      tables,
      cellMetadata,
      formulaCount,
      verifiedFormulaCount
    });

    const pct = Math.min(95, 25 + Math.round(((idx + 1) * 65) / Math.max(1, sheets.length)));
    emitProgress({
      phase: "TableSegmentation",
      sheetsProcessed: idx + 1,
      totalSheets: sheets.length,
      percentComplete: pct
    });
  });

  const visibleSheets = worksheets.filter((w) => w.visibility === "Visible").length;
  const hiddenSheets = worksheets.length - visibleSheets;

  emitProgress({
    phase: "Completed",
    sheetsProcessed: worksheets.length,
    totalSheets: worksheets.length,
    percentComplete: 100
  });

  const supportLevel: FormatSupportLevel =
    detectedFormat === "Xlsx" || detectedFormat === "Csv"
      ? "FullSupport"
      : detectedFormat === "Xlsm"
        ? "SupportedWithMacroNeutralization"
        : detectedFormat === "Unknown"
          ? "Unsupported"
          : "SupportedTabularAndFormulas";

  return {
    detectedFormat,
    supportLevel,
    dateSystem: options.dateSystem ?? "Standard1900",
    macrosNeutralized,
    totalSheets: worksheets.length,
    visibleSheets,
    hiddenSheets,
    totalTablesExtracted,
    totalAnalyticalRows,
    totalFormulasDetected,
    totalFormulasVerified,
    definedNames: options.definedNames ?? [],
    embeddedObjects,
    worksheets,
    warnings,
    progressLog
  };
}
