//! Authentication middleware. Credentials arrive as `Authorization: Bearer ...`;
//! API keys (`sfk_live_*`) and session tokens (`sfs_*`) are distinguished by prefix.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::header::AUTHORIZATION;
use axum::middleware::Next;
use axum::response::Response;

use super::api_key::API_KEY_PREFIX;
use super::service::AuthService;
use super::session::SESSION_TOKEN_PREFIX;
use super::{AuthenticatedSession, Principal, Scope};
use crate::error::AuthError;

/// State for API-key protected routes.
#[derive(Clone)]
pub struct ApiKeyGuard {
    pub auth: Arc<AuthService>,
    pub required_scope: Scope,
}

/// State for session-protected routes.
#[derive(Clone)]
pub struct SessionGuard {
    pub auth: Arc<AuthService>,
    pub required_scope: Scope,
}

/// State for routes accepting either credential type.
#[derive(Clone)]
pub struct AnyCredentialGuard {
    pub auth: Arc<AuthService>,
    pub required_scope: Scope,
}

/// Authenticate an API key, enforce its scope, and expose the principal.
pub async fn require_api_key(
    State(guard): State<ApiKeyGuard>,
    mut request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let token = bearer_token(&request).ok_or(AuthError::InvalidCredentials)?;
    if !token.starts_with(API_KEY_PREFIX) {
        return Err(AuthError::InvalidCredentials);
    }
    let principal = guard.auth.authenticate_api_key(token).await?;
    enforce_scope(&principal, guard.required_scope)?;
    request.extensions_mut().insert(principal);
    Ok(next.run(request).await)
}

/// Authenticate a session token, enforce its scope, and expose the session.
pub async fn require_session(
    State(guard): State<SessionGuard>,
    mut request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let token = bearer_token(&request).ok_or(AuthError::InvalidCredentials)?;
    if !token.starts_with(SESSION_TOKEN_PREFIX) {
        return Err(AuthError::InvalidCredentials);
    }
    let session = guard.auth.authenticate_session(token).await?;
    enforce_scope(&session.principal, guard.required_scope)?;
    request.extensions_mut().insert(session);
    Ok(next.run(request).await)
}

/// Accept either an API key or a session token (used by `whoami`).
pub async fn require_any_credential(
    State(guard): State<AnyCredentialGuard>,
    mut request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let token = bearer_token(&request).ok_or(AuthError::InvalidCredentials)?;
    if token.starts_with(API_KEY_PREFIX) {
        let principal = guard.auth.authenticate_api_key(token).await?;
        enforce_scope(&principal, guard.required_scope)?;
        request.extensions_mut().insert(principal);
    } else if token.starts_with(SESSION_TOKEN_PREFIX) {
        let session = guard.auth.authenticate_session(token).await?;
        enforce_scope(&session.principal, guard.required_scope)?;
        request.extensions_mut().insert(session);
    } else {
        return Err(AuthError::InvalidCredentials);
    }
    Ok(next.run(request).await)
}

fn bearer_token(request: &Request) -> Option<&str> {
    request
        .headers()
        .get(AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

fn enforce_scope(principal: &Principal, required: Scope) -> Result<(), AuthError> {
    if principal.has_scope(required) {
        Ok(())
    } else {
        Err(AuthError::InsufficientScope)
    }
}

/// Helper for handlers that need the session details directly.
pub fn session_from_extensions(request: &Request) -> Option<&AuthenticatedSession> {
    request.extensions().get::<AuthenticatedSession>()
}
