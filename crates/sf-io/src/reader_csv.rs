//! Streaming CSV reader with encoding detection, delimiter auto-detection,
//! platform capacity limits enforcement (`SF-IMP-003`), and `sf-core` integration.

use std::io::{BufRead, Read};

use sf_core::column::{Column, ColumnSchema, ColumnType, ConfidenceScore};
use sf_core::error::{ErrorCode, ErrorSeverity, SafiError};
use sf_core::limits::PlatformLimits;
use sf_core::table::{Sheet, Table};
use sf_core::value::CellValue;

use crate::delimiter::detect_delimiter;
use crate::encoding::detect_and_decode;

/// Imports a CSV stream into a validated [`Sheet`] containing a [`Table`],
/// enforcing `PlatformLimits` (`SF-IMP-003`) and encoding detection (`SF-IMP-002`).
pub fn read_csv_stream<R: Read>(
    mut reader: R,
    limits: PlatformLimits,
    sheet_name: impl Into<String>,
    table_id: impl Into<String>,
    table_name: impl Into<String>,
) -> Result<Sheet, SafiError> {
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|_| {
            SafiError::new(
                ErrorCode::SF_IMP_001,
                ErrorSeverity::Error,
                "CouldNotReadFile",
                "import.failed",
            )
        })
        .and_then(|_| {
            let file_bytes = bytes.len() as u64;
            // Preliminary limit check on file size before decoding
            if file_bytes > limits.max_file_bytes {
                return Err(SafiError::new(
                    ErrorCode::SF_IMP_003,
                    ErrorSeverity::Error,
                    "PlatformFileSizeLimitExceeded",
                    "import.limit_exceeded",
                ));
            }
            Ok(())
        })?;

    let (text, _) = detect_and_decode(&bytes)?;
    let mut cursor = std::io::Cursor::new(text.as_bytes());

    // Read first line for delimiter detection
    let mut first_line = String::new();
    let delimiter = if cursor.read_line(&mut first_line).is_ok() && !first_line.is_empty() {
        detect_delimiter(&first_line)
    } else {
        b','
    };

    // Reset cursor for csv reader
    let mut csv_cursor = std::io::Cursor::new(text.as_bytes());
    let mut csv_reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .from_reader(&mut csv_cursor);

    let headers = csv_reader.headers().map_err(|_| {
        SafiError::new(
            ErrorCode::SF_IMP_001,
            ErrorSeverity::Error,
            "InvalidCsvHeaders",
            "import.failed",
        )
    })?;

    let header_names: Vec<String> = headers.iter().map(ToString::to_string).collect();
    let col_count = header_names.len();
    if col_count == 0 {
        return Err(SafiError::new(
            ErrorCode::SF_IMP_001,
            ErrorSeverity::Error,
            "EmptyTableColumns",
            "import.failed",
        ));
    }

    let mut row_cells: Vec<Vec<CellValue>> = vec![Vec::new(); col_count];
    let mut row_count = 0usize;

    for result in csv_reader.records() {
        let record = result.map_err(|_| {
            SafiError::new(
                ErrorCode::SF_IMP_001,
                ErrorSeverity::Error,
                "CsvRecordReadFailed",
                "import.failed",
            )
        })?;

        row_count = row_count.saturating_add(1);
        if (row_count as u64) > limits.max_rows {
            return Err(SafiError::new(
                ErrorCode::SF_IMP_003,
                ErrorSeverity::Error,
                "PlatformRowCountLimitExceeded",
                "import.limit_exceeded",
            ));
        }

        for (col_idx, field) in record.iter().enumerate() {
            if col_idx < col_count {
                row_cells[col_idx].push(CellValue::from_raw(field));
            }
        }
    }

    let mut columns = Vec::with_capacity(col_count);
    let high_conf = ConfidenceScore::ONE;

    for (col_idx, name) in header_names.into_iter().enumerate() {
        let cells = row_cells.get(col_idx).cloned().unwrap_or_default();
        let schema = ColumnSchema::new(col_idx, name, ColumnType::Unknown, high_conf);
        columns.push(Column::new(schema, cells));
    }

    let table = Table::new(table_id, table_name, columns)?;
    Ok(Sheet::from_single_table(0, sheet_name, table))
}
