//! P0 Dedicated Financial Integrity, Official Provider Protocol & PostgreSQL 18 Acceptance Suite.
//!
//! Proves all Financial Integrity guarantees against:
//! 1. Real local HTTP mock Paddle & Paymob servers modeling official ready/draft checkout & refunds.
//! 2. Official Paymob (`{"type":"TRANSACTION","obj":{...}}`) & Paddle (`{"event_id","event_type","occurred_at","data"}`)
//!    JSON fixture files matching documented provider contracts.
//! 3. Real PostgreSQL 18 multi-instance concurrency, cumulative partial refunds, recurring monthly &
//!    annual subscription renewals, payload-hash security anomalies (`SF-PAY-009`), and cold server restarts.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::body::{to_bytes, Body};
use axum::extract::State as AxumState;
use axum::http::{Request, StatusCode};
use axum::routing::post;
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
use sf_server::billing::provider_http::{CheckoutMode, CustomerBillingData, PaddleCustomerContext};
use sf_server::billing::{
    sign_paddle_payload, sign_paymob_transaction, CheckoutProduct, CheckoutRequest,
    PaymobCallbackEnvelope, REFUND_WINDOW_SECS,
};
use sf_server::config::ServerConfig;
use sf_server::coupons::{CreateCouponRequest, DiscountKind};
use sf_server::entitlements::PlanTier;
use sf_server::error::AuthError;
use sf_server::pg_store::PgControlPlaneStore;
use sf_server::{app, AppState};

const PAYMOB_CAPTURED_FIXTURE: &str =
    include_str!("fixtures/paymob/transaction_processed_captured.json");
const PAYMOB_PARTIAL_REFUND_FIXTURE: &str =
    include_str!("fixtures/paymob/transaction_processed_partial_refund.json");
const PADDLE_COMPLETED_FIXTURE: &str = include_str!("fixtures/paddle/transaction_completed.json");
const PADDLE_RENEWAL_FIXTURE: &str =
    include_str!("fixtures/paddle/subscription_renewal_transaction_completed.json");
const PADDLE_ADJ_REFUND_FIXTURE: &str =
    include_str!("fixtures/paddle/adjustment_refund_approved.json");
const PADDLE_ADJ_CHARGEBACK_FIXTURE: &str =
    include_str!("fixtures/paddle/adjustment_chargeback.json");

fn real_egypt_billing_data() -> CustomerBillingData {
    CustomerBillingData {
        first_name: "Nadia".to_owned(),
        last_name: "Hassan".to_owned(),
        email: "nadia.hassan@customer.example".to_owned(),
        phone_number: "+201023456789".to_owned(),
        country: "EG".to_owned(),
    }
}

#[derive(Clone, Default)]
struct MockProviderState {
    paddle_create_calls: Arc<AtomicUsize>,
    paddle_refund_calls: Arc<AtomicUsize>,
    paymob_create_calls: Arc<AtomicUsize>,
    paymob_refund_calls: Arc<AtomicUsize>,
    paddle_fail_next: Arc<AtomicBool>,
    paddle_timeout_after_create: Arc<AtomicBool>,
    paddle_refund_pending_mode: Arc<AtomicBool>,
    paddle_refund_timeout_mode: Arc<AtomicBool>,
    paddle_omit_currency_next: Arc<AtomicBool>,
    paddle_txns_by_order: Arc<Mutex<HashMap<String, Value>>>,
    paymob_intentions_by_order: Arc<Mutex<HashMap<String, Value>>>,
    last_paymob_billing_data: Arc<Mutex<Option<Value>>>,
    next_paymob_order_id: Arc<AtomicUsize>,
}

