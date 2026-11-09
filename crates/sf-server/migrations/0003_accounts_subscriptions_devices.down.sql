-- 0003_accounts_subscriptions_devices.down.sql
-- Revert the account/subscription/device schema introduced by migration 0003.

BEGIN;

DROP TABLE IF EXISTS billing_webhook_events;
DROP TABLE IF EXISTS subscriptions;
DROP TABLE IF EXISTS devices;
DROP TABLE IF EXISTS plans;
DROP TABLE IF EXISTS password_reset_tokens;
DROP TABLE IF EXISTS email_verification_tokens;
DROP TABLE IF EXISTS user_credentials;

ALTER TABLE users
    DROP COLUMN IF EXISTS email_verified_at;

COMMIT;
