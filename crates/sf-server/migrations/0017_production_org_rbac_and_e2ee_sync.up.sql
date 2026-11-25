-- Migration 0017: Production Organizations, 4-Role Server RBAC, Verified-Email Invitations,
-- X25519 Device Key Directory, Wrapped Workspace Key Epochs, and Durable E2EE Sync Protocol.
--
-- Non-destructively extends Migration 0009 tables (`organizations`, `organization_members`,
-- `organization_invitations`, `encrypted_sync_records`, `encrypted_share_links`) and adds
-- per-device public-key and wrapped-workspace-key tables with strict PostgreSQL 18 RLS.

-- 1. Extend `organizations` for monthly/annual per-seat plans, billing workspace linkage,
--    key rotation epochs, and soft-deletion.
ALTER TABLE organizations
    DROP CONSTRAINT IF EXISTS organizations_billing_cycle_check;

ALTER TABLE organizations
    ADD CONSTRAINT organizations_billing_cycle_check
    CHECK (billing_cycle IN ('monthly_per_seat', 'annual_per_seat'));

ALTER TABLE organizations
    ADD COLUMN IF NOT EXISTS billing_workspace_id UUID REFERENCES workspaces(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS active_key_epoch INTEGER NOT NULL DEFAULT 1 CHECK (active_key_epoch >= 1),
    ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_organizations_owner_active
    ON organizations (owner_user_id)
    WHERE deleted_at IS NULL;

-- 2. Extend `organization_members` with `updated_at` and membership lookup indexes.
ALTER TABLE organization_members
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT now();

CREATE INDEX IF NOT EXISTS idx_organization_members_user_active
    ON organization_members (user_id, organization_id)
    WHERE removed_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_organization_members_org_active
    ON organization_members (organization_id)
    WHERE removed_at IS NULL;

-- 3. Extend `organization_invitations` with `declined_at`, `accepted_by_user_id`, and
--    race-free uniqueness on pending invitations per `(organization_id, invited_email_norm)`.
ALTER TABLE organization_invitations
    ADD COLUMN IF NOT EXISTS declined_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS accepted_by_user_id UUID REFERENCES users(id) ON DELETE SET NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_org_invitations_pending_email_unique
    ON organization_invitations (organization_id, invited_email_norm)
    WHERE accepted_at IS NULL AND revoked_at IS NULL AND declined_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_org_invitations_email_lookup
    ON organization_invitations (invited_email_norm, created_at DESC);

-- 4. Extend `encrypted_sync_records` with monotonic workspace sequence cursor, key epoch,
--    deletion tombstones, origin device identifier, and idempotency replay fingerprinting.
ALTER TABLE encrypted_sync_records
    ADD COLUMN IF NOT EXISTS workspace_seq BIGINT NOT NULL DEFAULT 1 CHECK (workspace_seq >= 1),
    ADD COLUMN IF NOT EXISTS key_epoch INTEGER NOT NULL DEFAULT 1 CHECK (key_epoch >= 1),
    ADD COLUMN IF NOT EXISTS deleted BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS origin_device_id TEXT NOT NULL DEFAULT 'legacy-device',
    ADD COLUMN IF NOT EXISTS last_idempotency_key TEXT,
    ADD COLUMN IF NOT EXISTS last_request_fingerprint TEXT;

CREATE INDEX IF NOT EXISTS idx_encrypted_sync_records_ws_seq
    ON encrypted_sync_records (workspace_id, workspace_seq ASC);

-- 5. Per-device X25519 public key directory (zero private keys ever touch the server).
CREATE TABLE organization_member_public_keys (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    device_id TEXT NOT NULL CHECK (length(btrim(device_id)) BETWEEN 1 AND 128),
    device_label TEXT NOT NULL DEFAULT 'Desktop',
    x25519_public_key BYTEA NOT NULL CHECK (octet_length(x25519_public_key) = 32),
    key_fingerprint TEXT NOT NULL CHECK (length(btrim(key_fingerprint)) >= 16),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at TIMESTAMPTZ,
    UNIQUE (organization_id, user_id, device_id)
);

CREATE INDEX idx_org_member_public_keys_active
    ON organization_member_public_keys (organization_id, user_id)
    WHERE revoked_at IS NULL;

ALTER TABLE organization_member_public_keys ENABLE ROW LEVEL SECURITY;
ALTER TABLE organization_member_public_keys FORCE ROW LEVEL SECURITY;
CREATE POLICY organization_member_public_keys_isolation ON organization_member_public_keys
    USING (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid
        OR workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::uuid
    )
    WITH CHECK (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid
        OR workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::uuid
    );

-- 6. Per-epoch wrapped workspace keys (`X25519 + HKDF-SHA256 + ChaCha20-Poly1305` envelopes).
CREATE TABLE organization_wrapped_keys (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    key_epoch INTEGER NOT NULL CHECK (key_epoch >= 1),
    recipient_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    recipient_device_id TEXT NOT NULL CHECK (length(btrim(recipient_device_id)) BETWEEN 1 AND 128),
    sender_user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    sender_ephemeral_pub BYTEA NOT NULL CHECK (octet_length(sender_ephemeral_pub) = 32),
    wrapped_key_ciphertext BYTEA NOT NULL CHECK (octet_length(wrapped_key_ciphertext) = 48),
    wrapped_key_nonce BYTEA NOT NULL CHECK (octet_length(wrapped_key_nonce) = 12),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at TIMESTAMPTZ,
    UNIQUE (organization_id, key_epoch, recipient_user_id, recipient_device_id)
);

CREATE INDEX idx_org_wrapped_keys_recipient_active
    ON organization_wrapped_keys (organization_id, recipient_user_id, key_epoch DESC)
    WHERE revoked_at IS NULL;

ALTER TABLE organization_wrapped_keys ENABLE ROW LEVEL SECURITY;
ALTER TABLE organization_wrapped_keys FORCE ROW LEVEL SECURITY;
CREATE POLICY organization_wrapped_keys_isolation ON organization_wrapped_keys
    USING (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid
        OR workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::uuid
    )
    WITH CHECK (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid
        OR workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::uuid
    );

-- 7. Upgrade RLS policies on Migration 0009 tables (`USING` + `WITH CHECK` + non-recursive
--    membership/invitation discovery).
DROP POLICY IF EXISTS organization_members_tenant_isolation ON organization_members;
CREATE POLICY organization_members_tenant_isolation ON organization_members
    USING (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid
        OR workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::uuid
        OR user_id = NULLIF(current_setting('app.current_user_id', true), '')::uuid
    )
    WITH CHECK (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid
        OR workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::uuid
    );

DROP POLICY IF EXISTS organizations_tenant_isolation ON organizations;
CREATE POLICY organizations_tenant_isolation ON organizations
    USING (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid
        OR workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::uuid
        OR owner_user_id = NULLIF(current_setting('app.current_user_id', true), '')::uuid
        OR EXISTS (
            SELECT 1
            FROM organization_members om
            WHERE om.organization_id = organizations.id
              AND om.user_id = NULLIF(current_setting('app.current_user_id', true), '')::uuid
              AND om.removed_at IS NULL
        )
    )
    WITH CHECK (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid
        OR workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::uuid
    );

DROP POLICY IF EXISTS organization_invitations_tenant_isolation ON organization_invitations;
CREATE POLICY organization_invitations_tenant_isolation ON organization_invitations
    USING (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid
        OR workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::uuid
        OR invited_email_norm = NULLIF(current_setting('app.current_user_email', true), '')
        OR token_hash = NULLIF(current_setting('app.invitation_token_hash', true), '')
    )
    WITH CHECK (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid
        OR workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::uuid
        OR invited_email_norm = NULLIF(current_setting('app.current_user_email', true), '')
    );

DROP POLICY IF EXISTS encrypted_sync_records_tenant_isolation ON encrypted_sync_records;
CREATE POLICY encrypted_sync_records_tenant_isolation ON encrypted_sync_records
    USING (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid
        OR workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::uuid
    )
    WITH CHECK (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid
        OR workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::uuid
    );

DROP POLICY IF EXISTS encrypted_share_links_tenant_isolation ON encrypted_share_links;
CREATE POLICY encrypted_share_links_tenant_isolation ON encrypted_share_links
    USING (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid
        OR workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::uuid
        OR token_hash = NULLIF(current_setting('app.share_link_token_hash', true), '')
    )
    WITH CHECK (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid
        OR workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::uuid
    );

-- Grant table privileges to `safi_p0_01_app` when present (for live restricted-role migration tests).
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'safi_p0_01_app') THEN
        GRANT SELECT, INSERT, UPDATE, DELETE ON
            organizations,
            organization_members,
            organization_invitations,
            encrypted_sync_records,
            encrypted_share_links,
            organization_member_public_keys,
            organization_wrapped_keys
        TO safi_p0_01_app;
    END IF;
END
$$;
