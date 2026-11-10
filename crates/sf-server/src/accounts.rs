//! Customer account and subscription administration (Phases 0.1 and 0.3).
//!
//! Every read or write across workspaces requires an [`OwnerGrant`], which only
//! `authorize_owner_grant` can mint for `mo7amed.rabei14@gmail.com`. Subscription
//! lifecycle operations update the subscription, append its history event, and
//! sync the legacy workspace label inside one transaction.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgPool, PgRow};
use sqlx::Row;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::auth::identity::{authorize_owner_grant, OwnerGrant, OwnerOnlyAction};
use crate::error::AuthError;

/// Subscription statuses that count as a workspace's one current subscription.
pub const CURRENT_SUBSCRIPTION_STATUSES: [&str; 4] =
    ["trialing", "active", "past_due", "grace_period"];

const SECONDS_PER_DAY: i64 = 86_400;

/// Account lifecycle status derived from soft-delete and suspension timestamps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountStatus {
    /// The customer can sign in and use the product.
    Active,
    /// The owner suspended the account; nothing is deleted.
    Suspended,
    /// The customer requested deletion and is inside the soft-delete window.
    Deleted,
}

impl AccountStatus {
    /// Canonical stored/API name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Suspended => "suspended",
            Self::Deleted => "deleted",
        }
    }

    /// Resolve the status from the two nullable timestamps.
    #[must_use]
    pub fn from_timestamps(suspended_at: Option<i64>, deleted_at: Option<i64>) -> Self {
        if deleted_at.is_some() {
            Self::Deleted
        } else if suspended_at.is_some() {
            Self::Suspended
        } else {
            Self::Active
        }
    }
}

/// One customer row for the owner console: identity, workspace, and plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerAccount {
    pub user_id: Uuid,
    pub email: String,
    pub display_name: String,
    pub status: AccountStatus,
    pub email_verified: bool,
    pub workspace_id: Uuid,
    pub workspace_name: String,
    pub role: String,
    pub plan_code: String,
    pub subscription_status: String,
    pub current_period_end: Option<i64>,
}

/// Subscription lifecycle actions recorded in the append-only history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionEventType {
    /// A subscription was provisioned.
    Created,
    /// A paid period renewed.
    Renewed,
    /// Plan changed, upgraded, or downgraded.
    PlanChanged,
    /// The customer or owner canceled the subscription.
    Canceled,
    /// A canceled subscription was reactivated.
    Reactivated,
    /// The period elapsed without renewal.
    Expired,
    /// The owner issued a refund.
    Refunded,
    /// The trial period was extended.
    TrialExtended,
}

impl SubscriptionEventType {
    /// Canonical stored name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Renewed => "renewed",
            Self::PlanChanged => "plan_changed",
            Self::Canceled => "canceled",
            Self::Reactivated => "reactivated",
            Self::Expired => "expired",
            Self::Refunded => "refunded",
            Self::TrialExtended => "trial_extended",
        }
    }
}

/// One append-only subscription event to persist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionEventDraft {
    pub workspace_id: Uuid,
    pub subscription_id: Option<Uuid>,
    pub event_type: SubscriptionEventType,
    pub from_status: Option<String>,
    pub to_status: Option<String>,
    pub amount_minor: Option<i32>,
    pub currency: Option<String>,
}

/// Three-letter uppercase ISO currency, the only shape the database accepts.
fn valid_currency(currency: &str) -> bool {
    currency.len() == 3 && currency.bytes().all(|byte| byte.is_ascii_uppercase())
}

impl SubscriptionEventDraft {
    fn validate(&self) -> Result<(), AuthError> {
        if let Some(amount) = self.amount_minor {
            if amount < 0 {
                return Err(AuthError::InvalidRequest);
            }
        }
        if let Some(currency) = self.currency.as_deref() {
            if !valid_currency(currency) {
                return Err(AuthError::InvalidRequest);
            }
        }
        Ok(())
    }
}

/// A plan catalog row as seen by the subscription lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanRecord {
    pub code: String,
    pub status: String,
    pub billing_interval: String,
}

/// The workspace's current subscription.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subscription {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub plan_code: String,
    pub status: String,
    pub current_period_start: i64,
    pub current_period_end: Option<i64>,
    pub cancel_at_period_end: bool,
    pub canceled_at: Option<i64>,
}

impl Subscription {
    /// Whether this row is the workspace's live subscription.
    #[must_use]
    pub fn is_current(&self) -> bool {
        CURRENT_SUBSCRIPTION_STATUSES.contains(&self.status.as_str())
    }
}

/// Final state applied by one atomic subscription operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionChange {
    pub plan_code: String,
    pub status: String,
    pub current_period_start: i64,
    pub current_period_end: Option<i64>,
    pub cancel_at_period_end: bool,
    pub canceled_at: Option<i64>,
    pub event_type: SubscriptionEventType,
    pub from_status: Option<String>,
    pub to_status: String,
    pub amount_minor: Option<i32>,
    pub currency: Option<String>,
    /// Recorded in event metadata when the operation is a cancellation.
    pub at_period_end: Option<bool>,
}

impl SubscriptionChange {
    fn event_metadata_json(&self) -> String {
        match self.at_period_end {
            Some(flag) => format!(r#"{{"at_period_end":{flag}}}"#),
            None => "{}".to_owned(),
        }
    }
}

/// One event as stored, exposed for tests and the owner history view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedSubscriptionEvent {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub subscription_id: Option<Uuid>,
    pub event_type: SubscriptionEventType,
    pub from_status: Option<String>,
    pub to_status: Option<String>,
    pub amount_minor: Option<i32>,
    pub currency: Option<String>,
    pub at_period_end: Option<bool>,
}

