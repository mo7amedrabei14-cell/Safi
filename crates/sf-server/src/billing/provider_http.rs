//! Real Asynchronous HTTP/1.1 + TLS (rustls) Payment Provider Adapters
//! for Paddle Billing API (USD) and Paymob Intention API (EGP).
//!
//! Security & Financial Integrity Guarantees:
//! - External origins MUST use HTTPS (`rustls` + `webpki-roots` root CAs).
//! - Plain `http://` is permitted ONLY for loopback (`127.0.0.1` / `[::1]`) contract/integration tests.
//! - Every outbound provider request enforces a bounded timeout so hung connections cannot stall checkout.
//! - Strict Failure Classification (Section 7.2):
//!   - **DefinitiveFailure**: URL/config error, TCP connect failure or TLS handshake failure *before*
//!     any request bytes are transmitted, or explicit `4xx` (`400`, `401`, `403`, `404`, `422`, `429`)
//!     provider rejection where no transaction/refund was created -> safe to release coupon reservation.
//!   - **UncertainOutcome**: write/flush/read failure after transmission begins, request timeout,
//!     any `5xx` upstream response after possible side effect, or malformed/truncated/oversized
//!     `2xx` response -> order/refund MUST move to `reconciliation_required` (`SF-PAY-008`), coupon
//!     MUST NOT be released, and caller MUST reconcile with the provider before ever retrying!
//! - Zero hardcoded fake customer PII: Paymob Intention API receives validated real customer billing
//!   details (`first_name`, `last_name`, `email`, `phone_number`, `country`) and never stores card data.

use std::sync::Arc;
use std::time::Duration;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, RootCertStore};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use url::Url;
use uuid::Uuid;

use crate::billing::CheckoutProduct;

/// Default bounded timeout for external provider API calls (10 seconds).
pub const DEFAULT_PROVIDER_TIMEOUT_MS: u64 = 10_000;

/// Maximum response payload accepted from a payment provider API (1 MiB).
const MAX_PROVIDER_RESPONSE_BYTES: usize = 1024 * 1024;

/// How the client should complete an initiated checkout session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CheckoutMode {
    /// Redirect the user directly to the provider-returned hosted checkout URL (Paymob Unified Checkout or Paddle ready transaction).
    #[default]
    RedirectUrl,
    /// Open official Paddle.js Checkout overlay using the server-verified draft `transaction_id`
    /// when customer/billing address are collected inside Paddle Checkout.
    PaddleJsOverlay,
}

impl CheckoutMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RedirectUrl => "redirect_url",
            Self::PaddleJsOverlay => "paddle_js_overlay",
        }
    }

    #[must_use]
    pub fn from_str_code(raw: &str) -> Self {
        match raw.trim() {
            "paddle_js_overlay" => Self::PaddleJsOverlay,
            _ => Self::RedirectUrl,
        }
    }
}

/// Verified customer billing details required by Paymob's Intention API (`POST /v1/intention/`).
///
/// Required Paymob fields (documented at developers.paymob.com/paymob-docs/developers/intention-apis/create-intention):
/// - `first_name` / `last_name`: cardholder/customer name for issuer risk screening and receipt generation.
/// - `email`: customer email for payment receipts and fraud signals (defaults to authenticated account email if omitted on checkout).
/// - `phone_number`: customer E.164/local phone required by Egyptian card/wallet/3DS flows.
/// - `country`: 2-letter ISO-3166 country code (e.g. `"EG"`).
///
/// SECURITY: Payment card numbers (PAN), CVV, and expiry dates are NEVER accepted or stored by SafiData.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerBillingData {
    pub first_name: String,
    pub last_name: String,
    pub email: String,
    pub phone_number: String,
    pub country: String,
}

impl CustomerBillingData {
    /// Validate real customer billing fields (rejects blank, malformed email/phone/country).
    pub fn validate(&self) -> Result<(), &'static str> {
        let first = self.first_name.trim();
        let last = self.last_name.trim();
        let email = self.email.trim();
        let phone = self.phone_number.trim();
        let country = self.country.trim();

        if first.is_empty() || first.len() > 80 || last.is_empty() || last.len() > 80 {
            return Err("invalid_billing_name");
        }
        if email.len() < 5
            || email.len() > 160
            || email.contains(char::is_whitespace)
            || !email.contains('@')
            || !email.split('@').nth(1).is_some_and(|dom| dom.contains('.'))
        {
            return Err("invalid_billing_email");
        }
        let digit_count = phone.chars().filter(|c| c.is_ascii_digit()).count();
        let valid_chars = phone
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | ' '));
        if !valid_chars || !(8..=16).contains(&digit_count) {
            return Err("invalid_billing_phone");
        }
        if country.len() != 2 || !country.chars().all(|c| c.is_ascii_alphabetic()) {
            return Err("invalid_billing_country");
        }
        Ok(())
    }
}

