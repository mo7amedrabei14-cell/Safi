-- 0004_auth_lookup_rls.up.sql
-- Permit the trusted authentication service to perform narrow account/key/session
-- lookups before it knows a workspace or user id. The marker is transaction-local
-- (set_config(..., true)) and is never derived from request headers or user input.
-- Workspace and user write checks remain enforced by their original identifiers.

BEGIN;

DROP POLICY memberships_workspace_isolation ON memberships;
CREATE POLICY memberships_workspace_isolation ON memberships
    USING (
        workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::UUID
        OR current_setting('app.auth_lookup', true) = 'true'
    )
    WITH CHECK (
        workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::UUID
        OR current_setting('app.auth_lookup', true) = 'true'
    );

DROP POLICY api_keys_workspace_isolation ON api_keys;
CREATE POLICY api_keys_workspace_isolation ON api_keys
    USING (
        workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::UUID
        OR current_setting('app.auth_lookup', true) = 'true'
    )
    WITH CHECK (
        workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::UUID
        OR current_setting('app.auth_lookup', true) = 'true'
    );

DROP POLICY sessions_workspace_isolation ON sessions;
CREATE POLICY sessions_workspace_isolation ON sessions
    USING (
        workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::UUID
        OR current_setting('app.auth_lookup', true) = 'true'
    )
    WITH CHECK (
        workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::UUID
        OR current_setting('app.auth_lookup', true) = 'true'
    );

DROP POLICY user_credentials_user_isolation ON user_credentials;
CREATE POLICY user_credentials_user_isolation ON user_credentials
    USING (
        user_id = NULLIF(current_setting('app.user_id', true), '')::UUID
        OR current_setting('app.auth_lookup', true) = 'true'
    )
    WITH CHECK (
        user_id = NULLIF(current_setting('app.user_id', true), '')::UUID
    );

-- Identifiers continue to use PostgreSQL's native uuidv7() generator.
COMMIT;
