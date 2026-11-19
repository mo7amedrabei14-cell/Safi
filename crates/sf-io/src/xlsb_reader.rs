//! Panic-free native MS-XLSB (`BIFF12`) binary workbook parser (`crates/sf-io`).
//!
//! Reads ZIP-packaged BIFF12 parts (`xl/workbook.bin`, `xl/sharedStrings.bin`, `xl/styles.bin`,
//! `xl/worksheets/*.bin`, `xl/comments*.bin`, and relationship XMLs) with:
//! - Strict BIFF12 stream framing (clean EOF vs truncated/malformed stream errors `SF-IMP-002`)
//! - Excel worksheet coordinate bounds (`0..=1_048_575` rows, `0..=16_383` columns)
//! - Independent checked dense-materialization extent budgets (`SF-IMP-005`) for values & formulas
//!   BEFORE calling `calamine::Range::from_sparse`
//! - Explicit preservation of undecoded BIFF12 formulas (`UnsupportedFormula`) so cached formula
//!   results are never silently downgraded to `LiteralValue`
//! - Zero `.unwrap()` / `.expect()` calls.

use std::io::Cursor;

use calamine::{Cell, CellErrorType, Data, Range};
use sf_core::error::{ErrorCode, ErrorSeverity, SafiError};
use zip::ZipArchive;

use crate::workbook_intelligence::{
    DefinedNameRecord, EmbeddedObjectHandling, EmbeddedObjectKind, EmbeddedObjectRecord,
    OoxmlMetadata, SheetRange, SheetVisibility, WorkbookDateSystem, check_zip_budget_and_scan,
    coords_to_a1, parse_rels_xml_map, read_zip_part_budgeted, seed_builtin_number_formats,
};

/// Maximum legal 0-based Excel/XLSB worksheet row index (`1,048,576` rows: `0..=1_048_575`).
pub(crate) const MAX_XLSB_ROW: u32 = 1_048_575;

/// Maximum legal 0-based Excel/XLSB worksheet column index (`16,384` columns: `0..=16_383`, `A..=XFD`).
pub(crate) const MAX_XLSB_COL: u32 = 16_383;

/// Maximum dense cell area permitted before `Range::from_sparse` or dense grid materialization.
///
/// Caps worst-case dense allocation to ~2 million slots (protecting Desktop/WASM memory budgets
/// against small XLSB files with two distant corner cells such as `A1` and `XFD1048576`).
pub(crate) const MAX_DENSE_GRID_CELLS: u64 = 2_000_000;

/// Maximum cells expanded by a single worksheet `BrtMergeCell` record.
const MAX_MERGE_CELLS: u64 = 100_000;

/// Explicit formula marker emitted when a BIFF12 formula record (`0x0008..=0x000B`) contains
/// expression tokens outside the local BIFF12 formula decoder.
///
/// Contains no `[` or `]` characters so it is classified explicitly as `UnsupportedFormula`
/// rather than `ExternalWorkbookReference` or `LiteralValue`.
pub(crate) const UNDECODED_BIFF12_FORMULA_MARKER: &str = "=(unsupported BIFF12 formula)";

#[derive(Debug)]
pub(crate) struct XlsbSheetParsed {
    pub(crate) name: String,
    pub(crate) range: Range<Data>,
    pub(crate) formula_range: Option<Range<String>>,
}

#[derive(Debug)]
pub(crate) struct XlsbWorkbookParsed {
    pub(crate) meta: OoxmlMetadata,
    pub(crate) sheets: Vec<XlsbSheetParsed>,
    pub(crate) defined_names: Vec<DefinedNameRecord>,
}

#[derive(Debug, Clone)]
enum SupBookKind {
    SelfWorkbook,
    External(String),
}

fn corrupted_xlsb(kind: &'static str) -> SafiError {
    SafiError::new(
        ErrorCode::SF_IMP_002,
        ErrorSeverity::Error,
        kind,
        "import.excel_corrupted",
    )
}

fn extent_limit_exceeded(kind: &'static str) -> SafiError {
    SafiError::new(
        ErrorCode::SF_IMP_005,
        ErrorSeverity::Error,
        kind,
        "import.zip_bomb_detected",
    )
}

/// Validates Excel coordinate bounds and checks that dense materialization of either
/// `[min..=max]` (`calamine::Range::from_sparse`) or `[0..=max]` (`build_worksheet_intelligence`)
/// stays strictly within `MAX_DENSE_GRID_CELLS` and `max_rows_limit`.
#[derive(Debug, Default)]
struct ExtentTracker {
    count: u64,
    min_row: u32,
    max_row: u32,
    min_col: u32,
    max_col: u32,
}

impl ExtentTracker {
    fn observe(&mut self, row: u32, col: u32, max_rows_limit: u64) -> Result<(), SafiError> {
        if row > MAX_XLSB_ROW || col > MAX_XLSB_COL {
            return Err(corrupted_xlsb("XlsbCoordinateOutOfBounds"));
        }
        if self.count == 0 {
            self.min_row = row;
            self.max_row = row;
            self.min_col = col;
            self.max_col = col;
        } else {
            self.min_row = self.min_row.min(row);
            self.max_row = self.max_row.max(row);
            self.min_col = self.min_col.min(col);
            self.max_col = self.max_col.max(col);
        }
        self.count = self.count.saturating_add(1);
        self.validate(max_rows_limit)
    }

    fn validate(&self, max_rows_limit: u64) -> Result<(), SafiError> {
        if self.count == 0 {
            return Ok(());
        }
        if self.max_row > MAX_XLSB_ROW || self.max_col > MAX_XLSB_COL {
            return Err(corrupted_xlsb("XlsbCoordinateOutOfBounds"));
        }

        // 1. Check bounding-box span materialized by `calamine::Range::from_sparse`.
        let span_rows = u64::from(self.max_row - self.min_row)
            .checked_add(1)
            .ok_or_else(|| extent_limit_exceeded("XlsbExtentArithmeticOverflow"))?;
        let span_cols = u64::from(self.max_col - self.min_col)
            .checked_add(1)
            .ok_or_else(|| extent_limit_exceeded("XlsbExtentArithmeticOverflow"))?;
        let span_cells = span_rows
            .checked_mul(span_cols)
            .ok_or_else(|| extent_limit_exceeded("XlsbExtentArithmeticOverflow"))?;

        if span_cells > MAX_DENSE_GRID_CELLS {
            return Err(extent_limit_exceeded("XlsbSparseExtentLimitExceeded"));
        }

        // 2. Check full dense grid `(0..=max_row) x (0..=max_col)` materialized by worksheet grid.
        let total_rows = u64::from(self.max_row)
            .checked_add(1)
            .ok_or_else(|| extent_limit_exceeded("XlsbExtentArithmeticOverflow"))?;
        let total_cols = u64::from(self.max_col)
            .checked_add(1)
            .ok_or_else(|| extent_limit_exceeded("XlsbExtentArithmeticOverflow"))?;
        let dense_cells = total_rows
            .checked_mul(total_cols)
            .ok_or_else(|| extent_limit_exceeded("XlsbExtentArithmeticOverflow"))?;

        if dense_cells > MAX_DENSE_GRID_CELLS {
            return Err(extent_limit_exceeded("XlsbDenseGridExtentLimitExceeded"));
        }
        if max_rows_limit > 0 && span_rows > max_rows_limit.saturating_add(64) {
            return Err(extent_limit_exceeded("XlsbRowExtentLimitExceeded"));
        }
        Ok(())
    }
}

