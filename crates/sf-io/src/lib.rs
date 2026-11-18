//! `sf-io` — CSV, delimited, and Excel streaming reader, encoding detector, zip bomb defense (`SF-IMP-005`), and size limits (`T1.04 & T1.05`).
//!
//! Zero network dependencies (`AGENTS.md` Rule #7).

pub mod delimiter;
pub mod encoding;
pub mod reader_csv;
pub mod reader_excel;
pub mod workbook_intelligence;

pub use delimiter::detect_delimiter;
pub use encoding::detect_and_decode;
pub use reader_csv::read_csv_stream;
pub use reader_excel::{MAX_DECOMPRESSION_RATIO, MAX_UNCOMPRESSED_BYTES, read_excel_stream};
pub use workbook_intelligence::{
    DefinedNameRecord, DetectedTableRegion, EmbeddedObjectHandling, EmbeddedObjectKind,
    EmbeddedObjectRecord, FormatCompatibilityEntry, FormatSupportLevel, FormulaVerificationStatus,
    IngestedCellDataType, IngestedCellMetadata, IngestionProgressUpdate, SeparatedRowKind,
    SeparatedSheetRow, SheetClassification, SheetRange, SheetVisibility, WorkbookDateSystem,
    WorkbookFormat, WorkbookIngestionReport, WorksheetIntelligence, coords_to_a1,
    format_compatibility_matrix, ingest_workbook_universal, segment_sheet_tables,
    sniff_workbook_format,
};

/// Canonical Cargo crate identifier within the `safi` workspace.
pub const CRATE_ID: &str = "sf-io";

#[cfg(test)]
mod tests {
    use super::{CRATE_ID, read_csv_stream, read_excel_stream};
    use sf_core::error::{ErrorCode, ErrorSeverity, SafiError};
    use sf_core::limits::PlatformLimits;

    #[test]
    fn crate_id_matches_workspace_member() {
        assert_eq!(CRATE_ID, "sf-io");
    }

    #[test]
    fn reads_utf8_arabic_csv_correctly() {
        let csv_data = "المعرف,الاسم,المبلغ\n001,أحمد محمد,150.50\n002,فاطمة علي,200.00";
        let limits = PlatformLimits {
            max_file_bytes: 1024 * 1024,
            max_rows: 100,
        };

        let sheet = read_csv_stream(csv_data.as_bytes(), limits, "Sheet1", "tbl_1", "المستفيدون");
        assert!(sheet.is_ok());
        if let Ok(s) = sheet {
            assert_eq!(s.total_rows(), 2);
            assert!(s.primary_table().is_some());
            if let Some(table) = s.primary_table() {
                assert_eq!(table.column_count(), 3);
                assert_eq!(table.cell(0, 0).map(|c| c.raw_text()), Some("001"));
                assert_eq!(table.cell(0, 1).map(|c| c.raw_text()), Some("أحمد محمد"));
                assert_eq!(table.cell(1, 2).map(|c| c.raw_text()), Some("200.00"));
            }
        }
    }

    #[test]
    fn enforces_platform_size_limits_with_sf_imp_003() {
        let csv_data = b"a,b\n1,2\n3,4\n";
        for max_file_bytes in [2, 0, csv_data.len() as u64 - 1] {
            let mut reader = std::io::Cursor::new(csv_data);
            let limits = PlatformLimits {
                max_file_bytes,
                max_rows: 100,
            };

            let res = read_csv_stream(&mut reader, limits, "Sheet1", "tbl_1", "table");
            assert_eq!(
                res.err(),
                Some(SafiError::new(
                    ErrorCode::SF_IMP_003,
                    ErrorSeverity::Error,
                    "PlatformFileSizeLimitExceeded",
                    "import.limit_exceeded",
                ))
            );
            assert_eq!(reader.position(), max_file_bytes + 1);
        }
    }

    #[test]
    fn reads_csv_exactly_at_file_size_limit() -> Result<(), SafiError> {
        let csv_data = b"a,b\n1,2";
        let limits = PlatformLimits {
            max_file_bytes: csv_data.len() as u64,
            max_rows: 100,
        };
        let mut reader = std::io::Cursor::new(csv_data);

        let sheet = read_csv_stream(&mut reader, limits, "Sheet1", "tbl_1", "table")?;
        assert_eq!(reader.position(), limits.max_file_bytes);
        assert_eq!(sheet.total_rows(), 1);
        assert_eq!(
            sheet
                .primary_table()
                .and_then(|table| table.cell(0, 1))
                .map(|cell| cell.raw_text()),
            Some("2")
        );
        Ok(())
    }

    #[test]
    fn reads_csv_with_u64_max_file_size_limit() {
        let limits = PlatformLimits {
            max_file_bytes: u64::MAX,
            max_rows: 100,
        };

        assert!(read_csv_stream(&b"a,b\n1,2"[..], limits, "Sheet1", "tbl_1", "table").is_ok());
    }

    #[test]
    fn reads_csv_quoted_and_multiline_fields() -> Result<(), SafiError> {
        let csv_data =
            b"id,note\n001,\"a,b\"\n002,\"line one\nline two\"\n003,\"say \"\"hi\"\"\"\n";
        let limits = PlatformLimits {
            max_file_bytes: csv_data.len() as u64,
            max_rows: 100,
        };

        let sheet = read_csv_stream(&csv_data[..], limits, "Sheet1", "tbl_1", "table")?;
        assert_eq!(sheet.total_rows(), 3);
        let Some(table) = sheet.primary_table() else {
            panic!("CSV table missing");
        };
        assert_eq!(table.column_count(), 2);
        assert_eq!(table.cell(0, 0).map(|cell| cell.raw_text()), Some("001"));
        assert_eq!(table.cell(0, 1).map(|cell| cell.raw_text()), Some("a,b"));
        assert_eq!(
            table.cell(1, 1).map(|cell| cell.raw_text()),
            Some("line one\nline two")
        );
        assert_eq!(
            table.cell(2, 1).map(|cell| cell.raw_text()),
            Some("say \"hi\"")
        );
        Ok(())
    }

    #[test]
    fn rejects_zip_bomb_or_invalid_excel_gracefully() {
        // Passing garbage bytes to Excel reader should return SF-IMP-001 or SF-IMP-005
        let garbage = b"PK\x03\x04fake_zip_bomb_garbage";
        let limits = PlatformLimits {
            max_file_bytes: 1024 * 1024,
            max_rows: 1000,
        };
        let res = read_excel_stream(std::io::Cursor::new(garbage), garbage.len() as u64, limits);
        assert!(res.is_err());
    }
}
