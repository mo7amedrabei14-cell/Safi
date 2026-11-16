//! Ephemeral local HTTP `sf-server` auth server for Chromium UI + Harness integration testing.
//!
//! Spawns a genuine `sf-server` Axum router with `AccountAuthService` + `AuthService` +
//! `InMemorySessionRepository` on `127.0.0.1:0` (or `AUTH_HARNESS_ADDR`), registers and
//! verifies the harness test account via the real Argon2id registration pipeline, and
//! prints `{"ready":true,"addr":"127.0.0.1:<port>"}` to stdout.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use sf_server::auth::account::{
    AccountAuthService, AccountRepository, LoginAccount, RegisterAccountRequest,
    VerificationEmailSender,
};
use sf_server::auth::service::in_memory_service;
use sf_server::error::AuthError;
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

#[tokio::main]
async fn main() {
    let bind_addr = std::env::var("AUTH_HARNESS_ADDR").unwrap_or_else(|_| "127.0.0.1:0".to_owned());
    let repo = Arc::new(HarnessAccountRepo::default());
    let mailer = Arc::new(HarnessMailer::default());
    let (auth_service, _, _, _) = in_memory_service();
    let auth_arc = Arc::new(auth_service);
    let account_auth = AccountAuthService::new(
        repo,
        mailer.clone(),
        auth_arc.clone(),
        "http://127.0.0.1:4175",
    );

    if account_auth
        .register(RegisterAccountRequest {
            email: "analyst@safidata.com".to_owned(),
            display_name: "Safi Analyst".to_owned(),
            password: "SafiAnalyst#2026".to_owned(),
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

    let state = AppState {
        auth: auth_arc,
        account_auth: Some(Arc::new(account_auth)),
        zero_trust: Default::default(),
        log_center: Arc::new(RwLock::new(sf_server::log_center::AuditLogCenter::new())),
    };
    let router = app(state);
    let Ok(listener) = tokio::net::TcpListener::bind(&bind_addr).await else {
        std::process::exit(4);
    };
    let Ok(addr) = listener.local_addr() else {
        std::process::exit(5);
    };
    println!(r#"{{"ready":true,"addr":"{addr}"}}"#);
    if axum::serve(listener, router).await.is_err() {
        std::process::exit(6);
    }
}
