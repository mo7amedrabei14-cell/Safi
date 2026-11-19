//! Append-only, BLAKE3 hash-chained Audit Log with real UTC ISO-8601 timestamps
//! and recursive secret/PII redaction (AGENTS.md Rule 4, Phase 05).
//!
//! Zero seeded fake entries. Every privileged action, denied attempt, billing event,
//! and configuration change is recorded with tamper-evident chaining.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use tokio::sync::RwLock;
use uuid::Uuid;

const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// Outcome classification of an audited action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditOutcome {
    Allowed,
    Denied,
    Error,
}

/// Immutable, hash-chained audit record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub id: Uuid,
    pub seq: u64,
    pub timestamp: String,
    pub actor_user_id: Option<Uuid>,
    pub action: String,
    pub target_type: String,
    pub target_id: String,
    pub outcome: AuditOutcome,
    pub before_state: Option<Value>,
    pub after_state: Option<Value>,
    pub previous_hash: String,
    pub current_hash: String,
}

/// Thread-safe, append-only hash-chained Audit Log store.
#[derive(Debug, Clone)]
pub struct AuditLogCenter {
    entries: Arc<RwLock<Vec<AuditLogEntry>>>,
}

impl Default for AuditLogCenter {
    fn default() -> Self {
        Self::new()
    }
}

impl AuditLogCenter {
    /// Create a clean, empty audit log center (NEVER seeds fake entries).
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Append a tamper-evident, secret-redacted audit entry with real UTC timestamp.
    #[allow(clippy::too_many_arguments)]
    pub async fn record(
        &self,
        actor_user_id: Option<Uuid>,
        action: impl Into<String>,
        target_type: impl Into<String>,
        target_id: impl Into<String>,
        outcome: AuditOutcome,
        before_state: Option<Value>,
        after_state: Option<Value>,
    ) -> AuditLogEntry {
        let mut guard = self.entries.write().await;
        let seq = (guard.len() as u64) + 1;
        let previous_hash = guard
            .last()
            .map_or_else(|| GENESIS_HASH.to_owned(), |last| last.current_hash.clone());
        let id = Uuid::now_v7();
        let timestamp = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned());

        let action_s = redact_text(&action.into());
        let target_type_s = redact_text(&target_type.into());
        let target_id_s = redact_text(&target_id.into());
        let before_clean = before_state.map(redact_json);
        let after_clean = after_state.map(redact_json);

        let current_hash = compute_entry_hash(
            seq,
            &id,
            &timestamp,
            actor_user_id,
            &action_s,
            &target_type_s,
            &target_id_s,
            outcome,
            before_clean.as_ref(),
            after_clean.as_ref(),
            &previous_hash,
        );

        let entry = AuditLogEntry {
            id,
            seq,
            timestamp,
            actor_user_id,
            action: action_s,
            target_type: target_type_s,
            target_id: target_id_s,
            outcome,
            before_state: before_clean,
            after_state: after_clean,
            previous_hash,
            current_hash,
        };
        guard.push(entry.clone());
        entry
    }

    /// Return the most recent audit entries (newest first), up to `limit`.
    pub async fn list_recent(&self, limit: usize) -> Vec<AuditLogEntry> {
        let guard = self.entries.read().await;
        let cap = limit.clamp(1, 500);
        guard.iter().rev().take(cap).cloned().collect()
    }

    /// Cryptographically verify the entire BLAKE3 hash chain from genesis to head.
    pub async fn verify_chain(&self) -> bool {
        let guard = self.entries.read().await;
        let mut expected_prev = GENESIS_HASH.to_owned();
        for entry in guard.iter() {
            if entry.previous_hash != expected_prev {
                return false;
            }
            let recomputed = compute_entry_hash(
                entry.seq,
                &entry.id,
                &entry.timestamp,
                entry.actor_user_id,
                &entry.action,
                &entry.target_type,
                &entry.target_id,
                entry.outcome,
                entry.before_state.as_ref(),
                entry.after_state.as_ref(),
                &entry.previous_hash,
            );
            if recomputed != entry.current_hash {
                return false;
            }
            expected_prev = entry.current_hash.clone();
        }
        true
    }
}

