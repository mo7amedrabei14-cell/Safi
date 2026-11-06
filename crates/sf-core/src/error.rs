//! Core error model (`SafiError`, `ErrorCode`, `ErrorDomain`, `ErrorSeverity`)
//! and privacy-preserving structured log records (Specification Sections 15.1, 15.3, 15.4).
//!
//! Hard Rule #4 & #9 (`AGENTS.md`):
//! - Every error carries a canonical `SF-<DOMAIN>-<NNN>` code.
//! - No silent failures.
//! - User data NEVER enters errors or logs: no cell values, no file names, and no column names.

use std::fmt;
use std::str::FromStr;

/// The 15 canonical error domains defined in Specification Section 15.4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ErrorDomain {
    /// `IMP` — استيراد (Import)
    Imp,
    /// `PRF` — تعرّف (Profiling & Type Inference)
    Prf,
    /// `CLN` — تنضيف (Cleaning)
    Cln,
    /// `CAL` — حساب (Calculation)
    Cal,
    /// `EXP` — تصدير (Export)
    Exp,
    /// `RCP` — وصفات (Recipes)
    Rcp,
    /// `CHT` — رسوم (Charts)
    Cht,
    /// `AUTH` — دخول (Authentication)
    Auth,
    /// `LIC` — ترخيص (Licensing)
    Lic,
    /// `PAY` — دفع (Billing & Payments)
    Pay,
    /// `SYN` — مزامنة (Encrypted Sync)
    Syn,
    /// `UPD` — تحديث (Updates)
    Upd,
    /// `AIX` — ذكاء اصطناعي (AI Proxy / Local AI)
    Aix,
    /// `SYS` — نظام (System / Storage / Memory)
    Sys,
    /// `UNK` — غير متوقع (Uncaught / Unknown Fallback)
    Unk,
}

impl ErrorDomain {
    /// All 15 canonical error domains in specification order (Section 15.4).
    pub const ALL: [Self; 15] = [
        Self::Imp,
        Self::Prf,
        Self::Cln,
        Self::Cal,
        Self::Exp,
        Self::Rcp,
        Self::Cht,
        Self::Auth,
        Self::Lic,
        Self::Pay,
        Self::Syn,
        Self::Upd,
        Self::Aix,
        Self::Sys,
        Self::Unk,
    ];

    /// Canonical uppercase ASCII code used in `SF-<DOMAIN>-<NNN>`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Imp => "IMP",
            Self::Prf => "PRF",
            Self::Cln => "CLN",
            Self::Cal => "CAL",
            Self::Exp => "EXP",
            Self::Rcp => "RCP",
            Self::Cht => "CHT",
            Self::Auth => "AUTH",
            Self::Lic => "LIC",
            Self::Pay => "PAY",
            Self::Syn => "SYN",
            Self::Upd => "UPD",
            Self::Aix => "AIX",
            Self::Sys => "SYS",
            Self::Unk => "UNK",
        }
    }

    /// Arabic domain label from Specification Section 15.4.
    #[must_use]
    pub const fn label_ar(self) -> &'static str {
        match self {
            Self::Imp => "استيراد",
            Self::Prf => "تعرّف",
            Self::Cln => "تنضيف",
            Self::Cal => "حساب",
            Self::Exp => "تصدير",
            Self::Rcp => "وصفات",
            Self::Cht => "رسوم",
            Self::Auth => "دخول",
            Self::Lic => "ترخيص",
            Self::Pay => "دفع",
            Self::Syn => "مزامنة",
            Self::Upd => "تحديث",
            Self::Aix => "ذكاء اصطناعي",
            Self::Sys => "نظام",
            Self::Unk => "غير متوقع",
        }
    }
}

impl fmt::Display for ErrorDomain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ErrorDomain {
    type Err = ErrorCodeParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "IMP" => Ok(Self::Imp),
            "PRF" => Ok(Self::Prf),
            "CLN" => Ok(Self::Cln),
            "CAL" => Ok(Self::Cal),
            "EXP" => Ok(Self::Exp),
            "RCP" => Ok(Self::Rcp),
            "CHT" => Ok(Self::Cht),
            "AUTH" => Ok(Self::Auth),
            "LIC" => Ok(Self::Lic),
            "PAY" => Ok(Self::Pay),
            "SYN" => Ok(Self::Syn),
            "UPD" => Ok(Self::Upd),
            "AIX" => Ok(Self::Aix),
            "SYS" => Ok(Self::Sys),
            "UNK" => Ok(Self::Unk),
            _ => Err(ErrorCodeParseError::UnknownDomain),
        }
    }
}

