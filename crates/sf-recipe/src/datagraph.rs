//! Studio `DataGraph`: one immutable source, deterministic transformations, verified metrics
//! (`STUDIO-PHASE-1`).
//!
//! Everything numeric is computed here, in Rust. Views receive [`VerifiedMetricToken`]s and only
//! format them; no view recomputes a sum, an average, or a formula.
//!
//! Pipeline: `origin → transformations → working table → metrics → tokens`.
//!
//! * The original table is never mutated. Edits are applied to a derived table and recorded as a
//!   replayable transformation log, so `replay() == working()` is an invariant and undo cannot
//!   corrupt the source.
//! * Metrics declare their column dependencies, so an edit recomputes only the metrics (and
//!   calculated columns) that actually depend on the edited column.
//! * Every token carries a `BLAKE3` fingerprint over formula + dataset version + provenance, so a
//!   stale or tampered number is detectable rather than silently displayed.
//!
//! Ownership note: the graph lives in `sf-recipe` because it applies cleaning transformations and
//! `sf-clean` already depends on `sf-core`; putting execution in `sf-core` would create a cycle
//! (`docs/reports/STUDIO-ARCHITECTURE-AUDIT.md`, section 6).
//!
//! Zero network dependencies (`AGENTS.md` Rule #7).

use rust_decimal::Decimal;
use sf_core::column::{Column, ColumnSchema, ColumnType, ConfidenceScore};
use sf_core::datagraph::{
    ColumnId, ColumnProvenance, DatasetVersion, MetricId, MetricTokenParts, Revision, RowId,
    RowProvenance, ScenarioId, SourceId, VerificationOutcome, VerificationStatus,
    VerifiedMetricToken,
};
use sf_core::error::{ErrorCode, ErrorSeverity, SafiError};
use sf_core::table::Table;
use sf_core::value::{CellValue, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

use crate::studio_anomaly::AnomalyFix;

/// Decimal places kept for averages; documented so results are reproducible across platforms.
pub const AVERAGE_SCALE: u32 = 6;

/// Maximum accepted formula length.
pub const MAX_FORMULA_LENGTH: usize = 512;

/// Maximum accepted nesting depth of a formula.
pub const MAX_FORMULA_DEPTH: usize = 32;

fn unknown_column() -> SafiError {
    SafiError::new(
        ErrorCode::SF_CAL_001,
        ErrorSeverity::Error,
        "UnknownColumnReference",
        "studio.unknown_column",
    )
}

fn invalid_formula() -> SafiError {
    SafiError::new(
        ErrorCode::SF_CAL_002,
        ErrorSeverity::Error,
        "InvalidFormulaSyntax",
        "studio.invalid_formula",
    )
}

fn division_by_zero() -> SafiError {
    SafiError::new(
        ErrorCode::SF_CAL_003,
        ErrorSeverity::Error,
        "DivisionByZero",
        "studio.division_by_zero",
    )
}

fn type_mismatch() -> SafiError {
    SafiError::new(
        ErrorCode::SF_CAL_004,
        ErrorSeverity::Error,
        "UnsupportedFormulaType",
        "studio.type_mismatch",
    )
}

fn unknown_parameter() -> SafiError {
    SafiError::new(
        ErrorCode::SF_CAL_005,
        ErrorSeverity::Error,
        "UnknownScenarioParameter",
        "studio.unknown_parameter",
    )
}

fn unknown_metric() -> SafiError {
    SafiError::new(
        ErrorCode::SF_CAL_006,
        ErrorSeverity::Error,
        "UnknownMetric",
        "studio.unknown_metric",
    )
}

fn stale_command() -> SafiError {
    SafiError::new(
        ErrorCode::SF_CAL_007,
        ErrorSeverity::Warn,
        "StaleStudioCommand",
        "studio.stale_command",
    )
}

fn verification_mismatch() -> SafiError {
    SafiError::new(
        ErrorCode::SF_CAL_008,
        ErrorSeverity::Error,
        "MetricVerificationMismatch",
        "studio.verification_mismatch",
    )
}

fn invalid_request() -> SafiError {
    SafiError::new(
        ErrorCode::SF_SYS_001,
        ErrorSeverity::Error,
        "InvalidStudioRequest",
        "studio.invalid_request",
    )
}

/// Aggregation functions allowed inside a metric formula.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AggFunc {
    /// Sum of the numeric values in a column.
    Sum,
    /// Mean of the numeric values in a column.
    Avg,
    /// Number of non-empty cells in a column.
    Count,
    /// Smallest numeric value in a column.
    Min,
    /// Largest numeric value in a column.
    Max,
}

impl AggFunc {
    /// Parses an aggregation name case-insensitively.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        let upper = name.to_ascii_uppercase();
        match upper.as_str() {
            "SUM" => Some(Self::Sum),
            "AVG" => Some(Self::Avg),
            "COUNT" => Some(Self::Count),
            "MIN" => Some(Self::Min),
            "MAX" => Some(Self::Max),
            _ => None,
        }
    }

    /// Returns the canonical function name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sum => "SUM",
            Self::Avg => "AVG",
            Self::Count => "COUNT",
            Self::Min => "MIN",
            Self::Max => "MAX",
        }
    }
}

/// Binary arithmetic operators allowed in formulas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    /// Addition.
    Add,
    /// Subtraction.
    Subtract,
    /// Multiplication.
    Multiply,
    /// Division (zero divisor is rejected, never approximated).
    Divide,
}

/// Validated, typed formula tree.
///
/// Aggregations are only legal in metric formulas; a reference is a column name in calculated
/// columns and a What-If parameter name in metrics.
#[derive(Debug, Clone, PartialEq)]
pub enum Formula {
    /// Exact decimal literal.
    Literal(Decimal),
    /// Column (calculated column) or parameter (metric) reference.
    Reference(String),
    /// Column aggregation (metrics only).
    Aggregate {
        /// Aggregation function.
        function: AggFunc,
        /// Column the aggregation runs over.
        column: String,
    },
    /// Arithmetic negation.
    Negate(Box<Self>),
    /// Binary arithmetic.
    Binary {
        /// Operator.
        op: BinaryOp,
        /// Left operand.
        left: Box<Self>,
        /// Right operand.
        right: Box<Self>,
    },
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(String),
    Identifier(String),
    BracketedIdentifier(String),
    OpenParen,
    CloseParen,
    Plus,
    Minus,
    Star,
    Slash,
}

fn tokenize(source: &str) -> Result<Vec<Token>, SafiError> {
    let mut tokens = Vec::new();
    let mut characters = source.chars().peekable();
    while let Some(character) = characters.peek().copied() {
        if character.is_whitespace() {
            let _ = characters.next();
            continue;
        }
        match character {
            '(' => {
                let _ = characters.next();
                tokens.push(Token::OpenParen);
            }
            ')' => {
                let _ = characters.next();
                tokens.push(Token::CloseParen);
            }
            '+' => {
                let _ = characters.next();
                tokens.push(Token::Plus);
            }
            '-' => {
                let _ = characters.next();
                tokens.push(Token::Minus);
            }
            '*' => {
                let _ = characters.next();
                tokens.push(Token::Star);
            }
            '/' => {
                let _ = characters.next();
                tokens.push(Token::Slash);
            }
            '.' | '0'..='9' => {
                let mut number = String::new();
                let mut digits = 0usize;
                while let Some(next) = characters.peek().copied() {
                    if next.is_ascii_digit() {
                        number.push(next);
                        digits += 1;
                        let _ = characters.next();
                    } else if next == '.' && !number.contains('.') {
                        number.push(next);
                        let _ = characters.next();
                    } else {
                        break;
                    }
                }
                if digits == 0 || number.starts_with('.') && number.ends_with('.') {
                    return Err(invalid_formula());
                }
                tokens.push(Token::Number(number));
            }
            '[' => {
                let _ = characters.next();
                let mut identifier = String::new();
                let mut closed = false;
                for next in characters.by_ref() {
                    if next == ']' {
                        closed = true;
                        break;
                    }
                    if next == '[' || next.is_control() {
                        return Err(invalid_formula());
                    }
                    identifier.push(next);
                    if identifier.len() > MAX_FORMULA_LENGTH {
                        return Err(invalid_formula());
                    }
                }
                let trimmed = identifier.trim();
                if !closed || trimmed.is_empty() {
                    return Err(invalid_formula());
                }
                tokens.push(Token::BracketedIdentifier(trimmed.to_owned()));
            }
            _ if character.is_alphabetic() || character == '_' => {
                let mut identifier = String::new();
                while let Some(next) = characters.peek().copied() {
                    if next.is_alphanumeric() || matches!(next, '_' | '.' | ':') {
                        identifier.push(next);
                        let _ = characters.next();
                    } else {
                        break;
                    }
                }
                tokens.push(Token::Identifier(identifier));
            }
            _ => return Err(invalid_formula()),
        }
        if tokens.len() > 128 {
            return Err(invalid_formula());
        }
    }
    Ok(tokens)
}

struct Parser<'a> {
    tokens: &'a [Token],
    cursor: usize,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&'a Token> {
        self.tokens.get(self.cursor)
    }

    fn advance(&mut self) -> Option<&'a Token> {
        let token = self.tokens.get(self.cursor);
        if token.is_some() {
            self.cursor += 1;
        }
        token
    }

    fn expression(&mut self) -> Result<Formula, SafiError> {
        let mut formula = self.term()?;
        loop {
            let op = match self.peek() {
                Some(Token::Plus) => BinaryOp::Add,
                Some(Token::Minus) => BinaryOp::Subtract,
                _ => return Ok(formula),
            };
            let _ = self.advance();
            let right = self.term()?;
            formula = Formula::Binary {
                op,
                left: Box::new(formula),
                right: Box::new(right),
            };
        }
    }

    fn term(&mut self) -> Result<Formula, SafiError> {
        let mut formula = self.factor()?;
        loop {
            let op = match self.peek() {
                Some(Token::Star) => BinaryOp::Multiply,
                Some(Token::Slash) => BinaryOp::Divide,
                _ => return Ok(formula),
            };
            let _ = self.advance();
            let right = self.factor()?;
            formula = Formula::Binary {
                op,
                left: Box::new(formula),
                right: Box::new(right),
            };
        }
    }

    fn factor(&mut self) -> Result<Formula, SafiError> {
        match self.advance() {
            Some(Token::Number(digits)) => Decimal::from_str(digits)
                .map(Formula::Literal)
                .map_err(|_| invalid_formula()),
            Some(Token::Minus) => Ok(Formula::Negate(Box::new(self.factor()?))),
            Some(Token::OpenParen) => {
                self.depth += 1;
                if self.depth > MAX_FORMULA_DEPTH {
                    return Err(invalid_formula());
                }
                let formula = self.expression()?;
                match self.advance() {
                    Some(Token::CloseParen) => {
                        self.depth -= 1;
                        Ok(formula)
                    }
                    _ => Err(invalid_formula()),
                }
            }
            Some(Token::Identifier(name)) => {
                if matches!(self.peek(), Some(Token::OpenParen)) {
                    let _ = self.advance();
                    let column = match self.advance() {
                        Some(Token::Identifier(column) | Token::BracketedIdentifier(column)) => {
                            column.clone()
                        }
                        _ => return Err(invalid_formula()),
                    };
                    match self.advance() {
                        Some(Token::CloseParen) => {}
                        _ => return Err(invalid_formula()),
                    }
                    let function = AggFunc::from_name(name).ok_or_else(invalid_formula)?;
                    Ok(Formula::Aggregate { function, column })
                } else {
                    Ok(Formula::Reference(name.clone()))
                }
            }
            Some(Token::BracketedIdentifier(name)) => Ok(Formula::Reference(name.clone())),
            _ => Err(invalid_formula()),
        }
    }
}

/// Parses a Studio formula into a validated tree.
///
/// # Errors
/// Returns `SF-CAL-002` when the source is empty, too long, too deeply nested, or syntactically
/// invalid. Unknown functions are rejected here rather than at evaluation time.
pub fn parse_formula(source: &str) -> Result<Formula, SafiError> {
    if source.trim().is_empty() || source.len() > MAX_FORMULA_LENGTH {
        return Err(invalid_formula());
    }
    let tokens = tokenize(source)?;
    if tokens.is_empty() {
        return Err(invalid_formula());
    }
    let mut parser = Parser {
        tokens: &tokens,
        cursor: 0,
        depth: 0,
    };
    let formula = parser.expression()?;
    if parser.cursor != tokens.len() {
        return Err(invalid_formula());
    }
    Ok(formula)
}

fn numeric_value(value: &Value) -> Option<Decimal> {
    match value {
        Value::Integer(integer) => Some(Decimal::from(*integer)),
        Value::Decimal(decimal) | Value::Percentage(decimal) => Some(*decimal),
        Value::Money(money) => Some(money.amount),
        Value::Measurement { magnitude, .. } => Some(*magnitude),
        _ => None,
    }
}

/// Reads a cell as an exact decimal, using the typed value when present and the matching text
/// otherwise (imported cells are raw text until profiling assigns a type).
fn cell_number(cell: &CellValue) -> Option<Decimal> {
    numeric_value(cell.value()).or_else(|| Decimal::from_str(cell.matching_text().trim()).ok())
}

/// How a formula resolves bare identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FormulaScope {
    /// Metric formulas: a bare identifier is a What-If parameter.
    Metric,
    /// Calculated columns: a bare identifier is a source column.
    Cell,
}

/// Columns and reference names a formula depends on, resolved for the given scope.
fn dependencies(
    formula: &Formula,
    scope: FormulaScope,
    columns: &mut BTreeSet<String>,
    references: &mut BTreeSet<String>,
) {
    match formula {
        Formula::Literal(_) => {}
        Formula::Reference(name) => match scope {
            FormulaScope::Metric => {
                references.insert(name.clone());
            }
            FormulaScope::Cell => {
                columns.insert(name.clone());
            }
        },
        Formula::Aggregate { column, .. } => {
            columns.insert(column.clone());
        }
        Formula::Negate(inner) => dependencies(inner, scope, columns, references),
        Formula::Binary { left, right, .. } => {
            dependencies(left, scope, columns, references);
            dependencies(right, scope, columns, references);
        }
    }
}

/// Returns `true` when the formula aggregates rows (legal only in metric formulas).
fn has_aggregate(formula: &Formula) -> bool {
    match formula {
        Formula::Aggregate { .. } => true,
        Formula::Negate(inner) => has_aggregate(inner),
        Formula::Binary { left, right, .. } => has_aggregate(left) || has_aggregate(right),
        Formula::Literal(_) | Formula::Reference(_) => false,
    }
}

trait FormulaContext {
    fn reference(&mut self, name: &str) -> Result<Decimal, SafiError>;
    fn aggregate(&mut self, function: AggFunc, column: &str) -> Result<Decimal, SafiError>;
}

fn evaluate(formula: &Formula, context: &mut dyn FormulaContext) -> Result<Decimal, SafiError> {
    match formula {
        Formula::Literal(value) => Ok(*value),
        Formula::Reference(name) => context.reference(name),
        Formula::Aggregate { function, column } => context.aggregate(*function, column),
        Formula::Negate(inner) => Ok(-evaluate(inner, context)?),
        Formula::Binary { op, left, right } => {
            let left = evaluate(left, context)?;
            let right = evaluate(right, context)?;
            match op {
                BinaryOp::Add => left.checked_add(right).ok_or_else(type_mismatch),
                BinaryOp::Subtract => left.checked_sub(right).ok_or_else(type_mismatch),
                BinaryOp::Multiply => left.checked_mul(right).ok_or_else(type_mismatch),
                BinaryOp::Divide => {
                    if right.is_zero() {
                        return Err(division_by_zero());
                    }
                    left.checked_div(right).ok_or_else(type_mismatch)
                }
            }
        }
    }
}

fn aggregate_column(
    table: &Table,
    row_provenance: &[RowProvenance],
    function: AggFunc,
    column: &str,
) -> Result<(Decimal, Vec<RowId>), SafiError> {
    if row_provenance.len() != table.row_count() {
        return Err(verification_mismatch());
    }
    let source = table.column_by_name(column).ok_or_else(unknown_column)?;
    let mut rows = Vec::new();
    let mut sum = Decimal::ZERO;
    let mut numeric = 0u64;
    let mut non_empty = 0u64;
    let mut extreme: Option<Decimal> = None;
    for (row, cell) in source.cells().iter().enumerate() {
        if cell.is_empty() {
            continue;
        }
        let row_id = row_provenance
            .get(row)
            .map(|entry| entry.row_id)
            .ok_or_else(verification_mismatch)?;
        non_empty += 1;
        if matches!(function, AggFunc::Count) {
            rows.push(row_id);
            continue;
        }
        let number = cell_number(cell).ok_or_else(type_mismatch)?;
        numeric += 1;
        rows.push(row_id);
        sum = sum.checked_add(number).ok_or_else(type_mismatch)?;
        extreme = Some(match (function, extreme) {
            (AggFunc::Min, Some(current)) => current.min(number),
            (AggFunc::Max, Some(current)) => current.max(number),
            _ => number,
        });
    }
    let value = match function {
        AggFunc::Count => Decimal::from(non_empty),
        AggFunc::Sum => sum,
        AggFunc::Avg => {
            if numeric == 0 {
                return Err(division_by_zero());
            }
            sum.checked_div(Decimal::from(numeric))
                .ok_or_else(type_mismatch)?
                .round_dp(AVERAGE_SCALE)
        }
        AggFunc::Min | AggFunc::Max => extreme.ok_or_else(type_mismatch)?,
    };
    Ok((value, rows))
}

struct MetricContext<'a> {
    table: &'a Table,
    row_provenance: &'a [RowProvenance],
    parameters: &'a BTreeMap<String, Decimal>,
    rows: BTreeSet<RowId>,
    columns: BTreeSet<String>,
}

impl FormulaContext for MetricContext<'_> {
    fn reference(&mut self, name: &str) -> Result<Decimal, SafiError> {
        self.parameters
            .get(name)
            .copied()
            .ok_or_else(unknown_parameter)
    }

    fn aggregate(&mut self, function: AggFunc, column: &str) -> Result<Decimal, SafiError> {
        let (value, rows) = aggregate_column(self.table, self.row_provenance, function, column)?;
        self.columns.insert(column.to_owned());
        self.rows.extend(rows);
        Ok(value)
    }
}

struct CellContext<'a> {
    table: &'a Table,
    row: usize,
}

impl FormulaContext for CellContext<'_> {
    fn reference(&mut self, name: &str) -> Result<Decimal, SafiError> {
        let column = self.table.column_by_name(name).ok_or_else(unknown_column)?;
        let cell = column.cell_at(self.row).ok_or_else(invalid_request)?;
        cell_number(cell).ok_or_else(type_mismatch)
    }

    fn aggregate(&mut self, _function: AggFunc, _column: &str) -> Result<Decimal, SafiError> {
        // Aggregations are metric-only; a calculated column is evaluated per row.
        Err(type_mismatch())
    }
}

/// A recorded, replayable change.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Transformation {
    /// Replace one cell of a source column using a legacy positional row index.
    SetCell {
        /// Zero-based row.
        row: usize,
        /// Column name.
        column: String,
        /// New raw text.
        raw: String,
    },
    /// Replace one cell of a source column using a stable row identity.
    SetCellByRowId {
        /// Stable row identity.
        row_id: RowId,
        /// Column name.
        column: String,
        /// New raw text.
        raw: String,
    },
    /// Append a calculated column derived from a formula.
    AddColumn {
        /// New column name.
        name: String,
        /// Formula source text.
        formula: String,
    },
    /// Sort working rows by a column without altering any [`RowId`] provenance.
    SortRows {
        /// Column name to sort by.
        column: String,
        /// Sort ascending (`true`) or descending (`false`).
        ascending: bool,
    },
    /// Keep only working rows matching the query while preserving stable [`RowId`]s.
    FilterRows {
        /// Column name to inspect.
        column: String,
        /// Case-insensitive substring query.
        query: String,
    },
    /// Delete a row identified by its stable [`RowId`] without ever reusing its identity.
    DeleteRow {
        /// Stable row identity to remove.
        row_id: RowId,
    },
    /// Insert a new row with a freshly allocated monotonic [`RowId`].
    InsertRow {
        /// Allocated stable row identity (never reused).
        row_id: RowId,
        /// Optional stable row identity after which the new row is placed; `None` appends at end.
        after_row_id: Option<RowId>,
        /// Raw cell text keyed by column name.
        values: BTreeMap<String, String>,
    },
    /// User-confirmed atomic anomaly remediation batch (undoable as one step).
    AnomalyFixBatch(AnomalyFix),
    /// Add a new editable source column with monotonic [`ColumnId`].
    AddSourceColumn {
        /// Stable column identity.
        column_id: ColumnId,
        /// Column display name.
        name: String,
        /// Initial raw text filled into existing working rows.
        default_value: String,
    },
    /// Rename an existing working column while preserving its stable [`ColumnId`].
    RenameColumn {
        /// Current column name.
        old_name: String,
        /// Replacement column name.
        new_name: String,
    },
    /// Remove a working column without ever reusing its [`ColumnId`].
    DeleteColumn {
        /// Column name to drop.
        name: String,
    },
    /// Trim leading/trailing and collapse internal whitespace on a column's cells atomically.
    TrimColumn {
        /// Target column name.
        column: String,
    },
}

