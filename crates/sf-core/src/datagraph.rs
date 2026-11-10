//! Shared Studio identities, dataset versioning, and verified metric tokens (`STUDIO-PHASE-1`).
//!
//! The Studio has one source of truth: an immutable original dataset, an ordered list of
//! deterministic transformations, and metric definitions evaluated by Rust. Every number that
//! reaches a view does so through a [`VerifiedMetricToken`] that binds the displayed value to the
//! formula, the dataset version, the contributing rows/columns, and a `BLAKE3` content
//! fingerprint.
//!
//! A fingerprint is a consistency check, not proof that the source data is correct and not a
//! signature. It never includes wall-clock metadata, so recomputing identical inputs yields the
//! identical fingerprint.
//!
//! Zero network dependencies (`AGENTS.md` Rule #7).

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::{ErrorCode, ErrorSeverity, SafiError};
use crate::value::Value;

/// Version identifier of the Rust engine that produced a token.
pub const STUDIO_ENGINE_VERSION: &str = "studio-engine-1.0.0";

/// Field separator used by the canonical fingerprint payload; never appears in identifiers,
/// formulas, or formatted decimals.
const FIELD_SEPARATOR: char = '\u{1f}';

fn invalid_identifier() -> SafiError {
    SafiError::new(
        ErrorCode::SF_SYS_001,
        ErrorSeverity::Error,
        "InvalidStudioIdentifier",
        "studio.invalid_identifier",
    )
}

macro_rules! studio_identifier {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Validates and stores a stable identifier (`1..=64` bytes of
            /// `[A-Za-z0-9_-.:]`).
            ///
            /// # Errors
            /// Returns `SF-SYS-001` when the identifier is empty, too long, or contains
            /// characters that could be confused with the fingerprint separator.
            pub fn new(raw: impl AsRef<str>) -> Result<Self, SafiError> {
                let raw = raw.as_ref();
                let valid = !raw.is_empty()
                    && raw.len() <= 64
                    && raw.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b':')
                    });
                if valid {
                    Ok(Self(raw.to_owned()))
                } else {
                    Err(invalid_identifier())
                }
            }

            /// Returns the identifier as a string slice.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }
    };
}

studio_identifier!(SourceId, "Stable identifier of an imported dataset source.");
studio_identifier!(MetricId, "Stable identifier of a Studio metric.");
studio_identifier!(ScenarioId, "Stable identifier of a What-If scenario.");

/// Monotonic version of the published dataset state (bumped by every accepted change).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct DatasetVersion(u64);

impl DatasetVersion {
    /// The initial version of a freshly imported dataset.
    pub const INITIAL: Self = Self(0);

    /// Returns the numeric version.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Returns the next version.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }

    /// Rebuilds a version from a persisted project file.
    #[must_use]
    pub const fn from_raw(value: u64) -> Self {
        Self(value)
    }
}

/// Monotonic revision of the Studio state; used to reject stale worker replies.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct Revision(u64);

impl Revision {
    /// The revision of a freshly created graph.
    pub const INITIAL: Self = Self(0);

    /// Returns the numeric revision.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Returns the next revision.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }

    /// Rebuilds a revision from a persisted project file.
    #[must_use]
    pub const fn from_raw(value: u64) -> Self {
        Self(value)
    }
}

/// Verification state carried by a metric token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VerificationStatus {
    /// The value was computed from the current dataset version and matched its fingerprint.
    Verified,
    /// The dataset, formula, or scenario changed after the token was produced.
    Stale,
    /// The metric could not be computed (see the engine error attached to it).
    Unverified,
}

/// Result of re-checking a token against a freshly recomputed value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerificationOutcome {
    /// Both the value and the content fingerprint match.
    Verified,
    /// The displayed value or its provenance no longer matches the recomputed result.
    Mismatch {
        /// Fingerprint recorded in the token that a view is showing.
        expected: String,
        /// Fingerprint of the freshly recomputed token.
        actual: String,
    },
}

impl VerificationOutcome {
    /// Returns `true` when the token matches the recomputation.
    #[must_use]
    pub const fn is_verified(&self) -> bool {
        matches!(self, Self::Verified)
    }
}

