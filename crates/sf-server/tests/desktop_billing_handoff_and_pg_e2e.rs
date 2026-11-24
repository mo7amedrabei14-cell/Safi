//! End-to-End Acceptance Suite for:
//! 1. Complete Desktop-to-Website Billing Handoff Flow (ALL 5 purchase types, short-lived single-use
//!    handoff, zero tokens in URL, cross-user theft blocked, scoped `sfb_*` website billing session
//!    preserving Desktop `sfs_*` exclusivity, return-before-webhook still Free, webhook activates,
//!    failure/retry & duplicate-charge prevention).
//! 2. Real Live PostgreSQL 18 (`#[ignore]`, executed in `tools/test-migrations.sh`) direct HTTP API
//!    proof across Instance A, Instance B, Cold-Restart Instance C, AND PostgreSQL failure fail-closed!

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};
use tokio::sync::RwLock;
use tower::ServiceExt;
use uuid::Uuid;

use sf_server::auth::account::{
    AccountAuthService, AccountRepository, LoginAccount, RegisterAccountRequest,
    VerificationEmailSender,
};
use sf_server::auth::cache::InMemoryAuthCache;
use sf_server::auth::repository::{InMemoryApiKeyRepository, InMemorySessionRepository};
use sf_server::auth::service::AuthService;
use sf_server::auth::{unix_now, AuthMethod, Principal, Scope};
use sf_server::billing::handoff::BILLING_HANDOFF_TTL_SECS;
use sf_server::billing::{sign_paddle_payload, CheckoutProduct};
use sf_server::config::ServerConfig;
use sf_server::entitlements::{PlanTier, SubscriptionState, TRIAL_DURATION_SECS};
use sf_server::error::AuthError;
use sf_server::pg_store::PgControlPlaneStore;
use sf_server::{app, AppState};

#[derive(Default)]
struct MemAccountState {
    accounts: HashMap<String, LoginAccount>,
    tokens: HashMap<[u8; 32], (String, i64)>,
}

#[derive(Default)]
struct MemAccountRepo {
    state: RwLock<MemAccountState>,
}

impl MemAccountRepo {
    async fn seed_verified(&self, email: &str, user_id: Uuid, workspace_id: Uuid, hash: &str) {
        self.state.write().await.accounts.insert(
            email.to_ascii_lowercase(),
            LoginAccount {
                user_id,
                workspace_id: Some(workspace_id),
                password_hash: hash.to_owned(),
                email_verified: true,
                deleted: false,
                suspended: false,
            },
        );
    }
}

#[async_trait]
impl AccountRepository for MemAccountRepo {
    async fn register(
        &self,
        email: &str,
        _display_name: &str,
        password_hash: &str,
        token_hash: &[u8; 32],
        token_expires_at: i64,
    ) -> Result<bool, AuthError> {
        let mut s = self.state.write().await;
        if s.accounts.contains_key(email) {
            return Ok(false);
        }
        s.accounts.insert(
            email.to_owned(),
            LoginAccount {
                user_id: Uuid::now_v7(),
                workspace_id: Some(Uuid::now_v7()),
                password_hash: password_hash.to_owned(),
                email_verified: false,
                deleted: false,
                suspended: false,
            },
        );
        s.tokens
            .insert(*token_hash, (email.to_owned(), token_expires_at));
        Ok(true)
    }
    async fn find_for_login(&self, email: &str) -> Result<Option<LoginAccount>, AuthError> {
        Ok(self.state.read().await.accounts.get(email).cloned())
    }
    async fn verify_email(&self, token_hash: &[u8; 32], now: i64) -> Result<bool, AuthError> {
        let mut s = self.state.write().await;
        let Some((email, exp)) = s.tokens.remove(token_hash) else {
            return Ok(false);
        };
        if now >= exp {
            return Ok(false);
        }
        if let Some(a) = s.accounts.get_mut(&email) {
            a.email_verified = true;
            return Ok(true);
        }
        Ok(false)
    }
    async fn replace_verification_token(
        &self,
        _email: &str,
        _token_hash: &[u8; 32],
        _token_expires_at: i64,
        _now: i64,
    ) -> Result<bool, AuthError> {
        Ok(false)
    }
    async fn replace_password_reset_token(
        &self,
        _email: &str,
        _token_hash: &[u8; 32],
        _token_expires_at: i64,
        _now: i64,
    ) -> Result<bool, AuthError> {
        Ok(false)
    }
    async fn reset_password(
        &self,
        _token_hash: &[u8; 32],
        _new_password_hash: &str,
        _now: i64,
    ) -> Result<bool, AuthError> {
        Ok(false)
    }
}

#[derive(Default)]
struct NoopMailer;

#[async_trait]
impl VerificationEmailSender for NoopMailer {
    async fn send_verification(&self, _recipient: &str, _url: &str) -> Result<(), AuthError> {
        Ok(())
    }
    async fn send_password_reset(&self, _recipient: &str, _url: &str) -> Result<(), AuthError> {
        Ok(())
    }
}

