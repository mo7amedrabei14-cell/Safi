//! Financial Integrity & Payment Provider Service: Paddle (International USD) + Paymob (Egypt EGP).
//!
//! Security & Financial Invariants:
//! - Real provider API creation ONLY (Paddle `POST /transactions` & Paymob `POST /v1/intention/`).
//!   Never fabricates `chk_*` sessions or checkout URLs.
//! - Durable server-side checkout idempotency keyed by `(workspace_id, idempotency_key)` with
//!   32-byte SHA-256 request fingerprint verification (`SF-PAY-007` on conflict) and per-key
//!   in-flight serialization + timeout reconciliation (`SF-PAY-008`).
//! - PostgreSQL 18 (`PgControlPlaneStore` / Migration 0013) is the authoritative financial source
//!   of truth whenever configured, with single-transaction atomic webhook processing.
//! - Paymob Trust-Boundary Enforcement: unsigned JSON fields (`workspace_id`, `user_id`, `product`,
//!   `seats`) are NEVER trusted. Incoming callbacks MUST correlate to a persisted internal order via
//!   the HMAC-signed `order_id`, validate `integration_id`, `currency`, `amount_cents`, and payment
//!   capture lifecycle, and derive all beneficiary/product data strictly from the stored order.
//! - Paddle Correlation & Deduplication: verifies HMAC-SHA256 + 5-minute anti-replay window,
//!   correlates strictly to a persisted order, validates currency/amount/price, blocks non-final
//!   checkout events from granting access, and deduplicates by BOTH `(provider, event_id)` AND
//!   `(provider, provider_transaction_id)` / `(order_id, grant_kind)`.
//! - Safe Coupon Lifecycle: reserves coupon before provider call, releases on definitive checkout
//!   failure, and commits permanently upon verified payment.
//! - Real Provider Refunds: invokes Paddle `POST /adjustments` or Paymob `POST /api/acceptance/void_refund/refund`
//!   with idempotent `refund_request_key`, honoring the 3-day window + statutory override, and only
//!   revoking entitlements once the provider confirms the refund.

pub mod pg_ledger;
pub mod provider_http;

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256, Sha512};
use subtle::ConstantTimeEq;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::auth::unix_now;
use crate::billing::pg_ledger::{PgCheckoutClaim, VerifiedWebhookKind, VerifiedWebhookTransition};
use crate::billing::provider_http::{
    CheckoutMode, CustomerBillingData, PaddleApiAdapter, PaddleCustomerContext, PaymobApiAdapter,
    ProviderCallError, DEFAULT_PROVIDER_TIMEOUT_MS,
};
use crate::config::{IntegrationStatus, ServerConfig};
use crate::coupons::CouponService;
use crate::entitlements::{EntitlementService, PlanTier, SubscriptionState, ORG_MIN_SEATS};
use crate::error::AuthError;
use crate::pg_store::PgControlPlaneStore;

type HmacSha256 = Hmac<Sha256>;
type HmacSha512 = Hmac<Sha512>;

/// Maximum age/skew tolerated for Paddle webhook timestamps (5 minutes).
pub const PADDLE_REPLAY_WINDOW_SECS: i64 = 300;
/// Approved owner refund policy window (3 days = 259,200 seconds).
pub const REFUND_WINDOW_SECS: i64 = 3 * 24 * 60 * 60;
/// Default checkout order & coupon reservation expiration (30 minutes).
pub const CHECKOUT_ORDER_TTL_SECS: i64 = 30 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BillingProvider {
    Paddle,
    Paymob,
    Manual,
}

impl BillingProvider {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Paddle => "paddle",
            Self::Paymob => "paymob",
            Self::Manual => "manual",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum BillingCurrency {
    Usd,
    Egp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckoutProduct {
    IndividualMonthly,
    IndividualAnnual,
    OrganizationSeatAnnual,
    PayPerFile,
    #[serde(rename = "credit_pack_25")]
    CreditPack25,
}

impl CheckoutProduct {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::IndividualMonthly => "individual_monthly",
            Self::IndividualAnnual => "individual_annual",
            Self::OrganizationSeatAnnual => "organization_seat_annual",
            Self::PayPerFile => "pay_per_file",
            Self::CreditPack25 => "credit_pack_25",
        }
    }

    #[must_use]
    pub fn from_str_code(raw: &str) -> Option<Self> {
        match raw.trim() {
            "individual_monthly" => Some(Self::IndividualMonthly),
            "individual_annual" => Some(Self::IndividualAnnual),
            "organization_seat_annual" => Some(Self::OrganizationSeatAnnual),
            "pay_per_file" => Some(Self::PayPerFile),
            "credit_pack_25" => Some(Self::CreditPack25),
            _ => None,
        }
    }
}

/// Validated Payment Order State Machine (Section 1.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentOrderState {
    Created,
    CheckoutPending,
    PaymentPending,
    Paid,
    Failed,
    Expired,
    Canceled,
    RefundPending,
    Refunded,
    PartiallyRefunded,
    Chargeback,
    ReconciliationRequired,
}

impl PaymentOrderState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::CheckoutPending => "checkout_pending",
            Self::PaymentPending => "payment_pending",
            Self::Paid => "paid",
            Self::Failed => "failed",
            Self::Expired => "expired",
            Self::Canceled => "canceled",
            Self::RefundPending => "refund_pending",
            Self::Refunded => "refunded",
            Self::PartiallyRefunded => "partially_refunded",
            Self::Chargeback => "chargeback",
            Self::ReconciliationRequired => "reconciliation_required",
        }
    }

    #[must_use]
    pub fn from_str_code(raw: &str) -> Option<Self> {
        match raw.trim() {
            "created" => Some(Self::Created),
            "checkout_pending" => Some(Self::CheckoutPending),
            "payment_pending" => Some(Self::PaymentPending),
            "paid" => Some(Self::Paid),
            "failed" => Some(Self::Failed),
            "expired" => Some(Self::Expired),
            "canceled" => Some(Self::Canceled),
            "refund_pending" => Some(Self::RefundPending),
            "refunded" => Some(Self::Refunded),
            "partially_refunded" => Some(Self::PartiallyRefunded),
            "chargeback" => Some(Self::Chargeback),
            "reconciliation_required" => Some(Self::ReconciliationRequired),
            _ => None,
        }
    }
}

/// Durable internal payment order record correlating checkout idempotency with provider references.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingOrderRecord {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub idempotency_key: String,
    pub request_fingerprint: [u8; 32],
    pub provider: BillingProvider,
    pub product: CheckoutProduct,
    pub provider_price_id: String,
    pub seats: u32,
    pub currency: BillingCurrency,
    pub original_amount_minor: u64,
    pub discount_amount_minor: u64,
    pub expected_amount_minor: u64,
    pub coupon_code: Option<String>,
    pub coupon_reservation_id: Option<Uuid>,
    pub expected_integration_id: Option<u64>,
    pub provider_order_ref: Option<String>,
    pub provider_intention_id: Option<String>,
    #[serde(default)]
    pub provider_subscription_id: Option<String>,
    #[serde(default)]
    pub checkout_mode: CheckoutMode,
    pub checkout_url: Option<String>,
    pub provider_transaction_id: Option<String>,
    pub state: PaymentOrderState,
    pub last_error_code: Option<String>,
    pub created_at: i64,
    pub expires_at: i64,
    pub updated_at: i64,
    pub paid_at: Option<i64>,
}

/// Durable financial entitlement/credit grant uniquely bound to one verified payment transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitlementGrantRecord {
    pub id: Uuid,
    pub order_id: Option<Uuid>,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub provider: BillingProvider,
    pub provider_transaction_id: String,
    #[serde(default)]
    pub provider_subscription_id: Option<String>,
    pub grant_kind: String,
    pub quantity: u32,
    #[serde(default)]
    pub period_start: Option<i64>,
    #[serde(default)]
    pub period_end: Option<i64>,
    pub revoked_at: Option<i64>,
    pub created_at: i64,
}

/// Durable idempotent refund/chargeback request ledger record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefundRecord {
    pub id: Uuid,
    pub order_id: Option<Uuid>,
    pub workspace_id: Uuid,
    pub provider: BillingProvider,
    pub external_invoice_id: String,
    pub refund_request_key: String,
    pub provider_refund_id: Option<String>,
    pub amount_minor: u64,
    pub currency: BillingCurrency,
    pub kind: String,
    pub status: String,
    pub mandatory_legal_override: bool,
    pub reason: String,
    pub requested_by: Option<Uuid>,
    pub confirmed_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CheckoutRequest {
    #[serde(default)]
    pub workspace_id: Uuid,
    #[serde(default)]
    pub user_id: Uuid,
    pub product: CheckoutProduct,
    pub billing_country_iso2: String,
    pub seats: Option<u32>,
    #[serde(default)]
    pub coupon_code: Option<String>,
    /// Client-provided idempotency key for one intentional purchase action (also accepted via `Idempotency-Key` header).
    #[serde(default)]
    pub idempotency_key: Option<String>,
    /// Verified real customer billing details REQUIRED for Paymob EGP checkout (zero fake personal data).
    #[serde(default)]
    pub billing_data: Option<CustomerBillingData>,
    /// Optional Paddle customer & billing address IDs for `ready` transactions; when omitted, Paddle
    /// returns a `draft` transaction for official Paddle.js Checkout overlay handoff.
    #[serde(default)]
    pub paddle_customer: Option<PaddleCustomerContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckoutResponse {
    pub order_id: Uuid,
    pub idempotency_key: String,
    pub order_state: PaymentOrderState,
    pub reused_existing_order: bool,
    pub provider: BillingProvider,
    pub currency: BillingCurrency,
    pub original_amount_minor: u64,
    pub discount_amount_minor: u64,
    pub amount_minor: u64,
    pub applied_coupon_code: Option<String>,
    pub provider_status: IntegrationStatus,
    pub provider_order_ref: Option<String>,
    pub checkout_session_id: Option<String>,
    #[serde(default)]
    pub checkout_mode: CheckoutMode,
    pub checkout_url: Option<String>,
}

/// Revenue and billing summary computed from actual invoices & webhook events.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BillingRevenueSummary {
    pub total_invoices: usize,
    pub paid_invoices: usize,
    pub refunded_invoices: usize,
    pub chargeback_invoices: usize,
    pub gross_usd_minor: u64,
    pub gross_egp_minor: u64,
    pub refunded_usd_minor: u64,
    pub refunded_egp_minor: u64,
    pub chargeback_usd_minor: u64,
    pub chargeback_egp_minor: u64,
}

/// Serializable snapshot of billing state for durable restart persistence.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BillingSnapshot {
    pub processed_events: Vec<(String, String)>,
    pub last_event_ts_by_workspace: Vec<(Uuid, i64)>,
    pub events: Vec<BillingEventRecord>,
    pub invoices: Vec<InvoiceRecord>,
    #[serde(default)]
    pub orders: Vec<BillingOrderRecord>,
    #[serde(default)]
    pub grants: Vec<EntitlementGrantRecord>,
    #[serde(default)]
    pub refunds: Vec<RefundRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingEventRecord {
    pub id: Uuid,
    pub provider: BillingProvider,
    pub external_event_id: String,
    pub event_type: String,
    pub workspace_id: Option<Uuid>,
    pub occurred_at: i64,
    pub processed_at: i64,
    pub outcome: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceRecord {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub provider: BillingProvider,
    pub external_invoice_id: String,
    #[serde(default)]
    pub order_id: Option<Uuid>,
    #[serde(default)]
    pub provider_transaction_id: Option<String>,
    pub amount_minor: u64,
    #[serde(default)]
    pub refunded_amount_minor: u64,
    pub currency: BillingCurrency,
    pub status: String,
    pub issued_at: i64,
    pub refunded_at: Option<i64>,
}

/// Parse an RFC 3339 / ISO-8601 UTC/offset timestamp (e.g. `2026-11-23T12:00:00.123456Z`,
/// `2026-11-23T14:00:00+02:00`, or Paymob's `2026-11-23T12:00:00.123456`) into Unix epoch seconds.
#[must_use]
pub fn parse_rfc3339_to_unix_secs(raw: &str) -> Option<i64> {
    let s = raw.trim();
    if s.len() < 19 {
        return None;
    }
    let bytes = s.as_bytes();
    if bytes[4] != b'-' || bytes[7] != b'-' || (bytes[10] != b'T' && bytes[10] != b' ') {
        return None;
    }
    if bytes[13] != b':' || bytes[16] != b':' {
        return None;
    }
    let year: i64 = s[0..4].parse().ok()?;
    let month: u32 = s[5..7].parse().ok()?;
    let day: u32 = s[8..10].parse().ok()?;
    let hour: u32 = s[11..13].parse().ok()?;
    let minute: u32 = s[14..16].parse().ok()?;
    let second: u32 = s[17..19].parse().ok()?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }

    let mut rest = &s[19..];
    if let Some(after_dot) = rest.strip_prefix('.') {
        let frac_len = after_dot.bytes().take_while(|b| b.is_ascii_digit()).count();
        rest = &after_dot[frac_len..];
    }

    let offset_secs: i64 = if rest.is_empty() || rest == "Z" || rest == "z" {
        0
    } else if rest.len() == 6 && (rest.starts_with('+') || rest.starts_with('-')) {
        let sign: i64 = if rest.starts_with('+') { 1 } else { -1 };
        let off_h: i64 = rest[1..3].parse().ok()?;
        let off_m: i64 = rest[4..6].parse().ok()?;
        if rest.as_bytes()[3] != b':' || off_h > 23 || off_m > 59 {
            return None;
        }
        sign * (off_h * 3600 + off_m * 60)
    } else {
        return None;
    };

    // Howard Hinnant's exact civil-to-days algorithm (Gregorian calendar, 1970-01-01 = 0).
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let m = u64::from(month);
    let d = u64::from(day);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + (doe as i64) - 719_468;
    let local_secs =
        days * 86_400 + i64::from(hour) * 3600 + i64::from(minute) * 60 + i64::from(second);
    Some(local_secs - offset_secs)
}

