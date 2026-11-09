//! PostgreSQL integration proof for the P0.02 account lifecycle.
//!
//! Run by tools/test-migrations.sh after migrations 0001–0004 are applied and
//! the NOBYPASSRLS application role has been provisioned.

use std::sync::Arc;

use async_trait::async_trait;
use sf_server::auth::account::{
    AccountAuthService, LoginAccountRequest, RegisterAccountRequest, SqlxAccountRepository,
    VerificationEmailSender,
};
use sf_server::auth::cache::InMemoryAuthCache;
use sf_server::auth::repository::{InMemoryApiKeyRepository, SqlxSessionRepository};
use sf_server::auth::service::AuthService;
use sf_server::auth::AuthMethod;
use sf_server::auth::Scope;
use sf_server::error::AuthError;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Default)]
struct CaptureMailer {
    messages: Mutex<Vec<(String, String)>>,
}

impl CaptureMailer {
    async fn messages(&self) -> Vec<(String, String)> {
        self.messages.lock().await.clone()
    }
}

#[async_trait]
impl VerificationEmailSender for CaptureMailer {
    async fn send_verification(
        &self,
        recipient: &str,
        verification_url: &str,
    ) -> Result<(), AuthError> {
        self.messages
            .lock()
            .await
            .push((recipient.to_owned(), verification_url.to_owned()));
        Ok(())
    }
}

fn verification_token(message: &(String, String)) -> String {
    message
        .1
        .split("verify_email=")
        .nth(1)
        .unwrap_or_default()
        .to_owned()
}

async fn scalar_count(pool: &PgPool, query: &str, value: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(query)
        .bind(value)
        .fetch_one(pool)
        .await
        .expect("count query should succeed")
}