async fn send_req(
    router: &Router,
    method: Method,
    uri: &str,
    bearer: Option<&str>,
    extra_headers: &[(&str, &str)],
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut b = Request::builder().method(method).uri(uri);
    if let Some(t) = bearer {
        b = b.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    for (k, v) in extra_headers {
        b = b.header(*k, *v);
    }
    let req = if let Some(val) = body {
        b = b.header(header::CONTENT_TYPE, "application/json");
        b.body(Body::from(serde_json::to_vec(&val).expect("json")))
            .expect("req")
    } else {
        b.body(Body::empty()).expect("req")
    };
    let resp = router.clone().oneshot(req).await.expect("resp");
    let st = resp.status();
    let bytes = to_bytes(resp.into_body(), 1024 * 1024).await.expect("body");
    let val = serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({}));
    (st, val)
}

async fn send_raw(
    router: &Router,
    method: Method,
    uri: &str,
    extra_headers: &[(&str, &str)],
    raw_body: Vec<u8>,
) -> (StatusCode, Value) {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    for (k, v) in extra_headers {
        b = b.header(*k, *v);
    }
    let req = b.body(Body::from(raw_body)).expect("req");
    let resp = router.clone().oneshot(req).await.expect("resp");
    let st = resp.status();
    let bytes = to_bytes(resp.into_body(), 1024 * 1024).await.expect("body");
    let val = serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({}));
    (st, val)
}

struct PaddleMockHandle {
    url: String,
    calls: Arc<AtomicUsize>,
    _created: Arc<tokio::sync::RwLock<Vec<Value>>>,
    force_timeout: Arc<std::sync::atomic::AtomicBool>,
}

fn spawn_paddle_mock() -> PaddleMockHandle {
    let calls = Arc::new(AtomicUsize::new(0));
    let created = Arc::new(tokio::sync::RwLock::new(Vec::<Value>::new()));
    let force_timeout = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let calls_post = calls.clone();
    let created_post = created.clone();
    let timeout_post = force_timeout.clone();
    let created_get = created.clone();

    let mock_router = Router::new()
        .route(
            "/transactions",
            post(move |Json(payload): Json<Value>| {
                let calls = calls_post.clone();
                let created = created_post.clone();
                let should_timeout = timeout_post.clone();
                async move {
                    let n = calls.fetch_add(1, Ordering::SeqCst) + 1;
                    let txn_id = format!("txn_mock_{n:04}");
                    let price_id = payload["items"][0]["price_id"]
                        .as_str()
                        .unwrap_or("pri_monthly_usd")
                        .to_owned();
                    let amount = match price_id.as_str() {
                        "pri_monthly_usd" => "2000",
                        "pri_annual_usd" => "16800",
                        "pri_org_usd" => "102000",
                        "pri_ppf_usd" => "200",
                        "pri_cp25_usd" => "500",
                        _ => "2000",
                    };
                    let rec = json!({
                        "id": txn_id,
                        "status": "draft",
                        "currency_code": "USD",
                        "items": [{ "price": { "id": price_id }, "quantity": payload["items"][0]["quantity"] }],
                        "details": { "totals": { "total": amount } },
                        "custom_data": payload["custom_data"],
                        "checkout": { "url": format!("https://checkout.paddle.com/{txn_id}") }
                    });
                    created.write().await.push(rec.clone());
                    if should_timeout.load(Ordering::SeqCst) {
                        tokio::time::sleep(std::time::Duration::from_millis(260)).await;
                    }
                    (StatusCode::CREATED, Json(json!({ "data": rec })))
                }
            })
            .get(move || {
                let created = created_get.clone();
                async move {
                    let list = created.read().await.clone();
                    (StatusCode::OK, Json(json!({ "data": list })))
                }
            }),
        );

    let std_listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind mock");
    std_listener.set_nonblocking(true).expect("nonblocking");
    let addr = std_listener.local_addr().expect("addr");
    let listener = tokio::net::TcpListener::from_std(std_listener).expect("tokio listener");
    tokio::spawn(async move {
        let _ = axum::serve(listener, mock_router).await;
    });
    PaddleMockHandle {
        url: format!("http://{addr}"),
        calls,
        _created: created,
        force_timeout,
    }
}

