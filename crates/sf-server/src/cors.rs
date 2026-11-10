//! Strict CORS for the packaged desktop webview and the local Vite server.
//!
//! Only the known Tauri asset origins and the Vite development origin are
//! accepted. Preflight requests are answered before the router, so an
//! authenticated route never sees them. No wildcard and no credentials are
//! ever sent.

use axum::extract::Request;
use axum::http::{header, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

/// Origins emitted by Tauri on its supported desktop platforms.
pub const ALLOWED_ORIGINS: [&str; 5] = [
    "tauri://localhost",
    "http://tauri.localhost",
    "https://tauri.localhost",
    "http://localhost:1420",
    "http://127.0.0.1:1420",
];

const ALLOWED_METHODS: [&str; 4] = ["GET", "POST", "DELETE", "OPTIONS"];
const ALLOWED_HEADERS: [&str; 3] = ["accept", "authorization", "content-type"];
const ALLOWED_METHODS_HEADER: &str = "GET, POST, DELETE, OPTIONS";
const ALLOWED_HEADERS_HEADER: &str = "accept, authorization, content-type";
const PREFLIGHT_MAX_AGE_SECONDS: &str = "600";

/// Apply the allowlist to account API requests.
pub async fn cors_middleware(request: Request, next: Next) -> Response {
    let origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let Some(origin) = origin else {
        return next.run(request).await;
    };

    let trusted = ALLOWED_ORIGINS.contains(&origin.as_str());
    if request.method() == Method::OPTIONS {
        if !trusted || !preflight_allowed(&request) {
            return StatusCode::FORBIDDEN.into_response();
        }
        let mut response = StatusCode::NO_CONTENT.into_response();
        insert_origin(&mut response, &origin);
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static(ALLOWED_METHODS_HEADER),
        );
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static(ALLOWED_HEADERS_HEADER),
        );
        response.headers_mut().insert(
            header::ACCESS_CONTROL_MAX_AGE,
            HeaderValue::from_static(PREFLIGHT_MAX_AGE_SECONDS),
        );
        append_vary(
            &mut response,
            "origin, access-control-request-method, access-control-request-headers",
        );
        return response;
    }

    let mut response = next.run(request).await;
    if trusted {
        insert_origin(&mut response, &origin);
        append_vary(&mut response, "origin");
    }
    response
}

fn insert_origin(response: &mut Response, origin: &str) {
    if let Ok(value) = HeaderValue::from_str(origin) {
        response
            .headers_mut()
            .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
    }
}

fn append_vary(response: &mut Response, additional: &str) {
    let existing = response
        .headers()
        .get(header::VARY)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let combined = match existing {
        Some(existing) if !existing.is_empty() => format!("{existing}, {additional}"),
        _ => additional.to_owned(),
    };
    if let Ok(value) = HeaderValue::from_str(&combined) {
        response.headers_mut().insert(header::VARY, value);
    }
}

