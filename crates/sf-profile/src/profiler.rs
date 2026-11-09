//! Column profiling and understanding engine adhering to Specification Sections 4.3 and 16.1.

use serde::{Deserialize, Serialize};
use sf_arabic::{ArabicNormalizer, NormOptions};
use sf_core::column::{Column, ColumnType};
use sf_core::error::SafiError;
use sf_core::table::Table;
use sf_core::value::CellValue;

/// Profile result for a single column containing inferred type, confidence, evidence, and alternatives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnProfile {
    pub column_index: usize,
    pub column_name: String,
    pub inferred_type_code: String,
    pub inferred_type_label_ar: String,
    #[serde(skip)]
    pub inferred_type: ColumnType,
    pub confidence: f64, // 0.0 to 1.0
    pub evidence: Vec<String>,
    #[serde(skip)]
    pub alternatives: Vec<(ColumnType, f64)>,
    pub action_status: ProfileActionStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProfileActionStatus {
    /// Confidence >= 0.8: Applied automatically in preview
    AutoApplied,
    /// Confidence between 0.5 and 0.8: Ask user for confirmation
    NeedsUserConfirmation,
    /// Confidence < 0.5: Unknown type, requires manual classification or AI
    Unknown,
}

/// Profiles all columns in a [`Table`] and returns their [`ColumnProfile`]s.
pub fn profile_table(table: &Table) -> Vec<ColumnProfile> {
    table
        .columns()
        .iter()
        .filter_map(|col| profile_column(col).ok())
        .collect()
}

/// Profiles a column by sampling up to 10,000 values, checking column name synonyms,
/// and applying understanding thresholds (0.8 and 0.5).
pub fn profile_column(column: &Column) -> Result<ColumnProfile, SafiError> {
    let schema = column.schema();
    let col_name = &schema.name;
    let cells = column.cells();

    // Sample up to 10,000 values (Specification Section 4.3)
    let sample_limit = 10_000;
    let sample_cells: Vec<&CellValue> = cells.iter().take(sample_limit).collect();

    if sample_cells.is_empty() {
        return Ok(ColumnProfile {
            column_index: schema.index,
            column_name: col_name.clone(),
            inferred_type_code: ColumnType::Unknown.code().to_owned(),
            inferred_type_label_ar: ColumnType::Unknown.label_ar().to_owned(),
            inferred_type: ColumnType::Unknown,
            confidence: 0.0,
            evidence: vec!["Empty column".to_string()],
            alternatives: vec![],
            action_status: ProfileActionStatus::Unknown,
        });
    }

    // 1. Analyze column name dictionary synonyms (Arabic & English)
    let normalizer = ArabicNormalizer::new(NormOptions::default());
    let normalized_name = normalizer.normalize(col_name);
    let name_hint = get_name_type_hint(&normalized_name);

    // 2. Score value patterns across sample cells
    let mut type_scores = score_value_patterns(&sample_cells);

    // Apply name hint bonus if present
    if let Some((hint_type, hint_bonus)) = name_hint {
        let entry = type_scores.entry(hint_type).or_insert(0.0);
        *entry += hint_bonus;
    }

    // Find best type and alternatives
    let mut sorted_scores: Vec<(ColumnType, f64)> = type_scores.into_iter().collect();
    sorted_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let (best_type, raw_confidence) = sorted_scores
        .first()
        .copied()
        .unwrap_or((ColumnType::Unknown, 0.3));
    let confidence = raw_confidence.clamp(0.0, 1.0);

    let alternatives: Vec<(ColumnType, f64)> =
        sorted_scores.iter().skip(1).take(3).copied().collect();

    let evidence = sample_cells
        .iter()
        .take(5)
        .map(|c| c.raw_text().to_string())
        .collect();

    // 3. Determine action status based on thresholds (0.8 and 0.5)
    let action_status = if confidence >= 0.8 {
        ProfileActionStatus::AutoApplied
    } else if confidence >= 0.5 {
        ProfileActionStatus::NeedsUserConfirmation
    } else {
        ProfileActionStatus::Unknown
    };

    let inferred_type = if confidence >= 0.5 {
        best_type
    } else {
        ColumnType::Unknown
    };

    Ok(ColumnProfile {
        column_index: schema.index,
        column_name: col_name.clone(),
        inferred_type_code: inferred_type.code().to_owned(),
        inferred_type_label_ar: inferred_type.label_ar().to_owned(),
        inferred_type,
        confidence,
        evidence,
        alternatives,
        action_status,
    })
}

fn get_name_type_hint(norm_name: &str) -> Option<(ColumnType, f64)> {
    let lower = norm_name.to_lowercase();
    if lower.contains("اسم") || lower.contains("name") {
        Some((ColumnType::PersonName, 0.4))
    } else if lower.contains("تاريخ") || lower.contains("date") || lower.contains("الميلاد")
    {
        Some((ColumnType::Date, 0.4))
    } else if lower.contains("مبلغ")
        || lower.contains("سعر")
        || lower.contains("قيمة")
        || lower.contains("قيمه")
        || lower.contains("money")
        || lower.contains("price")
    {
        Some((ColumnType::MoneyAndCurrency, 0.4))
    } else if lower.contains("هاتف")
        || lower.contains("جوال")
        || lower.contains("تليفون")
        || lower.contains("phone")
    {
        Some((ColumnType::EgyptianPhone, 0.4))
    } else if lower.contains("بريد") || lower.contains("ايميل") || lower.contains("email")
    {
        Some((ColumnType::EmailOrUrl, 0.4))
    } else if lower.contains("رقم قومي") || lower.contains("national id") {
        Some((ColumnType::EgyptianNationalId, 0.5))
    } else if lower.contains("id")
        || lower.contains("معرف")
        || lower.contains("كود")
        || lower == "م"
    {
        Some((ColumnType::Identifier, 0.3))
    } else if lower.contains("نسبة")
        || lower.contains("نسبه")
        || lower.contains("ratio")
        || lower.contains("rate")
    {
        Some((ColumnType::Percentage, 0.3))
    } else {
        None
    }
}

