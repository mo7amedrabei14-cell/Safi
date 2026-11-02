//! Core `Table`, `Sheet`, and `DataQualityWeights` structures
//! implementing Specification Sections 4.2, 4.4, and 4.11.

use crate::column::Column;
use crate::error::{ErrorCode, ErrorSeverity, SafeLogMetrics, SafiError};
use crate::value::CellValue;

/// Official data quality score dimension weights (0..=100 total) from Specification Section 4.11:
/// «الاكتمال 25، الاتساق 25، الصلاحية 20، التفرّد 15، المنطقية 15».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataQualityWeights;

impl DataQualityWeights {
    /// الاكتمال (`Completeness` = `25`).
    pub const COMPLETENESS: u8 = 25;
    /// الاتساق (`Consistency` = `25`).
    pub const CONSISTENCY: u8 = 25;
    /// الصلاحية (`Validity` = `20`).
    pub const VALIDITY: u8 = 20;
    /// التفرّد (`Uniqueness` = `15`).
    pub const UNIQUENESS: u8 = 15;
    /// المنطقية (`Logical Consistency` = `15`).
    pub const LOGIC: u8 = 15;
    /// Total maximum data quality score (`100`).
    pub const TOTAL: u8 =
        Self::COMPLETENESS + Self::CONSISTENCY + Self::VALIDITY + Self::UNIQUENESS + Self::LOGIC;
}

/// Rectangular table of [`Column`]s where every column is guaranteed to have the same row count.
#[derive(Debug, Clone, PartialEq)]
pub struct Table {
    id: String,
    name: String,
    columns: Vec<Column>,
    row_count: usize,
}

impl Table {
    /// Constructs a validated `Table`, verifying that all columns have identical row lengths.
    ///
    /// Returns `Err(SafiError)` with code `SF-SYS-001` (and safe numeric metrics only) if two
    /// columns have mismatched row counts.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        columns: Vec<Column>,
    ) -> Result<Self, SafiError> {
        let row_count = match columns.first() {
            Some(first) => first.len(),
            None => 0,
        };
        for col in &columns {
            if col.len() != row_count {
                let col_count_u32 = u32::try_from(columns.len()).unwrap_or(u32::MAX);
                return Err(SafiError::new(
                    ErrorCode::SF_SYS_001,
                    ErrorSeverity::Error,
                    "ColumnRowCountMismatch",
                    "table.invalid_shape",
                )
                .with_metrics(SafeLogMetrics {
                    rows_read: Some(row_count as u64),
                    columns_count: Some(col_count_u32),
                    cells_affected: None,
                    duration_ms: None,
                }));
            }
        }
        Ok(Self {
            id: id.into(),
            name: name.into(),
            columns,
            row_count,
        })
    }

    /// Returns the table identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the table name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the ordered slice of [`Column`]s.
    #[must_use]
    pub fn columns(&self) -> &[Column] {
        &self.columns
    }

    /// Returns the number of columns in the table.
    #[must_use]
    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    /// Returns the number of rows in the table.
    #[must_use]
    pub const fn row_count(&self) -> usize {
        self.row_count
    }

    /// Returns the total number of cells (`row_count * column_count`).
    #[must_use]
    pub fn total_cells(&self) -> usize {
        self.row_count.saturating_mul(self.columns.len())
    }

    /// Looks up a column by zero-based index.
    #[must_use]
    pub fn column_at(&self, col_index: usize) -> Option<&Column> {
        self.columns.get(col_index)
    }

    /// Looks up a column by its exact header name.
    #[must_use]
    pub fn column_by_name(&self, header_name: &str) -> Option<&Column> {
        self.columns
            .iter()
            .find(|col| col.schema().name == header_name)
    }

    /// Returns a reference to the [`CellValue`] at `(row_index, col_index)`, if within bounds.
    #[must_use]
    pub fn cell(&self, row_index: usize, col_index: usize) -> Option<&CellValue> {
        self.columns
            .get(col_index)
            .and_then(|col| col.cell_at(row_index))
    }

    /// Produces privacy-safe aggregate metrics for structured logging (`Section 15.3`)
    /// without exposing any file name, column name, or cell value.
    #[must_use]
    pub fn safe_metrics(&self, duration_ms: Option<u64>) -> SafeLogMetrics {
        let col_count_u32 = u32::try_from(self.columns.len()).unwrap_or(u32::MAX);
        SafeLogMetrics {
            rows_read: Some(self.row_count as u64),
            columns_count: Some(col_count_u32),
            cells_affected: Some(self.total_cells() as u64),
            duration_ms,
        }
    }
}

/// Worksheet (`Sheet`) within a workbook or file, supporting one or multiple detected tables
/// per sheet as required by Specification Section 4.2 («الأوراق المتعددة، وأكثر من جدول في الورقة الواحدة»).
#[derive(Debug, Clone, PartialEq)]
pub struct Sheet {
    index: usize,
    name: String,
    tables: Vec<Table>,
}

impl Sheet {
    /// Creates a new `Sheet` with zero or more detected [`Table`]s.
    #[must_use]
    pub fn new(index: usize, name: impl Into<String>, tables: Vec<Table>) -> Self {
        Self {
            index,
            name: name.into(),
            tables,
        }
    }

    /// Convenience constructor for a `Sheet` containing a single [`Table`].
    #[must_use]
    pub fn from_single_table(index: usize, name: impl Into<String>, table: Table) -> Self {
        Self::new(index, name, vec![table])
    }

    /// Returns the zero-based sheet index within the workbook.
    #[must_use]
    pub const fn index(&self) -> usize {
        self.index
    }

    /// Returns the sheet name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns all tables detected within this sheet.
    #[must_use]
    pub fn tables(&self) -> &[Table] {
        &self.tables
    }

    /// Returns a reference to the first (primary) table in this sheet, if any.
    #[must_use]
    pub fn primary_table(&self) -> Option<&Table> {
        self.tables.first()
    }

    /// Returns the sum of rows across all tables in this sheet.
    #[must_use]
    pub fn total_rows(&self) -> usize {
        self.tables
            .iter()
            .fold(0usize, |acc, tbl| acc.saturating_add(tbl.row_count()))
    }
}
