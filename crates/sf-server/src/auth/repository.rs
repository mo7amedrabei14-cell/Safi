//! Persistence for API keys and sessions. PostgreSQL is the source of truth;
//! the in-memory implementations exist for unit tests and local development and
//! are never selected in production wiring.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPool;
use sqlx::Row;
use tokio::sync::RwLock;
use uuid::Uuid;

use super::Scope;
use super::device::DeviceRegistration;
use crate::error::AuthError;

/// Durable API-key row. `secret_hash` is the only representation of the secret.
#[derive(Clone, Serialize, Deserialize)]
pub struct ApiKeyRecord {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub key_prefix: String,
    pub secret_hash: String,
    pub scopes: Vec<Scope>,
    pub revoked: bool,
    pub expires_at: Option<i64>,
    pub last_used_at: Option<i64>,
}

impl std::fmt::Debug for ApiKeyRecord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ApiKeyRecord")
            .field("id", &self.id)
            .field("workspace_id", &self.workspace_id)
            .field("user_id", &self.user_id)
            .field("key_prefix", &self.key_prefix)
            .field("secret_hash", &"[REDACTED]")
            .field("scopes", &self.scopes)
            .field("revoked", &self.revoked)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

impl ApiKeyRecord {
    /// Whether the key is past its expiry at `now` (UNIX seconds).
    #[must_use]
    pub fn is_expired_at(&self, now: i64) -> bool {
        self.expires_at.is_some_and(|expires_at| expires_at <= now)
    }
}

/// Durable session row; `token_hash` is the only representation of the token.
#[derive(Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub api_key_id: Option<Uuid>,
    pub token_hash: String,
    pub device_name: String,
    pub scopes: Vec<Scope>,
    pub created_at: i64,
    pub last_seen_at: i64,
    pub expires_at: i64,
    pub revoked: bool,
    #[serde(default)]
    pub device_id: Option<Uuid>,
    #[serde(default)]
    pub device_revoked: bool,
    #[serde(default)]
    pub account_disabled: bool,
}

impl std::fmt::Debug for SessionRecord {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SessionRecord")
            .field("id", &self.id)
            .field("workspace_id", &self.workspace_id)
            .field("user_id", &self.user_id)
            .field("token_hash", &"[REDACTED]")
            .field("device_name", &self.device_name)
            .field("scopes", &self.scopes)
            .field("expires_at", &self.expires_at)
            .field("revoked", &self.revoked)
            .field("device_id", &self.device_id)
            .field("device_revoked", &self.device_revoked)
            .field("account_disabled", &self.account_disabled)
            .finish()
    }
}

impl SessionRecord {
    /// Whether the session is past its expiry at `now` (UNIX seconds).
    #[must_use]
    pub fn is_expired_at(&self, now: i64) -> bool {
        self.expires_at <= now
    }
}

/// API-key lookup used by the authentication service.
#[async_trait]
pub trait ApiKeyRepository: Send + Sync {
    /// Find a key by its indexed prefix.
    async fn find_by_prefix(&self, prefix: &str) -> Result<Option<ApiKeyRecord>, AuthError>;
    /// Record a successful use (only on cache misses, to keep the hot path cheap).
    async fn touch_last_used(&self, id: Uuid, at: i64) -> Result<(), AuthError>;
}

/// Session persistence used by the authentication service.
#[async_trait]
pub trait SessionRepository: Send + Sync {
    /// Atomically replace the active device/session and return the linked device id.
    async fn replace_active(
        &self,
        record: &SessionRecord,
        device: Option<&DeviceRegistration>,
    ) -> Result<Option<Uuid>, AuthError>;
    /// Find a session by its token hash.
    async fn find_by_hash(&self, token_hash: &str) -> Result<Option<SessionRecord>, AuthError>;
    /// Update the sliding `last_seen_at`.
    async fn touch(&self, id: Uuid, at: i64) -> Result<(), AuthError>;
    /// Mark a session revoked.
    async fn revoke(&self, id: Uuid, at: i64) -> Result<(), AuthError>;
}

