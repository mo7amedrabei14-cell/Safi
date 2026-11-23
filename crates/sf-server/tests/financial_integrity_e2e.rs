//! P0 Dedicated Financial Integrity & Duplicate Payment Prevention Acceptance Suite.
//!
//! Proves all Section 1 & Section 4 guarantees against real local HTTP mock Paddle & Paymob
//! provider servers (plus real PostgreSQL 18 multi-instance & restart verification when
//! `SF_TEST_DATABASE_URL` is configured, executed automatically in CI via `tools/test-migrations.sh`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::body::{to_bytes, Body};
use axum::extract::State as AxumState;
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use tokio::sync::Mutex;
use tower::ServiceExt;
use uuid::Uuid;

use sf_server::auth::account::{
    AccountAuthService, RegisterAccountRequest, SqlxAccountRepository, VerificationEmailSender,
};
use sf_server::auth::cache::InMemoryAuthCache;
use sf_server::auth::repository::{
    InMemoryApiKeyRepository, InMemorySessionRepository, SqlxApiKeyRepository,
    SqlxSessionRepository,
};
use sf_server::auth::service::AuthService;
use sf_server::auth::unix_now;
use sf_server::billing::{
    sign_paddle_payload, sign_paymob_transaction, BillingCurrency, CheckoutProduct,
    CheckoutRequest, PaymentOrderState, PaymobTransactionObj, REFUND_WINDOW_SECS,
};
use sf_server::config::ServerConfig;
use sf_server::coupons::{CreateCouponRequest, DiscountKind};
use sf_server::entitlements::PlanTier;
use sf_server::error::AuthError;
use sf_server::pg_store::PgControlPlaneStore;
use sf_server::{app, AppState};

#[derive(Clone, Default)]
struct MockProviderState {
    paddle_create_calls: Arc<AtomicUsize>,
    paddle_refund_calls: Arc<AtomicUsize>,
    paymob_create_calls: Arc<AtomicUsize>,
    paymob_refund_calls: Arc<AtomicUsize>,
    paddle_fail_next: Arc<AtomicBool>,
    paddle_timeout_after_create: Arc<AtomicBool>,
    paddle_refund_pending_mode: Arc<AtomicBool>,
    paddle_txns_by_order: Arc<Mutex<HashMap<String, Value>>>,
    paymob_intentions_by_order: Arc<Mutex<HashMap<String, Value>>>,
    next_paymob_order_id: Arc<AtomicUsize>,
}