fn preflight_allowed(request: &Request) -> bool {
    let requested_method = request
        .headers()
        .get(header::ACCESS_CONTROL_REQUEST_METHOD)
        .and_then(|value| value.to_str().ok());
    let Some(requested_method) = requested_method else {
        return false;
    };
    let method = requested_method.to_ascii_uppercase();
    if !ALLOWED_METHODS.contains(&method.as_str()) {
        return false;
    }

    match request
        .headers()
        .get(header::ACCESS_CONTROL_REQUEST_HEADERS)
        .and_then(|value| value.to_str().ok())
    {
        None => true,
        Some(requested_headers) => {
            let mut any = false;
            for name in requested_headers.split(',') {
                let name = name.trim().to_ascii_lowercase();
                if name.is_empty() || !ALLOWED_HEADERS.contains(&name.as_str()) {
                    return false;
                }
                any = true;
            }
            any
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::middleware;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    use super::*;

    fn test_app() -> Router {
        Router::new()
            .route("/v1/test", get(|| async { "ok" }).post(|| async { "ok" }))
            .layer(middleware::from_fn(cors_middleware))
    }

    async fn send(request: Request<Body>) -> Response {
        test_app().oneshot(request).await.expect("router responds")
    }

    fn header(response: &Response, name: axum::http::HeaderName) -> Option<String> {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    }

    #[tokio::test]
    async fn actual_request_from_a_trusted_origin_is_echoed() {
        let response = send(
            Request::builder()
                .uri("/v1/test")
                .header(header::ORIGIN, "tauri://localhost")
                .body(Body::empty())
                .expect("request"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            header(&response, header::ACCESS_CONTROL_ALLOW_ORIGIN).as_deref(),
            Some("tauri://localhost")
        );
        assert!(header(&response, header::VARY)
            .expect("vary")
            .contains("origin"));
    }

    #[tokio::test]
    async fn actual_request_from_a_foreign_origin_gets_no_cors_headers() {
        let response = send(
            Request::builder()
                .uri("/v1/test")
                .header(header::ORIGIN, "https://evil.example")
                .body(Body::empty())
                .expect("request"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(header(&response, header::ACCESS_CONTROL_ALLOW_ORIGIN), None);
    }

    #[tokio::test]
    async fn preflight_from_a_trusted_origin_is_allowed() {
        let response = send(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/v1/auth/login")
                .header(header::ORIGIN, "http://tauri.localhost")
                .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
                .header(
                    header::ACCESS_CONTROL_REQUEST_HEADERS,
                    "content-type, authorization, accept",
                )
                .body(Body::empty())
                .expect("request"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            header(&response, header::ACCESS_CONTROL_ALLOW_ORIGIN).as_deref(),
            Some("http://tauri.localhost")
        );
        assert_eq!(
            header(&response, header::ACCESS_CONTROL_ALLOW_METHODS).as_deref(),
            Some(ALLOWED_METHODS_HEADER)
        );
        assert_eq!(
            header(&response, header::ACCESS_CONTROL_ALLOW_HEADERS).as_deref(),
            Some(ALLOWED_HEADERS_HEADER)
        );
        assert_eq!(
            header(&response, header::ACCESS_CONTROL_MAX_AGE).as_deref(),
            Some(PREFLIGHT_MAX_AGE_SECONDS)
        );
    }

    #[tokio::test]
    async fn preflight_from_a_foreign_origin_is_rejected() {
        let response = send(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/v1/auth/login")
                .header(header::ORIGIN, "https://evil.example")
                .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
                .body(Body::empty())
                .expect("request"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(header(&response, header::ACCESS_CONTROL_ALLOW_ORIGIN), None);
    }

    #[tokio::test]
    async fn preflight_rejects_a_method_outside_the_allowlist() {
        for method in ["PATCH", "PUT"] {
            let response = send(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/v1/auth/login")
                    .header(header::ORIGIN, "tauri://localhost")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, method)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await;
            assert_eq!(response.status(), StatusCode::FORBIDDEN, "{method}");
        }
    }

    #[tokio::test]
    async fn preflight_rejects_a_header_outside_the_allowlist() {
        let response = send(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/v1/auth/login")
                .header(header::ORIGIN, "tauri://localhost")
                .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
                .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "x-evil")
                .body(Body::empty())
                .expect("request"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn preflight_without_a_requested_method_is_rejected() {
        let response = send(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/v1/auth/login")
                .header(header::ORIGIN, "tauri://localhost")
                .body(Body::empty())
                .expect("request"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn a_request_without_an_origin_passes_through_unchanged() {
        let response = send(
            Request::builder()
                .uri("/v1/test")
                .body(Body::empty())
                .expect("request"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(header(&response, header::ACCESS_CONTROL_ALLOW_ORIGIN), None);
    }

    #[tokio::test]
    async fn post_is_routed_after_the_cors_layer() {
        let response = send(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/test")
                .header(header::ORIGIN, "http://localhost:1420")
                .body(Body::empty())
                .expect("request"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
    }
}
