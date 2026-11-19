//! Ephemeral local HTTP `sf-server` harness for real unmocked Chromium + Rust Desktop E2E tests.
//!
//! Spawns the genuine `sf-server` Axum router (`sf_server::app`) with:
//! - Argon2id `AccountAuthService` + `AuthService` + `OwnerMfaService` + `TemplatesHubService`
//!   + `DesktopPkceService` + `DeviceAuthorityService` + `EntitlementService`
//! - Pre-verified accounts:
//!   - `analyst@safidata.com` / `SafiAnalyst#2026` (Free tier)
//!   - `pro@safidata.com` / `SafiPro#2026` (Paid/VIP tier — entitled to `.safitemplate` export/import)
//!   - `owner@safidata.com` / `SafiOwner#2026` (Bootstrapped Platform Owner with confirmed RFC 6238 TOTP)
//! - Prints a single JSON readiness line containing `addr` and `ownerTotpBase32` to stdout.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use sf_server::auth::account::{
    AccountAuthService, AccountRepository, LoginAccount, RegisterAccountRequest,
    VerificationEmailSender,
};
use sf_server::auth::service::in_memory_service;
use sf_server::auth::unix_now;
use sf_server::config::ServerConfig;
use sf_server::error::AuthError;
use sf_server::owner_mfa::{compute_totp_code, decode_base32, TOTP_PERIOD_SECS};
use sf_server::{app, AppState};
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

async fn seed_verified_account(
    account_auth: &AccountAuthService,
    mailer: &HarnessMailer,
    repo: &HarnessAccountRepo,
    email: &str,
    display_name: &str,
    password: &str,
) -> (Uuid, Uuid) {
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
    let Some(acc) = repo.find_for_login(email).await.ok().flatten() else {
        std::process::exit(3);
    };
    (acc.user_id, acc.workspace_id.unwrap_or_else(Uuid::now_v7))
}

#[tokio::main]
async fn main() {
    let bind_addr = std::env::var("AUTH_HARNESS_ADDR").unwrap_or_else(|_| "127.0.0.1:0".to_owned());
    let public_origin = std::env::var("AUTH_HARNESS_PUBLIC_ORIGIN")
        .unwrap_or_else(|_| "http://127.0.0.1:4175".to_owned());

    let repo = Arc::new(HarnessAccountRepo::default());
    let mailer = Arc::new(HarnessMailer::default());
    let (auth_service, _, _, _) = in_memory_service();

    let mut cfg = ServerConfig::for_tests();
    cfg.api_base_url = Some(public_origin.clone());
    cfg.license_signing_seed = Some([0x55u8; 32]);

    let base_state = AppState::with_config(auth_service, cfg);
    let account_auth = AccountAuthService::new(
        repo.clone(),
        mailer.clone(),
        base_state.auth.clone(),
        public_origin,
    );

    let _free_ids = seed_verified_account(
        &account_auth,
        &mailer,
        &repo,
        "analyst@safidata.com",
        "Safi Analyst",
        "SafiAnalyst#2026",
    )
    .await;
    let (pro_uid, pro_ws) = seed_verified_account(
        &account_auth,
        &mailer,
        &repo,
        "pro@safidata.com",
        "Safi Pro User",
        "SafiPro#2026",
    )
    .await;
    let (owner_uid, _owner_ws) = seed_verified_account(
        &account_auth,
        &mailer,
        &repo,
        "owner@safidata.com",
        "Platform Owner",
        "SafiOwner#2026",
    )
    .await;

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
    // Confirm with previous 30s window so the current window remains unused for E2E live verify!
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
            "ownerTotpBase32": bundle.manual_secret_base32
        })
    );
    let _ = std::io::Write::flush(&mut std::io::stdout());
    if axum::serve(listener, router).await.is_err() {
        std::process::exit(6);
    }
}
