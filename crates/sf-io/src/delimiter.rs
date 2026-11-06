//! Delimiter detection (`comma`, `semicolon`, `tab`) for CSV/delimited files
//! adhering to Specification Section 4.2.

/// Detects the most likely delimiter (`b','`, `b';'`, or `b'\t'`) from header text sample.
pub fn detect_delimiter(sample: &str) -> u8 {
    let commas = sample.matches(',').count();
    let semicolons = sample.matches(';').count();
    let tabs = sample.matches('\t').count();

    if tabs > commas && tabs >= semicolons {
        b'\t'
    } else if semicolons > commas && semicolons >= tabs {
        b';'
    } else {
        b','
    }
}
