//! Before/after cleaning summary comparison.

use serde::{Deserialize, Serialize};

/// Summary statistics comparing table before and after cleaning.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CleaningSummary {
    pub rows_before: usize,
    pub rows_after: usize,
    pub duplicates_removed: usize,
    pub blanks_filled: usize,
    pub cells_corrected: usize,
    pub quality_score_before: f64,
    pub quality_score_after: f64,
}

/// Generates a before/after cleaning comparison summary.
#[must_use]
pub fn generate_cleaning_summary(
    rows_before: usize,
    rows_after: usize,
    blanks_filled: usize,
    cells_corrected: usize,
    quality_before: f64,
    quality_after: f64,
) -> CleaningSummary {
    let duplicates_removed = rows_before.saturating_sub(rows_after);
    CleaningSummary {
        rows_before,
        rows_after,
        duplicates_removed,
        blanks_filled,
        cells_corrected,
        quality_score_before: quality_before,
        quality_score_after: quality_after,
    }
}
