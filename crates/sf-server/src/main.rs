//! SafiData HTTP server host.
//!
//! Required runtime configuration is read from the process environment. The
//! service expects the P0.01/P0.02 migrations to have been applied beforehand.

use std::error::Error;
use std::io;

use axum::serve;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tokio::net::TcpListener;

use sf_server::auth::account::{
    AccountAuthService, SmtpVerificationEmailSender, SqlxAccountRepository,
};
use sf_server::auth::cache::{AuthCache, InMemoryAuthCache, ValkeyAuthCache};
use sf_server::auth::repository::{SqlxApiKeyRepository, SqlxSessionRepository};
use sf_server::auth::service::AuthService;
use sf_server::{app, AppState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let database_url = std::env::var("DATABASE_URL")
        .map_err(|_| io::Error::new(io::ErrorKind::NotFound, "DATABASE_URL is required"))?;
    let bind_address =
        std::env::var("SAFI_BIND_ADDRESS").unwrap_or_else(|_| "127.0.0.1:8080".to_owned());

    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(&database_url)
        .await?;

    let cache: Arc<dyn AuthCache> =
        match std::env::var("SAFI_VALKEY_URL") {
            Ok(url) => Arc::new(ValkeyAuthCache::connect(&url).await.map_err(|_| {
                io::Error::other("Unable to connect to the configured Valkey service")
            })?),
            Err(_) => Arc::new(InMemoryAuthCache::new()),
        };

    let auth = AuthService::new(
        Arc::new(SqlxApiKeyRepository::new(pool.clone())),
        Arc::new(SqlxSessionRepository::new(pool.clone())),
        cache,
    );
    let mut state = AppState::new(auth);

    let (email_sender, public_base_url) = SmtpVerificationEmailSender::from_env()
        .map_err(|_| io::Error::other("Invalid or missing SMTP account configuration"))?;
    let account_auth = AccountAuthService::new(
        Arc::new(SqlxAccountRepository::new(pool)),
        Arc::new(email_sender),
        state.auth.clone(),
        public_base_url,
    );
    state = state.with_account_auth(account_auth);

    let listener = TcpListener::bind(&bind_address).await?;
    println!("SafiData server listening on {bind_address}");
    serve(listener, app(state)).await?;

    Ok(())
}