struct Biff12Iter<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Biff12Iter<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// Reads the next BIFF12 record, distinguishing clean EOF (`Ok(None)`) from
    /// truncated/malformed record headers, invalid varint lengths, or truncated payloads (`Err`).
    fn next_record(&mut self) -> Result<Option<(u16, &'a [u8])>, SafiError> {
        if self.pos == self.data.len() {
            return Ok(None);
        }

        let b0 = *self
            .data
            .get(self.pos)
            .ok_or_else(|| corrupted_xlsb("TruncatedBiff12RecordId"))?;
        self.pos += 1;

        let rec_id = if (b0 & 0x80) != 0 {
            let b1 = *self
                .data
                .get(self.pos)
                .ok_or_else(|| corrupted_xlsb("TruncatedBiff12RecordId"))?;
            self.pos += 1;
            if (b1 & 0x80) != 0 {
                return Err(corrupted_xlsb("InvalidBiff12RecordIdEncoding"));
            }
            u16::from(b0 & 0x7F) | (u16::from(b1 & 0x7F) << 7)
        } else {
            u16::from(b0)
        };

        let mut len: usize = 0;
        let mut terminated = false;
        for shift in 0..4usize {
            let lb = *self
                .data
                .get(self.pos)
                .ok_or_else(|| corrupted_xlsb("TruncatedBiff12RecordLength"))?;
            self.pos += 1;
            len |= usize::from(lb & 0x7F) << (7 * shift);
            if (lb & 0x80) == 0 {
                terminated = true;
                break;
            }
        }
        if !terminated {
            return Err(corrupted_xlsb("InvalidBiff12RecordLengthEncoding"));
        }

        let end = self
            .pos
            .checked_add(len)
            .ok_or_else(|| corrupted_xlsb("Biff12RecordLengthOverflow"))?;
        let payload = self
            .data
            .get(self.pos..end)
            .ok_or_else(|| corrupted_xlsb("TruncatedBiff12RecordPayload"))?;
        self.pos = end;
        Ok(Some((rec_id, payload)))
    }
}

