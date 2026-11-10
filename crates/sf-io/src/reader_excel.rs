//! Excel (`XLSX`, `XLS`, `ODS`, `XLSM`) and spreadsheet reader using `calamine`
//! adhering to Specification Sections 4.2 and 8.2 (Zip Bomb defense `SF-IMP-005`, merged cells unmerging).

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Seek};

use calamine::{Data, Dimensions, Reader, Xlsx, open_workbook_from_rs};
use quick_xml::{
    Reader as XmlReader,
    events::{BytesEnd, BytesStart, Event},
};
use zip::ZipArchive;

use sf_core::column::{Column, ColumnSchema, ColumnType, ConfidenceScore};
use sf_core::error::{ErrorCode, ErrorSeverity, SafiError};
use sf_core::limits::PlatformLimits;
use sf_core::table::{Sheet, Table};
use sf_core::value::CellValue;

/// Maximum allowable decompression ratio (uncompressed size / compressed size)
/// to protect against zip bomb attacks (Specification Section 8.2 #1 -> SF-IMP-005).
pub const MAX_DECOMPRESSION_RATIO: u64 = 100;

/// Maximum uncompressed spreadsheet size in bytes (`500 MB`).
pub const MAX_UNCOMPRESSED_BYTES: u64 = 500 * 1024 * 1024;

const MAX_XLSX_ROWS: u32 = 1_048_576;
const MAX_XLSX_COLUMNS: u32 = 16_384;

fn invalid_workbook() -> SafiError {
    SafiError::new(
        ErrorCode::SF_IMP_001,
        ErrorSeverity::Error,
        "InvalidWorkbookMetadata",
        "import.failed",
    )
}

fn oversized_workbook() -> SafiError {
    SafiError::new(
        ErrorCode::SF_IMP_005,
        ErrorSeverity::Error,
        "ZipBombLimitExceeded",
        "import.security_rejected",
    )
}

// Bound actual reads as well as ZIP declarations, sharing the budget across parts.
fn read_xml_part<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
    remaining: &mut u64,
) -> Result<Vec<u8>, SafiError> {
    let part = archive.by_name(name).map_err(|_| invalid_workbook())?;
    if part.size() > *remaining {
        return Err(oversized_workbook());
    }
    let mut bytes = Vec::new();
    let result = part.take(*remaining + 1).read_to_end(&mut bytes);
    if bytes.len() as u64 > *remaining {
        return Err(oversized_workbook());
    }
    result.map_err(|_| invalid_workbook())?;
    *remaining -= bytes.len() as u64;
    Ok(bytes)
}

fn visit_xml(
    bytes: &[u8],
    visit: impl FnMut(&BytesStart<'_>, &XmlReader<&[u8]>) -> Result<(), SafiError>,
) -> Result<(), SafiError> {
    visit_xml_with_end(bytes, visit, |_| Ok(()))
}

fn visit_xml_with_end(
    bytes: &[u8],
    mut visit: impl FnMut(&BytesStart<'_>, &XmlReader<&[u8]>) -> Result<(), SafiError>,
    mut end: impl FnMut(&BytesEnd<'_>) -> Result<(), SafiError>,
) -> Result<(), SafiError> {
    let mut xml = XmlReader::from_reader(bytes);
    xml.check_comments(true).expand_empty_elements(true);
    let mut depth = 0usize;
    let mut root_seen = false;
    loop {
        match xml.read_event().map_err(|_| invalid_workbook())? {
            Event::Start(element) => {
                if depth == 0 {
                    if root_seen {
                        return Err(invalid_workbook());
                    }
                    root_seen = true;
                }
                // Keep quick-xml's duplicate-attribute and entity checks enabled.
                for attribute in element.attributes() {
                    attribute
                        .map_err(|_| invalid_workbook())?
                        .decode_and_unescape_value(&xml)
                        .map_err(|_| invalid_workbook())?;
                }
                visit(&element, &xml)?;
                depth += 1;
            }
            Event::End(element) => {
                end(&element)?;
                depth = depth.checked_sub(1).ok_or_else(invalid_workbook)?;
            }
            Event::Text(text) => {
                let text = text.unescape().map_err(|_| invalid_workbook())?;
                if depth == 0 && !text.trim().is_empty() {
                    return Err(invalid_workbook());
                }
            }
            Event::CData(_) if depth == 0 => return Err(invalid_workbook()),
            Event::Eof => {
                return if root_seen && depth == 0 {
                    Ok(())
                } else {
                    Err(invalid_workbook())
                };
            }
            _ => {}
        }
    }
}

fn parse_cell_reference(reference: &str) -> Result<(u32, u32), SafiError> {
    let column_len = reference
        .bytes()
        .take_while(u8::is_ascii_alphabetic)
        .count();
    if !(1..=3).contains(&column_len) {
        return Err(invalid_workbook());
    }
    let (column, row) = reference.split_at(column_len);
    if row.is_empty() || row.len() > 9 || !row.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid_workbook());
    }
    let column = column.bytes().fold(0u32, |value, letter| {
        value * 26 + u32::from(letter.to_ascii_uppercase() - b'A') + 1
    });
    let row: u32 = row.parse().map_err(|_| invalid_workbook())?;
    if row == 0 || row > MAX_XLSX_ROWS || column > MAX_XLSX_COLUMNS {
        return Err(invalid_workbook());
    }
    Ok((row - 1, column - 1))
}

// Calamine's merge parser neither decodes XML entities nor checks coordinate arithmetic.
fn parse_dimension_reference(reference: &str) -> Result<Dimensions, SafiError> {
    let mut parts = reference.split(':');
    let start = parse_cell_reference(parts.next().ok_or_else(invalid_workbook)?)?;
    let end = parts
        .next()
        .map(parse_cell_reference)
        .transpose()?
        .unwrap_or(start);
    if parts.next().is_some() {
        return Err(invalid_workbook());
    }
    validate_bounds(start, end)?;
    Ok(Dimensions { start, end })
}

fn check_sparse_bounds(
    cells: &mut Option<(u32, u32, u32, u32)>,
    (row, column): (u32, u32),
) -> Result<(), SafiError> {
    let (min_row, min_col, max_row, max_col) = cells.map_or(
        (row, column, row, column),
        |(min_row, min_col, max_row, max_col)| {
            (
                min_row.min(row),
                min_col.min(column),
                max_row.max(row),
                max_col.max(column),
            )
        },
    );
    let count = (u64::from(max_row - min_row) + 1) * (u64::from(max_col - min_col) + 1);
    if count.checked_mul(32).ok_or_else(oversized_workbook)? > MAX_UNCOMPRESSED_BYTES {
        return Err(oversized_workbook());
    }
    *cells = Some((min_row, min_col, max_row, max_col));
    Ok(())
}

fn reject_encoded_references(bytes: &[u8]) -> Result<(), SafiError> {
    let mut xml = XmlReader::from_reader(bytes);
    xml.expand_empty_elements(true);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match xml
            .read_event_into(&mut buf)
            .map_err(|_| invalid_workbook())?
        {
            Event::Start(element) => {
                for attribute in element.attributes() {
                    let attribute = attribute.map_err(|_| invalid_workbook())?;
                    if attribute.key.local_name().as_ref() == b"ref"
                        && attribute.value.windows(2).any(|w| w == b"&#")
                    {
                        return Err(invalid_workbook());
                    }
                }
            }
            Event::Eof => return Ok(()),
            _ => {}
        }
    }
}