/// Process-local API-key repository for tests.
#[derive(Default)]
pub struct InMemoryApiKeyRepository {
    records: RwLock<Vec<ApiKeyRecord>>,
    lookups: AtomicUsize,
}

impl InMemoryApiKeyRepository {
    /// Seed the repository.
    pub async fn new(records: Vec<ApiKeyRecord>) -> Self {
        Self {
            records: RwLock::new(records),
            lookups: AtomicUsize::new(0),
        }
    }

    /// Number of database lookups (cache-miss proof).
    #[must_use]
    pub fn lookups(&self) -> usize {
        self.lookups.load(Ordering::Relaxed)
    }
}

#[async_trait]
impl ApiKeyRepository for InMemoryApiKeyRepository {
    async fn find_by_prefix(&self, prefix: &str) -> Result<Option<ApiKeyRecord>, AuthError> {
        self.lookups.fetch_add(1, Ordering::Relaxed);
        let records = self.records.read().await;
        Ok(records
            .iter()
            .find(|record| record.key_prefix == prefix)
            .cloned())
    }

    async fn touch_last_used(&self, id: Uuid, at: i64) -> Result<(), AuthError> {
        let mut records = self.records.write().await;
        if let Some(record) = records.iter_mut().find(|record| record.id == id) {
            record.last_used_at = Some(at);
        }
        Ok(())
    }
}

/// Process-local device row used by the deterministic test repository.
#[derive(Clone)]
struct InMemoryDeviceRecord {
    id: Uuid,
    user_id: Uuid,
    installation_key_hash: [u8; 32],
    revoked: bool,
}

#[derive(Default)]
struct InMemorySessionState {
    records: Vec<SessionRecord>,
    devices: Vec<InMemoryDeviceRecord>,
}

/// Process-local session repository for tests.
#[derive(Default)]
pub struct InMemorySessionRepository {
    state: RwLock<InMemorySessionState>,
}

impl InMemorySessionRepository {
    /// Create an empty repository.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl SessionRepository for InMemorySessionRepository {
    async fn replace_active(
        &self,
        record: &SessionRecord,
        device: Option<&DeviceRegistration>,
    ) -> Result<Option<Uuid>, AuthError> {
        // One lock deliberately protects devices and sessions together so this
        // test implementation models the production transaction's atomicity.
        let mut state = self.state.write().await;

        let device_id = if let Some(registration) = device {
            let existing_id = state
                .devices
                .iter()
                .find(|existing| {
                    existing.user_id == record.user_id
                        && existing.installation_key_hash == registration.installation_key_hash
                })
                .map(|existing| existing.id);

            for existing in state
                .devices
                .iter_mut()
                .filter(|item| item.user_id == record.user_id)
            {
                existing.revoked = existing_id != Some(existing.id);
            }

            match existing_id {
                Some(id) => Some(id),
                None => {
                    let id = Uuid::now_v7();
                    state.devices.push(InMemoryDeviceRecord {
                        id,
                        user_id: record.user_id,
                        installation_key_hash: registration.installation_key_hash,
                        revoked: false,
                    });
                    Some(id)
                }
            }
        } else {
            for existing in state
                .devices
                .iter_mut()
                .filter(|item| item.user_id == record.user_id)
            {
                existing.revoked = true;
            }
            None
        };

        for existing in &mut state.records {
            if existing.user_id == record.user_id && !existing.revoked {
                existing.revoked = true;
                existing.last_seen_at = record.created_at;
            }
        }

        let mut next = record.clone();
        next.device_id = device_id;
        next.device_revoked = false;
        next.account_disabled = false;
        state.records.push(next);
        Ok(device_id)
    }

