//! Mandatory Runtime Acceptance Suite (Section 11):
//! - Single-Device Exclusivity & Automatic Revocation (including 10-way concurrent login race,
//!   Ed25519 cryptographic device proof, Owner audited device reset, and real server restart).
//! - Subscription, Billing, Refund, Chargeback, Suspension, Trial, AI Quota, and Coupon
//!   Enforcement (including 10-way concurrent coupon redemption race, duplicate/forged webhooks,
//!   Owner Control Center RBAC/TOTP step-up, and full restart durability).

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use serde_json::{json, Value};
use sf_server::auth::account::{
    AccountAuthService, AccountRepository, LoginAccount, RegisterAccountRequest,
    VerificationEmailSender,
};
use sf_server::auth::cache::InMemoryAuthCache;
use sf_server::auth::repository::{InMemoryApiKeyRepository, InMemorySessionRepository};
use sf_server::auth::service::AuthService;
use sf_server::auth::session::{parse_session_token, token_hash};
use sf_server::auth::unix_now;
use sf_server::billing::sign_paddle_payload;
use sf_server::config::ServerConfig;
use sf_server::error::AuthError;
use sf_server::owner_mfa::{compute_totp_code, decode_base32, TOTP_PERIOD_SECS};
use sf_server::{app, AppState};
use tokio::sync::RwLock;
use tower::ServiceExt;
use uuid::Uuid;

#[derive(Default)]
struct TestAccountState {
    accounts: HashMap<String, LoginAccount>,
    pending_tokens: HashMap<[u8; 32], (String, i64)>,
    reset_tokens: HashMap<[u8; 32], (String, i64)>,
}

#[derive(Default)]
struct TestAccountRepo {
    state: RwLock<TestAccountState>,
}

#[async_trait]
impl AccountRepository for TestAccountRepo {
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
        s.pending_tokens
            .insert(*token_hash, (email.to_owned(), token_expires_at));
        Ok(true)
    }

    async fn find_for_login(&self, email: &str) -> Result<Option<LoginAccount>, AuthError> {
        Ok(self.state.read().await.accounts.get(email).cloned())
    }

    async fn verify_email(&self, token_hash: &[u8; 32], now: i64) -> Result<bool, AuthError> {
        let mut s = self.state.write().await;
        let Some((email, exp)) = s.pending_tokens.remove(token_hash) else {
            return Ok(false);
        };
        if now >= exp {
            return Ok(false);
        }
        if let Some(acc) = s.accounts.get_mut(&email) {
            acc.email_verified = true;
            return Ok(true);
        }
        Ok(false)
    }

    async fn replace_verification_token(
        &self,
        email: &str,
        token_hash: &[u8; 32],
        token_expires_at: i64,
        _now: i64,
    ) -> Result<bool, AuthError> {
        let mut s = self.state.write().await;
        if !s.accounts.contains_key(email) {
            return Ok(false);
        }
        s.pending_tokens
            .insert(*token_hash, (email.to_owned(), token_expires_at));
        Ok(true)
    }

    async fn replace_password_reset_token(
        &self,
        email: &str,
        token_hash: &[u8; 32],
        token_expires_at: i64,
        _now: i64,
    ) -> Result<bool, AuthError> {
        let mut s = self.state.write().await;
        if !s.accounts.contains_key(email) {
            return Ok(false);
        }
        s.reset_tokens
            .insert(*token_hash, (email.to_owned(), token_expires_at));
        Ok(true)
    }

    async fn reset_password(
        &self,
        token_hash: &[u8; 32],
        password_hash: &str,
        now: i64,
    ) -> Result<bool, AuthError> {
        let mut s = self.state.write().await;
        let Some((email, exp)) = s.reset_tokens.remove(token_hash) else {
            return Ok(false);
        };
        if now >= exp {
            return Ok(false);
        }
        if let Some(acc) = s.accounts.get_mut(&email) {
            acc.password_hash = password_hash.to_owned();
            return Ok(true);
        }
        Ok(false)
    }
}

#[derive(Default)]
struct TestEmailSender {
    tokens: RwLock<HashMap<String, String>>,
}

impl TestEmailSender {
    async fn last_token(&self, email: &str) -> Option<String> {
        self.tokens.read().await.get(email).cloned()
    }
}

#[async_trait]
impl VerificationEmailSender for TestEmailSender {
    async fn send_verification(
        &self,
        recipient: &str,
        verification_url: &str,
    ) -> Result<(), AuthError> {
        if let Some(tok) = verification_url
            .split("verify_email=")
            .nth(1)
            .or_else(|| verification_url.split("token=").nth(1))
        {
            self.tokens
                .write()
                .await
                .insert(recipient.to_owned(), tok.to_owned());
        }
        Ok(())
    }

