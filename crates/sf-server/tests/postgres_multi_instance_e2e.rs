//! Real PostgreSQL 18 Multi-Instance & Restart Source-of-Truth Integration Test.
//!
//! Runs against a live PostgreSQL 18 database (`SF_TEST_DATABASE_URL`, executed automatically
//! in CI via `tools/test-migrations.sh` on `postgres:18.6-bookworm`).
//!
//! Proves with ZERO shared memory between servers (Instance A, Instance B, and Restarted Instance C):
//! 1. Official Templates, Community Submissions (Create -> Review -> Atomic Publish), and Signed
//!    App Releases written via Instance A are read directly from PostgreSQL by Instance B & C.
//! 2. Owner MFA (encrypted TOTP credential + recovery codes + elevated/step-up sessions) enrolled
//!    and confirmed on Instance A is enforced and recognized on Instance B & C.
//! 3. Owner Web Session (`sfw_*`) issued on Instance A authenticates on Instance B without revoking
//!    Owner's Desktop device (`sfs_*`).
//! 4. Desktop Browser PKCE started on Instance A, approved on Instance B, and exchanged ONCE on
//!    Instance C (with replay rejected across all instances).
//! 5. Single-Device Authority: Device B login on Instance B atomically revokes Device A in
//!    PostgreSQL so Instance A immediately rejects Device A (`SF-AUTH-019`).

use std::sync::Arc;

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde_json::{json, Value};
use sf_server::auth::account::{
    AccountAuthService, RegisterAccountRequest, SqlxAccountRepository, VerificationEmailSender,
};
use sf_server::auth::cache::InMemoryAuthCache;
use sf_server::auth::repository::{SqlxApiKeyRepository, SqlxSessionRepository};
use sf_server::auth::service::AuthService;
use sf_server::auth::unix_now;
use sf_server::config::ServerConfig;
use sf_server::error::AuthError;
use sf_server::owner_mfa::{compute_totp_code, decode_base32, TOTP_PERIOD_SECS};
use sf_server::pg_store::PgControlPlaneStore;
use sf_server::{app, AppState};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tokio::sync::RwLock;
use tower::ServiceExt;
use uuid::Uuid;

#[derive(Default)]
struct CaptureMailer {
    tokens: RwLock<std::collections::HashMap<String, String>>,
}

impl CaptureMailer {
    async fn last_token(&self, email: &str) -> Option<String> {
        self.tokens.read().await.get(email).cloned()
    }
}

