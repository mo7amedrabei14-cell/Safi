//! HTTP surface for health and authentication (Section 9.3).

use axum::extract::{Extension, State};
use axum::http::StatusCode;
use axum::middleware::from_fn_with_state;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::middleware::{
    require_any_credential, require_api_key, require_session, AnyCredentialGuard, ApiKeyGuard,
    SessionGuard,
};
use crate::auth::session::SessionView;
use crate::auth::{AuthenticatedSession, Principal, Scope};
use crate::error::AuthError;
use crate::{get_server_version, AppState};

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
}

/// Liveness/readiness probe used by deployments and CI.
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: get_server_version(),
    })
}

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub device_name: String,
}

#[derive(Debug, Serialize)]
pub struct CreateSessionResponse {
    pub token: String,
    pub session: SessionView,
}

#[derive(Debug, Serialize)]
pub struct CurrentSessionResponse {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub device_name: String,
    pub expires_at: i64,
    pub scopes: Vec<Scope>,
}

/// `POST /v1/auth/sessions` — exchange a write-scoped API key for a session.
pub async fn create_session(
    State(state): State<AppState>,
    Extension(principal): Extension<Principal>,
    Json(body): Json<CreateSessionRequest>,
) -> Result<(StatusCode, Json<CreateSessionResponse>), AuthError> {
    let issued = state
        .auth
        .create_session(&principal, &body.device_name)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(CreateSessionResponse {
            token: issued.token,
            session: issued.session,
        }),
    ))
}

/// `GET /v1/auth/sessions/current` — describe the caller's session.
pub async fn current_session(
    Extension(session): Extension<AuthenticatedSession>,
) -> Json<CurrentSessionResponse> {
    Json(CurrentSessionResponse {
        id: session.session_id,
        workspace_id: session.principal.workspace_id,
        device_name: session.device_name,
        expires_at: session.expires_at,
        scopes: session.principal.scopes,
    })
}

/// `DELETE /v1/auth/sessions/current` — revoke and evict the current session.
pub async fn revoke_current_session(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
) -> Result<StatusCode, AuthError> {
    state
        .auth
        .revoke_session(session.session_id, &session.token_hash)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /v1/auth/whoami` — identity, workspace, and scopes for any credential.
pub async fn whoami(Extension(principal): Extension<Principal>) -> Json<Principal> {
    Json(principal)
}

/// Build the complete application router (health + `/v1/auth/*`).
pub fn build_router(state: AppState) -> Router {
    let auth = state.auth.clone();

    let any_credential = Router::new()
        .route("/v1/auth/whoami", get(whoami))
        .route_layer(from_fn_with_state(
            AnyCredentialGuard {
                auth: auth.clone(),
                required_scope: Scope::Read,
            },
            require_any_credential,
        ));

    let api_key_only = Router::new()
        .route("/v1/auth/sessions", post(create_session))
        .route_layer(from_fn_with_state(
            ApiKeyGuard {
                auth: auth.clone(),
                required_scope: Scope::Write,
            },
            require_api_key,
        ));

    let session_only = Router::new()
        .route(
            "/v1/auth/sessions/current",
            get(current_session).delete(revoke_current_session),
        )
        .route_layer(from_fn_with_state(
            SessionGuard {
                auth,
                required_scope: Scope::Read,
            },
            require_session,
        ));

    Router::new()
        .route("/health", get(health))
        .merge(any_credential)
        .merge(api_key_only)
        .merge(session_only)
        .with_state(state)
}