#[allow(clippy::too_many_arguments)]
fn compute_entry_hash(
    seq: u64,
    id: &Uuid,
    timestamp: &str,
    actor_user_id: Option<Uuid>,
    action: &str,
    target_type: &str,
    target_id: &str,
    outcome: AuditOutcome,
    before_state: Option<&Value>,
    after_state: Option<&Value>,
    previous_hash: &str,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"safi:audit-chain:v1|");
    hasher.update(&seq.to_le_bytes());
    hasher.update(id.as_bytes());
    hasher.update(timestamp.as_bytes());
    if let Some(actor) = actor_user_id {
        hasher.update(actor.as_bytes());
    }
    hasher.update(action.as_bytes());
    hasher.update(target_type.as_bytes());
    hasher.update(target_id.as_bytes());
    let outcome_byte = match outcome {
        AuditOutcome::Allowed => b'A',
        AuditOutcome::Denied => b'D',
        AuditOutcome::Error => b'E',
    };
    hasher.update(&[outcome_byte]);
    if let Some(v) = before_state {
        if let Ok(s) = serde_json::to_string(v) {
            hasher.update(s.as_bytes());
        }
    }
    if let Some(v) = after_state {
        if let Ok(s) = serde_json::to_string(v) {
            hasher.update(s.as_bytes());
        }
    }
    hasher.update(previous_hash.as_bytes());
    hasher.finalize().to_hex().to_string()
}

fn is_sensitive_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    k.contains("password")
        || k.contains("secret")
        || k.contains("token")
        || k.contains("totp")
        || k.contains("otp")
        || k.contains("private_key")
        || k.contains("api_key")
        || k.contains("authorization")
        || k.contains("recovery_code")
}

fn redact_text(raw: &str) -> String {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("sfs_") || lower.contains("sfk_live_") || lower.contains("bearer ") {
        "[REDACTED_CREDENTIAL]".to_owned()
    } else {
        raw.to_owned()
    }
}

fn redact_json(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let cleaned = map
                .into_iter()
                .map(|(k, v)| {
                    if is_sensitive_key(&k) {
                        (k, Value::String("[REDACTED]".to_owned()))
                    } else {
                        (k, redact_json(v))
                    }
                })
                .collect();
            Value::Object(cleaned)
        }
        Value::Array(items) => Value::Array(items.into_iter().map(redact_json).collect()),
        Value::String(s) => Value::String(redact_text(&s)),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn audit_log_starts_empty_chains_hashes_and_redacts_secrets() {
        let log = AuditLogCenter::new();
        assert!(log.list_recent(50).await.is_empty());

        let actor = Uuid::now_v7();
        let e1 = log
            .record(
                Some(actor),
                "owner.friend.grant",
                "user",
                "target-1",
                AuditOutcome::Allowed,
                Some(json!({"plan": "free", "password": "plain-password"})),
                Some(json!({"plan": "friend_vip", "totp_secret": "GEZDGNBV"})),
            )
            .await;

        assert_eq!(e1.seq, 1);
        assert_eq!(e1.previous_hash, GENESIS_HASH);
        assert_eq!(
            e1.before_state.as_ref().and_then(|v| v.get("password")),
            Some(&Value::String("[REDACTED]".to_owned()))
        );
        assert_eq!(
            e1.after_state.as_ref().and_then(|v| v.get("totp_secret")),
            Some(&Value::String("[REDACTED]".to_owned()))
        );

        let e2 = log
            .record(
                Some(actor),
                "owner.maintenance.toggle",
                "system",
                "maintenance_mode",
                AuditOutcome::Allowed,
                Some(json!({"enabled": false})),
                Some(json!({"enabled": true})),
            )
            .await;

        assert_eq!(e2.seq, 2);
        assert_eq!(e2.previous_hash, e1.current_hash);
        assert!(log.verify_chain().await);
    }
}