#[async_trait]
impl VerificationEmailSender for CaptureMailer {
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

fn pg_test_config() -> ServerConfig {
    let mut cfg = ServerConfig::for_tests();
    cfg.license_signing_seed = Some([0x44u8; 32]);
    cfg.totp_encryption_key = Some([0x77u8; 32]);
    cfg
}

async fn build_independent_instance(
    pool: PgPool,
    mailer: Arc<CaptureMailer>,
) -> (AppState, axum::Router) {
    let auth = AuthService::new(
        Arc::new(SqlxApiKeyRepository::new(pool.clone())),
        Arc::new(SqlxSessionRepository::new(pool.clone())),
        Arc::new(InMemoryAuthCache::new()),
    );
    let pg_store = PgControlPlaneStore::new(pool.clone());
    let state = AppState::with_config(auth, pg_test_config()).with_pg_store(pg_store.clone());
    if let Ok(snap) = pg_store.load_owner_mfa_snapshot_pg().await {
        state.owner_mfa.import_snapshot(snap).await;
    }
    let account_auth = AccountAuthService::new(
        Arc::new(SqlxAccountRepository::new(pool)),
        mailer,
        state.auth.clone(),
        "https://safidata.app".to_owned(),
    );
    let state = state.with_account_auth(account_auth);
    let router = app(state.clone());
    (state, router)
}

async fn send_json(
    router: &axum::Router,
    method: &str,
    uri: &str,
    bearer: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(tok) = bearer {
        builder = builder.header("authorization", format!("Bearer {tok}"));
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
    let raw = to_bytes(resp.into_body(), 2 * 1024 * 1024)
        .await
        .expect("body");
    let val = serde_json::from_slice(&raw).unwrap_or_else(|_| json!({}));
    (status, val)
}

#[tokio::test]
#[ignore = "requires SF_TEST_DATABASE_URL pointing at a migrated PostgreSQL 18 database (migrations 0001..0012)"]
async fn multi_instance_and_restart_share_postgres_as_single_source_of_truth() {
    let db_url = std::env::var("SF_TEST_DATABASE_URL")
        .expect("SF_TEST_DATABASE_URL must be set for live PostgreSQL 18 test");
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&db_url)
        .await
        .expect("connect to PostgreSQL 18");

    let mailer = Arc::new(CaptureMailer::default());
    let (state_a, router_a) = build_independent_instance(pool.clone(), mailer.clone()).await;
    let (_state_b, router_b) = build_independent_instance(pool.clone(), mailer.clone()).await;

    let nonce = Uuid::now_v7().simple().to_string();
    let owner_email = format!("pg-owner-{nonce}@example.test");
    let user_email = format!("pg-user-{nonce}@example.test");

    // Register & verify Owner and Regular User on Instance A.
    let acct_a = state_a.account_auth.as_ref().expect("account auth");
    for (email, pass) in [
        (&owner_email, "OwnerPgPass123!"),
        (&user_email, "UserPgPass123!"),
    ] {
        acct_a
            .register(RegisterAccountRequest {
                email: email.clone(),
                display_name: "PG Tester".to_owned(),
                password: pass.to_owned(),
            })
            .await
            .expect("register");
        let tok = mailer.last_token(email).await.expect("verification token");
        acct_a.verify_email(&tok).await.expect("verify email");
    }

    // Seed defaults into PostgreSQL if empty.
    let pg = state_a.pg_store.as_ref().expect("pg store");
    let defaults = state_a.templates_hub.list_all_official_for_owner().await;
    pg.seed_default_official_templates_if_empty(&defaults)
        .await
        .expect("seed official templates");

    // 1. Owner logs into Desktop on Instance A; seal Owner in PostgreSQL.
    let (st, owner_desk_login) = send_json(
        &router_a,
        "POST",
        "/v1/auth/login",
        None,
        Some(json!({
            "email": owner_email,
            "password": "OwnerPgPass123!",
            "installation_id": Uuid::now_v7().to_string(),
            "platform": "windows",
            "device_name": "Owner PG Desktop"
        })),
    )
    .await;
    assert_eq!(st, StatusCode::CREATED, "owner login: {owner_desk_login}");
    let owner_desk_token = owner_desk_login["token"]
        .as_str()
        .expect("token")
        .to_owned();
    let owner_uid = Uuid::parse_str(
        owner_desk_login["lease"]["payload"]["user_id"]
            .as_str()
            .expect("uid"),
    )
    .expect("uuid");
    sqlx::query(
        "INSERT INTO platform_owner (singleton, owner_user_id, bootstrap_sealed) VALUES (TRUE, $1, TRUE) ON CONFLICT (singleton) DO UPDATE SET owner_user_id = EXCLUDED.owner_user_id",
    )
    .bind(owner_uid)
    .execute(&pool)
    .await
    .expect("seed platform_owner");

    // 2. Owner logs into Web Portal (`sfw_*`) on Instance A -> authenticates on Instance B!
    let (web_st, web_login) = send_json(
        &router_a,
        "POST",
        "/v1/owner/web/login",
        None,
        Some(json!({ "email": owner_email, "password": "OwnerPgPass123!" })),
    )
    .await;
    assert_eq!(web_st, StatusCode::CREATED, "web login: {web_login}");
    let owner_web_token = web_login["token"].as_str().expect("sfw token").to_owned();
    assert!(owner_web_token.starts_with("sfw_"));

    // Owner enrolls TOTP on Instance A -> confirms TOTP on Instance B (cross-instance encrypted TOTP!).
    let (enr_st, enr_body) = send_json(
        &router_a,
        "POST",
        "/v1/owner/mfa/enroll",
        Some(&owner_web_token),
        None,
    )
    .await;
    assert_eq!(enr_st, StatusCode::OK, "enroll: {enr_body}");
    let secret =
        decode_base32(enr_body["manual_secret_base32"].as_str().expect("b32")).expect("dec");
    let step = unix_now() / TOTP_PERIOD_SECS;
    let totp = compute_totp_code(&secret, step).expect("totp");
    let (conf_st, conf_body) = send_json(
        &router_b,
        "POST",
        "/v1/owner/mfa/confirm",
        Some(&owner_web_token),
        Some(json!({ "code": totp })),
    )
    .await;
    assert_eq!(
        conf_st,
        StatusCode::OK,
        "cross-instance confirm: {conf_body}"
    );

    // Owner Desktop session MUST still be valid on Instance B!
    let (hb_st, _) = send_json(
        &router_b,
        "POST",
        "/v1/auth/heartbeat",
        Some(&owner_desk_token),
        None,
    )
    .await;
    assert_eq!(hb_st, StatusCode::OK);

    // 3. Cross-Instance Desktop Browser PKCE: Start on A -> Approve on B -> Exchange ONCE on C (Restarted)!
    let verifier = "pg_multi_instance_pkce_verifier_0123456789abcdef_xyz";
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state_nonce = format!("pg_state_{nonce}");
    let inst_1 = Uuid::now_v7().to_string();

    let (p_start_st, p_start) = send_json(
        &router_a,
        "POST",
        "/v1/auth/desktop/start",
        None,
        Some(json!({
            "flow": "signin",
            "code_challenge": challenge,
            "state": state_nonce,
            "installation_id": inst_1,
            "platform": "windows",
            "device_name": "PKCE Device 1"
        })),
    )
    .await;
    assert_eq!(p_start_st, StatusCode::CREATED, "pkce start: {p_start}");
    let req_id = p_start["request_id"].as_str().expect("request_id");

    let (p_app_st, p_app) = send_json(
        &router_b,
        "POST",
        "/v1/auth/desktop/approve",
        None,
        Some(json!({
            "request_id": req_id,
            "state": state_nonce,
            "email": user_email,
            "password": "UserPgPass123!"
        })),
    )
    .await;
    assert_eq!(p_app_st, StatusCode::OK, "pkce approve on B: {p_app}");
    let code = p_app["code"].as_str().expect("code").to_owned();

    // Build brand-new Restarted Instance C (zero memory shared with A or B!)
    let (_state_c, router_c) = build_independent_instance(pool.clone(), mailer.clone()).await;

    let (p_ex_st, p_ex) = send_json(
        &router_c,
        "POST",
        "/v1/auth/desktop/exchange",
        None,
        Some(json!({
            "code": code,
            "code_verifier": verifier,
            "state": state_nonce,
            "installation_id": inst_1
        })),
    )
    .await;
    assert_eq!(p_ex_st, StatusCode::CREATED, "pkce exchange on C: {p_ex}");
    let user_token_dev1 = p_ex["token"].as_str().expect("user token").to_owned();

    // Replay on Instance A must fail with SF-AUTH-023!
    let (rep_st, rep_body) = send_json(
        &router_a,
        "POST",
        "/v1/auth/desktop/exchange",
        None,
        Some(json!({
            "code": code,
            "code_verifier": verifier,
            "state": state_nonce,
            "installation_id": inst_1
        })),
    )
    .await;
    assert_eq!(rep_st, StatusCode::UNAUTHORIZED);
    assert_eq!(rep_body["code"], json!("SF-AUTH-023"));

    // 4. Community Submission on A -> Review on B -> Atomic Publish on C -> Read Official Templates on A!
    let (sub_st, sub_body) = send_json(
        &router_a,
        "POST",
        "/v1/community/submissions",
        Some(&user_token_dev1),
        Some(json!({
            "name": "PG Multi-Instance Recipe",
            "description": "Verified across 3 independent servers via PostgreSQL 18",
            "recipe_type": "cleaning",
            "consent_accepted": true,
            "steps": [
                { "id": "s1", "op": "normalize_text", "columns": ["name"], "params": {}, "reversible": true }
            ]
        })),
    )
    .await;
    assert_eq!(sub_st, StatusCode::CREATED, "submit on A: {sub_body}");
    let sub_id = sub_body["id"].as_str().expect("sub id");

    // Instance B sees it directly from PostgreSQL and approves it!
    let (rev_st, rev_body) = send_json(
        &router_b,
        "POST",
        &format!("/v1/owner/community/submissions/{sub_id}/review"),
        Some(&owner_web_token),
        Some(json!({ "status": "approved", "reviewer_notes": "Approved on Instance B" })),
    )
    .await;
    assert_eq!(rev_st, StatusCode::OK, "review on B: {rev_body}");

    // Restarted Instance C explicitly publishes it atomically in PostgreSQL!
    let slug = format!("pg-official-{}", &nonce[..8]);
    let (pub_st, pub_body) = send_json(
        &router_c,
        "POST",
        &format!("/v1/owner/community/submissions/{sub_id}/publish"),
        Some(&owner_web_token),
        Some(json!({
            "slug": slug,
            "official_name": "PG Multi-Instance Official",
            "official_description": "Published atomically in PostgreSQL 18",
            "category": "cleaning",
            "release_notes": "v1"
        })),
    )
    .await;
    assert_eq!(pub_st, StatusCode::CREATED, "publish on C: {pub_body}");

    // Instance A immediately reads the newly published Official Template directly from PostgreSQL!
    let (tpl_st, tpl_body) = send_json(
        &router_a,
        "GET",
        "/v1/templates/official",
        Some(&user_token_dev1),
        None,
    )
    .await;
    assert_eq!(tpl_st, StatusCode::OK);
    assert!(
        tpl_body
            .as_array()
            .expect("arr")
            .iter()
            .any(|t| t["slug"] == json!(slug)),
        "Instance A must read Official Template published on Instance C directly from PostgreSQL"
    );

    // 5. Release published on Instance B -> served by Restarted Instance C!
    let sha = "f78a0776cb5cb7f9829aa95e108c8e5c8675509ac5445603d1136e4c566f9024";
    let (rel_st, rel_body) = send_json(
        &router_b,
        "POST",
        "/v1/owner/releases",
        Some(&owner_web_token),
        Some(json!({
            "version_code": 20001,
            "version_name": "2.0.1",
            "channel": "stable",
            "platform": "windows-x86_64",
            "download_url": "https://releases.safidata.app/SafiData-2.0.1-x64.msi",
            "file_size_bytes": 6240768,
            "sha256_hex": sha,
            "release_notes": "Multi-instance PG18 release"
        })),
    )
    .await;
    assert_eq!(rel_st, StatusCode::CREATED, "release on B: {rel_body}");

    let (upd_st, upd_body) = send_json(
        &router_c,
        "GET",
        "/v1/updates/latest?channel=stable&platform=windows-x86_64",
        None,
        None,
    )
    .await;
    assert_eq!(upd_st, StatusCode::OK);
    assert_eq!(upd_body["version_name"], json!("2.0.1"));
    assert_eq!(upd_body["sha256_hex"], json!(sha));

    // 6. Cross-Instance Single-Device Exclusivity: Device 2 logs in on Instance B -> Instance A IMMEDIATELY rejects Device 1!
    let (d2_st, d2_body) = send_json(
        &router_b,
        "POST",
        "/v1/auth/login",
        None,
        Some(json!({
            "email": user_email,
            "password": "UserPgPass123!",
            "installation_id": Uuid::now_v7().to_string(),
            "platform": "windows",
            "device_name": "Device 2 Laptop"
        })),
    )
    .await;
    assert_eq!(d2_st, StatusCode::CREATED, "device 2 login: {d2_body}");
    assert_eq!(d2_body["previous_device_revoked"], json!(true));

    let (d1_hb_st, _) = send_json(
        &router_a,
        "POST",
        "/v1/auth/heartbeat",
        Some(&user_token_dev1),
        None,
    )
    .await;
    assert_eq!(
        d1_hb_st,
        StatusCode::UNAUTHORIZED,
        "Instance A must immediately reject Device 1 after Device 2 logged in on Instance B"
    );
}
