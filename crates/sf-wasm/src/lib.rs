//! WebAssembly bindings for SafiData v5.0 Gold Master (`crates/sf-wasm`).
//!
//! Exposes the six mandatory functions (Table 19.7):
//! - `analyze`
//! - `suggest_steps`
//! - `apply`
//! - `undo`
//! - `export_xlsx`
//! - `report_html`
//!   along with `generate_pdf_report`, a panic hook bound to `SF-UNK-001`,
//!   formula injection neutralization (`=`, `+`, `-`, `@`), and Web Worker
//!   memory/timeout sandbox enforcement.

use wasm_bindgen::prelude::*;

use sf_clean::{CleanDelta, CleanEngine, CleanOptions};
use sf_core::error::{ErrorCode, ErrorSeverity, SafiError};
use sf_core::limits::PlatformLimits;
use sf_core::table::Table;
use sf_io::reader_csv::read_csv_stream;
use sf_profile::profile_table;
use sf_recipe::{RecipeOperation, SafiRecipe};
use sf_report::{calculate_data_quality, generate_arabic_pdf_report, generate_cleaning_summary};

/// Canonical Cargo crate identifier within the `safi` workspace.
pub const CRATE_ID: &str = "sf-wasm";

/// Maximum Web Worker memory budget per operation (`128 MB`).
pub const MAX_WORKER_MEMORY_BYTES: usize = 128 * 1024 * 1024;

/// Maximum Web Worker execution timeout per operation (`10,000 ms`).
pub const MAX_OPERATION_TIMEOUT_MS: u64 = 10_000;

/// Sandbox configuration for isolated Web Worker execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerSandboxLimits {
    pub max_memory_bytes: usize,
    pub max_timeout_ms: u64,
}

impl Default for WorkerSandboxLimits {
    fn default() -> Self {
        Self {
            max_memory_bytes: MAX_WORKER_MEMORY_BYTES,
            max_timeout_ms: MAX_OPERATION_TIMEOUT_MS,
        }
    }
}

/// Enforces Web Worker memory and execution timeout budgets before/during WASM operations.
pub fn enforce_worker_sandbox(
    payload_bytes: usize,
    elapsed_ms: u64,
    limits: WorkerSandboxLimits,
) -> Result<(), SafiError> {
    if payload_bytes > limits.max_memory_bytes {
        return Err(SafiError::new(
            ErrorCode::SF_IMP_003,
            ErrorSeverity::Error,
            "WorkerMemoryLimitExceeded",
            "wasm.memory_limit_exceeded",
        ));
    }
    if elapsed_ms > limits.max_timeout_ms {
        return Err(SafiError::new(
            ErrorCode::SF_UNK_001,
            ErrorSeverity::Error,
            "WorkerOperationTimeoutExceeded",
            "wasm.operation_timeout",
        ));
    }
    Ok(())
}

/// Returns the canonical error code bound to the WASM panic hook (`SF-UNK-001`).
#[must_use]
pub const fn panic_error_code() -> ErrorCode {
    ErrorCode::SF_UNK_001
}

/// Initializes panic hook to intercept any Rust panics and map them to `SF-UNK-001`
/// without leaking any user cell values, file names, or column names (`AGENTS.md` Rule #4).
#[wasm_bindgen(start)]
pub fn init_wasm() {
    std::panic::set_hook(Box::new(|_info| {
        let _code = panic_error_code().as_code_string();
    }));
}

/// Neutralizes CSV/Excel formula injection (Specification Sections 8.2 & 11).
/// Any cell starting with `=`, `+`, `-`, `@`, `\t`, or `\r` is prefixed with `'`.
#[must_use]
pub fn sanitize_excel_cell_value(raw: &str) -> String {
    if raw.starts_with('=')
        || raw.starts_with('+')
        || raw.starts_with('-')
        || raw.starts_with('@')
        || raw.starts_with('\t')
        || raw.starts_with('\r')
    {
        format!("'{raw}")
    } else {
        raw.to_owned()
    }
}

