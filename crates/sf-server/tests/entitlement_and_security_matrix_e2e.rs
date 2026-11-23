//! Comprehensive Entitlement Matrix (16 States), Free Plan Isolation, Unresolved Payment Guard,
//! General Security Regression Suite, and Live PostgreSQL 18 Multi-Instance & Cold-Restart E2E.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use axum::routing::post;
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

use sf_server::auth::cache::InMemoryAuthCache;
use sf_server::auth::repository::{InMemoryApiKeyRepository, InMemorySessionRepository};
use sf_server::auth::service::AuthService;
use sf_server::auth::{unix_now, AuthMethod, Principal, Scope};
use sf_server::billing::{BillingService, CheckoutProduct, CheckoutRequest};
use sf_server::config::ServerConfig;
use sf_server::entitlements::{
    ConsumeUsageRequest, EntitlementService, PlanTier, SubscriptionState, UsageKind,
    BILLING_GRACE_PERIOD_SECS, TRIAL_DURATION_SECS,
};
use sf_server::error::AuthError;
use sf_server::pg_store::PgControlPlaneStore;
use sf_server::{app, AppState};

async fn make_test_server() -> (AppState, Router, String, Uuid, Uuid) {
    let sessions = Arc::new(InMemorySessionRepository::new());
    let keys = Arc::new(InMemoryApiKeyRepository::new(Vec::new()).await);
    let cache = Arc::new(InMemoryAuthCache::new());
    let auth = AuthService::new(keys, sessions, cache);
    let state = AppState::with_config(auth, ServerConfig::for_tests());

    let user_id = Uuid::now_v7();
    let workspace_id = Uuid::now_v7();
    let principal = Principal {
        user_id,
        workspace_id,
        scopes: vec![Scope::Read, Scope::Write],
        auth_method: AuthMethod::Password,
    };
    let issued = state
        .auth
        .create_session(&principal, "Test Device")
        .await
        .expect("issue session");
    let router = app(state.clone());
    (state, router, issued.token, user_id, workspace_id)
}

async fn create_session_for(state: &AppState, user_id: Uuid, workspace_id: Uuid) -> String {
    let principal = Principal {
        user_id,
        workspace_id,
        scopes: vec![Scope::Read, Scope::Write],
        auth_method: AuthMethod::Password,
    };
    state
        .auth
        .create_session(&principal, "Secondary Device")
        .await
        .expect("session")
        .token
}

async fn send_json(
    router: &Router,
    method: Method,
    uri: &str,
    bearer: Option<&str>,
    extra_headers: &[(&str, &str)],
    body: Option<Value>,
) -> (StatusCode, axum::http::HeaderMap, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(tok) = bearer {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {tok}"));
    }
    for (k, v) in extra_headers {
        builder = builder.header(*k, *v);
    }
    let req = if let Some(json_val) = body {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
        builder
            .body(Body::from(serde_json::to_vec(&json_val).expect("encode")))
            .expect("request")
    } else {
        builder.body(Body::empty()).expect("request")
    };

    let resp = router.clone().oneshot(req).await.expect("response");
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = to_bytes(resp.into_body(), 1024 * 1024).await.expect("body");
    let parsed: Value = serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({}));
    (status, headers, parsed)
}

