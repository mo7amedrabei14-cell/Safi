-- 0005_customer_accounts_owner_ops.up.sql
-- Phase 0.1 — customer account status, constitution plan catalog, subscription
-- event history, and owner-only cross-workspace read policies.
-- Customer file contents are never stored here.

BEGIN;

-- Account lifecycle: active -> suspended (reversible) -> deleted (soft delete).
ALTER TABLE users
    ADD COLUMN suspended_at TIMESTAMPTZ;

-- Plan catalog mirroring config/business.json. Paid plans stay in draft until
-- the owner activates billing accounts; only Free is active at launch.
INSERT INTO plans (
    code,
    plan_tier,
    display_name,
    status,
    price_minor,
    currency,
    billing_interval,
    entitlements
)
VALUES
    (
        'individual_monthly',
        'personal',
        'SafiData Individual Monthly',
        'draft',
        NULL,
        NULL,
        'monthly',
        '{"ai_calls_per_month":400,"includes_premium_recipe_packs":true,"recipe_packs_duration":"active_subscription_only","watermark":false}'::JSONB
    ),
    (
        'individual_annual',
        'personal',
        'SafiData Individual Annual',
        'draft',
        NULL,
        NULL,
        'yearly',
        '{"ai_calls_per_month":400,"includes_premium_recipe_packs":true,"recipe_packs_duration":"active_subscription_only","watermark":false,"discount_percent":30}'::JSONB
    ),
    (
        'organization_seat',
        'team',
        'SafiData Organization Seat',
        'draft',
        NULL,
        NULL,
        'monthly',
        '{"ai_calls_per_seat_per_month":400,"min_seats":3,"includes_premium_recipe_packs":true,"recipe_packs_duration":"active_subscription_only","watermark":false}'::JSONB
    )
ON CONFLICT (code) DO UPDATE
SET
    plan_tier = EXCLUDED.plan_tier,
    display_name = EXCLUDED.display_name,
    status = EXCLUDED.status,
    price_minor = NULL,
    currency = NULL,
    billing_interval = EXCLUDED.billing_interval,
    entitlements = EXCLUDED.entitlements;

-- A plan can be priced in EGP (Paymob) and USD (Paddle) at the same time.
CREATE TABLE plan_prices (
    plan_code TEXT NOT NULL REFERENCES plans (code) ON DELETE CASCADE,
    currency CHAR(3) NOT NULL,
    billing_interval TEXT NOT NULL,
    amount_minor INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (plan_code, currency, billing_interval),
    CONSTRAINT plan_prices_currency_format CHECK (currency ~ '^[A-Z]{3}$'),
    CONSTRAINT plan_prices_interval_valid CHECK (
        billing_interval IN ('monthly', 'yearly', 'per_seat_monthly', 'per_seat_yearly')
    ),
    CONSTRAINT plan_prices_non_negative CHECK (amount_minor >= 0)
);

-- Minor units: USD cents and EGP piastres. Every amount matches the constitution.
INSERT INTO plan_prices (plan_code, currency, billing_interval, amount_minor)
VALUES
    ('individual_monthly', 'USD', 'monthly', 2000),
    ('individual_monthly', 'EGP', 'monthly', 100000),
    ('individual_annual', 'USD', 'yearly', 16800),
    ('individual_annual', 'EGP', 'yearly', 840000),
    ('organization_seat', 'USD', 'per_seat_monthly', 1700),
    ('organization_seat', 'EGP', 'per_seat_monthly', 85000),
    ('organization_seat', 'USD', 'per_seat_yearly', 20400),
    ('organization_seat', 'EGP', 'per_seat_yearly', 1020000)
ON CONFLICT (plan_code, currency, billing_interval) DO UPDATE
SET amount_minor = EXCLUDED.amount_minor;

-- Append-only subscription history: created, renewed, canceled, refunded, ...
CREATE TABLE subscription_events (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    workspace_id UUID NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    subscription_id UUID REFERENCES subscriptions (id) ON DELETE SET NULL,
    event_type TEXT NOT NULL,
    actor_user_id UUID REFERENCES users (id) ON DELETE SET NULL,
    actor_role TEXT NOT NULL DEFAULT 'system',
    from_status TEXT,
    to_status TEXT,
    amount_minor INTEGER,
    currency CHAR(3),
    metadata JSONB NOT NULL DEFAULT '{}'::JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT subscription_events_type_valid CHECK (
        event_type IN (
            'created',
            'renewed',
            'plan_changed',
            'canceled',
            'reactivated',
            'expired',
            'refunded',
            'trial_extended'
        )
    ),
    CONSTRAINT subscription_events_actor_role_valid CHECK (
        actor_role IN ('system', 'owner', 'customer', 'payment_provider')
    ),
    CONSTRAINT subscription_events_amount_non_negative CHECK (
        amount_minor IS NULL OR amount_minor >= 0
    ),
    CONSTRAINT subscription_events_currency_format CHECK (
        currency IS NULL OR currency ~ '^[A-Z]{3}$'
    )
);

CREATE INDEX subscription_events_workspace_created_idx
    ON subscription_events (workspace_id, created_at DESC);
CREATE INDEX subscription_events_type_created_idx
    ON subscription_events (event_type, created_at DESC);

ALTER TABLE subscription_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE subscription_events FORCE ROW LEVEL SECURITY;
CREATE POLICY subscription_events_workspace_isolation ON subscription_events
    USING (workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::UUID);

-- The owner console must read across every workspace. The marker is set only
-- inside a transaction opened by the owner-authorized service, never from a
-- request header or client input.
CREATE POLICY subscription_events_owner_read ON subscription_events
    FOR SELECT
    USING (current_setting('app.owner_admin', true) = 'true');

CREATE POLICY memberships_owner_read ON memberships
    FOR SELECT
    USING (current_setting('app.owner_admin', true) = 'true');

CREATE POLICY subscriptions_owner_read ON subscriptions
    FOR SELECT
    USING (current_setting('app.owner_admin', true) = 'true');

CREATE POLICY devices_owner_read ON devices
    FOR SELECT
    USING (current_setting('app.owner_admin', true) = 'true');

COMMIT;