/// All inputs required to build a [`VerifiedMetricToken`].
///
/// A struct is used instead of a long constructor so that callers cannot silently swap two
/// adjacent string arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct MetricTokenParts {
    /// Stable metric identifier.
    pub metric_id: MetricId,
    /// Human-readable metric name (already localized by the caller when needed).
    pub metric_name: String,
    /// Exact computed value.
    pub value: Value,
    /// Optional unit such as `EGP` or `%`.
    pub unit: Option<String>,
    /// Formula source text the engine parsed.
    pub formula: String,
    /// Monotonic version of that formula definition.
    pub formula_version: u32,
    /// Source dataset identifier.
    pub source_id: SourceId,
    /// Dataset version the value was computed from.
    pub source_version: DatasetVersion,
    /// Fingerprint of the immutable original dataset.
    pub source_fingerprint: String,
    /// Zero-based rows that contributed to the value.
    pub row_refs: Vec<u64>,
    /// Column names that contributed to the value.
    pub column_refs: Vec<String>,
    /// Digest of each transformation applied before the metric was evaluated.
    pub transformation_trace: Vec<String>,
    /// Calculation timestamp (metadata only, excluded from the fingerprint).
    pub computed_at_ms: u64,
    /// Verification state of the token.
    pub status: VerificationStatus,
}

/// A number that a Studio view may show, bound to its provenance and content fingerprint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerifiedMetricToken {
    /// Stable metric identifier.
    pub metric_id: MetricId,
    /// Human-readable metric name.
    pub metric_name: String,
    /// Exact computed value.
    pub value: Value,
    /// Optional unit.
    pub unit: Option<String>,
    /// Formula source text.
    pub formula: String,
    /// Monotonic formula version.
    pub formula_version: u32,
    /// Source dataset identifier.
    pub source_id: SourceId,
    /// Dataset version used for the computation.
    pub source_version: DatasetVersion,
    /// Fingerprint of the immutable original dataset.
    pub source_fingerprint: String,
    /// Contributing zero-based rows.
    pub row_refs: Vec<u64>,
    /// Contributing column names.
    pub column_refs: Vec<String>,
    /// Digest of each applied transformation.
    pub transformation_trace: Vec<String>,
    /// Calculation timestamp in milliseconds since the Unix epoch (metadata only).
    pub computed_at_ms: u64,
    /// Engine version that produced the token.
    pub engine_version: String,
    /// Verification state.
    pub status: VerificationStatus,
    fingerprint: String,
}

impl VerifiedMetricToken {
    /// Builds a token and seals it with a `BLAKE3` content fingerprint.
    #[must_use]
    pub fn from_parts(parts: MetricTokenParts) -> Self {
        let mut token = Self {
            metric_id: parts.metric_id,
            metric_name: parts.metric_name,
            value: parts.value,
            unit: parts.unit,
            formula: parts.formula,
            formula_version: parts.formula_version,
            source_id: parts.source_id,
            source_version: parts.source_version,
            source_fingerprint: parts.source_fingerprint,
            row_refs: parts.row_refs,
            column_refs: parts.column_refs,
            transformation_trace: parts.transformation_trace,
            computed_at_ms: parts.computed_at_ms,
            engine_version: STUDIO_ENGINE_VERSION.to_owned(),
            status: parts.status,
            fingerprint: String::new(),
        };
        token.fingerprint = token.compute_fingerprint();
        token
    }

    /// Returns the `BLAKE3` content fingerprint (hex).
    #[must_use]
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// Returns `true` only for tokens computed against the current dataset state.
    #[must_use]
    pub const fn is_verifiable(&self) -> bool {
        matches!(self.status, VerificationStatus::Verified)
    }

    /// Recomputes the fingerprint from the token's own content.
    ///
    /// Needed after a project is restored from disk, where a stored fingerprint could be stale or
    /// edited; the restored token must not be trusted until this runs.
    #[must_use]
    pub fn reseal(mut self) -> Self {
        self.fingerprint = self.compute_fingerprint();
        self
    }

