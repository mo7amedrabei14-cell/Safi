-- Migration 0007: Sealed platform owner identity, RFC 6238 TOTP MFA, recovery codes,
-- elevated sessions, hash-chained audit log, feature flags, and system settings.

CREATE TABLE platform_owner (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton = TRUE),
    owner_user_id UUID NOT NULL UNIQUE REFERENCES users(id) ON DELETE RESTRICT,
    bootstrap_sealed BOOLEAN NOT NULL DEFAULT TRUE CHECK (bootstrap_sealed = TRUE),
    bootstrapped_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE owner_totp_credentials (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    secret_ciphertext BYTEA NOT NULL,
    secret_nonce BYTEA NOT NULL,
    key_version INTEGER NOT NULL DEFAULT 1 CHECK (key_version >= 1),
    confirmed BOOLEAN NOT NULL DEFAULT FALSE,
    last_accepted_step BIGINT,
    failed_attempts INTEGER NOT NULL DEFAULT 0 CHECK (failed_attempts >= 0),
    locked_until TIMESTAMPTZ,
    enrolled_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    confirmed_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE owner_recovery_codes (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code_hash TEXT NOT NULL UNIQUE,
    used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_owner_recovery_codes_user_unused
    ON owner_recovery_codes(user_id)
    WHERE used_at IS NULL;

CREATE TABLE owner_elevated_sessions (
    session_id UUID PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    verified_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    last_step_up_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_owner_elevated_sessions_user
    ON owner_elevated_sessions(user_id, expires_at DESC);

CREATE TABLE owner_audit_log (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    seq BIGINT GENERATED ALWAYS AS IDENTITY UNIQUE,
    actor_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    action TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id TEXT NOT NULL,
    outcome TEXT NOT NULL CHECK (outcome IN ('allowed', 'denied', 'error')),
    before_state JSONB,
    after_state JSONB,
    previous_hash TEXT NOT NULL,
    current_hash TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_owner_audit_log_created_at
    ON owner_audit_log(created_at DESC);

CREATE TABLE feature_flags (
    flag_key TEXT PRIMARY KEY,
    enabled BOOLEAN NOT NULL DEFAULT FALSE,
    description TEXT NOT NULL DEFAULT '',
    updated_by UUID REFERENCES users(id) ON DELETE SET NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE system_settings (
    setting_key TEXT PRIMARY KEY,
    setting_value JSONB NOT NULL,
    updated_by UUID REFERENCES users(id) ON DELETE SET NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