impl Transformation {
    fn columns(&self) -> BTreeSet<String> {
        match self {
            Self::SetCell { column, .. }
            | Self::SetCellByRowId { column, .. }
            | Self::SortRows { column, .. }
            | Self::FilterRows { column, .. }
            | Self::TrimColumn { column } => BTreeSet::from([column.clone()]),
            Self::AddColumn { name, .. }
            | Self::AddSourceColumn { name, .. }
            | Self::DeleteColumn { name } => BTreeSet::from([name.clone()]),
            Self::RenameColumn { old_name, new_name } => {
                BTreeSet::from([old_name.clone(), new_name.clone()])
            }
            Self::DeleteRow { .. } => BTreeSet::new(),
            Self::InsertRow { values, .. } => values.keys().cloned().collect(),
            Self::AnomalyFixBatch(fix) => match fix {
                AnomalyFix::FillMissing { column, .. }
                | AnomalyFix::BlankInvalidDates { column, .. } => BTreeSet::from([column.clone()]),
                AnomalyFix::DropDuplicateRows { .. } => BTreeSet::new(),
            },
        }
    }

    fn touches_all_columns(&self) -> bool {
        matches!(
            self,
            Self::DeleteRow { .. }
                | Self::InsertRow { .. }
                | Self::FilterRows { .. }
                | Self::AnomalyFixBatch(AnomalyFix::DropDuplicateRows { .. })
        )
    }

    fn digest(&self) -> String {
        let payload = match self {
            Self::SetCell { row, column, raw } => {
                format!(
                    "set:{row}:{column}:{}",
                    blake3::hash(raw.as_bytes()).to_hex()
                )
            }
            Self::SetCellByRowId {
                row_id,
                column,
                raw,
            } => {
                format!(
                    "setid:{}:{column}:{}",
                    row_id.get(),
                    blake3::hash(raw.as_bytes()).to_hex()
                )
            }
            Self::AddColumn { name, formula } => format!("add:{name}:{formula}"),
            Self::SortRows { column, ascending } => format!("sort:{column}:{ascending}"),
            Self::FilterRows { column, query } => {
                format!(
                    "filter:{column}:{}",
                    blake3::hash(query.as_bytes()).to_hex()
                )
            }
            Self::DeleteRow { row_id } => format!("del:{}", row_id.get()),
            Self::InsertRow {
                row_id,
                after_row_id,
                values,
            } => {
                let mut pairs = String::new();
                for (key, value) in values {
                    pairs.push_str(key);
                    pairs.push('=');
                    pairs.push_str(&blake3::hash(value.as_bytes()).to_hex());
                    pairs.push(';');
                }
                let after = after_row_id.map_or(0, RowId::get);
                format!("ins:{}:{after}:{pairs}", row_id.get())
            }
            Self::AnomalyFixBatch(fix) => {
                let encoded = serde_json::to_string(fix).unwrap_or_default();
                format!("fix:{}", blake3::hash(encoded.as_bytes()).to_hex())
            }
            Self::AddSourceColumn {
                column_id,
                name,
                default_value,
            } => format!(
                "addcol:{}:{name}:{}",
                column_id.get(),
                blake3::hash(default_value.as_bytes()).to_hex()
            ),
            Self::RenameColumn { old_name, new_name } => format!("ren:{old_name}:{new_name}"),
            Self::DeleteColumn { name } => format!("delcol:{name}"),
            Self::TrimColumn { column } => format!("trim:{column}"),
        };
        blake3::hash(payload.as_bytes()).to_hex().to_string()
    }
}

/// Result of an accepted change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeOutcome {
    /// New graph revision.
    pub revision: Revision,
    /// Metrics that were recomputed (in deterministic order).
    pub recomputed: Vec<MetricId>,
    /// Rows that fed the recomputed metrics.
    pub rows: Vec<u64>,
    /// Columns that changed.
    pub columns: Vec<String>,
}

/// A named set of What-If parameter overrides.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Scenario {
    id: ScenarioId,
    name: String,
    parameters: BTreeMap<String, Decimal>,
}

impl Scenario {
    /// Returns the scenario identifier.
    #[must_use]
    pub fn id(&self) -> &ScenarioId {
        &self.id
    }

    /// Returns the display name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the parameter overrides.
    #[must_use]
    pub fn parameters(&self) -> &BTreeMap<String, Decimal> {
        &self.parameters
    }
}

/// Two values of the same metric under two scenarios.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioComparison {
    /// Metric identifier.
    pub metric_id: MetricId,
    /// Metric display name.
    pub metric_name: String,
    /// Value under the left scenario.
    pub left: Decimal,
    /// Value under the right scenario.
    pub right: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
struct MetricDefinition {
    id: MetricId,
    name: String,
    unit: Option<String>,
    source: String,
    formula: Formula,
    version: u32,
    columns: BTreeSet<String>,
    parameters: BTreeSet<String>,
}

/// The single source of truth for Studio data and metrics.
#[derive(Debug, Clone)]
pub struct DataGraph {
    source_id: SourceId,
    origin: Table,
    origin_fingerprint: String,
    dataset_version: DatasetVersion,
    revision: Revision,
    working: Table,
    row_provenance: Vec<RowProvenance>,
    column_provenance: Vec<ColumnProvenance>,
    next_row_id: u64,
    next_column_id: u64,
    transforms: Vec<Transformation>,
    redo: Vec<Transformation>,
    calculated: Vec<(String, Formula)>,
    metrics: BTreeMap<MetricId, MetricDefinition>,
    tokens: BTreeMap<MetricId, VerifiedMetricToken>,
    errors: BTreeMap<MetricId, SafiError>,
    defaults: BTreeMap<String, Decimal>,
    scenarios: BTreeMap<ScenarioId, Scenario>,
    active_scenario: Option<ScenarioId>,
    last_recomputed: Vec<MetricId>,
    last_rows: Vec<u64>,
    last_columns: Vec<String>,
}

fn initial_row_provenance(origin: &Table) -> Vec<RowProvenance> {
    (0..origin.row_count())
        .map(|index| {
            let row_id = RowId::from_raw((index as u64).saturating_add(1));
            RowProvenance {
                row_id,
                origin_row_id: Some(row_id),
                origin_row_index: Some(index),
            }
        })
        .collect()
}

fn initial_column_provenance(origin: &Table) -> Vec<ColumnProvenance> {
    origin
        .columns()
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let name = column.schema().name.clone();
            ColumnProvenance {
                column_id: ColumnId::from_raw((index as u64).saturating_add(1)),
                origin_column_index: Some(index),
                origin_name: Some(name.clone()),
                current_name: name,
            }
        })
        .collect()
}

fn table_fingerprint(table: &Table) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(table.id().as_bytes());
    hasher.update(&[0x1e]);
    hasher.update(table.name().as_bytes());
    for column in table.columns() {
        hasher.update(&[0x1f]);
        hasher.update(column.schema().name.as_bytes());
        for cell in column.cells() {
            hasher.update(&[0x1d]);
            hasher.update(cell.matching_text().as_bytes());
        }
    }
    hasher.finalize().to_hex().to_string()
}

fn unique_columns(table: &Table) -> Result<(), SafiError> {
    let mut seen = BTreeSet::new();
    for column in table.columns() {
        if column.schema().name.trim().is_empty() || !seen.insert(&column.schema().name) {
            return Err(invalid_request());
        }
    }
    Ok(())
}

fn replace_cell(
    table: &Table,
    row: usize,
    column: usize,
    cell: CellValue,
) -> Result<Table, SafiError> {
    let mut columns = Vec::with_capacity(table.column_count());
    for (index, existing) in table.columns().iter().enumerate() {
        let mut cells = existing.cells().to_vec();
        if index == column {
            let slot = cells.get_mut(row).ok_or_else(invalid_request)?;
            *slot = cell.clone();
        }
        columns.push(Column::new(existing.schema().clone(), cells));
    }
    Table::new(table.id(), table.name(), columns)
}

fn append_column(table: &Table, name: &str, cells: Vec<CellValue>) -> Result<Table, SafiError> {
    let mut columns = table.columns().to_vec();
    let schema = ColumnSchema::new(
        columns.len(),
        name,
        ColumnType::Unknown,
        ConfidenceScore::ONE,
    );
    columns.push(Column::new(schema, cells));
    Table::new(table.id(), table.name(), columns)
}

fn compare_cells(left: &CellValue, right: &CellValue) -> std::cmp::Ordering {
    match (left.is_empty(), right.is_empty()) {
        (true, true) => return std::cmp::Ordering::Equal,
        (true, false) => return std::cmp::Ordering::Greater,
        (false, true) => return std::cmp::Ordering::Less,
        (false, false) => {}
    }
    if let (Some(a), Some(b)) = (cell_number(left), cell_number(right)) {
        return a.cmp(&b);
    }
    let a_lower = left.visible_text().to_lowercase();
    let b_lower = right.visible_text().to_lowercase();
    a_lower
        .cmp(&b_lower)
        .then_with(|| left.visible_text().cmp(&right.visible_text()))
}

fn reorder_table_and_provenance(
    table: &Table,
    row_provenance: &[RowProvenance],
    indices: &[usize],
) -> Result<(Table, Vec<RowProvenance>), SafiError> {
    let mut columns = Vec::with_capacity(table.column_count());
    for column in table.columns() {
        let mut cells = Vec::with_capacity(indices.len());
        for &index in indices {
            let cell = column.cell_at(index).ok_or_else(invalid_request)?;
            cells.push(cell.clone());
        }
        columns.push(Column::new(column.schema().clone(), cells));
    }
    let mut next_prov = Vec::with_capacity(indices.len());
    for &index in indices {
        let entry = row_provenance
            .get(index)
            .copied()
            .ok_or_else(invalid_request)?;
        next_prov.push(entry);
    }
    Ok((Table::new(table.id(), table.name(), columns)?, next_prov))
}

fn insert_row_at(
    table: &Table,
    row_provenance: &[RowProvenance],
    insert_at: usize,
    row_id: RowId,
    values: &BTreeMap<String, String>,
) -> Result<(Table, Vec<RowProvenance>), SafiError> {
    if insert_at > table.row_count() || row_provenance.len() != table.row_count() {
        return Err(invalid_request());
    }
    let mut columns = Vec::with_capacity(table.column_count());
    for column in table.columns() {
        let mut cells = column.cells().to_vec();
        let raw = values
            .get(&column.schema().name)
            .cloned()
            .unwrap_or_default();
        cells.insert(insert_at, CellValue::from_raw(raw));
        columns.push(Column::new(column.schema().clone(), cells));
    }
    let mut next_prov = row_provenance.to_vec();
    next_prov.insert(
        insert_at,
        RowProvenance {
            row_id,
            origin_row_id: None,
            origin_row_index: None,
        },
    );
    Ok((Table::new(table.id(), table.name(), columns)?, next_prov))
}

struct MutableGraphState {
    table: Table,
    row_provenance: Vec<RowProvenance>,
    column_provenance: Vec<ColumnProvenance>,
    active_calculated: Vec<(String, Formula)>,
    next_column_id: u64,
}

fn refresh_calculated_columns(
    table: &mut Table,
    calculated: &[(String, Formula)],
) -> Result<(), SafiError> {
    for (name, formula) in calculated {
        let cells = evaluate_column(table, formula)?;
        let index = table
            .columns()
            .iter()
            .position(|column| column.schema().name == *name)
            .ok_or_else(unknown_column)?;
        let mut rebuilt = Vec::with_capacity(table.column_count());
        for (position, column) in table.columns().iter().enumerate() {
            if position == index {
                rebuilt.push(Column::new(column.schema().clone(), cells.clone()));
            } else {
                rebuilt.push(column.clone());
            }
        }
        *table = Table::new(table.id(), table.name(), rebuilt)?;
    }
    Ok(())
}

fn apply_transformation_step(
    state: &mut MutableGraphState,
    transformation: &Transformation,
) -> Result<(), SafiError> {
    match transformation {
        Transformation::SetCell { row, column, raw } => {
            let index = state
                .table
                .columns()
                .iter()
                .position(|existing| existing.schema().name == *column)
                .ok_or_else(unknown_column)?;
            state.table =
                replace_cell(&state.table, *row, index, CellValue::from_raw(raw.clone()))?;
            refresh_calculated_columns(&mut state.table, &state.active_calculated)?;
        }
        Transformation::SetCellByRowId {
            row_id,
            column,
            raw,
        } => {
            let row = state
                .row_provenance
                .iter()
                .position(|entry| entry.row_id == *row_id)
                .ok_or_else(invalid_request)?;
            let index = state
                .table
                .columns()
                .iter()
                .position(|existing| existing.schema().name == *column)
                .ok_or_else(unknown_column)?;
            state.table = replace_cell(&state.table, row, index, CellValue::from_raw(raw.clone()))?;
            refresh_calculated_columns(&mut state.table, &state.active_calculated)?;
        }
        Transformation::AddColumn { name, formula } => {
            let parsed = parse_formula(formula)?;
            let cells = evaluate_column(&state.table, &parsed)?;
            state.table = append_column(&state.table, name, cells)?;
            let column_id = ColumnId::from_raw(state.next_column_id);
            state.next_column_id = state.next_column_id.saturating_add(1);
            state.column_provenance.push(ColumnProvenance {
                column_id,
                origin_column_index: None,
                origin_name: Some(name.clone()),
                current_name: name.clone(),
            });
            state.active_calculated.push((name.clone(), parsed));
        }
        Transformation::SortRows { column, ascending } => {
            let col = state
                .table
                .column_by_name(column)
                .ok_or_else(unknown_column)?;
            let mut indices: Vec<usize> = (0..state.table.row_count()).collect();
            indices.sort_by(|&a, &b| {
                let left = col.cell_at(a);
                let right = col.cell_at(b);
                let ord = match (left, right) {
                    (Some(l), Some(r)) => compare_cells(l, r),
                    _ => std::cmp::Ordering::Equal,
                };
                let ord = if *ascending { ord } else { ord.reverse() };
                ord.then_with(|| a.cmp(&b))
            });
            let (table, row_provenance) =
                reorder_table_and_provenance(&state.table, &state.row_provenance, &indices)?;
            state.table = table;
            state.row_provenance = row_provenance;
        }
        Transformation::FilterRows { column, query } => {
            let col = state
                .table
                .column_by_name(column)
                .ok_or_else(unknown_column)?;
            let needle = query.trim().to_lowercase();
            let indices: Vec<usize> = (0..state.table.row_count())
                .filter(|&row| {
                    col.cell_at(row).is_some_and(|cell| {
                        needle.is_empty() || cell.visible_text().to_lowercase().contains(&needle)
                    })
                })
                .collect();
            let (table, row_provenance) =
                reorder_table_and_provenance(&state.table, &state.row_provenance, &indices)?;
            state.table = table;
            state.row_provenance = row_provenance;
        }
        Transformation::DeleteRow { row_id } => {
            let pos = state
                .row_provenance
                .iter()
                .position(|entry| entry.row_id == *row_id)
                .ok_or_else(invalid_request)?;
            let indices: Vec<usize> = (0..state.table.row_count()).filter(|&r| r != pos).collect();
            let (table, row_provenance) =
                reorder_table_and_provenance(&state.table, &state.row_provenance, &indices)?;
            state.table = table;
            state.row_provenance = row_provenance;
        }
        Transformation::InsertRow {
            row_id,
            after_row_id,
            values,
        } => {
            let insert_at = match after_row_id {
                Some(after) => state
                    .row_provenance
                    .iter()
                    .position(|entry| entry.row_id == *after)
                    .map(|idx| idx + 1)
                    .ok_or_else(invalid_request)?,
                None => state.table.row_count(),
            };
            let (mut table, row_provenance) = insert_row_at(
                &state.table,
                &state.row_provenance,
                insert_at,
                *row_id,
                values,
            )?;
            refresh_calculated_columns(&mut table, &state.active_calculated)?;
            state.table = table;
            state.row_provenance = row_provenance;
        }
        Transformation::AnomalyFixBatch(fix) => match fix {
            AnomalyFix::FillMissing {
                column,
                row_ids,
                replacement,
            } => {
                if row_ids.is_empty() {
                    return Err(invalid_formula());
                }
                let col_idx = state
                    .table
                    .columns()
                    .iter()
                    .position(|existing| existing.schema().name == *column)
                    .ok_or_else(unknown_column)?;
                let mut table = state.table.clone();
                for raw_id in row_ids {
                    let row_idx = state
                        .row_provenance
                        .iter()
                        .position(|entry| entry.row_id.get() == *raw_id)
                        .ok_or_else(invalid_request)?;
                    table = replace_cell(
                        &table,
                        row_idx,
                        col_idx,
                        CellValue::from_raw(replacement.clone()),
                    )?;
                }
                refresh_calculated_columns(&mut table, &state.active_calculated)?;
                state.table = table;
            }
            AnomalyFix::BlankInvalidDates { column, row_ids } => {
                if row_ids.is_empty() {
                    return Err(invalid_formula());
                }
                let col_idx = state
                    .table
                    .columns()
                    .iter()
                    .position(|existing| existing.schema().name == *column)
                    .ok_or_else(unknown_column)?;
                let mut table = state.table.clone();
                for raw_id in row_ids {
                    let row_idx = state
                        .row_provenance
                        .iter()
                        .position(|entry| entry.row_id.get() == *raw_id)
                        .ok_or_else(invalid_request)?;
                    table = replace_cell(&table, row_idx, col_idx, CellValue::from_raw(""))?;
                }
                refresh_calculated_columns(&mut table, &state.active_calculated)?;
                state.table = table;
            }
            AnomalyFix::DropDuplicateRows { row_ids } => {
                if row_ids.is_empty() {
                    return Err(invalid_formula());
                }
                let mut drop_set = BTreeSet::new();
                for raw_id in row_ids {
                    if !state
                        .row_provenance
                        .iter()
                        .any(|entry| entry.row_id.get() == *raw_id)
                    {
                        return Err(invalid_request());
                    }
                    let _ = drop_set.insert(*raw_id);
                }
                let indices: Vec<usize> = state
                    .row_provenance
                    .iter()
                    .enumerate()
                    .filter(|(_, entry)| !drop_set.contains(&entry.row_id.get()))
                    .map(|(idx, _)| idx)
                    .collect();
                let (table, row_provenance) =
                    reorder_table_and_provenance(&state.table, &state.row_provenance, &indices)?;
                state.table = table;
                state.row_provenance = row_provenance;
            }
        },
        Transformation::AddSourceColumn {
            column_id,
            name,
            default_value,
        } => {
            let trimmed = name.trim();
            if trimmed.is_empty()
                || state
                    .table
                    .columns()
                    .iter()
                    .any(|c| c.schema().name == trimmed)
            {
                return Err(invalid_request());
            }
            let cells: Vec<CellValue> = (0..state.table.row_count())
                .map(|_| CellValue::from_raw(default_value.clone()))
                .collect();
            state.table = append_column(&state.table, trimmed, cells)?;
            state.next_column_id = state.next_column_id.max(column_id.get().saturating_add(1));
            state.column_provenance.push(ColumnProvenance {
                column_id: *column_id,
                origin_column_index: None,
                origin_name: None,
                current_name: trimmed.to_owned(),
            });
            refresh_calculated_columns(&mut state.table, &state.active_calculated)?;
        }
        Transformation::RenameColumn { old_name, new_name } => {
            let trimmed_new = new_name.trim();
            if trimmed_new.is_empty()
                || state
                    .table
                    .columns()
                    .iter()
                    .any(|c| c.schema().name == trimmed_new && c.schema().name != *old_name)
            {
                return Err(invalid_request());
            }
            let col_idx = state
                .table
                .columns()
                .iter()
                .position(|c| c.schema().name == *old_name)
                .ok_or_else(unknown_column)?;
            let mut columns = Vec::with_capacity(state.table.column_count());
            for (idx, col) in state.table.columns().iter().enumerate() {
                let mut schema = col.schema().clone();
                if idx == col_idx {
                    schema.name = trimmed_new.to_owned();
                }
                columns.push(Column::new(schema, col.cells().to_vec()));
            }
            state.table = Table::new(state.table.id(), state.table.name(), columns)?;
            if let Some(prov) = state.column_provenance.get_mut(col_idx) {
                prov.current_name = trimmed_new.to_owned();
            }
            for (calc_name, _) in &mut state.active_calculated {
                if calc_name == old_name {
                    *calc_name = trimmed_new.to_owned();
                }
            }
        }
        Transformation::DeleteColumn { name } => {
            if state.table.column_count() <= 1 {
                return Err(invalid_formula());
            }
            let col_idx = state
                .table
                .columns()
                .iter()
                .position(|c| c.schema().name == *name)
                .ok_or_else(unknown_column)?;
            let mut columns = Vec::with_capacity(state.table.column_count().saturating_sub(1));
            for (idx, col) in state.table.columns().iter().enumerate() {
                if idx == col_idx {
                    continue;
                }
                let mut schema = col.schema().clone();
                schema.index = columns.len();
                columns.push(Column::new(schema, col.cells().to_vec()));
            }
            state.table = Table::new(state.table.id(), state.table.name(), columns)?;
            if col_idx < state.column_provenance.len() {
                let _ = state.column_provenance.remove(col_idx);
            }
            state
                .active_calculated
                .retain(|(calc_name, _)| calc_name != name);
        }
        Transformation::TrimColumn { column } => {
            let col_idx = state
                .table
                .columns()
                .iter()
                .position(|c| c.schema().name == *column)
                .ok_or_else(unknown_column)?;
            let mut columns = Vec::with_capacity(state.table.column_count());
            for (idx, existing) in state.table.columns().iter().enumerate() {
                if idx == col_idx {
                    let cells = existing
                        .cells()
                        .iter()
                        .map(|cell| {
                            let collapsed = cell
                                .raw_text()
                                .split_whitespace()
                                .collect::<Vec<_>>()
                                .join(" ");
                            CellValue::from_raw(collapsed)
                        })
                        .collect();
                    columns.push(Column::new(existing.schema().clone(), cells));
                } else {
                    columns.push(existing.clone());
                }
            }
            state.table = Table::new(state.table.id(), state.table.name(), columns)?;
            refresh_calculated_columns(&mut state.table, &state.active_calculated)?;
        }
    }
    Ok(())
}

