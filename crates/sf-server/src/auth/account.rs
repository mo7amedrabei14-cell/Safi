//! Email/password account lifecycle (P0.02).
//!
//! The account store is PostgreSQL-backed in production. Passwords are hashed
//! with Argon2id, email tokens are random one-time capabilities whose BLAKE3
//! hashes alone are persisted, and delivery happens only after the database
//! transaction commits.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier};
use argon2::{Algorithm, Argon2, Params, Version};
use async_trait::async_trait;
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Mailbox, Message, Tokio1Executor};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::Deserialize;
use sqlx::postgres::PgPool;
use sqlx::Row;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::auth::service::{AuthService, IssuedSession};
use crate::auth::{AuthMethod, Principal, Scope};
use crate::error::AuthError;
use crate::auth::unix_now;

const DEFAULT_VERIFICATION_TTL_SECS: i64 = 24 * 60 * 60;
const ARGON2_MEMORY_KIB: u32 = 65_536;
const ARGON2_ITERATIONS: u32 = 3;
const ARGON2_LANES: u32 = 1;

#[derive(Clone)]
pub struct LoginAccount {
    pub user_id: Uuid,
    pub workspace_id: Option<Uuid>,
    pub password_hash: String,
    pub email_verified: bool,
    pub deleted: bool,
}

impl std::fmt::Debug for LoginAccount {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LoginAccount")
            .field("user_id", &self.user_id)
            .field("workspace_id", &self.workspace_id)
            .field("password_hash", &"[REDACTED]")
            .field("email_verified", &self.email_verified)
            .field("deleted", &self.deleted)
            .finish()
    }
}

#[async_trait]
pub trait AccountRepository: Send + Sync {
    /// Atomically provisions user, credential, workspace, owner membership,
    /// Free subscription, and pending email token. False means duplicate email.
    async fn register(
        &self,
        email: &str,
        display_name: &str,
        password_hash: &str,
        token_hash: &[u8; 32],
        token_expires_at: i64,
    ) -> Result<bool, AuthError>;

    /// Returns the account's password hash and owning workspace for sign-in.
    async fn find_for_login(&self, email: &str) -> Result<Option<LoginAccount>, AuthError>;

    /// Atomically consumes a valid, unexpired token and verifies its user.
    async fn verify_email(
        &self,
        token_hash: &[u8; 32],
        now: i64,
    ) -> Result<bool, AuthError>;

    /// Replaces the pending token for an existing, unverified account.
    async fn replace_verification_token(
        &self,
        email: &str,
        token_hash: &[u8; 32],
        token_expires_at: i64,
        now: i64,
    ) -> Result<bool, AuthError>;
}

pub type SharedAccountRepository = Arc<dyn AccountRepository>;

#[async_trait]
pub trait VerificationEmailSender: Send + Sync {
    async fn send_verification(&self, recipient: &str, verification_url: &str)
        -> Result<(), AuthError>;
}

pub type SharedVerificationEmailSender = Arc<dyn VerificationEmailSender>;

#[derive(Debug, Deserialize)]
pub struct RegisterAccountRequest {
    pub email: String,
    pub display_name: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginAccountRequest {
    pub email: String,
    pub password: String,
    pub device_name: String,
}

#[derive(Debug, Deserialize)]
pub struct VerifyEmailRequest {
    pub token: String,
}

#[derive(Debug, Deserialize)]
pub struct ResendVerificationRequest {
    pub email: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AccountMessage {
    pub message: &'static str,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub session: crate::auth::session::SessionView,
}

/// Coordinates account provisioning, verification delivery, and password sign-in.
pub struct AccountAuthService {
    repository: SharedAccountRepository,
    email_sender: SharedVerificationEmailSender,
    sessions: Arc<AuthService>,
    public_base_url: String,
    verification_ttl_secs: i64,
}

impl std::fmt::Debug for AccountAuthService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AccountAuthService")
            .field("public_base_url", &self.public_base_url)
            .field("verification_ttl_secs", &self.verification_ttl_secs)
            .finish_non_exhaustive()
    }
}

