//! Authoritative PostgreSQL 18 Financial Integrity Ledger Repository (Migration 0013).
//!
//! Guarantees enforced at the database layer (never relying on in-memory maps):
//! 1. Durable server-side checkout idempotency via `UNIQUE (workspace_id, idempotency_key)` +
//!    32-byte SHA-256 `request_fingerprint` conflict detection (`SF-PAY-007`).
//! 2. Durable provider order correlation via `UNIQUE (provider, provider_order_ref)`.
//! 3. Atomic single-transaction webhook processing:
//!    - Claims `(provider, provider_event_id)` in `billing_webhook_events`.
//!    - Locks internal `billing_orders` (`SELECT ... FOR UPDATE`).
//!    - Enforces `(provider, provider_transaction_id)` and `(order_id, grant_kind)` uniqueness in
//!      `billing_entitlement_grants` so replayed transactions with new event IDs can NEVER double-grant.
//!    - Updates `billing_orders`, `billing_invoices`, `billing_entitlement_grants`,
//!      `workspace_billing_watermarks`, and `workspace_entitlement_state` in ONE atomic transaction.
//! 4. Durable idempotent refund requests via `UNIQUE (workspace_id, refund_request_key)`.

use sha2::{Digest, Sha256};
use sqlx::Row;
use uuid::Uuid;

use crate::billing::{
    BillingCurrency, BillingEventRecord, BillingOrderRecord, BillingProvider,
    BillingRevenueSummary, CheckoutProduct, InvoiceRecord, PaymentOrderState, RefundRecord,
};
use crate::entitlements::{PlanTier, SubscriptionState, WorkspaceEntitlementRecord};
use crate::error::AuthError;
use crate::pg_store::PgControlPlaneStore;

/// Result of claiming an idempotency slot in PostgreSQL before contacting a payment provider.
#[derive(Debug, Clone)]
pub enum PgCheckoutClaim {
    /// Newly created order row (`state = 'created'`); caller must now invoke provider API.
    Created(BillingOrderRecord),
    /// Existing order with identical `request_fingerprint` (retry / double-click / reconciliation).
    Existing(BillingOrderRecord),
}

/// Verified, provider-authenticated financial webhook transition ready for atomic PostgreSQL commit.
#[derive(Debug, Clone)]
pub struct VerifiedWebhookTransition {
    pub provider: BillingProvider,
    pub provider_event_id: String,
    pub event_type: String,
    pub raw_payload_hash: [u8; 32],
    pub occurred_at_unix: i64,
    pub now_secs: i64,
    /// Lookup key: internal order ID (Paddle `custom_data.safi_order_id`) or provider order ref.
    pub order_id_hint: Option<Uuid>,
    pub provider_order_ref: Option<String>,
    pub provider_transaction_id: Option<String>,
    pub external_invoice_id: Option<String>,
    /// Amount reported by provider (minor units) to cross-check against `order.expected_amount_minor`.
    pub reported_amount_minor: Option<u64>,
    pub reported_currency: Option<BillingCurrency>,
    pub reported_integration_id: Option<u64>,
    pub period_end_unix: Option<i64>,
    pub kind: VerifiedWebhookKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifiedWebhookKind {
    /// Final verified captured payment -> grants subscription / credits / pay-per-file ONCE.
    Paid,
    /// Non-final payment state (checkout opened, authorized not captured, pending) -> NO entitlement!
    PendingNoGrant,
    /// Payment failed -> marks order failed, NO entitlement.
    Failed,
    /// Subscription past due -> grace period warning.
    PastDue,
    /// Subscription canceled -> canceled state.
    Canceled,
    /// Confirmed refund (full or partial).
    Refunded {
        partial_amount_minor: Option<u64>,
        mandatory_legal_override: bool,
    },
    /// Confirmed chargeback/dispute -> immediately revoke paid access.
    Chargeback,
}

fn provider_to_str(p: BillingProvider) -> &'static str {
    match p {
        BillingProvider::Paddle => "paddle",
        BillingProvider::Paymob => "paymob",
        BillingProvider::Manual => "manual",
    }
}

fn str_to_provider(s: &str) -> BillingProvider {
    match s {
        "paddle" => BillingProvider::Paddle,
        "paymob" => BillingProvider::Paymob,
        _ => BillingProvider::Manual,
    }
}

fn currency_to_str(c: BillingCurrency) -> &'static str {
    match c {
        BillingCurrency::Usd => "USD",
        BillingCurrency::Egp => "EGP",
    }
}

fn str_to_currency(s: &str) -> BillingCurrency {
    if s.trim().eq_ignore_ascii_case("EGP") {
        BillingCurrency::Egp
    } else {
        BillingCurrency::Usd
    }
}

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

fn row_to_order(row: &sqlx::postgres::PgRow) -> Result<BillingOrderRecord, AuthError> {
    let fp_bytes: Vec<u8> = row
        .try_get("request_fingerprint")
        .map_err(|_| AuthError::BackendUnavailable)?;
    let mut fp = [0u8; 32];
    if fp_bytes.len() == 32 {
        fp.copy_from_slice(&fp_bytes);
    }
    let provider_str: String = row
        .try_get("provider")
        .map_err(|_| AuthError::BackendUnavailable)?;
    let product_str: String = row
        .try_get("product_code")
        .map_err(|_| AuthError::BackendUnavailable)?;
    let currency_str: String = row
        .try_get("currency")
        .map_err(|_| AuthError::BackendUnavailable)?;
    let state_str: String = row
        .try_get("state")
        .map_err(|_| AuthError::BackendUnavailable)?;
    Ok(BillingOrderRecord {
        id: row
            .try_get("id")
            .map_err(|_| AuthError::BackendUnavailable)?,
        workspace_id: row
            .try_get("workspace_id")
            .map_err(|_| AuthError::BackendUnavailable)?,
        user_id: row
            .try_get("user_id")
            .map_err(|_| AuthError::BackendUnavailable)?,
        idempotency_key: row
            .try_get("idempotency_key")
            .map_err(|_| AuthError::BackendUnavailable)?,
        request_fingerprint: fp,
        provider: str_to_provider(&provider_str),
        product: CheckoutProduct::from_str_code(&product_str).ok_or(AuthError::InvalidRequest)?,
        provider_price_id: row
            .try_get("provider_price_id")
            .map_err(|_| AuthError::BackendUnavailable)?,
        seats: u32::try_from(row.try_get::<i32, _>("seats").unwrap_or(1)).unwrap_or(1),
        currency: str_to_currency(&currency_str),
        original_amount_minor: u64::try_from(
            row.try_get::<i64, _>("original_amount_minor").unwrap_or(0),
        )
        .unwrap_or(0),
        discount_amount_minor: u64::try_from(
            row.try_get::<i64, _>("discount_amount_minor").unwrap_or(0),
        )
        .unwrap_or(0),
        expected_amount_minor: u64::try_from(
            row.try_get::<i64, _>("expected_amount_minor").unwrap_or(0),
        )
        .unwrap_or(0),
        coupon_code: row.try_get("coupon_code").ok().flatten(),
        coupon_reservation_id: row.try_get("coupon_reservation_id").ok().flatten(),
        expected_integration_id: row
            .try_get::<Option<i64>, _>("expected_integration_id")
            .ok()
            .flatten()
            .and_then(|v| u64::try_from(v).ok()),
        provider_order_ref: row.try_get("provider_order_ref").ok().flatten(),
        provider_intention_id: row.try_get("provider_intention_id").ok().flatten(),
        checkout_url: row.try_get("checkout_url").ok().flatten(),
        provider_transaction_id: row.try_get("provider_transaction_id").ok().flatten(),
        state: PaymentOrderState::from_str_code(&state_str).unwrap_or(PaymentOrderState::Created),
        last_error_code: row.try_get("last_error_code").ok().flatten(),
        created_at: row.try_get::<i64, _>("created_at_secs").unwrap_or(0),
        expires_at: row.try_get::<i64, _>("expires_at_secs").unwrap_or(0),
        updated_at: row.try_get::<i64, _>("updated_at_secs").unwrap_or(0),
        paid_at: row.try_get::<Option<i64>, _>("paid_at_secs").ok().flatten(),
    })
}

