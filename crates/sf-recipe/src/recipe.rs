//! Recipe schema implementation matching `recipe.schema.json` (Section 19.5) and S2 full schema.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sf_core::error::{ErrorCode, ErrorSeverity, SafiError};

/// The 9 supported step operations in Specification Section 19.5.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecipeOperation {
    NormalizeText,
    Dedupe,
    FillBlank,
    ParseDate,
    ParseNumber,
    NormalizePhone,
    Rename,
    Split,
    Merge,
}

/// A single cleaning or transformation step in a SafiRecipe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecipeStep {
    pub id: String,
    pub op: RecipeOperation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<Vec<String>>,
    pub params: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    pub reversible: bool,
}

/// Column rule matching specification Section 19.5 & 4.9.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnRule {
    pub match_cond: serde_json::Value,
    pub col_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meaning_ar: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_table: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub is_sensitive: bool,
}

/// Result of matching a single expected recipe column against new file columns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnMatchResult {
    pub original_column: String,
    pub matched_column: Option<String>,
    pub match_type: String, // "exact", "normalized", "position", "missing"
    pub confidence: f64,    // 1.0 = exact, 0.9 = normalized, 0.6 = position, 0.0 = missing
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// Comprehensive report of recipe column matching on a new file schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeMatchReport {
    pub is_exact_match: bool,
    pub has_schema_changes: bool,
    pub matches: Vec<ColumnMatchResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning_message: Option<String>,
}

/// SafiRecipe matching the full JSON Schema in Section 19.5.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SafiRecipe {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_with: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column_rules: Option<Vec<ColumnRule>>,
    pub steps: Vec<RecipeStep>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

fn canonicalize_json_value(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let sorted: BTreeMap<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), canonicalize_json_value(v)))
                .collect();
            let mut out = serde_json::Map::with_capacity(sorted.len());
            for (k, v) in sorted {
                out.insert(k, v);
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(canonicalize_json_value).collect())
        }
        other => other.clone(),
    }
}

impl SafiRecipe {
    /// Computes a deterministic content hash (BLAKE3) of the recipe core fields.
    pub fn compute_content_hash(&self) -> Result<String, SafiError> {
        let mut clone = self.clone();
        clone.content_hash = None;

        let raw_value = serde_json::to_value(&clone).map_err(|_| {
            SafiError::new(
                ErrorCode::SF_RCP_001,
                ErrorSeverity::Error,
                "RecipeSerializationFailed",
                "recipe.serialization_error",
            )
        })?;
        let canonical_value = canonicalize_json_value(&raw_value);
        let canonical_json = serde_json::to_string(&canonical_value).map_err(|_| {
            SafiError::new(
                ErrorCode::SF_RCP_001,
                ErrorSeverity::Error,
                "RecipeSerializationFailed",
                "recipe.serialization_error",
            )
        })?;

        let hash = blake3::hash(canonical_json.as_bytes());
        Ok(hash.to_hex().to_string())
    }

    /// Validates schema version and structural integrity, migrating legacy versions automatically.
    pub fn migrate_and_validate(raw_json: &serde_json::Value) -> Result<SafiRecipe, SafiError> {
        let version = raw_json
            .get("schema_version")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        match version {
            0 | 1 => {
                let mut recipe: SafiRecipe =
                    serde_json::from_value(raw_json.clone()).map_err(|_| {
                        SafiError::new(
                            ErrorCode::SF_RCP_001,
                            ErrorSeverity::Error,
                            "InvalidRecipeSchema",
                            "recipe.invalid_schema",
                        )
                    })?;
                recipe.schema_version = 1;
                Ok(recipe)
            }
            _ => Err(SafiError::new(
                ErrorCode::SF_RCP_001,
                ErrorSeverity::Error,
                "UnsupportedRecipeSchemaVersion",
                "recipe.invalid_version",
            )),
        }
    }