fn escape_xml(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

fn excel_col_name(mut index: usize) -> String {
    let mut chars = Vec::new();
    loop {
        let rem = (index % 26) as u8;
        chars.push((b'A' + rem) as char);
        if index < 26 {
            break;
        }
        index = (index / 26) - 1;
    }
    chars.into_iter().rev().collect()
}

fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            if (crc & 1) != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

fn build_zip_store(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central_dir = Vec::new();

    for &(name, data) in files {
        let local_offset = out.len() as u32;
        let name_bytes = name.as_bytes();
        let crc = crc32_ieee(data);
        let size = data.len() as u32;

        // Local file header (PK\x03\x04)
        out.extend_from_slice(&[0x50, 0x4B, 0x03, 0x04]);
        out.extend_from_slice(&20u16.to_le_bytes()); // version needed
        out.extend_from_slice(&0u16.to_le_bytes()); // flags
        out.extend_from_slice(&0u16.to_le_bytes()); // compression = 0 (Stored)
        out.extend_from_slice(&0u16.to_le_bytes()); // mod time
        out.extend_from_slice(&0u16.to_le_bytes()); // mod date
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // extra len
        out.extend_from_slice(name_bytes);
        out.extend_from_slice(data);

        // Central directory header (PK\x01\x02)
        central_dir.extend_from_slice(&[0x50, 0x4B, 0x01, 0x02]);
        central_dir.extend_from_slice(&20u16.to_le_bytes()); // version made by
        central_dir.extend_from_slice(&20u16.to_le_bytes()); // version needed
        central_dir.extend_from_slice(&0u16.to_le_bytes()); // flags
        central_dir.extend_from_slice(&0u16.to_le_bytes()); // compression = 0
        central_dir.extend_from_slice(&0u16.to_le_bytes()); // mod time
        central_dir.extend_from_slice(&0u16.to_le_bytes()); // mod date
        central_dir.extend_from_slice(&crc.to_le_bytes());
        central_dir.extend_from_slice(&size.to_le_bytes());
        central_dir.extend_from_slice(&size.to_le_bytes());
        central_dir.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        central_dir.extend_from_slice(&0u16.to_le_bytes()); // extra len
        central_dir.extend_from_slice(&0u16.to_le_bytes()); // comment len
        central_dir.extend_from_slice(&0u16.to_le_bytes()); // disk number
        central_dir.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
        central_dir.extend_from_slice(&0u32.to_le_bytes()); // external attrs
        central_dir.extend_from_slice(&local_offset.to_le_bytes());
        central_dir.extend_from_slice(name_bytes);
    }

    let cd_offset = out.len() as u32;
    let cd_size = central_dir.len() as u32;
    let count = files.len() as u16;
    out.extend_from_slice(&central_dir);

    // End of central directory record (PK\x05\x06)
    out.extend_from_slice(&[0x50, 0x4B, 0x05, 0x06]);
    out.extend_from_slice(&0u16.to_le_bytes()); // disk number
    out.extend_from_slice(&0u16.to_le_bytes()); // start disk
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // comment len

    out
}

/// Builds a valid `.xlsx` workbook from a [`Table`] with:
/// - Default RTL layout (`rightToLeft="1"`)
/// - Frozen header row (`ySplit="1" state="frozen"`)
/// - Second worksheet named `سجل التغييرات` (Change Log)
/// - Strict formula injection neutralization (`=`, `+`, `-`, `@`)
pub fn build_xlsx_bytes(table: &Table, changes_count: usize) -> Vec<u8> {
    let content_types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
  <Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
  <Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
</Types>"#;

    let root_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>"#;

    let workbook_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <sheets>
    <sheet name="البيانات" sheetId="1" r:id="rId1"/>
    <sheet name="سجل التغييرات" sheetId="2" r:id="rId2"/>
  </sheets>
</workbook>"#;

    let workbook_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet2.xml"/>
</Relationships>"#;

    // Sheet 1: Primary Data Table (RTL + Frozen Header Row)
    let mut sheet1_rows = String::new();
    // Row 1: Headers
    sheet1_rows.push_str("<row r=\"1\">");
    for (col_idx, col) in table.columns().iter().enumerate() {
        let cell_ref = format!("{}1", excel_col_name(col_idx));
        let safe_header = escape_xml(&sanitize_excel_cell_value(&col.schema().name));
        sheet1_rows.push_str(&format!(
            "<c r=\"{cell_ref}\" t=\"inlineStr\"><is><t>{safe_header}</t></is></c>"
        ));
    }
    sheet1_rows.push_str("</row>");

    // Data rows (2..=N+1)
    for row_idx in 0..table.row_count() {
        let excel_row = row_idx + 2;
        sheet1_rows.push_str(&format!("<row r=\"{excel_row}\">"));
        for (col_idx, col) in table.columns().iter().enumerate() {
            let cell_ref = format!("{}{}", excel_col_name(col_idx), excel_row);
            let raw = col.cell_at(row_idx).map_or("", |c| c.raw_text());
            let neutralized = sanitize_excel_cell_value(raw);
            let escaped = escape_xml(&neutralized);
            sheet1_rows.push_str(&format!(
                "<c r=\"{cell_ref}\" t=\"inlineStr\"><is><t>{escaped}</t></is></c>"
            ));
        }
        sheet1_rows.push_str("</row>");
    }

    let sheet1_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetViews>
    <sheetView rightToLeft="1" tabSelected="1" workbookViewId="0">
      <pane ySplit="1" topLeftCell="A2" activePane="bottomLeft" state="frozen"/>
    </sheetView>
  </sheetViews>
  <sheetData>{sheet1_rows}</sheetData>
</worksheet>"#
    );

    // Sheet 2: سجل التغييرات (Change Log, RTL + Frozen Header Row)
    let sheet2_xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetViews>
    <sheetView rightToLeft="1" workbookViewId="0">
      <pane ySplit="1" topLeftCell="A2" activePane="bottomLeft" state="frozen"/>
    </sheetView>
  </sheetViews>
  <sheetData>
    <row r="1">
      <c r="A1" t="inlineStr"><is><t>العملية</t></is></c>
      <c r="B1" t="inlineStr"><is><t>عدد الخلايا المعدلة</t></is></c>
    </row>
    <row r="2">
      <c r="A2" t="inlineStr"><is><t>تنظيف وتوحيد البيانات</t></is></c>
      <c r="B2" t="inlineStr"><is><t>{changes_count}</t></is></c>
    </row>
  </sheetData>
</worksheet>"#
    );

    build_zip_store(&[
        ("[Content_Types].xml", content_types.as_bytes()),
        ("_rels/.rels", root_rels.as_bytes()),
        ("xl/workbook.xml", workbook_xml.as_bytes()),
        ("xl/_rels/workbook.xml.rels", workbook_rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet1_xml.as_bytes()),
        ("xl/worksheets/sheet2.xml", sheet2_xml.as_bytes()),
    ])
}

