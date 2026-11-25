//! Authoritative PostgreSQL 18 SQLx Repository for Zero-Knowledge Encrypted Sync Records
//! and Encrypted Share Links (Migrations `0009` + `0017`).

use base64::Engine;
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::encrypted_sync::{
    EncryptedSyncRecord, ShareLinkRecord, SyncChangesResponse, ValidatedPushEnvelope,
};
use crate::error::AuthError;
use crate::pg_store::PgControlPlaneStore;

const SYNC_SELECT_COLUMNS: &str = r"
    id, workspace_id, user_id, resource_kind, resource_id,
    version, workspace_seq, ciphertext, nonce, key_id, key_epoch,
    content_hash, deleted, origin_device_id,
    EXTRACT(EPOCH FROM updated_at)::bigint AS updated_at_secs,
    last_idempotency_key, last_request_fingerprint
";

async fn set_sync_rls_context(
    tx: &mut Transaction<'_, Postgres>,
    workspace_id: Option<Uuid>,
    share_token_hash: Option<&str>,
) -> Result<(), AuthError> {
    if let Some(ws) = workspace_id {
        let ws_s = ws.to_string();
        sqlx::query(
            "SELECT set_config('app.current_workspace_id', $1, true), set_config('app.workspace_id', $1, true)",
        )
        .bind(&ws_s)
        .execute(&mut **tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    }
    if let Some(th) = share_token_hash {
        sqlx::query("SELECT set_config('app.share_link_token_hash', $1, true)")
            .bind(th)
            .execute(&mut **tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
    }
    Ok(())
}

fn row_to_sync_record(row: &sqlx::postgres::PgRow) -> Result<EncryptedSyncRecord, AuthError> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let ct: Vec<u8> = row
        .try_get("ciphertext")
        .map_err(|_| AuthError::BackendUnavailable)?;
    let nonce: Vec<u8> = row
        .try_get("nonce")
        .map_err(|_| AuthError::BackendUnavailable)?;
    let version_i64: i64 = row
        .try_get("version")
        .map_err(|_| AuthError::BackendUnavailable)?;
    let seq_i64: i64 = row.try_get("workspace_seq").unwrap_or(1);
    let epoch_i32: i32 = row.try_get("key_epoch").unwrap_or(1);
    let updated_secs: i64 = row
        .try_get("updated_at_secs")
        .map_err(|_| AuthError::BackendUnavailable)?;

    Ok(EncryptedSyncRecord {
        id: row
            .try_get("id")
            .map_err(|_| AuthError::BackendUnavailable)?,
        workspace_id: row
            .try_get("workspace_id")
            .map_err(|_| AuthError::BackendUnavailable)?,
        user_id: row
            .try_get("user_id")
            .map_err(|_| AuthError::BackendUnavailable)?,
        resource_kind: row
            .try_get("resource_kind")
            .map_err(|_| AuthError::BackendUnavailable)?,
        resource_id: row
            .try_get("resource_id")
            .map_err(|_| AuthError::BackendUnavailable)?,
        version: u64::try_from(version_i64).unwrap_or(1),
        workspace_seq: u64::try_from(seq_i64).unwrap_or(1),
        ciphertext_b64: b64.encode(&ct),
        nonce_b64: b64.encode(&nonce),
        key_id: row
            .try_get("key_id")
            .map_err(|_| AuthError::BackendUnavailable)?,
        key_epoch: u32::try_from(epoch_i32).unwrap_or(1),
        content_hash: row
            .try_get("content_hash")
            .map_err(|_| AuthError::BackendUnavailable)?,
        deleted: row.try_get("deleted").unwrap_or(false),
        origin_device_id: row
            .try_get("origin_device_id")
            .unwrap_or_else(|_| "desktop-client".to_owned()),
        updated_at: updated_secs,
        last_idempotency_key: row.try_get("last_idempotency_key").ok().flatten(),
        last_request_fingerprint: row.try_get("last_request_fingerprint").ok().flatten(),
    })
}

pub async fn push_envelope_pg(
    pg: &PgControlPlaneStore,
    workspace_id: Uuid,
    user_id: Uuid,
    validated: &ValidatedPushEnvelope,
    min_key_epoch: u32,
    now_secs: i64,
) -> Result<EncryptedSyncRecord, AuthError> {
    if validated.key_epoch < min_key_epoch {
        return Err(AuthError::SyncVersionConflict);
    }

    let mut tx = pg
        .pool()
        .begin()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    set_sync_rls_context(&mut tx, Some(workspace_id), None).await?;

    let ws_exists: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM workspaces WHERE id = $1 FOR UPDATE")
            .bind(workspace_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
    if ws_exists.is_none() {
        return Err(AuthError::InvalidRequest);
    }

    let live_org_epoch: Option<i32> = sqlx::query_scalar(
        "SELECT active_key_epoch FROM organizations WHERE workspace_id = $1 AND deleted_at IS NULL",
    )
    .bind(workspace_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;
    if let Some(ep) = live_org_epoch {
        if validated.key_epoch < u32::try_from(ep).unwrap_or(1) {
            return Err(AuthError::SyncVersionConflict);
        }
    }

    let existing_sql = format!(
        r"
        SELECT {SYNC_SELECT_COLUMNS}
        FROM encrypted_sync_records
        WHERE workspace_id = $1 AND resource_kind = $2 AND resource_id = $3
        FOR UPDATE
        "
    );
    let existing_row = sqlx::query(&existing_sql)
        .bind(workspace_id)
        .bind(&validated.resource_kind)
        .bind(&validated.resource_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

    if let Some(ref row) = existing_row {
        let existing = row_to_sync_record(row)?;
        if let Some(ref idem) = validated.idempotency_key {
            if existing.last_idempotency_key.as_deref() == Some(idem.as_str()) {
                if existing.last_request_fingerprint.as_deref()
                    == Some(validated.request_fingerprint.as_str())
                {
                    tx.commit()
                        .await
                        .map_err(|_| AuthError::BackendUnavailable)?;
                    return Ok(existing);
                }
                return Err(AuthError::IdempotencyConflict);
            }
        }
        if validated.expected_version != existing.version {
            return Err(AuthError::SyncVersionConflict);
        }
        let existing_nonce: Vec<u8> = row
            .try_get("nonce")
            .map_err(|_| AuthError::BackendUnavailable)?;
        if existing.key_epoch == validated.key_epoch && existing_nonce == validated.nonce {
            return Err(AuthError::SyncEnvelopeInvalid);
        }
    } else if validated.expected_version != 0 {
        return Err(AuthError::SyncVersionConflict);
    }

    let next_seq_i64: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(workspace_seq), 0) + 1 FROM encrypted_sync_records WHERE workspace_id = $1",
    )
    .bind(workspace_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    let next_version_i64 =
        i64::try_from(validated.expected_version + 1).map_err(|_| AuthError::InvalidRequest)?;
    let key_epoch_i32 =
        i32::try_from(validated.key_epoch).map_err(|_| AuthError::InvalidRequest)?;

    let upsert_sql = format!(
        r"
        INSERT INTO encrypted_sync_records (
            id, workspace_id, user_id, resource_kind, resource_id,
            version, workspace_seq, ciphertext, nonce, key_id, key_epoch,
            content_hash, deleted, origin_device_id, updated_at,
            last_idempotency_key, last_request_fingerprint
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
            to_timestamp($15::double precision), $16, $17
        )
        ON CONFLICT (workspace_id, resource_kind, resource_id) DO UPDATE
        SET user_id = EXCLUDED.user_id,
            version = EXCLUDED.version,
            workspace_seq = EXCLUDED.workspace_seq,
            ciphertext = EXCLUDED.ciphertext,
            nonce = EXCLUDED.nonce,
            key_id = EXCLUDED.key_id,
            key_epoch = EXCLUDED.key_epoch,
            content_hash = EXCLUDED.content_hash,
            deleted = EXCLUDED.deleted,
            origin_device_id = EXCLUDED.origin_device_id,
            updated_at = EXCLUDED.updated_at,
            last_idempotency_key = EXCLUDED.last_idempotency_key,
            last_request_fingerprint = EXCLUDED.last_request_fingerprint
        RETURNING {SYNC_SELECT_COLUMNS}
        "
    );

    let saved_row = sqlx::query(&upsert_sql)
        .bind(Uuid::now_v7())
        .bind(workspace_id)
        .bind(user_id)
        .bind(&validated.resource_kind)
        .bind(&validated.resource_id)
        .bind(next_version_i64)
        .bind(next_seq_i64)
        .bind(&validated.ciphertext)
        .bind(&validated.nonce)
        .bind(&validated.key_id)
        .bind(key_epoch_i32)
        .bind(&validated.content_hash)
        .bind(validated.deleted)
        .bind(&validated.origin_device_id)
        .bind(now_secs as f64)
        .bind(&validated.idempotency_key)
        .bind(&validated.request_fingerprint)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

    tx.commit()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    row_to_sync_record(&saved_row)
}

pub async fn pull_envelope_pg(
    pg: &PgControlPlaneStore,
    workspace_id: Uuid,
    resource_kind: &str,
    resource_id: &str,
) -> Result<Option<EncryptedSyncRecord>, AuthError> {
    let mut tx = pg
        .pool()
        .begin()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    set_sync_rls_context(&mut tx, Some(workspace_id), None).await?;

    let sql = format!(
        r"
        SELECT {SYNC_SELECT_COLUMNS}
        FROM encrypted_sync_records
        WHERE workspace_id = $1 AND resource_kind = $2 AND resource_id = $3
        "
    );
    let row_opt = sqlx::query(&sql)
        .bind(workspace_id)
        .bind(resource_kind)
        .bind(resource_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

    tx.commit()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    row_opt.map(|r| row_to_sync_record(&r)).transpose()
}

pub async fn list_changes_since_pg(
    pg: &PgControlPlaneStore,
    workspace_id: Uuid,
    since_seq: u64,
    limit: usize,
) -> Result<SyncChangesResponse, AuthError> {
    let since_i64 = i64::try_from(since_seq).unwrap_or(i64::MAX);
    let fetch_limit = i64::try_from(limit + 1).unwrap_or(501);

    let mut tx = pg
        .pool()
        .begin()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    set_sync_rls_context(&mut tx, Some(workspace_id), None).await?;

    let latest_i64: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(workspace_seq), 0) FROM encrypted_sync_records WHERE workspace_id = $1",
    )
    .bind(workspace_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    let sql = format!(
        r"
        SELECT {SYNC_SELECT_COLUMNS}
        FROM encrypted_sync_records
        WHERE workspace_id = $1 AND workspace_seq > $2
        ORDER BY workspace_seq ASC
        LIMIT $3
        "
    );
    let rows = sqlx::query(&sql)
        .bind(workspace_id)
        .bind(since_i64)
        .bind(fetch_limit)
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

    tx.commit()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

    let has_more = rows.len() > limit;
    let mut records = Vec::with_capacity(rows.len().min(limit));
    for r in rows.into_iter().take(limit) {
        records.push(row_to_sync_record(&r)?);
    }

    Ok(SyncChangesResponse {
        workspace_id,
        latest_workspace_seq: u64::try_from(latest_i64).unwrap_or(0),
        has_more,
        records,
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn create_share_link_pg(
    pg: &PgControlPlaneStore,
    workspace_id: Uuid,
    created_by: Uuid,
    resource_id: &str,
    token_hash: &str,
    wrapped_key_ciphertext: &[u8],
    wrapped_key_nonce: &[u8],
    now_secs: i64,
    expires_at_secs: i64,
) -> Result<Uuid, AuthError> {
    let link_id = Uuid::now_v7();
    let mut tx = pg
        .pool()
        .begin()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    set_sync_rls_context(&mut tx, Some(workspace_id), Some(token_hash)).await?;

    sqlx::query(
        r"
        INSERT INTO encrypted_share_links (
            id, workspace_id, created_by, resource_id, token_hash,
            wrapped_key_ciphertext, wrapped_key_nonce, expires_at, created_at
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7,
            to_timestamp($8::double precision),
            to_timestamp($9::double precision)
        )
        ",
    )
    .bind(link_id)
    .bind(workspace_id)
    .bind(created_by)
    .bind(resource_id)
    .bind(token_hash)
    .bind(wrapped_key_ciphertext)
    .bind(wrapped_key_nonce)
    .bind(expires_at_secs as f64)
    .bind(now_secs as f64)
    .execute(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    tx.commit()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    Ok(link_id)
}

pub async fn resolve_share_link_pg(
    pg: &PgControlPlaneStore,
    token_hash: &str,
    now_secs: i64,
) -> Result<ShareLinkRecord, AuthError> {
    let mut tx = pg
        .pool()
        .begin()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    set_sync_rls_context(&mut tx, None, Some(token_hash)).await?;

    let row = sqlx::query(
        r"
        SELECT id, workspace_id, created_by, resource_id, token_hash,
               wrapped_key_ciphertext, wrapped_key_nonce,
               EXTRACT(EPOCH FROM expires_at)::bigint AS expires_at_secs,
               EXTRACT(EPOCH FROM revoked_at)::bigint AS revoked_at_secs,
               EXTRACT(EPOCH FROM created_at)::bigint AS created_at_secs
        FROM encrypted_share_links
        WHERE token_hash = $1
        ",
    )
    .bind(token_hash)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?
    .ok_or(AuthError::ShareLinkInvalid)?;

    tx.commit()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

    let b64 = base64::engine::general_purpose::STANDARD;
    let wrapped: Vec<u8> = row
        .try_get("wrapped_key_ciphertext")
        .map_err(|_| AuthError::BackendUnavailable)?;
    let nonce: Vec<u8> = row
        .try_get("wrapped_key_nonce")
        .map_err(|_| AuthError::BackendUnavailable)?;
    let expires_at: i64 = row
        .try_get("expires_at_secs")
        .map_err(|_| AuthError::BackendUnavailable)?;
    let revoked_at: Option<i64> = row.try_get("revoked_at_secs").ok().flatten();
    let created_at: i64 = row
        .try_get("created_at_secs")
        .map_err(|_| AuthError::BackendUnavailable)?;

    if revoked_at.is_some() || now_secs >= expires_at {
        return Err(AuthError::ShareLinkInvalid);
    }

    Ok(ShareLinkRecord {
        id: row
            .try_get("id")
            .map_err(|_| AuthError::BackendUnavailable)?,
        workspace_id: row
            .try_get("workspace_id")
            .map_err(|_| AuthError::BackendUnavailable)?,
        created_by: row
            .try_get("created_by")
            .map_err(|_| AuthError::BackendUnavailable)?,
        resource_id: row
            .try_get("resource_id")
            .map_err(|_| AuthError::BackendUnavailable)?,
        token_hash: row
            .try_get("token_hash")
            .map_err(|_| AuthError::BackendUnavailable)?,
        wrapped_key_ciphertext_b64: b64.encode(&wrapped),
        wrapped_key_nonce_b64: b64.encode(&nonce),
        expires_at,
        revoked_at,
        created_at,
    })
}

pub async fn revoke_share_link_pg(
    pg: &PgControlPlaneStore,
    workspace_id: Uuid,
    link_id: Uuid,
    now_secs: i64,
) -> Result<(), AuthError> {
    let mut tx = pg
        .pool()
        .begin()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    set_sync_rls_context(&mut tx, Some(workspace_id), None).await?;

    let updated = sqlx::query(
        r"
        UPDATE encrypted_share_links
        SET revoked_at = to_timestamp($3::double precision)
        WHERE id = $1 AND workspace_id = $2
        ",
    )
    .bind(link_id)
    .bind(workspace_id)
    .bind(now_secs as f64)
    .execute(&mut *tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    if updated.rows_affected() == 0 {
        return Err(AuthError::ShareLinkInvalid);
    }

    tx.commit()
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
    Ok(())
}
