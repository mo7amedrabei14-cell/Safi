-- 0002_one_active_session_per_user.down.sql
-- Revert the uniqueness rule and restore only the legacy sessions this
-- migration revoked, provided they have not been changed since.

BEGIN;

DROP INDEX IF EXISTS sessions_one_active_session_per_user_key;

UPDATE sessions AS target
SET revoked_at = backup.previous_revoked_at
FROM session_enforcement_legacy_backup AS backup
WHERE target.id = backup.session_id
  AND target.revoked_at = backup.migration_revoked_at;

DROP TABLE IF EXISTS session_enforcement_legacy_backup;

COMMIT;