impl AccountAuthService {
    #[must_use]
    pub fn new(
        repository: SharedAccountRepository,
        email_sender: SharedVerificationEmailSender,
        sessions: Arc<AuthService>,
        public_base_url: impl Into<String>,
    ) -> Self {
        Self {
            repository,
            email_sender,
            sessions,
            public_base_url: public_base_url.into().trim_end_matches('/').to_owned(),
            verification_ttl_secs: DEFAULT_VERIFICATION_TTL_SECS,
        }
    }

    /// Override the token lifetime for controlled tests and operational policy.
    #[must_use]
    pub fn with_verification_ttl(mut self, ttl: Duration) -> Self {
        self.verification_ttl_secs = i64::try_from(ttl.as_secs()).unwrap_or(i64::MAX);
        self
    }

    /// Public registration always returns the same accepted response for new
    /// and existing addresses. Email delivery failure is deliberately non-enumerating.
    pub async fn register(&self, request: RegisterAccountRequest) -> Result<(), AuthError> {
        let email = normalize_email(&request.email)?;
        let display_name = validate_display_name(&request.display_name)?;
        validate_new_password(&request.password)?;

        let password = request.password;
        let password_hash = hash_password(password).await?;
        let (token, token_hash) = generate_verification_token();
        let now = unix_now();
        let expires_at = now.saturating_add(self.verification_ttl_secs);

        let created = self
            .repository
            .register(
                &email,
                &display_name,
                &password_hash,
                &token_hash,
                expires_at,
            )
            .await?;

        if created {
            self.dispatch_verification(&email, &token).await;
        }

        Ok(())
    }

    /// A verification token is a one-time capability and is never reflected in
    /// the response. Invalid, expired, and replayed values share one response.
    pub async fn verify_email(&self, token: &str) -> Result<(), AuthError> {
        let token_hash = hash_presented_verification_token(token)?;
        let accepted = self
            .repository
            .verify_email(&token_hash, unix_now())
            .await?;
        if accepted {
            Ok(())
        } else {
            Err(AuthError::InvalidRequest)
        }
    }

    /// Resend has the same outward result regardless of account existence,
    /// verification state, or SMTP availability.
    pub async fn resend_verification(&self, raw_email: &str) -> Result<(), AuthError> {
        let email = normalize_email(raw_email)?;
        let (token, token_hash) = generate_verification_token();
        let now = unix_now();
        let expires_at = now.saturating_add(self.verification_ttl_secs);
        let replaced = self
            .repository
            .replace_verification_token(&email, &token_hash, expires_at, now)
            .await?;

        if replaced {
            self.dispatch_verification(&email, &token).await;
        }
        Ok(())
    }

    /// Verifies both the password and email status before issuing the existing
    /// sfs_* session. Unknown, incorrect, deleted, and unverified accounts do
    /// not reveal which condition occurred.
    pub async fn login(&self, request: LoginAccountRequest) -> Result<IssuedSession, AuthError> {
        let email = normalize_email(&request.email)?;
        validate_login_password(&request.password)?;

        let account = self.repository.find_for_login(&email).await?;
        let encoded_hash = match account.as_ref() {
            Some(account) => account.password_hash.clone(),
            None => dummy_password_hash().await?,
        };

        let password_matches = verify_password(request.password, encoded_hash).await?;
        let Some(account) = account else {
            return Err(AuthError::InvalidCredentials);
        };
        if !password_matches || !account.email_verified || account.deleted {
            return Err(AuthError::InvalidCredentials);
        }
        let Some(workspace_id) = account.workspace_id else {
            return Err(AuthError::InvalidCredentials);
        };

        let principal = Principal {
            user_id: account.user_id,
            workspace_id,
            scopes: vec![Scope::Read, Scope::Write],
            auth_method: AuthMethod::Password,
        };
        self.sessions.create_session(&principal, &request.device_name).await
    }

    async fn dispatch_verification(&self, email: &str, token: &str) {
        let url = format!("{}/verify-email?token={token}", self.public_base_url);
        if self
            .email_sender
            .send_verification(email, &url)
            .await
            .is_err()
        {
            // Never log the email address, verification URL, token, or provider response.
            tracing::warn!(code = "SF-AUTH-005", "verification email delivery failed");
        }
    }
}

