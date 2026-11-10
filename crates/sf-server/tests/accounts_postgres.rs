//! Live PostgreSQL proof for Phase 0.1 customer-account administration.
//!
//! Run by tools/test-migrations.sh after migrations 0001–0005 are applied and
//! the NOBYPASSRLS application role has been provisioned.

use std::sync::Arc;

use sf_server::accounts::{
    AccountStatus, CustomerAccountService, SqlxCustomerAccountRepository, SubscriptionEventDraft,
    SubscriptionEventType,
};
use sf_server::auth::identity::OWNER_EMAIL;
use sf_server::error::AuthError;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

async fn insert_customer(
    admin: &PgPool,
    user_id: Uuid,
    workspace_id: Uuid,
    subscription_id: Uuid,
    email: &str,
    display_name: &str,
) {
    sqlx::query("INSERT INTO users (id, email, display_name) VALUES ($1, $2, $3)")
        .bind(user_id)
        .bind(email)
        .bind(display_name)
        .execute(admin)
        .await
        .expect("insert customer user");

    sqlx::query("INSERT INTO workspaces (id, name, plan_code) VALUES ($1, $2, 'free')")
        .bind(workspace_id)
        .bind(format!("{display_name}'s workspace"))
        .execute(admin)
        .await
        .expect("insert personal workspace");

    sqlx::query("INSERT INTO memberships (workspace_id, user_id, role) VALUES ($1, $2, 'owner')")
        .bind(workspace_id)
        .bind(user_id)
        .execute(admin)
        .await
        .expect("insert owner membership");

    sqlx::query(
        "INSERT INTO subscriptions (id, workspace_id, plan_code, status) VALUES ($1, $2, 'free', 'active')",
    )
    .bind(subscription_id)
    .bind(workspace_id)
    .execute(admin)
    .await
    .expect("insert free subscription");
}

#[tokio::test]
#[ignore = "requires SF_TEST_DATABASE_URL and the migrated PostgreSQL 18 NOBYPASSRLS app role"]
async fn owner_reads_and_updates_customers_across_workspaces() {
    let database_url =
        std::env::var("SF_TEST_DATABASE_URL").expect("SF_TEST_DATABASE_URL is required");
    let admin = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .expect("connect to disposable PostgreSQL database");

    let app = PgPoolOptions::new()
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
    let user_a = Uuid::now_v7();
    let user_b = Uuid::now_v7();
    let workspace_a = Uuid::now_v7();
    let workspace_b = Uuid::now_v7();
    let subscription_a = Uuid::now_v7();
    let subscription_b = Uuid::now_v7();

    insert_customer(
        &admin,
        user_a,
        workspace_a,
        subscription_a,
        &format!("p01-a-{unique}@example.test"),
        "Phase 0.1 Alpha",
    )
    .await;
    insert_customer(
        &admin,
        user_b,
        workspace_b,
        subscription_b,
        &format!("p01-b-{unique}@example.test"),
        "Phase 0.1 Beta",
    )
    .await;

    let service =
        CustomerAccountService::new(Arc::new(SqlxCustomerAccountRepository::new(app.clone())));

    assert_eq!(
        service
            .list_accounts("customer@example.test")
            .await
            .unwrap_err(),
        AuthError::InsufficientScope,
        "a non-owner must not read the customer database"
    );
    assert_eq!(
        service
            .suspend_account("customer@example.test", user_b, 1)
            .await
            .unwrap_err(),
        AuthError::InsufficientScope
    );

    let accounts = service
        .list_accounts(OWNER_EMAIL)
        .await
        .expect("owner reads every customer workspace");
    let alpha = accounts
        .iter()
        .find(|account| account.user_id == user_a)
        .expect("alpha account is visible");
    let beta = accounts
        .iter()
        .find(|account| account.user_id == user_b)
        .expect("beta account is visible");

    assert_eq!(alpha.workspace_id, workspace_a);
    assert_eq!(alpha.role, "owner");
    assert_eq!(alpha.plan_code, "free");
    assert_eq!(alpha.subscription_status, "active");
    assert_eq!(alpha.status, AccountStatus::Active);
    assert!(!alpha.email_verified);
    assert_eq!(beta.workspace_id, workspace_b);

    service
        .suspend_account(OWNER_EMAIL, user_b, 1_780_000_000)
        .await
        .expect("owner suspends the beta account");

    let suspended_at: Option<i64> = sqlx::query_scalar(
        "SELECT EXTRACT(EPOCH FROM suspended_at)::bigint FROM users WHERE id = $1",
    )
    .bind(user_b)
    .fetch_one(&admin)
    .await
    .expect("suspension timestamp is persisted");
    assert_eq!(suspended_at, Some(1_780_000_000));
    let accounts = service.list_accounts(OWNER_EMAIL).await.unwrap();
    assert_eq!(
        accounts
            .iter()
            .find(|account| account.user_id == user_b)
            .expect("beta account remains listed")
            .status,
        AccountStatus::Suspended
    );

    service
        .restore_account(OWNER_EMAIL, user_b)
        .await
        .expect("owner restores the beta account");
    let restored: bool = sqlx::query_scalar("SELECT suspended_at IS NULL FROM users WHERE id = $1")
        .bind(user_b)
        .fetch_one(&admin)
        .await
        .expect("restoration is persisted");
    assert!(restored);

    let event_id = service
        .record_subscription_event(
            OWNER_EMAIL,
            &SubscriptionEventDraft {
                workspace_id: workspace_a,
                subscription_id: Some(subscription_a),
                event_type: SubscriptionEventType::Created,
                from_status: None,
                to_status: Some("active".to_owned()),
                amount_minor: Some(2000),
                currency: Some("USD".to_owned()),
            },
            1_780_000_100,
        )
        .await
        .expect("owner appends a subscription event");
    let stored_event: (String, String, i32, String) = sqlx::query_as(
        "SELECT event_type, actor_role, amount_minor, currency FROM subscription_events WHERE id = $1",
    )
    .bind(event_id)
    .fetch_one(&admin)
    .await
    .expect("subscription event is persisted");
    assert_eq!(
        stored_event,
        (
            "created".to_owned(),
            "owner".to_owned(),
            2000,
            "USD".to_owned()
        )
    );

    // Remove only this test's records.
    for workspace_id in [workspace_a, workspace_b] {
        sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(workspace_id)
            .execute(&admin)
            .await
            .expect("remove test workspace");
    }
    for user_id in [user_a, user_b] {
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(&admin)
            .await
            .expect("remove test user");
    }
}

