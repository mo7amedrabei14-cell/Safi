-- Migration 0009: Organizations (min 3 seats), encrypted client-side sync,
-- encrypted share links, and Ed25519-signed Desktop release manifests.

CREATE TABLE organizations (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    workspace_id UUID NOT NULL UNIQUE REFERENCES workspaces(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    min_seats INTEGER NOT NULL DEFAULT 3 CHECK (min_seats >= 3),
    purchased_seats INTEGER NOT NULL DEFAULT 3 CHECK (purchased_seats >= 3),
    billing_cycle TEXT NOT NULL DEFAULT 'annual_per_seat' CHECK (billing_cycle IN ('annual_per_seat')),
    owner_user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE organizations ENABLE ROW LEVEL SECURITY;
ALTER TABLE organizations FORCE ROW LEVEL SECURITY;
CREATE POLICY organizations_tenant_isolation ON organizations
    USING (workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid);

CREATE TABLE organization_members (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('owner', 'admin', 'member', 'viewer')),
    invited_by UUID REFERENCES users(id) ON DELETE SET NULL,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    removed_at TIMESTAMPTZ,
    UNIQUE (organization_id, user_id)
);

ALTER TABLE organization_members ENABLE ROW LEVEL SECURITY;
ALTER TABLE organization_members FORCE ROW LEVEL SECURITY;
CREATE POLICY organization_members_tenant_isolation ON organization_members
    USING (workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid);

CREATE TABLE organization_invitations (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    invited_email_norm TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('admin', 'member', 'viewer')),
    token_hash TEXT NOT NULL UNIQUE,
    invited_by UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at TIMESTAMPTZ NOT NULL,
    accepted_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE organization_invitations ENABLE ROW LEVEL SECURITY;
ALTER TABLE organization_invitations FORCE ROW LEVEL SECURITY;
CREATE POLICY organization_invitations_tenant_isolation ON organization_invitations
    USING (workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid);

CREATE TABLE encrypted_sync_records (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    resource_kind TEXT NOT NULL CHECK (resource_kind IN ('project', 'recipe', 'dashboard', 'presentation', 'document')),
    resource_id TEXT NOT NULL,
    version BIGINT NOT NULL CHECK (version >= 1),
    ciphertext BYTEA NOT NULL,
    nonce BYTEA NOT NULL,
    key_id TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (workspace_id, resource_kind, resource_id)
);

ALTER TABLE encrypted_sync_records ENABLE ROW LEVEL SECURITY;
ALTER TABLE encrypted_sync_records FORCE ROW LEVEL SECURITY;
CREATE POLICY encrypted_sync_records_tenant_isolation ON encrypted_sync_records
    USING (workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid);

CREATE TABLE encrypted_share_links (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    created_by UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    resource_id TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    wrapped_key_ciphertext BYTEA NOT NULL,
    wrapped_key_nonce BYTEA NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE encrypted_share_links ENABLE ROW LEVEL SECURITY;
ALTER TABLE encrypted_share_links FORCE ROW LEVEL SECURITY;
CREATE POLICY encrypted_share_links_tenant_isolation ON encrypted_share_links
    USING (workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid);

CREATE TABLE signed_releases (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    version BIGINT NOT NULL CHECK (version >= 1),
    channel TEXT NOT NULL CHECK (channel IN ('stable', 'beta')),
    platform TEXT NOT NULL,
    min_required_version BIGINT NOT NULL CHECK (min_required_version >= 1),
    artifact_sha256 TEXT NOT NULL,
    signature_hex TEXT NOT NULL,
    is_mandatory BOOLEAN NOT NULL DEFAULT FALSE,
    release_notes TEXT NOT NULL DEFAULT '',
    published_by UUID REFERENCES users(id) ON DELETE SET NULL,
    published_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (channel, platform, version)
);
