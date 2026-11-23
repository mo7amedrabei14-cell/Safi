//! Authoritative PostgreSQL 18 Durable Entitlement, Monthly Usage, Fingerprinted Idempotency,
//! and Trial Anti-Abuse Repository (Migration 0015).
//!
//! Guarantees enforced at the database layer across multiple independent server instances & restarts:
//! 1. Atomic single-transaction usage consumption (`workspace_entitlement_state` + `workspace_monthly_usage`
//!    + `entitlement_usage_idempotency` + `credit_balances`) with per-workspace serialization.
//! 2. 32-byte SHA-256 request fingerprint bound to `(workspace_id, idempotency_key)` so reusing a key
//!    across different usage kinds (`DataOperation` vs `AiCall`) or different row/column/PII attributes
//!    ALWAYS fails closed with `409 Conflict` (`SF-PAY-007`).
//! 3. Durable monthly counters (`YYYY-MM`) that never reset on server restart or across instances.
//! 4. Durable trial anti-abuse claims (`email`, `phone`, `device`) surviving restarts.
//! 5. Zero swallowed database errors: if PostgreSQL is unavailable, protected operations fail closed.

use sqlx::Row;
use uuid::Uuid;

use crate::business_rules::rules;
use crate::entitlements::{
    compute_effective, period_ym_at, reconcile_expirations, ConsumeUsageRequest,
    ConsumeUsageResult, EffectiveEntitlements, EntitlementAggregateStats, MonthlyUsage, PlanTier,
    SubscriptionState, UsageKind, WorkspaceEntitlementRecord,
};
use crate::error::AuthError;
use crate::pg_store::PgControlPlaneStore;

fn plan_to_str(p: PlanTier) -> &'static str {
    match p {
        PlanTier::Free => "free",
        PlanTier::Trial => "trial",
        PlanTier::IndividualMonthly => "individual_monthly",
        PlanTier::IndividualAnnual => "individual_annual",
        PlanTier::OrganizationSeat => "organization_seat",
        PlanTier::FriendVip => "friend_vip",
        PlanTier::Owner => "owner",
    }
}

fn str_to_plan(s: &str) -> PlanTier {
    match s {
        "trial" => PlanTier::Trial,
        "individual_monthly" => PlanTier::IndividualMonthly,
        "individual_annual" => PlanTier::IndividualAnnual,
        "organization_seat" => PlanTier::OrganizationSeat,
        "friend_vip" => PlanTier::FriendVip,
        "owner" => PlanTier::Owner,
        _ => PlanTier::Free,
    }
}

fn sub_state_to_str(s: SubscriptionState) -> &'static str {
    match s {
        SubscriptionState::Active => "active",
        SubscriptionState::Trialing => "trialing",
        SubscriptionState::PastDue => "past_due",
        SubscriptionState::Canceled => "canceled",
        SubscriptionState::Expired => "expired",
        SubscriptionState::Free => "free",
    }
}

fn str_to_sub_state(s: &str) -> SubscriptionState {
    match s {
        "active" => SubscriptionState::Active,
        "trialing" => SubscriptionState::Trialing,
        "past_due" => SubscriptionState::PastDue,
        "canceled" => SubscriptionState::Canceled,
        "expired" => SubscriptionState::Expired,
        _ => SubscriptionState::Free,
    }
}

fn usage_kind_to_str(k: UsageKind) -> &'static str {
    match k {
        UsageKind::DataOperation => "data_operation",
        UsageKind::AiCall => "ai_call",
    }
}

