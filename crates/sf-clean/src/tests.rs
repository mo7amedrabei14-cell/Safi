//! Unit tests and proptest idempotence & undo validation for `sf-clean`.

use crate::{CleanEngine, CleanOptions, DateAmbiguityHandling};
use proptest::prelude::*;
use rust_decimal::Decimal;
use sf_core::column::{Column, ColumnSchema, ColumnType, ConfidenceScore};
use sf_core::error::{ErrorCode, SafiError};
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
            assert_eq!(first_cell.map(CellValue::raw_text), Some("أَحْمَدُ"));
            assert_eq!(
                first_cell.map(CellValue::visible_text).as_deref(),
                Some("احمد")
            );
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

#[test]
fn normalization_preserves_typed_values_and_display_choice() -> Result<(), SafiError> {
    let cell = CellValue::money(" 10.00 ", Decimal::new(1000, 2), Some("EGP".into()));
    let table = Table::new(
        "synthetic",
        "synthetic",
        vec![Column::new(
            ColumnSchema::new(
                0,
                "amount",
                ColumnType::MoneyAndCurrency,
                ConfidenceScore::ONE,
            ),
            vec![cell.clone()],
        )],
    )?;

    for update_display_text in [false, true] {
        let engine = CleanEngine::new(CleanOptions {
            update_display_text,
            ..Default::default()
        });
        let (cleaned, delta) = engine.arabic_normalize_table(&table)?;
        let cleaned_cell = cleaned.cell(0, 0);
        assert_eq!(cleaned_cell.map(CellValue::raw_text), Some(" 10.00 "));
        assert_eq!(cleaned_cell.map(CellValue::value), Some(cell.value()));
        assert_eq!(cleaned_cell.map(CellValue::matching_text), Some("10.00"));
        assert_eq!(
            cleaned_cell.map(CellValue::visible_text).as_deref(),
            Some(if update_display_text {
                "10.00"
            } else {
                " 10.00 "
            }),
        );
        let (again, second_delta) = engine.arabic_normalize_table(&cleaned)?;
        assert_eq!(again, cleaned);
        assert!(second_delta.cell_deltas.is_empty());
        assert_eq!(CleanEngine::undo_delta(&cleaned, &delta)?, table);
    }
    Ok(())
}

#[test]
fn date_normalization_preserves_source_and_is_idempotent() -> Result<(), SafiError> {
    let table = Table::new(
        "synthetic",
        "synthetic",
        vec![Column::new(
            ColumnSchema::new(0, "date", ColumnType::Date, ConfidenceScore::ONE),
            vec![CellValue::from_raw(" 23/05/2026 ")],
        )],
    )?;
    for update_display_text in [false, true] {
        let engine = CleanEngine::new(CleanOptions {
            update_display_text,
            ..Default::default()
        });
        let (cleaned, delta) = engine.normalize_dates_table(&table)?;
        assert_eq!(
            cleaned.cell(0, 0).map(CellValue::raw_text),
            Some(" 23/05/2026 ")
        );
        assert_eq!(
            cleaned.cell(0, 0).map(CellValue::matching_text),
            Some("2026-05-23")
        );
        assert_eq!(
            cleaned.cell(0, 0).map(CellValue::visible_text).as_deref(),
            Some(if update_display_text {
                "2026-05-23"
            } else {
                " 23/05/2026 "
            }),
        );
        let (again, second_delta) = engine.normalize_dates_table(&cleaned)?;
        assert_eq!(again, cleaned);
        assert!(second_delta.cell_deltas.is_empty());
        assert_eq!(CleanEngine::undo_delta(&cleaned, &delta)?, table);
    }
    Ok(())
}

