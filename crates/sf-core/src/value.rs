//! Core cell and value representations (`CellValue`, `Value`, `MoneyValue`, `ZeroPreservedText`)
//! implementing Specification Sections 4.3, 4.4, 4.5, 4.7, and 20.5.
//!
//! Key invariants:
//! 1. Original raw text is ALWAYS preserved on every [`CellValue`] (`Section 4.4`).
//! 2. Normalized text is used for matching; replacing visible text is an explicit user option (`CellDisplayMode`).
//! 3. Financial amounts use exact [`rust_decimal::Decimal`] — never `f32` or `f64` (`Sections 4.5 & 20.5`).
//! 4. Identifiers, Egyptian National IDs, and phone numbers are stored as [`ZeroPreservedText`] strings
//!    whose leading zeros are never stripped (`Sections 4.3 & 4.5`).

use std::fmt;

use rust_decimal::Decimal;

/// Recognized empty/missing value sentinels from Specification Section 4.7:
/// «تُعامل "-" و"N/A" و"لا يوجد" و"غير متوفر" كفراغ».
pub const RECOGNIZED_EMPTY_SENTINELS: [&str; 6] = ["", "-", "N/A", "n/a", "لا يوجد", "غير متوفر"];

/// Returns `true` if `raw` (after trimming outer whitespace) represents an empty cell
/// or one of the canonical empty sentinels in Specification Section 4.7.
#[must_use]
pub fn is_recognized_empty_token(raw: &str) -> bool {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "-" || trimmed.eq_ignore_ascii_case("n/a") {
        return true;
    }
    matches!(trimmed, "لا يوجد" | "غير متوفر")
}

/// String wrapper that guarantees leading zeros are never stripped (Sections 4.3 & 4.5).
///
/// Used for Identifiers (`معرّف`), Egyptian National IDs (`رقم قومي مصري`), and Phone numbers (`هاتف مصري`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ZeroPreservedText(String);

impl ZeroPreservedText {
    /// Creates a new `ZeroPreservedText` preserving every character (including leading `'0'`s) verbatim.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    /// Returns the underlying string slice with all leading zeros intact.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the number of leading ASCII (`'0'`) or Arabic-Indic (`'٠'` / `'۰'`) zero characters.
    #[must_use]
    pub fn leading_zero_count(&self) -> usize {
        self.0
            .chars()
            .take_while(|&ch| ch == '0' || ch == '٠' || ch == '۰')
            .count()
    }

    /// Returns `true` if this text starts with a leading zero (`'0'`, `'٠'`, or `'۰'`).
    #[must_use]
    pub fn has_leading_zero(&self) -> bool {
        self.leading_zero_count() > 0
    }

    /// Consumes the wrapper and returns the preserved inner `String`.
    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for ZeroPreservedText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for ZeroPreservedText {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for ZeroPreservedText {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

/// Exact monetary value representation using [`rust_decimal::Decimal`] (never floating-point — Sections 4.5 & 20.5).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MoneyValue {
    /// Exact decimal monetary amount.
    pub amount: Decimal,
    /// Optional currency symbol or ISO code (e.g. `"EGP"`, `"ج.م"`, `"USD"`, `"ر.س"`).
    pub currency: Option<String>,
}

impl MoneyValue {
    /// Constructs a new `MoneyValue` from an exact `Decimal` and optional currency code/symbol.
    #[must_use]
    pub fn new(amount: Decimal, currency: Option<String>) -> Self {
        Self { amount, currency }
    }
}

impl fmt::Display for MoneyValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.currency {
            Some(curr) if !curr.is_empty() => write!(f, "{} {}", self.amount, curr),
            _ => write!(f, "{}", self.amount),
        }
    }
}

