-- Rollback Migration 0013: remove financial integrity ledger tables and restore migration 0010 billing_invoices policy/constraints.

BEGIN;

DROP INDEX IF EXISTS billing_webhook_events_order_idx;
ALTER TABLE billing_webhook_events
    DROP COLUMN IF EXISTS error_detail,
    DROP COLUMN IF EXISTS outcome,
    DROP COLUMN IF EXISTS occurred_at,
    DROP COLUMN IF EXISTS workspace_id,
    DROP COLUMN IF EXISTS event_type,
    DROP COLUMN IF EXISTS provider_transaction_id,
    DROP COLUMN IF EXISTS order_id;

DROP POLICY IF EXISTS billing_invoices_tenant_isolation ON billing_invoices;
CREATE POLICY billing_invoices_tenant_isolation ON billing_invoices
    USING (workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid);

ALTER TABLE billing_invoices DROP CONSTRAINT IF EXISTS billing_invoices_status_check;
ALTER TABLE billing_invoices
    ADD CONSTRAINT billing_invoices_status_check
    CHECK (status IN ('paid', 'pending', 'refunded', 'chargeback', 'void'));

DROP INDEX IF EXISTS billing_invoices_provider_tx_uidx;
ALTER TABLE billing_invoices
    DROP COLUMN IF EXISTS refunded_amount_minor,
    DROP COLUMN IF EXISTS provider_transaction_id,
    DROP COLUMN IF EXISTS order_id;

DROP TABLE IF EXISTS workspace_entitlement_state;
DROP TABLE IF EXISTS workspace_billing_watermarks;
DROP TABLE IF EXISTS coupon_reservations;
DROP TABLE IF EXISTS billing_refunds;
DROP TABLE IF EXISTS billing_entitlement_grants;
DROP TABLE IF EXISTS billing_orders;

COMMIT;