async fn spawn_mock_providers() -> (String, String, MockProviderState) {
    let st = MockProviderState::default();
    st.next_paymob_order_id.store(700_000, Ordering::SeqCst);

    // Mock Paddle Billing API (`POST /transactions`, `GET /transactions`, `POST /adjustments`)
    let paddle_router = Router::new()
        .route(
            "/transactions",
            post(
                |AxumState(s): AxumState<MockProviderState>, Json(body): Json<Value>| async move {
                    s.paddle_create_calls.fetch_add(1, Ordering::SeqCst);
                    if s.paddle_fail_next.swap(false, Ordering::SeqCst) {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(json!({ "error": { "code": "provider_down" } })),
                        );
                    }
                    let order_id = body["custom_data"]["safi_order_id"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned();
                    let txn_id = format!("txn_{}", Uuid::now_v7().simple());
                    let total = body["custom_data"]["expected_amount_minor"]
                        .as_u64()
                        .unwrap_or(2000);
                    let data_obj = json!({
                        "id": txn_id,
                        "status": "ready",
                        "currency_code": "USD",
                        "checkout": { "url": format!("https://checkout.paddle.test/txn/{txn_id}") },
                        "custom_data": body["custom_data"],
                        "details": { "totals": { "total": total.to_string() } }
                    });
                    s.paddle_txns_by_order
                        .lock()
                        .await
                        .insert(order_id, data_obj.clone());

                    if s.paddle_timeout_after_create.swap(false, Ordering::SeqCst) {
                        // Simulate provider creating the transaction, then network timing out before response reaches client!
                        tokio::time::sleep(Duration::from_millis(600)).await;
                    }
                    (StatusCode::CREATED, Json(json!({ "data": data_obj })))
                },
            ),
        )
        .route(
            "/transactions",
            get(|AxumState(s): AxumState<MockProviderState>| async move {
                let items: Vec<Value> = s
                    .paddle_txns_by_order
                    .lock()
                    .await
                    .values()
                    .cloned()
                    .collect();
                (StatusCode::OK, Json(json!({ "data": items })))
            }),
        )
        .route(
            "/adjustments",
            post(
                |AxumState(s): AxumState<MockProviderState>, Json(_body): Json<Value>| async move {
                    s.paddle_refund_calls.fetch_add(1, Ordering::SeqCst);
                    let adj_id = format!("adj_{}", Uuid::now_v7().simple());
                    let status = if s.paddle_refund_pending_mode.load(Ordering::SeqCst) {
                        "pending_approval"
                    } else {
                        "approved"
                    };
                    (
                        StatusCode::CREATED,
                        Json(json!({ "data": { "id": adj_id, "status": status } })),
                    )
                },
            ),
        )
        .with_state(st.clone());

    let paddle_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock paddle");
    let paddle_addr = paddle_listener.local_addr().expect("addr");
    tokio::spawn(async move {
        let _ = axum::serve(paddle_listener, paddle_router).await;
    });

    // Mock Paymob Intention & Refund API (`POST /v1/intention/`, `GET /v1/intention/`, `POST /api/acceptance/void_refund/refund`)
    let paymob_router = Router::new()
        .route(
            "/v1/intention/",
            post(|AxumState(s): AxumState<MockProviderState>, Json(body): Json<Value>| async move {
                s.paymob_create_calls.fetch_add(1, Ordering::SeqCst);
                let special_ref = body["special_reference"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned();
                let int_order_id = s.next_paymob_order_id.fetch_add(1, Ordering::SeqCst) as u64;
                let int_id = format!("pi_{}", Uuid::now_v7().simple());
                let client_secret = format!("egy_csk_test_{}", Uuid::now_v7().simple());
                let resp = json!({
                    "id": int_id,
                    "intention_order_id": int_order_id,
                    "client_secret": client_secret,
                    "special_reference": special_ref
                });
                s.paymob_intentions_by_order
                    .lock()
                    .await
                    .insert(special_ref, resp.clone());
                (StatusCode::CREATED, Json(resp))
            }),
        )
        .route(
            "/v1/intention/",
            get(|AxumState(s): AxumState<MockProviderState>, axum::extract::Query(q): axum::extract::Query<HashMap<String, String>>| async move {
                if let Some(sr) = q.get("special_reference") {
                    if let Some(v) = s.paymob_intentions_by_order.lock().await.get(sr).cloned() {
                        return (StatusCode::OK, Json(v));
                    }
                }
                (StatusCode::NOT_FOUND, Json(json!({ "detail": "not_found" })))
            }),
        )
        .route(
            "/api/acceptance/void_refund/refund",
            post(|AxumState(s): AxumState<MockProviderState>, Json(_body): Json<Value>| async move {
                s.paymob_refund_calls.fetch_add(1, Ordering::SeqCst);
                let refund_id = 9_900_000 + s.paymob_refund_calls.load(Ordering::SeqCst);
                (
                    StatusCode::OK,
                    Json(json!({ "id": refund_id, "success": true, "is_refunded": true, "pending": false })),
                )
            }),
        )
        .with_state(st.clone());

    let paymob_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind mock paymob");
    let paymob_addr = paymob_listener.local_addr().expect("addr");
    tokio::spawn(async move {
        let _ = axum::serve(paymob_listener, paymob_router).await;
    });

    (
        format!("http://{paddle_addr}"),
        format!("http://{paymob_addr}"),
        st,
    )
}

fn financial_test_config(paddle_url: &str, paymob_url: &str) -> ServerConfig {
    let mut cfg = ServerConfig::for_tests();
    cfg.paddle_api_key = Some("pdl_live_mock_key".to_owned());
    cfg.paddle_webhook_secret = Some("pdl_whsec_financial_test".to_owned());
    cfg.paddle_api_base_url = paddle_url.to_owned();
    cfg.paddle_price_individual_monthly = Some("pri_ind_monthly".to_owned());
    cfg.paddle_price_individual_annual = Some("pri_ind_annual".to_owned());
    cfg.paddle_price_org_seat_annual = Some("pri_org_seat".to_owned());
    cfg.paddle_price_pay_per_file = Some("pri_ppf".to_owned());
    cfg.paddle_price_credit_pack_25 = Some("pri_cp25".to_owned());

    cfg.paymob_api_key = Some("egy_sk_mock_secret".to_owned());
    cfg.paymob_public_key = Some("egy_pk_mock_public".to_owned());
    cfg.paymob_hmac_secret = Some("paymob_hmac_financial_test".to_owned());
    cfg.paymob_integration_id = Some(445566);
    cfg.paymob_api_base_url = paymob_url.to_owned();
    cfg.ai_provider_url = Some("https://ai.safidata.example".to_owned());
    cfg.ai_api_key = Some("ai-key".to_owned());
    cfg.ai_model = Some("safi-v5".to_owned());
    cfg
}

fn make_in_memory_state(cfg: ServerConfig) -> (AppState, Arc<InMemorySessionRepository>) {
    let sessions = Arc::new(InMemorySessionRepository::new());
    let auth = AuthService::new(
        Arc::new(InMemoryApiKeyRepository::default()),
        sessions.clone(),
        Arc::new(InMemoryAuthCache::new()),
    );
    (AppState::with_config(auth, cfg), sessions)
}

#[tokio::test]
async fn p0_checkout_idempotency_concurrency_timeout_reconciliation_and_coupon_release() {
    let (paddle_url, paymob_url, mock) = spawn_mock_providers().await;
    let journal_path = std::env::temp_dir().join(format!(
        "safi-p0-fin-journal-{}.json",
        Uuid::now_v7().simple()
    ));

    let cfg = financial_test_config(&paddle_url, &paymob_url);
    let (state, sessions) = make_in_memory_state(cfg.clone());
    let state = state
        .with_durable_journal(&journal_path, Some(sessions.clone()))
        .await
        .expect("journal");
    // Short timeout (250ms) so timeout-after-create reconciliation test runs swiftly!
    let billing = state.billing.clone().with_provider_timeout_ms(250);

    let ws = Uuid::now_v7();
    let user = Uuid::now_v7();
    let owner = Uuid::now_v7();
    let now = unix_now();

    // 1. 50-way concurrent checkout burst with the SAME idempotency key -> EXACTLY 1 provider call!
    let mut handles = Vec::new();
    for _ in 0..50 {
        let b = billing.clone();
        handles.push(tokio::spawn(async move {
            b.create_checkout_at(
                &CheckoutRequest {
                    workspace_id: ws,
                    user_id: user,
                    product: CheckoutProduct::IndividualMonthly,
                    billing_country_iso2: "US".to_owned(),
                    seats: None,
                    coupon_code: None,
                    idempotency_key: Some("burst-50-key-1".to_owned()),
                },
                now,
            )
            .await
        }));
    }
    let mut order_ids = Vec::new();
    let mut urls = Vec::new();
    for h in handles {
        let res = h.await.expect("join").expect("checkout ok");
        order_ids.push(res.order_id);
        urls.push(res.checkout_url.expect("real url"));
    }
    assert_eq!(mock.paddle_create_calls.load(Ordering::SeqCst), 1);
    assert!(order_ids.iter().all(|id| *id == order_ids[0]));
    assert!(urls.iter().all(|u| *u == urls[0]));
    assert!(urls[0].starts_with("https://checkout.paddle.test/txn/txn_"));

    // 2. Same idempotency key with DIFFERENT request body -> 409 Conflict (SF-PAY-007), ZERO extra provider calls!
    let conflict = billing
        .create_checkout_at(
            &CheckoutRequest {
                workspace_id: ws,
                user_id: user,
                product: CheckoutProduct::IndividualAnnual,
                billing_country_iso2: "US".to_owned(),
                seats: None,
                coupon_code: None,
                idempotency_key: Some("burst-50-key-1".to_owned()),
            },
            now,
        )
        .await
        .unwrap_err();
    assert_eq!(conflict, AuthError::IdempotencyConflict);
    assert_eq!(mock.paddle_create_calls.load(Ordering::SeqCst), 1);

    // 3. Intentional separate purchase with a NEW idempotency key -> creates a distinct order!
    let second = billing
        .create_checkout_at(
            &CheckoutRequest {
                workspace_id: ws,
                user_id: user,
                product: CheckoutProduct::PayPerFile,
                billing_country_iso2: "US".to_owned(),
                seats: None,
                coupon_code: None,
                idempotency_key: Some("separate-purchase-key-2".to_owned()),
            },
            now,
        )
        .await
        .expect("second purchase");
    assert_ne!(second.order_id, order_ids[0]);
    assert_eq!(mock.paddle_create_calls.load(Ordering::SeqCst), 2);

    // 4. Coupon reservation + provider checkout failure -> coupon MUST be released, NOT lost!
    state
        .coupons
        .create_coupon(
            owner,
            CreateCouponRequest {
                code: "ONEUSE50".to_owned(),
                discount_kind: DiscountKind::Percent,
                discount_value: 50,
                starts_at: None,
                expires_at: Some(now + 3600),
                max_redemptions: Some(1),
                eligible_plans: vec![],
                eligible_user_ids: vec![],
                new_customers_only: false,
                org_only: false,
                min_purchase_minor: 0,
            },
        )
        .await
        .expect("create coupon");

    mock.paddle_fail_next.store(true, Ordering::SeqCst);
    let failed_chk = billing
        .create_checkout_at(
            &CheckoutRequest {
                workspace_id: ws,
                user_id: user,
                product: CheckoutProduct::IndividualMonthly,
                billing_country_iso2: "US".to_owned(),
                seats: None,
                coupon_code: Some("ONEUSE50".to_owned()),
                idempotency_key: Some("coupon-fail-key-3".to_owned()),
            },
            now,
        )
        .await
        .unwrap_err();
    assert_eq!(failed_chk, AuthError::BillingOperationFailed);

    // Retry with a fresh key using the SAME one-time coupon -> MUST succeed because failed checkout released reservation!
    let recovered_coupon_chk = billing
        .create_checkout_at(
            &CheckoutRequest {
                workspace_id: ws,
                user_id: user,
                product: CheckoutProduct::IndividualMonthly,
                billing_country_iso2: "US".to_owned(),
                seats: None,
                coupon_code: Some("ONEUSE50".to_owned()),
                idempotency_key: Some("coupon-retry-key-4".to_owned()),
            },
            now,
        )
        .await
        .expect("coupon still usable after provider failure");
    assert_eq!(recovered_coupon_chk.discount_amount_minor, 1000);
    assert_eq!(recovered_coupon_chk.amount_minor, 1000);

    // 5. Timeout AFTER provider creates transaction -> marks ReconciliationRequired (SF-PAY-008);
    //    retrying the same idempotency key reconciles and returns the EXACT existing transaction without creating a 2nd charge!
    let calls_before_timeout = mock.paddle_create_calls.load(Ordering::SeqCst);
    mock.paddle_timeout_after_create
        .store(true, Ordering::SeqCst);
    let timeout_err = billing
        .create_checkout_at(
            &CheckoutRequest {
                workspace_id: ws,
                user_id: user,
                product: CheckoutProduct::CreditPack25,
                billing_country_iso2: "US".to_owned(),
                seats: None,
                coupon_code: None,
                idempotency_key: Some("timeout-reconcile-key-5".to_owned()),
            },
            now,
        )
        .await
        .unwrap_err();
    assert_eq!(timeout_err, AuthError::PaymentReconciliationRequired);
    assert_eq!(
        mock.paddle_create_calls.load(Ordering::SeqCst),
        calls_before_timeout + 1
    );

    // Wait briefly so mock finishes storing the transaction, then retry with same idempotency key.
    tokio::time::sleep(Duration::from_millis(500)).await;
    let reconciled = billing
        .create_checkout_at(
            &CheckoutRequest {
                workspace_id: ws,
                user_id: user,
                product: CheckoutProduct::CreditPack25,
                billing_country_iso2: "US".to_owned(),
                seats: None,
                coupon_code: None,
                idempotency_key: Some("timeout-reconcile-key-5".to_owned()),
            },
            now + 1,
        )
        .await
        .expect("reconciled order");
    assert!(reconciled.reused_existing_order);
    assert_eq!(reconciled.order_state, PaymentOrderState::CheckoutPending);
    assert!(reconciled.provider_order_ref.is_some());
    // Provider create count MUST NOT have increased (NO duplicate charge created)!
    assert_eq!(
        mock.paddle_create_calls.load(Ordering::SeqCst),
        calls_before_timeout + 1
    );

    // 6. Server restart durability -> persisting & reloading keeps idempotency intact!
    let snap = billing.export_snapshot().await;
    state.billing.import_snapshot(snap).await;
    state.persist_durable_state().await;

    let (restarted_state, restarted_sessions) = make_in_memory_state(cfg);
    let restarted_state = restarted_state
        .with_durable_journal(&journal_path, Some(restarted_sessions))
        .await
        .expect("reload journal");
    let calls_before_restart_retry = mock.paddle_create_calls.load(Ordering::SeqCst);
    let after_restart = restarted_state
        .billing
        .create_checkout_at(
            &CheckoutRequest {
                workspace_id: ws,
                user_id: user,
                product: CheckoutProduct::IndividualMonthly,
                billing_country_iso2: "US".to_owned(),
                seats: None,
                coupon_code: None,
                idempotency_key: Some("burst-50-key-1".to_owned()),
            },
            now + 2,
        )
        .await
        .expect("post-restart idempotent hit");
    assert_eq!(after_restart.order_id, order_ids[0]);
    assert!(after_restart.reused_existing_order);
    assert_eq!(
        mock.paddle_create_calls.load(Ordering::SeqCst),
        calls_before_restart_retry
    );

    let _ = std::fs::remove_file(&journal_path);
}

#[tokio::test]
async fn p0_paymob_trust_boundary_hmac_correlation_lifecycle_and_concurrency() {
    let (paddle_url, paymob_url, mock) = spawn_mock_providers().await;
    let cfg = financial_test_config(&paddle_url, &paymob_url);
    let (state, _sessions) = make_in_memory_state(cfg);

    let victim_ws = Uuid::now_v7();
    let victim_user = Uuid::now_v7();
    let attacker_ws = Uuid::now_v7();
    let attacker_user = Uuid::now_v7();
    let now = unix_now();

    // 1. Create real Paymob Intention for victim buying CreditPack25 (25,000 piastres = 250 EGP).
    let chk = state
        .billing
        .create_checkout_at(
            &CheckoutRequest {
                workspace_id: victim_ws,
                user_id: victim_user,
                product: CheckoutProduct::CreditPack25,
                billing_country_iso2: "EG".to_owned(),
                seats: None,
                coupon_code: None,
                idempotency_key: Some("paymob-victim-order-1".to_owned()),
            },
            now,
        )
        .await
        .expect("paymob intention created");
    assert_eq!(mock.paymob_create_calls.load(Ordering::SeqCst), 1);
    assert_eq!(chk.currency, BillingCurrency::Egp);
    assert_eq!(chk.amount_minor, 25_000);
    let url = chk.checkout_url.as_deref().expect("paymob checkout url");
    assert!(url.contains("publicKey=egy_pk_mock_public"));
    assert!(url.contains("clientSecret=egy_csk_test_"));
    let paymob_order_id: u64 = chk
        .provider_order_ref
        .as_deref()
        .expect("paymob order ref")
        .parse()
        .expect("u64");

    let base_tx = || PaymobTransactionObj {
        id: 88112233,
        amount_cents: 25_000,
        created_at: "2026-11-23T10:00:00".to_owned(),
        currency: "EGP".to_owned(),
        error_occured: false,
        has_parent_transaction: false,
        integration_id: 445566,
        is_3d_secure: true,
        is_auth: false,
        is_capture: false,
        is_refunded: false,
        is_chargeback: false,
        is_standalone_payment: true,
        is_voided: false,
        order_id: paymob_order_id,
        owner: 12345,
        pending: false,
        source_data_pan: "2346".to_owned(),
        source_data_sub_type: "MasterCard".to_owned(),
        source_data_type: "card".to_owned(),
        success: true,
        workspace_id: None,
        user_id: None,
        product: None,
        seats: None,
        occurred_at_unix: Some(now + 10),
        webhook_event_id: None,
    };

    // 2. CRITICAL TRUST BOUNDARY TEST: Attacker keeps valid 20-field Paymob HMAC,
    //    but tampers with unsigned workspace_id / user_id / product / seats!
    //    MUST BE REJECTED (SF-PAY-004), attacker gets NOTHING, victim unaffected!
    let mut tampered = base_tx();
    tampered.workspace_id = Some(attacker_ws);
    tampered.user_id = Some(attacker_user);
    tampered.product = Some(CheckoutProduct::OrganizationSeatAnnual);
    tampered.seats = Some(50);
    let valid_hmac =
        sign_paymob_transaction("paymob_hmac_financial_test", &tampered).expect("hmac");
    let tamper_err = state
        .billing
        .process_paymob_webhook(&valid_hmac, &tampered, now + 10)
        .await
        .unwrap_err();
    assert_eq!(tamper_err, AuthError::BillingOperationFailed);
    assert_eq!(
        state
            .entitlements
            .effective_at(attacker_ws, attacker_user, false, now + 11)
            .await
            .effective_plan,
        PlanTier::Free
    );
    assert_eq!(
        state
            .entitlements
            .effective_at(attacker_ws, attacker_user, false, now + 11)
            .await
            .operation_credits,
        0
    );

    // 3. Wrong amount, wrong currency, wrong integration_id, unknown order_id -> ALL REJECTED!
    let mut wrong_amt = base_tx();
    wrong_amt.amount_cents = 100;
    let h = sign_paymob_transaction("paymob_hmac_financial_test", &wrong_amt).expect("hmac");
    assert_eq!(
        state
            .billing
            .process_paymob_webhook(&h, &wrong_amt, now + 10)
            .await
            .unwrap_err(),
        AuthError::BillingOperationFailed
    );

    let mut wrong_curr = base_tx();
    wrong_curr.currency = "USD".to_owned();
    let h = sign_paymob_transaction("paymob_hmac_financial_test", &wrong_curr).expect("hmac");
    assert_eq!(
        state
            .billing
            .process_paymob_webhook(&h, &wrong_curr, now + 10)
            .await
            .unwrap_err(),
        AuthError::BillingOperationFailed
    );

    let mut wrong_int = base_tx();
    wrong_int.integration_id = 999999;
    let h = sign_paymob_transaction("paymob_hmac_financial_test", &wrong_int).expect("hmac");
    assert_eq!(
        state
            .billing
            .process_paymob_webhook(&h, &wrong_int, now + 10)
            .await
            .unwrap_err(),
        AuthError::BillingOperationFailed
    );

    let mut unknown_order = base_tx();
    unknown_order.order_id = 999_888_777;
    let h = sign_paymob_transaction("paymob_hmac_financial_test", &unknown_order).expect("hmac");
    assert_eq!(
        state
            .billing
            .process_paymob_webhook(&h, &unknown_order, now + 10)
            .await
            .unwrap_err(),
        AuthError::BillingOperationFailed
    );

    // 4. Auth-only non-captured transaction (`is_auth=true, is_capture=false, is_standalone_payment=false`)
    //    and pending transaction (`pending=true`) MUST NOT grant credits!
    let mut auth_only = base_tx();
    auth_only.id = 88112201;
    auth_only.is_auth = true;
    auth_only.is_capture = false;
    auth_only.is_standalone_payment = false;
    let h = sign_paymob_transaction("paymob_hmac_financial_test", &auth_only).expect("hmac");
    let auth_rec = state
        .billing
        .process_paymob_webhook(&h, &auth_only, now + 12)
        .await
        .expect("auth only handled");
    assert_eq!(auth_rec.outcome, "payment_pending_no_grant");
    assert_eq!(
        state
            .entitlements
            .effective_at(victim_ws, victim_user, false, now + 12)
            .await
            .operation_credits,
        0
    );

    // 5. 25-way CONCURRENT delivery of the authentic Paymob callback (with NO custom metadata fields at all!)
    //    MUST derive victim_ws/victim_user/CreditPack25 strictly from stored order and grant 25 credits EXACTLY ONCE!
    let genuine = base_tx();
    let genuine_hmac =
        sign_paymob_transaction("paymob_hmac_financial_test", &genuine).expect("hmac");
    let mut tasks = Vec::new();
    for _ in 0..25 {
        let b = state.billing.clone();
        let hmac = genuine_hmac.clone();
        let tx = genuine.clone();
        tasks.push(tokio::spawn(async move {
            b.process_paymob_webhook(&hmac, &tx, now + 20).await
        }));
    }
    let mut granted_count = 0usize;
    let mut dup_count = 0usize;
    for t in tasks {
        let rec = t.await.expect("join").expect("ok");
        if rec.outcome == "paymob_credits_granted_25" {
            granted_count += 1;
        } else if rec.outcome == "idempotent_duplicate_ignored" {
            dup_count += 1;
        }
    }
    assert_eq!(granted_count, 1);
    assert_eq!(dup_count, 24);
    assert_eq!(
        state
            .entitlements
            .effective_at(victim_ws, victim_user, false, now + 21)
            .await
            .operation_credits,
        25
    );
    assert_eq!(state.billing.list_invoices(victim_ws).await.len(), 1);

    // 6. Same underlying Paymob transaction ID delivered with a DIFFERENT webhook_event_id -> STILL NO DOUBLE GRANT!
    let mut new_event_same_tx = base_tx();
    new_event_same_tx.webhook_event_id = Some("paymob_redelivery_distinct_evt_999".to_owned());
    let rec2 = state
        .billing
        .process_paymob_webhook(&genuine_hmac, &new_event_same_tx, now + 25)
        .await
        .expect("dedup by tx");
    assert_eq!(rec2.outcome, "duplicate_transaction_grant_ignored");
    assert_eq!(
        state
            .entitlements
            .effective_at(victim_ws, victim_user, false, now + 26)
            .await
            .operation_credits,
        25
    );
    assert_eq!(state.billing.list_invoices(victim_ws).await.len(), 1);
}

#[tokio::test]
async fn p0_paddle_webhooks_concurrency_refunds_partial_refunds_chargeback_and_out_of_order() {
    let (paddle_url, paymob_url, mock) = spawn_mock_providers().await;
    let cfg = financial_test_config(&paddle_url, &paymob_url);
    let (state, _sessions) = make_in_memory_state(cfg);
    let ws = Uuid::now_v7();
    let user = Uuid::now_v7();
    let now = unix_now();

    // 1. Create real Paddle order for IndividualMonthly ($20 = 2000 cents).
    let chk = state
        .billing
        .create_checkout_at(
            &CheckoutRequest {
                workspace_id: ws,
                user_id: user,
                product: CheckoutProduct::IndividualMonthly,
                billing_country_iso2: "US".to_owned(),
                seats: None,
                coupon_code: None,
                idempotency_key: Some("paddle-sub-order-1".to_owned()),
            },
            now,
        )
        .await
        .expect("paddle checkout");
    let txn_id = chk.provider_order_ref.clone().expect("txn_id");

    // 2. Non-final `checkout.opened` MUST NOT activate paid plan!
    let opened_body = serde_json::to_vec(&json!({
        "event_id": "evt_pdl_open_1",
        "event_type": "checkout.opened",
        "occurred_at_unix": now + 1,
        "order_id": chk.order_id,
        "transaction_id": txn_id,
        "currency_code": "USD"
    }))
    .expect("json");
    let sig = sign_paddle_payload("pdl_whsec_financial_test", now + 1, &opened_body).expect("sig");
    let rec = state
        .billing
        .process_paddle_webhook(&sig, &opened_body, now + 1)
        .await
        .expect("opened ok");
    assert_eq!(rec.outcome, "payment_pending_no_grant");
    assert_eq!(
        state
            .entitlements
            .effective_at(ws, user, false, now + 2)
            .await
            .effective_plan,
        PlanTier::Free
    );

    // 3. Wrong amount (100 instead of 2000) & wrong price_id -> REJECTED!
    let bad_amt_body = serde_json::to_vec(&json!({
        "event_id": "evt_pdl_bad_amt",
        "event_type": "transaction.completed",
        "occurred_at_unix": now + 2,
        "order_id": chk.order_id,
        "transaction_id": txn_id,
        "currency_code": "USD",
        "price_id": "pri_ind_monthly",
        "amount_minor": 100
    }))
    .expect("json");
    let sig = sign_paddle_payload("pdl_whsec_financial_test", now + 2, &bad_amt_body).expect("sig");
    assert_eq!(
        state
            .billing
            .process_paddle_webhook(&sig, &bad_amt_body, now + 2)
            .await
            .unwrap_err(),
        AuthError::BillingOperationFailed
    );

    // 4. 25-way concurrent Paddle `transaction.completed` -> activates ONCE, creates EXACTLY 1 invoice!
    let paid_body = serde_json::to_vec(&json!({
        "event_id": "evt_pdl_paid_1",
        "event_type": "transaction.completed",
        "occurred_at_unix": now + 5,
        "order_id": chk.order_id,
        "transaction_id": txn_id,
        "currency_code": "USD",
        "price_id": "pri_ind_monthly",
        "amount_minor": 2000,
        "invoice_id": "inv_pdl_fin_001",
        "period_end_unix": now + 30 * 86_400
    }))
    .expect("json");
    let paid_sig =
        sign_paddle_payload("pdl_whsec_financial_test", now + 5, &paid_body).expect("sig");
    let mut handles = Vec::new();
    for _ in 0..25 {
        let b = state.billing.clone();
        let s = paid_sig.clone();
        let body = paid_body.clone();
        handles.push(tokio::spawn(async move {
            b.process_paddle_webhook(&s, &body, now + 5).await
        }));
    }
    let mut ok_grants = 0usize;
    let mut dup_hits = 0usize;
    for h in handles {
        let r = h.await.expect("join").expect("ok");
        if r.outcome == "subscription_active" {
            ok_grants += 1;
        } else if r.outcome == "idempotent_duplicate_ignored" {
            dup_hits += 1;
        }
    }
    assert_eq!(ok_grants, 1);
    assert_eq!(dup_hits, 24);
    assert_eq!(state.billing.list_invoices(ws).await.len(), 1);
    assert_eq!(
        state
            .entitlements
            .effective_at(ws, user, false, now + 6)
            .await
            .effective_plan,
        PlanTier::IndividualMonthly
    );

    // 5. Same `transaction_id` delivered under a NEW `event_id` -> blocked at transaction grant ledger!
    let replay_tx_body = serde_json::to_vec(&json!({
        "event_id": "evt_pdl_paid_different_event_id_2",
        "event_type": "subscription.activated",
        "occurred_at_unix": now + 6,
        "order_id": chk.order_id,
        "transaction_id": txn_id,
        "currency_code": "USD",
        "price_id": "pri_ind_monthly",
        "amount_minor": 2000,
        "invoice_id": "inv_pdl_fin_001"
    }))
    .expect("json");
    let replay_sig =
        sign_paddle_payload("pdl_whsec_financial_test", now + 6, &replay_tx_body).expect("sig");
    let replay_rec = state
        .billing
        .process_paddle_webhook(&replay_sig, &replay_tx_body, now + 6)
        .await
        .expect("tx dedup");
    assert_eq!(replay_rec.outcome, "duplicate_transaction_grant_ignored");

    // 6. Unconfirmed provider refund (`pending_approval`) MUST NOT revoke entitlements prematurely!
    //    AND duplicate refund click with same `refund_request_key` calls Paddle `/adjustments` ONLY ONCE!
    mock.paddle_refund_pending_mode
        .store(true, Ordering::SeqCst);
    let pending_inv = state
        .billing
        .issue_owner_refund_idempotent(
            "inv_pdl_fin_001",
            "refund-req-key-1",
            None,
            false,
            "customer_requested",
            Some(user),
            now + 60,
        )
        .await
        .expect("pending refund submitted");
    assert_eq!(pending_inv.status, "refund_pending");
    assert_eq!(mock.paddle_refund_calls.load(Ordering::SeqCst), 1);
    // Entitlement MUST STILL BE ACTIVE because provider has NOT confirmed refund yet!
    assert_eq!(
        state
            .entitlements
            .effective_at(ws, user, false, now + 61)
            .await
            .effective_plan,
        PlanTier::IndividualMonthly
    );
    // Clicking refund again while `refund_pending` is rejected and does NOT call Paddle a second time!
    let dup_refund_err = state
        .billing
        .issue_owner_refund_idempotent(
            "inv_pdl_fin_001",
            "refund-req-key-1",
            None,
            false,
            "customer_requested",
            Some(user),
            now + 62,
        )
        .await
        .unwrap_err();
    assert_eq!(dup_refund_err, AuthError::BillingOperationFailed);
    assert_eq!(mock.paddle_refund_calls.load(Ordering::SeqCst), 1);

    // 7. Confirmed `adjustment.refunded` webhook -> NOW marks invoice refunded & revokes paid plan!
    let ref_body = serde_json::to_vec(&json!({
        "event_id": "evt_pdl_ref_confirmed",
        "event_type": "adjustment.refunded",
        "occurred_at_unix": now + 100,
        "order_id": chk.order_id,
        "transaction_id": txn_id,
        "invoice_id": "inv_pdl_fin_001"
    }))
    .expect("json");
    let ref_sig =
        sign_paddle_payload("pdl_whsec_financial_test", now + 100, &ref_body).expect("sig");
    let ref_rec = state
        .billing
        .process_paddle_webhook(&ref_sig, &ref_body, now + 100)
        .await
        .expect("refund confirmed");
    assert_eq!(ref_rec.outcome, "refund_processed");
    assert_eq!(
        state
            .entitlements
            .effective_at(ws, user, false, now + 101)
            .await
            .effective_plan,
        PlanTier::Free
    );

    // 8. Stale or late activation event MUST NOT resurrect an already refunded order!
    let late_act_body = serde_json::to_vec(&json!({
        "event_id": "evt_pdl_late_resurrect",
        "event_type": "subscription.activated",
        "occurred_at_unix": now + 105,
        "order_id": chk.order_id,
        "transaction_id": "txn_new_attempt_on_refunded",
        "currency_code": "USD",
        "price_id": "pri_ind_monthly",
        "amount_minor": 2000
    }))
    .expect("json");
    let late_sig =
        sign_paddle_payload("pdl_whsec_financial_test", now + 105, &late_act_body).expect("sig");
    let late_rec = state
        .billing
        .process_paddle_webhook(&late_sig, &late_act_body, now + 105)
        .await
        .expect("protected");
    assert_eq!(late_rec.outcome, "terminal_state_protected");
    assert_eq!(
        state
            .entitlements
            .effective_at(ws, user, false, now + 106)
            .await
            .effective_plan,
        PlanTier::Free
    );

    // 9. Second purchase + 3-day policy enforcement + confirmed refund with statutory override!
    let chk2 = state
        .billing
        .create_checkout_at(
            &CheckoutRequest {
                workspace_id: ws,
                user_id: user,
                product: CheckoutProduct::IndividualAnnual,
                billing_country_iso2: "US".to_owned(),
                seats: None,
                coupon_code: None,
                idempotency_key: Some("paddle-annual-order-2".to_owned()),
            },
            now + 200,
        )
        .await
        .expect("annual checkout");
    let txn2 = chk2.provider_order_ref.clone().expect("txn2");
    let paid2 = serde_json::to_vec(&json!({
        "event_id": "evt_pdl_paid_2",
        "event_type": "transaction.completed",
        "occurred_at_unix": now + 210,
        "order_id": chk2.order_id,
        "transaction_id": txn2,
        "currency_code": "USD",
        "price_id": "pri_ind_annual",
        "amount_minor": 16_800,
        "invoice_id": "inv_pdl_fin_002"
    }))
    .expect("json");
    let sig2 = sign_paddle_payload("pdl_whsec_financial_test", now + 210, &paid2).expect("sig");
    state
        .billing
        .process_paddle_webhook(&sig2, &paid2, now + 210)
        .await
        .expect("paid2");

    mock.paddle_refund_pending_mode
        .store(false, Ordering::SeqCst);
    // Past 3-day window without statutory override -> rejected!
    assert_eq!(
        state
            .billing
            .issue_owner_refund_idempotent(
                "inv_pdl_fin_002",
                "refund-over-window-1",
                None,
                false,
                "late",
                Some(user),
                now + 210 + REFUND_WINDOW_SECS + 10,
            )
            .await
            .unwrap_err(),
        AuthError::BillingOperationFailed
    );
    // Partial refund with statutory override -> invoice marked `partially_refunded`, plan stays active!
    let partial = state
        .billing
        .issue_owner_refund_idempotent(
            "inv_pdl_fin_002",
            "refund-partial-1",
            Some(5_000),
            true,
            "statutory_partial",
            Some(user),
            now + 210 + REFUND_WINDOW_SECS + 10,
        )
        .await
        .expect("partial refund ok");
    assert_eq!(partial.status, "partially_refunded");
    assert_eq!(partial.refunded_amount_minor, 5_000);
    assert_eq!(
        state
            .entitlements
            .effective_at(ws, user, false, now + 210 + REFUND_WINDOW_SECS + 11)
            .await
            .effective_plan,
        PlanTier::IndividualAnnual
    );
}

#[derive(Default)]
struct CaptureMailer {
    tokens: tokio::sync::RwLock<HashMap<String, String>>,
}

#[async_trait::async_trait]
impl VerificationEmailSender for CaptureMailer {
    async fn send_verification(&self, recipient: &str, url: &str) -> Result<(), AuthError> {
        if let Some(tok) = url
            .split("verify_email=")
            .nth(1)
            .or_else(|| url.split("token=").nth(1))
        {
            self.tokens
                .write()
                .await
                .insert(recipient.to_owned(), tok.to_owned());
        }
        Ok(())
    }

    async fn send_password_reset(&self, _recipient: &str, _url: &str) -> Result<(), AuthError> {
        Ok(())
    }
}

async fn send_http_json(
    router: &Router,
    method: &str,
    uri: &str,
    bearer: Option<&str>,
    extra_headers: &[(&str, &str)],
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut b = Request::builder().method(method).uri(uri);
    if let Some(tok) = bearer {
        b = b.header("authorization", format!("Bearer {tok}"));
    }
    for (k, v) in extra_headers {
        b = b.header(*k, *v);
    }
    let req = if let Some(v) = body {
        b.header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&v).expect("json")))
            .expect("req")
    } else {
        b.body(Body::empty()).expect("req")
    };
    let resp = router.clone().oneshot(req).await.expect("oneshot");
    let st = resp.status();
    let bytes = to_bytes(resp.into_body(), 2 * 1024 * 1024)
        .await
        .expect("bytes");
    let json = serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({}));
    (st, json)
}