/// Severity and log level aligned with Specification Section 15.3 and `errors.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ErrorSeverity {
    /// `trace` — disabled in production by default (Section 15.3).
    Trace,
    /// `debug` — disabled in production by default unless temporarily enabled for a slice (Section 15.3, O6).
    Debug,
    /// `info` — business events and low-confidence informational prompts (Section 15.3).
    Info,
    /// `warn` — handled anomalies or ambiguous user inputs requiring confirmation (Section 15.3).
    Warn,
    /// `error` — failed operation requiring attention (Section 15.3).
    Error,
    /// `fatal` — process-stopping failure that alerts the owner immediately (Section 15.3).
    Fatal,
}

impl ErrorSeverity {
    /// Canonical lowercase string representation used in JSON logs (`level`) and `errors.toml` (`severity`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
            Self::Fatal => "fatal",
        }
    }

    /// Canonical uppercase level name (`TRACE`, `DEBUG`, `INFO`, `WARN`, `ERROR`, `FATAL`).
    #[must_use]
    pub const fn as_uppercase_str(self) -> &'static str {
        match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
            Self::Fatal => "FATAL",
        }
    }

    /// Returns `true` if this level is active in production by default (Section 15.3:
    /// `TRACE` and `DEBUG` are disabled in production).
    #[must_use]
    pub const fn is_enabled_in_production_by_default(self) -> bool {
        !matches!(self, Self::Trace | Self::Debug)
    }
}

impl fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ErrorSeverity {
    type Err = ErrorCodeParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("trace") {
            Ok(Self::Trace)
        } else if s.eq_ignore_ascii_case("debug") {
            Ok(Self::Debug)
        } else if s.eq_ignore_ascii_case("info") {
            Ok(Self::Info)
        } else if s.eq_ignore_ascii_case("warn") || s.eq_ignore_ascii_case("warning") {
            Ok(Self::Warn)
        } else if s.eq_ignore_ascii_case("error") {
            Ok(Self::Error)
        } else if s.eq_ignore_ascii_case("fatal") {
            Ok(Self::Fatal)
        } else {
            Err(ErrorCodeParseError::InvalidSeverity)
        }
    }
}

/// Structural parse or validation error when constructing an [`ErrorCode`] or [`ErrorSeverity`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCodeParseError {
    /// Code does not follow `SF-<DOMAIN>-<NNN>`.
    InvalidFormat,
    /// Prefix is not `SF`.
    InvalidPrefix,
    /// Domain segment is not one of the 15 canonical domains in Section 15.4.
    UnknownDomain,
    /// Numeric suffix is not a 3-digit number in `001..=999`.
    InvalidNumber,
    /// Severity string is not one of `trace|debug|info|warn|error|fatal`.
    InvalidSeverity,
}

impl fmt::Display for ErrorCodeParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFormat => f.write_str("error code must follow SF-<DOMAIN>-<NNN>"),
            Self::InvalidPrefix => f.write_str("error code must start with 'SF-'"),
            Self::UnknownDomain => f.write_str("unknown error domain"),
            Self::InvalidNumber => f.write_str("error number must be a 3-digit value in 001..=999"),
            Self::InvalidSeverity => f.write_str("invalid error severity level"),
        }
    }
}

impl std::error::Error for ErrorCodeParseError {}

/// Validated error code in the canonical `SF-<DOMAIN>-<NNN>` format (Section 15.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ErrorCode {
    pub(crate) domain: ErrorDomain,
    pub(crate) number: u16,
}

impl ErrorCode {
    /// Minimum valid error number (`001`).
    pub const MIN_NUMBER: u16 = 1;
    /// Maximum valid error number (`999`).
    pub const MAX_NUMBER: u16 = 999;

    /// Constructs a validated `ErrorCode` from a domain and a number in `1..=999`.
    pub const fn new(domain: ErrorDomain, number: u16) -> Result<Self, ErrorCodeParseError> {
        if number < Self::MIN_NUMBER || number > Self::MAX_NUMBER {
            return Err(ErrorCodeParseError::InvalidNumber);
        }
        Ok(Self { domain, number })
    }

    /// Returns the error domain (`IMP`, `PRF`, `CLN`, ...).
    #[must_use]
    pub const fn domain(self) -> ErrorDomain {
        self.domain
    }