/// Optional Paddle customer & billing address identifiers for creating a `ready` transaction directly.
/// When omitted, Paddle creates a `draft` transaction intended for Paddle.js Checkout overlay handoff.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaddleCustomerContext {
    pub customer_id: Option<String>,
    pub address_id: Option<String>,
}

/// Classification of payment provider call errors so checkout/refund can decide safely between
/// definitive failure (release coupon, no charge occurred) vs uncertain outcome (mark `reconciliation_required`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderCallError {
    /// Provider credentials, price ID, or integration ID missing.
    NotConfigured(String),
    /// Provider definitely rejected the request BEFORE creating an order/charge (e.g. pre-send TCP/TLS failure or 4xx).
    DefinitiveFailure { status: u16, reason: String },
    /// Transport interruption, timeout, 5xx after possible side effect, or malformed/truncated response after sending request.
    /// Provider MAY have created the order/charge/refund — MUST reconcile before retrying!
    UncertainOutcome(String),
}

/// Verified result of creating a real Paddle transaction (`POST /transactions`).
#[derive(Debug, Clone)]
pub struct PaddleCreatedTransaction {
    pub transaction_id: String,
    pub checkout_url: Option<String>,
    pub checkout_mode: CheckoutMode,
    pub currency_code: String,
    pub returned_price_ids: Vec<String>,
    pub returned_total_minor: Option<u64>,
    pub status: String,
}

/// Verified result of creating a real Paymob payment intention (`POST /v1/intention/`).
#[derive(Debug, Clone)]
pub struct PaymobCreatedIntention {
    pub intention_id: String,
    pub intention_order_id: u64,
    pub client_secret: String,
    pub checkout_url: String,
}

/// Verified result of submitting or reconciling a real provider refund request.
#[derive(Debug, Clone)]
pub struct ProviderRefundOutcome {
    pub provider_refund_id: String,
    /// `true` ONLY when the provider explicitly confirms the refund is approved/completed.
    /// When `false` (`pending_approval` / `pending`), entitlements MUST NOT be revoked yet!
    pub confirmed: bool,
    pub raw_status: String,
}

#[derive(Debug, Clone)]
struct HttpResponse {
    status: u16,
    body: Vec<u8>,
}

fn build_tls_connector() -> Result<TlsConnector, ProviderCallError> {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config =
        ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(|_| ProviderCallError::DefinitiveFailure {
                status: 0,
                reason: "tls_config_error".to_owned(),
            })?
            .with_root_certificates(roots)
            .with_no_client_auth();
    Ok(TlsConnector::from(Arc::new(config)))
}

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "::1" | "[::1]")
}

fn validate_provider_url(raw_url: &str) -> Result<Url, ProviderCallError> {
    let parsed = Url::parse(raw_url).map_err(|_| ProviderCallError::DefinitiveFailure {
        status: 0,
        reason: "invalid_provider_url".to_owned(),
    })?;
    let host = parsed
        .host_str()
        .ok_or_else(|| ProviderCallError::DefinitiveFailure {
            status: 0,
            reason: "missing_provider_host".to_owned(),
        })?;
    match parsed.scheme() {
        "https" => Ok(parsed),
        "http" if is_loopback_host(host) => Ok(parsed),
        _ => Err(ProviderCallError::DefinitiveFailure {
            status: 0,
            reason: "non_https_external_provider_forbidden".to_owned(),
        }),
    }
}

fn decode_chunked_body(raw: &[u8]) -> Result<Vec<u8>, ProviderCallError> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < raw.len() {
        let Some(rel_end) = raw[pos..].windows(2).position(|w| w == b"\r\n") else {
            return Err(ProviderCallError::UncertainOutcome(
                "truncated_chunked_response".to_owned(),
            ));
        };
        let size_line = std::str::from_utf8(&raw[pos..pos + rel_end]).map_err(|_| {
            ProviderCallError::UncertainOutcome("invalid_chunk_size_utf8".to_owned())
        })?;
        let hex_part = size_line.split(';').next().unwrap_or("").trim();
        let chunk_len = usize::from_str_radix(hex_part, 16).map_err(|_| {
            ProviderCallError::UncertainOutcome("invalid_chunk_size_hex".to_owned())
        })?;
        pos += rel_end + 2;
        if chunk_len == 0 {
            return Ok(out);
        }
        if pos + chunk_len + 2 > raw.len() || out.len() + chunk_len > MAX_PROVIDER_RESPONSE_BYTES {
            return Err(ProviderCallError::UncertainOutcome(
                "incomplete_or_oversized_chunk".to_owned(),
            ));
        }
        out.extend_from_slice(&raw[pos..pos + chunk_len]);
        pos += chunk_len + 2;
    }
    Err(ProviderCallError::UncertainOutcome(
        "unterminated_chunked_response".to_owned(),
    ))
}