    async fn find_by_hash(&self, token_hash: &str) -> Result<Option<SessionRecord>, AuthError> {
        let state = self.state.read().await;
        let mut found = state
            .records
            .iter()
            .find(|record| record.token_hash == token_hash)
            .cloned();

        if let Some(record) = found.as_mut() {
            if let Some(device_id) = record.device_id {
                record.device_revoked = state
                    .devices
                    .iter()
                    .find(|device| device.id == device_id)
                    .map_or(true, |device| device.revoked);
            }
        }
        Ok(found)
    }

    async fn touch(&self, id: Uuid, at: i64) -> Result<(), AuthError> {
        let mut state = self.state.write().await;
        if let Some(record) = state.records.iter_mut().find(|record| record.id == id) {
            record.last_seen_at = at;
        }
        Ok(())
    }

    async fn revoke(&self, id: Uuid, at: i64) -> Result<(), AuthError> {
        let mut state = self.state.write().await;
        if let Some(record) = state.records.iter_mut().find(|record| record.id == id) {
            record.revoked = true;
            record.last_seen_at = at;
        }
        Ok(())
    }
}

/// SQLx-backed API-key repository (PostgreSQL 18).
pub struct SqlxApiKeyRepository {
    pool: PgPool,
}

impl SqlxApiKeyRepository {
    /// Wrap a connection pool.
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn backend_error() -> AuthError {
    AuthError::BackendUnavailable
}

#[async_trait]
impl ApiKeyRepository for SqlxApiKeyRepository {
    async fn find_by_prefix(&self, prefix: &str) -> Result<Option<ApiKeyRecord>, AuthError> {
        let mut transaction = self.pool.begin().await.map_err(|_| backend_error())?;
        sqlx::query("SELECT set_config('app.auth_lookup', 'true', true)")
            .execute(&mut *transaction)
            .await
            .map_err(|_| backend_error())?;

        let row = sqlx::query(
            r"
            SELECT id,
                   workspace_id,
                   created_by AS user_id,
                   key_prefix,
                   encode(secret_hash, 'hex') AS secret_hash,
                   scopes,
                   (revoked_at IS NOT NULL) AS revoked,
                   EXTRACT(EPOCH FROM expires_at)::bigint AS expires_at,
                   EXTRACT(EPOCH FROM last_used_at)::bigint AS last_used_at
            FROM api_keys
            WHERE key_prefix = $1
            ",
        )
        .bind(prefix)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| backend_error())?;

        let record = if let Some(row) = row {
            let scopes: Vec<String> = row.try_get("scopes").map_err(|_| backend_error())?;
            Some(ApiKeyRecord {
                id: row.try_get("id").map_err(|_| backend_error())?,
                workspace_id: row.try_get("workspace_id").map_err(|_| backend_error())?,
                user_id: row.try_get("user_id").map_err(|_| backend_error())?,
                key_prefix: row.try_get("key_prefix").map_err(|_| backend_error())?,
                secret_hash: row.try_get("secret_hash").map_err(|_| backend_error())?,
                scopes: super::parse_scopes(&scopes),
                revoked: row.try_get("revoked").map_err(|_| backend_error())?,
                expires_at: row.try_get("expires_at").map_err(|_| backend_error())?,
                last_used_at: row.try_get("last_used_at").map_err(|_| backend_error())?,
            })
        } else {
            None
        };

        transaction.commit().await.map_err(|_| backend_error())?;
        Ok(record)
    }

