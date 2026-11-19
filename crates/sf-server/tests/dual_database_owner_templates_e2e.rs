//! Mandatory End-to-End Acceptance Suite for:
//! - Dual Database Access Matrix (Regular vs Owner Central Access)
//! - Owner Web Session (`sfw_*`) vs Desktop Installation (`sfs_*`) Isolation & Live Sync
//! - Desktop Browser PKCE (`S256`) Authentication + Replay/Mismatch Guards
//! - Premium `.safitemplate` Entitlement Gate (Paid/VIP/Owner allowed; Free & Trial rejected `SF-RCP-005`)
//! - Official Versioned Templates, Community Contributions (`Share with SafiData` consent + secret/row blocking),
//!   Owner Review + Separate Explicit Publish + Rollback + Signed Releases + BLAKE3 Audit Chain + Restart Durability.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde_json::{json, Value};
use sf_server::auth::account::{
    AccountAuthService, AccountRepository, LoginAccount, RegisterAccountRequest,
    VerificationEmailSender,
};
use sf_server::auth::cache::InMemoryAuthCache;
use sf_server::auth::repository::{InMemoryApiKeyRepository, InMemorySessionRepository};
use sf_server::auth::service::AuthService;
use sf_server::auth::unix_now;
use sf_server::config::ServerConfig;
use sf_server::error::AuthError;
use sf_server::owner_mfa::{compute_totp_code, decode_base32, TOTP_PERIOD_SECS};
use sf_server::{app, AppState};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;
use tower::ServiceExt;
use uuid::Uuid;

#[derive(Default)]
struct MemAccountState {
    accounts: HashMap<String, LoginAccount>,
    pending_tokens: HashMap<[u8; 32], (String, i64)>,
}