    async fn send_password_reset(
        &self,
        _recipient: &str,
        _reset_url: &str,
    ) -> Result<(), AuthError> {
        Ok(())
    }
}

fn test_config() -> ServerConfig {
    let mut cfg = ServerConfig::for_tests();
    cfg.paddle_api_key = Some("pdl_live_test_key".to_owned());
    cfg.paddle_webhook_secret = Some("pdl_whsec_test_999".to_owned());
    cfg.paymob_api_key = Some("paymob_test_key".to_owned());
    cfg.paymob_hmac_secret = Some("paymob_hmac_test_888".to_owned());
    cfg.ai_provider_url = Some("https://ai.example.test/v1".to_owned());
    cfg.ai_api_key = Some("ai_test_key".to_owned());
    cfg.ai_model = Some("test-model".to_owned());
    cfg.license_signing_seed = Some([0x33u8; 32]);
    cfg
}

async fn send_json(
    router: &axum::Router,
    method: &str,
    uri: &str,
    bearer: Option<&str>,
    extra_headers: &[(&str, &str)],
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(tok) = bearer {
        builder = builder.header("authorization", format!("Bearer {tok}"));
    }
    for (k, v) in extra_headers {
        builder = builder.header(*k, *v);
    }
    let req = if let Some(b) = body {
        builder
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&b).expect("json")))
            .expect("req")
    } else {
        builder.body(Body::empty()).expect("req")
    };
    let resp = router.clone().oneshot(req).await.expect("oneshot");
    let status = resp.status();
    let raw = to_bytes(resp.into_body(), 1024 * 1024).await.expect("body");
    let val = serde_json::from_slice(&raw).unwrap_or_else(|_| json!({}));
    (status, val)
}

async fn register_and_verify(
    account_auth: &AccountAuthService,
    mailer: &TestEmailSender,
    email: &str,
    password: &str,
) {
    account_auth
        .register(RegisterAccountRequest {
            email: email.to_owned(),
            display_name: "Test User".to_owned(),
            password: password.to_owned(),
        })
        .await
        .expect("register");
    let token = mailer.last_token(email).await.expect("verification token");
    account_auth.verify_email(&token).await.expect("verify");
}

fn sign_device_proof(seed: [u8; 32], bearer: &str, ts: i64) -> (String, String) {
    let sk = SigningKey::from_bytes(&seed);
    let pk_hex = hex::encode(VerifyingKey::from(&sk).to_bytes());
    let raw = parse_session_token(bearer).expect("sfs token");
    let hash = token_hash(raw);
    let canonical = format!("safidata-device-proof-v1|{hash}|{ts}");
    let sig_hex = hex::encode(sk.sign(canonical.as_bytes()).to_bytes());
    (pk_hex, sig_hex)
}