/// Persistence contract for owner-account administration.
#[async_trait]
pub trait CustomerAccountRepository: Send + Sync {
    /// List every customer ordered by newest first.
    async fn list_accounts(&self, grant: &OwnerGrant) -> Result<Vec<CustomerAccount>, AuthError>;
    /// Suspend one account without deleting its data.
    async fn suspend_account(
        &self,
        grant: &OwnerGrant,
        user_id: Uuid,
        at: i64,
    ) -> Result<(), AuthError>;
    /// Restore a suspended account.
    async fn restore_account(&self, grant: &OwnerGrant, user_id: Uuid) -> Result<(), AuthError>;
    /// Append a subscription lifecycle event.
    async fn append_subscription_event(
        &self,
        grant: &OwnerGrant,
        draft: &SubscriptionEventDraft,
        at: i64,
    ) -> Result<Uuid, AuthError>;
    /// Read one plan from the catalog.
    async fn find_plan(
        &self,
        grant: &OwnerGrant,
        plan_code: &str,
    ) -> Result<Option<PlanRecord>, AuthError>;
    /// Read the workspace's current subscription, if it has one.
    async fn current_subscription(
        &self,
        grant: &OwnerGrant,
        workspace_id: Uuid,
    ) -> Result<Option<Subscription>, AuthError>;
    /// Apply the mutation, its history event, and the workspace label atomically.
    async fn apply_subscription_change(
        &self,
        grant: &OwnerGrant,
        workspace_id: Uuid,
        change: &SubscriptionChange,
        at: i64,
    ) -> Result<Subscription, AuthError>;
}

/// Owner-gated facade over the customer account repository.
pub struct CustomerAccountService {
    repository: Arc<dyn CustomerAccountRepository>,
}

impl std::fmt::Debug for CustomerAccountService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CustomerAccountService")
            .finish_non_exhaustive()
    }
}

fn validate_workspace_id(workspace_id: Uuid) -> Result<(), AuthError> {
    if workspace_id.is_nil() {
        Err(AuthError::InvalidRequest)
    } else {
        Ok(())
    }
}

fn validate_operation_time(at: i64) -> Result<(), AuthError> {
    if at <= 0 {
        Err(AuthError::InvalidRequest)
    } else {
        Ok(())
    }
}

fn validate_payment(amount_minor: Option<i32>, currency: Option<&str>) -> Result<(), AuthError> {
    match (amount_minor, currency) {
        (None, None) => Ok(()),
        (Some(amount), Some(currency)) if amount >= 0 && valid_currency(currency) => Ok(()),
        _ => Err(AuthError::InvalidRequest),
    }
}

fn billing_months(billing_interval: &str) -> Option<i64> {
    match billing_interval {
        "monthly" => Some(1),
        "yearly" => Some(12),
        _ => None,
    }
}

impl CustomerAccountService {
    /// Wrap a repository.
    #[must_use]
    pub fn new(repository: Arc<dyn CustomerAccountRepository>) -> Self {
        Self { repository }
    }

    /// Owner-only: list all customer accounts.
    pub async fn list_accounts(
        &self,
        actor_email: &str,
    ) -> Result<Vec<CustomerAccount>, AuthError> {
        let grant = authorize_owner_grant(actor_email, OwnerOnlyAction::AccessOwnerDashboard)?;
        self.repository.list_accounts(&grant).await
    }

    /// Owner-only: suspend one account.
    pub async fn suspend_account(
        &self,
        actor_email: &str,
        user_id: Uuid,
        at: i64,
    ) -> Result<(), AuthError> {
        let grant = authorize_owner_grant(actor_email, OwnerOnlyAction::SuspendUser)?;
        self.repository.suspend_account(&grant, user_id, at).await
    }

    /// Owner-only: restore one suspended account.
    pub async fn restore_account(&self, actor_email: &str, user_id: Uuid) -> Result<(), AuthError> {
        let grant = authorize_owner_grant(actor_email, OwnerOnlyAction::SuspendUser)?;
        self.repository.restore_account(&grant, user_id).await
    }

    /// Owner-only: append a subscription lifecycle event.
    pub async fn record_subscription_event(
        &self,
        actor_email: &str,
        draft: &SubscriptionEventDraft,
        at: i64,
    ) -> Result<Uuid, AuthError> {
        draft.validate()?;
        let grant = authorize_owner_grant(actor_email, OwnerOnlyAction::AccessOwnerDashboard)?;
        self.repository
            .append_subscription_event(&grant, draft, at)
            .await
    }

    /// Owner-only: read the workspace's current subscription.
    pub async fn current_subscription(
        &self,
        actor_email: &str,
        workspace_id: Uuid,
    ) -> Result<Option<Subscription>, AuthError> {
        let grant = authorize_owner_grant(actor_email, OwnerOnlyAction::AccessOwnerDashboard)?;
        validate_workspace_id(workspace_id)?;
        self.repository
            .current_subscription(&grant, workspace_id)
            .await
    }

    /// Owner-only: move a workspace to another active plan.
    pub async fn change_plan(
        &self,
        actor_email: &str,
        workspace_id: Uuid,
        plan_code: &str,
        at: i64,
    ) -> Result<Subscription, AuthError> {
        let grant = authorize_owner_grant(actor_email, OwnerOnlyAction::AccessOwnerDashboard)?;
        validate_workspace_id(workspace_id)?;
        validate_operation_time(at)?;

        let plan_code = plan_code.trim().to_ascii_lowercase();
        if plan_code.is_empty() || plan_code.len() > 64 {
            return Err(AuthError::InvalidRequest);
        }
        let plan = self
            .repository
            .find_plan(&grant, &plan_code)
            .await?
            .ok_or(AuthError::InvalidRequest)?;
        if plan.status != "active" {
            return Err(AuthError::InvalidRequest);
        }
        let current = self
            .repository
            .current_subscription(&grant, workspace_id)
            .await?
            .ok_or(AuthError::InvalidRequest)?;
        if current.plan_code == plan.code {
            return Err(AuthError::InvalidRequest);
        }

        let current_period_end = match plan.billing_interval.as_str() {
            "none" => None,
            interval => {
                let months = billing_months(interval).ok_or(AuthError::InvalidRequest)?;
                Some(add_months_utc(at, months).ok_or(AuthError::InvalidRequest)?)
            }
        };

        self.repository
            .apply_subscription_change(
                &grant,
                workspace_id,
                &SubscriptionChange {
                    plan_code: plan.code,
                    status: "active".to_owned(),
                    current_period_start: at,
                    current_period_end,
                    cancel_at_period_end: false,
                    canceled_at: None,
                    event_type: SubscriptionEventType::PlanChanged,
                    from_status: Some(current.status),
                    to_status: "active".to_owned(),
                    amount_minor: None,
                    currency: None,
                    at_period_end: None,
                },
                at,
            )
            .await
    }

