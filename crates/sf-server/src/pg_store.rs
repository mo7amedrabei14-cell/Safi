//! Durable Persistence Layer for SafiData Central Owner Database (Section 8).
//!
//! Provides:
//! 1. `PgControlPlaneStore`: Real SQLx PostgreSQL 18 persistence bound to migrations
//!    `0007..0010` (`platform_owner`, `owner_totp_credentials`, `owner_audit_log`,
//!    `feature_flags`, `system_settings`, `coupons`, `coupon_redemptions`,
//!    `device_replacement_events`, `users.session_generation`, `support_tickets`,
//!    `platform_announcements`) used whenever `DATABASE_URL` is configured.
//! 2. `DurableStateJournal`: Crash-safe atomic (temp -> `sync_all` -> rename) disk journal
//!    used for local restart-durability verification when a live PostgreSQL 18 daemon is
//!    unavailable on a local developer workstation.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::repository::InMemorySessionSnapshot;
use crate::billing::BillingSnapshot;
use crate::coupons::{CouponRecord, CouponRedemptionRecord, CouponSnapshot};
use crate::device_authority::{DeviceAuthoritySnapshot, DeviceReplacementEvent};
use crate::entitlements::EntitlementSnapshot;
use crate::error::AuthError;
use crate::log_center::AuditLogEntry;
use crate::owner_mfa::OwnerMfaSnapshot;
use crate::{FeatureFlag, ManagedAccountRecord, PlatformAnnouncement, SupportTicket};

/// Real PostgreSQL 18 SQLx repository for control-plane durability (migrations `0007..0010`).
#[derive(Debug, Clone)]
pub struct PgControlPlaneStore {
    pool: PgPool,
}

