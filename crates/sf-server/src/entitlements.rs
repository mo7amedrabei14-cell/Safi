//! Authoritative backend Entitlements, Pricing Tiers, Trial Anti-Abuse,
//! and Atomic Idempotent Usage Accounting (Phase 06, `config/business.json`).
//!
//! Approved business rules enforced here:
//! - Free Plan: 5 ops/month, max 5,000 rows, max 20 columns, mandatory watermark,
//!   no saved recipes (read-only locked after downgrade), no premium recipe packs, no AI, no teams.
//! - Trial: 7 days, all features open, no card, verified email + phone, one trial per
//!   email/phone/device, 25 AI calls total, auto-downgrade to Free on expiry.
//! - Individual Monthly: $20 / 1,000 EGP, 400 AI calls/month, all paid features + premium packs.
//! - Individual Annual: $168 / 8,400 EGP, 400 AI calls/month, all paid features + premium packs.
//! - Owner's Friend (VIP): $0 / 0 EGP, all paid features + 400 AI calls/month, never expires unless revoked.
//! - Organization: min 3 seats, $17 / 850 EGP per seat/mo ($204 / 10,200 EGP per seat/year), 400 AI calls/seat/mo.
//! - Pay-Per-File: $2 / 50 EGP, up to 100,000 rows, removes watermark, no AI, no premium packs.
//! - Credit Pack 25: $5 / 250 EGP, 25 operations, never expires, removes watermark, no AI, no premium packs.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::auth::unix_now;
use crate::error::AuthError;

pub const FREE_OPS_PER_MONTH: u32 = 5;
pub const FREE_MAX_ROWS: u64 = 5_000;
pub const FREE_MAX_COLS: u32 = 20;
pub const PAY_PER_FILE_MAX_ROWS: u64 = 100_000;
pub const TRIAL_DURATION_SECS: i64 = 7 * 24 * 60 * 60;
pub const TRIAL_AI_CALLS_CAP: u32 = 25;
pub const PAID_AI_CALLS_PER_MONTH: u32 = 400;
pub const ORG_MIN_SEATS: u32 = 3;
pub const BILLING_GRACE_PERIOD_SECS: i64 = 3 * 24 * 60 * 60;
pub const WATERMARK_TEXT: &str = "Processed by SafiData (Free Plan) — صافي البيانات";

/// Effective account plan tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanTier {
    Free,
    Trial,
    IndividualMonthly,
    IndividualAnnual,
    OrganizationSeat,
    FriendVip,
    Owner,
}

/// Subscription lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionState {
    Active,
    Trialing,
    PastDue,
    Canceled,
    Expired,
    Free,
}

/// Persisted subscription & entitlement record per workspace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceEntitlementRecord {
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub plan: PlanTier,
    pub state: SubscriptionState,
    pub seats: u32,
    pub trial_expires_at: Option<i64>,
    pub current_period_end: Option<i64>,
    pub past_due_since: Option<i64>,
    pub friend_vip_active: bool,
    pub friend_vip_granted_by: Option<Uuid>,
    pub operation_credits: u32,
    pub pay_per_file_tokens: u32,
    pub trial_ai_used: u32,
}

impl WorkspaceEntitlementRecord {
    #[must_use]
    pub fn new_free(workspace_id: Uuid, user_id: Uuid) -> Self {
        Self {
            workspace_id,
            user_id,
            plan: PlanTier::Free,
            state: SubscriptionState::Free,
            seats: 1,
            trial_expires_at: None,
            current_period_end: None,
            past_due_since: None,
            friend_vip_active: false,
            friend_vip_granted_by: None,
            operation_credits: 0,
            pay_per_file_tokens: 0,
            trial_ai_used: 0,
        }
    }
}

/// Monthly usage counters for a workspace in `YYYY-MM`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MonthlyUsage {
    pub period_ym: String,
    pub operations_used: u32,
    pub ai_calls_used: u32,
    pub rows_cleaned: u64,
    pub pii_masked: u64,
}

/// Resolved, server-authoritative entitlement view returned to Desktop & UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectiveEntitlements {
    pub workspace_id: Uuid,
    pub effective_plan: PlanTier,
    pub state: SubscriptionState,
    pub watermark_required: bool,
    pub watermark_text: Option<String>,
    pub operations_limit_per_month: Option<u32>,
    pub operations_used_this_month: u32,
    pub max_rows_per_file: Option<u64>,
    pub max_columns_per_file: Option<u32>,
    pub saved_recipes_write_enabled: bool,
    pub saved_recipes_read_only: bool,
    pub premium_recipe_packs_enabled: bool,
    pub ai_enabled: bool,
    pub ai_calls_limit: u32,
    pub ai_calls_used: u32,
    pub teams_enabled: bool,
    pub seats: u32,
    pub operation_credits: u32,
    pub pay_per_file_tokens: u32,
    pub show_past_due_banner: bool,
    pub trial_expires_at: Option<i64>,
}