#[tokio::test]
async fn postgres_account_registration_verification_and_login_are_atomic_and_rls_safe() {
    let database_url =
        std::env::var("SF_TEST_DATABASE_URL").expect("SF_TEST_DATABASE_URL is required");
    let admin_pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .expect("connect to disposable PostgreSQL database");

    // All application transactions run as the same NOBYPASSRLS role exercised
    // by the migration gate. The admin pool is used only for test setup/assertions.
    let app_pool = PgPoolOptions::new()
        .max_connections(4)
        .after_connect(|connection, _meta| {
            Box::pin(async move {
                sqlx::query("SET ROLE safi_p0_01_app")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect(&database_url)
        .await
        .expect("connect as the restricted application role");

    let unique = Uuid::now_v7().simple().to_string();
    let atomic_email = format!("p002-atomic-{unique}@example.test");
    let atomic_display_name = format!("P0.02 Atomic {unique}");
    let atomic_workspace_name = format!("{atomic_display_name}'s workspace");

    // Fail the final provisioning write deliberately. The repository must roll
    // back the user, workspace, owner membership, password hash, and email token.
    sqlx::query(
        r"
        CREATE OR REPLACE FUNCTION public.safi_p002_test_reject_subscription()
        RETURNS trigger
        LANGUAGE plpgsql
        AS $function$
        DECLARE
            registering_email TEXT;
        BEGIN
            SELECT email INTO registering_email
            FROM users
            WHERE id = NULLIF(current_setting('app.user_id', true), '')::UUID;

            IF registering_email LIKE 'p002-atomic-%@example.test' THEN
                RAISE EXCEPTION 'intentional P0.02 transaction rollback test';
            END IF;
            RETURN NEW;
        END
        $function$
        ",
    )
    .execute(&admin_pool)
    .await
    .expect("create the isolated rollback-test trigger function");
    sqlx::query(
        "CREATE TRIGGER safi_p002_test_reject_subscription BEFORE INSERT ON subscriptions FOR EACH ROW EXECUTE FUNCTION public.safi_p002_test_reject_subscription()",
    )
    .execute(&admin_pool)
    .await
    .expect("install the rollback-test trigger");

    let mailer = Arc::new(CaptureMailer::default());
    let auth = Arc::new(AuthService::new(
        Arc::new(InMemoryApiKeyRepository::default()),
        Arc::new(SqlxSessionRepository::new(app_pool.clone())),
        Arc::new(InMemoryAuthCache::new()),
    ));
    let service = AccountAuthService::new(
        Arc::new(SqlxAccountRepository::new(app_pool.clone())),
        mailer.clone(),
        auth.clone(),
        "https://safidata.example",
    );

    let failed_registration = service
        .register(RegisterAccountRequest {
            email: atomic_email.clone(),
            display_name: atomic_display_name.clone(),
            password: "A secure phrase 2026!".to_owned(),
        })
        .await;
    sqlx::query("DROP TRIGGER safi_p002_test_reject_subscription ON subscriptions")
        .execute(&admin_pool)
        .await
        .expect("remove the rollback-test trigger");
    sqlx::query("DROP FUNCTION public.safi_p002_test_reject_subscription()")
        .execute(&admin_pool)
        .await
        .expect("remove the rollback-test trigger function");

    assert_eq!(
        failed_registration.unwrap_err(),
        AuthError::BackendUnavailable
    );
    assert_eq!(
        scalar_count(
            &admin_pool,
            "SELECT count(*) FROM users WHERE lower(email) = lower($1)",
            &atomic_email
        )
        .await,
        0,
        "failed account provisioning must roll back the user row"
    );
    assert_eq!(
        scalar_count(
            &admin_pool,
            "SELECT count(*) FROM workspaces WHERE name = $1",
            &atomic_workspace_name
        )
        .await,
        0,
        "failed account provisioning must roll back its workspace"
    );
    assert!(mailer.messages().await.is_empty());

    let email = format!("p002-{unique}@example.test");
    let display_name = format!("P0.02 Integration {unique}");
    let workspace_name = format!("{display_name}'s workspace");
    let password = "A secure phrase 2026!";

    service
        .register(RegisterAccountRequest {
            email: email.clone(),
            display_name: display_name.clone(),
            password: password.to_owned(),
        })
        .await
        .expect("registration should commit through PostgreSQL");

    let first_messages = mailer.messages().await;
    assert_eq!(first_messages.len(), 1);
    assert_eq!(first_messages[0].0, email);
    assert!(first_messages[0]
        .1
        .starts_with("https://safidata.example/?verify_email="));
    let first_token = verification_token(&first_messages[0]);

    let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users WHERE lower(email) = lower($1)")
        .bind(&email)
        .fetch_one(&admin_pool)
        .await
        .expect("new user should be present");
    let workspace_id: Uuid = sqlx::query_scalar("SELECT id FROM workspaces WHERE name = $1")
        .bind(&workspace_name)
        .fetch_one(&admin_pool)
        .await
        .expect("personal workspace should be present");

    let owner_membership_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM memberships WHERE workspace_id = $1 AND user_id = $2 AND role = 'owner'",
    )
    .bind(workspace_id)
    .bind(user_id)
    .fetch_one(&admin_pool)
    .await
    .expect("owner membership count should be queryable");
    assert_eq!(owner_membership_count, 1);

    let membership_role: String =
        sqlx::query_scalar("SELECT role FROM memberships WHERE workspace_id = $1 AND user_id = $2")
            .bind(workspace_id)
            .bind(user_id)
            .fetch_one(&admin_pool)
            .await
            .expect("personal owner membership should exist");
    assert_eq!(membership_role, "owner");

    let password_hash: String =
        sqlx::query_scalar("SELECT password_hash FROM user_credentials WHERE user_id = $1")
            .bind(user_id)
            .fetch_one(&admin_pool)
            .await
            .expect("Argon2id credential should exist");
    assert!(password_hash.starts_with("$argon2id$"));
    assert_ne!(password_hash, password);

    let stored_token_hash: Vec<u8> = sqlx::query_scalar(
        "SELECT token_hash FROM email_verification_tokens WHERE user_id = $1 AND consumed_at IS NULL",
    )
    .bind(user_id)
    .fetch_one(&admin_pool)
    .await
    .expect("one pending verification hash should exist");
    let token_bytes = hex::decode(&first_token).expect("mail token is hexadecimal");
    assert_eq!(stored_token_hash.len(), 32);
    assert_eq!(stored_token_hash, blake3::hash(&token_bytes).as_bytes());

    let subscription: (String, String) =
        sqlx::query_as("SELECT plan_code, status FROM subscriptions WHERE workspace_id = $1")
            .bind(workspace_id)
            .fetch_one(&admin_pool)
            .await
            .expect("Free subscription should exist");
    assert_eq!(subscription, ("free".to_owned(), "active".to_owned()));

    assert_eq!(
        service
            .login(LoginAccountRequest {
                email: email.clone(),
                password: password.to_owned(),
                device_name: "PostgreSQL integration test".to_owned(),
            })
            .await
            .unwrap_err(),
        AuthError::InvalidCredentials,
        "an unverified account must not receive a session"
    );

    // Duplicate registration remains enumeration-safe and sends no second email.
    service
        .register(RegisterAccountRequest {
            email: email.clone(),
            display_name: display_name.clone(),
            password: password.to_owned(),
        })
        .await
        .expect("duplicate registration uses the generic accepted service result");
    assert_eq!(mailer.messages().await.len(), 1);

    // Resend atomically consumes the first link and replaces it with a new one.
    service
        .resend_verification(&email)
        .await
        .expect("verification resend should succeed");
    let messages = mailer.messages().await;
    assert_eq!(messages.len(), 2);
    let current_token = verification_token(&messages[1]);
    assert_eq!(
        service.verify_email(&first_token).await.unwrap_err(),
        AuthError::InvalidVerificationToken
    );
    service
        .verify_email(&current_token)
        .await
        .expect("the replacement token should verify the account");

    let is_verified: bool =
        sqlx::query_scalar("SELECT email_verified_at IS NOT NULL FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(&admin_pool)
            .await
            .expect("verification timestamp should be readable");
    assert!(is_verified);

    let issued = service
        .login(LoginAccountRequest {
            email: email.clone(),
            password: password.to_owned(),
            device_name: "PostgreSQL integration test".to_owned(),
        })
        .await
        .expect("verified account should receive a database-backed session");
    assert!(issued.token.starts_with("sfs_"));
    let authenticated = auth
        .authenticate_session(&issued.token)
        .await
        .expect("issued session should pass the existing session middleware");
    assert_eq!(authenticated.principal.user_id, user_id);
    assert_eq!(authenticated.principal.workspace_id, workspace_id);
    assert!(matches!(
        authenticated.principal.auth_method,
        AuthMethod::Session { .. }
    ));
    assert!(authenticated.principal.scopes.contains(&Scope::Read));
    assert!(authenticated.principal.scopes.contains(&Scope::Write));

    // Remove only this test's records. The migration script performs the final
    // full rollback after this integration test returns.
    sqlx::query("DELETE FROM workspaces WHERE id = $1")
        .bind(workspace_id)
        .execute(&admin_pool)
        .await
        .expect("remove integration workspace");
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&admin_pool)
        .await
        .expect("remove integration user");
}
