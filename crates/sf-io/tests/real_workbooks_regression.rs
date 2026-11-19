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

#[test]
fn ingests_genuine_excel_xlsb_binary_workbook() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = fs::read(fixture_path("enterprise_binary.xlsb"))?;
    let rep = ingest_workbook_universal(
        Cursor::new(bytes),
        PlatformLimits::DESKTOP_CLI,
        Some("enterprise_binary.xlsb"),
        None,
    )?;

    eprintln!(
        "XLSB report: format={:?} support={:?} sheets={} vis={} hid={} tables={} rows={} formulas={} verified={} warnings={:?}",
        rep.detected_format,
        rep.support_level,
        rep.total_sheets,
        rep.visible_sheets,
        rep.hidden_sheets,
        rep.total_tables_extracted,
        rep.total_analytical_rows,
        rep.total_formulas_detected,
        rep.total_formulas_verified,
        rep.warnings
    );
    for ws in &rep.worksheets {
        eprintln!(
            "  sheet='{}' vis={:?} tables={} merged={}",
            ws.name,
            ws.visibility,
            ws.tables.len(),
            ws.merged_ranges.len()
        );
        for t in &ws.tables {
            eprintln!(
                "    table headers={:?} rows={}",
                t.headers,
                t.data_rows.len()
            );
        }
    }

    assert_eq!(rep.detected_format, WorkbookFormat::Xlsb);
    assert_eq!(rep.total_sheets, 3);
    assert_eq!(rep.visible_sheets, 1);
    assert_eq!(rep.hidden_sheets, 2);
    assert!(rep.total_tables_extracted >= 2);
    assert_eq!(rep.worksheets[0].name, "Sales & KPIs");
    assert_eq!(rep.worksheets[0].visibility, SheetVisibility::Visible);
    assert_eq!(rep.worksheets[1].visibility, SheetVisibility::Hidden);
    assert_eq!(rep.worksheets[2].visibility, SheetVisibility::VeryHidden);
    assert!(rep.total_analytical_rows >= 5);
    assert_eq!(rep.worksheets[0].tables[0].data_rows[0][0], "00105");
    assert!(rep.worksheets[0].tables[1].data_rows.len() == 2);
    assert!(!rep.defined_names.is_empty());
    assert_eq!(
        rep.worksheets[0].auto_filter_range.as_deref(),
        Some("A4:E9")
    );
    assert!(
        rep.worksheets[0]
            .cell_metadata
            .iter()
            .any(|c| c.comment.is_some())
    );
    assert!(
        rep.worksheets[0]
            .cell_metadata
            .iter()
            .any(|c| c.hyperlink.is_some())
    );
    let xlsb_statuses: Vec<FormulaVerificationStatus> = rep.worksheets[0]
        .cell_metadata
        .iter()
        .filter(|c| c.formula.is_some())
        .map(|c| c.formula_status)
        .collect();
    assert!(xlsb_statuses.contains(&FormulaVerificationStatus::VerifiedCachedAndRecalculated));
    assert!(xlsb_statuses.contains(&FormulaVerificationStatus::VolatileFormula));
    assert!(xlsb_statuses.contains(&FormulaVerificationStatus::ExternalWorkbookReference));
    assert!(xlsb_statuses.contains(&FormulaVerificationStatus::FormulaError));
    Ok(())
}

fn build_in_memory_xlsb(
    wb_bin: &[u8],
    sheet_bin: &[u8],
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use std::io::Write;
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut cursor);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("[Content_Types].xml", opts)?;
        zip.write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="bin" ContentType="application/vnd.ms-excel.sheet.binary.macroEnabled.main"/></Types>"#,
        )?;
        zip.start_file("xl/workbook.bin", opts)?;
        zip.write_all(wb_bin)?;
        zip.start_file("xl/_rels/workbook.bin.rels", opts)?;
        zip.write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.bin"/></Relationships>"#,
        )?;
        zip.start_file("xl/worksheets/sheet1.bin", opts)?;
        zip.write_all(sheet_bin)?;
        zip.finish()?;
    }
    Ok(cursor.into_inner())
}

fn encode_biff12_record(rec_id: u16, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    if rec_id < 0x80 {
        out.push(rec_id as u8);
    } else {
        out.push(((rec_id & 0x7F) as u8) | 0x80);
        out.push(((rec_id >> 7) & 0x7F) as u8);
    }
    let mut len = payload.len();
    loop {
        let chunk = (len & 0x7F) as u8;
        len >>= 7;
        if len > 0 {
            out.push(chunk | 0x80);
        } else {
            out.push(chunk);
            break;
        }
    }
    out.extend_from_slice(payload);
    out
}

