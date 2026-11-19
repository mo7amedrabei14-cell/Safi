-- Rollback Migration 0008: drop billing invoices, credits, usage ledger, monthly counters,
-- friend VIP grants, trial claims, OAuth links, phone verifications, and restore provider checks.

DROP TABLE IF EXISTS billing_invoices;
DROP TABLE IF EXISTS credit_balances;
DROP TABLE IF EXISTS usage_ledger;
DROP TABLE IF EXISTS monthly_usage_counters;
DROP TABLE IF EXISTS friend_vip_grants;
DROP TABLE IF EXISTS trial_claims;

ALTER TABLE billing_webhook_events DROP CONSTRAINT IF EXISTS billing_webhook_events_provider_check;
ALTER TABLE billing_webhook_events
    ADD CONSTRAINT billing_webhook_events_provider_check
    CHECK (provider IN ('paymob', 'lemon_squeezy'));

ALTER TABLE subscriptions DROP CONSTRAINT IF EXISTS subscriptions_provider_check;
ALTER TABLE subscriptions
    ADD CONSTRAINT subscriptions_provider_check
    CHECK (provider IN ('paymob', 'lemon_squeezy'));

DROP TABLE IF EXISTS oauth_identities;
DROP TABLE IF EXISTS phone_verifications;
DROP INDEX IF EXISTS idx_users_verified_phone_unique;
ALTER TABLE users
    DROP COLUMN IF EXISTS phone_verified_at,
    DROP COLUMN IF EXISTS verified_phone;
