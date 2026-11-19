-- Revert Migration 0011
DROP TABLE IF EXISTS app_releases;
DROP POLICY IF EXISTS community_submissions_workspace_isolation ON community_template_submissions;
DROP TABLE IF EXISTS community_template_submissions;
DROP TABLE IF EXISTS official_template_versions;
DROP TABLE IF EXISTS official_templates;
DROP POLICY IF EXISTS owner_web_sessions_isolation ON owner_web_sessions;
DROP TABLE IF EXISTS owner_web_sessions;
DROP POLICY IF EXISTS desktop_auth_codes_isolation ON desktop_auth_codes;
DROP TABLE IF EXISTS desktop_auth_codes;
