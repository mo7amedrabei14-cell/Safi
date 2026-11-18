//! Universal Excel & Tabular Workbook Intelligence Engine (`Phase 1`).
//!
//! Supports `XLSX`, `XLS`, `XLSM`, `XLSB`, `CSV`, and `ODS` with:
//! - Format compatibility matrix and magic-byte + OOXML content inspection
//! - Visible, hidden, and very-hidden worksheet discovery & purpose classification
//! - Multi-table & irregular structure segmentation (title rows, merged/multi-row headers,
//!   repeated headers, subtotal/grand-total row separation, footnotes)
//! - Complete cell-level metadata (raw value, formula expression, cached value, recalculated
//!   verification, number format, 1900/1904 date system, comments, hyperlinks, leading-zero
//!   preservation)
//! - Embedded object inventory (charts, images, comments, hyperlinks, drawings, pivot tables)
//! - Cancellation and progress reporting with strict resource controls (`SF-IMP-003`, `SF-IMP-005`)

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, Ordering};

use calamine::{Data, DataType, Reader, Sheets, open_workbook_auto_from_rs};
use quick_xml::Reader as XmlReader;
use quick_xml::events::Event;
use sf_core::Decimal;
use sf_core::error::{ErrorCode, ErrorSeverity, SafiError};
use sf_core::limits::PlatformLimits;
use sf_core::value::is_recognized_empty_token;
use zip::ZipArchive;

use crate::delimiter::detect_delimiter;
use crate::encoding::detect_and_decode;
use crate::reader_excel::{MAX_DECOMPRESSION_RATIO, MAX_UNCOMPRESSED_BYTES};

/// Supported workbook and tabular file formats in the Universal Ingestion Matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WorkbookFormat {
    /// Office Open XML Workbook (`.xlsx`).
    Xlsx,
    /// Legacy Binary Excel 97-2003 Workbook (`.xls` BIFF8).
    Xls,
    /// Office Open XML Macro-Enabled Workbook (`.xlsm`) — macros neutralized on ingest.
    Xlsm,
    /// Office Open XML Binary Workbook (`.xlsb`).
    Xlsb,
    /// Comma/Tab/Semicolon/Pipe Delimited Text (`.csv`, `.tsv`).
    Csv,
    /// OpenDocument Spreadsheet (`.ods`).
    Ods,
    /// Unrecognized or unsupported container format.
    Unknown,
}

/// Support level for a given workbook format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatSupportLevel {
    /// Full structural, metadata, and cell-level ingestion.
    FullSupport,
    /// Full data/formula ingestion with VBA macros safely neutralized by security policy.
    SupportedWithMacroNeutralization,
    /// Data, sheets, and formulas ingested via multi-format reader; XML-specific drawings omitted.
    SupportedTabularAndFormulas,
    /// Unsupported or corrupted format.
    Unsupported,
}

/// Entry in the static and runtime Format Compatibility Matrix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatCompatibilityEntry {
    /// Format identifier.
    pub format: WorkbookFormat,
    /// Standard file extensions.
    pub extensions: Vec<String>,
    /// Support classification.
    pub support_level: FormatSupportLevel,
    /// Supported capabilities description.
    pub supported_features: Vec<String>,
    /// Known format limitations or security policies.
    pub limitations: Vec<String>,
}

/// Returns the canonical Format Compatibility Matrix for `XLSX`, `XLS`, `XLSM`, `XLSB`, `CSV`, and `ODS`.
#[must_use]
pub fn format_compatibility_matrix() -> Vec<FormatCompatibilityEntry> {
    vec![
        FormatCompatibilityEntry {
            format: WorkbookFormat::Xlsx,
            extensions: vec!["xlsx".to_string()],
            support_level: FormatSupportLevel::FullSupport,
            supported_features: vec![
                "Visible/Hidden/VeryHidden sheets".to_string(),
                "Multi-table & irregular header detection".to_string(),
                "Cell values, formulas, cached values & safe recalculation".to_string(),
                "Number formats, 1900/1904 date systems, merges, comments, hyperlinks".to_string(),
                "Defined names, Excel tables, embedded object inventory".to_string(),
            ],
            limitations: vec![
                "Password-encrypted workbooks require decryption prior to import".to_string(),
            ],
        },
        FormatCompatibilityEntry {
            format: WorkbookFormat::Xlsm,
            extensions: vec!["xlsm".to_string()],
            support_level: FormatSupportLevel::SupportedWithMacroNeutralization,
            supported_features: vec![
                "All XLSX worksheet, table, formula, and metadata features".to_string(),
                "Automatic VBA project detection and macro neutralization audit".to_string(),
            ],
            limitations: vec![
                "VBA macros (vbaProject.bin) are never executed by security policy".to_string(),
            ],
        },
        FormatCompatibilityEntry {
            format: WorkbookFormat::Xls,
            extensions: vec!["xls".to_string()],
            support_level: FormatSupportLevel::SupportedTabularAndFormulas,
            supported_features: vec![
                "Multi-sheet BIFF8 ingestion, cell values, formulas, defined names".to_string(),
                "Multi-table segmentation and subtotal separation".to_string(),
            ],
            limitations: vec![
                "Legacy BIFF5/encrypted XLS streams and embedded OLE drawings are reported as unsupported".to_string(),
            ],
        },
        FormatCompatibilityEntry {
            format: WorkbookFormat::Xlsb,
            extensions: vec!["xlsb".to_string()],
            support_level: FormatSupportLevel::SupportedTabularAndFormulas,
            supported_features: vec![
                "Binary OOXML sheets, cell values, formulas, defined names, multi-table detection".to_string(),
            ],
            limitations: vec![
                "VBA macros neutralized; binary drawing styles summarized".to_string(),
            ],
        },
        FormatCompatibilityEntry {
            format: WorkbookFormat::Csv,
            extensions: vec!["csv".to_string(), "tsv".to_string(), "txt".to_string()],
            support_level: FormatSupportLevel::FullSupport,
            supported_features: vec![
                "UTF-8/UTF-16/Windows-1256 Arabic encoding detection".to_string(),
                "Auto-delimiter detection, leading-zero preservation, multi-region segmentation".to_string(),
            ],
            limitations: vec![
                "Single-sheet flat text format (no native formulas or multiple worksheets)".to_string(),
            ],
        },
        FormatCompatibilityEntry {
            format: WorkbookFormat::Ods,
            extensions: vec!["ods".to_string()],
            support_level: FormatSupportLevel::SupportedTabularAndFormulas,
            supported_features: vec![
                "OpenDocument multi-sheet values, formulas, defined names, multi-table detection".to_string(),
            ],
            limitations: vec![
                "ODF chart/drawing objects are inventoried at summary level".to_string(),
            ],
        },
    ]
}

/// Worksheet visibility state inside the workbook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetVisibility {
    /// Visible worksheet tab.
    Visible,
    /// Hidden worksheet (can be unhidden via Excel UI).
    Hidden,
    /// VeryHidden worksheet (hidden via VBA / workbook properties).
    VeryHidden,
}

/// Semantic purpose classification of a worksheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetClassification {
    /// Primary transactional or operational dataset.
    MainDataset,
    /// Reference or master data table.
    ReferenceData,
    /// Small code/name lookup dictionary.
    LookupTable,
    /// Aggregated KPI / summary sheet.
    Summary,
    /// Formatted narrative or printable report sheet.
    Report,
    /// Parameters, settings, or constants sheet.
    Configuration,
    /// User guide, README, or notes sheet.
    Instructions,
    /// Intermediate formula helper sheet.
    SupportingCalculations,
    /// Pivot-table output layout.
    PivotOutput,
    /// Empty or unclassified sheet.
    Unknown,
}

/// Workbook date serial base system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkbookDateSystem {
    /// Standard 1900 date system (Windows/Excel default).
    Standard1900,
    /// 1904 date system (legacy Mac Excel).
    Macintosh1904,
}

/// Detected semantic/storage data type for an individual cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestedCellDataType {
    /// Empty or recognized sentinel (`NULL`, `N/A`, `-`, etc.).
    Empty,
    /// General text string.
    Text,
    /// Identifier / phone / account string with leading zeros preserved (`00123`).
    ZeroPreservedText,
    /// Exact integer value.
    Integer,
    /// Decimal / floating-point numeric value.
    Decimal,
    /// Currency-formatted number.
    Currency,
    /// Percentage-formatted number.
    Percentage,
    /// Logical boolean (`TRUE` / `FALSE`).
    Boolean,
    /// Calendar date.
    Date,
    /// Date with time component.
    DateTime,
    /// Time of day or duration.
    Time,
    /// Formula evaluation error (`#REF!`, `#DIV/0!`, `#VALUE!`, `#NAME?`, `#N/A`, `#NUM!`).
    FormulaError,
}

/// Formula verification classification for a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormulaVerificationStatus {
    /// Cell contains a literal constant (not a formula).
    LiteralValue,
    /// Formula cached result matches SafiData's independent recalculation.
    VerifiedCachedAndRecalculated,
    /// Formula has a cached result from Excel; function is not recalculated locally.
    CachedResultOnly,
    /// Formula had no cached value in file and was recalculated locally by SafiData.
    RecalculatedLocally,
    /// Formula uses volatile functions (`NOW`, `TODAY`, `RAND`, `OFFSET`, `INDIRECT`).
    VolatileFormula,
    /// Formula references an external workbook (`[Other.xlsx]Sheet1!A1`).
    ExternalWorkbookReference,
    /// Formula evaluates to an error token (`#REF!`, `#DIV/0!`, etc.).
    FormulaError,
    /// Formula syntax or function is unsupported for local recalculation and has no cached value.
    UnverifiedFormula,
}

/// Complete cell-level metadata captured during workbook ingestion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestedCellMetadata {
    /// Worksheet name.
    pub sheet_name: String,
    /// A1-style cell address (e.g., `"B4"`).
    pub address: String,
    /// Zero-based row index on the worksheet.
    pub row: u32,
    /// Zero-based column index on the worksheet.
    pub col: u32,
    /// Raw text representation of the cell value.
    pub raw_value: String,
    /// Formula expression without leading `=` if the cell contains a formula.
    pub formula: Option<String>,
    /// Cached value stored in the workbook for a formula cell.
    pub cached_value: Option<String>,
    /// Independently recalculated value when supported.
    pub recalculated_value: Option<String>,
    /// Formula verification status.
    pub formula_status: FormulaVerificationStatus,
    /// Detected cell data type.
    pub data_type: IngestedCellDataType,
    /// Number format string or ID if present.
    pub number_format: Option<String>,
    /// Origin A1 address if this cell is inside a merged range.
    pub merged_from: Option<String>,
    /// Hyperlink target URL or internal reference if present.
    pub hyperlink: Option<String>,
    /// Cell comment or note text if present.
    pub comment: Option<String>,
}

/// Rectangular coordinate range on a worksheet (0-based inclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SheetRange {
    /// Zero-based start row.
    pub start_row: u32,
    /// Zero-based start column.
    pub start_col: u32,
    /// Zero-based inclusive end row.
    pub end_row: u32,
    /// Zero-based inclusive end column.
    pub end_col: u32,
}

impl SheetRange {
    /// Formats this 0-based range as an Excel A1 reference (`A1:D25`).
    #[must_use]
    pub fn to_a1_range(&self) -> String {
        format!(
            "{}:{}",
            coords_to_a1(self.start_row, self.start_col),
            coords_to_a1(self.end_row, self.end_col)
        )
    }
}