fn evaluate_column(table: &Table, formula: &Formula) -> Result<Vec<CellValue>, SafiError> {
    let mut cells = Vec::with_capacity(table.row_count());
    for row in 0..table.row_count() {
        let mut context = CellContext { table, row };
        let value = evaluate(formula, &mut context)?;
        cells.push(
            CellValue::from_raw(value.normalize().to_string()).with_value(Value::Decimal(value)),
        );
    }
    Ok(cells)
}

struct ReplayedProvenance {
    table: Table,
    row_provenance: Vec<RowProvenance>,
    column_provenance: Vec<ColumnProvenance>,
    max_row_id: u64,
    next_column_id: u64,
}

impl DataGraph {
    /// Creates a graph over an immutable original dataset.
    ///
    /// # Errors
    /// Returns `SF-SYS-001` when column names are empty or duplicated.
    pub fn new(source_id: SourceId, origin: Table, now_ms: u64) -> Result<Self, SafiError> {
        unique_columns(&origin)?;
        let fingerprint = table_fingerprint(&origin);
        let row_provenance = initial_row_provenance(&origin);
        let column_provenance = initial_column_provenance(&origin);
        let next_row_id = (origin.row_count() as u64).saturating_add(1);
        let next_column_id = (origin.column_count() as u64).saturating_add(1);
        let working = origin.clone();
        let mut graph = Self {
            source_id,
            origin,
            origin_fingerprint: fingerprint,
            dataset_version: DatasetVersion::INITIAL,
            revision: Revision::INITIAL,
            working,
            row_provenance,
            column_provenance,
            next_row_id,
            next_column_id,
            transforms: Vec::new(),
            redo: Vec::new(),
            calculated: Vec::new(),
            metrics: BTreeMap::new(),
            tokens: BTreeMap::new(),
            errors: BTreeMap::new(),
            defaults: BTreeMap::new(),
            scenarios: BTreeMap::new(),
            active_scenario: None,
            last_recomputed: Vec::new(),
            last_rows: Vec::new(),
            last_columns: Vec::new(),
        };
        graph.refresh_all(now_ms);
        Ok(graph)
    }

    /// Returns the source identifier.
    #[must_use]
    pub fn source_id(&self) -> &SourceId {
        &self.source_id
    }

    /// Returns the immutable original dataset.
    #[must_use]
    pub fn origin(&self) -> &Table {
        &self.origin
    }

    /// Returns the derived dataset after all transformations.
    #[must_use]
    pub fn working(&self) -> &Table {
        &self.working
    }

    /// Returns row provenance entries aligned with the working table's display order.
    #[must_use]
    pub fn row_provenance(&self) -> &[RowProvenance] {
        &self.row_provenance
    }

    /// Returns column provenance entries aligned with the working table's column order.
    #[must_use]
    pub fn column_provenance(&self) -> &[ColumnProvenance] {
        &self.column_provenance
    }

    /// Resolves a stable [`RowId`] to its current zero-based index in `working()`, if still present.
    #[must_use]
    pub fn working_row_for_id(&self, row_id: RowId) -> Option<usize> {
        self.row_provenance
            .iter()
            .position(|entry| entry.row_id == row_id)
    }

    /// Returns the stable [`RowId`] currently displayed at zero-based row `index`.
    #[must_use]
    pub fn row_id_at(&self, index: usize) -> Option<RowId> {
        self.row_provenance.get(index).map(|entry| entry.row_id)
    }

    /// Resolves a column name to its stable [`ColumnId`].
    #[must_use]
    pub fn column_id_for_name(&self, name: &str) -> Option<ColumnId> {
        self.column_provenance
            .iter()
            .find(|entry| entry.current_name == name)
            .map(|entry| entry.column_id)
    }

