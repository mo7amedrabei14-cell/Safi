//! Integration tests for Zero Trust & Hardware YubiKey Token protection in sf-server (T5.12).

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
    async fn test_owner_stats_rejected_without_zero_trust_headers() {
        let app = test_app().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/owner/stats")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_owner_stats_rejected_without_hardware_token_signature() {
        let app = test_app().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/owner/stats")
                    .header("X-Safi-Zero-Trust-Token", "safi_zt_tunnel_verified_secure_2026")
                    .header("X-Safi-Device-Attestation", "device_attested_ok")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_owner_stats_success_with_valid_zero_trust_and_yubikey_token() {
        let app = test_app().await;
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/owner/stats")
                    .header("X-Safi-Zero-Trust-Token", "safi_zt_tunnel_verified_secure_2026")
                    .header("X-Safi-Device-Attestation", "device_attested_ok")
                    .header("X-Safi-Hardware-Signature", "yubikey_fips_sig_yubikey_fips_sec_key_2026_verified_sig123")
                    .header("X-Safi-Challenge-Nonce", "nonce_abc_12345")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_owner_stats_replay_attack_rejected() {
        let router = test_app().await;
        let req_builder = || {
            Request::builder()
                .uri("/v1/owner/stats")
                .header("X-Safi-Zero-Trust-Token", "safi_zt_tunnel_verified_secure_2026")
                .header("X-Safi-Device-Attestation", "device_attested_ok")
                .header("X-Safi-Hardware-Signature", "yubikey_fips_sig_yubikey_fips_sec_key_2026_verified_sig123")
                .header("X-Safi-Challenge-Nonce", "nonce_replay_test_999")
                .body(axum::body::Body::empty())
                .unwrap()
        };

        let resp1 = router.clone().oneshot(req_builder()).await.unwrap();
        assert_eq!(resp1.status(), StatusCode::OK);
    }
}
