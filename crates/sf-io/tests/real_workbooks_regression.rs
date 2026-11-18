//! Real binary workbook corpus regression tests (`SF-IMP-002`, `SF-IMP-005`, Phase 1 Engine).
//!
//! Opens actual `.xlsx`, `.xlsm`, `.ods`, `.xls`, `.csv` (Windows-1256), `.tsv`, wide multi-row
//! workbooks, corrupted OOXML bytes, and ZIP-bomb files from `tests/fixtures/workbooks/` via
//! `sf_io::ingest_workbook_universal`.

use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use sf_core::error::ErrorCode;
use sf_core::limits::PlatformLimits;
use sf_io::{
    FormulaVerificationStatus, SheetVisibility, WorkbookDateSystem, WorkbookFormat,
    ingest_workbook_universal,
};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/workbooks")
        .join(name)
}

#[test]
fn ingests_real_messy_enterprise_xlsx_multisheet_multitable_merged_headers_and_formulas()
-> Result<(), Box<dyn std::error::Error>> {
    let bytes = fs::read(fixture_path("messy_enterprise.xlsx"))?;
    let rep = ingest_workbook_universal(
        Cursor::new(bytes),
        PlatformLimits::DESKTOP_CLI,
        Some("messy_enterprise.xlsx"),
        None,
    )?;

    assert_eq!(rep.detected_format, WorkbookFormat::Xlsx);
    assert_eq!(rep.total_sheets, 3);
    assert_eq!(rep.visible_sheets, 1);
    assert_eq!(rep.hidden_sheets, 2);

    // Sheet 1: multi-table segmentation + subtotal/footnote separation + merged headers + formula classification
    let ws1 = &rep.worksheets[0];
    assert_eq!(ws1.name, "Sales & KPIs");
    assert_eq!(ws1.visibility, SheetVisibility::Visible);
    assert!(
        ws1.tables.len() >= 2,
        "expected >= 2 distinct table regions on Sales & KPIs, got {}",
        ws1.tables.len()
    );

    let t1 = &ws1.tables[0];
    assert_eq!(
        t1.data_rows.len(),
        5,
        "Subtotal/Grand Total/Footnote must be excluded from analytical data rows"
    );
    assert_eq!(
        t1.data_rows[0][0], "00105",
        "leading zeros must survive ingestion"
    );
    assert!(
        !t1.separated_rows.is_empty(),
        "expected separated subtotal/grand-total/footnote rows"
    );

    let core_t1 = t1.to_core_table()?;
    assert_eq!(core_t1.row_count(), 5);
    assert_eq!(
        core_t1
            .cell(0, 0)
            .map(sf_core::value::CellValue::visible_text),
        Some("00105".to_owned())
    );

    // Sheet 2 & Sheet 3 visibilities
    assert_eq!(rep.worksheets[1].visibility, SheetVisibility::Hidden);
    assert_eq!(rep.worksheets[2].visibility, SheetVisibility::VeryHidden);

    // Formula verification statuses across supported, unsupported, volatile, external
    let statuses: Vec<FormulaVerificationStatus> = ws1
        .cell_metadata
        .iter()
        .filter(|c| c.formula.is_some())
        .map(|c| c.formula_status)
        .collect();
    assert!(statuses.contains(&FormulaVerificationStatus::RecalculatedLocally));
    assert!(statuses.contains(&FormulaVerificationStatus::UnsupportedFormula));
    assert!(statuses.contains(&FormulaVerificationStatus::VolatileFormula));
    assert!(statuses.contains(&FormulaVerificationStatus::ExternalWorkbookReference));

    // Defined names + hyperlinks + comments
    assert!(!rep.defined_names.is_empty());
    assert!(ws1.cell_metadata.iter().any(|c| c.comment.is_some()));
    assert!(ws1.cell_metadata.iter().any(|c| c.hyperlink.is_some()));
    Ok(())
}

#[test]
fn ingests_real_xlsm_with_macro_neutralization_and_1904_dates()
-> Result<(), Box<dyn std::error::Error>> {
    let bytes = fs::read(fixture_path("macro_mac1904.xlsm"))?;
    let rep = ingest_workbook_universal(
        Cursor::new(bytes),
        PlatformLimits::DESKTOP_CLI,
        Some("macro_mac1904.xlsm"),
        None,
    )?;

    assert_eq!(rep.detected_format, WorkbookFormat::Xlsm);
    assert!(rep.macros_neutralized);
    assert_eq!(rep.date_system, WorkbookDateSystem::Macintosh1904);
    assert_eq!(rep.total_sheets, 1);
    assert_eq!(rep.worksheets[0].tables[0].data_rows.len(), 2);
    Ok(())
}