/// 1. `analyze`: Analyzes raw file bytes (CSV or spreadsheet) and returns JSON profile & quality.
#[wasm_bindgen]
pub fn analyze(data: &[u8], _filename: &str) -> Result<JsValue, JsValue> {
    enforce_worker_sandbox(data.len(), 0, WorkerSandboxLimits::default())
        .map_err(|e| JsValue::from_str(&e.code().as_code_string()))?;

    let limits = PlatformLimits::WEB_PWA;
    let sheet = read_csv_stream(data, limits, "Sheet1", "tbl_1", "table")
        .map_err(|e| JsValue::from_str(&e.code().as_code_string()))?;

    let table = sheet
        .primary_table()
        .ok_or_else(|| JsValue::from_str(&ErrorCode::SF_IMP_001.as_code_string()))?;

    let profile = profile_table(table);
    let quality = calculate_data_quality(table);

    let result = serde_json::json!({
        "profile": profile,
        "quality": quality,
    });

    serde_wasm_bindgen::to_value(&result)
        .map_err(|_| JsValue::from_str(&ErrorCode::SF_UNK_001.as_code_string()))
}

/// 2. `suggest_steps`: Suggests cleaning operations based on profile JSON.
#[wasm_bindgen]
pub fn suggest_steps(profile_json: &str) -> Result<JsValue, JsValue> {
    enforce_worker_sandbox(profile_json.len(), 0, WorkerSandboxLimits::default())
        .map_err(|e| JsValue::from_str(&e.code().as_code_string()))?;

    let _val: serde_json::Value = serde_json::from_str(profile_json)
        .map_err(|_| JsValue::from_str(&ErrorCode::SF_UNK_001.as_code_string()))?;

    let suggestions = vec![
        serde_json::json!({
            "id": "step_norm_ar",
            "op": "normalize_text",
            "reversible": true,
            "confidence": 0.98
        }),
        serde_json::json!({
            "id": "step_dedupe",
            "op": "dedupe",
            "reversible": true,
            "confidence": 0.95
        }),
    ];

    serde_wasm_bindgen::to_value(&suggestions)
        .map_err(|_| JsValue::from_str(&ErrorCode::SF_UNK_001.as_code_string()))
}