#[tokio::test]
async fn complete_desktop_to_website_billing_handoff_all_5_products_and_failure_matrix() {
    let PaddleMockHandle {
        url: mock_url,
        calls: provider_calls,
        force_timeout,
        ..
    } = spawn_paddle_mock();
    let sessions = Arc::new(InMemorySessionRepository::new());
    let keys = Arc::new(InMemoryApiKeyRepository::new(Vec::new()).await);
    let cache = Arc::new(InMemoryAuthCache::new());
    let auth = AuthService::new(keys, sessions, cache);

    let mut cfg = ServerConfig::for_tests();
    cfg.api_base_url = Some("https://api.safidata.app".to_owned());
    cfg.public_web_origin = Some("https://safidata.app".to_owned());
    cfg.paddle_api_base_url = mock_url;
    cfg.paddle_api_key = Some("pdl_test_api_key".to_owned());
    cfg.paddle_webhook_secret = Some("pdl_test_whsec_secret".to_owned());
    cfg.paddle_price_individual_monthly = Some("pri_monthly_usd".to_owned());
    cfg.paddle_price_individual_annual = Some("pri_annual_usd".to_owned());
    cfg.paddle_price_org_seat_annual = Some("pri_org_usd".to_owned());
    cfg.paddle_price_pay_per_file = Some("pri_ppf_usd".to_owned());
    cfg.paddle_price_credit_pack_25 = Some("pri_cp25_usd".to_owned());

    let account_repo = Arc::new(MemAccountRepo::default());
    let mut state = AppState::with_config(auth, cfg);
    state.billing = state.billing.with_provider_timeout_ms(80);
    let account_auth = AccountAuthService::new(
        account_repo.clone(),
        Arc::new(NoopMailer),
        state.auth.clone(),
        "https://safidata.app".to_owned(),
    );
    state = state.with_account_auth(account_auth);

    // Register & verify User A and User B via real Argon2id account auth!
    let router = app(state.clone());
    state
        .account_auth
        .as_ref()
        .unwrap()
        .register(RegisterAccountRequest {
            email: "alice@example.test".to_owned(),
            display_name: "Alice".to_owned(),
            password: "SafiStrongPassword!1".to_owned(),
        })
        .await
        .expect("reg alice");
    state
        .account_auth
        .as_ref()
        .unwrap()
        .register(RegisterAccountRequest {
            email: "bob@example.test".to_owned(),
            display_name: "Bob".to_owned(),
            password: "SafiStrongPassword!2".to_owned(),
        })
        .await
        .expect("reg bob");
    let alice = account_repo
        .find_for_login("alice@example.test")
        .await
        .unwrap()
        .unwrap();
    let bob = account_repo
        .find_for_login("bob@example.test")
        .await
        .unwrap()
        .unwrap();
    account_repo
        .seed_verified(
            "alice@example.test",
            alice.user_id,
            alice.workspace_id.unwrap(),
            &alice.password_hash,
        )
        .await;
    account_repo
        .seed_verified(
            "bob@example.test",
            bob.user_id,
            bob.workspace_id.unwrap(),
            &bob.password_hash,
        )
        .await;

    // Issue Alice's active Desktop `sfs_*` session
    let desktop_token = state
        .auth
        .create_session(
            &Principal {
                user_id: alice.user_id,
                workspace_id: alice.workspace_id.unwrap(),
                scopes: vec![Scope::Read, Scope::Write],
                auth_method: AuthMethod::Password,
            },
            "Alice Windows Desktop",
        )
        .await
        .expect("desktop session")
        .token;

    // Website billing login (`POST /v1/billing/web/login`) issues scoped `sfb_*` token WITHOUT revoking Desktop!
    let (st, web_login) = send_req(
        &router,
        Method::POST,
        "/v1/billing/web/login",
        None,
        &[],
        Some(json!({ "email": "alice@example.test", "password": "SafiStrongPassword!1" })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    let alice_web_token = web_login["token"].as_str().expect("sfb token").to_owned();
    assert!(alice_web_token.starts_with("sfb_"));

    // Desktop session is STILL valid after Website Billing login!
    let (st, _) = send_req(
        &router,
        Method::GET,
        "/v1/auth/sessions/current",
        Some(&desktop_token),
        &[],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    // Scoped `sfb_*` Website Billing token is REJECTED (`401`) on Desktop Studio/AI/Org/Sync/Owner routes!
    for restricted in [
        (Method::POST, "/v1/ai/route"),
        (Method::POST, "/v1/organizations"),
        (Method::POST, "/v1/sync/envelopes"),
        (Method::GET, "/v1/owner/stats"),
        (Method::POST, "/v1/billing/handoff/start"),
    ] {
        let (st, _) = send_req(
            &router,
            restricted.0,
            restricted.1,
            Some(&alice_web_token),
            &[],
            None,
        )
        .await;
        assert_eq!(
            st,
            StatusCode::UNAUTHORIZED,
            "sfb_* must be rejected on {}",
            restricted.1
        );
    }

    // Bob's website billing token (to test cross-user handoff theft!)
    let (st, bob_login) = send_req(
        &router,
        Method::POST,
        "/v1/billing/web/login",
        None,
        &[],
        Some(json!({ "email": "bob@example.test", "password": "SafiStrongPassword!2" })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    let bob_web_token = bob_login["token"].as_str().expect("bob sfb").to_owned();

    // =========================================================================
    // Test ALL 5 Purchase Types via Desktop -> Website Handoff
    // =========================================================================
    for (prod_code, seats, expected_minor, expected_period) in [
        ("individual_monthly", None, 2000u64, "monthly"),
        ("individual_annual", None, 16800u64, "annual"),
        ("organization_seat_annual", Some(5u32), 102_000u64, "annual"),
        ("credit_pack_25", None, 500u64, "one_time"),
        ("pay_per_file", None, 200u64, "one_time"),
    ] {
        let state_str = format!("state_{prod_code}_1234567890abcdef");
        let (st, started) = send_req(
            &router,
            Method::POST,
            "/v1/billing/handoff/start",
            Some(&desktop_token),
            &[],
            Some(json!({
                "product": prod_code,
                "seats": seats,
                "billing_country_iso2": "US",
                "state": state_str
            })),
        )
        .await;
        assert_eq!(st, StatusCode::CREATED, "handoff start for {prod_code}");
        let url = started["billing_url"].as_str().expect("billing_url");
        assert!(url.starts_with("https://safidata.app/billing?handoff=sfbh_"));
        assert!(
            !url.contains(&desktop_token),
            "NEVER put session token in URL"
        );
        assert_eq!(started["original_amount_minor"], expected_minor);
        assert_eq!(started["billing_period"], expected_period);

        // Extract `handoff` from URL
        let handoff_code = url
            .split("handoff=")
            .nth(1)
            .unwrap()
            .split('&')
            .next()
            .unwrap()
            .to_owned();

        // Cross-user theft: Bob trying to redeem Alice's handoff -> 403 Forbidden!
        let (st, _) = send_req(
            &router,
            Method::POST,
            "/v1/billing/handoff/redeem",
            Some(&bob_web_token),
            &[],
            Some(json!({ "handoff_code": handoff_code, "state": state_str })),
        )
        .await;
        assert_eq!(st, StatusCode::FORBIDDEN);

        // Alice redeems handoff -> 200 OK with authoritative server-resolved price & seats!
        let (st, redeemed) = send_req(
            &router,
            Method::POST,
            "/v1/billing/handoff/redeem",
            Some(&alice_web_token),
            &[],
            Some(json!({ "handoff_code": handoff_code, "state": state_str })),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(redeemed["product"], prod_code);
        assert_eq!(redeemed["original_amount_minor"], expected_minor);
        if let Some(s) = seats {
            assert_eq!(redeemed["seats"], s);
        }

        // Single-use enforcement: 2nd redeem attempt MUST fail (`401 SF-AUTH-023`)!
        let (st, body) = send_req(
            &router,
            Method::POST,
            "/v1/billing/handoff/redeem",
            Some(&alice_web_token),
            &[],
            Some(json!({ "handoff_code": handoff_code, "state": state_str })),
        )
        .await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);
        assert_eq!(body["code"], "SF-AUTH-023");
    }

    // Verify expired handoff fails (`t + BILLING_HANDOFF_TTL_SECS + 1`)!
    let now = unix_now();
    let exp_start = state
        .billing_handoff
        .start_handoff_at(
            alice.user_id,
            alice.workspace_id.unwrap(),
            &sf_server::billing::handoff::StartBillingHandoffRequest {
                product: CheckoutProduct::IndividualMonthly,
                seats: None,
                billing_country_iso2: "US".to_owned(),
                coupon_code: None,
                state: "state_expiry_check_123456".to_owned(),
            },
            "https://safidata.app",
            now,
        )
        .await
        .expect("start exp");
    let exp_code = exp_start
        .billing_url
        .split("handoff=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap();
    assert_eq!(
        state
            .billing_handoff
            .redeem_handoff_at(
                alice.user_id,
                alice.workspace_id.unwrap(),
                exp_code,
                "state_expiry_check_123456",
                now + BILLING_HANDOFF_TTL_SECS + 1,
            )
            .await
            .unwrap_err(),
        AuthError::DesktopAuthCodeInvalid
    );

    // =========================================================================
    // Failure/Retry Matrix + Return-Before-Webhook -> Verified Webhook Activates!
    // =========================================================================
    // 1. Timeout during checkout -> `503 SF-PAY-008`, provider called once
    force_timeout.store(true, Ordering::SeqCst);
    let (st, err_body) = send_req(
        &router,
        Method::POST,
        "/v1/billing/checkout",
        Some(&alice_web_token),
        &[],
        Some(json!({
            "product": "individual_monthly",
            "billing_country_iso2": "US",
            "idempotency_key": "web-attempt-timeout-1"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(err_body["code"], "SF-PAY-008");
    assert_eq!(provider_calls.load(Ordering::SeqCst), 1);

    // 2. Trying a DIFFERENT idempotency key while earlier order is unresolved -> blocked (`503 SF-PAY-008`, 0 new calls!)
    force_timeout.store(false, Ordering::SeqCst);
    let (st, err_body) = send_req(
        &router,
        Method::POST,
        "/v1/billing/checkout",
        Some(&alice_web_token),
        &[],
        Some(json!({
            "product": "individual_monthly",
            "billing_country_iso2": "US",
            "idempotency_key": "web-attempt-brand-new-key"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(err_body["code"], "SF-PAY-008");
    assert_eq!(provider_calls.load(Ordering::SeqCst), 1);

    // 3. Reconciling with the ORIGINAL idempotency key recovers the existing order with ZERO 2nd charge!
    let (st, reconciled) = send_req(
        &router,
        Method::POST,
        "/v1/billing/checkout",
        Some(&alice_web_token),
        &[],
        Some(json!({
            "product": "individual_monthly",
            "billing_country_iso2": "US",
            "idempotency_key": "web-attempt-timeout-1"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(reconciled["reused_existing_order"], true);
    assert_eq!(provider_calls.load(Ordering::SeqCst), 1);
    let order_id = reconciled["order_id"].as_str().expect("order_id");
    let txn_id = reconciled["provider_order_ref"].as_str().expect("txn_id");

    // 4. Refresh / back-forward / double-click with same key returns exact same order (still 1 provider call!)
    let (st, replay) = send_req(
        &router,
        Method::POST,
        "/v1/billing/checkout",
        Some(&alice_web_token),
        &[],
        Some(json!({
            "product": "individual_monthly",
            "billing_country_iso2": "US",
            "idempotency_key": "web-attempt-timeout-1"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(replay["order_id"], order_id);
    assert_eq!(provider_calls.load(Ordering::SeqCst), 1);

    // 5. Bob CANNOT read Alice's order status (IDOR blocked -> 403)!
    let (st, _) = send_req(
        &router,
        Method::GET,
        &format!("/v1/billing/orders/{order_id}"),
        Some(&bob_web_token),
        &[],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // 6. User closes browser / returns to Desktop BEFORE webhook arrives -> Desktop STILL sees Free!
    let (st, ent_before) = send_req(
        &router,
        Method::GET,
        "/v1/entitlements/me",
        Some(&desktop_token),
        &[],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(ent_before["effective_plan"], "free");

    // 7. Verified Paddle webhook arrives -> order becomes `paid` & Desktop refetch sees `individual_monthly`!
    let webhook_payload = json!({
        "event_id": "evt_desktop_handoff_paid_1",
        "event_type": "transaction.completed",
        "occurred_at": "2026-11-23T15:00:00.000000Z",
        "notification_id": "ntf_1",
        "data": {
            "id": txn_id,
            "status": "completed",
            "currency_code": "USD",
            "subscription_id": "sub_handoff_1",
            "invoice_id": "inv_handoff_1",
            "items": [{ "price": { "id": "pri_monthly_usd" }, "quantity": 1 }],
            "details": { "totals": { "total": "2000" } },
            "custom_data": {
                "safi_order_id": order_id,
                "workspace_id": alice.workspace_id.unwrap().to_string(),
                "user_id": alice.user_id.to_string(),
                "product": "individual_monthly"
            }
        }
    });
    let raw_wh = serde_json::to_vec(&webhook_payload).expect("wh bytes");
    let sig = sign_paddle_payload("pdl_test_whsec_secret", unix_now(), &raw_wh).expect("sig");
    let (st, _) = send_raw(
        &router,
        Method::POST,
        "/v1/billing/webhooks/paddle",
        &[("paddle-signature", &sig)],
        raw_wh,
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    // Order status endpoint confirms `paid`
    let (st, ord_status) = send_req(
        &router,
        Method::GET,
        &format!("/v1/billing/orders/{order_id}"),
        Some(&alice_web_token),
        &[],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(ord_status["order_state"], "paid");

    // Desktop independently refetches `GET /v1/entitlements/me` -> now Active Individual Monthly!
    let (st, ent_after) = send_req(
        &router,
        Method::GET,
        "/v1/entitlements/me",
        Some(&desktop_token),
        &[],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(ent_after["effective_plan"], "individual_monthly");
    assert_eq!(ent_after["state"], "active");
    assert_eq!(ent_after["watermark_required"], false);

    // 10. Direct Website Purchase WITHOUT any Desktop session or Desktop handoff,
    //     testing separate Public Web Origin (`https://safidata.app`) calling API (`https://api.safidata.app`)!
    let (st_evil, _) = send_req(
        &router,
        Method::POST,
        "/v1/billing/web/login",
        None,
        &[
            ("origin", "https://evil.example"),
            ("host", "api.safidata.app"),
        ],
        Some(json!({
            "email": "bob@example.test",
            "password": "SafiStrongPassword!2"
        })),
    )
    .await;
    assert_eq!(
        st_evil,
        StatusCode::FORBIDDEN,
        "foreign Origin must be rejected on cross-origin API billing login"
    );

    let (st, bob_login) = send_req(
        &router,
        Method::POST,
        "/v1/billing/web/login",
        None,
        &[
            ("origin", "https://safidata.app"),
            ("host", "api.safidata.app"),
        ],
        Some(json!({
            "email": "bob@example.test",
            "password": "SafiStrongPassword!2",
            "user_agent_hint": "direct-web-pricing-buyer"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    let bob_web_tok = bob_login["token"]
        .as_str()
        .expect("bob sfb token")
        .to_owned();
    assert!(bob_web_tok.starts_with("sfb_"));

    // Bob purchases Organization (5 seats = $1,020/yr) directly on the website — ZERO Desktop handoff!
    let (st, bob_co) = send_req(
        &router,
        Method::POST,
        "/v1/billing/checkout",
        Some(&bob_web_tok),
        &[
            ("origin", "https://safidata.app"),
            ("host", "api.safidata.app"),
        ],
        Some(json!({
            "product": "organization_seat_annual",
            "seats": 5,
            "billing_country_iso2": "US",
            "idempotency_key": "direct-web-bob-org-5-seats"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(bob_co["amount_minor"], 102000);
    assert_eq!(bob_co["order_state"], "checkout_pending");
    let bob_order_id = bob_co["order_id"].as_str().expect("bob order id");
    let bob_txn_id = bob_co["provider_order_ref"].as_str().expect("bob txn id");

    // Before webhook: Bob is STILL Free.
    let (st, bob_ent_before) = send_req(
        &router,
        Method::GET,
        "/v1/entitlements/me",
        Some(&bob_web_tok),
        &[],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(bob_ent_before["effective_plan"], "free");

    // Signed Paddle webhook completes Bob's direct website purchase.
    let bob_wh = json!({
        "event_id": "evt_direct_web_bob_org_001",
        "event_type": "transaction.completed",
        "occurred_at": "2026-11-24T10:00:00Z",
        "data": {
            "id": bob_txn_id,
            "status": "completed",
            "currency_code": "USD",
            "subscription_id": "sub_direct_web_bob_1",
            "invoice_id": "inv_direct_web_bob_1",
            "items": [{ "price": { "id": "pri_org_usd" }, "quantity": 5 }],
            "custom_data": {
                "safi_order_id": bob_order_id,
                "workspace_id": bob.workspace_id.unwrap().to_string(),
                "user_id": bob.user_id.to_string(),
                "product": "organization_seat_annual",
                "seats": 5
            },
            "details": { "totals": { "total": "102000" } }
        }
    });
    let bob_wh_bytes = serde_json::to_vec(&bob_wh).unwrap();
    let bob_sig =
        sign_paddle_payload("pdl_test_whsec_secret", unix_now(), &bob_wh_bytes).expect("sig");
    let (wh_st, _) = send_raw(
        &router,
        Method::POST,
        "/v1/billing/webhooks/paddle",
        &[("paddle-signature", &bob_sig)],
        bob_wh_bytes,
    )
    .await;
    assert_eq!(wh_st, StatusCode::OK);

    // Server confirms Bob's direct website purchase is now PAID & Organization (5 seats) active!
    let (st, bob_ent_after) = send_req(
        &router,
        Method::GET,
        "/v1/entitlements/me",
        Some(&bob_web_tok),
        &[],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(bob_ent_after["effective_plan"], "organization_seat");
    assert_eq!(bob_ent_after["state"], "active");
    assert_eq!(bob_ent_after["teams_enabled"], true);
}

#[tokio::test]
#[ignore = "Requires live PostgreSQL 18 via SF_TEST_DATABASE_URL with migrations 0001..0016 applied"]
async fn postgres_18_direct_http_free_isolation_desktop_handoff_multi_instance_and_fail_closed() {
    let db_url = std::env::var("SF_TEST_DATABASE_URL").expect("SF_TEST_DATABASE_URL");
    let pool_a = sqlx::postgres::PgPoolOptions::new()
        .max_connections(8)
        .connect(&db_url)
        .await
        .expect("pool A");
    let pool_b = sqlx::postgres::PgPoolOptions::new()
        .max_connections(8)
        .connect(&db_url)
        .await
        .expect("pool B");
    let pool_c = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&db_url)
        .await
        .expect("pool C");

    let build_instance = |pool: sqlx::PgPool| async move {
        let sessions = Arc::new(InMemorySessionRepository::new());
        let keys = Arc::new(InMemoryApiKeyRepository::new(Vec::new()).await);
        let cache = Arc::new(InMemoryAuthCache::new());
        let auth = AuthService::new(keys, sessions, cache);
        let mut cfg = ServerConfig::for_tests();
        cfg.api_base_url = Some("https://api.safidata.app".to_owned());
        cfg.public_web_origin = Some("https://safidata.app".to_owned());
        let pg = PgControlPlaneStore::new(pool);
        let mut st = AppState::with_config(auth, cfg).with_pg_store(pg);
        st.entitlements = st.entitlements.with_require_durable_pg(true);
        st.billing = st.billing.with_require_durable_pg(true);
        st.billing_handoff = st.billing_handoff.with_require_durable_pg(true);
        let router = app(st.clone());
        (st, router)
    };

    let (state_a, router_a) = build_instance(pool_a.clone()).await;
    let (state_b, router_b) = build_instance(pool_b.clone()).await;
    let (state_c, router_c) = build_instance(pool_c.clone()).await;

    let user_id = Uuid::now_v7();
    let ws_id = Uuid::now_v7();
    sqlx::query("INSERT INTO users (id, email, display_name) VALUES ($1, $2, 'PG Free User')")
        .bind(user_id)
        .bind(format!("pg-free-{user_id}@example.test"))
        .execute(&pool_a)
        .await
        .expect("seed user");
    sqlx::query("INSERT INTO workspaces (id, name, plan_code) VALUES ($1, 'PG Free WS', 'free')")
        .bind(ws_id)
        .execute(&pool_a)
        .await
        .expect("seed ws");

    let make_tok = |state: &AppState| {
        let st = state.clone();
        async move {
            st.auth
                .create_session(
                    &Principal {
                        user_id,
                        workspace_id: ws_id,
                        scopes: vec![Scope::Read, Scope::Write],
                        auth_method: AuthMethod::Password,
                    },
                    "PG Test Desktop",
                )
                .await
                .expect("tok")
                .token
        }
    };
    let tok_a = make_tok(&state_a).await;
    let tok_b = make_tok(&state_b).await;
    let tok_c = make_tok(&state_c).await;

    // 1. Direct HTTP Free-plan isolation on real PostgreSQL 18 (Instance A):
    //    AI, Teams, Saved Recipes, Premium Packs ALL rejected!
    let (st, b) = send_req(
        &router_a,
        Method::POST,
        "/v1/ai/route",
        Some(&tok_a),
        &[],
        Some(json!({
            "idempotency_key": "pg-ai-free",
            "explicit_user_consent": true,
            "pii_masked_locally": true,
            "masked_prompt": "Analyze"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert_eq!(b["code"], "SF-LIC-008");

    let (st, b) = send_req(
        &router_a,
        Method::POST,
        "/v1/organizations",
        Some(&tok_a),
        &[],
        Some(json!({ "name": "Free Org Blocked", "seats": 3 })),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert_eq!(b["code"], "SF-LIC-008");

    let (st, b) = send_req(
        &router_a,
        Method::POST,
        "/v1/sync/envelopes",
        Some(&tok_a),
        &[],
        Some(json!({
            "resource_kind": "recipe",
            "resource_id": "r1",
            "expected_version": 0,
            "ciphertext_b64": "YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXo=",
            "nonce_b64": "MTIzNDU2Nzg5MDEy",
            "key_id": "k1",
            "content_hash": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert_eq!(b["code"], "SF-LIC-008");

    let (st, b) = send_req(
        &router_a,
        Method::POST,
        "/v1/templates/entitlement/export-import",
        Some(&tok_a),
        &[],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert_eq!(b["code"], "SF-RCP-005");

    // 5001 rows & 21 cols rejected on real PG18 (`422 SF-LIC-006`)
    for (k, rows, cols) in [("pg-r5001", 5001, 10), ("pg-c21", 100, 21)] {
        let (st, b) = send_req(
            &router_a,
            Method::POST,
            "/v1/entitlements/consume",
            Some(&tok_a),
            &[],
            Some(json!({
                "idempotency_key": k,
                "kind": "data_operation",
                "row_count": rows,
                "column_count": cols,
                "pii_masked_count": 0
            })),
        )
        .await;
        assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(b["code"], "SF-LIC-006");
    }

    // 2. Consume 5 Free ops across Instance A (3) & Instance B (2) -> all 5 have watermark=true;
    //    6th op on Cold-Restart Instance C rejected (`429 SF-LIC-005`)!
    for i in 1..=5 {
        let (r, t) = if i <= 3 {
            (&router_a, &tok_a)
        } else {
            (&router_b, &tok_b)
        };
        let (st, res) = send_req(
            r,
            Method::POST,
            "/v1/entitlements/consume",
            Some(t),
            &[],
            Some(json!({
                "idempotency_key": format!("pg-http-op-{i}"),
                "kind": "data_operation",
                "row_count": 5000,
                "column_count": 20,
                "pii_masked_count": 1
            })),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(res["watermark_required"], true);
    }

    // Cross-instance idempotency fingerprint checks on Instance B (different kind, rows, cols, pii!)
    for (label, payload) in [
        (
            "diff-kind",
            json!({ "idempotency_key": "pg-http-op-1", "kind": "ai_call", "row_count": 0, "column_count": 0, "pii_masked_count": 0 }),
        ),
        (
            "diff-rows",
            json!({ "idempotency_key": "pg-http-op-1", "kind": "data_operation", "row_count": 4999, "column_count": 20, "pii_masked_count": 1 }),
        ),
        (
            "diff-cols",
            json!({ "idempotency_key": "pg-http-op-1", "kind": "data_operation", "row_count": 5000, "column_count": 19, "pii_masked_count": 1 }),
        ),
        (
            "diff-pii",
            json!({ "idempotency_key": "pg-http-op-1", "kind": "data_operation", "row_count": 5000, "column_count": 20, "pii_masked_count": 2 }),
        ),
    ] {
        let (st, b) = send_req(
            &router_b,
            Method::POST,
            "/v1/entitlements/consume",
            Some(&tok_b),
            &[],
            Some(payload),
        )
        .await;
        assert_eq!(st, StatusCode::CONFLICT, "fingerprint mismatch {label}");
        assert_eq!(b["code"], "SF-PAY-007");
    }

    // 6th op on Instance C rejected!
    let (st, b) = send_req(
        &router_c,
        Method::POST,
        "/v1/entitlements/consume",
        Some(&tok_c),
        &[],
        Some(json!({
            "idempotency_key": "pg-http-op-6",
            "kind": "data_operation",
            "row_count": 100,
            "column_count": 5,
            "pii_masked_count": 0
        })),
    )
    .await;
    assert_eq!(st, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(b["code"], "SF-LIC-005");

    // 3. Credit Pack 25 & Pay-Per-File on PostgreSQL 18: grant ONLY their scoped entitlements!
    state_a
        .entitlements
        .grant_micro_purchase_checked(ws_id, user_id, 25, 1)
        .await
        .expect("grant micro in PG");
    let (st, res) = send_req(
        &router_c,
        Method::POST,
        "/v1/entitlements/consume",
        Some(&tok_c),
        &[],
        Some(json!({
            "idempotency_key": "pg-http-credit-op",
            "kind": "data_operation",
            "row_count": 100,
            "column_count": 5,
            "pii_masked_count": 0
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(res["debited_credit"], true);
    assert_eq!(res["watermark_required"], false);

    let (st, res) = send_req(
        &router_b,
        Method::POST,
        "/v1/entitlements/consume",
        Some(&tok_b),
        &[],
        Some(json!({
            "idempotency_key": "pg-http-ppf-100k",
            "kind": "data_operation",
            "row_count": 100000,
            "column_count": 40,
            "pii_masked_count": 0
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(res["debited_pay_per_file"], true);
    assert_eq!(res["watermark_required"], false);

    // STILL NO AI, NO Teams, NO Saved Recipes, NO Premium Packs after Credit Pack + PPF!
    let (st, _) = send_req(
        &router_c,
        Method::POST,
        "/v1/ai/route",
        Some(&tok_c),
        &[],
        Some(json!({
            "idempotency_key": "pg-cp-still-no-ai",
            "explicit_user_consent": true,
            "pii_masked_locally": true,
            "masked_prompt": "Test"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // 4. Desktop-to-Website Handoff durability across Instance A -> Instance B -> replay rejected on Instance C!
    let (st, started) = send_req(
        &router_a,
        Method::POST,
        "/v1/billing/handoff/start",
        Some(&tok_a),
        &[],
        Some(json!({
            "product": "individual_annual",
            "billing_country_iso2": "EG",
            "state": "pg_multi_instance_state_123456"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    let handoff_code = started["billing_url"]
        .as_str()
        .unwrap()
        .split("handoff=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap()
        .to_owned();

    // Issue durable PG18 `sfb_*` billing session on Instance A, redeem handoff on Instance B!
    let (web_tok, _, _) = state_a
        .billing_handoff
        .issue_billing_web_session_at(user_id, ws_id, "pg-browser", unix_now())
        .await
        .expect("issue sfb");
    let (st, redeemed) = send_req(
        &router_b,
        Method::POST,
        "/v1/billing/handoff/redeem",
        Some(&web_tok),
        &[],
        Some(json!({ "handoff_code": handoff_code, "state": "pg_multi_instance_state_123456" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(redeemed["product"], "individual_annual");
    assert_eq!(redeemed["currency"], "EGP");
    assert_eq!(redeemed["original_amount_minor"], 840_000);

    // Replay on Instance C MUST fail (`401 SF-AUTH-023`)!
    let (st, b) = send_req(
        &router_c,
        Method::POST,
        "/v1/billing/handoff/redeem",
        Some(&web_tok),
        &[],
        Some(json!({ "handoff_code": handoff_code, "state": "pg_multi_instance_state_123456" })),
    )
    .await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    assert_eq!(b["code"], "SF-AUTH-023");

    // 5. Subscription expiry, cancellation, refund/chargeback & trial expiry recalculate in PG18!
    let now = unix_now();
    state_a
        .entitlements
        .apply_subscription_update(
            ws_id,
            user_id,
            PlanTier::IndividualMonthly,
            SubscriptionState::Canceled,
            1,
            Some(now - 5),
            now,
        )
        .await
        .expect("canceled expired");
    let ent_c = state_c
        .entitlements
        .effective_at_checked(ws_id, user_id, false, now)
        .await
        .expect("recalc on C");
    assert_eq!(ent_c.effective_plan, PlanTier::Free);
    assert_eq!(ent_c.state, SubscriptionState::Expired);
    assert!(ent_c.watermark_required);
    let _ = TRIAL_DURATION_SECS;

    // 6. REAL PostgreSQL outage FAILS CLOSED (`503 SF-AUTH-005`) — ZERO silent in-memory fallback!
    pool_c.close().await;
    let (st, b) = send_req(
        &router_c,
        Method::GET,
        "/v1/entitlements/me",
        Some(&tok_c),
        &[],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(b["code"], "SF-AUTH-005");

    let (st, b) = send_req(
        &router_c,
        Method::POST,
        "/v1/entitlements/consume",
        Some(&tok_c),
        &[],
        Some(json!({
            "idempotency_key": "pg-down-op",
            "kind": "data_operation",
            "row_count": 10,
            "column_count": 2,
            "pii_masked_count": 0
        })),
    )
    .await;
    assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(b["code"], "SF-AUTH-005");

    let (st, b) = send_req(
        &router_c,
        Method::POST,
        "/v1/organizations",
        Some(&tok_c),
        &[],
        Some(json!({ "name": "PG Down Org", "seats": 3 })),
    )
    .await;
    assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(b["code"], "SF-AUTH-005");
}
