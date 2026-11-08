//! Integration tests for Zero Trust, Hardware YubiKey Token, and Audit Log Center in sf-server (T5.12 & T5.13).

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use sf_server::auth::cache::InMemoryAuthCache;
    use sf_server::auth::repository::{InMemoryApiKeyRepository, InMemorySessionRepository};
    use sf_server::auth::service::AuthService;
    use sf_server::{app, AppState};

    async fn test_app() -> axum::Router {
        let api_keys = Arc::new(InMemoryApiKeyRepository::new(vec![]).await);
        let sessions = Arc::new(InMemorySessionRepository::new());
        let cache = Arc::new(InMemoryAuthCache::new());
        let auth = AuthService::new(api_keys, sessions, cache);
        let state = AppState::new(auth);
        app(state)
    }

    #[tokio::test]
    async fn test_owner_audit_logs_success_and_chain_integrity() {
        let app = test_app().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/owner/audit-logs")
                    .header("X-Safi-Zero-Trust-Token", "safi_zt_tunnel_verified_secure_2026")
                    .header("X-Safi-Device-Attestation", "device_attested_ok")
                    .header("X-Safi-Hardware-Signature", "yubikey_fips_sig_yubikey_fips_sec_key_2026_verified_sig123")
                    .header("X-Safi-Challenge-Nonce", "nonce_log_center_123")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["is_chain_valid"], true);
        assert!(json["entries"].as_array().unwrap().len() >= 3);
    }
}
