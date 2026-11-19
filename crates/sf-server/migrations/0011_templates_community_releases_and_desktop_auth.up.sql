-- Migration 0011: Dual-Database Central Control Plane Extensions
-- Adds durable PostgreSQL 18 tables for:
-- 1. Desktop Browser PKCE Authorization Codes (`desktop_auth_codes`)
-- 2. Dedicated Owner Web Sessions (`owner_web_sessions`) separate from Desktop Device sessions
-- 3. Official Versioned Template Library (`official_templates`, `official_template_versions`)
-- 4. Community Template Submissions (`community_template_submissions`, RLS-protected)
-- 5. Signed Desktop Application Releases (`app_releases`)

CREATE TABLE IF NOT EXISTS desktop_auth_codes (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    request_id UUID NOT NULL UNIQUE,
    flow TEXT NOT NULL CHECK (flow IN ('signin', 'signup')),
    code_challenge TEXT NOT NULL,
    code_challenge_method TEXT NOT NULL DEFAULT 'S256' CHECK (code_challenge_method = 'S256'),
    state_hash BYTEA NOT NULL,
    installation_id UUID NOT NULL,
    platform TEXT NOT NULL,
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    workspace_id UUID REFERENCES workspaces(id) ON DELETE CASCADE,
    code_hash BYTEA UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_desktop_auth_codes_request ON desktop_auth_codes (request_id);
CREATE INDEX IF NOT EXISTS idx_desktop_auth_codes_expires ON desktop_auth_codes (expires_at);

ALTER TABLE desktop_auth_codes ENABLE ROW LEVEL SECURITY;
ALTER TABLE desktop_auth_codes FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS desktop_auth_codes_isolation ON desktop_auth_codes;
CREATE POLICY desktop_auth_codes_isolation ON desktop_auth_codes
    USING (
        current_setting('app.auth_lookup', true) = 'true'
        OR workspace_id::text = current_setting('app.current_workspace_id', true)
    )
    WITH CHECK (
        current_setting('app.auth_lookup', true) = 'true'
        OR workspace_id::text = current_setting('app.current_workspace_id', true)
    );

CREATE TABLE IF NOT EXISTS owner_web_sessions (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    token_hash BYTEA NOT NULL UNIQUE,
    user_agent_hint TEXT NOT NULL DEFAULT 'owner-web',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_owner_web_sessions_user ON owner_web_sessions (user_id, revoked_at);

ALTER TABLE owner_web_sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE owner_web_sessions FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS owner_web_sessions_isolation ON owner_web_sessions;
CREATE POLICY owner_web_sessions_isolation ON owner_web_sessions
    USING (
        current_setting('app.auth_lookup', true) = 'true'
        OR current_setting('app.owner_override', true) = 'true'
        OR workspace_id::text = current_setting('app.current_workspace_id', true)
    )
    WITH CHECK (
        current_setting('app.auth_lookup', true) = 'true'
        OR current_setting('app.owner_override', true) = 'true'
        OR workspace_id::text = current_setting('app.current_workspace_id', true)
    );

CREATE TABLE IF NOT EXISTS official_templates (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    slug TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    category TEXT NOT NULL DEFAULT 'general',
    recipe_type TEXT NOT NULL DEFAULT 'cleaning',
    active BOOLEAN NOT NULL DEFAULT TRUE,
    current_version INTEGER NOT NULL DEFAULT 1,
    requires_paid_plan BOOLEAN NOT NULL DEFAULT FALSE,
    source_submission_id UUID,
    published_by UUID REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS official_template_versions (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    template_id UUID NOT NULL REFERENCES official_templates(id) ON DELETE CASCADE,
    version_number INTEGER NOT NULL CHECK (version_number >= 1),
    schema_version INTEGER NOT NULL DEFAULT 1,
    compatible_app_version TEXT NOT NULL DEFAULT '1.0.0',
    release_notes TEXT NOT NULL DEFAULT '',
    required_columns JSONB NOT NULL DEFAULT '[]'::jsonb,
    steps_json JSONB NOT NULL DEFAULT '[]'::jsonb,
    content_hash TEXT NOT NULL,
    published_by UUID REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (template_id, version_number)
);

CREATE TABLE IF NOT EXISTS community_template_submissions (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    contributor_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    recipe_type TEXT NOT NULL DEFAULT 'cleaning',
    schema_version INTEGER NOT NULL DEFAULT 1,
    app_version TEXT NOT NULL DEFAULT '1.0.0',
    required_columns JSONB NOT NULL DEFAULT '[]'::jsonb,
    steps_json JSONB NOT NULL DEFAULT '[]'::jsonb,
    content_hash TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (
        status IN ('pending', 'under_review', 'needs_changes', 'approved', 'rejected', 'published')
    ),
    reviewer_notes TEXT NOT NULL DEFAULT '',
    reviewed_by UUID REFERENCES users(id),
    consent_accepted BOOLEAN NOT NULL CHECK (consent_accepted = TRUE),
    consent_terms_version TEXT NOT NULL,
    consent_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    show_contributor_attribution BOOLEAN NOT NULL DEFAULT FALSE,
    published_template_id UUID REFERENCES official_templates(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_community_submissions_status
    ON community_template_submissions (status, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_community_submissions_user_month
    ON community_template_submissions (contributor_user_id, created_at DESC);

ALTER TABLE community_template_submissions ENABLE ROW LEVEL SECURITY;
ALTER TABLE community_template_submissions FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS community_submissions_workspace_isolation ON community_template_submissions;
CREATE POLICY community_submissions_workspace_isolation ON community_template_submissions
    USING (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id::text = current_setting('app.current_workspace_id', true)
    )
    WITH CHECK (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id::text = current_setting('app.current_workspace_id', true)
    );

CREATE TABLE IF NOT EXISTS app_releases (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    version_code BIGINT NOT NULL,
    version_name TEXT NOT NULL,
    channel TEXT NOT NULL DEFAULT 'stable' CHECK (channel IN ('stable', 'beta')),
    platform TEXT NOT NULL,
    download_url TEXT NOT NULL,
    sha256_hex TEXT NOT NULL,
    file_size_bytes BIGINT NOT NULL CHECK (file_size_bytes > 0),
    min_required_version BIGINT NOT NULL DEFAULT 100,
    release_notes TEXT NOT NULL DEFAULT '',
    signature_hex TEXT NOT NULL DEFAULT '',
    is_mandatory BOOLEAN NOT NULL DEFAULT FALSE,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    published_by UUID REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (channel, platform, version_code)
);
