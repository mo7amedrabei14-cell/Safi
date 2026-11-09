#!/usr/bin/env bash
# Live PostgreSQL 18 migration proof for P0.01 and the S4 core schema.
# This script MUST target a disposable database. It applies all migrations,
# verifies account constraints and real RLS behavior under a non-owner,
# NOBYPASSRLS role, then rolls every migration back.
set -euo pipefail

if [[ -z "${SF_TEST_DATABASE_URL:-}" ]]; then
    echo "ERROR: set SF_TEST_DATABASE_URL to a disposable PostgreSQL 18 database." >&2
    exit 2
fi

PSQL_BIN="${PSQL_BIN:-psql}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MIGRATIONS="$ROOT/crates/sf-server/migrations"

if ! command -v "$PSQL_BIN" >/dev/null 2>&1; then
    echo "ERROR: $PSQL_BIN is not installed; the live migration test cannot run." >&2
    exit 1
fi

SERVER_MAJOR="$("$PSQL_BIN" "$SF_TEST_DATABASE_URL" -v ON_ERROR_STOP=1 -tAc \
    "SELECT current_setting('server_version_num')::INTEGER / 10000;")"
if [[ "$SERVER_MAJOR" != "18" ]]; then
    echo "ERROR: PostgreSQL 18 is required; detected major version $SERVER_MAJOR." >&2
    exit 1
fi
SERVER_VERSION="$("$PSQL_BIN" "$SF_TEST_DATABASE_URL" -v ON_ERROR_STOP=1 -tAc "SHOW server_version;")"
echo "[test-migrations] PostgreSQL $SERVER_VERSION"

echo "[test-migrations] applying 0001 core schema"
"$PSQL_BIN" "$SF_TEST_DATABASE_URL" -v ON_ERROR_STOP=1 \
    -f "$MIGRATIONS/0001_s4_core.up.sql" >/dev/null

echo "[test-migrations] seeding synthetic legacy workspaces before P0.01"
"$PSQL_BIN" "$SF_TEST_DATABASE_URL" -v ON_ERROR_STOP=1 <<'SQL'
INSERT INTO users (id, email, display_name) VALUES
    ('10000000-0000-7000-8000-000000000001', 'p001-one@example.test', 'P0.01 One'),
    ('10000000-0000-7000-8000-000000000002', 'p001-two@example.test', 'P0.01 Two'),
    ('10000000-0000-7000-8000-000000000003', 'p001-three@example.test', 'P0.01 Three');

INSERT INTO workspaces (id, name, plan_code) VALUES
    ('20000000-0000-7000-8000-000000000001', 'P0.01 Free Workspace', 'free'),
    ('20000000-0000-7000-8000-000000000002', 'P0.01 Draft Plan Workspace', 'pro'),
    ('20000000-0000-7000-8000-000000000003', 'P0.01 Legacy Workspace', 'legacy_team');

INSERT INTO memberships (workspace_id, user_id, role) VALUES
    ('20000000-0000-7000-8000-000000000001', '10000000-0000-7000-8000-000000000001', 'owner'),
    ('20000000-0000-7000-8000-000000000002', '10000000-0000-7000-8000-000000000002', 'owner'),
    ('20000000-0000-7000-8000-000000000003', '10000000-0000-7000-8000-000000000001', 'owner'),
    ('20000000-0000-7000-8000-000000000003', '10000000-0000-7000-8000-000000000003', 'member');
SQL

echo "[test-migrations] applying 0002 and P0.01 migration 0003"
"$PSQL_BIN" "$SF_TEST_DATABASE_URL" -v ON_ERROR_STOP=1 \
    -f "$MIGRATIONS/0002_one_active_session_per_user.up.sql" >/dev/null
"$PSQL_BIN" "$SF_TEST_DATABASE_URL" -v ON_ERROR_STOP=1 \
    -f "$MIGRATIONS/0003_accounts_subscriptions_devices.up.sql" >/dev/null
"$PSQL_BIN" "$SF_TEST_DATABASE_URL" -v ON_ERROR_STOP=1 \
    -f "$MIGRATIONS/0004_auth_lookup_rls.up.sql" >/dev/null