    /// Owner-only: extend a paid period by one billing interval.
    ///
    /// The extension starts at `max(now, current_period_end)`, so renewing early
    /// never shortens a paid period. A scheduled cancellation is cleared because
    /// a successful renewal means the customer is continuing.
    pub async fn renew_subscription(
        &self,
        actor_email: &str,
        workspace_id: Uuid,
        at: i64,
        amount_minor: Option<i32>,
        currency: Option<&str>,
    ) -> Result<Subscription, AuthError> {
        let grant = authorize_owner_grant(actor_email, OwnerOnlyAction::AccessOwnerDashboard)?;
        validate_workspace_id(workspace_id)?;
        validate_operation_time(at)?;
        validate_payment(amount_minor, currency)?;

        let current = self
            .repository
            .current_subscription(&grant, workspace_id)
            .await?
            .ok_or(AuthError::InvalidRequest)?;
        let plan = self
            .repository
            .find_plan(&grant, &current.plan_code)
            .await?
            .ok_or(AuthError::InvalidRequest)?;
        if plan.status != "active" {
            return Err(AuthError::InvalidRequest);
        }
        let months = billing_months(&plan.billing_interval).ok_or(AuthError::InvalidRequest)?;

        let extension_base = current.current_period_end.map_or(at, |end| end.max(at));
        let new_period_end =
            add_months_utc(extension_base, months).ok_or(AuthError::InvalidRequest)?;
        if new_period_end <= current.current_period_start {
            return Err(AuthError::InvalidRequest);
        }

        self.repository
            .apply_subscription_change(
                &grant,
                workspace_id,
                &SubscriptionChange {
                    plan_code: current.plan_code,
                    status: "active".to_owned(),
                    current_period_start: current.current_period_start,
                    current_period_end: Some(new_period_end),
                    cancel_at_period_end: false,
                    canceled_at: None,
                    event_type: SubscriptionEventType::Renewed,
                    from_status: Some(current.status),
                    to_status: "active".to_owned(),
                    amount_minor,
                    currency: currency.map(str::to_owned),
                    at_period_end: None,
                },
                at,
            )
            .await
    }

    /// Owner-only: cancel now or at the end of the paid period.
    ///
    /// A scheduled cancellation keeps the subscription active until the period
    /// ends and is idempotent. An immediate cancellation moves it to `canceled`,
    /// which also frees the workspace's one-current-subscription slot.
    pub async fn cancel_subscription(
        &self,
        actor_email: &str,
        workspace_id: Uuid,
        at: i64,
        at_period_end: bool,
    ) -> Result<Subscription, AuthError> {
        let grant = authorize_owner_grant(actor_email, OwnerOnlyAction::CancelSubscription)?;
        validate_workspace_id(workspace_id)?;
        validate_operation_time(at)?;

        let current = self
            .repository
            .current_subscription(&grant, workspace_id)
            .await?
            .ok_or(AuthError::InvalidRequest)?;
        if current.current_period_end.is_none() {
            return Err(AuthError::InvalidRequest);
        }
        if at_period_end && current.cancel_at_period_end {
            return Ok(current);
        }

        let (status, cancel_at_period_end, canceled_at, to_status) = if at_period_end {
            (current.status.clone(), true, None, current.status.clone())
        } else {
            (
                "canceled".to_owned(),
                false,
                Some(at),
                "canceled".to_owned(),
            )
        };

        self.repository
            .apply_subscription_change(
                &grant,
                workspace_id,
                &SubscriptionChange {
                    plan_code: current.plan_code,
                    status,
                    current_period_start: current.current_period_start,
                    current_period_end: current.current_period_end,
                    cancel_at_period_end,
                    canceled_at,
                    event_type: SubscriptionEventType::Canceled,
                    from_status: Some(current.status),
                    to_status,
                    amount_minor: None,
                    currency: None,
                    at_period_end: Some(at_period_end),
                },
                at,
            )
            .await
    }
}

/// PostgreSQL implementation. Every method requires an un-forgeable owner grant.
pub struct SqlxCustomerAccountRepository {
    pool: PgPool,
}

impl std::fmt::Debug for SqlxCustomerAccountRepository {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SqlxCustomerAccountRepository")
            .finish_non_exhaustive()
    }
}

impl SqlxCustomerAccountRepository {
    /// Wrap a connection pool.
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn backend_error() -> AuthError {
    AuthError::BackendUnavailable
}

fn subscription_from_row(row: &PgRow) -> Result<Subscription, AuthError> {
    Ok(Subscription {
        id: row.try_get("id").map_err(|_| backend_error())?,
        workspace_id: row.try_get("workspace_id").map_err(|_| backend_error())?,
        plan_code: row.try_get("plan_code").map_err(|_| backend_error())?,
        status: row.try_get("status").map_err(|_| backend_error())?,
        current_period_start: row
            .try_get("current_period_start")
            .map_err(|_| backend_error())?,
        current_period_end: row
            .try_get("current_period_end")
            .map_err(|_| backend_error())?,
        cancel_at_period_end: row
            .try_get("cancel_at_period_end")
            .map_err(|_| backend_error())?,
        canceled_at: row.try_get("canceled_at").map_err(|_| backend_error())?,
    })
}

const SUBSCRIPTION_COLUMNS: &str = r"
    id,
    workspace_id,
    plan_code,
    status,
    EXTRACT(EPOCH FROM current_period_start)::bigint AS current_period_start,
    EXTRACT(EPOCH FROM current_period_end)::bigint AS current_period_end,
    cancel_at_period_end,
    EXTRACT(EPOCH FROM canceled_at)::bigint AS canceled_at
";

const CURRENT_STATUS_SQL: &str = "('trialing', 'active', 'past_due', 'grace_period')";

#[async_trait]
impl CustomerAccountRepository for SqlxCustomerAccountRepository {
    async fn list_accounts(&self, _grant: &OwnerGrant) -> Result<Vec<CustomerAccount>, AuthError> {
        let mut transaction = self.pool.begin().await.map_err(|_| backend_error())?;
        sqlx::query("SELECT set_config('app.owner_admin', 'true', true)")
            .execute(&mut *transaction)
            .await
            .map_err(|_| backend_error())?;

        let rows = sqlx::query(
            r"
            SELECT
                users.id AS user_id,
                users.email,
                users.display_name,
                EXTRACT(EPOCH FROM users.suspended_at)::bigint AS suspended_at,
                EXTRACT(EPOCH FROM users.deleted_at)::bigint AS deleted_at,
                (users.email_verified_at IS NOT NULL) AS email_verified,
                workspaces.id AS workspace_id,
                workspaces.name AS workspace_name,
                memberships.role,
                COALESCE(current_subscription.plan_code, workspaces.plan_code) AS plan_code,
                COALESCE(current_subscription.status, 'none') AS subscription_status,
                EXTRACT(EPOCH FROM current_subscription.current_period_end)::bigint
                    AS current_period_end
            FROM users
            INNER JOIN memberships
                ON memberships.user_id = users.id AND memberships.role = 'owner'
            INNER JOIN workspaces
                ON workspaces.id = memberships.workspace_id
            LEFT JOIN LATERAL (
                SELECT plan_code, status, current_period_end
                FROM subscriptions
                WHERE subscriptions.workspace_id = workspaces.id
                  AND subscriptions.status IN ('trialing', 'active', 'past_due', 'grace_period')
                ORDER BY subscriptions.created_at DESC
                LIMIT 1
            ) AS current_subscription ON TRUE
            WHERE users.deleted_at IS NULL
            ORDER BY users.created_at DESC
            ",
        )
        .fetch_all(&mut *transaction)
        .await
        .map_err(|_| backend_error())?;

        let mut accounts = Vec::with_capacity(rows.len());
        for row in rows {
            accounts.push(CustomerAccount {
                user_id: row.try_get("user_id").map_err(|_| backend_error())?,
                email: row.try_get("email").map_err(|_| backend_error())?,
                display_name: row.try_get("display_name").map_err(|_| backend_error())?,
                status: AccountStatus::from_timestamps(
                    row.try_get("suspended_at").map_err(|_| backend_error())?,
                    row.try_get("deleted_at").map_err(|_| backend_error())?,
                ),
                email_verified: row.try_get("email_verified").map_err(|_| backend_error())?,
                workspace_id: row.try_get("workspace_id").map_err(|_| backend_error())?,
                workspace_name: row.try_get("workspace_name").map_err(|_| backend_error())?,
                role: row.try_get("role").map_err(|_| backend_error())?,
                plan_code: row.try_get("plan_code").map_err(|_| backend_error())?,
                subscription_status: row
                    .try_get("subscription_status")
                    .map_err(|_| backend_error())?,
                current_period_end: row
                    .try_get("current_period_end")
                    .map_err(|_| backend_error())?,
            });
        }

        transaction.commit().await.map_err(|_| backend_error())?;
        Ok(accounts)
    }

