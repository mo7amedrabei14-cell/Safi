//! Ephemeral local HTTP `sf-server` harness for real unmocked Chromium + Rust Desktop E2E tests.
//!
//! Supports two modes:
//! 1. Default in-memory mode (used by `verify-desktop-full-journey-real-e2e.mjs` when `AUTH_HARNESS_PG_URL` is unset).
//! 2. Durable PostgreSQL 18 mode (`AUTH_HARNESS_PG_URL` set — used by `verify-org-sync-e2e.mjs` across
//!    real process kills & cold restarts, backed by `SqlxAccountRepository`, `SqlxSessionRepository`,
//!    and `PgControlPlaneStore` with `with_require_durable_pg(true)`).
//!
//! Pre-verified accounts:
//!   - `analyst@safidata.com` / `SafiAnalyst#2026` (Free tier)
//!   - `pro@safidata.com` / `SafiPro#2026` (Paid/VIP tier — entitled to `.safitemplate` export/import)
//!   - `owner@safidata.com` / `SafiOwner#2026` (Bootstrapped Platform Owner with confirmed RFC 6238 TOTP)
//!   - `alice@safidata.com` / `SafiAlice#2026` (Organization E2E Account A)
//!   - `bob@safidata.com` / `SafiBob#2026` (Organization E2E Account B)
//!   - `viewer@safidata.com` / `SafiViewer#2026` (Organization E2E Viewer Account)
//!   - `mallory@safidata.com` / `SafiMallory#2026` (Organization E2E Outsider Account)

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use axum::http::StatusCode;
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Value};
use sf_server::auth::account::{
    AccountAuthService, AccountRepository, LoginAccount, RegisterAccountRequest,
    SqlxAccountRepository, VerificationEmailSender,
};
use sf_server::auth::cache::InMemoryAuthCache;
use sf_server::auth::repository::{InMemoryApiKeyRepository, SqlxSessionRepository};
use sf_server::auth::service::{in_memory_service, AuthService};
use sf_server::auth::unix_now;
use sf_server::config::ServerConfig;
use sf_server::error::AuthError;
use sf_server::owner_mfa::{compute_totp_code, decode_base32, TOTP_PERIOD_SECS};
use sf_server::pg_store::PgControlPlaneStore;
use sf_server::{app, AppState, ManagedAccountRecord};
use sqlx::postgres::PgPoolOptions;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Default)]
struct HarnessAccountState {
    accounts: HashMap<String, LoginAccount>,
    pending_tokens: HashMap<[u8; 32], (String, i64)>,
    reset_tokens: HashMap<[u8; 32], (String, i64)>,
}

#[derive(Default)]
struct HarnessAccountRepo {
    state: RwLock<HarnessAccountState>,
}

