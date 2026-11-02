//! Column type taxonomy (`ColumnType`), confidence scoring (`ConfidenceScore`, `ConfidenceDecision`),
//! and column structures (`ColumnSchema`, `Column`) implementing Specification Sections 4.3 and 4.7.

use std::fmt;

use crate::error::{ErrorCode, ErrorSeverity, SafiError};
use crate::value::CellValue;

/// Maximum number of sample values inspected per column during type profiling (`10,000` — Section 4.3).
pub const MAX_COLUMN_SAMPLE_VALUES: usize = 10_000;

/// Confidence threshold at or above which a suggestion is automatically applied in preview (`0.8` — Section 4.3).
pub const AUTO_APPLY_CONFIDENCE_THRESHOLD: f64 = 0.8;

/// Confidence threshold at or above which a suggestion is presented to the user for confirmation (`0.5` — Section 4.3).
/// Any score strictly below `0.5` resolves to [`ColumnType::Unknown`] («غير معروف»).
pub const USER_CONFIRM_CONFIDENCE_THRESHOLD: f64 = 0.5;

/// The 12 standard column types in Specification Section 4.3 plus [`ColumnType::Unknown`] («غير معروف»).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ColumnType {
    /// 1. «اسم شخص» — Person full or partial name (`Section 4.3`).
    PersonName,
    /// 2. «تاريخ (ميلادي وهجري)» — Gregorian or Hijri (Umm al-Qura) date (`Section 4.3`).
    Date,
    /// 3. «مبلغ وعملة» — Financial amount and currency (`Section 4.3`, backed by `rust_decimal::Decimal`).
    MoneyAndCurrency,
    /// 4. «نسبة» — Percentage value (`Section 4.3`, backed by `rust_decimal::Decimal`).
    Percentage,
    /// 5. «هاتف مصري» — Egyptian mobile or landline phone number (`Section 4.3`, leading zeros preserved).
    EgyptianPhone,
    /// 6. «رقم قومي مصري» — 14-digit Egyptian National ID (`Sections 4.3 & 20.3`, stored as text always).
    EgyptianNationalId,
    /// 7. «بريد إلكتروني ورابط» — Email address or URL (`Section 4.3`).
    EmailOrUrl,
    /// 8. «محافظة أو مدينة» — Egyptian governorate, city, or administrative region (`Section 4.3`).
    GovernorateOrCity,
    /// 9. «فئة (قيم محدودة)» — Categorical / enumerated values (`Section 4.3`).
    Category,
    /// 10. «نعم/لا» — Boolean / binary yes-no field (`Section 4.3`).
    Boolean,
    /// 11. «معرّف» — Unique identifier with leading zeros preserved (`Section 4.3`).
    Identifier,
    /// 12. «إحداثيات ووحدات قياس» — Geographic coordinates or physical measurements (`Section 4.3`).
    CoordinateOrMeasurement,
    /// 13. «غير معروف» — Fallback when confidence is below `0.5` (`Section 4.3`).
    #[default]
    Unknown,
}

impl ColumnType {
    /// The 12 known semantic column types listed in the table of Specification Section 4.3.
    pub const KNOWN_TYPES: [Self; 12] = [
        Self::PersonName,
        Self::Date,
        Self::MoneyAndCurrency,
        Self::Percentage,
        Self::EgyptianPhone,
        Self::EgyptianNationalId,
        Self::EmailOrUrl,
        Self::GovernorateOrCity,
        Self::Category,
        Self::Boolean,
        Self::Identifier,
        Self::CoordinateOrMeasurement,
    ];

    /// All 13 column type variants (the 12 known types plus `Unknown`).
    pub const ALL: [Self; 13] = [
        Self::PersonName,
        Self::Date,
        Self::MoneyAndCurrency,
        Self::Percentage,
        Self::EgyptianPhone,
        Self::EgyptianNationalId,
        Self::EmailOrUrl,
        Self::GovernorateOrCity,
        Self::Category,
        Self::Boolean,
        Self::Identifier,
        Self::CoordinateOrMeasurement,
        Self::Unknown,
    ];