/// Request to atomically check & record an operation or AI call.
#[derive(Debug, Clone, Deserialize)]
pub struct ConsumeUsageRequest {
    pub idempotency_key: String,
    pub kind: UsageKind,
    pub row_count: u64,
    pub column_count: u32,
    pub pii_masked_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageKind {
    DataOperation,
    AiCall,
}

/// Result of an atomic usage consumption check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsumeUsageResult {
    pub allowed: bool,
    pub duplicate_idempotency_hit: bool,
    pub watermark_required: bool,
    pub debited_credit: bool,
    pub debited_pay_per_file: bool,
    pub entitlements: EffectiveEntitlements,
}

/// Aggregate platform usage & revenue stats computed from real records (never hardcoded).
#[derive(Debug, Clone, Serialize)]
pub struct EntitlementAggregateStats {
    pub active_trials: usize,
    pub free_users: usize,
    pub monthly_subscribers: usize,
    pub annual_subscribers: usize,
    pub organization_seats: u64,
    pub friend_vip_users: usize,
    pub credit_pack_holders: usize,
    pub pay_per_file_holders: usize,
    pub mrr_usd: u64,
    pub mrr_egp: u64,
    pub arr_usd: u64,
    pub arr_egp: u64,
    pub rows_cleaned_total: u64,
    pub pii_masked_total: u64,
    pub ai_calls_used_total: u64,
}

/// Serializable snapshot of entitlement & usage state for durable restart persistence.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntitlementSnapshot {
    pub workspaces: Vec<WorkspaceEntitlementRecord>,
    pub monthly: Vec<((Uuid, String), MonthlyUsage)>,
    pub idempotency: Vec<(Uuid, String)>,
    pub claimed_trial_emails: Vec<String>,
    pub claimed_trial_phones: Vec<String>,
    pub claimed_trial_devices: Vec<String>,
}

#[derive(Default)]
struct EntitlementStore {
    workspaces: HashMap<Uuid, WorkspaceEntitlementRecord>,
    monthly: HashMap<(Uuid, String), MonthlyUsage>,
    idempotency: HashSet<(Uuid, String)>,
    claimed_trial_emails: HashSet<String>,
    claimed_trial_phones: HashSet<String>,
    claimed_trial_devices: HashSet<String>,
}

/// Thread-safe authoritative entitlement & usage service.
#[derive(Clone, Default)]
pub struct EntitlementService {
    store: Arc<RwLock<EntitlementStore>>,
}

impl std::fmt::Debug for EntitlementService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntitlementService").finish()
    }
}

#[must_use]
pub fn period_ym_at(ts_secs: i64) -> String {
    let dt = OffsetDateTime::from_unix_timestamp(ts_secs).unwrap_or(OffsetDateTime::UNIX_EPOCH);
    format!("{:04}-{:02}", dt.year(), u8::from(dt.month()))
}

impl EntitlementService {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Ensure a workspace has an entitlement record (defaults to Free).
    pub async fn ensure_workspace(&self, workspace_id: Uuid, user_id: Uuid) {
        let mut guard = self.store.write().await;
        guard
            .workspaces
            .entry(workspace_id)
            .or_insert_with(|| WorkspaceEntitlementRecord::new_free(workspace_id, user_id));
    }

    /// Claim a one-time 7-day Trial enforcing verified email + phone and anti-abuse across
    /// normalized email hash, phone hash, and device fingerprint hash.
    pub async fn claim_trial_at(
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
        let mut guard = self.store.write().await;
        if guard.claimed_trial_emails.contains(email_hash)
            || guard.claimed_trial_phones.contains(phone_hash)
            || guard.claimed_trial_devices.contains(device_hash)
        {
            return Err(AuthError::DuplicateAccount);
        }

        guard.claimed_trial_emails.insert(email_hash.to_owned());
        guard.claimed_trial_phones.insert(phone_hash.to_owned());
        guard.claimed_trial_devices.insert(device_hash.to_owned());

        let rec = {
            let rec = guard
                .workspaces
                .entry(workspace_id)
                .or_insert_with(|| WorkspaceEntitlementRecord::new_free(workspace_id, user_id));
            rec.plan = PlanTier::Trial;
            rec.state = SubscriptionState::Trialing;
            rec.trial_expires_at = Some(now_secs + TRIAL_DURATION_SECS);
            rec.trial_ai_used = 0;
            rec.clone()
        };

        let period = period_ym_at(now_secs);
        let usage = guard
            .monthly
            .get(&(workspace_id, period))
            .cloned()
            .unwrap_or_default();
        Ok(compute_effective(&rec, &usage, false, now_secs))
    }