/// Structured, type-safe value stored inside a [`CellValue`] alongside the original raw text.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Value {
    /// Empty / missing value (including recognized sentinels `-`, `N/A`, `لا يوجد`, `غير متوفر` — Section 4.7).
    Empty,
    /// General text value (PersonName, GovernorateOrCity, Category, EmailOrUrl, or unparsed text).
    Text(String),
    /// Exact 64-bit signed integer for pure numeric counts (never used for IDs or phones).
    Integer(i64),
    /// Exact decimal number (`rust_decimal::Decimal`).
    Decimal(Decimal),
    /// Financial / monetary amount with exact `rust_decimal::Decimal` and optional currency (`Sections 4.3, 4.5, 20.5`).
    Money(MoneyValue),
    /// Exact decimal percentage value (`Section 4.3`).
    Percentage(Decimal),
    /// Boolean value (`نعم/لا` — `Section 4.3`).
    Boolean(bool),
    /// Normalized ISO-8601 date string (`YYYY-MM-DD` — `Sections 4.3 & 4.5`).
    DateIso(String),
    /// Phone number with leading zeros preserved in `raw_preserved` and optional normalized `E.164` (`Sections 4.3 & 4.5`).
    Phone {
        /// Phone digits as text with leading zeros preserved (e.g. `"01012345678"`).
        raw_preserved: ZeroPreservedText,
        /// Optional normalized E.164 representation (e.g. `"+201012345678"`).
        e164: Option<String>,
    },
    /// Egyptian National ID stored strictly as a 14-character string with leading zeros intact (`Sections 4.3, 4.5, 20.3`).
    NationalId(ZeroPreservedText),
    /// Identifier stored strictly as a string whose leading zeros are never stripped (`Section 4.3`).
    Identifier(ZeroPreservedText),
    /// Coordinate or physical measurement with exact `Decimal` magnitude and unit string (`Section 4.3`).
    Measurement {
        /// Exact decimal numeric magnitude.
        magnitude: Decimal,
        /// Unit symbol or name (e.g. `"kg"`, `"كم"`, `"m"`).
        unit: String,
    },
}

impl Value {
    /// Returns `true` if this structured value is `Value::Empty`.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    /// Formats the structured value as a canonical string representation.
    #[must_use]
    pub fn to_canonical_string(&self) -> String {
        match self {
            Self::Empty => String::new(),
            Self::Text(text) => text.clone(),
            Self::Integer(num) => num.to_string(),
            Self::Decimal(dec) => dec.to_string(),
            Self::Money(money) => money.to_string(),
            Self::Percentage(pct) => format!("{pct}%"),
            Self::Boolean(flag) => {
                if *flag {
                    "true".to_owned()
                } else {
                    "false".to_owned()
                }
            }
            Self::DateIso(iso) => iso.clone(),
            Self::Phone {
                raw_preserved,
                e164,
            } => match e164 {
                Some(norm) => norm.clone(),
                None => raw_preserved.as_str().to_owned(),
            },
            Self::NationalId(id) | Self::Identifier(id) => id.as_str().to_owned(),
            Self::Measurement { magnitude, unit } => format!("{magnitude} {unit}"),
        }
    }
}

/// Controls whether a cell displays the user's untouched original text or the normalized/cleaned text
/// (Specification Section 4.4: «النص الأصلي يُحفظ دائمًا، وتُحسب نسخة موحَّدة تُستخدم في المطابقة فقط.
/// استبدال النص الظاهر خيار يختاره المستخدم»).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum CellDisplayMode {
    /// Display the original untouched text (`raw`) while using `normalized` for matching (Default).
    #[default]
    Original,
    /// Replace visible text with the normalized / cleaned representation (Explicit user choice).
    Normalized,
}

/// A table cell that ALWAYS preserves the original raw text alongside its normalized matching form
/// and structured [`Value`] (Specification Section 4.4).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CellValue {
    raw: String,
    normalized: Option<String>,
    value: Value,
    display_mode: CellDisplayMode,
}

impl CellValue {
    /// Constructs a `CellValue` from raw source text, automatically classifying recognized empty
    /// sentinels (`""`, `"-"`, `"N/A"`, `"لا يوجد"`, `"غير متوفر"`) as [`Value::Empty`] while
    /// still preserving the exact `raw` string verbatim.
    #[must_use]
    pub fn from_raw(raw: impl Into<String>) -> Self {
        let raw_str = raw.into();
        let value = if is_recognized_empty_token(&raw_str) {
            Value::Empty
        } else {
            Value::Text(raw_str.clone())
        };
        Self {
            raw: raw_str,
            normalized: None,
            value,
            display_mode: CellDisplayMode::Original,
        }
    }

