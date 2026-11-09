//! Authentication service: verify API keys against PostgreSQL with a Valkey
//! read-through cache, then issue, validate, and revoke sessions.
//!
//! Security invariants:
//! - only BLAKE3 hashes of secrets are stored;
//! - API-key cache failures degrade latency, never correctness;
//! - session validation always reads PostgreSQL so device replacement and revocation
//!   cannot be bypassed by a stale session-cache entry.

use std::sync::Arc;
use std::time::Duration;

use uuid::Uuid;

use super::api_key::{generate_api_key, parse_api_key, verify_secret};
use super::cache::{api_key_cache_key, session_cache_key, SharedCache};
use super::repository::{
    ApiKeyRecord, SessionRecord, SharedApiKeyRepository, SharedSessionRepository,
};
use super::session::{generate_session_token, parse_session_token, token_hash, SessionView};
use super::{unix_now, AuthMethod, AuthenticatedSession, Principal, Scope};
use crate::error::AuthError;

const DEFAULT_KEY_CACHE_TTL: Duration = Duration::from_secs(300);
const DEFAULT_SESSION_TTL: Duration = Duration::from_secs(30 * 60);
const DEFAULT_TOUCH_INTERVAL_SECS: i64 = 60;

/// A newly issued session; `token` is shown to the client exactly once.
pub struct IssuedSession {
    pub token: String,
    pub session: SessionView,
}

/// API-key authentication and session lifecycle.
pub struct AuthService {
    api_keys: SharedApiKeyRepository,
    sessions: SharedSessionRepository,
    cache: SharedCache,
    key_cache_ttl: Duration,
    session_ttl: Duration,
    touch_interval_secs: i64,
}

impl AuthService {
    /// Build a service with the production defaults (5 min key cache, 30 min session).
    pub fn new(
        api_keys: SharedApiKeyRepository,
        sessions: SharedSessionRepository,
        cache: SharedCache,
    ) -> Self {
        Self {
            api_keys,
            sessions,
            cache,
            key_cache_ttl: DEFAULT_KEY_CACHE_TTL,
            session_ttl: DEFAULT_SESSION_TTL,
            touch_interval_secs: DEFAULT_TOUCH_INTERVAL_SECS,
        }
    }

    /// Override TTLs (tests and, later, plan-specific settings).
    #[must_use]
    pub fn with_ttls(mut self, key_cache_ttl: Duration, session_ttl: Duration) -> Self {
        self.key_cache_ttl = key_cache_ttl;
        self.session_ttl = session_ttl;
        self
    }

    /// Authenticate a presented API key and return its bound principal.
    pub async fn authenticate_api_key(&self, presented: &str) -> Result<Principal, AuthError> {
        let (prefix, secret) = parse_api_key(presented).ok_or(AuthError::InvalidCredentials)?;
        let record = self
            .load_api_key(prefix)
            .await?
            .ok_or(AuthError::InvalidCredentials)?;

        if !verify_secret(secret, &record.secret_hash) {
            return Err(AuthError::InvalidCredentials);
        }
        if record.revoked {
            return Err(AuthError::Revoked);
        }
        if record.is_expired_at(unix_now()) {
            return Err(AuthError::Expired);
        }

        Ok(Principal {
            user_id: record.user_id,
            workspace_id: record.workspace_id,
            scopes: record.scopes,
            auth_method: AuthMethod::ApiKey { key_id: record.id },
        })
    }

    /// Create a session for the device after a successful API-key authentication.
    pub async fn create_session(
        &self,
        principal: &Principal,
        device_name: &str,
    ) -> Result<IssuedSession, AuthError> {
        let device = device_name.trim();
        if device.is_empty() || device.chars().count() > 120 {
            return Err(AuthError::InvalidRequest);
        }

        let issued = generate_session_token();
        let now = unix_now();
        let api_key_id = match principal.auth_method {
            AuthMethod::ApiKey { key_id } => Some(key_id),
            AuthMethod::Session { .. } => None,
        };
        let record = SessionRecord {
            id: Uuid::now_v7(),
            workspace_id: principal.workspace_id,
            user_id: principal.user_id,
            api_key_id,
            token_hash: issued.token_hash().to_owned(),
            device_name: device.to_owned(),
            scopes: principal.scopes.clone(),
            created_at: now,
            last_seen_at: now,
            expires_at: now + self.session_ttl.as_secs() as i64,
            revoked: false,
        };

        self.sessions.replace_active(&record).await?;

        Ok(IssuedSession {
            token: issued.plaintext().to_owned(),
            session: session_view(&record),
        })
    }

