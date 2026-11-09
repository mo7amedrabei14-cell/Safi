//! `sf-report` — Data quality scoring (Section 4.11), before/after comparison summaries,
//! and embedded Typst vector Arabic PDF report generation.
//!
//! Zero network dependencies (`AGENTS.md` Rule #7).

pub mod pdf;
pub mod quality;
pub mod summary;

pub use pdf::{generate_arabic_pdf_report, render_typst_markup, shape_arabic_text};
pub use quality::{ColumnQualityScore, DataQualityReport, calculate_data_quality};
pub use summary::{CleaningSummary, generate_cleaning_summary};

/// Canonical Cargo crate identifier within the `safi` workspace.
pub const CRATE_ID: &str = "sf-report";

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::{
        CRATE_ID, calculate_data_quality, generate_arabic_pdf_report, generate_cleaning_summary,
        shape_arabic_text,
    };
    use sf_core::column::{Column, ColumnSchema, ColumnType, ConfidenceScore};
    use sf_core::table::Table;
    use sf_core::value::CellValue;

    #[test]
    fn crate_id_matches_workspace_member() {
        assert_eq!(CRATE_ID, "sf-report");
    }

    #[test]
    fn calculates_data_quality_score_out_of_100() {
        let col1 = Column::new(
            ColumnSchema::new(0, "الاسم", ColumnType::PersonName, ConfidenceScore::ONE),
            vec![
                CellValue::from_raw("أحمد محمد"),
                CellValue::from_raw("سارة علي"),
                CellValue::from_raw("محمود حسن"),
            ],
        );
        let col2 = Column::new(
            ColumnSchema::new(
                1,
                "المبلغ",
                ColumnType::MoneyAndCurrency,
                ConfidenceScore::ONE,
            ),
            vec![
                CellValue::from_raw("100.00"),
                CellValue::from_raw(""),
                CellValue::from_raw("300.00"),
            ],
        );
        let table_res = Table::new("tbl_q", "فحص الجودة", vec![col1, col2]);
        assert!(table_res.is_ok());
        if let Ok(table) = table_res {
            let report = calculate_data_quality(&table);
            assert!(report.overall_score > 0.0 && report.overall_score <= 100.0);
            assert_eq!(report.column_scores.len(), 2);
            // First column has 100% completeness, second has 66.67% completeness
            assert!((report.column_scores[0].completeness - 100.0).abs() < 1e-6);
            assert!(report.column_scores[1].completeness < 70.0);
        }
    }

    #[test]
    fn generates_rtl_arabic_vector_pdf_with_connected_letters_under_100ms() {
        let col = Column::new(
            ColumnSchema::new(0, "الاسم", ColumnType::PersonName, ConfidenceScore::ONE),
            vec![CellValue::from_raw("أحمد"), CellValue::from_raw("فاطمة")],
        );
        let table_res = Table::new("tbl_pdf", "جدول التقرير", vec![col]);
        assert!(table_res.is_ok());
        if let Ok(table) = table_res {
            let start = Instant::now();
            let quality = calculate_data_quality(&table);
            let summary = generate_cleaning_summary(10, 8, 1, 3, 72.5, quality.overall_score);
            let pdf_res = generate_arabic_pdf_report(&quality, &summary);
            let elapsed_ms = start.elapsed().as_millis();

            assert!(pdf_res.is_ok());
            assert!(
                elapsed_ms < 100,
                "PDF generation took {elapsed_ms}ms, exceeding 100ms budget"
            );

            if let Ok(pdf_bytes) = pdf_res {
                assert!(pdf_bytes.starts_with(b"%PDF-1.7"));
                assert!(pdf_bytes.ends_with(b"%%EOF\n"));
                let pdf_text = String::from_utf8_lossy(&pdf_bytes);
                assert!(pdf_text.contains("/Direction /R2L"));
                assert!(pdf_text.contains("/Lang (ar-EG)"));

                // Verify Arabic contextual letter shaping (connected presentation forms)
                let shaped = shape_arabic_text("صافي");
                // 'ص' initial (U+FEBB) + 'ا' final (U+FE8E) + 'ف' initial (U+FED3) + 'ي' final (U+FEF2)
                assert_eq!(shaped, "\u{FEBB}\u{FE8E}\u{FED3}\u{FEF2}");
            }
        }
    }
}
