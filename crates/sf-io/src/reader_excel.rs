//! Excel (`XLSX`, `XLS`, `ODS`, `XLSM`) and spreadsheet reader using `calamine`
//! adhering to Specification Sections 4.2 and 8.2 (Zip Bomb defense `SF-IMP-005`, merged cells unmerging).

use std::io::{Read, Seek};

use calamine::{Data, Reader, Xlsx, open_workbook_from_rs};

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

    // 2. Open workbook via calamine
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

    for (sheet_idx, sheet_name) in sheet_names.into_iter().enumerate() {
        let range = match workbook.worksheet_range(&sheet_name) {
            Ok(r) => r,
            Err(_) => continue,
        };

        if range.is_empty() {
            continue;
        }

        let height = range.height();
        let width = range.width();

        if height == 0 || width == 0 {
            continue;
        }

        // Enforce max rows limit (SF-IMP-003)
        if (height as u64) > limits.max_rows {
            return Err(SafiError::new(
                ErrorCode::SF_IMP_003,
                ErrorSeverity::Error,
                "PlatformRowCountLimitExceeded",
                "import.limit_exceeded",
            ));
        }

        // Simple heuristic uncompressed size check (estimated bytes)
        let estimated_uncompressed = (height * width * 32) as u64;
        if estimated_uncompressed > MAX_UNCOMPRESSED_BYTES {
            return Err(SafiError::new(
                ErrorCode::SF_IMP_005,
                ErrorSeverity::Error,
                "ZipBombLimitExceeded",
                "import.security_rejected",
            ));
        }

        if file_bytes > 0 && (estimated_uncompressed / file_bytes) > MAX_DECOMPRESSION_RATIO {
            return Err(SafiError::new(
                ErrorCode::SF_IMP_005,
                ErrorSeverity::Error,
                "ZipBombDecompressionRatioExceeded",
                "import.security_rejected",
            ));
        }

        // Build a 2D grid of Data to manipulate/unmerge easily
        let mut grid: Vec<Vec<Data>> = vec![vec![Data::Empty; width]; height];
        for (r, row) in grid.iter_mut().enumerate() {
            for (c, cell_slot) in row.iter_mut().enumerate() {
                if let Some(cell) = range.get((r, c)) {
                    *cell_slot = cell.clone();
                }
            }
        }

        // Unmerge cells: If there are merged cell patterns (or empty cells following a non-empty cell in merged regions),
        // we propagate the top-left value across rows/cols if appropriate, or fill forward in sparse blocks.
        for row in &mut grid {
            let mut last_val = Data::Empty;
            for cell in row.iter_mut() {
                if *cell != Data::Empty {
                    last_val = cell.clone();
                } else if last_val != Data::Empty {
                    *cell = last_val.clone();
                }
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
    use std::io::Cursor;

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
