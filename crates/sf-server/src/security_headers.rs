//! HTTP Security Headers & Anti-CSRF Middleware for SafiData API responses.
//!
//! Enforces:
//! - Strict security headers on every response (`X-Content-Type-Options`, `X-Frame-Options`,
//!   `Referrer-Policy`, `Permissions-Policy`, `Content-Security-Policy`, `Cache-Control: no-store`,
//!   and `Strict-Transport-Security`).
//! - Origin / `Sec-Fetch-Site` CSRF defense on state-changing requests (`POST`, `PUT`, `PATCH`,
//!   `DELETE`) for all non-webhook endpoints, rejecting cross-site browser origins with `403 Forbidden`.

use axum::extract::Request;
use axum::http::{header, HeaderName, HeaderValue, Method};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::cors::ALLOWED_ORIGINS;
use crate::error::AuthError;

const PRODUCTION_WEB_ORIGINS: [&str; 2] = ["https://safidata.app", "https://www.safidata.app"];

fn is_loopback_origin(origin: &str) -> bool {
    origin.starts_with("http://127.0.0.1:")
        || origin == "http://127.0.0.1"
        || origin.starts_with("http://localhost:")
        || origin == "http://localhost"
}

fn is_loopback_host(host: Option<&str>) -> bool {
    match host {
        Some(h) => {
            h == "127.0.0.1"
                || h.starts_with("127.0.0.1:")
                || h == "localhost"
                || h.starts_with("localhost:")
                || h == "[::1]"
                || h.starts_with("[::1]:")
        }
        None => true,
    }
}

fn is_trusted_csrf_origin(origin: &str, request_host: Option<&str>) -> bool {
    if ALLOWED_ORIGINS.contains(&origin) || PRODUCTION_WEB_ORIGINS.contains(&origin) {
        return true;
    }
    if is_loopback_origin(origin) && is_loopback_host(request_host) {
        return true;
    }
    // Exact same-origin check (`https://<host>` == `Host: <host>`).
    if let Some(host) = request_host {
        if origin == format!("https://{host}") {
            return true;
        }
    }
    false
}

/// Apply anti-CSRF validation on state-changing browser requests and attach strict security headers.
pub async fn security_headers_and_csrf_middleware(request: Request, next: Next) -> Response {
    let is_state_changing = matches!(
        *request.method(),
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    );
    let path = request.uri().path();
    let is_provider_webhook = path.starts_with("/v1/billing/webhooks/");

    if is_state_changing && !is_provider_webhook {
        let host = request
            .headers()
            .get(header::HOST)
            .and_then(|v| v.to_str().ok());
        let origin = request
            .headers()
            .get(header::ORIGIN)
            .and_then(|v| v.to_str().ok());
        let sec_fetch_site = request
            .headers()
            .get("sec-fetch-site")
            .and_then(|v| v.to_str().ok());

        if let Some(orig) = origin {
            if !is_trusted_csrf_origin(orig, host) {
                let mut denied = AuthError::InsufficientScope.into_response();
                apply_security_headers(&mut denied);
                return denied;
            }
        } else if sec_fetch_site.is_some_and(|s| s.eq_ignore_ascii_case("cross-site")) {
            let mut denied = AuthError::InsufficientScope.into_response();
            apply_security_headers(&mut denied);
            return denied;
        }
    }

    let mut response = next.run(request).await;
    apply_security_headers(&mut response);
    response
}

fn apply_security_headers(response: &mut Response) {
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("accelerometer=(), camera=(), geolocation=(), microphone=()"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'",
        ),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(
        header::STRICT_TRANSPORT_SECURITY,
        HeaderValue::from_static("max-age=63072000; includeSubDomains; preload"),
    );
}
