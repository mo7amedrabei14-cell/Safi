-- 0004_auth_lookup_rls.down.sql
-- Restore the exact workspace/user-only RLS policies from migrations 0001 and 0003.

BEGIN;

DROP POLICY memberships_workspace_isolation ON memberships;
CREATE POLICY memberships_workspace_isolation ON memberships
    USING (workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::UUID);

DROP POLICY api_keys_workspace_isolation ON api_keys;
CREATE POLICY api_keys_workspace_isolation ON api_keys
    USING (workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::UUID);

DROP POLICY sessions_workspace_isolation ON sessions;
CREATE POLICY sessions_workspace_isolation ON sessions
    USING (workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::UUID);

DROP POLICY user_credentials_user_isolation ON user_credentials;
CREATE POLICY user_credentials_user_isolation ON user_credentials
    USING (user_id = NULLIF(current_setting('app.user_id', true), '')::UUID)
    WITH CHECK (user_id = NULLIF(current_setting('app.user_id', true), '')::UUID);

COMMIT;
