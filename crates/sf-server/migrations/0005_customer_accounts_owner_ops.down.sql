-- 0005_customer_accounts_owner_ops.down.sql
-- Revert Phase 0.1 additions. Paid plans are removed only when no subscription
-- still references them, so a rollback never deletes customer history.

BEGIN;

DROP TABLE IF EXISTS subscription_events;
DROP TABLE IF EXISTS plan_prices;

DROP POLICY IF EXISTS devices_owner_read ON devices;
DROP POLICY IF EXISTS subscriptions_owner_read ON subscriptions;
DROP POLICY IF EXISTS memberships_owner_read ON memberships;

DELETE FROM plans
WHERE code IN ('individual_monthly', 'individual_annual', 'organization_seat')
  AND NOT EXISTS (
      SELECT 1
      FROM subscriptions
      WHERE subscriptions.plan_code = plans.code
  );

ALTER TABLE users
    DROP COLUMN IF EXISTS suspended_at;

COMMIT;