    /// Validate a session token, sliding its TTL when it is used.
    pub async fn authenticate_session(
        &self,
        presented: &str,
    ) -> Result<AuthenticatedSession, AuthError> {
        let raw = parse_session_token(presented).ok_or(AuthError::InvalidCredentials)?;
        let hash = token_hash(raw);
        let cache_key = session_cache_key(&hash);

        let mut record = self
            .sessions
            .find_by_hash(&hash)
            .await?
            .ok_or(AuthError::InvalidCredentials)?;

        if record.revoked {
            let _ = self.cache.delete(&cache_key).await;
            return Err(AuthError::Revoked);
        }
        let now = unix_now();
        if record.is_expired_at(now) {
            let _ = self.cache.delete(&cache_key).await;
            return Err(AuthError::Expired);
        }

        if now - record.last_seen_at >= self.touch_interval_secs {
            record.last_seen_at = now;
            self.sessions.touch(record.id, now).await?;
        }

        Ok(AuthenticatedSession {
            principal: Principal {
                user_id: record.user_id,
                workspace_id: record.workspace_id,
                scopes: record.scopes.clone(),
                auth_method: AuthMethod::Session {
                    session_id: record.id,
                },
            },
            session_id: record.id,
            device_name: record.device_name,
            expires_at: record.expires_at,
            token_hash: hash,
        })
    }

    /// Revoke a session and evict it from Valkey immediately.
    pub async fn revoke_session(
        &self,
        session_id: Uuid,
        session_token_hash: &str,
    ) -> Result<(), AuthError> {
        self.sessions.revoke(session_id, unix_now()).await?;
        // PostgreSQL is authoritative; a stale legacy cache entry is never read.
        let _ = self.cache.delete(&session_cache_key(session_token_hash)).await;
        Ok(())
    }

    async fn load_api_key(&self, prefix: &str) -> Result<Option<ApiKeyRecord>, AuthError> {
        let cache_key = api_key_cache_key(prefix);
        if let Some(raw) = self.cache.get(&cache_key).await? {
            if let Ok(record) = serde_json::from_str::<ApiKeyRecord>(&raw) {
                return Ok(Some(record));
            }
            let _ = self.cache.delete(&cache_key).await;
        }

        let record = self.api_keys.find_by_prefix(prefix).await?;
        if let Some(record) = &record {
            self.cache_record(&cache_key, record, self.key_cache_ttl)
                .await;
            let _ = self.api_keys.touch_last_used(record.id, unix_now()).await;
        }
        Ok(record)
    }

    async fn cache_record<T: serde::Serialize>(&self, key: &str, value: &T, ttl: Duration) {
        if let Ok(serialized) = serde_json::to_string(value) {
            // Cache outages must not turn into authentication outages.
            let _ = self.cache.set(key, serialized, ttl).await;
        }
    }

}

fn session_view(record: &SessionRecord) -> SessionView {
    SessionView {
        id: record.id,
        workspace_id: record.workspace_id,
        device_name: record.device_name.clone(),
        expires_at: record.expires_at,
        created_at: record.created_at,
        last_seen_at: record.last_seen_at,
    }
}

/// Build a service wired to in-memory stores, used by tests and local development.
#[must_use]
pub fn in_memory_service() -> (
    AuthService,
    Arc<super::repository::InMemoryApiKeyRepository>,
    Arc<super::repository::InMemorySessionRepository>,
    Arc<super::cache::InMemoryAuthCache>,
) {
    let api_keys = Arc::new(super::repository::InMemoryApiKeyRepository::default());
    let sessions = Arc::new(super::repository::InMemorySessionRepository::new());
    let cache = Arc::new(super::cache::InMemoryAuthCache::new());
    let service = AuthService::new(api_keys.clone(), sessions.clone(), cache.clone());
    (service, api_keys, sessions, cache)
}

/// Create and hash a key for seeding tests; returns `(plaintext, record)`.
#[must_use]
pub fn seed_api_key(
    workspace_id: Uuid,
    user_id: Uuid,
    scopes: Vec<Scope>,
    expires_at: Option<i64>,
) -> (String, ApiKeyRecord) {
    let generated = generate_api_key();
    let record = ApiKeyRecord {
        id: Uuid::now_v7(),
        workspace_id,
        user_id,
        key_prefix: generated.key_prefix().to_owned(),
        secret_hash: generated.secret_hash().to_owned(),
        scopes,
        revoked: false,
        expires_at,
        last_used_at: None,
    };
    (generated.plaintext().to_owned(), record)
}

#[cfg(test)]
mod tests {
    use super::super::cache::AuthCache;
    use super::super::repository::SessionRepository;
    use super::*;
    use crate::auth::repository::InMemoryApiKeyRepository;

    async fn fixture() -> (
        AuthService,
        Arc<InMemoryApiKeyRepository>,
        Arc<super::super::repository::InMemorySessionRepository>,
        String,
        Uuid,
        Uuid,
    ) {
        let workspace_id = Uuid::now_v7();
        let user_id = Uuid::now_v7();
        let (plaintext, record) =
            seed_api_key(workspace_id, user_id, vec![Scope::Read, Scope::Write], None);
        let api_keys = Arc::new(InMemoryApiKeyRepository::new(vec![record]).await);
        let sessions = Arc::new(super::super::repository::InMemorySessionRepository::new());
        let cache = Arc::new(super::super::cache::InMemoryAuthCache::new());
        let service = AuthService::new(api_keys.clone(), sessions.clone(), cache);
        (
            service,
            api_keys,
            sessions,
            plaintext,
            workspace_id,
            user_id,
        )
    }

