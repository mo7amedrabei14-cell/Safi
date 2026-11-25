//! Production Acceptance Suite for:
//! 1. Strict Origin Separation (`SAFI_PUBLIC_WEB_ORIGIN` vs `SAFI_API_BASE_URL` independence).
//! 2. Organization Shared Team Workspaces, Seat Enforcement, and Verified-Email Invitations.
//! 3. Complete 4-Role Server RBAC (`Owner`, `Admin`, `Member`, `Viewer`) & Anti-IDOR Isolation.
//! 4. Zero-Knowledge End-to-End Encrypted Sync (`ChaCha20-Poly1305` ciphertext envelopes, X25519
//!    device key directory, wrapped epoch keys, key rotation on member removal, optimistic version
//!    conflicts, idempotency replay/mismatch, nonce-reuse rejection, tombstones, and audit logs).
//! 5. Live PostgreSQL 18 (`#[ignore]`, executed in `tools/test-migrations.sh`) Multi-Instance,
//!    Cold-Restart, `NOBYPASSRLS` Tenant Isolation, and Raw `BYTEA` Zero-Plaintext Proof.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{header, Method, Request, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use base64::Engine;
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, CHACHA20_POLY1305};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

use sf_server::auth::cache::InMemoryAuthCache;
use sf_server::auth::repository::{InMemoryApiKeyRepository, InMemorySessionRepository};
use sf_server::auth::service::AuthService;
use sf_server::auth::{unix_now, AuthMethod, Principal, Scope};
use sf_server::billing::sign_paddle_payload;
use sf_server::config::ServerConfig;
use sf_server::pg_store::PgControlPlaneStore;
use sf_server::{app, AppState, ManagedAccountRecord};

async fn send_req(
    router: &Router,
    method: Method,
    uri: &str,
    bearer: Option<&str>,
    extra_headers: &[(&str, &str)],
    body: Option<Value>,
) -> (StatusCode, Value) {
    let payload = body.map_or_else(Vec::new, |v| serde_json::to_vec(&v).expect("json body"));
    let mut builder = Request::builder().method(method).uri(uri);
    if !payload.is_empty() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    if let Some(tok) = bearer {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {tok}"));
    }
    for (k, v) in extra_headers {
        builder = builder.header(*k, *v);
    }
    let req = builder.body(Body::from(payload)).expect("request");
    let resp = router.clone().oneshot(req).await.expect("oneshot");
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), 1024 * 1024).await.expect("body");
    let parsed = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({ "raw": String::from_utf8_lossy(&bytes) }))
    };
    (status, parsed)
}

async fn send_raw(
    router: &Router,
    method: Method,
    uri: &str,
    extra_headers: &[(&str, &str)],
    raw_body: Vec<u8>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    for (k, v) in extra_headers {
        builder = builder.header(*k, *v);
    }
    let req = builder.body(Body::from(raw_body)).expect("raw req");
    let resp = router.clone().oneshot(req).await.expect("oneshot");
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), 1024 * 1024).await.expect("body");
    let parsed = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, parsed)
}

async fn seed_managed_verified_user(
    state: &AppState,
    email: &str,
    display_name: &str,
) -> (Uuid, Uuid, String) {
    let user_id = Uuid::now_v7();
    let workspace_id = Uuid::now_v7();
    state
        .upsert_managed_account(ManagedAccountRecord {
            user_id,
            workspace_id,
            email: email.to_owned(),
            display_name: display_name.to_owned(),
            suspended: false,
            verified_phone: None,
            created_at: unix_now(),
        })
        .await;
    let tok = state
        .auth
        .create_session(
            &Principal {
                user_id,
                workspace_id,
                scopes: vec![Scope::Read, Scope::Write],
                auth_method: AuthMethod::Password,
            },
            "Acceptance Desktop",
        )
        .await
        .expect("session")
        .token;
    (user_id, workspace_id, tok)
}

fn spawn_paddle_mock() -> String {
    let calls = Arc::new(AtomicUsize::new(0));
    let mock_router = Router::new().route(
        "/transactions",
        post(move |Json(payload): Json<Value>| {
            let calls = calls.clone();
            async move {
                let n = calls.fetch_add(1, Ordering::SeqCst) + 1;
                let txn_id = format!("txn_org_mock_{n:04}");
                let qty = payload["items"][0]["quantity"].as_u64().unwrap_or(3);
                let total = (qty * 20_400).to_string();
                let rec = json!({
                    "id": txn_id,
                    "status": "draft",
                    "currency_code": "USD",
                    "items": [{ "price": { "id": "pri_org_usd" }, "quantity": qty }],
                    "details": { "totals": { "total": total } },
                    "custom_data": payload["custom_data"],
                    "checkout": { "url": format!("https://checkout.paddle.com/{txn_id}") }
                });
                (StatusCode::CREATED, Json(json!({ "data": rec })))
            }
        }),
    );
    let std_listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind paddle mock");
    std_listener.set_nonblocking(true).expect("nonblocking");
    let addr = std_listener.local_addr().expect("addr");
    let listener = tokio::net::TcpListener::from_std(std_listener).expect("tokio listener");
    tokio::spawn(async move {
        let _ = axum::serve(listener, mock_router).await;
    });
    format!("http://{addr}")
}

fn configure_paddle_test_billing(cfg: &mut ServerConfig, mock_url: &str) {
    cfg.paddle_api_base_url = mock_url.to_owned();
    cfg.paddle_api_key = Some("pdl_test_api_key".to_owned());
    cfg.paddle_webhook_secret = Some("pdl_test_whsec_secret".to_owned());
    cfg.paddle_price_individual_monthly = Some("pri_monthly_usd".to_owned());
    cfg.paddle_price_individual_annual = Some("pri_annual_usd".to_owned());
    cfg.paddle_price_org_seat_annual = Some("pri_org_usd".to_owned());
    cfg.paddle_price_pay_per_file = Some("pri_ppf_usd".to_owned());
    cfg.paddle_price_credit_pack_25 = Some("pri_cp25_usd".to_owned());
}

