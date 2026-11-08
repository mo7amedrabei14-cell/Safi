//! SafiData Server — Axum 0.8 backend core (Section 9, 15.7).
//!
//! Every response carries `X-Request-ID`; authentication failures are rendered
//! as `problem+json` with canonical `errors.toml` codes.

pub mod auth;
pub mod error;
pub mod log_center;
pub mod routes;
pub mod updater;
pub mod zero_trust;

use std::sync::Arc;

use axum::extract::Request;
use axum::http::HeaderValue;
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::Router;
use tokio::sync::RwLock;

use auth::service::AuthService;
use log_center::AuditLogCenter;

/// Request identifier attached by the middleware and echoed in the response header.
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub auth: Arc<AuthService>,
    pub zero_trust: zero_trust::ZeroTrustGuardState,
    pub log_center: Arc<RwLock<AuditLogCenter>>,
}

impl AppState {
    /// Build the state around a fully wired authentication service.
    #[must_use]
    pub fn new(auth: AuthService) -> Self {
        Self {
            auth: Arc::new(auth),
            zero_trust: zero_trust::ZeroTrustGuardState::default(),
            log_center: Arc::new(RwLock::new(AuditLogCenter::new())),
        }
    }
}

/// SafiData server version reported by `/health`.
#[must_use]
pub fn get_server_version() -> &'static str {
    "5.0.0-gold-master"
}

/// Generate a fresh request identifier.
#[must_use]
pub fn generate_request_id() -> String {
    format!("req-{}", uuid::Uuid::new_v4())
}

/// Ensure every request and response carries `X-Request-ID` (Section 15.7).
pub async fn request_id_middleware(mut request: Request, next: Next) -> Response {
    let incoming = request
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .map(str::to_owned);
    let request_id = incoming.unwrap_or_else(generate_request_id);

    request
        .extensions_mut()
        .insert(RequestId(request_id.clone()));
    let mut response = next.run(request).await;
    if let Ok(value) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", value);
    }
    response
}

/// Build the full application with the request-id middleware applied outermost.
pub fn app(state: AppState) -> Router {
    routes::build_router(state).layer(middleware::from_fn(request_id_middleware))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_server_version_and_request_id() {
        assert_eq!(get_server_version(), "5.0.0-gold-master");
        let req_id = generate_request_id();
        assert!(req_id.starts_with("req-"));
    }
}
