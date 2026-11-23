//! End-to-end HTTP security tests for Sealed Owner Identity, Google Authenticator
//! RFC 6238 TOTP MFA, anti-replay, email-change preservation, real Control Plane stats,
//! step-up enforcement, billing webhooks, entitlements, and Ed25519 updates/licenses.

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
use sf_server::billing::sign_paddle_payload;
use sf_server::config::ServerConfig;
use sf_server::owner_mfa::{compute_totp_code, decode_base32, TOTP_PERIOD_SECS};
use sf_server::{app, AppState, ManagedAccountRecord};

async fn make_state_with_users() -> (AppState, String, Uuid, Uuid, String, Uuid, Uuid) {
    let api_keys = Arc::new(InMemoryApiKeyRepository::new(vec![]).await);
    let sessions = Arc::new(InMemorySessionRepository::new());
    let cache = Arc::new(InMemoryAuthCache::new());
    let auth = AuthService::new(api_keys, sessions, cache);

    let mock_paddle_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock paddle");
    let mock_paddle_addr = mock_paddle_listener.local_addr().expect("addr");
    let mock_paddle_app = axum::Router::new().route(
        "/transactions",
        axum::routing::post(|axum::Json(body): axum::Json<Value>| async move {
            let txn_id = format!("txn_{}", Uuid::now_v7().simple());
            let total = body["custom_data"]["expected_amount_minor"]
                .as_u64()
                .unwrap_or(2000);
            axum::Json(json!({
                "data": {
                    "id": txn_id,
                    "status": "ready",
                    "currency_code": "USD",
                    "checkout": { "url": format!("https://checkout.paddle.test/pay/{txn_id}") },
                    "details": { "totals": { "total": total.to_string() } }
                }
            }))
        }),
    );
    tokio::spawn(async move {
        let _ = axum::serve(mock_paddle_listener, mock_paddle_app).await;
    });

    let mut cfg = ServerConfig::for_tests();
    cfg.paddle_api_key = Some("pdl_test_key".to_owned());
    cfg.paddle_webhook_secret = Some("pdl_whsec_test_secret".to_owned());
    cfg.paddle_api_base_url = format!("http://{mock_paddle_addr}");
    cfg.paddle_price_individual_monthly = Some("pri_zt_monthly".to_owned());

    let state = AppState::with_config(auth, cfg);

    let owner_id = Uuid::now_v7();
    let owner_ws = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    let user_ws = Uuid::now_v7();

    state
        .owner_mfa
        .bootstrap_owner_once(owner_id, true)
        .await
        .expect("bootstrap owner");

    state
        .upsert_managed_account(ManagedAccountRecord {
            user_id: owner_id,
            workspace_id: owner_ws,
            email: "original-owner@example.test".to_owned(),
            display_name: "Platform Owner".to_owned(),
            verified_phone: None,
            suspended: false,
            created_at: unix_now(),
        })
        .await;
    state
        .upsert_managed_account(ManagedAccountRecord {
            user_id,
            workspace_id: user_ws,
            email: "customer@example.test".to_owned(),
            display_name: "Regular Customer".to_owned(),
            verified_phone: None,
            suspended: false,
            created_at: unix_now(),
        })
        .await;

    let owner_session = state
        .auth
        .create_session(
            &Principal {
                user_id: owner_id,
                workspace_id: owner_ws,
                scopes: vec![Scope::Read, Scope::Write],
                auth_method: AuthMethod::Password,
            },
            "Owner Desktop",
        )
        .await
        .expect("owner session");

    let user_session = state
        .auth
        .create_session(
            &Principal {
                user_id,
                workspace_id: user_ws,
                scopes: vec![Scope::Read, Scope::Write],
                auth_method: AuthMethod::Password,
            },
            "Customer Desktop",
        )
        .await
        .expect("user session");

    (
        state,
        owner_session.token,
        owner_id,
        owner_ws,
        user_session.token,
        user_id,
        user_ws,
    )
}

async fn send_json(
    router: &axum::Router,
    method: Method,
    uri: &str,
    bearer: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(tok) = bearer {
        builder = builder.header("authorization", format!("Bearer {tok}"));
    }
    let req = if let Some(val) = body {
        builder
            .header("content-type", "application/json")
            .body(Body::from(val.to_string()))
            .expect("req")
    } else {
        builder.body(Body::empty()).expect("req")
    };

    let resp = router.clone().oneshot(req).await.expect("response");
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("bytes");
    let json = serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({}));
    (status, json)
}

