//! Cache-aside layer for API keys and sessions (Valkey 8).
//!
//! The hot path of every authenticated request is a single cache lookup. On a
//! miss the service reads PostgreSQL and refills the cache, so Valkey outages
//! degrade latency but never correctness.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use redis::aio::MultiplexedConnection;
use tokio::sync::RwLock;

use crate::error::AuthError;

/// Prefix of every authentication cache key.
pub const CACHE_KEY_PREFIX: &str = "safi:auth:";

/// Cache operations required by the auth service.
#[async_trait]
pub trait AuthCache: Send + Sync {
    /// Read a cached value.
    async fn get(&self, key: &str) -> Result<Option<String>, AuthError>;
    /// Store a value with a sliding TTL.
    async fn set(&self, key: &str, value: String, ttl: Duration) -> Result<(), AuthError>;
    /// Evict a value immediately (used on revocation).
    async fn delete(&self, key: &str) -> Result<(), AuthError>;
}

struct CacheEntry {
    value: String,
    expires_at: Instant,
}

/// Process-local cache used in tests and single-node development.
#[derive(Default)]
pub struct InMemoryAuthCache {
    entries: RwLock<HashMap<String, CacheEntry>>,
    hits: AtomicUsize,
    misses: AtomicUsize,
}

impl InMemoryAuthCache {
    /// Create an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of successful lookups (used to prove the hot path is cached).
    #[must_use]
    pub fn hits(&self) -> usize {
        self.hits.load(Ordering::Relaxed)
    }

    /// Number of lookups that had to fall through.
    #[must_use]
    pub fn misses(&self) -> usize {
        self.misses.load(Ordering::Relaxed)
    }
}

#[async_trait]
impl AuthCache for InMemoryAuthCache {
    async fn get(&self, key: &str) -> Result<Option<String>, AuthError> {
        let now = Instant::now();
        {
            let entries = self.entries.read().await;
            if let Some(entry) = entries.get(key) {
                if entry.expires_at > now {
                    self.hits.fetch_add(1, Ordering::Relaxed);
                    return Ok(Some(entry.value.clone()));
                }
            }
        }
        let mut entries = self.entries.write().await;
        let expired = entries
            .get(key)
            .is_some_and(|entry| entry.expires_at <= now);
        if expired {
            entries.remove(key);
        }
        self.misses.fetch_add(1, Ordering::Relaxed);
        Ok(None)
    }

    async fn set(&self, key: &str, value: String, ttl: Duration) -> Result<(), AuthError> {
        let mut entries = self.entries.write().await;
        entries.insert(
            key.to_owned(),
            CacheEntry {
                value,
                expires_at: Instant::now() + ttl,
            },
        );
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), AuthError> {
        let mut entries = self.entries.write().await;
        entries.remove(key);
        Ok(())
    }
}

/// Valkey 8 cache over RESP (the `redis` crate is protocol-compatible with Valkey).
pub struct ValkeyAuthCache {
    connection: MultiplexedConnection,
}

impl ValkeyAuthCache {
    /// Connect to a Valkey URL such as `redis://127.0.0.1:6379/0`.
    pub async fn connect(url: &str) -> Result<Self, AuthError> {
        let client = redis::Client::open(url).map_err(|_| AuthError::BackendUnavailable)?;
        let connection = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        Ok(Self { connection })
    }

    /// Wrap an existing multiplexed connection (shared with other server modules).
    #[must_use]
    pub fn from_connection(connection: MultiplexedConnection) -> Self {
        Self { connection }
    }
}

#[async_trait]
impl AuthCache for ValkeyAuthCache {
    async fn get(&self, key: &str) -> Result<Option<String>, AuthError> {
        let mut connection = self.connection.clone();
        redis::cmd("GET")
            .arg(key)
            .query_async::<Option<String>>(&mut connection)
            .await
            .map_err(|_| AuthError::BackendUnavailable)
    }

    async fn set(&self, key: &str, value: String, ttl: Duration) -> Result<(), AuthError> {
        let mut connection = self.connection.clone();
        redis::cmd("SET")
            .arg(key)
            .arg(value)
            .arg("EX")
            .arg(ttl.as_secs().max(1))
            .query_async::<()>(&mut connection)
            .await
            .map_err(|_| AuthError::BackendUnavailable)
    }

    async fn delete(&self, key: &str) -> Result<(), AuthError> {
        let mut connection = self.connection.clone();
        redis::cmd("DEL")
            .arg(key)
            .query_async::<()>(&mut connection)
            .await
            .map_err(|_| AuthError::BackendUnavailable)
    }
}

/// Name of the cached API-key record.
#[must_use]
pub fn api_key_cache_key(prefix: &str) -> String {
    format!("{CACHE_KEY_PREFIX}apikey:{prefix}")
}

/// Name of the cached session record.
#[must_use]
pub fn session_cache_key(token_hash: &str) -> String {
    format!("{CACHE_KEY_PREFIX}session:{token_hash}")
}

/// Shared handle used by the auth service.
pub type SharedCache = Arc<dyn AuthCache>;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn in_memory_cache_expires_and_counts_hits() {
        let cache = InMemoryAuthCache::new();
        cache
            .set("k", "v".to_owned(), Duration::from_millis(20))
            .await
            .unwrap();
        assert_eq!(cache.get("k").await.unwrap().as_deref(), Some("v"));
        assert_eq!(cache.hits(), 1);

        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(cache.get("k").await.unwrap().is_none());
        assert_eq!(cache.misses(), 1);
    }

    #[tokio::test]
    async fn delete_evicts_immediately() {
        let cache = InMemoryAuthCache::new();
        cache
            .set("k", "v".to_owned(), Duration::from_secs(60))
            .await
            .unwrap();
        cache.delete("k").await.unwrap();
        assert!(cache.get("k").await.unwrap().is_none());
    }

    #[test]
    fn cache_keys_are_namespaced() {
        assert_eq!(api_key_cache_key("abc"), "safi:auth:apikey:abc");
        assert_eq!(session_cache_key("def"), "safi:auth:session:def");
    }
}