/// Official Paddle Billing Webhook Envelope (`developer.paddle.com/webhooks/about/how-webhooks-work/`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaddleWebhookEnvelope {
    pub event_id: String,
    pub event_type: String,
    /// RFC 3339 timestamp (e.g. `"2026-11-23T12:00:00.000000Z"`).
    pub occurred_at: String,
    #[serde(default)]
    pub notification_id: Option<String>,
    /// Official entity snapshot (`transaction`, `subscription`, `adjustment`, etc.).
    pub data: Value,
}

/// Official Paymob nested `obj.order` object inside a Transaction Processed Callback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymobOfficialOrder {
    pub id: u64,
    #[serde(default)]
    pub merchant_order_id: Option<String>,
    #[serde(default)]
    pub amount_cents: Option<u64>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub data: Option<Value>,
}

/// Official Paymob nested `obj.source_data` object inside a Transaction Processed Callback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymobOfficialSourceData {
    pub pan: String,
    pub sub_type: String,
    #[serde(rename = "type")]
    pub source_type: String,
}

/// Official Paymob Transaction Processed Callback `obj` payload
/// (`developers.paymob.com/paymob-docs/developers/webhook-callbacks-and-hmac/transaction-callbacks`).
///
/// SECURITY: The 20 official nested fields are covered by Paymob's HMAC-SHA512 signature.
/// Beneficiary (`workspace_id`, `user_id`, `product`, `seats`) is ALWAYS derived from the
/// trusted server-created `billing_orders` row correlated by the HMAC-signed `obj.order.id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymobOfficialTransactionObj {
    pub id: u64,
    pub pending: bool,
    pub amount_cents: u64,
    pub success: bool,
    pub is_auth: bool,
    pub is_capture: bool,
    pub is_standalone_payment: bool,
    pub is_voided: bool,
    pub is_refunded: bool,
    /// Official Paymob cumulative refunded amount in minor units (piastres).
    #[serde(default)]
    pub refunded_amount_cents: u64,
    pub is_3d_secure: bool,
    pub integration_id: u64,
    #[serde(default)]
    pub profile_id: Option<u64>,
    pub has_parent_transaction: bool,
    pub order: PaymobOfficialOrder,
    pub created_at: String,
    pub currency: String,
    pub source_data: PaymobOfficialSourceData,
    pub error_occured: bool,
    pub owner: u64,
    #[serde(default)]
    pub is_chargeback: bool,
    /// Optional unsigned provider `data` / `extras` bag — NEVER trusted; conflicting fields are rejected!
    #[serde(default)]
    pub data: Option<Value>,
}

/// Official Paymob Transaction Processed Callback JSON Envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymobCallbackEnvelope {
    #[serde(rename = "type")]
    pub callback_type: String,
    pub obj: PaymobOfficialTransactionObj,
    #[serde(default)]
    pub issuer_bank: Option<Value>,
    #[serde(default)]
    pub transaction_processed_callback_responses: Option<Value>,
}

/// Compute deterministic 32-byte SHA-256 fingerprint for checkout idempotency conflict detection.
#[must_use]
pub fn compute_checkout_request_fingerprint(
    workspace_id: Uuid,
    user_id: Uuid,
    product: CheckoutProduct,
    seats: u32,
    billing_country_iso2: &str,
    coupon_code: Option<&str>,
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"safidata.checkout.v2:");
    h.update(workspace_id.as_bytes());
    h.update(user_id.as_bytes());
    h.update(product.as_str().as_bytes());
    h.update(b":");
    h.update(seats.to_be_bytes());
    h.update(b":");
    h.update(billing_country_iso2.trim().to_ascii_uppercase().as_bytes());
    h.update(b":");
    if let Some(code) = coupon_code {
        h.update(code.trim().to_ascii_uppercase().as_bytes());
    }
    let digest = h.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

/// Verify Paddle's `Paddle-Signature: ts=<unix>;h1=<hex>` header with constant-time HMAC-SHA256
/// and replay-window enforcement.
pub fn verify_paddle_signature(
    secret: &str,
    signature_header: &str,
    raw_body: &[u8],
    now_secs: i64,
) -> Result<(), AuthError> {
    if secret.trim().is_empty() {
        return Err(AuthError::BillingNotConfigured);
    }

    let mut ts_val: Option<i64> = None;
    let mut h1_hex: Option<&str> = None;
    for part in signature_header.split(';') {
        let trimmed = part.trim();
        if let Some(raw_ts) = trimmed.strip_prefix("ts=") {
            ts_val = raw_ts.parse::<i64>().ok();
        } else if let Some(raw_h1) = trimmed.strip_prefix("h1=") {
            h1_hex = Some(raw_h1);
        }
    }

    let ts = ts_val.ok_or(AuthError::InvalidWebhookSignature)?;
    let presented_hex = h1_hex.ok_or(AuthError::InvalidWebhookSignature)?;
    let presented_bytes =
        hex::decode(presented_hex).map_err(|_| AuthError::InvalidWebhookSignature)?;

    if (now_secs - ts).abs() > PADDLE_REPLAY_WINDOW_SECS {
        return Err(AuthError::WebhookReplayWindow);
    }

    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| AuthError::BillingNotConfigured)?;
    mac.update(ts.to_string().as_bytes());
    mac.update(b":");
    mac.update(raw_body);
    let expected = mac.finalize().into_bytes();

    if expected
        .as_slice()
        .ct_eq(presented_bytes.as_slice())
        .unwrap_u8()
        != 1
    {
        return Err(AuthError::InvalidWebhookSignature);
    }
    Ok(())
}

/// Compute a genuine Paddle `Paddle-Signature` header (used by contract/simulator tests).
pub fn sign_paddle_payload(secret: &str, ts: i64, raw_body: &[u8]) -> Result<String, AuthError> {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| AuthError::BillingNotConfigured)?;
    mac.update(ts.to_string().as_bytes());
    mac.update(b":");
    mac.update(raw_body);
    let digest = hex::encode(mac.finalize().into_bytes());
    Ok(format!("ts={ts};h1={digest}"))
}

/// Build the official Paymob HMAC-SHA512 concatenation string (20 sorted nested fields per
/// `developers.paymob.com/paymob-docs/developers/webhook-callbacks-and-hmac/hmac`).
#[must_use]
pub fn paymob_hmac_concat_string(tx: &PaymobOfficialTransactionObj) -> String {
    format!(
        "{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}",
        tx.amount_cents,
        tx.created_at,
        tx.currency,
        tx.error_occured,
        tx.has_parent_transaction,
        tx.id,
        tx.integration_id,
        tx.is_3d_secure,
        tx.is_auth,
        tx.is_capture,
        tx.is_refunded,
        tx.is_standalone_payment,
        tx.is_voided,
        tx.order.id,
        tx.owner,
        tx.pending,
        tx.source_data.pan,
        tx.source_data.sub_type,
        tx.source_data.source_type,
        tx.success,
    )
}

/// Verify Paymob's HMAC-SHA512 signature in constant time against the official nested `obj`.
pub fn verify_paymob_hmac(
    hmac_secret: &str,
    presented_hmac_hex: &str,
    tx: &PaymobOfficialTransactionObj,
) -> Result<(), AuthError> {
    if hmac_secret.trim().is_empty() {
        return Err(AuthError::BillingNotConfigured);
    }
    let presented_bytes =
        hex::decode(presented_hmac_hex.trim()).map_err(|_| AuthError::InvalidWebhookSignature)?;
    let concat = paymob_hmac_concat_string(tx);
    let mut mac = HmacSha512::new_from_slice(hmac_secret.as_bytes())
        .map_err(|_| AuthError::BillingNotConfigured)?;
    mac.update(concat.as_bytes());
    let expected = mac.finalize().into_bytes();

    if expected
        .as_slice()
        .ct_eq(presented_bytes.as_slice())
        .unwrap_u8()
        != 1
    {
        return Err(AuthError::InvalidWebhookSignature);
    }
    Ok(())
}

/// Compute a genuine Paymob HMAC-SHA512 hex digest over the official nested `obj`.
pub fn sign_paymob_transaction(
    hmac_secret: &str,
    tx: &PaymobOfficialTransactionObj,
) -> Result<String, AuthError> {
    let concat = paymob_hmac_concat_string(tx);
    let mut mac = HmacSha512::new_from_slice(hmac_secret.as_bytes())
        .map_err(|_| AuthError::BillingNotConfigured)?;
    mac.update(concat.as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

/// Choose provider & currency strictly from the single source of truth (`config/business.json`):
/// Egypt -> Paymob EGP, International -> Paddle USD.
#[must_use]
pub fn resolve_provider_and_price(
    country_iso2: &str,
    product: CheckoutProduct,
    seats: u32,
) -> (BillingProvider, BillingCurrency, u64) {
    crate::business_rules::rules().resolve_provider_and_price(country_iso2, product, seats)
}

#[derive(Debug, Clone, Default)]
struct PaddlePriceCatalog {
    individual_monthly: Option<String>,
    individual_annual: Option<String>,
    org_seat_annual: Option<String>,
    pay_per_file: Option<String>,
    credit_pack_25: Option<String>,
}

impl PaddlePriceCatalog {
    fn price_id_for(&self, product: CheckoutProduct) -> Option<&str> {
        match product {
            CheckoutProduct::IndividualMonthly => self.individual_monthly.as_deref(),
            CheckoutProduct::IndividualAnnual => self.individual_annual.as_deref(),
            CheckoutProduct::OrganizationSeatAnnual => self.org_seat_annual.as_deref(),
            CheckoutProduct::PayPerFile => self.pay_per_file.as_deref(),
            CheckoutProduct::CreditPack25 => self.credit_pack_25.as_deref(),
        }
    }
}

#[derive(Default)]
struct BillingStore {
    processed_events: HashSet<(String, String)>,
    processed_event_hashes: HashMap<(String, String), [u8; 32]>,
    security_anomalies: Vec<String>,
    last_event_ts_by_workspace: HashMap<Uuid, i64>,
    events: Vec<BillingEventRecord>,
    invoices: HashMap<String, InvoiceRecord>,
    orders: HashMap<Uuid, BillingOrderRecord>,
    orders_by_idempotency: HashMap<(Uuid, String), Uuid>,
    orders_by_provider_ref: HashMap<(BillingProvider, String), Uuid>,
    orders_by_subscription: HashMap<(BillingProvider, String), Uuid>,
    grants_by_provider_tx: HashSet<(BillingProvider, String)>,
    grants_by_order_kind: HashSet<(Uuid, String)>,
    grants_by_renewal_period: HashSet<(BillingProvider, String, i64)>,
    confirmed_provider_refund_ids: HashSet<(BillingProvider, String)>,
    grants: Vec<EntitlementGrantRecord>,
    refunds: HashMap<(Uuid, String), RefundRecord>,
}

type InFlightCheckoutGates = Arc<Mutex<HashMap<(Uuid, String), Arc<Mutex<()>>>>>;

/// Authoritative billing service coordinating real Paddle & Paymob adapters,
/// PostgreSQL 18 financial ledger, and entitlements.
#[derive(Clone)]
pub struct BillingService {
    entitlements: EntitlementService,
    coupons: Option<CouponService>,
    pg_store: Option<PgControlPlaneStore>,
    require_durable_pg: bool,
    paddle_adapter: Option<PaddleApiAdapter>,
    paddle_webhook_secret: Option<String>,
    paddle_prices: PaddlePriceCatalog,
    paymob_adapter: Option<PaymobApiAdapter>,
    paymob_hmac_secret: Option<String>,
    /// Single exclusive mutex for non-PG in-memory ledger fallback (eliminates TOCTOU races).
    store: Arc<Mutex<BillingStore>>,
    /// Per-(workspace_id, idempotency_key) in-flight gate so concurrent clicks never invoke provider twice.
    in_flight_checkouts: InFlightCheckoutGates,
}

impl fmt::Debug for BillingService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BillingService")
            .field("paddle_configured", &self.paddle_configured())
            .field("paymob_configured", &self.paymob_configured())
            .field("pg_backed", &self.pg_store.is_some())
            .field("require_durable_pg", &self.require_durable_pg)
            .finish()
    }
}