#[tokio::test]
async fn complete_16_state_entitlement_matrix_and_free_isolation_enforced() {
    let (state, router, token, user_id, workspace_id) = make_test_server().await;
    let now = unix_now();

    // =========================================================================
    // STATE 1: Free — strict 5 ops, 5,000 rows, 20 cols, watermark, NO AI/Teams/Saved/Premium
    // =========================================================================
    let (st, _hdr, ent) = send_json(
        &router,
        Method::GET,
        "/v1/entitlements/me",
        Some(&token),
        &[],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(ent["effective_plan"], "free");
    assert_eq!(ent["watermark_required"], true);
    assert_eq!(ent["operations_limit_per_month"], 5);
    assert_eq!(ent["max_rows_per_file"], 5000);
    assert_eq!(ent["max_columns_per_file"], 20);
    assert_eq!(ent["ai_enabled"], false);
    assert_eq!(ent["teams_enabled"], false);
    assert_eq!(ent["saved_recipes_write_enabled"], false);
    assert_eq!(ent["saved_recipes_read_only"], true);
    assert_eq!(ent["premium_recipe_packs_enabled"], false);

    // Direct API calls to paid-only endpoints MUST be rejected for Free accounts!
    let (st, _, body) = send_json(
        &router,
        Method::POST,
        "/v1/ai/route",
        Some(&token),
        &[],
        Some(json!({
            "idempotency_key": "free-ai-direct",
            "explicit_user_consent": true,
            "pii_masked_locally": true,
            "masked_prompt": "Summarize revenue"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "SF-LIC-008");

    let (st, _, body) = send_json(
        &router,
        Method::POST,
        "/v1/organizations",
        Some(&token),
        &[],
        Some(json!({ "name": "Free Team Bypass", "seats": 3 })),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "SF-LIC-008");

    let (st, _, body) = send_json(
        &router,
        Method::POST,
        "/v1/sync/envelopes",
        Some(&token),
        &[],
        Some(json!({
            "resource_kind": "recipe",
            "resource_id": "saved-recipe-1",
            "expected_version": 0,
            "ciphertext_b64": "YWJjZGVmZ2hpamtsbW5vcHFyc3R1dnd4eXo=",
            "nonce_b64": "MTIzNDU2Nzg5MDEy",
            "key_id": "k1",
            "content_hash": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "SF-LIC-008");

    let (st, _, body) = send_json(
        &router,
        Method::POST,
        "/v1/templates/entitlement/export-import",
        Some(&token),
        &[],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "SF-RCP-005");

    // 5001 rows rejected (`SF-LIC-006`) and 21 cols rejected (`SF-LIC-006`)
    let (st, _, body) = send_json(
        &router,
        Method::POST,
        "/v1/entitlements/consume",
        Some(&token),
        &[],
        Some(json!({
            "idempotency_key": "free-5001-rows",
            "kind": "data_operation",
            "row_count": 5001,
            "column_count": 10,
            "pii_masked_count": 0
        })),
    )
    .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "SF-LIC-006");

    let (st, _, body) = send_json(
        &router,
        Method::POST,
        "/v1/entitlements/consume",
        Some(&token),
        &[],
        Some(json!({
            "idempotency_key": "free-21-cols",
            "kind": "data_operation",
            "row_count": 100,
            "column_count": 21,
            "pii_masked_count": 0
        })),
    )
    .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["code"], "SF-LIC-006");

    // Exactly 5 ops succeed (5000 rows, 20 cols) with mandatory watermark
    for i in 1..=5 {
        let (st, _, res) = send_json(
            &router,
            Method::POST,
            "/v1/entitlements/consume",
            Some(&token),
            &[],
            Some(json!({
                "idempotency_key": format!("free-op-{i}"),
                "kind": "data_operation",
                "row_count": 5000,
                "column_count": 20,
                "pii_masked_count": 2
            })),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(res["watermark_required"], true);
    }

    // Reusing `free-op-1` for AiCall or different dataset size MUST fail with 409 SF-PAY-007!
    let (st, _, body) = send_json(
        &router,
        Method::POST,
        "/v1/entitlements/consume",
        Some(&token),
        &[],
        Some(json!({
            "idempotency_key": "free-op-1",
            "kind": "ai_call",
            "row_count": 0,
            "column_count": 0,
            "pii_masked_count": 0
        })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert_eq!(body["code"], "SF-PAY-007");

    let (st, _, body) = send_json(
        &router,
        Method::POST,
        "/v1/entitlements/consume",
        Some(&token),
        &[],
        Some(json!({
            "idempotency_key": "free-op-1",
            "kind": "data_operation",
            "row_count": 99999,
            "column_count": 50,
            "pii_masked_count": 0
        })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert_eq!(body["code"], "SF-PAY-007");

    // 6th op rejected (`429 SF-LIC-005`)
    let (st, _, body) = send_json(
        &router,
        Method::POST,
        "/v1/entitlements/consume",
        Some(&token),
        &[],
        Some(json!({
            "idempotency_key": "free-op-6",
            "kind": "data_operation",
            "row_count": 100,
            "column_count": 5,
            "pii_masked_count": 0
        })),
    )
    .await;
    assert_eq!(st, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body["code"], "SF-LIC-005");

    // =========================================================================
    // STATE 13 & 14: Credit Pack 25 & Pay-Per-File on Free account
    // =========================================================================
    state
        .entitlements
        .grant_micro_purchase(workspace_id, user_id, 25, 1)
        .await;
    // Credit Pack covers op 6 without watermark
    let (st, _, res) = send_json(
        &router,
        Method::POST,
        "/v1/entitlements/consume",
        Some(&token),
        &[],
        Some(json!({
            "idempotency_key": "free-credit-op",
            "kind": "data_operation",
            "row_count": 500,
            "column_count": 10,
            "pii_masked_count": 0
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(res["debited_credit"], true);
    assert_eq!(res["watermark_required"], false);

    // Pay-Per-File unlocks 100,000 rows without watermark, still rejects 100,001 rows & AI!
    let (st, _, res) = send_json(
        &router,
        Method::POST,
        "/v1/entitlements/consume",
        Some(&token),
        &[],
        Some(json!({
            "idempotency_key": "free-ppf-100k",
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

    let (st, _, body) = send_json(
        &router,
        Method::POST,
        "/v1/ai/route",
        Some(&token),
        &[],
        Some(json!({
            "idempotency_key": "credit-pack-no-ai",
            "explicit_user_consent": true,
            "pii_masked_locally": true,
            "masked_prompt": "Test"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "SF-LIC-008");

    // =========================================================================
    // STATE 2 & 3: Trial active -> Trial expired
    // =========================================================================
    let trial = state
        .entitlements
        .claim_trial_at(workspace_id, user_id, "em-1", "ph-1", "dev-1", now)
        .await
        .expect("claim trial");
    assert_eq!(trial.effective_plan, PlanTier::Trial);
    assert_eq!(trial.ai_calls_limit, 25);
    assert!(!trial.watermark_required);
    assert!(trial.saved_recipes_write_enabled);
    assert!(trial.teams_enabled);

    let expired_trial = state
        .entitlements
        .effective_at(workspace_id, user_id, false, now + TRIAL_DURATION_SECS + 10)
        .await;
    assert_eq!(expired_trial.effective_plan, PlanTier::Free);
    assert_eq!(expired_trial.state, SubscriptionState::Expired);
    assert!(expired_trial.watermark_required);
    assert!(!expired_trial.ai_enabled);
    assert!(!expired_trial.saved_recipes_write_enabled);

    // =========================================================================
    // STATE 4 & 5: Individual Monthly & Individual Annual
    // =========================================================================
    let ind_m = state
        .entitlements
        .apply_subscription_update(
            workspace_id,
            user_id,
            PlanTier::IndividualMonthly,
            SubscriptionState::Active,
            1,
            Some(now + 30 * 86_400),
            now,
        )
        .await
        .expect("ind monthly");
    assert_eq!(ind_m.effective_plan, PlanTier::IndividualMonthly);
    assert_eq!(ind_m.ai_calls_limit, 400);
    assert!(!ind_m.watermark_required);
    assert!(ind_m.saved_recipes_write_enabled);
    assert!(ind_m.premium_recipe_packs_enabled);
    assert!(!ind_m.teams_enabled);

    // Individual cannot create Team Organization (`403 SF-LIC-008`), CAN push saved recipe & exchange premium template!
    let (st, _, _) = send_json(
        &router,
        Method::POST,
        "/v1/organizations",
        Some(&token),
        &[],
        Some(json!({ "name": "Ind Blocked Team", "seats": 3 })),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    let (st, _, _) = send_json(
        &router,
        Method::POST,
        "/v1/templates/entitlement/export-import",
        Some(&token),
        &[],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    let ind_a = state
        .entitlements
        .apply_subscription_update(
            workspace_id,
            user_id,
            PlanTier::IndividualAnnual,
            SubscriptionState::Active,
            1,
            Some(now + 365 * 86_400),
            now,
        )
        .await
        .expect("ind annual");
    assert_eq!(ind_a.effective_plan, PlanTier::IndividualAnnual);
    assert_eq!(ind_a.ai_calls_limit, 400);

    // =========================================================================
    // STATE 6: Organization (min 3 seats -> 5 seats = 2,000 AI calls/mo + teams_enabled)
    // =========================================================================
    assert_eq!(
        state
            .entitlements
            .apply_subscription_update(
                workspace_id,
                user_id,
                PlanTier::OrganizationSeat,
                SubscriptionState::Active,
                2,
                Some(now + 365 * 86_400),
                now,
            )
            .await
            .unwrap_err(),
        AuthError::OrgMinSeatsRequired
    );
    let org = state
        .entitlements
        .apply_subscription_update(
            workspace_id,
            user_id,
            PlanTier::OrganizationSeat,
            SubscriptionState::Active,
            5,
            Some(now + 365 * 86_400),
            now,
        )
        .await
        .expect("org active");
    assert_eq!(org.effective_plan, PlanTier::OrganizationSeat);
    assert_eq!(org.ai_calls_limit, 2000);
    assert!(org.teams_enabled);

    // =========================================================================
    // STATE 7 & 8: Past Due within 3-day grace -> Past Due after grace
    // =========================================================================
    let past_due = state
        .entitlements
        .apply_subscription_update(
            workspace_id,
            user_id,
            PlanTier::IndividualMonthly,
            SubscriptionState::PastDue,
            1,
            Some(now + 30 * 86_400),
            now,
        )
        .await
        .expect("past due");
    assert_eq!(past_due.effective_plan, PlanTier::IndividualMonthly);
    assert!(past_due.show_past_due_banner);
    assert!(past_due.ai_enabled);

    let after_grace = state
        .entitlements
        .effective_at(
            workspace_id,
            user_id,
            false,
            now + BILLING_GRACE_PERIOD_SECS + 1,
        )
        .await;
    assert_eq!(after_grace.effective_plan, PlanTier::Free);
    assert_eq!(after_grace.state, SubscriptionState::Expired);
    assert!(!after_grace.ai_enabled);

    // =========================================================================
    // STATE 9 & 10: Canceled before period end -> Canceled after period end
    // =========================================================================
    let canceled = state
        .entitlements
        .apply_subscription_update(
            workspace_id,
            user_id,
            PlanTier::IndividualMonthly,
            SubscriptionState::Canceled,
            1,
            Some(now + 86_400),
            now,
        )
        .await
        .expect("canceled");
    assert_eq!(canceled.effective_plan, PlanTier::IndividualMonthly);
    assert_eq!(canceled.state, SubscriptionState::Canceled);

    let after_cancel = state
        .entitlements
        .effective_at(workspace_id, user_id, false, now + 86_400 + 1)
        .await;
    assert_eq!(after_cancel.effective_plan, PlanTier::Free);
    assert_eq!(after_cancel.state, SubscriptionState::Expired);

    // =========================================================================
    // STATE 11 & 12: Friend VIP granted -> Friend VIP revoked
    // =========================================================================
    let owner_id = Uuid::now_v7();
    let vip = state
        .entitlements
        .set_friend_vip_checked(workspace_id, user_id, owner_id, true)
        .await
        .expect("vip grant");
    assert_eq!(vip.effective_plan, PlanTier::FriendVip);
    assert_eq!(vip.ai_calls_limit, 400);
    assert!(!vip.watermark_required);
    assert!(vip.teams_enabled);

    let vip_revoked = state
        .entitlements
        .set_friend_vip_checked(workspace_id, user_id, owner_id, false)
        .await
        .expect("vip revoke");
    assert_eq!(vip_revoked.effective_plan, PlanTier::Free);

    // =========================================================================
    // STATE 15 & 16: Full Refund & Chargeback -> immediate revocation back to Free
    // =========================================================================
    state
        .entitlements
        .apply_subscription_update(
            workspace_id,
            user_id,
            PlanTier::IndividualMonthly,
            SubscriptionState::Active,
            1,
            Some(now + 30 * 86_400),
            now,
        )
        .await
        .expect("active");
    let refunded = state
        .entitlements
        .apply_subscription_update(
            workspace_id,
            user_id,
            PlanTier::Free,
            SubscriptionState::Canceled,
            1,
            Some(now),
            now,
        )
        .await
        .expect("refunded");
    assert_eq!(refunded.effective_plan, PlanTier::Free);
    assert!(!refunded.ai_enabled);

    let chargeback = state
        .entitlements
        .apply_subscription_update(
            workspace_id,
            user_id,
            PlanTier::Free,
            SubscriptionState::Expired,
            1,
            Some(now),
            now,
        )
        .await
        .expect("chargeback");
    assert_eq!(chargeback.effective_plan, PlanTier::Free);
    assert!(chargeback.watermark_required);
}

#[tokio::test]
async fn unresolved_reconciliation_blocks_different_idempotency_key_duplicate_charge() {
    let provider_calls = Arc::new(AtomicUsize::new(0));
    let counter_clone = provider_calls.clone();

    let slow_provider = Router::new().route(
        "/transactions",
        post(move || {
            let calls = counter_clone.clone();
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                (
                    StatusCode::CREATED,
                    axum::Json(json!({ "data": { "id": "txn_slow_1" } })),
                )
            }
        }),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, slow_provider).await;
    });

    let mut cfg = ServerConfig::for_tests();
    cfg.paddle_api_base_url = format!("http://{addr}");
    cfg.paddle_api_key = Some("pdl_test_key".to_owned());
    cfg.paddle_webhook_secret = Some("pdl_test_whsec".to_owned());
    cfg.paddle_price_individual_monthly = Some("pri_monthly_usd".to_owned());

    let ents = EntitlementService::new();
    let billing = BillingService::from_config(&cfg, ents).with_provider_timeout_ms(80);
    let ws = Uuid::now_v7();
    let user = Uuid::now_v7();

    // Attempt 1 times out -> enters `ReconciliationRequired`
    let err1 = billing
        .create_checkout(&CheckoutRequest {
            workspace_id: ws,
            user_id: user,
            product: CheckoutProduct::IndividualMonthly,
            billing_country_iso2: "US".to_owned(),
            seats: None,
            coupon_code: None,
            idempotency_key: Some("attempt-key-1".to_owned()),
            billing_data: None,
            paddle_customer: None,
        })
        .await
        .unwrap_err();
    assert_eq!(err1, AuthError::PaymentReconciliationRequired);
    assert_eq!(provider_calls.load(Ordering::SeqCst), 1);

    // Attempt 2 sends a DIFFERENT idempotency key for the SAME workspace + product ->
    // MUST fail closed with `SF-PAY-008` and make ZERO second provider call!
    let err2 = billing
        .create_checkout(&CheckoutRequest {
            workspace_id: ws,
            user_id: user,
            product: CheckoutProduct::IndividualMonthly,
            billing_country_iso2: "US".to_owned(),
            seats: None,
            coupon_code: None,
            idempotency_key: Some("attempt-key-2-brand-new".to_owned()),
            billing_data: None,
            paddle_customer: None,
        })
        .await
        .unwrap_err();
    assert_eq!(err2, AuthError::PaymentReconciliationRequired);
    assert_eq!(
        provider_calls.load(Ordering::SeqCst),
        1,
        "must never issue a second provider charge while a prior attempt is unresolved"
    );
}

#[tokio::test]
async fn general_security_regression_headers_csrf_cors_idor_rbac_and_fail_closed() {
    let (state, router, token_a, user_a, ws_a) = make_test_server().await;
    let now = unix_now();

    // 1. Security headers present on responses (including HSTS, CSP, X-Frame-Options, nosniff, no-store)
    let (st, headers, _) = send_json(&router, Method::GET, "/health", None, &[], None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(
        headers
            .get("x-content-type-options")
            .and_then(|v| v.to_str().ok()),
        Some("nosniff")
    );
    assert_eq!(
        headers.get("x-frame-options").and_then(|v| v.to_str().ok()),
        Some("DENY")
    );
    assert_eq!(
        headers.get("referrer-policy").and_then(|v| v.to_str().ok()),
        Some("no-referrer")
    );
    assert!(headers
        .get("strict-transport-security")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .contains("max-age=63072000"));
    assert!(headers
        .get("content-security-policy")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .contains("default-src 'none'"));
    assert_eq!(
        headers.get("cache-control").and_then(|v| v.to_str().ok()),
        Some("no-store")
    );

    // 2. Unauthorized access -> 401
    let (st, _, _) = send_json(&router, Method::GET, "/v1/entitlements/me", None, &[], None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);

    // 3. Regular user attempting Owner endpoint -> 403 SF-AUTH-013
    let (st, _, body) = send_json(
        &router,
        Method::GET,
        "/v1/owner/stats",
        Some(&token_a),
        &[],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "SF-AUTH-013");

    // 4. Cross-site CSRF Origin rejected with 403 on state-changing POST
    let (st, _, _) = send_json(
        &router,
        Method::POST,
        "/v1/entitlements/consume",
        Some(&token_a),
        &[("origin", "https://evil.example")],
        Some(json!({
            "idempotency_key": "csrf-blocked",
            "kind": "data_operation",
            "row_count": 10,
            "column_count": 2,
            "pii_masked_count": 0
        })),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    let (st, _, _) = send_json(
        &router,
        Method::POST,
        "/v1/entitlements/consume",
        Some(&token_a),
        &[("sec-fetch-site", "cross-site")],
        Some(json!({
            "idempotency_key": "csrf-fetch-site-blocked",
            "kind": "data_operation",
            "row_count": 10,
            "column_count": 2,
            "pii_masked_count": 0
        })),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // 5. Foreign CORS preflight rejected with 403 & no ACAO header
    let (st, headers, _) = send_json(
        &router,
        Method::OPTIONS,
        "/v1/entitlements/me",
        None,
        &[
            ("origin", "https://evil.example"),
            ("access-control-request-method", "GET"),
        ],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert!(headers.get("access-control-allow-origin").is_none());

    // 6. IDOR protection on `GET /v1/organizations/{org_id}/members`
    state
        .entitlements
        .apply_subscription_update(
            ws_a,
            user_a,
            PlanTier::OrganizationSeat,
            SubscriptionState::Active,
            3,
            Some(now + 365 * 86_400),
            now,
        )
        .await
        .expect("org plan");

    let (st, _, org) = send_json(
        &router,
        Method::POST,
        "/v1/organizations",
        Some(&token_a),
        &[],
        Some(json!({ "name": "Tenant A Org", "seats": 3 })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    let org_id = org["id"].as_str().expect("org id");

    // Member (User A) CAN list members
    let (st, _, members) = send_json(
        &router,
        Method::GET,
        &format!("/v1/organizations/{org_id}/members"),
        Some(&token_a),
        &[],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(members.as_array().expect("arr").len(), 1);

    // Non-member (User B from another workspace) MUST receive 403 OrgPermissionDenied (`SF-SYN-005`)!
    let token_b = create_session_for(&state, Uuid::now_v7(), Uuid::now_v7()).await;
    let (st, _, body) = send_json(
        &router,
        Method::GET,
        &format!("/v1/organizations/{org_id}/members"),
        Some(&token_b),
        &[],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert_eq!(body["code"], "SF-SYN-005");

    // 7. Strict/production mode MUST fail closed (`BackendUnavailable`) if `pg_store` is absent!
    let strict_ent = EntitlementService::new().with_require_durable_pg(true);
    assert_eq!(
        strict_ent
            .effective_at_checked(ws_a, user_a, false, now)
            .await
            .unwrap_err(),
        AuthError::BackendUnavailable
    );
    assert_eq!(
        strict_ent
            .consume_at(
                ws_a,
                user_a,
                false,
                &ConsumeUsageRequest {
                    idempotency_key: "strict-no-pg".to_owned(),
                    kind: UsageKind::DataOperation,
                    row_count: 10,
                    column_count: 2,
                    pii_masked_count: 0,
                },
                now,
            )
            .await
            .unwrap_err(),
        AuthError::BackendUnavailable
    );
}

#[tokio::test]
#[ignore = "Requires live PostgreSQL 18 via SF_TEST_DATABASE_URL with migrations 0001..0015 applied"]
async fn postgres_18_durable_entitlements_multi_instance_concurrency_and_restart() {
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

    let pg_a = PgControlPlaneStore::new(pool_a.clone());
    let pg_b = PgControlPlaneStore::new(pool_b);
    let pg_c = PgControlPlaneStore::new(pool_c);

    let inst_a = EntitlementService::new()
        .with_require_durable_pg(true)
        .with_pg_store(pg_a);
    let inst_b = EntitlementService::new()
        .with_require_durable_pg(true)
        .with_pg_store(pg_b);
    let inst_c_after_restart = EntitlementService::new()
        .with_require_durable_pg(true)
        .with_pg_store(pg_c);

    let ws = Uuid::now_v7();
    let user = Uuid::now_v7();
    let ws_trial = Uuid::now_v7();
    let u_trial = Uuid::now_v7();
    let ws_dup = Uuid::now_v7();
    let u_dup = Uuid::now_v7();
    let owner_id = Uuid::now_v7();
    let now: i64 = 1_794_700_800; // 2026-11-15

    // Seed real PostgreSQL `users` and `workspaces` rows so all FK constraints are genuinely verified!
    for (uid, wid, label) in [
        (user, ws, "free-race"),
        (u_trial, ws_trial, "trial-user"),
        (u_dup, ws_dup, "dup-user"),
        (owner_id, Uuid::now_v7(), "owner-actor"),
    ] {
        sqlx::query("INSERT INTO users (id, email, display_name) VALUES ($1, $2, $3)")
            .bind(uid)
            .bind(format!("{label}-{uid}@example.test"))
            .bind(label)
            .execute(&pool_a)
            .await
            .expect("seed user");
        sqlx::query("INSERT INTO workspaces (id, name, plan_code) VALUES ($1, $2, 'free')")
            .bind(wid)
            .bind(format!("WS {label}"))
            .execute(&pool_a)
            .await
            .expect("seed workspace");
    }

    // 1. 20 concurrent Free quota consumptions split across Instance A & Instance B -> EXACTLY 5 succeed!
    let mut handles = Vec::new();
    for i in 0..20 {
        let svc = if i % 2 == 0 {
            inst_a.clone()
        } else {
            inst_b.clone()
        };
        handles.push(tokio::spawn(async move {
            svc.consume_at(
                ws,
                user,
                false,
                &ConsumeUsageRequest {
                    idempotency_key: format!("pg-race-{ws}-{i}"),
                    kind: UsageKind::DataOperation,
                    row_count: 1000,
                    column_count: 10,
                    pii_masked_count: 5,
                },
                now,
            )
            .await
        }));
    }
    let mut ok_keys = Vec::new();
    let mut quota_exceeded = 0usize;
    for (idx, h) in handles.into_iter().enumerate() {
        match h.await.expect("join") {
            Ok(res) => {
                assert!(res.watermark_required);
                ok_keys.push(format!("pg-race-{ws}-{idx}"));
            }
            Err(AuthError::QuotaExceeded) => quota_exceeded += 1,
            Err(other) => panic!("unexpected PG error: {other:?}"),
        }
    }
    assert_eq!(ok_keys.len(), 5, "exact 5 Free ops across two instances");
    assert_eq!(quota_exceeded, 15, "remaining 15 rejected with SF-LIC-005");

    // 2. Cross-instance idempotency fingerprint check: reusing Instance A's key on Instance B
    //    with AiCall or different parameters MUST return 409 IdempotencyConflict (`SF-PAY-007`)!
    let valid_key = ok_keys[0].clone();
    let replay = inst_b
        .consume_at(
            ws,
            user,
            false,
            &ConsumeUsageRequest {
                idempotency_key: valid_key.clone(),
                kind: UsageKind::DataOperation,
                row_count: 1000,
                column_count: 10,
                pii_masked_count: 5,
            },
            now,
        )
        .await
        .expect("identical replay succeeds");
    assert!(replay.duplicate_idempotency_hit);

    assert_eq!(
        inst_b
            .consume_at(
                ws,
                user,
                false,
                &ConsumeUsageRequest {
                    idempotency_key: valid_key.clone(),
                    kind: UsageKind::AiCall,
                    row_count: 0,
                    column_count: 0,
                    pii_masked_count: 0,
                },
                now,
            )
            .await
            .unwrap_err(),
        AuthError::IdempotencyConflict
    );

    // 3. Cold Restart (brand-new Instance C with zero warm in-memory state):
    //    Usage MUST still be 5/5 and 6th op MUST still fail!
    let after_restart = inst_c_after_restart
        .effective_at_checked(ws, user, false, now)
        .await
        .expect("effective after restart");
    assert_eq!(after_restart.operations_used_this_month, 5);
    assert_eq!(
        inst_c_after_restart
            .consume_at(
                ws,
                user,
                false,
                &ConsumeUsageRequest {
                    idempotency_key: format!("pg-after-restart-6-{ws}"),
                    kind: UsageKind::DataOperation,
                    row_count: 10,
                    column_count: 2,
                    pii_masked_count: 0,
                },
                now,
            )
            .await
            .unwrap_err(),
        AuthError::QuotaExceeded
    );

    // 4. Durable Trial anti-abuse & Friend VIP grant/revoke survive across instances & restart!
    let email_h = format!("e-{ws_trial}");
    let phone_h = format!("p-{ws_trial}");
    let dev_h = format!("d-{ws_trial}");
    let claimed = inst_a
        .claim_trial_at(ws_trial, u_trial, &email_h, &phone_h, &dev_h, now)
        .await
        .expect("trial claimed in PG");
    assert_eq!(claimed.effective_plan, PlanTier::Trial);

    // Duplicate device claim on Instance C after restart MUST be rejected!
    assert_eq!(
        inst_c_after_restart
            .claim_trial_at(ws_dup, u_dup, "e-other", "p-other", &dev_h, now)
            .await
            .unwrap_err(),
        AuthError::DuplicateAccount
    );

    // Expired trial persists downgrade in PostgreSQL 18 on Instance C!
    let expired = inst_c_after_restart
        .effective_at_checked(ws_trial, u_trial, false, now + TRIAL_DURATION_SECS + 10)
        .await
        .expect("expired trial");
    assert_eq!(expired.effective_plan, PlanTier::Free);
    assert_eq!(expired.state, SubscriptionState::Expired);

    // Friend VIP granted on Instance A is immediately visible on Instance C, and revocable!
    inst_a
        .set_friend_vip_checked(ws_trial, u_trial, owner_id, true)
        .await
        .expect("grant vip");
    let vip_c = inst_c_after_restart
        .effective_at_checked(ws_trial, u_trial, false, now + TRIAL_DURATION_SECS + 20)
        .await
        .expect("vip on C");
    assert_eq!(vip_c.effective_plan, PlanTier::FriendVip);

    inst_b
        .set_friend_vip_checked(ws_trial, u_trial, owner_id, false)
        .await
        .expect("revoke vip");
    let revoked_c = inst_c_after_restart
        .effective_at_checked(ws_trial, u_trial, false, now + TRIAL_DURATION_SECS + 30)
        .await
        .expect("revoked on C");
    assert_eq!(revoked_c.effective_plan, PlanTier::Free);
}
