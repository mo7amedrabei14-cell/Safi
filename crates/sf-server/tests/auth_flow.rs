//! End-to-end tests for `/health` and `/v1/auth/*` over the real Axum router.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::Value;
use sf_server::auth::api_key::generate_api_key;
use sf_server::auth::cache::InMemoryAuthCache;
use sf_server::auth::repository::{
    ApiKeyRecord, InMemoryApiKeyRepository, InMemorySessionRepository,
};
use sf_server::auth::service::AuthService;
use sf_server::auth::{unix_now, Scope};
use sf_server::{app, AppState};
use tower::ServiceExt;
use uuid::Uuid;

struct Fixture {
    router: Router,
    api_keys: Arc<InMemoryApiKeyRepository>,
    workspace_id: Uuid,
    full_key: String,
    read_only_key: String,
}

async fn fixture() -> Fixture {
    let workspace_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();

    let full = generate_api_key();
    let read_only = generate_api_key();
    let expired = generate_api_key();
    let revoked = generate_api_key();

    let records = vec![
        ApiKeyRecord {
            id: Uuid::now_v7(),
            workspace_id,
            user_id,
            key_prefix: full.key_prefix().to_owned(),
            secret_hash: full.secret_hash().to_owned(),
            scopes: vec![Scope::Read, Scope::Write],
            revoked: false,
            expires_at: None,
            last_used_at: None,
        },
        ApiKeyRecord {
            id: Uuid::now_v7(),
            workspace_id,
            user_id,
            key_prefix: read_only.key_prefix().to_owned(),
            secret_hash: read_only.secret_hash().to_owned(),
            scopes: vec![Scope::Read],
            revoked: false,
            expires_at: None,
            last_used_at: None,
        },
        ApiKeyRecord {
            id: Uuid::now_v7(),
            workspace_id,
            user_id,
            key_prefix: expired.key_prefix().to_owned(),
            secret_hash: expired.secret_hash().to_owned(),
            scopes: vec![Scope::Read],
            revoked: false,
            expires_at: Some(unix_now() - 60),
            last_used_at: None,
        },
        ApiKeyRecord {
            id: Uuid::now_v7(),
            workspace_id,
            user_id,
            key_prefix: revoked.key_prefix().to_owned(),
            secret_hash: revoked.secret_hash().to_owned(),
            scopes: vec![Scope::Read],
            revoked: true,
            expires_at: None,
            last_used_at: None,
        },
    ];

    let api_keys = Arc::new(InMemoryApiKeyRepository::new(records).await);
    let sessions = Arc::new(InMemorySessionRepository::new());
    let cache = Arc::new(InMemoryAuthCache::new());
    let auth = AuthService::new(api_keys.clone(), sessions, cache);

    Fixture {
        router: app(AppState::new(auth)),
        api_keys,
        workspace_id,
        full_key: full.plaintext().to_owned(),
        read_only_key: read_only.plaintext().to_owned(),
    }
}