    /// Returns the numeric code within the domain (`1..=999`).
    #[must_use]
    pub const fn number(self) -> u16 {
        self.number
    }

    /// Formats the error code as `SF-<DOMAIN>-<NNN>`.
    #[must_use]
    pub fn as_code_string(self) -> String {
        format!("SF-{}-{:03}", self.domain.as_str(), self.number)
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SF-{}-{:03}", self.domain.as_str(), self.number)
    }
}

impl FromStr for ErrorCode {
    type Err = ErrorCodeParseError;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let mut parts = raw.split('-');
        let prefix = parts.next().ok_or(ErrorCodeParseError::InvalidFormat)?;
        let domain_str = parts.next().ok_or(ErrorCodeParseError::InvalidFormat)?;
        let number_str = parts.next().ok_or(ErrorCodeParseError::InvalidFormat)?;
        if parts.next().is_some() {
            return Err(ErrorCodeParseError::InvalidFormat);
        }
        if prefix != "SF" {
            return Err(ErrorCodeParseError::InvalidPrefix);
        }
        let domain = ErrorDomain::from_str(domain_str)?;
        if number_str.len() != 3 || !number_str.bytes().all(|b| b.is_ascii_digit()) {
            return Err(ErrorCodeParseError::InvalidNumber);
        }
        let number = number_str
            .parse::<u16>()
            .map_err(|_| ErrorCodeParseError::InvalidNumber)?;
        Self::new(domain, number)
    }
}

/// Short alphanumeric diagnostic token displayed to the user alongside an error code
/// (e.g. `7K2Q9` in `SF-IMP-014 · 7K2Q9`, Section 15.5).
///
/// Restricted to 3..=16 ASCII uppercase alphanumeric characters so arbitrary user strings,
/// file paths, or cell contents can never be smuggled into a diagnostic token.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DiagnosticToken(String);

impl DiagnosticToken {
    /// Validates and constructs a short diagnostic token (`3..=16` ASCII alphanumeric chars).
    pub fn new(raw: &str) -> Option<Self> {
        if !(3..=16).contains(&raw.len()) {
            return None;
        }
        if !raw
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        {
            return None;
        }
        Some(Self(raw.to_owned()))
    }

    /// Returns the validated token slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DiagnosticToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Safe numeric metrics attached to an engine error or log event (Sections 15.2, 15.3).
/// Contains ONLY aggregate counts and durations — never cell values, file names, or column names.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SafeLogMetrics {
    /// Number of rows read or processed (`rows_read`).
    pub rows_read: Option<u64>,
    /// Number of columns inspected or affected (`columns_count`).
    pub columns_count: Option<u32>,
    /// Number of cells affected (`cells_affected`).
    pub cells_affected: Option<u64>,
    /// Operation duration in milliseconds (`duration_ms`).
    pub duration_ms: Option<u64>,
}

/// Core engine error structure enforcing privacy-by-design (`AGENTS.md` Rules 4 & 9, Sections 15.1–15.5).
///
/// Intentionally holds NO free-form user message, NO file name, NO column name, and NO cell value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafiError {
    code: ErrorCode,
    severity: ErrorSeverity,
    error_class: &'static str,
    event: &'static str,
    diagnostic_token: Option<DiagnosticToken>,
    metrics: SafeLogMetrics,
}

impl SafiError {
    /// Creates a new `SafiError` with a canonical `ErrorCode`, `ErrorSeverity`, static `error_class`,
    /// and static `event` name (e.g. `"import.failed"`).
    #[must_use]
    pub const fn new(
        code: ErrorCode,
        severity: ErrorSeverity,
        error_class: &'static str,
        event: &'static str,
    ) -> Self {
        Self {
            code,
            severity,
            error_class,
            event,
            diagnostic_token: None,
            metrics: SafeLogMetrics {
                rows_read: None,
                columns_count: None,
                cells_affected: None,
                duration_ms: None,
            },
        }
    }

    /// Attaches a validated short diagnostic token (e.g. `7K2Q9`).
    #[must_use]
    pub fn with_diagnostic_token(mut self, token: DiagnosticToken) -> Self {
        self.diagnostic_token = Some(token);
        self
    }

    /// Attaches safe aggregate numeric metrics (`rows_read`, `columns_count`, `cells_affected`, `duration_ms`).
    #[must_use]
    pub const fn with_metrics(mut self, metrics: SafeLogMetrics) -> Self {
        self.metrics = metrics;
        self
    }

