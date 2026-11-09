-- 0004_auth_lookup_rls.down.sql
-- Remove only the SELECT-only authentication lookup policies.

BEGIN;

DROP POLICY user_credentials_auth_lookup_read ON user_credentials;
DROP POLICY sessions_auth_lookup_read ON sessions;
DROP POLICY api_keys_auth_lookup_read ON api_keys;
DROP POLICY memberships_auth_lookup_read ON memberships;

COMMIT;
