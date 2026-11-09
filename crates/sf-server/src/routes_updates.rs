//! HTTP routes for updates and manifests (Section 9.8).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::updater::ReleaseManifestRecord;
use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct UpdateQuery {
    pub channel: Option<String>,
    pub platform: Option<String>,
}

/// `GET /v1/updates/latest` — fetch latest release manifest for client desktop app.
pub async fn get_latest_update(
    Query(query): Query<UpdateQuery>,
) -> Json<ReleaseManifestRecord> {
    let channel = query.channel.unwrap_or_else(|| "stable".to_string());
    let platform = query.platform.unwrap_or_else(|| "windows-x86_64".to_string());

    // Public update server record (managed via isolated owner console)
    Json(ReleaseManifestRecord {
        version: 105,
        channel,
        platform,
        min_required_version: 100,
        delta_checksum: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
        signature_hex: "ed25519_verified_signature_manifest".to_string(),
        is_mandatory: false,
        release_notes: "تحديث الأداء والأمان الاستقرار الشامل".to_string(),
    })
}

pub fn build_update_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/updates/latest", axum::routing::get(get_latest_update))
        .with_state(state)
}