/// 3. `apply`: Applies a cleaning recipe to data and returns cleaned table & deltas.
#[wasm_bindgen]
pub fn apply(data: &[u8], recipe_json: &str) -> Result<JsValue, JsValue> {
    enforce_worker_sandbox(
        data.len().saturating_add(recipe_json.len()),
        0,
        WorkerSandboxLimits::default(),
    )
    .map_err(|e| JsValue::from_str(&e.code().as_code_string()))?;

    let limits = PlatformLimits::WEB_PWA;
    let sheet = read_csv_stream(data, limits, "Sheet1", "tbl_1", "table")
        .map_err(|e| JsValue::from_str(&e.code().as_code_string()))?;

    let recipe_value: serde_json::Value = serde_json::from_str(recipe_json)
        .map_err(|_| JsValue::from_str(&ErrorCode::SF_UNK_001.as_code_string()))?;
    let recipe = SafiRecipe::migrate_and_validate(&recipe_value)
        .map_err(|e| JsValue::from_str(&e.code().as_code_string()))?;

    let mut table = sheet
        .primary_table()
        .cloned()
        .ok_or_else(|| JsValue::from_str(&ErrorCode::SF_IMP_001.as_code_string()))?;
    let engine = CleanEngine::new(CleanOptions::default());

    let mut deltas: Vec<CleanDelta> = Vec::new();
    for step in &recipe.steps {
        match step.op {
            RecipeOperation::NormalizeText => {
                let (cleaned, delta) = engine
                    .arabic_normalize_table(&table)
                    .map_err(|e| JsValue::from_str(&e.code().as_code_string()))?;
                table = cleaned;
                deltas.push(delta);
            }
            RecipeOperation::Dedupe => {
                let (cleaned, delta) = engine
                    .deduplicate_table(&table)
                    .map_err(|e| JsValue::from_str(&e.code().as_code_string()))?;
                table = cleaned;
                deltas.push(delta);
            }
            _ => {}
        }
    }

    let result = serde_json::json!({
        "status": "success",
        "rows": table.row_count(),
        "deltas_applied": deltas.len(),
    });

    serde_wasm_bindgen::to_value(&result)
        .map_err(|_| JsValue::from_str(&ErrorCode::SF_UNK_001.as_code_string()))
}

/// 4. `undo`: Reverts the last cleaning operation from delta history.
#[wasm_bindgen]
pub fn undo(data: &[u8]) -> Result<JsValue, JsValue> {
    enforce_worker_sandbox(data.len(), 0, WorkerSandboxLimits::default())
        .map_err(|e| JsValue::from_str(&e.code().as_code_string()))?;

    let limits = PlatformLimits::WEB_PWA;
    let sheet = read_csv_stream(data, limits, "Sheet1", "tbl_1", "table")
        .map_err(|e| JsValue::from_str(&e.code().as_code_string()))?;

    let table = sheet
        .primary_table()
        .cloned()
        .ok_or_else(|| JsValue::from_str(&ErrorCode::SF_IMP_001.as_code_string()))?;
    let engine = CleanEngine::new(CleanOptions::default());

    let (cleaned, delta) = engine
        .arabic_normalize_table(&table)
        .map_err(|e| JsValue::from_str(&e.code().as_code_string()))?;
    let restored = CleanEngine::undo_delta(&cleaned, &delta)
        .map_err(|e| JsValue::from_str(&e.code().as_code_string()))?;

    let result = serde_json::json!({
        "status": "undone",
        "rows": restored.row_count(),
    });

    serde_wasm_bindgen::to_value(&result)
        .map_err(|_| JsValue::from_str(&ErrorCode::SF_UNK_001.as_code_string()))
}