    /// Constructs a `CellValue` with explicit `raw` text and a structured [`Value`].
    #[must_use]
    pub fn new(raw: impl Into<String>, value: Value) -> Self {
        Self {
            raw: raw.into(),
            normalized: None,
            value,
            display_mode: CellDisplayMode::Original,
        }
    }

    /// Constructs a monetary `CellValue` backed by exact [`rust_decimal::Decimal`].
    #[must_use]
    pub fn money(raw: impl Into<String>, amount: Decimal, currency: Option<String>) -> Self {
        Self::new(raw, Value::Money(MoneyValue::new(amount, currency)))
    }

    /// Constructs an identifier `CellValue` whose leading zeros are preserved in [`ZeroPreservedText`].
    #[must_use]
    pub fn identifier(raw: impl Into<String>) -> Self {
        let raw_str = raw.into();
        let preserved = ZeroPreservedText::new(raw_str.clone());
        Self::new(raw_str, Value::Identifier(preserved))
    }

    /// Constructs an Egyptian National ID `CellValue` stored strictly as a string with leading zeros preserved.
    #[must_use]
    pub fn national_id(raw: impl Into<String>, clean_digits: impl Into<String>) -> Self {
        Self::new(
            raw,
            Value::NationalId(ZeroPreservedText::new(clean_digits.into())),
        )
    }

    /// Constructs a phone `CellValue` preserving leading zeros alongside optional `E.164` normalization.
    #[must_use]
    pub fn phone(
        raw: impl Into<String>,
        preserved_digits: impl Into<String>,
        e164: Option<String>,
    ) -> Self {
        let normalized = e164.clone();
        let mut cell = Self::new(
            raw,
            Value::Phone {
                raw_preserved: ZeroPreservedText::new(preserved_digits.into()),
                e164,
            },
        );
        cell.normalized = normalized;
        cell
    }

    /// Attaches a normalized string used for matching (Section 4.4) without overwriting `raw`.
    #[must_use]
    pub fn with_normalized(mut self, normalized: impl Into<String>) -> Self {
        self.normalized = Some(normalized.into());
        self
    }

    /// Updates the structured [`Value`] while keeping the original `raw` string intact.
    #[must_use]
    pub fn with_value(mut self, value: Value) -> Self {
        self.value = value;
        self
    }

    /// Sets whether the visible text should show `Original` (`raw`) or `Normalized` (Section 4.4).
    #[must_use]
    pub const fn with_display_mode(mut self, display_mode: CellDisplayMode) -> Self {
        self.display_mode = display_mode;
        self
    }

    /// Returns the untouched original raw text as imported from the user's file (Section 4.4).
    #[must_use]
    pub fn raw_text(&self) -> &str {
        &self.raw
    }

    /// Returns the normalized string if one has been computed.
    #[must_use]
    pub fn normalized_text(&self) -> Option<&str> {
        self.normalized.as_deref()
    }

    /// Returns the text to use for matching and deduplication (`normalized` if available, else `raw` — Section 4.4).
    #[must_use]
    pub fn matching_text(&self) -> &str {
        self.normalized.as_deref().unwrap_or(&self.raw)
    }

    /// Returns the text to render in the UI according to the cell's [`CellDisplayMode`] (Section 4.4).
    #[must_use]
    pub fn visible_text(&self) -> String {
        match self.display_mode {
            CellDisplayMode::Original => self.raw.clone(),
            CellDisplayMode::Normalized => match &self.normalized {
                Some(norm) => norm.clone(),
                None => self.value.to_canonical_string(),
            },
        }
    }

    /// Returns a reference to the structured [`Value`].
    #[must_use]
    pub const fn value(&self) -> &Value {
        &self.value
    }

    /// Returns the active [`CellDisplayMode`].
    #[must_use]
    pub const fn display_mode(&self) -> CellDisplayMode {
        self.display_mode
    }

    /// Returns `true` if the structured value is [`Value::Empty`].
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.value.is_empty()
    }
}
