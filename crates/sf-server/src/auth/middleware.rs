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
use super::{unix_now, AuthenticatedSession, Principal, Scope};
use crate::billing::handoff::{BillingHandoffService, BILLING_WEB_TOKEN_PREFIX};
use crate::desktop_auth::DesktopAuthService;
use crate::error::AuthError;
use crate::pg_store::PgControlPlaneStore;

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

/// State for Owner Control Center routes accepting EITHER a Desktop `sfs_*` session OR a
/// dedicated Owner Web Portal `sfw_*` session (Section 6.1: Owner web login never counts as a
/// second Desktop device).
#[derive(Clone)]
pub struct OwnerSessionGuard {
    pub auth: Arc<AuthService>,
    pub desktop_auth: DesktopAuthService,
    pub pg_store: Option<PgControlPlaneStore>,
    pub required_scope: Scope,
}

/// State for Website Billing Portal & entitlement-read routes accepting EITHER a Desktop `sfs_*`
/// session OR a scoped Website Billing Portal `sfb_*` session (never revoking Desktop device).
#[derive(Clone)]
pub struct BillingSessionGuard {
    pub auth: Arc<AuthService>,
    pub billing_handoff: BillingHandoffService,
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
///
/// When the caller supplies an optional `X-Safi-Installation-Id` header (used by the native
/// desktop shell), `require_session` derives the user-scoped installation key hash and verifies
/// in constant time that the active session belongs to that exact device installation.
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
    if let Some(raw_header) = request.headers().get("x-safi-installation-id") {
        let raw_inst = raw_header
            .to_str()
            .map_err(|_| AuthError::InvalidCredentials)?
            .trim();
        let installation_id =
            uuid::Uuid::parse_str(raw_inst).map_err(|_| AuthError::InvalidCredentials)?;
        let derived = super::device::DeviceRegistration::from_installation_id(
            session.principal.user_id,
            installation_id,
            super::device::DevicePlatform::Web,
        )
        .map_err(|_| AuthError::InvalidCredentials)?;
        let derived_hex = hex::encode(derived.installation_key_hash);
        let Some(bound_hex) = session.device_installation_key_hash.as_deref() else {
            return Err(AuthError::Revoked);
        };
        use subtle::ConstantTimeEq;
        if derived_hex
            .as_bytes()
            .ct_eq(bound_hex.as_bytes())
            .unwrap_u8()
            != 1
        {
            return Err(AuthError::Revoked);
        }
    }
    enforce_scope(&session.principal, guard.required_scope)?;
    request.extensions_mut().insert(session);
    Ok(next.run(request).await)
}

/// Authenticate either a Desktop `sfs_*` session or a dedicated Owner Web `sfw_*` session.
pub async fn require_owner_session(
    State(guard): State<OwnerSessionGuard>,
    mut request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let token = bearer_token(&request).ok_or(AuthError::InvalidCredentials)?;
    let session = if token.starts_with("sfw_") {
        if let Some(pg) = &guard.pg_store {
            let token_hash_bytes = blake3::hash(token.trim().as_bytes());
            pg.authenticate_owner_web_session_pg(token_hash_bytes.as_bytes(), unix_now())
                .await?
        } else {
            guard
                .desktop_auth
                .authenticate_owner_web_session(token, unix_now())
                .await?
        }
    } else if token.starts_with(SESSION_TOKEN_PREFIX) {
        guard.auth.authenticate_session(token).await?
    } else {
        return Err(AuthError::InvalidCredentials);
    };
    enforce_scope(&session.principal, guard.required_scope)?;
    request.extensions_mut().insert(session);
    Ok(next.run(request).await)
}

/// Authenticate either a Desktop `sfs_*` session or a scoped Website Billing `sfb_*` session.
/// `sfb_*` tokens are strictly rejected on Desktop Studio, Sync, Org, AI, and Owner routes.
pub async fn require_billing_session(
    State(guard): State<BillingSessionGuard>,
    mut request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let token = bearer_token(&request).ok_or(AuthError::InvalidCredentials)?;
    let session = if token.starts_with(BILLING_WEB_TOKEN_PREFIX) {
        guard
            .billing_handoff
            .authenticate_billing_web_session_at(token, unix_now())
            .await?
    } else if token.starts_with(SESSION_TOKEN_PREFIX) {
        guard.auth.authenticate_session(token).await?
    } else {
        return Err(AuthError::InvalidCredentials);
    };
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
