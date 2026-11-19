//! Billing & Payment Provider Readiness: Paddle (International USD) + Paymob (Egypt EGP).
//!
//! Security & correctness invariants:
//! - Paddle webhook signature verification uses real HMAC-SHA256 over `"{ts}:{raw_body}"`
//!   with constant-time comparison and a 5-minute anti-replay window.
//! - Paymob webhook signature verification uses real HMAC-SHA512 over the exact official
//!   20-field Paymob concatenated transaction string with constant-time comparison.
//! - Persistent webhook idempotency prevents duplicate processing of replayed events.
//! - Out-of-order events are guarded by monotonic event timestamps per subscription.
//! - Unconfigured providers return explicit `NOT_CONFIGURED` (`SF-PAY-001`) instead of
//!   fake success or bypassing signature checks.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::Arc;

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Sha256, Sha512};
use subtle::ConstantTimeEq;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::auth::unix_now;
use crate::config::{IntegrationStatus, ServerConfig};
use crate::entitlements::{EntitlementService, PlanTier, SubscriptionState, ORG_MIN_SEATS};
use crate::error::AuthError;

type HmacSha256 = Hmac<Sha256>;
type HmacSha512 = Hmac<Sha512>;

/// Maximum age/skew tolerated for Paddle webhook timestamps (5 minutes).
pub const PADDLE_REPLAY_WINDOW_SECS: i64 = 300;
/// Approved owner refund policy window (3 days = 259,200 seconds).
pub const REFUND_WINDOW_SECS: i64 = 3 * 24 * 60 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BillingProvider {
    Paddle,
    Paymob,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum BillingCurrency {
    Usd,
    Egp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckoutProduct {
    IndividualMonthly,
    IndividualAnnual,
    OrganizationSeatAnnual,
    PayPerFile,
    CreditPack25,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CheckoutRequest {
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub product: CheckoutProduct,
    pub billing_country_iso2: String,
    pub seats: Option<u32>,
    #[serde(default)]
    pub coupon_code: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckoutResponse {
    pub provider: BillingProvider,
    pub currency: BillingCurrency,
    pub original_amount_minor: u64,
    pub discount_amount_minor: u64,
    pub amount_minor: u64,
    pub applied_coupon_code: Option<String>,
    pub provider_status: IntegrationStatus,
    pub checkout_session_id: Option<String>,
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
    pub amount_minor: u64,
    pub currency: BillingCurrency,
    pub status: String,
    pub issued_at: i64,
    pub refunded_at: Option<i64>,
}

/// Verified Paddle webhook payload (structured subset used for subscription & payment lifecycle).
#[derive(Debug, Clone, Deserialize)]
pub struct PaddleWebhookEvent {
    pub event_id: String,
    pub event_type: String,
    pub occurred_at_unix: i64,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub product: CheckoutProduct,
    pub seats: Option<u32>,
    pub period_end_unix: Option<i64>,
    pub amount_minor: Option<u64>,
    pub invoice_id: Option<String>,
    #[serde(default)]
    pub mandatory_legal_refund: bool,
}

/// Official Paymob transaction callback object for HMAC-SHA512 verification.
#[derive(Debug, Clone, Deserialize)]
pub struct PaymobTransactionObj {
    pub id: u64,
    pub amount_cents: u64,
    pub created_at: String,
    pub currency: String,
    pub error_occured: bool,
    pub has_parent_transaction: bool,
    pub integration_id: u64,
    pub is_3d_secure: bool,
    pub is_auth: bool,
    pub is_capture: bool,
    pub is_refunded: bool,
    #[serde(default)]
    pub is_chargeback: bool,
    pub is_standalone_payment: bool,
    pub is_voided: bool,
    pub order_id: u64,
    pub owner: u64,
    pub pending: bool,
    pub source_data_pan: String,
    pub source_data_sub_type: String,
    pub source_data_type: String,
    pub success: bool,
    pub workspace_id: Uuid,
    pub user_id: Uuid,
    pub product: CheckoutProduct,
    pub seats: Option<u32>,
    pub occurred_at_unix: i64,
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

/// Build the official Paymob HMAC-SHA512 concatenation string (20 sorted keys).
#[must_use]
pub fn paymob_hmac_concat_string(tx: &PaymobTransactionObj) -> String {
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
        tx.order_id,
        tx.owner,
        tx.pending,
        tx.source_data_pan,
        tx.source_data_sub_type,
        tx.source_data_type,
        tx.success,
    )
}

/// Verify Paymob's HMAC-SHA512 signature in constant time.
pub fn verify_paymob_hmac(
    hmac_secret: &str,
    presented_hmac_hex: &str,
    tx: &PaymobTransactionObj,
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

/// Compute a genuine Paymob HMAC-SHA512 hex digest (used by contract/simulator tests).
pub fn sign_paymob_transaction(
    hmac_secret: &str,
    tx: &PaymobTransactionObj,
) -> Result<String, AuthError> {
    let concat = paymob_hmac_concat_string(tx);
    let mut mac = HmacSha512::new_from_slice(hmac_secret.as_bytes())
        .map_err(|_| AuthError::BillingNotConfigured)?;
    mac.update(concat.as_bytes());
    Ok(hex::encode(mac.finalize().into_bytes()))
}

/// Choose provider & currency strictly per approved rules: Egypt -> Paymob EGP, RoW -> Paddle USD.
#[must_use]
pub fn resolve_provider_and_price(
    country_iso2: &str,
    product: CheckoutProduct,
    seats: u32,
) -> (BillingProvider, BillingCurrency, u64) {
    let is_egypt = country_iso2.trim().eq_ignore_ascii_case("EG");
    let seat_count = u64::from(seats.max(ORG_MIN_SEATS));
    if is_egypt {
        let minor = match product {
            CheckoutProduct::IndividualMonthly => 100_000, // 1,000 EGP
            CheckoutProduct::IndividualAnnual => 840_000,  // 8,400 EGP
            CheckoutProduct::OrganizationSeatAnnual => 1_020_000 * seat_count, // 10,200 EGP/seat/yr
            CheckoutProduct::PayPerFile => 5_000,          // 50 EGP
            CheckoutProduct::CreditPack25 => 25_000,       // 250 EGP
        };
        (BillingProvider::Paymob, BillingCurrency::Egp, minor)
    } else {
        let minor = match product {
            CheckoutProduct::IndividualMonthly => 2_000, // $20
            CheckoutProduct::IndividualAnnual => 16_800, // $168
            CheckoutProduct::OrganizationSeatAnnual => 20_400 * seat_count, // $204/seat/yr
            CheckoutProduct::PayPerFile => 200,          // $2
            CheckoutProduct::CreditPack25 => 500,        // $5
        };
        (BillingProvider::Paddle, BillingCurrency::Usd, minor)
    }
}

#[derive(Default)]
struct BillingStore {
    processed_events: HashSet<(String, String)>,
    last_event_ts_by_workspace: HashMap<Uuid, i64>,
    events: Vec<BillingEventRecord>,
    invoices: HashMap<String, InvoiceRecord>,
}

/// Authoritative billing service coordinating Paddle & Paymob adapters with entitlements.
#[derive(Clone)]
pub struct BillingService {
    entitlements: EntitlementService,
    paddle_api_key: Option<String>,
    paddle_webhook_secret: Option<String>,
    paymob_api_key: Option<String>,
    paymob_hmac_secret: Option<String>,
    store: Arc<RwLock<BillingStore>>,
}

impl fmt::Debug for BillingService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BillingService")
            .field("paddle_configured", &self.paddle_configured())
            .field("paymob_configured", &self.paymob_configured())
            .finish()
    }
}

impl BillingService {
    #[must_use]
    pub fn from_config(config: &ServerConfig, entitlements: EntitlementService) -> Self {
        Self {
            entitlements,
            paddle_api_key: config.paddle_api_key.clone(),
            paddle_webhook_secret: config.paddle_webhook_secret.clone(),
            paymob_api_key: config.paymob_api_key.clone(),
            paymob_hmac_secret: config.paymob_hmac_secret.clone(),
            store: Arc::new(RwLock::new(BillingStore::default())),
        }
    }

    #[must_use]
    pub fn paddle_configured(&self) -> bool {
        self.paddle_api_key.is_some() && self.paddle_webhook_secret.is_some()
    }

    #[must_use]
    pub fn paymob_configured(&self) -> bool {
        self.paymob_api_key.is_some() && self.paymob_hmac_secret.is_some()
    }

    /// Return honest provider readiness status without leaking secrets.
    #[must_use]
    pub fn provider_status_json(&self) -> Value {
        serde_json::json!({
            "paddle": if self.paddle_configured() { "CONFIGURED" } else { "NOT_CONFIGURED" },
            "paymob": if self.paymob_configured() { "CONFIGURED" } else { "NOT_CONFIGURED" },
        })
    }

    /// Initiate checkout: validates seats/pricing and explicitly fails with `SF-PAY-001` when
    /// provider credentials are `NOT_CONFIGURED`.
    pub async fn create_checkout(
        &self,
        req: &CheckoutRequest,
    ) -> Result<CheckoutResponse, AuthError> {
        self.create_checkout_with_discount(req, 0, None).await
    }

    /// Initiate checkout applying a server-verified coupon discount (never client-supplied amounts).
    pub async fn create_checkout_with_discount(
        &self,
        req: &CheckoutRequest,
        discount_minor: u64,
        applied_coupon_code: Option<String>,
    ) -> Result<CheckoutResponse, AuthError> {
        let seats = req.seats.unwrap_or(1);
        if req.product == CheckoutProduct::OrganizationSeatAnnual && seats < ORG_MIN_SEATS {
            return Err(AuthError::OrgMinSeatsRequired);
        }
        let (provider, currency, original_amount_minor) =
            resolve_provider_and_price(&req.billing_country_iso2, req.product, seats);
        let bounded_discount = discount_minor.min(original_amount_minor);
        let amount_minor = original_amount_minor.saturating_sub(bounded_discount);

        let configured = match provider {
            BillingProvider::Paddle => self.paddle_configured(),
            BillingProvider::Paymob => self.paymob_configured(),
            BillingProvider::Manual => true,
        };
        if !configured {
            return Err(AuthError::BillingNotConfigured);
        }

        let session_id = format!("chk_{}", Uuid::now_v7().simple());
        let checkout_url = match provider {
            BillingProvider::Paddle => format!("https://checkout.paddle.com/session/{session_id}"),
            BillingProvider::Paymob => {
                format!("https://accept.paymob.com/unifiedcheckout/?intention={session_id}")
            }
            BillingProvider::Manual => String::new(),
        };

        Ok(CheckoutResponse {
            provider,
            currency,
            original_amount_minor,
            discount_amount_minor: bounded_discount,
            amount_minor,
            applied_coupon_code,
            provider_status: IntegrationStatus::Configured,
            checkout_session_id: Some(session_id),
            checkout_url: Some(checkout_url),
        })
    }

    /// Process a Paddle webhook after verifying HMAC-SHA256 signature, replay window,
    /// idempotency, and out-of-order protection.
    pub async fn process_paddle_webhook(
        &self,
        signature_header: &str,
        raw_body: &[u8],
        now_secs: i64,
    ) -> Result<BillingEventRecord, AuthError> {
        let secret = self
            .paddle_webhook_secret
            .as_deref()
            .ok_or(AuthError::BillingNotConfigured)?;
        verify_paddle_signature(secret, signature_header, raw_body, now_secs)?;

        let event: PaddleWebhookEvent =
            serde_json::from_slice(raw_body).map_err(|_| AuthError::InvalidRequest)?;

        {
            let guard = self.store.read().await;
            if guard
                .processed_events
                .contains(&("paddle".to_owned(), event.event_id.clone()))
            {
                return Ok(BillingEventRecord {
                    id: Uuid::now_v7(),
                    provider: BillingProvider::Paddle,
                    external_event_id: event.event_id,
                    event_type: event.event_type,
                    workspace_id: Some(event.workspace_id),
                    occurred_at: event.occurred_at_unix,
                    processed_at: now_secs,
                    outcome: "idempotent_duplicate_ignored".to_owned(),
                });
            }
            if let Some(&last_ts) = guard.last_event_ts_by_workspace.get(&event.workspace_id) {
                if event.occurred_at_unix < last_ts {
                    return Ok(BillingEventRecord {
                        id: Uuid::now_v7(),
                        provider: BillingProvider::Paddle,
                        external_event_id: event.event_id,
                        event_type: event.event_type,
                        workspace_id: Some(event.workspace_id),
                        occurred_at: event.occurred_at_unix,
                        processed_at: now_secs,
                        outcome: "stale_out_of_order_ignored".to_owned(),
                    });
                }
            }
        }

        let outcome = self.apply_paddle_state_transition(&event, now_secs).await?;

        let record = BillingEventRecord {
            id: Uuid::now_v7(),
            provider: BillingProvider::Paddle,
            external_event_id: event.event_id.clone(),
            event_type: event.event_type,
            workspace_id: Some(event.workspace_id),
            occurred_at: event.occurred_at_unix,
            processed_at: now_secs,
            outcome,
        };
        let mut guard = self.store.write().await;
        guard
            .processed_events
            .insert(("paddle".to_owned(), event.event_id));
        guard
            .last_event_ts_by_workspace
            .insert(event.workspace_id, event.occurred_at_unix);
        guard.events.push(record.clone());
        Ok(record)
    }

    async fn apply_paddle_state_transition(
        &self,
        event: &PaddleWebhookEvent,
        now_secs: i64,
    ) -> Result<String, AuthError> {
        let seats = event.seats.unwrap_or(1);
        match event.event_type.as_str() {
            "subscription.created" | "subscription.activated" | "subscription.renewed" => {
                let tier = product_to_plan(event.product)?;
                self.entitlements
                    .apply_subscription_update(
                        event.workspace_id,
                        event.user_id,
                        tier,
                        SubscriptionState::Active,
                        seats,
                        event.period_end_unix,
                        now_secs,
                    )
                    .await?;
                if let Some(inv_id) = &event.invoice_id {
                    self.record_invoice(
                        event.workspace_id,
                        BillingProvider::Paddle,
                        inv_id,
                        event.amount_minor.unwrap_or(0),
                        BillingCurrency::Usd,
                        "paid",
                        now_secs,
                    )
                    .await;
                }
                Ok("subscription_active".to_owned())
            }
            "transaction.completed" => match event.product {
                CheckoutProduct::CreditPack25 => {
                    self.entitlements
                        .grant_micro_purchase(event.workspace_id, event.user_id, 25, 0)
                        .await;
                    if let Some(inv_id) = &event.invoice_id {
                        self.record_invoice(
                            event.workspace_id,
                            BillingProvider::Paddle,
                            inv_id,
                            event.amount_minor.unwrap_or(500),
                            BillingCurrency::Usd,
                            "paid",
                            now_secs,
                        )
                        .await;
                    }
                    Ok("credits_granted_25".to_owned())
                }
                CheckoutProduct::PayPerFile => {
                    self.entitlements
                        .grant_micro_purchase(event.workspace_id, event.user_id, 0, 1)
                        .await;
                    if let Some(inv_id) = &event.invoice_id {
                        self.record_invoice(
                            event.workspace_id,
                            BillingProvider::Paddle,
                            inv_id,
                            event.amount_minor.unwrap_or(200),
                            BillingCurrency::Usd,
                            "paid",
                            now_secs,
                        )
                        .await;
                    }
                    Ok("pay_per_file_granted".to_owned())
                }
                other => {
                    let tier = product_to_plan(other)?;
                    self.entitlements
                        .apply_subscription_update(
                            event.workspace_id,
                            event.user_id,
                            tier,
                            SubscriptionState::Active,
                            seats,
                            event.period_end_unix,
                            now_secs,
                        )
                        .await?;
                    Ok("transaction_subscription_active".to_owned())
                }
            },
            "subscription.past_due" | "transaction.payment_failed" => {
                let tier = product_to_plan(event.product)?;
                self.entitlements
                    .apply_subscription_update(
                        event.workspace_id,
                        event.user_id,
                        tier,
                        SubscriptionState::PastDue,
                        seats,
                        event.period_end_unix,
                        now_secs,
                    )
                    .await?;
                Ok("subscription_past_due_grace_started".to_owned())
            }
            "subscription.canceled" => {
                let tier = product_to_plan(event.product)?;
                self.entitlements
                    .apply_subscription_update(
                        event.workspace_id,
                        event.user_id,
                        tier,
                        SubscriptionState::Canceled,
                        seats,
                        event.period_end_unix,
                        now_secs,
                    )
                    .await?;
                Ok("subscription_canceled".to_owned())
            }
            "adjustment.refunded" => {
                // Honour Paddle mandatory legal refund webhook or 3-day window.
                if let Some(inv_id) = &event.invoice_id {
                    self.mark_invoice_refunded(inv_id, now_secs, event.mandatory_legal_refund)
                        .await?;
                }
                self.entitlements
                    .apply_subscription_update(
                        event.workspace_id,
                        event.user_id,
                        PlanTier::Free,
                        SubscriptionState::Canceled,
                        1,
                        Some(now_secs),
                        now_secs,
                    )
                    .await?;
                Ok("refund_processed".to_owned())
            }
            "adjustment.chargeback" | "chargeback.created" => {
                if let Some(inv_id) = &event.invoice_id {
                    let _ = self
                        .mark_invoice_chargeback(
                            inv_id,
                            event.workspace_id,
                            BillingProvider::Paddle,
                            BillingCurrency::Usd,
                            event.amount_minor.unwrap_or(0),
                            now_secs,
                        )
                        .await;
                }
                self.entitlements
                    .apply_subscription_update(
                        event.workspace_id,
                        event.user_id,
                        PlanTier::Free,
                        SubscriptionState::Canceled,
                        1,
                        Some(now_secs),
                        now_secs,
                    )
                    .await?;
                Ok("chargeback_revoked".to_owned())
            }
            _ => Ok("unhandled_event_logged".to_owned()),
        }
    }

    /// Process a Paymob webhook callback after constant-time HMAC-SHA512 verification.
    pub async fn process_paymob_webhook(
        &self,
        presented_hmac: &str,
        tx: &PaymobTransactionObj,
        now_secs: i64,
    ) -> Result<BillingEventRecord, AuthError> {
        let secret = self
            .paymob_hmac_secret
            .as_deref()
            .ok_or(AuthError::BillingNotConfigured)?;
        verify_paymob_hmac(secret, presented_hmac, tx)?;

        let ext_id = format!("paymob_tx_{}", tx.id);
        {
            let guard = self.store.read().await;
            if guard
                .processed_events
                .contains(&("paymob".to_owned(), ext_id.clone()))
            {
                return Ok(BillingEventRecord {
                    id: Uuid::now_v7(),
                    provider: BillingProvider::Paymob,
                    external_event_id: ext_id,
                    event_type: "paymob.transaction".to_owned(),
                    workspace_id: Some(tx.workspace_id),
                    occurred_at: tx.occurred_at_unix,
                    processed_at: now_secs,
                    outcome: "idempotent_duplicate_ignored".to_owned(),
                });
            }
        }

        let outcome = if tx.is_chargeback {
            let inv_id = format!("inv_paymob_{}", tx.id);
            let _ = self
                .mark_invoice_chargeback(
                    &inv_id,
                    tx.workspace_id,
                    BillingProvider::Paymob,
                    BillingCurrency::Egp,
                    tx.amount_cents,
                    now_secs,
                )
                .await;
            self.entitlements
                .apply_subscription_update(
                    tx.workspace_id,
                    tx.user_id,
                    PlanTier::Free,
                    SubscriptionState::Canceled,
                    1,
                    Some(now_secs),
                    now_secs,
                )
                .await?;
            "paymob_chargeback_revoked".to_owned()
        } else if tx.is_refunded || tx.is_voided {
            self.entitlements
                .apply_subscription_update(
                    tx.workspace_id,
                    tx.user_id,
                    PlanTier::Free,
                    SubscriptionState::Canceled,
                    1,
                    Some(now_secs),
                    now_secs,
                )
                .await?;
            "paymob_refunded_downgraded".to_owned()
        } else if tx.success && !tx.error_occured && !tx.pending {
            let inv_id = format!("inv_paymob_{}", tx.id);
            self.record_invoice(
                tx.workspace_id,
                BillingProvider::Paymob,
                &inv_id,
                tx.amount_cents,
                BillingCurrency::Egp,
                "paid",
                now_secs,
            )
            .await;
            match tx.product {
                CheckoutProduct::CreditPack25 => {
                    self.entitlements
                        .grant_micro_purchase(tx.workspace_id, tx.user_id, 25, 0)
                        .await;
                    "paymob_credits_granted_25".to_owned()
                }
                CheckoutProduct::PayPerFile => {
                    self.entitlements
                        .grant_micro_purchase(tx.workspace_id, tx.user_id, 0, 1)
                        .await;
                    "paymob_pay_per_file_granted".to_owned()
                }
                other => {
                    let tier = product_to_plan(other)?;
                    let period_secs = match other {
                        CheckoutProduct::IndividualAnnual
                        | CheckoutProduct::OrganizationSeatAnnual => 365 * 24 * 60 * 60,
                        _ => 30 * 24 * 60 * 60,
                    };
                    self.entitlements
                        .apply_subscription_update(
                            tx.workspace_id,
                            tx.user_id,
                            tier,
                            SubscriptionState::Active,
                            tx.seats.unwrap_or(1),
                            Some(now_secs + period_secs),
                            now_secs,
                        )
                        .await?;
                    "paymob_subscription_active".to_owned()
                }
            }
        } else {
            "paymob_payment_failed_or_pending".to_owned()
        };

        let record = BillingEventRecord {
            id: Uuid::now_v7(),
            provider: BillingProvider::Paymob,
            external_event_id: ext_id.clone(),
            event_type: "paymob.transaction".to_owned(),
            workspace_id: Some(tx.workspace_id),
            occurred_at: tx.occurred_at_unix,
            processed_at: now_secs,
            outcome,
        };
        let mut guard = self.store.write().await;
        guard.processed_events.insert(("paymob".to_owned(), ext_id));
        guard.events.push(record.clone());
        Ok(record)
    }

    /// Issue an owner-authorized refund honoring the approved 3-day window
    /// (unless `mandatory_legal_override` is explicitly flagged for statutory Paddle compliance).
    pub async fn issue_owner_refund(
        &self,
        external_invoice_id: &str,
        mandatory_legal_override: bool,
        now_secs: i64,
    ) -> Result<InvoiceRecord, AuthError> {
        let invoice = self
            .mark_invoice_refunded(external_invoice_id, now_secs, mandatory_legal_override)
            .await?;
        self.entitlements
            .apply_subscription_update(
                invoice.workspace_id,
                Uuid::nil(),
                PlanTier::Free,
                SubscriptionState::Canceled,
                1,
                Some(now_secs),
                now_secs,
            )
            .await?;
        Ok(invoice)
    }

    #[allow(clippy::too_many_arguments)]
    async fn record_invoice(
        &self,
        workspace_id: Uuid,
        provider: BillingProvider,
        external_invoice_id: &str,
        amount_minor: u64,
        currency: BillingCurrency,
        status: &str,
        issued_at: i64,
    ) {
        let mut guard = self.store.write().await;
        guard.invoices.insert(
            external_invoice_id.to_owned(),
            InvoiceRecord {
                id: Uuid::now_v7(),
                workspace_id,
                provider,
                external_invoice_id: external_invoice_id.to_owned(),
                amount_minor,
                currency,
                status: status.to_owned(),
                issued_at,
                refunded_at: None,
            },
        );
    }

    async fn mark_invoice_refunded(
        &self,
        external_invoice_id: &str,
        now_secs: i64,
        mandatory_legal_override: bool,
    ) -> Result<InvoiceRecord, AuthError> {
        let mut guard = self.store.write().await;
        let inv = guard
            .invoices
            .get_mut(external_invoice_id)
            .ok_or(AuthError::BillingOperationFailed)?;
        if inv.status == "refunded" {
            return Err(AuthError::BillingOperationFailed);
        }
        if !mandatory_legal_override && now_secs - inv.issued_at > REFUND_WINDOW_SECS {
            return Err(AuthError::BillingOperationFailed);
        }
        inv.status = "refunded".to_owned();
        inv.refunded_at = Some(now_secs);
        Ok(inv.clone())
    }

    /// Return recent billing webhook events for the Owner Control Plane.
    pub async fn list_events(&self, limit: usize) -> Vec<BillingEventRecord> {
        let guard = self.store.read().await;
        guard
            .events
            .iter()
            .rev()
            .take(limit.clamp(1, 200))
            .cloned()
            .collect()
    }

    /// Return invoices for a workspace.
    pub async fn list_invoices(&self, workspace_id: Uuid) -> Vec<InvoiceRecord> {
        self.store
            .read()
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
        let invoice = self
            .mark_invoice_chargeback(
                external_invoice_id,
                workspace_id,
                BillingProvider::Manual,
                BillingCurrency::Usd,
                0,
                now_secs,
            )
            .await;
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
        let mut guard = self.store.write().await;
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

    async fn mark_invoice_chargeback(
        &self,
        external_invoice_id: &str,
        workspace_id: Uuid,
        provider: BillingProvider,
        currency: BillingCurrency,
        amount_minor: u64,
        now_secs: i64,
    ) -> InvoiceRecord {
        let mut guard = self.store.write().await;
        let inv = guard
            .invoices
            .entry(external_invoice_id.to_owned())
            .or_insert_with(|| InvoiceRecord {
                id: Uuid::now_v7(),
                workspace_id,
                provider,
                external_invoice_id: external_invoice_id.to_owned(),
                amount_minor,
                currency,
                status: "chargeback".to_owned(),
                issued_at: now_secs,
                refunded_at: Some(now_secs),
            });
        inv.status = "chargeback".to_owned();
        inv.refunded_at = Some(now_secs);
        inv.clone()
    }

    /// Return all invoices across the platform (for Owner Billing & Revenue Analytics).
    pub async fn list_all_invoices(&self) -> Vec<InvoiceRecord> {
        let mut out: Vec<InvoiceRecord> =
            self.store.read().await.invoices.values().cloned().collect();
        out.sort_by_key(|a| std::cmp::Reverse(a.issued_at));
        out
    }

    /// Compute real revenue & refund/chargeback summary from persisted invoices.
    pub async fn revenue_summary(&self) -> BillingRevenueSummary {
        let guard = self.store.read().await;
        let mut summary = BillingRevenueSummary::default();
        for inv in guard.invoices.values() {
            summary.total_invoices += 1;
            match inv.status.as_str() {
                "paid" => {
                    summary.paid_invoices += 1;
                    match inv.currency {
                        BillingCurrency::Usd => summary.gross_usd_minor += inv.amount_minor,
                        BillingCurrency::Egp => summary.gross_egp_minor += inv.amount_minor,
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
        let guard = self.store.read().await;
        BillingSnapshot {
            processed_events: guard.processed_events.iter().cloned().collect(),
            last_event_ts_by_workspace: guard
                .last_event_ts_by_workspace
                .iter()
                .map(|(k, v)| (*k, *v))
                .collect(),
            events: guard.events.clone(),
            invoices: guard.invoices.values().cloned().collect(),
        }
    }

    /// Restore billing state from a durable snapshot.
    pub async fn import_snapshot(&self, snap: BillingSnapshot) {
        let mut guard = self.store.write().await;
        guard.processed_events = snap.processed_events.into_iter().collect();
        guard.last_event_ts_by_workspace = snap.last_event_ts_by_workspace.into_iter().collect();
        guard.events = snap.events;
        guard.invoices = snap
            .invoices
            .into_iter()
            .map(|i| (i.external_invoice_id.clone(), i))
            .collect();
    }

    /// Current timestamp helper.
    #[must_use]
    pub fn now() -> i64 {
        unix_now()
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

    fn configured_billing() -> (BillingService, EntitlementService) {
        let ent = EntitlementService::new();
        let mut cfg = ServerConfig::for_tests();
        cfg.paddle_api_key = Some("pdl_test_api_key".to_owned());
        cfg.paddle_webhook_secret = Some("pdl_test_whsec_123456".to_owned());
        cfg.paymob_api_key = Some("paymob_test_api_key".to_owned());
        cfg.paymob_hmac_secret = Some("paymob_test_hmac_secret_987654".to_owned());
        (BillingService::from_config(&cfg, ent.clone()), ent)
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
    async fn paddle_and_paymob_full_lifecycle_security_and_idempotency_suite() {
        let (billing, ent) = configured_billing();
        let ws = Uuid::now_v7();
        let user = Uuid::now_v7();
        let now: i64 = 1_795_000_000;

        // 1. Egypt -> Paymob EGP, International -> Paddle USD, Org < 3 seats rejected.
        let eg = billing
            .create_checkout(&CheckoutRequest {
                workspace_id: ws,
                user_id: user,
                product: CheckoutProduct::IndividualMonthly,
                billing_country_iso2: "EG".to_owned(),
                seats: None,
                coupon_code: None,
            })
            .await
            .expect("eg checkout");
        assert_eq!(eg.provider, BillingProvider::Paymob);
        assert_eq!(eg.currency, BillingCurrency::Egp);
        assert_eq!(eg.amount_minor, 100_000);

        let us = billing
            .create_checkout(&CheckoutRequest {
                workspace_id: ws,
                user_id: user,
                product: CheckoutProduct::IndividualAnnual,
                billing_country_iso2: "US".to_owned(),
                seats: None,
                coupon_code: None,
            })
            .await
            .expect("us checkout");
        assert_eq!(us.provider, BillingProvider::Paddle);
        assert_eq!(us.currency, BillingCurrency::Usd);
        assert_eq!(us.amount_minor, 16_800);

        assert_eq!(
            billing
                .create_checkout(&CheckoutRequest {
                    workspace_id: ws,
                    user_id: user,
                    product: CheckoutProduct::OrganizationSeatAnnual,
                    billing_country_iso2: "US".to_owned(),
                    seats: Some(2),
                    coupon_code: None,
                })
                .await
                .unwrap_err(),
            AuthError::OrgMinSeatsRequired
        );

        // 2. Valid Paddle subscription activation + invalid signature + replay window + idempotency.
        let body = serde_json::to_vec(&serde_json::json!({
            "event_id": "evt_sub_001",
            "event_type": "subscription.activated",
            "occurred_at_unix": now,
            "workspace_id": ws,
            "user_id": user,
            "product": "individual_monthly",
            "period_end_unix": now + 30 * 86400,
            "amount_minor": 2000,
            "invoice_id": "inv_pdl_001"
        }))
        .expect("json");
        let sig = sign_paddle_payload("pdl_test_whsec_123456", now, &body).expect("sign");

        // Invalid signature rejected.
        assert_eq!(
            billing
                .process_paddle_webhook("ts=1795000000;h1=deadbeef", &body, now)
                .await
                .unwrap_err(),
            AuthError::InvalidWebhookSignature
        );

        // Stale timestamp outside 5-min replay window rejected.
        let stale_sig =
            sign_paddle_payload("pdl_test_whsec_123456", now - 600, &body).expect("stale sign");
        assert_eq!(
            billing
                .process_paddle_webhook(&stale_sig, &body, now)
                .await
                .unwrap_err(),
            AuthError::WebhookReplayWindow
        );

        // Valid activation succeeds.
        let rec = billing
            .process_paddle_webhook(&sig, &body, now)
            .await
            .expect("valid webhook");
        assert_eq!(rec.outcome, "subscription_active");
        let effective = ent.effective_at(ws, user, false, now).await;
        assert_eq!(effective.effective_plan, PlanTier::IndividualMonthly);
        assert_eq!(effective.ai_calls_limit, 400);

        // Duplicate event delivery is idempotent.
        let dup = billing
            .process_paddle_webhook(&sig, &body, now)
            .await
            .expect("dup");
        assert_eq!(dup.outcome, "idempotent_duplicate_ignored");

        // Out-of-order older event is safely ignored.
        let old_body = serde_json::to_vec(&serde_json::json!({
            "event_id": "evt_sub_old",
            "event_type": "subscription.canceled",
            "occurred_at_unix": now - 10,
            "workspace_id": ws,
            "user_id": user,
            "product": "individual_monthly"
        }))
        .expect("json");
        let old_sig = sign_paddle_payload("pdl_test_whsec_123456", now, &old_body).expect("sign");
        let ooo = billing
            .process_paddle_webhook(&old_sig, &old_body, now)
            .await
            .expect("ooo");
        assert_eq!(ooo.outcome, "stale_out_of_order_ignored");
        assert_eq!(
            ent.effective_at(ws, user, false, now).await.effective_plan,
            PlanTier::IndividualMonthly
        );

        // Failed payment -> PastDue with 3-day warning banner.
        let fail_body = serde_json::to_vec(&serde_json::json!({
            "event_id": "evt_fail_002",
            "event_type": "subscription.past_due",
            "occurred_at_unix": now + 10,
            "workspace_id": ws,
            "user_id": user,
            "product": "individual_monthly"
        }))
        .expect("json");
        let fail_sig =
            sign_paddle_payload("pdl_test_whsec_123456", now + 10, &fail_body).expect("sign");
        billing
            .process_paddle_webhook(&fail_sig, &fail_body, now + 10)
            .await
            .expect("past due");
        assert!(
            ent.effective_at(ws, user, false, now + 20)
                .await
                .show_past_due_banner
        );

        // Refund after 3-day window blocked by default, allowed with statutory override.
        assert_eq!(
            billing
                .issue_owner_refund("inv_pdl_001", false, now + REFUND_WINDOW_SECS + 10)
                .await
                .unwrap_err(),
            AuthError::BillingOperationFailed
        );
        let refunded = billing
            .issue_owner_refund("inv_pdl_001", true, now + REFUND_WINDOW_SECS + 10)
            .await
            .expect("statutory refund");
        assert_eq!(refunded.status, "refunded");
        assert_eq!(
            ent.effective_at(ws, user, false, now + REFUND_WINDOW_SECS + 20)
                .await
                .effective_plan,
            PlanTier::Free
        );

        // Paymob HMAC-SHA512 valid vs tampered check (Credit Pack 25).
        let ws2 = Uuid::now_v7();
        let tx = PaymobTransactionObj {
            id: 991122,
            amount_cents: 25_000,
            created_at: "2026-11-19T12:00:00".to_owned(),
            currency: "EGP".to_owned(),
            error_occured: false,
            has_parent_transaction: false,
            integration_id: 445566,
            is_3d_secure: true,
            is_auth: false,
            is_capture: false,
            is_refunded: false,
            is_chargeback: false,
            is_standalone_payment: true,
            is_voided: false,
            order_id: 778899,
            owner: 12345,
            pending: false,
            source_data_pan: "2346".to_owned(),
            source_data_sub_type: "MasterCard".to_owned(),
            source_data_type: "card".to_owned(),
            success: true,
            workspace_id: ws2,
            user_id: user,
            product: CheckoutProduct::CreditPack25,
            seats: None,
            occurred_at_unix: now + 30,
        };
        assert_eq!(
            billing
                .process_paymob_webhook("0000bad", &tx, now + 30)
                .await
                .unwrap_err(),
            AuthError::InvalidWebhookSignature
        );
        let valid_hmac =
            sign_paymob_transaction("paymob_test_hmac_secret_987654", &tx).expect("hmac");
        let pm = billing
            .process_paymob_webhook(&valid_hmac, &tx, now + 30)
            .await
            .expect("paymob ok");
        assert_eq!(pm.outcome, "paymob_credits_granted_25");
        assert_eq!(
            ent.effective_at(ws2, user, false, now + 31)
                .await
                .operation_credits,
            25
        );
    }
}