    /// Queries a bounded page of working rows without serializing the entire dataset.
    ///
    /// Sorting or searching here only reorders/filters the viewport query and never alters
    /// stable [`RowId`] identities or metric aggregations.
    ///
    /// # Errors
    /// `SF-CAL-001` when `sort_column` or `filter_column` names a missing column.
    pub fn rows_page(&self, query: &RowQuery) -> Result<RowPage, SafiError> {
        if let Some(ref column) = query.sort_column
            && self.working.column_by_name(column).is_none()
        {
            return Err(unknown_column());
        }
        if let Some(ref column) = query.filter_column
            && self.working.column_by_name(column).is_none()
        {
            return Err(unknown_column());
        }
        let search_needle = query
            .search
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_lowercase);
        let filter_needle = query
            .filter_value
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_lowercase);
        let filter_col = query
            .filter_column
            .as_deref()
            .and_then(|name| self.working.column_by_name(name));

        let mut matched: Vec<usize> = (0..self.working.row_count())
            .filter(|&row| {
                if let (Some(col), Some(needle)) = (filter_col, filter_needle.as_deref()) {
                    let ok = col
                        .cell_at(row)
                        .is_some_and(|c| c.visible_text().to_lowercase().contains(needle));
                    if !ok {
                        return false;
                    }
                }
                if let Some(needle) = search_needle.as_deref() {
                    let any_match = self.working.columns().iter().any(|col| {
                        col.cell_at(row)
                            .is_some_and(|c| c.visible_text().to_lowercase().contains(needle))
                    });
                    if !any_match {
                        return false;
                    }
                }
                true
            })
            .collect();

        if let Some(ref sort_col_name) = query.sort_column
            && let Some(sort_col) = self.working.column_by_name(sort_col_name)
        {
            matched.sort_by(|&a, &b| {
                let ord = match (sort_col.cell_at(a), sort_col.cell_at(b)) {
                    (Some(l), Some(r)) => compare_cells(l, r),
                    _ => std::cmp::Ordering::Equal,
                };
                let ord = if query.ascending { ord } else { ord.reverse() };
                ord.then_with(|| a.cmp(&b))
            });
        }

        let filtered_rows = matched.len();
        let limit = query.limit.clamp(1, MAX_ROW_PAGE_LIMIT);
        let offset = query.offset.min(filtered_rows);
        let end = offset.saturating_add(limit).min(filtered_rows);
        let slice = &matched[offset..end];

        let mut rows = Vec::with_capacity(slice.len());
        for &working_row in slice {
            let provenance = self
                .row_provenance
                .get(working_row)
                .copied()
                .ok_or_else(verification_mismatch)?;
            let mut cells = Vec::with_capacity(self.working.column_count());
            for (col_idx, column) in self.working.columns().iter().enumerate() {
                let cell = column.cell_at(working_row).ok_or_else(invalid_request)?;
                let changed = provenance
                    .origin_row_index
                    .and_then(|orig_row| self.origin.cell(orig_row, col_idx))
                    .is_none_or(|orig_cell| orig_cell.raw_text() != cell.raw_text());
                let validation_error = match column.schema().column_type {
                    ColumnType::MoneyAndCurrency | ColumnType::Percentage
                        if !cell.is_empty() && cell_number(cell).is_none() =>
                    {
                        Some("SF-CAL-004".to_owned())
                    }
                    _ => None,
                };
                cells.push(RowCellView {
                    raw: cell.raw_text().to_owned(),
                    normalized: cell.matching_text().to_owned(),
                    visible: cell.visible_text().to_owned(),
                    changed,
                    validation_error,
                });
            }
            rows.push(RowPageItem {
                working_row,
                row_id: provenance.row_id.get(),
                origin_row_id: provenance.origin_row_id.map(RowId::get),
                origin_row_index: provenance.origin_row_index,
                cells,
            });
        }

        Ok(RowPage {
            total_rows: self.working.row_count(),
            filtered_rows,
            offset,
            limit,
            revision: self.revision.get(),
            rows,
        })
    }

    /// Returns the published revision.
    #[must_use]
    pub const fn revision(&self) -> Revision {
        self.revision
    }

    /// Returns the dataset version.
    #[must_use]
    pub const fn dataset_version(&self) -> DatasetVersion {
        self.dataset_version
    }

    /// Returns the fingerprint of the immutable original dataset.
    #[must_use]
    pub fn origin_fingerprint(&self) -> &str {
        &self.origin_fingerprint
    }

    /// Returns the metric names in deterministic order.
    #[must_use]
    pub fn metric_ids(&self) -> Vec<MetricId> {
        self.metrics.keys().cloned().collect()
    }

    /// Returns the token a view must render.
    #[must_use]
    pub fn token(&self, id: &MetricId) -> Option<&VerifiedMetricToken> {
        self.tokens.get(id)
    }

    /// Returns why a metric could not be computed, if it currently fails.
    #[must_use]
    pub fn metric_error(&self, id: &MetricId) -> Option<&SafiError> {
        self.errors.get(id)
    }

    /// Returns metrics recomputed by the most recent accepted change.
    #[must_use]
    pub fn last_recomputed(&self) -> &[MetricId] {
        &self.last_recomputed
    }

    /// Returns the rows that fed the most recent recomputation.
    #[must_use]
    pub fn last_rows(&self) -> &[u64] {
        &self.last_rows
    }

    /// Returns the columns changed by the most recent accepted change.
    #[must_use]
    pub fn last_columns(&self) -> &[String] {
        &self.last_columns
    }

    /// Returns the calculated columns in creation order.
    #[must_use]
    pub fn calculated_columns(&self) -> Vec<String> {
        self.calculated
            .iter()
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Returns the recorded transformations.
    #[must_use]
    pub fn transformations(&self) -> &[Transformation] {
        &self.transforms
    }

    /// Returns the defined parameters with their defaults.
    #[must_use]
    pub fn parameters(&self) -> &BTreeMap<String, Decimal> {
        &self.defaults
    }

    /// Returns all scenarios.
    #[must_use]
    pub fn scenarios(&self) -> &BTreeMap<ScenarioId, Scenario> {
        &self.scenarios
    }

    /// Returns the active scenario, if any.
    #[must_use]
    pub fn active_scenario(&self) -> Option<&ScenarioId> {
        self.active_scenario.as_ref()
    }

    /// Replays the recorded transformations from the immutable original dataset.
    ///
    /// # Errors
    /// Returns the first `SF-CAL-*` error raised by a transformation.
    pub fn replay(&self) -> Result<Table, SafiError> {
        Ok(self.rebuild_provenance_with(&self.origin)?.table)
    }

    /// Returns the dataset unchanged for `count` reversible steps. Used by tests to assert that
    /// undo/redo never touches the source.
    #[must_use]
    pub fn origin_unchanged(&self) -> bool {
        table_fingerprint(&self.origin) == self.origin_fingerprint
    }

    /// Applies a cell edit at display row index and recomputes only dependent metrics.
    ///
    /// Records the edit against the row's stable [`RowId`] so later sorts or row operations keep
    /// pointing to the same logical row.
    ///
    /// # Errors
    /// `SF-CAL-001` for an unknown column, `SF-CAL-004` for a formula-owned column, `SF-SYS-001`
    /// for out-of-range rows.
    pub fn set_cell(
        &mut self,
        row: usize,
        column: &str,
        raw: &str,
        now_ms: u64,
    ) -> Result<ChangeOutcome, SafiError> {
        let row_id = self.row_id_at(row).ok_or_else(invalid_request)?;
        self.set_cell_by_id(row_id, column, raw, now_ms)
    }

    /// Applies a cell edit targeting a stable [`RowId`] and recomputes only dependent metrics.
    ///
    /// # Errors
    /// `SF-CAL-001` for an unknown column, `SF-CAL-004` for a formula-owned column, `SF-SYS-001`
    /// when `row_id` does not exist in the working table.
    pub fn set_cell_by_id(
        &mut self,
        row_id: RowId,
        column: &str,
        raw: &str,
        now_ms: u64,
    ) -> Result<ChangeOutcome, SafiError> {
        if self
            .calculated
            .iter()
            .any(|(existing, _)| existing == column)
        {
            return Err(type_mismatch());
        }
        let index = self
            .working
            .columns()
            .iter()
            .position(|existing| existing.schema().name == column)
            .ok_or_else(unknown_column)?;
        let row = self
            .working_row_for_id(row_id)
            .ok_or_else(invalid_request)?;
        let working = replace_cell(&self.working, row, index, CellValue::from_raw(raw))?;
        self.working = working;
        self.transforms.push(Transformation::SetCellByRowId {
            row_id,
            column: column.to_owned(),
            raw: raw.to_owned(),
        });
        self.redo.clear();
        self.revision = self.revision.next();
        let recomputed = self.propagate(&BTreeSet::from([column.to_owned()]), now_ms)?;
        self.last_rows = vec![row_id.get()];
        Ok(ChangeOutcome {
            revision: self.revision,
            recomputed,
            rows: vec![row_id.get()],
            columns: vec![column.to_owned()],
        })
    }

    /// Sorts the working table by `column` while preserving every row's stable [`RowId`] and
    /// origin provenance.
    ///
    /// # Errors
    /// `SF-CAL-001` when `column` does not exist.
    pub fn sort_rows(
        &mut self,
        column: &str,
        ascending: bool,
        now_ms: u64,
    ) -> Result<ChangeOutcome, SafiError> {
        if self.working.column_by_name(column).is_none() {
            return Err(unknown_column());
        }
        let transformation = Transformation::SortRows {
            column: column.to_owned(),
            ascending,
        };
        let mut state = MutableGraphState {
            table: self.working.clone(),
            row_provenance: self.row_provenance.clone(),
            column_provenance: self.column_provenance.clone(),
            active_calculated: self.calculated.clone(),
            next_column_id: self.next_column_id,
        };
        apply_transformation_step(&mut state, &transformation)?;
        self.working = state.table;
        self.row_provenance = state.row_provenance;
        self.transforms.push(transformation);
        self.redo.clear();
        self.revision = self.revision.next();
        self.refresh_all(now_ms);
        Ok(ChangeOutcome {
            revision: self.revision,
            recomputed: self.last_recomputed.clone(),
            rows: self.last_rows.clone(),
            columns: vec![column.to_owned()],
        })
    }

    /// Filters working rows by case-insensitive substring on `column`, preserving stable [`RowId`]s.
    ///
    /// # Errors
    /// `SF-CAL-001` when `column` does not exist.
    pub fn filter_rows(
        &mut self,
        column: &str,
        query: &str,
        now_ms: u64,
    ) -> Result<ChangeOutcome, SafiError> {
        if self.working.column_by_name(column).is_none() {
            return Err(unknown_column());
        }
        let transformation = Transformation::FilterRows {
            column: column.to_owned(),
            query: query.to_owned(),
        };
        let mut state = MutableGraphState {
            table: self.working.clone(),
            row_provenance: self.row_provenance.clone(),
            column_provenance: self.column_provenance.clone(),
            active_calculated: self.calculated.clone(),
            next_column_id: self.next_column_id,
        };
        apply_transformation_step(&mut state, &transformation)?;
        self.working = state.table;
        self.row_provenance = state.row_provenance;
        self.transforms.push(transformation);
        self.redo.clear();
        self.revision = self.revision.next();
        self.refresh_all(now_ms);
        Ok(ChangeOutcome {
            revision: self.revision,
            recomputed: self.last_recomputed.clone(),
            rows: self.last_rows.clone(),
            columns: vec![column.to_owned()],
        })
    }

    /// Deletes a working row by stable [`RowId`]. Deleted IDs are never reassigned.
    ///
    /// # Errors
    /// `SF-SYS-001` when `row_id` does not exist or deletion would leave zero columns invalid.
    pub fn delete_row(&mut self, row_id: RowId, now_ms: u64) -> Result<ChangeOutcome, SafiError> {
        if self.working_row_for_id(row_id).is_none() {
            return Err(invalid_request());
        }
        let transformation = Transformation::DeleteRow { row_id };
        let mut state = MutableGraphState {
            table: self.working.clone(),
            row_provenance: self.row_provenance.clone(),
            column_provenance: self.column_provenance.clone(),
            active_calculated: self.calculated.clone(),
            next_column_id: self.next_column_id,
        };
        apply_transformation_step(&mut state, &transformation)?;
        self.working = state.table;
        self.row_provenance = state.row_provenance;
        self.transforms.push(transformation);
        self.redo.clear();
        self.revision = self.revision.next();
        self.refresh_all(now_ms);
        Ok(ChangeOutcome {
            revision: self.revision,
            recomputed: self.last_recomputed.clone(),
            rows: vec![row_id.get()],
            columns: self.last_columns.clone(),
        })
    }

    /// Inserts a new row with a freshly allocated monotonic [`RowId`] and recomputes derived state.
    ///
    /// # Errors
    /// `SF-CAL-001` if `values` references an unknown column, `SF-SYS-001` when `after_row_id`
    /// does not exist.
    pub fn insert_row(
        &mut self,
        after_row_id: Option<RowId>,
        values: BTreeMap<String, String>,
        now_ms: u64,
    ) -> Result<(RowId, ChangeOutcome), SafiError> {
        for column in values.keys() {
            if self.working.column_by_name(column).is_none() {
                return Err(unknown_column());
            }
        }
        let row_id = RowId::from_raw(self.next_row_id);
        let transformation = Transformation::InsertRow {
            row_id,
            after_row_id,
            values,
        };
        let mut state = MutableGraphState {
            table: self.working.clone(),
            row_provenance: self.row_provenance.clone(),
            column_provenance: self.column_provenance.clone(),
            active_calculated: self.calculated.clone(),
            next_column_id: self.next_column_id,
        };
        apply_transformation_step(&mut state, &transformation)?;
        self.next_row_id = self.next_row_id.saturating_add(1);
        self.working = state.table;
        self.row_provenance = state.row_provenance;
        self.transforms.push(transformation);
        self.redo.clear();
        self.revision = self.revision.next();
        self.last_rows = vec![row_id.get()];
        self.refresh_all(now_ms);
        let outcome = ChangeOutcome {
            revision: self.revision,
            recomputed: self.last_recomputed.clone(),
            rows: vec![row_id.get()],
            columns: self.last_columns.clone(),
        };
        Ok((row_id, outcome))
    }

    /// Applies an explicit, user-confirmed [`AnomalyFix`] atomically as a single undoable step.
    ///
    /// On any invalid column, empty target list, or missing `RowId`, returns an error and leaves
    /// the working table untouched.
    ///
    /// # Errors
    /// `SF-CAL-002` when `row_ids` is empty, `SF-CAL-001` when `column` does not exist, or
    /// `SF-SYS-001` when a target `RowId` is missing.
    pub fn apply_anomaly_fix(
        &mut self,
        fix: AnomalyFix,
        now_ms: u64,
    ) -> Result<ChangeOutcome, SafiError> {
        let affected_columns: Vec<String> = match &fix {
            AnomalyFix::FillMissing { column, .. }
            | AnomalyFix::BlankInvalidDates { column, .. } => vec![column.clone()],
            AnomalyFix::DropDuplicateRows { .. } => Vec::new(),
        };
        let affected_rows: Vec<u64> = match &fix {
            AnomalyFix::FillMissing { row_ids, .. }
            | AnomalyFix::BlankInvalidDates { row_ids, .. }
            | AnomalyFix::DropDuplicateRows { row_ids } => row_ids.clone(),
        };
        let transformation = Transformation::AnomalyFixBatch(fix);
        let mut state = MutableGraphState {
            table: self.working.clone(),
            row_provenance: self.row_provenance.clone(),
            column_provenance: self.column_provenance.clone(),
            active_calculated: self.calculated.clone(),
            next_column_id: self.next_column_id,
        };
        apply_transformation_step(&mut state, &transformation)?;
        self.working = state.table;
        self.row_provenance = state.row_provenance;
        self.transforms.push(transformation);
        self.redo.clear();
        self.revision = self.revision.next();
        self.last_rows = affected_rows.clone();
        self.refresh_all(now_ms);
        Ok(ChangeOutcome {
            revision: self.revision,
            recomputed: self.last_recomputed.clone(),
            rows: affected_rows,
            columns: affected_columns,
        })
    }

    /// Adds a new editable source column with a monotonic [`ColumnId`].
    ///
    /// # Errors
    /// `SF-SYS-001` when `name` is empty or collides with an existing column.
    pub fn add_source_column(
        &mut self,
        name: &str,
        default_value: &str,
        now_ms: u64,
    ) -> Result<(ColumnId, ChangeOutcome), SafiError> {
        let column_id = ColumnId::from_raw(self.next_column_id);
        let outcome = self.replay_add_source_column(column_id, name, default_value, now_ms)?;
        Ok((column_id, outcome))
    }

    fn replay_add_source_column(
        &mut self,
        column_id: ColumnId,
        name: &str,
        default_value: &str,
        now_ms: u64,
    ) -> Result<ChangeOutcome, SafiError> {
        let trimmed = name.trim();
        let transformation = Transformation::AddSourceColumn {
            column_id,
            name: trimmed.to_owned(),
            default_value: default_value.to_owned(),
        };
        let mut state = MutableGraphState {
            table: self.working.clone(),
            row_provenance: self.row_provenance.clone(),
            column_provenance: self.column_provenance.clone(),
            active_calculated: self.calculated.clone(),
            next_column_id: self.next_column_id,
        };
        apply_transformation_step(&mut state, &transformation)?;
        self.working = state.table;
        self.column_provenance = state.column_provenance;
        self.next_column_id = self.next_column_id.max(column_id.get().saturating_add(1));
        self.transforms.push(transformation);
        self.redo.clear();
        self.revision = self.revision.next();
        self.refresh_all(now_ms);
        Ok(ChangeOutcome {
            revision: self.revision,
            recomputed: self.last_recomputed.clone(),
            rows: Vec::new(),
            columns: vec![trimmed.to_owned()],
        })
    }

    /// Renames a working column while preserving its stable [`ColumnId`].
    ///
    /// # Errors
    /// `SF-CAL-001` when `old_name` does not exist or `SF-SYS-001` when `new_name` is invalid.
    pub fn rename_column(
        &mut self,
        old_name: &str,
        new_name: &str,
        now_ms: u64,
    ) -> Result<ChangeOutcome, SafiError> {
        let trimmed = new_name.trim();
        let transformation = Transformation::RenameColumn {
            old_name: old_name.to_owned(),
            new_name: trimmed.to_owned(),
        };
        let mut state = MutableGraphState {
            table: self.working.clone(),
            row_provenance: self.row_provenance.clone(),
            column_provenance: self.column_provenance.clone(),
            active_calculated: self.calculated.clone(),
            next_column_id: self.next_column_id,
        };
        apply_transformation_step(&mut state, &transformation)?;
        self.working = state.table;
        self.column_provenance = state.column_provenance;
        self.calculated = state.active_calculated;
        self.transforms.push(transformation);
        self.redo.clear();
        self.revision = self.revision.next();
        self.refresh_all(now_ms);
        Ok(ChangeOutcome {
            revision: self.revision,
            recomputed: self.last_recomputed.clone(),
            rows: Vec::new(),
            columns: vec![old_name.to_owned(), trimmed.to_owned()],
        })
    }

    /// Deletes a working column (never deleting the final column; never reusing its [`ColumnId`]).
    ///
    /// # Errors
    /// `SF-CAL-001` if unknown or `SF-CAL-002` when only one column remains.
    pub fn delete_column(&mut self, name: &str, now_ms: u64) -> Result<ChangeOutcome, SafiError> {
        let transformation = Transformation::DeleteColumn {
            name: name.to_owned(),
        };
        let mut state = MutableGraphState {
            table: self.working.clone(),
            row_provenance: self.row_provenance.clone(),
            column_provenance: self.column_provenance.clone(),
            active_calculated: self.calculated.clone(),
            next_column_id: self.next_column_id,
        };
        apply_transformation_step(&mut state, &transformation)?;
        self.working = state.table;
        self.column_provenance = state.column_provenance;
        self.calculated = state.active_calculated;
        self.transforms.push(transformation);
        self.redo.clear();
        self.revision = self.revision.next();
        self.refresh_all(now_ms);
        Ok(ChangeOutcome {
            revision: self.revision,
            recomputed: self.last_recomputed.clone(),
            rows: Vec::new(),
            columns: vec![name.to_owned()],
        })
    }

    /// Trims leading/trailing whitespace and collapses internal spaces across a column atomically.
    ///
    /// # Errors
    /// `SF-CAL-001` when `column` does not exist.
    pub fn trim_column(&mut self, column: &str, now_ms: u64) -> Result<ChangeOutcome, SafiError> {
        let transformation = Transformation::TrimColumn {
            column: column.to_owned(),
        };
        let mut state = MutableGraphState {
            table: self.working.clone(),
            row_provenance: self.row_provenance.clone(),
            column_provenance: self.column_provenance.clone(),
            active_calculated: self.calculated.clone(),
            next_column_id: self.next_column_id,
        };
        apply_transformation_step(&mut state, &transformation)?;
        self.working = state.table;
        self.transforms.push(transformation);
        self.redo.clear();
        self.revision = self.revision.next();
        self.refresh_all(now_ms);
        Ok(ChangeOutcome {
            revision: self.revision,
            recomputed: self.last_recomputed.clone(),
            rows: Vec::new(),
            columns: vec![column.to_owned()],
        })
    }

    /// Appends a calculated column derived per row.
    ///
    /// # Errors
    /// `SF-CAL-002` for invalid syntax and `SF-CAL-001`/`SF-CAL-004` when the formula references
    /// a missing or non-numeric column.
    pub fn add_calculated_column(
        &mut self,
        name: &str,
        formula_source: &str,
        now_ms: u64,
    ) -> Result<ChangeOutcome, SafiError> {
        let name = name.trim();
        if name.is_empty()
            || self
                .working
                .columns()
                .iter()
                .any(|column| column.schema().name == name)
        {
            return Err(invalid_request());
        }
        let formula = parse_formula(formula_source)?;
        if has_aggregate(&formula) {
            // A calculated column is row-level; aggregation belongs to metrics.
            return Err(type_mismatch());
        }
        let mut columns = BTreeSet::new();
        let mut references = BTreeSet::new();
        dependencies(&formula, FormulaScope::Cell, &mut columns, &mut references);
        for column in &columns {
            if self.working.column_by_name(column).is_none() {
                return Err(unknown_column());
            }
        }
        let cells = evaluate_column(&self.working, &formula)?;
        self.working = append_column(&self.working, name, cells)?;
        let column_id = ColumnId::from_raw(self.next_column_id);
        self.next_column_id = self.next_column_id.saturating_add(1);
        self.column_provenance.push(ColumnProvenance {
            column_id,
            origin_column_index: None,
            origin_name: Some(name.to_owned()),
            current_name: name.to_owned(),
        });
        self.calculated.push((name.to_owned(), formula));
        self.transforms.push(Transformation::AddColumn {
            name: name.to_owned(),
            formula: formula_source.to_owned(),
        });
        self.redo.clear();
        self.revision = self.revision.next();
        let recomputed = self.propagate(&BTreeSet::from([name.to_owned()]), now_ms)?;
        Ok(ChangeOutcome {
            revision: self.revision,
            recomputed,
            rows: Vec::new(),
            columns: vec![name.to_owned()],
        })
    }

    /// Defines a metric and computes its first token.
    ///
    /// # Errors
    /// `SF-CAL-001` for an unknown column, `SF-CAL-002` for invalid syntax, `SF-CAL-005` when a
    /// parameter has no default value.
    pub fn define_metric(
        &mut self,
        id: MetricId,
        name: &str,
        unit: Option<&str>,
        formula_source: &str,
        now_ms: u64,
    ) -> Result<ChangeOutcome, SafiError> {
        let formula = parse_formula(formula_source)?;
        let mut columns = BTreeSet::new();
        let mut parameters = BTreeSet::new();
        dependencies(
            &formula,
            FormulaScope::Metric,
            &mut columns,
            &mut parameters,
        );
        for column in &columns {
            if self.working.column_by_name(column).is_none() {
                return Err(unknown_column());
            }
        }
        let definition = MetricDefinition {
            id: id.clone(),
            name: name.to_owned(),
            unit: unit.map(str::to_owned),
            source: formula_source.to_owned(),
            formula,
            version: 1,
            columns,
            parameters,
        };
        self.metrics.insert(id.clone(), definition);
        self.revision = self.revision.next();
        self.refresh_metric(&id, now_ms);
        self.last_recomputed = vec![id.clone()];
        Ok(ChangeOutcome {
            revision: self.revision,
            recomputed: vec![id],
            rows: self.last_rows.clone(),
            columns: self.last_columns.clone(),
        })
    }

    /// Replaces a metric formula, bumping its formula version and invalidating older tokens.
    ///
    /// # Errors
    /// `SF-CAL-006` for an unknown metric, plus the validation errors of [`Self::define_metric`].
    pub fn update_metric_formula(
        &mut self,
        id: &MetricId,
        formula_source: &str,
        now_ms: u64,
    ) -> Result<ChangeOutcome, SafiError> {
        let formula = parse_formula(formula_source)?;
        let mut columns = BTreeSet::new();
        let mut parameters = BTreeSet::new();
        dependencies(
            &formula,
            FormulaScope::Metric,
            &mut columns,
            &mut parameters,
        );
        for column in &columns {
            if self.working.column_by_name(column).is_none() {
                return Err(unknown_column());
            }
        }
        let definition = self.metrics.get_mut(id).ok_or_else(unknown_metric)?;
        definition.formula = formula;
        definition.source = formula_source.to_owned();
        definition.version = definition.version.saturating_add(1);
        definition.columns = columns;
        definition.parameters = parameters;
        self.revision = self.revision.next();
        self.refresh_metric(id, now_ms);
        self.last_recomputed = vec![id.clone()];
        Ok(ChangeOutcome {
            revision: self.revision,
            recomputed: vec![id.clone()],
            rows: self.last_rows.clone(),
            columns: self.last_columns.clone(),
        })
    }

    /// Removes a metric.
    ///
    /// # Errors
    /// `SF-CAL-006` when the metric does not exist.
    pub fn remove_metric(&mut self, id: &MetricId) -> Result<(), SafiError> {
        if self.metrics.remove(id).is_none() {
            return Err(unknown_metric());
        }
        let _ = self.tokens.remove(id);
        let _ = self.errors.remove(id);
        self.revision = self.revision.next();
        Ok(())
    }

    /// Registers a What-If parameter with a default value.
    pub fn define_parameter(
        &mut self,
        name: &str,
        default: Decimal,
        now_ms: u64,
    ) -> Result<(), SafiError> {
        if name.trim().is_empty() {
            return Err(invalid_request());
        }
        let _ = self.defaults.insert(name.to_owned(), default);
        self.revision = self.revision.next();
        self.refresh_all(now_ms);
        Ok(())
    }

    /// Adds a scenario whose parameters override the defaults.
    ///
    /// # Errors
    /// `SF-SYS-001` when the identifier is already used.
    pub fn add_scenario(
        &mut self,
        id: ScenarioId,
        name: &str,
        parameters: BTreeMap<String, Decimal>,
    ) -> Result<(), SafiError> {
        if self.scenarios.contains_key(&id) || name.trim().is_empty() {
            return Err(invalid_request());
        }
        let _ = self.scenarios.insert(
            id.clone(),
            Scenario {
                id,
                name: name.trim().to_owned(),
                parameters,
            },
        );
        self.revision = self.revision.next();
        Ok(())
    }

    /// Renames an existing What-If scenario without altering its parameters.
    ///
    /// # Errors
    /// `SF-SYS-001` when the scenario does not exist or `new_name` is empty.
    pub fn rename_scenario(&mut self, id: &ScenarioId, new_name: &str) -> Result<(), SafiError> {
        let trimmed = new_name.trim();
        if trimmed.is_empty() {
            return Err(invalid_request());
        }
        let scenario = self.scenarios.get_mut(id).ok_or_else(invalid_request)?;
        scenario.name = trimmed.to_owned();
        self.revision = self.revision.next();
        Ok(())
    }

    /// Updates a numeric What-If parameter override inside an existing scenario and recomputes
    /// dependent metrics when that scenario is active.
    ///
    /// # Errors
    /// `SF-SYS-001` when `id` does not exist, `SF-CAL-005` when `parameter` has no registered
    /// default assumption.
    pub fn set_scenario_parameter(
        &mut self,
        id: &ScenarioId,
        parameter: &str,
        value: Decimal,
        now_ms: u64,
    ) -> Result<ChangeOutcome, SafiError> {
        if !self.defaults.contains_key(parameter) {
            return Err(unknown_parameter());
        }
        let scenario = self.scenarios.get_mut(id).ok_or_else(invalid_request)?;
        let _ = scenario.parameters.insert(parameter.to_owned(), value);
        self.revision = self.revision.next();
        let is_active = self.active_scenario.as_ref() == Some(id);
        let recomputed: Vec<MetricId> = self
            .metrics
            .values()
            .filter(|def| def.parameters.contains(parameter))
            .map(|def| def.id.clone())
            .collect();
        if is_active {
            for metric_id in &recomputed {
                self.refresh_metric(metric_id, now_ms);
            }
            self.last_recomputed = recomputed.clone();
            self.collect_provenance();
        }
        Ok(ChangeOutcome {
            revision: self.revision,
            recomputed,
            rows: self.last_rows.clone(),
            columns: self.last_columns.clone(),
        })
    }

    /// Updates a baseline assumption parameter and recomputes dependent metrics when active.
    ///
    /// # Errors
    /// `SF-CAL-005` when `parameter` is not registered.
    pub fn update_parameter(
        &mut self,
        parameter: &str,
        value: Decimal,
        now_ms: u64,
    ) -> Result<ChangeOutcome, SafiError> {
        if !self.defaults.contains_key(parameter) {
            return Err(unknown_parameter());
        }
        let _ = self.defaults.insert(parameter.to_owned(), value);
        self.revision = self.revision.next();
        let recomputed: Vec<MetricId> = self
            .metrics
            .values()
            .filter(|def| def.parameters.contains(parameter))
            .map(|def| def.id.clone())
            .collect();
        for metric_id in &recomputed {
            self.refresh_metric(metric_id, now_ms);
        }
        self.last_recomputed = recomputed.clone();
        self.collect_provenance();
        Ok(ChangeOutcome {
            revision: self.revision,
            recomputed,
            rows: self.last_rows.clone(),
            columns: self.last_columns.clone(),
        })
    }

    /// Activates a scenario (or clears it with `None`) and recomputes every metric.
    ///
    /// # Errors
    /// `SF-SYS-001` when the scenario does not exist.
    pub fn activate_scenario(
        &mut self,
        id: Option<&ScenarioId>,
        now_ms: u64,
    ) -> Result<(), SafiError> {
        if let Some(id) = id
            && !self.scenarios.contains_key(id)
        {
            return Err(invalid_request());
        }
        self.active_scenario = id.cloned();
        self.revision = self.revision.next();
        self.refresh_all(now_ms);
        Ok(())
    }

    /// Returns to the original assumptions by clearing the active scenario.
    pub fn reset_scenario(&mut self, now_ms: u64) -> Result<(), SafiError> {
        self.activate_scenario(None, now_ms)
    }

    /// Compares one metric under two scenarios without mutating the active state.
    ///
    /// # Errors
    /// `SF-SYS-001` for an unknown scenario and the evaluation errors of the metric.
    pub fn compare_scenarios(
        &self,
        left: &ScenarioId,
        right: &ScenarioId,
        now_ms: u64,
    ) -> Result<Vec<ScenarioComparison>, SafiError> {
        let left_parameters = self.parameters_for(Some(left))?;
        let right_parameters = self.parameters_for(Some(right))?;
        let mut comparisons = Vec::new();
        for definition in self.metrics.values() {
            let left_token = self.compute_metric(definition, &left_parameters, now_ms)?;
            let right_token = self.compute_metric(definition, &right_parameters, now_ms)?;
            comparisons.push(ScenarioComparison {
                metric_id: definition.id.clone(),
                metric_name: definition.name.clone(),
                left: metric_decimal(&left_token)?,
                right: metric_decimal(&right_token)?,
            });
        }
        Ok(comparisons)
    }

    /// Recomputes a metric and compares it with the token supplied by a view.
    ///
    /// Also validates that every provenance row/column identity referenced by the token exists in
    /// the current working graph state.
    ///
    /// # Errors
    /// `SF-CAL-006` for an unknown metric, plus evaluation errors.
    pub fn verify(&self, token: &VerifiedMetricToken) -> Result<VerificationOutcome, SafiError> {
        let definition = self
            .metrics
            .get(&token.metric_id)
            .ok_or_else(unknown_metric)?;
        if token.source_row_ids.len() != token.row_refs.len() {
            return Ok(VerificationOutcome::Mismatch {
                expected: token.fingerprint().to_owned(),
                actual: "missing-row-provenance".to_owned(),
            });
        }
        for row_id in &token.source_row_ids {
            if self.working_row_for_id(*row_id).is_none() {
                return Ok(VerificationOutcome::Mismatch {
                    expected: token.fingerprint().to_owned(),
                    actual: "stale-row-provenance".to_owned(),
                });
            }
        }
        if !definition.columns.is_empty() && token.column_ids.is_empty() {
            return Ok(VerificationOutcome::Mismatch {
                expected: token.fingerprint().to_owned(),
                actual: "missing-column-provenance".to_owned(),
            });
        }
        for column_id in &token.column_ids {
            if !self
                .column_provenance
                .iter()
                .any(|entry| entry.column_id == *column_id)
            {
                return Ok(VerificationOutcome::Mismatch {
                    expected: token.fingerprint().to_owned(),
                    actual: "stale-column-provenance".to_owned(),
                });
            }
        }
        let parameters = self.parameters_for(self.active_scenario.as_ref())?;
        let recomputed = self.compute_metric(definition, &parameters, token.computed_at_ms)?;
        Ok(token.verify_against(&recomputed))
    }

    /// Like [`Self::verify`] but fails with `SF-CAL-008` on any mismatch.
    ///
    /// # Errors
    /// `SF-CAL-008` when the displayed number no longer matches the computed one.
    pub fn verify_strict(&self, token: &VerifiedMetricToken) -> Result<(), SafiError> {
        match self.verify(token)? {
            VerificationOutcome::Verified => Ok(()),
            VerificationOutcome::Mismatch { .. } => Err(verification_mismatch()),
        }
    }

    /// Returns the public metric definitions.
    #[must_use]
    pub fn metric_specs(&self) -> Vec<MetricSpec> {
        self.metrics
            .values()
            .map(|definition| MetricSpec {
                id: definition.id.clone(),
                name: definition.name.clone(),
                unit: definition.unit.clone(),
                formula: definition.source.clone(),
                formula_version: definition.version,
            })
            .collect()
    }

    /// Captures the whole project for local persistence.
    #[must_use]
    pub fn to_snapshot(&self) -> ProjectSnapshot {
        ProjectSnapshot {
            schema_version: PROJECT_SCHEMA_VERSION,
            source_id: self.source_id.clone(),
            origin: self.origin.clone(),
            origin_fingerprint: self.origin_fingerprint.clone(),
            dataset_version: self.dataset_version.get(),
            revision: self.revision.get(),
            transformations: self.transforms.clone(),
            metrics: self.metric_specs(),
            parameters: self.defaults.clone(),
            scenarios: self.scenarios.values().cloned().collect(),
            active_scenario: self.active_scenario.clone(),
            row_provenance: self.row_provenance.clone(),
            column_provenance: self.column_provenance.clone(),
            next_row_id: self.next_row_id,
            next_column_id: self.next_column_id,
        }
    }

    /// Serializes the project as JSON.
    ///
    /// # Errors
    /// `SF-SYS-001` when serialization fails.
    pub fn to_json(&self) -> Result<String, SafiError> {
        serde_json::to_string(&self.to_snapshot()).map_err(|_| invalid_request())
    }

    /// Restores a project and re-verifies it against its own data and provenance.
    ///
    /// # Errors
    /// `SF-SYS-001` for a malformed or unsupported file and `SF-CAL-008` when the stored origin
    /// fingerprint or stored provenance does not match the replayed dataset state.
    pub fn from_json(json: &str, now_ms: u64) -> Result<Self, SafiError> {
        let snapshot: ProjectSnapshot =
            serde_json::from_str(json).map_err(|_| invalid_request())?;
        if snapshot.schema_version != PROJECT_SCHEMA_VERSION
            && snapshot.schema_version != LEGACY_PROJECT_SCHEMA_VERSION
        {
            return Err(invalid_request());
        }
        let mut graph = Self::new(snapshot.source_id.clone(), snapshot.origin.clone(), now_ms)?;
        if graph.origin_fingerprint != snapshot.origin_fingerprint {
            return Err(verification_mismatch());
        }
        for transformation in &snapshot.transformations {
            match transformation {
                Transformation::AddColumn { name, formula } => {
                    graph.add_calculated_column(name, formula, now_ms)?;
                }
                Transformation::SetCell { row, column, raw } => {
                    graph.set_cell(*row, column, raw, now_ms)?;
                }
                Transformation::SetCellByRowId {
                    row_id,
                    column,
                    raw,
                } => {
                    graph.set_cell_by_id(*row_id, column, raw, now_ms)?;
                }
                Transformation::SortRows { column, ascending } => {
                    graph.sort_rows(column, *ascending, now_ms)?;
                }
                Transformation::FilterRows { column, query } => {
                    graph.filter_rows(column, query, now_ms)?;
                }
                Transformation::DeleteRow { row_id } => {
                    graph.delete_row(*row_id, now_ms)?;
                }
                Transformation::InsertRow {
                    row_id,
                    after_row_id,
                    values,
                } => {
                    graph.replay_insert_row(*row_id, *after_row_id, values.clone(), now_ms)?;
                }
                Transformation::AnomalyFixBatch(fix) => {
                    graph.apply_anomaly_fix(fix.clone(), now_ms)?;
                }
                Transformation::AddSourceColumn {
                    column_id,
                    name,
                    default_value,
                } => {
                    graph.replay_add_source_column(*column_id, name, default_value, now_ms)?;
                }
                Transformation::RenameColumn { old_name, new_name } => {
                    graph.rename_column(old_name, new_name, now_ms)?;
                }
                Transformation::DeleteColumn { name } => {
                    graph.delete_column(name, now_ms)?;
                }
                Transformation::TrimColumn { column } => {
                    graph.trim_column(column, now_ms)?;
                }
            }
        }
        if snapshot.schema_version == PROJECT_SCHEMA_VERSION {
            if snapshot.row_provenance.is_empty() && graph.working.row_count() > 0 {
                return Err(verification_mismatch());
            }
            if snapshot.row_provenance != graph.row_provenance
                || snapshot.column_provenance != graph.column_provenance
            {
                return Err(verification_mismatch());
            }
            let unique_rows: BTreeSet<RowId> =
                snapshot.row_provenance.iter().map(|r| r.row_id).collect();
            if unique_rows.len() != snapshot.row_provenance.len() {
                return Err(verification_mismatch());
            }
            graph.next_row_id = graph.next_row_id.max(snapshot.next_row_id);
            graph.next_column_id = graph.next_column_id.max(snapshot.next_column_id);
        }
        for (name, value) in &snapshot.parameters {
            graph.define_parameter(name, *value, now_ms)?;
        }
        for scenario in snapshot.scenarios {
            graph.add_scenario(
                scenario.id().clone(),
                scenario.name(),
                scenario.parameters().clone(),
            )?;
        }
        for spec in &snapshot.metrics {
            graph.define_metric(
                spec.id.clone(),
                &spec.name,
                spec.unit.as_deref(),
                &spec.formula,
                now_ms,
            )?;
        }
        if let Some(active) = snapshot.active_scenario {
            graph.activate_scenario(Some(&active), now_ms)?;
        }
        graph.dataset_version = DatasetVersion::from_raw(snapshot.dataset_version);
        graph.revision = Revision::from_raw(snapshot.revision);
        graph.refresh_all(now_ms);
        Ok(graph)
    }

    fn replay_insert_row(
        &mut self,
        row_id: RowId,
        after_row_id: Option<RowId>,
        values: BTreeMap<String, String>,
        now_ms: u64,
    ) -> Result<(), SafiError> {
        if row_id.get() < self.next_row_id || self.working_row_for_id(row_id).is_some() {
            return Err(verification_mismatch());
        }
        let transformation = Transformation::InsertRow {
            row_id,
            after_row_id,
            values,
        };
        let mut state = MutableGraphState {
            table: self.working.clone(),
            row_provenance: self.row_provenance.clone(),
            column_provenance: self.column_provenance.clone(),
            active_calculated: self.calculated.clone(),
            next_column_id: self.next_column_id,
        };
        apply_transformation_step(&mut state, &transformation)?;
        self.next_row_id = row_id.get().saturating_add(1);
        self.working = state.table;
        self.row_provenance = state.row_provenance;
        self.transforms.push(transformation);
        self.revision = self.revision.next();
        self.refresh_all(now_ms);
        Ok(())
    }

    /// Recomputes a single metric token.
    ///
    /// # Errors
    /// `SF-CAL-006` for an unknown metric.
    pub fn refresh(&mut self, id: &MetricId, now_ms: u64) -> Result<(), SafiError> {
        if !self.metrics.contains_key(id) {
            return Err(unknown_metric());
        }
        self.refresh_metric(id, now_ms);
        Ok(())
    }

    /// Replaces the imported dataset, bumping its version so every older token is invalidated.
    ///
    /// # Errors
    /// Fails without mutating the graph when the recorded transformations cannot be replayed on
    /// the new dataset (for example, a removed column).
    pub fn replace_source(
        &mut self,
        origin: Table,
        now_ms: u64,
    ) -> Result<ChangeOutcome, SafiError> {
        unique_columns(&origin)?;
        let replayed = self.rebuild_provenance_with(&origin)?;
        self.origin = origin;
        self.origin_fingerprint = table_fingerprint(&self.origin);
        self.working = replayed.table;
        self.row_provenance = replayed.row_provenance;
        self.column_provenance = replayed.column_provenance;
        self.next_row_id = self.next_row_id.max(replayed.max_row_id);
        self.next_column_id = self.next_column_id.max(replayed.next_column_id);
        self.dataset_version = self.dataset_version.next();
        self.revision = self.revision.next();
        self.refresh_all(now_ms);
        let recomputed = self.last_recomputed.clone();
        Ok(ChangeOutcome {
            revision: self.revision,
            recomputed,
            rows: self.last_rows.clone(),
            columns: self.last_columns.clone(),
        })
    }

    /// Undoes the last transformation and recomputes every metric.
    ///
    /// # Errors
    /// `SF-CAL-007` when there is nothing to undo.
    pub fn undo(&mut self, now_ms: u64) -> Result<ChangeOutcome, SafiError> {
        let Some(transformation) = self.transforms.pop() else {
            return Err(stale_command());
        };
        if let Transformation::AddColumn { name, .. } = &transformation
            && let Some(position) = self
                .calculated
                .iter()
                .position(|(existing, _)| existing == name)
        {
            let _ = self.calculated.remove(position);
        }
        self.redo.push(transformation);
        self.rebuild_state()?;
        self.revision = self.revision.next();
        self.refresh_all(now_ms);
        Ok(ChangeOutcome {
            revision: self.revision,
            recomputed: self.last_recomputed.clone(),
            rows: Vec::new(),
            columns: self.last_columns.clone(),
        })
    }

    /// Re-applies the last undone transformation.
    ///
    /// # Errors
    /// `SF-CAL-007` when there is nothing to redo and `SF-CAL-*` when replaying fails.
    pub fn redo(&mut self, now_ms: u64) -> Result<ChangeOutcome, SafiError> {
        let Some(transformation) = self.redo.pop() else {
            return Err(stale_command());
        };
        let mut state = MutableGraphState {
            table: self.working.clone(),
            row_provenance: self.row_provenance.clone(),
            column_provenance: self.column_provenance.clone(),
            active_calculated: self.calculated.clone(),
            next_column_id: self.next_column_id,
        };
        apply_transformation_step(&mut state, &transformation)?;
        self.working = state.table;
        self.row_provenance = state.row_provenance;
        self.column_provenance = state.column_provenance;
        self.calculated = state.active_calculated;
        self.next_column_id = self.next_column_id.max(state.next_column_id);
        if let Transformation::InsertRow { row_id, .. } = &transformation {
            self.next_row_id = self.next_row_id.max(row_id.get().saturating_add(1));
        }
        self.transforms.push(transformation);
        self.revision = self.revision.next();
        self.refresh_all(now_ms);
        Ok(ChangeOutcome {
            revision: self.revision,
            recomputed: self.last_recomputed.clone(),
            rows: Vec::new(),
            columns: self.last_columns.clone(),
        })
    }

    /// Groups the working table by one column and aggregates another, all in Rust.
    ///
    /// Dashboards and the exported capsule render these values; they never aggregate by
    /// themselves. Groups appear in first-appearance order, which keeps output deterministic.
    ///
    /// # Errors
    /// `SF-CAL-001` for an unknown column, `SF-CAL-004` for a non-numeric value in a numeric
    /// aggregation.
    pub fn grouped_aggregate(
        &self,
        function: AggFunc,
        column: &str,
        group_by: &str,
    ) -> Result<Vec<GroupedValue>, SafiError> {
        let group_column = self
            .working
            .column_by_name(group_by)
            .ok_or_else(unknown_column)?;
        let value_column = self
            .working
            .column_by_name(column)
            .ok_or_else(unknown_column)?;
        let mut order: Vec<String> = Vec::new();
        let mut buckets: BTreeMap<String, (Decimal, u64, Vec<usize>, Vec<u64>)> = BTreeMap::new();
        for row in 0..self.working.row_count() {
            let label = group_column
                .cell_at(row)
                .map(|cell| cell.visible_text().to_owned())
                .unwrap_or_default();
            let entry = match buckets.get(&label) {
                Some(existing) => existing.clone(),
                None => {
                    order.push(label.clone());
                    (Decimal::ZERO, 0, Vec::new(), Vec::new())
                }
            };
            let (mut sum, mut count, mut indices, mut row_ids) = entry;
            let cell = value_column.cell_at(row).ok_or_else(invalid_request)?;
            if !cell.is_empty() {
                if matches!(function, AggFunc::Count) {
                    count += 1;
                } else {
                    let number = cell_number(cell).ok_or_else(type_mismatch)?;
                    sum = sum.checked_add(number).ok_or_else(type_mismatch)?;
                    count += 1;
                }
                indices.push(row);
                let row_id = self
                    .row_id_at(row)
                    .map(RowId::get)
                    .ok_or_else(verification_mismatch)?;
                row_ids.push(row_id);
            }
            let _ = buckets.insert(label, (sum, count, indices, row_ids));
        }
        let mut groups = Vec::with_capacity(order.len());
        for label in order {
            let (sum, count, indices, row_ids) =
                buckets.remove(&label).ok_or_else(invalid_request)?;
            let value = match function {
                AggFunc::Count => Decimal::from(count),
                AggFunc::Sum => sum,
                AggFunc::Avg => {
                    if count == 0 {
                        Decimal::ZERO
                    } else {
                        sum.checked_div(Decimal::from(count))
                            .ok_or_else(type_mismatch)?
                            .round_dp(AVERAGE_SCALE)
                    }
                }
                AggFunc::Min | AggFunc::Max => {
                    let mut extreme: Option<Decimal> = None;
                    for row in &indices {
                        let cell = value_column.cell_at(*row).ok_or_else(invalid_request)?;
                        let number = cell_number(cell).ok_or_else(type_mismatch)?;
                        extreme = Some(match (function, extreme) {
                            (AggFunc::Min, Some(current)) => current.min(number),
                            (AggFunc::Max, Some(current)) => current.max(number),
                            _ => number,
                        });
                    }
                    extreme.ok_or_else(type_mismatch)?
                }
            };
            groups.push(GroupedValue {
                label,
                value,
                row_refs: row_ids,
            });
        }
        Ok(groups)
    }

    /// Computes a complete, validated dataset for any of the 8 Quad-Studio dashboard chart types
    /// (`bar`, `line`, `area`, `pie`, `donut`, `scatter`, `histogram`, `kpi`) entirely in Rust.
    ///
    /// # Errors
    /// `SF-CAL-001` when a required column does not exist, or `SF-CAL-004` when the selected
    /// columns or cell values are unsuitable for the requested chart type (e.g. non-numeric X/Y
    /// in `scatter`, non-numeric values in `histogram`, or negative values / zero total in `pie`
    /// and `donut`).
    #[allow(clippy::too_many_lines)]
    pub fn compute_chart_series(
        &self,
        chart_type: ChartType,
        function: AggFunc,
        column: &str,
        group_by: &str,
    ) -> Result<ComputedChartSeries, SafiError> {
        match chart_type {
            ChartType::Bar | ChartType::Line | ChartType::Area => {
                let groups = self.grouped_aggregate(function, column, group_by)?;
                if groups.is_empty() {
                    return Err(type_mismatch());
                }
                let max_val = groups
                    .iter()
                    .map(|g| g.value)
                    .fold(Decimal::ZERO, Decimal::max);
                let min_val = groups
                    .iter()
                    .map(|g| g.value)
                    .fold(Decimal::ZERO, Decimal::min);
                let span = max_val.checked_sub(min_val).unwrap_or(Decimal::ZERO);
                let total = groups
                    .iter()
                    .try_fold(Decimal::ZERO, |acc, g| acc.checked_add(g.value))
                    .ok_or_else(type_mismatch)?;
                let count = groups.len();
                let mut points = Vec::with_capacity(count);
                for (idx, group) in groups.into_iter().enumerate() {
                    let ratio_pct = scale_percentage(group.value, max_val);
                    let x_pct = if count <= 1 {
                        50
                    } else {
                        u32::try_from(10 + (idx * 80) / (count - 1)).unwrap_or(50)
                    };
                    let y_pct = scale_vertical_coord(group.value, min_val, span);
                    points.push(ComputedChartPoint {
                        label: group.label,
                        value: group.value.normalize().to_string(),
                        x_value: None,
                        y_value: None,
                        share_pct: None,
                        svg_path: None,
                        ratio_pct,
                        start_pct: 0,
                        x_pct,
                        y_pct,
                        row_refs: group.row_refs,
                    });
                }
                Ok(ComputedChartSeries {
                    chart_type: chart_type.as_str().to_owned(),
                    function: function.as_str().to_owned(),
                    column: column.to_owned(),
                    group_by: group_by.to_owned(),
                    total_or_kpi: total.normalize().to_string(),
                    points,
                })
            }
            ChartType::Column => {
                let groups = self.grouped_aggregate(function, column, group_by)?;
                if groups.is_empty() {
                    return Err(type_mismatch());
                }
                let max_val = groups
                    .iter()
                    .map(|g| g.value)
                    .fold(Decimal::ZERO, Decimal::max);
                let total = groups
                    .iter()
                    .try_fold(Decimal::ZERO, |acc, g| acc.checked_add(g.value))
                    .ok_or_else(type_mismatch)?;
                let count = groups.len();
                let mut points = Vec::with_capacity(count);
                for (idx, group) in groups.into_iter().enumerate() {
                    let ratio_pct = scale_percentage(group.value, max_val);
                    let x_pct = if count <= 1 {
                        50
                    } else {
                        u32::try_from(12 + (idx * 76) / (count - 1)).unwrap_or(50)
                    };
                    let top_y = 90_u32.saturating_sub((ratio_pct * 76) / 100).clamp(14, 90);
                    points.push(ComputedChartPoint {
                        label: group.label,
                        value: group.value.normalize().to_string(),
                        x_value: None,
                        y_value: Some(ratio_pct.to_string()),
                        share_pct: Some(scale_share_str(group.value, total)),
                        svg_path: None,
                        ratio_pct,
                        start_pct: top_y,
                        x_pct,
                        y_pct: top_y,
                        row_refs: group.row_refs,
                    });
                }
                Ok(ComputedChartSeries {
                    chart_type: chart_type.as_str().to_owned(),
                    function: function.as_str().to_owned(),
                    column: column.to_owned(),
                    group_by: group_by.to_owned(),
                    total_or_kpi: total.normalize().to_string(),
                    points,
                })
            }
            ChartType::Stacked => {
                let groups = self.grouped_aggregate(function, column, group_by)?;
                if groups.is_empty() {
                    return Err(type_mismatch());
                }
                let abs_total = groups
                    .iter()
                    .map(|g| g.value.abs())
                    .fold(Decimal::ZERO, |acc, v| acc + v)
                    .max(Decimal::ONE);
                let total = groups
                    .iter()
                    .try_fold(Decimal::ZERO, |acc, g| acc.checked_add(g.value))
                    .ok_or_else(type_mismatch)?;
                let mut cumulative_pct = 0_u32;
                let mut points = Vec::with_capacity(groups.len());
                for (idx, group) in groups.into_iter().enumerate() {
                    let ratio_pct = scale_percentage(group.value.abs(), abs_total);
                    let start_pct = cumulative_pct.min(100);
                    cumulative_pct = cumulative_pct.saturating_add(ratio_pct).min(100);
                    points.push(ComputedChartPoint {
                        label: group.label,
                        value: group.value.normalize().to_string(),
                        x_value: Some(start_pct.to_string()),
                        y_value: Some(cumulative_pct.to_string()),
                        share_pct: Some(scale_share_str(group.value.abs(), abs_total)),
                        svg_path: None,
                        ratio_pct,
                        start_pct,
                        x_pct: u32::try_from(10 + idx * 14).unwrap_or(50).min(90),
                        y_pct: 50,
                        row_refs: group.row_refs,
                    });
                }
                Ok(ComputedChartSeries {
                    chart_type: chart_type.as_str().to_owned(),
                    function: function.as_str().to_owned(),
                    column: column.to_owned(),
                    group_by: group_by.to_owned(),
                    total_or_kpi: total.normalize().to_string(),
                    points,
                })
            }
            ChartType::Combo => {
                let groups = self.grouped_aggregate(function, column, group_by)?;
                if groups.is_empty() {
                    return Err(type_mismatch());
                }
                let max_val = groups
                    .iter()
                    .map(|g| g.value)
                    .fold(Decimal::ZERO, Decimal::max);
                let abs_total = groups
                    .iter()
                    .map(|g| g.value.abs())
                    .fold(Decimal::ZERO, |acc, v| acc + v)
                    .max(Decimal::ONE);
                let total = groups
                    .iter()
                    .try_fold(Decimal::ZERO, |acc, g| acc.checked_add(g.value))
                    .ok_or_else(type_mismatch)?;
                let count = groups.len();
                let mut running_abs = Decimal::ZERO;
                let mut points = Vec::with_capacity(count);
                for (idx, group) in groups.into_iter().enumerate() {
                    let ratio_pct = scale_percentage(group.value, max_val);
                    running_abs += group.value.abs();
                    let cum_pct = scale_percentage(running_abs, abs_total);
                    let x_pct = if count <= 1 {
                        50
                    } else {
                        u32::try_from(14 + (idx * 72) / (count - 1)).unwrap_or(50)
                    };
                    let y_pct = 90_u32.saturating_sub((cum_pct * 76) / 100).clamp(14, 90);
                    points.push(ComputedChartPoint {
                        label: group.label,
                        value: group.value.normalize().to_string(),
                        x_value: Some(cum_pct.to_string()),
                        y_value: Some(running_abs.normalize().to_string()),
                        share_pct: Some(cum_pct.to_string()),
                        svg_path: None,
                        ratio_pct,
                        start_pct: 90_u32.saturating_sub((ratio_pct * 76) / 100).clamp(14, 90),
                        x_pct,
                        y_pct,
                        row_refs: group.row_refs,
                    });
                }
                Ok(ComputedChartSeries {
                    chart_type: chart_type.as_str().to_owned(),
                    function: function.as_str().to_owned(),
                    column: column.to_owned(),
                    group_by: group_by.to_owned(),
                    total_or_kpi: total.normalize().to_string(),
                    points,
                })
            }
            ChartType::Pie | ChartType::Donut => {
                let groups = self.grouped_aggregate(function, column, group_by)?;
                if groups.is_empty() || groups.iter().any(|g| g.value < Decimal::ZERO) {
                    return Err(type_mismatch());
                }
                let total = groups
                    .iter()
                    .try_fold(Decimal::ZERO, |acc, g| acc.checked_add(g.value))
                    .ok_or_else(type_mismatch)?;
                if total <= Decimal::ZERO {
                    return Err(type_mismatch());
                }
                let is_donut = matches!(chart_type, ChartType::Donut);
                let mut cumulative_pct = 0_u32;
                let mut cumulative_frac = 0.0_f64;
                let mut points = Vec::with_capacity(groups.len());
                for (idx, group) in groups.into_iter().enumerate() {
                    let frac_dec = group.value.checked_div(total).ok_or_else(type_mismatch)?;
                    let share = (frac_dec * Decimal::from(100)).round_dp(2);
                    let ratio_pct = scale_percentage(group.value, total);
                    let start_pct = cumulative_pct.min(100);
                    cumulative_pct = cumulative_pct.saturating_add(ratio_pct).min(100);
                    let sweep_frac = frac_dec
                        .to_string()
                        .parse::<f64>()
                        .unwrap_or(0.0)
                        .clamp(0.0, 1.0);
                    let svg_path = sector_svg_path(cumulative_frac, sweep_frac, is_donut);
                    cumulative_frac = (cumulative_frac + sweep_frac).min(1.0);
                    let x_pct = u32::try_from(10 + idx * 15).unwrap_or(50).min(90);
                    points.push(ComputedChartPoint {
                        label: group.label,
                        value: group.value.normalize().to_string(),
                        x_value: None,
                        y_value: None,
                        share_pct: Some(share.normalize().to_string()),
                        svg_path,
                        ratio_pct,
                        start_pct,
                        x_pct,
                        y_pct: 50,
                        row_refs: group.row_refs,
                    });
                }
                Ok(ComputedChartSeries {
                    chart_type: chart_type.as_str().to_owned(),
                    function: function.as_str().to_owned(),
                    column: column.to_owned(),
                    group_by: group_by.to_owned(),
                    total_or_kpi: total.normalize().to_string(),
                    points,
                })
            }
            ChartType::Scatter => {
                let x_col = self
                    .working
                    .column_by_name(group_by)
                    .ok_or_else(unknown_column)?;
                let y_col = self
                    .working
                    .column_by_name(column)
                    .ok_or_else(unknown_column)?;
                let mut pairs: Vec<(u64, Decimal, Decimal)> = Vec::new();
                for row in 0..self.working.row_count() {
                    let x_cell = x_col.cell_at(row).ok_or_else(invalid_request)?;
                    let y_cell = y_col.cell_at(row).ok_or_else(invalid_request)?;
                    if x_cell.is_empty() && y_cell.is_empty() {
                        continue;
                    }
                    let x_num = cell_number(x_cell).ok_or_else(type_mismatch)?;
                    let y_num = cell_number(y_cell).ok_or_else(type_mismatch)?;
                    let row_id = self
                        .row_id_at(row)
                        .map(RowId::get)
                        .ok_or_else(verification_mismatch)?;
                    pairs.push((row_id, x_num, y_num));
                }
                if pairs.is_empty() {
                    return Err(type_mismatch());
                }
                let min_x = pairs
                    .iter()
                    .map(|(_, x, _)| *x)
                    .fold(pairs[0].1, Decimal::min);
                let max_x = pairs
                    .iter()
                    .map(|(_, x, _)| *x)
                    .fold(pairs[0].1, Decimal::max);
                let min_y = pairs
                    .iter()
                    .map(|(_, _, y)| *y)
                    .fold(pairs[0].2, Decimal::min);
                let max_y = pairs
                    .iter()
                    .map(|(_, _, y)| *y)
                    .fold(pairs[0].2, Decimal::max);
                let span_x = max_x.checked_sub(min_x).unwrap_or(Decimal::ZERO);
                let span_y = max_y.checked_sub(min_y).unwrap_or(Decimal::ZERO);
                let count_str = pairs.len().to_string();
                let mut points = Vec::with_capacity(pairs.len());
                for (row_id, x_val, y_val) in pairs {
                    let x_pct = scale_horizontal_coord(x_val, min_x, span_x);
                    let y_pct = scale_vertical_coord(y_val, min_y, span_y);
                    points.push(ComputedChartPoint {
                        label: format!("#{row_id}"),
                        value: y_val.normalize().to_string(),
                        x_value: Some(x_val.normalize().to_string()),
                        y_value: Some(y_val.normalize().to_string()),
                        share_pct: None,
                        svg_path: None,
                        ratio_pct: scale_percentage(y_val.abs(), max_y.abs().max(Decimal::ONE)),
                        start_pct: 0,
                        x_pct,
                        y_pct,
                        row_refs: vec![row_id],
                    });
                }
                Ok(ComputedChartSeries {
                    chart_type: chart_type.as_str().to_owned(),
                    function: function.as_str().to_owned(),
                    column: column.to_owned(),
                    group_by: group_by.to_owned(),
                    total_or_kpi: count_str,
                    points,
                })
            }
            ChartType::Bubble => self.compute_bubble_series(function, column, group_by),
            ChartType::Gauge => self.compute_gauge_series(function, column, group_by),
            ChartType::BoxPlot => self.compute_boxplot_series(function, column, group_by),
            ChartType::Heatmap => self.compute_heatmap_series(function, column, group_by),
            ChartType::Treemap => self.compute_treemap_series(function, column, group_by),
            ChartType::Waterfall => self.compute_waterfall_series(function, column, group_by),
            ChartType::Funnel => self.compute_funnel_series(function, column, group_by),
            ChartType::Table => self.compute_table_series(function, column, group_by),
            ChartType::PivotSummary => {
                self.compute_pivot_summary_series(function, column, group_by)
            }
            ChartType::Geographic => self.compute_geographic_series(function, column, group_by),
            ChartType::Histogram => {
                let val_col = self
                    .working
                    .column_by_name(column)
                    .ok_or_else(unknown_column)?;
                let mut values: Vec<(u64, Decimal)> = Vec::new();
                for row in 0..self.working.row_count() {
                    let cell = val_col.cell_at(row).ok_or_else(invalid_request)?;
                    if cell.is_empty() {
                        continue;
                    }
                    let num = cell_number(cell).ok_or_else(type_mismatch)?;
                    let row_id = self
                        .row_id_at(row)
                        .map(RowId::get)
                        .ok_or_else(verification_mismatch)?;
                    values.push((row_id, num));
                }
                if values.is_empty() {
                    return Err(type_mismatch());
                }
                let min_v = values
                    .iter()
                    .map(|(_, v)| *v)
                    .fold(values[0].1, Decimal::min);
                let max_v = values
                    .iter()
                    .map(|(_, v)| *v)
                    .fold(values[0].1, Decimal::max);
                let total_count = values.len();
                let mut points = Vec::new();
                if min_v == max_v {
                    let refs: Vec<u64> = values.into_iter().map(|(id, _)| id).collect();
                    points.push(ComputedChartPoint {
                        label: format!("{}", min_v.normalize()),
                        value: total_count.to_string(),
                        x_value: None,
                        y_value: None,
                        share_pct: Some("100".to_owned()),
                        svg_path: None,
                        ratio_pct: 100,
                        start_pct: 0,
                        x_pct: 50,
                        y_pct: 10,
                        row_refs: refs,
                    });
                } else {
                    let bin_count = 5_usize;
                    let width = (max_v - min_v)
                        .checked_div(Decimal::from(bin_count as u64))
                        .ok_or_else(type_mismatch)?;
                    let mut bins: Vec<(Decimal, Decimal, Vec<u64>)> = (0..bin_count)
                        .map(|i| {
                            let low = min_v + width * Decimal::from(i as u64);
                            let high = if i + 1 == bin_count {
                                max_v
                            } else {
                                min_v + width * Decimal::from((i + 1) as u64)
                            };
                            (low, high, Vec::new())
                        })
                        .collect();
                    for (row_id, v) in values {
                        let mut placed = false;
                        for (idx, (low, high, bucket)) in bins.iter_mut().enumerate() {
                            let in_bin = if idx + 1 == bin_count {
                                v >= *low && v <= *high
                            } else {
                                v >= *low && v < *high
                            };
                            if in_bin {
                                bucket.push(row_id);
                                placed = true;
                                break;
                            }
                        }
                        if !placed && let Some((_, _, last)) = bins.last_mut() {
                            last.push(row_id);
                        }
                    }
                    let max_bin_len = bins
                        .iter()
                        .map(|(_, _, b)| b.len())
                        .max()
                        .unwrap_or(1)
                        .max(1);
                    for (idx, (low, high, bucket)) in bins.into_iter().enumerate() {
                        let count = bucket.len();
                        let ratio_pct = u32::try_from((count * 100) / max_bin_len)
                            .unwrap_or(0)
                            .min(100);
                        let x_pct = u32::try_from(10 + (idx * 80) / (bin_count - 1)).unwrap_or(50);
                        let y_pct = 90_u32.saturating_sub((ratio_pct * 80) / 100).clamp(10, 90);
                        points.push(ComputedChartPoint {
                            label: format!(
                                "{}–{}",
                                low.round_dp(2).normalize(),
                                high.round_dp(2).normalize()
                            ),
                            value: count.to_string(),
                            x_value: None,
                            y_value: None,
                            share_pct: None,
                            svg_path: None,
                            ratio_pct,
                            start_pct: 0,
                            x_pct,
                            y_pct,
                            row_refs: bucket,
                        });
                    }
                }
                Ok(ComputedChartSeries {
                    chart_type: chart_type.as_str().to_owned(),
                    function: function.as_str().to_owned(),
                    column: column.to_owned(),
                    group_by: group_by.to_owned(),
                    total_or_kpi: total_count.to_string(),
                    points,
                })
            }
            ChartType::Kpi => {
                let (overall, row_ids) =
                    aggregate_column(&self.working, &self.row_provenance, function, column)?;
                let mut points = Vec::new();
                if !group_by.trim().is_empty() && self.working.column_by_name(group_by).is_some() {
                    let groups = self.grouped_aggregate(function, column, group_by)?;
                    let max_val = groups
                        .iter()
                        .map(|g| g.value)
                        .fold(Decimal::ZERO, Decimal::max);
                    for group in groups {
                        points.push(ComputedChartPoint {
                            label: group.label,
                            value: group.value.normalize().to_string(),
                            x_value: None,
                            y_value: None,
                            share_pct: None,
                            svg_path: None,
                            ratio_pct: scale_percentage(group.value, max_val),
                            start_pct: 0,
                            x_pct: 50,
                            y_pct: 50,
                            row_refs: group.row_refs,
                        });
                    }
                } else {
                    points.push(ComputedChartPoint {
                        label: format!("{}({column})", function.as_str()),
                        value: overall.normalize().to_string(),
                        x_value: None,
                        y_value: None,
                        share_pct: Some("100".to_owned()),
                        svg_path: None,
                        ratio_pct: 100,
                        start_pct: 0,
                        x_pct: 50,
                        y_pct: 50,
                        row_refs: row_ids.iter().copied().map(RowId::get).collect(),
                    });
                }
                Ok(ComputedChartSeries {
                    chart_type: chart_type.as_str().to_owned(),
                    function: function.as_str().to_owned(),
                    column: column.to_owned(),
                    group_by: group_by.to_owned(),
                    total_or_kpi: overall.normalize().to_string(),
                    points,
                })
            }
        }
    }

    /// Rebuilds the derived table and full provenance by replaying recorded transformations in order.
    fn rebuild_provenance_with(&self, origin: &Table) -> Result<ReplayedProvenance, SafiError> {
        let mut state = MutableGraphState {
            table: origin.clone(),
            row_provenance: initial_row_provenance(origin),
            column_provenance: initial_column_provenance(origin),
            active_calculated: Vec::new(),
            next_column_id: (origin.column_count() as u64).saturating_add(1),
        };
        let mut max_row_id = (origin.row_count() as u64).saturating_add(1);
        for transformation in &self.transforms {
            if let Transformation::InsertRow { row_id, .. } = transformation {
                max_row_id = max_row_id.max(row_id.get().saturating_add(1));
            }
            apply_transformation_step(&mut state, transformation)?;
        }
        Ok(ReplayedProvenance {
            table: state.table,
            row_provenance: state.row_provenance,
            column_provenance: state.column_provenance,
            max_row_id,
            next_column_id: state.next_column_id,
        })
    }

    fn rebuild_state(&mut self) -> Result<(), SafiError> {
        let replayed = self.rebuild_provenance_with(&self.origin)?;
        self.working = replayed.table;
        self.row_provenance = replayed.row_provenance;
        self.column_provenance = replayed.column_provenance;
        self.next_row_id = self.next_row_id.max(replayed.max_row_id);
        self.next_column_id = self.next_column_id.max(replayed.next_column_id);
        Ok(())
    }

    fn resolved_parameters(&self) -> BTreeMap<String, Decimal> {
        let mut resolved = self.defaults.clone();
        if let Some(active) = self.active_scenario.as_ref()
            && let Some(scenario) = self.scenarios.get(active)
        {
            for (name, value) in &scenario.parameters {
                let _ = resolved.insert(name.clone(), *value);
            }
        }
        resolved
    }

    fn parameters_for(
        &self,
        scenario: Option<&ScenarioId>,
    ) -> Result<BTreeMap<String, Decimal>, SafiError> {
        let Some(scenario) = scenario else {
            return Ok(self.defaults.clone());
        };
        let scenario = self.scenarios.get(scenario).ok_or_else(invalid_request)?;
        let mut resolved = self.defaults.clone();
        for (name, value) in &scenario.parameters {
            let _ = resolved.insert(name.clone(), *value);
        }
        Ok(resolved)
    }

    fn relevant_trace(&self, columns: &BTreeSet<String>) -> Vec<String> {
        self.transforms
            .iter()
            .filter(|transformation| {
                transformation.touches_all_columns()
                    || !transformation.columns().is_disjoint(columns)
            })
            .map(Transformation::digest)
            .collect()
    }

    fn compute_metric(
        &self,
        definition: &MetricDefinition,
        parameters: &BTreeMap<String, Decimal>,
        now_ms: u64,
    ) -> Result<VerifiedMetricToken, SafiError> {
        let mut context = MetricContext {
            table: &self.working,
            row_provenance: &self.row_provenance,
            parameters,
            rows: BTreeSet::new(),
            columns: BTreeSet::new(),
        };
        let value = evaluate(&definition.formula, &mut context)?;
        let source_row_ids: Vec<RowId> = context.rows.iter().copied().collect();
        let row_refs: Vec<u64> = source_row_ids.iter().copied().map(RowId::get).collect();
        let column_refs: Vec<String> = definition.columns.iter().cloned().collect();
        let column_ids: Vec<ColumnId> = column_refs
            .iter()
            .filter_map(|name| self.column_id_for_name(name))
            .collect();
        if column_ids.len() != column_refs.len() {
            return Err(verification_mismatch());
        }
        Ok(VerifiedMetricToken::from_parts(MetricTokenParts {
            metric_id: definition.id.clone(),
            metric_name: definition.name.clone(),
            value: Value::Decimal(value),
            unit: definition.unit.clone(),
            formula: definition.source.clone(),
            formula_version: definition.version,
            source_id: self.source_id.clone(),
            source_version: self.dataset_version,
            source_fingerprint: self.origin_fingerprint.clone(),
            row_refs,
            source_row_ids,
            column_refs,
            column_ids,
            transformation_trace: self.relevant_trace(&definition.columns),
            computed_at_ms: now_ms,
            status: VerificationStatus::Verified,
        }))
    }

    fn refresh_metric(&mut self, id: &MetricId, now_ms: u64) {
        let Some(definition) = self.metrics.get(id).cloned() else {
            return;
        };
        let parameters = self.resolved_parameters();
        match self.compute_metric(&definition, &parameters, now_ms) {
            Ok(token) => {
                let _ = self.errors.remove(id);
                if let Some(existing) = self.tokens.get_mut(id) {
                    *existing = token;
                } else {
                    let _ = self.tokens.insert(id.clone(), token);
                }
            }
            Err(error) => {
                let _ = self.tokens.remove(id);
                let _ = self.errors.insert(id.clone(), error);
            }
        }
    }

    fn refresh_all(&mut self, now_ms: u64) {
        let ids: Vec<MetricId> = self.metrics.keys().cloned().collect();
        for id in &ids {
            self.refresh_metric(id, now_ms);
        }
        self.last_recomputed = ids;
        self.collect_provenance();
    }

    fn collect_provenance(&mut self) {
        let mut rows = BTreeSet::new();
        let mut columns = BTreeSet::new();
        for token in self.tokens.values() {
            rows.extend(token.row_refs.iter().copied());
            columns.extend(token.column_refs.iter().cloned());
        }
        self.last_rows = rows.into_iter().collect();
        self.last_columns = columns.into_iter().collect();
    }

    fn propagate(
        &mut self,
        changed: &BTreeSet<String>,
        now_ms: u64,
    ) -> Result<Vec<MetricId>, SafiError> {
        let mut changed = changed.clone();
        // Recalculate dependent calculated columns until no further column changes.
        loop {
            let mut progressed = false;
            for (name, formula) in self.calculated.clone() {
                if changed.contains(&name) {
                    continue;
                }
                let mut columns = BTreeSet::new();
                let mut parameters = BTreeSet::new();
                dependencies(&formula, FormulaScope::Cell, &mut columns, &mut parameters);
                if columns.is_disjoint(&changed) {
                    continue;
                }
                let cells = evaluate_column(&self.working, &formula)?;
                let index = self
                    .working
                    .columns()
                    .iter()
                    .position(|column| column.schema().name == name)
                    .ok_or_else(unknown_column)?;
                let mut rebuilt = Vec::with_capacity(self.working.column_count());
                for (position, column) in self.working.columns().iter().enumerate() {
                    if position == index {
                        rebuilt.push(Column::new(column.schema().clone(), cells.clone()));
                    } else {
                        rebuilt.push(column.clone());
                    }
                }
                self.working = Table::new(self.working.id(), self.working.name(), rebuilt)?;
                let _ = changed.insert(name);
                progressed = true;
            }
            if !progressed {
                break;
            }
        }

        let affected: Vec<MetricId> = self
            .metrics
            .values()
            .filter(|definition| !definition.columns.is_disjoint(&changed))
            .map(|definition| definition.id.clone())
            .collect();
        for id in &affected {
            self.refresh_metric(id, now_ms);
        }
        self.last_recomputed = affected.clone();
        self.collect_provenance();
        Ok(affected)
    }

    fn compute_bubble_series(
        &self,
        function: AggFunc,
        column: &str,
        group_by: &str,
    ) -> Result<ComputedChartSeries, SafiError> {
        let x_col = self
            .working
            .column_by_name(group_by)
            .ok_or_else(unknown_column)?;
        let y_col = self
            .working
            .column_by_name(column)
            .ok_or_else(unknown_column)?;

        let mut pairs: Vec<(u64, Decimal, Decimal)> = Vec::new();
        let mut all_numeric = true;
        for row in 0..self.working.row_count() {
            let x_cell = x_col.cell_at(row).ok_or_else(invalid_request)?;
            let y_cell = y_col.cell_at(row).ok_or_else(invalid_request)?;
            if x_cell.is_empty() && y_cell.is_empty() {
                continue;
            }
            let Some(x_num) = cell_number(x_cell) else {
                all_numeric = false;
                break;
            };
            let y_num = cell_number(y_cell).ok_or_else(type_mismatch)?;
            let row_id = self
                .row_id_at(row)
                .map(RowId::get)
                .ok_or_else(verification_mismatch)?;
            pairs.push((row_id, x_num, y_num));
        }

        if all_numeric && !pairs.is_empty() {
            let min_x = pairs
                .iter()
                .map(|(_, x, _)| *x)
                .fold(pairs[0].1, Decimal::min);
            let max_x = pairs
                .iter()
                .map(|(_, x, _)| *x)
                .fold(pairs[0].1, Decimal::max);
            let min_y = pairs
                .iter()
                .map(|(_, _, y)| *y)
                .fold(pairs[0].2, Decimal::min);
            let max_y = pairs
                .iter()
                .map(|(_, _, y)| *y)
                .fold(pairs[0].2, Decimal::max);
            let max_abs = pairs
                .iter()
                .map(|(_, _, y)| y.abs())
                .fold(Decimal::ONE, Decimal::max);
            let span_x = max_x.checked_sub(min_x).unwrap_or(Decimal::ZERO);
            let span_y = max_y.checked_sub(min_y).unwrap_or(Decimal::ZERO);
            let count_str = pairs.len().to_string();
            let mut points = Vec::with_capacity(pairs.len());
            for (row_id, x_val, y_val) in pairs {
                let ratio_pct = scale_percentage(y_val.abs(), max_abs).max(12);
                let radius = bubble_radius_from_pct(ratio_pct);
                points.push(ComputedChartPoint {
                    label: format!("#{row_id}"),
                    value: y_val.normalize().to_string(),
                    x_value: Some(x_val.normalize().to_string()),
                    y_value: Some(y_val.normalize().to_string()),
                    share_pct: Some(format!("{radius:.1}")),
                    svg_path: None,
                    ratio_pct,
                    start_pct: radius.round() as u32,
                    x_pct: scale_horizontal_coord(x_val, min_x, span_x),
                    y_pct: scale_vertical_coord(y_val, min_y, span_y),
                    row_refs: vec![row_id],
                });
            }
            return Ok(ComputedChartSeries {
                chart_type: ChartType::Bubble.as_str().to_owned(),
                function: function.as_str().to_owned(),
                column: column.to_owned(),
                group_by: group_by.to_owned(),
                total_or_kpi: count_str,
                points,
            });
        }

        let groups = self.grouped_aggregate(function, column, group_by)?;
        if groups.is_empty() {
            return Err(type_mismatch());
        }
        let max_val = groups
            .iter()
            .map(|g| g.value.abs())
            .fold(Decimal::ONE, Decimal::max);
        let min_val = groups
            .iter()
            .map(|g| g.value)
            .fold(groups[0].value, Decimal::min);
        let max_y = groups
            .iter()
            .map(|g| g.value)
            .fold(groups[0].value, Decimal::max);
        let span = max_y.checked_sub(min_val).unwrap_or(Decimal::ZERO);
        let total = groups
            .iter()
            .try_fold(Decimal::ZERO, |acc, g| acc.checked_add(g.value))
            .ok_or_else(type_mismatch)?;
        let count = groups.len();
        let mut points = Vec::with_capacity(count);
        for (idx, group) in groups.into_iter().enumerate() {
            let ratio_pct = scale_percentage(group.value.abs(), max_val).max(12);
            let radius = bubble_radius_from_pct(ratio_pct);
            let x_pct = if count <= 1 {
                50
            } else {
                u32::try_from(14 + (idx * 72) / (count - 1)).unwrap_or(50)
            };
            let y_pct = scale_vertical_coord(group.value, min_val, span);
            points.push(ComputedChartPoint {
                label: group.label,
                value: group.value.normalize().to_string(),
                x_value: Some(x_pct.to_string()),
                y_value: Some(group.value.normalize().to_string()),
                share_pct: Some(format!("{radius:.1}")),
                svg_path: None,
                ratio_pct,
                start_pct: radius.round() as u32,
                x_pct,
                y_pct,
                row_refs: group.row_refs,
            });
        }
        Ok(ComputedChartSeries {
            chart_type: ChartType::Bubble.as_str().to_owned(),
            function: function.as_str().to_owned(),
            column: column.to_owned(),
            group_by: group_by.to_owned(),
            total_or_kpi: total.normalize().to_string(),
            points,
        })
    }

    fn compute_gauge_series(
        &self,
        function: AggFunc,
        column: &str,
        group_by: &str,
    ) -> Result<ComputedChartSeries, SafiError> {
        let (overall, overall_refs) =
            aggregate_column(&self.working, &self.row_provenance, function, column)?;
        let groups =
            if !group_by.trim().is_empty() && self.working.column_by_name(group_by).is_some() {
                self.grouped_aggregate(function, column, group_by)?
            } else {
                Vec::new()
            };

        let target_sum = if groups.is_empty() {
            overall.abs().max(Decimal::ONE)
        } else {
            groups
                .iter()
                .map(|g| g.value.abs())
                .fold(Decimal::ZERO, |acc, v| acc + v)
                .max(Decimal::ONE)
        };
        let top_val = groups
            .iter()
            .map(|g| g.value.abs())
            .fold(Decimal::ZERO, Decimal::max);
        let pct_fill = if groups.len() > 1 && target_sum > Decimal::ZERO {
            scale_percentage(top_val, target_sum).clamp(5, 100)
        } else if overall <= Decimal::from(100) && overall >= Decimal::ZERO {
            u32::try_from(overall.round().trunc())
                .unwrap_or(100)
                .min(100)
        } else {
            75
        };

        let mut points = Vec::new();
        if groups.is_empty() {
            points.push(ComputedChartPoint {
                label: format!("{}({column})", function.as_str()),
                value: overall.normalize().to_string(),
                x_value: Some(pct_fill.to_string()),
                y_value: Some("100".to_owned()),
                share_pct: Some(pct_fill.to_string()),
                svg_path: gauge_arc_svg_path(pct_fill),
                ratio_pct: pct_fill,
                start_pct: 0,
                x_pct: 50,
                y_pct: 50,
                row_refs: overall_refs.into_iter().map(RowId::get).collect(),
            });
        } else {
            for (idx, group) in groups.into_iter().enumerate() {
                let g_pct = scale_percentage(group.value.abs(), target_sum).clamp(2, 100);
                points.push(ComputedChartPoint {
                    label: group.label,
                    value: group.value.normalize().to_string(),
                    x_value: Some(g_pct.to_string()),
                    y_value: Some("100".to_owned()),
                    share_pct: Some(scale_share_str(group.value.abs(), target_sum)),
                    svg_path: if idx == 0 {
                        gauge_arc_svg_path(pct_fill)
                    } else {
                        gauge_arc_svg_path(g_pct)
                    },
                    ratio_pct: g_pct,
                    start_pct: pct_fill,
                    x_pct: 50,
                    y_pct: 50,
                    row_refs: group.row_refs,
                });
            }
        }
        Ok(ComputedChartSeries {
            chart_type: ChartType::Gauge.as_str().to_owned(),
            function: function.as_str().to_owned(),
            column: column.to_owned(),
            group_by: group_by.to_owned(),
            total_or_kpi: overall.normalize().to_string(),
            points,
        })
    }

    fn compute_boxplot_series(
        &self,
        function: AggFunc,
        column: &str,
        group_by: &str,
    ) -> Result<ComputedChartSeries, SafiError> {
        let group_column = self
            .working
            .column_by_name(group_by)
            .ok_or_else(unknown_column)?;
        let value_column = self
            .working
            .column_by_name(column)
            .ok_or_else(unknown_column)?;
        let mut order: Vec<String> = Vec::new();
        let mut buckets: BTreeMap<String, (Vec<Decimal>, Vec<u64>)> = BTreeMap::new();
        for row in 0..self.working.row_count() {
            let cell = value_column.cell_at(row).ok_or_else(invalid_request)?;
            if cell.is_empty() {
                continue;
            }
            let num = cell_number(cell).ok_or_else(type_mismatch)?;
            let label = group_column
                .cell_at(row)
                .map(|c| c.visible_text().to_owned())
                .unwrap_or_default();
            let row_id = self
                .row_id_at(row)
                .map(RowId::get)
                .ok_or_else(verification_mismatch)?;
            if !buckets.contains_key(&label) {
                order.push(label.clone());
            }
            let entry = buckets.entry(label).or_default();
            entry.0.push(num);
            entry.1.push(row_id);
        }
        if order.is_empty() {
            return Err(type_mismatch());
        }

        let mut global_min = Decimal::MAX;
        let mut global_max = Decimal::MIN;
        let mut summaries = Vec::with_capacity(order.len());
        for label in order {
            let (mut vals, row_refs) = buckets.remove(&label).ok_or_else(invalid_request)?;
            vals.sort();
            let min_v = vals[0];
            let max_v = vals[vals.len() - 1];
            let q1 = decimal_quantile(&vals, 1, 4);
            let median = decimal_quantile(&vals, 2, 4);
            let q3 = decimal_quantile(&vals, 3, 4);
            global_min = global_min.min(min_v);
            global_max = global_max.max(max_v);
            summaries.push((label, min_v, q1, median, q3, max_v, row_refs));
        }
        let span = global_max.checked_sub(global_min).unwrap_or(Decimal::ZERO);
        let count = summaries.len();
        let mut median_sum = Decimal::ZERO;
        let mut points = Vec::with_capacity(count);
        for (idx, (label, min_v, q1, median, q3, max_v, row_refs)) in
            summaries.into_iter().enumerate()
        {
            median_sum += median;
            let x_pct = if count <= 1 {
                50
            } else {
                u32::try_from(14 + (idx * 72) / (count - 1)).unwrap_or(50)
            };
            let min_y = scale_vertical_coord(min_v, global_min, span);
            let q1_y = scale_vertical_coord(q1, global_min, span);
            let med_y = scale_vertical_coord(median, global_min, span);
            let q3_y = scale_vertical_coord(q3, global_min, span);
            let max_y = scale_vertical_coord(max_v, global_min, span);
            let iqr = q3 - q1;
            let svg_path = Some(format!(
                "M {x_pct} {max_y} L {x_pct} {min_y} M {x_l} {q3_y} L {x_r} {q3_y} L {x_r} {q1_y} L {x_l} {q1_y} Z M {x_m_l} {med_y} L {x_m_r} {med_y}",
                x_l = x_pct.saturating_sub(4),
                x_r = (x_pct + 4).min(96),
                x_m_l = x_pct.saturating_sub(5),
                x_m_r = (x_pct + 5).min(96),
            ));
            points.push(ComputedChartPoint {
                label,
                value: median.normalize().to_string(),
                x_value: Some(format!(
                    "{}..{}",
                    min_v.round_dp(2).normalize(),
                    max_v.round_dp(2).normalize()
                )),
                y_value: Some(format!(
                    "{}..{}",
                    q1.round_dp(2).normalize(),
                    q3.round_dp(2).normalize()
                )),
                share_pct: Some(iqr.round_dp(2).normalize().to_string()),
                svg_path,
                ratio_pct: q3_y,
                start_pct: q1_y,
                x_pct,
                y_pct: med_y,
                row_refs,
            });
        }
        Ok(ComputedChartSeries {
            chart_type: ChartType::BoxPlot.as_str().to_owned(),
            function: function.as_str().to_owned(),
            column: column.to_owned(),
            group_by: group_by.to_owned(),
            total_or_kpi: median_sum.normalize().to_string(),
            points,
        })
    }

    fn compute_heatmap_series(
        &self,
        function: AggFunc,
        column: &str,
        group_by: &str,
    ) -> Result<ComputedChartSeries, SafiError> {
        let groups = self.grouped_aggregate(function, column, group_by)?;
        if groups.is_empty() {
            return Err(type_mismatch());
        }
        let min_v = groups
            .iter()
            .map(|g| g.value)
            .fold(groups[0].value, Decimal::min);
        let max_v = groups
            .iter()
            .map(|g| g.value)
            .fold(groups[0].value, Decimal::max);
        let span = max_v.checked_sub(min_v).unwrap_or(Decimal::ZERO);
        let total = groups
            .iter()
            .try_fold(Decimal::ZERO, |acc, g| acc.checked_add(g.value))
            .ok_or_else(type_mismatch)?;
        let cols = (groups.len() as f64).sqrt().ceil().max(2.0) as usize;
        let rows = groups.len().div_ceil(cols).max(1);
        let mut points = Vec::with_capacity(groups.len());
        for (idx, group) in groups.into_iter().enumerate() {
            let intensity = if span <= Decimal::ZERO {
                50
            } else {
                scale_percentage(group.value - min_v, span)
            };
            let grid_col = idx % cols;
            let grid_row = idx / cols;
            let x_pct = u32::try_from(8 + (grid_col * 84) / cols).unwrap_or(10);
            let y_pct = u32::try_from(12 + (grid_row * 76) / rows).unwrap_or(12);
            points.push(ComputedChartPoint {
                label: group.label,
                value: group.value.normalize().to_string(),
                x_value: Some(grid_col.to_string()),
                y_value: Some(grid_row.to_string()),
                share_pct: Some(intensity.to_string()),
                svg_path: None,
                ratio_pct: intensity,
                start_pct: u32::try_from(cols).unwrap_or(2),
                x_pct,
                y_pct,
                row_refs: group.row_refs,
            });
        }
        Ok(ComputedChartSeries {
            chart_type: ChartType::Heatmap.as_str().to_owned(),
            function: function.as_str().to_owned(),
            column: column.to_owned(),
            group_by: group_by.to_owned(),
            total_or_kpi: total.normalize().to_string(),
            points,
        })
    }

    fn compute_treemap_series(
        &self,
        function: AggFunc,
        column: &str,
        group_by: &str,
    ) -> Result<ComputedChartSeries, SafiError> {
        let mut groups = self.grouped_aggregate(function, column, group_by)?;
        if groups.is_empty() {
            return Err(type_mismatch());
        }
        groups.sort_by_key(|a| std::cmp::Reverse(a.value.abs()));
        let abs_total = groups
            .iter()
            .map(|g| g.value.abs())
            .fold(Decimal::ZERO, |acc, v| acc + v)
            .max(Decimal::ONE);
        let total = groups
            .iter()
            .try_fold(Decimal::ZERO, |acc, g| acc.checked_add(g.value))
            .ok_or_else(type_mismatch)?;

        let mut rx = 4.0_f64;
        let mut ry = 8.0_f64;
        let mut rw = 92.0_f64;
        let mut rh = 84.0_f64;
        let mut remaining_abs = abs_total
            .to_string()
            .parse::<f64>()
            .unwrap_or(1.0)
            .max(1e-9);
        let mut points = Vec::with_capacity(groups.len());
        let len = groups.len();
        for (idx, group) in groups.into_iter().enumerate() {
            let val_f = group
                .value
                .abs()
                .to_string()
                .parse::<f64>()
                .unwrap_or(0.0)
                .max(0.0);
            let frac = if idx + 1 == len || remaining_abs <= 1e-9 {
                1.0
            } else {
                (val_f / remaining_abs).clamp(0.06, 0.94)
            };
            remaining_abs = (remaining_abs - val_f).max(1e-9);
            let (tx, ty, tw, th) = if idx % 2 == 0 {
                let w = rw * frac;
                let rect = (rx, ry, w, rh);
                rx += w;
                rw = (rw - w).max(4.0);
                rect
            } else {
                let h = rh * frac;
                let rect = (rx, ry, rw, h);
                ry += h;
                rh = (rh - h).max(4.0);
                rect
            };
            let share_str = scale_share_str(group.value.abs(), abs_total);
            points.push(ComputedChartPoint {
                label: group.label,
                value: group.value.normalize().to_string(),
                x_value: Some(format!("{tx:.1},{ty:.1}")),
                y_value: Some(format!("{tw:.1},{th:.1}")),
                share_pct: Some(share_str),
                svg_path: Some(format!(
                    "M {tx:.1} {ty:.1} h {tw:.1} v {th:.1} h -{tw:.1} Z"
                )),
                ratio_pct: scale_percentage(group.value.abs(), abs_total),
                start_pct: 0,
                x_pct: (tx + tw / 2.0).round() as u32,
                y_pct: (ty + th / 2.0).round() as u32,
                row_refs: group.row_refs,
            });
        }
        Ok(ComputedChartSeries {
            chart_type: ChartType::Treemap.as_str().to_owned(),
            function: function.as_str().to_owned(),
            column: column.to_owned(),
            group_by: group_by.to_owned(),
            total_or_kpi: total.normalize().to_string(),
            points,
        })
    }

    fn compute_waterfall_series(
        &self,
        function: AggFunc,
        column: &str,
        group_by: &str,
    ) -> Result<ComputedChartSeries, SafiError> {
        let groups = self.grouped_aggregate(function, column, group_by)?;
        if groups.is_empty() {
            return Err(type_mismatch());
        }
        let mut running = Decimal::ZERO;
        let mut min_cum = Decimal::ZERO;
        let mut max_cum = Decimal::ZERO;
        let mut steps = Vec::with_capacity(groups.len());
        for group in groups {
            let start = running;
            running += group.value;
            min_cum = min_cum.min(start).min(running);
            max_cum = max_cum.max(start).max(running);
            steps.push((group.label, group.value, start, running, group.row_refs));
        }
        let span = max_cum.checked_sub(min_cum).unwrap_or(Decimal::ZERO);
        let step_count = steps.len() + 1;
        let mut points = Vec::with_capacity(step_count);
        for (idx, (label, delta, start, end, row_refs)) in steps.into_iter().enumerate() {
            let start_y = scale_vertical_coord(start, min_cum, span);
            let end_y = scale_vertical_coord(end, min_cum, span);
            let x_pct =
                u32::try_from(12 + (idx * 76) / step_count.saturating_sub(1).max(1)).unwrap_or(50);
            let delta_sign = if delta >= Decimal::ZERO { "+" } else { "" };
            points.push(ComputedChartPoint {
                label,
                value: delta.normalize().to_string(),
                x_value: Some(start.normalize().to_string()),
                y_value: Some(end.normalize().to_string()),
                share_pct: Some(format!("{delta_sign}{}", delta.normalize())),
                svg_path: None,
                ratio_pct: start_y.min(end_y),
                start_pct: start_y.max(end_y),
                x_pct,
                y_pct: end_y,
                row_refs,
            });
        }
        let zero_y = scale_vertical_coord(Decimal::ZERO, min_cum, span);
        let final_y = scale_vertical_coord(running, min_cum, span);
        points.push(ComputedChartPoint {
            label: "Net Total".to_owned(),
            value: running.normalize().to_string(),
            x_value: Some("0".to_owned()),
            y_value: Some(running.normalize().to_string()),
            share_pct: Some("Total".to_owned()),
            svg_path: None,
            ratio_pct: zero_y.min(final_y),
            start_pct: zero_y.max(final_y),
            x_pct: 88,
            y_pct: final_y,
            row_refs: Vec::new(),
        });
        Ok(ComputedChartSeries {
            chart_type: ChartType::Waterfall.as_str().to_owned(),
            function: function.as_str().to_owned(),
            column: column.to_owned(),
            group_by: group_by.to_owned(),
            total_or_kpi: running.normalize().to_string(),
            points,
        })
    }

    fn compute_funnel_series(
        &self,
        function: AggFunc,
        column: &str,
        group_by: &str,
    ) -> Result<ComputedChartSeries, SafiError> {
        let mut groups = self.grouped_aggregate(function, column, group_by)?;
        if groups.is_empty() {
            return Err(type_mismatch());
        }
        groups.sort_by_key(|a| std::cmp::Reverse(a.value.abs()));
        let top = groups[0].value.abs().max(Decimal::ONE);
        let total = groups
            .iter()
            .try_fold(Decimal::ZERO, |acc, g| acc.checked_add(g.value))
            .ok_or_else(type_mismatch)?;
        let count = groups.len();
        let mut points = Vec::with_capacity(count);
        for (idx, group) in groups.iter().enumerate() {
            let width_top = scale_percentage(group.value.abs(), top).max(16);
            let width_bottom = if idx + 1 < count {
                scale_percentage(groups[idx + 1].value.abs(), top).max(12)
            } else {
                (width_top * 3 / 4).max(10)
            };
            let retention = if idx == 0 {
                "100".to_owned()
            } else {
                scale_share_str(
                    group.value.abs(),
                    groups[idx - 1].value.abs().max(Decimal::ONE),
                )
            };
            points.push(ComputedChartPoint {
                label: group.label.clone(),
                value: group.value.normalize().to_string(),
                x_value: Some(retention),
                y_value: Some(top.normalize().to_string()),
                share_pct: Some(scale_share_str(group.value.abs(), top)),
                svg_path: None,
                ratio_pct: width_top,
                start_pct: width_bottom,
                x_pct: 50,
                y_pct: u32::try_from(10 + (idx * 80) / count.max(1)).unwrap_or(50),
                row_refs: group.row_refs.clone(),
            });
        }
        Ok(ComputedChartSeries {
            chart_type: ChartType::Funnel.as_str().to_owned(),
            function: function.as_str().to_owned(),
            column: column.to_owned(),
            group_by: group_by.to_owned(),
            total_or_kpi: total.normalize().to_string(),
            points,
        })
    }

    fn compute_table_series(
        &self,
        function: AggFunc,
        column: &str,
        group_by: &str,
    ) -> Result<ComputedChartSeries, SafiError> {
        let mut groups = self.grouped_aggregate(function, column, group_by)?;
        if groups.is_empty() {
            return Err(type_mismatch());
        }
        groups.sort_by_key(|a| std::cmp::Reverse(a.value));
        let abs_total = groups
            .iter()
            .map(|g| g.value.abs())
            .fold(Decimal::ZERO, |acc, v| acc + v)
            .max(Decimal::ONE);
        let max_val = groups
            .iter()
            .map(|g| g.value.abs())
            .fold(Decimal::ONE, Decimal::max);
        let total = groups
            .iter()
            .try_fold(Decimal::ZERO, |acc, g| acc.checked_add(g.value))
            .ok_or_else(type_mismatch)?;
        let mut points = Vec::with_capacity(groups.len());
        for (idx, group) in groups.into_iter().enumerate() {
            let row_count = group.row_refs.len();
            points.push(ComputedChartPoint {
                label: group.label,
                value: group.value.normalize().to_string(),
                x_value: Some(format!("#{}", idx + 1)),
                y_value: Some(row_count.to_string()),
                share_pct: Some(scale_share_str(group.value.abs(), abs_total)),
                svg_path: None,
                ratio_pct: scale_percentage(group.value.abs(), max_val),
                start_pct: u32::try_from(idx + 1).unwrap_or(1),
                x_pct: 50,
                y_pct: 50,
                row_refs: group.row_refs,
            });
        }
        Ok(ComputedChartSeries {
            chart_type: ChartType::Table.as_str().to_owned(),
            function: function.as_str().to_owned(),
            column: column.to_owned(),
            group_by: group_by.to_owned(),
            total_or_kpi: total.normalize().to_string(),
            points,
        })
    }

    fn compute_pivot_summary_series(
        &self,
        function: AggFunc,
        column: &str,
        group_by: &str,
    ) -> Result<ComputedChartSeries, SafiError> {
        let group_column = self
            .working
            .column_by_name(group_by)
            .ok_or_else(unknown_column)?;
        let value_column = self
            .working
            .column_by_name(column)
            .ok_or_else(unknown_column)?;
        let mut order: Vec<String> = Vec::new();
        let mut buckets: BTreeMap<String, (Vec<Decimal>, Vec<u64>)> = BTreeMap::new();
        for row in 0..self.working.row_count() {
            let cell = value_column.cell_at(row).ok_or_else(invalid_request)?;
            if cell.is_empty() {
                continue;
            }
            let num = cell_number(cell).ok_or_else(type_mismatch)?;
            let label = group_column
                .cell_at(row)
                .map(|c| c.visible_text().to_owned())
                .unwrap_or_default();
            let row_id = self
                .row_id_at(row)
                .map(RowId::get)
                .ok_or_else(verification_mismatch)?;
            if !buckets.contains_key(&label) {
                order.push(label.clone());
            }
            let entry = buckets.entry(label).or_default();
            entry.0.push(num);
            entry.1.push(row_id);
        }
        if order.is_empty() {
            return Err(type_mismatch());
        }
        let primary_groups = self.grouped_aggregate(function, column, group_by)?;
        let primary_map: BTreeMap<String, Decimal> = primary_groups
            .into_iter()
            .map(|g| (g.label, g.value))
            .collect();
        let mut grand_sum = Decimal::ZERO;
        let mut raw_entries = Vec::with_capacity(order.len());
        for label in order {
            let (vals, row_refs) = buckets.remove(&label).ok_or_else(invalid_request)?;
            let sum = vals.iter().copied().fold(Decimal::ZERO, |acc, v| acc + v);
            let cnt = vals.len();
            let avg = sum
                .checked_div(Decimal::from(cnt as u64))
                .unwrap_or(Decimal::ZERO)
                .round_dp(AVERAGE_SCALE);
            let min_v = vals.iter().copied().fold(vals[0], Decimal::min);
            let max_v = vals.iter().copied().fold(vals[0], Decimal::max);
            let primary_val = primary_map.get(&label).copied().unwrap_or(sum);
            grand_sum += sum;
            raw_entries.push((label, primary_val, cnt, avg, min_v, max_v, row_refs));
        }
        let abs_total = grand_sum.abs().max(Decimal::ONE);
        let max_primary = raw_entries
            .iter()
            .map(|e| e.1.abs())
            .fold(Decimal::ONE, Decimal::max);
        let mut points = Vec::with_capacity(raw_entries.len());
        for (label, primary_val, cnt, avg, min_v, max_v, row_refs) in raw_entries {
            points.push(ComputedChartPoint {
                label,
                value: primary_val.normalize().to_string(),
                x_value: Some(format!("n={cnt}, avg={}", avg.normalize())),
                y_value: Some(format!("{}..{}", min_v.normalize(), max_v.normalize())),
                share_pct: Some(scale_share_str(primary_val.abs(), abs_total)),
                svg_path: None,
                ratio_pct: scale_percentage(primary_val.abs(), max_primary),
                start_pct: u32::try_from(cnt).unwrap_or(0),
                x_pct: 50,
                y_pct: 50,
                row_refs,
            });
        }
        Ok(ComputedChartSeries {
            chart_type: ChartType::PivotSummary.as_str().to_owned(),
            function: function.as_str().to_owned(),
            column: column.to_owned(),
            group_by: group_by.to_owned(),
            total_or_kpi: grand_sum.normalize().to_string(),
            points,
        })
    }

    fn compute_geographic_series(
        &self,
        function: AggFunc,
        column: &str,
        group_by: &str,
    ) -> Result<ComputedChartSeries, SafiError> {
        let groups = self.grouped_aggregate(function, column, group_by)?;
        if groups.is_empty() {
            return Err(type_mismatch());
        }
        let max_val = groups
            .iter()
            .map(|g| g.value.abs())
            .fold(Decimal::ONE, Decimal::max);
        let abs_total = groups
            .iter()
            .map(|g| g.value.abs())
            .fold(Decimal::ZERO, |acc, v| acc + v)
            .max(Decimal::ONE);
        let total = groups
            .iter()
            .try_fold(Decimal::ZERO, |acc, g| acc.checked_add(g.value))
            .ok_or_else(type_mismatch)?;
        let mut points = Vec::with_capacity(groups.len());
        for (idx, group) in groups.into_iter().enumerate() {
            let (x_pct, y_pct) = geo_region_coords(&group.label, idx);
            let ratio_pct = scale_percentage(group.value.abs(), max_val).max(15);
            points.push(ComputedChartPoint {
                label: group.label,
                value: group.value.normalize().to_string(),
                x_value: Some(format!("{x_pct}")),
                y_value: Some(format!("{y_pct}")),
                share_pct: Some(scale_share_str(group.value.abs(), abs_total)),
                svg_path: None,
                ratio_pct,
                start_pct: 0,
                x_pct,
                y_pct,
                row_refs: group.row_refs,
            });
        }
        Ok(ComputedChartSeries {
            chart_type: ChartType::Geographic.as_str().to_owned(),
            function: function.as_str().to_owned(),
            column: column.to_owned(),
            group_by: group_by.to_owned(),
            total_or_kpi: total.normalize().to_string(),
            points,
        })
    }

    /// Computes a chart series optionally filtered by exact trimmed case-insensitive cell match.
    ///
    /// # Errors
    /// Propagates column-lookup, aggregation, or chart-compatibility `SafiError` codes.
    pub fn compute_chart_series_filtered(
        &self,
        chart_type: ChartType,
        function: AggFunc,
        column: &str,
        group_by: &str,
        filter_column: Option<&str>,
        filter_value: Option<&str>,
    ) -> Result<ComputedChartSeries, SafiError> {
        let Some(f_col) = filter_column.map(str::trim).filter(|s| !s.is_empty()) else {
            return self.compute_chart_series(chart_type, function, column, group_by);
        };
        let Some(f_val) = filter_value.map(str::trim).filter(|s| !s.is_empty()) else {
            return self.compute_chart_series(chart_type, function, column, group_by);
        };
        let target_col = self
            .working
            .column_by_name(f_col)
            .ok_or_else(unknown_column)?;
        let needle = f_val.to_lowercase();
        let matched_rows: Vec<usize> = (0..self.working.row_count())
            .filter(|&row| {
                target_col
                    .cell_at(row)
                    .is_some_and(|c| c.visible_text().trim().to_lowercase() == needle)
            })
            .collect();
        let mut columns = Vec::with_capacity(self.working.column_count());
        for col in self.working.columns() {
            let cells: Vec<CellValue> = matched_rows
                .iter()
                .filter_map(|&row_idx| col.cell_at(row_idx).cloned())
                .collect();
            columns.push(Column::new(col.schema().clone(), cells));
        }
        let filtered_table = Table::new(self.working.id(), self.working.name(), columns)?;
        let filtered_prov: Vec<RowProvenance> = matched_rows
            .iter()
            .filter_map(|&row_idx| self.row_provenance.get(row_idx).cloned())
            .collect();
        let mut sliced = self.clone();
        sliced.working = filtered_table;
        sliced.row_provenance = filtered_prov;
        sliced.compute_chart_series(chart_type, function, column, group_by)
    }
}