fn read_u16_le(slice: &[u8], offset: usize) -> Option<u16> {
    let bytes = slice.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32_le(slice: &[u8], offset: usize) -> Option<u32> {
    let bytes = slice.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_i32_le(slice: &[u8], offset: usize) -> Option<i32> {
    let bytes = slice.get(offset..offset.checked_add(4)?)?;
    Some(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_f64_le(slice: &[u8], offset: usize) -> Option<f64> {
    let bytes = slice.get(offset..offset.checked_add(8)?)?;
    Some(f64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}

fn decode_utf16le(bytes: &[u8]) -> String {
    let (decoded, _, _) = encoding_rs::UTF_16LE.decode(bytes);
    decoded.into_owned()
}

fn read_xl_wide_string(slice: &[u8], offset: usize) -> Option<(String, usize)> {
    let cch = usize::try_from(read_u32_le(slice, offset)?).ok()?;
    let byte_len = cch.checked_mul(2)?;
    let start = offset.checked_add(4)?;
    let end = start.checked_add(byte_len)?;
    let raw = slice.get(start..end)?;
    Some((decode_utf16le(raw), 4usize.checked_add(byte_len)?))
}

fn read_xl_nullable_wide_string(slice: &[u8], offset: usize) -> Option<(Option<String>, usize)> {
    let raw_cch = read_u32_le(slice, offset)?;
    if raw_cch == 0xFFFF_FFFF {
        return Some((None, 4));
    }
    let (text, consumed) = read_xl_wide_string(slice, offset)?;
    Some((Some(text), consumed))
}

fn read_rfx_range(slice: &[u8], offset: usize) -> Option<SheetRange> {
    let r1 = read_u32_le(slice, offset)?;
    let r2 = read_u32_le(slice, offset.checked_add(4)?)?;
    let c1 = read_u32_le(slice, offset.checked_add(8)?)?;
    let c2 = read_u32_le(slice, offset.checked_add(12)?)?;
    Some(SheetRange {
        start_row: r1.min(r2),
        end_row: r1.max(r2),
        start_col: c1.min(c2),
        end_col: c1.max(c2),
    })
}

fn decode_rk_value(rk: u32) -> Data {
    let mut val = if (rk & 0x02) != 0 {
        let signed = (rk as i32) >> 2;
        if (rk & 0x01) == 0 {
            return Data::Int(i64::from(signed));
        }
        f64::from(signed)
    } else {
        f64::from_bits(u64::from(rk & 0xFFFF_FFFC) << 32)
    };
    if (rk & 0x01) != 0 {
        val /= 100.0;
    }
    Data::Float(val)
}

fn decode_xlsb_error(code: u8) -> CellErrorType {
    match code {
        0x00 => CellErrorType::Null,
        0x07 => CellErrorType::Div0,
        0x0F => CellErrorType::Value,
        0x17 => CellErrorType::Ref,
        0x1D => CellErrorType::Name,
        0x24 => CellErrorType::Num,
        0x2A => CellErrorType::NA,
        0x2B => CellErrorType::GettingData,
        _ => CellErrorType::Value,
    }
}

fn format_biff12_error_str(code: u8) -> &'static str {
    match code {
        0x00 => "#NULL!",
        0x07 => "#DIV/0!",
        0x0F => "#VALUE!",
        0x17 => "#REF!",
        0x1D => "#NAME?",
        0x24 => "#NUM!",
        0x2A => "#N/A",
        0x2B => "#GETTING_DATA",
        _ => "#VALUE!",
    }
}

fn excel_ftab_name(iftab: u16) -> Option<&'static str> {
    match iftab {
        0 => Some("COUNT"),
        1 => Some("IF"),
        2 => Some("ISNA"),
        3 => Some("ISERROR"),
        4 => Some("SUM"),
        5 => Some("AVERAGE"),
        6 => Some("MIN"),
        7 => Some("MAX"),
        8 => Some("ROW"),
        9 => Some("COLUMN"),
        10 => Some("NA"),
        15 => Some("SIN"),
        16 => Some("COS"),
        17 => Some("TAN"),
        19 => Some("PI"),
        20 => Some("SQRT"),
        21 => Some("EXP"),
        22 => Some("LN"),
        23 => Some("LOG10"),
        24 => Some("ABS"),
        25 => Some("INT"),
        26 => Some("SIGN"),
        27 => Some("ROUND"),
        28 => Some("LOOKUP"),
        29 => Some("INDEX"),
        31 => Some("MID"),
        32 => Some("LEN"),
        33 => Some("VALUE"),
        36 => Some("AND"),
        37 => Some("OR"),
        38 => Some("NOT"),
        39 => Some("MOD"),
        63 => Some("RAND"),
        64 => Some("MATCH"),
        65 => Some("DATE"),
        74 => Some("NOW"),
        78 => Some("OFFSET"),
        101 => Some("HLOOKUP"),
        102 => Some("VLOOKUP"),
        111 => Some("CHAR"),
        112 => Some("LOWER"),
        113 => Some("UPPER"),
        115 => Some("LEFT"),
        116 => Some("RIGHT"),
        118 => Some("TRIM"),
        121 => Some("SUBSTITUTE"),
        148 => Some("INDIRECT"),
        221 => Some("TODAY"),
        227 => Some("MEDIAN"),
        228 => Some("SUMPRODUCT"),
        255 => Some("XLOOKUP"),
        269 => Some("STDEV"),
        336 => Some("CONCATENATE"),
        345 => Some("SUMIF"),
        346 => Some("COUNTIF"),
        347 => Some("SUMIFS"),
        348 => Some("COUNTIFS"),
        361 => Some("AVERAGEIF"),
        364 => Some("IFERROR"),
        _ => None,
    }
}

fn fixed_ftab_argc(iftab: u16) -> usize {
    match iftab {
        10 | 19 | 63 | 74 | 221 => 0,
        2 | 3 | 15 | 16 | 17 | 20 | 21 | 22 | 23 | 24 | 25 | 26 | 32 | 33 | 38 | 111 | 112
        | 113 | 118 => 1,
        27 | 39 | 115 | 116 | 346 | 364 => 2,
        31 | 65 | 121 => 3,
        _ => 1,
    }
}

fn normalize_zip_rel_path(base_dir: &str, target: &str) -> String {
    let cleaned = target.replace('\\', "/");
    if let Some(stripped) = cleaned.strip_prefix('/') {
        return stripped.to_string();
    }
    if cleaned.starts_with("xl/") {
        return cleaned;
    }
    if let Some(up) = cleaned.strip_prefix("../") {
        if base_dir.contains('/') {
            let parent = base_dir.rsplit_once('/').map_or("", |(p, _)| p);
            if parent.is_empty() {
                return up.to_string();
            }
            return format!("{parent}/{up}");
        }
        return up.to_string();
    }
    if base_dir.is_empty() {
        cleaned
    } else {
        format!("{base_dir}/{cleaned}")
    }
}

fn format_cell_ref(row: u32, col_word: u16) -> String {
    let col = u32::from(col_word & 0x3FFF);
    coords_to_a1(row, col)
}

fn decode_biff12_formula(
    rgce: &[u8],
    extern_sheets: &[String],
    defined_names: &[DefinedNameRecord],
) -> Option<String> {
    if rgce.is_empty() {
        return None;
    }
    let mut stack: Vec<String> = Vec::new();
    let mut pos: usize = 0;

    while let Some(&ptg) = rgce.get(pos) {
        pos += 1;
        match ptg {
            0x03..=0x0E | 0x10 | 0x11 => {
                let op = match ptg {
                    0x03 => "+",
                    0x04 => "-",
                    0x05 => "*",
                    0x06 => "/",
                    0x07 => "^",
                    0x08 => "&",
                    0x09 => "<",
                    0x0A => "<=",
                    0x0B => "=",
                    0x0C => ">=",
                    0x0D => ">",
                    0x0E => "<>",
                    0x10 => ",",
                    0x11 => ":",
                    _ => "?",
                };
                let right = stack.pop()?;
                let left = stack.pop()?;
                stack.push(format!("{left}{op}{right}"));
            }
            0x12 => {
                let top = stack.pop()?;
                stack.push(format!("+{top}"));
            }
            0x13 => {
                let top = stack.pop()?;
                stack.push(format!("-{top}"));
            }
            0x14 => {
                let top = stack.pop()?;
                stack.push(format!("{top}%"));
            }
            0x15 => {
                let top = stack.pop()?;
                stack.push(format!("({top})"));
            }
            0x16 => {
                stack.push(String::new());
            }
            0x17 => {
                let cch = usize::from(read_u16_le(rgce, pos)?);
                let byte_len = cch.checked_mul(2)?;
                let start = pos.checked_add(2)?;
                let end = start.checked_add(byte_len)?;
                let text = decode_utf16le(rgce.get(start..end)?);
                pos = end;
                stack.push(format!("\"{text}\""));
            }
            0x18 => {
                let sub = *rgce.get(pos)?;
                pos += 1;
                let skip = match sub {
                    0x19 => 12,
                    0x1D => 4,
                    _ => return None,
                };
                if pos.checked_add(skip)? > rgce.len() {
                    return None;
                }
                pos += skip;
            }
            0x19 => {
                let opt = *rgce.get(pos)?;
                pos += 1;
                if (opt & 0x04) != 0 {
                    let count = usize::from(read_u16_le(rgce, pos)?);
                    pos = pos.checked_add(2)?.checked_add((count + 1) * 2)?;
                    if pos > rgce.len() {
                        return None;
                    }
                } else {
                    if pos.checked_add(2)? > rgce.len() {
                        return None;
                    }
                    pos += 2;
                    if (opt & 0x10) != 0 {
                        let arg = stack.pop()?;
                        stack.push(format!("SUM({arg})"));
                    }
                }
            }
            0x1C => {
                let code = *rgce.get(pos)?;
                pos += 1;
                stack.push(format_biff12_error_str(code).to_string());
            }
            0x1D => {
                let val = *rgce.get(pos)?;
                pos += 1;
                stack.push(if val == 0 { "FALSE" } else { "TRUE" }.to_string());
            }
            0x1E => {
                let val = read_u16_le(rgce, pos)?;
                pos += 2;
                stack.push(val.to_string());
            }
            0x1F => {
                let val = read_f64_le(rgce, pos)?;
                pos += 8;
                if val.fract() == 0.0 && val.abs() < 1e15 {
                    stack.push(format!("{val:.0}"));
                } else {
                    stack.push(val.to_string());
                }
            }
            0x20 | 0x40 | 0x60 => {
                if pos.checked_add(14)? > rgce.len() {
                    return None;
                }
                pos += 14;
                stack.push("{...}".to_string());
            }
            0x21 | 0x41 | 0x61 => {
                let iftab = read_u16_le(rgce, pos)?;
                pos += 2;
                let argc = fixed_ftab_argc(iftab);
                if stack.len() < argc {
                    return None;
                }
                let start = stack.len() - argc;
                let args: Vec<String> = stack.drain(start..).collect();
                let name = excel_ftab_name(iftab)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("FUNC_{iftab}"));
                stack.push(format!("{name}({})", args.join(",")));
            }
            0x22 | 0x42 | 0x62 => {
                let argc = usize::from(*rgce.get(pos)?);
                let iftab = read_u16_le(rgce, pos + 1)?;
                pos += 3;
                if stack.len() < argc {
                    return None;
                }
                let start = stack.len() - argc;
                let args: Vec<String> = stack.drain(start..).collect();
                let name = excel_ftab_name(iftab)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("FUNC_{iftab}"));
                stack.push(format!("{name}({})", args.join(",")));
            }
            0x23 | 0x43 | 0x63 => {
                let idx = usize::try_from(read_u32_le(rgce, pos)?).ok()?;
                pos += 4;
                let name = idx
                    .checked_sub(1)
                    .and_then(|i| defined_names.get(i))
                    .map_or_else(|| format!("NAME_{idx}"), |d| d.name.clone());
                stack.push(name);
            }
            0x24 | 0x44 | 0x64 => {
                let row = read_u32_le(rgce, pos)?;
                let col_w = read_u16_le(rgce, pos + 4)?;
                pos += 6;
                stack.push(format_cell_ref(row, col_w));
            }
            0x25 | 0x45 | 0x65 => {
                let r1 = read_u32_le(rgce, pos)?;
                let r2 = read_u32_le(rgce, pos + 4)?;
                let c1 = read_u16_le(rgce, pos + 8)?;
                let c2 = read_u16_le(rgce, pos + 10)?;
                pos += 12;
                let a = format_cell_ref(r1, c1);
                let b = format_cell_ref(r2, c2);
                stack.push(format!("{a}:{b}"));
            }
            0x2A | 0x4A | 0x6A => {
                if pos.checked_add(6)? > rgce.len() {
                    return None;
                }
                pos += 6;
                stack.push("#REF!".to_string());
            }
            0x2B | 0x4B | 0x6B => {
                if pos.checked_add(12)? > rgce.len() {
                    return None;
                }
                pos += 12;
                stack.push("#REF!".to_string());
            }
            0x39 | 0x59 | 0x79 => {
                let ixti = usize::from(read_u16_le(rgce, pos)?);
                let name_idx = read_u32_le(rgce, pos + 2)?;
                pos += 6;
                let scope = extern_sheets
                    .get(ixti)
                    .map(String::as_str)
                    .unwrap_or("[ExternalWorkbook]");
                stack.push(format!("{scope}!NAME_{name_idx}"));
            }
            0x3A | 0x5A | 0x7A => {
                let ixti = usize::from(read_u16_le(rgce, pos)?);
                let row = read_u32_le(rgce, pos + 2)?;
                let col_w = read_u16_le(rgce, pos + 6)?;
                pos += 8;
                let scope = extern_sheets
                    .get(ixti)
                    .map(String::as_str)
                    .unwrap_or("[ExternalWorkbook]");
                stack.push(format!("{scope}!{}", format_cell_ref(row, col_w)));
            }
            0x3B | 0x5B | 0x7B => {
                let ixti = usize::from(read_u16_le(rgce, pos)?);
                let r1 = read_u32_le(rgce, pos + 2)?;
                let r2 = read_u32_le(rgce, pos + 6)?;
                let c1 = read_u16_le(rgce, pos + 10)?;
                let c2 = read_u16_le(rgce, pos + 12)?;
                pos += 14;
                let scope = extern_sheets
                    .get(ixti)
                    .map(String::as_str)
                    .unwrap_or("[ExternalWorkbook]");
                stack.push(format!(
                    "{scope}!{}:{}",
                    format_cell_ref(r1, c1),
                    format_cell_ref(r2, c2)
                ));
            }
            0x3C | 0x5C | 0x7C => {
                let ixti = usize::from(read_u16_le(rgce, pos)?);
                pos += 8;
                let scope = extern_sheets
                    .get(ixti)
                    .map(String::as_str)
                    .unwrap_or("[ExternalWorkbook]");
                stack.push(format!("{scope}!#REF!"));
            }
            0x3D | 0x5D | 0x7D => {
                let ixti = usize::from(read_u16_le(rgce, pos)?);
                pos += 14;
                let scope = extern_sheets
                    .get(ixti)
                    .map(String::as_str)
                    .unwrap_or("[ExternalWorkbook]");
                stack.push(format!("{scope}!#REF!"));
            }
            _ => return None,
        }
    }

    stack.pop().map(|expr| format!("={expr}"))
}

fn parse_styles_bin(bytes: &[u8], meta: &mut OoxmlMetadata) -> Result<(), SafiError> {
    seed_builtin_number_formats(meta);
    let mut iter = Biff12Iter::new(bytes);
    let mut in_cell_xfs = false;
    let mut saw_begin = false;
    let mut saw_end = false;
    while let Some((rec_id, payload)) = iter.next_record()? {
        match rec_id {
            0x0116 => saw_begin = true,
            0x0117 => saw_end = true,
            0x002C => {
                let ifmt =
                    read_u16_le(payload, 0).ok_or_else(|| corrupted_xlsb("TruncatedBrtFmt"))?;
                let (code, _) = read_xl_wide_string(payload, 2)
                    .ok_or_else(|| corrupted_xlsb("TruncatedBrtFmtCode"))?;
                meta.number_formats.insert(u32::from(ifmt), code);
            }
            0x0269 => in_cell_xfs = true,
            0x026A => in_cell_xfs = false,
            0x002F if in_cell_xfs => {
                let ifmt =
                    read_u16_le(payload, 2).ok_or_else(|| corrupted_xlsb("TruncatedBrtXF"))?;
                meta.cell_style_numfmt.push(u32::from(ifmt));
            }
            _ => {}
        }
    }
    if saw_begin && !saw_end {
        return Err(corrupted_xlsb("TruncatedStylesStream"));
    }
    Ok(())
}

fn parse_shared_strings_bin(bytes: &[u8]) -> Result<Vec<String>, SafiError> {
    let mut strings = Vec::new();
    let mut iter = Biff12Iter::new(bytes);
    let mut saw_begin = false;
    let mut saw_end = false;
    while let Some((rec_id, payload)) = iter.next_record()? {
        match rec_id {
            0x009F => saw_begin = true,
            0x00A0 => saw_end = true,
            0x0013 => {
                let (text, _) = read_xl_wide_string(payload, 1)
                    .ok_or_else(|| corrupted_xlsb("TruncatedBrtSSTItem"))?;
                strings.push(text);
            }
            _ => {}
        }
    }
    if saw_begin && !saw_end {
        return Err(corrupted_xlsb("TruncatedSharedStringsStream"));
    }
    Ok(strings)
}

fn parse_comments_bin(
    bytes: &[u8],
    sheet_name: &str,
    meta: &mut OoxmlMetadata,
) -> Result<(), SafiError> {
    let mut iter = Biff12Iter::new(bytes);
    let mut active_ref: Option<String> = None;
    let mut saw_begin = false;
    let mut saw_end = false;
    while let Some((rec_id, payload)) = iter.next_record()? {
        match rec_id {
            0x0274 => saw_begin = true,
            0x0275 => saw_end = true,
            0x027B => {
                let range = read_rfx_range(payload, 4)
                    .ok_or_else(|| corrupted_xlsb("TruncatedBrtBeginComment"))?;
                active_ref = Some(coords_to_a1(range.start_row, range.start_col));
            }
            0x027D => {
                if let Some(cell_ref) = active_ref.as_ref() {
                    let (text, _) = read_xl_wide_string(payload, 1)
                        .ok_or_else(|| corrupted_xlsb("TruncatedBrtCommentText"))?;
                    let trimmed = text.trim().to_string();
                    if !trimmed.is_empty() {
                        meta.sheet_comments
                            .insert((sheet_name.to_string(), cell_ref.clone()), trimmed);
                        meta.embedded_objects.push(EmbeddedObjectRecord {
                            kind: EmbeddedObjectKind::CommentOrNote,
                            location: format!("{sheet_name}!{cell_ref}"),
                            handling: EmbeddedObjectHandling::Extracted,
                            summary: format!("Comment at {sheet_name}!{cell_ref}"),
                        });
                    }
                }
            }
            0x027C => active_ref = None,
            _ => {}
        }
    }
    if saw_begin && !saw_end {
        return Err(corrupted_xlsb("TruncatedCommentsStream"));
    }
    Ok(())
}

pub(crate) fn parse_xlsb_package(
    bytes: &[u8],
    file_bytes: u64,
    max_rows_limit: u64,
) -> Result<XlsbWorkbookParsed, SafiError> {
    let mut cursor = Cursor::new(bytes);
    let mut archive =
        ZipArchive::new(&mut cursor).map_err(|_| corrupted_xlsb("CorruptedExcelArchive"))?;

    let (budget, mut total_uncompressed, mut meta, _file_names) =
        check_zip_budget_and_scan(&mut archive, file_bytes)?;

    if let Some(styles_bytes) = read_zip_part_budgeted(
        &mut archive,
        "xl/styles.bin",
        budget,
        &mut total_uncompressed,
    )? {
        parse_styles_bin(&styles_bytes, &mut meta)?;
    }

    let shared_strings = if let Some(sst_bytes) = read_zip_part_budgeted(
        &mut archive,
        "xl/sharedStrings.bin",
        budget,
        &mut total_uncompressed,
    )? {
        parse_shared_strings_bin(&sst_bytes)?
    } else {
        Vec::new()
    };

    let wb_rels = read_zip_part_budgeted(
        &mut archive,
        "xl/_rels/workbook.bin.rels",
        budget,
        &mut total_uncompressed,
    )?
    .map(|b| parse_rels_xml_map(&b))
    .unwrap_or_default();

    let wb_bytes = read_zip_part_budgeted(
        &mut archive,
        "xl/workbook.bin",
        budget,
        &mut total_uncompressed,
    )?
    .ok_or_else(|| corrupted_xlsb("MissingXlsbWorkbookStream"))?;

    if wb_bytes.is_empty() {
        return Err(corrupted_xlsb("EmptyXlsbWorkbookStream"));
    }

    struct RawSheetMeta {
        name: String,
        zip_path: String,
        is_worksheet: bool,
    }

    let mut sheet_metas: Vec<RawSheetMeta> = Vec::new();
    let mut sup_books: Vec<SupBookKind> = Vec::new();
    let mut raw_extern_entries: Vec<(i32, i32, i32)> = Vec::new();
    let mut pending_names: Vec<(String, bool, Vec<u8>)> = Vec::new();

    let mut wb_iter = Biff12Iter::new(&wb_bytes);
    let mut saw_begin_book = false;
    let mut saw_end_book = false;
    while let Some((rec_id, payload)) = wb_iter.next_record()? {
        match rec_id {
            0x0083 => saw_begin_book = true,
            0x0084 => saw_end_book = true,
            0x0099 => {
                if payload.first().is_some_and(|b| (b & 0x01) != 0) {
                    meta.date_system = Some(WorkbookDateSystem::Macintosh1904);
                }
            }
            0x009C => {
                let hs_state = read_u32_le(payload, 0)
                    .ok_or_else(|| corrupted_xlsb("TruncatedBrtBundleSh"))?;
                let (maybe_rel, rel_bytes) = read_xl_nullable_wide_string(payload, 8)
                    .ok_or_else(|| corrupted_xlsb("TruncatedBrtBundleShRel"))?;
                let name_offset = 8usize
                    .checked_add(rel_bytes)
                    .ok_or_else(|| corrupted_xlsb("BrtBundleShOffsetOverflow"))?;
                let (name, _) = read_xl_wide_string(payload, name_offset)
                    .ok_or_else(|| corrupted_xlsb("TruncatedBrtBundleShName"))?;
                let vis = match hs_state {
                    1 => SheetVisibility::Hidden,
                    2 => SheetVisibility::VeryHidden,
                    _ => SheetVisibility::Visible,
                };
                meta.sheet_visibility.insert(name.clone(), vis);
                if let Some(rid) = maybe_rel
                    && let Some(target) = wb_rels.get(&rid)
                {
                    let zip_path = normalize_zip_rel_path("xl", target);
                    let is_worksheet = zip_path.starts_with("xl/worksheets/");
                    if !is_worksheet {
                        meta.warnings.push(format!(
                            "Sheet '{name}' is not a standard worksheet; tabular extraction applies only to worksheet cells."
                        ));
                    }
                    sheet_metas.push(RawSheetMeta {
                        name,
                        zip_path,
                        is_worksheet,
                    });
                }
            }
            0x0165 => sup_books.push(SupBookKind::SelfWorkbook),
            0x0163 | 0x0164 | 0x0166 | 0x0167 => {
                let rel_label = read_xl_wide_string(payload, 0)
                    .map(|(s, _)| s)
                    .unwrap_or_else(|| "ExternalWorkbook".to_string());
                sup_books.push(SupBookKind::External(rel_label));
            }
            0x016A => {
                let cxti = read_u32_le(payload, 0)
                    .ok_or_else(|| corrupted_xlsb("TruncatedBrtExternSheet"))?;
                let count = usize::try_from(cxti).unwrap_or(0).min(payload.len() / 12);
                for i in 0..count {
                    let base = 4 + i * 12;
                    if let (Some(sup), Some(first), Some(last)) = (
                        read_i32_le(payload, base),
                        read_i32_le(payload, base + 4),
                        read_i32_le(payload, base + 8),
                    ) {
                        raw_extern_entries.push((sup, first, last));
                    }
                }
            }
            0x0027 => {
                let flags =
                    read_u32_le(payload, 0).ok_or_else(|| corrupted_xlsb("TruncatedBrtName"))?;
                let (name, str_len) = read_xl_wide_string(payload, 9)
                    .ok_or_else(|| corrupted_xlsb("TruncatedBrtNameText"))?;
                let is_built_in = (flags & 0x0020) != 0 || name.starts_with("_xlnm.");
                let len_off = 9usize
                    .checked_add(str_len)
                    .ok_or_else(|| corrupted_xlsb("BrtNameOffsetOverflow"))?;
                let rgce_len = read_u32_le(payload, len_off)
                    .and_then(|v| usize::try_from(v).ok())
                    .ok_or_else(|| corrupted_xlsb("TruncatedBrtNameFormulaLen"))?;
                let rgce_start = len_off
                    .checked_add(4)
                    .ok_or_else(|| corrupted_xlsb("BrtNameOffsetOverflow"))?;
                let rgce_end = rgce_start
                    .checked_add(rgce_len)
                    .ok_or_else(|| corrupted_xlsb("BrtNameFormulaOverflow"))?;
                let rgce = payload
                    .get(rgce_start..rgce_end)
                    .ok_or_else(|| corrupted_xlsb("TruncatedBrtNameFormula"))?
                    .to_vec();
                pending_names.push((name, is_built_in, rgce));
            }
            _ => {}
        }
    }

    if saw_begin_book && !saw_end_book {
        return Err(corrupted_xlsb("TruncatedXlsbWorkbookStream"));
    }

    let extern_sheets: Vec<String> = raw_extern_entries
        .iter()
        .map(|(sup_idx, itab_first, _)| {
            let sup = usize::try_from(*sup_idx)
                .ok()
                .and_then(|i| sup_books.get(i));
            match sup {
                Some(SupBookKind::External(id)) => format!("[{id}]ExternalSheet"),
                Some(SupBookKind::SelfWorkbook) | None => {
                    if *sup_idx > 0 {
                        "[ExternalWorkbook]ExternalSheet".to_string()
                    } else if *itab_first < 0 {
                        "ThisWorkbook".to_string()
                    } else {
                        usize::try_from(*itab_first)
                            .ok()
                            .and_then(|idx| sheet_metas.get(idx))
                            .map(|s| s.name.clone())
                            .unwrap_or_else(|| "Sheet1".to_string())
                    }
                }
            }
        })
        .collect();

    let mut defined_names: Vec<DefinedNameRecord> = Vec::with_capacity(pending_names.len());
    for (name, is_built_in, rgce) in &pending_names {
        let refers_to =
            decode_biff12_formula(rgce, &extern_sheets, &defined_names).unwrap_or_default();
        defined_names.push(DefinedNameRecord {
            name: name.clone(),
            refers_to,
            is_built_in: *is_built_in,
        });
    }

    let mut parsed_sheets = Vec::with_capacity(sheet_metas.len());
    for sheet in &sheet_metas {
        if !sheet.is_worksheet {
            parsed_sheets.push(XlsbSheetParsed {
                name: sheet.name.clone(),
                range: Range::empty(),
                formula_range: None,
            });
            continue;
        }

        let ws_rels_path = if let Some((dir, file)) = sheet.zip_path.rsplit_once('/') {
            format!("{dir}/_rels/{file}.rels")
        } else {
            format!("_rels/{}.rels", sheet.zip_path)
        };
        let ws_base_dir = sheet.zip_path.rsplit_once('/').map_or("", |(dir, _)| dir);
        let ws_rels =
            read_zip_part_budgeted(&mut archive, &ws_rels_path, budget, &mut total_uncompressed)?
                .map(|b| parse_rels_xml_map(&b))
                .unwrap_or_default();

        for target in ws_rels.values() {
            let resolved = normalize_zip_rel_path(ws_base_dir, target);
            if resolved.contains("comments")
                && resolved.ends_with(".bin")
                && let Some(c_bytes) = read_zip_part_budgeted(
                    &mut archive,
                    &resolved,
                    budget,
                    &mut total_uncompressed,
                )?
            {
                parse_comments_bin(&c_bytes, &sheet.name, &mut meta)?;
            }
        }

        let ws_bytes = read_zip_part_budgeted(
            &mut archive,
            &sheet.zip_path,
            budget,
            &mut total_uncompressed,
        )?
        .ok_or_else(|| corrupted_xlsb("MissingXlsbWorksheetStream"))?;

        if ws_bytes.is_empty() {
            return Err(corrupted_xlsb("EmptyXlsbWorksheetStream"));
        }

        let mut value_cells: Vec<Cell<Data>> = Vec::new();
        let mut formula_cells: Vec<Cell<String>> = Vec::new();
        let mut value_extent = ExtentTracker::default();
        let mut formula_extent = ExtentTracker::default();
        let mut current_row: u32 = 0;
        let mut saw_begin_sheet = false;
        let mut saw_end_sheet = false;

        let mut ws_iter = Biff12Iter::new(&ws_bytes);
        while let Some((rec_id, payload)) = ws_iter.next_record()? {
            match rec_id {
                0x0081 => saw_begin_sheet = true,
                0x0082 => saw_end_sheet = true,
                0x0000 => {
                    let row = read_u32_le(payload, 0)
                        .ok_or_else(|| corrupted_xlsb("TruncatedBrtRowHdr"))?;
                    if row > MAX_XLSB_ROW {
                        return Err(corrupted_xlsb("XlsbRowOutOfBounds"));
                    }
                    current_row = row;
                }
                0x0001..=0x000B => {
                    let col = read_u32_le(payload, 0)
                        .ok_or_else(|| corrupted_xlsb("TruncatedBrtCellCol"))?;
                    let style_word = read_u32_le(payload, 4)
                        .ok_or_else(|| corrupted_xlsb("TruncatedBrtCellStyle"))?;
                    if col > MAX_XLSB_COL {
                        return Err(corrupted_xlsb("XlsbColOutOfBounds"));
                    }
                    let style_ref = style_word & 0x00FF_FFFF;
                    if let Some(fmt_id) = meta.cell_style_numfmt.get(style_ref as usize)
                        && let Some(fmt_code) = meta.number_formats.get(fmt_id)
                    {
                        let addr = coords_to_a1(current_row, col);
                        meta.cell_numfmt_by_addr
                            .insert((sheet.name.clone(), addr), fmt_code.clone());
                    }

                    let mut cell_val: Option<Data> = None;
                    let mut formula_rgce: Option<&[u8]> = None;

                    match rec_id {
                        0x0001 => {}
                        0x0002 => {
                            let rk = read_u32_le(payload, 8)
                                .ok_or_else(|| corrupted_xlsb("TruncatedBrtCellRk"))?;
                            cell_val = Some(decode_rk_value(rk));
                        }
                        0x0003 => {
                            let err = *payload
                                .get(8)
                                .ok_or_else(|| corrupted_xlsb("TruncatedBrtCellError"))?;
                            cell_val = Some(Data::Error(decode_xlsb_error(err)));
                        }
                        0x0004 => {
                            let b = *payload
                                .get(8)
                                .ok_or_else(|| corrupted_xlsb("TruncatedBrtCellBool"))?;
                            cell_val = Some(Data::Bool(b != 0));
                        }
                        0x0005 => {
                            let f = read_f64_le(payload, 8)
                                .ok_or_else(|| corrupted_xlsb("TruncatedBrtCellReal"))?;
                            cell_val = Some(Data::Float(f));
                        }
                        0x0006 => {
                            let (text, _) = read_xl_wide_string(payload, 8)
                                .ok_or_else(|| corrupted_xlsb("TruncatedBrtCellSt"))?;
                            cell_val = Some(Data::String(text));
                        }
                        0x0007 => {
                            let isst = read_u32_le(payload, 8)
                                .ok_or_else(|| corrupted_xlsb("TruncatedBrtCellIsst"))?;
                            let text = shared_strings
                                .get(isst as usize)
                                .ok_or_else(|| corrupted_xlsb("InvalidSharedStringIndex"))?;
                            cell_val = Some(Data::String(text.clone()));
                        }
                        0x0008 => {
                            let (text, consumed) = read_xl_wide_string(payload, 8)
                                .ok_or_else(|| corrupted_xlsb("TruncatedBrtFmlaString"))?;
                            cell_val = Some(if text.is_empty() {
                                Data::Empty
                            } else {
                                Data::String(text)
                            });
                            let f_off = 8usize
                                .checked_add(consumed)
                                .and_then(|v| v.checked_add(2))
                                .ok_or_else(|| corrupted_xlsb("BrtFmlaStringOffsetOverflow"))?;
                            let cce = read_u32_le(payload, f_off)
                                .and_then(|v| usize::try_from(v).ok())
                                .ok_or_else(|| corrupted_xlsb("TruncatedBrtFmlaStringLen"))?;
                            let start = f_off
                                .checked_add(4)
                                .ok_or_else(|| corrupted_xlsb("BrtFmlaStringOffsetOverflow"))?;
                            let end = start
                                .checked_add(cce)
                                .ok_or_else(|| corrupted_xlsb("BrtFmlaStringOverflow"))?;
                            formula_rgce = Some(
                                payload
                                    .get(start..end)
                                    .ok_or_else(|| corrupted_xlsb("TruncatedBrtFmlaStringRgce"))?,
                            );
                        }
                        0x0009 => {
                            let f = read_f64_le(payload, 8)
                                .ok_or_else(|| corrupted_xlsb("TruncatedBrtFmlaNum"))?;
                            cell_val = Some(Data::Float(f));
                            let cce = read_u32_le(payload, 18)
                                .and_then(|v| usize::try_from(v).ok())
                                .ok_or_else(|| corrupted_xlsb("TruncatedBrtFmlaNumLen"))?;
                            let end = 22usize
                                .checked_add(cce)
                                .ok_or_else(|| corrupted_xlsb("BrtFmlaNumOverflow"))?;
                            formula_rgce = Some(
                                payload
                                    .get(22..end)
                                    .ok_or_else(|| corrupted_xlsb("TruncatedBrtFmlaNumRgce"))?,
                            );
                        }
                        0x000A => {
                            let b = *payload
                                .get(8)
                                .ok_or_else(|| corrupted_xlsb("TruncatedBrtFmlaBool"))?;
                            cell_val = Some(Data::Bool(b != 0));
                            let cce = read_u32_le(payload, 11)
                                .and_then(|v| usize::try_from(v).ok())
                                .ok_or_else(|| corrupted_xlsb("TruncatedBrtFmlaBoolLen"))?;
                            let end = 15usize
                                .checked_add(cce)
                                .ok_or_else(|| corrupted_xlsb("BrtFmlaBoolOverflow"))?;
                            formula_rgce = Some(
                                payload
                                    .get(15..end)
                                    .ok_or_else(|| corrupted_xlsb("TruncatedBrtFmlaBoolRgce"))?,
                            );
                        }
                        0x000B => {
                            let err = *payload
                                .get(8)
                                .ok_or_else(|| corrupted_xlsb("TruncatedBrtFmlaError"))?;
                            cell_val = Some(Data::Error(decode_xlsb_error(err)));
                            let cce = read_u32_le(payload, 11)
                                .and_then(|v| usize::try_from(v).ok())
                                .ok_or_else(|| corrupted_xlsb("TruncatedBrtFmlaErrorLen"))?;
                            let end = 15usize
                                .checked_add(cce)
                                .ok_or_else(|| corrupted_xlsb("BrtFmlaErrorOverflow"))?;
                            formula_rgce = Some(
                                payload
                                    .get(15..end)
                                    .ok_or_else(|| corrupted_xlsb("TruncatedBrtFmlaErrorRgce"))?,
                            );
                        }
                        _ => {}
                    }

                    // Validate formula extent independently BEFORE pushing or materializing values,
                    // and preserve undecoded formulas explicitly so they never become LiteralValue.
                    if let Some(rgce) = formula_rgce {
                        formula_extent.observe(current_row, col, max_rows_limit)?;
                        let formula = decode_biff12_formula(rgce, &extern_sheets, &defined_names)
                            .unwrap_or_else(|| UNDECODED_BIFF12_FORMULA_MARKER.to_string());
                        formula_cells.push(Cell::new((current_row, col), formula));
                    }
                    if let Some(val) = cell_val {
                        value_extent.observe(current_row, col, max_rows_limit)?;
                        value_cells.push(Cell::new((current_row, col), val));
                    }
                }
                0x00B0 => {
                    let range = read_rfx_range(payload, 0)
                        .ok_or_else(|| corrupted_xlsb("TruncatedBrtMergeCell"))?;
                    if range.end_row > MAX_XLSB_ROW || range.end_col > MAX_XLSB_COL {
                        return Err(corrupted_xlsb("XlsbMergeCoordinateOutOfBounds"));
                    }
                    let merge_cells = u64::from(range.end_row - range.start_row)
                        .checked_add(1)
                        .and_then(|r| {
                            u64::from(range.end_col - range.start_col)
                                .checked_add(1)
                                .and_then(|c| r.checked_mul(c))
                        })
                        .ok_or_else(|| extent_limit_exceeded("XlsbMergeExtentOverflow"))?;
                    if merge_cells > MAX_MERGE_CELLS {
                        return Err(extent_limit_exceeded("XlsbMergeExtentLimitExceeded"));
                    }
                    meta.sheet_merges
                        .entry(sheet.name.clone())
                        .or_default()
                        .push(range);
                }
                0x0097 => {
                    if let Some(state_byte) = payload.get(28)
                        && (state_byte & 0x01) != 0
                    {
                        meta.sheet_frozen.insert(sheet.name.clone());
                    }
                }
                0x00A1 => {
                    let range = read_rfx_range(payload, 0)
                        .ok_or_else(|| corrupted_xlsb("TruncatedBrtBeginAFilter"))?;
                    meta.sheet_autofilter
                        .insert(sheet.name.clone(), range.to_a1_range());
                }
                0x0217 => {
                    if read_u32_le(payload, 2).is_some_and(|v| v != 0) {
                        meta.sheet_protected.insert(sheet.name.clone());
                    }
                }
                0x01EE => {
                    let range = read_rfx_range(payload, 0)
                        .ok_or_else(|| corrupted_xlsb("TruncatedBrtHLink"))?;
                    let (maybe_rid, _) = read_xl_nullable_wide_string(payload, 16)
                        .ok_or_else(|| corrupted_xlsb("TruncatedBrtHLinkRel"))?;
                    let link_val = maybe_rid
                        .and_then(|rid| ws_rels.get(&rid).cloned())
                        .unwrap_or_default();
                    if !link_val.is_empty() {
                        let addr = coords_to_a1(range.start_row, range.start_col);
                        meta.sheet_hyperlinks
                            .insert((sheet.name.clone(), addr), link_val);
                    }
                }
                _ => {}
            }
        }

        if saw_begin_sheet && !saw_end_sheet {
            return Err(corrupted_xlsb("TruncatedXlsbWorksheetStream"));
        }

        // Independent final extent validation immediately prior to `Range::from_sparse`.
        value_extent.validate(max_rows_limit)?;
        formula_extent.validate(max_rows_limit)?;

        let range = if value_cells.is_empty() {
            Range::empty()
        } else {
            Range::from_sparse(value_cells)
        };
        let formula_range = if formula_cells.is_empty() {
            None
        } else {
            Some(Range::from_sparse(formula_cells))
        };

        parsed_sheets.push(XlsbSheetParsed {
            name: sheet.name.clone(),
            range,
            formula_range,
        });
    }

    Ok(XlsbWorkbookParsed {
        meta,
        sheets: parsed_sheets,
        defined_names,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn biff12_iter_distinguishes_clean_eof_from_malformed_streams() -> Result<(), SafiError> {
        // Clean empty stream -> Ok(None)
        let mut empty = Biff12Iter::new(&[]);
        assert_eq!(empty.next_record()?, None);

        // Truncated 2-byte record ID (high bit set, missing second byte) -> Err(SF-IMP-002)
        let mut trunc_id = Biff12Iter::new(&[0x80]);
        assert_eq!(
            trunc_id.next_record().err().map(|e| e.code()),
            Some(ErrorCode::SF_IMP_002)
        );

        // Invalid 2-byte record ID (second byte also has continuation bit set) -> Err(SF-IMP-002)
        let mut bad_id = Biff12Iter::new(&[0x81, 0x80, 0x00]);
        assert_eq!(
            bad_id.next_record().err().map(|e| e.code()),
            Some(ErrorCode::SF_IMP_002)
        );

        // Truncated varint length -> Err(SF-IMP-002)
        let mut trunc_len = Biff12Iter::new(&[0x01, 0x80]);
        assert_eq!(
            trunc_len.next_record().err().map(|e| e.code()),
            Some(ErrorCode::SF_IMP_002)
        );

        // Invalid 4-byte non-terminating varint length -> Err(SF-IMP-002)
        let mut bad_len = Biff12Iter::new(&[0x01, 0x80, 0x80, 0x80, 0x80]);
        assert_eq!(
            bad_len.next_record().err().map(|e| e.code()),
            Some(ErrorCode::SF_IMP_002)
        );

        // Truncated payload (declares 10 bytes, only 3 present) -> Err(SF-IMP-002)
        let mut trunc_payload = Biff12Iter::new(&[0x01, 0x0A, 0xAA, 0xBB, 0xCC]);
        assert_eq!(
            trunc_payload.next_record().err().map(|e| e.code()),
            Some(ErrorCode::SF_IMP_002)
        );
        Ok(())
    }

    #[test]
    fn extent_tracker_rejects_opposite_corner_sparse_bomb_without_allocating() {
        let mut tracker = ExtentTracker::default();
        assert!(tracker.observe(0, 0, 1_000_000).is_ok());
        // Opposite Excel corner XFD1048576 -> 1,048,576 * 16,384 = 17,179,869,184 cells
        let err = tracker.observe(MAX_XLSB_ROW, MAX_XLSB_COL, 1_000_000);
        assert_eq!(err.err().map(|e| e.code()), Some(ErrorCode::SF_IMP_005));

        // Out-of-bounds coordinate beyond Excel limits -> SF-IMP-002
        let mut oob = ExtentTracker::default();
        assert_eq!(
            oob.observe(MAX_XLSB_ROW + 1, 0, 1_000_000)
                .err()
                .map(|e| e.code()),
            Some(ErrorCode::SF_IMP_002)
        );
        assert_eq!(
            oob.observe(0, MAX_XLSB_COL + 1, 1_000_000)
                .err()
                .map(|e| e.code()),
            Some(ErrorCode::SF_IMP_002)
        );
    }
}