fn parse_http_response(raw: &[u8]) -> Result<HttpResponse, ProviderCallError> {
    if raw.len() > MAX_PROVIDER_RESPONSE_BYTES + 8192 {
        return Err(ProviderCallError::UncertainOutcome(
            "oversized_provider_response".to_owned(),
        ));
    }
    let Some(header_end) = raw.windows(4).position(|w| w == b"\r\n\r\n") else {
        return Err(ProviderCallError::UncertainOutcome(
            "malformed_or_truncated_http_headers".to_owned(),
        ));
    };
    let head_str = std::str::from_utf8(&raw[..header_end])
        .map_err(|_| ProviderCallError::UncertainOutcome("invalid_http_header_utf8".to_owned()))?;
    let mut lines = head_str.split("\r\n");
    let status_line = lines.next().unwrap_or("");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| {
            ProviderCallError::UncertainOutcome("invalid_http_status_line".to_owned())
        })?;

    let mut is_chunked = false;
    let mut content_length: Option<usize> = None;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("transfer-encoding")
                && v.to_ascii_lowercase().contains("chunked")
            {
                is_chunked = true;
            } else if k.trim().eq_ignore_ascii_case("content-length") {
                content_length = v.trim().parse::<usize>().ok();
            }
        }
    }

    let raw_body = &raw[header_end + 4..];
    if raw_body.len() > MAX_PROVIDER_RESPONSE_BYTES {
        return Err(ProviderCallError::UncertainOutcome(
            "oversized_provider_body".to_owned(),
        ));
    }
    let body = if is_chunked {
        decode_chunked_body(raw_body)?
    } else {
        if let Some(expected_len) = content_length {
            if raw_body.len() < expected_len {
                return Err(ProviderCallError::UncertainOutcome(
                    "truncated_provider_body".to_owned(),
                ));
            }
        }
        raw_body.to_vec()
    };
    Ok(HttpResponse { status, body })
}

