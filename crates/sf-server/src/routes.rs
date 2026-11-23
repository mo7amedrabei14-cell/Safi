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
    require_any_credential, require_api_key, require_billing_session, require_owner_session,
    require_session, AnyCredentialGuard, ApiKeyGuard, BillingSessionGuard, OwnerSessionGuard,
    SessionGuard,
};
use crate::auth::session::{parse_session_token, token_hash, SessionView};
use crate::auth::{unix_now, AuthMethod, AuthenticatedSession, Principal, Scope};
use crate::billing::handoff::{
    RedeemBillingHandoffRequest, RedeemedBillingHandoff, StartBillingHandoffRequest,
    StartBillingHandoffResponse,
};
use crate::billing::{
    resolve_provider_and_price, BillingEventRecord, BillingOrderRecord, BillingRevenueSummary,
    CheckoutRequest, CheckoutResponse, InvoiceRecord,
};
use crate::config::ReadinessReport;
use crate::coupons::{
    CouponPreviewResponse, CouponRecord, CouponRedemptionRecord, CreateCouponRequest,
    PreviewCouponRequest, UpdateCouponRequest,
};
use crate::desktop_auth::{StartDesktopPkceRequest, StartDesktopPkceResponse};
use crate::device_authority::{
    DeviceAuthorityService, DeviceReplacementEvent, DeviceStatus, RegisteredDeviceRecord,
    SignedOnlineAuthorizationLease,
};
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
use crate::templates_hub::{
    plan_allows_premium_recipe_exchange, AppReleaseRecord, CommunitySubmissionRecord,
    CreateCommunitySubmissionRequest, OfficialTemplateRecord, PublishAppReleaseRequest,
    PublishSubmissionRequest, ReviewSubmissionRequest,
};
use crate::zero_trust::{require_owner_mfa, OwnerAccessGuard};
use crate::{
    get_server_version, AppState, FeatureFlag, ManagedAccountRecord, PendingOAuthState,
    PendingPhoneOtp, PlatformAnnouncement, RateBucket, SupportTicket,
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

fn extract_client_ip(headers: &HeaderMap) -> String {
    for name in ["cf-connecting-ip", "x-forwarded-for", "x-real-ip"] {
        if let Some(raw) = headers.get(name).and_then(|v| v.to_str().ok()) {
            let first = raw.split(',').next().unwrap_or("").trim();
            if !first.is_empty() && first.len() <= 64 {
                return first.to_owned();
            }
        }
    }
    "direct".to_owned()
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

async fn check_ip_and_account_rate_limit(
    state: &AppState,
    scope: &str,
    headers: &HeaderMap,
    account_key: Option<&str>,
    max_ip_per_min: u32,
    max_account_per_min: u32,
) -> Result<(), AuthError> {
    let ip = extract_client_ip(headers);
    check_rate_limit(state, &format!("{scope}:ip:{ip}"), max_ip_per_min).await?;
    if let Some(raw_acct) = account_key {
        let acct = raw_acct.trim().to_ascii_lowercase();
        if !acct.is_empty() {
            let acct_hash = blake3::hash(acct.as_bytes()).to_hex();
            check_rate_limit(
                state,
                &format!("{scope}:acct:{acct_hash}"),
                max_account_per_min,
            )
            .await?;
        }
    }
    Ok(())
}

/// Public email/password registration.
pub async fn register_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RegisterAccountRequest>,
) -> Result<(StatusCode, Json<AccountMessage>), AuthError> {
    check_ip_and_account_rate_limit(&state, "auth:register", &headers, Some(&body.email), 30, 10)
        .await?;
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
    headers: HeaderMap,
    Json(body): Json<ResendVerificationRequest>,
) -> Result<(StatusCode, Json<AccountMessage>), AuthError> {
    check_ip_and_account_rate_limit(&state, "auth:resend", &headers, Some(&body.email), 15, 5)
        .await?;
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
    headers: HeaderMap,
    Json(body): Json<RequestPasswordResetRequest>,
) -> Result<(StatusCode, Json<AccountMessage>), AuthError> {
    check_ip_and_account_rate_limit(&state, "auth:forgot", &headers, Some(&body.email), 15, 5)
        .await?;
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

#[derive(Debug, Deserialize)]
pub struct HttpLoginRequest {
    #[serde(flatten)]
    pub login: LoginAccountRequest,
    #[serde(default)]
    pub device_public_key_hex: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EnrichedLoginResponse {
    pub token: String,
    pub session: SessionView,
    pub is_owner: bool,
    pub mfa_required: bool,
    pub device_id: Uuid,
    pub session_generation: u64,
    pub previous_device_revoked: bool,
    pub lease: SignedOnlineAuthorizationLease,
}

/// Verify credentials, atomically replace any previous active device (enforcing strict
/// 1-account = 1-active-device exclusivity), and issue a device-bound `sfs_*` session token
/// plus an initial 60-second Ed25519-signed Online Authorization Lease.
pub async fn login_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<HttpLoginRequest>,
) -> Result<(StatusCode, Json<EnrichedLoginResponse>), AuthError> {
    check_ip_and_account_rate_limit(
        &state,
        "auth:login",
        &headers,
        Some(&body.login.email),
        40,
        15,
    )
    .await?;
    let email_norm = body.login.email.trim().to_ascii_lowercase();
    let device_name_copy = body.login.device_name.clone();
    let installation_id = body.login.installation_id;
    let platform_copy = body.login.platform;
    let service: &AccountAuthService = state
        .account_auth
        .as_deref()
        .ok_or(AuthError::BackendUnavailable)?;
    let issued = service.login(body.login).await?;
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

    let platform_str = match platform_copy {
        crate::auth::device::DevicePlatform::Windows => "windows",
        crate::auth::device::DevicePlatform::Linux => "linux",
        crate::auth::device::DevicePlatform::Macos => "macos",
        crate::auth::device::DevicePlatform::Web => "web",
    };
    let (active_device, previous_device_id) = if let Some(pg) = &state.pg_store {
        let (dev, prev, _ev) = pg
            .activate_device_atomically_pg(
                user_id,
                workspace_id,
                &device_name_copy,
                platform_str,
                installation_id,
                body.device_public_key_hex.as_deref(),
                issued.session.id,
                unix_now(),
            )
            .await?;
        (dev, prev)
    } else {
        state
            .device_authority
            .activate_device_atomically(
                user_id,
                workspace_id,
                &device_name_copy,
                installation_id,
                body.device_public_key_hex,
                issued.session.id,
                authenticated.token_hash.clone(),
            )
            .await?
    };

    if let Some(prev_id) = previous_device_id {
        state
            .log_center
            .record(
                Some(user_id),
                "auth.device.replaced",
                "device",
                prev_id.to_string(),
                AuditOutcome::Allowed,
                Some(json!({ "previous_device_id": prev_id })),
                Some(json!({
                    "new_device_id": active_device.device_id,
                    "session_generation": active_device.session_generation,
                })),
            )
            .await;
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
    let now = unix_now();
    let entitlements = state
        .entitlements
        .effective_at(workspace_id, user_id, is_owner, now)
        .await;
    let lease = DeviceAuthorityService::sign_online_lease(
        issued.session.id,
        user_id,
        workspace_id,
        Some(active_device.device_id),
        active_device.session_generation,
        entitlements,
        true,
        state.config.license_signing_seed,
        now,
    );
    state.persist_durable_state().await;

    Ok((
        StatusCode::CREATED,
        Json(EnrichedLoginResponse {
            token: issued.token,
            session: issued.session,
            is_owner,
            mfa_required: is_owner,
            device_id: active_device.device_id,
            session_generation: active_device.session_generation,
            previous_device_revoked: previous_device_id.is_some(),
            lease,
        }),
    ))
}

/// `POST /v1/auth/authorize` and `POST /v1/auth/heartbeat`:
/// Authoritative online authorization & single-device check returning a fresh 60-second
/// Ed25519-signed Online Authorization Lease.
///
/// Explicitly distinguishes:
/// - `SF-AUTH-019` (`DeviceReplaced`) when another device took over the account
/// - `SF-AUTH-020` (`DeviceProofInvalid`) when the Ed25519 device proof signature fails
/// - `SF-AUTH-009` (`AccountSuspended`) when the Owner suspended the account
/// - `SF-AUTH-003` (`Revoked`) when logged out or reset by Owner
pub async fn online_authorize_or_heartbeat(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<SignedOnlineAuthorizationLease>, AuthError> {
    let bearer = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .ok_or(AuthError::InvalidCredentials)?;

    let raw_tok = parse_session_token(bearer).ok_or(AuthError::InvalidCredentials)?;
    let presented_hash = token_hash(raw_tok);

    // Check explicit device replacement reason BEFORE generic session error so Device A
    // receives exact SF-AUTH-019 ("Your account was activated on another device").
    if let Some(reason) = state
        .device_authority
        .revocation_reason_for_token_hash(&presented_hash)
        .await
    {
        return Err(match reason {
            DeviceStatus::ReplacedByNewDevice => AuthError::DeviceReplaced,
            _ => AuthError::Revoked,
        });
    }

    let session = state.auth.authenticate_session(bearer).await?;
    let user_id = session.principal.user_id;
    let workspace_id = session.principal.workspace_id;

    // Enforce account suspension.
    if state
        .control_plane
        .read()
        .await
        .managed_accounts
        .get(&user_id)
        .is_some_and(|a| a.suspended)
    {
        return Err(AuthError::AccountSuspended);
    }

    // Optional Ed25519 cryptographic device proof headers (`X-Safi-Device-Signature`, `X-Safi-Device-Timestamp`).
    let device_sig_header = match (
        headers
            .get("x-safi-device-signature")
            .and_then(|v| v.to_str().ok()),
        headers
            .get("x-safi-device-timestamp")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<i64>().ok()),
    ) {
        (Some(sig), Some(ts)) => Some((sig, ts)),
        _ => None,
    };

    let now = unix_now();
    let (device, session_generation) = if let Some(pg) = &state.pg_store {
        pg.verify_active_session_device_pg(
            user_id,
            session.session_id,
            &session.token_hash,
            device_sig_header,
            now,
        )
        .await?
    } else {
        state
            .device_authority
            .verify_active_session_device(
                user_id,
                session.session_id,
                &session.token_hash,
                device_sig_header,
                now,
            )
            .await?
    };

    let is_owner = state.owner_mfa.is_owner_user(user_id).await;
    let entitlements = state
        .entitlements
        .effective_at(workspace_id, user_id, is_owner, now)
        .await;
    let lease = DeviceAuthorityService::sign_online_lease(
        session.session_id,
        user_id,
        workspace_id,
        device.map(|d| d.device_id),
        session_generation,
        entitlements,
        true,
        state.config.license_signing_seed,
        now,
    );
    Ok(Json(lease))
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
) -> Result<Json<OwnerMfaStatus>, AuthError> {
    if let Some(pg) = &state.pg_store {
        let snap = pg.load_owner_mfa_snapshot_pg().await?;
        state.owner_mfa.import_snapshot(snap).await;
    }
    let status = state
        .owner_mfa
        .status_at(session.principal.user_id, session.session_id, unix_now())
        .await;
    Ok(Json(status))
}

/// `POST /v1/owner/mfa/enroll` — starts Google Authenticator TOTP enrollment for the sealed Owner only.
pub async fn owner_mfa_enroll(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
) -> Result<Json<TotpEnrollmentBundle>, AuthError> {
    if let Some(pg) = &state.pg_store {
        let snap = pg.load_owner_mfa_snapshot_pg().await?;
        state.owner_mfa.import_snapshot(snap).await;
    }
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
            if let Some(pg) = &state.pg_store {
                let snap = state.owner_mfa.export_snapshot().await;
                pg.persist_owner_mfa_snapshot_pg(&snap).await?;
            }
            let audit = state
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
            if let Some(pg) = &state.pg_store {
                pg.insert_audit_entry(&audit).await?;
            }
            state.persist_durable_state().await;
            Ok(Json(bundle))
        }
        Err(err) => {
            let audit = state
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
            if let Some(pg) = &state.pg_store {
                pg.insert_audit_entry(&audit).await?;
            }
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
    if let Some(pg) = &state.pg_store {
        let snap = pg.load_owner_mfa_snapshot_pg().await?;
        state.owner_mfa.import_snapshot(snap).await;
    }
    let user_id = session.principal.user_id;
    let res = state
        .owner_mfa
        .confirm_totp_at(user_id, session.session_id, &body.code, unix_now())
        .await;
    if let Some(pg) = &state.pg_store {
        let snap = state.owner_mfa.export_snapshot().await;
        pg.persist_owner_mfa_snapshot_pg(&snap).await?;
    }
    let outcome = if res.is_ok() {
        AuditOutcome::Allowed
    } else {
        AuditOutcome::Denied
    };
    let audit = state
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
    if let Some(pg) = &state.pg_store {
        pg.insert_audit_entry(&audit).await?;
    }
    state.persist_durable_state().await;
    Ok(Json(res?))
}

/// `POST /v1/owner/mfa/verify` and `/v1/owner/mfa/step-up` — verifies a live 6-digit TOTP code.
pub async fn owner_mfa_verify(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(body): Json<TotpCodeRequest>,
) -> Result<Json<ElevatedOwnerSession>, AuthError> {
    if let Some(pg) = &state.pg_store {
        let snap = pg.load_owner_mfa_snapshot_pg().await?;
        state.owner_mfa.import_snapshot(snap).await;
    }
    let user_id = session.principal.user_id;
    let res = state
        .owner_mfa
        .verify_totp_at(user_id, session.session_id, &body.code, unix_now())
        .await;
    if let Some(pg) = &state.pg_store {
        let snap = state.owner_mfa.export_snapshot().await;
        pg.persist_owner_mfa_snapshot_pg(&snap).await?;
    }
    let outcome = if res.is_ok() {
        AuditOutcome::Allowed
    } else {
        AuditOutcome::Denied
    };
    let audit = state
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
    if let Some(pg) = &state.pg_store {
        pg.insert_audit_entry(&audit).await?;
    }
    state.persist_durable_state().await;
    Ok(Json(res?))
}

/// `POST /v1/owner/mfa/recover` — consumes a single-use recovery code to establish an elevated session.
pub async fn owner_mfa_recover(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(body): Json<RecoveryCodeRequest>,
) -> Result<Json<ElevatedOwnerSession>, AuthError> {
    if let Some(pg) = &state.pg_store {
        let snap = pg.load_owner_mfa_snapshot_pg().await?;
        state.owner_mfa.import_snapshot(snap).await;
    }
    let user_id = session.principal.user_id;
    let res = state
        .owner_mfa
        .verify_recovery_code_at(user_id, session.session_id, &body.recovery_code, unix_now())
        .await;
    if let Some(pg) = &state.pg_store {
        let snap = state.owner_mfa.export_snapshot().await;
        pg.persist_owner_mfa_snapshot_pg(&snap).await?;
    }
    let outcome = if res.is_ok() {
        AuditOutcome::Allowed
    } else {
        AuditOutcome::Denied
    };
    let audit = state
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
    if let Some(pg) = &state.pg_store {
        pg.insert_audit_entry(&audit).await?;
    }
    state.persist_durable_state().await;
    Ok(Json(res?))
}

#[derive(Debug, Serialize)]
pub struct OwnerStatsResponse {
    pub total_users: usize,
    pub verified_phone_users: usize,
    pub suspended_users: usize,
    pub active_devices: usize,
    pub total_coupons: usize,
    pub total_coupon_redemptions: usize,
    pub open_support_tickets: usize,
    pub organizations_count: usize,
    pub maintenance_mode: bool,
    pub billing_providers: Value,
    pub revenue: BillingRevenueSummary,
    pub entitlements: EntitlementAggregateStats,
}

/// `GET /v1/owner/stats` — real live metrics computed from actual server stores (ZERO hardcoded numbers).
pub async fn owner_stats(State(state): State<AppState>) -> Json<OwnerStatsResponse> {
    let cp = state.control_plane.read().await;
    let total_users = cp.managed_accounts.len();
    let verified_phone_users = cp
        .managed_accounts
        .values()
        .filter(|a| a.verified_phone.is_some())
        .count();
    let suspended_users = cp.managed_accounts.values().filter(|a| a.suspended).count();
    let open_support_tickets = cp
        .support_tickets
        .iter()
        .filter(|t| t.status != "resolved" && t.status != "closed")
        .count();
    let maintenance_mode = cp.maintenance_mode;
    drop(cp);

    let all_devices = state.device_authority.list_all_devices().await;
    let active_devices = all_devices
        .iter()
        .filter(|d| d.status == DeviceStatus::Active)
        .count();
    let total_coupons = state.coupons.list_coupons().await.len();
    let total_coupon_redemptions = state.coupons.list_redemptions().await.len();
    let organizations_count = state.organizations.list_all_orgs().await.len();
    let revenue = state.billing.revenue_summary().await;
    let entitlements = state.entitlements.aggregate_stats(unix_now()).await;

    Json(OwnerStatsResponse {
        total_users,
        verified_phone_users,
        suspended_users,
        active_devices,
        total_coupons,
        total_coupon_redemptions,
        open_support_tickets,
        organizations_count,
        maintenance_mode,
        billing_providers: state.billing.provider_status_json(),
        revenue,
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
        .set_friend_vip_checked(
            body.workspace_id,
            body.user_id,
            session.principal.user_id,
            true,
        )
        .await?;
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
        .set_friend_vip_checked(
            body.workspace_id,
            body.user_id,
            session.principal.user_id,
            false,
        )
        .await?;
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

    let _ = state.auth.revoke_all_user_sessions(target_user_id).await;
    let _ = state
        .device_authority
        .owner_reset_devices(
            session.principal.user_id,
            target_user_id,
            "account_suspended",
        )
        .await;

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
    state.persist_durable_state().await;
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
    pub refund_request_key: Option<String>,
    #[serde(default)]
    pub partial_amount_minor: Option<u64>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub mandatory_legal_override: bool,
}

/// `POST /v1/owner/billing/refund` — issues idempotent provider-backed refund enforcing the 3-day policy window (step-up TOTP).
pub async fn owner_issue_refund(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(body): Json<IssueRefundRequest>,
) -> Result<Json<InvoiceRecord>, AuthError> {
    let req_key = body
        .refund_request_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("refund:{}:full", body.external_invoice_id));
    let invoice = state
        .billing
        .issue_owner_refund_idempotent(
            &body.external_invoice_id,
            &req_key,
            body.partial_amount_minor,
            body.mandatory_legal_override,
            body.reason.as_deref().unwrap_or("owner_authorized_refund"),
            Some(session.principal.user_id),
            unix_now(),
        )
        .await?;
    state.persist_durable_state().await;
    state
        .log_center
        .record(
            Some(session.principal.user_id),
            "owner.billing.refund",
            "invoice",
            body.external_invoice_id,
            AuditOutcome::Allowed,
            None,
            Some(json!({
                "status": invoice.status,
                "amount_minor": invoice.amount_minor,
                "refunded_amount_minor": invoice.refunded_amount_minor,
                "refund_request_key": req_key
            })),
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
// EXTENDED 14-SECTION OWNER CONTROL CENTER (PROFILE, DEVICES, COUPONS,
// CHARGEBACKS, ORGANIZATIONS, SUPPORT, ANNOUNCEMENTS, PRODUCT CONFIG)
// ============================================================================

#[derive(Debug, Serialize)]
pub struct OwnerCustomerProfileResponse {
    pub account: ManagedAccountRecord,
    pub entitlements: EffectiveEntitlements,
    pub invoices: Vec<InvoiceRecord>,
    pub devices: Vec<RegisteredDeviceRecord>,
    pub device_replacements: Vec<DeviceReplacementEvent>,
}

/// `GET /v1/owner/accounts/{user_id}/profile` — complete customer profile drill-down (Section 9.3).
pub async fn owner_customer_profile(
    State(state): State<AppState>,
    Path(target_user_id): Path<Uuid>,
) -> Result<Json<OwnerCustomerProfileResponse>, AuthError> {
    let acc = state
        .control_plane
        .read()
        .await
        .managed_accounts
        .get(&target_user_id)
        .cloned()
        .ok_or(AuthError::InvalidRequest)?;
    let is_owner = state.owner_mfa.is_owner_user(target_user_id).await;
    let entitlements = state
        .entitlements
        .effective_at(acc.workspace_id, target_user_id, is_owner, unix_now())
        .await;
    let invoices = state.billing.list_invoices(acc.workspace_id).await;
    let devices = state
        .device_authority
        .list_devices_for_user(target_user_id)
        .await;
    let device_replacements = state
        .device_authority
        .list_replacements_for_user(target_user_id)
        .await;
    Ok(Json(OwnerCustomerProfileResponse {
        account: acc,
        entitlements,
        invoices,
        devices,
        device_replacements,
    }))
}

#[derive(Debug, Serialize)]
pub struct OwnerDevicesOverviewResponse {
    pub devices: Vec<RegisteredDeviceRecord>,
    pub replacements: Vec<DeviceReplacementEvent>,
}

/// `GET /v1/owner/devices` — all registered devices & replacement audit events (Section 9.8).
pub async fn owner_list_devices(
    State(state): State<AppState>,
) -> Result<Json<OwnerDevicesOverviewResponse>, AuthError> {
    if let Some(pg) = &state.pg_store {
        let (devices, replacements) = pg.list_devices_and_replacements_pg(None).await?;
        return Ok(Json(OwnerDevicesOverviewResponse {
            devices,
            replacements,
        }));
    }
    let devices = state.device_authority.list_all_devices().await;
    let replacements = state.device_authority.list_recent_replacements(100).await;
    Ok(Json(OwnerDevicesOverviewResponse {
        devices,
        replacements,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ResetDevicesRequest {
    pub reason: String,
}

/// `POST /v1/owner/accounts/{user_id}/devices/reset` — audited Owner device reset (step-up TOTP).
pub async fn owner_reset_account_devices(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(target_user_id): Path<Uuid>,
    Json(body): Json<ResetDevicesRequest>,
) -> Result<Json<DeviceReplacementEvent>, AuthError> {
    let event = if let Some(pg) = &state.pg_store {
        pg.owner_reset_devices_pg(
            session.principal.user_id,
            target_user_id,
            &body.reason,
            unix_now(),
        )
        .await?
    } else {
        state
            .device_authority
            .owner_reset_devices(session.principal.user_id, target_user_id, &body.reason)
            .await?
    };
    state.auth.revoke_all_user_sessions(target_user_id).await?;
    let audit = state
        .log_center
        .record(
            Some(session.principal.user_id),
            "owner.devices.reset",
            "user",
            target_user_id.to_string(),
            AuditOutcome::Allowed,
            None,
            Some(json!({
                "reason": event.reason,
                "new_session_generation": event.new_session_generation,
            })),
        )
        .await;
    if let Some(pg) = &state.pg_store {
        pg.insert_audit_entry(&audit).await?;
    }
    state.persist_durable_state().await;
    Ok(Json(event))
}

#[derive(Debug, Deserialize)]
pub struct RecordChargebackRequest {
    pub external_invoice_id: String,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
}

/// `POST /v1/owner/billing/chargeback` — record chargeback & immediately revoke paid entitlements.
pub async fn owner_record_chargeback(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(body): Json<RecordChargebackRequest>,
) -> Result<Json<InvoiceRecord>, AuthError> {
    let inv = state
        .billing
        .record_chargeback(
            &body.external_invoice_id,
            body.workspace_id,
            body.user_id,
            unix_now(),
        )
        .await?;
    state
        .log_center
        .record(
            Some(session.principal.user_id),
            "owner.billing.chargeback",
            "invoice",
            body.external_invoice_id,
            AuditOutcome::Allowed,
            None,
            Some(json!({ "status": inv.status, "workspace_id": inv.workspace_id })),
        )
        .await;
    state.persist_durable_state().await;
    Ok(Json(inv))
}

/// `GET /v1/owner/billing/invoices` — list all invoices across the platform.
pub async fn owner_list_all_invoices(State(state): State<AppState>) -> Json<Vec<InvoiceRecord>> {
    Json(state.billing.list_all_invoices().await)
}

/// `GET /v1/owner/coupons` — list all coupons (Section 9.7).
pub async fn owner_list_coupons(State(state): State<AppState>) -> Json<Vec<CouponRecord>> {
    Json(state.coupons.list_coupons().await)
}

/// `POST /v1/owner/coupons` — create a promotional coupon (step-up TOTP).
pub async fn owner_create_coupon(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(body): Json<CreateCouponRequest>,
) -> Result<(StatusCode, Json<CouponRecord>), AuthError> {
    let coupon = state
        .coupons
        .create_coupon(session.principal.user_id, body)
        .await?;
    if let Some(pg) = &state.pg_store {
        pg.upsert_coupon(&coupon).await?;
    }
    state
        .log_center
        .record(
            Some(session.principal.user_id),
            "owner.coupon.create",
            "coupon",
            coupon.code.clone(),
            AuditOutcome::Allowed,
            None,
            serde_json::to_value(&coupon).ok(),
        )
        .await;
    state.persist_durable_state().await;
    Ok((StatusCode::CREATED, Json(coupon)))
}

/// `POST /v1/owner/coupons/{code}` — update/disable/archive a coupon (step-up TOTP).
pub async fn owner_update_coupon(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(code): Path<String>,
    Json(body): Json<UpdateCouponRequest>,
) -> Result<Json<CouponRecord>, AuthError> {
    let updated = state.coupons.update_coupon(&code, body).await?;
    if let Some(pg) = &state.pg_store {
        pg.upsert_coupon(&updated).await?;
    }
    state
        .log_center
        .record(
            Some(session.principal.user_id),
            "owner.coupon.update",
            "coupon",
            updated.code.clone(),
            AuditOutcome::Allowed,
            None,
            serde_json::to_value(&updated).ok(),
        )
        .await;
    state.persist_durable_state().await;
    Ok(Json(updated))
}

/// `GET /v1/owner/coupons/redemptions` — audit ledger of all coupon redemptions.
pub async fn owner_coupon_redemptions(
    State(state): State<AppState>,
) -> Json<Vec<CouponRedemptionRecord>> {
    Json(state.coupons.list_redemptions().await)
}

#[derive(Debug, Serialize)]
pub struct OwnerOrganizationRow {
    pub organization: Organization,
    pub active_members: u32,
}

/// `GET /v1/owner/organizations` — all organizations & seat usage (Section 9.9).
pub async fn owner_list_organizations(
    State(state): State<AppState>,
) -> Json<Vec<OwnerOrganizationRow>> {
    let rows = state
        .organizations
        .list_all_orgs()
        .await
        .into_iter()
        .map(|(organization, active_members)| OwnerOrganizationRow {
            organization,
            active_members,
        })
        .collect();
    Json(rows)
}

#[derive(Debug, Deserialize)]
pub struct CreateTicketRequest {
    pub user_id: Option<Uuid>,
    pub workspace_id: Option<Uuid>,
    pub subject: String,
    pub body: String,
    pub priority: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTicketRequest {
    pub status: Option<String>,
    pub priority: Option<String>,
    pub owner_notes: Option<String>,
}

/// `GET /v1/owner/support/tickets` — list customer support tickets (Section 9.10).
pub async fn owner_list_tickets(State(state): State<AppState>) -> Json<Vec<SupportTicket>> {
    Json(state.control_plane.read().await.support_tickets.clone())
}

/// `POST /v1/owner/support/tickets` — record a support ticket.
pub async fn owner_create_ticket(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(body): Json<CreateTicketRequest>,
) -> Result<(StatusCode, Json<SupportTicket>), AuthError> {
    let subj = body.subject.trim();
    if subj.is_empty() || subj.len() > 200 || body.body.trim().is_empty() {
        return Err(AuthError::InvalidRequest);
    }
    let now = unix_now();
    let ticket = SupportTicket {
        id: Uuid::now_v7(),
        user_id: body.user_id.unwrap_or(session.principal.user_id),
        workspace_id: body.workspace_id.unwrap_or(session.principal.workspace_id),
        subject: subj.to_owned(),
        body: body.body.trim().to_owned(),
        status: "open".to_owned(),
        priority: body.priority.unwrap_or_else(|| "normal".to_owned()),
        owner_notes: String::new(),
        created_at: now,
        updated_at: now,
    };
    state
        .control_plane
        .write()
        .await
        .support_tickets
        .push(ticket.clone());
    state.persist_durable_state().await;
    Ok((StatusCode::CREATED, Json(ticket)))
}

/// `POST /v1/owner/support/tickets/{id}` — update ticket status/priority/notes.
pub async fn owner_update_ticket(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(ticket_id): Path<Uuid>,
    Json(body): Json<UpdateTicketRequest>,
) -> Result<Json<SupportTicket>, AuthError> {
    let updated = {
        let mut cp = state.control_plane.write().await;
        let t = cp
            .support_tickets
            .iter_mut()
            .find(|t| t.id == ticket_id)
            .ok_or(AuthError::InvalidRequest)?;
        if let Some(s) = body.status {
            t.status = s;
        }
        if let Some(p) = body.priority {
            t.priority = p;
        }
        if let Some(n) = body.owner_notes {
            t.owner_notes = n;
        }
        t.updated_at = unix_now();
        t.clone()
    };
    state
        .log_center
        .record(
            Some(session.principal.user_id),
            "owner.support.ticket.update",
            "support_ticket",
            ticket_id.to_string(),
            AuditOutcome::Allowed,
            None,
            Some(json!({ "status": updated.status, "priority": updated.priority })),
        )
        .await;
    state.persist_durable_state().await;
    Ok(Json(updated))
}

#[derive(Debug, Deserialize)]
pub struct CreateAnnouncementRequest {
    pub title: String,
    pub body: String,
    pub severity: Option<String>,
    #[serde(default = "default_true")]
    pub published: bool,
}

const fn default_true() -> bool {
    true
}

/// `GET /v1/owner/announcements` — list platform announcements (Section 9.11).
pub async fn owner_list_announcements(
    State(state): State<AppState>,
) -> Json<Vec<PlatformAnnouncement>> {
    Json(state.control_plane.read().await.announcements.clone())
}

/// `POST /v1/owner/announcements` — publish an announcement.
pub async fn owner_create_announcement(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(body): Json<CreateAnnouncementRequest>,
) -> Result<(StatusCode, Json<PlatformAnnouncement>), AuthError> {
    let title = body.title.trim();
    if title.is_empty() || title.len() > 200 || body.body.trim().is_empty() {
        return Err(AuthError::InvalidRequest);
    }
    let ann = PlatformAnnouncement {
        id: Uuid::now_v7(),
        title: title.to_owned(),
        body: body.body.trim().to_owned(),
        severity: body.severity.unwrap_or_else(|| "info".to_owned()),
        published: body.published,
        created_at: unix_now(),
    };
    state
        .control_plane
        .write()
        .await
        .announcements
        .push(ann.clone());
    if let Some(pg) = &state.pg_store {
        pg.insert_announcement(&ann, session.principal.user_id)
            .await?;
    }
    state
        .log_center
        .record(
            Some(session.principal.user_id),
            "owner.announcement.create",
            "announcement",
            ann.id.to_string(),
            AuditOutcome::Allowed,
            None,
            Some(json!({ "title": ann.title, "published": ann.published })),
        )
        .await;
    state.persist_durable_state().await;
    Ok((StatusCode::CREATED, Json(ann)))
}

/// `GET /v1/owner/product-config` — non-secret business & integration configuration (Section 9.12).
pub async fn owner_product_config(State(state): State<AppState>) -> Json<Value> {
    let readiness = state.config.readiness_report();
    let r = crate::business_rules::rules();
    Json(json!({
        "server_version": get_server_version(),
        "readiness": readiness,
        "online_lease_ttl_secs": crate::device_authority::ONLINE_LEASE_TTL_SECS,
        "online_heartbeat_secs": crate::device_authority::ONLINE_HEARTBEAT_SECS,
        "refund_window_days": r.refund_window_secs / 86_400,
        "trial_days": r.trial_days,
        "free_plan_ops_per_month": r.free_ops_per_month,
        "free_plan_max_rows": r.free_max_rows,
        "free_plan_max_cols": r.free_max_cols,
        "org_min_seats": r.org_min_seats,
    }))
}

// ============================================================================
// DESKTOP BROWSER PKCE AUTH, DEDICATED OWNER WEB PORTAL SESSION,
// OFFICIAL TEMPLATES, COMMUNITY CONTRIBUTIONS & RELEASE MANAGEMENT
// ============================================================================

/// `POST /v1/auth/desktop/start` — start Desktop Browser PKCE (`S256`) authorization flow.
pub async fn desktop_pkce_start(
    State(state): State<AppState>,
    Json(body): Json<StartDesktopPkceRequest>,
) -> Result<(StatusCode, Json<StartDesktopPkceResponse>), AuthError> {
    check_rate_limit(&state, "auth:desktop_pkce_start", 40).await?;
    let origin = state
        .config
        .api_base_url
        .as_deref()
        .unwrap_or("https://safidata.app");
    let now = unix_now();
    if let Some(pg) = &state.pg_store {
        let state_trimmed = body.state.trim();
        let challenge_trimmed = body.code_challenge.trim();
        if state_trimmed.len() < 16 || challenge_trimmed.len() < 43 || body.installation_id.is_nil()
        {
            return Err(AuthError::DesktopAuthCodeInvalid);
        }
        let flow_str = match body.flow {
            crate::desktop_auth::DesktopAuthFlow::Signin => "signin",
            crate::desktop_auth::DesktopAuthFlow::Signup => "signup",
        };
        let platform_str = match body.platform {
            crate::auth::device::DevicePlatform::Windows => "windows",
            crate::auth::device::DevicePlatform::Linux => "linux",
            crate::auth::device::DevicePlatform::Macos => "macos",
            crate::auth::device::DevicePlatform::Web => "web",
        };
        let request_id = Uuid::now_v7();
        let expires_at = now + crate::desktop_auth::PKCE_CODE_TTL_SECS;
        let state_hash = blake3::hash(state_trimmed.as_bytes());
        let dev_name = if body.device_name.trim().is_empty() {
            "SafiData Desktop"
        } else {
            body.device_name.trim()
        };
        pg.start_desktop_pkce_pg(
            request_id,
            flow_str,
            challenge_trimmed,
            state_hash.as_bytes(),
            body.installation_id,
            platform_str,
            dev_name,
            expires_at,
        )
        .await?;
        let auth_url = format!(
            "{}/auth/desktop?flow={flow_str}&request_id={request_id}&state={state_trimmed}",
            origin.trim_end_matches('/')
        );
        return Ok((
            StatusCode::CREATED,
            Json(StartDesktopPkceResponse {
                request_id,
                auth_url,
                expires_at,
            }),
        ));
    }
    let resp = state.desktop_auth.start_pkce(body, origin, now).await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

#[derive(Debug, Deserialize)]
pub struct ApproveDesktopPkceRequest {
    pub request_id: Uuid,
    pub state: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct ApproveDesktopPkceResponse {
    pub code: String,
    pub state: String,
    pub callback_uri: String,
}

/// `POST /v1/auth/desktop/approve` — browser `/auth/desktop` verifies user credentials and mints
/// a single-use short-lived PKCE code (`safi://auth?code=...&state=...`; zero tokens in URL).
pub async fn desktop_pkce_approve(
    State(state): State<AppState>,
    Json(body): Json<ApproveDesktopPkceRequest>,
) -> Result<Json<ApproveDesktopPkceResponse>, AuthError> {
    check_rate_limit(&state, "auth:desktop_pkce_approve", 40).await?;
    let service: &AccountAuthService = state
        .account_auth
        .as_deref()
        .ok_or(AuthError::BackendUnavailable)?;
    let (user_id, workspace_id, email) = service
        .verify_credentials_only(&body.email, &body.password)
        .await?;

    if state
        .control_plane
        .read()
        .await
        .managed_accounts
        .get(&user_id)
        .is_some_and(|a| a.suspended)
    {
        return Err(AuthError::AccountSuspended);
    }

    let now = unix_now();
    if let Some(pg) = &state.pg_store {
        use ring::rand::{SecureRandom, SystemRandom};
        let mut raw = [0u8; 24];
        SystemRandom::new()
            .fill(&mut raw)
            .map_err(|_| AuthError::BackendUnavailable)?;
        let code = format!("sfcode_{}", hex::encode(raw));
        let code_hash = blake3::hash(code.as_bytes());
        let state_hash = blake3::hash(body.state.trim().as_bytes());
        let ok = pg
            .approve_desktop_pkce_pg(
                body.request_id,
                state_hash.as_bytes(),
                user_id,
                workspace_id,
                &email,
                code_hash.as_bytes(),
                now,
            )
            .await?;
        if !ok {
            return Err(AuthError::DesktopAuthCodeInvalid);
        }
        let callback_uri = format!("safi://auth?code={code}&state={}", body.state.trim());
        return Ok(Json(ApproveDesktopPkceResponse {
            code,
            state: body.state,
            callback_uri,
        }));
    }

    let (code, callback_uri) = state
        .desktop_auth
        .approve_pkce(
            body.request_id,
            &body.state,
            user_id,
            workspace_id,
            &email,
            now,
        )
        .await?;
    Ok(Json(ApproveDesktopPkceResponse {
        code,
        state: body.state,
        callback_uri,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ExchangeDesktopPkceRequest {
    pub code: String,
    pub code_verifier: String,
    pub state: String,
    pub installation_id: Uuid,
    #[serde(default)]
    pub device_public_key_hex: Option<String>,
}

/// `POST /v1/auth/desktop/exchange` — Desktop exchanges one-time PKCE code + verifier for a
/// single-device-bound `sfs_*` session token and signed 60s Online Authorization Lease.
pub async fn desktop_pkce_exchange(
    State(state): State<AppState>,
    Json(body): Json<ExchangeDesktopPkceRequest>,
) -> Result<(StatusCode, Json<EnrichedLoginResponse>), AuthError> {
    check_rate_limit(&state, "auth:desktop_pkce_exchange", 40).await?;
    let now = unix_now();
    let (user_id, workspace_id, email, device_name, platform) = if let Some(pg) = &state.pg_store {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine as _;
        use sha2::{Digest, Sha256};
        use subtle::ConstantTimeEq;

        let verifier = body.code_verifier.trim();
        if verifier.len() < 43 || verifier.len() > 128 {
            return Err(AuthError::DesktopAuthCodeInvalid);
        }
        let code_hash = blake3::hash(body.code.trim().as_bytes());
        let state_hash = blake3::hash(body.state.trim().as_bytes());
        let Some((expected_challenge, uid, ws, mail, plat_str, dev_name)) = pg
            .consume_desktop_pkce_atomic_pg(
                code_hash.as_bytes(),
                state_hash.as_bytes(),
                body.installation_id,
                now,
            )
            .await?
        else {
            return Err(AuthError::DesktopAuthCodeInvalid);
        };
        let computed = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        if computed
            .as_bytes()
            .ct_eq(expected_challenge.as_bytes())
            .unwrap_u8()
            != 1
        {
            return Err(AuthError::DesktopAuthCodeInvalid);
        }
        let plat = match plat_str.as_str() {
            "linux" => crate::auth::device::DevicePlatform::Linux,
            "macos" => crate::auth::device::DevicePlatform::Macos,
            "web" => crate::auth::device::DevicePlatform::Web,
            _ => crate::auth::device::DevicePlatform::Windows,
        };
        (uid, ws, mail, dev_name, plat)
    } else {
        state
            .desktop_auth
            .consume_pkce_code(
                &body.code,
                &body.code_verifier,
                &body.state,
                body.installation_id,
                now,
            )
            .await?
    };

    if state
        .control_plane
        .read()
        .await
        .managed_accounts
        .get(&user_id)
        .is_some_and(|a| a.suspended)
    {
        return Err(AuthError::AccountSuspended);
    }

    let principal = Principal {
        user_id,
        workspace_id,
        scopes: vec![Scope::Read, Scope::Write],
        auth_method: AuthMethod::Password,
    };
    let device = crate::auth::device::DeviceRegistration::from_installation_id(
        user_id,
        body.installation_id,
        platform,
    )?;
    let issued = state
        .auth
        .create_session_for_device(&principal, &device_name, device)
        .await?;
    let authenticated = state.auth.authenticate_session(&issued.token).await?;

    let platform_str = match platform {
        crate::auth::device::DevicePlatform::Windows => "windows",
        crate::auth::device::DevicePlatform::Linux => "linux",
        crate::auth::device::DevicePlatform::Macos => "macos",
        crate::auth::device::DevicePlatform::Web => "web",
    };
    let (active_device, previous_device_id) = if let Some(pg) = &state.pg_store {
        let (dev, prev, _ev) = pg
            .activate_device_atomically_pg(
                user_id,
                workspace_id,
                &device_name,
                platform_str,
                body.installation_id,
                body.device_public_key_hex.as_deref(),
                issued.session.id,
                now,
            )
            .await?;
        (dev, prev)
    } else {
        state
            .device_authority
            .activate_device_atomically(
                user_id,
                workspace_id,
                &device_name,
                body.installation_id,
                body.device_public_key_hex,
                issued.session.id,
                authenticated.token_hash.clone(),
            )
            .await?
    };

    state
        .upsert_managed_account(ManagedAccountRecord {
            user_id,
            workspace_id,
            email,
            display_name: device_name,
            verified_phone: None,
            suspended: false,
            created_at: unix_now(),
        })
        .await;

    let is_owner = state.owner_mfa.is_owner_user(user_id).await;
    let now = unix_now();
    let entitlements = state
        .entitlements
        .effective_at(workspace_id, user_id, is_owner, now)
        .await;
    let lease = DeviceAuthorityService::sign_online_lease(
        issued.session.id,
        user_id,
        workspace_id,
        Some(active_device.device_id),
        active_device.session_generation,
        entitlements,
        true,
        state.config.license_signing_seed,
        now,
    );
    state.persist_durable_state().await;

    Ok((
        StatusCode::CREATED,
        Json(EnrichedLoginResponse {
            token: issued.token,
            session: issued.session,
            is_owner,
            mfa_required: is_owner,
            device_id: active_device.device_id,
            session_generation: active_device.session_generation,
            previous_device_revoked: previous_device_id.is_some(),
            lease,
        }),
    ))
}

#[derive(Debug, Deserialize)]
pub struct OwnerWebLoginRequest {
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub user_agent_hint: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OwnerWebLoginResponse {
    pub token: String,
    pub session_id: Uuid,
    pub session_type: &'static str,
    pub expires_at: i64,
    pub is_owner: bool,
    pub mfa_required: bool,
}

/// `POST /v1/owner/web/login` — Dedicated Owner Web Portal sign-in (Section 6.1).
/// Strictly rejects non-Owner accounts (`SF-AUTH-013`) and issues an `sfw_*` Owner web session
/// WITHOUT counting as a second Desktop installation or revoking the Owner's Desktop device.
pub async fn owner_web_login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<OwnerWebLoginRequest>,
) -> Result<(StatusCode, Json<OwnerWebLoginResponse>), AuthError> {
    check_ip_and_account_rate_limit(
        &state,
        "owner:web_login",
        &headers,
        Some(&body.email),
        20,
        10,
    )
    .await?;
    let service: &AccountAuthService = state
        .account_auth
        .as_deref()
        .ok_or(AuthError::BackendUnavailable)?;
    let (user_id, workspace_id, _email) = service
        .verify_credentials_only(&body.email, &body.password)
        .await?;
    if let Some(pg) = &state.pg_store {
        let snap = pg.load_owner_mfa_snapshot_pg().await?;
        state.owner_mfa.import_snapshot(snap).await;
    }
    state.owner_mfa.require_owner_identity(user_id).await?;

    let now = unix_now();
    let (token, rec) = state
        .desktop_auth
        .issue_owner_web_session(
            user_id,
            workspace_id,
            body.user_agent_hint.as_deref().unwrap_or("owner-web"),
            now,
        )
        .await?;
    if let Some(pg) = &state.pg_store {
        pg.insert_owner_web_session(&rec).await?;
    }
    let audit = state
        .log_center
        .record(
            Some(user_id),
            "owner.web_session.created",
            "owner_web_session",
            rec.id.to_string(),
            AuditOutcome::Allowed,
            None,
            Some(json!({ "session_type": "owner_web", "desktop_device_untouched": true })),
        )
        .await;
    if let Some(pg) = &state.pg_store {
        pg.insert_audit_entry(&audit).await?;
    }
    state.persist_durable_state().await;

    Ok((
        StatusCode::CREATED,
        Json(OwnerWebLoginResponse {
            token,
            session_id: rec.id,
            session_type: "owner_web",
            expires_at: rec.expires_at,
            is_owner: true,
            mfa_required: true,
        }),
    ))
}

/// `DELETE /v1/owner/web/session` — revoke the current Owner Web Portal session & elevated MFA.
pub async fn owner_web_logout(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    headers: HeaderMap,
) -> Result<StatusCode, AuthError> {
    if let Some(pg) = &state.pg_store {
        if let Some(tok) = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
        {
            let h = blake3::hash(tok.trim().as_bytes());
            let _ = pg.revoke_owner_web_session_pg(h.as_bytes()).await?;
        }
    }
    state
        .owner_mfa
        .revoke_elevated_session(session.session_id)
        .await;
    state
        .desktop_auth
        .revoke_owner_web_session(session.session_id)
        .await;
    state.persist_durable_state().await;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /v1/announcements/public` — published product announcements for authenticated/regular users.
pub async fn list_public_announcements(
    State(state): State<AppState>,
) -> Json<Vec<PlatformAnnouncement>> {
    let list = state
        .control_plane
        .read()
        .await
        .announcements
        .iter()
        .filter(|a| a.published)
        .cloned()
        .collect();
    Json(list)
}

/// `GET /v1/templates/official` — official published templates authorized for caller's plan tier.
pub async fn list_official_templates_for_user(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
) -> Result<Json<Vec<OfficialTemplateRecord>>, AuthError> {
    if let Some(pg) = &state.pg_store {
        let snap = pg.load_owner_mfa_snapshot_pg().await?;
        state.owner_mfa.import_snapshot(snap).await;
    }
    let is_owner = state
        .owner_mfa
        .is_owner_user(session.principal.user_id)
        .await;
    let ent = state
        .entitlements
        .effective_at_checked(
            session.principal.workspace_id,
            session.principal.user_id,
            is_owner,
            unix_now(),
        )
        .await?;
    if let Some(pg) = &state.pg_store {
        let include_paid = plan_allows_premium_recipe_exchange(ent.effective_plan);
        let rows = pg.list_official_templates(false, include_paid).await?;
        return Ok(Json(rows));
    }
    Ok(Json(
        state
            .templates_hub
            .list_official_for_plan(ent.effective_plan)
            .await,
    ))
}

/// `POST /v1/templates/entitlement/export-import` — authoritative server entitlement check for
/// Premium Recipe Export / Import (`.safitemplate`). Rejects Free and Trial users (`SF-RCP-005`).
pub async fn check_premium_recipe_exchange_entitlement(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
) -> Result<Json<Value>, AuthError> {
    if let Some(pg) = &state.pg_store {
        let snap = pg.load_owner_mfa_snapshot_pg().await?;
        state.owner_mfa.import_snapshot(snap).await;
    }
    let is_owner = state
        .owner_mfa
        .is_owner_user(session.principal.user_id)
        .await;
    let ent = state
        .entitlements
        .effective_at_checked(
            session.principal.workspace_id,
            session.principal.user_id,
            is_owner,
            unix_now(),
        )
        .await?;
    if !plan_allows_premium_recipe_exchange(ent.effective_plan) {
        return Err(AuthError::PremiumRecipeEntitlementRequired);
    }
    Ok(Json(json!({
        "allowed": true,
        "effective_plan": ent.effective_plan,
        "format": ".safitemplate",
    })))
}

/// `POST /v1/community/submissions` — explicit user-approved recipe contribution (`Share with SafiData`).
pub async fn create_community_submission(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(body): Json<CreateCommunitySubmissionRequest>,
) -> Result<(StatusCode, Json<CommunitySubmissionRecord>), AuthError> {
    let user_id = session.principal.user_id;
    let workspace_id = session.principal.workspace_id;
    if state
        .control_plane
        .read()
        .await
        .managed_accounts
        .get(&user_id)
        .is_some_and(|a| a.suspended)
    {
        return Err(AuthError::AccountSuspended);
    }
    if let Some(pg) = &state.pg_store {
        // Validate consent, steps, secret/row smuggling BEFORE touching PostgreSQL.
        if !body.consent_accepted || body.rows.is_some() || body.data_rows.is_some() {
            return Err(AuthError::CommunitySubmissionRejected);
        }
        let name = body.name.trim();
        let desc = body.description.trim();
        if name.is_empty() || name.len() > 160 || desc.is_empty() || desc.len() > 2_000 {
            return Err(AuthError::CommunitySubmissionRejected);
        }
        if body.schema_version != 1 {
            return Err(AuthError::CommunitySubmissionRejected);
        }
        crate::templates_hub::validate_declarative_steps(&body.steps)?;
        let scan = crate::templates_hub::scan_recipe_definition(name, desc, &body.steps);
        if !scan.blocked_secrets.is_empty() {
            return Err(AuthError::CommunitySubmissionRejected);
        }
        let now = unix_now();
        let content_hash = blake3::hash(
            serde_json::to_string(&body.steps)
                .unwrap_or_default()
                .as_bytes(),
        )
        .to_hex()
        .to_string();
        let rec = CommunitySubmissionRecord {
            id: Uuid::now_v7(),
            contributor_user_id: user_id,
            workspace_id,
            name: name.to_owned(),
            description: desc.to_owned(),
            recipe_type: body.recipe_type,
            schema_version: body.schema_version,
            app_version: body.app_version,
            required_columns: body.required_columns,
            steps: body.steps,
            content_hash,
            status: crate::templates_hub::SubmissionStatus::Pending,
            reviewer_notes: String::new(),
            reviewed_by: None,
            consent_accepted: true,
            consent_terms_version: body.consent_terms_version,
            consent_at: now,
            show_contributor_attribution: body.show_contributor_attribution,
            pii_warnings: scan.pii_warnings,
            published_template_id: None,
            created_at: now,
            updated_at: now,
        };
        let saved = pg
            .create_community_submission_atomic(
                &rec,
                crate::templates_hub::DEFAULT_MONTHLY_COMMUNITY_SUBMISSIONS,
            )
            .await?;
        return Ok((StatusCode::CREATED, Json(saved)));
    }
    let rec = state
        .templates_hub
        .submit_community_recipe(user_id, workspace_id, body, unix_now())
        .await?;
    state.persist_durable_state().await;
    Ok((StatusCode::CREATED, Json(rec)))
}

/// `GET /v1/community/submissions/mine` — caller's own submissions only (never other users').
pub async fn list_my_community_submissions(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
) -> Result<Json<Vec<CommunitySubmissionRecord>>, AuthError> {
    if let Some(pg) = &state.pg_store {
        let rows = pg
            .list_my_community_submissions_pg(
                session.principal.user_id,
                session.principal.workspace_id,
            )
            .await?;
        return Ok(Json(rows));
    }
    Ok(Json(
        state
            .templates_hub
            .list_submissions_for_user(session.principal.user_id)
            .await,
    ))
}

/// `GET /v1/owner/community/submissions` — Owner-only view of all community template submissions.
pub async fn owner_list_community_submissions(
    State(state): State<AppState>,
) -> Result<Json<Vec<CommunitySubmissionRecord>>, AuthError> {
    if let Some(pg) = &state.pg_store {
        return Ok(Json(
            pg.list_all_community_submissions_for_owner_pg().await?,
        ));
    }
    Ok(Json(
        state.templates_hub.list_all_submissions_for_owner().await,
    ))
}

/// `GET /v1/owner/community/submissions/{id}` — Owner-only detail inspection of one submission.
pub async fn owner_get_community_submission(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<CommunitySubmissionRecord>, AuthError> {
    if let Some(pg) = &state.pg_store {
        let found = pg
            .list_all_community_submissions_for_owner_pg()
            .await?
            .into_iter()
            .find(|s| s.id == id)
            .ok_or(AuthError::InvalidRequest)?;
        return Ok(Json(found));
    }
    Ok(Json(state.templates_hub.get_submission(id).await?))
}

/// `POST /v1/owner/community/submissions/{id}/review` — Owner step-up TOTP review transition.
pub async fn owner_review_community_submission(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<Uuid>,
    Json(body): Json<ReviewSubmissionRequest>,
) -> Result<Json<CommunitySubmissionRecord>, AuthError> {
    let updated = if let Some(pg) = &state.pg_store {
        pg.review_community_submission_pg(
            id,
            session.principal.user_id,
            body.status,
            &body.reviewer_notes,
        )
        .await?
    } else {
        state
            .templates_hub
            .review_submission(id, session.principal.user_id, body, unix_now())
            .await?
    };
    let audit = state
        .log_center
        .record(
            Some(session.principal.user_id),
            "owner.community_submission.review",
            "community_submission",
            id.to_string(),
            AuditOutcome::Allowed,
            None,
            Some(json!({ "status": updated.status, "reviewer_notes": updated.reviewer_notes })),
        )
        .await;
    if let Some(pg) = &state.pg_store {
        pg.insert_audit_entry(&audit).await?;
    }
    state.persist_durable_state().await;
    Ok(Json(updated))
}

/// `POST /v1/owner/community/submissions/{id}/publish` — Owner step-up TOTP explicit official publish.
pub async fn owner_publish_community_submission(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<Uuid>,
    Json(body): Json<PublishSubmissionRequest>,
) -> Result<(StatusCode, Json<Value>), AuthError> {
    let (template, submission) = if let Some(pg) = &state.pg_store {
        pg.publish_community_submission_atomic_pg(id, session.principal.user_id, &body, unix_now())
            .await?
    } else {
        state
            .templates_hub
            .publish_submission(id, session.principal.user_id, body, unix_now())
            .await?
    };
    let audit = state
        .log_center
        .record(
            Some(session.principal.user_id),
            "owner.official_template.publish",
            "official_template",
            template.id.to_string(),
            AuditOutcome::Allowed,
            None,
            Some(json!({
                "slug": template.slug,
                "version": template.current_version,
                "source_submission_id": id,
            })),
        )
        .await;
    if let Some(pg) = &state.pg_store {
        pg.insert_audit_entry(&audit).await?;
    }
    state.persist_durable_state().await;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "template": template, "submission": submission })),
    ))
}

/// `GET /v1/owner/templates/official` — Owner-only full Official Template Library + version history.
pub async fn owner_list_official_templates(
    State(state): State<AppState>,
) -> Result<Json<Vec<OfficialTemplateRecord>>, AuthError> {
    if let Some(pg) = &state.pg_store {
        return Ok(Json(pg.list_official_templates(true, true).await?));
    }
    Ok(Json(
        state.templates_hub.list_all_official_for_owner().await,
    ))
}

#[derive(Debug, Deserialize)]
pub struct SetTemplateActiveRequest {
    pub active: bool,
}

/// `POST /v1/owner/templates/official/{id}/status` — Owner step-up deactivate/reactivate template.
pub async fn owner_set_official_template_status(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<Uuid>,
    Json(body): Json<SetTemplateActiveRequest>,
) -> Result<Json<OfficialTemplateRecord>, AuthError> {
    let updated = if let Some(pg) = &state.pg_store {
        pg.set_official_template_active_pg(id, body.active).await?
    } else {
        state
            .templates_hub
            .set_official_active(id, body.active, unix_now())
            .await?
    };
    let audit = state
        .log_center
        .record(
            Some(session.principal.user_id),
            "owner.official_template.status",
            "official_template",
            id.to_string(),
            AuditOutcome::Allowed,
            None,
            Some(json!({ "active": updated.active })),
        )
        .await;
    if let Some(pg) = &state.pg_store {
        pg.insert_audit_entry(&audit).await?;
    }
    state.persist_durable_state().await;
    Ok(Json(updated))
}

#[derive(Debug, Deserialize)]
pub struct RollbackTemplateRequest {
    pub version_number: u32,
}

/// `POST /v1/owner/templates/official/{id}/rollback` — Owner step-up version rollback.
pub async fn owner_rollback_official_template(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(id): Path<Uuid>,
    Json(body): Json<RollbackTemplateRequest>,
) -> Result<Json<OfficialTemplateRecord>, AuthError> {
    let updated = if let Some(pg) = &state.pg_store {
        pg.rollback_official_template_pg(id, body.version_number)
            .await?
    } else {
        state
            .templates_hub
            .rollback_official_version(id, body.version_number, unix_now())
            .await?
    };
    let audit = state
        .log_center
        .record(
            Some(session.principal.user_id),
            "owner.official_template.rollback",
            "official_template",
            id.to_string(),
            AuditOutcome::Allowed,
            None,
            Some(json!({ "current_version": updated.current_version })),
        )
        .await;
    if let Some(pg) = &state.pg_store {
        pg.insert_audit_entry(&audit).await?;
    }
    state.persist_durable_state().await;
    Ok(Json(updated))
}

/// `GET /v1/owner/releases` — Owner-only list of Desktop releases.
pub async fn owner_list_releases(
    State(state): State<AppState>,
) -> Result<Json<Vec<AppReleaseRecord>>, AuthError> {
    if let Some(pg) = &state.pg_store {
        return Ok(Json(pg.list_app_releases_pg().await?));
    }
    Ok(Json(state.templates_hub.list_releases().await))
}

/// `POST /v1/owner/releases` — Owner step-up TOTP publish signed Desktop release metadata.
pub async fn owner_publish_release(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(body): Json<PublishAppReleaseRequest>,
) -> Result<(StatusCode, Json<AppReleaseRecord>), AuthError> {
    let now = unix_now();
    let rec = if let Some(pg) = &state.pg_store {
        let sha = body.sha256_hex.trim().to_ascii_lowercase();
        if body.version_name.trim().is_empty()
            || body.download_url.trim().is_empty()
            || body.file_size_bytes == 0
            || sha.len() != 64
            || !sha.bytes().all(|b| b.is_ascii_hexdigit())
            || !matches!(body.channel.as_str(), "stable" | "beta")
        {
            return Err(AuthError::InvalidRequest);
        }
        let rec = AppReleaseRecord {
            id: Uuid::now_v7(),
            version_code: body.version_code,
            version_name: body.version_name.trim().to_owned(),
            channel: body.channel,
            platform: body.platform.trim().to_owned(),
            download_url: body.download_url.trim().to_owned(),
            sha256_hex: sha,
            file_size_bytes: body.file_size_bytes,
            min_required_version: body.min_required_version,
            release_notes: body.release_notes.trim().to_owned(),
            is_mandatory: body.is_mandatory,
            active: true,
            published_by: Some(session.principal.user_id),
            created_at: now,
        };
        pg.insert_app_release(&rec).await?;
        rec
    } else {
        state
            .templates_hub
            .publish_release(session.principal.user_id, body, now)
            .await?
    };
    let audit = state
        .log_center
        .record(
            Some(session.principal.user_id),
            "owner.release.publish",
            "app_release",
            rec.id.to_string(),
            AuditOutcome::Allowed,
            None,
            Some(json!({
                "version_code": rec.version_code,
                "version_name": rec.version_name,
                "platform": rec.platform,
                "sha256_hex": rec.sha256_hex,
            })),
        )
        .await;
    if let Some(pg) = &state.pg_store {
        pg.insert_audit_entry(&audit).await?;
    }
    state.persist_durable_state().await;
    Ok((StatusCode::CREATED, Json(rec)))
}

// ============================================================================
// ENTITLEMENTS, BILLING, ORGANIZATIONS, ENCRYPTED SYNC & AI ROUTER ROUTES
// ============================================================================

/// `GET /v1/entitlements/me` — server-authoritative plan, watermark, and usage limits.
pub async fn get_my_entitlements(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
) -> Result<Json<EffectiveEntitlements>, AuthError> {
    let is_owner = state
        .owner_mfa
        .is_owner_user(session.principal.user_id)
        .await;
    let ent = state
        .entitlements
        .effective_at_checked(
            session.principal.workspace_id,
            session.principal.user_id,
            is_owner,
            unix_now(),
        )
        .await?;
    Ok(Json(ent))
}

/// `POST /v1/entitlements/consume` — atomic, idempotent quota enforcement.
pub async fn consume_entitlement(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    headers: HeaderMap,
    Json(body): Json<ConsumeUsageRequest>,
) -> Result<Json<ConsumeUsageResult>, AuthError> {
    check_ip_and_account_rate_limit(
        &state,
        "entitlements:consume",
        &headers,
        Some(&session.principal.workspace_id.to_string()),
        180,
        120,
    )
    .await?;
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
    headers: HeaderMap,
    Json(body): Json<ClaimTrialRequest>,
) -> Result<Json<EffectiveEntitlements>, AuthError> {
    check_ip_and_account_rate_limit(
        &state,
        "entitlements:claim_trial",
        &headers,
        Some(&session.principal.user_id.to_string()),
        10,
        5,
    )
    .await?;
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

/// `POST /v1/billing/coupons/preview` — server-authoritative coupon validation & discount preview.
pub async fn billing_preview_coupon(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(body): Json<PreviewCouponRequest>,
) -> Result<Json<CouponPreviewResponse>, AuthError> {
    let seats = body.seats.unwrap_or(1);
    let (_provider, currency, original_minor) =
        resolve_provider_and_price(&body.billing_country_iso2, body.product, seats);
    let has_prior_invoices = !state
        .billing
        .list_invoices(session.principal.workspace_id)
        .await
        .is_empty();
    let preview = state
        .coupons
        .preview_at(
            &body.code,
            session.principal.user_id,
            body.product,
            original_minor,
            currency,
            !has_prior_invoices,
            unix_now(),
        )
        .await?;
    Ok(Json(preview))
}

/// `POST /v1/billing/checkout` — create real Paddle (USD) or Paymob (EGP) checkout session
/// with durable server-side idempotency (`Idempotency-Key` header or body `idempotency_key`)
/// and safe coupon reservation (released automatically if provider creation fails).
pub async fn billing_checkout(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    headers: HeaderMap,
    Json(mut body): Json<CheckoutRequest>,
) -> Result<Json<CheckoutResponse>, AuthError> {
    check_ip_and_account_rate_limit(
        &state,
        "billing:checkout",
        &headers,
        Some(&session.principal.workspace_id.to_string()),
        60,
        30,
    )
    .await?;
    body.workspace_id = session.principal.workspace_id;
    body.user_id = session.principal.user_id;
    if body
        .idempotency_key
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .is_empty()
    {
        body.idempotency_key = headers
            .get("idempotency-key")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty());
    }

    if let Some(ref mut bd) = body.billing_data {
        if bd.email.trim().is_empty() {
            if let Some(acct) = state.get_managed_account(session.principal.user_id).await {
                bd.email = acct.email;
            }
        }
    }

    let resp = state.billing.create_checkout(&body).await?;
    state.persist_durable_state().await;
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

#[derive(Debug, Deserialize)]
pub struct BillingWebLoginRequest {
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub user_agent_hint: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BillingWebLoginResponse {
    pub token: String,
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub workspace_id: Uuid,
    pub email: String,
    pub session_type: &'static str,
    pub expires_at: i64,
}

/// `POST /v1/billing/web/login` — Website Billing Portal authentication (`sfb_*`).
/// Authenticates email/password WITHOUT revoking or counting against the user's single active
/// Desktop installation (`sfs_*`). The issued `sfb_*` token is strictly scoped to billing &
/// entitlement-read endpoints and rejected by Desktop Studio / AI / Org / Sync / Owner routes.
pub async fn billing_web_login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<BillingWebLoginRequest>,
) -> Result<(StatusCode, Json<BillingWebLoginResponse>), AuthError> {
    check_ip_and_account_rate_limit(
        &state,
        "billing:web_login",
        &headers,
        Some(&body.email),
        30,
        15,
    )
    .await?;
    let service: &AccountAuthService = state
        .account_auth
        .as_deref()
        .ok_or(AuthError::BackendUnavailable)?;
    let (user_id, workspace_id, email) = service
        .verify_credentials_only(&body.email, &body.password)
        .await?;
    if state
        .control_plane
        .read()
        .await
        .managed_accounts
        .get(&user_id)
        .is_some_and(|a| a.suspended)
    {
        return Err(AuthError::AccountSuspended);
    }
    let now = unix_now();
    let (token, session_id, expires_at) = state
        .billing_handoff
        .issue_billing_web_session_at(
            user_id,
            workspace_id,
            body.user_agent_hint.as_deref().unwrap_or("billing-web"),
            now,
        )
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(BillingWebLoginResponse {
            token,
            session_id,
            user_id,
            workspace_id,
            email,
            session_type: "billing_web",
            expires_at,
        }),
    ))
}

/// `DELETE /v1/billing/web/session` — revoke the current Website Billing Portal `sfb_*` session.
pub async fn billing_web_logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, AuthError> {
    if let Some(tok) = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
    {
        state
            .billing_handoff
            .revoke_billing_web_session(tok)
            .await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// `POST /v1/billing/handoff/start` — Authenticated Desktop (`sfs_*`) initiates a short-lived
/// (180s), single-use Desktop-to-Website billing handoff returning `/billing?handoff=sfbh_...&state=...`.
/// ZERO access tokens or reusable credentials ever appear in the returned URL.
pub async fn billing_handoff_start(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    headers: HeaderMap,
    Json(body): Json<StartBillingHandoffRequest>,
) -> Result<(StatusCode, Json<StartBillingHandoffResponse>), AuthError> {
    check_ip_and_account_rate_limit(
        &state,
        "billing:handoff_start",
        &headers,
        Some(&session.principal.workspace_id.to_string()),
        60,
        30,
    )
    .await?;
    let origin = state
        .config
        .api_base_url
        .as_deref()
        .unwrap_or("https://safidata.app");
    let resp = state
        .billing_handoff
        .start_handoff_at(
            session.principal.user_id,
            session.principal.workspace_id,
            &body,
            origin,
            unix_now(),
        )
        .await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

/// `POST /v1/billing/handoff/redeem` — Authenticated Website `/billing` page redeems the
/// single-use Desktop handoff bound strictly to the matching `(user_id, workspace_id)` and
/// receives the authoritative server-resolved plan, seats, currency, and price from `config/business.json`.
pub async fn billing_handoff_redeem(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    headers: HeaderMap,
    Json(body): Json<RedeemBillingHandoffRequest>,
) -> Result<Json<RedeemedBillingHandoff>, AuthError> {
    check_ip_and_account_rate_limit(
        &state,
        "billing:handoff_redeem",
        &headers,
        Some(&session.principal.user_id.to_string()),
        60,
        30,
    )
    .await?;
    let redeemed = state
        .billing_handoff
        .redeem_handoff_at(
            session.principal.user_id,
            session.principal.workspace_id,
            &body.handoff_code,
            &body.state,
            unix_now(),
        )
        .await?;
    Ok(Json(redeemed))
}

#[derive(Debug, Serialize)]
pub struct BillingOrderStatusResponse {
    pub order_id: Uuid,
    pub order_state: crate::billing::PaymentOrderState,
    pub provider: crate::billing::BillingProvider,
    pub product: crate::billing::CheckoutProduct,
    pub seats: u32,
    pub currency: crate::billing::BillingCurrency,
    pub original_amount_minor: u64,
    pub discount_amount_minor: u64,
    pub expected_amount_minor: u64,
    pub checkout_url: Option<String>,
    pub paid_at: Option<i64>,
    pub updated_at: i64,
}

impl From<BillingOrderRecord> for BillingOrderStatusResponse {
    fn from(o: BillingOrderRecord) -> Self {
        Self {
            order_id: o.id,
            order_state: o.state,
            provider: o.provider,
            product: o.product,
            seats: o.seats,
            currency: o.currency,
            original_amount_minor: o.original_amount_minor,
            discount_amount_minor: o.discount_amount_minor,
            expected_amount_minor: o.expected_amount_minor,
            checkout_url: o.checkout_url,
            paid_at: o.paid_at,
            updated_at: o.updated_at,
        }
    }
}

/// `GET /v1/billing/orders/{order_id}` — authoritative server-confirmed order status for the
/// authenticated workspace ONLY (enforces strict tenant isolation; zero IDOR).
pub async fn billing_get_order_status(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Path(order_id): Path<Uuid>,
) -> Result<Json<BillingOrderStatusResponse>, AuthError> {
    let Some(order) = state.billing.get_order(order_id).await? else {
        return Err(AuthError::InvalidRequest);
    };
    if order.workspace_id != session.principal.workspace_id {
        return Err(AuthError::InsufficientScope);
    }
    Ok(Json(BillingOrderStatusResponse::from(order)))
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
    state.persist_durable_state().await;
    Ok(Json(rec))
}

#[derive(Debug, Deserialize)]
pub struct PaymobWebhookQuery {
    pub hmac: Option<String>,
}

/// `POST /v1/billing/webhooks/paymob` — Official Paymob Transaction Processed Callback
/// (`{"type":"TRANSACTION","obj":{...}}`) with HMAC-SHA512 query parameter verification.
pub async fn paymob_webhook(
    State(state): State<AppState>,
    Query(query): Query<PaymobWebhookQuery>,
    headers: HeaderMap,
    raw_body: Bytes,
) -> Result<Json<BillingEventRecord>, AuthError> {
    let hmac = query
        .hmac
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or(AuthError::InvalidWebhookSignature)?;
    let delivery_id = headers
        .get("x-paymob-delivery-id")
        .and_then(|v| v.to_str().ok());
    let rec = state
        .billing
        .process_paymob_webhook_raw(hmac, &raw_body, delivery_id, unix_now())
        .await?;
    state.persist_durable_state().await;
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
    let is_owner = state
        .owner_mfa
        .is_owner_user(session.principal.user_id)
        .await;
    let ent = state
        .entitlements
        .effective_at_checked(
            session.principal.workspace_id,
            session.principal.user_id,
            is_owner,
            unix_now(),
        )
        .await?;
    if !ent.teams_enabled {
        return Err(AuthError::PaidPlanRequired);
    }
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
    let is_owner = state
        .owner_mfa
        .is_owner_user(session.principal.user_id)
        .await;
    let ent = state
        .entitlements
        .effective_at_checked(
            session.principal.workspace_id,
            session.principal.user_id,
            is_owner,
            unix_now(),
        )
        .await?;
    if !ent.teams_enabled {
        return Err(AuthError::PaidPlanRequired);
    }
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
    Extension(session): Extension<AuthenticatedSession>,
    Path(org_id): Path<Uuid>,
) -> Result<Json<Vec<OrganizationMember>>, AuthError> {
    let is_owner = state
        .owner_mfa
        .is_owner_user(session.principal.user_id)
        .await;
    let members = state.organizations.list_members(org_id).await;
    let is_member = members
        .iter()
        .any(|m| m.user_id == session.principal.user_id && m.removed_at.is_none());
    if !is_owner && !is_member {
        return Err(AuthError::OrgPermissionDenied);
    }
    Ok(Json(members))
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
    let is_owner = state
        .owner_mfa
        .is_owner_user(session.principal.user_id)
        .await;
    let ent = state
        .entitlements
        .effective_at_checked(
            session.principal.workspace_id,
            session.principal.user_id,
            is_owner,
            unix_now(),
        )
        .await?;
    if !ent.saved_recipes_write_enabled {
        return Err(AuthError::PaidPlanRequired);
    }
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

/// `POST /v1/ai/route` — enforces explicit consent, local PII masking, Free-plan isolation,
/// authoritative AI quota, and honest provider readiness.
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
    let is_owner = state
        .owner_mfa
        .is_owner_user(session.principal.user_id)
        .await;
    let now = unix_now();
    // Enforce server-authoritative plan entitlement & AI quota BEFORE provider readiness so Free
    // accounts ALWAYS receive 403 PaidPlanRequired (`SF-LIC-008`) even if AI keys are not set.
    let current = state
        .entitlements
        .effective_at_checked(
            session.principal.workspace_id,
            session.principal.user_id,
            is_owner,
            now,
        )
        .await?;
    if !current.ai_enabled {
        return Err(AuthError::PaidPlanRequired);
    }
    if current.effective_plan != PlanTier::Owner && current.ai_calls_used >= current.ai_calls_limit
    {
        return Err(AuthError::AiQuotaExceeded);
    }
    if !state.config.ai_configured() {
        return Err(AuthError::AiProviderNotConfigured);
    }
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
            now,
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
    let desktop_auth = state.desktop_auth.clone();

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

    let billing_handoff = state.billing_handoff.clone();

    let billing_session_routes = Router::new()
        .route("/v1/entitlements/me", get(get_my_entitlements))
        .route("/v1/billing/handoff/redeem", post(billing_handoff_redeem))
        .route("/v1/billing/coupons/preview", post(billing_preview_coupon))
        .route("/v1/billing/checkout", post(billing_checkout))
        .route(
            "/v1/billing/orders/{order_id}",
            get(billing_get_order_status),
        )
        .route("/v1/billing/invoices", get(billing_invoices))
        .route("/v1/billing/web/session", delete(billing_web_logout))
        .route_layer(from_fn_with_state(
            BillingSessionGuard {
                auth: auth.clone(),
                billing_handoff,
                required_scope: Scope::Read,
            },
            require_billing_session,
        ));

    let session_only = Router::new()
        .route(
            "/v1/auth/sessions/current",
            get(current_session).delete(revoke_current_session),
        )
        .route("/v1/auth/email/change", post(change_account_email))
        .route("/v1/auth/phone/request-otp", post(request_phone_otp))
        .route("/v1/auth/phone/verify-otp", post(verify_phone_otp))
        // Entitlements consumption, trial claim & Desktop-initiated billing handoff (sfs_* ONLY)
        .route("/v1/entitlements/consume", post(consume_entitlement))
        .route("/v1/entitlements/trial/claim", post(claim_trial))
        .route("/v1/billing/handoff/start", post(billing_handoff_start))
        // Regular-user scoped central services (official templates, premium recipe check, community submission)
        .route("/v1/templates/official", get(list_official_templates_for_user))
        .route(
            "/v1/templates/entitlement/export-import",
            post(check_premium_recipe_exchange_entitlement),
        )
        .route("/v1/community/submissions", post(create_community_submission))
        .route(
            "/v1/community/submissions/mine",
            get(list_my_community_submissions),
        )
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

    // Owner MFA bootstrap/verify & Owner Web Logout (accepts BOTH Desktop sfs_* & Owner Web sfw_* sessions)
    let owner_mfa_routes = Router::new()
        .route("/v1/owner/web/session", delete(owner_web_logout))
        .route("/v1/owner/mfa/status", get(owner_mfa_status))
        .route("/v1/owner/mfa/enroll", post(owner_mfa_enroll))
        .route("/v1/owner/mfa/confirm", post(owner_mfa_confirm))
        .route("/v1/owner/mfa/verify", post(owner_mfa_verify))
        .route("/v1/owner/mfa/step-up", post(owner_mfa_verify))
        .route("/v1/owner/mfa/recover", post(owner_mfa_recover))
        .route_layer(from_fn_with_state(
            OwnerSessionGuard {
                auth: auth.clone(),
                desktop_auth: desktop_auth.clone(),
                pg_store: state.pg_store.clone(),
                required_scope: Scope::Read,
            },
            require_owner_session,
        ));

    // Owner Read / Standard Control Plane (Desktop sfs_* OR Owner Web sfw_* + Sealed Owner + Elevated TOTP)
    let owner_elevated_routes = Router::new()
        .route("/v1/owner/stats", get(owner_stats))
        .route("/v1/owner/accounts", get(owner_list_accounts))
        .route(
            "/v1/owner/accounts/{user_id}/profile",
            get(owner_customer_profile),
        )
        .route("/v1/owner/devices", get(owner_list_devices))
        .route("/v1/owner/billing/events", get(owner_billing_events))
        .route("/v1/owner/billing/invoices", get(owner_list_all_invoices))
        .route("/v1/owner/coupons", get(owner_list_coupons))
        .route(
            "/v1/owner/coupons/redemptions",
            get(owner_coupon_redemptions),
        )
        .route("/v1/owner/organizations", get(owner_list_organizations))
        .route(
            "/v1/owner/support/tickets",
            get(owner_list_tickets).post(owner_create_ticket),
        )
        .route("/v1/owner/support/tickets/{id}", post(owner_update_ticket))
        .route(
            "/v1/owner/announcements",
            get(owner_list_announcements).post(owner_create_announcement),
        )
        .route("/v1/owner/product-config", get(owner_product_config))
        .route("/v1/owner/system/maintenance", get(owner_get_maintenance))
        .route(
            "/v1/owner/feature-flags",
            get(owner_list_feature_flags).post(owner_set_feature_flag),
        )
        .route("/v1/owner/audit-logs", get(owner_audit_logs))
        .route(
            "/v1/owner/community/submissions",
            get(owner_list_community_submissions),
        )
        .route(
            "/v1/owner/community/submissions/{id}",
            get(owner_get_community_submission),
        )
        .route(
            "/v1/owner/templates/official",
            get(owner_list_official_templates),
        )
        .route("/v1/owner/releases", get(owner_list_releases))
        .route_layer(from_fn_with_state(
            OwnerAccessGuard {
                mfa: owner_mfa.clone(),
                pg_store: state.pg_store.clone(),
                require_step_up: false,
            },
            require_owner_mfa,
        ))
        .route_layer(from_fn_with_state(
            OwnerSessionGuard {
                auth: auth.clone(),
                desktop_auth: desktop_auth.clone(),
                pg_store: state.pg_store.clone(),
                required_scope: Scope::Read,
            },
            require_owner_session,
        ));

    // Owner High-Risk Step-Up Control Plane (Desktop sfs_* OR Owner Web sfw_* + Sealed Owner + Fresh 5-min Step-Up TOTP)
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
            "/v1/owner/accounts/{user_id}/devices/reset",
            post(owner_reset_account_devices),
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
        .route(
            "/v1/owner/billing/chargeback",
            post(owner_record_chargeback),
        )
        .route("/v1/owner/coupons", post(owner_create_coupon))
        .route("/v1/owner/coupons/{code}", post(owner_update_coupon))
        .route("/v1/owner/system/maintenance", post(owner_set_maintenance))
        .route(
            "/v1/owner/community/submissions/{id}/review",
            post(owner_review_community_submission),
        )
        .route(
            "/v1/owner/community/submissions/{id}/publish",
            post(owner_publish_community_submission),
        )
        .route(
            "/v1/owner/templates/official/{id}/status",
            post(owner_set_official_template_status),
        )
        .route(
            "/v1/owner/templates/official/{id}/rollback",
            post(owner_rollback_official_template),
        )
        .route("/v1/owner/releases", post(owner_publish_release))
        .route_layer(from_fn_with_state(
            OwnerAccessGuard {
                mfa: owner_mfa,
                pg_store: state.pg_store.clone(),
                require_step_up: true,
            },
            require_owner_mfa,
        ))
        .route_layer(from_fn_with_state(
            OwnerSessionGuard {
                auth,
                desktop_auth,
                pg_store: state.pg_store.clone(),
                required_scope: Scope::Read,
            },
            require_owner_session,
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
        .route("/v1/auth/desktop/start", post(desktop_pkce_start))
        .route("/v1/auth/desktop/approve", post(desktop_pkce_approve))
        .route("/v1/auth/desktop/exchange", post(desktop_pkce_exchange))
        .route("/v1/owner/web/login", post(owner_web_login))
        .route("/v1/auth/authorize", post(online_authorize_or_heartbeat))
        .route("/v1/auth/heartbeat", post(online_authorize_or_heartbeat))
        .route("/v1/auth/oauth/{provider}/start", get(oauth_start))
        .route("/v1/announcements/public", get(list_public_announcements))
        .route("/v1/billing/status", get(billing_status))
        .route("/v1/billing/web/login", post(billing_web_login))
        .route("/v1/billing/webhooks/paddle", post(paddle_webhook))
        .route("/v1/billing/webhooks/paymob", post(paymob_webhook))
        .route("/v1/sync/share-links/{token}", get(sync_resolve_share_link))
        .route("/v1/updates/latest", get(get_latest_update))
        .merge(any_credential)
        .merge(api_key_only)
        .merge(billing_session_routes)
        .merge(session_only)
        .merge(owner_mfa_routes)
        .merge(owner_elevated_routes)
        .merge(owner_step_up_routes)
        .with_state(state)
}