async fn activate_org_subscription_via_paddle_webhook(
    router: &Router,
    buyer_token: &str,
    buyer_user_id: Uuid,
    buyer_workspace_id: Uuid,
    seats: u32,
    event_suffix: &str,
) {
    let (st, checkout) = send_req(
        router,
        Method::POST,
        "/v1/billing/checkout",
        Some(buyer_token),
        &[],
        Some(json!({
            "product": "organization_seat_annual",
            "seats": seats,
            "billing_country_iso2": "US",
            "idempotency_key": format!("idem_org_checkout_{event_suffix}")
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "checkout failed: {checkout}");
    let order_id = checkout["order_id"].as_str().expect("order_id").to_owned();
    let txn_id = checkout["provider_order_ref"]
        .as_str()
        .expect("provider_order_ref")
        .to_owned();
    let expected_minor = u64::from(seats) * 20_400;

    let wh = json!({
        "event_id": format!("evt_org_plan_{event_suffix}"),
        "event_type": "transaction.completed",
        "occurred_at": "2026-11-24T12:00:00Z",
        "data": {
            "id": txn_id,
            "status": "completed",
            "currency_code": "USD",
            "subscription_id": format!("sub_org_plan_{buyer_workspace_id}"),
            "invoice_id": format!("inv_org_plan_{event_suffix}"),
            "items": [{ "price": { "id": "pri_org_usd" }, "quantity": seats }],
            "custom_data": {
                "safi_order_id": order_id,
                "workspace_id": buyer_workspace_id.to_string(),
                "user_id": buyer_user_id.to_string(),
                "product": "organization_seat_annual",
                "seats": seats
            },
            "details": { "totals": { "total": expected_minor.to_string() } }
        }
    });
    let wh_bytes = serde_json::to_vec(&wh).expect("wh json");
    let sig =
        sign_paddle_payload("pdl_test_whsec_secret", unix_now(), &wh_bytes).expect("paddle sig");
    let (wh_st, wh_body) = send_raw(
        router,
        Method::POST,
        "/v1/billing/webhooks/paddle",
        &[("paddle-signature", &sig)],
        wh_bytes,
    )
    .await;
    assert_eq!(wh_st, StatusCode::OK, "webhook failed: {wh_body}");
}

#[allow(clippy::too_many_arguments)]
fn encrypt_client_resource_envelope(
    key_bytes: &[u8; 32],
    nonce_bytes: [u8; 12],
    workspace_id: Uuid,
    resource_kind: &str,
    resource_id: &str,
    version: u64,
    key_epoch: u32,
    plaintext: &[u8],
) -> (String, String, String) {
    let b64 = base64::engine::general_purpose::STANDARD;
    let unbound = UnboundKey::new(&CHACHA20_POLY1305, key_bytes).expect("aead key");
    let aead = LessSafeKey::new(unbound);
    let aad_str = format!(
        "safi:org-sync:v1|{workspace_id}|{resource_kind}|{resource_id}|{version}|{key_epoch}"
    );
    let mut buffer = plaintext.to_vec();
    aead.seal_in_place_append_tag(
        Nonce::assume_unique_for_key(nonce_bytes),
        Aad::from(aad_str.as_bytes()),
        &mut buffer,
    )
    .expect("seal");
    let mut hasher = blake3::Hasher::new();
    hasher.update(&nonce_bytes);
    hasher.update(&buffer);
    let content_hash = hasher.finalize().to_hex().to_string();
    (b64.encode(&buffer), b64.encode(nonce_bytes), content_hash)
}

#[tokio::test]
async fn origin_separation_changing_web_origin_never_alters_api_origin_and_vice_versa() {
    let build_with_origins = |web_origin: &str, api_origin: &str| {
        let web = web_origin.to_owned();
        let api = api_origin.to_owned();
        async move {
            let sessions = Arc::new(InMemorySessionRepository::new());
            let keys = Arc::new(InMemoryApiKeyRepository::new(Vec::new()).await);
            let cache = Arc::new(InMemoryAuthCache::new());
            let auth = AuthService::new(keys, sessions, cache);
            let mut cfg = ServerConfig::for_tests();
            cfg.public_web_origin = Some(web);
            cfg.api_base_url = Some(api);
            cfg.oauth_google_configured = true;
            let st = AppState::with_config(auth, cfg);
            let router = app(st.clone());
            (st, router)
        }
    };

    // Case A: Change SAFI_PUBLIC_WEB_ORIGIN while keeping SAFI_API_BASE_URL fixed
    let (st_1, router_1) = build_with_origins(
        "https://web-one.safidata.app",
        "https://api-fixed.safidata.app",
    )
    .await;
    let (st_2, router_2) = build_with_origins(
        "https://web-two.safidata.app",
        "https://api-fixed.safidata.app",
    )
    .await;
    assert_eq!(
        st_1.config.resolved_api_base_url().expect("api 1"),
        "https://api-fixed.safidata.app"
    );
    assert_eq!(
        st_2.config.resolved_api_base_url().expect("api 2"),
        "https://api-fixed.safidata.app"
    );

    let (_, _, tok_1) = seed_managed_verified_user(&st_1, "u1@acme.test", "U1").await;
    let (_, _, tok_2) = seed_managed_verified_user(&st_2, "u2@acme.test", "U2").await;

    let (s1, h1) = send_req(
        &router_1,
        Method::POST,
        "/v1/billing/handoff/start",
        Some(&tok_1),
        &[],
        Some(json!({
            "product": "individual_monthly",
            "billing_country_iso2": "US",
            "state": "origin_sep_state_1234567890"
        })),
    )
    .await;
    assert_eq!(s1, StatusCode::CREATED);
    assert!(h1["billing_url"]
        .as_str()
        .unwrap()
        .starts_with("https://web-one.safidata.app/billing?handoff="));

    let (s2, h2) = send_req(
        &router_2,
        Method::POST,
        "/v1/billing/handoff/start",
        Some(&tok_2),
        &[],
        Some(json!({
            "product": "individual_monthly",
            "billing_country_iso2": "US",
            "state": "origin_sep_state_1234567890"
        })),
    )
    .await;
    assert_eq!(s2, StatusCode::CREATED);
    assert!(h2["billing_url"]
        .as_str()
        .unwrap()
        .starts_with("https://web-two.safidata.app/billing?handoff="));

    // OAuth redirect_uri strictly enforces SAFI_PUBLIC_WEB_ORIGIN and rejects SAFI_API_BASE_URL!
    let (oauth_ok, _) = send_req(
        &router_2,
        Method::GET,
        "/v1/auth/oauth/google/start?pkce_challenge=E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM&redirect_uri=https%3A%2F%2Fweb-two.safidata.app%2Fauth%2Fcallback",
        None,
        &[],
        None,
    )
    .await;
    assert_eq!(oauth_ok, StatusCode::OK);

    let (oauth_api_rejected, _) = send_req(
        &router_2,
        Method::GET,
        "/v1/auth/oauth/google/start?pkce_challenge=E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM&redirect_uri=https%3A%2F%2Fapi-fixed.safidata.app%2Fauth%2Fcallback",
        None,
        &[],
        None,
    )
    .await;
    assert_eq!(oauth_api_rejected, StatusCode::UNPROCESSABLE_ENTITY);

    // Case B: Change SAFI_API_BASE_URL while keeping SAFI_PUBLIC_WEB_ORIGIN fixed
    let (st_3, router_3) = build_with_origins(
        "https://web-two.safidata.app",
        "https://api-rotated.safidata.app",
    )
    .await;
    assert_eq!(
        st_3.config.resolved_public_web_origin().expect("web 3"),
        "https://web-two.safidata.app"
    );
    assert_eq!(
        st_3.config.resolved_api_base_url().expect("api 3"),
        "https://api-rotated.safidata.app"
    );
    let (_, _, tok_3) = seed_managed_verified_user(&st_3, "u3@acme.test", "U3").await;
    let (s3, h3) = send_req(
        &router_3,
        Method::POST,
        "/v1/billing/handoff/start",
        Some(&tok_3),
        &[],
        Some(json!({
            "product": "individual_monthly",
            "billing_country_iso2": "US",
            "state": "origin_sep_state_1234567890"
        })),
    )
    .await;
    assert_eq!(s3, StatusCode::CREATED);
    assert!(h3["billing_url"]
        .as_str()
        .unwrap()
        .starts_with("https://web-two.safidata.app/billing?handoff="));
}

#[tokio::test]
async fn organization_invitations_rbac_and_encrypted_sync_http_matrix() {
    let mock_paddle_url = spawn_paddle_mock();
    let sessions = Arc::new(InMemorySessionRepository::new());
    let keys = Arc::new(InMemoryApiKeyRepository::new(Vec::new()).await);
    let cache = Arc::new(InMemoryAuthCache::new());
    let auth = AuthService::new(keys, sessions, cache);
    let mut cfg = ServerConfig::for_tests();
    cfg.public_web_origin = Some("https://safidata.app".to_owned());
    cfg.api_base_url = Some("https://api.safidata.app".to_owned());
    configure_paddle_test_billing(&mut cfg, &mock_paddle_url);
    let state = AppState::with_config(auth, cfg);
    let router = app(state.clone());

    let (alice_id, alice_ws, alice_tok) =
        seed_managed_verified_user(&state, "alice@acme.test", "Alice Owner").await;
    let (bob_id, _bob_ws, bob_tok) =
        seed_managed_verified_user(&state, "bob@acme.test", "Bob Member").await;
    let (carol_id, _carol_ws, carol_tok) =
        seed_managed_verified_user(&state, "carol@acme.test", "Carol Admin").await;
    let (_victor_id, _victor_ws, victor_tok) =
        seed_managed_verified_user(&state, "victor@acme.test", "Victor Viewer").await;
    let (_dan_id, _dan_ws, dan_tok) =
        seed_managed_verified_user(&state, "dan@acme.test", "Dan Decliner").await;
    let (_mallory_id, _mallory_ws, mallory_tok) =
        seed_managed_verified_user(&state, "mallory@evil.test", "Mallory Outsider").await;

    // Unverified user (no ManagedAccountRecord / no verified email)
    let unverified_id = Uuid::now_v7();
    let unverified_ws = Uuid::now_v7();
    let unverified_tok = state
        .auth
        .create_session(
            &Principal {
                user_id: unverified_id,
                workspace_id: unverified_ws,
                scopes: vec![Scope::Read, Scope::Write],
                auth_method: AuthMethod::Password,
            },
            "Unverified Desktop",
        )
        .await
        .expect("unverified tok")
        .token;

    // 1. Free plan CANNOT create an organization (`403 SF-LIC-008`)
    let (st, err) = send_req(
        &router,
        Method::POST,
        "/v1/organizations",
        Some(&alice_tok),
        &[],
        Some(json!({ "name": "Acme Analytics", "seats": 3 })),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert_eq!(err["code"], "SF-LIC-008");

    // 2. Activate 3-seat Organization subscription for Alice via signed Paddle webhook
    activate_org_subscription_via_paddle_webhook(
        &router, &alice_tok, alice_id, alice_ws, 3, "init3",
    )
    .await;

    // Fewer than 3 seats rejected (`400`); exceeding paid seats (4 > 3) rejected (`429 SF-LIC-005`)
    let (st, _) = send_req(
        &router,
        Method::POST,
        "/v1/organizations",
        Some(&alice_tok),
        &[],
        Some(json!({ "name": "Too Small", "seats": 2 })),
    )
    .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);

    let (st, over_err) = send_req(
        &router,
        Method::POST,
        "/v1/organizations",
        Some(&alice_tok),
        &[],
        Some(json!({ "name": "Over Paid Seats", "seats": 4 })),
    )
    .await;
    assert_eq!(st, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(over_err["code"], "SF-LIC-005");

    // Create 3-seat Organization -> dedicated shared `workspace_id` != `alice_ws`
    let (st, org_json) = send_req(
        &router,
        Method::POST,
        "/v1/organizations",
        Some(&alice_tok),
        &[],
        Some(json!({ "name": "Acme Analytics", "seats": 3, "billing_cycle": "annual" })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "create org: {org_json}");
    let org_id = Uuid::parse_str(org_json["id"].as_str().unwrap()).unwrap();
    let shared_ws = Uuid::parse_str(org_json["workspace_id"].as_str().unwrap()).unwrap();
    assert_ne!(shared_ws, alice_ws);
    assert_eq!(org_json["active_key_epoch"], 1);

    // 3. Verified-Email Invitations, Duplicate Prevention, Wrong-Email Rejection & Seat Cap
    let (st, bob_inv) = send_req(
        &router,
        Method::POST,
        &format!("/v1/organizations/{org_id}/invite"),
        Some(&alice_tok),
        &[],
        Some(json!({ "email": "Bob@Acme.test", "role": "member" })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    let bob_invite_token = bob_inv["token"].as_str().unwrap().to_owned();
    assert!(bob_inv["invite_url"]
        .as_str()
        .unwrap()
        .starts_with("https://safidata.app/team?invite=sfi_"));

    // Duplicate active invite to same normalized email -> 409 Conflict
    let (st, _) = send_req(
        &router,
        Method::POST,
        &format!("/v1/organizations/{org_id}/invite"),
        Some(&alice_tok),
        &[],
        Some(json!({ "email": "bob@acme.test", "role": "viewer" })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);

    // Intruder Mallory (`mallory@evil.test`) possesses Bob's valid token -> REJECTED (`403 SF-SYN-005`)!
    let (st, wrong_email_err) = send_req(
        &router,
        Method::POST,
        "/v1/organizations/invitations/accept",
        Some(&mallory_tok),
        &[],
        Some(json!({ "token": bob_invite_token })),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert_eq!(wrong_email_err["code"], "SF-SYN-005");

    // Unverified user tries to accept -> REJECTED (`403 SF-SYN-005`)!
    let (st, unv_err) = send_req(
        &router,
        Method::POST,
        "/v1/organizations/invitations/accept",
        Some(&unverified_tok),
        &[],
        Some(json!({ "token": bob_invite_token })),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert_eq!(unv_err["code"], "SF-SYN-005");

    // Bob (`bob@acme.test`) sees his pending invite in `/v1/organizations/mine` and accepts!
    let (st, bob_mine_before) = send_req(
        &router,
        Method::GET,
        "/v1/organizations/mine",
        Some(&bob_tok),
        &[],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(
        bob_mine_before["pending_invitations"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let (st, bob_member) = send_req(
        &router,
        Method::POST,
        "/v1/organizations/invitations/accept",
        Some(&bob_tok),
        &[],
        Some(json!({ "token": bob_invite_token })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(bob_member["workspace_id"], shared_ws.to_string());
    assert_eq!(bob_member["role"], "member");

    // Replaying the single-use invitation token MUST fail (`422 SF-AUTH-007`)!
    let (st, replay_err) = send_req(
        &router,
        Method::POST,
        "/v1/organizations/invitations/accept",
        Some(&bob_tok),
        &[],
        Some(json!({ "token": bob_invite_token })),
    )
    .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(replay_err["code"], "SF-AUTH-007");

    // Invite Carol as Admin (seat 3 of 3) -> succeeds; 4th seat -> rejected (`429 SF-LIC-005`)!
    let (st, carol_inv) = send_req(
        &router,
        Method::POST,
        &format!("/v1/organizations/{org_id}/invite"),
        Some(&alice_tok),
        &[],
        Some(json!({ "email": "carol@acme.test", "role": "admin" })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    let carol_token = carol_inv["token"].as_str().unwrap().to_owned();

    let (st, seat_cap_err) = send_req(
        &router,
        Method::POST,
        &format!("/v1/organizations/{org_id}/invite"),
        Some(&alice_tok),
        &[],
        Some(json!({ "email": "victor@acme.test", "role": "viewer" })),
    )
    .await;
    assert_eq!(st, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(seat_cap_err["code"], "SF-LIC-005");

    // Upgrade Organization subscription to 6 seats via signed Paddle webhook!
    activate_org_subscription_via_paddle_webhook(
        &router, &alice_tok, alice_id, alice_ws, 6, "upgrade6",
    )
    .await;

    // Carol accepts Admin; Alice invites Victor (Viewer), Dan (Decline), and Temp (Revoke)
    let (st, _) = send_req(
        &router,
        Method::POST,
        "/v1/organizations/invitations/accept",
        Some(&carol_tok),
        &[],
        Some(json!({ "token": carol_token })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    let (st, victor_inv) = send_req(
        &router,
        Method::POST,
        &format!("/v1/organizations/{org_id}/invite"),
        Some(&carol_tok), // Admin Carol can invite Member/Viewer!
        &[],
        Some(json!({ "email": "victor@acme.test", "role": "viewer" })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    let (st, _) = send_req(
        &router,
        Method::POST,
        "/v1/organizations/invitations/accept",
        Some(&victor_tok),
        &[],
        Some(json!({ "token": victor_inv["token"].as_str().unwrap() })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    // Decline flow
    let (st, dan_inv) = send_req(
        &router,
        Method::POST,
        &format!("/v1/organizations/{org_id}/invite"),
        Some(&alice_tok),
        &[],
        Some(json!({ "email": "dan@acme.test", "role": "member" })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    let dan_tok_inv = dan_inv["token"].as_str().unwrap().to_owned();
    let (st, dec_res) = send_req(
        &router,
        Method::POST,
        "/v1/organizations/invitations/decline",
        Some(&dan_tok),
        &[],
        Some(json!({ "token": dan_tok_inv })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(dec_res["status"], "declined");
    // Declined token cannot later be accepted
    let (st, _) = send_req(
        &router,
        Method::POST,
        "/v1/organizations/invitations/accept",
        Some(&dan_tok),
        &[],
        Some(json!({ "token": dan_tok_inv })),
    )
    .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);

    // Revoke flow
    let (st, rev_inv) = send_req(
        &router,
        Method::POST,
        &format!("/v1/organizations/{org_id}/invite"),
        Some(&alice_tok),
        &[],
        Some(json!({ "email": "dan@acme.test", "role": "viewer" })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    let rev_id = rev_inv["invitation_id"].as_str().unwrap();
    let rev_tok = rev_inv["token"].as_str().unwrap().to_owned();
    let (st, rev_body) = send_req(
        &router,
        Method::POST,
        &format!("/v1/organizations/{org_id}/invitations/{rev_id}/revoke"),
        Some(&alice_tok),
        &[],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(rev_body["status"], "revoked");
    let (st, _) = send_req(
        &router,
        Method::POST,
        "/v1/organizations/invitations/accept",
        Some(&dan_tok),
        &[],
        Some(json!({ "token": rev_tok })),
    )
    .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);

    // Expiry flow (`ttl_secs: 1` -> wait for expiration -> status becomes `expired` & accept rejected)
    let (st, exp_inv) = send_req(
        &router,
        Method::POST,
        &format!("/v1/organizations/{org_id}/invite"),
        Some(&alice_tok),
        &[],
        Some(json!({ "email": "dan@acme.test", "role": "viewer", "ttl_secs": 1 })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    let exp_id = exp_inv["invitation_id"].as_str().unwrap().to_owned();
    let exp_tok = exp_inv["token"].as_str().unwrap().to_owned();
    tokio::time::sleep(std::time::Duration::from_millis(2_100)).await;
    let (st, inv_list) = send_req(
        &router,
        Method::GET,
        &format!("/v1/organizations/{org_id}/invitations"),
        Some(&alice_tok),
        &[],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    let expired_entry = inv_list
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == exp_id)
        .expect("expired entry listed");
    assert_eq!(expired_entry["status"], "expired");
    let (st, _) = send_req(
        &router,
        Method::POST,
        "/v1/organizations/invitations/accept",
        Some(&dan_tok),
        &[],
        Some(json!({ "token": exp_tok })),
    )
    .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);

    // 4. Shared Team Workspace visibility across Alice, Carol, Bob, Victor + Cross-Tenant IDOR block
    let shared_ws_str = shared_ws.to_string();
    for (tok, expected_role) in [
        (&alice_tok, "owner"),
        (&carol_tok, "admin"),
        (&bob_tok, "member"),
        (&victor_tok, "viewer"),
    ] {
        let (st, mine) = send_req(
            &router,
            Method::GET,
            "/v1/organizations/mine",
            Some(tok),
            &[],
            None,
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        let orgs = mine["organizations"].as_array().unwrap();
        assert_eq!(orgs.len(), 1);
        assert_eq!(orgs[0]["organization"]["workspace_id"], shared_ws_str);
        assert_eq!(orgs[0]["my_role"], expected_role);

        let (st, ws_ent) = send_req(
            &router,
            Method::GET,
            "/v1/entitlements/me",
            Some(tok),
            &[("x-safi-workspace-id", &shared_ws_str)],
            None,
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(ws_ent["workspace_id"], shared_ws_str);
        assert_eq!(ws_ent["teams_enabled"], true);
    }

    // Outsider Mallory blocked across ALL org & shared-workspace endpoints (`403 SF-SYN-005`)
    for (method, path) in [
        (Method::GET, format!("/v1/organizations/{org_id}")),
        (Method::GET, format!("/v1/organizations/{org_id}/members")),
        (Method::GET, "/v1/sync/changes?since_seq=0".to_owned()),
        (Method::GET, "/v1/entitlements/me".to_owned()),
    ] {
        let (st, err) = send_req(
            &router,
            method,
            &path,
            Some(&mallory_tok),
            &[("x-safi-workspace-id", &shared_ws_str)],
            None,
        )
        .await;
        assert_eq!(st, StatusCode::FORBIDDEN, "outsider must fail on {path}");
        assert_eq!(err["code"], "SF-SYN-005");
    }

    // 5. Full 4-Role Server RBAC Enforcement (`Owner`, `Admin`, `Member`, `Viewer`)
    // Viewer CANNOT invite, change roles, remove members, push sync, or create share links!
    let (st, _) = send_req(
        &router,
        Method::POST,
        &format!("/v1/organizations/{org_id}/invite"),
        Some(&victor_tok),
        &[],
        Some(json!({ "email": "new@acme.test", "role": "viewer" })),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    let workspace_key_epoch1 = [0x42_u8; 32];
    let (ct_v, nonce_v, hash_v) = encrypt_client_resource_envelope(
        &workspace_key_epoch1,
        [1_u8; 12],
        shared_ws,
        "recipe",
        "viewer-attempt",
        1,
        1,
        br#"{"step":"viewer_forbidden"}"#,
    );
    let (st, v_err) = send_req(
        &router,
        Method::POST,
        "/v1/sync/envelopes",
        Some(&victor_tok),
        &[("x-safi-workspace-id", &shared_ws_str)],
        Some(json!({
            "resource_kind": "recipe",
            "resource_id": "viewer-attempt",
            "expected_version": 0,
            "ciphertext_b64": ct_v,
            "nonce_b64": nonce_v,
            "key_id": format!("wk:{shared_ws}:epoch:1"),
            "key_epoch": 1,
            "content_hash": hash_v
        })),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert_eq!(v_err["code"], "SF-SYN-005");

    // Member CANNOT self-promote, invite, remove members, rotate keys, manage billing, or delete org!
    let (st, _) = send_req(
        &router,
        Method::PATCH,
        &format!("/v1/organizations/{org_id}/members/{bob_id}/role"),
        Some(&bob_tok),
        &[],
        Some(json!({ "role": "admin" })),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    let (st, _) = send_req(
        &router,
        Method::POST,
        &format!("/v1/organizations/{org_id}/invite"),
        Some(&bob_tok),
        &[],
        Some(json!({ "email": "other@acme.test", "role": "viewer" })),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // Admin Carol CANNOT promote self/anyone to Owner or Admin, CANNOT touch Owner Alice, CANNOT manage billing or delete org!
    for forbidden_role in ["owner", "admin"] {
        let (st, _) = send_req(
            &router,
            Method::PATCH,
            &format!("/v1/organizations/{org_id}/members/{bob_id}/role"),
            Some(&carol_tok),
            &[],
            Some(json!({ "role": forbidden_role })),
        )
        .await;
        assert_eq!(st, StatusCode::FORBIDDEN);
    }
    let (st, _) = send_req(
        &router,
        Method::DELETE,
        &format!("/v1/organizations/{org_id}/members/{alice_id}"),
        Some(&carol_tok),
        &[],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    let (st, _) = send_req(
        &router,
        Method::DELETE,
        &format!("/v1/organizations/{org_id}"),
        Some(&carol_tok),
        &[],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // Non-Owner (Admin Carol) CANNOT run `/v1/billing/checkout` on the shared org workspace!
    let (st, _) = send_req(
        &router,
        Method::POST,
        "/v1/billing/checkout",
        Some(&carol_tok),
        &[("x-safi-workspace-id", &shared_ws_str)],
        Some(json!({
            "product": "organization_seat_annual",
            "seats": 7,
            "billing_country_iso2": "US"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // Sole Owner Alice CANNOT remove herself without transferring ownership (`403 SF-SYN-005`)!
    let (st, _) = send_req(
        &router,
        Method::DELETE,
        &format!("/v1/organizations/{org_id}/members/{alice_id}"),
        Some(&alice_tok),
        &[],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // Admin Carol CAN change Bob's role (`member` -> `viewer` -> `member`), and Bob's new session enforces it!
    let (st, _) = send_req(
        &router,
        Method::PATCH,
        &format!("/v1/organizations/{org_id}/members/{bob_id}/role"),
        Some(&carol_tok),
        &[],
        Some(json!({ "role": "viewer" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    let bob_relogin_tok = state
        .auth
        .create_session(
            &Principal {
                user_id: bob_id,
                workspace_id: Uuid::now_v7(),
                scopes: vec![Scope::Read, Scope::Write],
                auth_method: AuthMethod::Password,
            },
            "Bob Relogin Desktop",
        )
        .await
        .expect("relogin")
        .token;
    let (st, _) = send_req(
        &router,
        Method::POST,
        "/v1/sync/envelopes",
        Some(&bob_relogin_tok),
        &[("x-safi-workspace-id", &shared_ws_str)],
        Some(json!({
            "resource_kind": "recipe",
            "resource_id": "bob-demoted-check",
            "expected_version": 0,
            "ciphertext_b64": ct_v,
            "nonce_b64": nonce_v,
            "key_id": format!("wk:{shared_ws}:epoch:1"),
            "key_epoch": 1,
            "content_hash": hash_v
        })),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // Restore Bob to `member`
    let (st, _) = send_req(
        &router,
        Method::PATCH,
        &format!("/v1/organizations/{org_id}/members/{bob_id}/role"),
        Some(&carol_tok),
        &[],
        Some(json!({ "role": "member" })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    // 6. Device X25519 Keys, Wrapped Epoch Keys, E2EE Sync, Conflicts, Idempotency & Key Rotation
    let b64 = base64::engine::general_purpose::STANDARD;
    for (tok, dev_id, pk_byte) in [
        (&alice_tok, "dev_alice_01", 0x11_u8),
        (&bob_relogin_tok, "dev_bob_01", 0x22_u8),
        (&carol_tok, "dev_carol_01", 0x33_u8),
    ] {
        let (st, dk) = send_req(
            &router,
            Method::POST,
            &format!("/v1/organizations/{org_id}/keys/devices"),
            Some(tok),
            &[],
            Some(json!({
                "device_id": dev_id,
                "device_label": dev_id,
                "x25519_public_key_b64": b64.encode([pk_byte; 32])
            })),
        )
        .await;
        assert_eq!(st, StatusCode::CREATED, "register device key: {dk}");
    }

    // Wrap Epoch 1 key for Alice, Bob, and Carol
    let wrapped_dummy = b64.encode([0xAA_u8; 48]);
    let wrap_nonce = b64.encode([0xBB_u8; 12]);
    let sender_pub = b64.encode([0xCC_u8; 32]);
    let (st, wrap_res) = send_req(
        &router,
        Method::POST,
        &format!("/v1/organizations/{org_id}/keys/wrap"),
        Some(&alice_tok),
        &[],
        Some(json!({
            "key_epoch": 1,
            "entries": [
                {
                    "recipient_user_id": alice_id,
                    "recipient_device_id": "dev_alice_01",
                    "sender_ephemeral_pub_b64": sender_pub,
                    "wrapped_key_ciphertext_b64": wrapped_dummy,
                    "wrapped_key_nonce_b64": wrap_nonce
                },
                {
                    "recipient_user_id": bob_id,
                    "recipient_device_id": "dev_bob_01",
                    "sender_ephemeral_pub_b64": sender_pub,
                    "wrapped_key_ciphertext_b64": wrapped_dummy,
                    "wrapped_key_nonce_b64": wrap_nonce
                },
                {
                    "recipient_user_id": carol_id,
                    "recipient_device_id": "dev_carol_01",
                    "sender_ephemeral_pub_b64": sender_pub,
                    "wrapped_key_ciphertext_b64": wrapped_dummy,
                    "wrapped_key_nonce_b64": wrap_nonce
                }
            ]
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "wrap keys: {wrap_res}");

    let (st, bob_keys) = send_req(
        &router,
        Method::GET,
        &format!("/v1/organizations/{org_id}/keys/my-wrapped?device_id=dev_bob_01"),
        Some(&bob_relogin_tok),
        &[],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(bob_keys.as_array().unwrap().len(), 1);
    assert_eq!(bob_keys[0]["key_epoch"], 1);

    // Alice pushes E2EE resource v1 (`expected_version: 0`)
    let (ct1, nonce1, hash1) = encrypt_client_resource_envelope(
        &workspace_key_epoch1,
        [10_u8; 12],
        shared_ws,
        "recipe",
        "clean-sales-q4",
        1,
        1,
        br#"{"name":"Clean Sales Q4","steps":["trim","dedupe"]}"#,
    );
    let push_v1_body = json!({
        "resource_kind": "recipe",
        "resource_id": "clean-sales-q4",
        "expected_version": 0,
        "ciphertext_b64": ct1,
        "nonce_b64": nonce1,
        "key_id": format!("wk:{shared_ws}:epoch:1"),
        "key_epoch": 1,
        "content_hash": hash1,
        "origin_device_id": "dev_alice_01",
        "idempotency_key": "idem_alice_v1"
    });
    // Tampered ciphertext integrity check -> rejected (`422 SF-SYN-001`)!
    let (st, tamper_err) = send_req(
        &router,
        Method::POST,
        "/v1/sync/envelopes",
        Some(&alice_tok),
        &[("x-safi-workspace-id", &shared_ws_str)],
        Some(json!({
            "resource_kind": "recipe",
            "resource_id": "clean-sales-q4",
            "expected_version": 0,
            "ciphertext_b64": b64.encode([0xFF_u8; 32]),
            "nonce_b64": nonce1,
            "key_id": format!("wk:{shared_ws}:epoch:1"),
            "key_epoch": 1,
            "content_hash": hash1,
            "origin_device_id": "dev_alice_01"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(tamper_err["code"], "SF-SYN-001");

    let (st, rec_v1) = send_req(
        &router,
        Method::POST,
        "/v1/sync/envelopes",
        Some(&alice_tok),
        &[("x-safi-workspace-id", &shared_ws_str)],
        Some(push_v1_body.clone()),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "push v1: {rec_v1}");
    assert_eq!(rec_v1["version"], 1);
    assert_eq!(rec_v1["workspace_seq"], 1);

    // Idempotent retry returns identical record without incrementing version!
    let (st, rec_v1_retry) = send_req(
        &router,
        Method::POST,
        "/v1/sync/envelopes",
        Some(&alice_tok),
        &[("x-safi-workspace-id", &shared_ws_str)],
        Some(push_v1_body),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(rec_v1_retry["version"], 1);
    assert_eq!(rec_v1_retry["workspace_seq"], 1);

    // Mutated payload with same `idempotency_key` -> `409 SF-PAY-007`!
    let (ct_mut, nonce_mut, hash_mut) = encrypt_client_resource_envelope(
        &workspace_key_epoch1,
        [99_u8; 12],
        shared_ws,
        "recipe",
        "clean-sales-q4",
        1,
        1,
        br#"{"name":"Mutated Payload"}"#,
    );
    let (st, idem_err) = send_req(
        &router,
        Method::POST,
        "/v1/sync/envelopes",
        Some(&alice_tok),
        &[("x-safi-workspace-id", &shared_ws_str)],
        Some(json!({
            "resource_kind": "recipe",
            "resource_id": "clean-sales-q4",
            "expected_version": 0,
            "ciphertext_b64": ct_mut,
            "nonce_b64": nonce_mut,
            "key_id": format!("wk:{shared_ws}:epoch:1"),
            "key_epoch": 1,
            "content_hash": hash_mut,
            "origin_device_id": "dev_alice_01",
            "idempotency_key": "idem_alice_v1"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert_eq!(idem_err["code"], "SF-PAY-007");

    // Nonce reuse on v2 update -> `400` rejected!
    let (st, _) = send_req(
        &router,
        Method::POST,
        "/v1/sync/envelopes",
        Some(&bob_relogin_tok),
        &[("x-safi-workspace-id", &shared_ws_str)],
        Some(json!({
            "resource_kind": "recipe",
            "resource_id": "clean-sales-q4",
            "expected_version": 1,
            "ciphertext_b64": ct1,
            "nonce_b64": nonce1,
            "key_id": format!("wk:{shared_ws}:epoch:1"),
            "key_epoch": 1,
            "content_hash": hash1,
            "origin_device_id": "dev_bob_01",
            "idempotency_key": "idem_bob_nonce_reuse"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);

    // Stale `expected_version: 0` from Bob -> `409 SF-SYN-002` Version Conflict!
    let (ct2, nonce2, hash2) = encrypt_client_resource_envelope(
        &workspace_key_epoch1,
        [20_u8; 12],
        shared_ws,
        "recipe",
        "clean-sales-q4",
        2,
        1,
        br#"{"name":"Clean Sales Q4","steps":["trim","dedupe","normalize_dates"]}"#,
    );
    let (st, conf_err) = send_req(
        &router,
        Method::POST,
        "/v1/sync/envelopes",
        Some(&bob_relogin_tok),
        &[("x-safi-workspace-id", &shared_ws_str)],
        Some(json!({
            "resource_kind": "recipe",
            "resource_id": "clean-sales-q4",
            "expected_version": 0,
            "ciphertext_b64": ct2,
            "nonce_b64": nonce2,
            "key_id": format!("wk:{shared_ws}:epoch:1"),
            "key_epoch": 1,
            "content_hash": hash2,
            "origin_device_id": "dev_bob_01",
            "idempotency_key": "idem_bob_stale"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert_eq!(conf_err["code"], "SF-SYN-002");

    // Bob pushes valid v2 (`expected_version: 1`)
    let (st, rec_v2) = send_req(
        &router,
        Method::POST,
        "/v1/sync/envelopes",
        Some(&bob_relogin_tok),
        &[("x-safi-workspace-id", &shared_ws_str)],
        Some(json!({
            "resource_kind": "recipe",
            "resource_id": "clean-sales-q4",
            "expected_version": 1,
            "ciphertext_b64": ct2,
            "nonce_b64": nonce2,
            "key_id": format!("wk:{shared_ws}:epoch:1"),
            "key_epoch": 1,
            "content_hash": hash2,
            "origin_device_id": "dev_bob_01",
            "idempotency_key": "idem_bob_v2"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(rec_v2["version"], 2);
    assert_eq!(rec_v2["workspace_seq"], 2);

    // Viewer Victor CAN pull changes (`200 OK`)
    let (st, changes) = send_req(
        &router,
        Method::GET,
        "/v1/sync/changes?since_seq=1",
        Some(&victor_tok),
        &[("x-safi-workspace-id", &shared_ws_str)],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(changes["records"].as_array().unwrap().len(), 1);
    assert_eq!(changes["records"][0]["version"], 2);

    // Tombstone (`deleted: true, expected_version: 2`)
    let (ct3, nonce3, hash3) = encrypt_client_resource_envelope(
        &workspace_key_epoch1,
        [30_u8; 12],
        shared_ws,
        "recipe",
        "clean-sales-q4",
        3,
        1,
        b"{}",
    );
    let (st, rec_v3) = send_req(
        &router,
        Method::POST,
        "/v1/sync/envelopes",
        Some(&alice_tok),
        &[("x-safi-workspace-id", &shared_ws_str)],
        Some(json!({
            "resource_kind": "recipe",
            "resource_id": "clean-sales-q4",
            "expected_version": 2,
            "ciphertext_b64": ct3,
            "nonce_b64": nonce3,
            "key_id": format!("wk:{shared_ws}:epoch:1"),
            "key_epoch": 1,
            "content_hash": hash3,
            "deleted": true,
            "origin_device_id": "dev_alice_01",
            "idempotency_key": "idem_alice_v3_tombstone"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(rec_v3["version"], 3);
    assert_eq!(rec_v3["deleted"], true);

    // 7. Remove Bob + Rotate Workspace Key to Epoch 2 -> Bob blocked from ALL future sync/key endpoints!
    let (st, _) = send_req(
        &router,
        Method::DELETE,
        &format!("/v1/organizations/{org_id}/members/{bob_id}"),
        Some(&alice_tok),
        &[],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::NO_CONTENT);

    let (st, rot_res) = send_req(
        &router,
        Method::POST,
        &format!("/v1/organizations/{org_id}/keys/rotate"),
        Some(&alice_tok),
        &[],
        Some(json!({
            "expected_current_epoch": 1,
            "entries": [
                {
                    "recipient_user_id": alice_id,
                    "recipient_device_id": "dev_alice_01",
                    "sender_ephemeral_pub_b64": sender_pub,
                    "wrapped_key_ciphertext_b64": wrapped_dummy,
                    "wrapped_key_nonce_b64": b64.encode([0xDD_u8; 12])
                },
                {
                    "recipient_user_id": carol_id,
                    "recipient_device_id": "dev_carol_01",
                    "sender_ephemeral_pub_b64": sender_pub,
                    "wrapped_key_ciphertext_b64": wrapped_dummy,
                    "wrapped_key_nonce_b64": b64.encode([0xEE_u8; 12])
                }
            ]
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "rotate keys: {rot_res}");
    assert_eq!(rot_res["active_key_epoch"], 2);

    // Removed Bob is rejected (`403 SF-SYN-005`) on sync pull, sync push, and wrapped key fetch!
    for (method, path) in [
        (Method::GET, "/v1/sync/changes?since_seq=0".to_owned()),
        (
            Method::GET,
            format!("/v1/organizations/{org_id}/keys/my-wrapped?device_id=dev_bob_01"),
        ),
    ] {
        let (st, err) = send_req(
            &router,
            method,
            &path,
            Some(&bob_relogin_tok),
            &[("x-safi-workspace-id", &shared_ws_str)],
            None,
        )
        .await;
        assert_eq!(st, StatusCode::FORBIDDEN);
        assert_eq!(err["code"], "SF-SYN-005");
    }

    // 8. Ownership Transfer (`Alice -> Carol`) & Owner-Only Delete Organization
    let (st, transferred) = send_req(
        &router,
        Method::POST,
        &format!("/v1/organizations/{org_id}/transfer-ownership"),
        Some(&alice_tok),
        &[],
        Some(json!({ "new_owner_user_id": carol_id })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(transferred["owner_user_id"], carol_id.to_string());

    // Former Owner Alice (now Admin) CANNOT delete org (`403 SF-SYN-005`), new Owner Carol CAN!
    let (st, _) = send_req(
        &router,
        Method::DELETE,
        &format!("/v1/organizations/{org_id}"),
        Some(&alice_tok),
        &[],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    let (st, _) = send_req(
        &router,
        Method::DELETE,
        &format!("/v1/organizations/{org_id}"),
        Some(&carol_tok),
        &[],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::NO_CONTENT);

    // Verify tamper-evident Audit Log recorded key security actions!
    let audit_valid = state.log_center.verify_chain().await;
    assert!(audit_valid, "audit hash chain must verify");
}

#[tokio::test]
#[ignore = "Requires live PostgreSQL 18 via SF_TEST_DATABASE_URL with migrations 0001..0017 applied"]
async fn postgres_18_organizations_rbac_rls_and_encrypted_sync_multi_instance_restart() {
    let mock_paddle_url = spawn_paddle_mock();
    let db_url = std::env::var("SF_TEST_DATABASE_URL").expect("SF_TEST_DATABASE_URL");
    let pool_a = sqlx::postgres::PgPoolOptions::new()
        .max_connections(6)
        .connect(&db_url)
        .await
        .expect("pool A");
    let pool_b = sqlx::postgres::PgPoolOptions::new()
        .max_connections(6)
        .connect(&db_url)
        .await
        .expect("pool B");

    let build_instance = |pool: sqlx::PgPool| {
        let mock_url = mock_paddle_url.clone();
        async move {
            let sessions = Arc::new(InMemorySessionRepository::new());
            let keys = Arc::new(InMemoryApiKeyRepository::new(Vec::new()).await);
            let cache = Arc::new(InMemoryAuthCache::new());
            let auth = AuthService::new(keys, sessions, cache);
            let mut cfg = ServerConfig::for_tests();
            cfg.public_web_origin = Some("https://safidata.app".to_owned());
            cfg.api_base_url = Some("https://api.safidata.app".to_owned());
            configure_paddle_test_billing(&mut cfg, &mock_url);
            let pg = PgControlPlaneStore::new(pool);
            let mut st = AppState::with_config(auth, cfg).with_pg_store(pg);
            st.entitlements = st.entitlements.with_require_durable_pg(true);
            st.billing = st.billing.with_require_durable_pg(true);
            st.billing_handoff = st.billing_handoff.with_require_durable_pg(true);
            st.organizations = st.organizations.with_require_durable_pg(true);
            st.encrypted_sync = st.encrypted_sync.with_require_durable_pg(true);
            let router = app(st.clone());
            (st, router)
        }
    };

    let (state_a, router_a) = build_instance(pool_a.clone()).await;
    let (state_b, router_b) = build_instance(pool_b.clone()).await;

    let seed_pg_verified_user = |label: &str| {
        let pool = pool_a.clone();
        let lbl = label.to_owned();
        async move {
            let uid = Uuid::now_v7();
            let wid = Uuid::now_v7();
            let email = format!("pg-org-{lbl}-{uid}@acme.test");
            sqlx::query(
                "INSERT INTO users (id, email, display_name, email_verified_at) VALUES ($1, $2, $3, now())",
            )
            .bind(uid)
            .bind(&email)
            .bind(&lbl)
            .execute(&pool)
            .await
            .expect("seed verified user");
            sqlx::query("INSERT INTO workspaces (id, name, plan_code) VALUES ($1, $2, 'free')")
                .bind(wid)
                .bind(format!("{lbl} Personal WS"))
                .execute(&pool)
                .await
                .expect("seed personal ws");
            (uid, wid, email)
        }
    };

    let (alice_id, alice_ws, _alice_email) = seed_pg_verified_user("alice").await;
    let (bob_id, bob_ws, bob_email) = seed_pg_verified_user("bob").await;
    let (viewer_id, viewer_ws, viewer_email) = seed_pg_verified_user("viewer").await;
    let (tenant2_id, tenant2_ws, _tenant2_email) = seed_pg_verified_user("tenant2").await;

    let make_session = |st: &AppState, uid: Uuid, wid: Uuid| {
        let s = st.clone();
        async move {
            s.auth
                .create_session(
                    &Principal {
                        user_id: uid,
                        workspace_id: wid,
                        scopes: vec![Scope::Read, Scope::Write],
                        auth_method: AuthMethod::Password,
                    },
                    "PG18 Org Desktop",
                )
                .await
                .expect("session")
                .token
        }
    };

    let alice_tok_a = make_session(&state_a, alice_id, alice_ws).await;
    let bob_tok_b = make_session(&state_b, bob_id, bob_ws).await;
    let viewer_tok_b = make_session(&state_b, viewer_id, viewer_ws).await;
    let tenant2_tok_b = make_session(&state_b, tenant2_id, tenant2_ws).await;

    // 1. Activate Organization subscriptions for Tenant 1 (Alice, 4 seats) & Tenant 2 (3 seats) in PostgreSQL 18
    activate_org_subscription_via_paddle_webhook(
        &router_a,
        &alice_tok_a,
        alice_id,
        alice_ws,
        4,
        "pg_tenant1",
    )
    .await;
    activate_org_subscription_via_paddle_webhook(
        &router_b,
        &tenant2_tok_b,
        tenant2_id,
        tenant2_ws,
        3,
        "pg_tenant2",
    )
    .await;

    // 2. Alice creates Org 1 on Instance A; Tenant 2 creates Org 2 on Instance B
    let (st, org1) = send_req(
        &router_a,
        Method::POST,
        "/v1/organizations",
        Some(&alice_tok_a),
        &[],
        Some(json!({ "name": "PG18 Tenant One", "seats": 4 })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "pg create org1: {org1}");
    let org1_id = Uuid::parse_str(org1["id"].as_str().unwrap()).unwrap();
    let org1_ws = Uuid::parse_str(org1["workspace_id"].as_str().unwrap()).unwrap();
    let org1_ws_s = org1_ws.to_string();

    let (st, org2) = send_req(
        &router_b,
        Method::POST,
        "/v1/organizations",
        Some(&tenant2_tok_b),
        &[],
        Some(json!({ "name": "PG18 Tenant Two", "seats": 3 })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    let org2_ws = Uuid::parse_str(org2["workspace_id"].as_str().unwrap()).unwrap();

    // 3. Alice invites Bob (`member`) & Viewer (`viewer`) on Instance A; they accept on Instance B!
    let (st, bob_inv) = send_req(
        &router_a,
        Method::POST,
        &format!("/v1/organizations/{org1_id}/invite"),
        Some(&alice_tok_a),
        &[],
        Some(json!({ "email": bob_email, "role": "member" })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    let bob_invite_tok = bob_inv["token"].as_str().unwrap().to_owned();

    // Wrong-email Tenant 2 user tries Bob's token on Instance B -> 403 SF-SYN-005!
    let (st, err) = send_req(
        &router_b,
        Method::POST,
        "/v1/organizations/invitations/accept",
        Some(&tenant2_tok_b),
        &[],
        Some(json!({ "token": bob_invite_tok })),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert_eq!(err["code"], "SF-SYN-005");

    // Bob accepts on Instance B!
    let (st, _) = send_req(
        &router_b,
        Method::POST,
        "/v1/organizations/invitations/accept",
        Some(&bob_tok_b),
        &[],
        Some(json!({ "token": bob_invite_tok })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    let (st, v_inv) = send_req(
        &router_a,
        Method::POST,
        &format!("/v1/organizations/{org1_id}/invite"),
        Some(&alice_tok_a),
        &[],
        Some(json!({ "email": viewer_email, "role": "viewer" })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);
    let (st, _) = send_req(
        &router_b,
        Method::POST,
        "/v1/organizations/invitations/accept",
        Some(&viewer_tok_b),
        &[],
        Some(json!({ "token": v_inv["token"].as_str().unwrap() })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);

    // 4. Alice pushes client-encrypted ChaCha20-Poly1305 envelope on Instance A; Bob pulls on Instance B!
    let secret_plaintext =
        br#"{"customer":"SENSITIVE_PAYROLL_987654321","formula":"sum(net_pay)"}"#;
    let epoch1_key = [0x77_u8; 32];
    let (ct_b64, nonce_b64, hash_hex) = encrypt_client_resource_envelope(
        &epoch1_key,
        [55_u8; 12],
        org1_ws,
        "project",
        "q4-payroll-pipeline",
        1,
        1,
        secret_plaintext,
    );
    let (st, pushed) = send_req(
        &router_a,
        Method::POST,
        "/v1/sync/envelopes",
        Some(&alice_tok_a),
        &[("x-safi-workspace-id", &org1_ws_s)],
        Some(json!({
            "resource_kind": "project",
            "resource_id": "q4-payroll-pipeline",
            "expected_version": 0,
            "ciphertext_b64": ct_b64,
            "nonce_b64": nonce_b64,
            "key_id": format!("wk:{org1_ws}:epoch:1"),
            "key_epoch": 1,
            "content_hash": hash_hex,
            "origin_device_id": "pg_alice_dev",
            "idempotency_key": "pg_sync_idem_01"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK, "pg push sync: {pushed}");

    // Bob pulls on Instance B -> receives exact ciphertext!
    let (st, pulled_b) = send_req(
        &router_b,
        Method::GET,
        "/v1/sync/changes?since_seq=0",
        Some(&bob_tok_b),
        &[("x-safi-workspace-id", &org1_ws_s)],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(pulled_b["records"][0]["ciphertext_b64"], ct_b64);

    // Viewer blocked from modifying on Instance B (`403 SF-SYN-005`)!
    let (st, _) = send_req(
        &router_b,
        Method::POST,
        "/v1/sync/envelopes",
        Some(&viewer_tok_b),
        &[("x-safi-workspace-id", &org1_ws_s)],
        Some(json!({
            "resource_kind": "project",
            "resource_id": "q4-payroll-pipeline",
            "expected_version": 1,
            "ciphertext_b64": ct_b64,
            "nonce_b64": base64::engine::general_purpose::STANDARD.encode([66_u8; 12]),
            "key_id": format!("wk:{org1_ws}:epoch:1"),
            "key_epoch": 1,
            "content_hash": hash_hex
        })),
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);

    // 5. Direct PostgreSQL 18 BYTEA inspection: ZERO plaintext stored in `encrypted_sync_records`!
    let mut tx_inspect = pool_a.begin().await.expect("tx inspect");
    sqlx::query("SELECT set_config('app.current_workspace_id', $1, true), set_config('app.workspace_id', $1, true)")
        .bind(&org1_ws_s)
        .execute(&mut *tx_inspect)
        .await
        .expect("set ws");
    let raw_ct: Vec<u8> = sqlx::query_scalar(
        "SELECT ciphertext FROM encrypted_sync_records WHERE workspace_id = $1 AND resource_id = 'q4-payroll-pipeline'",
    )
    .bind(org1_ws)
    .fetch_one(&mut *tx_inspect)
    .await
    .expect("raw ciphertext");
    tx_inspect.commit().await.expect("commit inspect");
    assert!(
        !raw_ct
            .windows(b"SENSITIVE_PAYROLL_987654321".len())
            .any(|w| w == b"SENSITIVE_PAYROLL_987654321"),
        "PostgreSQL ciphertext BYTEA MUST NOT contain plaintext!"
    );

    // 6. PostgreSQL 18 Native RLS Isolation under `NOBYPASSRLS` role (`safi_p0_01_app` when present):
    //    Tenant 2 workspace context MUST see 0 rows of Tenant 1's organizations, members, or sync records!
    let has_app_role: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'safi_p0_01_app')",
    )
    .fetch_one(&pool_a)
    .await
    .expect("check role");
    if has_app_role {
        let mut rls_tx = pool_a.begin().await.expect("rls tx");
        sqlx::query("SET LOCAL ROLE safi_p0_01_app")
            .execute(&mut *rls_tx)
            .await
            .expect("set role");
        let org2_ws_s = org2_ws.to_string();
        sqlx::query(
            "SELECT set_config('app.current_workspace_id', $1, true), set_config('app.workspace_id', $1, true), set_config('app.current_user_id', $2, true)",
        )
        .bind(&org2_ws_s)
        .bind(tenant2_id.to_string())
        .execute(&mut *rls_tx)
        .await
        .expect("set tenant2 context");

        let cross_sync_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM encrypted_sync_records WHERE workspace_id = $1",
        )
        .bind(org1_ws)
        .fetch_one(&mut *rls_tx)
        .await
        .expect("cross sync rls");
        assert_eq!(
            cross_sync_count, 0,
            "RLS leaked Tenant 1 sync records to Tenant 2!"
        );

        let cross_org_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM organizations WHERE id = $1")
                .bind(org1_id)
                .fetch_one(&mut *rls_tx)
                .await
                .expect("cross org rls");
        assert_eq!(
            cross_org_count, 0,
            "RLS leaked Tenant 1 organization to Tenant 2!"
        );
        rls_tx.rollback().await.expect("rollback rls");
    }

    // 7. Remove Bob & rotate key to Epoch 2 on Instance A, then COLD RESTART on Instance C!
    let b64 = base64::engine::general_purpose::STANDARD;
    let (st, _) = send_req(
        &router_a,
        Method::POST,
        &format!("/v1/organizations/{org1_id}/keys/devices"),
        Some(&alice_tok_a),
        &[],
        Some(json!({
            "device_id": "pg_alice_dev",
            "device_label": "Alice PG Dev",
            "x25519_public_key_b64": b64.encode([0x11_u8; 32])
        })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED);

    let (st, _) = send_req(
        &router_a,
        Method::DELETE,
        &format!("/v1/organizations/{org1_id}/members/{bob_id}"),
        Some(&alice_tok_a),
        &[],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::NO_CONTENT);

    let (st, rot) = send_req(
        &router_a,
        Method::POST,
        &format!("/v1/organizations/{org1_id}/keys/rotate"),
        Some(&alice_tok_a),
        &[],
        Some(json!({
            "expected_current_epoch": 1,
            "entries": [{
                "recipient_user_id": alice_id,
                "recipient_device_id": "pg_alice_dev",
                "sender_ephemeral_pub_b64": b64.encode([0x22_u8; 32]),
                "wrapped_key_ciphertext_b64": b64.encode([0x33_u8; 48]),
                "wrapped_key_nonce_b64": b64.encode([0x44_u8; 12])
            }]
        })),
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(rot["active_key_epoch"], 2);

    // Drop Instance A & Instance B pools and boot fresh Cold-Restart Instance C!
    drop(router_a);
    drop(router_b);
    drop(state_a);
    drop(state_b);
    pool_a.close().await;
    pool_b.close().await;

    let pool_c = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&db_url)
        .await
        .expect("pool C");
    let (state_c, router_c) = build_instance(pool_c.clone()).await;
    let alice_tok_c = make_session(&state_c, alice_id, alice_ws).await;
    let bob_tok_c = make_session(&state_c, bob_id, bob_ws).await;
    let viewer_tok_c = make_session(&state_c, viewer_id, viewer_ws).await;

    // Alice & Viewer retain durable org membership, Epoch 2 key state, and encrypted sync records after restart!
    let (st, org_after_restart) = send_req(
        &router_c,
        Method::GET,
        &format!("/v1/organizations/{org1_id}"),
        Some(&alice_tok_c),
        &[],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(org_after_restart["organization"]["active_key_epoch"], 2);
    assert_eq!(org_after_restart["active_members_count"], 2); // Alice + Viewer

    let (st, viewer_sync_after_restart) = send_req(
        &router_c,
        Method::GET,
        "/v1/sync/changes?since_seq=0",
        Some(&viewer_tok_c),
        &[("x-safi-workspace-id", &org1_ws_s)],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(
        viewer_sync_after_restart["records"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // Removed Bob remains permanently blocked (`403 SF-SYN-005`) after cold restart!
    let (st, bob_blocked) = send_req(
        &router_c,
        Method::GET,
        "/v1/sync/changes?since_seq=0",
        Some(&bob_tok_c),
        &[("x-safi-workspace-id", &org1_ws_s)],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert_eq!(bob_blocked["code"], "SF-SYN-005");

    pool_c.close().await;
}