const ORDER_SELECT_COLS: &str = r"
    id, workspace_id, user_id, idempotency_key, request_fingerprint, provider, product_code,
    provider_price_id, seats, currency, original_amount_minor, discount_amount_minor,
    expected_amount_minor, coupon_code, coupon_reservation_id, expected_integration_id,
    provider_order_ref, provider_intention_id, checkout_url, provider_transaction_id,
    state, last_error_code,
    EXTRACT(EPOCH FROM created_at)::BIGINT AS created_at_secs,
    EXTRACT(EPOCH FROM expires_at)::BIGINT AS expires_at_secs,
    EXTRACT(EPOCH FROM updated_at)::BIGINT AS updated_at_secs,
    EXTRACT(EPOCH FROM paid_at)::BIGINT AS paid_at_secs
";

impl PgControlPlaneStore {
    /// Atomically claim a checkout idempotency slot in PostgreSQL (`UNIQUE (workspace_id, idempotency_key)`).
    ///
    /// - First request inserts a new `billing_orders` row and returns `PgCheckoutClaim::Created`.
    /// - Concurrent or repeated requests with the SAME fingerprint return `PgCheckoutClaim::Existing`.
    /// - Same key with a DIFFERENT fingerprint fails closed with `AuthError::IdempotencyConflict` (`SF-PAY-007`).
    pub async fn claim_or_load_checkout_order_pg(
        &self,
        candidate: &BillingOrderRecord,
    ) -> Result<PgCheckoutClaim, AuthError> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        // Advisory lock scoped to (workspace_id, idempotency_key) serializes simultaneous races cleanly.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
            .bind(format!(
                "billing_order:{}:{}",
                candidate.workspace_id, candidate.idempotency_key
            ))
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        let existing_sql = format!(
            "SELECT {ORDER_SELECT_COLS} FROM billing_orders WHERE workspace_id = $1 AND idempotency_key = $2 FOR UPDATE"
        );
        if let Some(row) = sqlx::query(&existing_sql)
            .bind(candidate.workspace_id)
            .bind(&candidate.idempotency_key)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?
        {
            let existing = row_to_order(&row)?;
            tx.commit()
                .await
                .map_err(|_| AuthError::BackendUnavailable)?;
            if existing.request_fingerprint != candidate.request_fingerprint {
                return Err(AuthError::IdempotencyConflict);
            }
            return Ok(PgCheckoutClaim::Existing(existing));
        }

        let insert_sql = format!(
            r"
            INSERT INTO billing_orders (
                id, workspace_id, user_id, idempotency_key, request_fingerprint,
                provider, product_code, provider_price_id, seats, currency,
                original_amount_minor, discount_amount_minor, expected_amount_minor,
                coupon_code, coupon_reservation_id, expected_integration_id,
                state, expires_at
            )
            VALUES (
                $1, $2, $3, $4, $5,
                $6, $7, $8, $9, $10,
                $11, $12, $13,
                $14, $15, $16,
                'created', to_timestamp($17)
            )
            RETURNING {ORDER_SELECT_COLS}
            "
        );
        let row = sqlx::query(&insert_sql)
            .bind(candidate.id)
            .bind(candidate.workspace_id)
            .bind(candidate.user_id)
            .bind(&candidate.idempotency_key)
            .bind(candidate.request_fingerprint.as_slice())
            .bind(provider_to_str(candidate.provider))
            .bind(candidate.product.as_str())
            .bind(&candidate.provider_price_id)
            .bind(i32::try_from(candidate.seats).unwrap_or(1))
            .bind(currency_to_str(candidate.currency))
            .bind(i64::try_from(candidate.original_amount_minor).unwrap_or(0))
            .bind(i64::try_from(candidate.discount_amount_minor).unwrap_or(0))
            .bind(i64::try_from(candidate.expected_amount_minor).unwrap_or(0))
            .bind(&candidate.coupon_code)
            .bind(candidate.coupon_reservation_id)
            .bind(
                candidate
                    .expected_integration_id
                    .and_then(|v| i64::try_from(v).ok()),
            )
            .bind(candidate.expires_at)
            .fetch_one(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        let created = row_to_order(&row)?;
        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        Ok(PgCheckoutClaim::Created(created))
    }

    /// Persist provider checkout details or error/reconciliation state onto `billing_orders`
    /// BEFORE returning a response to the client.
    pub async fn update_checkout_order_pg(
        &self,
        order: &BillingOrderRecord,
    ) -> Result<BillingOrderRecord, AuthError> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        let sql = format!(
            r"
            UPDATE billing_orders
            SET provider_order_ref = $2,
                provider_intention_id = $3,
                checkout_url = $4,
                state = $5,
                last_error_code = $6,
                coupon_code = $7,
                coupon_reservation_id = $8,
                discount_amount_minor = $9,
                expected_amount_minor = $10,
                updated_at = now()
            WHERE id = $1
            RETURNING {ORDER_SELECT_COLS}
            "
        );
        let row = sqlx::query(&sql)
            .bind(order.id)
            .bind(&order.provider_order_ref)
            .bind(&order.provider_intention_id)
            .bind(&order.checkout_url)
            .bind(order.state.as_str())
            .bind(&order.last_error_code)
            .bind(&order.coupon_code)
            .bind(order.coupon_reservation_id)
            .bind(i64::try_from(order.discount_amount_minor).unwrap_or(0))
            .bind(i64::try_from(order.expected_amount_minor).unwrap_or(0))
            .fetch_one(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        let updated = row_to_order(&row)?;
        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        Ok(updated)
    }

    /// Fetch an order by internal ID directly from PostgreSQL.
    pub async fn get_billing_order_by_id_pg(
        &self,
        order_id: Uuid,
    ) -> Result<Option<BillingOrderRecord>, AuthError> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        let sql = format!("SELECT {ORDER_SELECT_COLS} FROM billing_orders WHERE id = $1");
        let row = sqlx::query(&sql)
            .bind(order_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        match row {
            Some(r) => Ok(Some(row_to_order(&r)?)),
            None => Ok(None),
        }
    }

