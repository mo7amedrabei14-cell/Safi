-- 0002_one_active_session_per_user.up.sql
-- Enforce one active desktop/browser session per account.
-- UUID identifiers remain native PostgreSQL uuidv7() values.

BEGIN;

-- Keep a reversible record of legacy sessions that must be revoked before the
-- unique index can be installed. A rollback restores only rows still carrying
-- the exact timestamp assigned by this migration.
CREATE TABLE session_enforcement_legacy_backup (
    migration_id UUID PRIMARY KEY DEFAULT uuidv7(),
    session_id UUID NOT NULL UNIQUE REFERENCES sessions (id) ON DELETE CASCADE,
    previous_revoked_at TIMESTAMPTZ,
    migration_revoked_at TIMESTAMPTZ NOT NULL
);

DO $do$
DECLARE
    migration_time TIMESTAMPTZ := clock_timestamp();
BEGIN
    WITH ranked_sessions AS (
        SELECT
            id,
            ROW_NUMBER() OVER (
                PARTITION BY user_id
                ORDER BY last_seen_at DESC, created_at DESC, id DESC
            ) AS session_rank
        FROM sessions
        WHERE revoked_at IS NULL
    )
    INSERT INTO session_enforcement_legacy_backup (
        session_id,
        previous_revoked_at,
        migration_revoked_at
    )
    SELECT
        id,
        NULL,
        migration_time
    FROM ranked_sessions
    WHERE session_rank > 1;

    UPDATE sessions AS target
    SET revoked_at = backup.migration_revoked_at
    FROM session_enforcement_legacy_backup AS backup
    WHERE target.id = backup.session_id;
END
$do$;

CREATE UNIQUE INDEX sessions_one_active_session_per_user_key
    ON sessions (user_id)
    WHERE revoked_at IS NULL;

COMMIT;
