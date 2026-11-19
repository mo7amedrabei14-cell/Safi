//! Server error taxonomy rendered as RFC 9457 `problem+json` with canonical
//! `errors.toml` codes (Section 9.3, 15.4). No credential material ever reaches
//! the response body or logs (AGENTS.md Rule 4).

use axum::{
    http::{header::CONTENT_TYPE, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

/// Every canonical server failure mode mapped 1:1 to `errors.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthError {
    InvalidCredentials,
    Expired,
    Revoked,
    InsufficientScope,
    BackendUnavailable,
    InvalidRequest,
    InvalidVerificationToken,
    DuplicateAccount,
    AccountSuspended,
    RateLimited,
    InvalidPhoneOtp,
    OAuthNotConfigured,
    OwnerRequired,
    OwnerMfaRequired,
    OwnerMfaLocked,
    OwnerStepUpRequired,
    OwnerBootstrapSealed,
    OwnerTotpKeyMissing,
    QuotaExceeded,
    DatasetLimitExceeded,
    AiQuotaExceeded,
    PaidPlanRequired,
    BillingNotConfigured,
    InvalidWebhookSignature,
    WebhookReplayWindow,
    BillingOperationFailed,
    SyncEnvelopeInvalid,
    SyncVersionConflict,
    ShareLinkInvalid,
    OrgMinSeatsRequired,
    OrgPermissionDenied,
    OrgSsoNotConfigured,
    AiProviderNotConfigured,
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
            Self::DuplicateAccount => "SF-AUTH-008",
            Self::AccountSuspended => "SF-AUTH-009",
            Self::RateLimited => "SF-AUTH-010",
            Self::InvalidPhoneOtp => "SF-AUTH-011",
            Self::OAuthNotConfigured => "SF-AUTH-012",
            Self::OwnerRequired => "SF-AUTH-013",
            Self::OwnerMfaRequired => "SF-AUTH-014",
            Self::OwnerMfaLocked => "SF-AUTH-015",
            Self::OwnerStepUpRequired => "SF-AUTH-016",
            Self::OwnerBootstrapSealed => "SF-AUTH-017",
            Self::OwnerTotpKeyMissing => "SF-AUTH-018",
            Self::QuotaExceeded => "SF-LIC-005",
            Self::DatasetLimitExceeded => "SF-LIC-006",
            Self::AiQuotaExceeded => "SF-LIC-007",
            Self::PaidPlanRequired => "SF-LIC-008",
            Self::BillingNotConfigured => "SF-PAY-001",
            Self::InvalidWebhookSignature => "SF-PAY-002",
            Self::WebhookReplayWindow => "SF-PAY-003",
            Self::BillingOperationFailed => "SF-PAY-004",
            Self::SyncEnvelopeInvalid => "SF-SYN-001",
            Self::SyncVersionConflict => "SF-SYN-002",
            Self::ShareLinkInvalid => "SF-SYN-003",
            Self::OrgMinSeatsRequired => "SF-SYN-004",
            Self::OrgPermissionDenied => "SF-SYN-005",
            Self::OrgSsoNotConfigured => "SF-SYN-006",
            Self::AiProviderNotConfigured => "SF-AIX-001",
        }
    }

    /// HTTP status that matches the failure mode.
    #[must_use]
    pub const fn status(self) -> StatusCode {
        match self {
            Self::InvalidCredentials
            | Self::Expired
            | Self::Revoked
            | Self::OwnerMfaRequired
            | Self::OwnerStepUpRequired
            | Self::InvalidWebhookSignature => StatusCode::UNAUTHORIZED,
            Self::InsufficientScope
            | Self::AccountSuspended
            | Self::OwnerRequired
            | Self::PaidPlanRequired
            | Self::OrgPermissionDenied => StatusCode::FORBIDDEN,
            Self::BackendUnavailable
            | Self::OAuthNotConfigured
            | Self::OwnerTotpKeyMissing
            | Self::BillingNotConfigured
            | Self::OrgSsoNotConfigured
            | Self::AiProviderNotConfigured => StatusCode::SERVICE_UNAVAILABLE,
            Self::InvalidRequest
            | Self::InvalidVerificationToken
            | Self::InvalidPhoneOtp
            | Self::WebhookReplayWindow
            | Self::BillingOperationFailed
            | Self::SyncEnvelopeInvalid
            | Self::OrgMinSeatsRequired
            | Self::DatasetLimitExceeded => StatusCode::UNPROCESSABLE_ENTITY,
            Self::DuplicateAccount | Self::OwnerBootstrapSealed | Self::SyncVersionConflict => {
                StatusCode::CONFLICT
            }
            Self::RateLimited
            | Self::OwnerMfaLocked
            | Self::QuotaExceeded
            | Self::AiQuotaExceeded => StatusCode::TOO_MANY_REQUESTS,
            Self::ShareLinkInvalid => StatusCode::GONE,
        }
    }

    /// Arabic user-facing title matching `errors.toml`.
    #[must_use]
    pub const fn title_ar(self) -> &'static str {
        match self {
            Self::InvalidCredentials => "فشل التحقق من الهوية",
            Self::Expired => "انتهت صلاحية الدخول",
            Self::Revoked => "تم إبطال بيانات الدخول",
            Self::InsufficientScope => "ليس لديك صلاحية لهذا الإجراء",
            Self::BackendUnavailable => "خدمة الدخول غير متاحة مؤقتًا",
            Self::InvalidRequest => "طلب غير صالح",
            Self::InvalidVerificationToken => "رابط التحقق غير صالح أو انتهت صلاحيته",
            Self::DuplicateAccount => "هذا الحساب مسجل بالفعل",
            Self::AccountSuspended => "الحساب موقوف مؤقتًا",
            Self::RateLimited => "تجاوزت الحد المسموح للمحاولات",
            Self::InvalidPhoneOtp => "رمز التحقق عبر الهاتف غير صالح أو منتهي الصلاحية",
            Self::OAuthNotConfigured => "تسجيل الدخول الخارجي غير مهيأ أو الحالة غير صالحة",
            Self::OwnerRequired => "الوصول محصور بمالك المنصة",
            Self::OwnerMfaRequired => "التحقق الثنائي لمالك المنصة مطلوب أو الرمز غير صحيح",
            Self::OwnerMfaLocked => "تم قفل التحقق الثنائي مؤقتًا بعد محاولات فاشلة",
            Self::OwnerStepUpRequired => "يلزم إعادة التحقق الثنائي لتنفيذ هذا الإجراء الحساس",
            Self::OwnerBootstrapSealed => "تعذّر تهيئة مالك المنصة لأنها مغلقة أو غير صالحة",
            Self::OwnerTotpKeyMissing => "مفتاح تشفير التحقق الثنائي غير مهيأ على الخادم",
            Self::QuotaExceeded => "استنفدت حد العمليات الشهري لخطتك",
            Self::DatasetLimitExceeded => "يتجاوز الملف حد الصفوف أو الأعمدة المسموح لخطتك",
            Self::AiQuotaExceeded => "استنفدت حصة استخدام الذكاء الاصطناعي",
            Self::PaidPlanRequired => "هذه الميزة متاحة للخطط المدفوعة فقط",
            Self::BillingNotConfigured => "بوابة الدفع غير مهيأة بعد",
            Self::InvalidWebhookSignature => "توقيع إشعار الدفع غير صالح",
            Self::WebhookReplayWindow => "توقيت إشعار الدفع خارج النافذة المسموحة",
            Self::BillingOperationFailed => "تعذّر تنفيذ عملية الفوترة المطلوبة",
            Self::SyncEnvelopeInvalid => "حزمة المزامنة المشفرة غير صالحة أو تعرضت للتعديل",
            Self::SyncVersionConflict => "تعارض في إصدار المزامنة",
            Self::ShareLinkInvalid => "رابط المشاركة المشفر منتهي الصلاحية أو ملغى",
            Self::OrgMinSeatsRequired => "الحد الأدنى للمؤسسات هو 3 مقاعد",
            Self::OrgPermissionDenied => "ليست لديك صلاحية لإدارة هذه المؤسسة",
            Self::OrgSsoNotConfigured => "الدخول الموحد (SSO/SCIM) غير مهيأ بعد",
            Self::AiProviderNotConfigured => "مزود الذكاء الاصطناعي غير مهيأ أو غير متاح دون اتصال",
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