#[async_trait]
impl AccountRepository for HarnessAccountRepo {
    async fn register(
        &self,
        email: &str,
        _display_name: &str,
        password_hash: &str,
        token_hash: &[u8; 32],
        token_expires_at: i64,
    ) -> Result<bool, AuthError> {
        let mut state = self.state.write().await;
        if state.accounts.contains_key(email) {
            return Ok(false);
        }
        state.accounts.insert(
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
        state
            .pending_tokens
            .insert(*token_hash, (email.to_owned(), token_expires_at));
        Ok(true)
    }

    async fn find_for_login(&self, email: &str) -> Result<Option<LoginAccount>, AuthError> {
        Ok(self.state.read().await.accounts.get(email).cloned())
    }

    async fn verify_email(&self, token_hash: &[u8; 32], now: i64) -> Result<bool, AuthError> {
        let mut state = self.state.write().await;
        let Some((email, expires_at)) = state.pending_tokens.remove(token_hash) else {
            return Ok(false);
        };
        if expires_at <= now {
            return Ok(false);
        }
        let Some(account) = state.accounts.get_mut(&email) else {
            return Ok(false);
        };
        if account.deleted {
            return Ok(false);
        }
        account.email_verified = true;
        Ok(true)
    }

    async fn replace_verification_token(
        &self,
        email: &str,
        token_hash: &[u8; 32],
        token_expires_at: i64,
        _now: i64,
    ) -> Result<bool, AuthError> {
        let mut state = self.state.write().await;
        let Some(account) = state.accounts.get(email) else {
            return Ok(false);
        };
        if account.email_verified || account.deleted {
            return Ok(false);
        }
        state.pending_tokens.retain(|_, (owner, _)| owner != email);
        state
            .pending_tokens
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
        let mut state = self.state.write().await;
        if !state.accounts.contains_key(email) {
            return Ok(false);
        }
        state.reset_tokens.retain(|_, (owner, _)| owner != email);
        state
            .reset_tokens
            .insert(*token_hash, (email.to_owned(), token_expires_at));
        Ok(true)
    }

    async fn reset_password(
        &self,
        token_hash: &[u8; 32],
        password_hash: &str,
        now: i64,
    ) -> Result<bool, AuthError> {
        let mut state = self.state.write().await;
        let Some((email, expires_at)) = state.reset_tokens.remove(token_hash) else {
            return Ok(false);
        };
        if expires_at <= now {
            return Ok(false);
        }
        let Some(account) = state.accounts.get_mut(&email) else {
            return Ok(false);
        };
        account.password_hash = password_hash.to_owned();
        Ok(true)
    }
}

#[derive(Default)]
struct HarnessMailer {
    urls: RwLock<Vec<String>>,
}

#[async_trait]
impl VerificationEmailSender for HarnessMailer {
    async fn send_verification(
        &self,
        _recipient: &str,
        verification_url: &str,
    ) -> Result<(), AuthError> {
        self.urls.write().await.push(verification_url.to_owned());
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

fn spawn_local_paddle_mock() -> String {
    let calls = Arc::new(AtomicUsize::new(0));
    let mock_router = Router::new().route(
        "/transactions",
        post(move |Json(payload): Json<Value>| {
            let calls = calls.clone();
            async move {
                let n = calls.fetch_add(1, Ordering::SeqCst) + 1;
                let txn_id = format!("txn_harness_{n:04}");
                let price_id = payload["items"][0]["price_id"]
                    .as_str()
                    .unwrap_or("pri_org_usd")
                    .to_owned();
                let qty = payload["items"][0]["quantity"].as_u64().unwrap_or(1);
                let unit_minor = match price_id.as_str() {
                    "pri_monthly_usd" => 2_000,
                    "pri_annual_usd" => 16_800,
                    "pri_org_usd" => 20_400,
                    "pri_ppf_usd" => 200,
                    "pri_cp25_usd" => 500,
                    _ => 2_000,
                };
                let total = (qty * unit_minor).to_string();
                let rec = json!({
                    "id": txn_id,
                    "status": "draft",
                    "currency_code": "USD",
                    "items": [{ "price": { "id": price_id }, "quantity": qty }],
                    "details": { "totals": { "total": total } },
                    "custom_data": payload["custom_data"],
                    "checkout": { "url": format!("https://checkout.paddle.com/{txn_id}") }
                });
                (StatusCode::CREATED, Json(json!({ "data": rec })))
            }
        }),
    );

    let std_listener = match std::net::TcpListener::bind("127.0.0.1:0") {
        Ok(l) => l,
        Err(_) => std::process::exit(10),
    };
    if std_listener.set_nonblocking(true).is_err() {
        std::process::exit(10);
    }
    let Ok(addr) = std_listener.local_addr() else {
        std::process::exit(10);
    };
    let Ok(listener) = tokio::net::TcpListener::from_std(std_listener) else {
        std::process::exit(10);
    };
    tokio::spawn(async move {
        let _ = axum::serve(listener, mock_router).await;
    });
    format!("http://{addr}")
}

async fn ensure_verified_account<R: AccountRepository + ?Sized>(
    account_auth: &AccountAuthService,
    mailer: &HarnessMailer,
    repo: &R,
    base_state: &AppState,
    email: &str,
    display_name: &str,
    password: &str,
) -> (Uuid, Uuid) {
    let norm_email = email.trim().to_ascii_lowercase();
    if let Ok(Some(existing)) = repo.find_for_login(&norm_email).await {
        let ws = existing.workspace_id.unwrap_or_else(Uuid::now_v7);
        base_state
            .upsert_managed_account(ManagedAccountRecord {
                user_id: existing.user_id,
                workspace_id: ws,
                email: norm_email,
                display_name: display_name.to_owned(),
                suspended: false,
                verified_phone: None,
                created_at: unix_now(),
            })
            .await;
        return (existing.user_id, ws);
    }

    if account_auth
        .register(RegisterAccountRequest {
            email: email.to_owned(),
            display_name: display_name.to_owned(),
            password: password.to_owned(),
        })
        .await
        .is_err()
    {
        std::process::exit(2);
    }
    let verify_url = mailer.urls.read().await.last().cloned().unwrap_or_default();
    let verify_token = verify_url.split("verify_email=").nth(1).unwrap_or("");
    if account_auth.verify_email(verify_token).await.is_err() {
        std::process::exit(3);
    }
    let Some(acc) = repo.find_for_login(&norm_email).await.ok().flatten() else {
        std::process::exit(3);
    };
    let ws = acc.workspace_id.unwrap_or_else(Uuid::now_v7);
    base_state
        .upsert_managed_account(ManagedAccountRecord {
            user_id: acc.user_id,
            workspace_id: ws,
            email: norm_email,
            display_name: display_name.to_owned(),
            suspended: false,
            verified_phone: None,
            created_at: unix_now(),
        })
        .await;
    (acc.user_id, ws)
}

#[tokio::main]
async fn main() {
    let bind_addr = std::env::var("AUTH_HARNESS_ADDR").unwrap_or_else(|_| "127.0.0.1:0".to_owned());
    let public_origin = std::env::var("AUTH_HARNESS_PUBLIC_ORIGIN")
        .unwrap_or_else(|_| "http://127.0.0.1:4175".to_owned());
    let web_origin =
        std::env::var("AUTH_HARNESS_WEB_ORIGIN").unwrap_or_else(|_| public_origin.clone());
    let paddle_secret = std::env::var("AUTH_HARNESS_PADDLE_SECRET")
        .unwrap_or_else(|_| "pdl_harness_whsec_secret".to_owned());
    let pg_url = std::env::var("AUTH_HARNESS_PG_URL")
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());

    let paddle_mock_url = spawn_local_paddle_mock();
    let mailer = Arc::new(HarnessMailer::default());

    let mut cfg = ServerConfig::for_tests();
    cfg.api_base_url = Some(public_origin.clone());
    cfg.public_web_origin = Some(web_origin.clone());
    cfg.license_signing_seed = Some([0x55u8; 32]);
    cfg.paddle_api_base_url = paddle_mock_url;
    cfg.paddle_api_key = Some("pdl_harness_api_key".to_owned());
    cfg.paddle_webhook_secret = Some(paddle_secret);
    cfg.paddle_price_individual_monthly = Some("pri_monthly_usd".to_owned());
    cfg.paddle_price_individual_annual = Some("pri_annual_usd".to_owned());
    cfg.paddle_price_org_seat_annual = Some("pri_org_usd".to_owned());
    cfg.paddle_price_pay_per_file = Some("pri_ppf_usd".to_owned());
    cfg.paddle_price_credit_pack_25 = Some("pri_cp25_usd".to_owned());

    let (base_state, account_auth, pro_uid, pro_ws, owner_uid, pg_backed) =
        if let Some(ref db_url) = pg_url {
            let Ok(pool) = PgPoolOptions::new()
                .max_connections(8)
                .connect(db_url)
                .await
            else {
                std::process::exit(11);
            };
            let auth_service = AuthService::new(
                Arc::new(InMemoryApiKeyRepository::default()),
                Arc::new(SqlxSessionRepository::new(pool.clone())),
                Arc::new(InMemoryAuthCache::new()),
            );
            let pg_store = PgControlPlaneStore::new(pool.clone());
            let mut st = AppState::with_config(auth_service, cfg).with_pg_store(pg_store);
            st.entitlements = st.entitlements.with_require_durable_pg(true);
            st.billing = st.billing.with_require_durable_pg(true);
            st.billing_handoff = st.billing_handoff.with_require_durable_pg(true);
            st.organizations = st.organizations.with_require_durable_pg(true);
            st.encrypted_sync = st.encrypted_sync.with_require_durable_pg(true);

            let sqlx_repo = Arc::new(SqlxAccountRepository::new(pool));
            let acc_auth = AccountAuthService::new(
                sqlx_repo.clone(),
                mailer.clone(),
                st.auth.clone(),
                web_origin,
            );

            let _ = ensure_verified_account(
                &acc_auth,
                &mailer,
                sqlx_repo.as_ref(),
                &st,
                "analyst@safidata.com",
                "Safi Analyst",
                "SafiAnalyst#2026",
            )
            .await;
            let (p_uid, p_ws) = ensure_verified_account(
                &acc_auth,
                &mailer,
                sqlx_repo.as_ref(),
                &st,
                "pro@safidata.com",
                "Safi Pro User",
                "SafiPro#2026",
            )
            .await;
            let (o_uid, _) = ensure_verified_account(
                &acc_auth,
                &mailer,
                sqlx_repo.as_ref(),
                &st,
                "owner@safidata.com",
                "Platform Owner",
                "SafiOwner#2026",
            )
            .await;
            for (em, name, pw) in [
                ("alice@safidata.com", "Alice Owner", "SafiAlice#2026"),
                ("bob@safidata.com", "Bob Member", "SafiBob#2026"),
                ("viewer@safidata.com", "Victor Viewer", "SafiViewer#2026"),
                (
                    "mallory@safidata.com",
                    "Mallory Outsider",
                    "SafiMallory#2026",
                ),
            ] {
                let _ = ensure_verified_account(
                    &acc_auth,
                    &mailer,
                    sqlx_repo.as_ref(),
                    &st,
                    em,
                    name,
                    pw,
                )
                .await;
            }
            (st, acc_auth, p_uid, p_ws, o_uid, true)
        } else {
            let repo = Arc::new(HarnessAccountRepo::default());
            let (auth_service, _, _, _) = in_memory_service();
            let st = AppState::with_config(auth_service, cfg);
            let acc_auth =
                AccountAuthService::new(repo.clone(), mailer.clone(), st.auth.clone(), web_origin);
            let _ = ensure_verified_account(
                &acc_auth,
                &mailer,
                repo.as_ref(),
                &st,
                "analyst@safidata.com",
                "Safi Analyst",
                "SafiAnalyst#2026",
            )
            .await;
            let (p_uid, p_ws) = ensure_verified_account(
                &acc_auth,
                &mailer,
                repo.as_ref(),
                &st,
                "pro@safidata.com",
                "Safi Pro User",
                "SafiPro#2026",
            )
            .await;
            let (o_uid, _) = ensure_verified_account(
                &acc_auth,
                &mailer,
                repo.as_ref(),
                &st,
                "owner@safidata.com",
                "Platform Owner",
                "SafiOwner#2026",
            )
            .await;
            for (em, name, pw) in [
                ("alice@safidata.com", "Alice Owner", "SafiAlice#2026"),
                ("bob@safidata.com", "Bob Member", "SafiBob#2026"),
                ("viewer@safidata.com", "Victor Viewer", "SafiViewer#2026"),
                (
                    "mallory@safidata.com",
                    "Mallory Outsider",
                    "SafiMallory#2026",
                ),
            ] {
                let _ =
                    ensure_verified_account(&acc_auth, &mailer, repo.as_ref(), &st, em, name, pw)
                        .await;
            }
            (st, acc_auth, p_uid, p_ws, o_uid, false)
        };

    // Bootstrap Owner + pre-confirm RFC 6238 TOTP so E2E verifies real TOTP codes live.
    if base_state
        .owner_mfa
        .bootstrap_owner_once(owner_uid, true)
        .await
        .is_err()
    {
        std::process::exit(7);
    }
    let Ok(bundle) = base_state
        .owner_mfa
        .enroll_totp(owner_uid, "owner@safidata.com")
        .await
    else {
        std::process::exit(8);
    };
    let Ok(secret_bytes) = decode_base32(&bundle.manual_secret_base32) else {
        std::process::exit(8);
    };
    let now = unix_now();
    let prev_step = (now / TOTP_PERIOD_SECS).saturating_sub(1);
    let Ok(confirm_code) = compute_totp_code(&secret_bytes, prev_step) else {
        std::process::exit(8);
    };
    if base_state
        .owner_mfa
        .confirm_totp_at(owner_uid, Uuid::now_v7(), &confirm_code, now)
        .await
        .is_err()
    {
        std::process::exit(9);
    }

    // Grant Paid/VIP entitlement to `pro@safidata.com` so `.safitemplate` E2E tests both Free (403) and Paid (200).
    base_state
        .entitlements
        .set_friend_vip(pro_ws, pro_uid, owner_uid, true)
        .await;

    let state = base_state.with_account_auth(account_auth);
    let router = app(state);
    let Ok(listener) = tokio::net::TcpListener::bind(&bind_addr).await else {
        std::process::exit(4);
    };
    let Ok(addr) = listener.local_addr() else {
        std::process::exit(5);
    };
    println!(
        "{}",
        serde_json::json!({
            "ready": true,
            "addr": addr.to_string(),
            "ownerTotpBase32": bundle.manual_secret_base32,
            "pgBacked": pg_backed
        })
    );
    let _ = std::io::Write::flush(&mut std::io::stdout());
    if axum::serve(listener, router).await.is_err() {
        std::process::exit(6);
    }
}