async fn send_json_request(
    method: &str,
    url_str: &str,
    extra_headers: &[(&str, &str)],
    json_body: Option<&Value>,
    timeout_ms: u64,
) -> Result<HttpResponse, ProviderCallError> {
    let url = validate_provider_url(url_str)?;
    let host = url
        .host_str()
        .ok_or_else(|| ProviderCallError::DefinitiveFailure {
            status: 0,
            reason: "missing_host".to_owned(),
        })?
        .to_owned();
    let is_https = url.scheme() == "https";
    let port = url
        .port_or_known_default()
        .unwrap_or(if is_https { 443 } else { 80 });
    let host_header = match url.port() {
        Some(p) => format!("{host}:{p}"),
        None => host.clone(),
    };
    let mut path_and_query = url.path().to_owned();
    if path_and_query.is_empty() {
        path_and_query.push('/');
    }
    if let Some(q) = url.query() {
        path_and_query.push('?');
        path_and_query.push_str(q);
    }

    let body_bytes = match json_body {
        Some(val) => serde_json::to_vec(val).map_err(|_| ProviderCallError::DefinitiveFailure {
            status: 0,
            reason: "json_serialize_error".to_owned(),
        })?,
        None => Vec::new(),
    };

    let mut req_buf = Vec::with_capacity(512 + body_bytes.len());
    req_buf.extend_from_slice(format!("{method} {path_and_query} HTTP/1.1\r\n").as_bytes());
    req_buf.extend_from_slice(format!("Host: {host_header}\r\n").as_bytes());
    req_buf.extend_from_slice(b"User-Agent: SafiData-Server/5.0\r\n");
    req_buf.extend_from_slice(b"Accept: application/json\r\n");
    req_buf.extend_from_slice(b"Connection: close\r\n");
    if json_body.is_some() {
        req_buf.extend_from_slice(b"Content-Type: application/json\r\n");
        req_buf.extend_from_slice(format!("Content-Length: {}\r\n", body_bytes.len()).as_bytes());
    }
    for (k, v) in extra_headers {
        req_buf.extend_from_slice(format!("{k}: {v}\r\n").as_bytes());
    }
    req_buf.extend_from_slice(b"\r\n");
    req_buf.extend_from_slice(&body_bytes);

    let dur = Duration::from_millis(timeout_ms.max(100));
    let connect_dur = dur.min(Duration::from_secs(5));

    // Phase 1: TCP connect (before any request bytes are transmitted -> DefinitiveFailure).
    let tcp = tokio::time::timeout(connect_dur, TcpStream::connect((host.as_str(), port)))
        .await
        .map_err(|_| ProviderCallError::DefinitiveFailure {
            status: 0,
            reason: "tcp_connect_timeout_before_send".to_owned(),
        })?
        .map_err(|e| ProviderCallError::DefinitiveFailure {
            status: 0,
            reason: format!("tcp_connect_failed:{e}"),
        })?;

    // Phase 2: TLS handshake (if HTTPS, before any request bytes are transmitted -> DefinitiveFailure)
    // followed by request transmit + response read (any error/timeout -> UncertainOutcome!).
    let raw_response = if is_https {
        let connector = build_tls_connector()?;
        let server_name = ServerName::try_from(host.clone()).map_err(|_| {
            ProviderCallError::DefinitiveFailure {
                status: 0,
                reason: "invalid_tls_server_name".to_owned(),
            }
        })?;
        let mut tls = tokio::time::timeout(connect_dur, connector.connect(server_name, tcp))
            .await
            .map_err(|_| ProviderCallError::DefinitiveFailure {
                status: 0,
                reason: "tls_handshake_timeout_before_send".to_owned(),
            })?
            .map_err(|e| ProviderCallError::DefinitiveFailure {
                status: 0,
                reason: format!("tls_handshake_failed:{e}"),
            })?;

        tokio::time::timeout(dur, async {
            tls.write_all(&req_buf).await.map_err(|e| {
                ProviderCallError::UncertainOutcome(format!("tls_write_failed:{e}"))
            })?;
            tls.flush().await.map_err(|e| {
                ProviderCallError::UncertainOutcome(format!("tls_flush_failed:{e}"))
            })?;
            let mut out = Vec::new();
            let mut limited = tls.take((MAX_PROVIDER_RESPONSE_BYTES + 8193) as u64);
            limited
                .read_to_end(&mut out)
                .await
                .map_err(|e| ProviderCallError::UncertainOutcome(format!("tls_read_failed:{e}")))?;
            Ok(out)
        })
        .await
        .map_err(|_| ProviderCallError::UncertainOutcome("provider_request_timeout".to_owned()))??
    } else {
        let mut stream = tcp;
        tokio::time::timeout(dur, async {
            stream.write_all(&req_buf).await.map_err(|e| {
                ProviderCallError::UncertainOutcome(format!("tcp_write_failed:{e}"))
            })?;
            stream.flush().await.map_err(|e| {
                ProviderCallError::UncertainOutcome(format!("tcp_flush_failed:{e}"))
            })?;
            let mut out = Vec::new();
            let mut limited = stream.take((MAX_PROVIDER_RESPONSE_BYTES + 8193) as u64);
            limited
                .read_to_end(&mut out)
                .await
                .map_err(|e| ProviderCallError::UncertainOutcome(format!("tcp_read_failed:{e}")))?;
            Ok(out)
        })
        .await
        .map_err(|_| ProviderCallError::UncertainOutcome("provider_request_timeout".to_owned()))??
    };

    parse_http_response(&raw_response)
}

fn classify_non_2xx_provider_error(provider: &str, op: &str, status: u16) -> ProviderCallError {
    if status >= 500 {
        // 5xx responses may occur AFTER the provider persisted the transaction or refund!
        ProviderCallError::UncertainOutcome(format!("{provider}_{op}_upstream_5xx_{status}"))
    } else {
        // 4xx (including 400, 401, 403, 404, 422, 429) means the provider definitively rejected the request.
        ProviderCallError::DefinitiveFailure {
            status,
            reason: format!("{provider}_{op}_status_{status}"),
        }
    }
}

/// Real Paddle Billing API HTTP adapter (`POST /transactions`, `GET /transactions`, `POST /adjustments`, `GET /adjustments`).
#[derive(Debug, Clone)]
pub struct PaddleApiAdapter {
    pub api_base_url: String,
    pub api_key: String,
    pub timeout_ms: u64,
}

