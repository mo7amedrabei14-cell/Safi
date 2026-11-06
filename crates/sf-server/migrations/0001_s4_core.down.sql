-- 0001_s4_core.down.sql — Reverts 0001_s4_core.up.sql completely.
-- Order is the exact reverse of creation; no extension is dropped because
-- pg_partman may be shared with other databases on the same instance.

BEGIN;

DO $do$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_namespace WHERE nspname = 'partman'
    ) AND EXISTS (
        SELECT 1 FROM pg_class WHERE relname = 'part_config'
    ) THEN
        DELETE FROM partman.part_config WHERE parent_table = 'public.audit_log';
    END IF;
END
$do$;

DROP TABLE IF EXISTS audit_log_default;
DROP TABLE IF EXISTS audit_log;
DROP TABLE IF EXISTS sessions;
DROP TABLE IF EXISTS api_keys;
DROP TABLE IF EXISTS memberships;
DROP TABLE IF EXISTS workspaces;
DROP TABLE IF EXISTS users;

DROP FUNCTION IF EXISTS touch_updated_at();

COMMIT;
