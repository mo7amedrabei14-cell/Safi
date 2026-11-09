//! Authentication failures rendered as RFC 9457 `problem+json` with a canonical
//! `errors.toml` code (Section 9.3, 15.4). No credential material ever reaches
//! the response body or the logs (AGENTS.md Rule 4).

use axum::{
    http::{header::CONTENT_TYPE, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

/// Every failure mode of the API-key/session layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthError {
    /// Missing, malformed, or wrong credential.
    InvalidCredentials,
    /// Credential is past its expiry.
    Expired,
    /// Credential was explicitly revoked.
    Revoked,
    /// Authenticated, but the credential lacks the required scope.
    InsufficientScope,
    /// PostgreSQL or Valkey is unreachable.
    BackendUnavailable,
    /// Request body or account field is malformed.
    InvalidRequest,
    /// Email-verification token is malformed, unknown, expired, or already consumed.
    InvalidVerificationToken,
}

impl AuthError {
    /// Canonical code from `errors.toml`.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidCredentials => "SF-AUTH-001",
            Self::Expired => "SF-AUTH-002",
            Self::Revoked => "SF-AUTH-003",
            Self::InsufficientScope => "SF-AUTH-004",
            Self::BackendUnavailable => "SF-AUTH-005",
            Self::InvalidRequest => "SF-AUTH-006",
            Self::InvalidVerificationToken => "SF-AUTH-007",
        }
    }

    /// HTTP status that matches the failure mode.
    #[must_use]
    pub const fn status(self) -> StatusCode {
        match self {
            Self::InvalidCredentials | Self::Expired | Self::Revoked => StatusCode::UNAUTHORIZED,
            Self::InsufficientScope => StatusCode::FORBIDDEN,
            Self::BackendUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::InvalidRequest | Self::InvalidVerificationToken => {
                StatusCode::UNPROCESSABLE_ENTITY
            }
        }
    }

    /// Arabic owner-facing title; the full action text lives in `errors.toml`.
    #[must_use]
    pub const fn title_ar(self) -> &'static str {
        match self {
            Self::InvalidCredentials => "فشل التحقق من الهوية",
            Self::Expired => "انتهت صلاحية الدخول",
            Self::Revoked => "تم إبطال بيانات الدخول",
            Self::InsufficientScope => "ليس لديك صلاحية لهذا الإجراء",
            Self::BackendUnavailable => "خدمة الدخول غير متاحة مؤقتًا",
            Self::InvalidRequest => "طلب الدخول غير صالح",
            Self::InvalidVerificationToken => "رابط التحقق غير صالح أو انتهت صلاحيته",
        }
    }
}

#[derive(Debug, Serialize)]
struct ProblemDetails {
    #[serde(rename = "type")]
    problem_type: &'static str,
    title: &'static str,
    status: u16,
    code: &'static str,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let body = ProblemDetails {
            problem_type: "about:blank",
            title: self.title_ar(),
            status: self.status().as_u16(),
            code: self.code(),
        };
        let mut response = (self.status(), Json(body)).into_response();
        response.headers_mut().insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        response
    }
}