/// Maximum rows returned by a single paginated grid query.
pub const MAX_ROW_PAGE_LIMIT: usize = 200;

/// Legacy persisted snapshot schema version without explicit provenance tables.
pub const LEGACY_PROJECT_SCHEMA_VERSION: u32 = 1;

/// Schema version of a persisted Studio project.
pub const PROJECT_SCHEMA_VERSION: u32 = 2;

/// Query specification for paginated/virtualized row retrieval without transferring the full table.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct RowQuery {
    /// Zero-based offset inside the filtered result set.
    pub offset: usize,
    /// Maximum number of rows to return (clamped to `1..=MAX_ROW_PAGE_LIMIT`).
    pub limit: usize,
    /// Optional global case-insensitive search substring across columns.
    pub search: Option<String>,
    /// Optional column name to sort by (view-only, does not mutate provenance).
    pub sort_column: Option<String>,
    /// Sort direction when `sort_column` is specified.
    pub ascending: bool,
    /// Optional column name for targeted filtering.
    pub filter_column: Option<String>,
    /// Optional filter value substring matched within `filter_column`.
    pub filter_value: Option<String>,
}

impl Default for RowQuery {
    fn default() -> Self {
        Self {
            offset: 0,
            limit: 50,
            search: None,
            sort_column: None,
            ascending: true,
            filter_column: None,
            filter_value: None,
        }
    }
}