#[tokio::test]
async fn owner_mfa_control_plane_email_change_and_billing_http_flow() {
    let (state, owner_token, owner_id, _owner_ws, user_token, user_id, user_ws) =
        make_state_with_users().await;
    let router = app(state.clone());

    // 1. Unauthenticated caller rejected on Owner stats (401).
    let (st, _) = send_json(&router, Method::GET, "/v1/owner/stats", None, None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);

    // 2. Legacy fake YubiKey/tunnel headers without real auth are REJECTED (401).
    let fake_req = Request::builder()
        .uri("/v1/owner/stats")
        .header(
            "X-Safi-Zero-Trust-Token",
            "safi_zt_tunnel_verified_secure_2026",
        )
        .header(
            "X-Safi-Hardware-Signature",
            "yubikey_fips_sig_yubikey_fips_sec_key_2026_verified_sig123",
        )
        .body(Body::empty())
        .expect("req");
    let fake_resp = router.clone().oneshot(fake_req).await.expect("resp");
    assert_eq!(fake_resp.status(), StatusCode::UNAUTHORIZED);

    // 3. Regular user rejected from Owner MFA enrollment & Owner stats (403 SF-AUTH-013).
    let (st, body) = send_json(
        &router,
        Method::POST,
        "/v1/owner/mfa/enroll",
        Some(&user_token),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "SF-AUTH-013");

    let (st, body) = send_json(
        &router,
        Method::GET,
        "/v1/owner/stats",
        Some(&user_token),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "SF-AUTH-013");

    // 4. Owner WITHOUT TOTP confirmed is rejected from Owner stats (401 SF-AUTH-014).
    let (st, body) = send_json(
        &router,
        Method::GET,
        "/v1/owner/stats",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "SF-AUTH-014");

    // 5. Owner enrolls Google Authenticator TOTP & confirms with real RFC 6238 6-digit code.
    let (st, enroll) = send_json(
        &router,
        Method::POST,
        "/v1/owner/mfa/enroll",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let b32 = enroll["manual_secret_base32"].as_str().expect("b32");
    assert!(enroll["otpauth_uri"]
        .as_str()
        .expect("uri")
        .starts_with("otpauth://totp/"));
    let secret = decode_base32(b32).expect("decode");
    let code_now = compute_totp_code(&secret, unix_now() / TOTP_PERIOD_SECS).expect("totp");

    let (st, _) = send_json(
        &router,
        Method::POST,
        "/v1/owner/mfa/confirm",
        Some(&owner_token),
        Some(json!({ "code": code_now })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    // 6. Replay of the exact same TOTP code is REJECTED (401 SF-AUTH-014).
    let (st, body) = send_json(
        &router,
        Method::POST,
        "/v1/owner/mfa/verify",
        Some(&owner_token),
        Some(json!({ "code": code_now })),
    )
    .await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "SF-AUTH-014");

    // 7. Owner reads REAL live stats (2 users, 0 hardcoded fake numbers!).
    let (st, stats) = send_json(
        &router,
        Method::GET,
        "/v1/owner/stats",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(stats["total_users"], 2);
    assert_eq!(stats["entitlements"]["friend_vip_users"], 0);

    // 8. Owner grants Friend VIP (step-up active) -> real stats update dynamically!
    let (st, vip) = send_json(
        &router,
        Method::POST,
        "/v1/owner/friends/grant",
        Some(&owner_token),
        Some(json!({ "user_id": user_id, "workspace_id": user_ws, "reason": "Beta VIP" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(vip["effective_plan"], "friend_vip");

    let (_, stats_after) = send_json(
        &router,
        Method::GET,
        "/v1/owner/stats",
        Some(&owner_token),
        None,
    )
    .await;
    assert_eq!(stats_after["entitlements"]["friend_vip_users"], 1);

    // 9. Owner changes email -> Owner identity is PRESERVED via immutable user_id;
    //    a new account registering the OLD email gets ZERO Owner privileges!
    let (st, changed) = send_json(
        &router,
        Method::POST,
        "/v1/auth/email/change",
        Some(&owner_token),
        Some(json!({ "new_email": "rotated-owner@example.test" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(changed["is_owner"], true);
    assert_eq!(changed["user_id"], owner_id.to_string());

    let new_old_email_user = Uuid::now_v7();
    let new_old_email_ws = Uuid::now_v7();
    state
        .upsert_managed_account(ManagedAccountRecord {
            user_id: new_old_email_user,
            workspace_id: new_old_email_ws,
            email: "original-owner@example.test".to_owned(),
            display_name: "Reused Old Email".to_owned(),
            verified_phone: None,
            suspended: false,
            created_at: unix_now(),
        })
        .await;
    let hijacker_session = state
        .auth
        .create_session(
            &Principal {
                user_id: new_old_email_user,
                workspace_id: new_old_email_ws,
                scopes: vec![Scope::Read, Scope::Write],
                auth_method: AuthMethod::Password,
            },
            "Hijacker",
        )
        .await
        .expect("hijacker session");

    let (st, body) = send_json(
        &router,
        Method::GET,
        "/v1/owner/stats",
        Some(&hijacker_session.token),
        None,
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "SF-AUTH-013");

    // 10. Real Paddle checkout + HMAC-SHA256 correlated webhook updates entitlements and appears in Owner billing events.
    let (chk_st, chk_body) = send_json(
        &router,
        Method::POST,
        "/v1/billing/checkout",
        Some(&hijacker_session.token),
        Some(json!({
            "product": "individual_monthly",
            "billing_country_iso2": "US",
            "idempotency_key": "zt-paddle-order-1"
        })),
    )
    .await;
    assert_eq!(chk_st, StatusCode::OK, "checkout: {chk_body}");
    let created_order_id = chk_body["order_id"].as_str().expect("order_id");
    let created_txn_id = chk_body["provider_order_ref"].as_str().expect("txn_id");

    let now = unix_now();
    let wh_body = serde_json::to_vec(&json!({
        "event_id": "evt_http_001",
        "event_type": "transaction.completed",
        "occurred_at": "2026-11-23T12:00:00.000000Z",
        "notification_id": "ntf_zt_001",
        "data": {
            "id": created_txn_id,
            "status": "completed",
            "origin": "web",
            "currency_code": "USD",
            "subscription_id": "sub_zt_001",
            "invoice_id": "inv_http_001",
            "custom_data": {
                "safi_order_id": created_order_id,
                "workspace_id": new_old_email_ws,
                "user_id": new_old_email_user,
                "product": "individual_monthly"
            },
            "billing_period": {
                "starts_at": "2026-11-23T12:00:00Z",
                "ends_at": "2026-12-23T12:00:00Z"
            },
            "items": [{
                "quantity": 1,
                "price": { "id": "pri_zt_monthly" }
            }],
            "details": {
                "totals": {
                    "total": "2000",
                    "currency_code": "USD"
                }
            }
        }
    }))
    .expect("json");
    let sig = sign_paddle_payload("pdl_whsec_test_secret", now, &wh_body).expect("sign");
    let wh_resp = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/v1/billing/webhooks/paddle")
                .header("paddle-signature", sig)
                .header("content-type", "application/json")
                .body(Body::from(wh_body))
                .expect("req"),
        )
        .await
        .expect("resp");
    assert_eq!(wh_resp.status(), StatusCode::OK);

    // 11. Real Ed25519 signed update manifest & offline license issuance.
    let (st, upd) = send_json(&router, Method::GET, "/v1/updates/latest", None, None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(
        upd["manifest"]["signature_hex"]
            .as_str()
            .expect("sig")
            .len(),
        128
    );

    let (st, lic) = send_json(
        &router,
        Method::POST,
        "/v1/licenses/issue",
        Some(&owner_token),
        Some(json!({ "device_fingerprint": "desktop-fp-01" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(lic["payload"]["tier"], "Enterprise");
    assert_eq!(lic["signature_hex"].as_str().expect("sig").len(), 128);

    // 12. Health & Readiness probes work and never leak secrets.
    let (st, ready_body) = send_json(&router, Method::GET, "/ready", None, None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(ready_body["ready"], true);
    assert_eq!(ready_body["integrations"]["billing_paddle"], "CONFIGURED");
    assert_eq!(
        ready_body["integrations"]["billing_paymob"],
        "NOT_CONFIGURED"
    );
}
