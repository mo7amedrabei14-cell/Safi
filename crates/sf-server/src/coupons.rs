//! Owner-controlled Promotions & Server-Authoritative Coupon Engine (Section 6.1).
//!
//! Security & integrity invariants:
//! - Only the sealed Platform Owner (with elevated/step-up TOTP) may create, edit,
//!   enable/disable, or archive coupons.
//! - All discount calculations occur strictly on the server (never trusting client prices).
//! - Prevents expired redemption, usage-limit overflow, double redemption by the same user,
//!   ineligible plan/customer/org redemption, below-minimum purchase, and concurrent races.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::auth::unix_now;
use crate::billing::{BillingCurrency, CheckoutProduct};
use crate::error::AuthError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscountKind {
    Percent,
    FixedUsd,
    FixedEgp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CouponRecord {
    pub id: Uuid,
    pub code: String,
    pub discount_kind: DiscountKind,
    /// For `Percent`: `1..=100`. For `FixedUsd`/`FixedEgp`: minor units (cents/piastres).
    pub discount_value: u64,
    pub enabled: bool,
    pub archived: bool,
    pub starts_at: Option<i64>,
    pub expires_at: Option<i64>,
    pub max_redemptions: Option<u32>,
    pub redemptions_count: u32,
    pub eligible_plans: Vec<CheckoutProduct>,
    pub eligible_user_ids: Vec<Uuid>,
    pub new_customers_only: bool,
    pub org_only: bool,
    pub min_purchase_minor: u64,
    pub created_by: Uuid,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateCouponRequest {
    pub code: String,
    pub discount_kind: DiscountKind,
    pub discount_value: u64,
    pub starts_at: Option<i64>,
    pub expires_at: Option<i64>,
    pub max_redemptions: Option<u32>,
    #[serde(default)]
    pub eligible_plans: Vec<CheckoutProduct>,
    #[serde(default)]
    pub eligible_user_ids: Vec<Uuid>,
    #[serde(default)]
    pub new_customers_only: bool,
    #[serde(default)]
    pub org_only: bool,
    #[serde(default)]
    pub min_purchase_minor: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateCouponRequest {
    pub enabled: Option<bool>,
    pub archived: Option<bool>,
    pub expires_at: Option<i64>,
    pub max_redemptions: Option<u32>,
    pub discount_value: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CouponRedemptionRecord {
    pub id: Uuid,
    pub coupon_id: Uuid,
    pub code: String,
    pub user_id: Uuid,
    pub workspace_id: Uuid,
    pub product: CheckoutProduct,
    pub original_amount_minor: u64,
    pub discount_amount_minor: u64,
    pub final_amount_minor: u64,
    pub currency: BillingCurrency,
    pub redeemed_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PreviewCouponRequest {
    pub code: String,
    pub product: CheckoutProduct,
    pub billing_country_iso2: String,
    pub seats: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CouponPreviewResponse {
    pub code: String,
    pub currency: BillingCurrency,
    pub original_amount_minor: u64,
    pub discount_amount_minor: u64,
    pub final_amount_minor: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CouponReservationRecord {
    pub id: Uuid,
    pub coupon_id: Uuid,
    pub code: String,
    pub user_id: Uuid,
    pub workspace_id: Uuid,
    pub order_id: Uuid,
    pub product: CheckoutProduct,
    pub original_amount_minor: u64,
    pub discount_amount_minor: u64,
    pub final_amount_minor: u64,
    pub currency: BillingCurrency,
    pub reserved_at: i64,
    pub expires_at: i64,
    pub committed: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CouponSnapshot {
    pub coupons: Vec<CouponRecord>,
    pub redemptions: Vec<CouponRedemptionRecord>,
    #[serde(default)]
    pub reservations: Vec<CouponReservationRecord>,
}

#[derive(Debug, Default)]
struct CouponStore {
    by_code: HashMap<String, CouponRecord>,
    redemptions: Vec<CouponRedemptionRecord>,
    redeemed_pairs: HashSet<(Uuid, Uuid)>, // (coupon_id, user_id)
    reservations: HashMap<Uuid, CouponReservationRecord>,
}

#[derive(Debug, Clone, Default)]
pub struct CouponService {
    store: Arc<RwLock<CouponStore>>,
}

fn normalize_code(raw: &str) -> Result<String, AuthError> {
    let cleaned = raw.trim().to_ascii_uppercase();
    if cleaned.len() < 3
        || cleaned.len() > 40
        || !cleaned
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(AuthError::InvalidRequest);
    }
    Ok(cleaned)
}

impl CouponService {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Export full durable state for crash-safe persistence.
    pub async fn export_snapshot(&self) -> CouponSnapshot {
        let guard = self.store.read().await;
        CouponSnapshot {
            coupons: guard.by_code.values().cloned().collect(),
            redemptions: guard.redemptions.clone(),
            reservations: guard.reservations.values().cloned().collect(),
        }
    }

    /// Restore full durable state after server restart.
    pub async fn import_snapshot(&self, snap: CouponSnapshot) {
        let mut guard = self.store.write().await;
        guard.by_code.clear();
        guard.redemptions.clear();
        guard.redeemed_pairs.clear();
        guard.reservations.clear();
        for c in snap.coupons {
            guard.by_code.insert(c.code.clone(), c);
        }
        for r in snap.redemptions {
            guard.redeemed_pairs.insert((r.coupon_id, r.user_id));
            guard.redemptions.push(r);
        }
        for res in snap.reservations {
            guard.reservations.insert(res.id, res);
        }
    }

    /// Owner-only: create a new coupon definition.
    pub async fn create_coupon(
        &self,
        owner_user_id: Uuid,
        req: CreateCouponRequest,
    ) -> Result<CouponRecord, AuthError> {
        let code = normalize_code(&req.code)?;
        if req.discount_value == 0 {
            return Err(AuthError::InvalidRequest);
        }
        if req.discount_kind == DiscountKind::Percent && req.discount_value > 100 {
            return Err(AuthError::InvalidRequest);
        }
        if let (Some(s), Some(e)) = (req.starts_at, req.expires_at) {
            if e <= s {
                return Err(AuthError::InvalidRequest);
            }
        }
        let mut guard = self.store.write().await;
        if guard.by_code.contains_key(&code) {
            return Err(AuthError::DuplicateAccount);
        }
        let now = unix_now();
        let rec = CouponRecord {
            id: Uuid::now_v7(),
            code: code.clone(),
            discount_kind: req.discount_kind,
            discount_value: req.discount_value,
            enabled: true,
            archived: false,
            starts_at: req.starts_at,
            expires_at: req.expires_at,
            max_redemptions: req.max_redemptions,
            redemptions_count: 0,
            eligible_plans: req.eligible_plans,
            eligible_user_ids: req.eligible_user_ids,
            new_customers_only: req.new_customers_only,
            org_only: req.org_only,
            min_purchase_minor: req.min_purchase_minor,
            created_by: owner_user_id,
            created_at: now,
            updated_at: now,
        };
        guard.by_code.insert(code, rec.clone());
        Ok(rec)
    }

    /// Owner-only: update, enable/disable, or archive an existing coupon.
    pub async fn update_coupon(
        &self,
        raw_code: &str,
        req: UpdateCouponRequest,
    ) -> Result<CouponRecord, AuthError> {
        let code = normalize_code(raw_code)?;
        let mut guard = self.store.write().await;
        let rec = guard
            .by_code
            .get_mut(&code)
            .ok_or(AuthError::CouponInvalid)?;
        if let Some(enabled) = req.enabled {
            rec.enabled = enabled;
        }
        if let Some(archived) = req.archived {
            rec.archived = archived;
            if archived {
                rec.enabled = false;
            }
        }
        if let Some(exp) = req.expires_at {
            rec.expires_at = Some(exp);
        }
        if let Some(max_r) = req.max_redemptions {
            if max_r == 0 {
                return Err(AuthError::InvalidRequest);
            }
            rec.max_redemptions = Some(max_r);
        }
        if let Some(val) = req.discount_value {
            if val == 0 || (rec.discount_kind == DiscountKind::Percent && val > 100) {
                return Err(AuthError::InvalidRequest);
            }
            rec.discount_value = val;
        }
        rec.updated_at = unix_now();
        Ok(rec.clone())
    }

    /// List all coupons for the Owner Control Center.
    pub async fn list_coupons(&self) -> Vec<CouponRecord> {
        let mut out: Vec<CouponRecord> =
            self.store.read().await.by_code.values().cloned().collect();
        out.sort_by_key(|a| std::cmp::Reverse(a.created_at));
        out
    }

    /// List redemption history for the Owner Control Center.
    pub async fn list_redemptions(&self) -> Vec<CouponRedemptionRecord> {
        self.store
            .read()
            .await
            .redemptions
            .iter()
            .rev()
            .cloned()
            .collect()
    }

    /// Validate and preview a coupon discount on the server without mutating redemption counts.
    #[allow(clippy::too_many_arguments)]
    pub async fn preview_at(
        &self,
        raw_code: &str,
        user_id: Uuid,
        product: CheckoutProduct,
        original_amount_minor: u64,
        currency: BillingCurrency,
        is_new_customer: bool,
        now_secs: i64,
    ) -> Result<CouponPreviewResponse, AuthError> {
        let code = normalize_code(raw_code).map_err(|_| AuthError::CouponInvalid)?;
        let guard = self.store.read().await;
        let coupon = guard.by_code.get(&code).ok_or(AuthError::CouponInvalid)?;
        if !coupon.enabled || coupon.archived {
            return Err(AuthError::CouponInvalid);
        }
        if let Some(starts_at) = coupon.starts_at {
            if now_secs < starts_at {
                return Err(AuthError::CouponInvalid);
            }
        }
        if let Some(expires_at) = coupon.expires_at {
            if now_secs >= expires_at {
                return Err(AuthError::CouponInvalid);
            }
        }
        if let Some(max_r) = coupon.max_redemptions {
            if coupon.redemptions_count >= max_r {
                return Err(AuthError::CouponInvalid);
            }
        }
        if guard.redeemed_pairs.contains(&(coupon.id, user_id)) {
            return Err(AuthError::CouponInvalid);
        }
        if coupon.new_customers_only && !is_new_customer {
            return Err(AuthError::CouponInvalid);
        }
        if coupon.org_only && product != CheckoutProduct::OrganizationSeatAnnual {
            return Err(AuthError::CouponInvalid);
        }
        if !coupon.eligible_plans.is_empty() && !coupon.eligible_plans.contains(&product) {
            return Err(AuthError::CouponInvalid);
        }
        if !coupon.eligible_user_ids.is_empty() && !coupon.eligible_user_ids.contains(&user_id) {
            return Err(AuthError::CouponInvalid);
        }
        if original_amount_minor < coupon.min_purchase_minor {
            return Err(AuthError::CouponInvalid);
        }
        let discount = match (coupon.discount_kind, currency) {
            (DiscountKind::Percent, _) => {
                original_amount_minor.saturating_mul(coupon.discount_value) / 100
            }
            (DiscountKind::FixedUsd, BillingCurrency::Usd)
            | (DiscountKind::FixedEgp, BillingCurrency::Egp) => {
                coupon.discount_value.min(original_amount_minor)
            }
            _ => return Err(AuthError::CouponInvalid),
        };
        Ok(CouponPreviewResponse {
            code: coupon.code.clone(),
            currency,
            original_amount_minor,
            discount_amount_minor: discount,
            final_amount_minor: original_amount_minor.saturating_sub(discount),
        })
    }

    /// Atomically validate and redeem a coupon on the server under an exclusive write lock,
    /// returning the exact server-computed discounted price (never trusting client prices).
    #[allow(clippy::too_many_arguments)]
    pub async fn redeem_at(
        &self,
        raw_code: &str,
        user_id: Uuid,
        workspace_id: Uuid,
        product: CheckoutProduct,
        original_amount_minor: u64,
        currency: BillingCurrency,
        is_new_customer: bool,
        now_secs: i64,
    ) -> Result<CouponRedemptionRecord, AuthError> {
        let code = normalize_code(raw_code).map_err(|_| AuthError::CouponInvalid)?;
        let mut guard = self.store.write().await;

        let coupon_snapshot = guard
            .by_code
            .get(&code)
            .cloned()
            .ok_or(AuthError::CouponInvalid)?;

        if !coupon_snapshot.enabled || coupon_snapshot.archived {
            return Err(AuthError::CouponInvalid);
        }
        if let Some(starts_at) = coupon_snapshot.starts_at {
            if now_secs < starts_at {
                return Err(AuthError::CouponInvalid);
            }
        }
        if let Some(expires_at) = coupon_snapshot.expires_at {
            if now_secs >= expires_at {
                return Err(AuthError::CouponInvalid);
            }
        }
        if let Some(max_r) = coupon_snapshot.max_redemptions {
            if coupon_snapshot.redemptions_count >= max_r {
                return Err(AuthError::CouponInvalid);
            }
        }
        if guard
            .redeemed_pairs
            .contains(&(coupon_snapshot.id, user_id))
        {
            return Err(AuthError::CouponInvalid);
        }
        if coupon_snapshot.new_customers_only && !is_new_customer {
            return Err(AuthError::CouponInvalid);
        }
        if coupon_snapshot.org_only && product != CheckoutProduct::OrganizationSeatAnnual {
            return Err(AuthError::CouponInvalid);
        }
        if !coupon_snapshot.eligible_plans.is_empty()
            && !coupon_snapshot.eligible_plans.contains(&product)
        {
            return Err(AuthError::CouponInvalid);
        }
        if !coupon_snapshot.eligible_user_ids.is_empty()
            && !coupon_snapshot.eligible_user_ids.contains(&user_id)
        {
            return Err(AuthError::CouponInvalid);
        }
        if original_amount_minor < coupon_snapshot.min_purchase_minor {
            return Err(AuthError::CouponInvalid);
        }

        let discount_amount = match (coupon_snapshot.discount_kind, currency) {
            (DiscountKind::Percent, _) => {
                (original_amount_minor.saturating_mul(coupon_snapshot.discount_value)) / 100
            }
            (DiscountKind::FixedUsd, BillingCurrency::Usd) => {
                coupon_snapshot.discount_value.min(original_amount_minor)
            }
            (DiscountKind::FixedEgp, BillingCurrency::Egp) => {
                coupon_snapshot.discount_value.min(original_amount_minor)
            }
            _ => return Err(AuthError::CouponInvalid),
        };
        let final_amount = original_amount_minor.saturating_sub(discount_amount);

        if let Some(live) = guard.by_code.get_mut(&code) {
            live.redemptions_count = live.redemptions_count.saturating_add(1);
            live.updated_at = now_secs;
        }
        guard.redeemed_pairs.insert((coupon_snapshot.id, user_id));

        let redemption = CouponRedemptionRecord {
            id: Uuid::now_v7(),
            coupon_id: coupon_snapshot.id,
            code,
            user_id,
            workspace_id,
            product,
            original_amount_minor,
            discount_amount_minor: discount_amount,
            final_amount_minor: final_amount,
            currency,
            redeemed_at: now_secs,
        };
        guard.redemptions.push(redemption.clone());
        Ok(redemption)
    }

    /// Atomically reserve a coupon slot for a specific checkout `order_id` BEFORE calling the
    /// payment provider. If provider checkout creation fails, call [`release_reservation`] so
    /// the customer's one-time coupon is never lost. Call [`commit_reservation`] upon verified payment.
    #[allow(clippy::too_many_arguments)]
    pub async fn reserve_at(
        &self,
        raw_code: &str,
        user_id: Uuid,
        workspace_id: Uuid,
        order_id: Uuid,
        product: CheckoutProduct,
        original_amount_minor: u64,
        currency: BillingCurrency,
        is_new_customer: bool,
        now_secs: i64,
        ttl_secs: i64,
    ) -> Result<CouponReservationRecord, AuthError> {
        let code = normalize_code(raw_code).map_err(|_| AuthError::CouponInvalid)?;
        let mut guard = self.store.write().await;

        // Prune expired uncommitted reservations before evaluating limits.
        guard
            .reservations
            .retain(|_, r| r.committed || r.expires_at > now_secs);

        // Idempotent hit: if this exact order already reserved this coupon, return it.
        if let Some(existing) = guard
            .reservations
            .values()
            .find(|r| r.order_id == order_id && r.code == code)
            .cloned()
        {
            return Ok(existing);
        }

        let coupon_snapshot = guard
            .by_code
            .get(&code)
            .cloned()
            .ok_or(AuthError::CouponInvalid)?;

        if !coupon_snapshot.enabled || coupon_snapshot.archived {
            return Err(AuthError::CouponInvalid);
        }
        if let Some(starts_at) = coupon_snapshot.starts_at {
            if now_secs < starts_at {
                return Err(AuthError::CouponInvalid);
            }
        }
        if let Some(expires_at) = coupon_snapshot.expires_at {
            if now_secs >= expires_at {
                return Err(AuthError::CouponInvalid);
            }
        }
        let active_reservations_for_coupon = guard
            .reservations
            .values()
            .filter(|r| {
                r.coupon_id == coupon_snapshot.id && !r.committed && r.expires_at > now_secs
            })
            .count();
        if let Some(max_r) = coupon_snapshot.max_redemptions {
            let used_plus_reserved = u64::from(coupon_snapshot.redemptions_count)
                + active_reservations_for_coupon as u64;
            if used_plus_reserved >= u64::from(max_r) {
                return Err(AuthError::CouponInvalid);
            }
        }
        if guard
            .redeemed_pairs
            .contains(&(coupon_snapshot.id, user_id))
            || guard.reservations.values().any(|r| {
                r.coupon_id == coupon_snapshot.id
                    && r.user_id == user_id
                    && (r.committed || r.expires_at > now_secs)
            })
        {
            return Err(AuthError::CouponInvalid);
        }
        if coupon_snapshot.new_customers_only && !is_new_customer {
            return Err(AuthError::CouponInvalid);
        }
        if coupon_snapshot.org_only && product != CheckoutProduct::OrganizationSeatAnnual {
            return Err(AuthError::CouponInvalid);
        }
        if !coupon_snapshot.eligible_plans.is_empty()
            && !coupon_snapshot.eligible_plans.contains(&product)
        {
            return Err(AuthError::CouponInvalid);
        }
        if !coupon_snapshot.eligible_user_ids.is_empty()
            && !coupon_snapshot.eligible_user_ids.contains(&user_id)
        {
            return Err(AuthError::CouponInvalid);
        }
        if original_amount_minor < coupon_snapshot.min_purchase_minor {
            return Err(AuthError::CouponInvalid);
        }

        let discount_amount = match (coupon_snapshot.discount_kind, currency) {
            (DiscountKind::Percent, _) => {
                (original_amount_minor.saturating_mul(coupon_snapshot.discount_value)) / 100
            }
            (DiscountKind::FixedUsd, BillingCurrency::Usd)
            | (DiscountKind::FixedEgp, BillingCurrency::Egp) => {
                coupon_snapshot.discount_value.min(original_amount_minor)
            }
            _ => return Err(AuthError::CouponInvalid),
        };
        let final_amount = original_amount_minor.saturating_sub(discount_amount);

        let reservation = CouponReservationRecord {
            id: Uuid::now_v7(),
            coupon_id: coupon_snapshot.id,
            code,
            user_id,
            workspace_id,
            order_id,
            product,
            original_amount_minor,
            discount_amount_minor: discount_amount,
            final_amount_minor: final_amount,
            currency,
            reserved_at: now_secs,
            expires_at: now_secs + ttl_secs.max(60),
            committed: false,
        };
        guard
            .reservations
            .insert(reservation.id, reservation.clone());
        Ok(reservation)
    }

    /// Release an uncommitted coupon reservation when provider checkout creation fails or expires.
    pub async fn release_reservation(&self, reservation_id: Uuid) {
        let mut guard = self.store.write().await;
        if let Some(existing) = guard.reservations.get(&reservation_id) {
            if !existing.committed {
                guard.reservations.remove(&reservation_id);
            }
        }
    }

    /// Permanently commit a previously reserved coupon once payment is confirmed (idempotent).
    pub async fn commit_reservation(
        &self,
        reservation_id: Uuid,
        now_secs: i64,
    ) -> Option<CouponRedemptionRecord> {
        let mut guard = self.store.write().await;
        let (already_committed, coupon_id, user_id) = {
            let res = guard.reservations.get(&reservation_id)?;
            (res.committed, res.coupon_id, res.user_id)
        };
        if already_committed {
            return guard
                .redemptions
                .iter()
                .find(|r| r.coupon_id == coupon_id && r.user_id == user_id)
                .cloned();
        }
        let snapshot = {
            let res = guard.reservations.get_mut(&reservation_id)?;
            res.committed = true;
            res.clone()
        };
        if let Some(live) = guard.by_code.get_mut(&snapshot.code) {
            live.redemptions_count = live.redemptions_count.saturating_add(1);
            live.updated_at = now_secs;
        }
        guard
            .redeemed_pairs
            .insert((snapshot.coupon_id, snapshot.user_id));
        let redemption = CouponRedemptionRecord {
            id: Uuid::now_v7(),
            coupon_id: snapshot.coupon_id,
            code: snapshot.code,
            user_id: snapshot.user_id,
            workspace_id: snapshot.workspace_id,
            product: snapshot.product,
            original_amount_minor: snapshot.original_amount_minor,
            discount_amount_minor: snapshot.discount_amount_minor,
            final_amount_minor: snapshot.final_amount_minor,
            currency: snapshot.currency,
            redeemed_at: now_secs,
        };
        guard.redemptions.push(redemption.clone());
        Some(redemption)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn coupon_lifecycle_expiration_limits_eligibility_and_concurrent_race() {
        let svc = CouponService::new();
        let owner = Uuid::now_v7();
        let now: i64 = 1_795_000_000;

        svc.create_coupon(
            owner,
            CreateCouponRequest {
                code: "LAUNCH25".to_owned(),
                discount_kind: DiscountKind::Percent,
                discount_value: 25,
                starts_at: Some(now - 60),
                expires_at: Some(now + 3600),
                max_redemptions: Some(2),
                eligible_plans: vec![CheckoutProduct::IndividualMonthly],
                eligible_user_ids: vec![],
                new_customers_only: true,
                org_only: false,
                min_purchase_minor: 1_000,
            },
        )
        .await
        .expect("create coupon");

        // Ineligible plan rejected.
        assert_eq!(
            svc.redeem_at(
                "LAUNCH25",
                Uuid::now_v7(),
                Uuid::now_v7(),
                CheckoutProduct::PayPerFile,
                200,
                BillingCurrency::Usd,
                true,
                now
            )
            .await
            .unwrap_err(),
            AuthError::CouponInvalid
        );

        // Existing customer rejected on new_customers_only coupon.
        assert_eq!(
            svc.redeem_at(
                "LAUNCH25",
                Uuid::now_v7(),
                Uuid::now_v7(),
                CheckoutProduct::IndividualMonthly,
                2_000,
                BillingCurrency::Usd,
                false,
                now
            )
            .await
            .unwrap_err(),
            AuthError::CouponInvalid
        );

        // Expired timestamp rejected.
        assert_eq!(
            svc.redeem_at(
                "LAUNCH25",
                Uuid::now_v7(),
                Uuid::now_v7(),
                CheckoutProduct::IndividualMonthly,
                2_000,
                BillingCurrency::Usd,
                true,
                now + 4000
            )
            .await
            .unwrap_err(),
            AuthError::CouponInvalid
        );

        // 8 concurrent users race for max_redemptions = 2 -> EXACTLY 2 succeed!
        let mut handles = Vec::new();
        for _ in 0..8 {
            let c = svc.clone();
            handles.push(tokio::spawn(async move {
                c.redeem_at(
                    "launch25",
                    Uuid::now_v7(),
                    Uuid::now_v7(),
                    CheckoutProduct::IndividualMonthly,
                    2_000,
                    BillingCurrency::Usd,
                    true,
                    now,
                )
                .await
            }));
        }
        let mut ok_count = 0usize;
        let mut err_count = 0usize;
        for h in handles {
            match h.await.expect("join") {
                Ok(r) => {
                    assert_eq!(r.discount_amount_minor, 500);
                    assert_eq!(r.final_amount_minor, 1_500);
                    ok_count += 1;
                }
                Err(AuthError::CouponInvalid) => err_count += 1,
                Err(other) => panic!("unexpected err: {other:?}"),
            }
        }
        assert_eq!(ok_count, 2);
        assert_eq!(err_count, 6);
    }
}