#[tokio::test]
async fn single_device_exclusivity_ed25519_proof_concurrency_reset_and_server_restart() {
    let journal_path = std::env::temp_dir().join(format!(
        "safi-device-journal-{}.json",
        Uuid::now_v7().simple()
    ));

    let accounts_repo = Arc::new(TestAccountRepo::default());
    let mailer = Arc::new(TestEmailSender::default());
    let api_keys = Arc::new(InMemoryApiKeyRepository::default());
    let sessions_repo = Arc::new(InMemorySessionRepository::new());
    let cache = Arc::new(InMemoryAuthCache::new());
    let auth = AuthService::new(api_keys.clone(), sessions_repo.clone(), cache.clone());

    let state = AppState::with_config(auth, test_config())
        .with_durable_journal(&journal_path, Some(sessions_repo.clone()))
        .await
        .expect("journal");
    let account_auth = AccountAuthService::new(
        accounts_repo.clone(),
        mailer.clone(),
        state.auth.clone(),
        "https://app.safidata.example".to_owned(),
    );
    register_and_verify(
        &account_auth,
        &mailer,
        "owner@example.com",
        "OwnerPass!1234",
    )
    .await;
    register_and_verify(
        &account_auth,
        &mailer,
        "alice@example.com",
        "AlicePass!1234",
    )
    .await;
    let state = state.with_account_auth(account_auth);
    let router = app(state.clone());

    // 1. Device A logs in with Ed25519 cryptographic device public key.
    let seed_a = [0x11u8; 32];
    let pk_a = hex::encode(VerifyingKey::from(&SigningKey::from_bytes(&seed_a)).to_bytes());
    let inst_a = Uuid::now_v7();
    let (st_a, login_a) = send_json(
        &router,
        "POST",
        "/v1/auth/login",
        None,
        &[],
        Some(json!({
            "email": "alice@example.com",
            "password": "AlicePass!1234",
            "device_name": "Device A Laptop",
            "installation_id": inst_a,
            "platform": "windows",
            "device_public_key_hex": pk_a,
        })),
    )
    .await;
    assert_eq!(st_a, StatusCode::CREATED);
    assert_eq!(login_a["session_generation"], json!(1));
    assert_eq!(login_a["previous_device_revoked"], json!(false));
    assert!(login_a["lease"]["signature_hex"].as_str().is_some());
    let token_a = login_a["token"].as_str().expect("token_a").to_owned();
    let alice_user_id = Uuid::parse_str(
        login_a["lease"]["payload"]["user_id"]
            .as_str()
            .expect("uid"),
    )
    .expect("uuid");

    // 2. Device A authorizes online with valid Ed25519 proof -> 200 OK signed 60s lease.
    let now = unix_now();
    let (_pk, sig_a) = sign_device_proof(seed_a, &token_a, now);
    let ts_str = now.to_string();
    let (st_auth_a, lease_a) = send_json(
        &router,
        "POST",
        "/v1/auth/authorize",
        Some(&token_a),
        &[
            ("x-safi-device-signature", &sig_a),
            ("x-safi-device-timestamp", &ts_str),
        ],
        None,
    )
    .await;
    assert_eq!(st_auth_a, StatusCode::OK);
    assert_eq!(lease_a["payload"]["can_execute_protected_ops"], json!(true));
    assert_eq!(lease_a["payload"]["session_generation"], json!(1));

    // 3. Stolen token A replayed WITHOUT Ed25519 proof or with wrong key -> SF-AUTH-020!
    let (st_missing_proof, body_missing) = send_json(
        &router,
        "POST",
        "/v1/auth/authorize",
        Some(&token_a),
        &[],
        None,
    )
    .await;
    assert_eq!(st_missing_proof, StatusCode::UNAUTHORIZED);
    assert_eq!(body_missing["code"], json!("SF-AUTH-020"));

    let (_pk_bad, sig_wrong_key) = sign_device_proof([0x99u8; 32], &token_a, now);
    let (st_bad_proof, body_bad) = send_json(
        &router,
        "POST",
        "/v1/auth/authorize",
        Some(&token_a),
        &[
            ("x-safi-device-signature", &sig_wrong_key),
            ("x-safi-device-timestamp", &ts_str),
        ],
        None,
    )
    .await;
    assert_eq!(st_bad_proof, StatusCode::UNAUTHORIZED);
    assert_eq!(body_bad["code"], json!("SF-AUTH-020"));

    // 4. Device B logs in to Alice's account -> Device A automatically revoked, gen = 2!
    let seed_b = [0x22u8; 32];
    let pk_b = hex::encode(VerifyingKey::from(&SigningKey::from_bytes(&seed_b)).to_bytes());
    let inst_b = Uuid::now_v7();
    let (st_b, login_b) = send_json(
        &router,
        "POST",
        "/v1/auth/login",
        None,
        &[],
        Some(json!({
            "email": "alice@example.com",
            "password": "AlicePass!1234",
            "device_name": "Device B Workstation",
            "installation_id": inst_b,
            "platform": "windows",
            "device_public_key_hex": pk_b,
        })),
    )
    .await;
    assert_eq!(st_b, StatusCode::CREATED);
    assert_eq!(login_b["session_generation"], json!(2));
    assert_eq!(login_b["previous_device_revoked"], json!(true));
    assert_eq!(
        state
            .device_authority
            .active_device_count(alice_user_id)
            .await,
        1
    );
    let token_b = login_b["token"].as_str().expect("token_b").to_owned();

    // 5. Device A heartbeat/authorize MUST immediately return SF-AUTH-019 ("Device replaced")!
    let (st_a_revoked, body_a_revoked) = send_json(
        &router,
        "POST",
        "/v1/auth/heartbeat",
        Some(&token_a),
        &[
            ("x-safi-device-signature", &sig_a),
            ("x-safi-device-timestamp", &ts_str),
        ],
        None,
    )
    .await;
    assert_eq!(st_a_revoked, StatusCode::UNAUTHORIZED);
    assert_eq!(body_a_revoked["code"], json!("SF-AUTH-019"));

    // 6. Device A protected endpoint `/v1/entitlements/consume` MUST fail!
    let (st_a_consume, _) = send_json(
        &router,
        "POST",
        "/v1/entitlements/consume",
        Some(&token_a),
        &[],
        Some(json!({
            "idempotency_key": "op-a-revoked",
            "kind": "data_operation",
            "row_count": 100,
            "column_count": 5,
            "pii_masked_count": 0
        })),
    )
    .await;
    assert_eq!(st_a_consume, StatusCode::UNAUTHORIZED);

    // 7. Device B continues operating normally with its Ed25519 proof.
    let (_pk_b2, sig_b) = sign_device_proof(seed_b, &token_b, now);
    let (st_b_ok, lease_b) = send_json(
        &router,
        "POST",
        "/v1/auth/heartbeat",
        Some(&token_b),
        &[
            ("x-safi-device-signature", &sig_b),
            ("x-safi-device-timestamp", &ts_str),
        ],
        None,
    )
    .await;
    assert_eq!(st_b_ok, StatusCode::OK);
    assert_eq!(lease_b["payload"]["session_generation"], json!(2));

    // 8. REAL SERVER RESTART: Drop router & state, construct brand-new server from disk journal!
    drop(router);
    drop(state);

    let restarted_sessions = Arc::new(InMemorySessionRepository::new());
    let restarted_auth = AuthService::new(
        Arc::new(InMemoryApiKeyRepository::default()),
        restarted_sessions.clone(),
        Arc::new(InMemoryAuthCache::new()),
    );
    let restarted_state = AppState::with_config(restarted_auth, test_config())
        .with_durable_journal(&journal_path, Some(restarted_sessions.clone()))
        .await
        .expect("restarted journal");
    let restarted_account_auth = AccountAuthService::new(
        accounts_repo.clone(),
        mailer.clone(),
        restarted_state.auth.clone(),
        "https://app.safidata.example".to_owned(),
    );
    let restarted_state = restarted_state.with_account_auth(restarted_account_auth);
    let restarted_router = app(restarted_state.clone());

    // After full server restart, Device A STILL fails with SF-AUTH-019 and Device B STILL works!
    let (st_a_after_restart, body_a_after) = send_json(
        &restarted_router,
        "POST",
        "/v1/auth/heartbeat",
        Some(&token_a),
        &[
            ("x-safi-device-signature", &sig_a),
            ("x-safi-device-timestamp", &ts_str),
        ],
        None,
    )
    .await;
    assert_eq!(st_a_after_restart, StatusCode::UNAUTHORIZED);
    assert_eq!(body_a_after["code"], json!("SF-AUTH-019"));

    let (st_b_after_restart, _) = send_json(
        &restarted_router,
        "POST",
        "/v1/auth/heartbeat",
        Some(&token_b),
        &[
            ("x-safi-device-signature", &sig_b),
            ("x-safi-device-timestamp", &ts_str),
        ],
        None,
    )
    .await;
    assert_eq!(st_b_after_restart, StatusCode::OK);
    assert!(restarted_state.log_center.verify_chain().await);

    // 9. Concurrent login race: 10 simultaneous logins for Alice -> strictly 1 active device!
    let mut handles = Vec::new();
    for i in 0..10 {
        let r = restarted_router.clone();
        handles.push(tokio::spawn(async move {
            send_json(
                &r,
                "POST",
                "/v1/auth/login",
                None,
                &[],
                Some(json!({
                    "email": "alice@example.com",
                    "password": "AlicePass!1234",
                    "device_name": format!("Race Device {i}"),
                    "installation_id": Uuid::now_v7(),
                    "platform": "windows",
                })),
            )
            .await
        }));
    }
    let mut winning_tokens = Vec::new();
    for h in handles {
        let (st, val) = h.await.expect("join");
        assert_eq!(st, StatusCode::CREATED);
        winning_tokens.push(val["token"].as_str().expect("tok").to_owned());
    }
    assert_eq!(
        restarted_state
            .device_authority
            .active_device_count(alice_user_id)
            .await,
        1
    );
    // Out of all 10 tokens issued concurrently, EXACTLY 1 must authorize!
    let mut still_active = 0usize;
    for tok in &winning_tokens {
        let (st, _) = send_json(
            &restarted_router,
            "POST",
            "/v1/auth/heartbeat",
            Some(tok),
            &[],
            None,
        )
        .await;
        if st == StatusCode::OK {
            still_active += 1;
        }
    }
    assert_eq!(
        still_active, 1,
        "strictly one device may survive concurrent login race"
    );

    // 10. Audited Owner administrative device reset revokes Alice's remaining active device.
    let (st_owner_login, owner_login) = send_json(
        &restarted_router,
        "POST",
        "/v1/auth/login",
        None,
        &[],
        Some(json!({
            "email": "owner@example.com",
            "password": "OwnerPass!1234",
            "device_name": "Owner Console",
            "installation_id": Uuid::now_v7(),
            "platform": "windows",
        })),
    )
    .await;
    assert_eq!(st_owner_login, StatusCode::CREATED);
    let owner_token = owner_login["token"].as_str().expect("owner_tok").to_owned();
    let owner_uid = Uuid::parse_str(
        owner_login["lease"]["payload"]["user_id"]
            .as_str()
            .expect("owner uid"),
    )
    .expect("uuid");
    restarted_state
        .owner_mfa
        .bootstrap_owner_once(owner_uid, true)
        .await
        .expect("bootstrap owner");
    let (_, enroll) = send_json(
        &restarted_router,
        "POST",
        "/v1/owner/mfa/enroll",
        Some(&owner_token),
        &[],
        None,
    )
    .await;
    let raw_secret =
        decode_base32(enroll["manual_secret_base32"].as_str().expect("b32")).expect("decode");
    let live_code = compute_totp_code(&raw_secret, unix_now() / TOTP_PERIOD_SECS).expect("totp");
    let (st_confirm, _) = send_json(
        &restarted_router,
        "POST",
        "/v1/owner/mfa/confirm",
        Some(&owner_token),
        &[],
        Some(json!({ "code": live_code })),
    )
    .await;
    assert_eq!(st_confirm, StatusCode::OK);

    // Non-owner trying Owner device reset fails closed (403 / SF-AUTH-013)!
    let active_alice_token = winning_tokens
        .iter()
        .find(|t| !t.is_empty())
        .cloned()
        .expect("tok");
    let (st_non_owner, _) = send_json(
        &restarted_router,
        "POST",
        &format!("/v1/owner/accounts/{alice_user_id}/devices/reset"),
        Some(&active_alice_token),
        &[],
        Some(json!({ "reason": "unauthorized attempt" })),
    )
    .await;
    assert!(st_non_owner == StatusCode::UNAUTHORIZED || st_non_owner == StatusCode::FORBIDDEN);

    // Owner executes audited reset -> active_device_count becomes 0!
    let (st_reset, reset_event) = send_json(
        &restarted_router,
        "POST",
        &format!("/v1/owner/accounts/{alice_user_id}/devices/reset"),
        Some(&owner_token),
        &[],
        Some(json!({ "reason": "Customer requested stolen laptop wipe" })),
    )
    .await;
    assert_eq!(st_reset, StatusCode::OK);
    assert_eq!(reset_event["trigger_kind"], json!("owner_reset"));
    assert_eq!(
        restarted_state
            .device_authority
            .active_device_count(alice_user_id)
            .await,
        0
    );
    assert!(restarted_state.log_center.verify_chain().await);
}

