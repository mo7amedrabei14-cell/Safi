-- Rollback Migration 0009: drop signed releases, encrypted share links,
-- encrypted sync records, organization invitations, members, and organizations.

DROP TABLE IF EXISTS signed_releases;
DROP TABLE IF EXISTS encrypted_share_links;
DROP TABLE IF EXISTS encrypted_sync_records;
DROP TABLE IF EXISTS organization_invitations;
DROP TABLE IF EXISTS organization_members;
DROP TABLE IF EXISTS organizations;
