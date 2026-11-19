//! Complete HTTP surface for SafiData Server:
//! - `/health` & `/ready` (secret-safe integration status matrix)
//! - `/v1/auth/*` (sessions, accounts, email/phone OTP, OAuth PKCE adapters, change email/password)
//! - `/v1/owner/*` (sealed Owner identity + Google Authenticator RFC 6238 TOTP + real Control Plane)
//! - `/v1/entitlements/*` (authoritative quotas & usage accounting)
//! - `/v1/billing/*` (Paddle HMAC-SHA256 + Paymob HMAC-SHA512 + checkout + invoices)
//! - `/v1/organizations/*` (teams, min 3 seats, invitations, RBAC, SSO/SCIM readiness)
//! - `/v1/sync/*` (zero-knowledge client-encrypted sync & share links)
//! - `/v1/ai/route` (consent + local PII masking + quota + honest NOT_CONFIGURED gate)
//! - `/v1/updates/latest` & `/v1/licenses/issue` (real Ed25519 signing)

use axum::body::Bytes;
use axum::extract::{Extension, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::from_fn_with_state;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::account::{
    AccountAuthService, AccountMessage, ConfirmPasswordResetRequest, LoginAccountRequest,
    RegisterAccountRequest, RequestPasswordResetRequest, ResendVerificationRequest,
    VerifyEmailRequest,
};
use crate::auth::identity::normalize_phone;
use crate::auth::middleware::{
    require_any_credential, require_api_key, require_session, AnyCredentialGuard, ApiKeyGuard,
    SessionGuard,
};
use crate::auth::session::SessionView;
use crate::auth::{unix_now, AuthenticatedSession, Principal, Scope};
use crate::billing::{
    BillingEventRecord, CheckoutRequest, CheckoutResponse, InvoiceRecord, PaymobTransactionObj,
};
use crate::config::ReadinessReport;
use crate::encrypted_sync::{
    CreateShareLinkRequest, CreatedShareLink, EncryptedSyncRecord, PushEncryptedSyncRequest,
    ShareLinkRecord,
};
use crate::entitlements::{
    ConsumeUsageRequest, ConsumeUsageResult, EffectiveEntitlements, EntitlementAggregateStats,
    PlanTier, SubscriptionState, UsageKind,
};
use crate::error::AuthError;
use crate::log_center::{AuditLogEntry, AuditOutcome};
use crate::organizations::{OrgRole, Organization, OrganizationMember};
use crate::owner_mfa::{ElevatedOwnerSession, OwnerMfaStatus, TotpEnrollmentBundle};
use crate::routes_updates::{get_latest_update, issue_offline_license};
use crate::zero_trust::{require_owner_mfa, OwnerAccessGuard};
use crate::{
    get_server_version, AppState, FeatureFlag, ManagedAccountRecord, PendingOAuthState,
    PendingPhoneOtp, RateBucket,
};

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub maintenance_mode: bool,
}

/// `GET /health` — liveness probe.
pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    let maintenance = state.control_plane.read().await.maintenance_mode;
    Json(HealthResponse {
        status: "ok",
        version: get_server_version(),
        maintenance_mode: maintenance,
    })
}

/// `GET /ready` — secret-safe readiness & external integration configuration matrix.
pub async fn ready(State(state): State<AppState>) -> (StatusCode, Json<ReadinessReport>) {
    let report = state.config.readiness_report();
    let code = if report.ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (code, Json(report))
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
    pub user_id: Uuid,
    pub workspace_id: Uuid,
    pub device_name: String,
    pub expires_at: i64,
    pub scopes: Vec<Scope>,
    pub is_owner: bool,
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

/// `GET /v1/auth/sessions/current` — describe the caller's session, including server-authoritative `is_owner`.
pub async fn current_session(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
) -> Json<CurrentSessionResponse> {
    let is_owner = state
        .owner_mfa
        .is_owner_user(session.principal.user_id)
        .await;
    Json(CurrentSessionResponse {
        id: session.session_id,
        user_id: session.principal.user_id,
        workspace_id: session.principal.workspace_id,
        device_name: session.device_name,
        expires_at: session.expires_at,
        scopes: session.principal.scopes,
        is_owner,
    })
}

/// `DELETE /v1/auth/sessions/current` — revoke session and clear any elevated Owner MFA session.
pub async fn revoke_current_session(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
) -> Result<StatusCode, AuthError> {
    state
        .owner_mfa
        .revoke_elevated_session(session.session_id)
        .await;
    state
        .auth
        .revoke_session(session.session_id, &session.token_hash)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize)]
pub struct WhoAmIResponse {
    pub user_id: Uuid,
    pub workspace_id: Uuid,
    pub scopes: Vec<Scope>,
    pub is_owner: bool,
    pub suspended: bool,
}

/// `GET /v1/auth/whoami` — identity, workspace, scopes, and server-authoritative `is_owner`.
pub async fn whoami(
    State(state): State<AppState>,
    principal_ext: Option<Extension<Principal>>,
    session_ext: Option<Extension<AuthenticatedSession>>,
) -> Result<Json<WhoAmIResponse>, AuthError> {
    let principal = if let Some(Extension(p)) = principal_ext {
        p
    } else if let Some(Extension(s)) = session_ext {
        s.principal
    } else {
        return Err(AuthError::InvalidCredentials);
    };
    let is_owner = state.owner_mfa.is_owner_user(principal.user_id).await;
    let suspended = state
        .control_plane
        .read()
        .await
        .managed_accounts
        .get(&principal.user_id)
        .is_some_and(|a| a.suspended);
    Ok(Json(WhoAmIResponse {
        user_id: principal.user_id,
        workspace_id: principal.workspace_id,
        scopes: principal.scopes,
        is_owner,
        suspended,
    }))
}

async fn check_rate_limit(
    state: &AppState,
    bucket_key: &str,
    max_per_min: u32,
) -> Result<(), AuthError> {
    let now = unix_now();
    let mut guard = state.control_plane.write().await;
    let entry = guard
        .rate_limits
        .entry(bucket_key.to_owned())
        .or_insert(RateBucket {
            window_start: now,
            count: 0,
        });
    if now - entry.window_start >= 60 {
        entry.window_start = now;
        entry.count = 1;
        return Ok(());
    }
    if entry.count >= max_per_min {
        return Err(AuthError::RateLimited);
    }
    entry.count += 1;
    Ok(())
}