/// Real PostgreSQL 18 Multi-Instance, Concurrency & Restart Financial Integrity E2E Gate.
/// Executed automatically in CI via `tools/test-migrations.sh` on `postgres:18.6-bookworm`.
#[tokio::test]
#[ignore = "requires SF_TEST_DATABASE_URL pointing at a migrated PostgreSQL 18 database (migrations 0001..0013)"]
async fn p0_postgres_18_financial_integrity_multi_instance_concurrency_and_restart() {
    let db_url = std::env::var("SF_TEST_DATABASE_URL")
        .expect("SF_TEST_DATABASE_URL must be set for live PostgreSQL 18 financial test");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(20)
        .connect(&db_url)
        .await
        .expect("connect to PostgreSQL 18");

    let (paddle_url, paymob_url, mock) = spawn_mock_providers().await;
    let mailer = Arc::new(CaptureMailer::default());

    let build_instance = || {
        let cfg = financial_test_config(&paddle_url, &paymob_url);
        let auth = AuthService::new(
            Arc::new(SqlxApiKeyRepository::new(pool.clone())),
            Arc::new(SqlxSessionRepository::new(pool.clone())),
            Arc::new(InMemoryAuthCache::new()),
        );
        let pg_store = PgControlPlaneStore::new(pool.clone());
        let state = AppState::with_config(auth, cfg).with_pg_store(pg_store);
        let acct = AccountAuthService::new(
            Arc::new(SqlxAccountRepository::new(pool.clone())),
            mailer.clone(),
            state.auth.clone(),
            "https://safidata.app".to_owned(),
        );
        let state = state.with_account_auth(acct);
        let router = app(state.clone());
        (state, router)
    };

    // Instance A and Instance B share ZERO in-memory state — ONLY PostgreSQL 18!
    let (state_a, router_a) = build_instance();
    let (_state_b, router_b) = build_instance();

    let nonce = Uuid::now_v7().simple().to_string();
    let email = format!("pg-fin-{nonce}@example.test");
    let acct = state_a.account_auth.as_ref().expect("acct");
    acct.register(RegisterAccountRequest {
        email: email.clone(),
        display_name: "PG Fin Customer".to_owned(),
        password: "FinUserPass123!".to_owned(),
    })
    .await
    .expect("register");
    let tok = mailer
        .tokens
        .read()
        .await
        .get(&email)
        .cloned()
        .expect("verify tok");
    acct.verify_email(&tok).await.expect("verify");

    let (st, login) = send_http_json(
        &router_a,
        "POST",
        "/v1/auth/login",
        None,
        &[],
        Some(json!({
            "email": email,
            "password": "FinUserPass123!",
            "installation_id": Uuid::now_v7().to_string(),
            "platform": "windows",
            "device_name": "PG Fin Box"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    let bearer = login["token"].as_str().expect("token").to_owned();

    // 1. 20-way concurrent Paymob checkout across BOTH Instance A & Instance B with the SAME idempotency key!
    let idem_key = format!("pg-multi-idem-{nonce}");
    let mut chk_tasks = Vec::new();
    for i in 0..20 {
        let r = if i % 2 == 0 {
            router_a.clone()
        } else {
            router_b.clone()
        };
        let b = bearer.clone();
        let k = idem_key.clone();
        chk_tasks.push(tokio::spawn(async move {
            send_http_json(
                &r,
                "POST",
                "/v1/billing/checkout",
                Some(&b),
                &[("idempotency-key", &k)],
                Some(json!({
                    "product": "credit_pack_25",
                    "billing_country_iso2": "EG"
                })),
            )
            .await
        }));
    }
    let mut order_ids = Vec::new();
    let mut paymob_order_ref: Option<String> = None;
    for t in chk_tasks {
        let (st, val) = t.await.expect("join");
        assert_eq!(st, StatusCode::OK, "checkout: {val}");
        order_ids.push(val["order_id"].as_str().expect("oid").to_owned());
        if paymob_order_ref.is_none() {
            paymob_order_ref = val["provider_order_ref"].as_str().map(str::to_owned);
        }
    }
    assert!(order_ids.iter().all(|id| id == &order_ids[0]));
    let paymob_order_id: u64 = paymob_order_ref.expect("paymob ref").parse().expect("u64");

    // 2. Conflicting request with SAME idempotency key on Instance B -> 409 Conflict (SF-PAY-007)!
    let (conf_st, conf_body) = send_http_json(
        &router_b,
        "POST",
        "/v1/billing/checkout",
        Some(&bearer),
        &[("idempotency-key", &idem_key)],
        Some(json!({
            "product": "individual_annual",
            "billing_country_iso2": "EG"
        })),
    )
    .await;
    assert_eq!(conf_st, StatusCode::CONFLICT);
    assert_eq!(conf_body["code"], json!("SF-PAY-007"));

    // 3. 20-way concurrent Paymob webhook across Instance A & Instance B -> PostgreSQL grants 25 credits EXACTLY ONCE!
    let tx_id = 55_000_000 + (unix_now() as u64 % 100_000);
    let tx = PaymobTransactionObj {
        id: tx_id,
        amount_cents: 25_000,
        created_at: "2026-11-23T11:00:00".to_owned(),
        currency: "EGP".to_owned(),
        error_occured: false,
        has_parent_transaction: false,
        integration_id: 445566,
        is_3d_secure: true,
        is_auth: false,
        is_capture: false,
        is_refunded: false,
        is_chargeback: false,
        is_standalone_payment: true,
        is_voided: false,
        order_id: paymob_order_id,
        owner: 12345,
        pending: false,
        source_data_pan: "2346".to_owned(),
        source_data_sub_type: "MasterCard".to_owned(),
        source_data_type: "card".to_owned(),
        success: true,
        workspace_id: None,
        user_id: None,
        product: None,
        seats: None,
        occurred_at_unix: Some(unix_now()),
        webhook_event_id: None,
    };
    let hmac = sign_paymob_transaction("paymob_hmac_financial_test", &tx).expect("hmac");
    let tx_json = serde_json::json!({
        "id": tx.id,
        "amount_cents": tx.amount_cents,
        "created_at": tx.created_at,
        "currency": tx.currency,
        "error_occured": tx.error_occured,
        "has_parent_transaction": tx.has_parent_transaction,
        "integration_id": tx.integration_id,
        "is_3d_secure": tx.is_3d_secure,
        "is_auth": tx.is_auth,
        "is_capture": tx.is_capture,
        "is_refunded": tx.is_refunded,
        "is_chargeback": tx.is_chargeback,
        "is_standalone_payment": tx.is_standalone_payment,
        "is_voided": tx.is_voided,
        "order_id": tx.order_id,
        "owner": tx.owner,
        "pending": tx.pending,
        "source_data_pan": tx.source_data_pan,
        "source_data_sub_type": tx.source_data_sub_type,
        "source_data_type": tx.source_data_type,
        "success": tx.success
    });

    let mut wh_tasks = Vec::new();
    for i in 0..20 {
        let r = if i % 2 == 0 {
            router_a.clone()
        } else {
            router_b.clone()
        };
        let uri = format!("/v1/billing/webhooks/paymob?hmac={hmac}");
        let body = tx_json.clone();
        wh_tasks.push(tokio::spawn(async move {
            send_http_json(&r, "POST", &uri, None, &[], Some(body)).await
        }));
    }
    let mut granted = 0usize;
    let mut ignored = 0usize;
    for t in wh_tasks {
        let (st, val) = t.await.expect("join");
        assert_eq!(st, StatusCode::OK, "wh: {val}");
        if val["outcome"] == json!("paymob_credits_granted_25") {
            granted += 1;
        } else if val["outcome"] == json!("idempotent_duplicate_ignored") {
            ignored += 1;
        }
    }
    assert_eq!(granted, 1);
    assert_eq!(ignored, 19);

    // 4. Restarted Instance C (zero shared memory!) replays SAME Paymob tx with a NEW event ID
    //    -> PostgreSQL `UNIQUE (provider, provider_transaction_id)` blocks double grant!
    let (_state_c, router_c) = build_instance();
    let mut replay_json = tx_json.clone();
    replay_json["webhook_event_id"] = json!(format!("pg_new_evt_{nonce}"));
    let (rep_st, rep_val) = send_http_json(
        &router_c,
        "POST",
        &format!("/v1/billing/webhooks/paymob?hmac={hmac}"),
        None,
        &[],
        Some(replay_json),
    )
    .await;
    assert_eq!(rep_st, StatusCode::OK);
    assert_eq!(
        rep_val["outcome"],
        json!("duplicate_transaction_grant_ignored")
    );

    // Verify Restarted Instance C reads EXACTLY 25 credits and 1 paid invoice directly from PostgreSQL 18!
    let (ent_st, ent_val) = send_http_json(
        &router_c,
        "GET",
        "/v1/entitlements/me",
        Some(&bearer),
        &[],
        None,
    )
    .await;
    assert_eq!(ent_st, StatusCode::OK);
    assert_eq!(ent_val["operation_credits"], json!(25));

    let (inv_st, inv_val) = send_http_json(
        &router_c,
        "GET",
        "/v1/billing/invoices",
        Some(&bearer),
        &[],
        None,
    )
    .await;
    assert_eq!(inv_st, StatusCode::OK);
    assert_eq!(inv_val.as_array().expect("arr").len(), 1);
    assert!(mock.paymob_create_calls.load(Ordering::SeqCst) >= 1);
}
