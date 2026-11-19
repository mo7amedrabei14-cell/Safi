//! SafiData Server — Axum 0.8 backend core (Section 9, 15.7, Desktop 1.0).
//!
//! Every response carries `X-Request-ID`; failures are rendered as RFC 9457
//! `problem+json` with canonical `errors.toml` codes.

pub mod accounts;
pub mod auth;
pub mod billing;
pub mod config;
pub mod cors;
pub mod encrypted_sync;
pub mod entitlements;
pub mod error;
pub mod log_center;
pub mod organizations;
pub mod owner_mfa;
pub mod routes;
pub mod routes_updates;
pub mod updater;
pub mod zero_trust;

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use axum::extract::Request;
use axum::http::HeaderValue;
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::Router;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

use auth::account::AccountAuthService;
use auth::service::AuthService;
use billing::BillingService;
use config::ServerConfig;
use encrypted_sync::EncryptedSyncService;
use entitlements::EntitlementService;
use log_center::AuditLogCenter;
use organizations::OrganizationService;
use owner_mfa::OwnerMfaService;
use updater::ReleaseAndLicenseSigner;

/// Request identifier attached by the middleware and echoed in the response header.
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

/// Feature flag controlled via the Owner Control Plane.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlag {
    pub key: String,
    pub enabled: bool,
    pub description: String,
    pub updated_at: i64,
}

/// Server-tracked account metadata used by real Owner Control Plane stats & actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedAccountRecord {
    pub user_id: Uuid,
    pub workspace_id: Uuid,
    pub email: String,
    pub display_name: String,
    pub verified_phone: Option<String>,
    pub suspended: bool,
    pub created_at: i64,
}

/// Pending phone OTP verification entry (hashed code, attempt cap, expiry).
#[derive(Debug, Clone)]
pub struct PendingPhoneOtp {
    pub phone_e164: String,
    pub channel: String,
    pub code_hash: String,
    pub attempts: u32,
    pub expires_at: i64,
}

/// Pending OAuth PKCE/state authorization session.
#[derive(Debug, Clone)]
pub struct PendingOAuthState {
    pub provider: String,
    pub pkce_challenge: String,
    pub redirect_uri: String,
    pub expires_at: i64,
}

/// Simple sliding-window rate bucket.
#[derive(Debug, Clone)]
pub struct RateBucket {
    pub window_start: i64,
    pub count: u32,
}

/// Mutable control-plane & auth-extension state shared across handlers.
#[derive(Debug, Default)]
pub struct ControlPlaneState {
    pub maintenance_mode: bool,
    pub maintenance_reason: Option<String>,
    pub feature_flags: BTreeMap<String, FeatureFlag>,
    pub managed_accounts: HashMap<Uuid, ManagedAccountRecord>,
    pub phone_otps: HashMap<Uuid, PendingPhoneOtp>,
    pub oauth_states: HashMap<String, PendingOAuthState>,
    pub rate_limits: HashMap<String, RateBucket>,
}

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<ServerConfig>,
    pub auth: Arc<AuthService>,
    pub account_auth: Option<Arc<AccountAuthService>>,
    pub owner_mfa: OwnerMfaService,
    pub entitlements: EntitlementService,
    pub billing: BillingService,
    pub organizations: OrganizationService,
    pub encrypted_sync: EncryptedSyncService,
    pub signer: ReleaseAndLicenseSigner,
    pub log_center: AuditLogCenter,
    pub control_plane: Arc<RwLock<ControlPlaneState>>,
}

impl AppState {
    /// Build application state with test/default configuration around an `AuthService`.
    #[must_use]
    pub fn new(auth: AuthService) -> Self {
        Self::with_config(auth, ServerConfig::for_tests())
    }

    /// Build application state using an explicit validated [`ServerConfig`].
    #[must_use]
    pub fn with_config(auth: AuthService, config: ServerConfig) -> Self {
        let entitlements = EntitlementService::new();
        let billing = BillingService::from_config(&config, entitlements.clone());
        let owner_mfa = OwnerMfaService::new(
            config.totp_encryption_key,
            config.totp_key_version,
            config.totp_issuer.clone(),
        );
        let signer =
            ReleaseAndLicenseSigner::new(config.update_signing_seed, config.license_signing_seed);
        Self {
            config: Arc::new(config),
            auth: Arc::new(auth),
            account_auth: None,
            owner_mfa,
            entitlements,
            billing,
            organizations: OrganizationService::new(),
            encrypted_sync: EncryptedSyncService::new(),
            signer,
            log_center: AuditLogCenter::new(),
            control_plane: Arc::new(RwLock::new(ControlPlaneState::default())),
        }
    }

    /// Attach the public account lifecycle without changing API-key/session routes.
    #[must_use]
    pub fn with_account_auth(mut self, account_auth: AccountAuthService) -> Self {
        self.account_auth = Some(Arc::new(account_auth));
        self
    }

    /// Register or refresh a managed account record so Owner stats & actions reflect real users.
    pub async fn upsert_managed_account(&self, record: ManagedAccountRecord) {
        self.entitlements
            .ensure_workspace(record.workspace_id, record.user_id)
            .await;
        self.control_plane
            .write()
            .await
            .managed_accounts
            .insert(record.user_id, record);
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
    routes::build_router(state)
        .layer(middleware::from_fn(cors::cors_middleware))
        .layer(middleware::from_fn(request_id_middleware))
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
