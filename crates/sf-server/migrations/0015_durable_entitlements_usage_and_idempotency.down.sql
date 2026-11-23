-- Rollback Migration 0015: Remove durable entitlement usage, idempotency, and trial anti-abuse tables.

BEGIN;

DROP TABLE IF EXISTS trial_anti_abuse_claims;
DROP TABLE IF EXISTS entitlement_usage_idempotency;
DROP TABLE IF EXISTS workspace_monthly_usage;

COMMIT;