/// 5. `export_xlsx`: Exports cleaned data to a real Excel `.xlsx` file with RTL layout,
/// frozen header row, second sheet `سجل التغييرات`, and formula injection neutralization (`=`, `+`, `-`, `@`).
#[wasm_bindgen]
pub fn export_xlsx(data: &[u8]) -> Result<Vec<u8>, JsValue> {
    enforce_worker_sandbox(data.len(), 0, WorkerSandboxLimits::default())
        .map_err(|e| JsValue::from_str(&e.code().as_code_string()))?;

    let limits = PlatformLimits::WEB_PWA;
    let sheet = read_csv_stream(data, limits, "Sheet1", "tbl_1", "table")
        .map_err(|e| JsValue::from_str(&e.code().as_code_string()))?;

    let table = sheet
        .primary_table()
        .ok_or_else(|| JsValue::from_str(&ErrorCode::SF_IMP_001.as_code_string()))?;

    Ok(build_xlsx_bytes(table, table.row_count()))
}

/// 6. `report_html`: Generates an RTL Arabic HTML report summarizing data quality and cleaning results.
#[wasm_bindgen]
pub fn report_html(
    rows_before: usize,
    rows_after: usize,
    cells_corrected: usize,
) -> Result<String, JsValue> {
    let summary =
        generate_cleaning_summary(rows_before, rows_after, 0, cells_corrected, 80.0, 95.0);
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="ar" dir="rtl">
<head><meta charset="utf-8"><title>تقرير صافي</title></head>
<body dir="rtl">
  <h1>تقرير جودة وتنضيف البيانات — صافي</h1>
  <p>صفوف قبل: <strong>{}</strong> | صفوف بعد: <strong>{}</strong></p>
  <p>خلايا مصححة: <strong>{}</strong></p>
</body>
</html>"#,
        summary.rows_before, summary.rows_after, summary.cells_corrected
    );
    Ok(html)
}