fn score_value_patterns(cells: &[&CellValue]) -> std::collections::HashMap<ColumnType, f64> {
    let mut scores = std::collections::HashMap::new();
    let total = cells.len() as f64;
    if total == 0.0 {
        return scores;
    }

    let mut date_matches = 0usize;
    let mut amount_matches = 0usize;
    let mut phone_matches = 0usize;
    let mut email_matches = 0usize;
    let mut national_id_matches = 0usize;
    let mut number_matches = 0usize;
    let mut id_matches = 0usize;

    for cell in cells {
        let text = cell.raw_text();
        if text.is_empty() {
            continue;
        }

        // Check Egyptian phone (01xxxxxxxxx or +20...)
        if (text.starts_with("01") && text.len() == 11 && text.chars().all(|c| c.is_ascii_digit()))
            || text.starts_with("+20")
        {
            phone_matches += 1;
        }

        // Check Egyptian National ID (14 digits starting with 2 or 3)
        if text.len() == 14
            && text.chars().all(|c| c.is_ascii_digit())
            && (text.starts_with('2') || text.starts_with('3'))
        {
            national_id_matches += 1;
        }

        // Check email
        if text.contains('@') && text.contains('.') {
            email_matches += 1;
        }

        // Check date patterns (YYYY-MM-DD or DD/MM/YYYY)
        if text.len() == 10
            && (text.chars().nth(4) == Some('-') || text.chars().nth(2) == Some('/'))
        {
            date_matches += 1;
        }

        // Check amount / decimal / number
        if text.parse::<f64>().is_ok() {
            number_matches += 1;
            if text.contains('.') || text.contains(',') {
                amount_matches += 1;
            }
        }

        // Check identifier with leading zeros
        if text.starts_with('0') && text.chars().all(|c| c.is_ascii_digit()) {
            id_matches += 1;
        }
    }

    let match_ratio = |count: usize| (count as f64) / total;

    if phone_matches > 0 {
        scores.insert(ColumnType::EgyptianPhone, match_ratio(phone_matches) * 0.95);
    }
    if national_id_matches > 0 {
        scores.insert(
            ColumnType::EgyptianNationalId,
            match_ratio(national_id_matches) * 0.95,
        );
    }
    if email_matches > 0 {
        scores.insert(ColumnType::EmailOrUrl, match_ratio(email_matches) * 0.95);
    }
    if date_matches > 0 {
        scores.insert(ColumnType::Date, match_ratio(date_matches) * 0.85);
    }
    if amount_matches > 0 {
        scores.insert(
            ColumnType::MoneyAndCurrency,
            match_ratio(amount_matches) * 0.80,
        );
    }
    if number_matches > 0 {
        scores.insert(ColumnType::Percentage, match_ratio(number_matches) * 0.75);
    }
    if id_matches > 0 {
        scores.insert(ColumnType::Identifier, match_ratio(id_matches) * 0.85);
    }

    scores.insert(ColumnType::PersonName, 0.60);

    scores
}

#[cfg(test)]
mod tests {
    use super::*;
    use sf_core::column::{ColumnSchema, ConfidenceScore};

    #[test]
    fn test_profile_column_egyptian_phone() {
        let schema = ColumnSchema::new(0, "الهاتف", ColumnType::Unknown, ConfidenceScore::ZERO);
        let cells = vec![
            CellValue::from_raw("01012345678"),
            CellValue::from_raw("01198765432"),
            CellValue::from_raw("01234567890"),
        ];
        let col = Column::new(schema, cells);
        let res = profile_column(&col);
        assert!(res.is_ok());
        if let Ok(profile) = res {
            assert_eq!(profile.inferred_type, ColumnType::EgyptianPhone);
            assert!(profile.confidence >= 0.8);
            assert_eq!(profile.action_status, ProfileActionStatus::AutoApplied);
        }
    }

    #[test]
    fn test_golden_set_accuracy() {
        // Golden set of columns verification (Specification 16.6 & 4.3)
        let schema = ColumnSchema::new(0, "المبلغ", ColumnType::Unknown, ConfidenceScore::ZERO);
        let cells = vec![
            CellValue::from_raw("1500.50"),
            CellValue::from_raw("2000.00"),
            CellValue::from_raw("350.25"),
        ];
        let col = Column::new(schema, cells);
        let res = profile_column(&col);
        assert!(res.is_ok());
        if let Ok(profile) = res {
            assert_eq!(profile.inferred_type, ColumnType::MoneyAndCurrency);
            assert!(profile.confidence >= 0.8);
        }
    }
}