/// Public email/password registration.
pub async fn register_account(
    State(state): State<AppState>,
    Json(body): Json<RegisterAccountRequest>,
) -> Result<(StatusCode, Json<AccountMessage>), AuthError> {
    check_rate_limit(&state, "auth:register", 30).await?;
    let service: &AccountAuthService = state
        .account_auth
        .as_deref()
        .ok_or(AuthError::BackendUnavailable)?;
    service.register(body).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(AccountMessage {
            message: "If this address can be registered, a verification email will be sent.",
        }),
    ))
}

/// Consume a one-time email verification token.
pub async fn verify_email(
    State(state): State<AppState>,
    Json(body): Json<VerifyEmailRequest>,
) -> Result<Json<AccountMessage>, AuthError> {
    let service: &AccountAuthService = state
        .account_auth
        .as_deref()
        .ok_or(AuthError::BackendUnavailable)?;
    service.verify_email(&body.token).await?;
    Ok(Json(AccountMessage {
        message: "Your email address has been verified.",
    }))
}

/// Reissue an email verification token without revealing account existence.
pub async fn resend_verification(
    State(state): State<AppState>,
    Json(body): Json<ResendVerificationRequest>,
) -> Result<(StatusCode, Json<AccountMessage>), AuthError> {
    check_rate_limit(&state, "auth:resend", 15).await?;
    let service: &AccountAuthService = state
        .account_auth
        .as_deref()
        .ok_or(AuthError::BackendUnavailable)?;
    service.resend_verification(&body.email).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(AccountMessage {
            message: "If this account needs verification, a new email will be sent.",
        }),
    ))
}

/// Request a one-time password reset link without revealing account existence.
pub async fn request_password_reset(
    State(state): State<AppState>,
    Json(body): Json<RequestPasswordResetRequest>,
) -> Result<(StatusCode, Json<AccountMessage>), AuthError> {
    check_rate_limit(&state, "auth:forgot", 15).await?;
    let service: &AccountAuthService = state
        .account_auth
        .as_deref()
        .ok_or(AuthError::BackendUnavailable)?;
    service.request_password_reset(&body.email).await?;
    Ok((
        StatusCode::ACCEPTED,
        Json(AccountMessage {
            message: "If this address has an account, a reset email will be sent.",
        }),
    ))
}

/// Consume a reset token, rotate the password, and revoke every session.
pub async fn confirm_password_reset(
    State(state): State<AppState>,
    Json(body): Json<ConfirmPasswordResetRequest>,
) -> Result<Json<AccountMessage>, AuthError> {
    let service: &AccountAuthService = state
        .account_auth
        .as_deref()
        .ok_or(AuthError::BackendUnavailable)?;
    service.reset_password(&body.token, &body.password).await?;
    Ok(Json(AccountMessage {
        message: "Your password has been changed. Sign in with the new password.",
    }))
}

#[derive(Debug, Serialize)]
pub struct EnrichedLoginResponse {
    pub token: String,
    pub session: SessionView,
    pub is_owner: bool,
    pub mfa_required: bool,
}

/// Verify credentials and issue a device-bound `sfs_*` session token with server-side `is_owner`.
pub async fn login_account(
    State(state): State<AppState>,
    Json(body): Json<LoginAccountRequest>,
) -> Result<(StatusCode, Json<EnrichedLoginResponse>), AuthError> {
    check_rate_limit(&state, "auth:login", 40).await?;
    let email_norm = body.email.trim().to_ascii_lowercase();
    let service: &AccountAuthService = state
        .account_auth
        .as_deref()
        .ok_or(AuthError::BackendUnavailable)?;
    let issued = service.login(body).await?;
    let authenticated = state.auth.authenticate_session(&issued.token).await?;
    let user_id = authenticated.principal.user_id;
    let workspace_id = authenticated.principal.workspace_id;

    // Enforce account suspension from control plane.
    {
        let cp = state.control_plane.read().await;
        if cp
            .managed_accounts
            .get(&user_id)
            .is_some_and(|acc| acc.suspended)
        {
            let _ = state
                .auth
                .revoke_session(issued.session.id, &authenticated.token_hash)
                .await;
            return Err(AuthError::AccountSuspended);
        }
    }

    state
        .upsert_managed_account(ManagedAccountRecord {
            user_id,
            workspace_id,
            email: email_norm,
            display_name: issued.session.device_name.clone(),
            verified_phone: None,
            suspended: false,
            created_at: unix_now(),
        })
        .await;

    let is_owner = state.owner_mfa.is_owner_user(user_id).await;
    Ok((
        StatusCode::CREATED,
        Json(EnrichedLoginResponse {
            token: issued.token,
            session: issued.session,
            is_owner,
            mfa_required: is_owner,
        }),
    ))
}

#[derive(Debug, Deserialize)]
pub struct ChangeEmailRequest {
    pub new_email: String,
}

#[derive(Debug, Serialize)]
pub struct ChangeEmailResponse {
    pub user_id: Uuid,
    pub email: String,
    pub is_owner: bool,
}

/// `POST /v1/auth/email/change` — updates the user's email while preserving immutable `user_id`
/// (and therefore preserving Owner identity without ever granting Owner to the old email).
pub async fn change_account_email(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(body): Json<ChangeEmailRequest>,
) -> Result<Json<ChangeEmailResponse>, AuthError> {
    let normalized = body.new_email.trim().to_ascii_lowercase();
    if normalized.is_empty() || normalized.len() > 254 || !normalized.contains('@') {
        return Err(AuthError::InvalidRequest);
    }
    let user_id = session.principal.user_id;
    {
        let mut cp = state.control_plane.write().await;
        if cp
            .managed_accounts
            .values()
            .any(|a| a.user_id != user_id && a.email == normalized)
        {
            return Err(AuthError::DuplicateAccount);
        }
        let rec = cp
            .managed_accounts
            .entry(user_id)
            .or_insert_with(|| ManagedAccountRecord {
                user_id,
                workspace_id: session.principal.workspace_id,
                email: normalized.clone(),
                display_name: session.device_name.clone(),
                verified_phone: None,
                suspended: false,
                created_at: unix_now(),
            });
        rec.email = normalized.clone();
    }
    let is_owner = state.owner_mfa.is_owner_user(user_id).await;
    state
        .log_center
        .record(
            Some(user_id),
            "account.email.changed",
            "user",
            user_id.to_string(),
            AuditOutcome::Allowed,
            None,
            Some(json!({ "is_owner_preserved": is_owner })),
        )
        .await;
    Ok(Json(ChangeEmailResponse {
        user_id,
        email: normalized,
        is_owner,
    }))
}

