//! Delta and undo tracking for data cleaning operations (Section 4.10).

use sf_core::table::Table;
use sf_core::value::CellValue;

/// Records a change to a single cell (row, col) from old value to new value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellDelta {
    pub row: usize,
    pub col: usize,
    pub old_value: CellValue,
    pub new_value: CellValue,
}

/// Represents the delta of an entire cleaning operation, allowing byte-identical undo.
#[derive(Debug, Clone, PartialEq)]
pub struct CleanDelta {
    pub operation_name: String,
    pub cell_deltas: Vec<CellDelta>,
    pub snapshot_before: Option<Table>,
    pub affected_cells_count: usize,
    pub duration_ms: u64,
}
