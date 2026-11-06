-- 0001_s4_core.up.sql — SafiData S4 core schema (Section 9.2, 9.3).
-- PostgreSQL 18 native: uuidv7(), virtual generated columns, native RLS, pg_partman.
-- No hosted-platform dependency: everything below runs on a plain PostgreSQL 18 instance.

BEGIN;

CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    email TEXT NOT NULL,
    display_name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ,
    CONSTRAINT users_email_not_blank CHECK (length(btrim(email)) > 3)
);

CREATE UNIQUE INDEX users_email_active_key ON users (lower(email)) WHERE deleted_at IS NULL;

CREATE TABLE workspaces (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    name TEXT NOT NULL,
    plan_code TEXT NOT NULL DEFAULT 'free',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ,
    CONSTRAINT workspaces_name_not_blank CHECK (length(btrim(name)) > 0)
);

CREATE TABLE memberships (
    workspace_id UUID NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, user_id),
    CONSTRAINT memberships_role_valid CHECK (role IN ('owner', 'admin', 'member'))
);

CREATE TABLE api_keys (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    workspace_id UUID NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    created_by UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    key_prefix TEXT NOT NULL,
    secret_hash BYTEA NOT NULL,
    label TEXT NOT NULL,
    scopes TEXT[] NOT NULL DEFAULT ARRAY['read']::TEXT[],
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    -- PostgreSQL 18 virtual generated column: never stored, always derived.
    is_active BOOLEAN GENERATED ALWAYS AS (revoked_at IS NULL) VIRTUAL,
    CONSTRAINT api_keys_prefix_hex CHECK (key_prefix ~ '^[0-9a-f]{16}$'),
    CONSTRAINT api_keys_hash_size CHECK (octet_length(secret_hash) = 32),
    CONSTRAINT api_keys_scopes_known CHECK (
        scopes <@ ARRAY['read', 'write', 'admin']::TEXT[]
    )
);

CREATE UNIQUE INDEX api_keys_prefix_key ON api_keys (key_prefix);
CREATE INDEX api_keys_workspace_idx ON api_keys (workspace_id);

CREATE TABLE sessions (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    workspace_id UUID NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    api_key_id UUID REFERENCES api_keys (id) ON DELETE SET NULL,
    token_hash BYTEA NOT NULL,
    device_name TEXT NOT NULL,
    scopes TEXT[] NOT NULL DEFAULT ARRAY['read']::TEXT[],
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    CONSTRAINT sessions_hash_size CHECK (octet_length(token_hash) = 32),
    CONSTRAINT sessions_device_not_blank CHECK (length(btrim(device_name)) > 0),
    CONSTRAINT sessions_scopes_known CHECK (
        scopes <@ ARRAY['read', 'write', 'admin']::TEXT[]
    )
);

CREATE UNIQUE INDEX sessions_token_hash_key ON sessions (token_hash);
CREATE INDEX sessions_workspace_expiry_idx ON sessions (workspace_id, expires_at);

-- Append-only audit trail, range partitioned monthly by created_at.
CREATE TABLE audit_log (
    id UUID NOT NULL DEFAULT uuidv7(),
    workspace_id UUID NOT NULL,
    actor_user_id UUID,
    action TEXT NOT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Virtual generated column: cheap action grouping for owner-console queries.
    action_domain TEXT GENERATED ALWAYS AS (split_part(action, '.', 1)) VIRTUAL,
    PRIMARY KEY (id, created_at)
) PARTITION BY RANGE (created_at);

-- pg_partman is optional: enable monthly partition management only when installed.
DO $do$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_available_extensions WHERE name = 'pg_partman') THEN
        CREATE EXTENSION IF NOT EXISTS pg_partman SCHEMA partman;
        PERFORM partman.create_parent(
            p_parent_table := 'public.audit_log',
            p_control := 'created_at',
            p_interval := '1 month',
            p_type := 'native'
        );
        UPDATE partman.part_config
        SET infinite_time_partitions = true,
            retention = NULL
        WHERE parent_table = 'public.audit_log';
    END IF;
END
$do$;

CREATE TABLE IF NOT EXISTS audit_log_default PARTITION OF audit_log DEFAULT;

-- Common updated_at trigger for mutable identity rows.
CREATE FUNCTION touch_updated_at() RETURNS TRIGGER
LANGUAGE plpgsql
AS $fn$
BEGIN
    NEW.updated_at = now();
    RETURN NEW;
END
$fn$;

CREATE TRIGGER users_touch_updated_at
    BEFORE UPDATE ON users
    FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

CREATE TRIGGER workspaces_touch_updated_at
    BEFORE UPDATE ON workspaces
    FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

-- Native row-level security: every workspace-owned row is visible only under
-- the workspace identifier set by the connection (`SET LOCAL app.workspace_id`).
ALTER TABLE memberships ENABLE ROW LEVEL SECURITY;
ALTER TABLE memberships FORCE ROW LEVEL SECURITY;
CREATE POLICY memberships_workspace_isolation ON memberships
    USING (workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::UUID);

ALTER TABLE api_keys ENABLE ROW LEVEL SECURITY;
ALTER TABLE api_keys FORCE ROW LEVEL SECURITY;
CREATE POLICY api_keys_workspace_isolation ON api_keys
    USING (workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::UUID);

ALTER TABLE sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE sessions FORCE ROW LEVEL SECURITY;
CREATE POLICY sessions_workspace_isolation ON sessions
    USING (workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::UUID);

ALTER TABLE audit_log ENABLE ROW LEVEL SECURITY;
ALTER TABLE audit_log FORCE ROW LEVEL SECURITY;
CREATE POLICY audit_log_workspace_isolation ON audit_log
    USING (workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::UUID);

COMMIT;