#[tokio::test]
#[ignore = "requires SF_TEST_DATABASE_URL and the migrated PostgreSQL 18 NOBYPASSRLS app role"]
async fn subscription_lifecycle_is_atomic_and_audited() {
    let database_url =
        std::env::var("SF_TEST_DATABASE_URL").expect("SF_TEST_DATABASE_URL is required");
    let admin = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .expect("connect to disposable PostgreSQL database");
    let app = PgPoolOptions::new()
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

    // Activate the monthly plan only for this test. Read its USD base price
    // from the canonical multi-currency plan_prices table rather than hardcoding
    // a price or relying on the draft plan's nullable legacy scalar fields.
    let activation = sqlx::query(
        r"
        UPDATE plans
        SET status = 'active',
            price_minor = (
                SELECT amount_minor
                FROM plan_prices
                WHERE plan_code = 'individual_monthly'
                  AND currency = 'USD'
                  AND billing_interval = 'monthly'
            ),
            currency = 'USD'
        WHERE code = 'individual_monthly'
          AND EXISTS (
              SELECT 1
              FROM plan_prices
              WHERE plan_code = 'individual_monthly'
                AND currency = 'USD'
                AND billing_interval = 'monthly'
          )
        ",
    )
    .execute(&admin)
    .await
    .expect("activate the monthly plan for the lifecycle proof");
    assert_eq!(
        activation.rows_affected(),
        1,
        "the lifecycle fixture requires the canonical monthly USD price row"
    );

    let unique = Uuid::now_v7().simple().to_string();
    let user_id = Uuid::now_v7();
    let workspace_id = Uuid::now_v7();
    let subscription_id = Uuid::now_v7();
    insert_customer(
        &admin,
        user_id,
        workspace_id,
        subscription_id,
        &format!("p03-{unique}@example.test"),
        "Phase 0.3 Lifecycle",
    )
    .await;

    let service =
        CustomerAccountService::new(Arc::new(SqlxCustomerAccountRepository::new(app.clone())));

    let at: i64 = sqlx::query_scalar(
        "SELECT EXTRACT(EPOCH FROM TIMESTAMPTZ '2026-01-31 00:00:00+00')::bigint",
    )
    .fetch_one(&admin)
    .await
    .expect("start timestamp");
    let end_of_february: i64 = sqlx::query_scalar(
        "SELECT EXTRACT(EPOCH FROM TIMESTAMPTZ '2026-02-28 00:00:00+00')::bigint",
    )
    .fetch_one(&admin)
    .await
    .expect("February end timestamp");
    let end_of_march: i64 = sqlx::query_scalar(
        "SELECT EXTRACT(EPOCH FROM TIMESTAMPTZ '2026-03-28 00:00:00+00')::bigint",
    )
    .fetch_one(&admin)
    .await
    .expect("March end timestamp");

    let free = service
        .current_subscription(OWNER_EMAIL, workspace_id)
        .await
        .expect("owner reads the current subscription")
        .expect("the free subscription exists");
    assert_eq!(free.plan_code, "free");
    assert_eq!(free.current_period_end, None);
    assert_eq!(
        service
            .change_plan(
                "customer@example.test",
                workspace_id,
                "individual_monthly",
                at
            )
            .await
            .unwrap_err(),
        AuthError::InsufficientScope,
        "a non-owner must not change a plan"
    );

    // Force the history insert to fail: the subscription update must roll back.
    sqlx::query(&format!(
        r"
            CREATE OR REPLACE FUNCTION public.safi_p003_reject_event()
            RETURNS trigger
            LANGUAGE plpgsql
            AS $function$
            BEGIN
                IF NEW.workspace_id = '{workspace_id}'::UUID THEN
                    RAISE EXCEPTION 'intentional P0.3 event failure';
                END IF;
                RETURN NEW;
            END
            $function$
            "
    ))
    .execute(&admin)
    .await
    .expect("create the atomicity trigger function");
    sqlx::query(
            "CREATE TRIGGER safi_p003_reject_event BEFORE INSERT ON subscription_events FOR EACH ROW EXECUTE FUNCTION public.safi_p003_reject_event()",
        )
        .execute(&admin)
        .await
        .expect("install the atomicity trigger");

    assert_eq!(
        service
            .change_plan(OWNER_EMAIL, workspace_id, "individual_monthly", at)
            .await
            .unwrap_err(),
        AuthError::BackendUnavailable,
        "a failed history insert must fail the whole operation"
    );
    sqlx::query("DROP TRIGGER safi_p003_reject_event ON subscription_events")
        .execute(&admin)
        .await
        .expect("remove the atomicity trigger");
    sqlx::query("DROP FUNCTION public.safi_p003_reject_event()")
        .execute(&admin)
        .await
        .expect("remove the atomicity trigger function");

    let plan_after_failure: String =
        sqlx::query_scalar("SELECT plan_code FROM subscriptions WHERE id = $1")
            .bind(subscription_id)
            .fetch_one(&admin)
            .await
            .expect("subscription still exists");
    assert_eq!(plan_after_failure, "free");
    let events_after_failure: i64 =
        sqlx::query_scalar("SELECT count(*) FROM subscription_events WHERE workspace_id = $1")
            .bind(workspace_id)
            .fetch_one(&admin)
            .await
            .expect("event count after rollback");
    assert_eq!(events_after_failure, 0);

    let changed = service
        .change_plan(OWNER_EMAIL, workspace_id, "individual_monthly", at)
        .await
        .expect("owner changes the plan");
    assert_eq!(changed.plan_code, "individual_monthly");
    assert_eq!(changed.current_period_start, at);
    assert_eq!(changed.current_period_end, Some(end_of_february));

    let workspace_plan: String =
        sqlx::query_scalar("SELECT plan_code FROM workspaces WHERE id = $1")
            .bind(workspace_id)
            .fetch_one(&admin)
            .await
            .expect("legacy workspace label is synced");
    assert_eq!(workspace_plan, "individual_monthly");

    let renewed = service
        .renew_subscription(OWNER_EMAIL, workspace_id, at, Some(2000), Some("USD"))
        .await
        .expect("owner renews the subscription");
    assert_eq!(renewed.current_period_end, Some(end_of_march));
    assert!(!renewed.cancel_at_period_end);

    let scheduled = service
        .cancel_subscription(OWNER_EMAIL, workspace_id, at + 3_600, true)
        .await
        .expect("owner schedules cancellation");
    assert_eq!(scheduled.status, "active");
    assert!(scheduled.cancel_at_period_end);
    assert_eq!(scheduled.canceled_at, None);

    let canceled = service
        .cancel_subscription(OWNER_EMAIL, workspace_id, at + 7_200, false)
        .await
        .expect("owner cancels immediately");
    assert_eq!(canceled.status, "canceled");
    assert_eq!(canceled.canceled_at, Some(at + 7_200));
    assert_eq!(
        service
            .current_subscription(OWNER_EMAIL, workspace_id)
            .await
            .unwrap(),
        None,
        "a canceled subscription is no longer current"
    );

    let events: Vec<(String, String, Option<String>, String)> = sqlx::query_as(
        r"
            SELECT event_type, actor_role, from_status, metadata::text
            FROM subscription_events
            WHERE workspace_id = $1
            ORDER BY created_at, id
            ",
    )
    .bind(workspace_id)
    .fetch_all(&admin)
    .await
    .expect("subscription history is readable");
    assert_eq!(
        events
            .iter()
            .map(|event| event.0.as_str())
            .collect::<Vec<_>>(),
        vec!["plan_changed", "renewed", "canceled", "canceled"]
    );
    assert!(events.iter().all(|event| event.1 == "owner"));
    assert_eq!(events[0].2.as_deref(), Some("active"));
    let scheduled_metadata: serde_json::Value =
        serde_json::from_str(&events[2].3).expect("valid cancellation metadata");
    assert_eq!(scheduled_metadata["at_period_end"], serde_json::json!(true));
    let immediate_metadata: serde_json::Value =
        serde_json::from_str(&events[3].3).expect("valid cancellation metadata");
    assert_eq!(
        immediate_metadata["at_period_end"],
        serde_json::json!(false)
    );

    // Remove only this test's records and restore the catalog state.
    sqlx::query("DELETE FROM workspaces WHERE id = $1")
        .bind(workspace_id)
        .execute(&admin)
        .await
        .expect("remove test workspace");
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(&admin)
        .await
        .expect("remove test user");
    sqlx::query(
        "UPDATE plans SET status = 'draft', price_minor = NULL, currency = NULL WHERE code = 'individual_monthly'",
    )
    .execute(&admin)
    .await
    .expect("restore the monthly plan draft status and nullable legacy price fields");
}