fn encode_xl_wide_string(text: &str) -> Vec<u8> {
    let units: Vec<u16> = text.encode_utf16().collect();
    let mut out = Vec::with_capacity(4 + units.len() * 2);
    out.extend_from_slice(&(units.len() as u32).to_le_bytes());
    for u in units {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out
}

fn minimal_valid_workbook_bin() -> Vec<u8> {
    let mut bundle_sh = Vec::new();
    bundle_sh.extend_from_slice(&0u32.to_le_bytes()); // hsState=Visible
    bundle_sh.extend_from_slice(&1u32.to_le_bytes()); // iTabID=1
    bundle_sh.extend_from_slice(&encode_xl_wide_string("rId1"));
    bundle_sh.extend_from_slice(&encode_xl_wide_string("Sheet1"));

    let mut wb = Vec::new();
    wb.extend_from_slice(&encode_biff12_record(0x0083, &[])); // BrtBeginBook
    wb.extend_from_slice(&encode_biff12_record(0x009C, &bundle_sh)); // BrtBundleSh
    wb.extend_from_slice(&encode_biff12_record(0x0084, &[])); // BrtEndBook
    wb
}

#[test]
fn rejects_xlsb_sparse_corner_value_and_formula_allocation_bombs()
-> Result<(), Box<dyn std::error::Error>> {
    // 1. Value-cell opposite corners (A1 + XFD1048576): must return structured SF-IMP-005 without OOM.
    let val_bytes = fs::read(fixture_path("adversarial_sparse_corners.xlsb"))?;
    let val_err = ingest_workbook_universal(
        Cursor::new(val_bytes),
        PlatformLimits::DESKTOP_CLI,
        Some("adversarial_sparse_corners.xlsb"),
        None,
    )
    .err()
    .ok_or("sparse opposite-corner XLSB values must fail with SF-IMP-005")?;
    assert_eq!(val_err.code(), ErrorCode::SF_IMP_005);

    // 2. Formula-cell opposite corners (A1 + XFD1048576): formula extent must be validated independently.
    let fmla_bytes = fs::read(fixture_path("adversarial_sparse_formula_corners.xlsb"))?;
    let fmla_err = ingest_workbook_universal(
        Cursor::new(fmla_bytes),
        PlatformLimits::DESKTOP_CLI,
        Some("adversarial_sparse_formula_corners.xlsb"),
        None,
    )
    .err()
    .ok_or("sparse opposite-corner XLSB formulas must fail with SF-IMP-005")?;
    assert_eq!(fmla_err.code(), ErrorCode::SF_IMP_005);

    // 3. Single cell beyond Excel coordinate limit (row 1_048_576): must fail with SF-IMP-002.
    let mut oob_sheet = Vec::new();
    oob_sheet.extend_from_slice(&encode_biff12_record(0x0081, &[]));
    oob_sheet.extend_from_slice(&encode_biff12_record(0x0000, &1_048_576u32.to_le_bytes()));
    oob_sheet.extend_from_slice(&encode_biff12_record(0x0082, &[]));
    let oob_xlsb = build_in_memory_xlsb(&minimal_valid_workbook_bin(), &oob_sheet)?;
    let oob_err = ingest_workbook_universal(
        Cursor::new(oob_xlsb),
        PlatformLimits::DESKTOP_CLI,
        Some("oob_row.xlsb"),
        None,
    )
    .err()
    .ok_or("out-of-bounds XLSB row must fail with SF-IMP-002")?;
    assert_eq!(oob_err.code(), ErrorCode::SF_IMP_002);

    // 4. Single valid cell at XFD1048576 (1x1 sparse range, but 17B dense grid origin-to-max):
    // must fail with SF-IMP-005 before allocating any dense worksheet grid.
    let mut far_sheet = Vec::new();
    far_sheet.extend_from_slice(&encode_biff12_record(0x0081, &[]));
    far_sheet.extend_from_slice(&encode_biff12_record(0x0000, &1_048_575u32.to_le_bytes()));
    let mut rk_payload = Vec::new();
    rk_payload.extend_from_slice(&16_383u32.to_le_bytes()); // col XFD
    rk_payload.extend_from_slice(&0u32.to_le_bytes()); // style
    rk_payload.extend_from_slice(&((42u32 << 2) | 0x02).to_le_bytes()); // RK int 42
    far_sheet.extend_from_slice(&encode_biff12_record(0x0002, &rk_payload));
    far_sheet.extend_from_slice(&encode_biff12_record(0x0082, &[]));
    let far_xlsb = build_in_memory_xlsb(&minimal_valid_workbook_bin(), &far_sheet)?;
    let far_err = ingest_workbook_universal(
        Cursor::new(far_xlsb),
        PlatformLimits::DESKTOP_CLI,
        Some("far_single_cell.xlsb"),
        None,
    )
    .err()
    .ok_or("single XLSB cell at XFD1048576 must fail with SF-IMP-005")?;
    assert_eq!(far_err.code(), ErrorCode::SF_IMP_005);

    Ok(())
}

#[test]
fn rejects_malformed_and_truncated_biff12_xlsb_streams() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Truncated worksheet BIFF12 payload after valid initial rows must fail (no silent partial success).
    let trunc_sheet_bytes = fs::read(fixture_path("malformed_truncated_sheet.xlsb"))?;
    let trunc_err = ingest_workbook_universal(
        Cursor::new(trunc_sheet_bytes),
        PlatformLimits::DESKTOP_CLI,
        Some("malformed_truncated_sheet.xlsb"),
        None,
    )
    .err()
    .ok_or("truncated XLSB worksheet stream must fail with SF-IMP-002")?;
    assert_eq!(trunc_err.code(), ErrorCode::SF_IMP_002);

    // 2. Truncated workbook.bin (missing BrtEndBook + invalid 4-byte non-terminating varint length).
    let mut bad_wb = minimal_valid_workbook_bin();
    bad_wb.truncate(bad_wb.len() - 3); // strip BrtEndBook
    bad_wb.extend_from_slice(&[0x01, 0x80, 0x80, 0x80, 0x80]); // invalid non-terminating 4-byte varint
    let valid_sheet = [
        encode_biff12_record(0x0081, &[]),
        encode_biff12_record(0x0082, &[]),
    ]
    .concat();
    let bad_wb_xlsb = build_in_memory_xlsb(&bad_wb, &valid_sheet)?;
    let wb_err = ingest_workbook_universal(
        Cursor::new(bad_wb_xlsb),
        PlatformLimits::DESKTOP_CLI,
        Some("malformed_varint_workbook.xlsb"),
        None,
    )
    .err()
    .ok_or("malformed BIFF12 varint in workbook.bin must fail with SF-IMP-002")?;
    assert_eq!(wb_err.code(), ErrorCode::SF_IMP_002);

    // 3. Truncated 2-byte record ID at EOF in worksheet stream.
    let mut trunc_id_sheet = encode_biff12_record(0x0081, &[]);
    trunc_id_sheet.push(0x85); // high bit set -> expects 2nd ID byte, hits EOF
    let trunc_id_xlsb = build_in_memory_xlsb(&minimal_valid_workbook_bin(), &trunc_id_sheet)?;
    let id_err = ingest_workbook_universal(
        Cursor::new(trunc_id_xlsb),
        PlatformLimits::DESKTOP_CLI,
        Some("truncated_rec_id.xlsb"),
        None,
    )
    .err()
    .ok_or("truncated 2-byte BIFF12 record ID must fail with SF-IMP-002")?;
    assert_eq!(id_err.code(), ErrorCode::SF_IMP_002);

    Ok(())
}