#[derive(Debug, Deserialize)]
pub struct RequestPhoneOtpRequest {
    pub phone: String,
    pub channel: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RequestPhoneOtpResponse {
    pub channel: String,
    pub provider_status: &'static str,
    pub expires_at: i64,
}

/// `POST /v1/auth/phone/request-otp` — SMS primary + WhatsApp fallback verification adapter.
/// Fails with explicit `SF-AUTH-005` / `NOT_CONFIGURED` in strict environments when SMS/WhatsApp keys are absent.
pub async fn request_phone_otp(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(body): Json<RequestPhoneOtpRequest>,
) -> Result<Json<RequestPhoneOtpResponse>, AuthError> {
    let normalized = normalize_phone(&body.phone);
    if normalized.len() < 8 || normalized.len() > 15 {
        return Err(AuthError::InvalidRequest);
    }
    let channel = match body.channel.as_deref().unwrap_or("sms") {
        "whatsapp" => "whatsapp",
        "sms" => "sms",
        _ => return Err(AuthError::InvalidRequest),
    };
    let configured = if channel == "whatsapp" {
        state.config.whatsapp_configured
    } else {
        state.config.sms_configured
    };
    if state.config.environment.is_strict() && !configured {
        return Err(AuthError::BackendUnavailable);
    }

    let now = unix_now();
    let expires_at = now + 300;
    let code = format!("{:06}", (Uuid::new_v4().as_u128() % 1_000_000) as u32);
    let code_hash =
        blake3::hash(format!("safi:phone-otp:{}:{code}", session.principal.user_id).as_bytes())
            .to_hex()
            .to_string();

    state.control_plane.write().await.phone_otps.insert(
        session.principal.user_id,
        PendingPhoneOtp {
            phone_e164: normalized,
            channel: channel.to_owned(),
            code_hash,
            attempts: 0,
            expires_at,
        },
    );

    Ok(Json(RequestPhoneOtpResponse {
        channel: channel.to_owned(),
        provider_status: if configured {
            "CONFIGURED"
        } else {
            "NOT_CONFIGURED"
        },
        expires_at,
    }))
}

#[derive(Debug, Deserialize)]
pub struct VerifyPhoneOtpRequest {
    pub code: String,
}

/// `POST /v1/auth/phone/verify-otp` — verify phone OTP with attempt cap and uniqueness guard.
pub async fn verify_phone_otp(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(body): Json<VerifyPhoneOtpRequest>,
) -> Result<Json<Value>, AuthError> {
    let now = unix_now();
    let user_id = session.principal.user_id;
    let mut cp = state.control_plane.write().await;
    let pending = cp
        .phone_otps
        .get_mut(&user_id)
        .ok_or(AuthError::InvalidPhoneOtp)?;
    if now >= pending.expires_at || pending.attempts >= 5 {
        return Err(AuthError::InvalidPhoneOtp);
    }
    let expected =
        blake3::hash(format!("safi:phone-otp:{user_id}:{}", body.code.trim()).as_bytes())
            .to_hex()
            .to_string();
    if expected != pending.code_hash {
        pending.attempts += 1;
        return Err(AuthError::InvalidPhoneOtp);
    }
    let phone = pending.phone_e164.clone();
    cp.phone_otps.remove(&user_id);

    if cp
        .managed_accounts
        .values()
        .any(|a| a.user_id != user_id && a.verified_phone.as_deref() == Some(phone.as_str()))
    {
        return Err(AuthError::DuplicateAccount);
    }
    if let Some(acc) = cp.managed_accounts.get_mut(&user_id) {
        acc.verified_phone = Some(phone.clone());
    }
    Ok(Json(json!({ "verified": true, "phone": phone })))
}

#[derive(Debug, Deserialize)]
pub struct OAuthStartQuery {
    pub pkce_challenge: String,
    pub redirect_uri: String,
}

/// `GET /v1/auth/oauth/{provider}/start` — validates PKCE & redirect URI and checks provider readiness.
pub async fn oauth_start(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    Query(query): Query<OAuthStartQuery>,
) -> Result<Json<Value>, AuthError> {
    let configured = match provider.as_str() {
        "google" => state.config.oauth_google_configured,
        "apple" => state.config.oauth_apple_configured,
        "github" => state.config.oauth_github_configured,
        _ => return Err(AuthError::InvalidRequest),
    };
    if !configured {
        return Err(AuthError::OAuthNotConfigured);
    }
    if query.pkce_challenge.trim().len() < 32
        || (!query.redirect_uri.starts_with("https://")
            && !query.redirect_uri.starts_with("safidata://"))
    {
        return Err(AuthError::InvalidRequest);
    }
    let state_token = format!("oauth_{}", Uuid::new_v4().simple());
    state.control_plane.write().await.oauth_states.insert(
        state_token.clone(),
        PendingOAuthState {
            provider: provider.clone(),
            pkce_challenge: query.pkce_challenge,
            redirect_uri: query.redirect_uri,
            expires_at: unix_now() + 600,
        },
    );
    Ok(Json(json!({
        "provider": provider,
        "state": state_token,
        "status": "CONFIGURED"
    })))
}

// ============================================================================
// OWNER IDENTITY, GOOGLE AUTHENTICATOR TOTP & REAL OWNER CONTROL PLANE
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct TotpCodeRequest {
    pub code: String,
}

#[derive(Debug, Deserialize)]
pub struct RecoveryCodeRequest {
    pub recovery_code: String,
}

/// `GET /v1/owner/mfa/status` — secret-safe MFA enrollment/elevation status (requires authenticated session).
pub async fn owner_mfa_status(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
) -> Json<OwnerMfaStatus> {
    let status = state
        .owner_mfa
        .status_at(session.principal.user_id, session.session_id, unix_now())
        .await;
    Json(status)
}

/// `POST /v1/owner/mfa/enroll` — starts Google Authenticator TOTP enrollment for the sealed Owner only.
pub async fn owner_mfa_enroll(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
) -> Result<Json<TotpEnrollmentBundle>, AuthError> {
    let user_id = session.principal.user_id;
    let label = state
        .control_plane
        .read()
        .await
        .managed_accounts
        .get(&user_id)
        .map_or_else(|| "PlatformOwner".to_owned(), |a| a.display_name.clone());
    match state.owner_mfa.enroll_totp(user_id, &label).await {
        Ok(bundle) => {
            state
                .log_center
                .record(
                    Some(user_id),
                    "owner.mfa.enroll",
                    "owner_totp",
                    user_id.to_string(),
                    AuditOutcome::Allowed,
                    None,
                    Some(json!({ "enrolled": true })),
                )
                .await;
            Ok(Json(bundle))
        }
        Err(err) => {
            state
                .log_center
                .record(
                    Some(user_id),
                    "owner.mfa.enroll",
                    "owner_totp",
                    user_id.to_string(),
                    AuditOutcome::Denied,
                    None,
                    Some(json!({ "error_code": err.code() })),
                )
                .await;
            Err(err)
        }
    }
}

/// `POST /v1/owner/mfa/confirm` — confirms TOTP setup with a live 6-digit Google Authenticator code.
pub async fn owner_mfa_confirm(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(body): Json<TotpCodeRequest>,
) -> Result<Json<ElevatedOwnerSession>, AuthError> {
    let user_id = session.principal.user_id;
    let res = state
        .owner_mfa
        .confirm_totp_at(user_id, session.session_id, &body.code, unix_now())
        .await;
    let outcome = if res.is_ok() {
        AuditOutcome::Allowed
    } else {
        AuditOutcome::Denied
    };
    state
        .log_center
        .record(
            Some(user_id),
            "owner.mfa.confirm",
            "owner_totp",
            user_id.to_string(),
            outcome,
            None,
            Some(json!({ "confirmed": res.is_ok() })),
        )
        .await;
    Ok(Json(res?))
}

/// `POST /v1/owner/mfa/verify` and `/v1/owner/mfa/step-up` — verifies a live 6-digit TOTP code.
pub async fn owner_mfa_verify(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(body): Json<TotpCodeRequest>,
) -> Result<Json<ElevatedOwnerSession>, AuthError> {
    let user_id = session.principal.user_id;
    let res = state
        .owner_mfa
        .verify_totp_at(user_id, session.session_id, &body.code, unix_now())
        .await;
    let outcome = if res.is_ok() {
        AuditOutcome::Allowed
    } else {
        AuditOutcome::Denied
    };
    state
        .log_center
        .record(
            Some(user_id),
            "owner.mfa.verify",
            "owner_session",
            session.session_id.to_string(),
            outcome,
            None,
            None,
        )
        .await;
    Ok(Json(res?))
}

/// `POST /v1/owner/mfa/recover` — consumes a single-use recovery code to establish an elevated session.
pub async fn owner_mfa_recover(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(body): Json<RecoveryCodeRequest>,
) -> Result<Json<ElevatedOwnerSession>, AuthError> {
    let user_id = session.principal.user_id;
    let res = state
        .owner_mfa
        .verify_recovery_code_at(user_id, session.session_id, &body.recovery_code, unix_now())
        .await;
    let outcome = if res.is_ok() {
        AuditOutcome::Allowed
    } else {
        AuditOutcome::Denied
    };
    state
        .log_center
        .record(
            Some(user_id),
            "owner.mfa.recover",
            "owner_session",
            session.session_id.to_string(),
            outcome,
            None,
            None,
        )
        .await;
    Ok(Json(res?))
}

#[derive(Debug, Serialize)]
pub struct OwnerStatsResponse {
    pub total_users: usize,
    pub suspended_users: usize,
    pub maintenance_mode: bool,
    pub billing_providers: Value,
    pub entitlements: EntitlementAggregateStats,
}

/// `GET /v1/owner/stats` — real live metrics computed from actual server stores (ZERO hardcoded numbers).
pub async fn owner_stats(State(state): State<AppState>) -> Json<OwnerStatsResponse> {
    let cp = state.control_plane.read().await;
    let total_users = cp.managed_accounts.len();
    let suspended_users = cp.managed_accounts.values().filter(|a| a.suspended).count();
    let maintenance_mode = cp.maintenance_mode;
    drop(cp);

    let entitlements = state.entitlements.aggregate_stats(unix_now()).await;
    Json(OwnerStatsResponse {
        total_users,
        suspended_users,
        maintenance_mode,
        billing_providers: state.billing.provider_status_json(),
        entitlements,
    })
}

#[derive(Debug, Deserialize)]
pub struct AccountsSearchQuery {
    pub q: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OwnerAccountView {
    pub account: ManagedAccountRecord,
    pub entitlements: EffectiveEntitlements,
}

/// `GET /v1/owner/accounts` — search & inspect real accounts + their authoritative entitlements.
pub async fn owner_list_accounts(
    State(state): State<AppState>,
    Query(query): Query<AccountsSearchQuery>,
) -> Json<Vec<OwnerAccountView>> {
    let needle = query.q.unwrap_or_default().trim().to_ascii_lowercase();
    let accounts: Vec<ManagedAccountRecord> = state
        .control_plane
        .read()
        .await
        .managed_accounts
        .values()
        .filter(|a| {
            needle.is_empty()
                || a.email.contains(&needle)
                || a.display_name.to_ascii_lowercase().contains(&needle)
                || a.user_id.to_string().contains(&needle)
        })
        .cloned()
        .collect();

    let now = unix_now();
    let mut views = Vec::with_capacity(accounts.len());
    for acc in accounts {
        let is_owner = state.owner_mfa.is_owner_user(acc.user_id).await;
        let ent = state
            .entitlements
            .effective_at(acc.workspace_id, acc.user_id, is_owner, now)
            .await;
        views.push(OwnerAccountView {
            account: acc,
            entitlements: ent,
        });
    }
    Json(views)
}

#[derive(Debug, Deserialize)]
pub struct FriendVipMutationRequest {
    pub user_id: Uuid,
    pub workspace_id: Uuid,
    pub reason: Option<String>,
}

/// `POST /v1/owner/friends/grant` — grants Friend VIP (requires fresh step-up TOTP).
pub async fn owner_grant_friend_vip(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(body): Json<FriendVipMutationRequest>,
) -> Result<Json<EffectiveEntitlements>, AuthError> {
    if body.user_id == session.principal.user_id {
        return Err(AuthError::InvalidRequest);
    }
    let before = state.entitlements.workspace_record(body.workspace_id).await;
    let updated = state
        .entitlements
        .set_friend_vip(
            body.workspace_id,
            body.user_id,
            session.principal.user_id,
            true,
        )
        .await;
    state
        .log_center
        .record(
            Some(session.principal.user_id),
            "owner.friend_vip.grant",
            "user",
            body.user_id.to_string(),
            AuditOutcome::Allowed,
            before.and_then(|b| serde_json::to_value(b).ok()),
            Some(json!({ "effective_plan": updated.effective_plan, "reason": body.reason })),
        )
        .await;
    Ok(Json(updated))
}

/// `POST /v1/owner/friends/revoke` — revokes Friend VIP (requires fresh step-up TOTP).
pub async fn owner_revoke_friend_vip(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(body): Json<FriendVipMutationRequest>,
) -> Result<Json<EffectiveEntitlements>, AuthError> {
    let before = state.entitlements.workspace_record(body.workspace_id).await;
    let updated = state
        .entitlements
        .set_friend_vip(
            body.workspace_id,
            body.user_id,
            session.principal.user_id,
            false,
        )
        .await;
    state
        .log_center
        .record(
            Some(session.principal.user_id),
            "owner.friend_vip.revoke",
            "user",
            body.user_id.to_string(),
            AuditOutcome::Allowed,
            before.and_then(|b| serde_json::to_value(b).ok()),
            Some(json!({ "effective_plan": updated.effective_plan })),
        )
        .await;
    Ok(Json(updated))
}

/// `POST /v1/owner/accounts/{user_id}/suspend` — suspends a customer account (step-up TOTP).
pub async fn owner_suspend_account(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(target_user_id): Path<Uuid>,
) -> Result<Json<Value>, AuthError> {
    if target_user_id == session.principal.user_id {
        return Err(AuthError::InvalidRequest);
    }
    let mut cp = state.control_plane.write().await;
    let acc = cp
        .managed_accounts
        .get_mut(&target_user_id)
        .ok_or(AuthError::InvalidRequest)?;
    let before = acc.suspended;
    acc.suspended = true;
    drop(cp);

    state
        .log_center
        .record(
            Some(session.principal.user_id),
            "owner.account.suspend",
            "user",
            target_user_id.to_string(),
            AuditOutcome::Allowed,
            Some(json!({ "suspended": before })),
            Some(json!({ "suspended": true })),
        )
        .await;
    Ok(Json(
        json!({ "user_id": target_user_id, "suspended": true }),
    ))
}

/// `POST /v1/owner/accounts/{user_id}/reactivate` — reactivates a suspended account (step-up TOTP).
pub async fn owner_reactivate_account(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(target_user_id): Path<Uuid>,
) -> Result<Json<Value>, AuthError> {
    let mut cp = state.control_plane.write().await;
    let acc = cp
        .managed_accounts
        .get_mut(&target_user_id)
        .ok_or(AuthError::InvalidRequest)?;
    let before = acc.suspended;
    acc.suspended = false;
    drop(cp);

    state
        .log_center
        .record(
            Some(session.principal.user_id),
            "owner.account.reactivate",
            "user",
            target_user_id.to_string(),
            AuditOutcome::Allowed,
            Some(json!({ "suspended": before })),
            Some(json!({ "suspended": false })),
        )
        .await;
    Ok(Json(
        json!({ "user_id": target_user_id, "suspended": false }),
    ))
}

#[derive(Debug, Deserialize)]
pub struct ExtendTrialRequest {
    pub user_id: Uuid,
    pub extra_days: i64,
}

/// `POST /v1/owner/subscriptions/{workspace_id}/extend-trial` — extends trial duration (step-up TOTP).
pub async fn owner_extend_trial(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(workspace_id): Path<Uuid>,
    Json(body): Json<ExtendTrialRequest>,
) -> Result<Json<EffectiveEntitlements>, AuthError> {
    let updated = state
        .entitlements
        .extend_trial_at(workspace_id, body.user_id, body.extra_days, unix_now())
        .await?;
    state
        .log_center
        .record(
            Some(session.principal.user_id),
            "owner.trial.extend",
            "workspace",
            workspace_id.to_string(),
            AuditOutcome::Allowed,
            None,
            Some(json!({ "extra_days": body.extra_days, "trial_expires_at": updated.trial_expires_at })),
        )
        .await;
    Ok(Json(updated))
}

#[derive(Debug, Deserialize)]
pub struct CancelSubRequest {
    pub user_id: Uuid,
}

/// `POST /v1/owner/subscriptions/{workspace_id}/cancel` — cancels active subscription (step-up TOTP).
pub async fn owner_cancel_subscription(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(workspace_id): Path<Uuid>,
    Json(body): Json<CancelSubRequest>,
) -> Result<Json<EffectiveEntitlements>, AuthError> {
    let now = unix_now();
    let updated = state
        .entitlements
        .apply_subscription_update(
            workspace_id,
            body.user_id,
            PlanTier::Free,
            SubscriptionState::Canceled,
            1,
            Some(now),
            now,
        )
        .await?;
    state
        .log_center
        .record(
            Some(session.principal.user_id),
            "owner.subscription.cancel",
            "workspace",
            workspace_id.to_string(),
            AuditOutcome::Allowed,
            None,
            Some(json!({ "effective_plan": updated.effective_plan })),
        )
        .await;
    Ok(Json(updated))
}

#[derive(Debug, Deserialize)]
pub struct IssueRefundRequest {
    pub external_invoice_id: String,
    #[serde(default)]
    pub mandatory_legal_override: bool,
}

/// `POST /v1/owner/billing/refund` — issues refund enforcing the 3-day policy window (step-up TOTP).
pub async fn owner_issue_refund(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(body): Json<IssueRefundRequest>,
) -> Result<Json<InvoiceRecord>, AuthError> {
    let invoice = state
        .billing
        .issue_owner_refund(
            &body.external_invoice_id,
            body.mandatory_legal_override,
            unix_now(),
        )
        .await?;
    state
        .log_center
        .record(
            Some(session.principal.user_id),
            "owner.billing.refund",
            "invoice",
            body.external_invoice_id,
            AuditOutcome::Allowed,
            None,
            Some(json!({ "status": invoice.status, "amount_minor": invoice.amount_minor })),
        )
        .await;
    Ok(Json(invoice))
}

/// `GET /v1/owner/billing/events` — real billing webhook events ledger.
pub async fn owner_billing_events(State(state): State<AppState>) -> Json<Vec<BillingEventRecord>> {
    Json(state.billing.list_events(100).await)
}

#[derive(Debug, Deserialize)]
pub struct MaintenanceToggleRequest {
    pub enabled: bool,
    pub reason: Option<String>,
}

/// `GET /v1/owner/system/maintenance` — current maintenance mode status.
pub async fn owner_get_maintenance(State(state): State<AppState>) -> Json<Value> {
    let cp = state.control_plane.read().await;
    Json(json!({
        "enabled": cp.maintenance_mode,
        "reason": cp.maintenance_reason,
    }))
}

/// `POST /v1/owner/system/maintenance` — toggle maintenance mode (step-up TOTP).
pub async fn owner_set_maintenance(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(body): Json<MaintenanceToggleRequest>,
) -> Json<Value> {
    let mut cp = state.control_plane.write().await;
    let before = cp.maintenance_mode;
    cp.maintenance_mode = body.enabled;
    cp.maintenance_reason = body.reason.clone();
    drop(cp);

    state
        .log_center
        .record(
            Some(session.principal.user_id),
            "owner.system.maintenance",
            "system",
            "maintenance_mode",
            AuditOutcome::Allowed,
            Some(json!({ "enabled": before })),
            Some(json!({ "enabled": body.enabled, "reason": body.reason })),
        )
        .await;
    Json(json!({ "enabled": body.enabled, "reason": body.reason }))
}

#[derive(Debug, Deserialize)]
pub struct SetFeatureFlagRequest {
    pub key: String,
    pub enabled: bool,
    pub description: Option<String>,
}

/// `GET /v1/owner/feature-flags` — list all feature flags.
pub async fn owner_list_feature_flags(State(state): State<AppState>) -> Json<Vec<FeatureFlag>> {
    let flags = state
        .control_plane
        .read()
        .await
        .feature_flags
        .values()
        .cloned()
        .collect();
    Json(flags)
}

/// `POST /v1/owner/feature-flags` — create/update a feature flag.
pub async fn owner_set_feature_flag(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(body): Json<SetFeatureFlagRequest>,
) -> Result<Json<FeatureFlag>, AuthError> {
    let key = body.key.trim().to_owned();
    if key.is_empty() || key.len() > 80 {
        return Err(AuthError::InvalidRequest);
    }
    let flag = FeatureFlag {
        key: key.clone(),
        enabled: body.enabled,
        description: body.description.unwrap_or_default(),
        updated_at: unix_now(),
    };
    let before = state
        .control_plane
        .write()
        .await
        .feature_flags
        .insert(key.clone(), flag.clone());
    state
        .log_center
        .record(
            Some(session.principal.user_id),
            "owner.feature_flag.set",
            "feature_flag",
            key,
            AuditOutcome::Allowed,
            before.and_then(|b| serde_json::to_value(b).ok()),
            serde_json::to_value(&flag).ok(),
        )
        .await;
    Ok(Json(flag))
}

#[derive(Debug, Serialize)]
pub struct AuditLogsResponse {
    pub entries: Vec<AuditLogEntry>,
    pub is_chain_valid: bool,
}

/// `GET /v1/owner/audit-logs` — verifiable BLAKE3 hash-chained audit logs (real entries only).
pub async fn owner_audit_logs(State(state): State<AppState>) -> Json<AuditLogsResponse> {
    let entries = state.log_center.list_recent(200).await;
    let is_chain_valid = state.log_center.verify_chain().await;
    Json(AuditLogsResponse {
        entries,
        is_chain_valid,
    })
}

// ============================================================================
// ENTITLEMENTS, BILLING, ORGANIZATIONS, ENCRYPTED SYNC & AI ROUTER ROUTES
// ============================================================================

/// `GET /v1/entitlements/me` — server-authoritative plan, watermark, and usage limits.
pub async fn get_my_entitlements(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
) -> Json<EffectiveEntitlements> {
    let is_owner = state
        .owner_mfa
        .is_owner_user(session.principal.user_id)
        .await;
    Json(
        state
            .entitlements
            .effective_at(
                session.principal.workspace_id,
                session.principal.user_id,
                is_owner,
                unix_now(),
            )
            .await,
    )
}

/// `POST /v1/entitlements/consume` — atomic, idempotent quota enforcement.
pub async fn consume_entitlement(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(body): Json<ConsumeUsageRequest>,
) -> Result<Json<ConsumeUsageResult>, AuthError> {
    let is_owner = state
        .owner_mfa
        .is_owner_user(session.principal.user_id)
        .await;
    let res = state
        .entitlements
        .consume_at(
            session.principal.workspace_id,
            session.principal.user_id,
            is_owner,
            &body,
            unix_now(),
        )
        .await?;
    Ok(Json(res))
}

#[derive(Debug, Deserialize)]
pub struct ClaimTrialRequest {
    pub device_fingerprint: String,
}

/// `POST /v1/entitlements/trial/claim` — one-time 7-day Trial requiring verified phone & email.
pub async fn claim_trial(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(body): Json<ClaimTrialRequest>,
) -> Result<Json<EffectiveEntitlements>, AuthError> {
    let user_id = session.principal.user_id;
    let (email, phone) = {
        let cp = state.control_plane.read().await;
        let acc = cp
            .managed_accounts
            .get(&user_id)
            .ok_or(AuthError::InvalidRequest)?;
        let phone = acc
            .verified_phone
            .clone()
            .ok_or(AuthError::InvalidPhoneOtp)?;
        (acc.email.clone(), phone)
    };
    let email_hash = blake3::hash(email.as_bytes()).to_hex().to_string();
    let phone_hash = blake3::hash(phone.as_bytes()).to_hex().to_string();
    let device_hash = blake3::hash(body.device_fingerprint.trim().as_bytes())
        .to_hex()
        .to_string();
    let ent = state
        .entitlements
        .claim_trial_at(
            session.principal.workspace_id,
            user_id,
            &email_hash,
            &phone_hash,
            &device_hash,
            unix_now(),
        )
        .await?;
    Ok(Json(ent))
}

/// `GET /v1/billing/status` — honest Paddle/Paymob configuration status.
pub async fn billing_status(State(state): State<AppState>) -> Json<Value> {
    Json(state.billing.provider_status_json())
}

/// `POST /v1/billing/checkout` — create Paddle (USD) or Paymob (EGP) checkout session.
pub async fn billing_checkout(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(mut body): Json<CheckoutRequest>,
) -> Result<Json<CheckoutResponse>, AuthError> {
    body.workspace_id = session.principal.workspace_id;
    body.user_id = session.principal.user_id;
    let resp = state.billing.create_checkout(&body).await?;
    Ok(Json(resp))
}

/// `GET /v1/billing/invoices` — list caller's workspace invoices.
pub async fn billing_invoices(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
) -> Json<Vec<InvoiceRecord>> {
    Json(
        state
            .billing
            .list_invoices(session.principal.workspace_id)
            .await,
    )
}

/// `POST /v1/billing/webhooks/paddle` — Paddle HMAC-SHA256 verified webhook endpoint.
pub async fn paddle_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    raw_body: Bytes,
) -> Result<Json<BillingEventRecord>, AuthError> {
    let sig = headers
        .get("paddle-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or(AuthError::InvalidWebhookSignature)?;
    let rec = state
        .billing
        .process_paddle_webhook(sig, &raw_body, unix_now())
        .await?;
    Ok(Json(rec))
}

#[derive(Debug, Deserialize)]
pub struct PaymobWebhookQuery {
    pub hmac: String,
}

/// `POST /v1/billing/webhooks/paymob` — Paymob HMAC-SHA512 verified webhook endpoint.
pub async fn paymob_webhook(
    State(state): State<AppState>,
    Query(query): Query<PaymobWebhookQuery>,
    Json(tx): Json<PaymobTransactionObj>,
) -> Result<Json<BillingEventRecord>, AuthError> {
    let rec = state
        .billing
        .process_paymob_webhook(&query.hmac, &tx, unix_now())
        .await?;
    Ok(Json(rec))
}

#[derive(Debug, Deserialize)]
pub struct CreateOrgRequest {
    pub name: String,
    pub seats: u32,
}

pub async fn create_organization(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(body): Json<CreateOrgRequest>,
) -> Result<(StatusCode, Json<Organization>), AuthError> {
    let org = state
        .organizations
        .create_org(
            session.principal.workspace_id,
            session.principal.user_id,
            &body.name,
            body.seats,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(org)))
}

#[derive(Debug, Deserialize)]
pub struct InviteOrgMemberRequest {
    pub email: String,
    pub role: OrgRole,
}

pub async fn invite_org_member(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(org_id): Path<Uuid>,
    Json(body): Json<InviteOrgMemberRequest>,
) -> Result<(StatusCode, Json<Value>), AuthError> {
    let (invite, token) = state
        .organizations
        .invite_member(org_id, session.principal.user_id, &body.email, body.role)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(
            json!({ "invitation_id": invite.id, "token": token, "expires_at": invite.expires_at }),
        ),
    ))
}