    async fn touch_last_used(&self, id: Uuid, at: i64) -> Result<(), AuthError> {
        let mut transaction = self.pool.begin().await.map_err(|_| backend_error())?;
        sqlx::query("SELECT set_config('app.auth_lookup', 'true', true)")
            .execute(&mut *transaction)
            .await
            .map_err(|_| backend_error())?;
        let workspace_id =
            sqlx::query_scalar::<_, Uuid>("SELECT workspace_id FROM api_keys WHERE id = $1")
                .bind(id)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(|_| backend_error())?;
        let Some(workspace_id) = workspace_id else {
            transaction.commit().await.map_err(|_| backend_error())?;
            return Ok(());
        };
        sqlx::query("SELECT set_config('app.workspace_id', $1, true)")
            .bind(workspace_id.to_string())
            .execute(&mut *transaction)
            .await
            .map_err(|_| backend_error())?;
        sqlx::query("UPDATE api_keys SET last_used_at = to_timestamp($2) WHERE id = $1")
            .bind(id)
            .bind(at)
            .execute(&mut *transaction)
            .await
            .map_err(|_| backend_error())?;
        transaction.commit().await.map_err(|_| backend_error())?;
        Ok(())
    }
}

/// SQLx-backed session repository (PostgreSQL 18).
pub struct SqlxSessionRepository {
    pool: PgPool,
}

impl SqlxSessionRepository {
    /// Wrap a connection pool.
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SessionRepository for SqlxSessionRepository {
    async fn replace_active(
        &self,
        record: &SessionRecord,
        device: Option<&DeviceRegistration>,
    ) -> Result<Option<Uuid>, AuthError> {
        let mut transaction = self.pool.begin().await.map_err(|_| backend_error())?;
        sqlx::query("SELECT set_config('app.auth_lookup', 'true', true)")
            .execute(&mut *transaction)
            .await
            .map_err(|_| backend_error())?;
        sqlx::query("SELECT set_config('app.workspace_id', $1, true)")
            .bind(record.workspace_id.to_string())
            .execute(&mut *transaction)
            .await
            .map_err(|_| backend_error())?;

        // Serialize all sign-ins for an account, including concurrent attempts
        // from different installations and across different workspaces.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(record.user_id.to_string())
            .execute(&mut *transaction)
            .await
            .map_err(|_| backend_error())?;

        let device_id = if let Some(registration) = device {
            let installation_hash = hex::encode(registration.installation_key_hash);
            let existing_id = sqlx::query_scalar::<_, Uuid>(
                "SELECT id FROM devices WHERE user_id = $1 AND installation_key_hash = decode($2, 'hex') FOR UPDATE",
            )
            .bind(record.user_id)
            .bind(&installation_hash)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|_| backend_error())?;

            // Deactivate other installations before reactivating/inserting this
            // one so the partial unique index is never transiently violated.
            sqlx::query(
                r"
                UPDATE devices
                SET revoked_at = to_timestamp($2)
                WHERE user_id = $1
                  AND revoked_at IS NULL
                  AND ($3::uuid IS NULL OR id <> $3)
                ",
            )
            .bind(record.user_id)
            .bind(record.created_at)
            .bind(existing_id)
            .execute(&mut *transaction)
            .await
            .map_err(|_| backend_error())?;

            if let Some(existing_id) = existing_id {
                sqlx::query(
                    r"
                    UPDATE devices
                    SET device_name = $2,
                        platform = $3,
                        last_seen_at = to_timestamp($4),
                        revoked_at = NULL
                    WHERE id = $1 AND user_id = $5
                    ",
                )
                .bind(existing_id)
                .bind(&record.device_name)
                .bind(registration.platform.as_str())
                .bind(record.last_seen_at)
                .bind(record.user_id)
                .execute(&mut *transaction)
                .await
                .map_err(|_| backend_error())?;
                Some(existing_id)
            } else {
                Some(
                    sqlx::query_scalar::<_, Uuid>(
                        r"
                        INSERT INTO devices (
                            workspace_id, user_id, device_name, platform,
                            installation_key_hash, registered_at, last_seen_at
                        )
                        VALUES ($1, $2, $3, $4, decode($5, 'hex'), to_timestamp($6), to_timestamp($7))
                        RETURNING id
                        ",
                    )
                    .bind(record.workspace_id)
                    .bind(record.user_id)
                    .bind(&record.device_name)
                    .bind(registration.platform.as_str())
                    .bind(&installation_hash)
                    .bind(record.created_at)
                    .bind(record.last_seen_at)
                    .fetch_one(&mut *transaction)
                    .await
                    .map_err(|_| backend_error())?,
                )
            }
        } else {
            // API-key sessions remain supported but have no account-installation
            // identity. Deactivate any prior device record in this transaction.
            sqlx::query(
                "UPDATE devices SET revoked_at = to_timestamp($2) WHERE user_id = $1 AND revoked_at IS NULL",
            )
            .bind(record.user_id)
            .bind(record.created_at)
            .execute(&mut *transaction)
            .await
            .map_err(|_| backend_error())?;
            None
        };

        sqlx::query(
            r"
            UPDATE sessions
            SET revoked_at = to_timestamp($2)
            WHERE user_id = $1
              AND id <> $3
              AND revoked_at IS NULL
            ",
        )
        .bind(record.user_id)
        .bind(record.created_at)
        .bind(record.id)
        .execute(&mut *transaction)
        .await
        .map_err(|_| backend_error())?;

        sqlx::query(
            r"
            INSERT INTO sessions (
                id, workspace_id, user_id, api_key_id, device_id, token_hash,
                device_name, scopes, created_at, last_seen_at, expires_at, revoked_at
            )
            VALUES (
                $1, $2, $3, $4, $5, decode($6, 'hex'), $7, $8,
                to_timestamp($9), to_timestamp($10), to_timestamp($11), NULL
            )
            ",
        )
        .bind(record.id)
        .bind(record.workspace_id)
        .bind(record.user_id)
        .bind(record.api_key_id)
        .bind(device_id)
        .bind(&record.token_hash)
        .bind(&record.device_name)
        .bind(super::scopes_to_strings(&record.scopes))
        .bind(record.created_at)
        .bind(record.last_seen_at)
        .bind(record.expires_at)
        .execute(&mut *transaction)
        .await
        .map_err(|_| backend_error())?;

        transaction.commit().await.map_err(|_| backend_error())?;
        Ok(device_id)
    }