/// Cell payload returned for a single visible row page entry.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RowCellView {
    /// Unmodified raw cell text.
    pub raw: String,
    /// Normalized canonical string.
    pub normalized: String,
    /// Formatted visible string.
    pub visible: String,
    /// Whether the working cell differs from the immutable original cell.
    pub changed: bool,
    /// Validation error text, if any.
    pub validation_error: Option<String>,
}

/// One working row inside a paginated [`RowPage`] along with its stable provenance identity.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RowPageItem {
    /// Current zero-based position in the working table.
    pub working_row: usize,
    /// Stable monotonic row identity (`RowId::get()`).
    pub row_id: u64,
    /// Original source row identity (`None` for rows inserted inside Studio).
    pub origin_row_id: Option<u64>,
    /// Original zero-based row index inside the immutable source table (`None` if inserted).
    pub origin_row_index: Option<usize>,
    /// Cell views aligned with working columns.
    pub cells: Vec<RowCellView>,
}

/// Bounded response returned by [`DataGraph::rows_page`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RowPage {
    /// Total rows in the working table before query filters.
    pub total_rows: usize,
    /// Total rows matching the query filters.
    pub filtered_rows: usize,
    /// Actual offset used.
    pub offset: usize,
    /// Clamped page limit used.
    pub limit: usize,
    /// Graph revision at the time the page was produced.
    pub revision: u64,
    /// Slice of rows for the requested window.
    pub rows: Vec<RowPageItem>,
}

