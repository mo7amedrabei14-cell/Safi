//! Panic-free native MS-XLSB (`BIFF12`) binary workbook parser (`crates/sf-io`).
//!
//! Reads ZIP-packaged BIFF12 parts (`xl/workbook.bin`, `xl/sharedStrings.bin`, `xl/styles.bin`,
//! `xl/worksheets/*.bin`, `xl/comments*.bin`, and relationship XMLs) with full bounds checking,
//! decompression-budget enforcement (`SF-IMP-005`), and zero `.unwrap()` calls.

use std::io::Cursor;

use calamine::{Cell, CellErrorType, Data, Range};
use sf_core::error::{ErrorCode, ErrorSeverity, SafiError};
use zip::ZipArchive;

use crate::workbook_intelligence::{
    DefinedNameRecord, EmbeddedObjectHandling, EmbeddedObjectKind, EmbeddedObjectRecord,
    OoxmlMetadata, SheetRange, SheetVisibility, WorkbookDateSystem, check_zip_budget_and_scan,
    coords_to_a1, parse_rels_xml_map, read_zip_part_budgeted, seed_builtin_number_formats,
};

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

struct Biff12Iter<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Biff12Iter<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn next_record(&mut self) -> Option<(u16, &'a [u8])> {
        let b0 = *self.data.get(self.pos)?;
        self.pos += 1;
        let rec_id = if (b0 & 0x80) != 0 {
            let b1 = *self.data.get(self.pos)?;
            self.pos += 1;
            u16::from(b0 & 0x7F) | (u16::from(b1 & 0x7F) << 7)
        } else {
            u16::from(b0)
        };

        let mut len: usize = 0;
        for shift in 0..4usize {
            let lb = *self.data.get(self.pos)?;
            self.pos += 1;
            len |= usize::from(lb & 0x7F) << (7 * shift);
            if (lb & 0x80) == 0 {
                break;
            }
        }

        let end = self.pos.checked_add(len)?;
        let payload = self.data.get(self.pos..end)?;
        self.pos = end;
        Some((rec_id, payload))
    }
}

