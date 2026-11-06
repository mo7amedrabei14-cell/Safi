#!/usr/bin/env bash
# tools/test-migrations.sh — Live PostgreSQL 18 migration proof for T4.02.
#
# Requires SF_TEST_DATABASE_URL pointing at a disposable database and `psql`.
# Applies the up migration, proves workspace isolation through RLS, applies the
# down migration, and proves the schema is gone. Never run this against a
# database that holds real data.
set -euo pipefail

: "${SF_TEST_DATABASE_URL:?set SF_TEST_DATABASE_URL to a disposable PostgreSQL 18 database}"
PSQL_BIN="${PSQL_BIN:-psql}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UP="$ROOT/crates/sf-server/migrations/0001_s4_core.up.sql"
DOWN="$ROOT/crates/sf-server/migrations/0001_s4_core.down.sql"

if ! command -v "$PSQL_BIN" >/dev/null 2>&1; then
    echo "ERROR: $PSQL_BIN is not installed; the live migration test cannot run." >&2
    exit 1
fi

echo "[test-migrations] applying up migration"
"$PSQL_BIN" "$SF_TEST_DATABASE_URL" -v ON_ERROR_STOP=1 -f "$UP" >/dev/null

echo "[test-migrations] checking workspace isolation through RLS"
"$PSQL_BIN" "$SF_TEST_DATABASE_URL" -v ON_ERROR_STOP=1 <<'SQL'
DO $$
DECLARE
    user_one UUID;
    user_two UUID;
    workspace_one UUID;
    workspace_two UUID;
    visible_memberships INTEGER;
BEGIN
    INSERT INTO users (email, display_name) VALUES ('one@example.test', 'One') RETURNING id INTO user_one;
    INSERT INTO users (email, display_name) VALUES ('two@example.test', 'Two') RETURNING id INTO user_two;
    INSERT INTO workspaces (name) VALUES ('Workspace One') RETURNING id INTO workspace_one;
    INSERT INTO workspaces (name) VALUES ('Workspace Two') RETURNING id INTO workspace_two;

    PERFORM set_config('app.workspace_id', workspace_one::TEXT, true);
    INSERT INTO memberships (workspace_id, user_id, role) VALUES (workspace_one, user_one, 'owner');

    PERFORM set_config('app.workspace_id', workspace_two::TEXT, true);
    INSERT INTO memberships (workspace_id, user_id, role) VALUES (workspace_two, user_two, 'owner');

    PERFORM set_config('app.workspace_id', workspace_one::TEXT, true);
    SELECT count(*) INTO visible_memberships FROM memberships;

    IF visible_memberships <> 1 THEN
        RAISE EXCEPTION 'RLS isolation failed: workspace one saw % membership rows', visible_memberships;
    END IF;

    PERFORM set_config('app.workspace_id', workspace_two::TEXT, true);
    SELECT count(*) INTO visible_memberships FROM memberships;
    IF visible_memberships <> 1 THEN
        RAISE EXCEPTION 'RLS isolation failed: workspace two saw % membership rows', visible_memberships;
    END IF;
END
$$;
SQL

echo "[test-migrations] applying down migration"
"$PSQL_BIN" "$SF_TEST_DATABASE_URL" -v ON_ERROR_STOP=1 -f "$DOWN" >/dev/null

REMAINING="$("$PSQL_BIN" "$SF_TEST_DATABASE_URL" -tAc "SELECT count(*) FROM information_schema.tables WHERE table_schema = 'public' AND table_name IN ('users','workspaces','memberships','api_keys','sessions','audit_log');")"
if [ "$REMAINING" != "0" ]; then
    echo "ERROR: down migration left $REMAINING table(s) behind." >&2
    exit 1
fi

echo "[test-migrations] OK: up/down succeeded and workspace isolation held."
