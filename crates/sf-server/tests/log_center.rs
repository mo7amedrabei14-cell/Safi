//! Integration tests for verifiable BLAKE3 hash-chained Owner Audit Logs (Phase 05).

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

use sf_server::auth::cache::InMemoryAuthCache;
use sf_server::auth::repository::{InMemoryApiKeyRepository, InMemorySessionRepository};
use sf_server::auth::service::AuthService;
use sf_server::auth::{unix_now, AuthMethod, Principal, Scope};
use sf_server::owner_mfa::{compute_totp_code, decode_base32, TOTP_PERIOD_SECS};
use sf_server::{app, AppState};

#[tokio::test]
async fn owner_audit_logs_start_empty_record_real_actions_and_verify_blake3_chain() {
    let api_keys = Arc::new(InMemoryApiKeyRepository::new(vec![]).await);
    let sessions = Arc::new(InMemorySessionRepository::new());
    let cache = Arc::new(InMemoryAuthCache::new());
    let auth = AuthService::new(api_keys, sessions, cache);
    let state = AppState::new(auth);

    let owner_id = Uuid::now_v7();
    let owner_ws = Uuid::now_v7();
    state
        .owner_mfa
        .bootstrap_owner_once(owner_id, true)
        .await
        .expect("bootstrap");

    let session = state
        .auth
        .create_session(
            &Principal {
                user_id: owner_id,
                workspace_id: owner_ws,
                scopes: vec![Scope::Read, Scope::Write],
                auth_method: AuthMethod::Password,
            },
            "Owner Device",
        )
        .await
        .expect("session");

    let router = app(state);

    // Enroll + confirm TOTP (generates 2 real audit entries, zero seeded fake entries).
    let enroll_resp = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/owner/mfa/enroll")
                .header("authorization", format!("Bearer {}", session.token))
                .body(Body::empty())
                .expect("req"),
        )
        .await
        .expect("resp");
    assert_eq!(enroll_resp.status(), StatusCode::OK);
    let enroll_bytes = axum::body::to_bytes(enroll_resp.into_body(), usize::MAX)
        .await
        .expect("bytes");
    let enroll_json: Value = serde_json::from_slice(&enroll_bytes).expect("json");
    let secret =
        decode_base32(enroll_json["manual_secret_base32"].as_str().expect("b32")).expect("decode");
    let code = compute_totp_code(&secret, unix_now() / TOTP_PERIOD_SECS).expect("code");

    let confirm_resp = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/owner/mfa/confirm")
                .header("authorization", format!("Bearer {}", session.token))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "code": code }).to_string()))
                .expect("req"),
        )
        .await
        .expect("resp");
    assert_eq!(confirm_resp.status(), StatusCode::OK);

    // Toggle maintenance mode (generates 3rd real audit entry with before/after).
    let maint_resp = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/owner/system/maintenance")
                .header("authorization", format!("Bearer {}", session.token))
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({ "enabled": true, "reason": "Scheduled DB migration" }).to_string(),
                ))
                .expect("req"),
        )
        .await
        .expect("resp");
    assert_eq!(maint_resp.status(), StatusCode::OK);

    // Read audit logs & verify chain integrity.
    let logs_resp = router
        .oneshot(
            Request::builder()
                .uri("/v1/owner/audit-logs")
                .header("authorization", format!("Bearer {}", session.token))
                .body(Body::empty())
                .expect("req"),
        )
        .await
        .expect("resp");
    assert_eq!(logs_resp.status(), StatusCode::OK);

    let bytes = axum::body::to_bytes(logs_resp.into_body(), usize::MAX)
        .await
        .expect("bytes");
    let body: Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(body["is_chain_valid"], true);
    let entries = body["entries"].as_array().expect("array");
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0]["action"], "owner.system.maintenance");
    assert_eq!(entries[0]["before_state"]["enabled"], false);
    assert_eq!(entries[0]["after_state"]["enabled"], true);
}