impl PaddleApiAdapter {
    /// Call Paddle's official `POST /transactions` endpoint.
    ///
    /// Supports BOTH official Paddle Checkout flows:
    /// 1. When `customer.customer_id` + `customer.address_id` are supplied -> Paddle returns a `ready`
    ///    transaction with a hosted `checkout.url` (`CheckoutMode::RedirectUrl`).
    /// 2. When customer/address are collected interactively by Paddle Checkout -> Paddle returns a
    ///    `draft` transaction where `checkout.url` may be `null` (`CheckoutMode::PaddleJsOverlay`),
    ///    and the client opens Paddle.js Checkout with the server-verified `transaction_id`.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_transaction(
        &self,
        order_id: Uuid,
        workspace_id: Uuid,
        user_id: Uuid,
        product: CheckoutProduct,
        price_id: &str,
        quantity: u32,
        expected_amount_minor: u64,
        idempotency_key: &str,
        customer: Option<&PaddleCustomerContext>,
    ) -> Result<PaddleCreatedTransaction, ProviderCallError> {
        let base = self.api_base_url.trim_end_matches('/');
        let url = format!("{base}/transactions");
        let auth = format!("Bearer {}", self.api_key);
        let mut payload = json!({
            "items": [{
                "price_id": price_id,
                "quantity": quantity.max(1)
            }],
            "currency_code": "USD",
            "collection_mode": "automatic",
            "custom_data": {
                "safi_order_id": order_id.to_string(),
                "workspace_id": workspace_id.to_string(),
                "user_id": user_id.to_string(),
                "product": product.as_str(),
                "idempotency_key": idempotency_key,
                "expected_amount_minor": expected_amount_minor
            }
        });
        if let Some(ctx) = customer {
            if let Some(ref cid) = ctx.customer_id {
                if !cid.trim().is_empty() {
                    payload["customer_id"] = Value::String(cid.trim().to_owned());
                }
            }
            if let Some(ref aid) = ctx.address_id {
                if !aid.trim().is_empty() {
                    payload["address_id"] = Value::String(aid.trim().to_owned());
                }
            }
        }

        let resp = send_json_request(
            "POST",
            &url,
            &[("Authorization", auth.as_str())],
            Some(&payload),
            self.timeout_ms,
        )
        .await?;

        if resp.status < 200 || resp.status >= 300 {
            return Err(classify_non_2xx_provider_error(
                "paddle",
                "create_transaction",
                resp.status,
            ));
        }

        let val: Value = serde_json::from_slice(&resp.body)
            .map_err(|_| ProviderCallError::UncertainOutcome("paddle_invalid_json".to_owned()))?;
        let created = parse_paddle_transaction_data(&val, resp.status)?;
        validate_paddle_created_transaction(&created, price_id, expected_amount_minor)?;
        Ok(created)
    }

    /// Reconcile an uncertain/timed-out checkout order by querying Paddle's official
    /// `GET /transactions?status=draft,ready,billed,completed&per_page=50` endpoint
    /// (filtering strictly by `custom_data.safi_order_id`) without creating a second transaction.
    pub async fn reconcile_transaction_by_order_id(
        &self,
        order_id: Uuid,
        expected_price_id: &str,
        expected_amount_minor: u64,
    ) -> Result<Option<PaddleCreatedTransaction>, ProviderCallError> {
        let base = self.api_base_url.trim_end_matches('/');
        let url = format!("{base}/transactions?status=draft,ready,billed,completed&per_page=50");
        let auth = format!("Bearer {}", self.api_key);
        let resp = send_json_request(
            "GET",
            &url,
            &[("Authorization", auth.as_str())],
            None,
            self.timeout_ms,
        )
        .await?;

        if resp.status < 200 || resp.status >= 300 {
            return Err(ProviderCallError::UncertainOutcome(format!(
                "paddle_reconcile_status_{}",
                resp.status
            )));
        }
        let val: Value = serde_json::from_slice(&resp.body).map_err(|_| {
            ProviderCallError::UncertainOutcome("paddle_reconcile_invalid_json".to_owned())
        })?;

        let order_str = order_id.to_string();
        if let Some(arr) = val.get("data").and_then(Value::as_array) {
            for item in arr {
                let matches_order = item
                    .get("custom_data")
                    .and_then(|c| c.get("safi_order_id"))
                    .and_then(Value::as_str)
                    == Some(order_str.as_str());
                if matches_order {
                    let wrapped = json!({ "data": item });
                    let found = parse_paddle_transaction_data(&wrapped, resp.status)?;
                    validate_paddle_created_transaction(
                        &found,
                        expected_price_id,
                        expected_amount_minor,
                    )?;
                    return Ok(Some(found));
                }
            }
            return Ok(None);
        }
        if val.get("data").is_some_and(Value::is_object) {
            let found = parse_paddle_transaction_data(&val, resp.status)?;
            validate_paddle_created_transaction(&found, expected_price_id, expected_amount_minor)?;
            return Ok(Some(found));
        }
        Ok(None)
    }

    /// Submit a real Paddle refund adjustment (`POST /adjustments`).
    pub async fn create_refund_adjustment(
        &self,
        transaction_id: &str,
        refund_request_key: &str,
        reason: &str,
        partial_amount_minor: Option<u64>,
    ) -> Result<ProviderRefundOutcome, ProviderCallError> {
        let base = self.api_base_url.trim_end_matches('/');
        let url = format!("{base}/adjustments");
        let auth = format!("Bearer {}", self.api_key);
        let mut payload = json!({
            "action": "refund",
            "transaction_id": transaction_id,
            "reason": if reason.trim().is_empty() { "owner_authorized_refund" } else { reason },
            "type": if partial_amount_minor.is_some() { "partial" } else { "full" }
        });
        if let Some(amt) = partial_amount_minor {
            payload["amount"] = Value::String(amt.to_string());
        }

        let resp = send_json_request(
            "POST",
            &url,
            &[
                ("Authorization", auth.as_str()),
                ("Idempotency-Key", refund_request_key),
            ],
            Some(&payload),
            self.timeout_ms,
        )
        .await?;

        if resp.status < 200 || resp.status >= 300 {
            return Err(classify_non_2xx_provider_error(
                "paddle",
                "create_refund",
                resp.status,
            ));
        }
        let val: Value = serde_json::from_slice(&resp.body).map_err(|_| {
            ProviderCallError::UncertainOutcome("paddle_refund_invalid_json".to_owned())
        })?;
        parse_paddle_adjustment_response(&val)
    }

    /// Reconcile an uncertain Paddle refund via `GET /adjustments?transaction_id=<tx_id>`.
    pub async fn reconcile_refund_adjustment(
        &self,
        transaction_id: &str,
    ) -> Result<Option<ProviderRefundOutcome>, ProviderCallError> {
        let base = self.api_base_url.trim_end_matches('/');
        let url = format!("{base}/adjustments?transaction_id={transaction_id}&action=refund");
        let auth = format!("Bearer {}", self.api_key);
        let resp = send_json_request(
            "GET",
            &url,
            &[("Authorization", auth.as_str())],
            None,
            self.timeout_ms,
        )
        .await?;

        if resp.status < 200 || resp.status >= 300 {
            return Err(ProviderCallError::UncertainOutcome(format!(
                "paddle_reconcile_refund_status_{}",
                resp.status
            )));
        }
        let val: Value = serde_json::from_slice(&resp.body).map_err(|_| {
            ProviderCallError::UncertainOutcome("paddle_reconcile_refund_invalid_json".to_owned())
        })?;
        if let Some(arr) = val.get("data").and_then(Value::as_array) {
            if let Some(first) = arr.first() {
                let wrapped = json!({ "data": first });
                return Ok(Some(parse_paddle_adjustment_response(&wrapped)?));
            }
            return Ok(None);
        }
        Ok(None)
    }
}

