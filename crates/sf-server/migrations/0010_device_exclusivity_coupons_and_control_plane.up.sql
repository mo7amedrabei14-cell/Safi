-- Migration 0010: Monotonic session_generation, cryptographic device public keys,
-- device replacement audit history, Owner-controlled coupons & redemptions,
-- support tickets, and platform announcements.

ALTER TABLE users
    ADD COLUMN IF NOT EXISTS session_generation BIGINT NOT NULL DEFAULT 1 CHECK (session_generation >= 1);

ALTER TABLE devices
    ADD COLUMN IF NOT EXISTS public_key_hex TEXT,
    ADD COLUMN IF NOT EXISTS replaced_by_device_id UUID REFERENCES devices(id) ON DELETE SET NULL;

ALTER TABLE sessions
    ADD COLUMN IF NOT EXISTS session_generation BIGINT NOT NULL DEFAULT 1 CHECK (session_generation >= 1),
    ADD COLUMN IF NOT EXISTS revocation_reason TEXT;

CREATE TABLE device_replacement_events (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    previous_device_id UUID REFERENCES devices(id) ON DELETE SET NULL,
    new_device_id UUID REFERENCES devices(id) ON DELETE SET NULL,
    trigger_kind TEXT NOT NULL CHECK (trigger_kind IN ('new_device_login', 'owner_reset')),
    actor_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
    reason TEXT NOT NULL DEFAULT '',
    new_session_generation BIGINT NOT NULL CHECK (new_session_generation >= 1),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_device_replacement_events_user
    ON device_replacement_events(user_id, created_at DESC);

CREATE TABLE coupons (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    code TEXT NOT NULL UNIQUE,
    discount_kind TEXT NOT NULL CHECK (discount_kind IN ('percent', 'fixed_usd', 'fixed_egp')),
    discount_value BIGINT NOT NULL CHECK (discount_value > 0),
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    archived BOOLEAN NOT NULL DEFAULT FALSE,
    starts_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    max_redemptions INTEGER CHECK (max_redemptions IS NULL OR max_redemptions > 0),
    redemptions_count INTEGER NOT NULL DEFAULT 0 CHECK (redemptions_count >= 0),
    eligible_plans JSONB NOT NULL DEFAULT '[]'::jsonb,
    eligible_user_ids JSONB NOT NULL DEFAULT '[]'::jsonb,
    new_customers_only BOOLEAN NOT NULL DEFAULT FALSE,
    org_only BOOLEAN NOT NULL DEFAULT FALSE,
    min_purchase_minor BIGINT NOT NULL DEFAULT 0 CHECK (min_purchase_minor >= 0),
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE coupon_redemptions (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    coupon_id UUID NOT NULL REFERENCES coupons(id) ON DELETE RESTRICT,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    plan_code TEXT NOT NULL,
    original_amount_minor BIGINT NOT NULL CHECK (original_amount_minor >= 0),
    discount_amount_minor BIGINT NOT NULL CHECK (discount_amount_minor >= 0),
    final_amount_minor BIGINT NOT NULL CHECK (final_amount_minor >= 0),
    currency TEXT NOT NULL CHECK (currency IN ('USD', 'EGP')),
    redeemed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (coupon_id, user_id)
);

ALTER TABLE coupon_redemptions ENABLE ROW LEVEL SECURITY;
ALTER TABLE coupon_redemptions FORCE ROW LEVEL SECURITY;
CREATE POLICY coupon_redemptions_tenant_isolation ON coupon_redemptions
    USING (workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid);

CREATE TABLE support_tickets (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    subject TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'in_progress', 'resolved', 'closed')),
    owner_note TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE support_tickets ENABLE ROW LEVEL SECURITY;
ALTER TABLE support_tickets FORCE ROW LEVEL SECURITY;
CREATE POLICY support_tickets_tenant_isolation ON support_tickets
    USING (workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid);

CREATE TABLE platform_announcements (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    severity TEXT NOT NULL DEFAULT 'info' CHECK (severity IN ('info', 'warning', 'critical')),
    published BOOLEAN NOT NULL DEFAULT TRUE,
    created_by UUID REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Extend billing_invoices status CHECK to record chargebacks explicitly.
ALTER TABLE billing_invoices DROP CONSTRAINT IF EXISTS billing_invoices_status_check;
ALTER TABLE billing_invoices
    ADD CONSTRAINT billing_invoices_status_check
    CHECK (status IN ('paid', 'pending', 'refunded', 'chargeback', 'void'));
