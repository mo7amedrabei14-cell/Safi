//! Platform capacity and resource limit constants (Specification Sections 18.1 `ن11` & 20.4 #8).
//!
//! - Web (PWA via OPFS streaming): up to `250 MB` or `500,000` rows.
//! - Desktop (Tauri) and CLI (via Polars streaming): up to `10 GB` or `10,000,000` rows.

use crate::error::{ErrorCode, ErrorSeverity, SafeLogMetrics, SafiError};

/// One binary megabyte (`1024 * 1024` bytes).
pub const ONE_MIB_BYTES: u64 = 1024 * 1024;

/// One binary gigabyte (`1024 * 1024 * 1024` bytes).
pub const ONE_GIB_BYTES: u64 = 1024 * 1024 * 1024;

/// Maximum input file size on Web PWA (`250 MB` = `262,144,000` bytes — Section 20.4 #8).
pub const WEB_PWA_MAX_FILE_BYTES: u64 = 250 * ONE_MIB_BYTES;

/// Maximum row count on Web PWA (`500,000` rows — Section 20.4 #8).
pub const WEB_PWA_MAX_ROWS: u64 = 500_000;

/// Maximum input file size on Desktop (Tauri) and CLI (`10 GB` = `10,737,418,240` bytes — Section 20.4 #8).
pub const DESKTOP_CLI_MAX_FILE_BYTES: u64 = 10 * ONE_GIB_BYTES;

/// Maximum row count on Desktop (Tauri) and CLI (`10,000,000` rows — Section 20.4 #8).
pub const DESKTOP_CLI_MAX_ROWS: u64 = 10_000_000;

/// Target execution platform for the SafiData engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlatformTarget {
    /// Browser PWA running via WASM Web Worker and OPFS (`250 MB` / `500,000` rows).
    WebPwa,
    /// Native Desktop application via Tauri 2.2 (`10 GB` / `10,000,000` rows).
    Desktop,
    /// Native command-line interface (`10 GB` / `10,000,000` rows).
    Cli,
}

impl PlatformTarget {
    /// Canonical lowercase platform identifier used in structured logs (Section 15.3).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WebPwa => "web",
            Self::Desktop => "desktop",
            Self::Cli => "cli",
        }
    }

    /// Returns the locked capacity limits for this platform (Section 20.4 #8).
    #[must_use]
    pub const fn limits(self) -> PlatformLimits {
        match self {
            Self::WebPwa => PlatformLimits::WEB_PWA,
            Self::Desktop | Self::Cli => PlatformLimits::DESKTOP_CLI,
        }
    }
}

/// File size and row count limits for a target platform (Section 20.4 #8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlatformLimits {
    /// Maximum allowed file size in bytes.
    pub max_file_bytes: u64,
    /// Maximum allowed number of rows.
    pub max_rows: u64,
}

impl PlatformLimits {
    /// Locked limits for Web PWA (`250 MB` or `500,000` rows).
    pub const WEB_PWA: Self = Self {
        max_file_bytes: WEB_PWA_MAX_FILE_BYTES,
        max_rows: WEB_PWA_MAX_ROWS,
    };

    /// Locked limits for Desktop and CLI (`10 GB` or `10,000,000` rows).
    pub const DESKTOP_CLI: Self = Self {
        max_file_bytes: DESKTOP_CLI_MAX_FILE_BYTES,
        max_rows: DESKTOP_CLI_MAX_ROWS,
    };

    /// Validates that `file_bytes` and `row_count` do not exceed this platform's limits.
    /// Returns `Err(SafiError)` with code `SF-IMP-003` (`errors.toml`) if either bound is exceeded.
    pub const fn validate(self, file_bytes: u64, row_count: u64) -> Result<(), SafiError> {
        if file_bytes > self.max_file_bytes {
            return Err(SafiError::new(
                ErrorCode::SF_IMP_003,
                ErrorSeverity::Error,
                "PlatformFileSizeLimitExceeded",
                "import.limit_exceeded",
            )
            .with_metrics(SafeLogMetrics {
                rows_read: Some(row_count),
                columns_count: None,
                cells_affected: None,
                duration_ms: None,
            }));
        }
        if row_count > self.max_rows {
            return Err(SafiError::new(
                ErrorCode::SF_IMP_003,
                ErrorSeverity::Error,
                "PlatformRowCountLimitExceeded",
                "import.limit_exceeded",
            )
            .with_metrics(SafeLogMetrics {
                rows_read: Some(row_count),
                columns_count: None,
                cells_affected: None,
                duration_ms: None,
            }));
        }
        Ok(())
    }
}