fn parse_paddle_adjustment_response(
    val: &Value,
) -> Result<ProviderRefundOutcome, ProviderCallError> {
    let data = val.get("data").unwrap_or(val);
    let adj_id = data
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ProviderCallError::UncertainOutcome("paddle_refund_missing_id".to_owned()))?
        .to_owned();
    let raw_status = data
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("pending_approval")
        .to_owned();
    let confirmed = matches!(raw_status.as_str(), "approved" | "completed" | "refunded");
    Ok(ProviderRefundOutcome {
        provider_refund_id: adj_id,
        confirmed,
        raw_status,
    })
}

fn parse_paddle_transaction_data(
    val: &Value,
    status: u16,
) -> Result<PaddleCreatedTransaction, ProviderCallError> {
    let data = val.get("data").unwrap_or(val);
    let transaction_id = data
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            ProviderCallError::UncertainOutcome("paddle_missing_transaction_id".to_owned())
        })?
        .to_owned();

    // NEVER assume missing currency is USD — strictly require and validate returned currency_code!
    let currency_code = data
        .get("currency_code")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ProviderCallError::DefinitiveFailure {
            status,
            reason: "paddle_missing_currency_code".to_owned(),
        })?
        .to_ascii_uppercase();
    if currency_code != "USD" {
        return Err(ProviderCallError::DefinitiveFailure {
            status,
            reason: format!("paddle_currency_mismatch:{currency_code}"),
        });
    }

    // Strictly require and validate returned transaction status (`draft` or `ready` for checkout; `billed`/`completed` on reconciliation).
    let tx_status = data
        .get("status")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ProviderCallError::DefinitiveFailure {
            status,
            reason: "paddle_missing_status".to_owned(),
        })?
        .to_ascii_lowercase();
    if !matches!(
        tx_status.as_str(),
        "draft" | "ready" | "billed" | "completed"
    ) {
        return Err(ProviderCallError::DefinitiveFailure {
            status,
            reason: format!("paddle_unexpected_status:{tx_status}"),
        });
    }

    let checkout_url = data
        .get("checkout")
        .and_then(|c| c.get("url"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned);

    // Official Paddle behavior:
    // - `ready` transaction with checkout URL -> RedirectUrl mode.
    // - `draft` transaction (or `ready` without hosted URL) -> PaddleJsOverlay mode using `transaction_id`!
    let checkout_mode = if checkout_url.is_some() {
        CheckoutMode::RedirectUrl
    } else if tx_status == "draft" {
        CheckoutMode::PaddleJsOverlay
    } else {
        return Err(ProviderCallError::DefinitiveFailure {
            status,
            reason: "paddle_ready_missing_checkout_url".to_owned(),
        });
    };

    let mut returned_price_ids = Vec::new();
    if let Some(items) = data.get("items").and_then(Value::as_array) {
        for item in items {
            if let Some(pid) = item
                .get("price")
                .and_then(|p| p.get("id"))
                .and_then(Value::as_str)
                .or_else(|| item.get("price_id").and_then(Value::as_str))
            {
                returned_price_ids.push(pid.to_owned());
            }
        }
    }

    let returned_total_minor = data
        .get("details")
        .and_then(|d| d.get("totals"))
        .and_then(|t| t.get("total"))
        .and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
        });

    Ok(PaddleCreatedTransaction {
        transaction_id,
        checkout_url,
        checkout_mode,
        currency_code,
        returned_price_ids,
        returned_total_minor,
        status: tx_status,
    })
}

