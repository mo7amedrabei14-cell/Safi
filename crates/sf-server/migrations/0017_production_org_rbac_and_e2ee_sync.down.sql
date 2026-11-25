-- Rollback Migration 0017: Restore Migration 0009 Organization & Sync schema cleanly.

DROP POLICY IF EXISTS encrypted_share_links_tenant_isolation ON encrypted_share_links;
CREATE POLICY encrypted_share_links_tenant_isolation ON encrypted_share_links
    USING (workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid);

DROP POLICY IF EXISTS encrypted_sync_records_tenant_isolation ON encrypted_sync_records;
CREATE POLICY encrypted_sync_records_tenant_isolation ON encrypted_sync_records
    USING (workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid);

DROP POLICY IF EXISTS organization_invitations_tenant_isolation ON organization_invitations;
CREATE POLICY organization_invitations_tenant_isolation ON organization_invitations
    USING (workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid);

DROP POLICY IF EXISTS organizations_tenant_isolation ON organizations;
CREATE POLICY organizations_tenant_isolation ON organizations
    USING (workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid);

DROP POLICY IF EXISTS organization_members_tenant_isolation ON organization_members;
CREATE POLICY organization_members_tenant_isolation ON organization_members
    USING (workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid);

DROP TABLE IF EXISTS organization_wrapped_keys;
DROP TABLE IF EXISTS organization_member_public_keys;

DROP INDEX IF EXISTS idx_encrypted_sync_records_ws_seq;
ALTER TABLE encrypted_sync_records
    DROP COLUMN IF EXISTS last_request_fingerprint,
    DROP COLUMN IF EXISTS last_idempotency_key,
    DROP COLUMN IF EXISTS origin_device_id,
    DROP COLUMN IF EXISTS deleted,
    DROP COLUMN IF EXISTS key_epoch,
    DROP COLUMN IF EXISTS workspace_seq;

DROP INDEX IF EXISTS idx_org_invitations_email_lookup;
DROP INDEX IF EXISTS idx_org_invitations_pending_email_unique;
ALTER TABLE organization_invitations
    DROP COLUMN IF EXISTS accepted_by_user_id,
    DROP COLUMN IF EXISTS declined_at;

DROP INDEX IF EXISTS idx_organization_members_org_active;
DROP INDEX IF EXISTS idx_organization_members_user_active;
ALTER TABLE organization_members
    DROP COLUMN IF EXISTS updated_at;

DROP INDEX IF EXISTS idx_organizations_owner_active;
ALTER TABLE organizations
    DROP COLUMN IF EXISTS deleted_at,
    DROP COLUMN IF EXISTS active_key_epoch,
    DROP COLUMN IF EXISTS billing_workspace_id;

UPDATE organizations SET billing_cycle = 'annual_per_seat' WHERE billing_cycle <> 'annual_per_seat';
ALTER TABLE organizations
    DROP CONSTRAINT IF EXISTS organizations_billing_cycle_check;
ALTER TABLE organizations
    ADD CONSTRAINT organizations_billing_cycle_check
    CHECK (billing_cycle IN ('annual_per_seat'));