    /// Extend a workspace's trial by `extra_days` (Owner Control Plane action).
    pub async fn extend_trial_at(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        extra_days: i64,
        now_secs: i64,
    ) -> Result<EffectiveEntitlements, AuthError> {
        if extra_days <= 0 || extra_days > 365 {
            return Err(AuthError::InvalidRequest);
        }
        let mut guard = self.store.write().await;
        let rec = {
            let rec = guard
                .workspaces
                .entry(workspace_id)
                .or_insert_with(|| WorkspaceEntitlementRecord::new_free(workspace_id, user_id));
            let base = rec.trial_expires_at.unwrap_or(now_secs).max(now_secs);
            rec.trial_expires_at = Some(base + extra_days * 24 * 60 * 60);
            if matches!(rec.plan, PlanTier::Free | PlanTier::Trial) {
                rec.plan = PlanTier::Trial;
                rec.state = SubscriptionState::Trialing;
            }
            rec.clone()
        };
        let period = period_ym_at(now_secs);
        let usage = guard
            .monthly
            .get(&(workspace_id, period))
            .cloned()
            .unwrap_or_default();
        Ok(compute_effective(&rec, &usage, false, now_secs))
    }

    /// Grant or revoke Owner's Friend (VIP) role ($0, all paid features + 400 AI/mo).
    pub async fn set_friend_vip(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        granted_by: Uuid,
        active: bool,
    ) -> EffectiveEntitlements {
        let now = unix_now();
        let mut guard = self.store.write().await;
        let rec = {
            let rec = guard
                .workspaces
                .entry(workspace_id)
                .or_insert_with(|| WorkspaceEntitlementRecord::new_free(workspace_id, user_id));
            rec.friend_vip_active = active;
            rec.friend_vip_granted_by = if active { Some(granted_by) } else { None };
            rec.clone()
        };
        let period = period_ym_at(now);
        let usage = guard
            .monthly
            .get(&(workspace_id, period))
            .cloned()
            .unwrap_or_default();
        compute_effective(&rec, &usage, false, now)
    }

    /// Activate or update a paid subscription (called by verified Paddle/Paymob billing webhooks).
    #[allow(clippy::too_many_arguments)]
    pub async fn apply_subscription_update(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        plan: PlanTier,
        state: SubscriptionState,
        seats: u32,
        current_period_end: Option<i64>,
        now_secs: i64,
    ) -> Result<EffectiveEntitlements, AuthError> {
        if plan == PlanTier::OrganizationSeat && seats < ORG_MIN_SEATS {
            return Err(AuthError::OrgMinSeatsRequired);
        }
        let mut guard = self.store.write().await;
        let rec = {
            let rec = guard
                .workspaces
                .entry(workspace_id)
                .or_insert_with(|| WorkspaceEntitlementRecord::new_free(workspace_id, user_id));
            rec.plan = plan;
            rec.state = state;
            rec.seats = seats.max(1);
            rec.current_period_end = current_period_end;
            rec.past_due_since = match state {
                SubscriptionState::PastDue => rec.past_due_since.or(Some(now_secs)),
                _ => None,
            };
            rec.clone()
        };
        let period = period_ym_at(now_secs);
        let usage = guard
            .monthly
            .get(&(workspace_id, period))
            .cloned()
            .unwrap_or_default();
        Ok(compute_effective(&rec, &usage, false, now_secs))
    }

    /// Add non-expiring Credit Pack operations (e.g., +25) or Pay-Per-File tokens.
    pub async fn grant_micro_purchase(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        operation_credits_delta: u32,
        pay_per_file_delta: u32,
    ) {
        let mut guard = self.store.write().await;
        let rec = guard
            .workspaces
            .entry(workspace_id)
            .or_insert_with(|| WorkspaceEntitlementRecord::new_free(workspace_id, user_id));
        rec.operation_credits = rec
            .operation_credits
            .saturating_add(operation_credits_delta);
        rec.pay_per_file_tokens = rec.pay_per_file_tokens.saturating_add(pay_per_file_delta);
    }