echo "[test-migrations] verifying account, token, device, plan, subscription, and webhook constraints"
"$PSQL_BIN" "$SF_TEST_DATABASE_URL" -v ON_ERROR_STOP=1 <<'SQL'
DO $$
DECLARE
    row_count INTEGER;
    selected_plan TEXT;
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'users'
          AND column_name = 'email_verified_at'
    ) THEN
        RAISE EXCEPTION 'P0.01 did not add users.email_verified_at';
    END IF;

    SELECT count(*) INTO row_count FROM plans WHERE status = 'active';
    IF row_count <> 1 OR NOT EXISTS (
        SELECT 1 FROM plans
        WHERE code = 'free' AND status = 'active'
          AND price_minor = 0 AND currency = 'EGP'
    ) THEN
        RAISE EXCEPTION 'Only the Free plan may be active in P0.01';
    END IF;

    IF EXISTS (
        SELECT 1 FROM plans
        WHERE code <> 'free'
          AND (status <> 'draft' OR price_minor IS NOT NULL OR currency IS NOT NULL)
    ) THEN
        RAISE EXCEPTION 'A paid plan was activated or assigned a price prematurely';
    END IF;

    IF EXISTS (SELECT 1 FROM subscriptions WHERE plan_code <> 'free') THEN
        RAISE EXCEPTION 'A legacy workspace was backfilled onto a non-active plan';
    END IF;

    SELECT plan_code INTO selected_plan FROM workspaces
    WHERE id = '20000000-0000-7000-8000-000000000002';
    IF selected_plan <> 'pro' THEN
        RAISE EXCEPTION 'P0.01 rewrote the legacy workspace plan label';
    END IF;

    SELECT count(*) INTO row_count
    FROM subscriptions
    WHERE workspace_id IN (
        '20000000-0000-7000-8000-000000000001',
        '20000000-0000-7000-8000-000000000002',
        '20000000-0000-7000-8000-000000000003'
    )
      AND status IN ('trialing', 'active', 'past_due', 'grace_period');
    IF row_count <> 3 THEN
        RAISE EXCEPTION 'Expected one current subscription per legacy workspace; found %', row_count;
    END IF;

    BEGIN
        INSERT INTO user_credentials (user_id, password_hash)
        VALUES ('10000000-0000-7000-8000-000000000001', 'plain-text-password');
        RAISE EXCEPTION 'Plaintext password was accepted';
    EXCEPTION WHEN check_violation THEN
        NULL;
    END;

    INSERT INTO user_credentials (user_id, password_hash) VALUES
        ('10000000-0000-7000-8000-000000000001',
         '$argon2id$v=19$m=65536,t=3,p=1$c2FsdA$ZGF0YQ'),
        ('10000000-0000-7000-8000-000000000002',
         '$argon2id$v=19$m=65536,t=3,p=1$c2FsdA$ZGF0YQ');

    INSERT INTO email_verification_tokens (user_id, token_hash, expires_at)
    VALUES (
        '10000000-0000-7000-8000-000000000001',
        decode(repeat('a', 64), 'hex'),
        now() + INTERVAL '1 hour'
    );

    BEGIN
        INSERT INTO email_verification_tokens (user_id, token_hash, expires_at)
        VALUES (
            '10000000-0000-7000-8000-000000000001',
            decode(repeat('b', 64), 'hex'),
            now() + INTERVAL '1 hour'
        );
        RAISE EXCEPTION 'A second pending email token was accepted';
    EXCEPTION WHEN unique_violation THEN
        NULL;
    END;

    BEGIN
        INSERT INTO password_reset_tokens (user_id, token_hash, expires_at)
        VALUES (
            '10000000-0000-7000-8000-000000000002',
            decode(repeat('c', 62), 'hex'),
            now() + INTERVAL '1 hour'
        );
        RAISE EXCEPTION 'A 31-byte password reset token hash was accepted';
    EXCEPTION WHEN check_violation THEN
        NULL;
    END;

    BEGIN
        INSERT INTO email_verification_tokens (user_id, token_hash, expires_at)
        VALUES (
            '10000000-0000-7000-8000-000000000002',
            decode(repeat('d', 64), 'hex'),
            now() - INTERVAL '1 second'
        );
        RAISE EXCEPTION 'An expired-at-creation token was accepted';
    EXCEPTION WHEN check_violation THEN
        NULL;
    END;

    INSERT INTO password_reset_tokens (user_id, token_hash, expires_at)
    VALUES (
        '10000000-0000-7000-8000-000000000002',
        decode(repeat('e', 64), 'hex'),
        now() + INTERVAL '1 hour'
    );

    INSERT INTO devices (
        workspace_id, user_id, device_name, platform, installation_key_hash
    ) VALUES
        ('20000000-0000-7000-8000-000000000001',
         '10000000-0000-7000-8000-000000000001',
         'P0.01 Device One', 'windows', decode(repeat('1', 64), 'hex')),
        ('20000000-0000-7000-8000-000000000002',
         '10000000-0000-7000-8000-000000000002',
         'P0.01 Device Two', 'web', decode(repeat('2', 64), 'hex'));

    BEGIN
        INSERT INTO devices (
            workspace_id, user_id, device_name, platform, installation_key_hash
        ) VALUES (
            '20000000-0000-7000-8000-000000000002',
            '10000000-0000-7000-8000-000000000003',
            'Wrong Workspace Device', 'linux', decode(repeat('3', 64), 'hex')
        );
        RAISE EXCEPTION 'A device was attached to a workspace without user membership';
    EXCEPTION WHEN foreign_key_violation THEN
        NULL;
    END;

    BEGIN
        INSERT INTO devices (
            workspace_id, user_id, device_name, platform, installation_key_hash
        ) VALUES (
            '20000000-0000-7000-8000-000000000003',
            '10000000-0000-7000-8000-000000000001',
            'Second Active Device', 'linux', decode(repeat('4', 64), 'hex')
        );
        RAISE EXCEPTION 'A second active device for the same user was accepted';
    EXCEPTION WHEN unique_violation THEN
        NULL;
    END;

    BEGIN
        INSERT INTO subscriptions (workspace_id, plan_code, status)
        VALUES ('20000000-0000-7000-8000-000000000001', 'free', 'active');
        RAISE EXCEPTION 'A second current subscription for one workspace was accepted';
    EXCEPTION WHEN unique_violation THEN
        NULL;
    END;

    INSERT INTO billing_webhook_events (provider, provider_event_id, payload_hash)
    VALUES ('paymob', 'p0-01-event-1', decode(repeat('f', 64), 'hex'));

    BEGIN
        INSERT INTO billing_webhook_events (provider, provider_event_id, payload_hash)
        VALUES ('paymob', 'p0-01-event-1', decode(repeat('0', 64), 'hex'));
        RAISE EXCEPTION 'A duplicate provider event was accepted';
    EXCEPTION WHEN unique_violation THEN
        NULL;
    END;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'billing_webhook_events'
          AND column_name IN ('payload', 'payload_json', 'raw_body', 'customer_file')
    ) THEN
        RAISE EXCEPTION 'Webhook table stores a raw payload or customer file field';
    END IF;
