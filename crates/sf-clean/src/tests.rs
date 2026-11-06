//! Unit tests and proptest idempotence & undo validation for `sf-clean`.

use crate::{CleanEngine, CleanOptions, DateAmbiguityHandling};
use sf_core::column::{Column, ColumnSchema, ColumnType, ConfidenceScore};
use sf_core::error::ErrorCode;
use sf_core::table::Table;
use sf_core::value::CellValue;

#[test]
fn test_arabic_normalization_clean() {
    let schema = ColumnSchema::new(0, "الاسم", ColumnType::PersonName, ConfidenceScore::ONE);
    let col = Column::new(schema, vec![CellValue::from_raw("أَحْمَدُ")]);
    let table_res = Table::new("tbl_1", "Sheet1", vec![col]);
    assert!(table_res.is_ok());
    if let Ok(table) = table_res {
        let engine = CleanEngine::new(CleanOptions {
            update_display_text: true,
            ..Default::default()
        });

        let res = engine.arabic_normalize_table(&table);
        assert!(res.is_ok());
        if let Ok((cleaned, delta)) = res {
            let first_cell = cleaned.column_at(0).and_then(|c| c.cell_at(0));
            assert_eq!(first_cell.map(CellValue::raw_text), Some("احمد"));
            assert_eq!(delta.affected_cells_count, 1);

            // Test Undo
            let undone_res = CleanEngine::undo_delta(&cleaned, &delta);
            assert!(undone_res.is_ok());
            if let Ok(undone) = undone_res {
                let restored = undone.column_at(0).and_then(|c| c.cell_at(0));
                assert_eq!(restored.map(CellValue::raw_text), Some("أَحْمَدُ"));
            }
        }
    }
}

#[test]
fn test_ambiguous_date_reports_sf_cln_001() {
    let schema = ColumnSchema::new(0, "تاريخ", ColumnType::Date, ConfidenceScore::ONE);
    let col = Column::new(schema, vec![CellValue::from_raw("05/10/2026")]);
    let table_res = Table::new("tbl_1", "Sheet1", vec![col]);
    assert!(table_res.is_ok());
    if let Ok(table) = table_res {
        let engine = CleanEngine::new(CleanOptions {
            date_ambiguity: DateAmbiguityHandling::ReportErrorSFCLN001,
            ..Default::default()
        });

        let res = engine.normalize_dates_table(&table);
        assert!(res.is_err());
        if let Err(err) = res {
            assert_eq!(err.code(), ErrorCode::SF_CLN_001);
        }
    }
}