    #[tokio::test]
    async fn api_key_is_verified_and_cached_after_first_lookup() {
        let (service, api_keys, _sessions, plaintext, workspace_id, user_id) = fixture().await;

        let first = service.authenticate_api_key(&plaintext).await.unwrap();
        assert_eq!(first.workspace_id, workspace_id);
        assert_eq!(first.user_id, user_id);
        assert!(first.has_scope(Scope::Write));

        let second = service.authenticate_api_key(&plaintext).await.unwrap();
        assert_eq!(second.workspace_id, workspace_id);
        assert_eq!(
            api_keys.lookups(),
            1,
            "second call must be served from cache"
        );
    }

    #[tokio::test]
    async fn wrong_secret_is_rejected() {
        let (service, _api_keys, _sessions, plaintext, _workspace, _user) = fixture().await;
        let mut tampered = plaintext.clone();
        tampered.push('a');
        assert_eq!(
            service.authenticate_api_key(&tampered).await.unwrap_err(),
            AuthError::InvalidCredentials
        );
    }

    #[tokio::test]
    async fn revoked_and_expired_keys_are_rejected() {
        let workspace_id = Uuid::now_v7();
        let user_id = Uuid::now_v7();
        let (plaintext, mut record) = seed_api_key(workspace_id, user_id, vec![Scope::Read], None);
        record.revoked = true;
        let api_keys = Arc::new(InMemoryApiKeyRepository::new(vec![record.clone()]).await);
        let cache = Arc::new(super::super::cache::InMemoryAuthCache::new());
        let service = AuthService::new(
            api_keys.clone(),
            Arc::new(super::super::repository::InMemorySessionRepository::new()),
            cache.clone(),
        );
        assert_eq!(
            service.authenticate_api_key(&plaintext).await.unwrap_err(),
            AuthError::Revoked
        );

        let (plaintext_expired, expired) = seed_api_key(
            workspace_id,
            user_id,
            vec![Scope::Read],
            Some(unix_now() - 1),
        );
        let service = AuthService::new(
            Arc::new(InMemoryApiKeyRepository::new(vec![expired]).await),
            Arc::new(super::super::repository::InMemorySessionRepository::new()),
            cache,
        );
        assert_eq!(
            service
                .authenticate_api_key(&plaintext_expired)
                .await
                .unwrap_err(),
            AuthError::Expired
        );
    }

    #[tokio::test]
    async fn session_lifecycle_uses_cache_and_revocation_is_immediate() {
        let (service, _api_keys, sessions, plaintext, _workspace, _user) = fixture().await;
        let principal = service.authenticate_api_key(&plaintext).await.unwrap();

        let issued = service
            .create_session(&principal, "تطبيق سطح المكتب")
            .await
            .unwrap();
        assert!(issued.token.starts_with("sfs_"));

        let authenticated = service.authenticate_session(&issued.token).await.unwrap();
        assert_eq!(authenticated.session_id, issued.session.id);
        assert_eq!(authenticated.principal.workspace_id, principal.workspace_id);

        service
            .revoke_session(issued.session.id, &authenticated.token_hash)
            .await
            .unwrap();

        assert_eq!(
            service
                .authenticate_session(&issued.token)
                .await
                .unwrap_err(),
            AuthError::Revoked
        );
        // The repository still holds the row; revocation is enforced on read.
        assert!(sessions
            .find_by_hash(&authenticated.token_hash)
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn expired_session_is_rejected() {
        let (service, _api_keys, _sessions, plaintext, _workspace, _user) = fixture().await;
        let principal = service.authenticate_api_key(&plaintext).await.unwrap();
        let issued = service.create_session(&principal, "جهاز").await.unwrap();

        let hash = token_hash(&issued.token);
        let expired = SessionRecord {
            id: issued.session.id,
            workspace_id: principal.workspace_id,
            user_id: principal.user_id,
            api_key_id: None,
            token_hash: hash.clone(),
            device_name: "جهاز".to_owned(),
            scopes: principal.scopes.clone(),
            created_at: unix_now() - 100,
            last_seen_at: unix_now() - 100,
            expires_at: unix_now() - 1,
            revoked: false,
        };
        let cache = Arc::new(super::super::cache::InMemoryAuthCache::new());
        cache
            .set(
                &session_cache_key(&hash),
                serde_json::to_string(&expired).unwrap(),
                Duration::from_secs(60),
            )
            .await
            .unwrap();
        let service = AuthService::new(
            Arc::new(InMemoryApiKeyRepository::new(vec![]).await),
            Arc::new(super::super::repository::InMemorySessionRepository::new()),
            cache,
        );
        assert_eq!(
            service
                .authenticate_session(&issued.token)
                .await
                .unwrap_err(),
            AuthError::Expired
        );
    }
}