    async fn suspend_account(
        &self,
        _grant: &OwnerGrant,
        user_id: Uuid,
        at: i64,
    ) -> Result<(), AuthError> {
        let result = sqlx::query(
            "UPDATE users SET suspended_at = to_timestamp($2::double precision) WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(user_id)
        .bind(at as f64)
        .execute(&self.pool)
        .await
        .map_err(|_| backend_error())?;

        if result.rows_affected() == 1 {
            Ok(())
        } else {
            Err(AuthError::InvalidRequest)
        }
    }

    async fn restore_account(&self, _grant: &OwnerGrant, user_id: Uuid) -> Result<(), AuthError> {
        let result = sqlx::query(
            "UPDATE users SET suspended_at = NULL WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(|_| backend_error())?;

        if result.rows_affected() == 1 {
            Ok(())
        } else {
            Err(AuthError::InvalidRequest)
        }
    }

    async fn append_subscription_event(
        &self,
        _grant: &OwnerGrant,
        draft: &SubscriptionEventDraft,
        at: i64,
    ) -> Result<Uuid, AuthError> {
        draft.validate()?;
        let mut transaction = self.pool.begin().await.map_err(|_| backend_error())?;
        sqlx::query("SELECT set_config('app.workspace_id', $1, true)")
            .bind(draft.workspace_id.to_string())
            .execute(&mut *transaction)
            .await
            .map_err(|_| backend_error())?;

        let id = sqlx::query_scalar::<_, Uuid>(
            r"
            INSERT INTO subscription_events (
                workspace_id,
                subscription_id,
                event_type,
                actor_role,
                from_status,
                to_status,
                amount_minor,
                currency,
                created_at
            )
            VALUES ($1, $2, $3, 'owner', $4, $5, $6, $7, to_timestamp($8::double precision))
            RETURNING id
            ",
        )
        .bind(draft.workspace_id)
        .bind(draft.subscription_id)
        .bind(draft.event_type.as_str())
        .bind(draft.from_status.as_deref())
        .bind(draft.to_status.as_deref())
        .bind(draft.amount_minor)
        .bind(draft.currency.as_deref())
        .bind(at as f64)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|_| backend_error())?;

        transaction.commit().await.map_err(|_| backend_error())?;
        Ok(id)
    }

    async fn find_plan(
        &self,
        _grant: &OwnerGrant,
        plan_code: &str,
    ) -> Result<Option<PlanRecord>, AuthError> {
        let row = sqlx::query("SELECT code, status, billing_interval FROM plans WHERE code = $1")
            .bind(plan_code)
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| backend_error())?;

        row.map(|row| {
            Ok(PlanRecord {
                code: row.try_get("code").map_err(|_| backend_error())?,
                status: row.try_get("status").map_err(|_| backend_error())?,
                billing_interval: row
                    .try_get("billing_interval")
                    .map_err(|_| backend_error())?,
            })
        })
        .transpose()
    }

    async fn current_subscription(
        &self,
        _grant: &OwnerGrant,
        workspace_id: Uuid,
    ) -> Result<Option<Subscription>, AuthError> {
        let mut transaction = self.pool.begin().await.map_err(|_| backend_error())?;
        sqlx::query("SELECT set_config('app.owner_admin', 'true', true)")
            .execute(&mut *transaction)
            .await
            .map_err(|_| backend_error())?;

        let query = format!(
            "SELECT {SUBSCRIPTION_COLUMNS}
             FROM subscriptions
             WHERE workspace_id = $1 AND status IN {CURRENT_STATUS_SQL}
             ORDER BY created_at DESC
             LIMIT 1"
        );
        let row = sqlx::query(&query)
            .bind(workspace_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|_| backend_error())?;

        let subscription = row.as_ref().map(subscription_from_row).transpose()?;
        transaction.commit().await.map_err(|_| backend_error())?;
        Ok(subscription)
    }