impl PgControlPlaneStore {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Persist a hash-chained Owner audit log entry to `owner_audit_log`.
    pub async fn insert_audit_entry(&self, entry: &AuditLogEntry) -> Result<(), AuthError> {
        let outcome_str = match entry.outcome {
            crate::log_center::AuditOutcome::Allowed => "allowed",
            crate::log_center::AuditOutcome::Denied => "denied",
            crate::log_center::AuditOutcome::Error => "error",
        };
        sqlx::query(
            r"
            INSERT INTO owner_audit_log (
                id, actor_user_id, action, target_type, target_id,
                outcome, before_state, after_state, previous_hash, current_hash
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (current_hash) DO NOTHING
            ",
        )
        .bind(entry.id)
        .bind(entry.actor_user_id)
        .bind(&entry.action)
        .bind(&entry.target_type)
        .bind(&entry.target_id)
        .bind(outcome_str)
        .bind(&entry.before_state)
        .bind(&entry.after_state)
        .bind(&entry.previous_hash)
        .bind(&entry.current_hash)
        .execute(&self.pool)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
        Ok(())
    }

    /// Upsert a feature flag in `feature_flags`.
    pub async fn upsert_feature_flag(
        &self,
        flag: &FeatureFlag,
        actor_user_id: Uuid,
    ) -> Result<(), AuthError> {
        sqlx::query(
            r"
            INSERT INTO feature_flags (flag_key, enabled, description, updated_by, updated_at)
            VALUES ($1, $2, $3, $4, now())
            ON CONFLICT (flag_key) DO UPDATE
            SET enabled = EXCLUDED.enabled,
                description = EXCLUDED.description,
                updated_by = EXCLUDED.updated_by,
                updated_at = now()
            ",
        )
        .bind(&flag.key)
        .bind(flag.enabled)
        .bind(&flag.description)
        .bind(actor_user_id)
        .execute(&self.pool)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
        Ok(())
    }

    /// Upsert a system setting (such as `maintenance_mode`) in `system_settings`.
    pub async fn upsert_system_setting(
        &self,
        key: &str,
        value: &Value,
        actor_user_id: Uuid,
    ) -> Result<(), AuthError> {
        sqlx::query(
            r"
            INSERT INTO system_settings (setting_key, setting_value, updated_by, updated_at)
            VALUES ($1, $2, $3, now())
            ON CONFLICT (setting_key) DO UPDATE
            SET setting_value = EXCLUDED.setting_value,
                updated_by = EXCLUDED.updated_by,
                updated_at = now()
            ",
        )
        .bind(key)
        .bind(value)
        .bind(actor_user_id)
        .execute(&self.pool)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
        Ok(())
    }

    /// Atomically increment `users.session_generation` and insert into `device_replacement_events`.
    pub async fn persist_device_replacement(
        &self,
        event: &DeviceReplacementEvent,
    ) -> Result<(), AuthError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("UPDATE users SET session_generation = $2 WHERE id = $1")
            .bind(event.user_id)
            .bind(i64::try_from(event.new_session_generation).unwrap_or(1))
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query(
            r"
            INSERT INTO device_replacement_events (
                id, user_id, previous_device_id, new_device_id, trigger_kind,
                actor_user_id, reason, new_session_generation
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ",
        )
        .bind(event.id)
        .bind(event.user_id)
        .bind(event.previous_device_id)
        .bind(event.new_device_id)
        .bind(&event.trigger_kind)
        .bind(event.actor_user_id)
        .bind(&event.reason)
        .bind(i64::try_from(event.new_session_generation).unwrap_or(1))
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        Ok(())
    }

    /// Persist a coupon definition in `coupons`.
    pub async fn upsert_coupon(&self, coupon: &CouponRecord) -> Result<(), AuthError> {
        let kind = match coupon.discount_kind {
            crate::coupons::DiscountKind::Percent => "percent",
            crate::coupons::DiscountKind::FixedUsd => "fixed_usd",
            crate::coupons::DiscountKind::FixedEgp => "fixed_egp",
        };
        let plans_json =
            serde_json::to_value(&coupon.eligible_plans).unwrap_or_else(|_| Value::Array(vec![]));
        let users_json = serde_json::to_value(&coupon.eligible_user_ids)
            .unwrap_or_else(|_| Value::Array(vec![]));
        sqlx::query(
            r"
            INSERT INTO coupons (
                id, code, discount_kind, discount_value, enabled, archived,
                starts_at, expires_at, max_redemptions, redemptions_count,
                eligible_plans, eligible_user_ids, new_customers_only, org_only,
                min_purchase_minor, created_by
            )
            VALUES (
                $1, $2, $3, $4, $5, $6,
                to_timestamp($7), to_timestamp($8), $9, $10,
                $11, $12, $13, $14, $15, $16
            )
            ON CONFLICT (code) DO UPDATE
            SET enabled = EXCLUDED.enabled,
                archived = EXCLUDED.archived,
                expires_at = EXCLUDED.expires_at,
                max_redemptions = EXCLUDED.max_redemptions,
                redemptions_count = EXCLUDED.redemptions_count,
                discount_value = EXCLUDED.discount_value,
                updated_at = now()
            ",
        )
        .bind(coupon.id)
        .bind(&coupon.code)
        .bind(kind)
        .bind(i64::try_from(coupon.discount_value).unwrap_or(0))
        .bind(coupon.enabled)
        .bind(coupon.archived)
        .bind(coupon.starts_at)
        .bind(coupon.expires_at)
        .bind(
            coupon
                .max_redemptions
                .map(|v| i32::try_from(v).unwrap_or(i32::MAX)),
        )
        .bind(i32::try_from(coupon.redemptions_count).unwrap_or(0))
        .bind(plans_json)
        .bind(users_json)
        .bind(coupon.new_customers_only)
        .bind(coupon.org_only)
        .bind(i64::try_from(coupon.min_purchase_minor).unwrap_or(0))
        .bind(coupon.created_by)
        .execute(&self.pool)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
        Ok(())
    }

    /// Persist a coupon redemption atomically in `coupon_redemptions` under workspace RLS.
    pub async fn insert_coupon_redemption(
        &self,
        redemption: &CouponRedemptionRecord,
    ) -> Result<(), AuthError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        sqlx::query("SELECT set_config('app.current_workspace_id', $1, true)")
            .bind(redemption.workspace_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        let currency = match redemption.currency {
            crate::billing::BillingCurrency::Usd => "USD",
            crate::billing::BillingCurrency::Egp => "EGP",
        };
        let plan_code = serde_json::to_string(&redemption.product).unwrap_or_default();
        sqlx::query(
            r"
            INSERT INTO coupon_redemptions (
                id, coupon_id, user_id, workspace_id, plan_code,
                original_amount_minor, discount_amount_minor, final_amount_minor, currency
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ",
        )
        .bind(redemption.id)
        .bind(redemption.coupon_id)
        .bind(redemption.user_id)
        .bind(redemption.workspace_id)
        .bind(plan_code)
        .bind(i64::try_from(redemption.original_amount_minor).unwrap_or(0))
        .bind(i64::try_from(redemption.discount_amount_minor).unwrap_or(0))
        .bind(i64::try_from(redemption.final_amount_minor).unwrap_or(0))
        .bind(currency)
        .execute(&mut *tx)
        .await
        .map_err(|_| AuthError::CouponInvalid)?;
        tx.commit()
            .await
            .map_err(|_| AuthError::BackendUnavailable)?;
        Ok(())
    }

    /// Persist a platform announcement in `platform_announcements`.
    pub async fn insert_announcement(
        &self,
        announcement: &PlatformAnnouncement,
        actor_user_id: Uuid,
    ) -> Result<(), AuthError> {
        sqlx::query(
            r"
            INSERT INTO platform_announcements (id, title, body, severity, published, created_by)
            VALUES ($1, $2, $3, $4, $5, $6)
            ",
        )
        .bind(announcement.id)
        .bind(&announcement.title)
        .bind(&announcement.body)
        .bind(&announcement.severity)
        .bind(announcement.published)
        .bind(actor_user_id)
        .execute(&self.pool)
        .await
        .map_err(|_| AuthError::BackendUnavailable)?;
        Ok(())
    }
}

/// Complete durable server state snapshot for atomic local restart persistence.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DurableServerSnapshot {
    pub device_authority: DeviceAuthoritySnapshot,
    pub coupons: CouponSnapshot,
    #[serde(default)]
    pub entitlements: EntitlementSnapshot,
    #[serde(default)]
    pub billing: BillingSnapshot,
    #[serde(default)]
    pub owner_mfa: OwnerMfaSnapshot,
    #[serde(default)]
    pub sessions: InMemorySessionSnapshot,
    pub maintenance_mode: bool,
    pub maintenance_reason: Option<String>,
    pub feature_flags: BTreeMap<String, FeatureFlag>,
    pub managed_accounts: Vec<ManagedAccountRecord>,
    pub support_tickets: Vec<SupportTicket>,
    pub announcements: Vec<PlatformAnnouncement>,
    pub audit_entries: Vec<AuditLogEntry>,
}

/// Crash-safe atomic (temp file -> `sync_all` -> rename) durable file journal.
#[derive(Debug, Clone)]
pub struct DurableStateJournal {
    path: PathBuf,
}

impl DurableStateJournal {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Atomically write the server state snapshot with `sync_all` + rename.
    pub fn write_atomic(&self, snapshot: &DurableServerSnapshot) -> Result<(), AuthError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|_| AuthError::BackendUnavailable)?;
        }
        let encoded =
            serde_json::to_vec_pretty(snapshot).map_err(|_| AuthError::BackendUnavailable)?;
        let tmp_path = self
            .path
            .with_extension(format!("tmp.{}", Uuid::new_v4().simple()));
        {
            let mut file =
                fs::File::create(&tmp_path).map_err(|_| AuthError::BackendUnavailable)?;
            file.write_all(&encoded)
                .map_err(|_| AuthError::BackendUnavailable)?;
            file.sync_all().map_err(|_| AuthError::BackendUnavailable)?;
        }
        fs::rename(&tmp_path, &self.path).map_err(|_| AuthError::BackendUnavailable)?;
        Ok(())
    }

    /// Load the last committed snapshot if it exists.
    pub fn load(&self) -> Result<Option<DurableServerSnapshot>, AuthError> {
        if !self.path.exists() {
            return Ok(None);
        }
        let raw = fs::read(&self.path).map_err(|_| AuthError::BackendUnavailable)?;
        let snap: DurableServerSnapshot =
            serde_json::from_slice(&raw).map_err(|_| AuthError::BackendUnavailable)?;
        Ok(Some(snap))
    }
}