    /// Returns the `SF-<DOMAIN>-<NNN>` error code.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        self.code
    }

    /// Returns the error domain (`IMP`, `PRF`, `CLN`, ...).
    #[must_use]
    pub const fn domain(&self) -> ErrorDomain {
        self.code.domain()
    }

    /// Returns the error severity level.
    #[must_use]
    pub const fn severity(&self) -> ErrorSeverity {
        self.severity
    }

    /// Returns the static error classification identifier (e.g. `"UnsupportedEncoding"`).
    #[must_use]
    pub const fn error_class(&self) -> &'static str {
        self.error_class
    }

    /// Returns the static event identifier (e.g. `"import.failed"`).
    #[must_use]
    pub const fn event(&self) -> &'static str {
        self.event
    }

    /// Returns the optional short diagnostic token.
    #[must_use]
    pub const fn diagnostic_token(&self) -> Option<&DiagnosticToken> {
        self.diagnostic_token.as_ref()
    }

    /// Returns the safe numeric metrics.
    #[must_use]
    pub const fn metrics(&self) -> SafeLogMetrics {
        self.metrics
    }

    /// Formats the user-visible diagnostic badge per Section 15.5 (`SF-IMP-014 · 7K2Q9` or `SF-IMP-014`).
    #[must_use]
    pub fn user_diagnostic_badge(&self) -> String {
        match &self.diagnostic_token {
            Some(tok) => format!("{} · {}", self.code, tok),
            None => self.code.as_code_string(),
        }
    }

    /// Converts this error into a unified `SafeEngineLogRecord` (Section 15.3) ready for JSON emission.
    #[must_use]
    pub fn to_log_record(&self, context: SafeLogContext<'_>) -> SafeEngineLogRecord {
        SafeEngineLogRecord {
            timestamp_utc: context.timestamp_utc.to_owned(),
            level: self.severity,
            service: context.service,
            env: context.env,
            version: context.version.to_owned(),
            request_id: context.request_id.to_owned(),
            trace_id: context.trace_id.map(str::to_owned),
            user_id: context.user_id.map(str::to_owned),
            workspace_id: context.workspace_id.map(str::to_owned),
            device_id: context.device_id.map(str::to_owned),
            event: self.event,
            error_code: Some(self.code),
            error_class: Some(self.error_class),
            metrics: self.metrics,
            platform: context.platform,
            app_version: context.app_version.map(str::to_owned),
            locale: context.locale.to_owned(),
        }
    }
}

impl fmt::Display for SafiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {} ({}, severity={})",
            self.code, self.event, self.error_class, self.severity
        )
    }
}

impl std::error::Error for SafiError {}

/// Safe execution metadata passed when constructing a [`SafeEngineLogRecord`] (Section 15.3).
/// All identifiers (`user_id`, `workspace_id`, `device_id`) must be opaque/hashed IDs, never emails or names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SafeLogContext<'a> {
    /// UTC timestamp in ISO-8601 with milliseconds (e.g. `"2026-10-29T12:41:07.215Z"`).
    pub timestamp_utc: &'a str,
    /// Service name (e.g. `"engine"`).
    pub service: &'static str,
    /// Deployment environment (e.g. `"prod"`, `"staging"`, `"local"`).
    pub env: &'static str,
    /// Version + Git hash (e.g. `"0.1.0+a3f91c2"`).
    pub version: &'a str,
    /// Correlation request identifier (e.g. `"req_9f2a1c"`).
    pub request_id: &'a str,
    /// Optional OpenTelemetry trace ID.
    pub trace_id: Option<&'a str>,
    /// Optional opaque user identifier (e.g. `"u_4821"`).
    pub user_id: Option<&'a str>,
    /// Optional opaque workspace identifier (e.g. `"ws_104"`).
    pub workspace_id: Option<&'a str>,
    /// Optional hashed device identifier.
    pub device_id: Option<&'a str>,
    /// Execution platform (e.g. `"web"`, `"windows"`, `"linux"`, `"cli"`).
    pub platform: &'static str,
    /// Optional application version string.
    pub app_version: Option<&'a str>,
    /// Active UI locale (e.g. `"ar-EG"`).
    pub locale: &'a str,
}