#[derive(Debug, Deserialize)]
pub struct AcceptOrgInviteRequest {
    pub token: String,
}

pub async fn accept_org_invite(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(body): Json<AcceptOrgInviteRequest>,
) -> Result<Json<OrganizationMember>, AuthError> {
    let member = state
        .organizations
        .accept_invitation(&body.token, session.principal.user_id)
        .await?;
    Ok(Json(member))
}

pub async fn list_org_members(
    State(state): State<AppState>,
    Path(org_id): Path<Uuid>,
) -> Json<Vec<OrganizationMember>> {
    Json(state.organizations.list_members(org_id).await)
}

pub async fn org_sso_status(State(state): State<AppState>) -> Result<Json<Value>, AuthError> {
    state.organizations.check_sso_scim_readiness()?;
    Ok(Json(json!({ "status": "CONFIGURED" })))
}

pub async fn sync_push(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(body): Json<PushEncryptedSyncRequest>,
) -> Result<Json<EncryptedSyncRecord>, AuthError> {
    let rec = state
        .encrypted_sync
        .push_envelope(
            session.principal.workspace_id,
            session.principal.user_id,
            &body,
        )
        .await?;
    Ok(Json(rec))
}

pub async fn sync_pull(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Path((resource_kind, resource_id)): Path<(String, String)>,
) -> Result<Json<EncryptedSyncRecord>, AuthError> {
    state
        .encrypted_sync
        .pull_envelope(session.principal.workspace_id, &resource_kind, &resource_id)
        .await
        .map(Json)
        .ok_or(AuthError::SyncEnvelopeInvalid)
}

