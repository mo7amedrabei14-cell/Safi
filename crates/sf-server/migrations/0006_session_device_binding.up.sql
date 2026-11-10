-- 0006_session_device_binding.up.sql
-- P0.04 — bind each password-account session to one stable installation.
-- Legacy active sessions/devices are revoked once; users sign in again with a
-- stable installation identity. No customer file or table data is stored.

BEGIN;

-- A device belongs to a user, but its originating workspace may differ from
-- the workspace selected for a later login. Keep the ownership FK user-scoped
-- so historic sessions remain attached when the same account changes workspace.
ALTER TABLE devices
    ADD CONSTRAINT devices_id_user_key UNIQUE (id, user_id);

ALTER TABLE sessions
    ADD COLUMN device_id UUID,
    ADD CONSTRAINT sessions_device_user_fk
        FOREIGN KEY (device_id, user_id)
        REFERENCES devices (id, user_id)
        ON DELETE CASCADE;

CREATE INDEX sessions_device_id_idx
    ON sessions (device_id)
    WHERE device_id IS NOT NULL;

-- These transaction-local policies permit authentication to find/revoke a
-- previous device in a different workspace. The setting is established only
-- by the trusted server transaction, never by request input.
CREATE POLICY devices_auth_lookup_read ON devices
    FOR SELECT
    USING (current_setting('app.auth_lookup', true) = 'true');

CREATE POLICY devices_auth_lookup_insert ON devices
    FOR INSERT
    WITH CHECK (
        current_setting('app.auth_lookup', true) = 'true'
        AND workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::UUID
    );

CREATE POLICY devices_auth_lookup_update ON devices
    FOR UPDATE
    USING (current_setting('app.auth_lookup', true) = 'true')
    WITH CHECK (current_setting('app.auth_lookup', true) = 'true');

CREATE POLICY sessions_auth_lookup_update ON sessions
    FOR UPDATE
    USING (current_setting('app.auth_lookup', true) = 'true')
    WITH CHECK (current_setting('app.auth_lookup', true) = 'true');

-- The migration itself must use the same narrowly scoped auth capability when
-- it snapshots and revokes legacy rows under FORCE ROW LEVEL SECURITY.
SELECT set_config('app.auth_lookup', 'true', true);

-- Reversible audit of legacy active rows. Rollback restores only rows that
-- still have the precise timestamp assigned by this migration; a later login
-- or revocation is never undone.
CREATE TABLE sessions_p004_legacy_backup (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    session_id UUID NOT NULL UNIQUE REFERENCES sessions (id) ON DELETE CASCADE,
    previous_revoked_at TIMESTAMPTZ,
    migration_revoked_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE devices_p004_legacy_backup (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    device_id UUID NOT NULL UNIQUE REFERENCES devices (id) ON DELETE CASCADE,
    previous_revoked_at TIMESTAMPTZ,
    migration_revoked_at TIMESTAMPTZ NOT NULL
);

DO $do$
DECLARE
    migration_time TIMESTAMPTZ := clock_timestamp();
BEGIN
    INSERT INTO sessions_p004_legacy_backup (
        session_id, previous_revoked_at, migration_revoked_at
    )
    SELECT id, revoked_at, migration_time
    FROM sessions
    WHERE revoked_at IS NULL;

    INSERT INTO devices_p004_legacy_backup (
        device_id, previous_revoked_at, migration_revoked_at
    )
    SELECT id, revoked_at, migration_time
    FROM devices
    WHERE revoked_at IS NULL;

    UPDATE sessions AS target
    SET revoked_at = migration_time
    FROM sessions_p004_legacy_backup AS backup
    WHERE target.id = backup.session_id;

    UPDATE devices AS target
    SET revoked_at = migration_time
    FROM devices_p004_legacy_backup AS backup
    WHERE target.id = backup.device_id;
END
$do$;

COMMIT;