fn validate_paddle_created_transaction(
    created: &PaddleCreatedTransaction,
    expected_price_id: &str,
    expected_amount_minor: u64,
) -> Result<(), ProviderCallError> {
    if !created.returned_price_ids.is_empty()
        && !created
            .returned_price_ids
            .iter()
            .any(|p| p == expected_price_id)
    {
        return Err(ProviderCallError::DefinitiveFailure {
            status: 422,
            reason: format!("paddle_price_id_mismatch:expected={expected_price_id}"),
        });
    }
    if let Some(ret_total) = created.returned_total_minor {
        if ret_total != expected_amount_minor {
            return Err(ProviderCallError::DefinitiveFailure {
                status: 422,
                reason: format!(
                    "paddle_amount_mismatch:expected={expected_amount_minor},got={ret_total}"
                ),
            });
        }
    }
    Ok(())
}

/// Real Paymob Intention API HTTP adapter (`POST /v1/intention/` & `POST /api/acceptance/void_refund/refund`).
#[derive(Debug, Clone)]
pub struct PaymobApiAdapter {
    pub api_base_url: String,
    pub secret_key: String,
    pub public_key: String,
    pub integration_id: u64,
    pub timeout_ms: u64,
}

impl PaymobApiAdapter {
    /// Call Paymob's official `POST /v1/intention/` API using real, validated customer billing details
    /// (NEVER hardcoded fake personal data) and bind the returned `intention_order_id` and `client_secret`
    /// to SafiData's internal order.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_intention(
        &self,
        order_id: Uuid,
        product: CheckoutProduct,
        seats: u32,
        expected_amount_minor: u64,
        billing_data: &CustomerBillingData,
        notification_url: Option<&str>,
        redirection_url: Option<&str>,
    ) -> Result<PaymobCreatedIntention, ProviderCallError> {
        billing_data
            .validate()
            .map_err(|reason| ProviderCallError::DefinitiveFailure {
                status: 422,
                reason: reason.to_owned(),
            })?;

        let base = self.api_base_url.trim_end_matches('/');
        let url = format!("{base}/v1/intention/");
        let auth = format!("Token {}", self.secret_key);
        let mut payload = json!({
            "amount": expected_amount_minor,
            "currency": "EGP",
            "payment_methods": [self.integration_id],
            "special_reference": order_id.to_string(),
            "items": [{
                "name": product.as_str(),
                "amount": expected_amount_minor,
                "description": format!("SafiData {} (seats={})", product.as_str(), seats.max(1)),
                "quantity": 1
            }],
            "billing_data": {
                "first_name": billing_data.first_name.trim(),
                "last_name": billing_data.last_name.trim(),
                "email": billing_data.email.trim(),
                "phone_number": billing_data.phone_number.trim(),
                "country": billing_data.country.trim().to_ascii_uppercase()
            },
            "extras": {
                "safi_order_id": order_id.to_string()
            }
        });
        if let Some(nurl) = notification_url.filter(|s| !s.trim().is_empty()) {
            payload["notification_url"] = Value::String(nurl.trim().to_owned());
        }
        if let Some(rurl) = redirection_url.filter(|s| !s.trim().is_empty()) {
            payload["redirection_url"] = Value::String(rurl.trim().to_owned());
        }

        let resp = send_json_request(
            "POST",
            &url,
            &[("Authorization", auth.as_str())],
            Some(&payload),
            self.timeout_ms,
        )
        .await?;

        if resp.status < 200 || resp.status >= 300 {
            return Err(classify_non_2xx_provider_error(
                "paymob",
                "create_intention",
                resp.status,
            ));
        }

        let val: Value = serde_json::from_slice(&resp.body)
            .map_err(|_| ProviderCallError::UncertainOutcome("paymob_invalid_json".to_owned()))?;
        parse_paymob_intention_data(&val, &self.api_base_url, &self.public_key)
    }

    /// Reconcile an uncertain Paymob intention by querying `GET /v1/intention/?special_reference=<order_id>`.
    pub async fn reconcile_intention_by_order_id(
        &self,
        order_id: Uuid,
    ) -> Result<Option<PaymobCreatedIntention>, ProviderCallError> {
        let base = self.api_base_url.trim_end_matches('/');
        let url = format!("{base}/v1/intention/?special_reference={order_id}");
        let auth = format!("Token {}", self.secret_key);
        let resp = send_json_request(
            "GET",
            &url,
            &[("Authorization", auth.as_str())],
            None,
            self.timeout_ms,
        )
        .await?;

        if resp.status == 404 {
            return Ok(None);
        }
        if resp.status < 200 || resp.status >= 300 {
            return Err(ProviderCallError::UncertainOutcome(format!(
                "paymob_reconcile_status_{}",
                resp.status
            )));
        }
        let val: Value = serde_json::from_slice(&resp.body).map_err(|_| {
            ProviderCallError::UncertainOutcome("paymob_reconcile_invalid_json".to_owned())
        })?;
        if val.get("client_secret").is_none() && val.get("intention_order_id").is_none() {
            return Ok(None);
        }
        Ok(Some(parse_paymob_intention_data(
            &val,
            &self.api_base_url,
            &self.public_key,
        )?))
    }

    /// Submit a real Paymob refund (`POST /api/acceptance/void_refund/refund`).
    pub async fn refund_transaction(
        &self,
        transaction_id: u64,
        amount_cents: u64,
    ) -> Result<ProviderRefundOutcome, ProviderCallError> {
        let base = self.api_base_url.trim_end_matches('/');
        let url = format!("{base}/api/acceptance/void_refund/refund");
        let auth = format!("Token {}", self.secret_key);
        let payload = json!({
            "transaction_id": transaction_id,
            "amount_cents": amount_cents
        });
        let resp = send_json_request(
            "POST",
            &url,
            &[("Authorization", auth.as_str())],
            Some(&payload),
            self.timeout_ms,
        )
        .await?;

        if resp.status < 200 || resp.status >= 300 {
            return Err(classify_non_2xx_provider_error(
                "paymob",
                "refund",
                resp.status,
            ));
        }
        let val: Value = serde_json::from_slice(&resp.body).map_err(|_| {
            ProviderCallError::UncertainOutcome("paymob_refund_invalid_json".to_owned())
        })?;
        let refund_id = val
            .get("id")
            .and_then(|v| {
                v.as_u64()
                    .map(|n| n.to_string())
                    .or_else(|| v.as_str().map(str::to_owned))
            })
            .ok_or_else(|| {
                ProviderCallError::UncertainOutcome("paymob_refund_missing_id".to_owned())
            })?;
        let success = val.get("success").and_then(Value::as_bool).unwrap_or(false);
        let is_refunded = val
            .get("is_refunded")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let pending = val.get("pending").and_then(Value::as_bool).unwrap_or(false);
        let confirmed = (success || is_refunded) && !pending;
        let raw_status = if confirmed {
            "refunded".to_owned()
        } else {
            "pending".to_owned()
        };
        Ok(ProviderRefundOutcome {
            provider_refund_id: refund_id,
            confirmed,
            raw_status,
        })
    }
}

