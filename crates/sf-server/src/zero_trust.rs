//! Owner Privileged Access Guard — strictly backed by immutable `owner_user_id`
//! and Google Authenticator-compatible RFC 6238 TOTP elevated/step-up sessions.
//!
//! All legacy static tunnel headers and simulated YubiKey strings have been removed.

use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};

use crate::auth::{unix_now, AuthenticatedSession};
use crate::error::AuthError;
use crate::owner_mfa::OwnerMfaService;

/// Guard configuration for Owner routes.
#[derive(Debug, Clone)]
pub struct OwnerAccessGuard {
    pub mfa: OwnerMfaService,
    pub require_step_up: bool,
}

/// Axum middleware enforcing:
/// 1. Valid authenticated session (`AuthenticatedSession` in request extensions).
/// 2. Caller `user_id` matches the sealed immutable `platform_owner` record (`SF-AUTH-013`).
/// 3. Active, non-expired RFC 6238 TOTP elevated Owner session (`SF-AUTH-014`).
/// 4. Optional fresh 5-minute step-up TOTP verification for high-risk actions (`SF-AUTH-016`).
pub async fn require_owner_mfa(
    State(guard): State<OwnerAccessGuard>,
    request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let session = request
        .extensions()
        .get::<AuthenticatedSession>()
        .ok_or(AuthError::InvalidCredentials)?;
    let now = unix_now();
    if guard.require_step_up {
        guard
            .mfa
            .require_step_up_at(session.principal.user_id, session.session_id, now)
            .await?;
    } else {
        guard
            .mfa
            .require_elevated_session_at(session.principal.user_id, session.session_id, now)
            .await?;
    }
    Ok(next.run(request).await)
}