    async fn apply_subscription_change(
        &self,
        _grant: &OwnerGrant,
        workspace_id: Uuid,
        change: &SubscriptionChange,
        at: i64,
    ) -> Result<Subscription, AuthError> {
        let mut transaction = self.pool.begin().await.map_err(|_| backend_error())?;
        sqlx::query("SELECT set_config('app.owner_admin', 'true', true)")
            .execute(&mut *transaction)
            .await
            .map_err(|_| backend_error())?;
        sqlx::query("SELECT set_config('app.workspace_id', $1, true)")
            .bind(workspace_id.to_string())
            .execute(&mut *transaction)
            .await
            .map_err(|_| backend_error())?;

        let update = format!(
            r"
            UPDATE subscriptions
            SET
                plan_code = $2,
                status = $3,
                current_period_start = to_timestamp($4::double precision),
                current_period_end = CASE
                    WHEN $5::double precision IS NULL THEN NULL
                    ELSE to_timestamp($5::double precision)
                END,
                cancel_at_period_end = $6,
                canceled_at = CASE
                    WHEN $7::double precision IS NULL THEN NULL
                    ELSE to_timestamp($7::double precision)
                END
            WHERE workspace_id = $1 AND status IN {CURRENT_STATUS_SQL}
            RETURNING {SUBSCRIPTION_COLUMNS}
            "
        );
        let row = sqlx::query(&update)
            .bind(workspace_id)
            .bind(&change.plan_code)
            .bind(&change.status)
            .bind(change.current_period_start as f64)
            .bind(change.current_period_end.map(|value| value as f64))
            .bind(change.cancel_at_period_end)
            .bind(change.canceled_at.map(|value| value as f64))
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|_| backend_error())?
            .ok_or(AuthError::InvalidRequest)?;
        let subscription = subscription_from_row(&row)?;

        let workspace_update = sqlx::query("UPDATE workspaces SET plan_code = $2 WHERE id = $1")
            .bind(workspace_id)
            .bind(&change.plan_code)
            .execute(&mut *transaction)
            .await
            .map_err(|_| backend_error())?;
        if workspace_update.rows_affected() != 1 {
            return Err(AuthError::InvalidRequest);
        }

        sqlx::query(
            r"
            INSERT INTO subscription_events (
                workspace_id,
                subscription_id,
                event_type,
                actor_role,
                from_status,
                to_status,
                amount_minor,
                currency,
                metadata,
                created_at
            )
            VALUES ($1, $2, $3, 'owner', $4, $5, $6, $7, $8::TEXT::JSONB, to_timestamp($9::double precision))
            ",
        )
        .bind(workspace_id)
        .bind(subscription.id)
        .bind(change.event_type.as_str())
        .bind(change.from_status.as_deref())
        .bind(&change.to_status)
        .bind(change.amount_minor)
        .bind(change.currency.as_deref())
        .bind(change.event_metadata_json())
        .bind(at as f64)
        .execute(&mut *transaction)
        .await
        .map_err(|_| backend_error())?;

        transaction.commit().await.map_err(|_| backend_error())?;
        Ok(subscription)
    }
}

#[derive(Debug, Default)]
struct MemoryAccountState {
    accounts: Vec<CustomerAccount>,
    plans: HashMap<String, PlanRecord>,
    subscriptions: HashMap<Uuid, Subscription>,
    events: Vec<RecordedSubscriptionEvent>,
}

/// Process-local repository for unit tests and local development.
#[derive(Debug, Default)]
pub struct InMemoryCustomerAccountRepository {
    state: RwLock<MemoryAccountState>,
}

impl InMemoryCustomerAccountRepository {
    /// Seed accounts for tests.
    pub async fn seed(&self, accounts: Vec<CustomerAccount>) {
        self.state.write().await.accounts = accounts;
    }

    /// Seed the plan catalog for tests.
    pub async fn seed_plans(&self, plans: Vec<PlanRecord>) {
        self.state.write().await.plans = plans
            .into_iter()
            .map(|plan| (plan.code.clone(), plan))
            .collect();
    }

    /// Seed the workspace's current subscription for tests.
    pub async fn seed_subscription(&self, subscription: Subscription) {
        self.state
            .write()
            .await
            .subscriptions
            .insert(subscription.workspace_id, subscription);
    }

    /// Number of recorded subscription events.
    pub async fn event_count(&self) -> usize {
        self.state.read().await.events.len()
    }

    /// Every recorded event, oldest first.
    pub async fn events(&self) -> Vec<RecordedSubscriptionEvent> {
        self.state.read().await.events.clone()
    }
}

#[async_trait]
impl CustomerAccountRepository for InMemoryCustomerAccountRepository {
    async fn list_accounts(&self, _grant: &OwnerGrant) -> Result<Vec<CustomerAccount>, AuthError> {
        Ok(self.state.read().await.accounts.clone())
    }

    async fn suspend_account(
        &self,
        _grant: &OwnerGrant,
        user_id: Uuid,
        _at: i64,
    ) -> Result<(), AuthError> {
        let mut state = self.state.write().await;
        let Some(account) = state.accounts.iter_mut().find(|row| row.user_id == user_id) else {
            return Err(AuthError::InvalidRequest);
        };
        account.status = AccountStatus::Suspended;
        Ok(())
    }

    async fn restore_account(&self, _grant: &OwnerGrant, user_id: Uuid) -> Result<(), AuthError> {
        let mut state = self.state.write().await;
        let Some(account) = state.accounts.iter_mut().find(|row| row.user_id == user_id) else {
            return Err(AuthError::InvalidRequest);
        };
        account.status = AccountStatus::Active;
        Ok(())
    }

    async fn append_subscription_event(
        &self,
        _grant: &OwnerGrant,
        draft: &SubscriptionEventDraft,
        _at: i64,
    ) -> Result<Uuid, AuthError> {
        draft.validate()?;
        let id = Uuid::now_v7();
        self.state
            .write()
            .await
            .events
            .push(RecordedSubscriptionEvent {
                id,
                workspace_id: draft.workspace_id,
                subscription_id: draft.subscription_id,
                event_type: draft.event_type,
                from_status: draft.from_status.clone(),
                to_status: draft.to_status.clone(),
                amount_minor: draft.amount_minor,
                currency: draft.currency.clone(),
                at_period_end: None,
            });
        Ok(id)
    }

    async fn find_plan(
        &self,
        _grant: &OwnerGrant,
        plan_code: &str,
    ) -> Result<Option<PlanRecord>, AuthError> {
        Ok(self.state.read().await.plans.get(plan_code).cloned())
    }

    async fn current_subscription(
        &self,
        _grant: &OwnerGrant,
        workspace_id: Uuid,
    ) -> Result<Option<Subscription>, AuthError> {
        Ok(self
            .state
            .read()
            .await
            .subscriptions
            .get(&workspace_id)
            .filter(|subscription| subscription.is_current())
            .cloned())
    }

