//! Five cleaning operations and delta-based undo engine
//! adhering to Specification Sections 4.5, 4.6, 4.7, and 4.10.

use std::collections::HashSet;
use std::time::Instant;

use sf_arabic::{ArabicNormalizer, NormOptions};
use sf_core::column::{Column, ColumnType};
use sf_core::error::{ErrorCode, ErrorSeverity, SafiError};
use sf_core::table::Table;
use sf_core::value::CellValue;

use crate::delta::{CellDelta, CleanDelta};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanOperation {
    ArabicNormalization,
    Deduplication,
    FillBlanks,
    DateNormalization,
    NumberAndPhoneNormalization,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlankFillStrategy {
    PreviousValue,
    Mean,
    Median,
    Mode,
    LeaveBlank,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeduplicateKeep {
    First,
    Complete,
    Latest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateAmbiguityHandling {
    ReportErrorSFCLN001,
    AssumeDMY,
    AssumeMDY,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CleanOptions {
    pub update_display_text: bool,
    pub blank_strategy: BlankFillStrategy,
    pub dedup_keep: DeduplicateKeep,
    pub date_ambiguity: DateAmbiguityHandling,
}

impl Default for CleanOptions {
    fn default() -> Self {
        Self {
            update_display_text: false, // Rule #1: Original text is preserved by default
            blank_strategy: BlankFillStrategy::LeaveBlank,
            dedup_keep: DeduplicateKeep::First,
            date_ambiguity: DateAmbiguityHandling::ReportErrorSFCLN001,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CleanEngine {
    pub options: CleanOptions,
}

impl CleanEngine {
    #[must_use]
    pub const fn new(options: CleanOptions) -> Self {
        Self { options }
    }

    /// Operation 1: Arabic Normalization (Section 4.4)
    pub fn arabic_normalize_table(&self, table: &Table) -> Result<(Table, CleanDelta), SafiError> {
        let start = Instant::now();
        let normalizer = ArabicNormalizer::new(NormOptions::default());
        let mut new_columns = Vec::new();
        let mut cell_deltas = Vec::new();

        for (col_idx, col) in table.columns().iter().enumerate() {
            let schema = col.schema();
            let mut new_cells = Vec::with_capacity(col.len());
            for (row_idx, cell) in col.cells().iter().enumerate() {
                let raw = cell.raw_text();
                let normalized = normalizer.normalize(raw);
                if raw != normalized {
                    let new_cell = if self.options.update_display_text {
                        CellValue::from_raw(&normalized).with_normalized(&normalized)
                    } else {
                        cell.clone().with_normalized(&normalized)
                    };
                    cell_deltas.push(CellDelta {
                        row: row_idx,
                        col: col_idx,
                        old_value: cell.clone(),
                        new_value: new_cell.clone(),
                    });
                    new_cells.push(new_cell);
                } else {
                    new_cells.push(cell.clone());
                }
            }
            new_columns.push(Column::new(schema.clone(), new_cells));
        }

        let affected = cell_deltas.len();
        let new_table = Table::new(
            table.id().to_string(),
            table.name().to_string(),
            new_columns,
        )?;
        let delta = CleanDelta {
            operation_name: "ArabicNormalization".to_string(),
            cell_deltas,
            snapshot_before: None,
            affected_cells_count: affected,
            duration_ms: u64::try_from(start.elapsed().as_millis()).unwrap_or(0),
        };

        Ok((new_table, delta))
    }

    /// Operation 2: Deduplication (Section 4.6)
    pub fn deduplicate_table(&self, table: &Table) -> Result<(Table, CleanDelta), SafiError> {
        let start = Instant::now();
        let cols = table.columns();
        let row_count = table.row_count();

        let mut seen = HashSet::new();
        let mut keep_rows = Vec::new();
        let mut cell_deltas = Vec::new();

        for r in 0..row_count {
            let mut row_key = String::new();
            for col in cols {
                if let Some(cell) = col.cell_at(r) {
                    row_key.push_str(cell.matching_text());
                    row_key.push('|');
                }
            }

            if seen.insert(row_key) {
                keep_rows.push(r);
            } else {
                cell_deltas.push(CellDelta {
                    row: r,
                    col: 0,
                    old_value: CellValue::from_raw("duplicate_row"),
                    new_value: CellValue::from_raw(""),
                });
            }
        }

        let mut new_columns = Vec::new();
        for col in cols {
            let old_cells = col.cells();
            let new_cells: Vec<CellValue> =
                keep_rows.iter().map(|&r| old_cells[r].clone()).collect();
            new_columns.push(Column::new(col.schema().clone(), new_cells));
        }

        let new_table = Table::new(
            table.id().to_string(),
            table.name().to_string(),
            new_columns,
        )?;
        let affected = cell_deltas.len();
        let delta = CleanDelta {
            operation_name: "Deduplication".to_string(),
            cell_deltas,
            snapshot_before: Some(table.clone()),
            affected_cells_count: affected,
            duration_ms: u64::try_from(start.elapsed().as_millis()).unwrap_or(0),
        };

        Ok((new_table, delta))
    }

    /// Operation 3: Fill Blanks (Section 4.7)
    pub fn fill_blanks_table(&self, table: &Table) -> Result<(Table, CleanDelta), SafiError> {
        let start = Instant::now();
        let cols = table.columns();
        let mut new_columns = Vec::new();
        let mut cell_deltas = Vec::new();

        let blank_literals = ["-", "N/A", "n/a", "لا يوجد", "غير متوفر", ""];

        for (col_idx, col) in cols.iter().enumerate() {
            let schema = col.schema();
            // Identity columns are never filled (Section 4.7 rule: "أعمدة الهوية لا تُملأ أبدًا")
            if schema.column_type.is_identity_column() {
                new_columns.push(col.clone());
                continue;
            }

            let mut new_cells = Vec::with_capacity(col.len());
            let cells = col.cells();
            let mut last_valid = CellValue::from_raw("");

            for (row_idx, cell) in cells.iter().enumerate() {
                let raw = cell.raw_text().trim();
                let is_blank = blank_literals.contains(&raw);

                if is_blank {
                    let filled_cell = match self.options.blank_strategy {
                        BlankFillStrategy::PreviousValue => last_valid.clone(),
                        _ => CellValue::from_raw(""),
                    };

                    if filled_cell != *cell {
                        cell_deltas.push(CellDelta {
                            row: row_idx,
                            col: col_idx,
                            old_value: cell.clone(),
                            new_value: filled_cell.clone(),
                        });
                    }
                    new_cells.push(filled_cell);
                } else {
                    last_valid = cell.clone();
                    new_cells.push(cell.clone());
                }
            }
            new_columns.push(Column::new(schema.clone(), new_cells));
        }

        let affected = cell_deltas.len();
        let new_table = Table::new(
            table.id().to_string(),
            table.name().to_string(),
            new_columns,
        )?;
        let delta = CleanDelta {
            operation_name: "FillBlanks".to_string(),
            cell_deltas,
            snapshot_before: None,
            affected_cells_count: affected,
            duration_ms: u64::try_from(start.elapsed().as_millis()).unwrap_or(0),
        };

        Ok((new_table, delta))
    }

    /// Operation 4: Date Normalization (Section 4.5)
    pub fn normalize_dates_table(&self, table: &Table) -> Result<(Table, CleanDelta), SafiError> {
        let start = Instant::now();
        let cols = table.columns();
        let mut new_columns = Vec::new();
        let mut cell_deltas = Vec::new();

        for (col_idx, col) in cols.iter().enumerate() {
            let schema = col.schema();
            if schema.column_type != ColumnType::Date && !schema.name.contains("تاريخ") {
                new_columns.push(col.clone());
                continue;
            }

            let mut new_cells = Vec::with_capacity(col.len());
            for (row_idx, cell) in col.cells().iter().enumerate() {
                let raw = cell.raw_text().trim();
                if raw.is_empty() {
                    new_cells.push(cell.clone());
                    continue;
                }

                // Basic ISO or date parsing check
                let normalized = if raw.len() == 10 && raw.chars().nth(4) == Some('-') {
                    raw.to_string()
                } else if raw.contains('/') {
                    let parts: Vec<&str> = raw.split('/').collect();
                    if parts.len() == 3 {
                        let p0 = parts[0].parse::<u32>().unwrap_or(0);
                        let p1 = parts[1].parse::<u32>().unwrap_or(0);
                        let p2 = parts[2].parse::<u32>().unwrap_or(0);

                        if p0 > 12 {
                            format!("{p2:04}-{p1:02}-{p0:02}")
                        } else if p1 > 12 {
                            format!("{p2:04}-{p0:02}-{p1:02}")
                        } else {
                            match self.options.date_ambiguity {
                                DateAmbiguityHandling::ReportErrorSFCLN001 => {
                                    return Err(SafiError::new(
                                        ErrorCode::SF_CLN_001,
                                        ErrorSeverity::Warn,
                                        "AmbiguousDateEncountered",
                                        "clean.ambiguous_date",
                                    ));
                                }
                                DateAmbiguityHandling::AssumeDMY => {
                                    format!("{p2:04}-{p1:02}-{p0:02}")
                                }
                                DateAmbiguityHandling::AssumeMDY => {
                                    format!("{p2:04}-{p0:02}-{p1:02}")
                                }
                            }
                        }
                    } else {
                        raw.to_string()
                    }
                } else {
                    raw.to_string()
                };

                let new_cell = CellValue::from_raw(&normalized);
                if new_cell != *cell {
                    cell_deltas.push(CellDelta {
                        row: row_idx,
                        col: col_idx,
                        old_value: cell.clone(),
                        new_value: new_cell.clone(),
                    });
                }
                new_cells.push(new_cell);
            }
            new_columns.push(Column::new(schema.clone(), new_cells));
        }

        let affected = cell_deltas.len();
        let new_table = Table::new(
            table.id().to_string(),
            table.name().to_string(),
            new_columns,
        )?;
        let delta = CleanDelta {
            operation_name: "DateNormalization".to_string(),
            cell_deltas,
            snapshot_before: None,
            affected_cells_count: affected,
            duration_ms: u64::try_from(start.elapsed().as_millis()).unwrap_or(0),
        };

        Ok((new_table, delta))
    }

    /// Operation 5: Number and Phone Normalization (Section 4.5)
    pub fn normalize_numbers_phones_table(
        &self,
        table: &Table,
    ) -> Result<(Table, CleanDelta), SafiError> {
        let start = Instant::now();
        let cols = table.columns();
        let mut new_columns = Vec::new();
        let mut cell_deltas = Vec::new();

        for (col_idx, col) in cols.iter().enumerate() {
            let schema = col.schema();
            let mut new_cells = Vec::with_capacity(col.len());
            for (row_idx, cell) in col.cells().iter().enumerate() {
                let raw = cell.raw_text().trim();
                if raw.is_empty() {
                    new_cells.push(cell.clone());
                    continue;
                }

                // If column preserves leading zeros (Phones, IDs), do not strip leading zeros
                let normalized = if schema.column_type.preserves_leading_zeros() {
                    raw.to_string()
                } else if raw.starts_with("01") && raw.len() == 11 {
                    format!("+20{}", &raw[1..])
                } else {
                    raw.to_string()
                };

                let new_cell = CellValue::from_raw(&normalized);
                if new_cell != *cell {
                    cell_deltas.push(CellDelta {
                        row: row_idx,
                        col: col_idx,
                        old_value: cell.clone(),
                        new_value: new_cell.clone(),
                    });
                }
                new_cells.push(new_cell);
            }
            new_columns.push(Column::new(schema.clone(), new_cells));
        }

        let affected = cell_deltas.len();
        let new_table = Table::new(
            table.id().to_string(),
            table.name().to_string(),
            new_columns,
        )?;
        let delta = CleanDelta {
            operation_name: "NumberAndPhoneNormalization".to_string(),
            cell_deltas,
            snapshot_before: None,
            affected_cells_count: affected,
            duration_ms: u64::try_from(start.elapsed().as_millis()).unwrap_or(0),
        };

        Ok((new_table, delta))
    }

    /// Delta-based Undo implementation (Section 4.10)
    pub fn undo_delta(table: &Table, delta: &CleanDelta) -> Result<Table, SafiError> {
        if let Some(snapshot) = &delta.snapshot_before {
            return Ok(snapshot.clone());
        }

        let cols = table.columns();
        let mut grid: Vec<Vec<CellValue>> = cols.iter().map(|c| c.cells().to_vec()).collect();

        for cd in &delta.cell_deltas {
            if let Some(col_cells) = grid.get_mut(cd.col) {
                if cd.row < col_cells.len() {
                    col_cells[cd.row] = cd.old_value.clone();
                }
            }
        }

        let mut new_columns = Vec::new();
        for (col, cells) in cols.iter().zip(grid) {
            new_columns.push(Column::new(col.schema().clone(), cells));
        }

        Table::new(
            table.id().to_string(),
            table.name().to_string(),
            new_columns,
        )
    }
}
