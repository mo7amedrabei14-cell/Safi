-- 0004_auth_lookup_rls.up.sql
-- Permit the trusted authentication service to resolve credentials before it
-- knows the caller's workspace/user id. This adds SELECT-only policies; the
-- original FOR ALL policies continue to enforce write isolation.
-- The marker is transaction-local and is never derived from request input.

BEGIN;

CREATE POLICY memberships_auth_lookup_read ON memberships
    FOR SELECT
    USING (current_setting('app.auth_lookup', true) = 'true');

CREATE POLICY api_keys_auth_lookup_read ON api_keys
    FOR SELECT
    USING (current_setting('app.auth_lookup', true) = 'true');

CREATE POLICY sessions_auth_lookup_read ON sessions
    FOR SELECT
    USING (current_setting('app.auth_lookup', true) = 'true');

CREATE POLICY user_credentials_auth_lookup_read ON user_credentials
    FOR SELECT
    USING (current_setting('app.auth_lookup', true) = 'true');

-- Identifiers continue to use PostgreSQL's native uuidv7() generator.
COMMIT;