/// Unified structured engine log record conforming to Specification Section 15.3.
///
/// Guarantees zero customer data fields: no cell values, no file names, and no column names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafeEngineLogRecord {
    /// UTC ISO-8601 timestamp (`timestamp`).
    pub timestamp_utc: String,
    /// Log severity level (`level`).
    pub level: ErrorSeverity,
    /// Service identifier (`service`).
    pub service: &'static str,
    /// Environment identifier (`env`).
    pub env: &'static str,
    /// Semantic version + git commit (`version`).
    pub version: String,
    /// Unified request identifier (`request_id`).
    pub request_id: String,
    /// Optional trace identifier (`trace_id`).
    pub trace_id: Option<String>,
    /// Optional opaque user identifier (`user_id`).
    pub user_id: Option<String>,
    /// Optional opaque workspace identifier (`workspace_id`).
    pub workspace_id: Option<String>,
    /// Optional hashed device identifier (`device_id`).
    pub device_id: Option<String>,
    /// Static event name such as `"import.failed"` (`event`).
    pub event: &'static str,
    /// Optional canonical `SF-<DOMAIN>-<NNN>` error code (`error_code`).
    pub error_code: Option<ErrorCode>,
    /// Optional static error class name (`error_class`).
    pub error_class: Option<&'static str>,
    /// Aggregate numeric metrics (`rows_read`, `columns_count`, `cells_affected`, `duration_ms`).
    pub metrics: SafeLogMetrics,
    /// Runtime platform (`platform`).
    pub platform: &'static str,
    /// Optional client app version (`app_version`).
    pub app_version: Option<String>,
    /// Active locale (`locale`).
    pub locale: String,
}

impl SafeEngineLogRecord {
    /// Serializes the record into a single-line deterministic JSON string matching Specification Section 15.3.
    #[must_use]
    pub fn to_json_line(&self) -> String {
        let mut out = String::with_capacity(256);
        out.push('{');
        push_json_str_field(&mut out, "timestamp", &self.timestamp_utc, true);
        push_json_str_field(&mut out, "level", self.level.as_str(), false);
        push_json_str_field(&mut out, "service", self.service, false);
        push_json_str_field(&mut out, "env", self.env, false);
        push_json_str_field(&mut out, "version", &self.version, false);
        push_json_str_field(&mut out, "request_id", &self.request_id, false);
        if let Some(trace_id) = &self.trace_id {
            push_json_str_field(&mut out, "trace_id", trace_id, false);
        }
        if let Some(user_id) = &self.user_id {
            push_json_str_field(&mut out, "user_id", user_id, false);
        }
        if let Some(workspace_id) = &self.workspace_id {
            push_json_str_field(&mut out, "workspace_id", workspace_id, false);
        }
        if let Some(device_id) = &self.device_id {
            push_json_str_field(&mut out, "device_id", device_id, false);
        }
        push_json_str_field(&mut out, "event", self.event, false);
        if let Some(code) = self.error_code {
            push_json_str_field(&mut out, "error_code", &code.as_code_string(), false);
        }
        if let Some(error_class) = self.error_class {
            push_json_str_field(&mut out, "error_class", error_class, false);
        }
        if let Some(rows_read) = self.metrics.rows_read {
            push_json_u64_field(&mut out, "rows_read", rows_read);
        }
        if let Some(columns_count) = self.metrics.columns_count {
            push_json_u64_field(&mut out, "columns_count", u64::from(columns_count));
        }
        if let Some(cells_affected) = self.metrics.cells_affected {
            push_json_u64_field(&mut out, "cells_affected", cells_affected);
        }
        if let Some(duration_ms) = self.metrics.duration_ms {
            push_json_u64_field(&mut out, "duration_ms", duration_ms);
        }
        push_json_str_field(&mut out, "platform", self.platform, false);
        if let Some(app_version) = &self.app_version {
            push_json_str_field(&mut out, "app_version", app_version, false);
        }
        push_json_str_field(&mut out, "locale", &self.locale, false);
        out.push('}');
        out
    }
}

fn push_json_str_field(out: &mut String, key: &str, value: &str, is_first: bool) {
    if !is_first {
        out.push(',');
    }
    out.push('"');
    escape_json_into(out, key);
    out.push_str("\":\"");
    escape_json_into(out, value);
    out.push('"');
}

fn push_json_u64_field(out: &mut String, key: &str, value: u64) {
    out.push_str(",\"");
    escape_json_into(out, key);
    out.push_str("\":");
    out.push_str(&value.to_string());
}

fn escape_json_into(out: &mut String, raw: &str) {
    for ch in raw.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
}