    async fn apply_subscription_change(
        &self,
        _grant: &OwnerGrant,
        workspace_id: Uuid,
        change: &SubscriptionChange,
        _at: i64,
    ) -> Result<Subscription, AuthError> {
        let mut state = self.state.write().await;
        let Some(subscription) = state.subscriptions.get_mut(&workspace_id) else {
            return Err(AuthError::InvalidRequest);
        };
        if !subscription.is_current() {
            return Err(AuthError::InvalidRequest);
        }

        subscription.plan_code = change.plan_code.clone();
        subscription.status = change.status.clone();
        subscription.current_period_start = change.current_period_start;
        subscription.current_period_end = change.current_period_end;
        subscription.cancel_at_period_end = change.cancel_at_period_end;
        subscription.canceled_at = change.canceled_at;
        let updated = subscription.clone();

        state.events.push(RecordedSubscriptionEvent {
            id: Uuid::now_v7(),
            workspace_id,
            subscription_id: Some(updated.id),
            event_type: change.event_type,
            from_status: change.from_status.clone(),
            to_status: Some(change.to_status.clone()),
            amount_minor: change.amount_minor,
            currency: change.currency.clone(),
            at_period_end: change.at_period_end,
        });
        Ok(updated)
    }
}

/// Convert days since 1970-01-01 to a proleptic Gregorian date (Howard Hinnant).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    let year = if month <= 2 { year + 1 } else { year };
    (year, month as u32, day as u32)
}

/// Convert a proleptic Gregorian date to days since 1970-01-01.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let month_prime = i64::from(month) + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// Add calendar months in UTC, clamping to the last valid day of the month.
fn add_months_utc(timestamp: i64, months: i64) -> Option<i64> {
    let days = timestamp.div_euclid(SECONDS_PER_DAY);
    let seconds = timestamp.rem_euclid(SECONDS_PER_DAY);
    let (year, month, day) = civil_from_days(days);

    let total_months = year
        .checked_mul(12)?
        .checked_add(i64::from(month) - 1)?
        .checked_add(months)?;
    let new_year = total_months.div_euclid(12);
    let new_month = (total_months.rem_euclid(12) + 1) as u32;
    let new_day = day.min(days_in_month(new_year, new_month));

    days_from_civil(new_year, new_month, new_day)
        .checked_mul(SECONDS_PER_DAY)?
        .checked_add(seconds)
}