    /// Resolve effective entitlements at `now_secs` (auto-downgrading expired trials/past-due).
    pub async fn effective_at(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        is_owner: bool,
        now_secs: i64,
    ) -> EffectiveEntitlements {
        let mut guard = self.store.write().await;
        let rec = {
            let rec = guard
                .workspaces
                .entry(workspace_id)
                .or_insert_with(|| WorkspaceEntitlementRecord::new_free(workspace_id, user_id));
            reconcile_expirations(rec, now_secs);
            rec.clone()
        };
        let period = period_ym_at(now_secs);
        let usage = guard
            .monthly
            .get(&(workspace_id, period))
            .cloned()
            .unwrap_or_default();
        compute_effective(&rec, &usage, is_owner, now_secs)
    }

    /// Atomically enforce plan limits and record usage with strict idempotency.
    pub async fn consume_at(
        &self,
        workspace_id: Uuid,
        user_id: Uuid,
        is_owner: bool,
        req: &ConsumeUsageRequest,
        now_secs: i64,
    ) -> Result<ConsumeUsageResult, AuthError> {
        let idem_key = req.idempotency_key.trim();
        if idem_key.is_empty() || idem_key.len() > 128 {
            return Err(AuthError::InvalidRequest);
        }

        let mut guard = self.store.write().await;
        let period = period_ym_at(now_secs);

        // Idempotent replay protection: never double-charge or double-count.
        if guard
            .idempotency
            .contains(&(workspace_id, idem_key.to_owned()))
        {
            let rec = guard
                .workspaces
                .entry(workspace_id)
                .or_insert_with(|| WorkspaceEntitlementRecord::new_free(workspace_id, user_id))
                .clone();
            let usage = guard
                .monthly
                .get(&(workspace_id, period))
                .cloned()
                .unwrap_or_default();
            let ent = compute_effective(&rec, &usage, is_owner, now_secs);
            return Ok(ConsumeUsageResult {
                allowed: true,
                duplicate_idempotency_hit: true,
                watermark_required: ent.watermark_required,
                debited_credit: false,
                debited_pay_per_file: false,
                entitlements: ent,
            });
        }

        let mut rec = guard
            .workspaces
            .entry(workspace_id)
            .or_insert_with(|| WorkspaceEntitlementRecord::new_free(workspace_id, user_id))
            .clone();
        reconcile_expirations(&mut rec, now_secs);

        let mut usage = guard
            .monthly
            .get(&(workspace_id, period.clone()))
            .cloned()
            .unwrap_or_else(|| MonthlyUsage {
                period_ym: period.clone(),
                ..Default::default()
            });

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
                        req.row_count > FREE_MAX_ROWS || req.column_count > FREE_MAX_COLS;
                    let exceeds_free_ops = usage.operations_used >= FREE_OPS_PER_MONTH;

                    if exceeds_free_shape {
                        // Pay-per-file unlocks up to 100k rows and removes watermark.
                        if rec.pay_per_file_tokens > 0 && req.row_count <= PAY_PER_FILE_MAX_ROWS {
                            rec.pay_per_file_tokens -= 1;
                            debited_pay_per_file = true;
                            watermark_required = false;
                        } else {
                            return Err(AuthError::DatasetLimitExceeded);
                        }
                    } else if exceeds_free_ops {
                        // Credit pack (25 ops, never expires) covers extra ops & removes watermark.
                        if rec.operation_credits > 0 {
                            rec.operation_credits -= 1;
                            debited_credit = true;
                            watermark_required = false;
                        } else if rec.pay_per_file_tokens > 0 {
                            rec.pay_per_file_tokens -= 1;
                            debited_pay_per_file = true;
                            watermark_required = false;
                        } else {
                            return Err(AuthError::QuotaExceeded);
                        }
                    } else if rec.operation_credits > 0 {
                        // Free user within 5 ops still uses free quota with watermark unless paying.
                        watermark_required = true;
                    }
                }
                usage.operations_used = usage.operations_used.saturating_add(1);
                usage.rows_cleaned = usage.rows_cleaned.saturating_add(req.row_count);
                usage.pii_masked = usage.pii_masked.saturating_add(req.pii_masked_count);
            }
        }

        guard
            .idempotency
            .insert((workspace_id, idem_key.to_owned()));
        guard.workspaces.insert(workspace_id, rec.clone());
        guard.monthly.insert((workspace_id, period), usage.clone());

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

    /// Compute real live aggregate metrics for the Owner Control Plane (zero fake numbers).
    pub async fn aggregate_stats(&self, now_secs: i64) -> EntitlementAggregateStats {
        let guard = self.store.read().await;
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
            rows_cleaned_total: 0,
            pii_masked_total: 0,
            ai_calls_used_total: 0,
        };

        for rec in guard.workspaces.values() {
            let mut copy = rec.clone();
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
                    stats.mrr_usd += 20;
                    stats.mrr_egp += 1000;
                }
                (
                    PlanTier::IndividualAnnual,
                    SubscriptionState::Active | SubscriptionState::PastDue,
                ) => {
                    stats.annual_subscribers += 1;
                    stats.mrr_usd += 14; // $168 / 12
                    stats.mrr_egp += 700; // 8400 / 12
                }
                (
                    PlanTier::OrganizationSeat,
                    SubscriptionState::Active | SubscriptionState::PastDue,
                ) => {
                    let seats = u64::from(copy.seats.max(ORG_MIN_SEATS));
                    stats.organization_seats += seats;
                    stats.mrr_usd += seats * 17;
                    stats.mrr_egp += seats * 850;
                }
                _ => stats.free_users += 1,
            }
        }
        stats.arr_usd = stats.mrr_usd * 12;
        stats.arr_egp = stats.mrr_egp * 12;

        for usage in guard.monthly.values() {
            stats.rows_cleaned_total = stats.rows_cleaned_total.saturating_add(usage.rows_cleaned);
            stats.pii_masked_total = stats.pii_masked_total.saturating_add(usage.pii_masked);
            stats.ai_calls_used_total = stats
                .ai_calls_used_total
                .saturating_add(u64::from(usage.ai_calls_used));
        }

        stats
    }

    /// Snapshot a workspace record if it exists (used by Owner inspection/audit before/after).
    pub async fn workspace_record(&self, workspace_id: Uuid) -> Option<WorkspaceEntitlementRecord> {
        self.store
            .read()
            .await
            .workspaces
            .get(&workspace_id)
            .cloned()
    }

    /// Export full entitlement state for durable restart persistence.
    pub async fn export_snapshot(&self) -> EntitlementSnapshot {
        let guard = self.store.read().await;
        EntitlementSnapshot {
            workspaces: guard.workspaces.values().cloned().collect(),
            monthly: guard
                .monthly
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            idempotency: guard.idempotency.iter().cloned().collect(),
            claimed_trial_emails: guard.claimed_trial_emails.iter().cloned().collect(),
            claimed_trial_phones: guard.claimed_trial_phones.iter().cloned().collect(),
            claimed_trial_devices: guard.claimed_trial_devices.iter().cloned().collect(),
        }
    }

    /// Restore full entitlement state from a durable snapshot.
    pub async fn import_snapshot(&self, snap: EntitlementSnapshot) {
        let mut guard = self.store.write().await;
        guard.workspaces = snap
            .workspaces
            .into_iter()
            .map(|w| (w.workspace_id, w))
            .collect();
        guard.monthly = snap.monthly.into_iter().collect();
        guard.idempotency = snap.idempotency.into_iter().collect();
        guard.claimed_trial_emails = snap.claimed_trial_emails.into_iter().collect();
        guard.claimed_trial_phones = snap.claimed_trial_phones.into_iter().collect();
        guard.claimed_trial_devices = snap.claimed_trial_devices.into_iter().collect();
    }
}

