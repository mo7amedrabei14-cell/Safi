//! SafiData HTTP server host with fail-closed production config validation,
//! `--check-config` verification CLI, and one-time sealed `--bootstrap-owner` CLI.

use std::error::Error;
use std::io;
use std::sync::Arc;

use axum::serve;
use sqlx::postgres::PgPoolOptions;
use sqlx::Row;
use tokio::net::TcpListener;
use uuid::Uuid;

use sf_server::auth::account::{
    AccountAuthService, SmtpVerificationEmailSender, SqlxAccountRepository,
};
use sf_server::auth::cache::{AuthCache, InMemoryAuthCache, ValkeyAuthCache};
use sf_server::auth::repository::{SqlxApiKeyRepository, SqlxSessionRepository};
use sf_server::auth::service::AuthService;
use sf_server::config::ServerConfig;
use sf_server::pg_store::PgControlPlaneStore;
use sf_server::{app, AppState};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    let config =
        ServerConfig::from_env().map_err(|msg| io::Error::new(io::ErrorKind::InvalidData, msg))?;

    if args.iter().any(|a| a == "--check-config") {
        let report = config.readiness_report();
        println!("{}", serde_json::to_string_pretty(&report)?);
        if !report.ready {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "SF-SYS-032: missing required config for {:?}: {}",
                    report.environment,
                    report.missing_required.join(", ")
                ),
            )
            .into());
        }
        return Ok(());
    }

    if args.iter().any(|a| a == "--bootstrap-owner") {
        return run_sealed_owner_bootstrap(&config).await;
    }

    // Fail closed in staging/production when mandatory security/persistence vars are missing.
    config
        .validate_for_startup()
        .map_err(|msg| io::Error::new(io::ErrorKind::InvalidData, msg))?;

    let database_url = config
        .database_url
        .as_deref()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "DATABASE_URL is required"))?;

    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(database_url)
        .await?;

    // Strict fail-closed Valkey requirement in production/staging; no silent in-memory fallback.
    let cache: Arc<dyn AuthCache> = match config.valkey_url.as_deref() {
        Some(url) => Arc::new(ValkeyAuthCache::connect(url).await.map_err(|_| {
            io::Error::other("SF-AUTH-005: Unable to connect to the configured Valkey service")
        })?),
        None if config.environment.is_strict() => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "SF-SYS-032: SAFI_VALKEY_URL is required in staging/production",
            )
            .into());
        }
        None => Arc::new(InMemoryAuthCache::new()),
    };

    let auth = AuthService::new(
        Arc::new(SqlxApiKeyRepository::new(pool.clone())),
        Arc::new(SqlxSessionRepository::new(pool.clone())),
        cache,
    );
    let mut state = AppState::with_config(auth, config.clone())
        .with_pg_store(PgControlPlaneStore::new(pool.clone()));

    // Load sealed platform_owner (if already bootstrapped) into OwnerMfaService.
    if let Ok(Some(row)) = sqlx::query("SELECT owner_user_id FROM platform_owner LIMIT 1")
        .fetch_optional(&pool)
        .await
    {
        if let Ok(owner_user_id) = row.try_get::<Uuid, _>("owner_user_id") {
            let _ = state
                .owner_mfa
                .bootstrap_owner_once(owner_user_id, true)
                .await;
        }
    }

    let (email_sender, public_base_url) = SmtpVerificationEmailSender::from_env()
        .map_err(|_| io::Error::other("Invalid or missing SMTP account configuration"))?;
    let account_auth = AccountAuthService::new(
        Arc::new(SqlxAccountRepository::new(pool)),
        Arc::new(email_sender),
        state.auth.clone(),
        public_base_url,
    );
    state = state.with_account_auth(account_auth);

    let listener = TcpListener::bind(&config.bind_addr).await?;
    println!("SafiData server listening on {}", config.bind_addr);
    serve(listener, app(state)).await?;

    Ok(())
}

/// One-time sealed Owner bootstrap reading `SAFI_OWNER_BOOTSTRAP_EMAIL` from the server environment.
/// Resolves the verified user's immutable `users.id`, inserts the singleton `platform_owner` row,
/// and fails closed on any second invocation.
async fn run_sealed_owner_bootstrap(config: &ServerConfig) -> Result<(), Box<dyn Error>> {
    let database_url = config
        .database_url
        .as_deref()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "DATABASE_URL is required"))?;
    let raw_email = std::env::var("SAFI_OWNER_BOOTSTRAP_EMAIL").map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "SF-AUTH-017: SAFI_OWNER_BOOTSTRAP_EMAIL must be set for one-time owner bootstrap",
        )
    })?;
    let normalized = raw_email.trim().to_ascii_lowercase();
    if normalized.is_empty() || !normalized.contains('@') {
        return Err(Box::new(io::Error::new(
            io::ErrorKind::InvalidInput,
            "SF-AUTH-017: invalid bootstrap email",
        )));
    }

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;

    let existing: Option<Uuid> =
        sqlx::query_scalar("SELECT owner_user_id FROM platform_owner LIMIT 1")
            .fetch_optional(&pool)
            .await?;
    if existing.is_some() {
        return Err(Box::new(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "SF-AUTH-017: platform_owner bootstrap is already sealed",
        )));
    }

    let row = sqlx::query(
        "SELECT id, email_verified_at IS NOT NULL AS verified FROM users WHERE lower(email) = $1 AND deleted_at IS NULL AND suspended_at IS NULL",
    )
    .bind(&normalized)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "SF-AUTH-017: bootstrap account must exist and be active before sealing",
        )
    })?;

    let verified: bool = row.try_get("verified")?;
    if !verified {
        return Err(Box::new(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "SF-AUTH-017: bootstrap account email must be verified before sealing",
        )));
    }
    let owner_user_id: Uuid = row.try_get("id")?;

    sqlx::query(
        "INSERT INTO platform_owner (singleton, owner_user_id, bootstrap_sealed) VALUES (TRUE, $1, TRUE)",
    )
    .bind(owner_user_id)
    .execute(&pool)
    .await?;

    println!("Platform owner sealed permanently for immutable user_id={owner_user_id}. Remove SAFI_OWNER_BOOTSTRAP_EMAIL from the environment now.");
    Ok(())
}