    /// Hierarchical column matching on new file schema with schema change detection (SF-RCP-003).
    pub fn match_columns_on_new_file(
        &self,
        expected_columns: &[String],
        new_file_columns: &[String],
    ) -> RecipeMatchReport {
        let mut matches = Vec::new();
        let mut has_changes = false;
        let mut missing_count = 0;
        let mut extra_count = 0;

        let mut available_new_cols: std::collections::HashSet<usize> =
            (0..new_file_columns.len()).collect();

        for (expected_idx, exp_col) in expected_columns.iter().enumerate() {
            let mut matched_col = None;
            let mut match_type = "missing";
            let mut confidence = 0.0;
            let mut warning = None;

            // 1. Exact name match
            if let Some(&new_idx) = available_new_cols
                .iter()
                .find(|&&idx| new_file_columns[idx] == *exp_col)
            {
                matched_col = Some(new_file_columns[new_idx].clone());
                match_type = "exact";
                confidence = 1.0;
                available_new_cols.remove(&new_idx);
                if new_idx != expected_idx {
                    has_changes = true;
                }
            } else {
                // 2. Normalized Arabic name match
                let norm_exp =
                    sf_arabic::normalize_text(exp_col, sf_arabic::NormOptions::default());
                if let Some(&new_idx) = available_new_cols.iter().find(|&&idx| {
                    sf_arabic::normalize_text(
                        &new_file_columns[idx],
                        sf_arabic::NormOptions::default(),
                    ) == norm_exp
                }) {
                    matched_col = Some(new_file_columns[new_idx].clone());
                    match_type = "normalized";
                    confidence = 0.9;
                    available_new_cols.remove(&new_idx);
                    has_changes = true;
                    warning = Some(format!(
                        "تم مطابقة العمود '{}' بـ '{}' عبر التوحيد العربي.",
                        exp_col, new_file_columns[new_idx]
                    ));
                } else if expected_idx < new_file_columns.len()
                    && available_new_cols.contains(&expected_idx)
                {
                    // 3. Position match fallback
                    let pos_col = &new_file_columns[expected_idx];
                    matched_col = Some(pos_col.clone());
                    match_type = "position";
                    confidence = 0.6;
                    available_new_cols.remove(&expected_idx);
                    has_changes = true;
                    warning = Some(format!(
                        "تغيّر اسم العمود في الموضع {} من '{}' إلى '{}'.",
                        expected_idx + 1,
                        exp_col,
                        pos_col
                    ));
                } else {
                    missing_count += 1;
                    has_changes = true;
                    warning = Some(format!(
                        "العمود المطلوب '{}' مفقود في الملف الجديد.",
                        exp_col
                    ));
                }
            }

            matches.push(ColumnMatchResult {
                original_column: exp_col.clone(),
                matched_column: matched_col,
                match_type: match_type.to_string(),
                confidence,
                warning,
            });
        }

        if !available_new_cols.is_empty() {
            extra_count = available_new_cols.len();
            has_changes = true;
        }

        let is_exact = !has_changes && missing_count == 0 && extra_count == 0;
        let mut warning_code = None;
        let mut warning_message = None;

        if has_changes {
            warning_code = Some("SF-RCP-003".to_string());
            warning_message = Some(format!(
                "تنبيه تغيير شكل الملف (SF-RCP-003): تم رصد أعمدة جديدة ({}), أعمدة ناقصة ({}), أو تغييرات في الترتيب/الأسماء. يرجى مراجعة المطابقة المقترحة.",
                extra_count, missing_count
            ));
        }

        RecipeMatchReport {
            is_exact_match: is_exact,
            has_schema_changes: has_changes,
            matches,
            warning_code,
            warning_message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recipe_migration_and_hashing() {
        let legacy_json = serde_json::json!({
            "schema_version": 0,
            "id": "legacy_rec_1",
            "name": "وصفة قديمة",
            "steps": []
        });

        let migrated = SafiRecipe::migrate_and_validate(&legacy_json);
        assert!(migrated.is_ok());
        if let Ok(rec) = migrated {
            assert_eq!(rec.schema_version, 1);
            assert!(rec.compute_content_hash().is_ok());
        }

        let invalid_json = serde_json::json!({
            "schema_version": 99,
            "id": "bad",
            "name": "خطأ",
            "steps": []
        });
        assert!(SafiRecipe::migrate_and_validate(&invalid_json).is_err());
    }

    #[test]
    fn test_column_matcher_hierarchical_and_drift() {
        let recipe = SafiRecipe {
            schema_version: 1,
            id: "rec-matcher".to_string(),
            name: "وصفة المطابقة".to_string(),
            created_with: None,
            region: None,
            column_rules: None,
            steps: vec![],
            content_hash: None,
        };

        let expected = vec![
            "الاسم".to_string(),
            "المبلغ".to_string(),
            "التاريخ".to_string(),
        ];

        // Test 1: Exact match
        let new_exact = vec![
            "الاسم".to_string(),
            "المبلغ".to_string(),
            "التاريخ".to_string(),
        ];
        let report_exact = recipe.match_columns_on_new_file(&expected, &new_exact);
        assert!(report_exact.is_exact_match);
        assert!(!report_exact.has_schema_changes);

        // Test 2: Schema drift (extra column and missing/renamed column) with SF-RCP-003 warning
        let new_drift = vec![
            "الاسم الكامل".to_string(),
            "المبلغ".to_string(),
            "رقم الهاتف".to_string(),
        ];
        let report_drift = recipe.match_columns_on_new_file(&expected, &new_drift);
        assert!(!report_drift.is_exact_match);
        assert!(report_drift.has_schema_changes);
        assert_eq!(report_drift.warning_code.as_deref(), Some("SF-RCP-003"));
    }
}
