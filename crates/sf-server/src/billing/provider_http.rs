//! Real Asynchronous HTTP/1.1 + TLS (rustls) Payment Provider Adapters
//! for Paddle Billing API (USD) and Paymob Intention API (EGP).
//!
//! Security & Financial Integrity Guarantees:
//! - External origins MUST use HTTPS (`rustls` + `webpki-roots` root CAs).
//! - Plain `http://` is permitted ONLY for loopback (`127.0.0.1` / `[::1]`) contract/integration tests.
//! - Every outbound provider request enforces a bounded timeout so hung connections cannot stall checkout.
//! - Distinguishes **definitive failure** (provider returned 4xx/5xx or invalid payload — safe to release
//!   coupon reservation and mark order failed) from **uncertain timeout / transport interruption**
//!   (order MUST move to `reconciliation_required` and query provider state before any retry!).

use std::sync::Arc;
use std::time::Duration;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, RootCertStore};
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

/// Classification of payment provider call errors so checkout can decide safely between
/// definitive failure (release coupon) vs uncertain timeout (mark `reconciliation_required`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderCallError {
    /// Provider credentials, price ID, or integration ID missing.
    NotConfigured(String),
    /// Provider definitely rejected the request (4xx/5xx or invalid schema).
    DefinitiveFailure { status: u16, reason: String },
    /// Network timeout or broken connection after request transmission; provider MAY have created the order!
    UncertainTimeout(String),
}

/// Verified result of creating a real Paddle transaction (`POST /transactions`).
#[derive(Debug, Clone)]
pub struct PaddleCreatedTransaction {
    pub transaction_id: String,
    pub checkout_url: String,
    pub currency_code: String,
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

/// Verified result of submitting a real provider refund request.
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
            return Err(ProviderCallError::UncertainTimeout(
                "truncated_chunked_response".to_owned(),
            ));
        };
        let size_line = std::str::from_utf8(&raw[pos..pos + rel_end]).map_err(|_| {
            ProviderCallError::DefinitiveFailure {
                status: 0,
                reason: "invalid_chunk_size_utf8".to_owned(),
            }
        })?;
        let hex_part = size_line.split(';').next().unwrap_or("").trim();
        let chunk_len = usize::from_str_radix(hex_part, 16).map_err(|_| {
            ProviderCallError::DefinitiveFailure {
                status: 0,
                reason: "invalid_chunk_size_hex".to_owned(),
            }
        })?;
        pos += rel_end + 2;
        if chunk_len == 0 {
            break;
        }
        if pos + chunk_len + 2 > raw.len() || out.len() + chunk_len > MAX_PROVIDER_RESPONSE_BYTES {
            return Err(ProviderCallError::UncertainTimeout(
                "incomplete_or_oversized_chunk".to_owned(),
            ));
        }
        out.extend_from_slice(&raw[pos..pos + chunk_len]);
        pos += chunk_len + 2;
    }
    Ok(out)
}

fn parse_http_response(raw: &[u8]) -> Result<HttpResponse, ProviderCallError> {
    let Some(header_end) = raw.windows(4).position(|w| w == b"\r\n\r\n") else {
        return Err(ProviderCallError::UncertainTimeout(
            "malformed_or_truncated_http_headers".to_owned(),
        ));
    };
    let head_str = std::str::from_utf8(&raw[..header_end]).map_err(|_| {
        ProviderCallError::DefinitiveFailure {
            status: 0,
            reason: "invalid_http_header_utf8".to_owned(),
        }
    })?;
    let mut lines = head_str.split("\r\n");
    let status_line = lines.next().unwrap_or("");
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| ProviderCallError::DefinitiveFailure {
            status: 0,
            reason: "invalid_http_status_line".to_owned(),
        })?;

    let mut is_chunked = false;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            if k.trim().eq_ignore_ascii_case("transfer-encoding")
                && v.to_ascii_lowercase().contains("chunked")
            {
                is_chunked = true;
            }
        }
    }

    let raw_body = &raw[header_end + 4..];
    let body = if is_chunked {
        decode_chunked_body(raw_body)?
    } else {
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
    let raw_response = tokio::time::timeout(dur, async {
        let tcp = TcpStream::connect((host.as_str(), port))
            .await
            .map_err(|e| ProviderCallError::DefinitiveFailure {
                status: 0,
                reason: format!("tcp_connect_failed:{e}"),
            })?;

        if is_https {
            let connector = build_tls_connector()?;
            let server_name = ServerName::try_from(host.clone()).map_err(|_| {
                ProviderCallError::DefinitiveFailure {
                    status: 0,
                    reason: "invalid_tls_server_name".to_owned(),
                }
            })?;
            let mut tls = connector.connect(server_name, tcp).await.map_err(|e| {
                ProviderCallError::DefinitiveFailure {
                    status: 0,
                    reason: format!("tls_handshake_failed:{e}"),
                }
            })?;
            tls.write_all(&req_buf).await.map_err(|e| {
                ProviderCallError::UncertainTimeout(format!("tls_write_failed:{e}"))
            })?;
            tls.flush().await.map_err(|e| {
                ProviderCallError::UncertainTimeout(format!("tls_flush_failed:{e}"))
            })?;
            let mut out = Vec::new();
            let mut limited = tls.take((MAX_PROVIDER_RESPONSE_BYTES + 8192) as u64);
            limited
                .read_to_end(&mut out)
                .await
                .map_err(|e| ProviderCallError::UncertainTimeout(format!("tls_read_failed:{e}")))?;
            Ok(out)
        } else {
            let mut stream = tcp;
            stream.write_all(&req_buf).await.map_err(|e| {
                ProviderCallError::UncertainTimeout(format!("tcp_write_failed:{e}"))
            })?;
            stream.flush().await.map_err(|e| {
                ProviderCallError::UncertainTimeout(format!("tcp_flush_failed:{e}"))
            })?;
            let mut out = Vec::new();
            let mut limited = stream.take((MAX_PROVIDER_RESPONSE_BYTES + 8192) as u64);
            limited
                .read_to_end(&mut out)
                .await
                .map_err(|e| ProviderCallError::UncertainTimeout(format!("tcp_read_failed:{e}")))?;
            Ok(out)
        }
    })
    .await
    .map_err(|_| ProviderCallError::UncertainTimeout("provider_request_timeout".to_owned()))??;

    parse_http_response(&raw_response)
}