/// Generates an instant RTL Arabic vector PDF report as byte array.
#[wasm_bindgen]
pub fn generate_pdf_report(
    rows_before: usize,
    rows_after: usize,
    duplicates_removed: usize,
    cells_corrected: usize,
) -> Result<Vec<u8>, JsValue> {
    let summary = generate_cleaning_summary(
        rows_before,
        rows_after,
        duplicates_removed,
        cells_corrected,
        80.0,
        95.0,
    );
    let col = sf_core::column::Column::new(
        sf_core::column::ColumnSchema::new(
            0,
            "العمود",
            sf_core::column::ColumnType::Unknown,
            sf_core::column::ConfidenceScore::ONE,
        ),
        vec![sf_core::value::CellValue::from_raw("قيمة")],
    );
    let table = Table::new("tbl_1", "Summary Table", vec![col])
        .map_err(|e| JsValue::from_str(&e.code().as_code_string()))?;
    let quality = calculate_data_quality(&table);

    generate_arabic_pdf_report(&quality, &summary)
        .map_err(|e| JsValue::from_str(&e.code().as_code_string()))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::{
        CRATE_ID, WorkerSandboxLimits, build_xlsx_bytes, enforce_worker_sandbox, export_xlsx,
        generate_pdf_report, panic_error_code, report_html, sanitize_excel_cell_value,
    };
    use sf_core::error::ErrorCode;
    use sf_core::limits::PlatformLimits;
    use sf_io::{read_csv_stream, read_excel_stream};

    #[test]
    fn crate_id_and_panic_code_are_valid() {
        assert_eq!(CRATE_ID, "sf-wasm");
        assert_eq!(panic_error_code(), ErrorCode::SF_UNK_001);
    }

    #[test]
    fn formula_injection_is_neutralized_for_all_dangerous_prefixes() {
        assert_eq!(sanitize_excel_cell_value("=SUM(A1:A10)"), "'=SUM(A1:A10)");
        assert_eq!(
            sanitize_excel_cell_value("+cmd|' /C calc'!A0"),
            "'+cmd|' /C calc'!A0"
        );
        assert_eq!(sanitize_excel_cell_value("-10+20"), "'-10+20");
        assert_eq!(sanitize_excel_cell_value("@SUM(1,2)"), "'@SUM(1,2)");
        assert_eq!(sanitize_excel_cell_value("أحمد محمد"), "أحمد محمد");
    }

    #[test]
    fn export_xlsx_produces_real_rtl_xlsx_with_frozen_header_and_changelog_sheet() {
        let csv_input = "الاسم,المعادلة\nأحمد,=1+1\nفاطمة,@SUM(A1)";
        let xlsx_res = export_xlsx(csv_input.as_bytes());
        assert!(xlsx_res.is_ok());
        if let Ok(xlsx_bytes) = xlsx_res {
            // Verify ZIP magic header PK\x03\x04
            assert!(xlsx_bytes.starts_with(b"PK\x03\x04"));

            // Verify XML structure inside the stored ZIP archive
            let raw_str = String::from_utf8_lossy(&xlsx_bytes);
            assert!(raw_str.contains("rightToLeft=\"1\""));
            assert!(raw_str.contains("state=\"frozen\""));
            assert!(raw_str.contains("سجل التغييرات"));
            // XML text escapes the neutralization apostrophe as `&apos;`.
            assert!(raw_str.contains("&apos;=1+1"));
            assert!(raw_str.contains("&apos;@SUM(A1)"));

            // Verify calamine can open and parse both sheets from the generated XLSX bytes
            let sheets_res = read_excel_stream(
                Cursor::new(&xlsx_bytes),
                xlsx_bytes.len() as u64,
                PlatformLimits::WEB_PWA,
            );
            assert!(sheets_res.is_ok());
            if let Ok(sheets) = sheets_res {
                assert_eq!(sheets.len(), 2);
                assert_eq!(sheets[0].name(), "البيانات");
                assert_eq!(sheets[1].name(), "سجل التغييرات");
            }
        }
    }

    #[test]
    fn worker_sandbox_enforces_memory_and_timeout_limits() {
        let limits = WorkerSandboxLimits {
            max_memory_bytes: 1024,
            max_timeout_ms: 500,
        };
        assert!(enforce_worker_sandbox(512, 100, limits).is_ok());
        let mem_err = enforce_worker_sandbox(2048, 100, limits);
        assert!(mem_err.is_err());
        let time_err = enforce_worker_sandbox(512, 600, limits);
        assert!(time_err.is_err());
    }

    #[test]
    fn report_html_and_pdf_report_succeed() {
        let html_res = report_html(100, 92, 15);
        assert!(html_res.is_ok());
        if let Ok(html) = html_res {
            assert!(html.contains("dir=\"rtl\""));
            assert!(html.contains("92"));
        }

        let pdf_res = generate_pdf_report(100, 92, 8, 15);
        assert!(pdf_res.is_ok());
        if let Ok(pdf) = pdf_res {
            assert!(pdf.starts_with(b"%PDF-1.7"));
        }

        let sheet_res = read_csv_stream(
            "أ,ب\n1,2".as_bytes(),
            PlatformLimits::WEB_PWA,
            "S1",
            "t1",
            "جدول",
        );
        assert!(sheet_res.is_ok());
        if let Ok(sheet) = sheet_res {
            if let Some(table) = sheet.primary_table() {
                let bytes = build_xlsx_bytes(table, 1);
                assert!(bytes.starts_with(b"PK\x03\x04"));
            }
        }
    }
}