/// Public, serializable description of a metric definition (used by project persistence).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MetricSpec {
    /// Stable metric identifier.
    pub id: MetricId,
    /// Display name.
    pub name: String,
    /// Optional unit.
    pub unit: Option<String>,
    /// Formula source text.
    pub formula: String,
    /// Monotonic formula version.
    pub formula_version: u32,
}

/// A complete, verifiable Studio project.
///
/// Restoring replays the recorded transformations on the stored origin and then re-derives every
/// metric, so a tampered number in the file cannot be presented as verified: the token is
/// recomputed from the data and the formula only.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProjectSnapshot {
    /// Snapshot schema version.
    pub schema_version: u32,
    /// Source identifier.
    pub source_id: SourceId,
    /// Immutable original dataset.
    pub origin: Table,
    /// Fingerprint of the original dataset at save time.
    pub origin_fingerprint: String,
    /// Published dataset version.
    pub dataset_version: u64,
    /// Published revision.
    pub revision: u64,
    /// Recorded transformations in order.
    pub transformations: Vec<Transformation>,
    /// Metric definitions.
    pub metrics: Vec<MetricSpec>,
    /// What-If parameter defaults.
    pub parameters: BTreeMap<String, Decimal>,
    /// Saved scenarios.
    pub scenarios: Vec<Scenario>,
    /// Currently active What-If scenario, if any.
    #[serde(default)]
    pub active_scenario: Option<ScenarioId>,
    /// Working-row to origin-row provenance mapping aligned with working row order.
    #[serde(default)]
    pub row_provenance: Vec<RowProvenance>,
    /// Working-column to origin-column provenance mapping.
    #[serde(default)]
    pub column_provenance: Vec<ColumnProvenance>,
    /// Next monotonic row identity (retained across save/restore so deleted IDs are never reused).
    #[serde(default)]
    pub next_row_id: u64,
    /// Next monotonic column identity.
    #[serde(default)]
    pub next_column_id: u64,
}