impl BillingService {
    #[must_use]
    pub fn from_config(config: &ServerConfig, entitlements: EntitlementService) -> Self {
        let paddle_adapter = config.paddle_api_key.as_ref().map(|key| PaddleApiAdapter {
            api_base_url: config.paddle_api_base_url.clone(),
            api_key: key.clone(),
            timeout_ms: DEFAULT_PROVIDER_TIMEOUT_MS,
        });
        let paymob_adapter = match (
            config.paymob_api_key.as_ref(),
            config.paymob_public_key.as_ref(),
            config.paymob_integration_id,
        ) {
            (Some(sk), Some(pk), Some(int_id)) => Some(PaymobApiAdapter {
                api_base_url: config.paymob_api_base_url.clone(),
                secret_key: sk.clone(),
                public_key: pk.clone(),
                integration_id: int_id,
                timeout_ms: DEFAULT_PROVIDER_TIMEOUT_MS,
            }),
            _ => None,
        };
        Self {
            entitlements,
            coupons: None,
            pg_store: None,
            require_durable_pg: config.environment.is_strict(),
            paddle_adapter,
            paddle_webhook_secret: config.paddle_webhook_secret.clone(),
            paddle_prices: PaddlePriceCatalog {
                individual_monthly: config.paddle_price_individual_monthly.clone(),
                individual_annual: config.paddle_price_individual_annual.clone(),
                org_seat_annual: config.paddle_price_org_seat_annual.clone(),
                pay_per_file: config.paddle_price_pay_per_file.clone(),
                credit_pack_25: config.paddle_price_credit_pack_25.clone(),
            },
            paymob_adapter,
            paymob_hmac_secret: config.paymob_hmac_secret.clone(),
            store: Arc::new(Mutex::new(BillingStore::default())),
            in_flight_checkouts: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Attach authoritative PostgreSQL 18 financial ledger repository (and wire PG into entitlements).
    #[must_use]
    pub fn with_pg_store(mut self, pg_store: PgControlPlaneStore) -> Self {
        self.entitlements = self.entitlements.with_pg_store(pg_store.clone());
        self.pg_store = Some(pg_store);
        self
    }

    /// Require PostgreSQL 18 durability (production/staging fail-closed guard).
    #[must_use]
    pub fn with_require_durable_pg(mut self, required: bool) -> Self {
        self.require_durable_pg = required;
        self
    }

    fn ensure_durable_ledger(&self) -> Result<(), AuthError> {
        if self.require_durable_pg && self.pg_store.is_none() {
            return Err(AuthError::BackendUnavailable);
        }
        Ok(())
    }

    /// Attach server-authoritative coupon service for reserve/commit/release lifecycle.
    #[must_use]
    pub fn with_coupons(mut self, coupons: CouponService) -> Self {
        self.coupons = Some(coupons);
        self
    }

    /// Override provider HTTP timeout (used in timeout/reconciliation tests).
    #[must_use]
    pub fn with_provider_timeout_ms(mut self, timeout_ms: u64) -> Self {
        if let Some(ref mut p) = self.paddle_adapter {
            p.timeout_ms = timeout_ms;
        }
        if let Some(ref mut m) = self.paymob_adapter {
            m.timeout_ms = timeout_ms;
        }
        self
    }

    #[must_use]
    pub fn paddle_configured(&self) -> bool {
        self.paddle_adapter.is_some() && self.paddle_webhook_secret.is_some()
    }

    #[must_use]
    pub fn paymob_configured(&self) -> bool {
        self.paymob_adapter.is_some() && self.paymob_hmac_secret.is_some()
    }

    /// Return honest provider readiness status without leaking secrets.
    #[must_use]
    pub fn provider_status_json(&self) -> Value {
        serde_json::json!({
            "paddle": if self.paddle_configured() { "CONFIGURED" } else { "NOT_CONFIGURED" },
            "paymob": if self.paymob_configured() { "CONFIGURED" } else { "NOT_CONFIGURED" },
            "financial_ledger": if self.pg_store.is_some() { "POSTGRESQL_18" } else { "IN_MEMORY_TEST_ONLY" },
        })
    }

    async fn checkout_key_mutex(&self, workspace_id: Uuid, key: &str) -> Arc<Mutex<()>> {
        let mut map = self.in_flight_checkouts.lock().await;
        map.entry((workspace_id, key.to_owned()))
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    /// Initiate an idempotent, real provider checkout (Paddle Transactions API or Paymob Intention API).
    ///
    /// Guarantees:
    /// - Requires a non-empty `idempotency_key` (never silently creates duplicates on retry).
    /// - Same `(workspace_id, idempotency_key)` + same fingerprint returns the exact existing order.
    /// - Same `(workspace_id, idempotency_key)` + different fingerprint fails with `SF-PAY-007` (`409 Conflict`).
    /// - Reserves coupons before contacting provider, releases coupon if provider rejects checkout,
    ///   and reconciles via provider API if an earlier attempt timed out (`ReconciliationRequired`).
    pub async fn create_checkout(
        &self,
        req: &CheckoutRequest,
    ) -> Result<CheckoutResponse, AuthError> {
        self.create_checkout_at(req, unix_now()).await
    }

    /// Deterministic timestamp variant of [`create_checkout`].
    pub async fn create_checkout_at(
        &self,
        req: &CheckoutRequest,
        now_secs: i64,
    ) -> Result<CheckoutResponse, AuthError> {
        self.ensure_durable_ledger()?;
        let raw_key = req
            .idempotency_key
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty() && s.len() <= 128)
            .ok_or(AuthError::InvalidRequest)?;
        let idempotency_key = raw_key.to_owned();

        let seats = req.seats.unwrap_or(1);
        if req.product == CheckoutProduct::OrganizationSeatAnnual && seats < ORG_MIN_SEATS {
            return Err(AuthError::OrgMinSeatsRequired);
        }

        let (provider, currency, original_amount_minor) =
            resolve_provider_and_price(&req.billing_country_iso2, req.product, seats);

        // Validate provider readiness & approved server-side price/integration IDs BEFORE touching coupons or orders.
        let (provider_price_id, expected_integration_id) = match provider {
            BillingProvider::Paddle => {
                if !self.paddle_configured() {
                    return Err(AuthError::BillingNotConfigured);
                }
                let price_id = self
                    .paddle_prices
                    .price_id_for(req.product)
                    .ok_or(AuthError::BillingNotConfigured)?
                    .to_owned();
                (price_id, None)
            }
            BillingProvider::Paymob => {
                let adapter = self
                    .paymob_adapter
                    .as_ref()
                    .ok_or(AuthError::BillingNotConfigured)?;
                if self.paymob_hmac_secret.is_none() {
                    return Err(AuthError::BillingNotConfigured);
                }
                (
                    format!(
                        "paymob_int_{}_{}",
                        adapter.integration_id,
                        req.product.as_str()
                    ),
                    Some(adapter.integration_id),
                )
            }
            BillingProvider::Manual => return Err(AuthError::InvalidRequest),
        };

        let fingerprint = compute_checkout_request_fingerprint(
            req.workspace_id,
            req.user_id,
            req.product,
            seats,
            &req.billing_country_iso2,
            req.coupon_code.as_deref(),
        );

        // Serialize simultaneous requests sharing the exact same (workspace_id, idempotency_key)
        // so double-clicks and 50x bursts wait for the first provider call and reuse its persisted order.
        let key_lock = self
            .checkout_key_mutex(req.workspace_id, &idempotency_key)
            .await;
        let _key_guard = key_lock.lock().await;

        // Section 6: Paymob EGP checkout REQUIRES real validated customer billing details (zero fake PII!).
        if provider == BillingProvider::Paymob {
            let billing_data = req.billing_data.as_ref().ok_or(AuthError::InvalidRequest)?;
            billing_data
                .validate()
                .map_err(|_| AuthError::InvalidRequest)?;
        }

        let candidate_order_id = Uuid::now_v7();
        let candidate = BillingOrderRecord {
            id: candidate_order_id,
            workspace_id: req.workspace_id,
            user_id: req.user_id,
            idempotency_key: idempotency_key.clone(),
            request_fingerprint: fingerprint,
            provider,
            product: req.product,
            provider_price_id,
            seats,
            currency,
            original_amount_minor,
            discount_amount_minor: 0,
            expected_amount_minor: original_amount_minor,
            coupon_code: req
                .coupon_code
                .as_deref()
                .map(|s| s.trim().to_ascii_uppercase())
                .filter(|s| !s.is_empty()),
            coupon_reservation_id: None,
            expected_integration_id,
            provider_order_ref: None,
            provider_intention_id: None,
            provider_subscription_id: None,
            checkout_mode: CheckoutMode::RedirectUrl,
            checkout_url: None,
            provider_transaction_id: None,
            state: PaymentOrderState::Created,
            last_error_code: None,
            created_at: now_secs,
            expires_at: now_secs + CHECKOUT_ORDER_TTL_SECS,
            updated_at: now_secs,
            paid_at: None,
        };

        // Step 1: Atomically claim or load existing order by `(workspace_id, idempotency_key)`.
        // Across multiple server instances sharing PostgreSQL 18, ONLY the leader instance that
        // receives `PgCheckoutClaim::Created` may invoke the external payment provider API.
        // Any concurrent follower seeing `state == Created` waits for the leader's persisted result!
        let mut order = match self.claim_or_load_order(&candidate).await? {
            PgCheckoutClaim::Created(new_order) => new_order,
            PgCheckoutClaim::Existing(mut existing) => {
                if existing.state == PaymentOrderState::Created {
                    let wait_budget = std::time::Duration::from_millis(
                        self.paddle_adapter
                            .as_ref()
                            .map(|a| a.timeout_ms)
                            .or_else(|| self.paymob_adapter.as_ref().map(|a| a.timeout_ms))
                            .unwrap_or(DEFAULT_PROVIDER_TIMEOUT_MS)
                            .clamp(500, 15_000),
                    );
                    let deadline = tokio::time::Instant::now() + wait_budget;
                    while existing.state == PaymentOrderState::Created
                        && tokio::time::Instant::now() < deadline
                    {
                        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
                        if let Some(refreshed) = self.get_order(existing.id).await? {
                            existing = refreshed;
                        }
                    }
                    if existing.state == PaymentOrderState::Created {
                        // Leader is still in-flight or crashed -> NEVER issue a blind second provider call!
                        return Err(AuthError::PaymentReconciliationRequired);
                    }
                }
                // Already created successfully -> return exact stored order without contacting provider again!
                if matches!(
                    existing.state,
                    PaymentOrderState::CheckoutPending
                        | PaymentOrderState::PaymentPending
                        | PaymentOrderState::Paid
                ) {
                    return Ok(order_to_checkout_response(&existing, true));
                }
                // Uncertain previous timeout -> MUST reconcile with provider before ever creating a second charge!
                if existing.state == PaymentOrderState::ReconciliationRequired {
                    return self.reconcile_uncertain_order(existing, now_secs).await;
                }
                // Previous attempt definitively failed or expired -> caller may retry safely using same order record
                // ONLY if it had zero provider_order_ref; otherwise fail closed to avoid duplicate charges.
                if existing.provider_order_ref.is_some() {
                    return Err(AuthError::PaymentReconciliationRequired);
                }
                existing
            }
        };

        // Step 2: Reserve coupon (NEVER permanently commit until payment succeeds; release if provider fails!).
        let mut reserved_coupon_id: Option<Uuid> = order.coupon_reservation_id;
        if let Some(ref raw_code) = req.coupon_code {
            if let Some(ref coupons) = self.coupons {
                let has_prior_invoices = !self.list_invoices(req.workspace_id).await.is_empty();
                let res = coupons
                    .reserve_at(
                        raw_code,
                        req.user_id,
                        req.workspace_id,
                        order.id,
                        req.product,
                        original_amount_minor,
                        currency,
                        !has_prior_invoices,
                        now_secs,
                        CHECKOUT_ORDER_TTL_SECS,
                    )
                    .await;
                match res {
                    Ok(r) => {
                        order.coupon_code = Some(r.code);
                        order.coupon_reservation_id = Some(r.id);
                        order.discount_amount_minor = r.discount_amount_minor;
                        order.expected_amount_minor = r.final_amount_minor;
                        reserved_coupon_id = Some(r.id);
                    }
                    Err(err) => {
                        order.state = PaymentOrderState::Failed;
                        order.last_error_code = Some(err.code().to_owned());
                        order.updated_at = now_secs;
                        self.persist_order(&order).await?;
                        return Err(err);
                    }
                }
            }
        }

        // Step 3: Call the REAL payment provider API (Paddle POST /transactions or Paymob POST /v1/intention/).
        let provider_result = match provider {
            BillingProvider::Paddle => {
                let adapter = self
                    .paddle_adapter
                    .as_ref()
                    .ok_or(AuthError::BillingNotConfigured)?;
                let qty = if req.product == CheckoutProduct::OrganizationSeatAnnual {
                    seats
                } else {
                    1
                };
                adapter
                    .create_transaction(
                        order.id,
                        order.workspace_id,
                        order.user_id,
                        order.product,
                        &order.provider_price_id,
                        qty,
                        order.expected_amount_minor,
                        &order.idempotency_key,
                        req.paddle_customer.as_ref(),
                    )
                    .await
                    .map(|created| {
                        (
                            created.transaction_id.clone(),
                            Some(created.transaction_id),
                            created.checkout_mode,
                            created.checkout_url,
                        )
                    })
            }
            BillingProvider::Paymob => {
                let adapter = self
                    .paymob_adapter
                    .as_ref()
                    .ok_or(AuthError::BillingNotConfigured)?;
                let billing_data = req.billing_data.as_ref().ok_or(AuthError::InvalidRequest)?;
                adapter
                    .create_intention(
                        order.id,
                        order.product,
                        order.seats,
                        order.expected_amount_minor,
                        billing_data,
                        None,
                        None,
                    )
                    .await
                    .map(|created| {
                        (
                            created.intention_order_id.to_string(),
                            Some(created.intention_id),
                            CheckoutMode::RedirectUrl,
                            Some(created.checkout_url),
                        )
                    })
            }
            BillingProvider::Manual => Err(ProviderCallError::DefinitiveFailure {
                status: 400,
                reason: "manual_checkout_unsupported".to_owned(),
            }),
        };

        // Step 4: Persist outcome BEFORE returning to client (ZERO swallowed persistence errors!).
        match provider_result {
            Ok((provider_order_ref, provider_intention_id, checkout_mode, checkout_url)) => {
                order.provider_order_ref = Some(provider_order_ref);
                order.provider_intention_id = provider_intention_id;
                order.checkout_mode = checkout_mode;
                order.checkout_url = checkout_url;
                order.state = PaymentOrderState::CheckoutPending;
                order.last_error_code = None;
                order.updated_at = now_secs;
                let saved = self.persist_order(&order).await?;
                Ok(order_to_checkout_response(&saved, false))
            }
            Err(ProviderCallError::NotConfigured(_)) => {
                if let (Some(coupons), Some(res_id)) = (&self.coupons, reserved_coupon_id) {
                    coupons.release_reservation(res_id).await;
                }
                order.state = PaymentOrderState::Failed;
                order.coupon_reservation_id = None;
                order.last_error_code = Some(AuthError::BillingNotConfigured.code().to_owned());
                order.updated_at = now_secs;
                self.persist_order(&order).await?;
                Err(AuthError::BillingNotConfigured)
            }
            Err(ProviderCallError::DefinitiveFailure { .. }) => {
                // Provider definitely did NOT create the checkout -> release coupon reservation so customer keeps it!
                if let (Some(coupons), Some(res_id)) = (&self.coupons, reserved_coupon_id) {
                    coupons.release_reservation(res_id).await;
                }
                order.state = PaymentOrderState::Failed;
                order.coupon_reservation_id = None;
                order.last_error_code = Some(AuthError::BillingOperationFailed.code().to_owned());
                order.updated_at = now_secs;
                self.persist_order(&order).await?;
                Err(AuthError::BillingOperationFailed)
            }
            Err(ProviderCallError::UncertainOutcome(_)) => {
                // CRITICAL: Provider MAY have created the transaction! Mark `reconciliation_required`,
                // KEEP coupon reservation held, and refuse to blindly retry creation.
                order.state = PaymentOrderState::ReconciliationRequired;
                order.last_error_code =
                    Some(AuthError::PaymentReconciliationRequired.code().to_owned());
                order.updated_at = now_secs;
                self.persist_order(&order).await?;
                Err(AuthError::PaymentReconciliationRequired)
            }
        }
    }

    async fn reconcile_uncertain_order(
        &self,
        mut order: BillingOrderRecord,
        now_secs: i64,
    ) -> Result<CheckoutResponse, AuthError> {
        match order.provider {
            BillingProvider::Paddle => {
                let adapter = self
                    .paddle_adapter
                    .as_ref()
                    .ok_or(AuthError::BillingNotConfigured)?;
                match adapter
                    .reconcile_transaction_by_order_id(
                        order.id,
                        &order.provider_price_id,
                        order.expected_amount_minor,
                    )
                    .await
                {
                    Ok(Some(found)) => {
                        order.provider_order_ref = Some(found.transaction_id.clone());
                        order.provider_intention_id = Some(found.transaction_id);
                        order.checkout_mode = found.checkout_mode;
                        order.checkout_url = found.checkout_url;
                        order.state = PaymentOrderState::CheckoutPending;
                        order.last_error_code = None;
                        order.updated_at = now_secs;
                        let saved = self.persist_order(&order).await?;
                        Ok(order_to_checkout_response(&saved, true))
                    }
                    Ok(None) | Err(_) => {
                        // Still uncertain -> NEVER blindly issue a second charge!
                        Err(AuthError::PaymentReconciliationRequired)
                    }
                }
            }
            BillingProvider::Paymob => {
                let adapter = self
                    .paymob_adapter
                    .as_ref()
                    .ok_or(AuthError::BillingNotConfigured)?;
                match adapter.reconcile_intention_by_order_id(order.id).await {
                    Ok(Some(found)) => {
                        order.provider_order_ref = Some(found.intention_order_id.to_string());
                        order.provider_intention_id = Some(found.intention_id);
                        order.checkout_mode = CheckoutMode::RedirectUrl;
                        order.checkout_url = Some(found.checkout_url);
                        order.state = PaymentOrderState::CheckoutPending;
                        order.last_error_code = None;
                        order.updated_at = now_secs;
                        let saved = self.persist_order(&order).await?;
                        Ok(order_to_checkout_response(&saved, true))
                    }
                    Ok(None) | Err(_) => Err(AuthError::PaymentReconciliationRequired),
                }
            }
            BillingProvider::Manual => Err(AuthError::PaymentReconciliationRequired),
        }
    }

    async fn claim_or_load_order(
        &self,
        candidate: &BillingOrderRecord,
    ) -> Result<PgCheckoutClaim, AuthError> {
        self.ensure_durable_ledger()?;
        if let Some(ref pg) = self.pg_store {
            return pg.claim_or_load_checkout_order_pg(candidate).await;
        }
        let mut guard = self.store.lock().await;
        let key = (candidate.workspace_id, candidate.idempotency_key.clone());
        if let Some(&existing_id) = guard.orders_by_idempotency.get(&key) {
            if let Some(existing) = guard.orders.get(&existing_id).cloned() {
                if existing.request_fingerprint != candidate.request_fingerprint {
                    return Err(AuthError::IdempotencyConflict);
                }
                return Ok(PgCheckoutClaim::Existing(existing));
            }
        }
        // Refuse creating a second charge with a different idempotency key while an earlier
        // attempt for the SAME workspace + product is unresolved (`ReconciliationRequired`).
        if guard.orders.values().any(|o| {
            o.workspace_id == candidate.workspace_id
                && o.product == candidate.product
                && o.state == PaymentOrderState::ReconciliationRequired
                && o.expires_at > candidate.created_at
        }) {
            return Err(AuthError::PaymentReconciliationRequired);
        }
        guard.orders_by_idempotency.insert(key, candidate.id);
        guard.orders.insert(candidate.id, candidate.clone());
        Ok(PgCheckoutClaim::Created(candidate.clone()))
    }

    async fn persist_order(
        &self,
        order: &BillingOrderRecord,
    ) -> Result<BillingOrderRecord, AuthError> {
        if let Some(ref pg) = self.pg_store {
            return pg.update_checkout_order_pg(order).await;
        }
        let mut guard = self.store.lock().await;
        if let Some(ref pref) = order.provider_order_ref {
            guard
                .orders_by_provider_ref
                .insert((order.provider, pref.clone()), order.id);
        }
        if let Some(ref psub) = order.provider_subscription_id {
            guard
                .orders_by_subscription
                .insert((order.provider, psub.clone()), order.id);
        }
        guard.orders.insert(order.id, order.clone());
        Ok(order.clone())
    }

    /// Retrieve an internal billing order by ID.
    pub async fn get_order(&self, order_id: Uuid) -> Result<Option<BillingOrderRecord>, AuthError> {
        if let Some(ref pg) = self.pg_store {
            return pg.get_billing_order_by_id_pg(order_id).await;
        }
        Ok(self.store.lock().await.orders.get(&order_id).cloned())
    }

    /// Count recorded security/integrity anomalies (e.g. webhook event_id payload-hash mismatch).
    pub async fn count_security_anomalies(&self) -> Result<usize, AuthError> {
        if let Some(ref pg) = self.pg_store {
            let n = pg.count_security_anomalies_pg().await?;
            return Ok(usize::try_from(n).unwrap_or(0));
        }
        Ok(self.store.lock().await.security_anomalies.len())
    }

    /// Process an official Paddle Billing webhook envelope (`event_id`, `event_type`, RFC 3339 `occurred_at`,
    /// `notification_id`, nested `data`) after verifying HMAC-SHA256 signature, replay window,
    /// order/subscription correlation, currency/amount/price validation, payload-hash integrity, and atomic commit.
    pub async fn process_paddle_webhook(
        &self,
        signature_header: &str,
        raw_body: &[u8],
        now_secs: i64,
    ) -> Result<BillingEventRecord, AuthError> {
        self.ensure_durable_ledger()?;
        let secret = self
            .paddle_webhook_secret
            .as_deref()
            .ok_or(AuthError::BillingNotConfigured)?;
        verify_paddle_signature(secret, signature_header, raw_body, now_secs)?;

        let envelope: PaddleWebhookEnvelope =
            serde_json::from_slice(raw_body).map_err(|_| AuthError::InvalidRequest)?;
        if envelope.event_id.trim().is_empty() || envelope.event_type.trim().is_empty() {
            return Err(AuthError::InvalidRequest);
        }
        let occurred_at_unix =
            parse_rfc3339_to_unix_secs(&envelope.occurred_at).ok_or(AuthError::InvalidRequest)?;

        let data = &envelope.data;
        let data_id = data
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let data_status = data
            .get("status")
            .and_then(Value::as_str)
            .map(|s| s.trim().to_ascii_lowercase());
        let custom_data = data.get("custom_data");

        let order_id_hint = custom_data
            .and_then(|c| c.get("safi_order_id"))
            .and_then(Value::as_str)
            .and_then(|s| Uuid::parse_str(s).ok());

        // Validate reported currency strictly (NEVER assume missing currency is USD on transactions/adjustments!).
        let raw_currency = data.get("currency_code").and_then(Value::as_str);
        let reported_currency = match raw_currency {
            Some(c) if c.trim().eq_ignore_ascii_case("USD") => Some(BillingCurrency::Usd),
            Some(c) if c.trim().eq_ignore_ascii_case("EGP") => Some(BillingCurrency::Egp),
            Some(_) => return Err(AuthError::BillingOperationFailed),
            None => {
                if envelope.event_type.starts_with("transaction.")
                    || envelope.event_type.starts_with("adjustment.")
                {
                    return Err(AuthError::InvalidRequest);
                }
                None
            }
        };

        let is_subscription_event = envelope.event_type.starts_with("subscription.");
        let is_adjustment_event = envelope.event_type.starts_with("adjustment.");

        let subscription_id = if is_subscription_event {
            data_id.clone()
        } else {
            data.get("subscription_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        };

        let transaction_id = if is_adjustment_event {
            data.get("transaction_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        } else if envelope.event_type.starts_with("transaction.") {
            data_id.clone()
        } else {
            data.get("transaction_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        };

        let invoice_id = data
            .get("invoice_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned);

        let parse_minor = |v: &Value| -> Option<u64> {
            v.as_u64()
                .or_else(|| v.as_str().and_then(|s| s.trim().parse::<u64>().ok()))
        };
        let reported_amount_minor = data
            .get("details")
            .and_then(|d| d.get("totals"))
            .and_then(|t| t.get("total"))
            .and_then(parse_minor)
            .or_else(|| {
                data.get("totals")
                    .and_then(|t| t.get("total"))
                    .and_then(parse_minor)
            });

        let period_obj = data
            .get("billing_period")
            .or_else(|| data.get("current_billing_period"));
        let period_start_unix = period_obj
            .and_then(|p| p.get("starts_at"))
            .and_then(Value::as_str)
            .and_then(parse_rfc3339_to_unix_secs);
        let period_end_unix = period_obj
            .and_then(|p| p.get("ends_at"))
            .and_then(Value::as_str)
            .and_then(parse_rfc3339_to_unix_secs);

        let origin = data
            .get("origin")
            .and_then(Value::as_str)
            .map(|s| s.trim().to_ascii_lowercase());
        let is_renewal_hint = matches!(
            origin.as_deref(),
            Some("subscription_recurring" | "subscription_payment_method_change")
        );

        let kind = match envelope.event_type.as_str() {
            // Non-final events MUST NEVER activate paid entitlements!
            "checkout.opened"
            | "transaction.created"
            | "transaction.ready"
            | "transaction.billed" => VerifiedWebhookKind::PendingNoGrant,
            "transaction.completed" => {
                // Only a verified `completed` transaction activates an entitlement!
                if data_status.as_deref() != Some("completed") {
                    VerifiedWebhookKind::PendingNoGrant
                } else {
                    VerifiedWebhookKind::Paid
                }
            }
            "transaction.updated" => match data_status.as_deref() {
                Some("past_due") => VerifiedWebhookKind::PastDue,
                Some("canceled") => VerifiedWebhookKind::Canceled,
                _ => VerifiedWebhookKind::PendingNoGrant,
            },
            "transaction.payment_failed" => VerifiedWebhookKind::Failed,
            "transaction.past_due" | "subscription.past_due" => VerifiedWebhookKind::PastDue,
            "subscription.created" => {
                // Subscription metadata creation/binding: never grant twice; entitlement is activated
                // by `transaction.completed` (or status transitions).
                VerifiedWebhookKind::PendingNoGrant
            }
            "subscription.updated" => match data_status.as_deref() {
                Some("past_due") => VerifiedWebhookKind::PastDue,
                Some("canceled") => VerifiedWebhookKind::Canceled,
                _ => VerifiedWebhookKind::PendingNoGrant,
            },
            "subscription.canceled" => VerifiedWebhookKind::Canceled,
            "adjustment.created" | "adjustment.updated" => {
                let action = data
                    .get("action")
                    .and_then(Value::as_str)
                    .map(|s| s.trim().to_ascii_lowercase())
                    .unwrap_or_default();
                let adj_status = data_status.as_deref().unwrap_or("pending_approval");
                match action.as_str() {
                    "chargeback" => VerifiedWebhookKind::Chargeback,
                    "refund" => {
                        // NEVER revoke entitlements on `pending_approval` or `rejected`!
                        if adj_status == "approved" {
                            let adj_type = data
                                .get("type")
                                .and_then(Value::as_str)
                                .map(|s| s.trim().to_ascii_lowercase());
                            let partial_minor = if adj_type.as_deref() == Some("partial") {
                                reported_amount_minor
                            } else {
                                None
                            };
                            VerifiedWebhookKind::Refunded {
                                partial_amount_minor: partial_minor,
                                cumulative_refunded_minor: None,
                                provider_refund_id: data_id.clone(),
                                mandatory_legal_override: true,
                            }
                        } else {
                            VerifiedWebhookKind::PendingNoGrant
                        }
                    }
                    _ => VerifiedWebhookKind::PendingNoGrant,
                }
            }
            _ => VerifiedWebhookKind::PendingNoGrant,
        };

        let mut payload_hash = [0u8; 32];
        payload_hash.copy_from_slice(&Sha256::digest(raw_body));

        let transition = VerifiedWebhookTransition {
            provider: BillingProvider::Paddle,
            provider_event_id: envelope.event_id.clone(),
            event_type: envelope.event_type.clone(),
            raw_payload_hash: payload_hash,
            occurred_at_unix,
            now_secs,
            order_id_hint,
            provider_order_ref: transaction_id.clone(),
            provider_transaction_id: transaction_id.clone(),
            provider_subscription_id: subscription_id.clone(),
            external_invoice_id: invoice_id.clone(),
            reported_amount_minor,
            reported_currency,
            reported_integration_id: None,
            period_start_unix,
            period_end_unix,
            is_renewal_hint,
            kind,
        };

        // Cross-check nested `items[].price.id` and `custom_data` against the trusted stored order
        // BEFORE executing the atomic database commit!
        if let Some(ref order) = self
            .resolve_stored_order_for_check(
                BillingProvider::Paddle,
                transition.order_id_hint,
                transition.provider_order_ref.as_deref(),
                transition.provider_transaction_id.as_deref(),
                transition.provider_subscription_id.as_deref(),
            )
            .await?
        {
            if let Some(items) = data.get("items").and_then(Value::as_array) {
                if !items.is_empty() {
                    let any_match = items.iter().any(|item| {
                        item.get("price")
                            .and_then(|p| p.get("id"))
                            .and_then(Value::as_str)
                            .or_else(|| item.get("price_id").and_then(Value::as_str))
                            == Some(order.provider_price_id.as_str())
                    });
                    if !any_match {
                        return Err(AuthError::BillingOperationFailed);
                    }
                }
            }
            if let Some(cd) = custom_data {
                if let Some(ws_str) = cd.get("workspace_id").and_then(Value::as_str) {
                    let ws =
                        Uuid::parse_str(ws_str).map_err(|_| AuthError::BillingOperationFailed)?;
                    if ws != order.workspace_id {
                        return Err(AuthError::BillingOperationFailed);
                    }
                }
                if let Some(uid_str) = cd.get("user_id").and_then(Value::as_str) {
                    let uid =
                        Uuid::parse_str(uid_str).map_err(|_| AuthError::BillingOperationFailed)?;
                    if uid != order.user_id {
                        return Err(AuthError::BillingOperationFailed);
                    }
                }
                if let Some(prod_str) = cd.get("product").and_then(Value::as_str) {
                    let prod = CheckoutProduct::from_str_code(prod_str)
                        .ok_or(AuthError::BillingOperationFailed)?;
                    if prod != order.product {
                        return Err(AuthError::BillingOperationFailed);
                    }
                }
            }
        }

        self.execute_verified_webhook_atomic(&transition).await
    }

    /// Process an official Paymob Transaction Processed Callback raw JSON envelope
    /// (`{"type":"TRANSACTION","obj":{...}}`) after constant-time HMAC-SHA512 verification.
    ///
    /// CRITICAL TRUST-BOUNDARY ENFORCEMENT (Section 2):
    /// - Enforces `type == "TRANSACTION"`.
    /// - Verifies HMAC-SHA512 over the exact 20 official nested `obj` fields (`obj.order.id`, `obj.source_data.*`, etc.).
    /// - Correlates strictly by the HMAC-signed `obj.order.id` (`provider_order_ref`) against `billing_orders`
    ///   (NEVER confuses `obj.id` with `obj.order.id`!).
    /// - Verifies `obj.integration_id`, `obj.currency`, `obj.amount_cents`, and capture lifecycle.
    /// - Uses official `obj.refunded_amount_cents` for cumulative partial/full refund accounting.
    /// - NEVER trusts unsigned metadata (`obj.data` / `obj.order.data`) for beneficiary decisions;
    ///   any conflicting unsigned field is REJECTED (`SF-PAY-004`).
    pub async fn process_paymob_webhook_raw(
        &self,
        presented_hmac: &str,
        raw_body: &[u8],
        delivery_event_id: Option<&str>,
        now_secs: i64,
    ) -> Result<BillingEventRecord, AuthError> {
        self.ensure_durable_ledger()?;
        let secret = self
            .paymob_hmac_secret
            .as_deref()
            .ok_or(AuthError::BillingNotConfigured)?;
        if presented_hmac.trim().is_empty() {
            return Err(AuthError::InvalidWebhookSignature);
        }

        let envelope: PaymobCallbackEnvelope =
            serde_json::from_slice(raw_body).map_err(|_| AuthError::InvalidRequest)?;
        if !envelope
            .callback_type
            .trim()
            .eq_ignore_ascii_case("TRANSACTION")
        {
            return Err(AuthError::InvalidRequest);
        }

        let tx = &envelope.obj;
        verify_paymob_hmac(secret, presented_hmac, tx)?;

        let expected_int = self
            .paymob_adapter
            .as_ref()
            .map(|a| a.integration_id)
            .ok_or(AuthError::BillingNotConfigured)?;
        if tx.integration_id != expected_int {
            return Err(AuthError::BillingOperationFailed);
        }

        let reported_currency = if tx.currency.trim().eq_ignore_ascii_case("EGP") {
            BillingCurrency::Egp
        } else if tx.currency.trim().eq_ignore_ascii_case("USD") {
            BillingCurrency::Usd
        } else {
            return Err(AuthError::BillingOperationFailed);
        };

        // Strictly correlate via HMAC-signed `obj.order.id` (NOT `obj.id`!).
        let provider_order_ref = tx.order.id.to_string();
        let stored_order = self
            .resolve_stored_order_for_check(
                BillingProvider::Paymob,
                None,
                Some(&provider_order_ref),
                None,
                None,
            )
            .await?
            .ok_or(AuthError::BillingOperationFailed)?;

        // Reject tampered unsigned beneficiary metadata if present in `obj.data` or `obj.order.data`!
        for unsigned_bag in [tx.data.as_ref(), tx.order.data.as_ref()]
            .into_iter()
            .flatten()
        {
            if let Some(ws_str) = unsigned_bag.get("workspace_id").and_then(Value::as_str) {
                let ws = Uuid::parse_str(ws_str).map_err(|_| AuthError::BillingOperationFailed)?;
                if ws != stored_order.workspace_id {
                    return Err(AuthError::BillingOperationFailed);
                }
            }
            if let Some(uid_str) = unsigned_bag.get("user_id").and_then(Value::as_str) {
                let uid =
                    Uuid::parse_str(uid_str).map_err(|_| AuthError::BillingOperationFailed)?;
                if uid != stored_order.user_id {
                    return Err(AuthError::BillingOperationFailed);
                }
            }
            if let Some(prod_str) = unsigned_bag.get("product").and_then(Value::as_str) {
                let prod = CheckoutProduct::from_str_code(prod_str)
                    .ok_or(AuthError::BillingOperationFailed)?;
                if prod != stored_order.product {
                    return Err(AuthError::BillingOperationFailed);
                }
            }
            if let Some(seats_val) = unsigned_bag.get("seats").and_then(Value::as_u64) {
                if u64::from(stored_order.seats) != seats_val {
                    return Err(AuthError::BillingOperationFailed);
                }
            }
        }

        // Strict Paymob payment & cumulative partial refund lifecycle:
        // - Chargeback -> Chargeback
        // - Cumulative `refunded_amount_cents > 0` -> cumulative Refunded (partial or full)
        // - `is_voided` or `is_refunded` (with `refunded_amount_cents == 0`) -> full cumulative Refunded (`tx.amount_cents`)
        // - `pending` or auth-only without capture (`is_auth && !is_capture && !is_standalone_payment`) -> PendingNoGrant
        // - `success && !error_occured` (captured or standalone) -> Paid
        let kind = if tx.is_chargeback {
            VerifiedWebhookKind::Chargeback
        } else if tx.refunded_amount_cents > 0 {
            VerifiedWebhookKind::Refunded {
                partial_amount_minor: None,
                cumulative_refunded_minor: Some(tx.refunded_amount_cents),
                provider_refund_id: None,
                mandatory_legal_override: true,
            }
        } else if tx.is_refunded || tx.is_voided {
            VerifiedWebhookKind::Refunded {
                partial_amount_minor: None,
                cumulative_refunded_minor: Some(tx.amount_cents),
                provider_refund_id: None,
                mandatory_legal_override: true,
            }
        } else if tx.pending || (tx.is_auth && !tx.is_capture && !tx.is_standalone_payment) {
            VerifiedWebhookKind::PendingNoGrant
        } else if tx.success && !tx.error_occured {
            VerifiedWebhookKind::Paid
        } else {
            VerifiedWebhookKind::Failed
        };

        let provider_tx_id = tx.id.to_string();
        let event_id = delivery_event_id
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| {
                if tx.refunded_amount_cents > 0 || tx.is_refunded || tx.is_voided {
                    format!(
                        "paymob_tx_{}_ref_{}_{}_{}",
                        tx.id, tx.refunded_amount_cents, tx.is_refunded, tx.is_voided
                    )
                } else {
                    format!(
                        "paymob_tx_{}_{}_{}_{}",
                        tx.id, tx.pending, tx.is_capture, tx.success
                    )
                }
            });

        let mut payload_hash = [0u8; 32];
        payload_hash.copy_from_slice(&Sha256::digest(raw_body));

        let occurred_at_unix = parse_rfc3339_to_unix_secs(&tx.created_at).unwrap_or(now_secs);

        let transition = VerifiedWebhookTransition {
            provider: BillingProvider::Paymob,
            provider_event_id: event_id,
            event_type: "paymob.transaction".to_owned(),
            raw_payload_hash: payload_hash,
            occurred_at_unix,
            now_secs,
            order_id_hint: Some(stored_order.id),
            provider_order_ref: Some(provider_order_ref),
            provider_transaction_id: Some(provider_tx_id.clone()),
            provider_subscription_id: None,
            external_invoice_id: Some(format!("inv_paymob_{provider_tx_id}")),
            reported_amount_minor: Some(tx.amount_cents),
            reported_currency: Some(reported_currency),
            reported_integration_id: Some(tx.integration_id),
            period_start_unix: None,
            period_end_unix: None,
            is_renewal_hint: false,
            kind,
        };

        self.execute_verified_webhook_atomic(&transition).await
    }

    async fn resolve_stored_order_for_check(
        &self,
        provider: BillingProvider,
        order_id_hint: Option<Uuid>,
        provider_order_ref: Option<&str>,
        provider_tx_id: Option<&str>,
        provider_subscription_id: Option<&str>,
    ) -> Result<Option<BillingOrderRecord>, AuthError> {
        if let Some(ref pg) = self.pg_store {
            if let Some(oid) = order_id_hint {
                if let Some(o) = pg.get_billing_order_by_id_pg(oid).await? {
                    if o.provider == provider {
                        return Ok(Some(o));
                    }
                }
            }
            if let Some(pref) = provider_order_ref {
                if let Some(o) = pg
                    .find_billing_order_by_provider_ref_pg(provider, pref)
                    .await?
                {
                    return Ok(Some(o));
                }
            }
            if let Some(psub) = provider_subscription_id {
                if let Some(o) = pg
                    .find_billing_order_by_subscription_id_pg(provider, psub)
                    .await?
                {
                    return Ok(Some(o));
                }
            }
            if let Some(ptx) = provider_tx_id {
                if let Some(o) = pg
                    .find_billing_order_by_provider_ref_pg(provider, ptx)
                    .await?
                {
                    return Ok(Some(o));
                }
            }
            return Ok(None);
        }
        let guard = self.store.lock().await;
        if let Some(oid) = order_id_hint {
            if let Some(o) = guard.orders.get(&oid) {
                if o.provider == provider {
                    return Ok(Some(o.clone()));
                }
            }
        }
        if let Some(pref) = provider_order_ref {
            if let Some(&oid) = guard
                .orders_by_provider_ref
                .get(&(provider, pref.to_owned()))
            {
                return Ok(guard.orders.get(&oid).cloned());
            }
        }
        if let Some(psub) = provider_subscription_id {
            if let Some(&oid) = guard
                .orders_by_subscription
                .get(&(provider, psub.to_owned()))
            {
                return Ok(guard.orders.get(&oid).cloned());
            }
        }
        if let Some(ptx) = provider_tx_id {
            if let Some(&oid) = guard
                .orders_by_provider_ref
                .get(&(provider, ptx.to_owned()))
            {
                return Ok(guard.orders.get(&oid).cloned());
            }
            if let Some(o) = guard.orders.values().find(|o| {
                o.provider == provider && o.provider_transaction_id.as_deref() == Some(ptx)
            }) {
                return Ok(Some(o.clone()));
            }
        }
        Ok(None)
    }

    async fn execute_verified_webhook_atomic(
        &self,
        t: &VerifiedWebhookTransition,
    ) -> Result<BillingEventRecord, AuthError> {
        // Production PostgreSQL 18 path: ONE atomic DB transaction!
        if let Some(ref pg) = self.pg_store {
            let (rec, updated_ent, committed_coupon) = pg.process_webhook_atomic_pg(t).await?;
            if let Some(ent_rec) = updated_ent {
                self.entitlements.sync_workspace_record(ent_rec).await;
            }
            if let (Some(coupons), Some(res_id)) = (&self.coupons, committed_coupon) {
                coupons.commit_reservation(res_id, t.now_secs).await;
            }
            return Ok(rec);
        }

        // In-memory atomic fallback (single exclusive async Mutex — zero concurrent race!).
        let mut guard = self.store.lock().await;
        let event_key = (t.provider.as_str().to_owned(), t.provider_event_id.clone());
        if guard.processed_events.contains(&event_key) {
            if let Some(existing_hash) = guard.processed_event_hashes.get(&event_key) {
                if existing_hash != &t.raw_payload_hash {
                    guard.security_anomalies.push(format!(
                        "webhook_event_id_payload_hash_mismatch:{}:{}",
                        t.provider.as_str(),
                        t.provider_event_id
                    ));
                    return Err(AuthError::WebhookPayloadHashConflict);
                }
            }
            return Ok(BillingEventRecord {
                id: Uuid::now_v7(),
                provider: t.provider,
                external_event_id: t.provider_event_id.clone(),
                event_type: t.event_type.clone(),
                workspace_id: None,
                occurred_at: t.occurred_at_unix,
                processed_at: t.now_secs,
                outcome: "idempotent_duplicate_ignored".to_owned(),
            });
        }

        // Locate correlated stored order strictly (order_id, provider_order_ref, subscription_id, or tx_id).
        let order_id = t
            .order_id_hint
            .filter(|oid| {
                guard
                    .orders
                    .get(oid)
                    .is_some_and(|o| o.provider == t.provider)
            })
            .or_else(|| {
                t.provider_order_ref.as_ref().and_then(|pref| {
                    guard
                        .orders_by_provider_ref
                        .get(&(t.provider, pref.clone()))
                        .copied()
                })
            })
            .or_else(|| {
                t.provider_subscription_id.as_ref().and_then(|psub| {
                    guard
                        .orders_by_subscription
                        .get(&(t.provider, psub.clone()))
                        .copied()
                })
            })
            .or_else(|| {
                t.provider_transaction_id.as_ref().and_then(|ptx| {
                    guard
                        .orders
                        .values()
                        .find(|o| {
                            o.provider == t.provider
                                && (o.provider_transaction_id.as_deref() == Some(ptx.as_str())
                                    || o.provider_order_ref.as_deref() == Some(ptx.as_str()))
                        })
                        .map(|o| o.id)
                })
            })
            .ok_or(AuthError::BillingOperationFailed)?;

        let order = guard
            .orders
            .get(&order_id)
            .cloned()
            .ok_or(AuthError::BillingOperationFailed)?;

        // Validate currency, integration_id, and paid amount strictly against stored order!
        if let Some(curr) = t.reported_currency {
            if curr != order.currency {
                return Err(AuthError::BillingOperationFailed);
            }
        }
        if let Some(exp_int) = order.expected_integration_id {
            if t.reported_integration_id != Some(exp_int) {
                return Err(AuthError::BillingOperationFailed);
            }
        }
        if matches!(t.kind, VerifiedWebhookKind::Paid) {
            match t.reported_amount_minor {
                Some(rep_amt) if rep_amt == order.expected_amount_minor => {}
                _ => return Err(AuthError::BillingOperationFailed),
            }
        }

        // Out-of-order monotonic watermark check.
        if let Some(&last_ts) = guard.last_event_ts_by_workspace.get(&order.workspace_id) {
            if t.occurred_at_unix < last_ts {
                guard.processed_events.insert(event_key.clone());
                guard
                    .processed_event_hashes
                    .insert(event_key, t.raw_payload_hash);
                let rec = BillingEventRecord {
                    id: Uuid::now_v7(),
                    provider: t.provider,
                    external_event_id: t.provider_event_id.clone(),
                    event_type: t.event_type.clone(),
                    workspace_id: Some(order.workspace_id),
                    occurred_at: t.occurred_at_unix,
                    processed_at: t.now_secs,
                    outcome: "stale_out_of_order_ignored".to_owned(),
                };
                guard.events.push(rec.clone());
                return Ok(rec);
            }
        }

        // Protect terminal Refunded / Chargeback state from resurrection by stale events.
        if matches!(
            order.state,
            PaymentOrderState::Refunded | PaymentOrderState::Chargeback
        ) && !matches!(
            t.kind,
            VerifiedWebhookKind::Refunded { .. } | VerifiedWebhookKind::Chargeback
        ) {
            guard.processed_events.insert(event_key.clone());
            guard
                .processed_event_hashes
                .insert(event_key, t.raw_payload_hash);
            let rec = BillingEventRecord {
                id: Uuid::now_v7(),
                provider: t.provider,
                external_event_id: t.provider_event_id.clone(),
                event_type: t.event_type.clone(),
                workspace_id: Some(order.workspace_id),
                occurred_at: t.occurred_at_unix,
                processed_at: t.now_secs,
                outcome: "terminal_state_protected".to_owned(),
            };
            guard.events.push(rec.clone());
            return Ok(rec);
        }

        let outcome: String;
        match &t.kind {
            VerifiedWebhookKind::Paid => {
                let provider_tx_id = t
                    .provider_transaction_id
                    .as_deref()
                    .or(order.provider_order_ref.as_deref())
                    .filter(|s| !s.trim().is_empty())
                    .ok_or(AuthError::BillingOperationFailed)?
                    .to_owned();
                let effective_sub_id = t
                    .provider_subscription_id
                    .clone()
                    .or_else(|| order.provider_subscription_id.clone());
                let is_subscription_product = matches!(
                    order.product,
                    CheckoutProduct::IndividualMonthly
                        | CheckoutProduct::IndividualAnnual
                        | CheckoutProduct::OrganizationSeatAnnual
                );
                let is_renewal = is_subscription_product
                    && order.state == PaymentOrderState::Paid
                    && effective_sub_id.is_some()
                    && (t.is_renewal_hint
                        || order
                            .provider_transaction_id
                            .as_deref()
                            .is_some_and(|orig_tx| orig_tx != provider_tx_id));

                let (grant_kind, qty) = if is_renewal {
                    let q = match order.product {
                        CheckoutProduct::OrganizationSeatAnnual => order.seats.max(ORG_MIN_SEATS),
                        _ => 1u32,
                    };
                    ("subscription_renewal", q)
                } else {
                    match order.product {
                        CheckoutProduct::CreditPack25 => ("credit_pack_25", 25u32),
                        CheckoutProduct::PayPerFile => ("pay_per_file", 1u32),
                        CheckoutProduct::OrganizationSeatAnnual => {
                            ("subscription", order.seats.max(ORG_MIN_SEATS))
                        }
                        _ => ("subscription", 1u32),
                    }
                };

                let existing_ent = self
                    .entitlements
                    .snapshot_workspace(order.workspace_id)
                    .await;
                let base_period_start = if is_renewal {
                    t.period_start_unix
                        .or_else(|| existing_ent.as_ref().and_then(|e| e.current_period_end))
                        .unwrap_or(t.now_secs)
                } else {
                    t.period_start_unix.unwrap_or(t.now_secs)
                };
                let period_end = t.period_end_unix.or_else(|| match order.product {
                    CheckoutProduct::IndividualMonthly => Some(base_period_start + 30 * 86_400),
                    CheckoutProduct::IndividualAnnual | CheckoutProduct::OrganizationSeatAnnual => {
                        Some(base_period_start + 365 * 86_400)
                    }
                    CheckoutProduct::CreditPack25 | CheckoutProduct::PayPerFile => None,
                });

                let duplicate_tx = guard
                    .grants_by_provider_tx
                    .contains(&(t.provider, provider_tx_id.clone()));
                let duplicate_initial = !is_renewal
                    && guard
                        .grants_by_order_kind
                        .contains(&(order.id, grant_kind.to_owned()));
                let duplicate_renewal_period = is_renewal
                    && effective_sub_id
                        .as_ref()
                        .zip(period_end)
                        .is_some_and(|(sub, pe)| {
                            guard
                                .grants_by_renewal_period
                                .contains(&(t.provider, sub.clone(), pe))
                        });

                if duplicate_tx || duplicate_initial || duplicate_renewal_period {
                    guard.processed_events.insert(event_key.clone());
                    guard
                        .processed_event_hashes
                        .insert(event_key, t.raw_payload_hash);
                    let rec = BillingEventRecord {
                        id: Uuid::now_v7(),
                        provider: t.provider,
                        external_event_id: t.provider_event_id.clone(),
                        event_type: t.event_type.clone(),
                        workspace_id: Some(order.workspace_id),
                        occurred_at: t.occurred_at_unix,
                        processed_at: t.now_secs,
                        outcome: "duplicate_transaction_grant_ignored".to_owned(),
                    };
                    guard.events.push(rec.clone());
                    return Ok(rec);
                }

                // Commit coupon reservation on initial purchase.
                if !is_renewal {
                    if let (Some(coupons), Some(res_id)) =
                        (&self.coupons, order.coupon_reservation_id)
                    {
                        coupons.commit_reservation(res_id, t.now_secs).await;
                    }
                }

                // Apply entitlement transition first; only commit ledger marks if it succeeds!
                match order.product {
                    CheckoutProduct::CreditPack25 => {
                        self.entitlements
                            .grant_micro_purchase(order.workspace_id, order.user_id, 25, 0)
                            .await;
                        outcome = if t.provider == BillingProvider::Paymob {
                            "paymob_credits_granted_25".to_owned()
                        } else {
                            "credits_granted_25".to_owned()
                        };
                    }
                    CheckoutProduct::PayPerFile => {
                        self.entitlements
                            .grant_micro_purchase(order.workspace_id, order.user_id, 0, 1)
                            .await;
                        outcome = if t.provider == BillingProvider::Paymob {
                            "paymob_pay_per_file_granted".to_owned()
                        } else {
                            "pay_per_file_granted".to_owned()
                        };
                    }
                    other => {
                        let tier = product_to_plan(other)?;
                        let next_period_end = match (
                            existing_ent.as_ref().and_then(|e| e.current_period_end),
                            period_end,
                        ) {
                            (Some(prev), Some(next)) if is_renewal => Some(prev.max(next)),
                            (_, next) => next,
                        };
                        self.entitlements
                            .apply_subscription_update(
                                order.workspace_id,
                                order.user_id,
                                tier,
                                SubscriptionState::Active,
                                order.seats.max(1),
                                next_period_end,
                                t.now_secs,
                            )
                            .await?;
                        outcome = if is_renewal {
                            "subscription_renewed".to_owned()
                        } else if t.provider == BillingProvider::Paymob {
                            "paymob_subscription_active".to_owned()
                        } else {
                            "subscription_active".to_owned()
                        };
                    }
                }

                guard
                    .grants_by_provider_tx
                    .insert((t.provider, provider_tx_id.clone()));
                if !is_renewal {
                    guard
                        .grants_by_order_kind
                        .insert((order.id, grant_kind.to_owned()));
                } else if let (Some(sub), Some(pe)) = (effective_sub_id.as_ref(), period_end) {
                    guard
                        .grants_by_renewal_period
                        .insert((t.provider, sub.clone(), pe));
                }
                guard.grants.push(EntitlementGrantRecord {
                    id: Uuid::now_v7(),
                    order_id: if is_renewal { None } else { Some(order.id) },
                    workspace_id: order.workspace_id,
                    user_id: order.user_id,
                    provider: t.provider,
                    provider_transaction_id: provider_tx_id.clone(),
                    provider_subscription_id: effective_sub_id.clone(),
                    grant_kind: grant_kind.to_owned(),
                    quantity: qty,
                    period_start: if is_subscription_product {
                        Some(base_period_start)
                    } else {
                        None
                    },
                    period_end,
                    revoked_at: None,
                    created_at: t.now_secs,
                });

                let ext_inv = t
                    .external_invoice_id
                    .clone()
                    .unwrap_or_else(|| format!("inv_{}_{}", t.provider.as_str(), provider_tx_id));
                let inv_amt = t
                    .reported_amount_minor
                    .unwrap_or(order.expected_amount_minor);
                guard.invoices.insert(
                    ext_inv.clone(),
                    InvoiceRecord {
                        id: Uuid::now_v7(),
                        workspace_id: order.workspace_id,
                        provider: t.provider,
                        external_invoice_id: ext_inv,
                        order_id: Some(order.id),
                        provider_transaction_id: Some(provider_tx_id.clone()),
                        amount_minor: inv_amt,
                        refunded_amount_minor: 0,
                        currency: order.currency,
                        status: "paid".to_owned(),
                        issued_at: t.now_secs,
                        refunded_at: None,
                    },
                );

                if let Some(ref sub_id) = effective_sub_id {
                    guard
                        .orders_by_subscription
                        .insert((t.provider, sub_id.clone()), order.id);
                }
                if let Some(o_mut) = guard.orders.get_mut(&order.id) {
                    if !is_renewal {
                        o_mut.state = PaymentOrderState::Paid;
                        o_mut.provider_transaction_id = Some(provider_tx_id);
                        o_mut.paid_at = Some(t.now_secs);
                    }
                    if o_mut.provider_subscription_id.is_none() {
                        o_mut.provider_subscription_id = effective_sub_id;
                    }
                    o_mut.updated_at = t.now_secs;
                }
            }
            VerifiedWebhookKind::PendingNoGrant => {
                if let Some(ref sub_id) = t.provider_subscription_id {
                    guard
                        .orders_by_subscription
                        .insert((t.provider, sub_id.clone()), order.id);
                }
                if let Some(o_mut) = guard.orders.get_mut(&order.id) {
                    if o_mut.provider_subscription_id.is_none() {
                        o_mut.provider_subscription_id = t.provider_subscription_id.clone();
                    }
                    if matches!(
                        o_mut.state,
                        PaymentOrderState::Created | PaymentOrderState::CheckoutPending
                    ) {
                        o_mut.state = PaymentOrderState::PaymentPending;
                        o_mut.updated_at = t.now_secs;
                    }
                }
                outcome = "payment_pending_no_grant".to_owned();
            }
            VerifiedWebhookKind::Failed => {
                if let Some(o_mut) = guard.orders.get_mut(&order.id) {
                    if o_mut.state != PaymentOrderState::Paid {
                        o_mut.state = PaymentOrderState::Failed;
                        o_mut.updated_at = t.now_secs;
                    }
                }
                outcome = "payment_failed_no_grant".to_owned();
            }
            VerifiedWebhookKind::PastDue => {
                let tier = product_to_plan(order.product)?;
                self.entitlements
                    .apply_subscription_update(
                        order.workspace_id,
                        order.user_id,
                        tier,
                        SubscriptionState::PastDue,
                        order.seats.max(1),
                        t.period_end_unix,
                        t.now_secs,
                    )
                    .await?;
                outcome = "subscription_past_due_grace_started".to_owned();
            }
            VerifiedWebhookKind::Canceled => {
                let tier = product_to_plan(order.product)?;
                self.entitlements
                    .apply_subscription_update(
                        order.workspace_id,
                        order.user_id,
                        tier,
                        SubscriptionState::Canceled,
                        order.seats.max(1),
                        t.period_end_unix,
                        t.now_secs,
                    )
                    .await?;
                if let Some(o_mut) = guard.orders.get_mut(&order.id) {
                    if o_mut.state != PaymentOrderState::Paid {
                        o_mut.state = PaymentOrderState::Canceled;
                        o_mut.updated_at = t.now_secs;
                    }
                }
                outcome = "subscription_canceled".to_owned();
            }
            VerifiedWebhookKind::Refunded {
                partial_amount_minor,
                cumulative_refunded_minor,
                provider_refund_id,
                mandatory_legal_override,
            } => {
                let target_inv_key = guard
                    .invoices
                    .values()
                    .find(|i| {
                        i.order_id == Some(order.id)
                            || t.external_invoice_id
                                .as_deref()
                                .is_some_and(|id| i.external_invoice_id == id)
                    })
                    .map(|i| i.external_invoice_id.clone());

                let (captured_amt, already_ref, issued_at) = target_inv_key
                    .as_ref()
                    .and_then(|k| guard.invoices.get(k))
                    .map(|inv| (inv.amount_minor, inv.refunded_amount_minor, inv.issued_at))
                    .unwrap_or((order.expected_amount_minor, 0, t.now_secs));

                if !*mandatory_legal_override && t.now_secs - issued_at > REFUND_WINDOW_SECS {
                    return Err(AuthError::BillingOperationFailed);
                }

                let new_total = if let Some(cum) = *cumulative_refunded_minor {
                    if cum > captured_amt {
                        return Err(AuthError::BillingOperationFailed);
                    }
                    if cum <= already_ref {
                        guard.processed_events.insert(event_key.clone());
                        guard
                            .processed_event_hashes
                            .insert(event_key, t.raw_payload_hash);
                        let rec = BillingEventRecord {
                            id: Uuid::now_v7(),
                            provider: t.provider,
                            external_event_id: t.provider_event_id.clone(),
                            event_type: t.event_type.clone(),
                            workspace_id: Some(order.workspace_id),
                            occurred_at: t.occurred_at_unix,
                            processed_at: t.now_secs,
                            outcome: "cumulative_refund_duplicate_or_stale_ignored".to_owned(),
                        };
                        guard.events.push(rec.clone());
                        return Ok(rec);
                    }
                    cum
                } else {
                    if let Some(ref pref_id) = provider_refund_id {
                        if guard
                            .confirmed_provider_refund_ids
                            .contains(&(t.provider, pref_id.clone()))
                        {
                            guard.processed_events.insert(event_key.clone());
                            guard
                                .processed_event_hashes
                                .insert(event_key, t.raw_payload_hash);
                            let rec = BillingEventRecord {
                                id: Uuid::now_v7(),
                                provider: t.provider,
                                external_event_id: t.provider_event_id.clone(),
                                event_type: t.event_type.clone(),
                                workspace_id: Some(order.workspace_id),
                                occurred_at: t.occurred_at_unix,
                                processed_at: t.now_secs,
                                outcome: "duplicate_provider_refund_ignored".to_owned(),
                            };
                            guard.events.push(rec.clone());
                            return Ok(rec);
                        }
                    }
                    let rem = captured_amt.saturating_sub(already_ref);
                    let delta = partial_amount_minor.unwrap_or(rem);
                    if delta == 0 || already_ref.saturating_add(delta) > captured_amt {
                        return Err(AuthError::BillingOperationFailed);
                    }
                    already_ref + delta
                };

                if let Some(ref pref_id) = provider_refund_id {
                    guard
                        .confirmed_provider_refund_ids
                        .insert((t.provider, pref_id.clone()));
                }

                let is_partial = new_total < captured_amt;
                if let Some(ref inv_key) = target_inv_key {
                    if let Some(inv) = guard.invoices.get_mut(inv_key) {
                        inv.refunded_amount_minor = new_total;
                        inv.status = if is_partial {
                            "partially_refunded".to_owned()
                        } else {
                            "refunded".to_owned()
                        };
                        inv.refunded_at = Some(t.now_secs);
                    }
                }
                if let Some(o_mut) = guard.orders.get_mut(&order.id) {
                    o_mut.state = if is_partial {
                        PaymentOrderState::PartiallyRefunded
                    } else {
                        PaymentOrderState::Refunded
                    };
                    o_mut.updated_at = t.now_secs;
                }
                if !is_partial {
                    for g in &mut guard.grants {
                        if g.order_id == Some(order.id) && g.revoked_at.is_none() {
                            g.revoked_at = Some(t.now_secs);
                        }
                    }
                    self.entitlements
                        .apply_subscription_update(
                            order.workspace_id,
                            order.user_id,
                            PlanTier::Free,
                            SubscriptionState::Canceled,
                            1,
                            Some(t.now_secs),
                            t.now_secs,
                        )
                        .await?;
                }
                outcome = if is_partial {
                    "partial_refund_processed".to_owned()
                } else {
                    "refund_processed".to_owned()
                };
            }
            VerifiedWebhookKind::Chargeback => {
                let target_inv_key = guard
                    .invoices
                    .values()
                    .find(|i| {
                        i.order_id == Some(order.id)
                            || t.external_invoice_id
                                .as_deref()
                                .is_some_and(|id| i.external_invoice_id == id)
                    })
                    .map(|i| i.external_invoice_id.clone());
                if let Some(inv_key) = target_inv_key {
                    if let Some(inv) = guard.invoices.get_mut(&inv_key) {
                        inv.status = "chargeback".to_owned();
                        inv.refunded_at = Some(t.now_secs);
                    }
                } else if let Some(ref ext_inv) = t.external_invoice_id {
                    guard.invoices.insert(
                        ext_inv.clone(),
                        InvoiceRecord {
                            id: Uuid::now_v7(),
                            workspace_id: order.workspace_id,
                            provider: t.provider,
                            external_invoice_id: ext_inv.clone(),
                            order_id: Some(order.id),
                            provider_transaction_id: t.provider_transaction_id.clone(),
                            amount_minor: order.expected_amount_minor,
                            refunded_amount_minor: order.expected_amount_minor,
                            currency: order.currency,
                            status: "chargeback".to_owned(),
                            issued_at: t.now_secs,
                            refunded_at: Some(t.now_secs),
                        },
                    );
                }
                if let Some(o_mut) = guard.orders.get_mut(&order.id) {
                    o_mut.state = PaymentOrderState::Chargeback;
                    o_mut.updated_at = t.now_secs;
                }
                for g in &mut guard.grants {
                    if g.order_id == Some(order.id) && g.revoked_at.is_none() {
                        g.revoked_at = Some(t.now_secs);
                    }
                }
                self.entitlements
                    .apply_subscription_update(
                        order.workspace_id,
                        order.user_id,
                        PlanTier::Free,
                        SubscriptionState::Canceled,
                        1,
                        Some(t.now_secs),
                        t.now_secs,
                    )
                    .await?;
                outcome = "chargeback_revoked".to_owned();
            }
        }

        let record = BillingEventRecord {
            id: Uuid::now_v7(),
            provider: t.provider,
            external_event_id: t.provider_event_id.clone(),
            event_type: t.event_type.clone(),
            workspace_id: Some(order.workspace_id),
            occurred_at: t.occurred_at_unix,
            processed_at: t.now_secs,
            outcome,
        };
        guard.processed_events.insert(event_key.clone());
        guard
            .processed_event_hashes
            .insert(event_key, t.raw_payload_hash);
        let prev = guard
            .last_event_ts_by_workspace
            .get(&order.workspace_id)
            .copied()
            .unwrap_or(0);
        guard
            .last_event_ts_by_workspace
            .insert(order.workspace_id, prev.max(t.occurred_at_unix));
        guard.events.push(record.clone());
        Ok(record)
    }

    /// Issue an owner-authorized refund honoring the approved 3-day window
    /// (unless `mandatory_legal_override` is explicitly flagged for statutory compliance),
    /// invoking the real provider refund API with idempotent `refund_request_key`, and
    /// revoking entitlements ONLY once the provider confirms the refund.
    pub async fn issue_owner_refund(
        &self,
        external_invoice_id: &str,
        mandatory_legal_override: bool,
        now_secs: i64,
    ) -> Result<InvoiceRecord, AuthError> {
        let default_key = format!("refund:{external_invoice_id}:full");
        self.issue_owner_refund_idempotent(
            external_invoice_id,
            &default_key,
            None,
            mandatory_legal_override,
            "owner_authorized_refund",
            None,
            now_secs,
        )
        .await
    }

    /// Full idempotent owner refund entry point supporting partial refunds, stable request keys,
    /// and real provider refund calls.
    #[allow(clippy::too_many_arguments)]
    pub async fn issue_owner_refund_idempotent(
        &self,
        external_invoice_id: &str,
        refund_request_key: &str,
        partial_amount_minor: Option<u64>,
        mandatory_legal_override: bool,
        reason: &str,
        actor_user_id: Option<Uuid>,
        now_secs: i64,
    ) -> Result<InvoiceRecord, AuthError> {
        let req_key = refund_request_key.trim();
        if req_key.is_empty() {
            return Err(AuthError::InvalidRequest);
        }

        // Load invoice from PostgreSQL (if attached) or in-memory store.
        let invoice = if let Some(ref pg) = self.pg_store {
            pg.get_invoice_by_external_id_pg(external_invoice_id)
                .await?
                .ok_or(AuthError::BillingOperationFailed)?
        } else {
            self.store
                .lock()
                .await
                .invoices
                .get(external_invoice_id)
                .cloned()
                .ok_or(AuthError::BillingOperationFailed)?
        };

        // Idempotent retry check: if this EXACT `(workspace_id, refund_request_key)` was already
        // claimed (e.g. `refund_pending` / `reconciliation_required` / `confirmed`), return the
        // existing invoice state without ever firing a second external provider refund!
        let already_claimed = if let Some(ref pg) = self.pg_store {
            pg.has_refund_request_key_pg(invoice.workspace_id, req_key)
                .await?
        } else {
            self.store
                .lock()
                .await
                .refunds
                .contains_key(&(invoice.workspace_id, req_key.to_owned()))
        };
        if already_claimed {
            return Ok(invoice);
        }

        if matches!(
            invoice.status.as_str(),
            "refunded" | "chargeback" | "refund_pending"
        ) {
            return Err(AuthError::BillingOperationFailed);
        }
        if !mandatory_legal_override && now_secs - invoice.issued_at > REFUND_WINDOW_SECS {
            return Err(AuthError::BillingOperationFailed);
        }

        let refund_amount = match partial_amount_minor {
            Some(amt) => {
                if amt == 0
                    || amt
                        > invoice
                            .amount_minor
                            .saturating_sub(invoice.refunded_amount_minor)
                {
                    return Err(AuthError::InvalidRequest);
                }
                amt
            }
            None => invoice
                .amount_minor
                .saturating_sub(invoice.refunded_amount_minor),
        };
        let is_partial = partial_amount_minor.is_some() && refund_amount < invoice.amount_minor;

        let claim_rec = RefundRecord {
            id: Uuid::now_v7(),
            order_id: invoice.order_id,
            workspace_id: invoice.workspace_id,
            provider: invoice.provider,
            external_invoice_id: invoice.external_invoice_id.clone(),
            refund_request_key: req_key.to_owned(),
            provider_refund_id: None,
            amount_minor: refund_amount,
            currency: invoice.currency,
            kind: if is_partial {
                "partial_refund".to_owned()
            } else {
                "refund".to_owned()
            },
            status: "requested".to_owned(),
            mandatory_legal_override,
            reason: reason.to_owned(),
            requested_by: actor_user_id,
            confirmed_at: None,
            created_at: now_secs,
        };

        // Atomically claim `(workspace_id, refund_request_key)` so repeated clicks NEVER trigger 2 provider refunds!
        let claimed = if let Some(ref pg) = self.pg_store {
            pg.claim_refund_request_pg(&claim_rec).await?
        } else {
            let mut guard = self.store.lock().await;
            let key = (invoice.workspace_id, req_key.to_owned());
            if let std::collections::hash_map::Entry::Vacant(e) = guard.refunds.entry(key) {
                e.insert(claim_rec.clone());
                if let Some(inv_mut) = guard.invoices.get_mut(external_invoice_id) {
                    inv_mut.status = "refund_pending".to_owned();
                }
                Some(claim_rec.clone())
            } else {
                None
            }
        };

        let Some(refund_row) = claimed else {
            // Duplicate refund request key -> return current invoice state WITHOUT calling provider again!
            return if let Some(ref pg) = self.pg_store {
                pg.get_invoice_by_external_id_pg(external_invoice_id)
                    .await?
                    .ok_or(AuthError::BillingOperationFailed)
            } else {
                self.store
                    .lock()
                    .await
                    .invoices
                    .get(external_invoice_id)
                    .cloned()
                    .ok_or(AuthError::BillingOperationFailed)
            };
        };

        // Call real external provider refund API when provider is Paddle or Paymob!
        let provider_outcome = match invoice.provider {
            BillingProvider::Paddle => {
                let adapter = self
                    .paddle_adapter
                    .as_ref()
                    .ok_or(AuthError::BillingNotConfigured)?;
                let tx_id = invoice
                    .provider_transaction_id
                    .as_deref()
                    .ok_or(AuthError::BillingOperationFailed)?;
                adapter
                    .create_refund_adjustment(
                        tx_id,
                        req_key,
                        reason,
                        if is_partial {
                            Some(refund_amount)
                        } else {
                            None
                        },
                    )
                    .await
            }
            BillingProvider::Paymob => {
                let adapter = self
                    .paymob_adapter
                    .as_ref()
                    .ok_or(AuthError::BillingNotConfigured)?;
                let tx_num = invoice
                    .provider_transaction_id
                    .as_deref()
                    .and_then(|s| s.parse::<u64>().ok())
                    .ok_or(AuthError::BillingOperationFailed)?;
                adapter.refund_transaction(tx_num, refund_amount).await
            }
            BillingProvider::Manual => Ok(provider_http::ProviderRefundOutcome {
                provider_refund_id: format!("manual_ref_{}", refund_row.id.simple()),
                confirmed: true,
                raw_status: "refunded".to_owned(),
            }),
        };

        let (provider_ref_id, confirmed, failed, reconciliation_required) = match provider_outcome {
            Ok(out) => (Some(out.provider_refund_id), out.confirmed, false, false),
            Err(ProviderCallError::UncertainOutcome(_)) => (None, false, false, true),
            Err(_) => (None, false, true, false),
        };

        if let Some(ref pg) = self.pg_store {
            let (updated_inv, updated_ent) = pg
                .finalize_refund_request_pg(
                    refund_row.id,
                    external_invoice_id,
                    invoice.workspace_id,
                    provider_ref_id.as_deref(),
                    confirmed,
                    if is_partial {
                        Some(refund_amount)
                    } else {
                        None
                    },
                    failed,
                    reconciliation_required,
                    now_secs,
                )
                .await?;
            if let Some(ent_rec) = updated_ent {
                self.entitlements.sync_workspace_record(ent_rec).await;
            }
            return Ok(updated_inv);
        }

        // In-memory finalization.
        let mut guard = self.store.lock().await;
        if reconciliation_required {
            if let Some(r) = guard
                .refunds
                .get_mut(&(invoice.workspace_id, req_key.to_owned()))
            {
                r.status = "reconciliation_required".to_owned();
            }
            return Err(AuthError::PaymentReconciliationRequired);
        }
        if failed {
            if let Some(r) = guard
                .refunds
                .get_mut(&(invoice.workspace_id, req_key.to_owned()))
            {
                r.status = "failed".to_owned();
            }
            if let Some(inv_mut) = guard.invoices.get_mut(external_invoice_id) {
                inv_mut.status = "paid".to_owned();
            }
            return Err(AuthError::BillingOperationFailed);
        }
        if let Some(r) = guard
            .refunds
            .get_mut(&(invoice.workspace_id, req_key.to_owned()))
        {
            r.provider_refund_id = provider_ref_id;
            r.status = if confirmed {
                "confirmed".to_owned()
            } else {
                "requested".to_owned()
            };
            if confirmed {
                r.confirmed_at = Some(now_secs);
            }
        }
        let updated_inv = {
            let inv_mut = guard
                .invoices
                .get_mut(external_invoice_id)
                .ok_or(AuthError::BillingOperationFailed)?;
            if confirmed {
                inv_mut.refunded_amount_minor = inv_mut
                    .refunded_amount_minor
                    .saturating_add(refund_amount)
                    .min(inv_mut.amount_minor);
                inv_mut.status = if is_partial {
                    "partially_refunded".to_owned()
                } else {
                    "refunded".to_owned()
                };
                inv_mut.refunded_at = Some(now_secs);
            } else {
                inv_mut.status = "refund_pending".to_owned();
            }
            inv_mut.clone()
        };
        if confirmed {
            if let Some(oid) = updated_inv.order_id {
                if let Some(o_mut) = guard.orders.get_mut(&oid) {
                    o_mut.state = if is_partial {
                        PaymentOrderState::PartiallyRefunded
                    } else {
                        PaymentOrderState::Refunded
                    };
                    o_mut.updated_at = now_secs;
                }
            }
            if !is_partial {
                self.entitlements
                    .apply_subscription_update(
                        updated_inv.workspace_id,
                        Uuid::nil(),
                        PlanTier::Free,
                        SubscriptionState::Canceled,
                        1,
                        Some(now_secs),
                        now_secs,
                    )
                    .await?;
            }
        }
        Ok(updated_inv)
    }

    /// Return recent billing webhook events for the Owner Control Plane (PostgreSQL authoritative when attached).
    pub async fn list_events(&self, limit: usize) -> Vec<BillingEventRecord> {
        if let Some(ref pg) = self.pg_store {
            if let Ok(rows) = pg.list_billing_events_pg(limit).await {
                return rows;
            }
        }
        let guard = self.store.lock().await;
        guard
            .events
            .iter()
            .rev()
            .take(limit.clamp(1, 200))
            .cloned()
            .collect()
    }

    /// Return invoices for a workspace (PostgreSQL authoritative when attached).
    pub async fn list_invoices(&self, workspace_id: Uuid) -> Vec<InvoiceRecord> {
        if let Some(ref pg) = self.pg_store {
            if let Ok(rows) = pg.list_workspace_invoices_pg(workspace_id).await {
                return rows;
            }
        }
        self.store
            .lock()
            .await
            .invoices
            .values()
            .filter(|i| i.workspace_id == workspace_id)
            .cloned()
            .collect()
    }

    /// Record a dispute/chargeback and immediately revoke paid entitlements.
    pub async fn record_chargeback(
        &self,
        external_invoice_id: &str,
        workspace_id: Uuid,
        user_id: Uuid,
        now_secs: i64,
    ) -> Result<InvoiceRecord, AuthError> {
        if let Some(ref pg) = self.pg_store {
            let inv = pg
                .record_chargeback_pg(external_invoice_id, workspace_id, now_secs)
                .await?;
            self.entitlements
                .apply_subscription_update(
                    inv.workspace_id,
                    user_id,
                    PlanTier::Free,
                    SubscriptionState::Canceled,
                    1,
                    Some(now_secs),
                    now_secs,
                )
                .await?;
            return Ok(inv);
        }

        let mut guard = self.store.lock().await;
        let inv = guard
            .invoices
            .entry(external_invoice_id.to_owned())
            .or_insert_with(|| InvoiceRecord {
                id: Uuid::now_v7(),
                workspace_id,
                provider: BillingProvider::Manual,
                external_invoice_id: external_invoice_id.to_owned(),
                order_id: None,
                provider_transaction_id: None,
                amount_minor: 0,
                refunded_amount_minor: 0,
                currency: BillingCurrency::Usd,
                status: "chargeback".to_owned(),
                issued_at: now_secs,
                refunded_at: Some(now_secs),
            });
        inv.status = "chargeback".to_owned();
        inv.refunded_at = Some(now_secs);
        let invoice = inv.clone();
        if let Some(oid) = invoice.order_id {
            if let Some(o_mut) = guard.orders.get_mut(&oid) {
                o_mut.state = PaymentOrderState::Chargeback;
                o_mut.updated_at = now_secs;
            }
        }
        self.entitlements
            .apply_subscription_update(
                invoice.workspace_id,
                user_id,
                PlanTier::Free,
                SubscriptionState::Canceled,
                1,
                Some(now_secs),
                now_secs,
            )
            .await?;
        guard.events.push(BillingEventRecord {
            id: Uuid::now_v7(),
            provider: invoice.provider,
            external_event_id: format!("chg_{}", Uuid::now_v7().simple()),
            event_type: "chargeback.recorded".to_owned(),
            workspace_id: Some(invoice.workspace_id),
            occurred_at: now_secs,
            processed_at: now_secs,
            outcome: "chargeback_revoked".to_owned(),
        });
        Ok(invoice)
    }

    /// Return all invoices across the platform (for Owner Billing & Revenue Analytics).
    pub async fn list_all_invoices(&self) -> Vec<InvoiceRecord> {
        if let Some(ref pg) = self.pg_store {
            if let Ok(rows) = pg.list_all_invoices_pg().await {
                return rows;
            }
        }
        let mut out: Vec<InvoiceRecord> =
            self.store.lock().await.invoices.values().cloned().collect();
        out.sort_by_key(|a| std::cmp::Reverse(a.issued_at));
        out
    }

    /// Compute real revenue & refund/chargeback summary from persisted invoices.
    pub async fn revenue_summary(&self) -> BillingRevenueSummary {
        if let Some(ref pg) = self.pg_store {
            if let Ok(sum) = pg.revenue_summary_pg().await {
                return sum;
            }
        }
        let guard = self.store.lock().await;
        let mut summary = BillingRevenueSummary::default();
        for inv in guard.invoices.values() {
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
        summary
    }

    /// Export serializable billing state for durable restart persistence.
    pub async fn export_snapshot(&self) -> BillingSnapshot {
        let guard = self.store.lock().await;
        BillingSnapshot {
            processed_events: guard.processed_events.iter().cloned().collect(),
            last_event_ts_by_workspace: guard
                .last_event_ts_by_workspace
                .iter()
                .map(|(k, v)| (*k, *v))
                .collect(),
            events: guard.events.clone(),
            invoices: guard.invoices.values().cloned().collect(),
            orders: guard.orders.values().cloned().collect(),
            grants: guard.grants.clone(),
            refunds: guard.refunds.values().cloned().collect(),
        }
    }

    /// Restore billing state from a durable snapshot.
    pub async fn import_snapshot(&self, snap: BillingSnapshot) {
        let mut guard = self.store.lock().await;
        guard.processed_events = snap.processed_events.into_iter().collect();
        guard.last_event_ts_by_workspace = snap.last_event_ts_by_workspace.into_iter().collect();
        guard.events = snap.events;
        guard.invoices = snap
            .invoices
            .into_iter()
            .map(|i| (i.external_invoice_id.clone(), i))
            .collect();
        guard.orders.clear();
        guard.orders_by_idempotency.clear();
        guard.orders_by_provider_ref.clear();
        guard.orders_by_subscription.clear();
        for o in snap.orders {
            guard
                .orders_by_idempotency
                .insert((o.workspace_id, o.idempotency_key.clone()), o.id);
            if let Some(ref pref) = o.provider_order_ref {
                guard
                    .orders_by_provider_ref
                    .insert((o.provider, pref.clone()), o.id);
            }
            if let Some(ref psub) = o.provider_subscription_id {
                guard
                    .orders_by_subscription
                    .insert((o.provider, psub.clone()), o.id);
            }
            guard.orders.insert(o.id, o);
        }
        guard.grants_by_provider_tx.clear();
        guard.grants_by_order_kind.clear();
        guard.grants_by_renewal_period.clear();
        for g in &snap.grants {
            guard
                .grants_by_provider_tx
                .insert((g.provider, g.provider_transaction_id.clone()));
            if g.grant_kind != "subscription_renewal" {
                if let Some(oid) = g.order_id {
                    guard
                        .grants_by_order_kind
                        .insert((oid, g.grant_kind.clone()));
                }
            } else if let (Some(ref sub), Some(pe)) = (&g.provider_subscription_id, g.period_end) {
                guard
                    .grants_by_renewal_period
                    .insert((g.provider, sub.clone(), pe));
            }
        }
        guard.grants = snap.grants;
        guard.refunds = snap
            .refunds
            .into_iter()
            .map(|r| ((r.workspace_id, r.refund_request_key.clone()), r))
            .collect();
    }

    /// Current timestamp helper.
    #[must_use]
    pub fn now() -> i64 {
        unix_now()
    }
}

fn order_to_checkout_response(
    order: &BillingOrderRecord,
    reused_existing_order: bool,
) -> CheckoutResponse {
    CheckoutResponse {
        order_id: order.id,
        idempotency_key: order.idempotency_key.clone(),
        order_state: order.state,
        reused_existing_order,
        provider: order.provider,
        currency: order.currency,
        original_amount_minor: order.original_amount_minor,
        discount_amount_minor: order.discount_amount_minor,
        amount_minor: order.expected_amount_minor,
        applied_coupon_code: order.coupon_code.clone(),
        provider_status: IntegrationStatus::Configured,
        provider_order_ref: order.provider_order_ref.clone(),
        checkout_session_id: order
            .provider_intention_id
            .clone()
            .or_else(|| order.provider_order_ref.clone()),
        checkout_mode: order.checkout_mode,
        checkout_url: order.checkout_url.clone(),
    }
}

fn product_to_plan(product: CheckoutProduct) -> Result<PlanTier, AuthError> {
    match product {
        CheckoutProduct::IndividualMonthly => Ok(PlanTier::IndividualMonthly),
        CheckoutProduct::IndividualAnnual => Ok(PlanTier::IndividualAnnual),
        CheckoutProduct::OrganizationSeatAnnual => Ok(PlanTier::OrganizationSeat),
        CheckoutProduct::PayPerFile | CheckoutProduct::CreditPack25 => {
            Err(AuthError::InvalidRequest)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkout_product_credit_pack_25_serializes_and_deserializes_consistently() {
        let serialized = serde_json::to_string(&CheckoutProduct::CreditPack25).expect("ser");
        assert_eq!(serialized, "\"credit_pack_25\"");
        let parsed: CheckoutProduct = serde_json::from_str("\"credit_pack_25\"").expect("de");
        assert_eq!(parsed, CheckoutProduct::CreditPack25);
        assert_eq!(parsed.as_str(), "credit_pack_25");
    }

    #[test]
    fn rfc3339_parser_handles_utc_fractional_and_offsets() {
        assert_eq!(parse_rfc3339_to_unix_secs("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(
            parse_rfc3339_to_unix_secs("2026-11-23T12:00:00.123456Z"),
            parse_rfc3339_to_unix_secs("2026-11-23T14:00:00+02:00")
        );
        assert_eq!(
            parse_rfc3339_to_unix_secs("2026-11-23T12:00:00.000000"),
            parse_rfc3339_to_unix_secs("2026-11-23T12:00:00Z")
        );
        assert_eq!(parse_rfc3339_to_unix_secs("not-a-timestamp"), None);
    }

    #[test]
    fn paymob_customer_billing_data_rejects_invalid_and_accepts_real_details() {
        let good = CustomerBillingData {
            first_name: "Nadia".to_owned(),
            last_name: "Hassan".to_owned(),
            email: "nadia.hassan@example.org".to_owned(),
            phone_number: "+201023456789".to_owned(),
            country: "EG".to_owned(),
        };
        assert!(good.validate().is_ok());

        let mut bad_email = good.clone();
        bad_email.email = "not-an-email".to_owned();
        assert!(bad_email.validate().is_err());

        let mut bad_phone = good.clone();
        bad_phone.phone_number = "123".to_owned();
        assert!(bad_phone.validate().is_err());

        let mut bad_country = good;
        bad_country.country = "EGYPT".to_owned();
        assert!(bad_country.validate().is_err());
    }

    #[tokio::test]
    async fn unconfigured_providers_fail_closed_with_sf_pay_001() {
        let ent = EntitlementService::new();
        let cfg = ServerConfig::for_tests();
        let billing = BillingService::from_config(&cfg, ent);

        let err = billing
            .create_checkout(&CheckoutRequest {
                workspace_id: Uuid::now_v7(),
                user_id: Uuid::now_v7(),
                product: CheckoutProduct::IndividualMonthly,
                billing_country_iso2: "US".to_owned(),
                seats: None,
                coupon_code: None,
                idempotency_key: Some("idem-unconf-1".to_owned()),
                billing_data: None,
                paddle_customer: None,
            })
            .await
            .unwrap_err();
        assert_eq!(err, AuthError::BillingNotConfigured);

        let wh_err = billing
            .process_paddle_webhook("ts=1;h1=00", b"{}", 1)
            .await
            .unwrap_err();
        assert_eq!(wh_err, AuthError::BillingNotConfigured);
    }

    #[tokio::test]
    async fn missing_idempotency_key_and_org_min_seats_rejected() {
        let ent = EntitlementService::new();
        let mut cfg = ServerConfig::for_tests();
        cfg.paddle_api_key = Some("pdl_test_key".to_owned());
        cfg.paddle_webhook_secret = Some("pdl_whsec".to_owned());
        cfg.paddle_price_org_seat_annual = Some("pri_org_1".to_owned());
        let billing = BillingService::from_config(&cfg, ent);

        assert_eq!(
            billing
                .create_checkout(&CheckoutRequest {
                    workspace_id: Uuid::now_v7(),
                    user_id: Uuid::now_v7(),
                    product: CheckoutProduct::IndividualMonthly,
                    billing_country_iso2: "US".to_owned(),
                    seats: None,
                    coupon_code: None,
                    idempotency_key: None,
                    billing_data: None,
                    paddle_customer: None,
                })
                .await
                .unwrap_err(),
            AuthError::InvalidRequest
        );

        assert_eq!(
            billing
                .create_checkout(&CheckoutRequest {
                    workspace_id: Uuid::now_v7(),
                    user_id: Uuid::now_v7(),
                    product: CheckoutProduct::OrganizationSeatAnnual,
                    billing_country_iso2: "US".to_owned(),
                    seats: Some(2),
                    coupon_code: None,
                    idempotency_key: Some("idem-org-2".to_owned()),
                    billing_data: None,
                    paddle_customer: None,
                })
                .await
                .unwrap_err(),
            AuthError::OrgMinSeatsRequired
        );
    }
}