fn normalize_email(raw: &str) -> Result<String, AuthError> {
    let email = raw.trim().to_ascii_lowercase();
    if email.is_empty() || email.len() > 254 || Mailbox::from_str(&email).is_err() {
        return Err(AuthError::InvalidRequest);
    }
    Ok(email)
}

fn validate_display_name(raw: &str) -> Result<String, AuthError> {
    let value = raw.trim();
    let character_count = value.chars().count();
    if character_count == 0
        || character_count > 80
        || value.chars().any(char::is_control)
    {
        return Err(AuthError::InvalidRequest);
    }
    Ok(value.to_owned())
}

fn validate_new_password(password: &str) -> Result<(), AuthError> {
    if password.len() < 12 || password.len() > 128 || password.chars().all(char::is_whitespace) {
        return Err(AuthError::InvalidRequest);
    }
    Ok(())
}

fn validate_login_password(password: &str) -> Result<(), AuthError> {
    if password.is_empty() || password.len() > 128 {
        return Err(AuthError::InvalidRequest);
    }
    Ok(())
}

fn argon2id() -> Result<Argon2<'static>, AuthError> {
    let params = Params::new(
        ARGON2_MEMORY_KIB,
        ARGON2_ITERATIONS,
        ARGON2_LANES,
        None,
    )
    .map_err(|_| AuthError::BackendUnavailable)?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

fn hash_password_sync(password: &[u8]) -> Result<String, AuthError> {
    let engine = argon2id()?;
    engine
        .hash_password(password)
        .map(|hash| hash.to_string())
        .map_err(|_| AuthError::BackendUnavailable)
}

async fn hash_password(password: String) -> Result<String, AuthError> {
    tokio::task::spawn_blocking(move || hash_password_sync(password.as_bytes()))
        .await
        .map_err(|_| AuthError::BackendUnavailable)?
}

fn verify_password_sync(password: &[u8], encoded_hash: &str) -> Result<bool, AuthError> {
    let parsed = PasswordHash::new(encoded_hash).map_err(|_| AuthError::BackendUnavailable)?;
    let engine = argon2id()?;
    Ok(engine.verify_password(password, &parsed).is_ok())
}

async fn verify_password(password: String, encoded_hash: String) -> Result<bool, AuthError> {
    tokio::task::spawn_blocking(move || {
        verify_password_sync(password.as_bytes(), &encoded_hash)
    })
    .await
    .map_err(|_| AuthError::BackendUnavailable)?
}

static DUMMY_PASSWORD_HASH: OnceLock<Result<String, AuthError>> = OnceLock::new();

async fn dummy_password_hash() -> Result<String, AuthError> {
    tokio::task::spawn_blocking(|| {
        DUMMY_PASSWORD_HASH
            .get_or_init(|| hash_password_sync(b"SafiData dummy account password"))
            .clone()
    })
    .await
    .map_err(|_| AuthError::BackendUnavailable)?
}

fn generate_verification_token() -> (String, [u8; 32]) {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let token = hex::encode(bytes);
    let token_hash = *blake3::hash(&bytes).as_bytes();
    (token, token_hash)
}

fn hash_presented_verification_token(raw: &str) -> Result<[u8; 32], AuthError> {
    let token = hex::decode(raw.trim()).map_err(|_| AuthError::InvalidRequest)?;
    let bytes: [u8; 32] = token.try_into().map_err(|_| AuthError::InvalidRequest)?;
    Ok(*blake3::hash(&bytes).as_bytes())
}

/// SMTP sender using mandatory STARTTLS. All credentials and hostnames are read
/// from environment variables and never stored in the repository.
pub struct SmtpVerificationEmailSender {
    transport: AsyncSmtpTransport<Tokio1Executor>,
    from: Mailbox,
}

impl std::fmt::Debug for SmtpVerificationEmailSender {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SmtpVerificationEmailSender")
            .field("from", &self.from)
            .field("credentials", &"[REDACTED]")
            .finish()
    }
}