#[test]
fn preserves_undecoded_xlsb_formula_as_unsupported_with_cached_value_and_warning()
-> Result<(), Box<dyn std::error::Error>> {
    let bytes = fs::read(fixture_path("xlsb_unsupported_formula_cached.xlsb"))?;
    let rep = ingest_workbook_universal(
        Cursor::new(bytes),
        PlatformLimits::DESKTOP_CLI,
        Some("xlsb_unsupported_formula_cached.xlsb"),
        None,
    )?;

    assert_eq!(rep.detected_format, WorkbookFormat::Xlsb);
    assert_eq!(rep.total_formulas_detected, 1);
    assert_eq!(rep.total_formulas_verified, 0);

    let c2 = rep.worksheets[0]
        .cell_metadata
        .iter()
        .find(|c| c.address == "C2")
        .ok_or("C2 formula metadata must be preserved even when BIFF12 token is undecoded")?;
    assert_eq!(c2.formula.as_deref(), Some("=(unsupported BIFF12 formula)"));
    assert_eq!(c2.cached_value.as_deref(), Some("250"));
    assert_eq!(c2.recalculated_value, None);
    assert_eq!(
        c2.formula_status,
        FormulaVerificationStatus::UnsupportedFormula
    );
    assert_ne!(c2.formula_status, FormulaVerificationStatus::LiteralValue);
    assert_ne!(
        c2.formula_status,
        FormulaVerificationStatus::VerifiedCachedAndRecalculated
    );
    assert!(
        rep.warnings
            .iter()
            .any(|w| w.contains("outside local recalculation") && w.contains("undecoded binary")),
        "expected transparent warning disclosing undecoded BIFF12 formula, got: {:?}",
        rep.warnings
    );

    Ok(())
}