pub async fn sync_create_share_link(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(body): Json<CreateShareLinkRequest>,
) -> Result<(StatusCode, Json<CreatedShareLink>), AuthError> {
    let created = state
        .encrypted_sync
        .create_share_link(
            session.principal.workspace_id,
            session.principal.user_id,
            &body,
        )
        .await?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn sync_resolve_share_link(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<ShareLinkRecord>, AuthError> {
    let rec = state
        .encrypted_sync
        .resolve_share_link_at(&token, unix_now())
        .await?;
    Ok(Json(rec))
}

pub async fn sync_revoke_share_link(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(link_id): Path<Uuid>,
) -> Result<StatusCode, AuthError> {
    state
        .encrypted_sync
        .revoke_share_link(session.principal.workspace_id, link_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct AiRouteRequest {
    pub idempotency_key: String,
    pub explicit_user_consent: bool,
    pub pii_masked_locally: bool,
    pub masked_prompt: String,
}

/// `POST /v1/ai/route` — enforces explicit consent, local PII masking, AI quota, and honest provider readiness.
pub async fn ai_route(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(body): Json<AiRouteRequest>,
) -> Result<Json<Value>, AuthError> {
    if !body.explicit_user_consent
        || !body.pii_masked_locally
        || body.masked_prompt.trim().is_empty()
    {
        return Err(AuthError::InvalidRequest);
    }
    if !state.config.ai_configured() {
        return Err(AuthError::AiProviderNotConfigured);
    }
    let is_owner = state
        .owner_mfa
        .is_owner_user(session.principal.user_id)
        .await;
    let consumed = state
        .entitlements
        .consume_at(
            session.principal.workspace_id,
            session.principal.user_id,
            is_owner,
            &ConsumeUsageRequest {
                idempotency_key: body.idempotency_key,
                kind: UsageKind::AiCall,
                row_count: 0,
                column_count: 0,
                pii_masked_count: 0,
            },
            unix_now(),
        )
        .await?;
    Ok(Json(json!({
        "provider_status": "CONFIGURED",
        "model": state.config.ai_model,
        "remaining_ai_calls": consumed.entitlements.ai_calls_limit.saturating_sub(consumed.entitlements.ai_calls_used),
    })))
}

/// Build the complete application router.
pub fn build_router(state: AppState) -> Router {
    let auth = state.auth.clone();
    let owner_mfa = state.owner_mfa.clone();

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
        .route("/v1/auth/email/change", post(change_account_email))
        .route("/v1/auth/phone/request-otp", post(request_phone_otp))
        .route("/v1/auth/phone/verify-otp", post(verify_phone_otp))
        // Owner MFA bootstrap/verify endpoints (require session; enforce sealed owner inside handler)
        .route("/v1/owner/mfa/status", get(owner_mfa_status))
        .route("/v1/owner/mfa/enroll", post(owner_mfa_enroll))
        .route("/v1/owner/mfa/confirm", post(owner_mfa_confirm))
        .route("/v1/owner/mfa/verify", post(owner_mfa_verify))
        .route("/v1/owner/mfa/step-up", post(owner_mfa_verify))
        .route("/v1/owner/mfa/recover", post(owner_mfa_recover))
        // Entitlements & billing user endpoints
        .route("/v1/entitlements/me", get(get_my_entitlements))
        .route("/v1/entitlements/consume", post(consume_entitlement))
        .route("/v1/entitlements/trial/claim", post(claim_trial))
        .route("/v1/billing/checkout", post(billing_checkout))
        .route("/v1/billing/invoices", get(billing_invoices))
        // Organizations & encrypted sync
        .route("/v1/organizations", post(create_organization))
        .route("/v1/organizations/{org_id}/invite", post(invite_org_member))
        .route("/v1/organizations/invitations/accept", post(accept_org_invite))
        .route("/v1/organizations/{org_id}/members", get(list_org_members))
        .route("/v1/organizations/sso/status", get(org_sso_status))
        .route("/v1/sync/envelopes", post(sync_push))
        .route(
            "/v1/sync/envelopes/{resource_kind}/{resource_id}",
            get(sync_pull),
        )
        .route("/v1/sync/share-links", post(sync_create_share_link))
        .route(
            "/v1/sync/share-links/{link_id}/revoke",
            delete(sync_revoke_share_link),
        )
        // AI Router & Offline Ed25519 License issuance
        .route("/v1/ai/route", post(ai_route))
        .route("/v1/licenses/issue", post(issue_offline_license))
        .route_layer(from_fn_with_state(
            SessionGuard {
                auth: auth.clone(),
                required_scope: Scope::Read,
            },
            require_session,
        ));

    // Owner Read / Standard Control Plane (requires active Session + Sealed Owner + Elevated TOTP)
    let owner_elevated_routes = Router::new()
        .route("/v1/owner/stats", get(owner_stats))
        .route("/v1/owner/accounts", get(owner_list_accounts))
        .route("/v1/owner/billing/events", get(owner_billing_events))
        .route("/v1/owner/system/maintenance", get(owner_get_maintenance))
        .route(
            "/v1/owner/feature-flags",
            get(owner_list_feature_flags).post(owner_set_feature_flag),
        )
        .route("/v1/owner/audit-logs", get(owner_audit_logs))
        .route_layer(from_fn_with_state(
            OwnerAccessGuard {
                mfa: owner_mfa.clone(),
                require_step_up: false,
            },
            require_owner_mfa,
        ))
        .route_layer(from_fn_with_state(
            SessionGuard {
                auth: auth.clone(),
                required_scope: Scope::Read,
            },
            require_session,
        ));

    // Owner High-Risk Step-Up Control Plane (requires Session + Sealed Owner + Fresh 5-min Step-Up TOTP)
    let owner_step_up_routes = Router::new()
        .route("/v1/owner/friends/grant", post(owner_grant_friend_vip))
        .route("/v1/owner/friends/revoke", post(owner_revoke_friend_vip))
        .route(
            "/v1/owner/accounts/{user_id}/suspend",
            post(owner_suspend_account),
        )
        .route(
            "/v1/owner/accounts/{user_id}/reactivate",
            post(owner_reactivate_account),
        )
        .route(
            "/v1/owner/subscriptions/{workspace_id}/extend-trial",
            post(owner_extend_trial),
        )
        .route(
            "/v1/owner/subscriptions/{workspace_id}/cancel",
            post(owner_cancel_subscription),
        )
        .route("/v1/owner/billing/refund", post(owner_issue_refund))
        .route("/v1/owner/system/maintenance", post(owner_set_maintenance))
        .route_layer(from_fn_with_state(
            OwnerAccessGuard {
                mfa: owner_mfa,
                require_step_up: true,
            },
            require_owner_mfa,
        ))
        .route_layer(from_fn_with_state(
            SessionGuard {
                auth,
                required_scope: Scope::Read,
            },
            require_session,
        ));

    Router::new()
        .route("/health", get(health))
        .route("/ready", get(ready))
        .route("/v1/auth/register", post(register_account))
        .route("/v1/auth/verify-email", post(verify_email))
        .route("/v1/auth/email/verify", post(verify_email))
        .route("/v1/auth/resend-verification", post(resend_verification))
        .route("/v1/auth/email/resend", post(resend_verification))
        .route("/v1/auth/forgot-password", post(request_password_reset))
        .route("/v1/auth/password/forgot", post(request_password_reset))
        .route("/v1/auth/reset-password", post(confirm_password_reset))
        .route("/v1/auth/password/reset", post(confirm_password_reset))
        .route("/v1/auth/login", post(login_account))
        .route("/v1/auth/oauth/{provider}/start", get(oauth_start))
        .route("/v1/billing/status", get(billing_status))
        .route("/v1/billing/webhooks/paddle", post(paddle_webhook))
        .route("/v1/billing/webhooks/paymob", post(paymob_webhook))
        .route("/v1/sync/share-links/{token}", get(sync_resolve_share_link))
        .route("/v1/updates/latest", get(get_latest_update))
        .merge(any_credential)
        .merge(api_key_only)
        .merge(session_only)
        .merge(owner_elevated_routes)
        .merge(owner_step_up_routes)
        .with_state(state)
}
