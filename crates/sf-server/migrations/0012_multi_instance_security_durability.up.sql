-- Migration 0012: Multi-Instance Security Durability for PostgreSQL 18 Source of Truth
-- Ensures Owner Web TOTP elevation, Desktop PKCE metadata, Device Authority state,
-- and Community Submission scanner warnings are read & written directly from PostgreSQL
-- across restarts and multiple backend instances.

ALTER TABLE owner_web_sessions
    ADD COLUMN IF NOT EXISTS elevated_verified_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS elevated_expires_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS last_step_up_at TIMESTAMPTZ;

ALTER TABLE desktop_auth_codes
    ADD COLUMN IF NOT EXISTS device_name TEXT NOT NULL DEFAULT 'Desktop',
    ADD COLUMN IF NOT EXISTS email TEXT;

ALTER TABLE devices
    ADD COLUMN IF NOT EXISTS installation_id UUID,
    ADD COLUMN IF NOT EXISTS active_session_id UUID,
    ADD COLUMN IF NOT EXISTS session_generation BIGINT NOT NULL DEFAULT 1 CHECK (session_generation >= 1),
    ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'active' CHECK (
        status IN ('active', 'replaced_by_new_device', 'revoked_by_owner', 'signed_out')
    );

ALTER TABLE community_template_submissions
    ADD COLUMN IF NOT EXISTS pii_warnings JSONB NOT NULL DEFAULT '[]'::jsonb;