impl SmtpVerificationEmailSender {
    pub fn from_env() -> Result<(Self, String), AuthError> {
        let host = std::env::var("SAFI_SMTP_HOST").map_err(|_| AuthError::BackendUnavailable)?;
        let username =
            std::env::var("SAFI_SMTP_USERNAME").map_err(|_| AuthError::BackendUnavailable)?;
        let password =
            std::env::var("SAFI_SMTP_PASSWORD").map_err(|_| AuthError::BackendUnavailable)?;
        let from_raw =
            std::env::var("SAFI_SMTP_FROM").map_err(|_| AuthError::BackendUnavailable)?;
        let public_base_url =
            std::env::var("SAFI_PUBLIC_BASE_URL").map_err(|_| AuthError::BackendUnavailable)?;
        if !public_base_url.starts_with("https://") {
            return Err(AuthError::InvalidRequest);
        }
        let port = std::env::var("SAFI_SMTP_PORT")
            .ok()
            .map(|value| value.parse::<u16>().map_err(|_| AuthError::InvalidRequest))
            .transpose()?
            .unwrap_or(587);
        let from = Mailbox::from_str(&from_raw).map_err(|_| AuthError::InvalidRequest)?;
        let credentials = Credentials::new(username, password);
        let transport = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&host)
            .map_err(|_| AuthError::BackendUnavailable)?
            .port(port)
            .credentials(credentials)
            .timeout(Some(Duration::from_secs(15)))
            .build();

        Ok((
            Self { transport, from },
            public_base_url.trim_end_matches('/').to_owned(),
        ))
    }
}

#[async_trait]
impl VerificationEmailSender for SmtpVerificationEmailSender {
    async fn send_verification(
        &self,
        recipient: &str,
        verification_url: &str,
    ) -> Result<(), AuthError> {
        let to = Mailbox::from_str(recipient).map_err(|_| AuthError::InvalidRequest)?;
        let message = Message::builder()
            .from(self.from.clone())
            .to(to)
            .subject("Verify your SafiData email")
            .header(ContentType::TEXT_PLAIN)
            .body(format!(
                "Welcome to SafiData.\n\nVerify your email address by opening this link:\n{verification_url}\n\nThis link expires in 24 hours and can only be used once.\nIf you did not create this account, you can ignore this message."
            ))
            .map_err(|_| AuthError::BackendUnavailable)?;
        self.transport
            .send(message)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        Ok(())
    }
}

/// PostgreSQL account repository. All account provisioning steps are one
/// transaction; no email is sent until after that transaction has committed.
pub struct SqlxAccountRepository {
    pool: PgPool,
}

impl SqlxAccountRepository {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AccountRepository for SqlxAccountRepository {
    async fn register(
        &self,
        email: &str,
        display_name: &str,
        password_hash: &str,
        token_hash: &[u8; 32],
        token_expires_at: i64,
    ) -> Result<bool, AuthError> {
        let mut transaction = self.pool.begin().await.map_err(|_| AuthError::BackendUnavailable)?;
        let user_id = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO users (email, display_name) VALUES ($1, $2) ON CONFLICT DO NOTHING RETURNING id",
        )
        .bind(email)
        .bind(display_name)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        let Some(user_id) = user_id else {
            transaction.rollback().await.map_err(|_| AuthError::BackendUnavailable)?;
            return Ok(false);
        };

