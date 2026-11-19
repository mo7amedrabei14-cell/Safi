//! HTTP routes for Ed25519-signed Desktop updates and offline licenses (Phase 15).

use axum::extract::{Extension, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::auth::{unix_now, AuthenticatedSession};
use crate::entitlements::PlanTier;
use crate::error::AuthError;
use crate::updater::{IssuedLicensePayload, ReleaseManifestRecord, SignedLicenseBundle};
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct UpdateQuery {
    pub channel: Option<String>,
    pub platform: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SignedUpdateResponse {
    pub manifest: ReleaseManifestRecord,
    pub version_name: String,
    pub download_url: String,
    pub file_size_bytes: u64,
    pub sha256_hex: String,
    pub verifying_key_hex: String,
}

/// `GET /v1/updates/latest` — returns a genuinely Ed25519-signed Desktop release manifest
/// backed by Owner-managed `app_releases` metadata (zero dummy hashes).
/// Fails closed with `SF-AUTH-005` if `SAFI_UPDATE_SIGNING_PRIVATE_KEY` is not configured.
pub async fn get_latest_update(
    State(state): State<AppState>,
    Query(query): Query<UpdateQuery>,
) -> Result<Json<SignedUpdateResponse>, AuthError> {
    let channel = query
        .channel
        .filter(|c| matches!(c.as_str(), "stable" | "beta"))
        .unwrap_or_else(|| "stable".to_owned());
    let platform = query
        .platform
        .filter(|p| !p.trim().is_empty() && p.len() <= 64)
        .unwrap_or_else(|| "windows-x86_64".to_owned());

    let (release, unsigned) = state
        .templates_hub
        .latest_release(&channel, &platform)
        .await
        .ok_or(AuthError::InvalidRequest)?;
    let manifest = state.signer.sign_manifest(unsigned)?;
    let verifying_key_hex = state
        .signer
        .update_verifying_key_hex()
        .ok_or(AuthError::BackendUnavailable)?;

    Ok(Json(SignedUpdateResponse {
        manifest,
        version_name: release.version_name,
        download_url: release.download_url,
        file_size_bytes: release.file_size_bytes,
        sha256_hex: release.sha256_hex,
        verifying_key_hex,
    }))
}

#[derive(Debug, Deserialize)]
pub struct IssueLicenseRequest {
    pub device_fingerprint: String,
}

/// `POST /v1/licenses/issue` — issues a real Ed25519-signed offline Desktop license
/// matching the caller's server-authoritative entitlement tier.
pub async fn issue_offline_license(
    State(state): State<AppState>,
    Extension(session): Extension<AuthenticatedSession>,
    Json(body): Json<IssueLicenseRequest>,
) -> Result<Json<SignedLicenseBundle>, AuthError> {
    let fp = body.device_fingerprint.trim();
    if fp.is_empty() || fp.len() > 128 {
        return Err(AuthError::InvalidRequest);
    }
    let now = unix_now();
    let is_owner = state
        .owner_mfa
        .is_owner_user(session.principal.user_id)
        .await;
    let ent = state
        .entitlements
        .effective_at(
            session.principal.workspace_id,
            session.principal.user_id,
            is_owner,
            now,
        )
        .await;
    let tier = match ent.effective_plan {
        PlanTier::Free => "Free",
        PlanTier::Trial
        | PlanTier::IndividualMonthly
        | PlanTier::IndividualAnnual
        | PlanTier::FriendVip => "Pro",
        PlanTier::OrganizationSeat => "Team",
        PlanTier::Owner => "Enterprise",
    };

    let now_u64 = u64::try_from(now.max(0)).unwrap_or(0);
    let payload = IssuedLicensePayload {
        license_id: format!("lic_{}", uuid::Uuid::now_v7().simple()),
        user_id: session.principal.user_id.to_string(),
        tier: tier.to_owned(),
        issued_at_secs: now_u64,
        expires_at_secs: now_u64.saturating_add(30 * 24 * 60 * 60),
        device_fingerprint: fp.to_owned(),
    };
    let signed = state.signer.sign_license(payload)?;
    Ok(Json(signed))
}
