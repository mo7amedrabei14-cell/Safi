//! `sf-io` — CSV, delimited, and Excel streaming reader, encoding detector, zip bomb defense (`SF-IMP-005`), and size limits (`T1.04 & T1.05`).
//!
//! Zero network dependencies (`AGENTS.md` Rule #7).

pub mod delimiter;
pub mod encoding;
pub mod reader_csv;
pub mod reader_excel;

pub use delimiter::detect_delimiter;
pub use encoding::detect_and_decode;
pub use reader_csv::read_csv_stream;
pub use reader_excel::{MAX_DECOMPRESSION_RATIO, MAX_UNCOMPRESSED_BYTES, read_excel_stream};

/// Canonical Cargo crate identifier within the `safi` workspace.
pub const CRATE_ID: &str = "sf-io";

#[cfg(test)]
mod tests {
    use super::{CRATE_ID, read_csv_stream, read_excel_stream};
    use sf_core::error::ErrorCode;
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
        let csv_data = "a,b\n1,2";
        let strict_limits = PlatformLimits {
            max_file_bytes: 2, // Too small
            max_rows: 100,
        };

        let res = read_csv_stream(
            csv_data.as_bytes(),
            strict_limits,
            "Sheet1",
            "tbl_1",
            "جدول",
        );
        assert!(res.is_err());
        if let Err(err) = res {
            assert_eq!(err.code(), ErrorCode::SF_IMP_003);
        }
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