        let workspace_name = format!("{display_name}'s workspace");
        let workspace_id = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO workspaces (name, plan_code) VALUES ($1, 'free') RETURNING id",
        )
        .bind(workspace_name)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        sqlx::query("SELECT set_config('app.workspace_id', $1, true)")
            .bind(workspace_id.to_string())
            .execute(&mut *transaction)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.user_id', $1, true)")
            .bind(user_id.to_string())
            .execute(&mut *transaction)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        sqlx::query(
            "INSERT INTO memberships (workspace_id, user_id, role) VALUES ($1, $2, 'owner')",
        )
        .bind(workspace_id)
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        sqlx::query(
            "INSERT INTO user_credentials (user_id, password_hash) VALUES ($1, $2)",
        )
        .bind(user_id)
        .bind(password_hash)
        .execute(&mut *transaction)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        sqlx::query(
            "INSERT INTO email_verification_tokens (user_id, token_hash, expires_at) VALUES ($1, $2, to_timestamp($3))",
        )
        .bind(user_id)
        .bind(token_hash.as_slice())
        .bind(token_expires_at)
        .execute(&mut *transaction)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        sqlx::query(
            "INSERT INTO subscriptions (workspace_id, plan_code, status) VALUES ($1, 'free', 'active')",
        )
        .bind(workspace_id)
        .execute(&mut *transaction)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        transaction.commit().await.map_err(|_| AuthError::BackendUnavailable)?;
        Ok(true)
    }

    async fn find_for_login(&self, email: &str) -> Result<Option<LoginAccount>, AuthError> {
        let mut transaction = self.pool.begin().await.map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.auth_lookup', 'true', true)")
            .execute(&mut *transaction)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        let row = sqlx::query(
            r"
            SELECT users.id AS user_id,
                   membership.workspace_id,
                   user_credentials.password_hash,
                   (users.email_verified_at IS NOT NULL) AS email_verified,
                   (users.deleted_at IS NOT NULL) AS deleted
            FROM users
            INNER JOIN user_credentials ON user_credentials.user_id = users.id
            LEFT JOIN LATERAL (
                SELECT workspace_id
                FROM memberships
                WHERE memberships.user_id = users.id
                ORDER BY (memberships.role = 'owner') DESC, memberships.created_at ASC
                LIMIT 1
            ) AS membership ON TRUE
            WHERE lower(users.email) = $1
            ",
        )
        .bind(email)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        let account = row
            .map(|row| {
                Ok(LoginAccount {
                    user_id: row.try_get("user_id").map_err(|_| AuthError::BackendUnavailable)?,
                    workspace_id: row.try_get("workspace_id").map_err(|_| AuthError::BackendUnavailable)?,
                    password_hash: row.try_get("password_hash").map_err(|_| AuthError::BackendUnavailable)?,
                    email_verified: row.try_get("email_verified").map_err(|_| AuthError::BackendUnavailable)?,
                    deleted: row.try_get("deleted").map_err(|_| AuthError::BackendUnavailable)?,
                })
            })
            .transpose()?;

        transaction.commit().await.map_err(|_| AuthError::BackendUnavailable)?;
        Ok(account)
    }

    async fn verify_email(
        &self,
        token_hash: &[u8; 32],
        now: i64,
    ) -> Result<bool, AuthError> {
        let mut transaction = self.pool.begin().await.map_err(|_| AuthError::BackendUnavailable)?;
        let user_id = sqlx::query_scalar::<_, Uuid>(
            r"
            UPDATE email_verification_tokens
            SET consumed_at = to_timestamp($2)
            WHERE token_hash = $1
              AND consumed_at IS NULL
              AND expires_at > to_timestamp($2)
            RETURNING user_id
            ",
        )
        .bind(token_hash.as_slice())
        .bind(now)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        let Some(user_id) = user_id else {
            transaction.commit().await.map_err(|_| AuthError::BackendUnavailable)?;
            return Ok(false);
        };

        let result = sqlx::query(
            "UPDATE users SET email_verified_at = COALESCE(email_verified_at, to_timestamp($2)), updated_at = to_timestamp($2) WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(user_id)
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        transaction.commit().await.map_err(|_| AuthError::BackendUnavailable)?;
        Ok(result.rows_affected() == 1)
    }

    async fn replace_verification_token(
        &self,
        email: &str,
        token_hash: &[u8; 32],
        token_expires_at: i64,
        now: i64,
    ) -> Result<bool, AuthError> {
        let mut transaction = self.pool.begin().await.map_err(|_| AuthError::BackendUnavailable)?;
        let user_id = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM users WHERE lower(email) = $1 AND email_verified_at IS NULL AND deleted_at IS NULL",
        )
        .bind(email)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        let Some(user_id) = user_id else {
            transaction.commit().await.map_err(|_| AuthError::BackendUnavailable)?;
            return Ok(false);
        };

        sqlx::query(
            "UPDATE email_verification_tokens SET consumed_at = to_timestamp($2) WHERE user_id = $1 AND consumed_at IS NULL",
        )
        .bind(user_id)
        .bind(now)
        .execute(&mut *transaction)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        sqlx::query(
            "INSERT INTO email_verification_tokens (user_id, token_hash, expires_at) VALUES ($1, $2, to_timestamp($3))",
        )
        .bind(user_id)
        .bind(token_hash.as_slice())
        .bind(token_expires_at)
        .execute(&mut *transaction)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        transaction.commit().await.map_err(|_| AuthError::BackendUnavailable)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::service::in_memory_service;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[derive(Default)]
    struct MemoryState {
        accounts: HashMap<String, LoginAccount>,
        pending_tokens: HashMap<[u8; 32], (String, i64)>,
    }

    #[derive(Default)]
    struct MemoryAccountRepository {
        state: RwLock<MemoryState>,
    }

    #[async_trait]
    impl AccountRepository for MemoryAccountRepository {
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
            let user_id = Uuid::now_v7();
            let workspace_id = Uuid::now_v7();
            state.accounts.insert(
                email.to_owned(),
                LoginAccount {
                    user_id,
                    workspace_id: Some(workspace_id),
                    password_hash: password_hash.to_owned(),
                    email_verified: false,
                    deleted: false,
                },
            );
            state.pending_tokens.insert(
                *token_hash,
                (email.to_owned(), token_expires_at),
            );
            Ok(true)
        }

        async fn find_for_login(&self, email: &str) -> Result<Option<LoginAccount>, AuthError> {
            Ok(self.state.read().await.accounts.get(email).cloned())
        }

        async fn verify_email(
            &self,
            token_hash: &[u8; 32],
            now: i64,
        ) -> Result<bool, AuthError> {
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
            state.pending_tokens.insert(
                *token_hash,
                (email.to_owned(), token_expires_at),
            );
            Ok(true)
        }
    }

    #[derive(Default, Clone)]
    struct MemoryEmailSender {
        messages: Arc<RwLock<Vec<(String, String)>>>,
        fail: Arc<AtomicBool>,
    }

    impl MemoryEmailSender {
        async fn messages(&self) -> Vec<(String, String)> {
            self.messages.read().await.clone()
        }

        fn set_fail(&self, fail: bool) {
            self.fail.store(fail, Ordering::Relaxed);
        }
    }

    #[async_trait]
    impl VerificationEmailSender for MemoryEmailSender {
        async fn send_verification(
            &self,
            recipient: &str,
            verification_url: &str,
        ) -> Result<(), AuthError> {
            if self.fail.load(Ordering::Relaxed) {
                return Err(AuthError::BackendUnavailable);
            }
            self.messages
                .write()
                .await
                .push((recipient.to_owned(), verification_url.to_owned()));
            Ok(())
        }
    }

    struct Fixture {
        service: AccountAuthService,
        mailer: MemoryEmailSender,
        sessions: Arc<AuthService>,
    }

    fn fixture() -> Fixture {
        let sessions = Arc::new(in_memory_service().0);
        let mailer = MemoryEmailSender::default();
        let service = AccountAuthService::new(
            Arc::new(MemoryAccountRepository::default()),
            Arc::new(mailer.clone()),
            sessions.clone(),
            "https://safidata.example",
        );
        Fixture {
            service,
            mailer,
            sessions,
        }
    }

    fn request(email: &str) -> RegisterAccountRequest {
        RegisterAccountRequest {
            email: email.to_owned(),
            display_name: "Safi User".to_owned(),
            password: "A secure phrase 2026!".to_owned(),
        }
    }

    fn token_from_message(message: &(String, String)) -> String {
        message
            .1
            .split("token=")
            .nth(1)
            .unwrap_or_default()
            .to_owned()
    }

    #[tokio::test]
    async fn registration_requires_email_verification_before_login() {
        let fixture = fixture();
        fixture.service.register(request(" USER@example.test ")).await.unwrap();
        let messages = fixture.mailer.messages().await;
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].0, "user@example.test");
        assert!(messages[0].1.starts_with("https://safidata.example/verify-email?token="));

        let failed = fixture.service.login(LoginAccountRequest {
            email: "user@example.test".to_owned(),
            password: "A secure phrase 2026!".to_owned(),
            device_name: "Browser".to_owned(),
        }).await;
        assert_eq!(failed.unwrap_err(), AuthError::InvalidCredentials);

        fixture.service.verify_email(&token_from_message(&messages[0])).await.unwrap();
        let issued = fixture.service.login(LoginAccountRequest {
            email: "user@example.test".to_owned(),
            password: "A secure phrase 2026!".to_owned(),
            device_name: "Browser".to_owned(),
        }).await.unwrap();
        assert!(issued.token.starts_with("sfs_"));
        let authenticated = fixture.sessions.authenticate_session(&issued.token).await.unwrap();
        assert_eq!(authenticated.principal.auth_method, AuthMethod::Session {
            session_id: issued.session.id
        });
    }

    #[tokio::test]
    async fn duplicate_registration_is_non_enumerating_and_does_not_send_twice() {
        let fixture = fixture();
        fixture.service.register(request("user@example.test")).await.unwrap();
        fixture.service.register(request("USER@example.test")).await.unwrap();
        assert_eq!(fixture.mailer.messages().await.len(), 1);
    }

    #[tokio::test]
    async fn wrong_password_unknown_user_and_unverified_user_share_failure() {
        let fixture = fixture();
        fixture.service.register(request("user@example.test")).await.unwrap();
        let unknown = fixture.service.login(LoginAccountRequest {
            email: "unknown@example.test".to_owned(),
            password: "A secure phrase 2026!".to_owned(),
            device_name: "Browser".to_owned(),
        }).await;
        let wrong = fixture.service.login(LoginAccountRequest {
            email: "user@example.test".to_owned(),
            password: "A different phrase 2026!".to_owned(),
            device_name: "Browser".to_owned(),
        }).await;
        assert_eq!(unknown.unwrap_err(), AuthError::InvalidCredentials);
        assert_eq!(wrong.unwrap_err(), AuthError::InvalidCredentials);
    }

    #[tokio::test]
    async fn resend_replaces_the_old_token_and_verifies_only_the_new_one() {
        let fixture = fixture();
        fixture.service.register(request("user@example.test")).await.unwrap();
        let first = fixture.mailer.messages().await;
        fixture.service.resend_verification("user@example.test").await.unwrap();
        let second = fixture.mailer.messages().await;
        assert_eq!(second.len(), 2);

        assert_eq!(
            fixture.service.verify_email(&token_from_message(&first[0])).await.unwrap_err(),
            AuthError::InvalidRequest
        );
        fixture.service.verify_email(&token_from_message(&second[1])).await.unwrap();
    }

    #[tokio::test]
    async fn expired_and_replayed_tokens_are_rejected() {
        let fixture = fixture();
        let service = fixture.service.with_verification_ttl(Duration::ZERO);
        service.register(request("user@example.test")).await.unwrap();
        let messages = fixture.mailer.messages().await;
        let token = token_from_message(&messages[0]);
        assert_eq!(service.verify_email(&token).await.unwrap_err(), AuthError::InvalidRequest);
        assert_eq!(service.verify_email(&token).await.unwrap_err(), AuthError::InvalidRequest);
    }

    #[tokio::test]
    async fn mailer_failure_does_not_verify_the_account_or_break_resend() {
        let fixture = fixture();
        fixture.mailer.set_fail(true);
        fixture.service.register(request("user@example.test")).await.unwrap();
        let blocked = fixture.service.login(LoginAccountRequest {
            email: "user@example.test".to_owned(),
            password: "A secure phrase 2026!".to_owned(),
            device_name: "Browser".to_owned(),
        }).await;
        assert_eq!(blocked.unwrap_err(), AuthError::InvalidCredentials);
        fixture.service.resend_verification("user@example.test").await.unwrap();
    }

    #[tokio::test]
    async fn registration_validates_email_name_and_password_before_storage() {
        let fixture = fixture();
        assert_eq!(
            fixture.service.register(RegisterAccountRequest {
                email: "not-an-email".to_owned(),
                display_name: "Safi User".to_owned(),
                password: "A secure phrase 2026!".to_owned(),
            }).await.unwrap_err(),
            AuthError::InvalidRequest
        );
        assert_eq!(
            fixture.service.register(RegisterAccountRequest {
                email: "user@example.test".to_owned(),
                display_name: "   ".to_owned(),
                password: "A secure phrase 2026!".to_owned(),
            }).await.unwrap_err(),
            AuthError::InvalidRequest
        );
        assert_eq!(
            fixture.service.register(RegisterAccountRequest {
                email: "user@example.test".to_owned(),
                display_name: "Safi User".to_owned(),
                password: "short".to_owned(),
            }).await.unwrap_err(),
            AuthError::InvalidRequest
        );
        assert!(fixture.mailer.messages().await.is_empty());
    }
}
