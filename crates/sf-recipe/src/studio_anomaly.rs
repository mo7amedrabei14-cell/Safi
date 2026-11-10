//! Anomaly detection for the Studio (`STUDIO-PHASE-2`, section 4.5).
//!
//! The detector only *describes* problems: the kind, the affected rows, the method used, and a
//! suggested correction. It never edits the data. Applying a correction is an explicit
//! transformation recorded in the graph, so it can be undone.
//!
//! All messages are translation keys, not sentences, so no user-visible English leaks into the
//! engine (`AGENTS.md` Rule 8).

use std::collections::BTreeMap;

use rust_decimal::Decimal;
use sf_core::table::Table;
use sf_core::value::CellValue;

/// Category of detected problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AnomalyKind {
    /// Empty cells, including recognized empty sentinels.
    MissingValue,
    /// Two structurally identical rows.
    DuplicateRow,
    /// A column mixes numeric and non-numeric values.
    InconsistentType,
    /// A date-shaped value that is not a real calendar date.
    ImpossibleDate,
    /// A numeric value outside `1.5 × IQR` of its column.
    StatisticalOutlier,
}

impl AnomalyKind {
    /// i18n key describing the problem.
    #[must_use]
    pub const fn message_key(self) -> &'static str {
        match self {
            Self::MissingValue => "studio.anomaly.missing_value",
            Self::DuplicateRow => "studio.anomaly.duplicate_row",
            Self::InconsistentType => "studio.anomaly.inconsistent_type",
            Self::ImpossibleDate => "studio.anomaly.impossible_date",
            Self::StatisticalOutlier => "studio.anomaly.statistical_outlier",
        }
    }

    /// Identifier of the method used to detect it (shown to the user as the "why").
    #[must_use]
    pub const fn method_key(self) -> &'static str {
        match self {
            Self::MissingValue => "studio.method.empty_cell",
            Self::DuplicateRow => "studio.method.structural_row_key",
            Self::InconsistentType => "studio.method.type_consistency",
            Self::ImpossibleDate => "studio.method.calendar_validation",
            Self::StatisticalOutlier => "studio.method.iqr_1_5",
        }
    }

    /// Identifier of the suggested correction. Applying it still needs the user's confirmation.
    #[must_use]
    pub const fn suggestion_key(self) -> &'static str {
        match self {
            Self::MissingValue => "studio.fix.fill_or_remove_missing",
            Self::DuplicateRow => "studio.fix.drop_duplicate_rows",
            Self::InconsistentType => "studio.fix.confirm_column_type",
            Self::ImpossibleDate => "studio.fix.correct_or_blank_date",
            Self::StatisticalOutlier => "studio.fix.review_outlier",
        }
    }
}

/// One detected problem, with its evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anomaly {
    /// Problem category.
    pub kind: AnomalyKind,
    /// Column the problem belongs to (`None` for whole-row problems).
    pub column: Option<String>,
    /// Zero-based affected rows.
    pub rows: Vec<u64>,
    /// How many rows are affected (convenience for the UI).
    pub affected_rows: usize,
}

fn numeric(cell: &CellValue) -> Option<Decimal> {
    match cell.value() {
        sf_core::value::Value::Integer(integer) => Some(Decimal::from(*integer)),
        sf_core::value::Value::Decimal(value) | sf_core::value::Value::Percentage(value) => {
            Some(*value)
        }
        sf_core::value::Value::Money(money) => Some(money.amount),
        sf_core::value::Value::Measurement { magnitude, .. } => Some(*magnitude),
        _ => cell.matching_text().trim().parse::<Decimal>().ok(),
    }
}

fn impossible_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let year = text.get(0..4).and_then(|value| value.parse::<u32>().ok());
    let month = text.get(5..7).and_then(|value| value.parse::<u32>().ok());
    let day = text.get(8..10).and_then(|value| value.parse::<u32>().ok());
    let (Some(year), Some(month), Some(day)) = (year, month, day) else {
        return false;
    };
    if !(1..=12).contains(&month) || day == 0 {
        return true;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if leap => 29,
        _ => 28,
    };
    day > max_day
}