fn read_u16_le(slice: &[u8], offset: usize) -> Option<u16> {
    let bytes = slice.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32_le(slice: &[u8], offset: usize) -> Option<u32> {
    let bytes = slice.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_i32_le(slice: &[u8], offset: usize) -> Option<i32> {
    let bytes = slice.get(offset..offset + 4)?;
    Some(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_f64_le(slice: &[u8], offset: usize) -> Option<f64> {
    let bytes = slice.get(offset..offset + 8)?;
    Some(f64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ]))
}

fn decode_utf16le(bytes: &[u8]) -> String {
    let (decoded, _, _) = encoding_rs::UTF_16LE.decode(bytes);
    decoded.into_owned()
}

fn read_xl_wide_string(slice: &[u8], offset: usize) -> Option<(String, usize)> {
    let cch = read_u32_le(slice, offset)? as usize;
    let byte_len = cch.checked_mul(2)?;
    let start = offset.checked_add(4)?;
    let end = start.checked_add(byte_len)?;
    let raw = slice.get(start..end)?;
    Some((decode_utf16le(raw), 4 + byte_len))
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
    let r2 = read_u32_le(slice, offset + 4)?;
    let c1 = read_u32_le(slice, offset + 8)?;
    let c2 = read_u32_le(slice, offset + 12)?;
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
                let start = pos + 2;
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
                if pos + skip > rgce.len() {
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
                    if pos + 2 > rgce.len() {
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
                if pos + 14 > rgce.len() {
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
                if pos + 6 > rgce.len() {
                    return None;
                }
                pos += 6;
                stack.push("#REF!".to_string());
            }
            0x2B | 0x4B | 0x6B => {
                if pos + 12 > rgce.len() {
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

fn parse_styles_bin(bytes: &[u8], meta: &mut OoxmlMetadata) {
    seed_builtin_number_formats(meta);
    let mut iter = Biff12Iter::new(bytes);
    let mut in_cell_xfs = false;
    while let Some((rec_id, payload)) = iter.next_record() {
        match rec_id {
            0x002C => {
                if let Some(ifmt) = read_u16_le(payload, 0)
                    && let Some((code, _)) = read_xl_wide_string(payload, 2)
                {
                    meta.number_formats.insert(u32::from(ifmt), code);
                }
            }
            0x0269 => in_cell_xfs = true,
            0x026A => in_cell_xfs = false,
            0x002F if in_cell_xfs => {
                let ifmt = read_u16_le(payload, 2).unwrap_or(0);
                meta.cell_style_numfmt.push(u32::from(ifmt));
            }
            _ => {}
        }
    }
}

fn parse_shared_strings_bin(bytes: &[u8]) -> Vec<String> {
    let mut strings = Vec::new();
    let mut iter = Biff12Iter::new(bytes);
    while let Some((rec_id, payload)) = iter.next_record() {
        if rec_id == 0x0013
            && payload.len() >= 5
            && let Some((text, _)) = read_xl_wide_string(payload, 1)
        {
            strings.push(text);
        }
    }
    strings
}

fn parse_comments_bin(bytes: &[u8], sheet_name: &str, meta: &mut OoxmlMetadata) {
    let mut iter = Biff12Iter::new(bytes);
    let mut active_ref: Option<String> = None;
    while let Some((rec_id, payload)) = iter.next_record() {
        match rec_id {
            0x027B => {
                // BrtBeginComment: authorId(4) + rfx(16)
                if let Some(range) = read_rfx_range(payload, 4) {
                    active_ref = Some(coords_to_a1(range.start_row, range.start_col));
                }
            }
            0x027D => {
                // BrtCommentText: richStr flags(1) + XLWideString
                if let Some(cell_ref) = active_ref.as_ref()
                    && payload.len() >= 5
                    && let Some((text, _)) = read_xl_wide_string(payload, 1)
                {
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
}

pub(crate) fn parse_xlsb_package(
    bytes: &[u8],
    file_bytes: u64,
) -> Result<XlsbWorkbookParsed, SafiError> {
    let mut cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(&mut cursor).map_err(|_| {
        SafiError::new(
            ErrorCode::SF_IMP_002,
            ErrorSeverity::Error,
            "CorruptedExcelArchive",
            "import.excel_corrupted",
        )
    })?;

    let (budget, mut total_uncompressed, mut meta, _file_names) =
        check_zip_budget_and_scan(&mut archive, file_bytes)?;

    if let Some(styles_bytes) = read_zip_part_budgeted(
        &mut archive,
        "xl/styles.bin",
        budget,
        &mut total_uncompressed,
    )? {
        parse_styles_bin(&styles_bytes, &mut meta);
    }

    let shared_strings = if let Some(sst_bytes) = read_zip_part_budgeted(
        &mut archive,
        "xl/sharedStrings.bin",
        budget,
        &mut total_uncompressed,
    )? {
        parse_shared_strings_bin(&sst_bytes)
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
    .ok_or_else(|| {
        SafiError::new(
            ErrorCode::SF_IMP_002,
            ErrorSeverity::Error,
            "MissingXlsbWorkbookStream",
            "import.excel_corrupted",
        )
    })?;

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
    while let Some((rec_id, payload)) = wb_iter.next_record() {
        match rec_id {
            0x0099 => {
                if payload.first().is_some_and(|b| (b & 0x01) != 0) {
                    meta.date_system = Some(WorkbookDateSystem::Macintosh1904);
                }
            }
            0x009C => {
                if let Some(hs_state) = read_u32_le(payload, 0)
                    && let Some((maybe_rel, rel_bytes)) = read_xl_nullable_wide_string(payload, 8)
                    && let Some((name, _)) = read_xl_wide_string(payload, 8 + rel_bytes)
                {
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
            }
            0x0165 => sup_books.push(SupBookKind::SelfWorkbook),
            0x0163 | 0x0164 | 0x0166 | 0x0167 => {
                let rel_label = read_xl_wide_string(payload, 0)
                    .map(|(s, _)| s)
                    .unwrap_or_else(|| "ExternalWorkbook".to_string());
                sup_books.push(SupBookKind::External(rel_label));
            }
            0x016A => {
                if let Some(cxti) = read_u32_le(payload, 0) {
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
            }
            0x0027 => {
                if let Some(flags) = read_u32_le(payload, 0)
                    && let Some((name, str_len)) = read_xl_wide_string(payload, 9)
                {
                    let is_built_in = (flags & 0x0020) != 0 || name.starts_with("_xlnm.");
                    let rgce_start = 9 + str_len + 4;
                    let rgce_len = read_u32_le(payload, 9 + str_len)
                        .and_then(|v| usize::try_from(v).ok())
                        .unwrap_or(0);
                    let rgce = payload
                        .get(rgce_start..rgce_start.saturating_add(rgce_len))
                        .unwrap_or(&[])
                        .to_vec();
                    pending_names.push((name, is_built_in, rgce));
                }
            }
            _ => {}
        }
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
                parse_comments_bin(&c_bytes, &sheet.name, &mut meta);
            }
        }

        let Some(ws_bytes) = read_zip_part_budgeted(
            &mut archive,
            &sheet.zip_path,
            budget,
            &mut total_uncompressed,
        )?
        else {
            parsed_sheets.push(XlsbSheetParsed {
                name: sheet.name.clone(),
                range: Range::empty(),
                formula_range: None,
            });
            continue;
        };

        let mut value_cells: Vec<Cell<Data>> = Vec::new();
        let mut formula_cells: Vec<Cell<String>> = Vec::new();
        let mut current_row: u32 = 0;

        let mut ws_iter = Biff12Iter::new(&ws_bytes);
        while let Some((rec_id, payload)) = ws_iter.next_record() {
            match rec_id {
                0x0000 => {
                    if let Some(row) = read_u32_le(payload, 0) {
                        current_row = row;
                    }
                }
                0x0001..=0x000B => {
                    let Some(col) = read_u32_le(payload, 0) else {
                        continue;
                    };
                    let style_ref = read_u32_le(payload, 4).unwrap_or(0) & 0x00FF_FFFF;
                    if let Some(fmt_id) = meta.cell_style_numfmt.get(style_ref as usize)
                        && let Some(fmt_code) = meta.number_formats.get(fmt_id)
                    {
                        let addr = coords_to_a1(current_row, col);
                        meta.cell_numfmt_by_addr
                            .insert((sheet.name.clone(), addr), fmt_code.clone());
                    }

                    let mut cell_val: Option<Data> = None;
                    let mut rgce_slice: Option<&[u8]> = None;

                    match rec_id {
                        0x0001 => {}
                        0x0002 => {
                            if let Some(rk) = read_u32_le(payload, 8) {
                                cell_val = Some(decode_rk_value(rk));
                            }
                        }
                        0x0003 => {
                            if let Some(&err) = payload.get(8) {
                                cell_val = Some(Data::Error(decode_xlsb_error(err)));
                            }
                        }
                        0x0004 => {
                            if let Some(&b) = payload.get(8) {
                                cell_val = Some(Data::Bool(b != 0));
                            }
                        }
                        0x0005 => {
                            if let Some(f) = read_f64_le(payload, 8) {
                                cell_val = Some(Data::Float(f));
                            }
                        }
                        0x0006 => {
                            if let Some((text, _)) = read_xl_wide_string(payload, 8) {
                                cell_val = Some(Data::String(text));
                            }
                        }
                        0x0007 => {
                            if let Some(isst) = read_u32_le(payload, 8)
                                && let Some(text) = shared_strings.get(isst as usize)
                            {
                                cell_val = Some(Data::String(text.clone()));
                            }
                        }
                        0x0008 => {
                            if let Some((text, consumed)) = read_xl_wide_string(payload, 8) {
                                if !text.is_empty() {
                                    cell_val = Some(Data::String(text));
                                }
                                let f_off = 8 + consumed + 2;
                                if let Some(cce) = read_u32_le(payload, f_off) {
                                    let end = f_off + 4 + (cce as usize);
                                    rgce_slice = payload.get(f_off + 4..end);
                                }
                            }
                        }
                        0x0009 => {
                            if let Some(f) = read_f64_le(payload, 8) {
                                cell_val = Some(Data::Float(f));
                            }
                            if let Some(cce) = read_u32_le(payload, 18) {
                                let end = 22 + (cce as usize);
                                rgce_slice = payload.get(22..end);
                            }
                        }
                        0x000A => {
                            if let Some(&b) = payload.get(8) {
                                cell_val = Some(Data::Bool(b != 0));
                            }
                            if let Some(cce) = read_u32_le(payload, 11) {
                                let end = 15 + (cce as usize);
                                rgce_slice = payload.get(15..end);
                            }
                        }
                        0x000B => {
                            if let Some(&err) = payload.get(8) {
                                cell_val = Some(Data::Error(decode_xlsb_error(err)));
                            }
                            if let Some(cce) = read_u32_le(payload, 11) {
                                let end = 15 + (cce as usize);
                                rgce_slice = payload.get(15..end);
                            }
                        }
                        _ => {}
                    }

                    if let Some(val) = cell_val {
                        value_cells.push(Cell::new((current_row, col), val));
                    }
                    if let Some(rgce) = rgce_slice
                        && let Some(formula) =
                            decode_biff12_formula(rgce, &extern_sheets, &defined_names)
                    {
                        formula_cells.push(Cell::new((current_row, col), formula));
                    }
                }
                0x00B0 => {
                    if let Some(range) = read_rfx_range(payload, 0) {
                        meta.sheet_merges
                            .entry(sheet.name.clone())
                            .or_default()
                            .push(range);
                    }
                }
                0x0097 => {
                    if let Some(state_byte) = payload.get(28)
                        && (state_byte & 0x01) != 0
                    {
                        meta.sheet_frozen.insert(sheet.name.clone());
                    }
                }
                0x00A1 => {
                    if let Some(range) = read_rfx_range(payload, 0) {
                        meta.sheet_autofilter
                            .insert(sheet.name.clone(), range.to_a1_range());
                    }
                }
                0x0217 => {
                    if read_u32_le(payload, 2).is_some_and(|v| v != 0) {
                        meta.sheet_protected.insert(sheet.name.clone());
                    }
                }
                0x01EE => {
                    if let Some(range) = read_rfx_range(payload, 0)
                        && let Some((maybe_rid, _)) = read_xl_nullable_wide_string(payload, 16)
                    {
                        let link_val = maybe_rid
                            .and_then(|rid| ws_rels.get(&rid).cloned())
                            .unwrap_or_default();
                        if !link_val.is_empty() {
                            let addr = coords_to_a1(range.start_row, range.start_col);
                            meta.sheet_hyperlinks
                                .insert((sheet.name.clone(), addr), link_val);
                        }
                    }
                }
                _ => {}
            }
        }

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