#[test]
fn number_normalization_preserves_typed_money_and_identifiers() -> Result<(), SafiError> {
    let cells = [
        CellValue::money(" 123.50 ", Decimal::new(12350, 2), Some("EGP".into())),
        CellValue::identifier(" 00123 "),
        CellValue::from_raw(" 01012345678 "),
    ];
    let types = [
        ColumnType::MoneyAndCurrency,
        ColumnType::Identifier,
        ColumnType::Unknown,
    ];
    let columns = cells
        .iter()
        .zip(types)
        .enumerate()
        .map(|(index, (cell, column_type))| {
            Column::new(
                ColumnSchema::new(index, "synthetic", column_type, ConfidenceScore::ONE),
                vec![cell.clone()],
            )
        })
        .collect();
    let table = Table::new("synthetic", "synthetic", columns)?;
    for update_display_text in [false, true] {
        let engine = CleanEngine::new(CleanOptions {
            update_display_text,
            ..Default::default()
        });
        let (cleaned, delta) = engine.normalize_numbers_phones_table(&table)?;
        for (index, expected) in ["123.50", "00123", "+201012345678"].into_iter().enumerate() {
            let result = cleaned.cell(0, index);
            assert_eq!(
                result.map(CellValue::raw_text),
                Some(cells[index].raw_text())
            );
            assert_eq!(result.map(CellValue::value), Some(cells[index].value()));
            assert_eq!(result.map(CellValue::matching_text), Some(expected));
            assert_eq!(
                result.map(CellValue::visible_text).as_deref(),
                Some(if update_display_text {
                    expected
                } else {
                    cells[index].raw_text()
                })
            );
        }
        let (again, second_delta) = engine.normalize_numbers_phones_table(&cleaned)?;
        assert_eq!(again, cleaned);
        assert!(second_delta.cell_deltas.is_empty());
        assert_eq!(CleanEngine::undo_delta(&cleaned, &delta)?, table);
    }
    Ok(())
}

#[test]
fn normalization_composes_with_existing_normalized_text() -> Result<(), SafiError> {
    let table = Table::new(
        "synthetic",
        "synthetic",
        vec![Column::new(
            ColumnSchema::new(0, "synthetic", ColumnType::Unknown, ConfidenceScore::ONE),
            vec![CellValue::from_raw("source").with_normalized(" 01012345678 ")],
        )],
    )?;
    let engine = CleanEngine::default();
    let (trimmed, first_delta) = engine.arabic_normalize_table(&table)?;
    let (normalized, second_delta) = engine.normalize_numbers_phones_table(&trimmed)?;
    assert_eq!(
        normalized.cell(0, 0).map(CellValue::raw_text),
        Some("source")
    );
    assert_eq!(
        normalized.cell(0, 0).map(CellValue::matching_text),
        Some("+201012345678")
    );
    assert_eq!(
        CleanEngine::undo_delta(&normalized, &second_delta)?,
        trimmed
    );
    assert_eq!(CleanEngine::undo_delta(&trimmed, &first_delta)?, table);
    Ok(())
}

#[test]
fn compound_name_normalization_is_idempotent_with_mixed_whitespace() -> Result<(), SafiError> {
    let table = Table::new(
        "synthetic",
        "synthetic",
        vec![Column::new(
            ColumnSchema::new(0, "name", ColumnType::PersonName, ConfidenceScore::ONE),
            ["عبد\tالله", "عبد  الله", "عبد\u{200f} الله", "أبو\nبكر"]
                .into_iter()
                .map(CellValue::from_raw)
                .collect(),
        )],
    )?;
    let engine = CleanEngine::default();
    let (cleaned, delta) = engine.arabic_normalize_table(&table)?;
    let (again, second_delta) = engine.arabic_normalize_table(&cleaned)?;
    assert_eq!(again, cleaned);
    assert!(second_delta.cell_deltas.is_empty());
    for (row, expected) in ["عبدالله", "عبدالله", "عبدالله", "ابوبكر"]
        .into_iter()
        .enumerate()
    {
        assert_eq!(
            cleaned.cell(row, 0).map(CellValue::matching_text),
            Some(expected)
        );
        assert_eq!(
            cleaned.cell(row, 0).map(CellValue::raw_text),
            table.cell(row, 0).map(CellValue::raw_text)
        );
    }
    assert_eq!(CleanEngine::undo_delta(&cleaned, &delta)?, table);
    Ok(())
}