/// Classification of non-data rows separated from an extracted table region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeparatedSheetRow {
    /// Zero-based worksheet row index.
    pub sheet_row: u32,
    /// Role of this row (`Title`, `Subtitle`, `RepeatedHeader`, `Subtotal`, `GrandTotal`, `Footnote`).
    pub kind: SeparatedRowKind,
    /// Non-empty text cells on this row.
    pub values: Vec<String>,
}

/// Role of a row excluded from primary analytical records to prevent double-counting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeparatedRowKind {
    /// Banner title above a table.
    Title,
    /// Subtitle or metadata note above a table.
    Subtitle,
    /// Repeated header row inside a multi-page sheet.
    RepeatedHeader,
    /// Group subtotal row.
    Subtotal,
    /// Sheet or table grand total row.
    GrandTotal,
    /// Footnote or source note below a table.
    Footnote,
}

/// A distinct structured table region detected on a worksheet (supports multiple tables per sheet).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedTableRegion {
    /// Stable region identifier (`tbl_1`, `tbl_2`, ...).
    pub table_id: String,
    /// Human-readable table title (from Excel table name, banner title, or sheet name).
    pub title: String,
    /// Worksheet name containing this table.
    pub sheet_name: String,
    /// Bounding range of the table (including header rows).
    pub bounds: SheetRange,
    /// Zero-based worksheet row indices forming the header (supports multi-row headers).
    pub header_rows: Vec<u32>,
    /// Resolved canonical column headers (flattening merged/multi-row headers cleanly).
    pub headers: Vec<String>,
    /// Analytical data rows (excluding repeated headers, subtotals, grand totals, and footnotes).
    pub data_rows: Vec<Vec<String>>,
    /// Zero-based worksheet row index for each entry in `data_rows`.
    pub source_row_indices: Vec<u32>,
    /// Separated title, subtotal, grand-total, repeated-header, and footnote rows.
    pub separated_rows: Vec<SeparatedSheetRow>,
    /// Calibrated confidence (`0..=100`) in the table boundary and header resolution.
    pub confidence_pct: u8,
}

/// Embedded non-cell object discovered inside the workbook package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedObjectRecord {
    /// Kind of embedded content.
    pub kind: EmbeddedObjectKind,
    /// Package path or worksheet reference.
    pub location: String,
    /// Handling status (`Extracted`, `Summarized`, `Neutralized`, `Unsupported`).
    pub handling: EmbeddedObjectHandling,
    /// Human-readable description.
    pub summary: String,
}

/// Category of embedded workbook object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddedObjectKind {
    /// Embedded chart (`xl/charts/chart*.xml`).
    Chart,
    /// Embedded image/media (`xl/media/*`).
    Image,
    /// Worksheet comment or threaded note (`xl/comments*.xml`).
    CommentOrNote,
    /// Cell hyperlink.
    Hyperlink,
    /// Vector shape / SmartArt / drawing (`xl/drawings/*`).
    DrawingOrShape,
    /// Pivot table definition (`xl/pivotTables/*`).
    PivotTable,
    /// Formal Excel table (`xl/tables/table*.xml`).
    ExcelTable,
    /// VBA macro project (`xl/vbaProject.bin`).
    VbaMacroProject,
}

/// How the ingestion engine handled an embedded object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddedObjectHandling {
    /// Fully extracted into cell/table metadata (e.g., comments, hyperlinks, Excel tables).
    Extracted,
    /// Inventoried and summarized in the workbook report (e.g., charts, images, pivot tables).
    Summarized,
    /// Safely stripped/neutralized by security policy (e.g., VBA macros).
    NeutralizedBySecurityPolicy,
    /// Unsupported binary object reported transparently.
    Unsupported,
}

/// Named range or defined formula in the workbook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinedNameRecord {
    /// Defined name identifier.
    pub name: String,
    /// Formula or range reference (`Sheet1!$A$1:$D$50`).
    pub refers_to: String,
    /// Whether this defined name represents a print area or auto-filter range.
    pub is_built_in: bool,
}

/// Complete metadata and extracted regions for a single worksheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorksheetIntelligence {
    /// Worksheet name.
    pub name: String,
    /// Zero-based worksheet index in workbook tab order.
    pub index: usize,
    /// Visibility state (`Visible`, `Hidden`, `VeryHidden`).
    pub visibility: SheetVisibility,
    /// Semantic purpose classification.
    pub classification: SheetClassification,
    /// Confidence (`0..=100`) in the sheet classification.
    pub classification_confidence_pct: u8,
    /// Explanation of why this sheet received its classification.
    pub classification_reason: String,
    /// Used coordinate range if the sheet is non-empty.
    pub used_range: Option<SheetRange>,
    /// Total raw rows in the used range.
    pub raw_row_count: u32,
    /// Total raw columns in the used range.
    pub raw_col_count: u32,
    /// Merged cell ranges in A1 notation (`A1:C1`).
    pub merged_ranges: Vec<String>,
    /// Whether frozen panes are active on this sheet.
    pub has_frozen_panes: bool,
    /// Auto-filter A1 range if active on this sheet.
    pub auto_filter_range: Option<String>,
    /// Print area A1 range if configured on this sheet.
    pub print_area: Option<String>,
    /// Whether worksheet protection is enabled.
    pub is_protected: bool,
    /// Distinct table regions detected on this sheet (1 or more).
    pub tables: Vec<DetectedTableRegion>,
    /// Rich cell metadata for non-empty and formula cells on this sheet.
    pub cell_metadata: Vec<IngestedCellMetadata>,
    /// Count of formula cells on this sheet.
    pub formula_count: usize,
    /// Count of formula cells verified by independent local recalculation.
    pub verified_formula_count: usize,
}

/// Progress snapshot emitted during workbook ingestion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestionProgressUpdate {
    /// Current phase name (`Preflight`, `SheetDiscovery`, `TableSegmentation`, `FormulaVerification`, `Completed`).
    pub phase: String,
    /// Number of worksheets processed so far.
    pub sheets_processed: usize,
    /// Total worksheets discovered.
    pub total_sheets: usize,
    /// Completion percentage (`0..=100`).
    pub percent_complete: u8,
}

/// Complete universal workbook ingestion report (`Phase 1` canonical output).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkbookIngestionReport {
    /// Detected file format.
    pub detected_format: WorkbookFormat,
    /// Format support level.
    pub support_level: FormatSupportLevel,
    /// Workbook date serial system (`Standard1900` vs `Macintosh1904`).
    pub date_system: WorkbookDateSystem,
    /// Whether a VBA macro project (`vbaProject.bin`) was detected and neutralized.
    pub macros_neutralized: bool,
    /// Total worksheets discovered (including hidden and very-hidden).
    pub total_sheets: usize,
    /// Count of visible worksheets.
    pub visible_sheets: usize,
    /// Count of hidden or very-hidden worksheets.
    pub hidden_sheets: usize,
    /// Total structured table regions extracted across all sheets.
    pub total_tables_extracted: usize,
    /// Total analytical data rows extracted (excluding titles, subtotals, grand totals).
    pub total_analytical_rows: usize,
    /// Total formula cells discovered across all sheets.
    pub total_formulas_detected: usize,
    /// Total formula cells verified locally.
    pub total_formulas_verified: usize,
    /// Named ranges and defined names discovered.
    pub defined_names: Vec<DefinedNameRecord>,
    /// Embedded objects inventory (charts, images, comments, hyperlinks, drawings, pivots, macros).
    pub embedded_objects: Vec<EmbeddedObjectRecord>,
    /// Per-worksheet intelligence and extracted table regions.
    pub worksheets: Vec<WorksheetIntelligence>,
    /// Transparent warnings or unsupported feature disclosures (never silent).
    pub warnings: Vec<String>,
    /// Progress checkpoints recorded during ingestion.
    pub progress_log: Vec<IngestionProgressUpdate>,
}

/// Converts 0-based `(row, col)` coordinates to an Excel A1 cell address.
#[must_use]
pub fn coords_to_a1(row: u32, col: u32) -> String {
    let mut c = col + 1;
    let mut col_str = String::new();
    while c > 0 {
        let rem = ((c - 1) % 26) as u8;
        col_str.insert(0, char::from(b'A' + rem));
        c = (c - 1) / 26;
    }
    format!("{col_str}{}", row + 1)
}

/// Detects the workbook format from magic bytes and an optional file name hint.
#[must_use]
pub fn sniff_workbook_format(bytes: &[u8], filename_hint: Option<&str>) -> WorkbookFormat {
    let ext = filename_hint
        .and_then(|name| name.rsplit_once('.'))
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();

    if bytes.starts_with(b"PK\x03\x04") {
        let mut cursor = Cursor::new(bytes);
        if let Ok(zip) = ZipArchive::new(&mut cursor) {
            let names: Vec<String> = zip.file_names().map(str::to_owned).collect();
            if names.iter().any(|n| n.eq_ignore_ascii_case("xl/workbook.bin")) {
                return WorkbookFormat::Xlsb;
            }
            if names
                .iter()
                .any(|n| n.eq_ignore_ascii_case("xl/vbaProject.bin"))
                || ext == "xlsm"
            {
                return WorkbookFormat::Xlsm;
            }
            if names.iter().any(|n| n.eq_ignore_ascii_case("content.xml"))
                && (ext == "ods" || names.iter().any(|n| n == "mimetype"))
            {
                return WorkbookFormat::Ods;
            }
            if names.iter().any(|n| n.starts_with("xl/")) {
                return WorkbookFormat::Xlsx;
            }
        }
        return match ext.as_str() {
            "xlsm" => WorkbookFormat::Xlsm,
            "xlsb" => WorkbookFormat::Xlsb,
            "ods" => WorkbookFormat::Ods,
            _ => WorkbookFormat::Xlsx,
        };
    }

    if bytes.starts_with(b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1") {
        return WorkbookFormat::Xls;
    }

    match ext.as_str() {
        "csv" | "tsv" | "txt" => WorkbookFormat::Csv,
        "xlsx" => WorkbookFormat::Xlsx,
        "xlsm" => WorkbookFormat::Xlsm,
        "xlsb" => WorkbookFormat::Xlsb,
        "xls" => WorkbookFormat::Xls,
        "ods" => WorkbookFormat::Ods,
        _ => {
            if !bytes.is_empty() && bytes.iter().take(512).all(|&b| b != 0) {
                WorkbookFormat::Csv
            } else {
                WorkbookFormat::Unknown
            }
        }
    }
}

#[derive(Debug, Default)]
struct OoxmlMetadata {
    date_system: Option<WorkbookDateSystem>,
    sheet_visibility: BTreeMap<String, SheetVisibility>,
    sheet_merges: BTreeMap<String, Vec<SheetRange>>,
    sheet_frozen: BTreeSet<String>,
    sheet_autofilter: BTreeMap<String, String>,
    sheet_protected: BTreeSet<String>,
    sheet_hyperlinks: BTreeMap<(String, String), String>,
    sheet_comments: BTreeMap<(String, String), String>,
    number_formats: BTreeMap<u32, String>,
    cell_style_numfmt: Vec<u32>,
    cell_numfmt_by_addr: BTreeMap<(String, String), String>,
    embedded_objects: Vec<EmbeddedObjectRecord>,
    macros_neutralized: bool,
    warnings: Vec<String>,
}