fn parse_paymob_intention_data(
    val: &Value,
    api_base_url: &str,
    public_key: &str,
) -> Result<PaymobCreatedIntention, ProviderCallError> {
    let intention_id = val
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            ProviderCallError::UncertainOutcome("paymob_missing_intention_id".to_owned())
        })?
        .to_owned();
    let intention_order_id = val
        .get("intention_order_id")
        .and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
        })
        .ok_or_else(|| {
            ProviderCallError::UncertainOutcome("paymob_missing_intention_order_id".to_owned())
        })?;
    let client_secret = val
        .get("client_secret")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            ProviderCallError::UncertainOutcome("paymob_missing_client_secret".to_owned())
        })?
        .to_owned();

    // Official Paymob Unified Checkout URL using the actual returned `client_secret` and configured `publicKey`.
    let checkout_origin = if api_base_url.starts_with("http://127.0.0.1")
        || api_base_url.starts_with("http://[::1]")
    {
        api_base_url.trim_end_matches('/')
    } else {
        "https://accept.paymob.com"
    };
    let checkout_url = format!(
        "{checkout_origin}/unifiedcheckout/?publicKey={public_key}&clientSecret={client_secret}"
    );

    Ok(PaymobCreatedIntention {
        intention_id,
        intention_order_id,
        client_secret,
        checkout_url,
    })
}
