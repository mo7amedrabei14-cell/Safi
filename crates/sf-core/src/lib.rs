//! `sf-core` — Foundational domain types, column taxonomy, exact decimal and zero-preserving values,
//! platform capacity limits, and privacy-preserving error/logging structures (`T1.02`).
//!
//! Zero network dependencies (`AGENTS.md` Rule #7).

pub mod column;
pub mod error;
pub mod generated_errors;
pub mod limits;
pub mod pii;
pub mod table;
pub mod value;

pub use rust_decimal::Decimal;

pub use crate::column::{
    AUTO_APPLY_CONFIDENCE_THRESHOLD, Column, ColumnSchema, ColumnType, ConfidenceDecision,
    ConfidenceScore, MAX_COLUMN_SAMPLE_VALUES, USER_CONFIRM_CONFIDENCE_THRESHOLD,
};
pub use crate::error::{
    DiagnosticToken, ErrorCode, ErrorCodeParseError, ErrorDomain, ErrorSeverity,
    SafeEngineLogRecord, SafeLogContext, SafeLogMetrics, SafiError,
};
pub use crate::generated_errors::{ErrorCatalogEntry, OFFICIAL_ERROR_CATALOG};
pub use crate::limits::{
    DESKTOP_CLI_MAX_FILE_BYTES, DESKTOP_CLI_MAX_ROWS, ONE_GIB_BYTES, ONE_MIB_BYTES, PlatformLimits,
    PlatformTarget, WEB_PWA_MAX_FILE_BYTES, WEB_PWA_MAX_ROWS,
};
pub use crate::pii::{PiiMatch, PiiProtector, PiiType};
pub use crate::table::{DataQualityWeights, Sheet, Table};
pub use crate::value::{
    CellDisplayMode, CellValue, MoneyValue, RECOGNIZED_EMPTY_SENTINELS, Value, ZeroPreservedText,
    is_recognized_empty_token,
};