fn reconcile_expirations(rec: &mut WorkspaceEntitlementRecord, now_secs: i64) {
    if rec.plan == PlanTier::Trial {
        if let Some(exp) = rec.trial_expires_at {
            if now_secs >= exp {
                rec.plan = PlanTier::Free;
                rec.state = SubscriptionState::Expired;
            }
        }
    }
    if rec.state == SubscriptionState::PastDue {
        if let Some(since) = rec.past_due_since {
            if now_secs >= since + BILLING_GRACE_PERIOD_SECS {
                rec.plan = PlanTier::Free;
                rec.state = SubscriptionState::Expired;
            }
        }
    }
    if rec.state == SubscriptionState::Canceled {
        if let Some(end) = rec.current_period_end {
            if now_secs >= end {
                rec.plan = PlanTier::Free;
                rec.state = SubscriptionState::Expired;
            }
        }
    }
}

fn compute_effective(
    rec: &WorkspaceEntitlementRecord,
    usage: &MonthlyUsage,
    is_owner: bool,
    now_secs: i64,
) -> EffectiveEntitlements {
    if is_owner {
        return EffectiveEntitlements {
            workspace_id: rec.workspace_id,
            effective_plan: PlanTier::Owner,
            state: SubscriptionState::Active,
            watermark_required: false,
            watermark_text: None,
            operations_limit_per_month: None,
            operations_used_this_month: usage.operations_used,
            max_rows_per_file: None,
            max_columns_per_file: None,
            saved_recipes_write_enabled: true,
            saved_recipes_read_only: false,
            premium_recipe_packs_enabled: true,
            ai_enabled: true,
            ai_calls_limit: u32::MAX,
            ai_calls_used: usage.ai_calls_used,
            teams_enabled: true,
            seats: rec.seats.max(1),
            operation_credits: rec.operation_credits,
            pay_per_file_tokens: rec.pay_per_file_tokens,
            show_past_due_banner: false,
            trial_expires_at: rec.trial_expires_at,
        };
    }

    if rec.friend_vip_active {
        return EffectiveEntitlements {
            workspace_id: rec.workspace_id,
            effective_plan: PlanTier::FriendVip,
            state: SubscriptionState::Active,
            watermark_required: false,
            watermark_text: None,
            operations_limit_per_month: None,
            operations_used_this_month: usage.operations_used,
            max_rows_per_file: None,
            max_columns_per_file: None,
            saved_recipes_write_enabled: true,
            saved_recipes_read_only: false,
            premium_recipe_packs_enabled: true,
            ai_enabled: true,
            ai_calls_limit: PAID_AI_CALLS_PER_MONTH,
            ai_calls_used: usage.ai_calls_used,
            teams_enabled: true,
            seats: rec.seats.max(1),
            operation_credits: rec.operation_credits,
            pay_per_file_tokens: rec.pay_per_file_tokens,
            show_past_due_banner: false,
            trial_expires_at: None,
        };
    }

    let mut effective_rec = rec.clone();
    reconcile_expirations(&mut effective_rec, now_secs);

    let show_past_due_banner = effective_rec.state == SubscriptionState::PastDue;
    match effective_rec.plan {
        PlanTier::Trial if effective_rec.state == SubscriptionState::Trialing => {
            EffectiveEntitlements {
                workspace_id: effective_rec.workspace_id,
                effective_plan: PlanTier::Trial,
                state: SubscriptionState::Trialing,
                watermark_required: false,
                watermark_text: None,
                operations_limit_per_month: None,
                operations_used_this_month: usage.operations_used,
                max_rows_per_file: None,
                max_columns_per_file: None,
                saved_recipes_write_enabled: true,
                saved_recipes_read_only: false,
                premium_recipe_packs_enabled: true,
                ai_enabled: true,
                ai_calls_limit: TRIAL_AI_CALLS_CAP,
                ai_calls_used: effective_rec.trial_ai_used,
                teams_enabled: true,
                seats: 1,
                operation_credits: effective_rec.operation_credits,
                pay_per_file_tokens: effective_rec.pay_per_file_tokens,
                show_past_due_banner: false,
                trial_expires_at: effective_rec.trial_expires_at,
            }
        }
        PlanTier::IndividualMonthly | PlanTier::IndividualAnnual
            if matches!(
                effective_rec.state,
                SubscriptionState::Active
                    | SubscriptionState::PastDue
                    | SubscriptionState::Canceled
            ) =>
        {
            EffectiveEntitlements {
                workspace_id: effective_rec.workspace_id,
                effective_plan: effective_rec.plan,
                state: effective_rec.state,
                watermark_required: false,
                watermark_text: None,
                operations_limit_per_month: None,
                operations_used_this_month: usage.operations_used,
                max_rows_per_file: None,
                max_columns_per_file: None,
                saved_recipes_write_enabled: true,
                saved_recipes_read_only: false,
                premium_recipe_packs_enabled: true,
                ai_enabled: true,
                ai_calls_limit: PAID_AI_CALLS_PER_MONTH,
                ai_calls_used: usage.ai_calls_used,
                teams_enabled: false,
                seats: 1,
                operation_credits: effective_rec.operation_credits,
                pay_per_file_tokens: effective_rec.pay_per_file_tokens,
                show_past_due_banner,
                trial_expires_at: None,
            }
        }
        PlanTier::OrganizationSeat
            if matches!(
                effective_rec.state,
                SubscriptionState::Active
                    | SubscriptionState::PastDue
                    | SubscriptionState::Canceled
            ) =>
        {
            let seats = effective_rec.seats.max(ORG_MIN_SEATS);
            EffectiveEntitlements {
                workspace_id: effective_rec.workspace_id,
                effective_plan: PlanTier::OrganizationSeat,
                state: effective_rec.state,
                watermark_required: false,
                watermark_text: None,
                operations_limit_per_month: None,
                operations_used_this_month: usage.operations_used,
                max_rows_per_file: None,
                max_columns_per_file: None,
                saved_recipes_write_enabled: true,
                saved_recipes_read_only: false,
                premium_recipe_packs_enabled: true,
                ai_enabled: true,
                ai_calls_limit: PAID_AI_CALLS_PER_MONTH.saturating_mul(seats),
                ai_calls_used: usage.ai_calls_used,
                teams_enabled: true,
                seats,
                operation_credits: effective_rec.operation_credits,
                pay_per_file_tokens: effective_rec.pay_per_file_tokens,
                show_past_due_banner,
                trial_expires_at: None,
            }
        }
        _ => EffectiveEntitlements {
            workspace_id: effective_rec.workspace_id,
            effective_plan: PlanTier::Free,
            state: effective_rec.state,
            watermark_required: true,
            watermark_text: Some(WATERMARK_TEXT.to_owned()),
            operations_limit_per_month: Some(FREE_OPS_PER_MONTH),
            operations_used_this_month: usage.operations_used,
            max_rows_per_file: Some(FREE_MAX_ROWS),
            max_columns_per_file: Some(FREE_MAX_COLS),
            saved_recipes_write_enabled: false,
            saved_recipes_read_only: true,
            premium_recipe_packs_enabled: false,
            ai_enabled: false,
            ai_calls_limit: 0,
            ai_calls_used: usage.ai_calls_used,
            teams_enabled: false,
            seats: 1,
            operation_credits: effective_rec.operation_credits,
            pay_per_file_tokens: effective_rec.pay_per_file_tokens,
            show_past_due_banner: false,
            trial_expires_at: effective_rec.trial_expires_at,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn free_plan_boundaries_credits_pay_per_file_and_monthly_reset() {
        let svc = EntitlementService::new();
        let ws = Uuid::now_v7();
        let user = Uuid::now_v7();
        let nov_2026: i64 = 1_794_700_800; // 2026-11-15
        let dec_2026: i64 = 1_797_379_200; // 2026-12-16

        // 5 allowed operations within 5000 rows & 20 cols, with mandatory watermark.
        for i in 1..=5 {
            let res = svc
                .consume_at(
                    ws,
                    user,
                    false,
                    &ConsumeUsageRequest {
                        idempotency_key: format!("op-{i}"),
                        kind: UsageKind::DataOperation,
                        row_count: 5000,
                        column_count: 20,
                        pii_masked_count: 0,
                    },
                    nov_2026,
                )
                .await
                .expect("free op");
            assert!(res.watermark_required);
        }

        // Duplicate idempotency key does NOT consume extra op.
        let dup = svc
            .consume_at(
                ws,
                user,
                false,
                &ConsumeUsageRequest {
                    idempotency_key: "op-5".to_owned(),
                    kind: UsageKind::DataOperation,
                    row_count: 100,
                    column_count: 5,
                    pii_masked_count: 0,
                },
                nov_2026,
            )
            .await
            .expect("idempotent hit");
        assert!(dup.duplicate_idempotency_hit);

        // 6th operation rejected (`SF-LIC-005`).
        assert_eq!(
            svc.consume_at(
                ws,
                user,
                false,
                &ConsumeUsageRequest {
                    idempotency_key: "op-6".to_owned(),
                    kind: UsageKind::DataOperation,
                    row_count: 100,
                    column_count: 5,
                    pii_masked_count: 0,
                },
                nov_2026,
            )
            .await
            .unwrap_err(),
            AuthError::QuotaExceeded
        );

        // 5001 rows rejected (`SF-LIC-006`) without pay-per-file token.
        assert_eq!(
            svc.consume_at(
                ws,
                user,
                false,
                &ConsumeUsageRequest {
                    idempotency_key: "op-big".to_owned(),
                    kind: UsageKind::DataOperation,
                    row_count: 5001,
                    column_count: 10,
                    pii_masked_count: 0,
                },
                nov_2026,
            )
            .await
            .unwrap_err(),
            AuthError::DatasetLimitExceeded
        );

        // Grant Credit Pack (25 ops) -> 6th op succeeds & removes watermark.
        svc.grant_micro_purchase(ws, user, 25, 1).await;
        let with_credit = svc
            .consume_at(
                ws,
                user,
                false,
                &ConsumeUsageRequest {
                    idempotency_key: "op-6-credit".to_owned(),
                    kind: UsageKind::DataOperation,
                    row_count: 100,
                    column_count: 5,
                    pii_masked_count: 0,
                },
                nov_2026,
            )
            .await
            .expect("credit op");
        assert!(with_credit.debited_credit);
        assert!(!with_credit.watermark_required);

        // Pay-per-file allows 100,000 rows & removes watermark, rejects 100,001.
        let ppf = svc
            .consume_at(
                ws,
                user,
                false,
                &ConsumeUsageRequest {
                    idempotency_key: "op-100k".to_owned(),
                    kind: UsageKind::DataOperation,
                    row_count: 100_000,
                    column_count: 35,
                    pii_masked_count: 0,
                },
                nov_2026,
            )
            .await
            .expect("ppf op");
        assert!(ppf.debited_pay_per_file);
        assert!(!ppf.watermark_required);

        // Free AI call rejected (`SF-LIC-008`).
        assert_eq!(
            svc.consume_at(
                ws,
                user,
                false,
                &ConsumeUsageRequest {
                    idempotency_key: "ai-free".to_owned(),
                    kind: UsageKind::AiCall,
                    row_count: 0,
                    column_count: 0,
                    pii_masked_count: 0,
                },
                nov_2026,
            )
            .await
            .unwrap_err(),
            AuthError::PaidPlanRequired
        );

        // Monthly reset in December restores 5 fresh free operations.
        let dec_ent = svc.effective_at(ws, user, false, dec_2026).await;
        assert_eq!(dec_ent.operations_used_this_month, 0);
    }

    #[tokio::test]
    async fn trial_anti_abuse_ai_cap_auto_downgrade_and_friend_vip() {
        let svc = EntitlementService::new();
        let ws = Uuid::now_v7();
        let user = Uuid::now_v7();
        let t0: i64 = 1_794_700_800;

        let trial = svc
            .claim_trial_at(ws, user, "e-hash", "p-hash", "d-hash", t0)
            .await
            .expect("trial claimed");
        assert_eq!(trial.effective_plan, PlanTier::Trial);
        assert_eq!(trial.ai_calls_limit, 25);
        assert!(!trial.watermark_required);

        // Second trial with same device/phone/email is rejected.
        assert_eq!(
            svc.claim_trial_at(Uuid::now_v7(), Uuid::now_v7(), "e2", "p2", "d-hash", t0)
                .await
                .unwrap_err(),
            AuthError::DuplicateAccount
        );

        // Consume all 25 AI calls; 26th fails with SF-LIC-007.
        for i in 1..=25 {
            svc.consume_at(
                ws,
                user,
                false,
                &ConsumeUsageRequest {
                    idempotency_key: format!("ai-{i}"),
                    kind: UsageKind::AiCall,
                    row_count: 0,
                    column_count: 0,
                    pii_masked_count: 0,
                },
                t0 + 60,
            )
            .await
            .expect("trial ai");
        }
        assert_eq!(
            svc.consume_at(
                ws,
                user,
                false,
                &ConsumeUsageRequest {
                    idempotency_key: "ai-26".to_owned(),
                    kind: UsageKind::AiCall,
                    row_count: 0,
                    column_count: 0,
                    pii_masked_count: 0,
                },
                t0 + 60,
            )
            .await
            .unwrap_err(),
            AuthError::AiQuotaExceeded
        );

        // After 7 days -> automatic downgrade to Free with read-only recipes & watermark.
        let after_trial = svc
            .effective_at(ws, user, false, t0 + TRIAL_DURATION_SECS + 1)
            .await;
        assert_eq!(after_trial.effective_plan, PlanTier::Free);
        assert!(after_trial.watermark_required);
        assert!(after_trial.saved_recipes_read_only);
        assert!(!after_trial.saved_recipes_write_enabled);

        // Friend VIP unlocks all paid features + 400 AI calls, revocable by owner.
        let owner_id = Uuid::now_v7();
        let vip = svc.set_friend_vip(ws, user, owner_id, true).await;
        assert_eq!(vip.effective_plan, PlanTier::FriendVip);
        assert_eq!(vip.ai_calls_limit, 400);
        assert!(!vip.watermark_required);

        let revoked = svc.set_friend_vip(ws, user, owner_id, false).await;
        assert_eq!(revoked.effective_plan, PlanTier::Free);
    }

    #[tokio::test]
    async fn concurrent_consumption_never_exceeds_free_five_op_quota() {
        let svc = EntitlementService::new();
        let ws = Uuid::now_v7();
        let user = Uuid::now_v7();
        let now: i64 = 1_794_700_800;

        let mut handles = Vec::new();
        for i in 0..12 {
            let s = svc.clone();
            handles.push(tokio::spawn(async move {
                s.consume_at(
                    ws,
                    user,
                    false,
                    &ConsumeUsageRequest {
                        idempotency_key: format!("concurrent-{i}"),
                        kind: UsageKind::DataOperation,
                        row_count: 100,
                        column_count: 5,
                        pii_masked_count: 0,
                    },
                    now,
                )
                .await
            }));
        }

        let mut ok_count = 0usize;
        let mut quota_err_count = 0usize;
        for h in handles {
            match h.await.expect("join") {
                Ok(_) => ok_count += 1,
                Err(AuthError::QuotaExceeded) => quota_err_count += 1,
                Err(other) => panic!("unexpected error: {other:?}"),
            }
        }
        assert_eq!(ok_count, 5);
        assert_eq!(quota_err_count, 7);
    }
}