fn inspect_ooxml_package(bytes: &[u8], file_bytes: u64) -> Result<OoxmlMetadata, SafiError> {
    let mut cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(&mut cursor).map_err(|_| {
        SafiError::new(
            ErrorCode::SF_IMP_002,
            ErrorSeverity::Error,
            "CorruptedExcelArchive",
            "import.excel_corrupted",
        )
    })?;

    let budget = if file_bytes == 0 {
        MAX_UNCOMPRESSED_BYTES
    } else {
        MAX_UNCOMPRESSED_BYTES.min(file_bytes.saturating_mul(MAX_DECOMPRESSION_RATIO))
    };
    let mut total_uncompressed: u64 = 0;

    let file_names: Vec<String> = archive.file_names().map(str::to_owned).collect();
    if file_names
        .iter()
        .any(|n| n.eq_ignore_ascii_case("EncryptedPackage") || n.eq_ignore_ascii_case("EncryptionInfo"))
    {
        return Err(SafiError::new(
            ErrorCode::SF_IMP_002,
            ErrorSeverity::Error,
            "EncryptedWorkbookUnsupported",
            "import.excel_corrupted",
        ));
    }

    let mut meta = OoxmlMetadata::default();

    for name in &file_names {
        let lower = name.to_ascii_lowercase();
        if lower == "xl/vbaproject.bin" || lower.ends_with("/vbaproject.bin") {
            meta.macros_neutralized = true;
            meta.embedded_objects.push(EmbeddedObjectRecord {
                kind: EmbeddedObjectKind::VbaMacroProject,
                location: name.clone(),
                handling: EmbeddedObjectHandling::NeutralizedBySecurityPolicy,
                summary: "VBA macro binary detected and neutralized by local security policy".to_string(),
            });
            meta.warnings.push(
                "Workbook contains a VBA macro project (vbaProject.bin); macros were neutralized and never executed."
                    .to_string(),
            );
        } else if lower.starts_with("xl/charts/chart") && lower.ends_with(".xml") {
            meta.embedded_objects.push(EmbeddedObjectRecord {
                kind: EmbeddedObjectKind::Chart,
                location: name.clone(),
                handling: EmbeddedObjectHandling::Summarized,
                summary: format!("Embedded OOXML chart definition ({name})"),
            });
        } else if lower.starts_with("xl/media/") {
            meta.embedded_objects.push(EmbeddedObjectRecord {
                kind: EmbeddedObjectKind::Image,
                location: name.clone(),
                handling: EmbeddedObjectHandling::Summarized,
                summary: format!("Embedded workbook media asset ({name})"),
            });
        } else if lower.starts_with("xl/drawings/drawing") && lower.ends_with(".xml") {
            meta.embedded_objects.push(EmbeddedObjectRecord {
                kind: EmbeddedObjectKind::DrawingOrShape,
                location: name.clone(),
                handling: EmbeddedObjectHandling::Summarized,
                summary: format!("Worksheet drawing layer ({name})"),
            });
        } else if lower.starts_with("xl/pivottables/pivottable") && lower.ends_with(".xml") {
            meta.embedded_objects.push(EmbeddedObjectRecord {
                kind: EmbeddedObjectKind::PivotTable,
                location: name.clone(),
                handling: EmbeddedObjectHandling::Summarized,
                summary: format!("Excel PivotTable definition ({name})"),
            });
        } else if lower.starts_with("xl/tables/table") && lower.ends_with(".xml") {
            meta.embedded_objects.push(EmbeddedObjectRecord {
                kind: EmbeddedObjectKind::ExcelTable,
                location: name.clone(),
                handling: EmbeddedObjectHandling::Extracted,
                summary: format!("Structured Excel Table definition ({name})"),
            });
        }
    }

    // Read helper that enforces the decompression budget (SF-IMP-005).
    let read_part = |archive: &mut ZipArchive<&mut Cursor<&[u8]>>,
                     part_name: &str,
                     total: &mut u64|
     -> Result<Option<Vec<u8>>, SafiError> {
        let Ok(file) = archive.by_name(part_name) else {
            return Ok(None);
        };
        let mut buf = Vec::new();
        let max_to_read = budget.saturating_sub(*total).saturating_add(1);
        file.take(max_to_read).read_to_end(&mut buf).map_err(|_| {
            SafiError::new(
                ErrorCode::SF_IMP_002,
                ErrorSeverity::Error,
                "CorruptedExcelArchive",
                "import.excel_corrupted",
            )
        })?;
        *total = total.saturating_add(buf.len() as u64);
        if *total > budget {
            return Err(SafiError::new(
                ErrorCode::SF_IMP_005,
                ErrorSeverity::Error,
                "ZipBombDecompressionLimitExceeded",
                "import.zip_bomb_detected",
            ));
        }
        Ok(Some(buf))
    };

    // 1. Parse xl/styles.xml for number formats
    if let Some(styles_bytes) = read_part(&mut archive, "xl/styles.xml", &mut total_uncompressed)? {
        parse_styles_xml(&styles_bytes, &mut meta);
    }

    // 2. Parse xl/_rels/workbook.xml.rels and xl/workbook.xml for sheet visibility & date1904
    let mut rel_target_by_id: BTreeMap<String, String> = BTreeMap::new();
    if let Some(rels_bytes) =
        read_part(&mut archive, "xl/_rels/workbook.xml.rels", &mut total_uncompressed)?
    {
        let mut xml = XmlReader::from_reader(rels_bytes.as_slice());
        xml.expand_empty_elements(true);
        let mut buf = Vec::new();
        while let Ok(event) = xml.read_event_into(&mut buf) {
            match event {
                Event::Start(el) => {
                    if el.local_name().as_ref() == b"Relationship" {
                        let mut id = None;
                        let mut target = None;
                        for attr in el.attributes().flatten() {
                            match attr.key.local_name().as_ref() {
                                b"Id" => {
                                    id = attr.decode_and_unescape_value(&xml).ok().map(|s| s.into_owned());
                                }
                                b"Target" => {
                                    target =
                                        attr.decode_and_unescape_value(&xml).ok().map(|s| s.into_owned());
                                }
                                _ => {}
                            }
                        }
                        if let (Some(i), Some(t)) = (id, target) {
                            rel_target_by_id.insert(i, t);
                        }
                    }
                }
                Event::Eof => break,
                _ => {}
            }
            buf.clear();
        }
    }

    let mut sheet_path_to_name: BTreeMap<String, String> = BTreeMap::new();
    if let Some(wb_bytes) = read_part(&mut archive, "xl/workbook.xml", &mut total_uncompressed)? {
        let mut xml = XmlReader::from_reader(wb_bytes.as_slice());
        xml.expand_empty_elements(true);
        let mut buf = Vec::new();
        while let Ok(event) = xml.read_event_into(&mut buf) {
            match event {
                Event::Start(el) => match el.local_name().as_ref() {
                    b"workbookPr" => {
                        for attr in el.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"date1904" {
                                let v = attr
                                    .decode_and_unescape_value(&xml)
                                    .unwrap_or_default();
                                if v == "1" || v.eq_ignore_ascii_case("true") {
                                    meta.date_system = Some(WorkbookDateSystem::Macintosh1904);
                                }
                            }
                        }
                    }
                    b"sheet" => {
                        let mut name = None;
                        let mut state = SheetVisibility::Visible;
                        let mut rid = None;
                        for attr in el.attributes().flatten() {
                            match attr.key.local_name().as_ref() {
                                b"name" => {
                                    name = attr
                                        .decode_and_unescape_value(&xml)
                                        .ok()
                                        .map(|s| s.into_owned());
                                }
                                b"state" => {
                                    let s = attr
                                        .decode_and_unescape_value(&xml)
                                        .unwrap_or_default();
                                    if s.eq_ignore_ascii_case("hidden") {
                                        state = SheetVisibility::Hidden;
                                    } else if s.eq_ignore_ascii_case("veryHidden") {
                                        state = SheetVisibility::VeryHidden;
                                    }
                                }
                                b"id" => {
                                    rid = attr
                                        .decode_and_unescape_value(&xml)
                                        .ok()
                                        .map(|s| s.into_owned());
                                }
                                _ => {}
                            }
                        }
                        if let Some(sheet_name) = name {
                            meta.sheet_visibility.insert(sheet_name.clone(), state);
                            if let Some(r) = rid
                                && let Some(target) = rel_target_by_id.get(&r)
                            {
                                let normalized = if let Some(stripped) = target.strip_prefix('/') {
                                    stripped.to_string()
                                } else if target.starts_with("xl/") {
                                    target.clone()
                                } else {
                                    format!("xl/{target}")
                                };
                                sheet_path_to_name.insert(normalized, sheet_name);
                            }
                        }
                    }
                    _ => {}
                },
                Event::Eof => break,
                _ => {}
            }
            buf.clear();
        }
    }

    // 3. Parse each worksheet XML for merges, frozen panes, autoFilter, protection, hyperlinks, and cell style indices
    for (sheet_path, sheet_name) in &sheet_path_to_name {
        if let Some(ws_bytes) = read_part(&mut archive, sheet_path, &mut total_uncompressed)? {
            parse_worksheet_xml(&ws_bytes, sheet_name, &mut meta);
        }
    }

    // 4. Parse comments XML files if present
    let comment_files: Vec<String> = file_names
        .iter()
        .filter(|n| n.starts_with("xl/comments") && n.ends_with(".xml"))
        .cloned()
        .collect();
    for (idx, comment_path) in comment_files.iter().enumerate() {
        if let Some(c_bytes) = read_part(&mut archive, comment_path, &mut total_uncompressed)? {
            let fallback_sheet = sheet_path_to_name
                .values()
                .nth(idx)
                .cloned()
                .unwrap_or_else(|| format!("Sheet{}", idx + 1));
            parse_comments_xml(&c_bytes, &fallback_sheet, &mut meta);
        }
    }

    Ok(meta)
}