async fn spawn_mock_providers() -> (String, String, MockProviderState) {
    let st = MockProviderState::default();
    st.next_paymob_order_id.store(700_000, Ordering::SeqCst);

    let paddle_router = Router::new()
        .route(
            "/transactions",
            post(
                |AxumState(st): AxumState<MockProviderState>, Json(body): Json<Value>| async move {
                    let call_n = st.paddle_create_calls.fetch_add(1, Ordering::SeqCst) + 1;
                    if st.paddle_fail_next.swap(false, Ordering::SeqCst) {
                        return (
                            StatusCode::UNPROCESSABLE_ENTITY,
                            Json(json!({ "error": { "code": "invalid_price" } })),
                        );
                    }
                    let order_id = body["custom_data"]["safi_order_id"]
                        .as_str()
                        .unwrap_or("unknown")
                        .to_owned();
                    let expected_amt = body["custom_data"]["expected_amount_minor"]
                        .as_u64()
                        .unwrap_or(2000);
                    let price_id = body["items"][0]["price_id"]
                        .as_str()
                        .unwrap_or("pri_pro_monthly")
                        .to_owned();
                    let has_customer_and_address = body
                        .get("customer_id")
                        .and_then(Value::as_str)
                        .is_some_and(|s| !s.is_empty())
                        && body
                            .get("address_id")
                            .and_then(Value::as_str)
                            .is_some_and(|s| !s.is_empty());

                    let txn_id = format!("txn_live_{call_n:04}");
                    // Official Paddle behavior:
                    // - With customer_id + address_id -> status = "ready", hosted checkout.url returned.
                    // - Without customer_id/address_id -> status = "draft", checkout.url = null (Paddle.js overlay handoff).
                    let mut txn = if has_customer_and_address {
                        json!({
                            "id": txn_id,
                            "status": "ready",
                            "currency_code": "USD",
                            "checkout": {
                                "url": format!("https://checkout.paddle.com/txn/{txn_id}")
                            },
                            "items": [{ "price": { "id": price_id } }],
                            "custom_data": body["custom_data"],
                            "details": { "totals": { "total": expected_amt.to_string() } }
                        })
                    } else {
                        json!({
                            "id": txn_id,
                            "status": "draft",
                            "currency_code": "USD",
                            "checkout": { "url": Value::Null },
                            "items": [{ "price": { "id": price_id } }],
                            "custom_data": body["custom_data"],
                            "details": { "totals": { "total": expected_amt.to_string() } }
                        })
                    };
                    if st.paddle_omit_currency_next.swap(false, Ordering::SeqCst) {
                        txn.as_object_mut().expect("obj").remove("currency_code");
                    }
                    st.paddle_txns_by_order
                        .lock()
                        .await
                        .insert(order_id, txn.clone());

                    if st.paddle_timeout_after_create.swap(false, Ordering::SeqCst) {
                        tokio::time::sleep(Duration::from_millis(600)).await;
                    }
                    (StatusCode::CREATED, Json(json!({ "data": txn })))
                },
            )
            .get(|AxumState(st): AxumState<MockProviderState>| async move {
                let items: Vec<Value> = st
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
                |AxumState(st): AxumState<MockProviderState>, Json(body): Json<Value>| async move {
                    let n = st.paddle_refund_calls.fetch_add(1, Ordering::SeqCst) + 1;
                    if st.paddle_refund_timeout_mode.swap(false, Ordering::SeqCst) {
                        tokio::time::sleep(Duration::from_millis(600)).await;
                    }
                    let status = if st.paddle_refund_pending_mode.load(Ordering::SeqCst) {
                        "pending_approval"
                    } else {
                        "approved"
                    };
                    (
                        StatusCode::CREATED,
                        Json(json!({
                            "data": {
                                "id": format!("adj_live_{n:04}"),
                                "transaction_id": body["transaction_id"],
                                "action": "refund",
                                "status": status
                            }
                        })),
                    )
                },
            ),
        )
        .with_state(st.clone());

    let paymob_router = Router::new()
        .route(
            "/v1/intention/",
            post(
                |AxumState(st): AxumState<MockProviderState>, Json(body): Json<Value>| async move {
                    st.paymob_create_calls.fetch_add(1, Ordering::SeqCst);
                    *st.last_paymob_billing_data.lock().await = body.get("billing_data").cloned();
                    let order_num = st.next_paymob_order_id.fetch_add(1, Ordering::SeqCst) as u64;
                    let special_ref = body["special_reference"]
                        .as_str()
                        .unwrap_or("unknown")
                        .to_owned();
                    let intention = json!({
                        "id": format!("pi_live_{order_num}"),
                        "intention_order_id": order_num,
                        "client_secret": format!("egy_csk_live_{order_num}"),
                        "special_reference": special_ref
                    });
                    st.paymob_intentions_by_order
                        .lock()
                        .await
                        .insert(special_ref, intention.clone());
                    (StatusCode::CREATED, Json(intention))
                },
            )
            .get(
                |AxumState(st): AxumState<MockProviderState>,
                 axum::extract::Query(q): axum::extract::Query<HashMap<String, String>>| async move {
                    let key = q.get("special_reference").cloned().unwrap_or_default();
                    if let Some(found) = st.paymob_intentions_by_order.lock().await.get(&key).cloned() {
                        (StatusCode::OK, Json(found))
                    } else {
                        (StatusCode::NOT_FOUND, Json(json!({ "detail": "not_found" })))
                    }
                },
            ),
        )
        .route(
            "/api/acceptance/void_refund/refund",
            post(
                |AxumState(st): AxumState<MockProviderState>, Json(body): Json<Value>| async move {
                    let n = st.paymob_refund_calls.fetch_add(1, Ordering::SeqCst) + 1;
                    (
                        StatusCode::OK,
                        Json(json!({
                            "id": 990_000 + n,
                            "transaction_id": body["transaction_id"],
                            "amount_cents": body["amount_cents"],
                            "success": true,
                            "is_refunded": true,
                            "pending": false
                        })),
                    )
                },
            ),
        )
        .with_state(st.clone());

    let p_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind paddle mock");
    let p_addr = p_listener.local_addr().expect("addr");
    tokio::spawn(async move {
        let _ = axum::serve(p_listener, paddle_router).await;
    });

    let m_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind paymob mock");
    let m_addr = m_listener.local_addr().expect("addr");
    tokio::spawn(async move {
        let _ = axum::serve(m_listener, paymob_router).await;
    });

    (format!("http://{p_addr}"), format!("http://{m_addr}"), st)
}

fn financial_test_config(paddle_url: &str, paymob_url: &str) -> ServerConfig {
    let mut cfg = ServerConfig::for_tests();
    cfg.paddle_api_base_url = paddle_url.to_owned();
    cfg.paddle_api_key = Some("pdl_test_live_key".to_owned());
    cfg.paddle_webhook_secret = Some("pdl_whsec_financial_p0".to_owned());
    cfg.paddle_price_individual_monthly = Some("pri_pro_monthly".to_owned());
    cfg.paddle_price_individual_annual = Some("pri_pro_annual".to_owned());
    cfg.paddle_price_org_seat_annual = Some("pri_org_seat_annual".to_owned());
    cfg.paddle_price_pay_per_file = Some("pri_pay_per_file".to_owned());
    cfg.paddle_price_credit_pack_25 = Some("pri_credit_pack_25".to_owned());

    cfg.paymob_api_base_url = paymob_url.to_owned();
    cfg.paymob_api_key = Some("egy_sk_test_live_key".to_owned());
    cfg.paymob_public_key = Some("egy_pk_test_live_key".to_owned());
    cfg.paymob_hmac_secret = Some("paymob_hmac_financial_p0".to_owned());
    cfg.paymob_integration_id = Some(445566);
    cfg
}

async fn test_state(paddle_url: &str, paymob_url: &str) -> AppState {
    let cfg = financial_test_config(paddle_url, paymob_url);
    let auth = AuthService::new(
        Arc::new(InMemoryApiKeyRepository::new(Vec::new()).await),
        Arc::new(InMemorySessionRepository::new()),
        Arc::new(InMemoryAuthCache::new()),
    );
    AppState::with_config(auth, cfg)
}

fn load_paymob_fixture(
    raw: &str,
    order_id: u64,
    tx_id: u64,
    safi_order_id: Uuid,
) -> PaymobCallbackEnvelope {
    let replaced = raw.replace("__SAFI_ORDER_ID__", &safi_order_id.to_string());
    let mut env: PaymobCallbackEnvelope = serde_json::from_str(&replaced).expect("paymob fixture");
    env.obj.order.id = order_id;
    env.obj.id = tx_id;
    env
}

/// P0-1: Checkout idempotency, 20-way concurrency, ready vs draft Paddle lifecycle,
/// missing currency rejection, coupon release, timeout reconciliation, and real Paymob billing data.
#[tokio::test]
async fn p0_checkout_idempotency_ready_draft_reconciliation_and_real_paymob_billing() {
    let (paddle_url, paymob_url, mock) = spawn_mock_providers().await;
    let state = test_state(&paddle_url, &paymob_url).await;
    let ws = Uuid::now_v7();
    let uid = Uuid::now_v7();
    let now = unix_now();

    // 1. 20-way concurrent Paddle READY checkout (with customer_id + address_id) -> provider called ONCE!
    let mut tasks = Vec::new();
    for _ in 0..20 {
        let billing = state.billing.clone();
        tasks.push(tokio::spawn(async move {
            billing
                .create_checkout(&CheckoutRequest {
                    workspace_id: ws,
                    user_id: uid,
                    product: CheckoutProduct::IndividualMonthly,
                    billing_country_iso2: "US".to_owned(),
                    seats: None,
                    coupon_code: None,
                    idempotency_key: Some("idem-paddle-ready-20x".to_owned()),
                    billing_data: None,
                    paddle_customer: Some(PaddleCustomerContext {
                        customer_id: Some("ctm_real_001".to_owned()),
                        address_id: Some("add_real_001".to_owned()),
                    }),
                })
                .await
        }));
    }
    let mut order_ids = Vec::new();
    let mut urls = Vec::new();
    for t in tasks {
        let r = t.await.expect("join").expect("checkout ok");
        assert_eq!(r.checkout_mode, CheckoutMode::RedirectUrl);
        let url = r.checkout_url.expect("ready checkout url");
        assert!(url.starts_with("https://checkout.paddle.com/txn/"));
        order_ids.push(r.order_id);
        urls.push(url);
    }
    assert!(order_ids.iter().all(|id| *id == order_ids[0]));
    assert!(urls.iter().all(|u| u == &urls[0]));
    assert_eq!(mock.paddle_create_calls.load(Ordering::SeqCst), 1);

    // 2. Official Paddle DRAFT checkout (without customer_id/address_id) -> PaddleJsOverlay, checkout_url = None!
    let draft = state
        .billing
        .create_checkout(&CheckoutRequest {
            workspace_id: ws,
            user_id: uid,
            product: CheckoutProduct::IndividualMonthly,
            billing_country_iso2: "US".to_owned(),
            seats: None,
            coupon_code: None,
            idempotency_key: Some("idem-paddle-draft-1".to_owned()),
            billing_data: None,
            paddle_customer: None,
        })
        .await
        .expect("draft checkout ok");
    assert_eq!(draft.checkout_mode, CheckoutMode::PaddleJsOverlay);
    assert!(draft.checkout_url.is_none());
    assert!(draft
        .checkout_session_id
        .as_deref()
        .is_some_and(|s| s.starts_with("txn_live_")));

    // 3. Missing currency_code in Paddle API response MUST NOT assume USD — fail closed!
    mock.paddle_omit_currency_next.store(true, Ordering::SeqCst);
    let missing_curr_err = state
        .billing
        .create_checkout(&CheckoutRequest {
            workspace_id: ws,
            user_id: uid,
            product: CheckoutProduct::IndividualMonthly,
            billing_country_iso2: "US".to_owned(),
            seats: None,
            coupon_code: None,
            idempotency_key: Some("idem-paddle-missing-curr".to_owned()),
            billing_data: None,
            paddle_customer: None,
        })
        .await
        .unwrap_err();
    assert_eq!(missing_curr_err, AuthError::BillingOperationFailed);

    // 4. Conflicting request with SAME idempotency key -> SF-PAY-007 (409 Conflict)!
    let conflict = state
        .billing
        .create_checkout(&CheckoutRequest {
            workspace_id: ws,
            user_id: uid,
            product: CheckoutProduct::IndividualAnnual,
            billing_country_iso2: "US".to_owned(),
            seats: None,
            coupon_code: None,
            idempotency_key: Some("idem-paddle-ready-20x".to_owned()),
            billing_data: None,
            paddle_customer: None,
        })
        .await
        .unwrap_err();
    assert_eq!(conflict, AuthError::IdempotencyConflict);

    // 5. Coupon reservation released on definitive 422 provider failure so customer keeps their one-time coupon!
    let owner_id = Uuid::now_v7();
    state
        .coupons
        .create_coupon(
            owner_id,
            CreateCouponRequest {
                code: "ONCE25".to_owned(),
                discount_kind: DiscountKind::Percent,
                discount_value: 25,
                starts_at: Some(now - 60),
                expires_at: Some(now + 3600),
                max_redemptions: Some(1),
                eligible_plans: vec![CheckoutProduct::IndividualMonthly],
                eligible_user_ids: Vec::new(),
                new_customers_only: false,
                org_only: false,
                min_purchase_minor: 0,
            },
        )
        .await
        .expect("create coupon");

    mock.paddle_fail_next.store(true, Ordering::SeqCst);
    let failed_res = state
        .billing
        .create_checkout(&CheckoutRequest {
            workspace_id: ws,
            user_id: uid,
            product: CheckoutProduct::IndividualMonthly,
            billing_country_iso2: "US".to_owned(),
            seats: None,
            coupon_code: Some("ONCE25".to_owned()),
            idempotency_key: Some("idem-coupon-fail-1".to_owned()),
            billing_data: None,
            paddle_customer: None,
        })
        .await;
    assert_eq!(failed_res.unwrap_err(), AuthError::BillingOperationFailed);

    let ok_after_release = state
        .billing
        .create_checkout(&CheckoutRequest {
            workspace_id: ws,
            user_id: uid,
            product: CheckoutProduct::IndividualMonthly,
            billing_country_iso2: "US".to_owned(),
            seats: None,
            coupon_code: Some("ONCE25".to_owned()),
            idempotency_key: Some("idem-coupon-retry-2".to_owned()),
            billing_data: None,
            paddle_customer: None,
        })
        .await
        .expect("coupon usable after release");
    assert_eq!(ok_after_release.amount_minor, 1500);
    assert_eq!(ok_after_release.discount_amount_minor, 500);

    // 6. Uncertain provider timeout -> SF-PAY-008 + reconcile via official GET /transactions without 2nd charge!
    let short_state = test_state(&paddle_url, &paymob_url).await;
    let short_billing = short_state.billing.with_provider_timeout_ms(200);
    let before_calls = mock.paddle_create_calls.load(Ordering::SeqCst);
    mock.paddle_timeout_after_create
        .store(true, Ordering::SeqCst);
    let timeout_err = short_billing
        .create_checkout(&CheckoutRequest {
            workspace_id: ws,
            user_id: uid,
            product: CheckoutProduct::CreditPack25,
            billing_country_iso2: "US".to_owned(),
            seats: None,
            coupon_code: None,
            idempotency_key: Some("idem-timeout-rec-1".to_owned()),
            billing_data: None,
            paddle_customer: None,
        })
        .await
        .unwrap_err();
    assert_eq!(timeout_err, AuthError::PaymentReconciliationRequired);
    assert_eq!(
        mock.paddle_create_calls.load(Ordering::SeqCst),
        before_calls + 1
    );
    tokio::time::sleep(Duration::from_millis(650)).await;

    let reconciled = short_billing
        .create_checkout(&CheckoutRequest {
            workspace_id: ws,
            user_id: uid,
            product: CheckoutProduct::CreditPack25,
            billing_country_iso2: "US".to_owned(),
            seats: None,
            coupon_code: None,
            idempotency_key: Some("idem-timeout-rec-1".to_owned()),
            billing_data: None,
            paddle_customer: None,
        })
        .await
        .expect("reconcile succeeds");
    assert!(reconciled.reused_existing_order);
    assert_eq!(
        mock.paddle_create_calls.load(Ordering::SeqCst),
        before_calls + 1,
        "reconciliation MUST NOT create a second provider transaction!"
    );

    // 7. Paymob EGP checkout rejects missing billing_data (zero fake PII!) and forwards real customer data!
    let missing_bd = state
        .billing
        .create_checkout(&CheckoutRequest {
            workspace_id: ws,
            user_id: uid,
            product: CheckoutProduct::CreditPack25,
            billing_country_iso2: "EG".to_owned(),
            seats: None,
            coupon_code: None,
            idempotency_key: Some("idem-paymob-missing-bd".to_owned()),
            billing_data: None,
            paddle_customer: None,
        })
        .await
        .unwrap_err();
    assert_eq!(missing_bd, AuthError::InvalidRequest);

    let paymob_ok = state
        .billing
        .create_checkout(&CheckoutRequest {
            workspace_id: ws,
            user_id: uid,
            product: CheckoutProduct::CreditPack25,
            billing_country_iso2: "EG".to_owned(),
            seats: None,
            coupon_code: None,
            idempotency_key: Some("idem-paymob-real-bd".to_owned()),
            billing_data: Some(real_egypt_billing_data()),
            paddle_customer: None,
        })
        .await
        .expect("paymob with real customer billing data succeeds");
    assert!(paymob_ok.checkout_url.as_deref().is_some_and(|u| u
        .contains("/unifiedcheckout/?publicKey=egy_pk_test_live_key&clientSecret=egy_csk_live_")));
    let forwarded = mock
        .last_paymob_billing_data
        .lock()
        .await
        .clone()
        .expect("forwarded billing_data");
    assert_eq!(forwarded["email"], json!("nadia.hassan@customer.example"));
    assert_eq!(forwarded["phone_number"], json!("+201023456789"));
    assert_eq!(forwarded["first_name"], json!("Nadia"));
    assert_ne!(forwarded["email"], json!("billing@safidata.app"));
}

/// P0-2: Official Paymob `{"type":"TRANSACTION","obj":{...}}` envelope fixtures, HMAC-SHA512 over
/// nested fields (`obj.order.id`, `obj.source_data.*`), trust boundary, capture lifecycle,
/// concurrency, and cumulative `obj.refunded_amount_cents` partial-to-full refund accounting.
#[tokio::test]
async fn p0_paymob_official_envelope_trust_boundary_cumulative_partial_refunds_and_concurrency() {
    let (paddle_url, paymob_url, _mock) = spawn_mock_providers().await;
    let state = test_state(&paddle_url, &paymob_url).await;
    let ws = Uuid::now_v7();
    let uid = Uuid::now_v7();
    let attacker_ws = Uuid::now_v7();
    let now = unix_now();

    let order = state
        .billing
        .create_checkout(&CheckoutRequest {
            workspace_id: ws,
            user_id: uid,
            product: CheckoutProduct::IndividualMonthly,
            billing_country_iso2: "EG".to_owned(),
            seats: None,
            coupon_code: None,
            idempotency_key: Some("idem-paymob-official-1".to_owned()),
            billing_data: Some(real_egypt_billing_data()),
            paddle_customer: None,
        })
        .await
        .expect("paymob order");
    let paymob_order_id: u64 = order
        .provider_order_ref
        .as_deref()
        .expect("order ref")
        .parse()
        .expect("u64");
    assert_eq!(order.amount_minor, 100_000);

    let make_captured = || {
        let mut env = load_paymob_fixture(
            PAYMOB_CAPTURED_FIXTURE,
            paymob_order_id,
            910_001,
            order.order_id,
        );
        env.obj.amount_cents = 100_000;
        env.obj.order.amount_cents = Some(100_000);
        env
    };

    // 1. Wrong envelope `type` != "TRANSACTION" rejected!
    let mut wrong_type = make_captured();
    wrong_type.callback_type = "TOKEN".to_owned();
    let sig = sign_paymob_transaction("paymob_hmac_financial_p0", &wrong_type.obj).expect("sig");
    let raw = serde_json::to_vec(&wrong_type).expect("json");
    assert_eq!(
        state
            .billing
            .process_paymob_webhook_raw(&sig, &raw, None, now)
            .await
            .unwrap_err(),
        AuthError::InvalidRequest
    );

    // 2. Invalid HMAC & Missing HMAC rejected!
    let valid_env = make_captured();
    let valid_raw = serde_json::to_vec(&valid_env).expect("json");
    assert_eq!(
        state
            .billing
            .process_paymob_webhook_raw("deadbeef", &valid_raw, None, now)
            .await
            .unwrap_err(),
        AuthError::InvalidWebhookSignature
    );
    assert_eq!(
        state
            .billing
            .process_paymob_webhook_raw("", &valid_raw, None, now)
            .await
            .unwrap_err(),
        AuthError::InvalidWebhookSignature
    );

    // 3. Wrong `obj.order.id` (including confusing `obj.id` with `obj.order.id`!) rejected!
    let mut confused_ids = make_captured();
    confused_ids.obj.order.id = 910_001; // transaction id instead of order id!
    let sig = sign_paymob_transaction("paymob_hmac_financial_p0", &confused_ids.obj).expect("sig");
    assert_eq!(
        state
            .billing
            .process_paymob_webhook_raw(
                &sig,
                &serde_json::to_vec(&confused_ids).expect("j"),
                None,
                now
            )
            .await
            .unwrap_err(),
        AuthError::BillingOperationFailed
    );

    // 4. Wrong amount, wrong currency, wrong integration_id rejected!
    let mut bad_amt = make_captured();
    bad_amt.obj.amount_cents = 50_000;
    let sig = sign_paymob_transaction("paymob_hmac_financial_p0", &bad_amt.obj).expect("sig");
    assert_eq!(
        state
            .billing
            .process_paymob_webhook_raw(&sig, &serde_json::to_vec(&bad_amt).expect("j"), None, now)
            .await
            .unwrap_err(),
        AuthError::BillingOperationFailed
    );

    let mut bad_curr = make_captured();
    bad_curr.obj.currency = "USD".to_owned();
    let sig = sign_paymob_transaction("paymob_hmac_financial_p0", &bad_curr.obj).expect("sig");
    assert_eq!(
        state
            .billing
            .process_paymob_webhook_raw(&sig, &serde_json::to_vec(&bad_curr).expect("j"), None, now)
            .await
            .unwrap_err(),
        AuthError::BillingOperationFailed
    );

    let mut bad_int = make_captured();
    bad_int.obj.integration_id = 999_999;
    let sig = sign_paymob_transaction("paymob_hmac_financial_p0", &bad_int.obj).expect("sig");
    assert_eq!(
        state
            .billing
            .process_paymob_webhook_raw(&sig, &serde_json::to_vec(&bad_int).expect("j"), None, now)
            .await
            .unwrap_err(),
        AuthError::BillingOperationFailed
    );

    // 5. Tampered unsigned beneficiary metadata inside `obj.data` rejected!
    let mut tampered = make_captured();
    tampered.obj.data = Some(json!({ "workspace_id": attacker_ws.to_string() }));
    let sig = sign_paymob_transaction("paymob_hmac_financial_p0", &tampered.obj).expect("sig");
    assert_eq!(
        state
            .billing
            .process_paymob_webhook_raw(&sig, &serde_json::to_vec(&tampered).expect("j"), None, now)
            .await
            .unwrap_err(),
        AuthError::BillingOperationFailed
    );

    // 6. Pending payment & Auth-without-capture NEVER activate subscription!
    let mut pending_env = make_captured();
    pending_env.obj.pending = true;
    pending_env.obj.success = false;
    let sig = sign_paymob_transaction("paymob_hmac_financial_p0", &pending_env.obj).expect("sig");
    let rec_p = state
        .billing
        .process_paymob_webhook_raw(
            &sig,
            &serde_json::to_vec(&pending_env).expect("j"),
            Some("deliv-pend-1"),
            now,
        )
        .await
        .expect("pending recorded");
    assert_eq!(rec_p.outcome, "payment_pending_no_grant");

    let mut auth_only = make_captured();
    auth_only.obj.is_auth = true;
    auth_only.obj.is_capture = false;
    auth_only.obj.is_standalone_payment = false;
    let sig = sign_paymob_transaction("paymob_hmac_financial_p0", &auth_only.obj).expect("sig");
    let rec_a = state
        .billing
        .process_paymob_webhook_raw(
            &sig,
            &serde_json::to_vec(&auth_only).expect("j"),
            Some("deliv-auth-1"),
            now,
        )
        .await
        .expect("auth-only recorded");
    assert_eq!(rec_a.outcome, "payment_pending_no_grant");
    assert_eq!(
        state
            .entitlements
            .effective_at(ws, uid, false, now)
            .await
            .effective_plan,
        PlanTier::Free
    );

    // 7. 20-way concurrent captured official callback -> activates subscription EXACTLY ONCE!
    let captured_env = make_captured();
    let captured_raw = serde_json::to_vec(&captured_env).expect("json");
    let captured_sig =
        sign_paymob_transaction("paymob_hmac_financial_p0", &captured_env.obj).expect("sig");
    let mut wh_tasks = Vec::new();
    for _ in 0..20 {
        let b = state.billing.clone();
        let s = captured_sig.clone();
        let r = captured_raw.clone();
        wh_tasks.push(tokio::spawn(async move {
            b.process_paymob_webhook_raw(&s, &r, None, now + 1).await
        }));
    }
    let mut granted = 0usize;
    for t in wh_tasks {
        let rec = t.await.expect("join").expect("ok");
        if rec.outcome == "paymob_subscription_active" {
            granted += 1;
        }
    }
    assert_eq!(granted, 1);
    assert_eq!(
        state
            .entitlements
            .effective_at(ws, uid, false, now + 2)
            .await
            .effective_plan,
        PlanTier::IndividualMonthly
    );

    // Same transaction ID arriving under a DIFFERENT delivery event ID -> still deduplicated!
    let dup_new_deliv = state
        .billing
        .process_paymob_webhook_raw(
            &captured_sig,
            &captured_raw,
            Some("deliv-different-id"),
            now + 2,
        )
        .await
        .expect("dedup ok");
    assert_eq!(dup_new_deliv.outcome, "duplicate_transaction_grant_ignored");

    // 8. Official Paymob cumulative partial refunds via `obj.refunded_amount_cents`!
    // Step 8a: First partial refund 30,000 / 100,000 -> `partially_refunded`, subscription STILL Active!
    let mut pref1 = load_paymob_fixture(
        PAYMOB_PARTIAL_REFUND_FIXTURE,
        paymob_order_id,
        910_001,
        order.order_id,
    );
    pref1.obj.refunded_amount_cents = 30_000;
    let sig1 = sign_paymob_transaction("paymob_hmac_financial_p0", &pref1.obj).expect("sig");
    let r1 = state
        .billing
        .process_paymob_webhook_raw(
            &sig1,
            &serde_json::to_vec(&pref1).expect("j"),
            Some("pm-ref-1"),
            now + 10,
        )
        .await
        .expect("partial 1");
    assert_eq!(r1.outcome, "partial_refund_processed");
    assert_eq!(
        state.billing.list_invoices(ws).await[0].refunded_amount_minor,
        30_000
    );
    assert_eq!(
        state
            .entitlements
            .effective_at(ws, uid, false, now + 11)
            .await
            .effective_plan,
        PlanTier::IndividualMonthly,
        "partial refund MUST NOT revoke active subscription"
    );

    // Step 8b: Repeated cumulative 30,000 under new delivery ID -> ignored, stays 30,000!
    let r1_dup = state
        .billing
        .process_paymob_webhook_raw(
            &sig1,
            &serde_json::to_vec(&pref1).expect("j"),
            Some("pm-ref-1-dup"),
            now + 12,
        )
        .await
        .expect("dup cum");
    assert_eq!(
        r1_dup.outcome,
        "cumulative_refund_duplicate_or_stale_ignored"
    );
    assert_eq!(
        state.billing.list_invoices(ws).await[0].refunded_amount_minor,
        30_000
    );

    // Step 8c: Out-of-order lower cumulative 20,000 -> ignored, NEVER corrupts/reduces 30,000!
    let mut pref_low = pref1.clone();
    pref_low.obj.refunded_amount_cents = 20_000;
    pref_low.obj.created_at = "2026-11-23T12:06:00.000000".to_owned();
    let sig_low = sign_paymob_transaction("paymob_hmac_financial_p0", &pref_low.obj).expect("sig");
    let r_low = state
        .billing
        .process_paymob_webhook_raw(
            &sig_low,
            &serde_json::to_vec(&pref_low).expect("j"),
            Some("pm-ref-low"),
            now + 13,
        )
        .await
        .expect("low cum");
    assert_eq!(
        r_low.outcome,
        "cumulative_refund_duplicate_or_stale_ignored"
    );
    assert_eq!(
        state.billing.list_invoices(ws).await[0].refunded_amount_minor,
        30_000
    );

    // Step 8d: Second cumulative partial refund 70,000 -> delta 40,000, total 70,000, still Active!
    let mut pref2 = pref1.clone();
    pref2.obj.refunded_amount_cents = 70_000;
    pref2.obj.created_at = "2026-11-23T12:07:00.000000".to_owned();
    let sig2 = sign_paymob_transaction("paymob_hmac_financial_p0", &pref2.obj).expect("sig");
    let r2 = state
        .billing
        .process_paymob_webhook_raw(
            &sig2,
            &serde_json::to_vec(&pref2).expect("j"),
            Some("pm-ref-2"),
            now + 14,
        )
        .await
        .expect("partial 2");
    assert_eq!(r2.outcome, "partial_refund_processed");
    assert_eq!(
        state.billing.list_invoices(ws).await[0].refunded_amount_minor,
        70_000
    );
    assert_eq!(
        state
            .entitlements
            .effective_at(ws, uid, false, now + 15)
            .await
            .effective_plan,
        PlanTier::IndividualMonthly
    );

    // Step 8e: Cumulative refund exceeding captured amount (120,000 > 100,000) -> REJECTED!
    let mut pref_over = pref2.clone();
    pref_over.obj.refunded_amount_cents = 120_000;
    let sig_over =
        sign_paymob_transaction("paymob_hmac_financial_p0", &pref_over.obj).expect("sig");
    assert_eq!(
        state
            .billing
            .process_paymob_webhook_raw(
                &sig_over,
                &serde_json::to_vec(&pref_over).expect("j"),
                Some("pm-ref-over"),
                now + 16
            )
            .await
            .unwrap_err(),
        AuthError::BillingOperationFailed
    );

    // Step 8f: Full cumulative refund (100,000) -> `refunded` + revokes subscription!
    let mut pref_full = pref2.clone();
    pref_full.obj.refunded_amount_cents = 100_000;
    pref_full.obj.is_refunded = true;
    pref_full.obj.created_at = "2026-11-23T12:08:00.000000".to_owned();
    let sig_full =
        sign_paymob_transaction("paymob_hmac_financial_p0", &pref_full.obj).expect("sig");
    let r_full = state
        .billing
        .process_paymob_webhook_raw(
            &sig_full,
            &serde_json::to_vec(&pref_full).expect("j"),
            Some("pm-ref-full"),
            now + 17,
        )
        .await
        .expect("full refund");
    assert_eq!(r_full.outcome, "refund_processed");
    assert_eq!(
        state.billing.list_invoices(ws).await[0].refunded_amount_minor,
        100_000
    );
    assert_eq!(
        state
            .entitlements
            .effective_at(ws, uid, false, now + 18)
            .await
            .effective_plan,
        PlanTier::Free
    );
}

/// P0-3: Official Paddle webhook fixtures, payload-hash security anomaly (`SF-PAY-009`),
/// recurring monthly & annual subscription renewals (2+ cycles each), adjustments, chargeback,
/// out-of-order protection, and uncertain refund timeout reconciliation.
#[tokio::test]
async fn p0_paddle_official_envelopes_renewals_payload_hash_anomaly_and_refund_reconciliation() {
    let (paddle_url, paymob_url, mock) = spawn_mock_providers().await;
    let state = test_state(&paddle_url, &paymob_url).await;
    let ws = Uuid::now_v7();
    let uid = Uuid::now_v7();
    let now = unix_now();

    // 1. Create monthly subscription checkout order.
    let order = state
        .billing
        .create_checkout(&CheckoutRequest {
            workspace_id: ws,
            user_id: uid,
            product: CheckoutProduct::IndividualMonthly,
            billing_country_iso2: "US".to_owned(),
            seats: None,
            coupon_code: None,
            idempotency_key: Some("idem-paddle-sub-monthly-1".to_owned()),
            billing_data: None,
            paddle_customer: Some(PaddleCustomerContext {
                customer_id: Some("ctm_sub_1".to_owned()),
                address_id: Some("add_sub_1".to_owned()),
            }),
        })
        .await
        .expect("monthly order");
    let initial_txn = order.provider_order_ref.clone().expect("txn");

    // 2. Non-final events (`checkout.opened`, `transaction.created`, `transaction.ready`, `transaction.updated` draft)
    //    MUST NEVER grant entitlements!
    for (i, etype) in [
        "checkout.opened",
        "transaction.created",
        "transaction.ready",
        "transaction.updated",
    ]
    .iter()
    .enumerate()
    {
        let raw = serde_json::to_vec(&json!({
            "event_id": format!("evt_nonfinal_{i}"),
            "event_type": etype,
            "occurred_at": "2026-11-23T11:59:00.000000Z",
            "notification_id": format!("ntf_nf_{i}"),
            "data": {
                "id": initial_txn,
                "status": "ready",
                "currency_code": "USD",
                "custom_data": { "safi_order_id": order.order_id.to_string() }
            }
        }))
        .expect("json");
        let sig = sign_paddle_payload("pdl_whsec_financial_p0", now, &raw).expect("sig");
        let rec = state
            .billing
            .process_paddle_webhook(&sig, &raw, now)
            .await
            .expect("non-final accepted");
        assert_eq!(rec.outcome, "payment_pending_no_grant");
    }
    assert_eq!(
        state
            .entitlements
            .effective_at(ws, uid, false, now)
            .await
            .effective_plan,
        PlanTier::Free
    );

    // 3. Official Paddle `transaction.completed` fixture activates subscription & binds `sub_01j9safi_monthly_001`!
    let mut initial_val: Value = serde_json::from_str(
        &PADDLE_COMPLETED_FIXTURE.replace("__SAFI_ORDER_ID__", &order.order_id.to_string()),
    )
    .expect("fixture");
    initial_val["data"]["id"] = Value::String(initial_txn.clone());
    let initial_raw = serde_json::to_vec(&initial_val).expect("json");
    let initial_sig =
        sign_paddle_payload("pdl_whsec_financial_p0", now, &initial_raw).expect("sig");
    let rec_init = state
        .billing
        .process_paddle_webhook(&initial_sig, &initial_raw, now)
        .await
        .expect("initial completed");
    assert_eq!(rec_init.outcome, "subscription_active");
    let period_m1 = state
        .entitlements
        .snapshot_workspace(ws)
        .await
        .expect("ent")
        .current_period_end
        .expect("period m1");

    // 4. Payload-hash integrity (Section 3.3 & 7.3):
    //    - Identical body with same event_id -> idempotent duplicate!
    let dup_ok = state
        .billing
        .process_paddle_webhook(&initial_sig, &initial_raw, now)
        .await
        .expect("exact dup ok");
    assert_eq!(dup_ok.outcome, "idempotent_duplicate_ignored");

    //    - DIFFERENT payload with SAME event_id -> SF-PAY-009 (409 Conflict) + security anomaly recorded!
    let mut tampered_same_id = initial_val.clone();
    tampered_same_id["data"]["details"]["totals"]["total"] = json!("9999");
    let tampered_raw = serde_json::to_vec(&tampered_same_id).expect("json");
    let tampered_sig =
        sign_paddle_payload("pdl_whsec_financial_p0", now, &tampered_raw).expect("sig");
    let anomaly_err = state
        .billing
        .process_paddle_webhook(&tampered_sig, &tampered_raw, now)
        .await
        .unwrap_err();
    assert_eq!(anomaly_err, AuthError::WebhookPayloadHashConflict);
    assert_eq!(
        state.billing.count_security_anomalies().await.expect("cnt"),
        1
    );

    // 5. Recurring Monthly Renewal #1 (Month 2) via official renewal fixture (correlates by `subscription_id` ONLY — NO `safi_order_id` needed!).
    let m2_val: Value = serde_json::from_str(PADDLE_RENEWAL_FIXTURE).expect("renewal fixture");
    let m2_raw = serde_json::to_vec(&m2_val).expect("json");
    let m2_sig = sign_paddle_payload("pdl_whsec_financial_p0", now, &m2_raw).expect("sig");
    let rec_m2 = state
        .billing
        .process_paddle_webhook(&m2_sig, &m2_raw, now)
        .await
        .expect("m2 renewal");
    assert_eq!(rec_m2.outcome, "subscription_renewed");
    let period_m2 = state
        .entitlements
        .snapshot_workspace(ws)
        .await
        .expect("ent")
        .current_period_end
        .expect("period m2");
    assert!(period_m2 > period_m1);

    // Same renewal transaction arriving under a DIFFERENT `event_id` -> deduplicated, period unchanged!
    let mut m2_new_evt = m2_val.clone();
    m2_new_evt["event_id"] = json!("evt_01j9safi_renewal_m2_duplicate_evt");
    let m2_dup_raw = serde_json::to_vec(&m2_new_evt).expect("json");
    let m2_dup_sig = sign_paddle_payload("pdl_whsec_financial_p0", now, &m2_dup_raw).expect("sig");
    let rec_m2_dup = state
        .billing
        .process_paddle_webhook(&m2_dup_sig, &m2_dup_raw, now)
        .await
        .expect("m2 dup");
    assert_eq!(rec_m2_dup.outcome, "duplicate_transaction_grant_ignored");

    // 6. Recurring Monthly Renewal #2 (Month 3) -> creates 3rd distinct invoice and extends period again!
    let mut m3_val = m2_val.clone();
    m3_val["event_id"] = json!("evt_01j9safi_renewal_m3");
    m3_val["occurred_at"] = json!("2027-01-23T12:00:05.000000Z");
    m3_val["data"]["id"] = json!("txn_01j9safi_renewal_003");
    m3_val["data"]["invoice_id"] = json!("inv_01j9safi_renewal_003");
    m3_val["data"]["billing_period"]["starts_at"] = json!("2027-01-23T12:00:00Z");
    m3_val["data"]["billing_period"]["ends_at"] = json!("2027-02-23T12:00:00Z");
    let m3_raw = serde_json::to_vec(&m3_val).expect("json");
    let m3_sig = sign_paddle_payload("pdl_whsec_financial_p0", now, &m3_raw).expect("sig");
    let rec_m3 = state
        .billing
        .process_paddle_webhook(&m3_sig, &m3_raw, now)
        .await
        .expect("m3 renewal");
    assert_eq!(rec_m3.outcome, "subscription_renewed");
    let period_m3 = state
        .entitlements
        .snapshot_workspace(ws)
        .await
        .expect("ent")
        .current_period_end
        .expect("period m3");
    assert!(period_m3 > period_m2);
    assert_eq!(state.billing.list_invoices(ws).await.len(), 3);

    // 7. Recurring Annual Renewals (2 successive annual cycles on a second workspace)!
    let ann_ws = Uuid::now_v7();
    let ann_uid = Uuid::now_v7();
    let ann_order = state
        .billing
        .create_checkout(&CheckoutRequest {
            workspace_id: ann_ws,
            user_id: ann_uid,
            product: CheckoutProduct::IndividualAnnual,
            billing_country_iso2: "US".to_owned(),
            seats: None,
            coupon_code: None,
            idempotency_key: Some("idem-paddle-annual-1".to_owned()),
            billing_data: None,
            paddle_customer: None,
        })
        .await
        .expect("annual order");
    let ann_init_txn = ann_order.provider_order_ref.clone().expect("ann txn");
    let ann_events = [
        (
            "evt_ann_y1",
            "2026-11-23T12:00:00Z",
            ann_init_txn.as_str(),
            "inv_ann_y1",
            "web",
            Some(ann_order.order_id.to_string()),
            "2026-11-23T12:00:00Z",
            "2027-11-23T12:00:00Z",
            "subscription_active",
        ),
        (
            "evt_ann_y2",
            "2027-11-23T12:00:05Z",
            "txn_ann_y2",
            "inv_ann_y2",
            "subscription_recurring",
            None,
            "2027-11-23T12:00:00Z",
            "2028-11-23T12:00:00Z",
            "subscription_renewed",
        ),
        (
            "evt_ann_y3",
            "2028-11-23T12:00:05Z",
            "txn_ann_y3",
            "inv_ann_y3",
            "subscription_recurring",
            None,
            "2028-11-23T12:00:00Z",
            "2029-11-23T12:00:00Z",
            "subscription_renewed",
        ),
    ];
    let mut prev_end = 0_i64;
    for (ev, occ, tx, inv, orig, oid, ps, pe, expected_outcome) in ann_events {
        let mut data = json!({
            "id": tx,
            "status": "completed",
            "origin": orig,
            "currency_code": "USD",
            "subscription_id": "sub_ann_001",
            "invoice_id": inv,
            "billing_period": { "starts_at": ps, "ends_at": pe },
            "items": [{ "price": { "id": "pri_pro_annual" } }],
            "details": { "totals": { "total": "16800", "currency_code": "USD" } }
        });
        if let Some(oid_str) = oid {
            data["custom_data"] = json!({ "safi_order_id": oid_str });
        }
        let raw = serde_json::to_vec(&json!({
            "event_id": ev,
            "event_type": "transaction.completed",
            "occurred_at": occ,
            "data": data
        }))
        .expect("json");
        let sig = sign_paddle_payload("pdl_whsec_financial_p0", now, &raw).expect("sig");
        let rec = state
            .billing
            .process_paddle_webhook(&sig, &raw, now)
            .await
            .expect("annual event");
        assert_eq!(rec.outcome, expected_outcome);
        let cur_end = state
            .entitlements
            .snapshot_workspace(ann_ws)
            .await
            .expect("ent")
            .current_period_end
            .expect("end");
        assert!(cur_end > prev_end);
        prev_end = cur_end;
    }
    assert_eq!(state.billing.list_invoices(ann_ws).await.len(), 3);

    // 8. Official Paddle Adjustment Refund (partial -> keeps active) + Duplicate adj ignored + Chargeback revokes!
    let mut adj_ref: Value = serde_json::from_str(PADDLE_ADJ_REFUND_FIXTURE).expect("adj ref");
    adj_ref["occurred_at"] = json!("2027-01-24T12:00:00.000000Z");
    adj_ref["data"]["transaction_id"] = Value::String(initial_txn.clone());
    // Pending approval adjustment MUST NOT revoke or change invoice refunded total!
    let mut adj_pending = adj_ref.clone();
    adj_pending["event_id"] = json!("evt_adj_pending_1");
    adj_pending["data"]["status"] = json!("pending_approval");
    let raw_ap = serde_json::to_vec(&adj_pending).expect("j");
    let sig_ap = sign_paddle_payload("pdl_whsec_financial_p0", now, &raw_ap).expect("s");
    let rec_ap = state
        .billing
        .process_paddle_webhook(&sig_ap, &raw_ap, now)
        .await
        .expect("adj pending");
    assert_eq!(rec_ap.outcome, "payment_pending_no_grant");

    // Approved partial refund adjustment ($5 of $20) -> partially_refunded, subscription stays active!
    let raw_ar = serde_json::to_vec(&adj_ref).expect("j");
    let sig_ar = sign_paddle_payload("pdl_whsec_financial_p0", now, &raw_ar).expect("s");
    let rec_ar = state
        .billing
        .process_paddle_webhook(&sig_ar, &raw_ar, now)
        .await
        .expect("adj approved");
    assert_eq!(rec_ar.outcome, "partial_refund_processed");

    // Duplicate same adjustment ID under a new event ID -> ignored, never double-counted!
    let mut adj_dup = adj_ref.clone();
    adj_dup["event_id"] = json!("evt_adj_dup_new_id");
    let raw_ad = serde_json::to_vec(&adj_dup).expect("j");
    let sig_ad = sign_paddle_payload("pdl_whsec_financial_p0", now, &raw_ad).expect("s");
    let rec_ad = state
        .billing
        .process_paddle_webhook(&sig_ad, &raw_ad, now)
        .await
        .expect("adj dup");
    assert_eq!(rec_ad.outcome, "duplicate_provider_refund_ignored");

    // Chargeback fixture -> immediately revokes & blocks stale resurrection!
    let mut adj_cb: Value = serde_json::from_str(PADDLE_ADJ_CHARGEBACK_FIXTURE).expect("cb");
    adj_cb["occurred_at"] = json!("2027-01-25T12:00:00.000000Z");
    adj_cb["data"]["transaction_id"] = Value::String(initial_txn.clone());
    let raw_cb = serde_json::to_vec(&adj_cb).expect("j");
    let sig_cb = sign_paddle_payload("pdl_whsec_financial_p0", now, &raw_cb).expect("s");
    let rec_cb = state
        .billing
        .process_paddle_webhook(&sig_cb, &raw_cb, now)
        .await
        .expect("chargeback");
    assert_eq!(rec_cb.outcome, "chargeback_revoked");
    assert_eq!(
        state
            .entitlements
            .effective_at(ws, uid, false, now + 100)
            .await
            .effective_plan,
        PlanTier::Free
    );

    // 9. Uncertain refund timeout -> SF-PAY-008 (`reconciliation_required`), invoice stays `refund_pending`,
    //    entitlement stays intact, and retry with same key does NOT invoke provider refund twice!
    let short_billing = state.billing.clone().with_provider_timeout_ms(200);
    mock.paddle_refund_timeout_mode
        .store(true, Ordering::SeqCst);
    let ref_calls_before = mock.paddle_refund_calls.load(Ordering::SeqCst);
    let ref_timeout_err = short_billing
        .issue_owner_refund_idempotent(
            "inv_ann_y1",
            "ref-timeout-key-1",
            None,
            false,
            "timeout_test",
            None,
            now + 60,
        )
        .await
        .unwrap_err();
    assert_eq!(ref_timeout_err, AuthError::PaymentReconciliationRequired);
    assert_eq!(
        mock.paddle_refund_calls.load(Ordering::SeqCst),
        ref_calls_before + 1
    );
    // Entitlement NOT revoked on uncertain refund timeout!
    assert_eq!(
        state
            .entitlements
            .effective_at(ann_ws, ann_uid, false, now + 61)
            .await
            .effective_plan,
        PlanTier::IndividualAnnual
    );
    // Retrying with same key does NOT fire a second refund call!
    let dup_ref = short_billing
        .issue_owner_refund_idempotent(
            "inv_ann_y1",
            "ref-timeout-key-1",
            None,
            false,
            "timeout_test",
            None,
            now + 62,
        )
        .await
        .expect("returns pending invoice without calling provider twice");
    assert_eq!(dup_ref.status, "refund_pending");
    assert_eq!(
        mock.paddle_refund_calls.load(Ordering::SeqCst),
        ref_calls_before + 1
    );

    // Refund window check (> 3 days without legal override rejected).
    let expired_ref_err = state
        .billing
        .issue_owner_refund_idempotent(
            "inv_ann_y2",
            "ref-window-expired",
            None,
            false,
            "too_late",
            None,
            now + REFUND_WINDOW_SECS + 100,
        )
        .await
        .unwrap_err();
    assert_eq!(expired_ref_err, AuthError::BillingOperationFailed);
}

#[derive(Default)]
struct CaptureMailer {
    tokens: tokio::sync::RwLock<HashMap<String, String>>,
}

#[async_trait::async_trait]
impl VerificationEmailSender for CaptureMailer {
    async fn send_verification(
        &self,
        to_email: &str,
        verification_link: &str,
    ) -> Result<(), AuthError> {
        if let Some(tok) = verification_link.split("verify_email=").nth(1) {
            self.tokens
                .write()
                .await
                .insert(to_email.to_owned(), tok.to_owned());
        }
        Ok(())
    }

    async fn send_password_reset(
        &self,
        _to_email: &str,
        _reset_link: &str,
    ) -> Result<(), AuthError> {
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
    let json = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| json!({ "raw_body": String::from_utf8_lossy(&bytes).to_string() }));
    (st, json)
}

/// Real PostgreSQL 18 Multi-Instance, Concurrency, Official Envelopes, Cumulative Partial Refunds,
/// Recurring Subscription Renewals, Payload-Hash Anomaly & Restart Financial Integrity E2E Gate.
/// Executed automatically in CI via `tools/test-migrations.sh` on PostgreSQL 18.
#[tokio::test]
#[ignore = "requires SF_TEST_DATABASE_URL pointing at a migrated PostgreSQL 18 database (migrations 0001..0014)"]
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
    let ws_id: Uuid = login["session"]["workspace_id"]
        .as_str()
        .expect("ws")
        .parse()
        .expect("uuid");

    // 1. 20-way concurrent Paymob `credit_pack_25` checkout across Instance A & Instance B!
    //    (Verifies our fix for the `422` serde CreditPack25 bug + real customer billing_data!)
    let idem_key = format!("pg-multi-idem-{nonce}");
    let billing_payload = json!({
        "first_name": "Nadia",
        "last_name": "Hassan",
        "email": email,
        "phone_number": "+201023456789",
        "country": "EG"
    });
    let mut chk_tasks = Vec::new();
    for i in 0..20 {
        let r = if i % 2 == 0 {
            router_a.clone()
        } else {
            router_b.clone()
        };
        let b = bearer.clone();
        let k = idem_key.clone();
        let bd = billing_payload.clone();
        chk_tasks.push(tokio::spawn(async move {
            send_http_json(
                &r,
                "POST",
                "/v1/billing/checkout",
                Some(&b),
                &[("idempotency-key", &k)],
                Some(json!({
                    "product": "credit_pack_25",
                    "billing_country_iso2": "EG",
                    "billing_data": bd
                })),
            )
            .await
        }));
    }
    let mut order_ids = Vec::new();
    let mut paymob_order_ref: Option<String> = None;
    for t in chk_tasks {
        let (st, val) = t.await.expect("join");
        assert_eq!(st, StatusCode::OK, "checkout failed: {val}");
        order_ids.push(val["order_id"].as_str().expect("oid").to_owned());
        let pref = val["provider_order_ref"]
            .as_str()
            .expect("provider_order_ref")
            .to_owned();
        if let Some(ref first_ref) = paymob_order_ref {
            assert_eq!(
                first_ref, &pref,
                "every caller must receive identical provider_order_ref"
            );
        } else {
            paymob_order_ref = Some(pref);
        }
    }
    assert!(order_ids.iter().all(|id| id == &order_ids[0]));
    assert_eq!(
        mock.paymob_create_calls.load(Ordering::SeqCst),
        1,
        "EXACTLY ONE Paymob intention must be created across 20 concurrent requests on 2 independent instances!"
    );
    let safi_oid: Uuid = order_ids[0].parse().expect("oid uuid");
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
            "billing_country_iso2": "EG",
            "billing_data": billing_payload
        })),
    )
    .await;
    assert_eq!(conf_st, StatusCode::CONFLICT, "conflict: {conf_body}");
    assert_eq!(conf_body["code"], json!("SF-PAY-007"));

    // 3. Missing `?hmac` on Paymob webhook -> 401 SF-PAY-002!
    let tx_id = 55_000_000 + (unix_now() as u64 % 100_000);
    let paymob_env = load_paymob_fixture(PAYMOB_CAPTURED_FIXTURE, paymob_order_id, tx_id, safi_oid);
    let paymob_val = serde_json::to_value(&paymob_env).expect("val");
    let (no_hmac_st, no_hmac_body) = send_http_json(
        &router_a,
        "POST",
        "/v1/billing/webhooks/paymob",
        None,
        &[],
        Some(paymob_val.clone()),
    )
    .await;
    assert_eq!(no_hmac_st, StatusCode::UNAUTHORIZED);
    assert_eq!(no_hmac_body["code"], json!("SF-PAY-002"));

    // 4. 20-way concurrent official Paymob envelope callback across Instance A & Instance B ->
    //    PostgreSQL grants 25 credits EXACTLY ONCE!
    let hmac = sign_paymob_transaction("paymob_hmac_financial_p0", &paymob_env.obj).expect("hmac");
    let mut wh_tasks = Vec::new();
    for i in 0..20 {
        let r = if i % 2 == 0 {
            router_a.clone()
        } else {
            router_b.clone()
        };
        let uri = format!("/v1/billing/webhooks/paymob?hmac={hmac}");
        let body_val = paymob_val.clone();
        wh_tasks.push(tokio::spawn(async move {
            send_http_json(&r, "POST", &uri, None, &[], Some(body_val)).await
        }));
    }
    let mut granted_count = 0usize;
    for t in wh_tasks {
        let (st, val) = t.await.expect("join");
        assert_eq!(st, StatusCode::OK, "paymob webhook: {val}");
        if val["outcome"] == json!("paymob_credits_granted_25") {
            granted_count += 1;
        }
    }
    assert_eq!(granted_count, 1);

    // 5. Same delivery event_id with DIFFERENT payload hash on Instance B -> 409 SF-PAY-009 + anomaly persisted!
    let mut tampered_same_evt = paymob_env.clone();
    tampered_same_evt.issuer_bank = Some(json!("TAMPERED_BANK"));
    let tampered_hmac =
        sign_paymob_transaction("paymob_hmac_financial_p0", &tampered_same_evt.obj).expect("h");
    let (anom_st, anom_body) = send_http_json(
        &router_b,
        "POST",
        &format!("/v1/billing/webhooks/paymob?hmac={tampered_hmac}"),
        None,
        &[],
        Some(serde_json::to_value(&tampered_same_evt).expect("v")),
    )
    .await;
    assert_eq!(anom_st, StatusCode::CONFLICT, "anomaly: {anom_body}");
    assert_eq!(anom_body["code"], json!("SF-PAY-009"));

    // 6. Cumulative Paymob partial refunds across Instance A & B in PostgreSQL 18!
    let mut pm_part1 = paymob_env.clone();
    pm_part1.obj.refunded_amount_cents = 10_000;
    pm_part1.obj.created_at = "2026-11-23T12:05:00.000000".to_owned();
    let h_p1 = sign_paymob_transaction("paymob_hmac_financial_p0", &pm_part1.obj).expect("h");
    let (st_p1, b_p1) = send_http_json(
        &router_a,
        "POST",
        &format!("/v1/billing/webhooks/paymob?hmac={h_p1}"),
        None,
        &[("x-paymob-delivery-id", &format!("pg-pm-ref1-{nonce}"))],
        Some(serde_json::to_value(&pm_part1).expect("v")),
    )
    .await;
    assert_eq!(st_p1, StatusCode::OK, "part1: {b_p1}");
    assert_eq!(b_p1["outcome"], json!("partial_refund_processed"));

    // Out-of-order lower cumulative (5_000 < 10_000) on Instance B -> ignored!
    let mut pm_low = pm_part1.clone();
    pm_low.obj.refunded_amount_cents = 5_000;
    pm_low.obj.created_at = "2026-11-23T12:06:00.000000".to_owned();
    let h_low = sign_paymob_transaction("paymob_hmac_financial_p0", &pm_low.obj).expect("h");
    let (st_low, b_low) = send_http_json(
        &router_b,
        "POST",
        &format!("/v1/billing/webhooks/paymob?hmac={h_low}"),
        None,
        &[("x-paymob-delivery-id", &format!("pg-pm-reflow-{nonce}"))],
        Some(serde_json::to_value(&pm_low).expect("v")),
    )
    .await;
    assert_eq!(st_low, StatusCode::OK);
    assert_eq!(
        b_low["outcome"],
        json!("cumulative_refund_duplicate_or_stale_ignored")
    );

    // 7. Paddle Monthly Subscription + 2 successive renewals across Instance A & B in PostgreSQL 18!
    let (p_st, p_chk) = send_http_json(
        &router_a,
        "POST",
        "/v1/billing/checkout",
        Some(&bearer),
        &[("idempotency-key", &format!("pg-paddle-sub-{nonce}"))],
        Some(json!({
            "product": "individual_monthly",
            "billing_country_iso2": "US"
        })),
    )
    .await;
    assert_eq!(p_st, StatusCode::OK, "paddle sub checkout: {p_chk}");
    assert_eq!(p_chk["checkout_mode"], json!("paddle_js_overlay"));
    let p_oid = p_chk["order_id"].as_str().expect("p_oid");
    let p_txn = p_chk["provider_order_ref"].as_str().expect("p_txn");
    let sub_id = format!("sub_pg_{nonce}");
    let now = unix_now();

    let send_paddle_wh = |router: Router, payload: Value| async move {
        let raw = serde_json::to_vec(&payload).expect("json");
        let sig = sign_paddle_payload("pdl_whsec_financial_p0", now, &raw).expect("sig");
        let req = Request::builder()
            .method("POST")
            .uri("/v1/billing/webhooks/paddle")
            .header("paddle-signature", sig)
            .header("content-type", "application/json")
            .body(Body::from(raw))
            .expect("req");
        let resp = router.oneshot(req).await.expect("oneshot");
        let st = resp.status();
        let bytes = to_bytes(resp.into_body(), 1024 * 1024).await.expect("b");
        let val: Value = serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({}));
        (st, val)
    };

    // Initial purchase on Instance A
    let (st1, b1) = send_paddle_wh(
        router_a.clone(),
        json!({
            "event_id": format!("evt_pg_m1_{nonce}"),
            "event_type": "transaction.completed",
            "occurred_at": "2026-11-23T12:10:00.000000Z",
            "data": {
                "id": p_txn,
                "status": "completed",
                "origin": "web",
                "currency_code": "USD",
                "subscription_id": sub_id,
                "invoice_id": format!("inv_pg_m1_{nonce}"),
                "custom_data": { "safi_order_id": p_oid },
                "billing_period": { "starts_at": "2026-11-23T12:10:00Z", "ends_at": "2026-12-23T12:10:00Z" },
                "items": [{ "price": { "id": "pri_pro_monthly" } }],
                "details": { "totals": { "total": "2000", "currency_code": "USD" } }
            }
        }),
    )
    .await;
    assert_eq!(st1, StatusCode::OK, "m1: {b1}");
    assert_eq!(b1["outcome"], json!("subscription_active"));

    // Renewal Month 2 on Instance B (NO custom_data.safi_order_id — correlated strictly via `subscription_id`!)
    let (st2, b2) = send_paddle_wh(
        router_b.clone(),
        json!({
            "event_id": format!("evt_pg_m2_{nonce}"),
            "event_type": "transaction.completed",
            "occurred_at": "2026-12-23T12:10:05.000000Z",
            "data": {
                "id": format!("txn_pg_m2_{nonce}"),
                "status": "completed",
                "origin": "subscription_recurring",
                "currency_code": "USD",
                "subscription_id": sub_id,
                "invoice_id": format!("inv_pg_m2_{nonce}"),
                "billing_period": { "starts_at": "2026-12-23T12:10:00Z", "ends_at": "2027-01-23T12:10:00Z" },
                "items": [{ "price": { "id": "pri_pro_monthly" } }],
                "details": { "totals": { "total": "2000", "currency_code": "USD" } }
            }
        }),
    )
    .await;
    assert_eq!(st2, StatusCode::OK, "m2: {b2}");
    assert_eq!(b2["outcome"], json!("subscription_renewed"));

    // Renewal Month 3 on Instance A
    let (st3, b3) = send_paddle_wh(
        router_a.clone(),
        json!({
            "event_id": format!("evt_pg_m3_{nonce}"),
            "event_type": "transaction.completed",
            "occurred_at": "2027-01-23T12:10:05.000000Z",
            "data": {
                "id": format!("txn_pg_m3_{nonce}"),
                "status": "completed",
                "origin": "subscription_recurring",
                "currency_code": "USD",
                "subscription_id": sub_id,
                "invoice_id": format!("inv_pg_m3_{nonce}"),
                "billing_period": { "starts_at": "2027-01-23T12:10:00Z", "ends_at": "2027-02-23T12:10:00Z" },
                "items": [{ "price": { "id": "pri_pro_monthly" } }],
                "details": { "totals": { "total": "2000", "currency_code": "USD" } }
            }
        }),
    )
    .await;
    assert_eq!(st3, StatusCode::OK, "m3: {b3}");
    assert_eq!(b3["outcome"], json!("subscription_renewed"));

    // 8. Cold Server Restart (Brand-New Instance C) -> verify ALL durable state directly from PostgreSQL 18!
    let (state_c, router_c) = build_instance();
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
    let invs = inv_val.as_array().expect("invoices array");
    // 1 Paymob credit pack invoice (partially refunded 10_000) + 3 Paddle subscription invoices (M1, M2, M3)!
    assert_eq!(
        invs.len(),
        4,
        "expected 4 durable invoices after restart: {inv_val}"
    );
    let paymob_inv = invs
        .iter()
        .find(|i| i["provider"] == json!("paymob"))
        .expect("paymob inv");
    assert_eq!(paymob_inv["refunded_amount_minor"], json!(10_000));
    assert_eq!(paymob_inv["status"], json!("partially_refunded"));

    let pg_c = state_c.pg_store.as_ref().expect("pg");
    let ent_c = pg_c
        .load_workspace_entitlement_pg(ws_id)
        .await
        .expect("load ent")
        .expect("ent exists");
    assert_eq!(ent_c.operation_credits, 25);
    assert_eq!(ent_c.plan, PlanTier::IndividualMonthly);
    assert_eq!(
        pg_c.count_workspace_grants_pg(ws_id).await.expect("grants"),
        4
    );
    assert!(pg_c.count_security_anomalies_pg().await.expect("anom") >= 1);
    assert!(mock.paymob_create_calls.load(Ordering::SeqCst) >= 1);
}