END
$$;
SQL

echo "[test-migrations] verifying real RLS isolation as a non-owner, NOBYPASSRLS role"
"$PSQL_BIN" "$SF_TEST_DATABASE_URL" -v ON_ERROR_STOP=1 <<'SQL'
CREATE ROLE safi_p0_01_app NOLOGIN NOBYPASSRLS;
GRANT USAGE ON SCHEMA public TO safi_p0_01_app;
GRANT SELECT, INSERT, UPDATE ON users, workspaces, memberships, sessions, devices, subscriptions, user_credentials, email_verification_tokens TO safi_p0_01_app;

BEGIN;
SET LOCAL ROLE safi_p0_01_app;
SELECT set_config('app.workspace_id', '20000000-0000-7000-8000-000000000001', true);
SELECT set_config('app.user_id', '10000000-0000-7000-8000-000000000001', true);

DO $$
DECLARE
    row_count INTEGER;
BEGIN
    SELECT count(*) INTO row_count FROM memberships;
    IF row_count <> 1 THEN
        RAISE EXCEPTION 'Workspace RLS failed for memberships: saw % rows', row_count;
    END IF;

    SELECT count(*) INTO row_count FROM devices;
    IF row_count <> 1 THEN
        RAISE EXCEPTION 'Workspace RLS failed for devices: saw % rows', row_count;
    END IF;

    SELECT count(*) INTO row_count FROM subscriptions;
    IF row_count <> 1 THEN
        RAISE EXCEPTION 'Workspace RLS failed for subscriptions: saw % rows', row_count;
    END IF;

    SELECT count(*) INTO row_count FROM user_credentials;
    IF row_count <> 1 THEN
        RAISE EXCEPTION 'User RLS failed for user_credentials: saw % rows', row_count;
    END IF;

    PERFORM set_config('app.user_id', '10000000-0000-7000-8000-000000000002', true);
    SELECT count(*) INTO row_count FROM user_credentials;
    IF row_count <> 1 THEN
        RAISE EXCEPTION 'User RLS failed to switch credential visibility';
    END IF;

    PERFORM set_config('app.user_id', '', true);
    SELECT count(*) INTO row_count FROM user_credentials;
    IF row_count <> 0 THEN
        RAISE EXCEPTION 'Credential rows leaked without app.user_id: saw % rows', row_count;
    END IF;

    PERFORM set_config('app.auth_lookup', 'true', true);
    SELECT count(*) INTO row_count FROM user_credentials;
    IF row_count <> 2 THEN
        RAISE EXCEPTION 'Trusted auth lookup could not read the credential records: saw % rows', row_count;
    END IF;
    PERFORM set_config('app.auth_lookup', 'false', true);
    SELECT count(*) INTO row_count FROM user_credentials;
    IF row_count <> 0 THEN
        RAISE EXCEPTION 'Disabling auth lookup did not restore user isolation';
    END IF;

    PERFORM set_config('app.workspace_id', '20000000-0000-7000-8000-000000000002', true);
    SELECT count(*) INTO row_count FROM devices;
    IF row_count <> 1 THEN
        RAISE EXCEPTION 'Workspace RLS failed when switching device workspace';
    END IF;

    SELECT count(*) INTO row_count FROM subscriptions;
    IF row_count <> 1 THEN
        RAISE EXCEPTION 'Workspace RLS failed when switching subscription workspace';
    END IF;