fn metric_decimal(token: &VerifiedMetricToken) -> Result<Decimal, SafiError> {
    match &token.value {
        Value::Decimal(value) | Value::Percentage(value) => Ok(*value),
        Value::Integer(integer) => Ok(Decimal::from(*integer)),
        _ => Err(type_mismatch()),
    }
}

/// Formats a metric value for display and narrative substitution without float conversion.
#[must_use]
pub fn datagraph_display(value: &Value) -> String {
    match value {
        Value::Decimal(decimal) | Value::Percentage(decimal) => decimal.normalize().to_string(),
        Value::Integer(integer) => integer.to_string(),
        Value::Money(money) => money.amount.normalize().to_string(),
        Value::Empty => String::new(),
        other => other.to_canonical_string(),
    }
}

/// One row of a grouped aggregation computed by Rust (dashboards and the offline capsule).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupedValue {
    /// Group label taken from the source cells.
    pub label: String,
    /// Exact aggregated value.
    pub value: Decimal,
    /// Rows that fed this group.
    pub row_refs: Vec<u64>,
}

/// Supported Quad-Studio dashboard chart visualisations (`STUDIO-PHASE-2` + 21-chart expansion).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChartType {
    /// Horizontal categorical bar chart.
    #[default]
    Bar,
    /// Vertical categorical column chart.
    Column,
    /// Ordered series line chart.
    Line,
    /// Filled series area chart.
    Area,
    /// Stacked categorical bar/column chart.
    Stacked,
    /// Combined bar + line chart.
    Combo,
    /// Proportional pie chart (requires non-negative values and positive total).
    Pie,
    /// Proportional donut ring chart (requires non-negative values and positive total).
    Donut,
    /// Two-column numeric `(x, y)` scatter plot (`group_by` = X column, `column` = Y column).
    Scatter,
    /// Two-column numeric bubble chart with magnitude radius.
    Bubble,
    /// Numeric frequency distribution histogram across equal-width bins.
    Histogram,
    /// Quartile & median distribution box plot.
    BoxPlot,
    /// Matrix intensity heatmap.
    Heatmap,
    /// Hierarchical proportional treemap.
    Treemap,
    /// Cumulative sequential waterfall bridge chart.
    Waterfall,
    /// Stage conversion funnel chart.
    Funnel,
    /// Radial progress gauge chart.
    Gauge,
    /// Single headline KPI card with optional group breakdown.
    Kpi,
    /// Tabular summary grid visual.
    Table,
    /// Cross-tabulated pivot summary visual.
    PivotSummary,
    /// Regional/geographic choropleth/symbol visual.
    Geographic,
}

impl ChartType {
    /// Parses a chart type identifier case-insensitively.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "bar" => Some(Self::Bar),
            "column" => Some(Self::Column),
            "line" => Some(Self::Line),
            "area" => Some(Self::Area),
            "stacked" => Some(Self::Stacked),
            "combo" => Some(Self::Combo),
            "pie" => Some(Self::Pie),
            "donut" => Some(Self::Donut),
            "scatter" => Some(Self::Scatter),
            "bubble" => Some(Self::Bubble),
            "histogram" => Some(Self::Histogram),
            "boxplot" | "box_plot" => Some(Self::BoxPlot),
            "heatmap" => Some(Self::Heatmap),
            "treemap" => Some(Self::Treemap),
            "waterfall" => Some(Self::Waterfall),
            "funnel" => Some(Self::Funnel),
            "gauge" => Some(Self::Gauge),
            "kpi" => Some(Self::Kpi),
            "table" => Some(Self::Table),
            "pivotsummary" | "pivot_summary" | "pivot" => Some(Self::PivotSummary),
            "geographic" | "geo" | "map" => Some(Self::Geographic),
            _ => None,
        }
    }

    /// Canonical lowercase identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bar => "bar",
            Self::Column => "column",
            Self::Line => "line",
            Self::Area => "area",
            Self::Stacked => "stacked",
            Self::Combo => "combo",
            Self::Pie => "pie",
            Self::Donut => "donut",
            Self::Scatter => "scatter",
            Self::Bubble => "bubble",
            Self::Histogram => "histogram",
            Self::BoxPlot => "boxplot",
            Self::Heatmap => "heatmap",
            Self::Treemap => "treemap",
            Self::Waterfall => "waterfall",
            Self::Funnel => "funnel",
            Self::Gauge => "gauge",
            Self::Kpi => "kpi",
            Self::Table => "table",
            Self::PivotSummary => "pivotsummary",
            Self::Geographic => "geographic",
        }
    }
}

/// One pre-computed chart data point produced in Rust (never calculated in TypeScript).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputedChartPoint {
    /// Category, bin range, or point label.
    pub label: String,
    /// Primary numeric value formatted canonically.
    pub value: String,
    /// Optional X-axis numeric value (for Scatter charts).
    pub x_value: Option<String>,
    /// Optional Y-axis numeric value (for Scatter charts).
    pub y_value: Option<String>,
    /// Optional percentage share string (`0..=100`, for Pie and Donut charts).
    pub share_pct: Option<String>,
    /// Pre-computed SVG path (`d="..."`) for circular charts (`pie`, `donut`) in a `0..=100` viewBox.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub svg_path: Option<String>,
    /// Pre-scaled magnitude percentage (`0..=100`) for bar/histogram/pie widths.
    pub ratio_pct: u32,
    /// Pre-scaled cumulative start percentage (`0..=100`) for pie/donut ring offsets.
    pub start_pct: u32,
    /// Pre-scaled horizontal SVG coordinate (`10..=90`) for line/area/scatter charts.
    pub x_pct: u32,
    /// Pre-scaled vertical SVG coordinate (`10..=90`, top=10, bottom=90) for line/area/scatter charts.
    pub y_pct: u32,
    /// Stable source row identities (`RowId::get()`) contributing to this point.
    pub row_refs: Vec<u64>,
}

/// Complete Rust-computed chart dataset ready for rendering in Desktop UI or `.safi.html` capsule.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputedChartSeries {
    /// Canonical chart type (`bar`, `line`, `area`, `pie`, `donut`, `scatter`, `histogram`, `kpi`).
    pub chart_type: String,
    /// Canonical aggregation function (`SUM`, `AVG`, `COUNT`, `MIN`, `MAX`).
    pub function: String,
    /// Primary value column (or Y column for `scatter`).
    pub column: String,
    /// Group-by column (or X column for `scatter`).
    pub group_by: String,
    /// Overall headline/total value computed in Rust (used by KPI, Pie, Donut, and summary badges).
    pub total_or_kpi: String,
    /// Pre-computed points/slices/bins/coordinates.
    pub points: Vec<ComputedChartPoint>,
}

fn sector_svg_path(start_frac: f64, sweep_frac: f64, donut: bool) -> Option<String> {
    if sweep_frac <= 0.0 {
        return None;
    }
    if sweep_frac >= 0.999_999 {
        return Some(if donut {
            "M 50.00 10.00 A 40 40 0 1 1 50.00 90.00 A 40 40 0 1 1 50.00 10.00 M 50.00 28.00 A 22 22 0 1 0 50.00 72.00 A 22 22 0 1 0 50.00 28.00 Z".to_owned()
        } else {
            "M 50.00 10.00 A 40 40 0 1 1 50.00 90.00 A 40 40 0 1 1 50.00 10.00 Z".to_owned()
        });
    }
    let start_rad = -std::f64::consts::FRAC_PI_2 + start_frac * std::f64::consts::TAU;
    let end_rad = start_rad + sweep_frac * std::f64::consts::TAU;
    let large_arc = if sweep_frac > 0.5 { 1 } else { 0 };
    let ox1 = 50.0 + 40.0 * start_rad.cos();
    let oy1 = 50.0 + 40.0 * start_rad.sin();
    let ox2 = 50.0 + 40.0 * end_rad.cos();
    let oy2 = 50.0 + 40.0 * end_rad.sin();
    if donut {
        let ix2 = 50.0 + 22.0 * end_rad.cos();
        let iy2 = 50.0 + 22.0 * end_rad.sin();
        let ix1 = 50.0 + 22.0 * start_rad.cos();
        let iy1 = 50.0 + 22.0 * start_rad.sin();
        Some(format!(
            "M {ox1:.2} {oy1:.2} A 40 40 0 {large_arc} 1 {ox2:.2} {oy2:.2} L {ix2:.2} {iy2:.2} A 22 22 0 {large_arc} 0 {ix1:.2} {iy1:.2} Z"
        ))
    } else {
        Some(format!(
            "M 50.00 50.00 L {ox1:.2} {oy1:.2} A 40 40 0 {large_arc} 1 {ox2:.2} {oy2:.2} Z"
        ))
    }
}

fn scale_percentage(value: Decimal, maximum: Decimal) -> u32 {
    if maximum <= Decimal::ZERO || value <= Decimal::ZERO {
        return 0;
    }
    let ratio = value.checked_div(maximum).unwrap_or(Decimal::ZERO);
    let percent = (ratio * Decimal::from(100)).round();
    let clamped = percent.max(Decimal::ZERO).min(Decimal::from(100));
    u32::try_from(clamped.trunc()).unwrap_or(100)
}

fn scale_share_str(value: Decimal, total: Decimal) -> String {
    if total <= Decimal::ZERO {
        return "0".to_owned();
    }
    let share = (value * Decimal::from(100))
        .checked_div(total)
        .unwrap_or(Decimal::ZERO)
        .round_dp(2)
        .normalize();
    share.to_string()
}

fn scale_horizontal_coord(value: Decimal, min_val: Decimal, span: Decimal) -> u32 {
    if span <= Decimal::ZERO {
        return 50;
    }
    let norm = value
        .checked_sub(min_val)
        .and_then(|d| d.checked_div(span))
        .unwrap_or(Decimal::ZERO);
    let scaled = (Decimal::from(10) + (norm * Decimal::from(80))).round();
    u32::try_from(scaled.max(Decimal::from(10)).min(Decimal::from(90)).trunc()).unwrap_or(50)
}

fn scale_vertical_coord(value: Decimal, min_val: Decimal, span: Decimal) -> u32 {
    if span <= Decimal::ZERO {
        return 50;
    }
    let norm = value
        .checked_sub(min_val)
        .and_then(|d| d.checked_div(span))
        .unwrap_or(Decimal::ZERO);
    let scaled = (Decimal::from(90) - (norm * Decimal::from(80))).round();
    u32::try_from(scaled.max(Decimal::from(10)).min(Decimal::from(90)).trunc()).unwrap_or(50)
}

fn bubble_radius_from_pct(ratio_pct: u32) -> f64 {
    let norm = (f64::from(ratio_pct.min(100)) / 100.0).clamp(0.05, 1.0);
    2.5 + norm.sqrt() * 8.5
}

fn decimal_quantile(sorted: &[Decimal], numerator: usize, denominator: usize) -> Decimal {
    if sorted.is_empty() {
        return Decimal::ZERO;
    }
    let idx = ((sorted.len().saturating_sub(1)) * numerator) / denominator.max(1);
    sorted[idx.min(sorted.len() - 1)]
}

fn gauge_arc_svg_path(pct: u32) -> Option<String> {
    let frac = (f64::from(pct.min(100)) / 100.0).clamp(0.02, 1.0);
    let start_rad = std::f64::consts::PI;
    let end_rad = std::f64::consts::PI - frac * std::f64::consts::PI;
    let x1 = 50.0 + 35.0 * start_rad.cos();
    let y1 = 68.0 - 35.0 * start_rad.sin();
    let x2 = 50.0 + 35.0 * end_rad.cos();
    let y2 = 68.0 - 35.0 * end_rad.sin();
    Some(format!("M {x1:.2} {y1:.2} A 35 35 0 0 1 {x2:.2} {y2:.2}"))
}

fn geo_region_coords(label: &str, fallback_idx: usize) -> (u32, u32) {
    let lower = label.trim().to_ascii_lowercase();
    if lower.contains("alex") || label.contains("الإسكندرية") || label.contains("اسكندرية")
    {
        (30, 22)
    } else if lower.contains("port said") || label.contains("بورسعيد") || lower.contains("دمياط")
    {
        (60, 20)
    } else if lower.contains("delta")
        || lower.contains("dakahl")
        || lower.contains("gharbia")
        || lower.contains("tanta")
        || label.contains("الدلتا")
        || label.contains("المنصورة")
        || label.contains("طنطا")
    {
        (48, 28)
    } else if lower.contains("sharq")
        || lower.contains("ismail")
        || lower.contains("suez")
        || label.contains("الشرقية")
        || label.contains("الإسماعيلية")
        || label.contains("السويس")
    {
        (64, 34)
    } else if lower.contains("cairo") || label.contains("القاهرة") {
        (52, 42)
    } else if lower.contains("giza") || label.contains("الجيزة") || label.contains("الفيوم")
    {
        (44, 46)
    } else if lower.contains("minya")
        || lower.contains("asyut")
        || lower.contains("upper")
        || label.contains("المنيا")
        || label.contains("أسيوط")
        || label.contains("الصعيد")
    {
        (46, 66)
    } else if lower.contains("luxor")
        || lower.contains("aswan")
        || label.contains("الأقصر")
        || label.contains("أسوان")
    {
        (56, 82)
    } else if lower.contains("red sea")
        || lower.contains("sinai")
        || label.contains("البحر الأحمر")
        || label.contains("سيناء")
    {
        (76, 54)
    } else {
        let mut hash: u32 = (fallback_idx as u32).wrapping_mul(37);
        for b in label.as_bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(u32::from(*b));
        }
        let x = 18 + (hash % 64);
        let y = 20 + ((hash / 64) % 60);
        (x, y)
    }
}

/// Returns `true` when two tables are byte-identical for Studio purposes.
#[must_use]
pub fn tables_equal(left: &Table, right: &Table) -> bool {
    table_fingerprint(left) == table_fingerprint(right)
}

#[cfg(test)]
mod tests;