/// Midnight UTC on a calendar date, used by the billing-period tests.
#[cfg(test)]
fn unix_timestamp_utc(year: i64, month: u32, day: u32) -> i64 {
    days_from_civil(year, month, day) * SECONDS_PER_DAY
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::identity::OWNER_EMAIL;

    fn account(user_id: Uuid, workspace_id: Uuid, email: &str) -> CustomerAccount {
        CustomerAccount {
            user_id,
            email: email.to_owned(),
            display_name: "Safi Customer".to_owned(),
            status: AccountStatus::Active,
            email_verified: true,
            workspace_id,
            workspace_name: "Safi Customer's workspace".to_owned(),
            role: "owner".to_owned(),
            plan_code: "free".to_owned(),
            subscription_status: "active".to_owned(),
            current_period_end: None,
        }
    }

    fn plan(code: &str, status: &str, billing_interval: &str) -> PlanRecord {
        PlanRecord {
            code: code.to_owned(),
            status: status.to_owned(),
            billing_interval: billing_interval.to_owned(),
        }
    }

    fn subscription(
        workspace_id: Uuid,
        plan_code: &str,
        period_start: i64,
        period_end: Option<i64>,
    ) -> Subscription {
        Subscription {
            id: Uuid::now_v7(),
            workspace_id,
            plan_code: plan_code.to_owned(),
            status: "active".to_owned(),
            current_period_start: period_start,
            current_period_end: period_end,
            cancel_at_period_end: false,
            canceled_at: None,
        }
    }

    async fn seeded(
        plan_code: &str,
        period_start: i64,
        period_end: Option<i64>,
    ) -> (
        Arc<InMemoryCustomerAccountRepository>,
        CustomerAccountService,
        Uuid,
    ) {
        let workspace_id = Uuid::now_v7();
        let repository = Arc::new(InMemoryCustomerAccountRepository::default());
        repository
            .seed_plans(vec![
                plan("free", "active", "none"),
                plan("individual_monthly", "active", "monthly"),
                plan("individual_annual", "active", "yearly"),
                plan("pro_monthly", "draft", "monthly"),
            ])
            .await;
        repository
            .seed_subscription(subscription(
                workspace_id,
                plan_code,
                period_start,
                period_end,
            ))
            .await;
        let service = CustomerAccountService::new(repository.clone());
        (repository, service, workspace_id)
    }

    #[test]
    fn account_status_derives_from_soft_delete_and_suspension() {
        assert_eq!(
            AccountStatus::from_timestamps(None, None),
            AccountStatus::Active
        );
        assert_eq!(
            AccountStatus::from_timestamps(Some(1), None),
            AccountStatus::Suspended
        );
        assert_eq!(
            AccountStatus::from_timestamps(None, Some(1)),
            AccountStatus::Deleted
        );
        assert_eq!(
            AccountStatus::from_timestamps(Some(1), Some(2)),
            AccountStatus::Deleted
        );
        assert_eq!(AccountStatus::Active.as_str(), "active");
        assert_eq!(AccountStatus::Suspended.as_str(), "suspended");
        assert_eq!(AccountStatus::Deleted.as_str(), "deleted");
    }

    #[test]
    fn billing_calendar_clamps_month_ends_and_handles_leap_years() {
        assert_eq!(
            add_months_utc(unix_timestamp_utc(2026, 1, 31), 1),
            Some(unix_timestamp_utc(2026, 2, 28))
        );
        assert_eq!(
            add_months_utc(unix_timestamp_utc(2024, 1, 31), 1),
            Some(unix_timestamp_utc(2024, 2, 29))
        );
        assert_eq!(
            add_months_utc(unix_timestamp_utc(2026, 3, 31), 1),
            Some(unix_timestamp_utc(2026, 4, 30))
        );
        assert_eq!(
            add_months_utc(unix_timestamp_utc(2025, 12, 15), 12),
            Some(unix_timestamp_utc(2026, 12, 15))
        );
        assert_eq!(
            add_months_utc(unix_timestamp_utc(2026, 2, 28), 12),
            Some(unix_timestamp_utc(2027, 2, 28))
        );
        // Time of day is preserved, not truncated to midnight.
        assert_eq!(
            add_months_utc(unix_timestamp_utc(2026, 1, 15) + 3_600, 1),
            Some(unix_timestamp_utc(2026, 2, 15) + 3_600)
        );
    }

    #[tokio::test]
    async fn only_the_owner_can_list_suspend_restore_or_record_events() {
        let user_id = Uuid::now_v7();
        let workspace_id = Uuid::now_v7();
        let repository = Arc::new(InMemoryCustomerAccountRepository::default());
        repository
            .seed(vec![account(
                user_id,
                workspace_id,
                "customer@example.test",
            )])
            .await;
        let service = CustomerAccountService::new(repository.clone());
        let draft = SubscriptionEventDraft {
            workspace_id,
            subscription_id: None,
            event_type: SubscriptionEventType::Created,
            from_status: None,
            to_status: Some("active".to_owned()),
            amount_minor: None,
            currency: None,
        };

        assert_eq!(
            service
                .list_accounts("customer@example.test")
                .await
                .unwrap_err(),
            AuthError::InsufficientScope
        );
        assert_eq!(
            service
                .suspend_account("customer@example.test", user_id, 1)
                .await
                .unwrap_err(),
            AuthError::InsufficientScope
        );
        assert_eq!(
            service
                .record_subscription_event("customer@example.test", &draft, 1)
                .await
                .unwrap_err(),
            AuthError::InsufficientScope
        );

        let accounts = service.list_accounts(OWNER_EMAIL).await.unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].workspace_id, workspace_id);
        assert_eq!(accounts[0].status, AccountStatus::Active);

        service
            .suspend_account(OWNER_EMAIL, user_id, 2)
            .await
            .unwrap();
        assert_eq!(
            service.list_accounts(OWNER_EMAIL).await.unwrap()[0].status,
            AccountStatus::Suspended
        );
        service.restore_account(OWNER_EMAIL, user_id).await.unwrap();
        assert_eq!(
            service.list_accounts(OWNER_EMAIL).await.unwrap()[0].status,
            AccountStatus::Active
        );

        let event_id = service
            .record_subscription_event(OWNER_EMAIL, &draft, 3)
            .await
            .unwrap();
        assert_ne!(event_id, Uuid::nil());
        assert_eq!(repository.event_count().await, 1);
    }

    #[tokio::test]
    async fn subscription_events_validate_currency_and_amount() {
        let repository = Arc::new(InMemoryCustomerAccountRepository::default());
        let service = CustomerAccountService::new(repository);
        let workspace_id = Uuid::now_v7();

        let mut draft = SubscriptionEventDraft {
            workspace_id,
            subscription_id: None,
            event_type: SubscriptionEventType::Renewed,
            from_status: Some("active".to_owned()),
            to_status: Some("active".to_owned()),
            amount_minor: Some(2000),
            currency: Some("USD".to_owned()),
        };
        assert!(service
            .record_subscription_event(OWNER_EMAIL, &draft, 1)
            .await
            .is_ok());

        draft.currency = Some("usd".to_owned());
        assert_eq!(
            service
                .record_subscription_event(OWNER_EMAIL, &draft, 2)
                .await
                .unwrap_err(),
            AuthError::InvalidRequest
        );

        draft.currency = Some("USD".to_owned());
        draft.amount_minor = Some(-1);
        assert_eq!(
            service
                .record_subscription_event(OWNER_EMAIL, &draft, 3)
                .await
                .unwrap_err(),
            AuthError::InvalidRequest
        );
    }

    #[tokio::test]
    async fn subscription_lifecycle_rejects_every_non_owner() {
        let start = unix_timestamp_utc(2026, 1, 15);
        let end = unix_timestamp_utc(2026, 2, 15);
        let (_repository, service, workspace_id) =
            seeded("individual_monthly", start, Some(end)).await;

        assert_eq!(
            service
                .current_subscription("customer@example.test", workspace_id)
                .await
                .unwrap_err(),
            AuthError::InsufficientScope
        );
        assert_eq!(
            service
                .change_plan(
                    "customer@example.test",
                    workspace_id,
                    "individual_annual",
                    start
                )
                .await
                .unwrap_err(),
            AuthError::InsufficientScope
        );
        assert_eq!(
            service
                .renew_subscription("customer@example.test", workspace_id, start, None, None)
                .await
                .unwrap_err(),
            AuthError::InsufficientScope
        );
        assert_eq!(
            service
                .cancel_subscription("customer@example.test", workspace_id, start, true)
                .await
                .unwrap_err(),
            AuthError::InsufficientScope
        );
    }

    #[tokio::test]
    async fn change_plan_requires_an_active_plan_and_records_the_event() {
        let start = unix_timestamp_utc(2026, 1, 15);
        let change_at = unix_timestamp_utc(2026, 1, 20);
        let (repository, service, workspace_id) =
            seeded("free", unix_timestamp_utc(2026, 1, 1), None).await;

        // A draft plan cannot be sold before billing is activated.
        assert_eq!(
            service
                .change_plan(OWNER_EMAIL, workspace_id, "pro_monthly", change_at)
                .await
                .unwrap_err(),
            AuthError::InvalidRequest
        );
        // Unknown and unchanged plans are rejected.
        assert_eq!(
            service
                .change_plan(OWNER_EMAIL, workspace_id, "does_not_exist", change_at)
                .await
                .unwrap_err(),
            AuthError::InvalidRequest
        );
        assert_eq!(
            service
                .change_plan(OWNER_EMAIL, workspace_id, "free", change_at)
                .await
                .unwrap_err(),
            AuthError::InvalidRequest
        );

        let changed = service
            .change_plan(OWNER_EMAIL, workspace_id, " Individual_Monthly ", change_at)
            .await
            .expect("owner changes to an active plan");
        assert_eq!(changed.plan_code, "individual_monthly");
        assert_eq!(changed.status, "active");
        assert_eq!(changed.current_period_start, change_at);
        assert_eq!(
            changed.current_period_end,
            Some(unix_timestamp_utc(2026, 2, 20))
        );
        assert!(!changed.cancel_at_period_end);

        let events = repository.events().await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, SubscriptionEventType::PlanChanged);
        assert_eq!(events[0].from_status.as_deref(), Some("active"));
        assert_eq!(events[0].to_status.as_deref(), Some("active"));
        assert_eq!(events[0].subscription_id, Some(changed.id));

        // Annual plans extend to the same day next year.
        let annual = service
            .change_plan(
                OWNER_EMAIL,
                workspace_id,
                "individual_annual",
                unix_timestamp_utc(2026, 3, 15),
            )
            .await
            .unwrap();
        assert_eq!(
            annual.current_period_end,
            Some(unix_timestamp_utc(2027, 3, 15))
        );
        assert_eq!(start, unix_timestamp_utc(2026, 1, 15));
    }

    #[tokio::test]
    async fn renewal_extends_from_the_paid_period_and_clears_scheduled_cancellation() {
        let start = unix_timestamp_utc(2026, 1, 15);
        let end = unix_timestamp_utc(2026, 2, 15);
        let (repository, service, workspace_id) =
            seeded("individual_monthly", start, Some(end)).await;

        service
            .cancel_subscription(
                OWNER_EMAIL,
                workspace_id,
                unix_timestamp_utc(2026, 1, 20),
                true,
            )
            .await
            .unwrap();

        let renewed = service
            .renew_subscription(
                OWNER_EMAIL,
                workspace_id,
                unix_timestamp_utc(2026, 1, 20),
                Some(2000),
                Some("USD"),
            )
            .await
            .expect("owner renews early");
        assert_eq!(renewed.status, "active");
        assert_eq!(renewed.current_period_start, start);
        assert_eq!(
            renewed.current_period_end,
            Some(unix_timestamp_utc(2026, 3, 15))
        );
        assert!(!renewed.cancel_at_period_end);

        let events = repository.events().await;
        let renewal = events
            .iter()
            .find(|event| event.event_type == SubscriptionEventType::Renewed)
            .expect("renewal is audited");
        assert_eq!(renewal.from_status.as_deref(), Some("active"));
        assert_eq!(renewal.to_status.as_deref(), Some("active"));
        assert_eq!(renewal.amount_minor, Some(2000));
        assert_eq!(renewal.currency.as_deref(), Some("USD"));

        // Renewing after expiry restarts from the operation time.
        let late = service
            .renew_subscription(
                OWNER_EMAIL,
                workspace_id,
                unix_timestamp_utc(2026, 4, 10),
                None,
                None,
            )
            .await
            .unwrap();
        assert_eq!(
            late.current_period_end,
            Some(unix_timestamp_utc(2026, 5, 10))
        );

        // Payment details must be complete and valid.
        assert_eq!(
            service
                .renew_subscription(OWNER_EMAIL, workspace_id, start, Some(2000), None)
                .await
                .unwrap_err(),
            AuthError::InvalidRequest
        );
        assert_eq!(
            service
                .renew_subscription(OWNER_EMAIL, workspace_id, start, None, Some("usd"))
                .await
                .unwrap_err(),
            AuthError::InvalidRequest
        );
    }

    #[tokio::test]
    async fn cancellation_can_be_scheduled_or_immediate_and_is_audited_once() {
        let start = unix_timestamp_utc(2026, 1, 15);
        let end = unix_timestamp_utc(2026, 2, 15);
        let (repository, service, workspace_id) =
            seeded("individual_monthly", start, Some(end)).await;
        let cancel_at = unix_timestamp_utc(2026, 1, 20);

        let scheduled = service
            .cancel_subscription(OWNER_EMAIL, workspace_id, cancel_at, true)
            .await
            .unwrap();
        assert_eq!(scheduled.status, "active");
        assert!(scheduled.cancel_at_period_end);
        assert_eq!(scheduled.canceled_at, None);
        assert_eq!(scheduled.current_period_end, Some(end));

        // Repeating the same request is idempotent and writes no second event.
        let repeated = service
            .cancel_subscription(OWNER_EMAIL, workspace_id, cancel_at, true)
            .await
            .unwrap();
        assert_eq!(repeated, scheduled);
        assert_eq!(repository.event_count().await, 1);
        let event = &repository.events().await[0];
        assert_eq!(event.event_type, SubscriptionEventType::Canceled);
        assert_eq!(event.at_period_end, Some(true));
        assert_eq!(event.to_status.as_deref(), Some("active"));

        let immediate = service
            .cancel_subscription(
                OWNER_EMAIL,
                workspace_id,
                unix_timestamp_utc(2026, 1, 25),
                false,
            )
            .await
            .unwrap();
        assert_eq!(immediate.status, "canceled");
        assert_eq!(immediate.canceled_at, Some(unix_timestamp_utc(2026, 1, 25)));
        assert!(!immediate.cancel_at_period_end);
        assert_eq!(
            service
                .current_subscription(OWNER_EMAIL, workspace_id)
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            service
                .renew_subscription(OWNER_EMAIL, workspace_id, cancel_at, None, None)
                .await
                .unwrap_err(),
            AuthError::InvalidRequest
        );

        let events = repository.events().await;
        assert_eq!(events.len(), 2);
        assert_eq!(events[1].at_period_end, Some(false));
        assert_eq!(events[1].to_status.as_deref(), Some("canceled"));
    }

    #[tokio::test]
    async fn the_free_plan_cannot_be_renewed_or_canceled() {
        let (repository, service, workspace_id) =
            seeded("free", unix_timestamp_utc(2026, 1, 1), None).await;
        let at = unix_timestamp_utc(2026, 1, 20);

        assert_eq!(
            service
                .renew_subscription(OWNER_EMAIL, workspace_id, at, None, None)
                .await
                .unwrap_err(),
            AuthError::InvalidRequest
        );
        assert_eq!(
            service
                .cancel_subscription(OWNER_EMAIL, workspace_id, at, true)
                .await
                .unwrap_err(),
            AuthError::InvalidRequest
        );
        assert_eq!(
            service
                .cancel_subscription(OWNER_EMAIL, workspace_id, at, false)
                .await
                .unwrap_err(),
            AuthError::InvalidRequest
        );
        assert_eq!(repository.event_count().await, 0);
    }

    #[tokio::test]
    async fn lifecycle_rejects_nil_workspaces_and_invalid_timestamps() {
        let (_repository, service, workspace_id) =
            seeded("individual_monthly", 1_780_000_000, Some(1_782_000_000)).await;

        assert_eq!(
            service
                .current_subscription(OWNER_EMAIL, Uuid::nil())
                .await
                .unwrap_err(),
            AuthError::InvalidRequest
        );
        assert_eq!(
            service
                .cancel_subscription(OWNER_EMAIL, workspace_id, 0, true)
                .await
                .unwrap_err(),
            AuthError::InvalidRequest
        );
        assert_eq!(
            service
                .change_plan(OWNER_EMAIL, workspace_id, "   ", 1_780_000_000)
                .await
                .unwrap_err(),
            AuthError::InvalidRequest
        );
    }
}