    /// Stable machine-readable identifier for recipes and serialization.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::PersonName => "person_name",
            Self::Date => "date",
            Self::MoneyAndCurrency => "money_and_currency",
            Self::Percentage => "percentage",
            Self::EgyptianPhone => "egyptian_phone",
            Self::EgyptianNationalId => "egyptian_national_id",
            Self::EmailOrUrl => "email_or_url",
            Self::GovernorateOrCity => "governorate_or_city",
            Self::Category => "category",
            Self::Boolean => "boolean",
            Self::Identifier => "identifier",
            Self::CoordinateOrMeasurement => "coordinate_or_measurement",
            Self::Unknown => "unknown",
        }
    }

    /// Canonical Arabic label from Specification Section 4.3.
    #[must_use]
    pub const fn label_ar(self) -> &'static str {
        match self {
            Self::PersonName => "اسم شخص",
            Self::Date => "تاريخ (ميلادي وهجري)",
            Self::MoneyAndCurrency => "مبلغ وعملة",
            Self::Percentage => "نسبة",
            Self::EgyptianPhone => "هاتف مصري",
            Self::EgyptianNationalId => "رقم قومي مصري",
            Self::EmailOrUrl => "بريد إلكتروني ورابط",
            Self::GovernorateOrCity => "محافظة أو مدينة",
            Self::Category => "فئة (قيم محدودة)",
            Self::Boolean => "نعم/لا",
            Self::Identifier => "معرّف",
            Self::CoordinateOrMeasurement => "إحداثيات ووحدات قياس",
            Self::Unknown => "غير معروف",
        }
    }

    /// Returns `true` if values of this column type must ALWAYS be stored as text
    /// without ever stripping leading zeros (Sections 4.3 & 4.5).
    #[must_use]
    pub const fn preserves_leading_zeros(self) -> bool {
        matches!(
            self,
            Self::Identifier | Self::EgyptianNationalId | Self::EgyptianPhone
        )
    }

    /// Returns `true` if this column represents an identity attribute that must NEVER
    /// be auto-imputed/filled when empty (Specification Section 4.7: «أعمدة الهوية لا تُملأ أبدًا»).
    #[must_use]
    pub const fn is_identity_column(self) -> bool {
        matches!(
            self,
            Self::Identifier
                | Self::EgyptianNationalId
                | Self::EgyptianPhone
                | Self::PersonName
                | Self::EmailOrUrl
        )
    }

    /// Returns `true` if numeric values in this column must be represented with exact
    /// `rust_decimal::Decimal` rather than floating-point (Sections 4.3, 4.5, 20.5).
    #[must_use]
    pub const fn requires_exact_decimal(self) -> bool {
        matches!(
            self,
            Self::MoneyAndCurrency | Self::Percentage | Self::CoordinateOrMeasurement
        )
    }
}

impl fmt::Display for ColumnType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// Decision band determined from a [`ConfidenceScore`] according to Specification Section 4.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConfidenceDecision {
    /// Score `>= 0.8`: automatically applied in preview («ثقة 0.8 فأكثر: يُطبَّق الاقتراح تلقائيًا في المعاينة»).
    AutoApply,
    /// Score in `[0.5, 0.8)`: presented to the user for confirmation («بين 0.5 و0.8: يُعرض للمستخدم ليؤكد»).
    UserConfirm,
    /// Score `< 0.5`: classified as `Unknown` («أقل من 0.5: غير معروف»).
    UnknownFallback,
}

/// Validated confidence score in `[0.0, 1.0]` with standard threshold evaluation (Section 4.3).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct ConfidenceScore(f64);

impl ConfidenceScore {
    /// Minimum possible confidence (`0.0`).
    pub const ZERO: Self = Self(0.0);
    /// Threshold for user confirmation (`0.5`).
    pub const USER_CONFIRM_MIN: Self = Self(USER_CONFIRM_CONFIDENCE_THRESHOLD);
    /// Threshold for automatic application in preview (`0.8`).
    pub const AUTO_APPLY_MIN: Self = Self(AUTO_APPLY_CONFIDENCE_THRESHOLD);
    /// Maximum possible confidence (`1.0`).
    pub const ONE: Self = Self(1.0);