fn row_to_workspace_entitlement(
    row: &sqlx::postgres::PgRow,
) -> Result<WorkspaceEntitlementRecord, AuthError> {
    Ok(WorkspaceEntitlementRecord {
        workspace_id: row
            .try_get("workspace_id")
            .map_err(|_| AuthError::BackendUnavailable)?,
        user_id: row
            .try_get("user_id")
            .map_err(|_| AuthError::BackendUnavailable)?,
        plan: str_to_plan(
            &row.try_get::<String, _>("plan_tier")
                .map_err(|_| AuthError::BackendUnavailable)?,
        ),
        state: str_to_sub_state(
            &row.try_get::<String, _>("subscription_state")
                .map_err(|_| AuthError::BackendUnavailable)?,
        ),
        seats: u32::try_from(row.try_get::<i32, _>("seats").unwrap_or(1)).unwrap_or(1),
        trial_expires_at: row.try_get("trial_expires_at").ok().flatten(),
        current_period_end: row.try_get("current_period_end").ok().flatten(),
        past_due_since: row.try_get("past_due_since").ok().flatten(),
        friend_vip_active: row.try_get("friend_vip_active").unwrap_or(false),
        friend_vip_granted_by: row.try_get("friend_vip_granted_by").ok().flatten(),
        operation_credits: u32::try_from(row.try_get::<i32, _>("operation_credits").unwrap_or(0))
            .unwrap_or(0),
        pay_per_file_tokens: u32::try_from(
            row.try_get::<i32, _>("pay_per_file_tokens").unwrap_or(0),
        )
        .unwrap_or(0),
        trial_ai_used: u32::try_from(row.try_get::<i32, _>("trial_ai_used").unwrap_or(0))
            .unwrap_or(0),
    })
}

