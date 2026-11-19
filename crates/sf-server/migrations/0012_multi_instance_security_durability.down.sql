-- Rollback Migration 0012: Multi-Instance Security Durability

ALTER TABLE community_template_submissions
    DROP COLUMN IF EXISTS pii_warnings;

ALTER TABLE devices
    DROP COLUMN IF EXISTS status,
    DROP COLUMN IF EXISTS session_generation,
    DROP COLUMN IF EXISTS active_session_id,
    DROP COLUMN IF EXISTS installation_id;

ALTER TABLE desktop_auth_codes
    DROP COLUMN IF EXISTS email,
    DROP COLUMN IF EXISTS device_name;

ALTER TABLE owner_web_sessions
    DROP COLUMN IF EXISTS last_step_up_at,
    DROP COLUMN IF EXISTS elevated_expires_at,
    DROP COLUMN IF EXISTS elevated_verified_at;