#[derive(Default)]
struct MemAccountRepo {
    state: RwLock<MemAccountState>,
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
struct CaptureMailer {
    tokens: RwLock<HashMap<String, String>>,
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

fn test_config() -> ServerConfig {
    let mut cfg = ServerConfig::for_tests();
    cfg.license_signing_seed = Some([0x55u8; 32]);
    cfg
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

async fn register_and_verify(
    account_auth: &AccountAuthService,
    mailer: &CaptureMailer,
    email: &str,
    password: &str,
) {
    account_auth
        .register(RegisterAccountRequest {
            email: email.to_owned(),
            display_name: email.split('@').next().unwrap_or("User").to_owned(),
            password: password.to_owned(),
        })
        .await
        .expect("register");
    let tok = mailer.last_token(email).await.expect("verification token");
    account_auth.verify_email(&tok).await.expect("verify");
}

fn pkce_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

fn valid_declarative_steps() -> Value {
    json!([
        { "id": "s1", "op": "normalize_text", "columns": ["customer_name"], "params": {}, "reversible": true },
        { "id": "s2", "op": "dedupe", "columns": ["customer_name"], "params": {}, "reversible": true }
    ])
}

#[tokio::test]
async fn dual_database_owner_web_desktop_pkce_entitlements_community_and_restart_durability() {
    let journal_path = std::env::temp_dir().join(format!(
        "safi-dual-db-e2e-{}-{}.json",
        std::process::id(),
        Uuid::now_v7()
    ));

    let account_repo = Arc::new(MemAccountRepo::default());
    let session_repo = Arc::new(InMemorySessionRepository::new());
    let api_key_repo = Arc::new(InMemoryApiKeyRepository::default());
    let cache = Arc::new(InMemoryAuthCache::new());
    let mailer = Arc::new(CaptureMailer::default());

    let auth = AuthService::new(api_key_repo.clone(), session_repo.clone(), cache.clone());
    let state = AppState::with_config(auth, test_config())
        .with_durable_journal(&journal_path, Some(session_repo.clone()))
        .await
        .expect("journal");
    let account_auth = AccountAuthService::new(
        account_repo.clone(),
        mailer.clone(),
        state.auth.clone(),
        "https://safidata.app".to_owned(),
    );

    // 1. Register Owner + 4 regular accounts (Free, Trial, VIP, Paid)
    register_and_verify(&account_auth, &mailer, "owner@test.local", "OwnerPass123!").await;
    register_and_verify(&account_auth, &mailer, "free@test.local", "UserPass123!").await;
    register_and_verify(&account_auth, &mailer, "trial@test.local", "UserPass123!").await;
    register_and_verify(&account_auth, &mailer, "vip@test.local", "UserPass123!").await;
    register_and_verify(&account_auth, &mailer, "paid@test.local", "UserPass123!").await;

    let state = state.with_account_auth(account_auth);
    let router = app(state.clone());

    // 2. Owner signs into Desktop (`sfs_*`) and enrolls Google Authenticator TOTP
    let owner_inst = Uuid::now_v7().to_string();
    let (status, owner_desktop_login) = send_json(
        &router,
        "POST",
        "/v1/auth/login",
        None,
        Some(json!({
            "email": "owner@test.local",
            "password": "OwnerPass123!",
            "installation_id": owner_inst,
            "platform": "windows",
            "device_name": "Owner Workstation"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let owner_desktop_token = owner_desktop_login["token"]
        .as_str()
        .expect("desktop token")
        .to_owned();
    let owner_user_id = Uuid::parse_str(
        owner_desktop_login["lease"]["payload"]["user_id"]
            .as_str()
            .expect("uid"),
    )
    .expect("uuid");
    state
        .owner_mfa
        .bootstrap_owner_once(owner_user_id, true)
        .await
        .expect("bootstrap owner");

    // 3. Owner ALSO signs into protected Website Owner Portal (`POST /v1/owner/web/login` -> `sfw_*`)
    let (status, owner_web_login) = send_json(
        &router,
        "POST",
        "/v1/owner/web/login",
        None,
        Some(json!({
            "email": "owner@test.local",
            "password": "OwnerPass123!"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let owner_web_token = owner_web_login["token"]
        .as_str()
        .expect("web token")
        .to_owned();
    assert!(owner_web_token.starts_with("sfw_"));
    assert_eq!(owner_web_login["is_owner"], json!(true));

    // CRITICAL: Owner's Desktop session MUST remain ACTIVE and NOT revoked by Owner Web Login!
    let (hb_status, hb_body) = send_json(
        &router,
        "POST",
        "/v1/auth/heartbeat",
        Some(&owner_desktop_token),
        None,
    )
    .await;
    assert_eq!(
        hb_status,
        StatusCode::OK,
        "Owner Desktop must remain active after Owner Web login: {hb_body}"
    );

    // Regular user trying Website Owner Portal login MUST be rejected with 403 SF-AUTH-008!
    let (non_owner_web_status, non_owner_web_body) = send_json(
        &router,
        "POST",
        "/v1/owner/web/login",
        None,
        Some(json!({
            "email": "free@test.local",
            "password": "UserPass123!"
        })),
    )
    .await;
    assert_eq!(non_owner_web_status, StatusCode::FORBIDDEN);
    assert_eq!(non_owner_web_body["code"], json!("SF-AUTH-013"));

    // Enroll & confirm Google Authenticator TOTP on Owner Web Session (step - 1),
    // and independently verify TOTP on Owner Desktop Session (current step).
    let (enroll_status, enroll_body) = send_json(
        &router,
        "POST",
        "/v1/owner/mfa/enroll",
        Some(&owner_web_token),
        None,
    )
    .await;
    assert_eq!(enroll_status, StatusCode::OK);
    let secret = decode_base32(
        enroll_body["manual_secret_base32"]
            .as_str()
            .expect("base32"),
    )
    .expect("decode");
    let step = unix_now() / TOTP_PERIOD_SECS;
    let totp_web = compute_totp_code(&secret, step - 1).expect("totp web");
    let (confirm_status, _) = send_json(
        &router,
        "POST",
        "/v1/owner/mfa/confirm",
        Some(&owner_web_token),
        Some(json!({ "code": totp_web })),
    )
    .await;
    assert_eq!(confirm_status, StatusCode::OK);

    // Desktop Owner session MUST independently complete Google Authenticator TOTP MFA!
    let (desk_unmfa_st, _) = send_json(
        &router,
        "GET",
        "/v1/owner/community/submissions",
        Some(&owner_desktop_token),
        None,
    )
    .await;
    assert_eq!(desk_unmfa_st, StatusCode::UNAUTHORIZED);
    let totp_desk = compute_totp_code(&secret, step).expect("totp desktop");
    let (desk_verify_st, _) = send_json(
        &router,
        "POST",
        "/v1/owner/mfa/verify",
        Some(&owner_desktop_token),
        Some(json!({ "code": totp_desk })),
    )
    .await;
    assert_eq!(desk_verify_st, StatusCode::OK);

    // 4. Desktop Browser PKCE Auth (`start` -> `approve` -> `exchange`)
    let pkce_verifier = "safi_desktop_pkce_verifier_high_entropy_0123456789abcdef";
    let pkce_state = "safi_state_nonce_abcdef0123456789";
    let free_inst = Uuid::now_v7().to_string();
    let (start_status, start_body) = send_json(
        &router,
        "POST",
        "/v1/auth/desktop/start",
        None,
        Some(json!({
            "flow": "signin",
            "code_challenge": pkce_challenge(pkce_verifier),
            "code_challenge_method": "S256",
            "state": pkce_state,
            "installation_id": free_inst,
            "platform": "windows",
            "device_name": "Free User Desktop"
        })),
    )
    .await;
    assert_eq!(start_status, StatusCode::CREATED);
    let request_id = start_body["request_id"]
        .as_str()
        .expect("request_id")
        .to_owned();

    let (approve_status, approve_body) = send_json(
        &router,
        "POST",
        "/v1/auth/desktop/approve",
        None,
        Some(json!({
            "request_id": request_id,
            "state": pkce_state,
            "email": "free@test.local",
            "password": "UserPass123!"
        })),
    )
    .await;
    assert_eq!(approve_status, StatusCode::OK);
    let callback_url = approve_body["callback_uri"].as_str().expect("callback_uri");
    assert!(callback_url.starts_with("safi://auth?code=sfcode_"));
    assert!(
        !callback_url.contains("sfs_"),
        "Session token must NEVER appear in callback URL"
    );
    let code = approve_body["code"].as_str().expect("code").to_owned();

    // Wrong verifier -> 401 SF-AUTH-023
    let (bad_ver_status, bad_ver_body) = send_json(
        &router,
        "POST",
        "/v1/auth/desktop/exchange",
        None,
        Some(json!({
            "code": code,
            "code_verifier": "wrong_verifier_abcdefghijklmnopqrstuvwxyz_0123456789",
            "state": pkce_state,
            "installation_id": free_inst
        })),
    )
    .await;
    assert_eq!(bad_ver_status, StatusCode::UNAUTHORIZED);
    assert_eq!(bad_ver_body["code"], json!("SF-AUTH-023"));

    // Valid PKCE exchange -> 201 Created + real `sfs_*` token
    let (ex_status, ex_body) = send_json(
        &router,
        "POST",
        "/v1/auth/desktop/exchange",
        None,
        Some(json!({
            "code": code,
            "code_verifier": pkce_verifier,
            "state": pkce_state,
            "installation_id": free_inst
        })),
    )
    .await;
    assert_eq!(ex_status, StatusCode::CREATED);
    let free_token = ex_body["token"].as_str().expect("free token").to_owned();
    assert!(free_token.starts_with("sfs_"));

    // Replay of same PKCE code -> 401 SF-AUTH-023
    let (replay_status, replay_body) = send_json(
        &router,
        "POST",
        "/v1/auth/desktop/exchange",
        None,
        Some(json!({
            "code": code,
            "code_verifier": pkce_verifier,
            "state": pkce_state,
            "installation_id": free_inst
        })),
    )
    .await;
    assert_eq!(replay_status, StatusCode::UNAUTHORIZED);
    assert_eq!(replay_body["code"], json!("SF-AUTH-023"));

    // 5. Dual Database Access Matrix — Regular User Scoped Access vs Owner-Only Denials
    let (off_tpl_status, off_tpl_body) = send_json(
        &router,
        "GET",
        "/v1/templates/official",
        Some(&free_token),
        None,
    )
    .await;
    assert_eq!(off_tpl_status, StatusCode::OK);
    assert_eq!(
        off_tpl_body.as_array().expect("official templates").len(),
        2,
        "Free user sees only Free-tier official templates (2), not paid-only template"
    );
    let (own_tpl_status, own_tpl_body) = send_json(
        &router,
        "GET",
        "/v1/templates/official",
        Some(&owner_desktop_token),
        None,
    )
    .await;
    assert_eq!(own_tpl_status, StatusCode::OK);
    assert_eq!(
        own_tpl_body
            .as_array()
            .expect("owner official templates")
            .len(),
        3,
        "Owner/Paid sees all 3 official templates including paid-tier templates"
    );

    let (pub_ann_status, _) = send_json(
        &router,
        "GET",
        "/v1/announcements/public",
        Some(&free_token),
        None,
    )
    .await;
    assert_eq!(pub_ann_status, StatusCode::OK);

    for forbidden_owner_endpoint in [
        "/v1/owner/stats",
        "/v1/owner/accounts",
        "/v1/owner/community/submissions",
        "/v1/owner/templates/official",
        "/v1/owner/releases",
        "/v1/owner/audit-logs",
    ] {
        let (st, body) = send_json(
            &router,
            "GET",
            forbidden_owner_endpoint,
            Some(&free_token),
            None,
        )
        .await;
        assert_eq!(
            st,
            StatusCode::FORBIDDEN,
            "Regular user must be denied {forbidden_owner_endpoint}: {body}"
        );
    }

    // 6. Premium `.safitemplate` Entitlement Check (Free = DENIED, Trial = DENIED, VIP = ALLOWED, Paid = ALLOWED, Owner = ALLOWED)
    let (free_perm_st, free_perm_body) = send_json(
        &router,
        "POST",
        "/v1/templates/entitlement/export-import",
        Some(&free_token),
        None,
    )
    .await;
    assert_eq!(free_perm_st, StatusCode::FORBIDDEN);
    assert_eq!(free_perm_body["code"], json!("SF-RCP-005"));

    // Trial user logs in and starts trial -> STILL denied `.safitemplate`!
    let (trial_st, trial_login) = send_json(
        &router,
        "POST",
        "/v1/auth/login",
        None,
        Some(json!({
            "email": "trial@test.local",
            "password": "UserPass123!",
            "installation_id": Uuid::now_v7().to_string(),
            "platform": "windows",
            "device_name": "Trial Desktop"
        })),
    )
    .await;
    assert_eq!(trial_st, StatusCode::CREATED, "trial login: {trial_login}");
    let trial_token = trial_login["token"]
        .as_str()
        .expect("trial token")
        .to_owned();
    let trial_uid = Uuid::parse_str(
        trial_login["lease"]["payload"]["user_id"]
            .as_str()
            .expect("uid"),
    )
    .expect("uuid");
    let trial_ws = Uuid::parse_str(trial_login["session"]["workspace_id"].as_str().expect("ws"))
        .expect("uuid");
    state
        .entitlements
        .claim_trial_at(
            trial_ws,
            trial_uid,
            "trial_email_hash",
            "trial_phone_hash",
            "fp_trial_01",
            unix_now(),
        )
        .await
        .expect("start trial");
    let (trial_perm_st, trial_perm_body) = send_json(
        &router,
        "POST",
        "/v1/templates/entitlement/export-import",
        Some(&trial_token),
        None,
    )
    .await;
    assert_eq!(trial_perm_st, StatusCode::FORBIDDEN);
    assert_eq!(trial_perm_body["code"], json!("SF-RCP-005"));

    // Friend VIP user -> ALLOWED!
    let (vip_st, vip_login) = send_json(
        &router,
        "POST",
        "/v1/auth/login",
        None,
        Some(json!({
            "email": "vip@test.local",
            "password": "UserPass123!",
            "installation_id": Uuid::now_v7().to_string(),
            "platform": "windows",
            "device_name": "VIP Desktop"
        })),
    )
    .await;
    assert_eq!(vip_st, StatusCode::CREATED, "vip login: {vip_login}");
    let vip_token = vip_login["token"].as_str().expect("vip token").to_owned();
    let vip_uid = Uuid::parse_str(
        vip_login["lease"]["payload"]["user_id"]
            .as_str()
            .expect("uid"),
    )
    .expect("uuid");
    let vip_ws =
        Uuid::parse_str(vip_login["session"]["workspace_id"].as_str().expect("ws")).expect("uuid");
    state
        .entitlements
        .set_friend_vip(vip_ws, vip_uid, owner_user_id, true)
        .await;
    let (vip_perm_st, vip_perm_body) = send_json(
        &router,
        "POST",
        "/v1/templates/entitlement/export-import",
        Some(&vip_token),
        None,
    )
    .await;
    assert_eq!(vip_perm_st, StatusCode::OK);
    assert_eq!(vip_perm_body["allowed"], json!(true));

    // Paid subscriber -> ALLOWED!
    let (paid_st, paid_login) = send_json(
        &router,
        "POST",
        "/v1/auth/login",
        None,
        Some(json!({
            "email": "paid@test.local",
            "password": "UserPass123!",
            "installation_id": Uuid::now_v7().to_string(),
            "platform": "windows",
            "device_name": "Paid Desktop"
        })),
    )
    .await;
    assert_eq!(paid_st, StatusCode::CREATED, "paid login: {paid_login}");
    let paid_token = paid_login["token"].as_str().expect("paid token").to_owned();
    let paid_uid = Uuid::parse_str(
        paid_login["lease"]["payload"]["user_id"]
            .as_str()
            .expect("uid"),
    )
    .expect("uuid");
    let paid_ws =
        Uuid::parse_str(paid_login["session"]["workspace_id"].as_str().expect("ws")).expect("uuid");
    state
        .entitlements
        .apply_subscription_update(
            paid_ws,
            paid_uid,
            sf_server::entitlements::PlanTier::IndividualMonthly,
            sf_server::entitlements::SubscriptionState::Active,
            1,
            Some(unix_now() + 86_400 * 30),
            unix_now(),
        )
        .await
        .expect("activate paid plan");
    let (paid_perm_st, paid_perm_body) = send_json(
        &router,
        "POST",
        "/v1/templates/entitlement/export-import",
        Some(&paid_token),
        None,
    )
    .await;
    assert_eq!(paid_perm_st, StatusCode::OK);
    assert_eq!(paid_perm_body["allowed"], json!(true));

    // Owner -> ALLOWED!
    let (owner_perm_st, _) = send_json(
        &router,
        "POST",
        "/v1/templates/entitlement/export-import",
        Some(&owner_desktop_token),
        None,
    )
    .await;
    assert_eq!(owner_perm_st, StatusCode::OK);

    // 7. Community Contributions (`Share with SafiData`) Safety Checks + Cross-Interface Owner Sync + Separate Explicit Publish
    // Missing consent -> 400 SF-RCP-006
    let (no_consent_st, no_consent_body) = send_json(
        &router,
        "POST",
        "/v1/community/submissions",
        Some(&free_token),
        Some(json!({
            "name": "Invoice Cleaner",
            "description": "Cleans supplier invoices",
            "recipe_type": "cleaning",
            "consent_accepted": false,
            "steps": valid_declarative_steps()
        })),
    )
    .await;
    assert_eq!(no_consent_st, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(no_consent_body["code"], json!("SF-RCP-006"));

    // Smuggled customer rows -> 422 SF-RCP-006
    let (rows_st, rows_body) = send_json(
        &router,
        "POST",
        "/v1/community/submissions",
        Some(&free_token),
        Some(json!({
            "name": "Bad Row Leak",
            "description": "Contains customer rows",
            "recipe_type": "cleaning",
            "consent_accepted": true,
            "steps": valid_declarative_steps(),
            "rows": [["secret_salary", "90000"]]
        })),
    )
    .await;
    assert_eq!(rows_st, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(rows_body["code"], json!("SF-RCP-006"));

    // Embedded secret key -> 422 SF-RCP-006
    let (secret_st, secret_body) = send_json(
        &router,
        "POST",
        "/v1/community/submissions",
        Some(&free_token),
        Some(json!({
            "name": "Secret Leak",
            "description": "Contains sk_live_1234567890abcdef",
            "recipe_type": "cleaning",
            "consent_accepted": true,
            "steps": valid_declarative_steps()
        })),
    )
    .await;
    assert_eq!(secret_st, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(secret_body["code"], json!("SF-RCP-006"));

    // Valid submission -> 201 Created (`pending`), NOT in Official Library yet!
    let (sub_st, sub_body) = send_json(
        &router,
        "POST",
        "/v1/community/submissions",
        Some(&free_token),
        Some(json!({
            "name": "Cairo Payroll Normalizer",
            "description": "Normalizes Arabic employee names and payroll columns",
            "recipe_type": "cleaning",
            "consent_accepted": true,
            "steps": valid_declarative_steps()
        })),
    )
    .await;
    assert_eq!(sub_st, StatusCode::CREATED, "submission body: {sub_body}");
    assert_eq!(sub_body["status"], json!("pending"));
    let submission_id = sub_body["id"].as_str().expect("submission id").to_owned();

    // Owner reviews & approves from Website `/owner` (`owner_web_token`)
    let (rev_st, rev_body) = send_json(
        &router,
        "POST",
        &format!("/v1/owner/community/submissions/{submission_id}/review"),
        Some(&owner_web_token),
        Some(json!({
            "status": "approved",
            "reviewer_notes": "Clean declarative steps verified on Web Portal"
        })),
    )
    .await;
    assert_eq!(rev_st, StatusCode::OK, "review body: {rev_body}");
    assert_eq!(rev_body["status"], json!("approved"));

    // Approved does NOT automatically publish! Official library must not contain it yet.
    let (_, pre_pub_tpls) = send_json(
        &router,
        "GET",
        "/v1/templates/official",
        Some(&free_token),
        None,
    )
    .await;
    assert!(
        !pre_pub_tpls
            .as_array()
            .expect("arr")
            .iter()
            .any(|t| t["slug"] == json!("cairo-payroll-official")),
        "Approval alone must NEVER auto-publish into the Official Library"
    );

    // Owner checks from Desktop Owner session (`owner_desktop_token`) — sees EXACT SAME live state!
    let (desk_list_st, desk_list_body) = send_json(
        &router,
        "GET",
        "/v1/owner/community/submissions",
        Some(&owner_desktop_token),
        None,
    )
    .await;
    assert_eq!(desk_list_st, StatusCode::OK);
    assert!(
        desk_list_body
            .as_array()
            .expect("subs")
            .iter()
            .any(|s| s["id"] == json!(submission_id) && s["status"] == json!("approved")),
        "Desktop Owner view must reflect Web Owner review immediately from central store"
    );

    // Owner explicitly publishes to Official Template Library from Desktop Owner session
    let (pub_st, pub_body) = send_json(
        &router,
        "POST",
        &format!("/v1/owner/community/submissions/{submission_id}/publish"),
        Some(&owner_desktop_token),
        Some(json!({
            "slug": "cairo-payroll-official",
            "official_name": "Cairo Payroll Normalizer",
            "official_description": "Normalizes Arabic employee names and payroll columns",
            "category": "payroll",
            "release_notes": "Initial official publication"
        })),
    )
    .await;
    assert_eq!(pub_st, StatusCode::CREATED, "publish body: {pub_body}");
    let published_template_id = pub_body["template"]["id"]
        .as_str()
        .expect("tpl id")
        .to_owned();

    // Now regular users receive the newly published Official Template!
    let (_, post_pub_tpls) = send_json(
        &router,
        "GET",
        "/v1/templates/official",
        Some(&free_token),
        None,
    )
    .await;
    assert!(post_pub_tpls
        .as_array()
        .expect("arr")
        .iter()
        .any(|t| t["slug"] == json!("cairo-payroll-official")));

    // Owner can deactivate and reactivate an Official Template
    let (deact_st, deact_body) = send_json(
        &router,
        "POST",
        &format!("/v1/owner/templates/official/{published_template_id}/status"),
        Some(&owner_web_token),
        Some(json!({ "active": false })),
    )
    .await;
    assert_eq!(deact_st, StatusCode::OK);
    assert_eq!(deact_body["active"], json!(false));

    let (_, hidden_tpls) = send_json(
        &router,
        "GET",
        "/v1/templates/official",
        Some(&free_token),
        None,
    )
    .await;
    assert!(!hidden_tpls
        .as_array()
        .expect("arr")
        .iter()
        .any(|t| t["slug"] == json!("cairo-payroll-official")));

    let (react_st, _) = send_json(
        &router,
        "POST",
        &format!("/v1/owner/templates/official/{published_template_id}/status"),
        Some(&owner_web_token),
        Some(json!({ "active": true })),
    )
    .await;
    assert_eq!(react_st, StatusCode::OK);

    // 8. Owner-Managed Signed Desktop Release -> Verified via `GET /v1/updates/latest`
    let real_sha = "f78a0776cb5cb7f9829aa95e108c8e5c8675509ac5445603d1136e4c566f9024";
    let (rel_st, rel_body) = send_json(
        &router,
        "POST",
        "/v1/owner/releases",
        Some(&owner_web_token),
        Some(json!({
            "version_code": 10002,
            "version_name": "1.0.2",
            "channel": "stable",
            "platform": "windows-x86_64",
            "download_url": "https://downloads.safidata.app/SafiData-Desktop-1.0.2-x64.msi",
            "file_size_bytes": 6240768,
            "sha256_hex": real_sha,
            "release_notes": "Dual-database security & local search release"
        })),
    )
    .await;
    assert_eq!(rel_st, StatusCode::CREATED, "release body: {rel_body}");

    let (upd_st, upd_body) = send_json(
        &router,
        "GET",
        "/v1/updates/latest?channel=stable&platform=windows-x86_64",
        None,
        None,
    )
    .await;
    assert_eq!(upd_st, StatusCode::OK, "updates body: {upd_body}");
    assert_eq!(upd_body["version_name"], json!("1.0.2"));
    assert_eq!(upd_body["sha256_hex"], json!(real_sha));
    assert_eq!(upd_body["file_size_bytes"], json!(6240768));
    assert!(
        upd_body["manifest"]["signature_hex"]
            .as_str()
            .is_some_and(|s| s.len() == 128),
        "expected 64-byte Ed25519 signature hex: {upd_body}"
    );

    // Verify BLAKE3 Audit Chain is intact and recorded community & release actions
    let (aud_st, aud_body) = send_json(
        &router,
        "GET",
        "/v1/owner/audit-logs",
        Some(&owner_web_token),
        None,
    )
    .await;
    assert_eq!(aud_st, StatusCode::OK);
    assert_eq!(aud_body["is_chain_valid"], json!(true));
    let actions: Vec<&str> = aud_body["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .filter_map(|e| e["action"].as_str())
        .collect();
    assert!(actions.contains(&"owner.community_submission.review"));
    assert!(actions.contains(&"owner.official_template.publish"));
    assert!(actions.contains(&"owner.release.publish"));

    // 9. Full Server Restart Durability Check from DurableStateJournal
    let restarted_sessions = Arc::new(InMemorySessionRepository::new());
    let restarted_cache = Arc::new(InMemoryAuthCache::new());
    let restarted_auth =
        AuthService::new(api_key_repo, restarted_sessions.clone(), restarted_cache);
    let restarted_state = AppState::with_config(restarted_auth, test_config())
        .with_durable_journal(&journal_path, Some(restarted_sessions))
        .await
        .expect("restarted journal");
    let restarted_account_auth = AccountAuthService::new(
        account_repo,
        mailer,
        restarted_state.auth.clone(),
        "https://safidata.app".to_owned(),
    );
    let restarted_state = restarted_state.with_account_auth(restarted_account_auth);
    let restarted_router = app(restarted_state);

    let (after_upd_st, after_upd_body) = send_json(
        &restarted_router,
        "GET",
        "/v1/updates/latest?channel=stable&platform=windows-x86_64",
        None,
        None,
    )
    .await;
    assert_eq!(after_upd_st, StatusCode::OK);
    assert_eq!(after_upd_body["version_name"], json!("1.0.2"));
    assert_eq!(after_upd_body["sha256_hex"], json!(real_sha));

    let (after_tpl_st, after_tpl_body) = send_json(
        &restarted_router,
        "GET",
        "/v1/templates/official",
        Some(&free_token),
        None,
    )
    .await;
    assert_eq!(after_tpl_st, StatusCode::OK);
    assert!(
        after_tpl_body
            .as_array()
            .expect("templates")
            .iter()
            .any(|t| t["slug"] == json!("cairo-payroll-official")),
        "Published official template must survive full server restart"
    );

    let _ = std::fs::remove_file(&journal_path);
}