async fn ensure_and_lock_workspace_entitlement(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    user_id: Uuid,
) -> Result<WorkspaceEntitlementRecord, AuthError> {
    sqlx::query(
        r"
        INSERT INTO workspace_entitlement_state (workspace_id, user_id)
        VALUES ($1, $2)
        ON CONFLICT (workspace_id) DO NOTHING
        ",
    )
    .bind(workspace_id)
    .bind(user_id)
    .execute(&mut **tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    let row = sqlx::query(
        r"
        SELECT
            workspace_id, user_id, plan_tier, subscription_state, seats,
            trial_expires_at, current_period_end, past_due_since,
            friend_vip_active, friend_vip_granted_by,
            operation_credits, pay_per_file_tokens, trial_ai_used
        FROM workspace_entitlement_state
        WHERE workspace_id = $1
        FOR UPDATE
        ",
    )
    .bind(workspace_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    row_to_workspace_entitlement(&row)
}

async fn save_workspace_entitlement(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    rec: &WorkspaceEntitlementRecord,
) -> Result<(), AuthError> {
    // Satisfy `credit_balances` FORCE RLS policy (`app.current_workspace_id`) for this workspace.
    sqlx::query("SELECT set_config('app.current_workspace_id', $1, true)")
        .bind(rec.workspace_id.to_string())
        .execute(&mut **tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

    sqlx::query(
        r"
        UPDATE workspace_entitlement_state
        SET plan_tier = $2,
            subscription_state = $3,
            seats = $4,
            trial_expires_at = $5,
            current_period_end = $6,
            past_due_since = $7,
            friend_vip_active = $8,
            friend_vip_granted_by = $9,
            operation_credits = $10,
            pay_per_file_tokens = $11,
            trial_ai_used = $12,
            updated_at = now()
        WHERE workspace_id = $1
        ",
    )
    .bind(rec.workspace_id)
    .bind(plan_to_str(rec.plan))
    .bind(sub_state_to_str(rec.state))
    .bind(i32::try_from(rec.seats.max(1)).unwrap_or(1))
    .bind(rec.trial_expires_at)
    .bind(rec.current_period_end)
    .bind(rec.past_due_since)
    .bind(rec.friend_vip_active)
    .bind(rec.friend_vip_granted_by)
    .bind(i32::try_from(rec.operation_credits).unwrap_or(0))
    .bind(i32::try_from(rec.pay_per_file_tokens).unwrap_or(0))
    .bind(i32::try_from(rec.trial_ai_used).unwrap_or(0))
    .execute(&mut **tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    sqlx::query(
        r"
        INSERT INTO credit_balances (workspace_id, operation_credits, pay_per_file_tokens, updated_at)
        VALUES ($1, $2, $3, now())
        ON CONFLICT (workspace_id) DO UPDATE
        SET operation_credits = EXCLUDED.operation_credits,
            pay_per_file_tokens = EXCLUDED.pay_per_file_tokens,
            updated_at = now()
        ",
    )
    .bind(rec.workspace_id)
    .bind(i32::try_from(rec.operation_credits).unwrap_or(0))
    .bind(i32::try_from(rec.pay_per_file_tokens).unwrap_or(0))
    .execute(&mut **tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    Ok(())
}

async fn load_or_init_monthly_usage_for_update(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    period_ym: &str,
) -> Result<MonthlyUsage, AuthError> {
    sqlx::query(
        r"
        INSERT INTO workspace_monthly_usage (workspace_id, period_ym)
        VALUES ($1, $2)
        ON CONFLICT (workspace_id, period_ym) DO NOTHING
        ",
    )
    .bind(workspace_id)
    .bind(period_ym)
    .execute(&mut **tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    let row = sqlx::query(
        r"
        SELECT operations_used, ai_calls_used, rows_cleaned, pii_masked
        FROM workspace_monthly_usage
        WHERE workspace_id = $1 AND period_ym = $2
        FOR UPDATE
        ",
    )
    .bind(workspace_id)
    .bind(period_ym)
    .fetch_one(&mut **tx)
    .await
    .map_err(|_| AuthError::BackendUnavailable)?;

    Ok(MonthlyUsage {
        period_ym: period_ym.to_owned(),
        operations_used: u32::try_from(row.try_get::<i32, _>("operations_used").unwrap_or(0))
            .unwrap_or(0),
        ai_calls_used: u32::try_from(row.try_get::<i32, _>("ai_calls_used").unwrap_or(0))
            .unwrap_or(0),
        rows_cleaned: u64::try_from(row.try_get::<i64, _>("rows_cleaned").unwrap_or(0))
            .unwrap_or(0),
        pii_masked: u64::try_from(row.try_get::<i64, _>("pii_masked").unwrap_or(0)).unwrap_or(0),
    })
}

impl PgControlPlaneStore {
    /// Resolve authoritative effective entitlements & monthly usage directly from PostgreSQL 18,
    /// persisting any elapsed trial/past-due/canceled downgrades atomically.
    pub async fn effective_entitlements_pg(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        is_owner: bool,
        now_secs: i64,
    ) -> Result<EffectiveEntitlements, AuthError> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        let mut rec = ensure_and_lock_workspace_entitlement(&mut tx, workspace_id, user_id).await?;
        let before_plan = rec.plan;
        let before_state = rec.state;
        reconcile_expirations(&mut rec, now_secs);
        if rec.plan != before_plan || rec.state != before_state {
            save_workspace_entitlement(&mut tx, &rec).await?;
        }

        let period = period_ym_at(now_secs);
        let usage = load_or_init_monthly_usage_for_update(&mut tx, workspace_id, &period).await?;
        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        Ok(compute_effective(&rec, &usage, is_owner, now_secs))
    }

    /// Atomically enforce plan limits, record monthly usage, debit micro-purchases, and persist
    /// fingerprinted usage idempotency inside ONE PostgreSQL 18 transaction.
    pub async fn consume_usage_atomic_pg(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        is_owner: bool,
        req: &ConsumeUsageRequest,
        request_fingerprint: [u8; 32],
        now_secs: i64,
    ) -> Result<ConsumeUsageResult, AuthError> {
        let idem_key = req.idempotency_key.trim();
        if idem_key.is_empty() || idem_key.len() > 128 {
            return Err(AuthError::InvalidRequest);
        }

        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        // Per-workspace transaction advisory lock eliminates cross-instance concurrency races!
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
            .bind(format!("ent_consume:{workspace_id}"))
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        let mut rec = ensure_and_lock_workspace_entitlement(&mut tx, workspace_id, user_id).await?;
        reconcile_expirations(&mut rec, now_secs);
        let period = period_ym_at(now_secs);
        let mut usage =
            load_or_init_monthly_usage_for_update(&mut tx, workspace_id, &period).await?;

        // Step 1: Check durable fingerprinted idempotency table FIRST!
        // Reusing an existing idempotency key with a DIFFERENT kind or request attributes
        // MUST fail closed with 409 Conflict (`SF-PAY-007`) — NEVER authorize an AI call
        // or larger dataset using a prior DataOperation key!
        if let Some(existing_idem) = sqlx::query(
            r"
            SELECT request_fingerprint, watermark_required
            FROM entitlement_usage_idempotency
            WHERE workspace_id = $1 AND idempotency_key = $2
            ",
        )
        .bind(workspace_id)
        .bind(idem_key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?
        {
            let stored_fp: Vec<u8> = existing_idem
                .try_get("request_fingerprint")
                .map_err(|_| AuthError::BackendUnavailable)?;
            if stored_fp.as_slice() != request_fingerprint.as_slice() {
                // Rollback implicitly and reject cross-operation/parameter key misuse!
                return Err(AuthError::IdempotencyConflict);
            }
            let stored_wm: bool = existing_idem.try_get("watermark_required").unwrap_or(true);
            let ent = compute_effective(&rec, &usage, is_owner, now_secs);
            tx.commit()
                .await
                .map_err(|_| AuthError::BackendUnavailable)?;
            return Ok(ConsumeUsageResult {
                allowed: true,
                duplicate_idempotency_hit: true,
                watermark_required: stored_wm,
                debited_credit: false,
                debited_pay_per_file: false,
                entitlements: ent,
            });
        }

        // Step 2: Enforce authoritative business rules from `config/business.json`.
        let r = rules();
        let current = compute_effective(&rec, &usage, is_owner, now_secs);
        let mut debited_credit = false;
        let mut debited_pay_per_file = false;
        let mut watermark_required = current.watermark_required;

        match req.kind {
            UsageKind::AiCall => {
                if !current.ai_enabled {
                    return Err(AuthError::PaidPlanRequired);
                }
                if current.effective_plan != PlanTier::Owner
                    && current.ai_calls_used >= current.ai_calls_limit
                {
                    return Err(AuthError::AiQuotaExceeded);
                }
                if current.effective_plan == PlanTier::Trial {
                    rec.trial_ai_used = rec.trial_ai_used.saturating_add(1);
                }
                usage.ai_calls_used = usage.ai_calls_used.saturating_add(1);
            }
            UsageKind::DataOperation => {
                if current.effective_plan == PlanTier::Free {
                    let exceeds_free_shape =
                        req.row_count > r.free_max_rows || req.column_count > r.free_max_cols;
                    let exceeds_free_ops = usage.operations_used >= r.free_ops_per_month;

                    if exceeds_free_shape {
                        // Pay-per-file unlocks up to 100k rows and removes watermark (no AI / no premium packs).
                        if rec.pay_per_file_tokens > 0 && req.row_count <= r.pay_per_file_max_rows {
                            rec.pay_per_file_tokens -= 1;
                            debited_pay_per_file = true;
                            watermark_required = !r.pay_per_file_removes_watermark;
                        } else {
                            return Err(AuthError::DatasetLimitExceeded);
                        }
                    } else if exceeds_free_ops {
                        // Credit pack (25 ops, never expires) covers extra ops & removes watermark.
                        if rec.operation_credits > 0 {
                            rec.operation_credits -= 1;
                            debited_credit = true;
                            watermark_required = !r.credit_pack_25_removes_watermark;
                        } else if rec.pay_per_file_tokens > 0 {
                            rec.pay_per_file_tokens -= 1;
                            debited_pay_per_file = true;
                            watermark_required = !r.pay_per_file_removes_watermark;
                        } else {
                            return Err(AuthError::QuotaExceeded);
                        }
                    } else {
                        // Free user within the 5 monthly operations ALWAYS receives mandatory watermark.
                        watermark_required = true;
                    }
                }
                usage.operations_used = usage.operations_used.saturating_add(1);
                usage.rows_cleaned = usage.rows_cleaned.saturating_add(req.row_count);
                usage.pii_masked = usage.pii_masked.saturating_add(req.pii_masked_count);
            }
        }

        // Step 3: Persist updated entitlement record, monthly counters, and idempotency fingerprint atomically!
        save_workspace_entitlement(&mut tx, &rec).await?;

        sqlx::query(
            r"
            UPDATE workspace_monthly_usage
            SET operations_used = $3,
                ai_calls_used = $4,
                rows_cleaned = $5,
                pii_masked = $6,
                updated_at = now()
            WHERE workspace_id = $1 AND period_ym = $2
            ",
        )
        .bind(workspace_id)
        .bind(&period)
        .bind(i32::try_from(usage.operations_used).unwrap_or(i32::MAX))
        .bind(i32::try_from(usage.ai_calls_used).unwrap_or(i32::MAX))
        .bind(i64::try_from(usage.rows_cleaned).unwrap_or(i64::MAX))
        .bind(i64::try_from(usage.pii_masked).unwrap_or(i64::MAX))
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        sqlx::query(
            r"
            INSERT INTO entitlement_usage_idempotency (
                workspace_id, idempotency_key, request_fingerprint, usage_kind,
                watermark_required, debited_credit, debited_pay_per_file
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ",
        )
        .bind(workspace_id)
        .bind(idem_key)
        .bind(request_fingerprint.as_slice())
        .bind(usage_kind_to_str(req.kind))
        .bind(watermark_required)
        .bind(debited_credit)
        .bind(debited_pay_per_file)
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        let updated = compute_effective(&rec, &usage, is_owner, now_secs);
        Ok(ConsumeUsageResult {
            allowed: true,
            duplicate_idempotency_hit: false,
            watermark_required,
            debited_credit,
            debited_pay_per_file,
            entitlements: updated,
        })
    }

    /// Atomically claim a one-time 7-day Trial in PostgreSQL 18 enforcing email/phone/device uniqueness.
    pub async fn claim_trial_pg(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        email_hash: &str,
        phone_hash: &str,
        device_hash: &str,
        now_secs: i64,
    ) -> Result<EffectiveEntitlements, AuthError> {
        if email_hash.is_empty() || phone_hash.is_empty() || device_hash.is_empty() {
            return Err(AuthError::InvalidRequest);
        }
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        for (kind, hash) in [
            ("email", email_hash),
            ("phone", phone_hash),
            ("device", device_hash),
        ] {
            let inserted: Option<String> = sqlx::query_scalar(
                r"
                INSERT INTO trial_anti_abuse_claims (claim_kind, claim_hash, workspace_id, user_id)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (claim_kind, claim_hash) DO NOTHING
                RETURNING claim_kind
                ",
            )
            .bind(kind)
            .bind(hash)
            .bind(workspace_id)
            .bind(user_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
            if inserted.is_none() {
                return Err(AuthError::DuplicateAccount);
            }
        }

        let mut rec = ensure_and_lock_workspace_entitlement(&mut tx, workspace_id, user_id).await?;
        rec.plan = PlanTier::Trial;
        rec.state = SubscriptionState::Trialing;
        rec.trial_expires_at = Some(now_secs + rules().trial_duration_secs);
        rec.trial_ai_used = 0;
        save_workspace_entitlement(&mut tx, &rec).await?;

        let period = period_ym_at(now_secs);
        let usage = load_or_init_monthly_usage_for_update(&mut tx, workspace_id, &period).await?;
        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        Ok(compute_effective(&rec, &usage, false, now_secs))
    }

    /// Extend a workspace's trial durably in PostgreSQL 18.
    pub async fn extend_trial_pg(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        extra_days: i64,
        now_secs: i64,
    ) -> Result<EffectiveEntitlements, AuthError> {
        if extra_days <= 0 || extra_days > 365 {
            return Err(AuthError::InvalidRequest);
        }
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        let mut rec = ensure_and_lock_workspace_entitlement(&mut tx, workspace_id, user_id).await?;
        let base = rec.trial_expires_at.unwrap_or(now_secs).max(now_secs);
        rec.trial_expires_at = Some(base + extra_days * 86_400);
        if matches!(rec.plan, PlanTier::Free | PlanTier::Trial) {
            rec.plan = PlanTier::Trial;
            rec.state = SubscriptionState::Trialing;
        }
        save_workspace_entitlement(&mut tx, &rec).await?;

        let period = period_ym_at(now_secs);
        let usage = load_or_init_monthly_usage_for_update(&mut tx, workspace_id, &period).await?;
        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        Ok(compute_effective(&rec, &usage, false, now_secs))
    }

    /// Grant or revoke Friend VIP durably in PostgreSQL 18.
    pub async fn set_friend_vip_pg(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        granted_by: Uuid,
        active: bool,
        now_secs: i64,
    ) -> Result<EffectiveEntitlements, AuthError> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        let mut rec = ensure_and_lock_workspace_entitlement(&mut tx, workspace_id, user_id).await?;
        rec.friend_vip_active = active;
        rec.friend_vip_granted_by = if active { Some(granted_by) } else { None };
        save_workspace_entitlement(&mut tx, &rec).await?;

        let period = period_ym_at(now_secs);
        let usage = load_or_init_monthly_usage_for_update(&mut tx, workspace_id, &period).await?;
        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        Ok(compute_effective(&rec, &usage, false, now_secs))
    }

    /// Persist a subscription plan/state transition durably in PostgreSQL 18.
    #[allow(clippy::too_many_arguments)]
    pub async fn apply_subscription_update_pg(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        plan: PlanTier,
        state: SubscriptionState,
        seats: u32,
        current_period_end: Option<i64>,
        now_secs: i64,
    ) -> Result<EffectiveEntitlements, AuthError> {
        if plan == PlanTier::OrganizationSeat && seats < rules().org_min_seats {
            return Err(AuthError::OrgMinSeatsRequired);
        }
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        let mut rec = ensure_and_lock_workspace_entitlement(&mut tx, workspace_id, user_id).await?;
        rec.plan = plan;
        rec.state = state;
        rec.seats = seats.max(1);
        rec.current_period_end = current_period_end;
        rec.past_due_since = match state {
            SubscriptionState::PastDue => rec.past_due_since.or(Some(now_secs)),
            _ => None,
        };
        save_workspace_entitlement(&mut tx, &rec).await?;

        let period = period_ym_at(now_secs);
        let usage = load_or_init_monthly_usage_for_update(&mut tx, workspace_id, &period).await?;
        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        Ok(compute_effective(&rec, &usage, false, now_secs))
    }

    /// Persist a micro-purchase grant durably in PostgreSQL 18.
    pub async fn grant_micro_purchase_pg(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        operation_credits_delta: u32,
        pay_per_file_delta: u32,
    ) -> Result<(), AuthError> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        let mut rec = ensure_and_lock_workspace_entitlement(&mut tx, workspace_id, user_id).await?;
        rec.operation_credits = rec
            .operation_credits
            .saturating_add(operation_credits_delta);
        rec.pay_per_file_tokens = rec.pay_per_file_tokens.saturating_add(pay_per_file_delta);
        save_workspace_entitlement(&mut tx, &rec).await?;
        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        Ok(())
    }

    /// Compute authoritative live aggregate platform stats directly from PostgreSQL 18.
    pub async fn aggregate_stats_pg(
        &self,
        now_secs: i64,
    ) -> Result<EntitlementAggregateStats, AuthError> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        let ws_rows = sqlx::query(
            r"
            SELECT
                workspace_id, user_id, plan_tier, subscription_state, seats,
                trial_expires_at, current_period_end, past_due_since,
                friend_vip_active, friend_vip_granted_by,
                operation_credits, pay_per_file_tokens, trial_ai_used
            FROM workspace_entitlement_state
            ",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        let usage_row = sqlx::query(
            r"
            SELECT
                COALESCE(SUM(rows_cleaned), 0)::BIGINT AS rows_cleaned_total,
                COALESCE(SUM(pii_masked), 0)::BIGINT AS pii_masked_total,
                COALESCE(SUM(ai_calls_used), 0)::BIGINT AS ai_calls_used_total
            FROM workspace_monthly_usage
            ",
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        let r = rules();
        let mut stats = EntitlementAggregateStats {
            active_trials: 0,
            free_users: 0,
            monthly_subscribers: 0,
            annual_subscribers: 0,
            organization_seats: 0,
            friend_vip_users: 0,
            credit_pack_holders: 0,
            pay_per_file_holders: 0,
            mrr_usd: 0,
            mrr_egp: 0,
            arr_usd: 0,
            arr_egp: 0,
            rows_cleaned_total: u64::try_from(
                usage_row
                    .try_get::<i64, _>("rows_cleaned_total")
                    .unwrap_or(0),
            )
            .unwrap_or(0),
            pii_masked_total: u64::try_from(
                usage_row.try_get::<i64, _>("pii_masked_total").unwrap_or(0),
            )
            .unwrap_or(0),
            ai_calls_used_total: u64::try_from(
                usage_row
                    .try_get::<i64, _>("ai_calls_used_total")
                    .unwrap_or(0),
            )
            .unwrap_or(0),
        };

        for row in ws_rows {
            if let Ok(mut copy) = row_to_workspace_entitlement(&row) {
                reconcile_expirations(&mut copy, now_secs);
                if copy.operation_credits > 0 {
                    stats.credit_pack_holders += 1;
                }
                if copy.pay_per_file_tokens > 0 {
                    stats.pay_per_file_holders += 1;
                }
                if copy.friend_vip_active {
                    stats.friend_vip_users += 1;
                    continue;
                }
                match (copy.plan, copy.state) {
                    (PlanTier::Trial, SubscriptionState::Trialing) => stats.active_trials += 1,
                    (
                        PlanTier::IndividualMonthly,
                        SubscriptionState::Active | SubscriptionState::PastDue,
                    ) => {
                        stats.monthly_subscribers += 1;
                        stats.mrr_usd += r.individual_monthly_usd;
                        stats.mrr_egp += r.individual_monthly_egp;
                    }
                    (
                        PlanTier::IndividualAnnual,
                        SubscriptionState::Active | SubscriptionState::PastDue,
                    ) => {
                        stats.annual_subscribers += 1;
                        stats.mrr_usd += r.individual_annual_usd / 12;
                        stats.mrr_egp += r.individual_annual_egp / 12;
                    }
                    (
                        PlanTier::OrganizationSeat,
                        SubscriptionState::Active | SubscriptionState::PastDue,
                    ) => {
                        let seats = u64::from(copy.seats.max(r.org_min_seats));
                        stats.organization_seats += seats;
                        stats.mrr_usd += seats * r.org_seat_monthly_usd;
                        stats.mrr_egp += seats * r.org_seat_monthly_egp;
                    }
                    _ => stats.free_users += 1,
                }
            }
        }
        stats.arr_usd = stats.mrr_usd * 12;
        stats.arr_egp = stats.mrr_egp * 12;
        Ok(stats)
    }
}