#[test]
fn normalization_can_change_only_display_and_undo_it() -> Result<(), SafiError> {
    let table = Table::new(
        "synthetic",
        "synthetic",
        vec![Column::new(
            ColumnSchema::new(0, "date", ColumnType::Date, ConfidenceScore::ONE),
            vec![
                CellValue::from_raw(" 2026-05-23 ").with_normalized("2026-05-23"),
                CellValue::from_raw("2026-05-24"),
                CellValue::from_raw(""),
                CellValue::from_raw("ـ").with_normalized(""),
            ],
        )],
    )?;
    for normalize in [
        CleanEngine::arabic_normalize_table,
        CleanEngine::normalize_dates_table,
        CleanEngine::normalize_numbers_phones_table,
    ] {
        let (unchanged, no_delta) = normalize(&CleanEngine::default(), &table)?;
        assert_eq!(unchanged, table);
        assert!(no_delta.cell_deltas.is_empty());
        let engine = CleanEngine::new(CleanOptions {
            update_display_text: true,
            ..Default::default()
        });
        let (visible, delta) = normalize(&engine, &table)?;
        assert_eq!(
            visible.cell(0, 0).map(CellValue::raw_text),
            Some(" 2026-05-23 ")
        );
        assert_eq!(
            visible.cell(0, 0).map(CellValue::visible_text).as_deref(),
            Some("2026-05-23")
        );
        assert_eq!(visible.cell(1, 0), table.cell(1, 0));
        assert_eq!(visible.cell(2, 0), table.cell(2, 0));
        assert_eq!(visible.cell(3, 0).map(CellValue::raw_text), Some("ـ"));
        assert_eq!(
            visible.cell(3, 0).map(CellValue::visible_text).as_deref(),
            Some("")
        );
        assert_eq!(delta.affected_cells_count, 2);
        assert_eq!(CleanEngine::undo_delta(&visible, &delta)?, table);
        let (again, second_delta) = normalize(&CleanEngine::default(), &visible)?;
        assert_eq!(again, visible);
        assert!(second_delta.cell_deltas.is_empty());
    }
    Ok(())
}

#[test]
fn deduplication_preserves_cell_boundaries_and_undo() -> Result<(), SafiError> {
    let columns = [["a|", "a", "a|"], ["b", "|b", "b"]]
        .into_iter()
        .enumerate()
        .map(|(index, values)| {
            Column::new(
                ColumnSchema::new(
                    index,
                    "synthetic",
                    ColumnType::Unknown,
                    ConfidenceScore::ONE,
                ),
                values.into_iter().map(CellValue::from_raw).collect(),
            )
        })
        .collect();
    let table = Table::new("synthetic", "synthetic", columns)?;
    let engine = CleanEngine::default();
    let (cleaned, delta) = engine.deduplicate_table(&table)?;
    assert_eq!(cleaned.row_count(), 2);
    for row in 0..2 {
        for col in 0..2 {
            assert_eq!(cleaned.cell(row, col), table.cell(row, col));
        }
    }
    assert_eq!(delta.affected_cells_count, 1);
    assert_eq!(CleanEngine::undo_delta(&cleaned, &delta)?, table);
    let (again, second_delta) = engine.deduplicate_table(&cleaned)?;
    assert_eq!(again, cleaned);
    assert!(second_delta.cell_deltas.is_empty());
    Ok(())
}

proptest! {
    #[test]
    fn deduplication_matches_structural_row_equality(
        rows in prop::collection::vec(("[a-z|]{0,12}", "[a-z|]{0,12}"), 0..40)
    ) {
        let columns = (0..2).map(|index| Column::new(
            ColumnSchema::new(index, "synthetic", ColumnType::Unknown, ConfidenceScore::ONE),
            rows.iter().map(|(left, right)| CellValue::from_raw(if index == 0 { left } else { right })).collect(),
        )).collect();
        let table = Table::new("synthetic", "synthetic", columns)?;
        let (cleaned, delta) = CleanEngine::default().deduplicate_table(&table)?;
        let mut seen = std::collections::HashSet::new();
        let expected: Vec<_> = rows.iter().filter(|row| seen.insert(*row)).collect();
        prop_assert_eq!(cleaned.row_count(), expected.len());
        for (index, (left, right)) in expected.iter().enumerate() {
            prop_assert_eq!(cleaned.cell(index, 0).map(CellValue::raw_text), Some(left.as_str()));
            prop_assert_eq!(cleaned.cell(index, 1).map(CellValue::raw_text), Some(right.as_str()));
        }
        prop_assert_eq!(CleanEngine::undo_delta(&cleaned, &delta)?, table);
    }
}