    /// Returns a copy of this token with a different verification status.
    ///
    /// The fingerprint is unchanged because the status is not part of the content identity.
    #[must_use]
    pub fn with_status(mut self, status: VerificationStatus) -> Self {
        self.status = status;
        self
    }

    /// Re-checks this token against a freshly recomputed token.
    ///
    /// The comparison covers the value, formula, formula version, dataset version, fingerprint,
    /// and provenance, because they are all part of the content fingerprint.
    #[must_use]
    pub fn verify_against(&self, recomputed: &Self) -> VerificationOutcome {
        if self.fingerprint == recomputed.fingerprint && self.value == recomputed.value {
            VerificationOutcome::Verified
        } else {
            VerificationOutcome::Mismatch {
                expected: self.fingerprint.clone(),
                actual: recomputed.fingerprint.clone(),
            }
        }
    }

    /// Returns a copy of this token recomputed under another dataset or formula version, marked
    /// as stale, which is what a view must show while a recomputation is in flight.
    #[must_use]
    pub fn as_stale(&self) -> Self {
        self.clone().with_status(VerificationStatus::Stale)
    }

    fn compute_fingerprint(&self) -> String {
        let mut payload = String::with_capacity(256);
        for field in [
            self.metric_id.as_str(),
            self.metric_name.as_str(),
            &canonical_value(&self.value),
            self.unit.as_deref().unwrap_or(""),
            self.formula.trim(),
            &self.formula_version.to_string(),
            self.source_id.as_str(),
            &self.source_version.get().to_string(),
            self.source_fingerprint.as_str(),
            &self.engine_version,
        ] {
            payload.push_str(field);
            payload.push(FIELD_SEPARATOR);
        }
        for row in &self.row_refs {
            payload.push_str(&row.to_string());
            payload.push(',');
        }
        payload.push(FIELD_SEPARATOR);
        for column in &self.column_refs {
            payload.push_str(column);
            payload.push(',');
        }
        payload.push(FIELD_SEPARATOR);
        for digest in &self.transformation_trace {
            payload.push_str(digest);
            payload.push(';');
        }
        blake3::hash(payload.as_bytes()).to_hex().to_string()
    }
}

