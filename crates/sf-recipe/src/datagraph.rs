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
    DatasetVersion, MetricId, MetricTokenParts, Revision, ScenarioId, SourceId,
    VerificationOutcome, VerificationStatus, VerifiedMetricToken,
};
use sf_core::error::{ErrorCode, ErrorSeverity, SafiError};
use sf_core::table::Table;
use sf_core::value::{CellValue, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

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
                        Some(Token::Identifier(column)) => column.clone(),
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
    function: AggFunc,
    column: &str,
) -> Result<(Decimal, Vec<u64>), SafiError> {
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
        non_empty += 1;
        if matches!(function, AggFunc::Count) {
            continue;
        }
        let number = cell_number(cell).ok_or_else(type_mismatch)?;
        numeric += 1;
        rows.push(row as u64);
        sum = sum.checked_add(number).ok_or_else(type_mismatch)?;
        extreme = Some(match (function, extreme) {
            (AggFunc::Min, Some(current)) => current.min(number),
            (AggFunc::Max, Some(current)) => current.max(number),
            _ => number,
        });
    }
    let value = match function {
        AggFunc::Count => {
            rows = (0..source.len() as u64)
                .filter(|row| {
                    source
                        .cell_at(*row as usize)
                        .is_some_and(|cell| !cell.is_empty())
                })
                .collect();
            Decimal::from(non_empty)
        }
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
    parameters: &'a BTreeMap<String, Decimal>,
    rows: BTreeSet<u64>,
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
        let (value, rows) = aggregate_column(self.table, function, column)?;
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
    /// Replace one cell of a source column.
    SetCell {
        /// Zero-based row.
        row: usize,
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
}

impl Transformation {
    fn columns(&self) -> BTreeSet<String> {
        match self {
            Self::SetCell { column, .. } => BTreeSet::from([column.clone()]),
            Self::AddColumn { name, .. } => BTreeSet::from([name.clone()]),
        }
    }

    fn digest(&self) -> String {
        let payload = match self {
            Self::SetCell { row, column, raw } => {
                format!(
                    "set:{row}:{column}:{}",
                    blake3::hash(raw.as_bytes()).to_hex()
                )
            }
            Self::AddColumn { name, formula } => format!("add:{name}:{formula}"),
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

fn apply_transformation(
    table: &Table,
    transformation: &Transformation,
) -> Result<Table, SafiError> {
    match transformation {
        Transformation::SetCell { row, column, raw } => {
            let index = table
                .columns()
                .iter()
                .position(|existing| existing.schema().name == *column)
                .ok_or_else(unknown_column)?;
            replace_cell(table, *row, index, CellValue::from_raw(raw.clone()))
        }
        Transformation::AddColumn { name, formula } => {
            let formula = parse_formula(formula)?;
            let cells = evaluate_column(table, &formula)?;
            append_column(table, name, cells)
        }
    }
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

impl DataGraph {
    /// Creates a graph over an immutable original dataset.
    ///
    /// # Errors
    /// Returns `SF-SYS-001` when column names are empty or duplicated.
    pub fn new(source_id: SourceId, origin: Table, now_ms: u64) -> Result<Self, SafiError> {
        unique_columns(&origin)?;
        let fingerprint = table_fingerprint(&origin);
        let working = origin.clone();
        let mut graph = Self {
            source_id,
            origin,
            origin_fingerprint: fingerprint,
            dataset_version: DatasetVersion::INITIAL,
            revision: Revision::INITIAL,
            working,
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
        self.rebuild_with(&self.origin)
    }

    /// Returns the dataset unchanged for `count` reversible steps. Used by tests to assert that
    /// undo/redo never touches the source.
    #[must_use]
    pub fn origin_unchanged(&self) -> bool {
        table_fingerprint(&self.origin) == self.origin_fingerprint
    }

    /// Applies a cell edit and recomputes only dependent metrics.
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
        if row >= self.working.row_count() {
            return Err(invalid_request());
        }
        let working = replace_cell(&self.working, row, index, CellValue::from_raw(raw))?;
        self.working = working;
        self.transforms.push(Transformation::SetCell {
            row,
            column: column.to_owned(),
            raw: raw.to_owned(),
        });
        self.redo.clear();
        self.revision = self.revision.next();
        let recomputed = self.propagate(&BTreeSet::from([column.to_owned()]), now_ms)?;
        self.last_rows = vec![row as u64];
        Ok(ChangeOutcome {
            revision: self.revision,
            recomputed,
            rows: vec![row as u64],
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
        if self.scenarios.contains_key(&id) {
            return Err(invalid_request());
        }
        let _ = self.scenarios.insert(
            id.clone(),
            Scenario {
                id,
                name: name.to_owned(),
                parameters,
            },
        );
        self.revision = self.revision.next();
        Ok(())
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
    /// # Errors
    /// `SF-CAL-006` for an unknown metric, plus evaluation errors.
    pub fn verify(&self, token: &VerifiedMetricToken) -> Result<VerificationOutcome, SafiError> {
        let definition = self
            .metrics
            .get(&token.metric_id)
            .ok_or_else(unknown_metric)?;
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
        let replayed = self.rebuild_with(&origin)?;
        self.origin = origin;
        self.origin_fingerprint = table_fingerprint(&self.origin);
        self.working = replayed;
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
        let working = apply_transformation(&self.working, &transformation)?;
        if let Transformation::AddColumn { name, formula } = &transformation {
            let parsed = parse_formula(formula)?;
            self.calculated.push((name.clone(), parsed));
        }
        self.working = working;
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

    /// Rebuilds the derived table: source edits are replayed on the immutable original, then
    /// every calculated column is re-derived in creation order. Calculated columns are derived
    /// state, not point-in-time patches, which is what makes `replay() == working()` hold.

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
        let mut buckets: BTreeMap<String, (Decimal, u64, Vec<u64>)> = BTreeMap::new();
        for row in 0..self.working.row_count() {
            let label = group_column
                .cell_at(row)
                .map(|cell| cell.visible_text().to_owned())
                .unwrap_or_default();
            let entry = match buckets.get(&label) {
                Some(existing) => existing.clone(),
                None => {
                    order.push(label.clone());
                    (Decimal::ZERO, 0, Vec::new())
                }
            };
            let (mut sum, mut count, mut rows) = entry;
            let cell = value_column.cell_at(row).ok_or_else(invalid_request)?;
            if !cell.is_empty() {
                if matches!(function, AggFunc::Count) {
                    count += 1;
                } else {
                    let number = cell_number(cell).ok_or_else(type_mismatch)?;
                    sum = sum.checked_add(number).ok_or_else(type_mismatch)?;
                    count += 1;
                }
                rows.push(row as u64);
            }
            let _ = buckets.insert(label, (sum, count, rows));
        }
        let mut groups = Vec::with_capacity(order.len());
        for label in order {
            let (sum, count, rows) = buckets.remove(&label).ok_or_else(invalid_request)?;
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
                    for row in &rows {
                        let cell = value_column
                            .cell_at(*row as usize)
                            .ok_or_else(invalid_request)?;
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
                row_refs: rows,
            });
        }
        Ok(groups)
    }

    fn rebuild_with(&self, origin: &Table) -> Result<Table, SafiError> {
        let mut table = origin.clone();
        for transformation in &self.transforms {
            if matches!(transformation, Transformation::SetCell { .. }) {
                table = apply_transformation(&table, transformation)?;
            }
        }
        for (name, formula) in &self.calculated {
            let cells = evaluate_column(&table, formula)?;
            table = append_column(&table, name, cells)?;
        }
        Ok(table)
    }

    fn rebuild_state(&mut self) -> Result<(), SafiError> {
        self.working = self.replay()?;
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
            .filter(|transformation| !transformation.columns().is_disjoint(columns))
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
            parameters,
            rows: BTreeSet::new(),
            columns: BTreeSet::new(),
        };
        let value = evaluate(&definition.formula, &mut context)?;
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
            row_refs: context.rows.into_iter().collect(),
            column_refs: definition.columns.iter().cloned().collect(),
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
}

/// Schema version of a persisted Studio project.
pub const PROJECT_SCHEMA_VERSION: u32 = 1;

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

/// Returns `true` when two tables are byte-identical for Studio purposes.
#[must_use]
pub fn tables_equal(left: &Table, right: &Table) -> bool {
    table_fingerprint(left) == table_fingerprint(right)
}

#[cfg(test)]
mod tests;
