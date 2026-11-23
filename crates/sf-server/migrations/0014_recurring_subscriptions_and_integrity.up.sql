-- Migration 0014: Recurring Subscription Renewal Modeling, Paddle Draft/Overlay Mode,
-- Refund Reconciliation State, and Webhook Payload-Hash Security Anomalies.

BEGIN;

-- 1. Persist provider_subscription_id and checkout_mode on billing_orders.
ALTER TABLE billing_orders
    ADD COLUMN IF NOT EXISTS provider_subscription_id TEXT,
    ADD COLUMN IF NOT EXISTS checkout_mode TEXT NOT NULL DEFAULT 'redirect_url'
        CHECK (checkout_mode IN ('redirect_url', 'paddle_js_overlay'));

CREATE UNIQUE INDEX IF NOT EXISTS billing_orders_provider_sub_uidx
    ON billing_orders (provider, provider_subscription_id)
    WHERE provider_subscription_id IS NOT NULL;

-- 2. Track provider_subscription_id on billing_invoices for complete recurring history.
ALTER TABLE billing_invoices
    ADD COLUMN IF NOT EXISTS provider_subscription_id TEXT;

CREATE INDEX IF NOT EXISTS billing_invoices_provider_sub_idx
    ON billing_invoices (provider, provider_subscription_id)
    WHERE provider_subscription_id IS NOT NULL;

-- 3. Track active provider subscription binding on workspace_entitlement_state.
ALTER TABLE workspace_entitlement_state
    ADD COLUMN IF NOT EXISTS provider TEXT
        CHECK (provider IS NULL OR provider IN ('paddle', 'paymob', 'manual')),
    ADD COLUMN IF NOT EXISTS provider_subscription_id TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS workspace_entitlement_provider_sub_uidx
    ON workspace_entitlement_state (provider, provider_subscription_id)
    WHERE provider_subscription_id IS NOT NULL;

-- 4. Model recurring subscription renewals explicitly on billing_entitlement_grants:
--    - Keep UNIQUE (provider, provider_transaction_id) so one transaction NEVER grants twice.
--    - Scope initial-order uniqueness to non-renewal grants so legitimate monthly/annual renewals
--      create distinct period grants and distinct invoices instead of colliding with initial purchase.
--    - Enforce per-period renewal uniqueness on (provider, provider_subscription_id, period_end).
ALTER TABLE billing_entitlement_grants
    ALTER COLUMN order_id DROP NOT NULL,
    ADD COLUMN IF NOT EXISTS provider_subscription_id TEXT,
    ADD COLUMN IF NOT EXISTS period_start TIMESTAMPTZ;

ALTER TABLE billing_entitlement_grants
    DROP CONSTRAINT IF EXISTS billing_entitlement_grants_grant_kind_check;
ALTER TABLE billing_entitlement_grants
    ADD CONSTRAINT billing_entitlement_grants_grant_kind_check
    CHECK (grant_kind IN ('subscription', 'subscription_renewal', 'credit_pack_25', 'pay_per_file'));

ALTER TABLE billing_entitlement_grants
    DROP CONSTRAINT IF EXISTS billing_entitlement_grants_order_id_grant_kind_key;

CREATE UNIQUE INDEX IF NOT EXISTS billing_entitlement_grants_initial_order_uidx
    ON billing_entitlement_grants (order_id, grant_kind)
    WHERE order_id IS NOT NULL AND grant_kind <> 'subscription_renewal';

CREATE UNIQUE INDEX IF NOT EXISTS billing_entitlement_grants_renewal_period_uidx
    ON billing_entitlement_grants (provider, provider_subscription_id, period_end)
    WHERE grant_kind = 'subscription_renewal'
      AND provider_subscription_id IS NOT NULL
      AND period_end IS NOT NULL;

-- 5. Allow `reconciliation_required` on billing_refunds for uncertain provider timeouts.
ALTER TABLE billing_refunds
    DROP CONSTRAINT IF EXISTS billing_refunds_status_check;
ALTER TABLE billing_refunds
    ADD CONSTRAINT billing_refunds_status_check
    CHECK (status IN ('requested', 'confirmed', 'failed', 'reconciliation_required'));

-- 6. Immutable audit table for webhook integrity/security anomalies (e.g. same event_id with different payload hash).
CREATE TABLE IF NOT EXISTS billing_security_anomalies (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    provider TEXT NOT NULL CHECK (provider IN ('paddle', 'paymob', 'manual')),
    provider_event_id TEXT NOT NULL,
    anomaly_kind TEXT NOT NULL,
    original_payload_hash BYTEA NOT NULL,
    conflicting_payload_hash BYTEA NOT NULL,
    detected_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS billing_security_anomalies_provider_event_idx
    ON billing_security_anomalies (provider, provider_event_id, detected_at DESC);

COMMIT;
