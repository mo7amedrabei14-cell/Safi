-- Rollback Migration 0008: preserve billing history and fail explicitly if incompatible
-- provider rows exist rather than silently deleting or rewriting payment records.

DO $$
DECLARE
    incompatible_subs INTEGER;
    incompatible_events INTEGER;
BEGIN
    SELECT count(*) INTO incompatible_subs
    FROM subscriptions
    WHERE provider NOT IN ('manual', 'paymob', 'lemon_squeezy');
    IF incompatible_subs > 0 THEN
        RAISE EXCEPTION 'Cannot safely roll back migration 0008: % row(s) in subscriptions use provider values outside (manual, paymob, lemon_squeezy). Archive or migrate Paddle subscription history before downgrading.', incompatible_subs;
    END IF;

    SELECT count(*) INTO incompatible_events
    FROM billing_webhook_events
    WHERE provider NOT IN ('paymob', 'lemon_squeezy');
    IF incompatible_events > 0 THEN
        RAISE EXCEPTION 'Cannot safely roll back migration 0008: % row(s) in billing_webhook_events use provider values outside (paymob, lemon_squeezy). Archive or migrate Paddle/manual webhook history before downgrading.', incompatible_events;
    END IF;
END
$$;

DROP TABLE IF EXISTS billing_invoices;
DROP TABLE IF EXISTS credit_balances;
DROP TABLE IF EXISTS usage_ledger;
DROP TABLE IF EXISTS monthly_usage_counters;
DROP TABLE IF EXISTS friend_vip_grants;
DROP TABLE IF EXISTS trial_claims;

ALTER TABLE billing_webhook_events DROP CONSTRAINT IF EXISTS billing_webhook_provider_valid;
ALTER TABLE billing_webhook_events DROP CONSTRAINT IF EXISTS billing_webhook_events_provider_check;
ALTER TABLE billing_webhook_events
    ADD CONSTRAINT billing_webhook_provider_valid
    CHECK (provider IN ('paymob', 'lemon_squeezy'));

ALTER TABLE subscriptions DROP CONSTRAINT IF EXISTS subscriptions_provider_valid;
ALTER TABLE subscriptions DROP CONSTRAINT IF EXISTS subscriptions_provider_check;
ALTER TABLE subscriptions
    ADD CONSTRAINT subscriptions_provider_valid
    CHECK (provider IN ('manual', 'paymob', 'lemon_squeezy'));

DROP TABLE IF EXISTS oauth_identities;
DROP TABLE IF EXISTS phone_verifications;
DROP INDEX IF EXISTS idx_users_verified_phone_unique;
ALTER TABLE users
    DROP COLUMN IF EXISTS phone_verified_at,
    DROP COLUMN IF EXISTS verified_phone;