/// Canonical Cargo crate identifier within the `safi` workspace.
pub const CRATE_ID: &str = "sf-core";

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use rust_decimal::Decimal;

    use super::{
        AUTO_APPLY_CONFIDENCE_THRESHOLD, CRATE_ID, CellDisplayMode, CellValue, Column,
        ColumnSchema, ColumnType, ConfidenceDecision, ConfidenceScore, DESKTOP_CLI_MAX_FILE_BYTES,
        DESKTOP_CLI_MAX_ROWS, DataQualityWeights, DiagnosticToken, ErrorCode, ErrorCodeParseError,
        ErrorDomain, ErrorSeverity, MAX_COLUMN_SAMPLE_VALUES, MoneyValue, PlatformTarget,
        PiiProtector, PiiType, RECOGNIZED_EMPTY_SENTINELS, SafeLogContext, SafeLogMetrics, SafiError,
        Sheet, Table, USER_CONFIRM_CONFIDENCE_THRESHOLD, Value, WEB_PWA_MAX_FILE_BYTES,
        WEB_PWA_MAX_ROWS, ZeroPreservedText, is_recognized_empty_token,
    };

    #[test]
    fn crate_id_matches_workspace_member() {
        assert_eq!(CRATE_ID, "sf-core");
    }

    #[test]
    fn test_pii_protection_engine() {
        let secret = b"safi_workspace_secret_key_2026";
        let matches = PiiProtector::protect("29901010101234", secret);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].pii_type, PiiType::EgyptianNationalId);
        assert_eq!(matches[0].redacted_text, "299******234");
    }

    #[test]
    fn cell_value_always_preserves_original_raw_text_per_section_4_4() {
        let cell = CellValue::from_raw("  أحمد   إبراهيم  ")
            .with_normalized("احمد ابراهيم")
            .with_value(Value::Text("أحمد إبراهيم".to_owned()));

        // Original text is always preserved verbatim
        assert_eq!(cell.raw_text(), "  أحمد   إبراهيم  ");
        assert_eq!(cell.normalized_text(), Some("احمد ابراهيم"));
        // Matching uses the normalized string
        assert_eq!(cell.matching_text(), "احمد ابراهيم");
        // Default display mode shows the untouched original text
        assert_eq!(cell.display_mode(), CellDisplayMode::Original);
        assert_eq!(cell.visible_text(), "  أحمد   إبراهيم  ");

        // User explicitly opts into Normalized display mode
        let switched = cell.with_display_mode(CellDisplayMode::Normalized);
        assert_eq!(switched.raw_text(), "  أحمد   إبراهيم  ");
        assert_eq!(switched.visible_text(), "احمد ابراهيم");
    }

    #[test]
    fn empty_sentinels_classify_as_empty_while_retaining_raw_text_per_section_4_7() {
        for sentinel in RECOGNIZED_EMPTY_SENTINELS {
            assert!(is_recognized_empty_token(sentinel));
            let cell = CellValue::from_raw(sentinel);
            assert!(cell.is_empty());
            assert_eq!(cell.value(), &Value::Empty);
            assert_eq!(cell.raw_text(), sentinel);
        }
        assert!(!is_recognized_empty_token("القاهرة"));
    }

    #[test]
    fn financial_amounts_use_exact_rust_decimal_without_float_drift() {
        let a = Decimal::from_str("0.1").ok();
        let b = Decimal::from_str("0.2").ok();
        let expected = Decimal::from_str("0.3").ok();
        assert_eq!(a.zip(b).map(|(x, y)| x + y), expected);

        let amount = Decimal::new(125_050, 2); // 1250.50
        let cell = CellValue::money("١٬٢٥٠٫٥٠ ج.م", amount, Some("EGP".to_owned()));
        assert_eq!(cell.raw_text(), "١٬٢٥٠٫٥٠ ج.م");
        assert_eq!(
            cell.value(),
            &Value::Money(MoneyValue::new(amount, Some("EGP".to_owned())))
        );
        assert_eq!(cell.value().to_canonical_string(), "1250.50 EGP");
    }

    #[test]
    fn identifiers_national_ids_and_phones_never_strip_leading_zeros() {
        let id_cell = CellValue::identifier("0004821");
        assert_eq!(id_cell.raw_text(), "0004821");
        assert_eq!(
            id_cell.value(),
            &Value::Identifier(ZeroPreservedText::new("0004821"))
        );
        if let Value::Identifier(preserved) = id_cell.value() {
            assert!(preserved.has_leading_zero());
            assert_eq!(preserved.leading_zero_count(), 3);
            assert_eq!(preserved.as_str(), "0004821");
        } else {
            panic!("expected Value::Identifier");
        }

        let phone_cell = CellValue::phone(
            "01012345678",
            "01012345678",
            Some("+201012345678".to_owned()),
        );
        assert_eq!(phone_cell.raw_text(), "01012345678");
        assert_eq!(phone_cell.matching_text(), "+201012345678");
        if let Value::Phone {
            raw_preserved,
            e164,
        } = phone_cell.value()
        {
            assert!(raw_preserved.has_leading_zero());
            assert_eq!(raw_preserved.as_str(), "01012345678");
            assert_eq!(e164.as_deref(), Some("+201012345678"));
        } else {
            panic!("expected Value::Phone");
        }

        let nid_cell = CellValue::national_id("29901010101234", "29901010101234");
        assert_eq!(
            nid_cell.value(),
            &Value::NationalId(ZeroPreservedText::new("29901010101234"))
        );

        let arabic_zero_id = ZeroPreservedText::new("٠٠٩٨٧");
        assert!(arabic_zero_id.has_leading_zero());
        assert_eq!(arabic_zero_id.leading_zero_count(), 2);
    }

    #[test]
    fn column_type_covers_all_12_known_types_plus_unknown_per_section_4_3() {
        assert_eq!(ColumnType::KNOWN_TYPES.len(), 12);
        assert_eq!(ColumnType::ALL.len(), 13);
        assert_eq!(ColumnType::default(), ColumnType::Unknown);
        assert_eq!(ColumnType::Unknown.label_ar(), "غير معروف");

        // Leading-zero preservation rule
        assert!(ColumnType::Identifier.preserves_leading_zeros());
        assert!(ColumnType::EgyptianNationalId.preserves_leading_zeros());
        assert!(ColumnType::EgyptianPhone.preserves_leading_zeros());
        assert!(!ColumnType::MoneyAndCurrency.preserves_leading_zeros());

        // Identity columns never auto-filled (Section 4.7)
        assert!(ColumnType::PersonName.is_identity_column());
        assert!(ColumnType::EgyptianPhone.is_identity_column());
        assert!(ColumnType::EgyptianNationalId.is_identity_column());
        assert!(ColumnType::EmailOrUrl.is_identity_column());
        assert!(ColumnType::Identifier.is_identity_column());
        assert!(!ColumnType::Category.is_identity_column());

        // Exact decimal requirement
        assert!(ColumnType::MoneyAndCurrency.requires_exact_decimal());
        assert!(ColumnType::Percentage.requires_exact_decimal());
        assert!(ColumnType::CoordinateOrMeasurement.requires_exact_decimal());
    }

    #[test]
    fn confidence_score_enforces_bounds_and_thresholds_0_8_and_0_5() {
        assert_eq!(MAX_COLUMN_SAMPLE_VALUES, 10_000);
        assert!((AUTO_APPLY_CONFIDENCE_THRESHOLD - 0.8).abs() < f64::EPSILON);
        assert!((USER_CONFIRM_CONFIDENCE_THRESHOLD - 0.5).abs() < f64::EPSILON);

        let high = ConfidenceScore::new(0.8).ok();
        assert!(high.is_some());
        if let Some(score) = high {
            assert_eq!(score.decision(), ConfidenceDecision::AutoApply);
            assert!(score.should_auto_apply());
            assert_eq!(
                score.resolve_column_type(ColumnType::EgyptianPhone),
                ColumnType::EgyptianPhone
            );
        }

        let medium = ConfidenceScore::new(0.65).ok();
        assert!(medium.is_some());
        if let Some(score) = medium {
            assert_eq!(score.decision(), ConfidenceDecision::UserConfirm);
            assert!(score.requires_user_confirmation());
            assert_eq!(
                score.resolve_column_type(ColumnType::Date),
                ColumnType::Date
            );
        }

        let low = ConfidenceScore::new(0.49).ok();
        assert!(low.is_some());
        if let Some(score) = low {
            assert_eq!(score.decision(), ConfidenceDecision::UnknownFallback);
            assert!(score.is_unknown_fallback());
            // Below 0.5 automatically falls back to Unknown per Section 4.3
            assert_eq!(
                score.resolve_column_type(ColumnType::Date),
                ColumnType::Unknown
            );
        }

        // Out-of-bounds or NaN values are rejected with SF-PRF-002
        for invalid in [-0.01, 1.01, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let err = ConfidenceScore::new(invalid).err();
            assert!(err.is_some());
            if let Some(e) = err {
                assert_eq!(e.code(), ErrorCode::SF_PRF_002);
            }
        }
    }

    #[test]
    fn table_and_sheet_validate_rectangular_shape_and_quality_weights() {
        assert_eq!(DataQualityWeights::TOTAL, 100);

        let high_conf = ConfidenceScore::ONE;
        let col_id = Column::new(
            ColumnSchema::new(0, "المعرف", ColumnType::Identifier, high_conf)
                .with_hxl_tag("#beneficiary+id"),
            vec![CellValue::identifier("001"), CellValue::identifier("002")],
        );
        let col_name = Column::new(
            ColumnSchema::new(1, "الاسم", ColumnType::PersonName, high_conf),
            vec![
                CellValue::from_raw("أحمد محمد"),
                CellValue::from_raw("لا يوجد"),
            ],
        );

        let table_res = Table::new("tbl_1", "المستفيدون", vec![col_id, col_name]);
        assert!(table_res.is_ok());
        if let Ok(table) = table_res {
            assert_eq!(table.row_count(), 2);
            assert_eq!(table.column_count(), 2);
            assert_eq!(table.total_cells(), 4);
            assert_eq!(table.cell(0, 0).map(CellValue::raw_text), Some("001"));
            assert_eq!(
                table.column_by_name("الاسم").map(Column::empty_cell_count),
                Some(1)
            );

            let sheet = Sheet::from_single_table(0, "ورقة1", table);
            assert_eq!(sheet.total_rows(), 2);
            assert_eq!(sheet.tables().len(), 1);
        }

        // Mismatched row count must fail with SF-SYS-001
        let short_col = Column::new(
            ColumnSchema::new(0, "أ", ColumnType::Category, high_conf),
            vec![CellValue::from_raw("1")],
        );
        let long_col = Column::new(
            ColumnSchema::new(1, "ب", ColumnType::Category, high_conf),
            vec![CellValue::from_raw("1"), CellValue::from_raw("2")],
        );
        let mismatch_err = Table::new("tbl_bad", "جدول", vec![short_col, long_col]).err();
        assert!(mismatch_err.is_some());
        if let Some(err) = mismatch_err {
            assert_eq!(err.code(), ErrorCode::SF_SYS_001);
        }
    }

    #[test]
    fn error_code_and_safe_log_record_match_section_15_without_any_user_data() {
        assert_eq!(ErrorDomain::ALL.len(), 15);

        let parsed = ErrorCode::from_str("SF-IMP-014");
        assert_eq!(parsed, ErrorCode::new(ErrorDomain::Imp, 14));
        if let Ok(code) = parsed {
            assert_eq!(code.to_string(), "SF-IMP-014");
            let token = DiagnosticToken::new("7K2Q9");
            assert!(token.is_some());
            if let Some(tok) = token {
                let err = SafiError::new(
                    code,
                    ErrorSeverity::Error,
                    "UnsupportedEncoding",
                    "import.failed",
                )
                .with_diagnostic_token(tok)
                .with_metrics(SafeLogMetrics {
                    rows_read: Some(18_420),
                    columns_count: None,
                    cells_affected: None,
                    duration_ms: Some(912),
                });

                assert_eq!(err.user_diagnostic_badge(), "SF-IMP-014 · 7K2Q9");

                let log_record = err.to_log_record(SafeLogContext {
                    timestamp_utc: "2026-10-29T12:41:07.215Z",
                    service: "engine",
                    env: "prod",
                    version: "1.0.3+a3f91c2",
                    request_id: "req_9f2a1c",
                    trace_id: None,
                    user_id: Some("u_4821"),
                    workspace_id: None,
                    device_id: None,
                    platform: "windows",
                    app_version: None,
                    locale: "ar-EG",
                });

                let json = log_record.to_json_line();
                assert_eq!(
                    json,
                    r#"{"timestamp":"2026-10-29T12:41:07.215Z","level":"error","service":"engine","env":"prod","version":"1.0.3+a3f91c2","request_id":"req_9f2a1c","user_id":"u_4821","event":"import.failed","error_code":"SF-IMP-014","error_class":"UnsupportedEncoding","rows_read":18420,"duration_ms":912,"platform":"windows","locale":"ar-EG"}"#
                );
            }
        }

        // Invalid error codes are rejected
        assert_eq!(
            ErrorCode::from_str("ERR-IMP-001"),
            Err(ErrorCodeParseError::InvalidPrefix)
        );
        assert_eq!(
            ErrorCode::from_str("SF-BAD-001"),
            Err(ErrorCodeParseError::UnknownDomain)
        );
        assert_eq!(
            ErrorCode::from_str("SF-IMP-000"),
            Err(ErrorCodeParseError::InvalidNumber)
        );
        assert_eq!(
            ErrorCode::from_str("SF-IMP-12"),
            Err(ErrorCodeParseError::InvalidNumber)
        );

        // DiagnosticToken rejects file paths, spaces, or lowercase user text
        assert!(DiagnosticToken::new("secret_file.xlsx").is_none());
        assert!(DiagnosticToken::new("رواتب الموظفين").is_none());
    }

    #[test]
    fn platform_limits_enforce_section_20_4_bounds() {
        assert_eq!(WEB_PWA_MAX_FILE_BYTES, 250 * 1024 * 1024);
        assert_eq!(WEB_PWA_MAX_ROWS, 500_000);
        assert_eq!(DESKTOP_CLI_MAX_FILE_BYTES, 10 * 1024 * 1024 * 1024);
        assert_eq!(DESKTOP_CLI_MAX_ROWS, 10_000_000);

        let web_limits = PlatformTarget::WebPwa.limits();
        assert!(
            web_limits
                .validate(WEB_PWA_MAX_FILE_BYTES, WEB_PWA_MAX_ROWS)
                .is_ok()
        );

        let over_size = web_limits.validate(WEB_PWA_MAX_FILE_BYTES + 1, 100).err();
        assert!(over_size.is_some());
        if let Some(err) = over_size {
            assert_eq!(err.code(), ErrorCode::SF_IMP_003);
            assert_eq!(err.error_class(), "PlatformFileSizeLimitExceeded");
        }

        let over_rows = web_limits.validate(1024, WEB_PWA_MAX_ROWS + 1).err();
        assert!(over_rows.is_some());
        if let Some(err) = over_rows {
            assert_eq!(err.code(), ErrorCode::SF_IMP_003);
            assert_eq!(err.error_class(), "PlatformRowCountLimitExceeded");
        }

        let desktop_limits = PlatformTarget::Desktop.limits();
        assert!(
            desktop_limits
                .validate(DESKTOP_CLI_MAX_FILE_BYTES, DESKTOP_CLI_MAX_ROWS)
                .is_ok()
        );
        assert_eq!(PlatformTarget::Cli.limits(), desktop_limits);
    }
}