fn request(method: &str, uri: &str, bearer: Option<&str>, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(token) = bearer {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    match body {
        Some(value) => builder
            .header("content-type", "application/json")
            .body(Body::from(value.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn send(router: &Router, request: Request<Body>) -> axum::response::Response {
    router.clone().oneshot(request).await.unwrap()
}

#[tokio::test]
async fn health_is_public_and_carries_request_id() {
    let fixture = fixture().await;
    let response = send(&fixture.router, request("GET", "/health", None, None)).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().contains_key("x-request-id"));
    let body = json_body(response).await;
    assert_eq!(body["status"], "ok");
    assert_eq!(body["version"], "5.0.0-gold-master");
}

#[tokio::test]
async fn incoming_request_id_is_echoed() {
    let fixture = fixture().await;
    let mut req = request("GET", "/health", None, None);
    req.headers_mut()
        .insert("x-request-id", "req-test-123".parse().unwrap());
    let response = send(&fixture.router, req).await;

    assert_eq!(response.headers()["x-request-id"], "req-test-123");
}

#[tokio::test]
async fn missing_or_malformed_credentials_are_rejected_with_problem_json() {
    let fixture = fixture().await;

    let response = send(
        &fixture.router,
        request("GET", "/v1/auth/whoami", None, None),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = json_body(response).await;
    assert_eq!(body["code"], "SF-AUTH-001");
    assert_eq!(body["status"], 401);

    let response = send(
        &fixture.router,
        request("GET", "/v1/auth/whoami", Some("sfk_live_bad_bad"), None),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn api_key_authentication_is_cached_after_first_lookup() {
    let fixture = fixture().await;

    let first = send(
        &fixture.router,
        request("GET", "/v1/auth/whoami", Some(&fixture.full_key), None),
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);
    let body = json_body(first).await;
    assert_eq!(body["workspace_id"], fixture.workspace_id.to_string());

    let second = send(
        &fixture.router,
        request("GET", "/v1/auth/whoami", Some(&fixture.full_key), None),
    )
    .await;
    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(fixture.api_keys.lookups(), 1);
}

#[tokio::test]
async fn expired_and_revoked_keys_are_rejected_with_specific_codes() {
    let fixture = fixture().await;
    let workspace_id = fixture.workspace_id;
    let user_id = Uuid::now_v7();

    let expired = generate_api_key();
    let revoked = generate_api_key();
    let rows = vec![
        ApiKeyRecord {
            id: Uuid::now_v7(),
            workspace_id,
            user_id,
            key_prefix: expired.key_prefix().to_owned(),
            secret_hash: expired.secret_hash().to_owned(),
            scopes: vec![Scope::Read],
            revoked: false,
            expires_at: Some(unix_now() - 1),
            last_used_at: None,
        },
        ApiKeyRecord {
            id: Uuid::now_v7(),
            workspace_id,
            user_id,
            key_prefix: revoked.key_prefix().to_owned(),
            secret_hash: revoked.secret_hash().to_owned(),
            scopes: vec![Scope::Read],
            revoked: true,
            expires_at: None,
            last_used_at: None,
        },
    ];
    let api_keys = Arc::new(InMemoryApiKeyRepository::new(rows).await);
    let auth = AuthService::new(
        api_keys,
        Arc::new(InMemorySessionRepository::new()),
        Arc::new(InMemoryAuthCache::new()),
    );
    let router = app(AppState::new(auth));

    let response = send(
        &router,
        request("GET", "/v1/auth/whoami", Some(expired.plaintext()), None),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(json_body(response).await["code"], "SF-AUTH-002");

    let response = send(
        &router,
        request("GET", "/v1/auth/whoami", Some(revoked.plaintext()), None),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(json_body(response).await["code"], "SF-AUTH-003");
}

#[tokio::test]
async fn read_only_key_cannot_create_a_session() {
    let fixture = fixture().await;
    let response = send(
        &fixture.router,
        request(
            "POST",
            "/v1/auth/sessions",
            Some(&fixture.read_only_key),
            Some(serde_json::json!({ "device_name": "متصفح" })),
        ),
    )
    .await;

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(json_body(response).await["code"], "SF-AUTH-004");
}

#[tokio::test]
async fn blank_device_name_is_rejected() {
    let fixture = fixture().await;
    let response = send(
        &fixture.router,
        request(
            "POST",
            "/v1/auth/sessions",
            Some(&fixture.full_key),
            Some(serde_json::json!({ "device_name": "   " })),
        ),
    )
    .await;

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(json_body(response).await["code"], "SF-AUTH-006");
}

#[tokio::test]
async fn session_lifecycle_create_read_revoke() {
    let fixture = fixture().await;

    let created = send(
        &fixture.router,
        request(
            "POST",
            "/v1/auth/sessions",
            Some(&fixture.full_key),
            Some(serde_json::json!({ "device_name": "تطبيق Windows" })),
        ),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let created_body = json_body(created).await;
    let token = created_body["token"].as_str().unwrap().to_owned();
    assert!(token.starts_with("sfs_"));
    assert_eq!(created_body["session"]["device_name"], "تطبيق Windows");

    let current = send(
        &fixture.router,
        request("GET", "/v1/auth/sessions/current", Some(&token), None),
    )
    .await;
    assert_eq!(current.status(), StatusCode::OK);
    assert_eq!(json_body(current).await["device_name"], "تطبيق Windows");

    let revoked = send(
        &fixture.router,
        request("DELETE", "/v1/auth/sessions/current", Some(&token), None),
    )
    .await;
    assert_eq!(revoked.status(), StatusCode::NO_CONTENT);

    let after = send(
        &fixture.router,
        request("GET", "/v1/auth/sessions/current", Some(&token), None),
    )
    .await;
    assert_eq!(after.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(json_body(after).await["code"], "SF-AUTH-003");
}

#[tokio::test]
async fn api_key_route_rejects_a_session_token() {
    let fixture = fixture().await;

    let created = send(
        &fixture.router,
        request(
            "POST",
            "/v1/auth/sessions",
            Some(&fixture.full_key),
            Some(serde_json::json!({ "device_name": "جهاز" })),
        ),
    )
    .await;
    let token = json_body(created).await["token"]
        .as_str()
        .unwrap()
        .to_owned();

    let response = send(
        &fixture.router,
        request(
            "POST",
            "/v1/auth/sessions",
            Some(&token),
            Some(serde_json::json!({ "device_name": "جهاز آخر" })),
        ),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