    /// Validates that `value` is finite and within `[0.0, 1.0]`.
    /// Returns `Err(SafiError)` with code `SF-PRF-002` if `value` is `NaN` or outside `[0.0, 1.0]`.
    pub fn new(value: f64) -> Result<Self, SafiError> {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(SafiError::new(
                ErrorCode::SF_PRF_002,
                ErrorSeverity::Error,
                "InvalidConfidenceScoreRange",
                "profile.invalid_confidence",
            ));
        }
        // Normalize -0.0 to +0.0 for deterministic equality
        let normalized = if value == 0.0 { 0.0 } else { value };
        Ok(Self(normalized))
    }

    /// Returns the underlying `f64` score in `[0.0, 1.0]`.
    #[must_use]
    pub const fn value(self) -> f64 {
        self.0
    }

    /// Evaluates the decision band for this confidence score per Specification Section 4.3.
    #[must_use]
    pub fn decision(self) -> ConfidenceDecision {
        if self.0 >= AUTO_APPLY_CONFIDENCE_THRESHOLD {
            ConfidenceDecision::AutoApply
        } else if self.0 >= USER_CONFIRM_CONFIDENCE_THRESHOLD {
            ConfidenceDecision::UserConfirm
        } else {
            ConfidenceDecision::UnknownFallback
        }
    }

    /// Returns `true` if `self >= 0.8` (auto-applied in preview).
    #[must_use]
    pub fn should_auto_apply(self) -> bool {
        self.decision() == ConfidenceDecision::AutoApply
    }

    /// Returns `true` if `0.5 <= self < 0.8` (requires user confirmation).
    #[must_use]
    pub fn requires_user_confirmation(self) -> bool {
        self.decision() == ConfidenceDecision::UserConfirm
    }

    /// Returns `true` if `self < 0.5` (classified as `Unknown`).
    #[must_use]
    pub fn is_unknown_fallback(self) -> bool {
        self.decision() == ConfidenceDecision::UnknownFallback
    }

    /// Resolves the effective [`ColumnType`] given a candidate type and this confidence score:
    /// if the score is strictly below `0.5`, returns [`ColumnType::Unknown`] per Section 4.3.
    #[must_use]
    pub fn resolve_column_type(self, candidate: ColumnType) -> ColumnType {
        if self.is_unknown_fallback() {
            ColumnType::Unknown
        } else {
            candidate
        }
    }
}

impl Default for ConfidenceScore {
    fn default() -> Self {
        Self::ZERO
    }
}

/// Metadata and inferred type schema for a single column (`ColumnSchema`).
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnSchema {
    /// Zero-based column position in the table.
    pub index: usize,
    /// Original column header name from the imported table.
    pub name: String,
    /// Effective inferred or user-confirmed [`ColumnType`].
    pub column_type: ColumnType,
    /// Confidence score in `[0.0, 1.0]` for the column type.
    pub confidence: ConfidenceScore,
    /// Whether the column allows empty / missing cells.
    pub nullable: bool,
    /// Optional Humanitarian Exchange Language (HXL) tag such as `"#adm1+code"` (`Section 18.10 N1`).
    pub hxl_tag: Option<String>,
}

impl ColumnSchema {
    /// Creates a new `ColumnSchema` applying the Section 4.3 rule: if `confidence < 0.5`,
    /// `column_type` automatically resolves to [`ColumnType::Unknown`].
    #[must_use]
    pub fn new(
        index: usize,
        name: impl Into<String>,
        candidate_type: ColumnType,
        confidence: ConfidenceScore,
    ) -> Self {
        let resolved_type = confidence.resolve_column_type(candidate_type);
        Self {
            index,
            name: name.into(),
            column_type: resolved_type,
            confidence,
            nullable: true,
            hxl_tag: None,
        }
    }

    /// Attaches an optional HXL tag (e.g. `"#adm1+code"`).
    #[must_use]
    pub fn with_hxl_tag(mut self, tag: impl Into<String>) -> Self {
        self.hxl_tag = Some(tag.into());
        self
    }

    /// Sets whether this column permits empty values.
    #[must_use]
    pub const fn with_nullable(mut self, nullable: bool) -> Self {
        self.nullable = nullable;
        self
    }

    /// Returns the [`ConfidenceDecision`] (`AutoApply`, `UserConfirm`, or `UnknownFallback`).
    #[must_use]
    pub fn confidence_decision(&self) -> ConfidenceDecision {
        self.confidence.decision()
    }
}

/// A complete table column combining its [`ColumnSchema`] and ordered [`CellValue`] vector.
#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    schema: ColumnSchema,
    cells: Vec<CellValue>,
}

impl Column {
    /// Constructs a new `Column` from its schema and ordered cells.
    #[must_use]
    pub const fn new(schema: ColumnSchema, cells: Vec<CellValue>) -> Self {
        Self { schema, cells }
    }

    /// Returns a reference to the column's schema.
    #[must_use]
    pub const fn schema(&self) -> &ColumnSchema {
        &self.schema
    }

    /// Returns a slice of all cells in this column.
    #[must_use]
    pub fn cells(&self) -> &[CellValue] {
        &self.cells
    }

    /// Returns the number of rows (cells) in this column.
    #[must_use]
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// Returns `true` if this column has zero rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Returns a reference to the cell at `row_index`, if within bounds.
    #[must_use]
    pub fn cell_at(&self, row_index: usize) -> Option<&CellValue> {
        self.cells.get(row_index)
    }

    /// Counts how many cells in this column have [`crate::value::Value::Empty`].
    #[must_use]
    pub fn empty_cell_count(&self) -> usize {
        self.cells.iter().filter(|cell| cell.is_empty()).count()
    }

    /// Counts how many cells in this column are non-empty.
    #[must_use]
    pub fn non_empty_cell_count(&self) -> usize {
        self.cells.len().saturating_sub(self.empty_cell_count())
    }
}