    /// Look up an internal order by `(provider, provider_order_ref)` directly in PostgreSQL.
    pub async fn find_billing_order_by_provider_ref_pg(
        &self,
        provider: BillingProvider,
        provider_order_ref: &str,
    ) -> Result<Option<BillingOrderRecord>, AuthError> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        let sql = format!(
            "SELECT {ORDER_SELECT_COLS} FROM billing_orders WHERE provider = $1 AND provider_order_ref = $2"
        );
        let row = sqlx::query(&sql)
            .bind(provider_to_str(provider))
            .bind(provider_order_ref)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        match row {
            Some(r) => Ok(Some(row_to_order(&r)?)),
            None => Ok(None),
        }
    }

    /// Execute a verified payment webhook transition inside ONE atomic PostgreSQL 18 transaction.
    ///
    /// Returns `(BillingEventRecord, Option<WorkspaceEntitlementRecord>, Option<Uuid /* committed coupon_reservation_id */>)`.
    /// If ANY validation or database step fails, the ENTIRE transaction rolls back with ZERO partial grants!
    pub async fn process_webhook_atomic_pg(
        &self,
        t: &VerifiedWebhookTransition,
    ) -> Result<
        (
            BillingEventRecord,
            Option<WorkspaceEntitlementRecord>,
            Option<Uuid>,
        ),
        AuthError,
    > {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        let provider_str = provider_to_str(t.provider);

        // Step 1: Atomically claim `(provider, provider_event_id)` via DB uniqueness constraint!
        let claimed_id: Option<Uuid> = sqlx::query_scalar(
            r"
            INSERT INTO billing_webhook_events (
                provider, provider_event_id, payload_hash, processing_status,
                event_type, occurred_at
            )
            VALUES ($1, $2, $3, 'received', $4, to_timestamp($5))
            ON CONFLICT (provider, provider_event_id) DO NOTHING
            RETURNING id
            ",
        )
        .bind(provider_str)
        .bind(&t.provider_event_id)
        .bind(t.raw_payload_hash.as_slice())
        .bind(&t.event_type)
        .bind(t.occurred_at_unix)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        let Some(webhook_row_id) = claimed_id else {
            // Concurrent or sequential duplicate delivery -> ZERO side effects!
            tx.commit()
                .await
                .map_err(|_| AuthError::BackendUnavailable)?;
            return Ok((
                BillingEventRecord {
                    id: Uuid::now_v7(),
                    provider: t.provider,
                    external_event_id: t.provider_event_id.clone(),
                    event_type: t.event_type.clone(),
                    workspace_id: None,
                    occurred_at: t.occurred_at_unix,
                    processed_at: t.now_secs,
                    outcome: "idempotent_duplicate_ignored".to_owned(),
                },
                None,
                None,
            ));
        };

        // Step 2: Locate and lock (`FOR UPDATE`) the trusted server-created internal `billing_orders` row!
        let order_row = if let Some(oid) = t.order_id_hint {
            let sql = format!(
                "SELECT {ORDER_SELECT_COLS} FROM billing_orders WHERE id = $1 AND provider = $2 FOR UPDATE"
            );
            sqlx::query(&sql)
                .bind(oid)
                .bind(provider_str)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|_| AuthError::BackendUnavailable)?
        } else if let Some(ref pref) = t.provider_order_ref {
            let sql = format!(
                "SELECT {ORDER_SELECT_COLS} FROM billing_orders WHERE provider = $1 AND (provider_order_ref = $2 OR provider_transaction_id = $2) FOR UPDATE"
            );
            sqlx::query(&sql)
                .bind(provider_str)
                .bind(pref)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|_| AuthError::BackendUnavailable)?
        } else if let Some(ref ptx) = t.provider_transaction_id {
            let sql = format!(
                "SELECT {ORDER_SELECT_COLS} FROM billing_orders WHERE provider = $1 AND (provider_transaction_id = $2 OR provider_order_ref = $2) FOR UPDATE"
            );
            sqlx::query(&sql)
                .bind(provider_str)
                .bind(ptx)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|_| AuthError::BackendUnavailable)?
        } else {
            None
        };

        let Some(row) = order_row else {
            // Unknown / uncorrelated order -> ROLL BACK transaction cleanly, activate NOTHING!
            return Err(AuthError::BillingOperationFailed);
        };
        let order = row_to_order(&row)?;

        // Step 3: Verify amount, currency, and Paymob integration_id against the trusted stored order!
        if let Some(curr) = t.reported_currency {
            if curr != order.currency {
                return Err(AuthError::BillingOperationFailed);
            }
        }
        if let Some(exp_int) = order.expected_integration_id {
            if let Some(rep_int) = t.reported_integration_id {
                if rep_int != exp_int {
                    return Err(AuthError::BillingOperationFailed);
                }
            } else if t.provider == BillingProvider::Paymob {
                return Err(AuthError::BillingOperationFailed);
            }
        }
        if matches!(t.kind, VerifiedWebhookKind::Paid) {
            match t.reported_amount_minor {
                Some(rep_amt) if rep_amt == order.expected_amount_minor => {}
                _ => return Err(AuthError::BillingOperationFailed),
            }
        }

        // Step 4: Lock workspace watermark and reject stale out-of-order events.
        sqlx::query(
            r"
            INSERT INTO workspace_billing_watermarks (workspace_id, last_event_occurred_at)
            VALUES ($1, 0)
            ON CONFLICT (workspace_id) DO NOTHING
            ",
        )
        .bind(order.workspace_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        let last_ts: i64 = sqlx::query_scalar(
            "SELECT last_event_occurred_at FROM workspace_billing_watermarks WHERE workspace_id = $1 FOR UPDATE",
        )
        .bind(order.workspace_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        if t.occurred_at_unix < last_ts {
            sqlx::query(
                r"
                UPDATE billing_webhook_events
                SET processing_status = 'ignored',
                    processed_at = now(),
                    order_id = $2,
                    workspace_id = $3,
                    outcome = 'stale_out_of_order_ignored'
                WHERE id = $1
                ",
            )
            .bind(webhook_row_id)
            .bind(order.id)
            .bind(order.workspace_id)
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
            tx.commit()
                .await
                .map_err(|_| AuthError::BackendUnavailable)?;
            return Ok((
                BillingEventRecord {
                    id: webhook_row_id,
                    provider: t.provider,
                    external_event_id: t.provider_event_id.clone(),
                    event_type: t.event_type.clone(),
                    workspace_id: Some(order.workspace_id),
                    occurred_at: t.occurred_at_unix,
                    processed_at: t.now_secs,
                    outcome: "stale_out_of_order_ignored".to_owned(),
                },
                None,
                None,
            ));
        }

        // Never let a non-refund/non-chargeback event resurrect an already refunded or chargebacked order!
        if matches!(
            order.state,
            PaymentOrderState::Refunded | PaymentOrderState::Chargeback
        ) && !matches!(
            t.kind,
            VerifiedWebhookKind::Refunded { .. } | VerifiedWebhookKind::Chargeback
        ) {
            sqlx::query(
                r"
                UPDATE billing_webhook_events
                SET processing_status = 'ignored',
                    processed_at = now(),
                    order_id = $2,
                    workspace_id = $3,
                    outcome = 'terminal_state_protected'
                WHERE id = $1
                ",
            )
            .bind(webhook_row_id)
            .bind(order.id)
            .bind(order.workspace_id)
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
            tx.commit()
                .await
                .map_err(|_| AuthError::BackendUnavailable)?;
            return Ok((
                BillingEventRecord {
                    id: webhook_row_id,
                    provider: t.provider,
                    external_event_id: t.provider_event_id.clone(),
                    event_type: t.event_type.clone(),
                    workspace_id: Some(order.workspace_id),
                    occurred_at: t.occurred_at_unix,
                    processed_at: t.now_secs,
                    outcome: "terminal_state_protected".to_owned(),
                },
                None,
                None,
            ));
        }

        // Load/lock workspace entitlement state row inside the SAME transaction.
        sqlx::query(
            r"
            INSERT INTO workspace_entitlement_state (workspace_id, user_id)
            VALUES ($1, $2)
            ON CONFLICT (workspace_id) DO NOTHING
            ",
        )
        .bind(order.workspace_id)
        .bind(order.user_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        let ent_row = sqlx::query(
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
        .bind(order.workspace_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        let mut ent = WorkspaceEntitlementRecord {
            workspace_id: ent_row
                .try_get("workspace_id")
                .map_err(|_| AuthError::BackendUnavailable)?,
            user_id: ent_row
                .try_get("user_id")
                .map_err(|_| AuthError::BackendUnavailable)?,
            plan: str_to_plan(
                &ent_row
                    .try_get::<String, _>("plan_tier")
                    .unwrap_or_else(|_| "free".to_owned()),
            ),
            state: str_to_sub_state(
                &ent_row
                    .try_get::<String, _>("subscription_state")
                    .unwrap_or_else(|_| "free".to_owned()),
            ),
            seats: u32::try_from(ent_row.try_get::<i32, _>("seats").unwrap_or(1)).unwrap_or(1),
            trial_expires_at: ent_row.try_get("trial_expires_at").ok().flatten(),
            current_period_end: ent_row.try_get("current_period_end").ok().flatten(),
            past_due_since: ent_row.try_get("past_due_since").ok().flatten(),
            friend_vip_active: ent_row.try_get("friend_vip_active").unwrap_or(false),
            friend_vip_granted_by: ent_row.try_get("friend_vip_granted_by").ok().flatten(),
            operation_credits: u32::try_from(
                ent_row.try_get::<i32, _>("operation_credits").unwrap_or(0),
            )
            .unwrap_or(0),
            pay_per_file_tokens: u32::try_from(
                ent_row
                    .try_get::<i32, _>("pay_per_file_tokens")
                    .unwrap_or(0),
            )
            .unwrap_or(0),
            trial_ai_used: u32::try_from(ent_row.try_get::<i32, _>("trial_ai_used").unwrap_or(0))
                .unwrap_or(0),
        };

        let mut updated_entitlement: Option<WorkspaceEntitlementRecord> = None;
        let mut committed_coupon_res: Option<Uuid> = None;
        let outcome: String;

        match &t.kind {
            VerifiedWebhookKind::Paid => {
                let provider_tx_id = t
                    .provider_transaction_id
                    .as_deref()
                    .or(order.provider_order_ref.as_deref())
                    .filter(|s| !s.trim().is_empty())
                    .ok_or(AuthError::BillingOperationFailed)?;
                let (grant_kind, qty, plan_code_opt) = match order.product {
                    CheckoutProduct::CreditPack25 => ("credit_pack_25", 25_i32, None),
                    CheckoutProduct::PayPerFile => ("pay_per_file", 1_i32, None),
                    CheckoutProduct::IndividualMonthly => {
                        ("subscription", 1_i32, Some("individual_monthly"))
                    }
                    CheckoutProduct::IndividualAnnual => {
                        ("subscription", 1_i32, Some("individual_annual"))
                    }
                    CheckoutProduct::OrganizationSeatAnnual => (
                        "subscription",
                        i32::try_from(order.seats.max(1)).unwrap_or(3),
                        Some("organization_seat"),
                    ),
                };

                let period_end = t.period_end_unix.or_else(|| match order.product {
                    CheckoutProduct::IndividualMonthly => Some(t.now_secs + 30 * 86_400),
                    CheckoutProduct::IndividualAnnual | CheckoutProduct::OrganizationSeatAnnual => {
                        Some(t.now_secs + 365 * 86_400)
                    }
                    CheckoutProduct::CreditPack25 | CheckoutProduct::PayPerFile => None,
                });

                // Step 5: Insert UNIQUE financial grant row!
                // Even if the same provider transaction arrives under a NEW webhook event_id,
                // `UNIQUE (provider, provider_transaction_id)` & `UNIQUE (order_id, grant_kind)` block double grants!
                let inserted_grant: Option<Uuid> = sqlx::query_scalar(
                    r"
                    INSERT INTO billing_entitlement_grants (
                        order_id, workspace_id, user_id, provider, provider_transaction_id,
                        grant_kind, quantity, plan_code, period_end
                    )
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, CASE WHEN $9::BIGINT IS NOT NULL THEN to_timestamp($9::BIGINT) ELSE NULL END)
                    ON CONFLICT DO NOTHING
                    RETURNING id
                    ",
                )
                .bind(order.id)
                .bind(order.workspace_id)
                .bind(order.user_id)
                .bind(provider_str)
                .bind(provider_tx_id)
                .bind(grant_kind)
                .bind(qty)
                .bind(plan_code_opt)
                .bind(period_end)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|_| AuthError::BackendUnavailable)?;

                if inserted_grant.is_none() {
                    sqlx::query(
                        r"
                        UPDATE billing_webhook_events
                        SET processing_status = 'ignored',
                            processed_at = now(),
                            order_id = $2,
                            provider_transaction_id = $3,
                            workspace_id = $4,
                            outcome = 'duplicate_transaction_grant_ignored'
                        WHERE id = $1
                        ",
                    )
                    .bind(webhook_row_id)
                    .bind(order.id)
                    .bind(provider_tx_id)
                    .bind(order.workspace_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| AuthError::BackendUnavailable)?;
                    tx.commit()
                        .await
                        .map_err(|_| AuthError::BackendUnavailable)?;
                    return Ok((
                        BillingEventRecord {
                            id: webhook_row_id,
                            provider: t.provider,
                            external_event_id: t.provider_event_id.clone(),
                            event_type: t.event_type.clone(),
                            workspace_id: Some(order.workspace_id),
                            occurred_at: t.occurred_at_unix,
                            processed_at: t.now_secs,
                            outcome: "duplicate_transaction_grant_ignored".to_owned(),
                        },
                        None,
                        None,
                    ));
                }

                // Record invoice uniquely bound to order + provider transaction.
                let ext_inv = t
                    .external_invoice_id
                    .clone()
                    .unwrap_or_else(|| format!("inv_{}_{}", provider_str, provider_tx_id));
                sqlx::query(
                    r"
                    INSERT INTO billing_invoices (
                        workspace_id, provider, external_invoice_id, amount_minor, currency,
                        status, order_id, provider_transaction_id
                    )
                    VALUES ($1, $2, $3, $4, $5, 'paid', $6, $7)
                    ON CONFLICT DO NOTHING
                    ",
                )
                .bind(order.workspace_id)
                .bind(provider_str)
                .bind(&ext_inv)
                .bind(i64::try_from(order.expected_amount_minor).unwrap_or(0))
                .bind(currency_to_str(order.currency))
                .bind(order.id)
                .bind(provider_tx_id)
                .execute(&mut *tx)
                .await
                .map_err(|_| AuthError::BackendUnavailable)?;

                // Update internal order state -> `paid`.
                sqlx::query(
                    r"
                    UPDATE billing_orders
                    SET state = 'paid',
                        provider_transaction_id = $2,
                        paid_at = to_timestamp($3),
                        updated_at = now()
                    WHERE id = $1
                    ",
                )
                .bind(order.id)
                .bind(provider_tx_id)
                .bind(t.now_secs)
                .execute(&mut *tx)
                .await
                .map_err(|_| AuthError::BackendUnavailable)?;

                // Commit coupon reservation if attached to this order.
                if let Some(res_id) = order.coupon_reservation_id {
                    let _ = sqlx::query(
                        "UPDATE coupon_reservations SET status = 'committed', updated_at = now() WHERE id = $1 AND status = 'reserved'",
                    )
                    .bind(res_id)
                    .execute(&mut *tx)
                    .await;
                    committed_coupon_res = Some(res_id);
                }

                // Apply entitlement transition atomically inside the SAME DB transaction!
                match order.product {
                    CheckoutProduct::CreditPack25 => {
                        ent.operation_credits = ent.operation_credits.saturating_add(25);
                        outcome = if t.provider == BillingProvider::Paymob {
                            "paymob_credits_granted_25".to_owned()
                        } else {
                            "credits_granted_25".to_owned()
                        };
                    }
                    CheckoutProduct::PayPerFile => {
                        ent.pay_per_file_tokens = ent.pay_per_file_tokens.saturating_add(1);
                        outcome = if t.provider == BillingProvider::Paymob {
                            "paymob_pay_per_file_granted".to_owned()
                        } else {
                            "pay_per_file_granted".to_owned()
                        };
                    }
                    CheckoutProduct::IndividualMonthly => {
                        ent.plan = PlanTier::IndividualMonthly;
                        ent.state = SubscriptionState::Active;
                        ent.seats = 1;
                        ent.current_period_end = period_end;
                        ent.past_due_since = None;
                        outcome = if t.provider == BillingProvider::Paymob {
                            "paymob_subscription_active".to_owned()
                        } else {
                            "subscription_active".to_owned()
                        };
                    }
                    CheckoutProduct::IndividualAnnual => {
                        ent.plan = PlanTier::IndividualAnnual;
                        ent.state = SubscriptionState::Active;
                        ent.seats = 1;
                        ent.current_period_end = period_end;
                        ent.past_due_since = None;
                        outcome = if t.provider == BillingProvider::Paymob {
                            "paymob_subscription_active".to_owned()
                        } else {
                            "subscription_active".to_owned()
                        };
                    }
                    CheckoutProduct::OrganizationSeatAnnual => {
                        ent.plan = PlanTier::OrganizationSeat;
                        ent.state = SubscriptionState::Active;
                        ent.seats = order.seats.max(3);
                        ent.current_period_end = period_end;
                        ent.past_due_since = None;
                        outcome = if t.provider == BillingProvider::Paymob {
                            "paymob_subscription_active".to_owned()
                        } else {
                            "subscription_active".to_owned()
                        };
                    }
                }
                updated_entitlement = Some(ent.clone());
            }
            VerifiedWebhookKind::PendingNoGrant => {
                if matches!(
                    order.state,
                    PaymentOrderState::Created | PaymentOrderState::CheckoutPending
                ) {
                    sqlx::query(
                        "UPDATE billing_orders SET state = 'payment_pending', updated_at = now() WHERE id = $1",
                    )
                    .bind(order.id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| AuthError::BackendUnavailable)?;
                }
                outcome = "payment_pending_no_grant".to_owned();
            }
            VerifiedWebhookKind::Failed => {
                if !matches!(order.state, PaymentOrderState::Paid) {
                    sqlx::query(
                        "UPDATE billing_orders SET state = 'failed', updated_at = now() WHERE id = $1",
                    )
                    .bind(order.id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| AuthError::BackendUnavailable)?;
                }
                outcome = "payment_failed_no_grant".to_owned();
            }
            VerifiedWebhookKind::PastDue => {
                ent.state = SubscriptionState::PastDue;
                if ent.past_due_since.is_none() {
                    ent.past_due_since = Some(t.now_secs);
                }
                updated_entitlement = Some(ent.clone());
                outcome = "subscription_past_due_grace_started".to_owned();
            }
            VerifiedWebhookKind::Canceled => {
                ent.state = SubscriptionState::Canceled;
                ent.current_period_end = t.period_end_unix.or(ent.current_period_end);
                updated_entitlement = Some(ent.clone());
                if !matches!(order.state, PaymentOrderState::Paid) {
                    sqlx::query(
                        "UPDATE billing_orders SET state = 'canceled', updated_at = now() WHERE id = $1",
                    )
                    .bind(order.id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| AuthError::BackendUnavailable)?;
                }
                outcome = "subscription_canceled".to_owned();
            }
            VerifiedWebhookKind::Refunded {
                partial_amount_minor,
                ..
            } => {
                let is_partial = partial_amount_minor
                    .is_some_and(|amt| amt > 0 && amt < order.expected_amount_minor);
                let new_order_state = if is_partial {
                    "partially_refunded"
                } else {
                    "refunded"
                };
                sqlx::query(
                    "UPDATE billing_orders SET state = $2, updated_at = now() WHERE id = $1",
                )
                .bind(order.id)
                .bind(new_order_state)
                .execute(&mut *tx)
                .await
                .map_err(|_| AuthError::BackendUnavailable)?;
                let refunded_amt = partial_amount_minor.unwrap_or(order.expected_amount_minor);
                sqlx::query(
                    r"
                    UPDATE billing_invoices
                    SET status = $2,
                        refunded_amount_minor = LEAST(amount_minor, refunded_amount_minor + $3),
                        refunded_at = to_timestamp($4)
                    WHERE order_id = $1 OR external_invoice_id = $5
                    ",
                )
                .bind(order.id)
                .bind(new_order_state)
                .bind(i64::try_from(refunded_amt).unwrap_or(0))
                .bind(t.now_secs)
                .bind(t.external_invoice_id.as_deref().unwrap_or(""))
                .execute(&mut *tx)
                .await
                .map_err(|_| AuthError::BackendUnavailable)?;

                if !is_partial {
                    sqlx::query(
                        "UPDATE billing_entitlement_grants SET revoked_at = to_timestamp($2) WHERE order_id = $1 AND revoked_at IS NULL",
                    )
                    .bind(order.id)
                    .bind(t.now_secs)
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| AuthError::BackendUnavailable)?;
                    match order.product {
                        CheckoutProduct::CreditPack25 => {
                            ent.operation_credits = ent.operation_credits.saturating_sub(25);
                        }
                        CheckoutProduct::PayPerFile => {
                            ent.pay_per_file_tokens = ent.pay_per_file_tokens.saturating_sub(1);
                        }
                        _ => {
                            ent.plan = PlanTier::Free;
                            ent.state = SubscriptionState::Canceled;
                            ent.seats = 1;
                            ent.current_period_end = Some(t.now_secs);
                            ent.past_due_since = None;
                        }
                    }
                    updated_entitlement = Some(ent.clone());
                }
                outcome = if is_partial {
                    "partial_refund_processed".to_owned()
                } else {
                    "refund_processed".to_owned()
                };
            }
            VerifiedWebhookKind::Chargeback => {
                sqlx::query(
                    "UPDATE billing_orders SET state = 'chargeback', updated_at = now() WHERE id = $1",
                )
                .bind(order.id)
                .execute(&mut *tx)
                .await
                .map_err(|_| AuthError::BackendUnavailable)?;
                sqlx::query(
                    r"
                    UPDATE billing_invoices
                    SET status = 'chargeback',
                        refunded_at = to_timestamp($2)
                    WHERE order_id = $1 OR external_invoice_id = $3
                    ",
                )
                .bind(order.id)
                .bind(t.now_secs)
                .bind(t.external_invoice_id.as_deref().unwrap_or(""))
                .execute(&mut *tx)
                .await
                .map_err(|_| AuthError::BackendUnavailable)?;
                sqlx::query(
                    "UPDATE billing_entitlement_grants SET revoked_at = to_timestamp($2) WHERE order_id = $1 AND revoked_at IS NULL",
                )
                .bind(order.id)
                .bind(t.now_secs)
                .execute(&mut *tx)
                .await
                .map_err(|_| AuthError::BackendUnavailable)?;
                ent.plan = PlanTier::Free;
                ent.state = SubscriptionState::Canceled;
                ent.seats = 1;
                ent.current_period_end = Some(t.now_secs);
                ent.past_due_since = None;
                updated_entitlement = Some(ent.clone());
                outcome = "chargeback_revoked".to_owned();
            }
        }

        if let Some(ref updated) = updated_entitlement {
            sqlx::query(
                r"
                UPDATE workspace_entitlement_state
                SET plan_tier = $2,
                    subscription_state = $3,
                    seats = $4,
                    trial_expires_at = $5,
                    current_period_end = $6,
                    past_due_since = $7,
                    operation_credits = $8,
                    pay_per_file_tokens = $9,
                    updated_at = now()
                WHERE workspace_id = $1
                ",
            )
            .bind(updated.workspace_id)
            .bind(plan_to_str(updated.plan))
            .bind(sub_state_to_str(updated.state))
            .bind(i32::try_from(updated.seats.max(1)).unwrap_or(1))
            .bind(updated.trial_expires_at)
            .bind(updated.current_period_end)
            .bind(updated.past_due_since)
            .bind(i32::try_from(updated.operation_credits).unwrap_or(0))
            .bind(i32::try_from(updated.pay_per_file_tokens).unwrap_or(0))
            .execute(&mut *tx)
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
            .bind(updated.workspace_id)
            .bind(i32::try_from(updated.operation_credits).unwrap_or(0))
            .bind(i32::try_from(updated.pay_per_file_tokens).unwrap_or(0))
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        }

        // Update monotonic workspace watermark and mark webhook processed inside the SAME transaction!
        sqlx::query(
            r"
            UPDATE workspace_billing_watermarks
            SET last_event_occurred_at = GREATEST(last_event_occurred_at, $2),
                updated_at = now()
            WHERE workspace_id = $1
            ",
        )
        .bind(order.workspace_id)
        .bind(t.occurred_at_unix)
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        sqlx::query(
            r"
            UPDATE billing_webhook_events
            SET processing_status = 'processed',
                processed_at = now(),
                order_id = $2,
                provider_transaction_id = $3,
                workspace_id = $4,
                outcome = $5
            WHERE id = $1
            ",
        )
        .bind(webhook_row_id)
        .bind(order.id)
        .bind(t.provider_transaction_id.as_deref())
        .bind(order.workspace_id)
        .bind(&outcome)
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        Ok((
            BillingEventRecord {
                id: webhook_row_id,
                provider: t.provider,
                external_event_id: t.provider_event_id.clone(),
                event_type: t.event_type.clone(),
                workspace_id: Some(order.workspace_id),
                occurred_at: t.occurred_at_unix,
                processed_at: t.now_secs,
                outcome,
            },
            updated_entitlement,
            committed_coupon_res,
        ))
    }

    /// Read authoritative workspace entitlement state directly from PostgreSQL 18.
    pub async fn load_workspace_entitlement_pg(
        &self,
        workspace_id: Uuid,
    ) -> Result<Option<WorkspaceEntitlementRecord>, AuthError> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
            .execute(&mut *tx)
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
            ",
        )
        .bind(workspace_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        let Some(ent_row) = row else {
            return Ok(None);
        };
        Ok(Some(WorkspaceEntitlementRecord {
            workspace_id: ent_row
                .try_get("workspace_id")
                .map_err(|_| AuthError::BackendUnavailable)?,
            user_id: ent_row
                .try_get("user_id")
                .map_err(|_| AuthError::BackendUnavailable)?,
            plan: str_to_plan(
                &ent_row
                    .try_get::<String, _>("plan_tier")
                    .unwrap_or_else(|_| "free".to_owned()),
            ),
            state: str_to_sub_state(
                &ent_row
                    .try_get::<String, _>("subscription_state")
                    .unwrap_or_else(|_| "free".to_owned()),
            ),
            seats: u32::try_from(ent_row.try_get::<i32, _>("seats").unwrap_or(1)).unwrap_or(1),
            trial_expires_at: ent_row.try_get("trial_expires_at").ok().flatten(),
            current_period_end: ent_row.try_get("current_period_end").ok().flatten(),
            past_due_since: ent_row.try_get("past_due_since").ok().flatten(),
            friend_vip_active: ent_row.try_get("friend_vip_active").unwrap_or(false),
            friend_vip_granted_by: ent_row.try_get("friend_vip_granted_by").ok().flatten(),
            operation_credits: u32::try_from(
                ent_row.try_get::<i32, _>("operation_credits").unwrap_or(0),
            )
            .unwrap_or(0),
            pay_per_file_tokens: u32::try_from(
                ent_row
                    .try_get::<i32, _>("pay_per_file_tokens")
                    .unwrap_or(0),
            )
            .unwrap_or(0),
            trial_ai_used: u32::try_from(ent_row.try_get::<i32, _>("trial_ai_used").unwrap_or(0))
                .unwrap_or(0),
        }))
    }

    /// List workspace invoices directly from PostgreSQL 18.
    pub async fn list_workspace_invoices_pg(
        &self,
        workspace_id: Uuid,
    ) -> Result<Vec<InvoiceRecord>, AuthError> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        let rows = sqlx::query(
            r"
            SELECT
                id, workspace_id, provider, external_invoice_id, amount_minor, currency, status,
                order_id, provider_transaction_id, refunded_amount_minor,
                EXTRACT(EPOCH FROM issued_at)::BIGINT AS issued_at_secs,
                EXTRACT(EPOCH FROM refunded_at)::BIGINT AS refunded_at_secs
            FROM billing_invoices
            WHERE workspace_id = $1
            ORDER BY issued_at DESC
            ",
        )
        .bind(workspace_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        Ok(rows
            .into_iter()
            .filter_map(|r| row_to_invoice(&r).ok())
            .collect())
    }

    /// List all invoices across the platform directly from PostgreSQL 18 (Owner Control Plane).
    pub async fn list_all_invoices_pg(&self) -> Result<Vec<InvoiceRecord>, AuthError> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        let rows = sqlx::query(
            r"
            SELECT
                id, workspace_id, provider, external_invoice_id, amount_minor, currency, status,
                order_id, provider_transaction_id, refunded_amount_minor,
                EXTRACT(EPOCH FROM issued_at)::BIGINT AS issued_at_secs,
                EXTRACT(EPOCH FROM refunded_at)::BIGINT AS refunded_at_secs
            FROM billing_invoices
            ORDER BY issued_at DESC
            ",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        Ok(rows
            .into_iter()
            .filter_map(|r| row_to_invoice(&r).ok())
            .collect())
    }

    /// Fetch a single invoice by `external_invoice_id` directly from PostgreSQL 18.
    pub async fn get_invoice_by_external_id_pg(
        &self,
        external_invoice_id: &str,
    ) -> Result<Option<InvoiceRecord>, AuthError> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        let row = sqlx::query(
            r"
            SELECT
                id, workspace_id, provider, external_invoice_id, amount_minor, currency, status,
                order_id, provider_transaction_id, refunded_amount_minor,
                EXTRACT(EPOCH FROM issued_at)::BIGINT AS issued_at_secs,
                EXTRACT(EPOCH FROM refunded_at)::BIGINT AS refunded_at_secs
            FROM billing_invoices
            WHERE external_invoice_id = $1
            ",
        )
        .bind(external_invoice_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        match row {
            Some(r) => Ok(Some(row_to_invoice(&r)?)),
            None => Ok(None),
        }
    }

    /// List recent billing webhook events directly from PostgreSQL 18.
    pub async fn list_billing_events_pg(
        &self,
        limit: usize,
    ) -> Result<Vec<BillingEventRecord>, AuthError> {
        let rows = sqlx::query(
            r"
            SELECT
                id, provider, provider_event_id,
                COALESCE(event_type, 'webhook') AS event_type,
                workspace_id,
                COALESCE(EXTRACT(EPOCH FROM occurred_at)::BIGINT, EXTRACT(EPOCH FROM received_at)::BIGINT) AS occurred_at_secs,
                COALESCE(EXTRACT(EPOCH FROM processed_at)::BIGINT, EXTRACT(EPOCH FROM received_at)::BIGINT) AS processed_at_secs,
                COALESCE(outcome, processing_status) AS outcome
            FROM billing_webhook_events
            ORDER BY received_at DESC
            LIMIT $1
            ",
        )
        .bind(i64::try_from(limit.clamp(1, 200)).unwrap_or(100))
        .fetch_all(self.pool())
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let prov: String = row.try_get("provider").unwrap_or_default();
            out.push(BillingEventRecord {
                id: row.try_get("id").unwrap_or_else(|_| Uuid::now_v7()),
                provider: str_to_provider(&prov),
                external_event_id: row.try_get("provider_event_id").unwrap_or_default(),
                event_type: row.try_get("event_type").unwrap_or_default(),
                workspace_id: row.try_get("workspace_id").ok().flatten(),
                occurred_at: row.try_get::<i64, _>("occurred_at_secs").unwrap_or(0),
                processed_at: row.try_get::<i64, _>("processed_at_secs").unwrap_or(0),
                outcome: row.try_get("outcome").unwrap_or_default(),
            });
        }
        Ok(out)
    }

    /// Compute authoritative revenue summary directly from PostgreSQL 18 `billing_invoices`.
    pub async fn revenue_summary_pg(&self) -> Result<BillingRevenueSummary, AuthError> {
        let invoices = self.list_all_invoices_pg().await?;
        let mut summary = BillingRevenueSummary::default();
        for inv in invoices {
            summary.total_invoices += 1;
            match inv.status.as_str() {
                "paid" | "refund_pending" | "partially_refunded" => {
                    summary.paid_invoices += 1;
                    match inv.currency {
                        BillingCurrency::Usd => {
                            summary.gross_usd_minor += inv.amount_minor;
                            summary.refunded_usd_minor += inv.refunded_amount_minor;
                        }
                        BillingCurrency::Egp => {
                            summary.gross_egp_minor += inv.amount_minor;
                            summary.refunded_egp_minor += inv.refunded_amount_minor;
                        }
                    }
                }
                "refunded" => {
                    summary.refunded_invoices += 1;
                    match inv.currency {
                        BillingCurrency::Usd => summary.refunded_usd_minor += inv.amount_minor,
                        BillingCurrency::Egp => summary.refunded_egp_minor += inv.amount_minor,
                    }
                }
                "chargeback" => {
                    summary.chargeback_invoices += 1;
                    match inv.currency {
                        BillingCurrency::Usd => summary.chargeback_usd_minor += inv.amount_minor,
                        BillingCurrency::Egp => summary.chargeback_egp_minor += inv.amount_minor,
                    }
                }
                _ => {}
            }
        }
        Ok(summary)
    }

    /// Atomically claim a refund request slot in PostgreSQL (`UNIQUE (workspace_id, refund_request_key)`).
    /// Prevents duplicate refund clicks from submitting a second external provider refund!
    pub async fn claim_refund_request_pg(
        &self,
        rec: &RefundRecord,
    ) -> Result<Option<RefundRecord>, AuthError> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        let inserted: Option<Uuid> = sqlx::query_scalar(
            r"
            INSERT INTO billing_refunds (
                id, order_id, workspace_id, provider, external_invoice_id,
                refund_request_key, amount_minor, currency, kind, status,
                mandatory_legal_override, reason, requested_by
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'requested', $10, $11, $12)
            ON CONFLICT (workspace_id, refund_request_key) DO NOTHING
            RETURNING id
            ",
        )
        .bind(rec.id)
        .bind(rec.order_id)
        .bind(rec.workspace_id)
        .bind(provider_to_str(rec.provider))
        .bind(&rec.external_invoice_id)
        .bind(&rec.refund_request_key)
        .bind(i64::try_from(rec.amount_minor).unwrap_or(0))
        .bind(currency_to_str(rec.currency))
        .bind(&rec.kind)
        .bind(rec.mandatory_legal_override)
        .bind(&rec.reason)
        .bind(rec.requested_by)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        if inserted.is_none() {
            // Already submitted with this exact idempotency key -> return None so caller does NOT call provider again!
            tx.commit()
                .await
                .map_err(|_| AuthError::BackendUnavailable)?;
            return Ok(None);
        }

        // Mark invoice `refund_pending` while waiting for provider confirmation (entitlements remain intact!).
        sqlx::query(
            "UPDATE billing_invoices SET status = 'refund_pending' WHERE external_invoice_id = $1 AND status = 'paid'",
        )
        .bind(&rec.external_invoice_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        Ok(Some(rec.clone()))
    }

    /// Finalize a refund request after external provider response (`confirmed = true` -> revokes
    /// entitlements atomically; `confirmed = false` -> keeps `refund_pending` until webhook confirms!).
    #[allow(clippy::too_many_arguments)]
    pub async fn finalize_refund_request_pg(
        &self,
        refund_id: Uuid,
        external_invoice_id: &str,
        workspace_id: Uuid,
        provider_refund_id: Option<&str>,
        confirmed: bool,
        partial_amount_minor: Option<u64>,
        failed: bool,
        now_secs: i64,
    ) -> Result<(InvoiceRecord, Option<WorkspaceEntitlementRecord>), AuthError> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        if failed {
            sqlx::query(
                "UPDATE billing_refunds SET status = 'failed', updated_at = now() WHERE id = $1",
            )
            .bind(refund_id)
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
            sqlx::query(
                "UPDATE billing_invoices SET status = 'paid' WHERE external_invoice_id = $1 AND status = 'refund_pending'",
            )
            .bind(external_invoice_id)
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
            tx.commit()
                .await
                .map_err(|_| AuthError::BackendUnavailable)?;
            return Err(AuthError::BillingOperationFailed);
        }

        let refund_status = if confirmed { "confirmed" } else { "requested" };
        sqlx::query(
            r"
            UPDATE billing_refunds
            SET provider_refund_id = COALESCE($2, provider_refund_id),
                status = $3,
                confirmed_at = CASE WHEN $4 THEN to_timestamp($5) ELSE confirmed_at END,
                updated_at = now()
            WHERE id = $1
            ",
        )
        .bind(refund_id)
        .bind(provider_refund_id)
        .bind(refund_status)
        .bind(confirmed)
        .bind(now_secs)
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        let inv_row = sqlx::query(
            r"
            SELECT
                id, workspace_id, provider, external_invoice_id, amount_minor, currency, status,
                order_id, provider_transaction_id, refunded_amount_minor,
                EXTRACT(EPOCH FROM issued_at)::BIGINT AS issued_at_secs,
                EXTRACT(EPOCH FROM refunded_at)::BIGINT AS refunded_at_secs
            FROM billing_invoices
            WHERE external_invoice_id = $1
            FOR UPDATE
            ",
        )
        .bind(external_invoice_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| AuthError::BillingOperationFailed)?;
        let mut inv = row_to_invoice(&inv_row)?;

        let mut updated_ent = None;
        if confirmed {
            let is_partial =
                partial_amount_minor.is_some_and(|amt| amt > 0 && amt < inv.amount_minor);
            let new_status = if is_partial {
                "partially_refunded"
            } else {
                "refunded"
            };
            let add_refund = partial_amount_minor.unwrap_or(inv.amount_minor);
            inv.status = new_status.to_owned();
            inv.refunded_amount_minor = inv
                .refunded_amount_minor
                .saturating_add(add_refund)
                .min(inv.amount_minor);
            inv.refunded_at = Some(now_secs);

            sqlx::query(
                r"
                UPDATE billing_invoices
                SET status = $2,
                    refunded_amount_minor = $3,
                    refunded_at = to_timestamp($4)
                WHERE external_invoice_id = $1
                ",
            )
            .bind(external_invoice_id)
            .bind(new_status)
            .bind(i64::try_from(inv.refunded_amount_minor).unwrap_or(0))
            .bind(now_secs)
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

            if let Some(oid) = inv.order_id {
                sqlx::query(
                    "UPDATE billing_orders SET state = $2, updated_at = now() WHERE id = $1",
                )
                .bind(oid)
                .bind(new_status)
                .execute(&mut *tx)
                .await
                .map_err(|_| AuthError::BackendUnavailable)?;
            }

            if !is_partial {
                sqlx::query(
                    r"
                    UPDATE workspace_entitlement_state
                    SET plan_tier = 'free',
                        subscription_state = 'canceled',
                        seats = 1,
                        current_period_end = $2,
                        past_due_since = NULL,
                        updated_at = now()
                    WHERE workspace_id = $1
                    ",
                )
                .bind(workspace_id)
                .bind(now_secs)
                .execute(&mut *tx)
                .await
                .map_err(|_| AuthError::BackendUnavailable)?;
                updated_ent = Some(WorkspaceEntitlementRecord {
                    workspace_id,
                    user_id: Uuid::nil(),
                    plan: PlanTier::Free,
                    state: SubscriptionState::Canceled,
                    seats: 1,
                    trial_expires_at: None,
                    current_period_end: Some(now_secs),
                    past_due_since: None,
                    friend_vip_active: false,
                    friend_vip_granted_by: None,
                    operation_credits: 0,
                    pay_per_file_tokens: 0,
                    trial_ai_used: 0,
                });
            }
        } else {
            inv.status = "refund_pending".to_owned();
        }

        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        Ok((inv, updated_ent))
    }

    /// Record a manual Owner chargeback atomically in PostgreSQL 18.
    pub async fn record_chargeback_pg(
        &self,
        external_invoice_id: &str,
        workspace_id: Uuid,
        now_secs: i64,
    ) -> Result<InvoiceRecord, AuthError> {
        let mut tx = self
            .pool()
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.owner_override', 'true', true)")
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;

        sqlx::query(
            r"
            INSERT INTO billing_invoices (
                workspace_id, provider, external_invoice_id, amount_minor, currency, status, refunded_at
            )
            VALUES ($1, 'manual', $2, 0, 'USD', 'chargeback', to_timestamp($3))
            ON CONFLICT (external_invoice_id) DO UPDATE
            SET status = 'chargeback',
                refunded_at = to_timestamp($3)
            ",
        )
        .bind(workspace_id)
        .bind(external_invoice_id)
        .bind(now_secs)
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        sqlx::query(
            r"
            UPDATE billing_orders
            SET state = 'chargeback', updated_at = now()
            WHERE id = (SELECT order_id FROM billing_invoices WHERE external_invoice_id = $1)
            ",
        )
        .bind(external_invoice_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        sqlx::query(
            r"
            UPDATE workspace_entitlement_state
            SET plan_tier = 'free',
                subscription_state = 'canceled',
                seats = 1,
                current_period_end = $2,
                past_due_since = NULL,
                updated_at = now()
            WHERE workspace_id = $1
            ",
        )
        .bind(workspace_id)
        .bind(now_secs)
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        let row = sqlx::query(
            r"
            SELECT
                id, workspace_id, provider, external_invoice_id, amount_minor, currency, status,
                order_id, provider_transaction_id, refunded_amount_minor,
                EXTRACT(EPOCH FROM issued_at)::BIGINT AS issued_at_secs,
                EXTRACT(EPOCH FROM refunded_at)::BIGINT AS refunded_at_secs
            FROM billing_invoices
            WHERE external_invoice_id = $1
            ",
        )
        .bind(external_invoice_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
        let inv = row_to_invoice(&row)?;

        let payload_hash = Sha256::digest(external_invoice_id.as_bytes());
        sqlx::query(
            r"
            INSERT INTO billing_webhook_events (
                provider, provider_event_id, payload_hash, processing_status,
                event_type, workspace_id, occurred_at, processed_at, outcome
            )
            VALUES ('manual', $1, $2, 'processed', 'chargeback.recorded', $3, to_timestamp($4), now(), 'chargeback_revoked')
            ON CONFLICT (provider, provider_event_id) DO NOTHING
            ",
        )
        .bind(format!("chg_{}", Uuid::now_v7().simple()))
        .bind(payload_hash.as_slice())
        .bind(workspace_id)
        .bind(now_secs)
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;

        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        Ok(inv)
    }
}

fn row_to_invoice(row: &sqlx::postgres::PgRow) -> Result<InvoiceRecord, AuthError> {
    let prov: String = row
        .try_get("provider")
        .map_err(|_| AuthError::BackendUnavailable)?;
    let curr: String = row
        .try_get("currency")
        .map_err(|_| AuthError::BackendUnavailable)?;
    Ok(InvoiceRecord {
        id: row
            .try_get("id")
            .map_err(|_| AuthError::BackendUnavailable)?,
        workspace_id: row
            .try_get("workspace_id")
            .map_err(|_| AuthError::BackendUnavailable)?,
        provider: str_to_provider(&prov),
        external_invoice_id: row
            .try_get("external_invoice_id")
            .map_err(|_| AuthError::BackendUnavailable)?,
        order_id: row.try_get("order_id").ok().flatten(),
        provider_transaction_id: row.try_get("provider_transaction_id").ok().flatten(),
        amount_minor: u64::try_from(row.try_get::<i64, _>("amount_minor").unwrap_or(0))
            .unwrap_or(0),
        refunded_amount_minor: u64::try_from(
            row.try_get::<i64, _>("refunded_amount_minor").unwrap_or(0),
        )
        .unwrap_or(0),
        currency: str_to_currency(&curr),
        status: row
            .try_get("status")
            .map_err(|_| AuthError::BackendUnavailable)?,
        issued_at: row.try_get::<i64, _>("issued_at_secs").unwrap_or(0),
        refunded_at: row
            .try_get::<Option<i64>, _>("refunded_at_secs")
            .ok()
            .flatten(),
    })
}