    async fn find_by_hash(&self, token_hash: &str) -> Result<Option<SessionRecord>, AuthError> {
        let mut transaction = self.pool.begin().await.map_err(|_| backend_error())?;
        sqlx::query("SELECT set_config('app.auth_lookup', 'true', true)")
            .execute(&mut *transaction)
            .await
            .map_err(|_| backend_error())?;

        let row = sqlx::query(
            r"
            SELECT sessions.id,
                   sessions.workspace_id,
                   sessions.user_id,
                   sessions.api_key_id,
                   sessions.device_id,
                   (
                       sessions.device_id IS NOT NULL
                       AND (devices.id IS NULL OR devices.revoked_at IS NOT NULL)
                   ) AS device_revoked,
                   (users.suspended_at IS NOT NULL OR users.deleted_at IS NOT NULL) AS account_disabled,
                   sessions.device_name,
                   sessions.scopes,
                   EXTRACT(EPOCH FROM sessions.created_at)::bigint AS created_at,
                   EXTRACT(EPOCH FROM sessions.last_seen_at)::bigint AS last_seen_at,
                   EXTRACT(EPOCH FROM sessions.expires_at)::bigint AS expires_at,
                   (sessions.revoked_at IS NOT NULL) AS revoked
            FROM sessions
            LEFT JOIN devices ON devices.id = sessions.device_id AND devices.user_id = sessions.user_id
            INNER JOIN users ON users.id = sessions.user_id
            WHERE sessions.token_hash = decode($1, 'hex')
            ",
        )
        .bind(token_hash)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| backend_error())?;