fn parse_styles_xml(bytes: &[u8], meta: &mut OoxmlMetadata) {
    // Built-in Excel number formats
    for (id, code) in [
        (0u32, "General"),
        (1, "0"),
        (2, "0.00"),
        (3, "#,##0"),
        (4, "#,##0.00"),
        (9, "0%"),
        (10, "0.00%"),
        (14, "yyyy-mm-dd"),
        (15, "d-mmm-yy"),
        (16, "d-mmm"),
        (17, "mmm-yy"),
        (20, "h:mm"),
        (21, "h:mm:ss"),
        (22, "yyyy-mm-dd h:mm"),
        (49, "@"),
    ] {
        meta.number_formats.insert(id, code.to_string());
    }

    let mut xml = XmlReader::from_reader(bytes);
    xml.expand_empty_elements(true);
    let mut buf = Vec::new();
    let mut in_cell_xfs = false;
    while let Ok(event) = xml.read_event_into(&mut buf) {
        match event {
            Event::Start(el) => match el.local_name().as_ref() {
                b"numFmt" => {
                    let mut id = None;
                    let mut code = None;
                    for attr in el.attributes().flatten() {
                        match attr.key.local_name().as_ref() {
                            b"numFmtId" => {
                                id = attr
                                    .decode_and_unescape_value(&xml)
                                    .ok()
                                    .and_then(|s| s.parse::<u32>().ok());
                            }
                            b"formatCode" => {
                                code = attr
                                    .decode_and_unescape_value(&xml)
                                    .ok()
                                    .map(|s| s.into_owned());
                            }
                            _ => {}
                        }
                    }
                    if let (Some(i), Some(c)) = (id, code) {
                        meta.number_formats.insert(i, c);
                    }
                }
                b"cellXfs" => {
                    in_cell_xfs = true;
                }
                b"xf" if in_cell_xfs => {
                    let mut num_fmt_id = 0u32;
                    for attr in el.attributes().flatten() {
                        if attr.key.local_name().as_ref() == b"numFmtId"
                            && let Ok(v) = attr.decode_and_unescape_value(&xml)
                            && let Ok(parsed) = v.parse::<u32>()
                        {
                            num_fmt_id = parsed;
                        }
                    }
                    meta.cell_style_numfmt.push(num_fmt_id);
                }
                _ => {}
            },
            Event::End(el) if el.local_name().as_ref() == b"cellXfs" => {
                in_cell_xfs = false;
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
}

fn parse_worksheet_xml(bytes: &[u8], sheet_name: &str, meta: &mut OoxmlMetadata) {
    let mut xml = XmlReader::from_reader(bytes);
    xml.expand_empty_elements(true);
    let mut buf = Vec::new();
    while let Ok(event) = xml.read_event_into(&mut buf) {
        match event {
            Event::Start(el) => match el.local_name().as_ref() {
                b"pane" => {
                    for attr in el.attributes().flatten() {
                        if attr.key.local_name().as_ref() == b"state"
                            && let Ok(v) = attr.decode_and_unescape_value(&xml)
                            && v.contains("frozen")
                        {
                            meta.sheet_frozen.insert(sheet_name.to_string());
                        }
                    }
                }
                b"autoFilter" => {
                    for attr in el.attributes().flatten() {
                        if attr.key.local_name().as_ref() == b"ref"
                            && let Ok(v) = attr.decode_and_unescape_value(&xml)
                        {
                            meta.sheet_autofilter
                                .insert(sheet_name.to_string(), v.into_owned());
                        }
                    }
                }
                b"sheetProtection" => {
                    meta.sheet_protected.insert(sheet_name.to_string());
                }
                b"mergeCell" => {
                    for attr in el.attributes().flatten() {
                        if attr.key.local_name().as_ref() == b"ref"
                            && let Ok(v) = attr.decode_and_unescape_value(&xml)
                            && let Some(range) = parse_a1_range(&v)
                        {
                            meta.sheet_merges
                                .entry(sheet_name.to_string())
                                .or_default()
                                .push(range);
                        }
                    }
                }
                b"hyperlink" => {
                    let mut cell_ref = None;
                    let mut location = None;
                    for attr in el.attributes().flatten() {
                        match attr.key.local_name().as_ref() {
                            b"ref" => {
                                cell_ref = attr
                                    .decode_and_unescape_value(&xml)
                                    .ok()
                                    .map(|s| s.into_owned());
                            }
                            b"location" | b"display" | b"id" => {
                                if location.is_none() {
                                    location = attr
                                        .decode_and_unescape_value(&xml)
                                        .ok()
                                        .map(|s| s.into_owned());
                                }
                            }
                            _ => {}
                        }
                    }
                    if let Some(r) = cell_ref {
                        let target = location.unwrap_or_else(|| "external-link".to_string());
                        meta.sheet_hyperlinks
                            .insert((sheet_name.to_string(), r.clone()), target.clone());
                        meta.embedded_objects.push(EmbeddedObjectRecord {
                            kind: EmbeddedObjectKind::Hyperlink,
                            location: format!("{sheet_name}!{r}"),
                            handling: EmbeddedObjectHandling::Extracted,
                            summary: format!("Hyperlink at {sheet_name}!{r} -> {target}"),
                        });
                    }
                }
                b"c" => {
                    let mut addr = None;
                    let mut style_idx = None;
                    for attr in el.attributes().flatten() {
                        match attr.key.local_name().as_ref() {
                            b"r" => {
                                addr = attr
                                    .decode_and_unescape_value(&xml)
                                    .ok()
                                    .map(|s| s.into_owned());
                            }
                            b"s" => {
                                style_idx = attr
                                    .decode_and_unescape_value(&xml)
                                    .ok()
                                    .and_then(|s| s.parse::<usize>().ok());
                            }
                            _ => {}
                        }
                    }
                    if let (Some(a), Some(s_idx)) = (addr, style_idx)
                        && let Some(&num_fmt_id) = meta.cell_style_numfmt.get(s_idx)
                        && let Some(fmt_str) = meta.number_formats.get(&num_fmt_id)
                        && fmt_str != "General"
                    {
                        meta.cell_numfmt_by_addr
                            .insert((sheet_name.to_string(), a), fmt_str.clone());
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
}

fn parse_comments_xml(bytes: &[u8], sheet_name: &str, meta: &mut OoxmlMetadata) {
    let mut xml = XmlReader::from_reader(bytes);
    xml.expand_empty_elements(true);
    let mut buf = Vec::new();
    let mut current_ref: Option<String> = None;
    let mut current_text = String::new();
    while let Ok(event) = xml.read_event_into(&mut buf) {
        match event {
            Event::Start(el) if el.local_name().as_ref() == b"comment" => {
                current_text.clear();
                for attr in el.attributes().flatten() {
                    if attr.key.local_name().as_ref() == b"ref" {
                        current_ref = attr
                            .decode_and_unescape_value(&xml)
                            .ok()
                            .map(|s| s.into_owned());
                    }
                }
            }
            Event::Text(t) if current_ref.is_some() => {
                if let Ok(decoded) = t.unescape() {
                    if !current_text.is_empty() {
                        current_text.push(' ');
                    }
                    current_text.push_str(decoded.trim());
                }
            }
            Event::End(el) if el.local_name().as_ref() == b"comment" => {
                if let Some(r) = current_ref.take() {
                    let trimmed = current_text.trim().to_string();
                    if !trimmed.is_empty() {
                        meta.sheet_comments
                            .insert((sheet_name.to_string(), r.clone()), trimmed.clone());
                        meta.embedded_objects.push(EmbeddedObjectRecord {
                            kind: EmbeddedObjectKind::CommentOrNote,
                            location: format!("{sheet_name}!{r}"),
                            handling: EmbeddedObjectHandling::Extracted,
                            summary: format!("Comment at {sheet_name}!{r}"),
                        });
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
}

fn parse_a1_coord(reference: &str) -> Option<(u32, u32)> {
    let trimmed = reference.trim().replace('$', "");
    let col_len = trimmed.bytes().take_while(u8::is_ascii_alphabetic).count();
    if !(1..=3).contains(&col_len) {
        return None;
    }
    let (col_part, row_part) = trimmed.split_at(col_len);
    let row_1based = row_part.parse::<u32>().ok()?;
    if row_1based == 0 {
        return None;
    }
    let col_1based = col_part.bytes().fold(0u32, |acc, b| {
        acc * 26 + u32::from(b.to_ascii_uppercase().saturating_sub(b'A')) + 1
    });
    Some((row_1based - 1, col_1based - 1))
}

fn parse_a1_range(reference: &str) -> Option<SheetRange> {
    let mut parts = reference.split(':');
    let (start_row, start_col) = parse_a1_coord(parts.next()?)?;
    let (end_row, end_col) = match parts.next() {
        Some(end_str) => parse_a1_coord(end_str)?,
        None => (start_row, start_col),
    };
    Some(SheetRange {
        start_row: start_row.min(end_row),
        start_col: start_col.min(end_col),
        end_row: start_row.max(end_row),
        end_col: start_col.max(end_col),
    })
}

/// Ingests any supported workbook (`XLSX`, `XLS`, `XLSM`, `XLSB`, `CSV`, `ODS`) with optional cancellation.
///
/// # Errors
/// Returns [`SafiError`] if platform size limits (`SF-IMP-003`), ZIP bomb limits (`SF-IMP-005`),
/// or archive corruption/encryption checks (`SF-IMP-002`) fail, or if cancelled.
pub fn ingest_workbook_universal<R: Read + Seek>(
    mut reader: R,
    limits: PlatformLimits,
    filename_hint: Option<&str>,
    cancel_flag: Option<&AtomicBool>,
) -> Result<WorkbookIngestionReport, SafiError> {
    let check_cancelled = || -> Result<(), SafiError> {
        if cancel_flag.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            return Err(SafiError::new(
                ErrorCode::SF_IMP_002,
                ErrorSeverity::Error,
                "WorkbookIngestionCancelled",
                "import.cancelled",
            ));
        }
        Ok(())
    };

    check_cancelled()?;
    reader.seek(SeekFrom::Start(0)).map_err(|_| {
        SafiError::new(
            ErrorCode::SF_IMP_002,
            ErrorSeverity::Error,
            "WorkbookSeekFailed",
            "import.excel_corrupted",
        )
    })?;

    let mut raw_bytes = Vec::new();
    let max_read = limits.max_file_bytes.saturating_add(1);
    reader
        .take(max_read)
        .read_to_end(&mut raw_bytes)
        .map_err(|_| {
            SafiError::new(
                ErrorCode::SF_IMP_002,
                ErrorSeverity::Error,
                "WorkbookReadFailed",
                "import.excel_corrupted",
            )
        })?;

    if (raw_bytes.len() as u64) > limits.max_file_bytes {
        return Err(SafiError::new(
            ErrorCode::SF_IMP_003,
            ErrorSeverity::Error,
            "PlatformFileSizeLimitExceeded",
            "import.limit_exceeded",
        ));
    }

    let format = sniff_workbook_format(&raw_bytes, filename_hint);
    let mut progress_log = vec![IngestionProgressUpdate {
        phase: "Preflight".to_string(),
        sheets_processed: 0,
        total_sheets: 0,
        percent_complete: 10,
    }];

    if format == WorkbookFormat::Csv {
        return ingest_csv_as_workbook(&raw_bytes, limits, filename_hint, progress_log);
    }

    if format == WorkbookFormat::Unknown {
        return Err(SafiError::new(
            ErrorCode::SF_IMP_002,
            ErrorSeverity::Error,
            "UnsupportedOrCorruptedWorkbookFormat",
            "import.excel_corrupted",
        ));
    }

    let ooxml_meta = if matches!(format, WorkbookFormat::Xlsx | WorkbookFormat::Xlsm) {
        inspect_ooxml_package(&raw_bytes, raw_bytes.len() as u64)?
    } else {
        OoxmlMetadata::default()
    };

    check_cancelled()?;

    let mut workbook: Sheets<Cursor<&[u8]>> = open_workbook_auto_from_rs(Cursor::new(raw_bytes.as_slice()))
        .map_err(|_| {
            SafiError::new(
                ErrorCode::SF_IMP_002,
                ErrorSeverity::Error,
                "WorkbookOpenFailed",
                "import.excel_corrupted",
            )
        })?;

    let defined_names: Vec<DefinedNameRecord> = workbook
        .defined_names()
        .iter()
        .map(|(name, formula)| DefinedNameRecord {
            is_built_in: name.starts_with("_xlnm."),
            name: name.clone(),
            refers_to: formula.clone(),
        })
        .collect();

    let sheet_names = workbook.sheet_names();
    let total_sheets = sheet_names.len();
    progress_log.push(IngestionProgressUpdate {
        phase: "SheetDiscovery".to_string(),
        sheets_processed: 0,
        total_sheets,
        percent_complete: 25,
    });

    let mut worksheets = Vec::with_capacity(total_sheets);
    let mut total_tables_extracted = 0usize;
    let mut total_analytical_rows = 0usize;
    let mut total_formulas_detected = 0usize;
    let mut total_formulas_verified = 0usize;
    let mut cumulative_rows = 0u64;

    for (idx, sheet_name) in sheet_names.iter().enumerate() {
        check_cancelled()?;

        let range = workbook.worksheet_range(sheet_name).map_err(|_| {
            SafiError::new(
                ErrorCode::SF_IMP_002,
                ErrorSeverity::Error,
                "WorksheetRangeReadFailed",
                "import.excel_corrupted",
            )
        })?;

        let formula_range = workbook.worksheet_formula(sheet_name).ok();
        let merges = ooxml_meta
            .sheet_merges
            .get(sheet_name)
            .cloned()
            .unwrap_or_default();

        let ws_intel = build_worksheet_intelligence(
            sheet_name,
            idx,
            &range,
            formula_range.as_ref(),
            &merges,
            &ooxml_meta,
            &defined_names,
            limits.max_rows,
            &mut cumulative_rows,
        )?;

        total_tables_extracted += ws_intel.tables.len();
        total_analytical_rows += ws_intel
            .tables
            .iter()
            .map(|t| t.data_rows.len())
            .sum::<usize>();
        total_formulas_detected += ws_intel.formula_count;
        total_formulas_verified += ws_intel.verified_formula_count;
        worksheets.push(ws_intel);

        let pct = 25 + (((idx + 1) * 65) / total_sheets.max(1)) as u8;
        progress_log.push(IngestionProgressUpdate {
            phase: "TableSegmentation".to_string(),
            sheets_processed: idx + 1,
            total_sheets,
            percent_complete: pct.min(95),
        });
    }

    let visible_sheets = worksheets
        .iter()
        .filter(|w| w.visibility == SheetVisibility::Visible)
        .count();
    let hidden_sheets = total_sheets.saturating_sub(visible_sheets);

    progress_log.push(IngestionProgressUpdate {
        phase: "Completed".to_string(),
        sheets_processed: total_sheets,
        total_sheets,
        percent_complete: 100,
    });

    let support_level = match format {
        WorkbookFormat::Xlsx | WorkbookFormat::Csv => FormatSupportLevel::FullSupport,
        WorkbookFormat::Xlsm => FormatSupportLevel::SupportedWithMacroNeutralization,
        WorkbookFormat::Xls | WorkbookFormat::Xlsb | WorkbookFormat::Ods => {
            FormatSupportLevel::SupportedTabularAndFormulas
        }
        WorkbookFormat::Unknown => FormatSupportLevel::Unsupported,
    };

    Ok(WorkbookIngestionReport {
        detected_format: format,
        support_level,
        date_system: ooxml_meta
            .date_system
            .unwrap_or(WorkbookDateSystem::Standard1900),
        macros_neutralized: ooxml_meta.macros_neutralized,
        total_sheets,
        visible_sheets,
        hidden_sheets,
        total_tables_extracted,
        total_analytical_rows,
        total_formulas_detected,
        total_formulas_verified,
        defined_names,
        embedded_objects: ooxml_meta.embedded_objects,
        worksheets,
        warnings: ooxml_meta.warnings,
        progress_log,
    })
}

fn ingest_csv_as_workbook(
    raw_bytes: &[u8],
    limits: PlatformLimits,
    filename_hint: Option<&str>,
    mut progress_log: Vec<IngestionProgressUpdate>,
) -> Result<WorkbookIngestionReport, SafiError> {
    let (decoded, _) = detect_and_decode(raw_bytes)?;
    let delimiter = detect_delimiter(&decoded);
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(false)
        .flexible(true)
        .from_reader(decoded.as_bytes());

    let mut grid: Vec<Vec<String>> = Vec::new();
    for record in rdr.records() {
        let rec = record.map_err(|_| {
            SafiError::new(
                ErrorCode::SF_IMP_002,
                ErrorSeverity::Error,
                "CsvParseFailed",
                "import.csv_parse_failed",
            )
        })?;
        if (grid.len() as u64) >= limits.max_rows.saturating_add(10) {
            return Err(SafiError::new(
                ErrorCode::SF_IMP_003,
                ErrorSeverity::Error,
                "PlatformRowLimitExceeded",
                "import.limit_exceeded",
            ));
        }
        grid.push(rec.iter().map(|s| s.trim().to_string()).collect());
    }

    let sheet_name = filename_hint
        .and_then(|n| n.rsplit(['/', '\\']).next())
        .and_then(|n| n.split('.').next())
        .filter(|s| !s.is_empty())
        .unwrap_or("Sheet1")
        .to_string();

    let max_cols = grid.iter().map(Vec::len).max().unwrap_or(0);
    for row in &mut grid {
        row.resize(max_cols, String::new());
    }

    let tables = segment_sheet_tables(&sheet_name, &grid, &[]);
    let total_analytical_rows: usize = tables.iter().map(|t| t.data_rows.len()).sum();
    if (total_analytical_rows as u64) > limits.max_rows {
        return Err(SafiError::new(
            ErrorCode::SF_IMP_003,
            ErrorSeverity::Error,
            "PlatformRowLimitExceeded",
            "import.limit_exceeded",
        ));
    }

    let mut cell_metadata = Vec::new();
    for (r_idx, row) in grid.iter().enumerate() {
        for (c_idx, val) in row.iter().enumerate() {
            if val.is_empty() {
                continue;
            }
            let r_u32 = u32::try_from(r_idx).unwrap_or(0);
            let c_u32 = u32::try_from(c_idx).unwrap_or(0);
            cell_metadata.push(IngestedCellMetadata {
                sheet_name: sheet_name.clone(),
                address: coords_to_a1(r_u32, c_u32),
                row: r_u32,
                col: c_u32,
                raw_value: val.clone(),
                formula: None,
                cached_value: None,
                recalculated_value: None,
                formula_status: FormulaVerificationStatus::LiteralValue,
                data_type: classify_text_cell_type(val, None),
                number_format: None,
                merged_from: None,
                hyperlink: None,
                comment: None,
            });
        }
    }

    let used_range = if grid.is_empty() || max_cols == 0 {
        None
    } else {
        Some(SheetRange {
            start_row: 0,
            start_col: 0,
            end_row: u32::try_from(grid.len().saturating_sub(1)).unwrap_or(0),
            end_col: u32::try_from(max_cols.saturating_sub(1)).unwrap_or(0),
        })
    };

    let (classification, confidence, reason) = classify_worksheet(
        &sheet_name,
        SheetVisibility::Visible,
        &tables,
        0,
        false,
        total_analytical_rows,
    );

    let ws = WorksheetIntelligence {
        name: sheet_name,
        index: 0,
        visibility: SheetVisibility::Visible,
        classification,
        classification_confidence_pct: confidence,
        classification_reason: reason,
        used_range,
        raw_row_count: u32::try_from(grid.len()).unwrap_or(0),
        raw_col_count: u32::try_from(max_cols).unwrap_or(0),
        merged_ranges: Vec::new(),
        has_frozen_panes: false,
        auto_filter_range: None,
        print_area: None,
        is_protected: false,
        tables: tables.clone(),
        cell_metadata,
        formula_count: 0,
        verified_formula_count: 0,
    };

    progress_log.push(IngestionProgressUpdate {
        phase: "Completed".to_string(),
        sheets_processed: 1,
        total_sheets: 1,
        percent_complete: 100,
    });

    Ok(WorkbookIngestionReport {
        detected_format: WorkbookFormat::Csv,
        support_level: FormatSupportLevel::FullSupport,
        date_system: WorkbookDateSystem::Standard1900,
        macros_neutralized: false,
        total_sheets: 1,
        visible_sheets: 1,
        hidden_sheets: 0,
        total_tables_extracted: tables.len(),
        total_analytical_rows,
        total_formulas_detected: 0,
        total_formulas_verified: 0,
        defined_names: Vec::new(),
        embedded_objects: Vec::new(),
        worksheets: vec![ws],
        warnings: Vec::new(),
        progress_log,
    })
}

#[allow(clippy::too_many_arguments)]
fn build_worksheet_intelligence(
    sheet_name: &str,
    index: usize,
    range: &calamine::Range<Data>,
    formula_range: Option<&calamine::Range<String>>,
    merges: &[SheetRange],
    ooxml_meta: &OoxmlMetadata,
    defined_names: &[DefinedNameRecord],
    max_rows_limit: u64,
    cumulative_rows: &mut u64,
) -> Result<WorksheetIntelligence, SafiError> {
    let visibility = ooxml_meta
        .sheet_visibility
        .get(sheet_name)
        .copied()
        .unwrap_or(SheetVisibility::Visible);

    let Some((start_r, start_c)) = range.start() else {
        return Ok(WorksheetIntelligence {
            name: sheet_name.to_string(),
            index,
            visibility,
            classification: SheetClassification::Unknown,
            classification_confidence_pct: 100,
            classification_reason: "Empty worksheet".to_string(),
            used_range: None,
            raw_row_count: 0,
            raw_col_count: 0,
            merged_ranges: merges.iter().map(SheetRange::to_a1_range).collect(),
            has_frozen_panes: ooxml_meta.sheet_frozen.contains(sheet_name),
            auto_filter_range: ooxml_meta.sheet_autofilter.get(sheet_name).cloned(),
            print_area: find_print_area(sheet_name, defined_names),
            is_protected: ooxml_meta.sheet_protected.contains(sheet_name),
            tables: Vec::new(),
            cell_metadata: Vec::new(),
            formula_count: 0,
            verified_formula_count: 0,
        });
    };

    let (end_r, end_c) = range.end().unwrap_or((start_r, start_c));
    let total_r = (end_r + 1) as usize;
    let total_c = (end_c + 1) as usize;

    let mut grid = vec![vec![String::new(); total_c]; total_r];
    let mut numeric_lookup: BTreeMap<(u32, u32), f64> = BTreeMap::new();

    for (r_offset, row) in range.rows().enumerate() {
        let abs_r = start_r as usize + r_offset;
        for (c_offset, cell) in row.iter().enumerate() {
            let abs_c = start_c as usize + c_offset;
            let text = format_calamine_cell(cell);
            if let Some(num) = cell.get_float().or_else(|| cell.get_int().map(|i| i as f64)) {
                numeric_lookup.insert((abs_r as u32, abs_c as u32), num);
            }
            if let Some(slot) = grid.get_mut(abs_r).and_then(|r| r.get_mut(abs_c)) {
                *slot = text;
            }
        }
    }

    // Propagate merged header/cell values and record merge origins
    let mut merged_origin_by_coord: BTreeMap<(u32, u32), String> = BTreeMap::new();
    for m in merges {
        let origin_addr = coords_to_a1(m.start_row, m.start_col);
        let origin_val = grid
            .get(m.start_row as usize)
            .and_then(|r| r.get(m.start_col as usize))
            .cloned()
            .unwrap_or_default();
        for r in m.start_row..=m.end_row {
            for c in m.start_col..=m.end_col {
                if (r, c) != (m.start_row, m.start_col) {
                    merged_origin_by_coord.insert((r, c), origin_addr.clone());
                    // Propagate horizontal merged banners/headers when destination is empty
                    if r == m.start_row
                        && let Some(slot) = grid.get_mut(r as usize).and_then(|row| row.get_mut(c as usize))
                        && slot.is_empty()
                    {
                        *slot = origin_val.clone();
                    }
                }
            }
        }
    }

    // Collect cell metadata & verify formulas
    let mut cell_metadata = Vec::new();
    let mut formula_count = 0usize;
    let mut verified_formula_count = 0usize;

    for r in 0..total_r {
        for c in 0..total_c {
            let r_u32 = u32::try_from(r).unwrap_or(0);
            let c_u32 = u32::try_from(c).unwrap_or(0);
            let addr = coords_to_a1(r_u32, c_u32);
            let raw_val = grid[r][c].clone();
            let raw_data = range.get_value((r_u32, c_u32));
            let formula_str = formula_range
                .and_then(|fr| fr.get_value((r_u32, c_u32)))
                .filter(|f| !f.trim().is_empty())
                .cloned();
            let num_fmt = ooxml_meta
                .cell_numfmt_by_addr
                .get(&(sheet_name.to_string(), addr.clone()))
                .cloned();
            let hyperlink = ooxml_meta
                .sheet_hyperlinks
                .get(&(sheet_name.to_string(), addr.clone()))
                .cloned();
            let comment = ooxml_meta
                .sheet_comments
                .get(&(sheet_name.to_string(), addr.clone()))
                .cloned();
            let merged_from = merged_origin_by_coord.get(&(r_u32, c_u32)).cloned();

            if raw_val.is_empty()
                && formula_str.is_none()
                && hyperlink.is_none()
                && comment.is_none()
                && merged_from.is_none()
            {
                continue;
            }

            let data_type = classify_calamine_cell_type(raw_data, &raw_val, num_fmt.as_deref());
            let (cached_value, recalculated_value, formula_status) =
                if let Some(ref f_expr) = formula_str {
                    formula_count += 1;
                    let cached = if raw_val.is_empty() {
                        None
                    } else {
                        Some(raw_val.clone())
                    };
                    let recalc = evaluate_simple_sheet_formula(f_expr, &numeric_lookup);
                    let status = classify_formula_status(f_expr, cached.as_deref(), recalc.as_deref(), data_type);
                    if matches!(
                        status,
                        FormulaVerificationStatus::VerifiedCachedAndRecalculated
                            | FormulaVerificationStatus::RecalculatedLocally
                    ) {
                        verified_formula_count += 1;
                    }
                    // If cell had no cached value in workbook, populate grid with recalculated result
                    if raw_val.is_empty()
                        && let Some(ref rec_str) = recalc
                    {
                        grid[r][c] = rec_str.clone();
                    }
                    (cached, recalc, status)
                } else {
                    (None, None, FormulaVerificationStatus::LiteralValue)
                };

            cell_metadata.push(IngestedCellMetadata {
                sheet_name: sheet_name.to_string(),
                address: addr,
                row: r_u32,
                col: c_u32,
                raw_value: grid[r][c].clone(),
                formula: formula_str,
                cached_value,
                recalculated_value,
                formula_status,
                data_type,
                number_format: num_fmt,
                merged_from,
                hyperlink,
                comment,
            });
        }
    }

    let tables = segment_sheet_tables(sheet_name, &grid, merges);
    let sheet_analytical_rows: usize = tables.iter().map(|t| t.data_rows.len()).sum();
    *cumulative_rows = cumulative_rows.saturating_add(sheet_analytical_rows as u64);
    if *cumulative_rows > max_rows_limit {
        return Err(SafiError::new(
            ErrorCode::SF_IMP_003,
            ErrorSeverity::Error,
            "PlatformRowLimitExceeded",
            "import.limit_exceeded",
        ));
    }

    let has_pivot_object = ooxml_meta
        .embedded_objects
        .iter()
        .any(|o| o.kind == EmbeddedObjectKind::PivotTable);
    let (classification, confidence, reason) = classify_worksheet(
        sheet_name,
        visibility,
        &tables,
        formula_count,
        has_pivot_object,
        sheet_analytical_rows,
    );

    Ok(WorksheetIntelligence {
        name: sheet_name.to_string(),
        index,
        visibility,
        classification,
        classification_confidence_pct: confidence,
        classification_reason: reason,
        used_range: Some(SheetRange {
            start_row: start_r,
            start_col: start_c,
            end_row: end_r,
            end_col: end_c,
        }),
        raw_row_count: end_r.saturating_sub(start_r) + 1,
        raw_col_count: end_c.saturating_sub(start_c) + 1,
        merged_ranges: merges.iter().map(SheetRange::to_a1_range).collect(),
        has_frozen_panes: ooxml_meta.sheet_frozen.contains(sheet_name),
        auto_filter_range: ooxml_meta.sheet_autofilter.get(sheet_name).cloned(),
        print_area: find_print_area(sheet_name, defined_names),
        is_protected: ooxml_meta.sheet_protected.contains(sheet_name),
        tables,
        cell_metadata,
        formula_count,
        verified_formula_count,
    })
}

fn find_print_area(sheet_name: &str, defined_names: &[DefinedNameRecord]) -> Option<String> {
    defined_names
        .iter()
        .find(|d| d.name == "_xlnm.Print_Area" && d.refers_to.contains(sheet_name))
        .map(|d| d.refers_to.clone())
}

fn format_calamine_cell(cell: &Data) -> String {
    match cell {
        Data::Empty => String::new(),
        Data::String(s) => s.trim().to_string(),
        Data::Int(i) => i.to_string(),
        Data::Float(f) => {
            if f.fract() == 0.0 && f.abs() < 1e15 {
                format!("{f:.0}")
            } else {
                Decimal::from_f64_retain(*f)
                    .map(|d| d.normalize().to_string())
                    .unwrap_or_else(|| f.to_string())
            }
        }
        Data::Bool(b) => {
            if *b {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            }
        }
        Data::DateTime(dt) => dt.to_string(),
        Data::DateTimeIso(s) | Data::DurationIso(s) => s.trim().to_string(),
        Data::Error(err) => format!("{err:?}"),
    }
}

fn classify_calamine_cell_type(
    cell: Option<&Data>,
    raw_text: &str,
    num_fmt: Option<&str>,
) -> IngestedCellDataType {
    if raw_text.is_empty() || is_recognized_empty_token(raw_text) {
        return IngestedCellDataType::Empty;
    }
    if let Some(data) = cell {
        match data {
            Data::Empty => return IngestedCellDataType::Empty,
            Data::Bool(_) => return IngestedCellDataType::Boolean,
            Data::Error(_) => return IngestedCellDataType::FormulaError,
            Data::DateTime(_) | Data::DateTimeIso(_) => {
                if raw_text.contains(':') {
                    return IngestedCellDataType::DateTime;
                }
                return IngestedCellDataType::Date;
            }
            Data::DurationIso(_) => return IngestedCellDataType::Time,
            Data::Int(_) | Data::Float(_) => {
                if let Some(fmt) = num_fmt {
                    let lower = fmt.to_ascii_lowercase();
                    if lower.contains('%') {
                        return IngestedCellDataType::Percentage;
                    }
                    if lower.contains('$')
                        || lower.contains('€')
                        || lower.contains('£')
                        || lower.contains("egp")
                        || lower.contains("sar")
                        || lower.contains("aed")
                        || lower.contains("usd")
                        || lower.contains("ج.م")
                        || lower.contains("ر.س")
                    {
                        return IngestedCellDataType::Currency;
                    }
                    if (lower.contains('y') || lower.contains('m') || lower.contains('d'))
                        && !lower.contains('#')
                        && !lower.contains('0')
                    {
                        if lower.contains('h') || lower.contains('s') {
                            return IngestedCellDataType::DateTime;
                        }
                        return IngestedCellDataType::Date;
                    }
                }
                if matches!(data, Data::Int(_)) && !raw_text.contains('.') {
                    return IngestedCellDataType::Integer;
                }
                return IngestedCellDataType::Decimal;
            }
            Data::String(_) => {}
        }
    }
    classify_text_cell_type(raw_text, num_fmt)
}

fn classify_text_cell_type(raw_text: &str, num_fmt: Option<&str>) -> IngestedCellDataType {
    let trimmed = raw_text.trim();
    if trimmed.is_empty() || is_recognized_empty_token(trimmed) {
        return IngestedCellDataType::Empty;
    }
    if trimmed.starts_with('#') && (trimmed.ends_with('!') || trimmed.ends_with('?') || trimmed == "#N/A") {
        return IngestedCellDataType::FormulaError;
    }
    // Leading-zero preservation check (e.g. "00123", "+201012345678")
    if (trimmed.len() > 1
        && trimmed.starts_with('0')
        && !trimmed.starts_with("0.")
        && trimmed.bytes().all(|b| b.is_ascii_digit()))
        || (trimmed.starts_with('+') && trimmed.len() > 4 && trimmed[1..].bytes().all(|b| b.is_ascii_digit()))
    {
        return IngestedCellDataType::ZeroPreservedText;
    }
    if trimmed.eq_ignore_ascii_case("true") || trimmed.eq_ignore_ascii_case("false") {
        return IngestedCellDataType::Boolean;
    }
    if trimmed.ends_with('%')
        && trimmed[..trimmed.len() - 1]
            .trim()
            .parse::<f64>()
            .is_ok()
    {
        return IngestedCellDataType::Percentage;
    }
    let mut currency_stripped = trimmed.trim_start_matches(['$', '€', '£']).trim();
    for suffix in ["EGP", "SAR", "AED", "USD", "ج.م", "ر.س"] {
        if let Some(s) = currency_stripped.strip_suffix(suffix) {
            currency_stripped = s.trim();
        }
    }
    let currency_stripped = currency_stripped.replace(',', "");
    if currency_stripped.len() != trimmed.replace(',', "").len()
        && currency_stripped.trim().parse::<f64>().is_ok()
    {
        return IngestedCellDataType::Currency;
    }
    if let Ok(i) = trimmed.parse::<i64>() {
        let _ = i;
        if num_fmt.is_some_and(|f| f.contains('%')) {
            return IngestedCellDataType::Percentage;
        }
        return IngestedCellDataType::Integer;
    }
    if trimmed.replace(',', "").parse::<f64>().is_ok() {
        return IngestedCellDataType::Decimal;
    }
    if looks_like_iso_date(trimmed) {
        if trimmed.contains(':') {
            return IngestedCellDataType::DateTime;
        }
        return IngestedCellDataType::Date;
    }
    IngestedCellDataType::Text
}

fn looks_like_iso_date(s: &str) -> bool {
    let date_part = s.split([' ', 'T']).next().unwrap_or("");
    let parts: Vec<&str> = date_part.split(['-', '/']).collect();
    if parts.len() != 3 {
        return false;
    }
    let lens = (parts[0].len(), parts[1].len(), parts[2].len());
    let all_digits = parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    all_digits && (lens.0 == 4 || lens.2 == 4)
}

fn evaluate_simple_sheet_formula(
    formula: &str,
    numeric_lookup: &BTreeMap<(u32, u32), f64>,
) -> Option<String> {
    let expr = formula.trim().strip_prefix('=').unwrap_or(formula.trim()).trim();
    let upper = expr.to_ascii_uppercase();
    if upper.contains('!') || upper.contains('[') {
        return None;
    }

    for func in ["SUM", "AVERAGE", "MIN", "MAX", "COUNT"] {
        if let Some(inner) = upper
            .strip_prefix(func)
            .and_then(|s| s.trim_start().strip_prefix('('))
            .and_then(|s| s.strip_suffix(')'))
        {
            let range = parse_a1_range(inner.trim())?;
            let mut vals = Vec::new();
            for r in range.start_row..=range.end_row {
                for c in range.start_col..=range.end_col {
                    if let Some(&v) = numeric_lookup.get(&(r, c)) {
                        vals.push(v);
                    }
                }
            }
            if vals.is_empty() && func != "COUNT" {
                return Some("0".to_string());
            }
            let result = match func {
                "SUM" => vals.iter().sum::<f64>(),
                "AVERAGE" => vals.iter().sum::<f64>() / (vals.len() as f64),
                "MIN" => vals.iter().copied().fold(f64::INFINITY, f64::min),
                "MAX" => vals.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                "COUNT" => vals.len() as f64,
                _ => return None,
            };
            return Some(format_recalc_number(result));
        }
    }

    // Simple binary cell arithmetic: A2+B2, A2-B2, A2*B2, A2/B2
    for op in ['+', '-', '*', '/'] {
        if let Some((left, right)) = upper.split_once(op) {
            let l_val = parse_a1_coord(left.trim())
                .and_then(|coord| numeric_lookup.get(&coord).copied())
                .or_else(|| left.trim().parse::<f64>().ok())?;
            let r_val = parse_a1_coord(right.trim())
                .and_then(|coord| numeric_lookup.get(&coord).copied())
                .or_else(|| right.trim().parse::<f64>().ok())?;
            let res = match op {
                '+' => l_val + r_val,
                '-' => l_val - r_val,
                '*' => l_val * r_val,
                '/' => {
                    if r_val.abs() < f64::EPSILON {
                        return Some("#DIV/0!".to_string());
                    }
                    l_val / r_val
                }
                _ => return None,
            };
            return Some(format_recalc_number(res));
        }
    }

    None
}

fn format_recalc_number(val: f64) -> String {
    if val.fract().abs() < 1e-9 && val.abs() < 1e15 {
        format!("{val:.0}")
    } else {
        Decimal::from_f64_retain(val)
            .map(|d| d.round_dp(6).normalize().to_string())
            .unwrap_or_else(|| val.to_string())
    }
}

fn classify_formula_status(
    formula: &str,
    cached: Option<&str>,
    recalculated: Option<&str>,
    data_type: IngestedCellDataType,
) -> FormulaVerificationStatus {
    if data_type == IngestedCellDataType::FormulaError
        || cached.is_some_and(|c| c.starts_with('#'))
        || recalculated.is_some_and(|r| r.starts_with('#'))
    {
        return FormulaVerificationStatus::FormulaError;
    }
    let upper = formula.to_ascii_uppercase();
    if upper.contains('[') && upper.contains(']') {
        return FormulaVerificationStatus::ExternalWorkbookReference;
    }
    for volatile in ["NOW(", "TODAY(", "RAND(", "RANDBETWEEN(", "OFFSET(", "INDIRECT("] {
        if upper.contains(volatile) {
            return FormulaVerificationStatus::VolatileFormula;
        }
    }
    match (cached, recalculated) {
        (Some(c), Some(r)) => {
            let c_num = c.replace(',', "").parse::<f64>().ok();
            let r_num = r.replace(',', "").parse::<f64>().ok();
            if c == r || c_num.zip(r_num).is_some_and(|(a, b)| (a - b).abs() < 1e-4) {
                FormulaVerificationStatus::VerifiedCachedAndRecalculated
            } else {
                FormulaVerificationStatus::CachedResultOnly
            }
        }
        (None, Some(_)) => FormulaVerificationStatus::RecalculatedLocally,
        (Some(_), None) => FormulaVerificationStatus::CachedResultOnly,
        (None, None) => FormulaVerificationStatus::UnverifiedFormula,
    }
}

/// Segments a worksheet grid into one or more structured [`DetectedTableRegion`] blocks,
/// separating title banners, multi-row headers, repeated headers, subtotals, grand totals, and footnotes.
#[must_use]
pub fn segment_sheet_tables(
    sheet_name: &str,
    grid: &[Vec<String>],
    _merges: &[SheetRange],
) -> Vec<DetectedTableRegion> {
    if grid.is_empty() {
        return Vec::new();
    }

    // 1. Identify vertical row bands separated by completely empty rows.
    let mut row_bands: Vec<(usize, usize)> = Vec::new();
    let mut band_start: Option<usize> = None;
    for (r_idx, row) in grid.iter().enumerate() {
        let non_empty = row.iter().any(|c| !c.trim().is_empty());
        match (band_start, non_empty) {
            (None, true) => band_start = Some(r_idx),
            (Some(s), false) => {
                row_bands.push((s, r_idx - 1));
                band_start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = band_start {
        row_bands.push((s, grid.len() - 1));
    }

    let mut pending_banner_titles: Vec<(u32, String)> = Vec::new();
    let mut tables = Vec::new();

    for (r_start, r_end) in row_bands {
        // Check if this band is just a 1-2 row title banner (1 non-empty cell per row)
        let band_rows = &grid[r_start..=r_end];
        let max_non_empty_in_band = band_rows
            .iter()
            .map(|r| r.iter().filter(|c| !c.trim().is_empty()).count())
            .max()
            .unwrap_or(0);

        if band_rows.len() <= 2 && max_non_empty_in_band <= 1 {
            for (offset, r) in band_rows.iter().enumerate() {
                if let Some(text) = r.iter().find(|c| !c.trim().is_empty()) {
                    pending_banner_titles.push((
                        u32::try_from(r_start + offset).unwrap_or(0),
                        text.trim().to_string(),
                    ));
                }
            }
            continue;
        }

        // 2. Within this row band, split horizontally by empty column gaps to detect side-by-side tables.
        let max_cols = band_rows.iter().map(Vec::len).max().unwrap_or(0);
        let mut col_bands: Vec<(usize, usize)> = Vec::new();
        let mut c_start: Option<usize> = None;
        for c_idx in 0..max_cols {
            let col_has_data = band_rows
                .iter()
                .any(|r| r.get(c_idx).is_some_and(|v| !v.trim().is_empty()));
            match (c_start, col_has_data) {
                (None, true) => c_start = Some(c_idx),
                (Some(s), false) => {
                    col_bands.push((s, c_idx - 1));
                    c_start = None;
                }
                _ => {}
            }
        }
        if let Some(s) = c_start {
            col_bands.push((s, max_cols - 1));
        }

        for (c_start_idx, c_end_idx) in col_bands {
            if let Some(region) = build_table_region_from_block(
                sheet_name,
                grid,
                r_start,
                r_end,
                c_start_idx,
                c_end_idx,
                tables.len() + 1,
                &mut pending_banner_titles,
            ) {
                tables.push(region);
            }
        }
    }

    tables
}

#[allow(clippy::too_many_arguments)]
fn build_table_region_from_block(
    sheet_name: &str,
    grid: &[Vec<String>],
    r_start: usize,
    r_end: usize,
    c_start: usize,
    c_end: usize,
    table_seq: usize,
    pending_banner_titles: &mut Vec<(u32, String)>,
) -> Option<DetectedTableRegion> {
    let width = c_end.saturating_sub(c_start) + 1;
    if width == 0 {
        return None;
    }

    let mut separated_rows = Vec::new();
    for (row_idx, title_text) in pending_banner_titles.drain(..) {
        let kind = if separated_rows.is_empty() {
            SeparatedRowKind::Title
        } else {
            SeparatedRowKind::Subtitle
        };
        separated_rows.push(SeparatedSheetRow {
            sheet_row: row_idx,
            kind,
            values: vec![title_text],
        });
    }

    let mut cur_r = r_start;
    // Strip leading single-cell title/subtitle rows attached directly to the top of the block
    while cur_r < r_end {
        let slice = &grid[cur_r][c_start..=c_end.min(grid[cur_r].len().saturating_sub(1))];
        let non_empty: Vec<String> = slice
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let distinct_non_empty: BTreeSet<&String> = non_empty.iter().collect();
        if distinct_non_empty.len() == 1 && width > 1 {
            let kind = if separated_rows
                .iter()
                .any(|s| s.kind == SeparatedRowKind::Title)
            {
                SeparatedRowKind::Subtitle
            } else {
                SeparatedRowKind::Title
            };
            separated_rows.push(SeparatedSheetRow {
                sheet_row: u32::try_from(cur_r).unwrap_or(0),
                kind,
                values: vec![non_empty[0].clone()],
            });
            cur_r += 1;
        } else {
            break;
        }
    }

    if cur_r > r_end {
        return None;
    }

    // Check if the first 2 rows form a multi-row hierarchical header
    let first_row = extract_row_slice(&grid[cur_r], c_start, c_end);
    let mut header_rows = vec![u32::try_from(cur_r).unwrap_or(0)];
    let mut headers = first_row.clone();

    if cur_r + 1 < r_end {
        let second_row = extract_row_slice(&grid[cur_r + 1], c_start, c_end);
        let third_row = extract_row_slice(&grid[cur_r + 2], c_start, c_end);
        let second_all_text = second_row
            .iter()
            .filter(|s| !s.is_empty())
            .all(|s| s.parse::<f64>().is_err());
        let third_has_numbers = third_row
            .iter()
            .filter(|s| !s.is_empty())
            .any(|s| s.replace(',', "").parse::<f64>().is_ok());
        let first_has_duplicates = {
            let non_empty: Vec<&String> = first_row.iter().filter(|s| !s.is_empty()).collect();
            let unique: BTreeSet<&String> = non_empty.iter().copied().collect();
            non_empty.len() > unique.len()
        };

        if second_all_text && third_has_numbers && first_has_duplicates {
            header_rows.push(u32::try_from(cur_r + 1).unwrap_or(0));
            headers = first_row
                .iter()
                .zip(second_row.iter())
                .enumerate()
                .map(|(idx, (top, sub))| match (top.is_empty(), sub.is_empty()) {
                    (false, false) if top != sub => format!("{top} — {sub}"),
                    (false, _) => top.clone(),
                    (true, false) => sub.clone(),
                    (true, true) => format!("Column_{}", idx + 1),
                })
                .collect();
            cur_r += 2;
        } else {
            cur_r += 1;
        }
    } else {
        cur_r += 1;
    }

    // Deduplicate and fill blank header names
    let mut seen_headers: BTreeMap<String, usize> = BTreeMap::new();
    for (idx, h) in headers.iter_mut().enumerate() {
        let base = if h.trim().is_empty() {
            format!("Column_{}", idx + 1)
        } else {
            h.trim().to_string()
        };
        let count = seen_headers.entry(base.clone()).or_insert(0);
        *count += 1;
        if *count > 1 {
            *h = format!("{base}_{count}");
        } else {
            *h = base;
        }
    }

    let mut data_rows = Vec::new();
    let mut source_row_indices = Vec::new();

    for row_idx in cur_r..=r_end {
        let row_vals = extract_row_slice(&grid[row_idx], c_start, c_end);
        let non_empty: Vec<&String> = row_vals.iter().filter(|s| !s.trim().is_empty()).collect();
        if non_empty.is_empty() {
            continue;
        }

        // Check for repeated header row
        let matches_header = row_vals
            .iter()
            .zip(headers.iter())
            .filter(|(a, b)| !a.is_empty() && a.eq_ignore_ascii_case(b))
            .count();
        if matches_header >= headers.len().max(2) / 2 && matches_header >= 2 {
            separated_rows.push(SeparatedSheetRow {
                sheet_row: u32::try_from(row_idx).unwrap_or(0),
                kind: SeparatedRowKind::RepeatedHeader,
                values: row_vals,
            });
            continue;
        }

        // Check for subtotal / grand total row
        if let Some(total_kind) = detect_total_row_kind(&row_vals) {
            separated_rows.push(SeparatedSheetRow {
                sheet_row: u32::try_from(row_idx).unwrap_or(0),
                kind: total_kind,
                values: row_vals,
            });
            continue;
        }

        // Check for trailing footnote row (single text cell starting with Note / Source / * / ملاحظة)
        if row_idx == r_end && non_empty.len() == 1 && width > 1 && is_footnote_text(non_empty[0]) {
            separated_rows.push(SeparatedSheetRow {
                sheet_row: u32::try_from(row_idx).unwrap_or(0),
                kind: SeparatedRowKind::Footnote,
                values: vec![non_empty[0].clone()],
            });
            continue;
        }

        data_rows.push(row_vals);
        source_row_indices.push(u32::try_from(row_idx).unwrap_or(0));
    }

    let title = separated_rows
        .iter()
        .find(|s| s.kind == SeparatedRowKind::Title)
        .and_then(|s| s.values.first())
        .cloned()
        .unwrap_or_else(|| {
            if table_seq == 1 {
                sheet_name.to_string()
            } else {
                format!("{sheet_name} (Table {table_seq})")
            }
        });

    let confidence_pct = if !data_rows.is_empty() && headers.len() >= 2 {
        95
    } else if !data_rows.is_empty() {
        80
    } else {
        60
    };

    Some(DetectedTableRegion {
        table_id: format!("tbl_{table_seq}"),
        title,
        sheet_name: sheet_name.to_string(),
        bounds: SheetRange {
            start_row: header_rows.first().copied().unwrap_or(0),
            start_col: u32::try_from(c_start).unwrap_or(0),
            end_row: u32::try_from(r_end).unwrap_or(0),
            end_col: u32::try_from(c_end).unwrap_or(0),
        },
        header_rows,
        headers,
        data_rows,
        source_row_indices,
        separated_rows,
        confidence_pct,
    })
}

fn extract_row_slice(row: &[String], c_start: usize, c_end: usize) -> Vec<String> {
    (c_start..=c_end)
        .map(|c| row.get(c).map(|s| s.trim().to_string()).unwrap_or_default())
        .collect()
}

fn detect_total_row_kind(row_vals: &[String]) -> Option<SeparatedRowKind> {
    let first_text = row_vals
        .iter()
        .find(|s| !s.trim().is_empty())?
        .to_ascii_lowercase();
    let trimmed = first_text.trim();
    if trimmed == "grand total"
        || trimmed == "total"
        || trimmed.starts_with("grand total")
        || trimmed.starts_with("total ")
        || trimmed == "الإجمالي العام"
        || trimmed == "الإجمالي"
        || trimmed == "الاجمالي"
        || trimmed == "المجموع الكلي"
        || trimmed == "المجموع"
    {
        return Some(SeparatedRowKind::GrandTotal);
    }
    if trimmed.starts_with("subtotal")
        || trimmed.starts_with("sub-total")
        || trimmed.contains("subtotal")
        || trimmed.starts_with("إجمالي فرعي")
        || trimmed.starts_with("اجمالي فرعي")
        || trimmed.starts_with("مجموع فرعي")
    {
        return Some(SeparatedRowKind::Subtotal);
    }
    None
}

fn is_footnote_text(text: &str) -> bool {
    let lower = text.trim().to_ascii_lowercase();
    lower.starts_with('*')
        || lower.starts_with("note:")
        || lower.starts_with("notes:")
        || lower.starts_with("source:")
        || lower.starts_with("ملاحظة:")
        || lower.starts_with("المصدر:")
}

fn classify_worksheet(
    sheet_name: &str,
    visibility: SheetVisibility,
    tables: &[DetectedTableRegion],
    formula_count: usize,
    has_pivot_object: bool,
    analytical_rows: usize,
) -> (SheetClassification, u8, String) {
    let lower = sheet_name.to_ascii_lowercase();
    if analytical_rows == 0 && tables.is_empty() {
        return (
            SheetClassification::Unknown,
            90,
            "Worksheet has no structured tabular records".to_string(),
        );
    }
    if lower.contains("readme")
        || lower.contains("instruction")
        || lower.contains("guide")
        || lower.contains("ارشادات")
        || lower.contains("إرشادات")
        || lower.contains("تعليمات")
    {
        return (
            SheetClassification::Instructions,
            94,
            "Sheet name and structure match instructions/documentation".to_string(),
        );
    }
    if lower.contains("config")
        || lower.contains("setting")
        || lower.contains("param")
        || lower.contains("إعدادات")
        || lower.contains("ثوابت")
    {
        return (
            SheetClassification::Configuration,
            92,
            "Sheet name matches configuration/parameters".to_string(),
        );
    }
    if has_pivot_object || lower.contains("pivot") || lower.contains("محوري") {
        return (
            SheetClassification::PivotOutput,
            93,
            "Contains pivot structure or pivot nomenclature".to_string(),
        );
    }
    if lower.contains("lookup")
        || lower.contains("mapping")
        || lower.contains("code")
        || lower.contains("أكواد")
        || lower.contains("ترميز")
    {
        return (
            SheetClassification::LookupTable,
            90,
            "Compact code/description mapping sheet".to_string(),
        );
    }
    if lower.contains("ref") || lower.contains("master") || lower.contains("مرجع") {
        return (
            SheetClassification::ReferenceData,
            88,
            "Reference/master entity table".to_string(),
        );
    }
    if lower.contains("summary")
        || lower.contains("dashboard")
        || lower.contains("kpi")
        || lower.contains("ملخص")
        || lower.contains("مؤشرات")
    {
        return (
            SheetClassification::Summary,
            92,
            "Aggregated summary/KPI sheet".to_string(),
        );
    }
    if lower.contains("report") || lower.contains("تقرير") {
        return (
            SheetClassification::Report,
            89,
            "Formatted report worksheet".to_string(),
        );
    }
    if visibility != SheetVisibility::Visible && formula_count > 0 {
        return (
            SheetClassification::SupportingCalculations,
            87,
            "Hidden worksheet containing intermediate formulas".to_string(),
        );
    }
    let col_count = tables.first().map_or(0, |t| t.headers.len());
    if col_count <= 3 && analytical_rows <= 50 && formula_count == 0 {
        return (
            SheetClassification::LookupTable,
            82,
            "Small 2-3 column reference dictionary".to_string(),
        );
    }
    (
        SheetClassification::MainDataset,
        91,
        "Primary analytical dataset".to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn build_custom_xlsx(
        workbook_xml: &str,
        sheet1_xml: &str,
        extra_files: &[(&str, &[u8])],
    ) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut cursor);
            let opts = SimpleFileOptions::default();

            let _ = zip.start_file("[Content_Types].xml", opts);
            let _ = zip.write_all(
                br#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
  <Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
</Types>"#,
            );

            let _ = zip.start_file("_rels/.rels", opts);
            let _ = zip.write_all(
                br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>"#,
            );

            let _ = zip.start_file("xl/_rels/workbook.xml.rels", opts);
            let _ = zip.write_all(
                br#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
</Relationships>"#,
            );

            let _ = zip.start_file("xl/workbook.xml", opts);
            let _ = zip.write_all(workbook_xml.as_bytes());

            let _ = zip.start_file("xl/worksheets/sheet1.xml", opts);
            let _ = zip.write_all(sheet1_xml.as_bytes());

            for (path, data) in extra_files {
                let _ = zip.start_file(*path, opts);
                let _ = zip.write_all(data);
            }
            let _ = zip.finish();
        }
        cursor.into_inner()
    }

    #[test]
    fn compatibility_matrix_covers_all_six_required_formats() {
        let matrix = format_compatibility_matrix();
        assert_eq!(matrix.len(), 6);
        let formats: BTreeSet<WorkbookFormat> = matrix.iter().map(|m| m.format).collect();
        assert!(formats.contains(&WorkbookFormat::Xlsx));
        assert!(formats.contains(&WorkbookFormat::Xls));
        assert!(formats.contains(&WorkbookFormat::Xlsm));
        assert!(formats.contains(&WorkbookFormat::Xlsb));
        assert!(formats.contains(&WorkbookFormat::Csv));
        assert!(formats.contains(&WorkbookFormat::Ods));
    }

    #[test]
    fn segments_multiple_tables_titles_and_subtotals_on_single_sheet() {
        let grid = vec![
            vec![
                "Regional Sales Report 2026".to_string(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ],
            vec![String::new(); 6],
            vec![
                "Branch".to_string(),
                "Revenue".to_string(),
                String::new(),
                "Code".to_string(),
                "Manager".to_string(),
                String::new(),
            ],
            vec![
                "Cairo".to_string(),
                "1200".to_string(),
                String::new(),
                "B01".to_string(),
                "Ahmed".to_string(),
                String::new(),
            ],
            vec![
                "Alexandria".to_string(),
                "800".to_string(),
                String::new(),
                "B02".to_string(),
                "Sara".to_string(),
                String::new(),
            ],
            vec![
                "Grand Total".to_string(),
                "2000".to_string(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ],
        ];

        let tables = segment_sheet_tables("Sales", &grid, &[]);
        assert_eq!(tables.len(), 2);
        assert_eq!(tables[0].title, "Regional Sales Report 2026");
        assert_eq!(tables[0].headers, vec!["Branch", "Revenue"]);
        assert_eq!(tables[0].data_rows.len(), 2);
        assert!(
            tables[0]
                .separated_rows
                .iter()
                .any(|s| s.kind == SeparatedRowKind::GrandTotal)
        );
        assert_eq!(tables[1].headers, vec!["Code", "Manager"]);
        assert_eq!(tables[1].data_rows.len(), 2);
    }

    #[test]
    fn ingests_xlsm_neutralizes_macros_verifies_formulas_and_detects_1904_dates() {
        let wb_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"
          xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <workbookPr date1904="1"/>
  <sheets>
    <sheet name="Summary_KPIs" sheetId="1" state="hidden" r:id="rId1"/>
  </sheets>
</workbook>"#;

        let sheet1_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData>
    <row r="1">
      <c r="A1" t="inlineStr"><is><t>Item</t></is></c>
      <c r="B1" t="inlineStr"><is><t>Amount</t></is></c>
    </row>
    <row r="2">
      <c r="A2" t="inlineStr"><is><t>00125</t></is></c>
      <c r="B2"><v>100</v></c>
    </row>
    <row r="3">
      <c r="A3" t="inlineStr"><is><t>00126</t></is></c>
      <c r="B3"><v>250</v></c>
    </row>
    <row r="4">
      <c r="A4" t="inlineStr"><is><t>Total</t></is></c>
      <c r="B4"><f>SUM(B2:B3)</f><v>350</v></c>
    </row>
  </sheetData>
</worksheet>"#;

        let bytes = build_custom_xlsx(
            wb_xml,
            sheet1_xml,
            &[("xl/vbaProject.bin", b"DUMMY_VBA_PROJECT")],
        );
        let limits = PlatformLimits {
            max_file_bytes: 10 * 1024 * 1024,
            max_rows: 1000,
        };
        let report = ingest_workbook_universal(
            Cursor::new(bytes),
            limits,
            Some("finance.xlsm"),
            None,
        );
        assert!(report.is_ok());
        if let Ok(rep) = report {
            assert_eq!(rep.detected_format, WorkbookFormat::Xlsm);
            assert!(rep.macros_neutralized);
            assert_eq!(rep.date_system, WorkbookDateSystem::Macintosh1904);
            assert_eq!(rep.hidden_sheets, 1);
            assert_eq!(rep.total_formulas_detected, 1);
            assert_eq!(rep.total_formulas_verified, 1);
            let ws = &rep.worksheets[0];
            assert_eq!(ws.visibility, SheetVisibility::Hidden);
            assert_eq!(ws.classification, SheetClassification::Summary);
            // Total row is separated so analytical rows == 2
            assert_eq!(ws.tables[0].data_rows.len(), 2);
            // Leading zeros preserved as ZeroPreservedText
            let a2 = ws.cell_metadata.iter().find(|c| c.address == "A2");
            assert_eq!(
                a2.map(|c| c.data_type),
                Some(IngestedCellDataType::ZeroPreservedText)
            );
            let b4 = ws.cell_metadata.iter().find(|c| c.address == "B4");
            assert_eq!(
                b4.map(|c| c.formula_status),
                Some(FormulaVerificationStatus::VerifiedCachedAndRecalculated)
            );
        }
    }
}