/// Detects anomalies in a table without modifying it.
///
/// The result is ordered by kind, then by column name, then by row, so two runs on the same data
/// produce the same output.
#[must_use]
pub fn detect_anomalies(table: &Table) -> Vec<Anomaly> {
    let mut anomalies = Vec::new();

    for column in table.columns() {
        let name = column.schema().name.clone();
        let mut missing = Vec::new();
        let mut non_numeric = Vec::new();
        let mut numeric_values: Vec<(u64, Decimal)> = Vec::new();
        let mut impossible = Vec::new();
        for (row, cell) in column.cells().iter().enumerate() {
            let row = row as u64;
            if cell.is_empty() {
                missing.push(row);
                continue;
            }
            match numeric(cell) {
                Some(value) => numeric_values.push((row, value)),
                None => non_numeric.push(row),
            }
            if impossible_date(cell.matching_text().trim()) {
                impossible.push(row);
            }
        }
        if !missing.is_empty() {
            anomalies.push(Anomaly {
                kind: AnomalyKind::MissingValue,
                column: Some(name.clone()),
                affected_rows: missing.len(),
                rows: missing,
            });
        }
        if !impossible.is_empty() {
            anomalies.push(Anomaly {
                kind: AnomalyKind::ImpossibleDate,
                column: Some(name.clone()),
                affected_rows: impossible.len(),
                rows: impossible,
            });
        }
        if !numeric_values.is_empty() && !non_numeric.is_empty() {
            anomalies.push(Anomaly {
                kind: AnomalyKind::InconsistentType,
                column: Some(name.clone()),
                affected_rows: non_numeric.len(),
                rows: non_numeric,
            });
        }
        if numeric_values.len() >= 4 {
            let mut sorted: Vec<Decimal> = numeric_values.iter().map(|(_, value)| *value).collect();
            sorted.sort();
            let count = sorted.len();
            // Nearest-rank quantiles; no floating point, so bounds are reproducible.
            let q1 = sorted[count / 4];
            let q3 = sorted[(count * 3) / 4];
            let iqr = q3 - q1;
            let lower = q1 - iqr * Decimal::new(15, 1);
            let upper = q3 + iqr * Decimal::new(15, 1);
            let outliers: Vec<u64> = numeric_values
                .iter()
                .filter(|(_, value)| *value < lower || *value > upper)
                .map(|(row, _)| *row)
                .collect();
            if !outliers.is_empty() {
                anomalies.push(Anomaly {
                    kind: AnomalyKind::StatisticalOutlier,
                    column: Some(name),
                    affected_rows: outliers.len(),
                    rows: outliers,
                });
            }
        }
    }

    let mut seen: BTreeMap<Vec<String>, u64> = BTreeMap::new();
    let mut duplicates = Vec::new();
    for row in 0..table.row_count() {
        let key: Vec<String> = table
            .columns()
            .iter()
            .filter_map(|column| column.cell_at(row))
            .map(|cell| cell.matching_text().to_owned())
            .collect();
        if seen.insert(key, row as u64).is_some() {
            duplicates.push(row as u64);
        }
    }
    if !duplicates.is_empty() {
        anomalies.push(Anomaly {
            kind: AnomalyKind::DuplicateRow,
            column: None,
            affected_rows: duplicates.len(),
            rows: duplicates,
        });
    }

    anomalies.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.column.cmp(&right.column))
            .then_with(|| left.rows.first().cmp(&right.rows.first()))
    });
    anomalies
}

#[cfg(test)]
mod tests {
    use sf_core::column::{Column, ColumnSchema, ColumnType, ConfidenceScore};
    use sf_core::error::SafiError;
    use sf_core::table::Table;
    use sf_core::value::CellValue;

    use super::{AnomalyKind, detect_anomalies};

    fn table(columns: &[(&str, &[&str])]) -> Result<Table, SafiError> {
        let columns = columns
            .iter()
            .enumerate()
            .map(|(index, (name, values))| {
                Column::new(
                    ColumnSchema::new(index, *name, ColumnType::Unknown, ConfidenceScore::ONE),
                    values
                        .iter()
                        .map(|value| CellValue::from_raw(*value))
                        .collect(),
                )
            })
            .collect();
        Table::new("tbl", "data", columns)
    }

    #[test]
    fn reports_missing_values_and_duplicate_rows_without_changing_data() -> Result<(), SafiError> {
        let table = table(&[
            ("region", &["Cairo", "", "Cairo"]),
            ("amount", &["10", "10", "10"]),
        ])?;
        let anomalies = detect_anomalies(&table);
        assert!(anomalies.iter().any(|anomaly| {
            anomaly.kind == AnomalyKind::MissingValue
                && anomaly.column.as_deref() == Some("region")
                && anomaly.rows == vec![1]
        }));
        assert!(anomalies.iter().any(|anomaly| {
            anomaly.kind == AnomalyKind::DuplicateRow && anomaly.rows == vec![2]
        }));
        assert_eq!(table.row_count(), 3);
        Ok(())
    }

    #[test]
    fn flags_mixed_types_and_impossible_dates() -> Result<(), SafiError> {
        let table = table(&[
            ("amount", &["10", "abc", "30", "40"]),
            (
                "date",
                &["2026-13-01", "2026-02-30", "2026-01-01", "2026-01-02"],
            ),
        ])?;
        let anomalies = detect_anomalies(&table);
        assert!(anomalies.iter().any(|anomaly| {
            anomaly.kind == AnomalyKind::InconsistentType
                && anomaly.column.as_deref() == Some("amount")
                && anomaly.rows == vec![1]
        }));
        let dates: Vec<u64> = anomalies
            .iter()
            .filter(|anomaly| anomaly.kind == AnomalyKind::ImpossibleDate)
            .flat_map(|anomaly| anomaly.rows.clone())
            .collect();
        assert_eq!(dates, vec![0, 1]);
        Ok(())
    }

    #[test]
    fn flags_statistical_outliers_using_iqr_bounds() -> Result<(), SafiError> {
        let table = table(&[("amount", &["10", "11", "12", "13", "1000"])])?;
        let anomalies = detect_anomalies(&table);
        let outlier = anomalies
            .iter()
            .find(|anomaly| anomaly.kind == AnomalyKind::StatisticalOutlier);
        assert_eq!(outlier.map(|anomaly| anomaly.rows.clone()), Some(vec![4]));
        Ok(())
    }

    #[test]
    fn clean_data_reports_nothing() -> Result<(), SafiError> {
        for row in [["Cairo", "1"], ["Giza", "2"], ["Cairo", "3"]] {
            assert_eq!(row.len(), 2);
        }
        let table = table(&[
            ("region", &["Cairo", "Giza", "Cairo"]),
            ("amount", &["10", "20", "30"]),
        ])?;
        assert!(detect_anomalies(&table).is_empty());
        Ok(())
    }
}