struct WorkbookPreflight {
    merges: BTreeMap<String, Vec<Dimensions>>,
    decompressed_bytes: u64,
}

fn preflight_workbook<R: Read + Seek>(
    reader: &mut R,
    file_bytes: u64,
) -> Result<WorkbookPreflight, SafiError> {
    const WORKSHEET_TYPE: &str =
        "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml";
    let mut archive = ZipArchive::new(reader).map_err(|_| invalid_workbook())?;
    let budget = if file_bytes == 0 {
        MAX_UNCOMPRESSED_BYTES
    } else {
        MAX_UNCOMPRESSED_BYTES.min(file_bytes.saturating_mul(MAX_DECOMPRESSION_RATIO))
    };
    let mut remaining = budget;
    let mut overrides = BTreeMap::new();
    let mut extensions = BTreeSet::new();
    {
        let bytes = read_xml_part(&mut archive, "[Content_Types].xml", &mut remaining)?;
        visit_xml(&bytes, |element, xml| {
            let mut part = None;
            let mut extension = None;
            let mut worksheet = false;
            for attribute in element.attributes() {
                let attribute = attribute.map_err(|_| invalid_workbook())?;
                let value = attribute
                    .decode_and_unescape_value(xml)
                    .map_err(|_| invalid_workbook())?;
                match attribute.key.as_ref() {
                    b"PartName" => part = Some(value.into_owned()),
                    b"Extension" => extension = Some(value.into_owned()),
                    b"ContentType" => worksheet = value == WORKSHEET_TYPE,
                    _ => {}
                }
            }
            match element.local_name().as_ref() {
                b"Override" => {
                    let part = part.ok_or_else(invalid_workbook)?;
                    let part = part.strip_prefix('/').ok_or_else(invalid_workbook)?;
                    overrides.insert(part.to_owned(), worksheet);
                }
                b"Default" if worksheet => {
                    extensions.insert(extension.ok_or_else(invalid_workbook)?);
                }
                _ => {}
            }
            Ok(())
        })?;
    }
    let mut parts: BTreeSet<String> = overrides
        .iter()
        .filter(|(_, worksheet)| **worksheet)
        .map(|(part, _)| part.clone())
        .collect();
    for name in archive.file_names() {
        if !overrides.contains_key(name)
            && name
                .rsplit_once('.')
                .is_some_and(|(_, ext)| extensions.contains(ext))
        {
            parts.insert(name.to_owned());
        }
    }
    let metadata_parts: Vec<_> = archive
        .file_names()
        .filter(|name| {
            name.eq_ignore_ascii_case("xl/sharedStrings.xml")
                || name.eq_ignore_ascii_case("xl/styles.xml")
        })
        .map(str::to_owned)
        .collect();
    for name in metadata_parts {
        let bytes = read_xml_part(&mut archive, &name, &mut remaining)?;
        visit_xml(&bytes, |_, _| Ok(()))?;
        reject_encoded_references(&bytes)?;
    }
    let mut sheet_parts = BTreeMap::new();
    if archive.file_names().any(|name| name == "xl/workbook.xml") {
        let mut relationships = BTreeMap::new();
        let bytes = read_xml_part(&mut archive, "xl/_rels/workbook.xml.rels", &mut remaining)?;
        visit_xml(&bytes, |element, xml| {
            if element.local_name().as_ref() != b"Relationship" {
                return Ok(());
            }
            let mut id = None;
            let mut target = None;
            for attribute in element.attributes() {
                let attribute = attribute.map_err(|_| invalid_workbook())?;
                match attribute.key.as_ref() {
                    b"Id" => id = Some(attribute.value.to_vec()),
                    b"Target" => {
                        target = Some(
                            xml.decoder()
                                .decode(&attribute.value)
                                .map_err(|_| invalid_workbook())?
                                .into_owned(),
                        )
                    }
                    _ => {}
                }
            }
            relationships.insert(
                id.ok_or_else(invalid_workbook)?,
                target.ok_or_else(invalid_workbook)?,
            );
            Ok(())
        })?;
        let bytes = read_xml_part(&mut archive, "xl/workbook.xml", &mut remaining)?;
        visit_xml(&bytes, |element, xml| {
            if element.local_name().as_ref() != b"sheet" {
                return Ok(());
            }
            let mut name = None;
            let mut id = None;
            for attribute in element.attributes() {
                let attribute = attribute.map_err(|_| invalid_workbook())?;
                match attribute.key.as_ref() {
                    b"name" => {
                        name = Some(
                            attribute
                                .decode_and_unescape_value(xml)
                                .map_err(|_| invalid_workbook())?
                                .into_owned(),
                        )
                    }
                    b"r:id" | b"relationships:id" => id = Some(attribute.value.to_vec()),
                    _ => {}
                }
            }
            let target = relationships
                .get(&id.ok_or_else(invalid_workbook)?)
                .ok_or_else(invalid_workbook)?;
            // Mirror calamine's paths and raw relationship IDs, not untrusted ContentType labels.
            let path = if target.starts_with("/xl/") {
                target[1..].to_owned()
            } else if target.starts_with("xl/") {
                target.clone()
            } else {
                format!("xl/{target}")
            };
            if !matches!(
                path.split('/').nth(1),
                Some("worksheets" | "chartsheets" | "dialogsheets")
            ) {
                return Ok(());
            }
            parts.insert(path.clone());
            if sheet_parts
                .insert(name.ok_or_else(invalid_workbook)?, path)
                .is_some()
            {
                return Err(invalid_workbook());
            }
            Ok(())
        })?;
    }
    let worksheet_parts: BTreeSet<String> = parts.iter().cloned().collect();
    let mut part_merges = BTreeMap::new();
    for part in parts {
        let bytes = read_xml_part(&mut archive, &part, &mut remaining)?;
        let mut merges = Vec::new();
        let mut cells = None;
        let row_index = Cell::new(0u32);
        let col_index = Cell::new(0u32);
        visit_xml_with_end(
            &bytes,
            |element, xml| {
                match element.local_name().as_ref() {
                    b"dimension" => {
                        let reference = element
                            .try_get_attribute("ref")
                            .map_err(|_| invalid_workbook())?
                            .ok_or_else(invalid_workbook)?;
                        if reference.value.windows(2).any(|w| w == b"&#") {
                            return Err(invalid_workbook());
                        }
                        let reference = reference
                            .decode_and_unescape_value(xml)
                            .map_err(|_| invalid_workbook())?;
                        let dimensions = parse_dimension_reference(&reference)?;
                        check_sparse_bounds(&mut cells, dimensions.start)?;
                        check_sparse_bounds(&mut cells, dimensions.end)?;
                    }
                    b"mergeCell" => {
                        let reference = element
                            .try_get_attribute("ref")
                            .map_err(|_| invalid_workbook())?
                            .ok_or_else(invalid_workbook)?
                            .decode_and_unescape_value(xml)
                            .map_err(|_| invalid_workbook())?;
                        merges.push(parse_dimension_reference(&reference)?);
                    }
                    b"row" => {
                        if let Some(reference) = element
                            .try_get_attribute("r")
                            .map_err(|_| invalid_workbook())?
                        {
                            let reference = reference
                                .decode_and_unescape_value(xml)
                                .map_err(|_| invalid_workbook())?;
                            let row = reference.parse::<u32>().map_err(|_| invalid_workbook())?;
                            if row == 0 || row > MAX_XLSX_ROWS {
                                return Err(invalid_workbook());
                            }
                            row_index.set(row - 1);
                        }
                        if row_index.get() >= MAX_XLSX_ROWS {
                            return Err(invalid_workbook());
                        }
                        col_index.set(0);
                    }
                    b"c" => {
                        let position = if let Some(reference) = element
                            .try_get_attribute("r")
                            .map_err(|_| invalid_workbook())?
                        {
                            let reference = reference
                                .decode_and_unescape_value(xml)
                                .map_err(|_| invalid_workbook())?;
                            let (row, col) = parse_cell_reference(&reference)?;
                            row_index.set(row);
                            (row, col)
                        } else {
                            (row_index.get(), col_index.get())
                        };
                        check_sparse_bounds(&mut cells, position)?;
                        col_index.set(position.1 + 1);
                    }
                    _ => {}
                }
                Ok(())
            },
            |element| {
                if element.local_name().as_ref() == b"row" {
                    row_index.set(
                        row_index
                            .get()
                            .checked_add(1)
                            .ok_or_else(invalid_workbook)?,
                    );
                    col_index.set(0);
                }
                Ok(())
            },
        )?;
        part_merges.insert(part, merges);
    }
    // Calamine reads shared strings and styles itself; an entity-encoded coordinate reference in
    // those parts cannot be validated against the cells we see, so the package is refused.
    let metadata_parts: Vec<String> = archive
        .file_names()
        .filter(|name| {
            name.ends_with(".xml")
                && !name.starts_with('[')
                && !name.contains("_rels/")
                && !worksheet_parts.contains(*name)
        })
        .map(str::to_owned)
        .collect();
    for part in metadata_parts {
        let bytes = read_xml_part(&mut archive, &part, &mut remaining)?;
        reject_encoded_references(&bytes)?;
    }
    let merges = sheet_parts
        .into_iter()
        .map(|(name, part)| {
            Ok::<_, SafiError>((
                name,
                part_merges.get(&part).ok_or_else(invalid_workbook)?.clone(),
            ))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    Ok(WorkbookPreflight {
        merges,
        decompressed_bytes: budget - remaining,
    })
}

fn validate_bounds(start: (u32, u32), end: (u32, u32)) -> Result<(), SafiError> {
    if start.0 > end.0 || start.1 > end.1 || end.0 >= MAX_XLSX_ROWS || end.1 >= MAX_XLSX_COLUMNS {
        return Err(invalid_workbook());
    }
    Ok(())
}

fn estimate_expanded_sheet_bytes(
    range: &calamine::Range<Data>,
    merges: &[Dimensions],
    cell_count: u64,
) -> Result<u64, SafiError> {
    let mut estimated = cell_count.checked_mul(32).ok_or_else(oversized_workbook)?;
    if estimated > MAX_UNCOMPRESSED_BYTES {
        return Err(oversized_workbook());
    }
    let mut charge = |value: &Data, count: u64| -> Result<(), SafiError> {
        let length = match value {
            Data::String(text) | Data::DateTimeIso(text) | Data::DurationIso(text) => {
                text.len() as u64
            }
            _ => 0,
        };
        // Grid plus raw/typed text in both row and column buffers coexist during conversion.
        let bytes = length
            .checked_mul(count)
            .and_then(|bytes| bytes.checked_mul(5))
            .ok_or_else(oversized_workbook)?;
        estimated = estimated
            .checked_add(bytes)
            .ok_or_else(oversized_workbook)?;
        if estimated > MAX_UNCOMPRESSED_BYTES {
            return Err(oversized_workbook());
        }
        Ok(())
    };
    for (_, _, value) in range.used_cells() {
        charge(value, 1)?;
    }
    for merge in merges {
        if let Some(value) = range.get_value(merge.start) {
            let area = (u64::from(merge.end.0 - merge.start.0) + 1)
                * (u64::from(merge.end.1 - merge.start.1) + 1);
            charge(value, area)?;
        }
    }
    Ok(estimated)
}

/// Reads an Excel/ODS workbook from a seekable stream, enforcing zip bomb defense (`SF-IMP-005`),
/// platform capacity limits (`SF-IMP-003`), and unmerging merged cells.
pub fn read_excel_stream<R: Read + Seek>(
    mut reader: R,
    file_bytes: u64,
    limits: PlatformLimits,
) -> Result<Vec<Sheet>, SafiError> {
    // 1. Enforce platform file size limit (SF-IMP-003)
    if file_bytes > limits.max_file_bytes {
        return Err(SafiError::new(
            ErrorCode::SF_IMP_003,
            ErrorSeverity::Error,
            "PlatformFileSizeLimitExceeded",
            "import.limit_exceeded",
        ));
    }

    let preflight = preflight_workbook(&mut reader, file_bytes)?;
    reader.rewind().map_err(|_| invalid_workbook())?;
    let mut workbook = open_workbook_from_rs::<Xlsx<_>, _>(&mut reader).map_err(|_| {
        SafiError::new(
            ErrorCode::SF_IMP_001,
            ErrorSeverity::Error,
            "CouldNotOpenWorkbook",
            "import.failed",
        )
    })?;
    let sheet_names = workbook.sheet_names().to_owned();
    if sheet_names.is_empty() {
        return Err(SafiError::new(
            ErrorCode::SF_IMP_001,
            ErrorSeverity::Error,
            "EmptyWorkbookSheets",
            "import.failed",
        ));
    }

    let mut sheets = Vec::new();
    let mut estimated_workbook_bytes = preflight.decompressed_bytes;

    for (sheet_idx, sheet_name) in sheet_names.into_iter().enumerate() {
        let range = workbook
            .worksheet_range(&sheet_name)
            .map_err(|_| invalid_workbook())?;
        let merges = preflight
            .merges
            .get(&sheet_name)
            .ok_or_else(invalid_workbook)?;
        let mut bounds = range.start().zip(range.end());
        if let Some((start, end)) = bounds {
            validate_bounds(start, end)?;
        }
        for merge in merges {
            bounds = Some(match bounds {
                Some((start, end)) => (
                    (start.0.min(merge.start.0), start.1.min(merge.start.1)),
                    (end.0.max(merge.end.0), end.1.max(merge.end.1)),
                ),
                None => (merge.start, merge.end),
            });
        }
        let Some((start, end)) = bounds else {
            continue;
        };
        // Use u64 so the full XLSX rectangle cannot overflow on 32-bit/WASM targets.
        let height = u64::from(end.0 - start.0) + 1;
        let width = u64::from(end.1 - start.1) + 1;

        // Enforce max rows limit, including the header (SF-IMP-003).
        if height > limits.max_rows {
            return Err(SafiError::new(
                ErrorCode::SF_IMP_003,
                ErrorSeverity::Error,
                "PlatformRowCountLimitExceeded",
                "import.limit_exceeded",
            ));
        }

        let estimated = estimate_expanded_sheet_bytes(&range, merges, height * width)?;
        estimated_workbook_bytes = estimated_workbook_bytes
            .checked_add(estimated)
            .ok_or_else(oversized_workbook)?;
        if estimated_workbook_bytes > MAX_UNCOMPRESSED_BYTES {
            return Err(oversized_workbook());
        }

        if file_bytes > 0 && (estimated_workbook_bytes / file_bytes) > MAX_DECOMPRESSION_RATIO {
            return Err(SafiError::new(
                ErrorCode::SF_IMP_005,
                ErrorSeverity::Error,
                "ZipBombDecompressionRatioExceeded",
                "import.security_rejected",
            ));
        }

        let height = height as usize;
        let width = width as usize;
        let mut grid: Vec<Vec<Data>> = vec![vec![Data::Empty; width]; height];
        for (r, row) in grid.iter_mut().enumerate() {
            for (c, cell_slot) in row.iter_mut().enumerate() {
                if let Some(cell) = range.get_value((start.0 + r as u32, start.1 + c as u32)) {
                    *cell_slot = cell.clone();
                }
            }
        }

        // Always copy from the original range, never from an earlier merge's output.
        for merge in merges {
            let value = range.get_value(merge.start).cloned().unwrap_or(Data::Empty);
            for row in
                &mut grid[(merge.start.0 - start.0) as usize..=(merge.end.0 - start.0) as usize]
            {
                row[(merge.start.1 - start.1) as usize..=(merge.end.1 - start.1) as usize]
                    .fill(value.clone());
            }
        }

        // Extract header row (row 0)
        let mut header_names = Vec::with_capacity(width);
        for (col_idx, cell_data) in grid[0].iter().enumerate() {
            let name = data_to_string(cell_data);
            header_names.push(if name.is_empty() {
                format!("Column_{}", col_idx + 1)
            } else {
                name
            });
        }

        // Extract data rows (row 1..height)
        let mut row_cells: Vec<Vec<CellValue>> = vec![Vec::new(); width];

        for row in grid.iter().skip(1) {
            for (col_idx, cell_data) in row.iter().enumerate() {
                let raw_str = data_to_string(cell_data);
                row_cells[col_idx].push(CellValue::from_raw(&raw_str));
            }
        }

        let mut columns = Vec::with_capacity(width);
        let high_conf = ConfidenceScore::ONE;

        for (col_idx, name) in header_names.into_iter().enumerate() {
            let cells = row_cells.get(col_idx).cloned().unwrap_or_default();
            let schema = ColumnSchema::new(col_idx, name, ColumnType::Unknown, high_conf);
            columns.push(Column::new(schema, cells));
        }

        let table_id = format!("tbl_{}_{}", sheet_idx, sheet_name);
        let table = Table::new(table_id, sheet_name.clone(), columns)?;
        sheets.push(Sheet::from_single_table(sheet_idx, sheet_name, table));
    }

    if sheets.is_empty() {
        return Err(SafiError::new(
            ErrorCode::SF_IMP_001,
            ErrorSeverity::Error,
            "NoValidSheetsFound",
            "import.failed",
        ));
    }

    Ok(sheets)
}

fn data_to_string(data: &Data) -> String {
    match data {
        Data::Empty => String::new(),
        Data::String(s) => s.clone(),
        Data::Float(f) => {
            if f.fract() == 0.0 && *f > -1e15 && *f < 1e15 {
                (*f as i64).to_string()
            } else {
                f.to_string()
            }
        }
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::Error(e) => format!("{:?}", e),
        Data::DateTime(dt) => dt.to_string(),
        Data::DateTimeIso(s) => s.clone(),
        Data::DurationIso(s) => s.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn xlsx_fixture(
        rows: &[&[(&str, &str)]],
        merges: &str,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        xlsx_fixture_part(rows, merges, "xl/worksheets/sheet1.xml")
    }

    fn xlsx_fixture_part(
        rows: &[&[(&str, &str)]],
        merges: &str,
        worksheet_part: &str,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut sheet = String::from(
            r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>"#,
        );
        for row in rows {
            sheet.push_str("<row>");
            for (reference, value) in *row {
                sheet.push_str(&format!(
                    r#"<c r="{reference}" t="inlineStr"><is><t>{value}</t></is></c>"#
                ));
            }
            sheet.push_str("</row>");
        }
        sheet.push_str("</sheetData>");
        sheet.push_str(merges);
        sheet.push_str("</worksheet>");
        xlsx_archive(&[(worksheet_part, sheet.as_str())])
    }

    fn xlsx_archive(worksheets: &[(&str, &str)]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut content_types = String::from(
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>"#,
        );
        let mut sheets = String::new();
        let mut targets = String::new();
        for (index, (part, _)) in worksheets.iter().enumerate() {
            let id = index + 1;
            content_types.push_str(&format!(
                r#"<Override PartName="/{part}" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>"#
            ));
            sheets.push_str(&format!(
                r#"<sheet name="Synthetic{id}" sheetId="{id}" r:id="rId{id}"/>"#
            ));
            let target = part
                .strip_prefix("xl/")
                .map_or_else(|| format!("/{part}"), str::to_owned);
            targets.push_str(&format!(
                r#"<Relationship Id="rId{id}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="{target}"/>"#
            ));
        }
        let content_types = format!("{content_types}</Types>");
        let workbook = format!(
            r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets>{sheets}</sheets></workbook>"#
        );
        let relationships = format!(
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{targets}</Relationships>"#
        );
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for (path, contents) in [
            ("[Content_Types].xml", content_types.as_str()),
            ("_rels/.rels", r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#),
            ("xl/workbook.xml", workbook.as_str()),
            ("xl/_rels/workbook.xml.rels", relationships.as_str()),
        ].into_iter().chain(worksheets.iter().copied()) {
            zip.start_file(path, options)?;
            zip.write_all(contents.as_bytes())?;
        }
        Ok(zip.finish()?.into_inner())
    }

    fn import_fixture(bytes: &[u8], max_rows: u64) -> Result<Vec<Sheet>, SafiError> {
        read_excel_stream(
            Cursor::new(bytes),
            bytes.len() as u64,
            PlatformLimits {
                max_file_bytes: u64::MAX,
                max_rows,
            },
        )
    }

    fn assert_table(sheets: &[Sheet], headers: &[&str], rows: &[&[&str]]) {
        assert_eq!(sheets.len(), 1);
        let table = &sheets[0].tables()[0];
        assert_eq!(table.column_count(), headers.len());
        assert_eq!(table.row_count(), rows.len());
        for (column, header) in table.columns().iter().zip(headers) {
            assert_eq!(column.schema().name, *header);
        }
        for (r, row) in rows.iter().enumerate() {
            assert_eq!(row.len(), headers.len());
            for (c, value) in row.iter().enumerate() {
                assert_eq!(table.cell(r, c).map(CellValue::raw_text), Some(*value));
            }
        }
    }

    #[test]
    fn unmerged_sparse_cells_stay_blank() -> TestResult {
        let bytes = xlsx_fixture(
            &[
                &[("A1", "Left"), ("D1", "Right")],
                &[("A2", "alpha"), ("C2", "gamma")],
                &[("D3", "delta")],
            ],
            "",
        )?;
        assert_table(
            &import_fixture(&bytes, 3)?,
            &["Left", "Column_2", "Column_3", "Right"],
            &[&["alpha", "", "gamma", ""], &["", "", "", "delta"]],
        );
        Ok(())
    }

    #[test]
    fn horizontal_merges_do_not_fill_adjacent_blanks() -> TestResult {
        let bytes = xlsx_fixture(
            &[
                &[("A1", "Left"), ("B1", "Group"), ("E1", "Right")],
                &[("A2", "alpha"), ("C2", "merged")],
            ],
            r#"<mergeCells count="2"><mergeCell ref="B1:C1"/><mergeCell ref="C2:D2"/></mergeCells>"#,
        )?;
        assert_table(
            &import_fixture(&bytes, 2)?,
            &["Left", "Group", "Group", "Column_4", "Right"],
            &[&["alpha", "", "merged", "merged", ""]],
        );
        Ok(())
    }

    #[test]
    fn vertical_merges_copy_top_left_only_inside_region() -> TestResult {
        let bytes = xlsx_fixture(
            &[
                &[("A1", "Left"), ("B1", "Middle"), ("C1", "Right")],
                &[("B2", "merged")],
                &[("A4", "edge")],
                &[("C5", "last")],
            ],
            r#"<mergeCells count="1"><mergeCell ref="B2:B4"/></mergeCells>"#,
        )?;
        assert_table(
            &import_fixture(&bytes, 5)?,
            &["Left", "Middle", "Right"],
            &[
                &["", "merged", ""],
                &["", "merged", ""],
                &["edge", "merged", ""],
                &["", "", "last"],
            ],
        );
        Ok(())
    }

    #[test]
    fn merges_use_absolute_offset_range_origin() -> TestResult {
        let bytes = xlsx_fixture(
            &[
                &[("C5", "Left"), ("D5", "Middle"), ("E5", "Right")],
                &[("C6", "merged")],
                &[("E8", "last")],
            ],
            r#"<mergeCells count="1"><mergeCell ref="C6:D7"/></mergeCells>"#,
        )?;
        let mut workbook = open_workbook_from_rs::<Xlsx<_>, _>(Cursor::new(&bytes))?;
        assert_eq!(
            workbook.worksheet_range("Synthetic1")?.start(),
            Some((4, 2))
        );
        assert_table(
            &import_fixture(&bytes, 4)?,
            &["Left", "Middle", "Right"],
            &[
                &["merged", "merged", ""],
                &["merged", "merged", ""],
                &["", "", "last"],
            ],
        );
        Ok(())
    }

    #[test]
    fn merges_extend_beyond_nonempty_used_range() -> TestResult {
        let bytes = xlsx_fixture(
            &[&[("C5", "Header")], &[("C6", "merged")]],
            r#"<mergeCells count="1"><mergeCell ref="C6:E8"/></mergeCells>"#,
        )?;
        assert_table(
            &import_fixture(&bytes, 4)?,
            &["Header", "Column_2", "Column_3"],
            &[
                &["merged", "merged", "merged"],
                &["merged", "merged", "merged"],
                &["merged", "merged", "merged"],
            ],
        );
        Ok(())
    }

    #[test]
    fn used_range_still_respects_row_bound() -> TestResult {
        let bytes = xlsx_fixture(&[&[("C5", "Header")], &[("C8", "last")]], "")?;
        assert!(matches!(
            import_fixture(&bytes, 3),
            Err(error) if error.code() == ErrorCode::SF_IMP_003
        ));
        assert_eq!(import_fixture(&bytes, 4)?[0].total_rows(), 3);
        Ok(())
    }

    #[test]
    fn expanded_merge_respects_row_bound_including_header() -> TestResult {
        let bytes = xlsx_fixture(
            &[&[("C5", "Header")], &[("C6", "merged")]],
            r#"<mergeCells count="1"><mergeCell ref="C6:C8"/></mergeCells>"#,
        )?;
        assert!(matches!(
            import_fixture(&bytes, 3),
            Err(error) if error.code() == ErrorCode::SF_IMP_003
        ));
        assert_eq!(import_fixture(&bytes, 4)?[0].total_rows(), 3);
        Ok(())
    }

    #[test]
    fn expanded_merge_respects_size_estimate_guard() -> TestResult {
        let bytes = xlsx_fixture(
            &[&[("A1", "Header")], &[("A2", "merged")]],
            r#"<mergeCells count="1"><mergeCell ref="A2:XFD1048576"/></mergeCells>"#,
        )?;
        assert!(matches!(
            import_fixture(&bytes, u64::MAX),
            Err(error) if error.code() == ErrorCode::SF_IMP_005
        ));
        Ok(())
    }

    #[test]
    fn sparse_cell_coordinates_are_bounded_before_calamine_allocates() -> TestResult {
        let bytes = xlsx_archive(&[
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row><c r="A1"/><c r="XFD1048576"/></row></sheetData></worksheet>"#,
            ),
            (
                "xl/worksheets/sheet2.xml",
                r#"<worksheet><sheetData><row><c r="A1"/></row></sheetData></worksheet>"#,
            ),
        ])?;
        assert!(
            matches!(preflight_workbook(&mut Cursor::new(&bytes), bytes.len() as u64),
            Err(error) if error.code() == ErrorCode::SF_IMP_005)
        );
        Ok(())
    }

    #[test]
    fn expanded_merge_text_cost_is_checked_without_allocating_the_grid() -> TestResult {
        let mut range = calamine::Range::new((0, 0), (1, 0));
        range.set_value((1, 0), Data::String("x".repeat(32_767)));
        let merge = parse_dimension_reference("A2:A100000")?;
        assert!(
            matches!(estimate_expanded_sheet_bytes(&range, &[merge], 100_000),
            Err(error) if error.code() == ErrorCode::SF_IMP_005)
        );
        assert!(
            matches!(estimate_expanded_sheet_bytes(&range, &[], u64::MAX),
            Err(error) if error.code() == ErrorCode::SF_IMP_005)
        );
        Ok(())
    }

    #[test]
    fn expanded_merge_text_cost_is_cumulative() -> TestResult {
        let mut range = calamine::Range::new((0, 0), (1, 1));
        range.set_value((1, 0), Data::String("x".repeat(1024)));
        range.set_value((1, 1), Data::String("y".repeat(1024)));
        let merges = [
            parse_dimension_reference("A2:A100001")?,
            parse_dimension_reference("B2:B100001")?,
        ];
        for merge in &merges {
            estimate_expanded_sheet_bytes(&range, std::slice::from_ref(merge), 200_002)?;
        }
        assert!(
            matches!(estimate_expanded_sheet_bytes(&range, &merges, 200_002),
            Err(error) if error.code() == ErrorCode::SF_IMP_005)
        );
        Ok(())
    }

    #[test]
    fn import_checks_expanded_text_cost_before_copying() -> TestResult {
        let value = "x".repeat(512);
        let bytes = xlsx_fixture(
            &[&[("A1", "Header")], &[("A2", value.as_str())]],
            r#"<mergeCells><mergeCell ref="A2:A257"/></mergeCells>"#,
        )?;
        assert!(matches!(import_fixture(&bytes, 257),
            Err(error) if error.code() == ErrorCode::SF_IMP_005));
        let small = xlsx_fixture(
            &[&[("A1", "Header")], &[("A2", value.as_str())]],
            r#"<mergeCells><mergeCell ref="A2:A3"/></mergeCells>"#,
        )?;
        assert_table(
            &import_fixture(&small, 3)?,
            &["Header"],
            &[&[value.as_str()], &[value.as_str()]],
        );
        Ok(())
    }

    #[test]
    fn malformed_merge_reference_is_import_failure() -> TestResult {
        let bytes = xlsx_fixture(
            &[&[("A1", "Header")], &[("A2", "alpha")]],
            r#"<mergeCells count="1"><mergeCell ref="A2:!"/></mergeCells>"#,
        )?;
        assert!(matches!(
            import_fixture(&bytes, 10),
            Err(error) if error.code() == ErrorCode::SF_IMP_001
        ));
        Ok(())
    }

    #[test]
    fn missing_merge_reference_is_import_failure() -> TestResult {
        let bytes = xlsx_fixture(
            &[&[("A1", "Header")], &[("A2", "alpha")]],
            r#"<mergeCells count="1"><mergeCell/></mergeCells>"#,
        )?;
        assert!(matches!(
            import_fixture(&bytes, 10),
            Err(error) if error.code() == ErrorCode::SF_IMP_001
        ));
        Ok(())
    }

    #[test]
    fn pinned_calamine_merge_apis_lose_missing_reference() -> TestResult {
        let bytes = xlsx_fixture(
            &[&[("A1", "Header")], &[("A2", "alpha")]],
            r#"<mergeCells count="1"><mergeCell/></mergeCells>"#,
        )?;
        let mut workbook = open_workbook_from_rs::<Xlsx<_>, _>(Cursor::new(&bytes))?;
        workbook.load_merged_regions()?;
        assert!(workbook.merged_regions().is_empty());
        assert!(matches!(
            workbook.worksheet_merge_cells("Synthetic1"),
            Some(Ok(regions)) if regions.is_empty()
        ));
        Ok(())
    }

    #[test]
    fn preflight_checks_nonstandard_worksheet_parts() -> TestResult {
        for part in ["xl/custom/data.xml", "sheets/data.xml"] {
            let rows: &[&[(&str, &str)]] = &[&[("A1", "Header")], &[("A2", "merged")]];
            let valid = xlsx_fixture_part(
                rows,
                r#"<mergeCells><mergeCell ref="A2:B3"></mergeCell></mergeCells>"#,
                part,
            )?;
            preflight_workbook(&mut Cursor::new(&valid), valid.len() as u64)?;
            // Pinned calamine rejects these paths before reading cells; only preflight accepts them.
            assert!(open_workbook_from_rs::<Xlsx<_>, _>(Cursor::new(&valid)).is_err());
            let invalid =
                xlsx_fixture_part(rows, r#"<mergeCells><mergeCell/></mergeCells>"#, part)?;
            assert!(
                matches!(preflight_workbook(&mut Cursor::new(&invalid), invalid.len() as u64),
                Err(error) if error.code() == ErrorCode::SF_IMP_001)
            );
        }
        Ok(())
    }

    #[test]
    fn worksheet_filename_need_not_be_sheet1() -> TestResult {
        let bytes = xlsx_fixture_part(
            &[&[("A1", "Header")], &[("A2", "merged")]],
            r#"<mergeCells><mergeCell ref="A2:B3"></mergeCell></mergeCells>"#,
            "xl/worksheets/data.xml",
        )?;
        assert_table(
            &import_fixture(&bytes, 3)?,
            &["Header", "Column_2"],
            &[&["merged", "merged"], &["merged", "merged"]],
        );
        Ok(())
    }

    #[test]
    fn content_type_defaults_and_overrides_select_worksheet_parts() -> TestResult {
        for reference in [r#" ref="A1:B2""#, ""] {
            let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            let worksheet =
                format!("<worksheet><mergeCells><mergeCell{reference}/></mergeCells></worksheet>");
            for (path, xml) in [
                (
                    "[Content_Types].xml",
                    r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="sheet" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/other.sheet" ContentType="application/xml"/></Types>"#,
                ),
                ("custom/data.sheet", worksheet.as_str()),
                ("other.sheet", "<nonworksheet><mergeCell/></nonworksheet>"),
            ] {
                zip.start_file(path, options)?;
                zip.write_all(xml.as_bytes())?;
            }
            let bytes = zip.finish()?.into_inner();
            let result = preflight_workbook(&mut Cursor::new(&bytes), bytes.len() as u64);
            if reference.is_empty() {
                assert!(matches!(result, Err(error) if error.code() == ErrorCode::SF_IMP_001));
            } else {
                result?;
            }
        }
        Ok(())
    }

    #[test]
    fn entity_encoded_merge_references_match_plain_references() -> TestResult {
        let rows: &[&[(&str, &str)]] = &[&[("A1", "Header")], &[("A2", "merged")]];
        let encoded = xlsx_fixture(
            rows,
            r#"<mergeCells><mergeCell ref="A&#50;:B&#50;"/></mergeCells>"#,
        )?;
        let plain = xlsx_fixture(rows, r#"<mergeCells><mergeCell ref="A2:B2"/></mergeCells>"#)?;
        let mut workbook = open_workbook_from_rs::<Xlsx<_>, _>(Cursor::new(&encoded))?;
        assert!(workbook.load_merged_regions().is_err());
        assert_eq!(import_fixture(&encoded, 3)?, import_fixture(&plain, 3)?);
        Ok(())
    }

    #[test]
    fn entity_encoded_references_in_calamine_metadata_are_rejected() -> TestResult {
        for part in ["xl/sharedStrings.xml", "xl/styles.xml"] {
            let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
            let options =
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
            for (path, xml) in [
                (
                    "[Content_Types].xml",
                    r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/></Types>"#,
                ),
                (part, r#"<root ref="A&#50;:B&#50;"/>"#),
            ] {
                zip.start_file(path, options)?;
                zip.write_all(xml.as_bytes())?;
            }
            let bytes = zip.finish()?.into_inner();
            let result = preflight_workbook(&mut Cursor::new(&bytes), bytes.len() as u64);
            assert!(
                matches!(result, Err(error) if error.code() == ErrorCode::SF_IMP_001),
                "{part}"
            );
        }
        let bytes = xlsx_fixture(&[&[("A1", "Header")]], r#"<dimension ref="A&#49;"/>"#)?;
        assert!(
            matches!(import_fixture(&bytes, 2), Err(error) if error.code() == ErrorCode::SF_IMP_001)
        );
        Ok(())
    }

    #[test]
    fn worksheet_relationships_cannot_bypass_merge_validation() -> TestResult {
        let bytes = xlsx_fixture(
            &[&[("A1", "Header")], &[("A2", "value")]],
            r#"<mergeCells><mergeCell/></mergeCells>"#,
        )?;
        let mut archive = ZipArchive::new(Cursor::new(&bytes))?;
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        for index in 0..archive.len() {
            let mut part = archive.by_index(index)?;
            let name = part.name().to_owned();
            let mut contents = String::new();
            part.read_to_string(&mut contents)?;
            if name == "[Content_Types].xml" {
                contents = contents.replace(
                    "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml",
                    "application/xml",
                );
            }
            zip.start_file(
                name,
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )?;
            zip.write_all(contents.as_bytes())?;
        }
        let mislabeled = zip.finish()?.into_inner();
        assert!(matches!(import_fixture(&mislabeled, 3),
            Err(error) if error.code() == ErrorCode::SF_IMP_001));
        Ok(())
    }

    #[test]
    fn invalid_merge_dimensions_and_attributes_are_import_failures() -> TestResult {
        for attributes in [
            r#"ref="""#,
            r#"ref="   ""#,
            r#"ref="A2:B3" ref="A2:B3""#,
            r#"ref="B3:A2""#,
            r#"ref="B2:A3""#,
            r#"ref="A3:B2""#,
            r#"ref="A0:A2""#,
            r#"ref="A2:XFE3""#,
            r#"ref="A2:A1048577""#,
            r#"ref="A2:A99999999999999999999""#,
            r#"ref="A2:ZZZZZZZZZZ3""#,
            r#"ref="A2:B3:C4""#,
            r#"ref="A2:""#,
            r#"ref= "A2:B3" broken"#,
        ] {
            let merges = format!("<mergeCells><mergeCell {attributes}/></mergeCells>");
            let bytes = xlsx_fixture(&[&[("A1", "Header")], &[("A2", "value")]], &merges)?;
            assert!(
                matches!(import_fixture(&bytes, u64::MAX),
                Err(error) if error.code() == ErrorCode::SF_IMP_001),
                "{attributes}"
            );
        }
        Ok(())
    }

    #[test]
    fn malformed_xml_is_import_failure() -> TestResult {
        for xml in [
            b"<worksheet><mergeCells>".as_slice(),
            b"<worksheet><mergeCells></worksheet>",
            b"<worksheet/><worksheet/>",
            b"<worksheet>&undefined;</worksheet>",
        ] {
            assert!(matches!(visit_xml(xml, |_, _| Ok(())),
                Err(error) if error.code() == ErrorCode::SF_IMP_001));
        }
        let bytes = xlsx_fixture(
            &[&[("A1", "Header")]],
            r#"<mergeCells><mergeCell ref="A1:B2"/></wrong>"#,
        )?;
        assert!(matches!(import_fixture(&bytes, 3),
            Err(error) if error.code() == ErrorCode::SF_IMP_001));
        Ok(())
    }

    #[test]
    fn dimension_elements_are_checked_before_calamine_arithmetic() -> TestResult {
        let bytes = xlsx_fixture(&[&[("A1", "Header")]], r#"<dimension ref="B3:A1"/>"#)?;
        assert!(matches!(import_fixture(&bytes, 3),
            Err(error) if error.code() == ErrorCode::SF_IMP_001));
        Ok(())
    }

    #[test]
    fn blank_merge_origin_does_not_copy_adjacent_or_interior_values() -> TestResult {
        let bytes = xlsx_fixture(
            &[
                &[("A1", "Left"), ("C1", "Right")],
                &[("A2", "adjacent"), ("C3", "interior")],
            ],
            r#"<mergeCells><mergeCell ref="B2:C3"/></mergeCells>"#,
        )?;
        assert_table(
            &import_fixture(&bytes, 3)?,
            &["Left", "Column_2", "Right"],
            &[&["adjacent", "", ""], &["", "", ""]],
        );
        Ok(())
    }

    #[test]
    fn merges_can_expand_before_used_range() -> TestResult {
        let bytes = xlsx_fixture(
            &[&[("D6", "value")]],
            r#"<mergeCells><mergeCell ref="C5:C7"/></mergeCells>"#,
        )?;
        assert_table(
            &import_fixture(&bytes, 3)?,
            &["Column_1", "Column_2"],
            &[&["", "value"], &["", ""]],
        );
        Ok(())
    }

    #[test]
    fn merge_at_last_xlsx_cell_is_valid() -> TestResult {
        let bytes = xlsx_fixture(
            &[&[("XFD1048575", "Header")], &[("XFD1048576", "value")]],
            r#"<mergeCells><mergeCell ref="XFD1048576"/></mergeCells>"#,
        )?;
        assert_table(&import_fixture(&bytes, 2)?, &["Header"], &[&["value"]]);
        Ok(())
    }

    #[test]
    fn preflight_decompression_budget_is_shared_and_enforced() -> TestResult {
        let bytes = xlsx_fixture(&[&[("A1", "Header")]], "")?;
        let mut archive = ZipArchive::new(Cursor::new(&bytes))?;
        let size = archive.by_name("[Content_Types].xml")?.size();
        let mut remaining = size;
        assert_eq!(
            read_xml_part(&mut archive, "[Content_Types].xml", &mut remaining)?.len() as u64,
            size
        );
        assert_eq!(remaining, 0);
        assert!(
            matches!(read_xml_part(&mut archive, "xl/worksheets/sheet1.xml", &mut remaining),
            Err(error) if error.code() == ErrorCode::SF_IMP_005)
        );
        assert!(matches!(preflight_workbook(&mut Cursor::new(&bytes), 1),
            Err(error) if error.code() == ErrorCode::SF_IMP_005));
        Ok(())
    }

    #[test]
    fn test_zip_bomb_detection_sf_imp_005() {
        let limits = PlatformLimits {
            max_file_bytes: 100,
            max_rows: 10,
        };
        let dummy = b"not a real xlsx";
        let res = read_excel_stream(Cursor::new(dummy), dummy.len() as u64, limits);
        assert!(res.is_err());
    }
}