        let record = if let Some(row) = row {
            let scopes: Vec<String> = row.try_get("scopes").map_err(|_| backend_error())?;
            Some(SessionRecord {
                id: row.try_get("id").map_err(|_| backend_error())?,
                workspace_id: row.try_get("workspace_id").map_err(|_| backend_error())?,
                user_id: row.try_get("user_id").map_err(|_| backend_error())?,
                api_key_id: row.try_get("api_key_id").map_err(|_| backend_error())?,
                device_id: row.try_get("device_id").map_err(|_| backend_error())?,
                device_revoked: row.try_get("device_revoked").map_err(|_| backend_error())?,
                account_disabled: row.try_get("account_disabled").map_err(|_| backend_error())?,
                token_hash: token_hash.to_owned(),
                device_name: row.try_get("device_name").map_err(|_| backend_error())?,
                scopes: super::parse_scopes(&scopes),
                created_at: row.try_get("created_at").map_err(|_| backend_error())?,
                last_seen_at: row.try_get("last_seen_at").map_err(|_| backend_error())?,
                expires_at: row.try_get("expires_at").map_err(|_| backend_error())?,
                revoked: row.try_get("revoked").map_err(|_| backend_error())?,
            })
        } else {
            None
        };

        transaction.commit().await.map_err(|_| backend_error())?;
        Ok(record)
    }

    async fn touch(&self, id: Uuid, at: i64) -> Result<(), AuthError> {
        let mut transaction = self.pool.begin().await.map_err(|_| backend_error())?;
        sqlx::query("SELECT set_config('app.auth_lookup', 'true', true)")
            .execute(&mut *transaction)
            .await
            .map_err(|_| backend_error())?;
        let workspace_id =
            sqlx::query_scalar::<_, Uuid>("SELECT workspace_id FROM sessions WHERE id = $1")
                .bind(id)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(|_| backend_error())?;
        let Some(workspace_id) = workspace_id else {
            transaction.commit().await.map_err(|_| backend_error())?;
            return Ok(());
        };
        sqlx::query("SELECT set_config('app.workspace_id', $1, true)")
            .bind(workspace_id.to_string())
            .execute(&mut *transaction)
            .await
            .map_err(|_| backend_error())?;
        sqlx::query("UPDATE sessions SET last_seen_at = to_timestamp($2) WHERE id = $1")
            .bind(id)
            .bind(at)
            .execute(&mut *transaction)
            .await
            .map_err(|_| backend_error())?;
        transaction.commit().await.map_err(|_| backend_error())?;
        Ok(())
    }

    async fn revoke(&self, id: Uuid, at: i64) -> Result<(), AuthError> {
        let mut transaction = self.pool.begin().await.map_err(|_| backend_error())?;
        sqlx::query("SELECT set_config('app.auth_lookup', 'true', true)")
            .execute(&mut *transaction)
            .await
            .map_err(|_| backend_error())?;
        let workspace_id =
            sqlx::query_scalar::<_, Uuid>("SELECT workspace_id FROM sessions WHERE id = $1")
                .bind(id)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(|_| backend_error())?;
        let Some(workspace_id) = workspace_id else {
            transaction.commit().await.map_err(|_| backend_error())?;
            return Ok(());
        };
        sqlx::query("SELECT set_config('app.workspace_id', $1, true)")
            .bind(workspace_id.to_string())
            .execute(&mut *transaction)
            .await
            .map_err(|_| backend_error())?;
        sqlx::query(
            "UPDATE sessions SET revoked_at = to_timestamp($2) WHERE id = $1 AND revoked_at IS NULL",
        )
        .bind(id)
        .bind(at)
        .execute(&mut *transaction)
        .await
        .map_err(|_| backend_error())?;
        transaction.commit().await.map_err(|_| backend_error())?;
        Ok(())
    }
}

/// Shared handles exposed to the service.
pub type SharedApiKeyRepository = Arc<dyn ApiKeyRepository>;
/// Shared session repository handle.
pub type SharedSessionRepository = Arc<dyn SessionRepository>;
