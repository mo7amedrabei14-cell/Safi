-- Migration 0014 Rollback: Restore Migration 0013 constraints safely (refusing silent data loss).

BEGIN;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM billing_entitlement_grants
        WHERE order_id IS NULL OR grant_kind = 'subscription_renewal'
    ) THEN
        RAISE EXCEPTION 'Cannot roll back migration 0014 while recurring subscription_renewal grant rows exist; archive or migrate them first.';
    END IF;
    IF EXISTS (
        SELECT 1 FROM billing_refunds WHERE status = 'reconciliation_required'
    ) THEN
        RAISE EXCEPTION 'Cannot roll back migration 0014 while reconciliation_required refund rows exist; resolve them first.';
    END IF;
END
$$;

DROP TABLE IF EXISTS billing_security_anomalies;

ALTER TABLE billing_refunds
    DROP CONSTRAINT IF EXISTS billing_refunds_status_check;
ALTER TABLE billing_refunds
    ADD CONSTRAINT billing_refunds_status_check
    CHECK (status IN ('requested', 'confirmed', 'failed'));

DROP INDEX IF EXISTS billing_entitlement_grants_renewal_period_uidx;
DROP INDEX IF EXISTS billing_entitlement_grants_initial_order_uidx;

ALTER TABLE billing_entitlement_grants
    DROP CONSTRAINT IF EXISTS billing_entitlement_grants_grant_kind_check;
ALTER TABLE billing_entitlement_grants
    ADD CONSTRAINT billing_entitlement_grants_grant_kind_check
    CHECK (grant_kind IN ('subscription', 'credit_pack_25', 'pay_per_file'));

ALTER TABLE billing_entitlement_grants
    DROP COLUMN IF EXISTS period_start,
    DROP COLUMN IF EXISTS provider_subscription_id;

ALTER TABLE billing_entitlement_grants
    ALTER COLUMN order_id SET NOT NULL;

ALTER TABLE billing_entitlement_grants
    ADD CONSTRAINT billing_entitlement_grants_order_id_grant_kind_key UNIQUE (order_id, grant_kind);

DROP INDEX IF EXISTS workspace_entitlement_provider_sub_uidx;
ALTER TABLE workspace_entitlement_state
    DROP COLUMN IF EXISTS provider_subscription_id,
    DROP COLUMN IF EXISTS provider;

DROP INDEX IF EXISTS billing_invoices_provider_sub_idx;
ALTER TABLE billing_invoices
    DROP COLUMN IF EXISTS provider_subscription_id;

DROP INDEX IF EXISTS billing_orders_provider_sub_uidx;
ALTER TABLE billing_orders
    DROP COLUMN IF EXISTS checkout_mode,
    DROP COLUMN IF EXISTS provider_subscription_id;

COMMIT;