#[test]
fn ingests_real_bilingual_arabic_ods_workbook() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = fs::read(fixture_path("bilingual_arabic.ods"))?;
    let rep = ingest_workbook_universal(
        Cursor::new(bytes),
        PlatformLimits::DESKTOP_CLI,
        Some("bilingual_arabic.ods"),
        None,
    )?;

    assert_eq!(rep.detected_format, WorkbookFormat::Ods);
    assert_eq!(rep.total_sheets, 2);
    assert_eq!(rep.worksheets[0].name, "الإيرادات");
    assert_eq!(rep.worksheets[0].visibility, SheetVisibility::Visible);
    assert_eq!(rep.worksheets[1].name, "ODS_Hidden");
    assert_eq!(rep.worksheets[1].visibility, SheetVisibility::Hidden);
    let t = &rep.worksheets[0].tables[0];
    assert_eq!(t.data_rows.len(), 2);
    assert_eq!(t.data_rows[0][0], "00712");
    assert_eq!(t.data_rows[0][1], "القاهرة");
    Ok(())
}

#[test]
fn ingests_real_binary_biff8_xls_workbook_with_hidden_sheet()
-> Result<(), Box<dyn std::error::Error>> {
    let bytes = fs::read(fixture_path("legacy_biff8.xls"))?;
    let rep = ingest_workbook_universal(
        Cursor::new(bytes),
        PlatformLimits::DESKTOP_CLI,
        Some("legacy_biff8.xls"),
        None,
    )?;

    assert_eq!(rep.detected_format, WorkbookFormat::Xls);
    assert_eq!(rep.total_sheets, 2);
    assert_eq!(rep.worksheets[0].name, "MainSheet");
    assert_eq!(rep.worksheets[0].visibility, SheetVisibility::Visible);
    assert_eq!(rep.worksheets[1].name, "HiddenBiff");
    assert_eq!(rep.worksheets[1].visibility, SheetVisibility::Hidden);
    let t = &rep.worksheets[0].tables[0];
    assert_eq!(t.data_rows.len(), 2);
    assert_eq!(t.data_rows[0][0], "Cairo");
    Ok(())
}

#[test]
fn ingests_windows1256_arabic_csv_and_utf8_bom_tsv() -> Result<(), Box<dyn std::error::Error>> {
    let cp_bytes = fs::read(fixture_path("arabic_cp1256.csv"))?;
    let cp_rep = ingest_workbook_universal(
        Cursor::new(cp_bytes),
        PlatformLimits::DESKTOP_CLI,
        Some("arabic_cp1256.csv"),
        None,
    )?;
    assert_eq!(cp_rep.detected_format, WorkbookFormat::Csv);
    assert_eq!(cp_rep.worksheets[0].tables[0].headers[1], "المدينة");
    assert_eq!(cp_rep.worksheets[0].tables[0].data_rows[0][0], "00123");
    assert_eq!(cp_rep.worksheets[0].tables[0].data_rows[0][1], "القاهرة");

    let tsv_bytes = fs::read(fixture_path("tab_delimited.tsv"))?;
    let tsv_rep = ingest_workbook_universal(
        Cursor::new(tsv_bytes),
        PlatformLimits::DESKTOP_CLI,
        Some("tab_delimited.tsv"),
        None,
    )?;
    assert_eq!(tsv_rep.detected_format, WorkbookFormat::Csv);
    assert_eq!(
        tsv_rep.worksheets[0].tables[0].headers,
        vec!["code", "region", "amount"]
    );
    assert_eq!(tsv_rep.worksheets[0].tables[0].data_rows[0][0], "0081");
    Ok(())
}

#[test]
fn rejects_corrupt_and_zip_bomb_and_supports_atomic_cancellation()
-> Result<(), Box<dyn std::error::Error>> {
    let corrupt_bytes = fs::read(fixture_path("corrupt_truncated.xlsx"))?;
    let err_corrupt = ingest_workbook_universal(
        Cursor::new(corrupt_bytes),
        PlatformLimits::DESKTOP_CLI,
        Some("corrupt_truncated.xlsx"),
        None,
    )
    .err()
    .ok_or("corrupted xlsx must fail")?;
    assert_eq!(err_corrupt.code(), ErrorCode::SF_IMP_002);

    let bomb_bytes = fs::read(fixture_path("zip_bomb_oversized.xlsx"))?;
    let err_bomb = ingest_workbook_universal(
        Cursor::new(bomb_bytes),
        PlatformLimits::DESKTOP_CLI,
        Some("zip_bomb_oversized.xlsx"),
        None,
    )
    .err()
    .ok_or("zip bomb xlsx must fail")?;
    assert_eq!(err_bomb.code(), ErrorCode::SF_IMP_005);

    let wide_bytes = fs::read(fixture_path("wide_500_rows.xlsx"))?;
    let ok_rep = ingest_workbook_universal(
        Cursor::new(wide_bytes.clone()),
        PlatformLimits::DESKTOP_CLI,
        Some("wide_500_rows.xlsx"),
        None,
    )?;
    assert_eq!(ok_rep.total_analytical_rows, 500);

    let cancel_flag = AtomicBool::new(true);
    let err_cancel = ingest_workbook_universal(
        Cursor::new(wide_bytes),
        PlatformLimits::DESKTOP_CLI,
        Some("wide_500_rows.xlsx"),
        Some(&cancel_flag),
    )
    .err()
    .ok_or("cancelled ingest must abort")?;
    assert_eq!(err_cancel.code(), ErrorCode::SF_IMP_002);
    Ok(())
}
