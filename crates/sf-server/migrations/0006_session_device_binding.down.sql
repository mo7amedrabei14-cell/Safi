-- 0006_session_device_binding.down.sql
-- Undo P0.04. Restore only legacy rows that have not been changed since this
-- migration revoked them; later account activity remains authoritative.

BEGIN;

-- Restore only the migration's own rows while the temporary auth-lookup
-- UPDATE policies are still present, even for a non-BYPASSRLS migration role.
SELECT set_config('app.auth_lookup', 'true', true);

UPDATE sessions AS target
SET revoked_at = backup.previous_revoked_at
FROM sessions_p004_legacy_backup AS backup
WHERE target.id = backup.session_id
  AND target.revoked_at = backup.migration_revoked_at
  AND NOT EXISTS (
      SELECT 1
      FROM sessions AS newer
      WHERE newer.user_id = target.user_id
        AND newer.id <> target.id
        AND newer.created_at > backup.migration_revoked_at
  );

UPDATE devices AS target
SET revoked_at = backup.previous_revoked_at
FROM devices_p004_legacy_backup AS backup
WHERE target.id = backup.device_id
  AND target.revoked_at = backup.migration_revoked_at
  AND NOT EXISTS (
      SELECT 1
      FROM devices AS newer
      WHERE newer.user_id = target.user_id
        AND newer.id <> target.id
        AND newer.registered_at > backup.migration_revoked_at
  );

DROP POLICY IF EXISTS sessions_auth_lookup_update ON sessions;
DROP POLICY IF EXISTS devices_auth_lookup_update ON devices;
DROP POLICY IF EXISTS devices_auth_lookup_insert ON devices;
DROP POLICY IF EXISTS devices_auth_lookup_read ON devices;

DROP TABLE IF EXISTS devices_p004_legacy_backup;
DROP TABLE IF EXISTS sessions_p004_legacy_backup;

DROP INDEX IF EXISTS sessions_device_id_idx;
ALTER TABLE sessions DROP CONSTRAINT IF EXISTS sessions_device_user_fk;
ALTER TABLE sessions DROP COLUMN IF EXISTS device_id;
ALTER TABLE devices DROP CONSTRAINT IF EXISTS devices_id_user_key;

COMMIT;