#[tokio::test]
async fn subscription_chargeback_coupons_concurrency_control_center_and_restart_suite() {
    let journal_path = std::env::temp_dir().join(format!(
        "safi-billing-journal-{}.json",
        Uuid::now_v7().simple()
    ));

    let accounts_repo = Arc::new(TestAccountRepo::default());
    let mailer = Arc::new(TestEmailSender::default());
    let sessions_repo = Arc::new(InMemorySessionRepository::new());
    let auth = AuthService::new(
        Arc::new(InMemoryApiKeyRepository::default()),
        sessions_repo.clone(),
        Arc::new(InMemoryAuthCache::new()),
    );
    let state = AppState::with_config(auth, test_config())
        .with_durable_journal(&journal_path, Some(sessions_repo.clone()))
        .await
        .expect("journal");
    let account_auth = AccountAuthService::new(
        accounts_repo.clone(),
        mailer.clone(),
        state.auth.clone(),
        "https://app.safidata.example".to_owned(),
    );
    register_and_verify(
        &account_auth,
        &mailer,
        "owner2@example.com",
        "OwnerPass!1234",
    )
    .await;
    register_and_verify(&account_auth, &mailer, "bob@example.com", "BobPass!123456").await;
    let state = state.with_account_auth(account_auth);
    let router = app(state.clone());

    // Login Owner + enroll & confirm TOTP.
    let (_, owner_login) = send_json(
        &router,
        "POST",
        "/v1/auth/login",
        None,
        &[],
        Some(json!({
            "email": "owner2@example.com",
            "password": "OwnerPass!1234",
            "device_name": "Owner Box",
            "installation_id": Uuid::now_v7(),
            "platform": "windows",
        })),
    )
    .await;
    let owner_token = owner_login["token"].as_str().expect("ot").to_owned();
    let owner_uid = Uuid::parse_str(
        owner_login["lease"]["payload"]["user_id"]
            .as_str()
            .expect("uid"),
    )
    .expect("uuid");
    state
        .owner_mfa
        .bootstrap_owner_once(owner_uid, true)
        .await
        .expect("boot");
    let (_, enroll) = send_json(
        &router,
        "POST",
        "/v1/owner/mfa/enroll",
        Some(&owner_token),
        &[],
        None,
    )
    .await;
    let secret = decode_base32(enroll["manual_secret_base32"].as_str().expect("b32")).expect("b32");
    let code = compute_totp_code(&secret, unix_now() / TOTP_PERIOD_SECS).expect("totp");
    let (st_c, _) = send_json(
        &router,
        "POST",
        "/v1/owner/mfa/confirm",
        Some(&owner_token),
        &[],
        Some(json!({ "code": code })),
    )
    .await;
    assert_eq!(st_c, StatusCode::OK);

    // Login Bob (Free plan).
    let (_, bob_login) = send_json(
        &router,
        "POST",
        "/v1/auth/login",
        None,
        &[],
        Some(json!({
            "email": "bob@example.com",
            "password": "BobPass!123456",
            "device_name": "Bob PC",
            "installation_id": Uuid::now_v7(),
            "platform": "windows",
        })),
    )
    .await;
    let bob_token = bob_login["token"].as_str().expect("bt").to_owned();
    let bob_uid = Uuid::parse_str(
        bob_login["lease"]["payload"]["user_id"]
            .as_str()
            .expect("uid"),
    )
    .expect("uuid");
    let bob_ws = Uuid::parse_str(
        bob_login["lease"]["payload"]["workspace_id"]
            .as_str()
            .expect("ws"),
    )
    .expect("uuid");

    // 1. Free user attempting AI call -> blocked (SF-LIC-008)!
    let (st_ai_free, ai_free_body) = send_json(
        &router,
        "POST",
        "/v1/ai/route",
        Some(&bob_token),
        &[],
        Some(json!({
            "idempotency_key": "ai-free-1",
            "explicit_user_consent": true,
            "pii_masked_locally": true,
            "masked_prompt": "Summarize totals"
        })),
    )
    .await;
    assert_eq!(st_ai_free, StatusCode::FORBIDDEN);
    assert_eq!(ai_free_body["code"], json!("SF-LIC-008"));

    // 2. Owner creates LAUNCH25 (25% off, cap 1) & EXPIRED10 (expired).
    let now = unix_now();
    let (st_cp, _) = send_json(
        &router,
        "POST",
        "/v1/owner/coupons",
        Some(&owner_token),
        &[],
        Some(json!({
            "code": "LAUNCH25",
            "discount_kind": "percent",
            "discount_value": 25,
            "expires_at": now + 86400,
            "max_redemptions": 1
        })),
    )
    .await;
    assert_eq!(st_cp, StatusCode::CREATED);

    let (st_exp_cp, _) = send_json(
        &router,
        "POST",
        "/v1/owner/coupons",
        Some(&owner_token),
        &[],
        Some(json!({
            "code": "EXPIRED10",
            "discount_kind": "percent",
            "discount_value": 10,
            "expires_at": now - 60,
        })),
    )
    .await;
    assert_eq!(st_exp_cp, StatusCode::CREATED);

    // Expired coupon rejected with SF-PAY-005.
    let (st_prev_exp, prev_exp) = send_json(
        &router,
        "POST",
        "/v1/billing/coupons/preview",
        Some(&bob_token),
        &[],
        Some(json!({
            "code": "EXPIRED10",
            "product": "individual_monthly",
            "billing_country_iso2": "US"
        })),
    )
    .await;
    assert_eq!(st_prev_exp, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(prev_exp["code"], json!("SF-PAY-005"));

    // Valid coupon preview computes exact server-side 25% off $20 = $15 (1500 cents).
    let (st_prev_ok, prev_ok) = send_json(
        &router,
        "POST",
        "/v1/billing/coupons/preview",
        Some(&bob_token),
        &[],
        Some(json!({
            "code": "LAUNCH25",
            "product": "individual_monthly",
            "billing_country_iso2": "US"
        })),
    )
    .await;
    assert_eq!(st_prev_ok, StatusCode::OK);
    assert_eq!(prev_ok["original_amount_minor"], json!(2000));
    assert_eq!(prev_ok["discount_amount_minor"], json!(500));
    assert_eq!(prev_ok["final_amount_minor"], json!(1500));

    // 3. 10-way concurrent coupon redemption race for max_redemptions=1 -> strictly 1 succeeds!
    let mut race = Vec::new();
    for _ in 0..10 {
        let r = router.clone();
        let tok = bob_token.clone();
        race.push(tokio::spawn(async move {
            send_json(
                &r,
                "POST",
                "/v1/billing/checkout",
                Some(&tok),
                &[],
                Some(json!({
                    "workspace_id": Uuid::nil(),
                    "user_id": Uuid::nil(),
                    "product": "individual_monthly",
                    "billing_country_iso2": "US",
                    "coupon_code": "LAUNCH25"
                })),
            )
            .await
        }));
    }
    let mut succeeded = 0usize;
    let mut rejected_coupon = 0usize;
    for h in race {
        let (st, val) = h.await.expect("join");
        if st == StatusCode::OK {
            assert_eq!(val["amount_minor"], json!(1500));
            assert_eq!(val["discount_amount_minor"], json!(500));
            succeeded += 1;
        } else if val["code"] == json!("SF-PAY-005") {
            rejected_coupon += 1;
        }
    }
    assert_eq!(succeeded, 1);
    assert_eq!(rejected_coupon, 9);

    // 4. Signed Paddle activation webhook -> Bob becomes IndividualMonthly & AI call succeeds!
    let act_body = serde_json::to_vec(&json!({
        "event_id": "evt_bob_act_1",
        "event_type": "subscription.activated",
        "occurred_at_unix": now,
        "workspace_id": bob_ws,
        "user_id": bob_uid,
        "product": "individual_monthly",
        "period_end_unix": now + 30 * 86400,
        "amount_minor": 1500,
        "invoice_id": "inv_bob_001"
    }))
    .expect("json");
    let act_sig = sign_paddle_payload("pdl_whsec_test_999", now, &act_body).expect("sig");
    let req_act = Request::builder()
        .method("POST")
        .uri("/v1/billing/webhooks/paddle")
        .header("paddle-signature", &act_sig)
        .body(Body::from(act_body.clone()))
        .expect("req");
    let resp_act = router.clone().oneshot(req_act).await.expect("oneshot");
    assert_eq!(resp_act.status(), StatusCode::OK);

    // Forged webhook signature rejected (SF-PAY-002).
    let req_bad_wh = Request::builder()
        .method("POST")
        .uri("/v1/billing/webhooks/paddle")
        .header("paddle-signature", format!("ts={now};h1=0000bad"))
        .body(Body::from(act_body))
        .expect("req");
    let resp_bad_wh = router.clone().oneshot(req_bad_wh).await.expect("oneshot");
    assert_eq!(resp_bad_wh.status(), StatusCode::UNAUTHORIZED);

    // Paid Bob can now call AI!
    let (st_ai_paid, _) = send_json(
        &router,
        "POST",
        "/v1/ai/route",
        Some(&bob_token),
        &[],
        Some(json!({
            "idempotency_key": "ai-paid-1",
            "explicit_user_consent": true,
            "pii_masked_locally": true,
            "masked_prompt": "Analyze sales trend"
        })),
    )
    .await;
    assert_eq!(st_ai_paid, StatusCode::OK);

    // 5. Paddle Chargeback webhook -> invoice marked "chargeback" & paid access IMMEDIATELY revoked!
    let cb_body = serde_json::to_vec(&json!({
        "event_id": "evt_bob_cb_2",
        "event_type": "adjustment.chargeback",
        "occurred_at_unix": now + 5,
        "workspace_id": bob_ws,
        "user_id": bob_uid,
        "product": "individual_monthly",
        "amount_minor": 1500,
        "invoice_id": "inv_bob_001"
    }))
    .expect("json");
    let cb_sig = sign_paddle_payload("pdl_whsec_test_999", now + 5, &cb_body).expect("sig");
    let req_cb = Request::builder()
        .method("POST")
        .uri("/v1/billing/webhooks/paddle")
        .header("paddle-signature", &cb_sig)
        .body(Body::from(cb_body))
        .expect("req");
    let resp_cb = router.clone().oneshot(req_cb).await.expect("oneshot");
    assert_eq!(resp_cb.status(), StatusCode::OK);

    // Immediately after chargeback, Bob's AI call MUST be blocked with SF-LIC-008!
    let (st_ai_post_cb, post_cb_body) = send_json(
        &router,
        "POST",
        "/v1/ai/route",
        Some(&bob_token),
        &[],
        Some(json!({
            "idempotency_key": "ai-post-cb",
            "explicit_user_consent": true,
            "pii_masked_locally": true,
            "masked_prompt": "Should be blocked"
        })),
    )
    .await;
    assert_eq!(st_ai_post_cb, StatusCode::FORBIDDEN);
    assert_eq!(post_cb_body["code"], json!("SF-LIC-008"));

    // 6. Owner Control Center 14-section APIs return live server data (Profile, Tickets, Announcements, Product Config, Stats).
    let (st_prof, prof) = send_json(
        &router,
        "GET",
        &format!("/v1/owner/accounts/{bob_uid}/profile"),
        Some(&owner_token),
        &[],
        None,
    )
    .await;
    assert_eq!(st_prof, StatusCode::OK);
    assert_eq!(prof["invoices"][0]["status"], json!("chargeback"));

    let (st_tkt, _) = send_json(
        &router,
        "POST",
        "/v1/owner/support/tickets",
        Some(&owner_token),
        &[],
        Some(json!({
            "user_id": bob_uid,
            "workspace_id": bob_ws,
            "subject": "Chargeback review",
            "body": "Follow up with payment processor"
        })),
    )
    .await;
    assert_eq!(st_tkt, StatusCode::CREATED);

    let (st_ann, _) = send_json(
        &router,
        "POST",
        "/v1/owner/announcements",
        Some(&owner_token),
        &[],
        Some(json!({
            "title": "Desktop 1.0 Security Update",
            "body": "Single-device authorization is active."
        })),
    )
    .await;
    assert_eq!(st_ann, StatusCode::CREATED);

    let (st_cfg, cfg_val) = send_json(
        &router,
        "GET",
        "/v1/owner/product-config",
        Some(&owner_token),
        &[],
        None,
    )
    .await;
    assert_eq!(st_cfg, StatusCode::OK);
    assert_eq!(cfg_val["online_lease_ttl_secs"], json!(60));

    // 7. Suspend Bob -> Bob's authorize & login fail with SF-AUTH-009 (AccountSuspended)!
    let (st_susp, _) = send_json(
        &router,
        "POST",
        &format!("/v1/owner/accounts/{bob_uid}/suspend"),
        Some(&owner_token),
        &[],
        None,
    )
    .await;
    assert_eq!(st_susp, StatusCode::OK);

    let (st_bob_relogin, bob_relogin_body) = send_json(
        &router,
        "POST",
        "/v1/auth/login",
        None,
        &[],
        Some(json!({
            "email": "bob@example.com",
            "password": "BobPass!123456",
            "device_name": "Bob PC 2",
            "installation_id": Uuid::now_v7(),
            "platform": "windows",
        })),
    )
    .await;
    assert_eq!(st_bob_relogin, StatusCode::FORBIDDEN);
    assert_eq!(bob_relogin_body["code"], json!("SF-AUTH-009"));

    // 8. Non-owner attempting Owner Control Center endpoints -> rejected!
    let (st_forbid, _) = send_json(
        &router,
        "GET",
        "/v1/owner/stats",
        Some(&bob_token),
        &[],
        None,
    )
    .await;
    assert!(st_forbid == StatusCode::UNAUTHORIZED || st_forbid == StatusCode::FORBIDDEN);

    // 9. Restart server from durable journal -> coupons, chargeback invoice, tickets, announcements & stats survive!
    drop(router);
    drop(state);
    let restarted_sessions = Arc::new(InMemorySessionRepository::new());
    let restarted_auth = AuthService::new(
        Arc::new(InMemoryApiKeyRepository::default()),
        restarted_sessions.clone(),
        Arc::new(InMemoryAuthCache::new()),
    );
    let restarted_state = AppState::with_config(restarted_auth, test_config())
        .with_durable_journal(&journal_path, Some(restarted_sessions))
        .await
        .expect("restarted");
    let restarted_router = app(restarted_state.clone());

    let (st_stats, stats_after) = send_json(
        &restarted_router,
        "GET",
        "/v1/owner/stats",
        Some(&owner_token),
        &[],
        None,
    )
    .await;
    assert_eq!(st_stats, StatusCode::OK);
    assert_eq!(stats_after["total_coupons"], json!(2));
    assert_eq!(stats_after["total_coupon_redemptions"], json!(1));
    assert_eq!(stats_after["revenue"]["chargeback_invoices"], json!(1));
    assert_eq!(stats_after["open_support_tickets"], json!(1));
    assert_eq!(stats_after["suspended_users"], json!(1));
    assert!(restarted_state.log_center.verify_chain().await);
}