END
$$;
COMMIT;
SQL

echo "[test-migrations] exercising SQLx account registration, verification, and login under the restricted app role"
SF_TEST_DATABASE_URL="$SF_TEST_DATABASE_URL" cargo test -p sf-server --test account_postgres -- --ignored --test-threads=1

echo "[test-migrations] rolling back 0004, 0003, 0002, and 0001 in reverse order"
"$PSQL_BIN" "$SF_TEST_DATABASE_URL" -v ON_ERROR_STOP=1 \
    -f "$MIGRATIONS/0004_auth_lookup_rls.down.sql" >/dev/null
"$PSQL_BIN" "$SF_TEST_DATABASE_URL" -v ON_ERROR_STOP=1 \
    -f "$MIGRATIONS/0003_accounts_subscriptions_devices.down.sql" >/dev/null
"$PSQL_BIN" "$SF_TEST_DATABASE_URL" -v ON_ERROR_STOP=1 \
    -f "$MIGRATIONS/0002_one_active_session_per_user.down.sql" >/dev/null
"$PSQL_BIN" "$SF_TEST_DATABASE_URL" -v ON_ERROR_STOP=1 \
    -f "$MIGRATIONS/0001_s4_core.down.sql" >/dev/null

REMAINING="$("$PSQL_BIN" "$SF_TEST_DATABASE_URL" -tAc \
    "SELECT count(*) FROM information_schema.tables WHERE table_schema = 'public' AND table_name IN ('users','workspaces','memberships','api_keys','sessions','audit_log','user_credentials','email_verification_tokens','password_reset_tokens','devices','plans','subscriptions','billing_webhook_events','session_enforcement_legacy_backup');")"
if [[ "$REMAINING" != "0" ]]; then
    echo "ERROR: rollback left $REMAINING migration table(s) behind." >&2
    exit 1
fi

COLUMN_REMAINING="$("$PSQL_BIN" "$SF_TEST_DATABASE_URL" -tAc \
    "SELECT count(*) FROM information_schema.columns WHERE table_schema = 'public' AND table_name = 'users' AND column_name = 'email_verified_at';")"
if [[ "$COLUMN_REMAINING" != "0" ]]; then
    echo "ERROR: rollback left users.email_verified_at behind." >&2
    exit 1
fi

"$PSQL_BIN" "$SF_TEST_DATABASE_URL" -v ON_ERROR_STOP=1 <<'SQL'
DROP OWNED BY safi_p0_01_app;
DROP ROLE safi_p0_01_app;
SQL

echo "[test-migrations] PASS: PostgreSQL 18 apply/constraints/RLS/rollback verified."