/// Real Paddle Billing API HTTP adapter (`POST /transactions`, `GET /transactions`, `POST /adjustments`).
#[derive(Debug, Clone)]
pub struct PaddleApiAdapter {
    pub api_base_url: String,
    pub api_key: String,
    pub timeout_ms: u64,
}

impl PaddleApiAdapter {
    /// Call Paddle's official `POST /transactions` endpoint to create a real transaction & checkout URL.
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
    ) -> Result<PaddleCreatedTransaction, ProviderCallError> {
        let base = self.api_base_url.trim_end_matches('/');
        let url = format!("{base}/transactions");
        let auth = format!("Bearer {}", self.api_key);
        let payload = json!({
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

        let resp = send_json_request(
            "POST",
            &url,
            &[("Authorization", auth.as_str())],
            Some(&payload),
            self.timeout_ms,
        )
        .await?;

        if resp.status < 200 || resp.status >= 300 {
            if resp.status == 502 || resp.status == 503 || resp.status == 504 {
                return Err(ProviderCallError::UncertainTimeout(format!(
                    "paddle_upstream_status_{}",
                    resp.status
                )));
            }
            return Err(ProviderCallError::DefinitiveFailure {
                status: resp.status,
                reason: format!("paddle_create_transaction_status_{}", resp.status),
            });
        }

        let val: Value = serde_json::from_slice(&resp.body).map_err(|_| {
            ProviderCallError::DefinitiveFailure {
                status: resp.status,
                reason: "paddle_invalid_json".to_owned(),
            }
        })?;
        parse_paddle_transaction_data(&val, resp.status)
    }

    /// Reconcile an uncertain/timed-out checkout order by querying Paddle `GET /transactions`
    /// without creating a second transaction.
    pub async fn reconcile_transaction_by_order_id(
        &self,
        order_id: Uuid,
    ) -> Result<Option<PaddleCreatedTransaction>, ProviderCallError> {
        let base = self.api_base_url.trim_end_matches('/');
        let url = format!("{base}/transactions?order_id={order_id}");
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
            return Err(ProviderCallError::UncertainTimeout(format!(
                "paddle_reconcile_status_{}",
                resp.status
            )));
        }
        let val: Value = serde_json::from_slice(&resp.body).map_err(|_| {
            ProviderCallError::UncertainTimeout("paddle_reconcile_invalid_json".to_owned())
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
                    return Ok(Some(parse_paddle_transaction_data(&wrapped, resp.status)?));
                }
            }
            return Ok(None);
        }
        if val.get("data").is_some_and(Value::is_object) {
            return Ok(Some(parse_paddle_transaction_data(&val, resp.status)?));
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
            return Err(ProviderCallError::DefinitiveFailure {
                status: resp.status,
                reason: format!("paddle_refund_status_{}", resp.status),
            });
        }
        let val: Value = serde_json::from_slice(&resp.body).map_err(|_| {
            ProviderCallError::DefinitiveFailure {
                status: resp.status,
                reason: "paddle_refund_invalid_json".to_owned(),
            }
        })?;
        let data = val.get("data").unwrap_or(&val);
        let adj_id = data
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| ProviderCallError::DefinitiveFailure {
                status: resp.status,
                reason: "paddle_refund_missing_id".to_owned(),
            })?
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
        .ok_or_else(|| ProviderCallError::DefinitiveFailure {
            status,
            reason: "paddle_missing_transaction_id".to_owned(),
        })?
        .to_owned();
    let checkout_url = data
        .get("checkout")
        .and_then(|c| c.get("url"))
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ProviderCallError::DefinitiveFailure {
            status,
            reason: "paddle_missing_checkout_url".to_owned(),
        })?
        .to_owned();
    let currency_code = data
        .get("currency_code")
        .and_then(Value::as_str)
        .unwrap_or("USD")
        .to_ascii_uppercase();
    if currency_code != "USD" {
        return Err(ProviderCallError::DefinitiveFailure {
            status,
            reason: format!("paddle_currency_mismatch:{currency_code}"),
        });
    }
    let returned_total_minor = data
        .get("details")
        .and_then(|d| d.get("totals"))
        .and_then(|t| t.get("total"))
        .and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
        });
    let tx_status = data
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("ready")
        .to_owned();

    Ok(PaddleCreatedTransaction {
        transaction_id,
        checkout_url,
        currency_code,
        returned_total_minor,
        status: tx_status,
    })
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
    /// Call Paymob's official `POST /v1/intention/` API and bind the returned `intention_order_id`
    /// and `client_secret` to SafiData's internal order.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_intention(
        &self,
        order_id: Uuid,
        product: CheckoutProduct,
        seats: u32,
        expected_amount_minor: u64,
    ) -> Result<PaymobCreatedIntention, ProviderCallError> {
        let base = self.api_base_url.trim_end_matches('/');
        let url = format!("{base}/v1/intention/");
        let auth = format!("Token {}", self.secret_key);
        let payload = json!({
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
                "first_name": "SafiData",
                "last_name": "Customer",
                "email": "billing@safidata.app",
                "phone_number": "+201000000000",
                "country": "EG"
            },
            "extras": {
                "safi_order_id": order_id.to_string()
            }
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
            if resp.status == 502 || resp.status == 503 || resp.status == 504 {
                return Err(ProviderCallError::UncertainTimeout(format!(
                    "paymob_upstream_status_{}",
                    resp.status
                )));
            }
            return Err(ProviderCallError::DefinitiveFailure {
                status: resp.status,
                reason: format!("paymob_create_intention_status_{}", resp.status),
            });
        }

        let val: Value = serde_json::from_slice(&resp.body).map_err(|_| {
            ProviderCallError::DefinitiveFailure {
                status: resp.status,
                reason: "paymob_invalid_json".to_owned(),
            }
        })?;
        parse_paymob_intention_data(&val, &self.api_base_url, &self.public_key, resp.status)
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
            return Err(ProviderCallError::UncertainTimeout(format!(
                "paymob_reconcile_status_{}",
                resp.status
            )));
        }
        let val: Value = serde_json::from_slice(&resp.body).map_err(|_| {
            ProviderCallError::UncertainTimeout("paymob_reconcile_invalid_json".to_owned())
        })?;
        if val.get("client_secret").is_none() && val.get("intention_order_id").is_none() {
            return Ok(None);
        }
        Ok(Some(parse_paymob_intention_data(
            &val,
            &self.api_base_url,
            &self.public_key,
            resp.status,
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
            return Err(ProviderCallError::DefinitiveFailure {
                status: resp.status,
                reason: format!("paymob_refund_status_{}", resp.status),
            });
        }
        let val: Value = serde_json::from_slice(&resp.body).map_err(|_| {
            ProviderCallError::DefinitiveFailure {
                status: resp.status,
                reason: "paymob_refund_invalid_json".to_owned(),
            }
        })?;
        let refund_id = val
            .get("id")
            .and_then(|v| {
                v.as_u64()
                    .map(|n| n.to_string())
                    .or_else(|| v.as_str().map(str::to_owned))
            })
            .ok_or_else(|| ProviderCallError::DefinitiveFailure {
                status: resp.status,
                reason: "paymob_refund_missing_id".to_owned(),
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
    status: u16,
) -> Result<PaymobCreatedIntention, ProviderCallError> {
    let intention_id = val
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ProviderCallError::DefinitiveFailure {
            status,
            reason: "paymob_missing_intention_id".to_owned(),
        })?
        .to_owned();
    let intention_order_id = val
        .get("intention_order_id")
        .and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
        })
        .ok_or_else(|| ProviderCallError::DefinitiveFailure {
            status,
            reason: "paymob_missing_intention_order_id".to_owned(),
        })?;
    let client_secret = val
        .get("client_secret")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ProviderCallError::DefinitiveFailure {
            status,
            reason: "paymob_missing_client_secret".to_owned(),
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