/// Canonical, scale-insensitive rendering of a value for fingerprinting.
///
/// `1250.5` and `1250.50` are the same number, so decimals are normalized before hashing.
#[must_use]
pub fn canonical_value(value: &Value) -> String {
    match value {
        Value::Integer(integer) => {
            format!("decimal:{}", crate::Decimal::from(*integer).normalize())
        }
        Value::Decimal(decimal) | Value::Percentage(decimal) => {
            format!("decimal:{}", decimal.normalize())
        }
        Value::Money(money) => format!("money:{}", money.amount.normalize()),
        Value::Empty => "empty".to_owned(),
        other => format!("other:{}", other.to_canonical_string()),
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::{
        DatasetVersion, MetricId, MetricTokenParts, SourceId, VerificationOutcome,
        VerificationStatus, VerifiedMetricToken,
    };
    use crate::error::SafiError;
    use crate::value::Value;

    /// Alias so the test signatures read the same as the rest of the workspace.
    type SafiErrorLike = SafiError;

    fn parts(value: Decimal) -> Result<MetricTokenParts, crate::error::SafiError> {
        Ok(MetricTokenParts {
            metric_id: MetricId::new("total_revenue")?,
            metric_name: "Total revenue".to_owned(),
            value: Value::Decimal(value),
            unit: Some("EGP".to_owned()),
            formula: "SUM(amount)".to_owned(),
            formula_version: 1,
            source_id: SourceId::new("sales")?,
            source_version: DatasetVersion::INITIAL,
            source_fingerprint: "source".to_owned(),
            row_refs: vec![0, 1],
            column_refs: vec!["amount".to_owned()],
            transformation_trace: vec!["t1".to_owned()],
            computed_at_ms: 1,
            status: VerificationStatus::Verified,
        })
    }

    fn token(value: Decimal) -> Result<VerifiedMetricToken, crate::error::SafiError> {
        Ok(VerifiedMetricToken::from_parts(parts(value)?))
    }

    #[test]
    fn identifiers_reject_empty_whitespace_and_separator_like_input() {
        assert!(SourceId::new("").is_err());
        assert!(SourceId::new("has space").is_err());
        assert!(SourceId::new("a".repeat(65)).is_err());
        assert!(MetricId::new("total_revenue").is_ok());
        assert!(MetricId::new("metric.v2:sum").is_ok());
    }

    #[test]
    fn fingerprint_ignores_timestamp_and_status_but_tracks_content() -> Result<(), SafiErrorLike> {
        let base = token(Decimal::new(1250, 2))?;
        let mut later = parts(Decimal::new(1250, 2))?;
        later.computed_at_ms = 9_999_999;
        later.status = VerificationStatus::Stale;
        let later = VerifiedMetricToken::from_parts(later);
        assert_eq!(base.fingerprint(), later.fingerprint());
        assert_eq!(base.computed_at_ms, 1);
        assert_eq!(later.computed_at_ms, 9_999_999);

        let scaled = token(Decimal::new(12_500, 3))?;
        assert_eq!(scaled.value, base.value);
        assert_eq!(scaled.fingerprint(), base.fingerprint());
        Ok(())
    }

    #[test]
    fn fingerprint_changes_when_value_formula_version_source_or_rows_change()
    -> Result<(), SafiErrorLike> {
        let base = token(Decimal::new(100, 0))?;

        let mut value_changed = parts(Decimal::new(108, 0))?;
        value_changed.value = Value::Decimal(Decimal::new(108, 0));
        let value_changed = VerifiedMetricToken::from_parts(value_changed);
        assert_ne!(base.fingerprint(), value_changed.fingerprint());

        let mut formula_changed = parts(Decimal::new(100, 0))?;
        formula_changed.formula = "SUM(amount) * 2".to_owned();
        let formula_changed = VerifiedMetricToken::from_parts(formula_changed);
        assert_ne!(base.fingerprint(), formula_changed.fingerprint());

        let mut version_changed = parts(Decimal::new(100, 0))?;
        version_changed.formula_version = 2;
        let version_changed = VerifiedMetricToken::from_parts(version_changed);
        assert_ne!(base.fingerprint(), version_changed.fingerprint());

        let mut source_changed = parts(Decimal::new(100, 0))?;
        source_changed.source_version = DatasetVersion::INITIAL.next();
        let source_changed = VerifiedMetricToken::from_parts(source_changed);
        assert_ne!(base.fingerprint(), source_changed.fingerprint());

        let mut rows_changed = parts(Decimal::new(100, 0))?;
        rows_changed.row_refs = vec![0, 1, 2];
        let rows_changed = VerifiedMetricToken::from_parts(rows_changed);
        assert_ne!(base.fingerprint(), rows_changed.fingerprint());
        Ok(())
    }

    #[test]
    fn verification_detects_a_displayed_number_that_no_longer_matches() -> Result<(), SafiErrorLike>
    {
        let shown = token(Decimal::new(100, 0))?;
        let recomputed = token(Decimal::new(100, 0))?;
        assert_eq!(
            shown.verify_against(&recomputed),
            VerificationOutcome::Verified
        );

        let recomputed = token(Decimal::new(900, 0))?;
        match shown.verify_against(&recomputed) {
            VerificationOutcome::Mismatch { expected, actual } => {
                assert_eq!(expected, shown.fingerprint());
                assert_eq!(actual, recomputed.fingerprint());
                assert_ne!(expected, actual);
            }
            VerificationOutcome::Verified => panic!("a different value must not verify"),
        }
        Ok(())
    }

    #[test]
    fn stale_tokens_keep_their_content_and_are_not_verifiable() -> Result<(), SafiErrorLike> {
        let token = token(Decimal::new(100, 0))?;
        let stale = token.as_stale();
        assert_eq!(stale.status, VerificationStatus::Stale);
        assert!(!stale.is_verifiable());
        assert!(token.is_verifiable());
        assert_eq!(stale.fingerprint(), token.fingerprint());
        Ok(())
    }
}
