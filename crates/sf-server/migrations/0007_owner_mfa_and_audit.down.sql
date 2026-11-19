-- Rollback Migration 0007: drop owner MFA, audit log, feature flags, and system settings.

DROP TABLE IF EXISTS system_settings;
DROP TABLE IF EXISTS feature_flags;
DROP TABLE IF EXISTS owner_audit_log;
DROP TABLE IF EXISTS owner_elevated_sessions;
DROP TABLE IF EXISTS owner_recovery_codes;
DROP TABLE IF EXISTS owner_totp_credentials;
DROP TABLE IF EXISTS platform_owner;
