-- Migration 0013: Financial Integrity Ledger, Durable Checkout Idempotency,
-- Provider Order Correlation, Unique Financial Entitlement Grants, Refund Ledger,
-- Coupon Reservations, and Atomic PostgreSQL Entitlement State.

BEGIN;

CREATE TABLE billing_orders (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    idempotency_key TEXT NOT NULL,
    request_fingerprint BYTEA NOT NULL,
    provider TEXT NOT NULL CHECK (provider IN ('paddle', 'paymob', 'manual')),
    product_code TEXT NOT NULL CHECK (
        product_code IN (
            'individual_monthly',
            'individual_annual',
            'organization_seat_annual',
            'pay_per_file',
            'credit_pack_25'
        )
    ),
    provider_price_id TEXT NOT NULL,
    seats INTEGER NOT NULL DEFAULT 1 CHECK (seats >= 1),
    currency CHAR(3) NOT NULL CHECK (currency IN ('USD', 'EGP')),
    original_amount_minor BIGINT NOT NULL CHECK (original_amount_minor >= 0),
    discount_amount_minor BIGINT NOT NULL DEFAULT 0 CHECK (discount_amount_minor >= 0),
    expected_amount_minor BIGINT NOT NULL CHECK (expected_amount_minor >= 0),
    coupon_code TEXT,
    coupon_reservation_id UUID,
    expected_integration_id BIGINT,
    provider_order_ref TEXT,
    provider_intention_id TEXT,
    checkout_url TEXT,
    provider_transaction_id TEXT,
    state TEXT NOT NULL DEFAULT 'created' CHECK (
        state IN (
            'created',
            'checkout_pending',
            'payment_pending',
            'paid',
            'failed',
            'expired',
            'canceled',
            'refund_pending',
            'refunded',
            'partially_refunded',
            'chargeback',
            'reconciliation_required'
        )
    ),
    last_error_code TEXT,
    expires_at TIMESTAMPTZ NOT NULL,
    paid_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT billing_orders_idem_not_blank CHECK (length(btrim(idempotency_key)) > 0),
    CONSTRAINT billing_orders_fingerprint_size CHECK (octet_length(request_fingerprint) = 32),
    UNIQUE (workspace_id, idempotency_key)
);

CREATE UNIQUE INDEX billing_orders_provider_order_ref_uidx
    ON billing_orders (provider, provider_order_ref)
    WHERE provider_order_ref IS NOT NULL;

CREATE UNIQUE INDEX billing_orders_provider_tx_uidx
    ON billing_orders (provider, provider_transaction_id)
    WHERE provider_transaction_id IS NOT NULL;

CREATE INDEX billing_orders_workspace_created_idx
    ON billing_orders (workspace_id, created_at DESC);

ALTER TABLE billing_orders ENABLE ROW LEVEL SECURITY;
ALTER TABLE billing_orders FORCE ROW LEVEL SECURITY;
CREATE POLICY billing_orders_tenant_isolation ON billing_orders
    USING (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid
        OR workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::uuid
    )
    WITH CHECK (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid
        OR workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::uuid
    );

CREATE TABLE billing_entitlement_grants (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    order_id UUID NOT NULL REFERENCES billing_orders(id) ON DELETE RESTRICT,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider TEXT NOT NULL CHECK (provider IN ('paddle', 'paymob', 'manual')),
    provider_transaction_id TEXT NOT NULL,
    grant_kind TEXT NOT NULL CHECK (grant_kind IN ('subscription', 'credit_pack_25', 'pay_per_file')),
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    plan_code TEXT,
    period_end TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT billing_grants_tx_not_blank CHECK (length(btrim(provider_transaction_id)) > 0),
    UNIQUE (provider, provider_transaction_id),
    UNIQUE (order_id, grant_kind)
);

ALTER TABLE billing_entitlement_grants ENABLE ROW LEVEL SECURITY;
ALTER TABLE billing_entitlement_grants FORCE ROW LEVEL SECURITY;
CREATE POLICY billing_entitlement_grants_tenant_isolation ON billing_entitlement_grants
    USING (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid
        OR workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::uuid
    )
    WITH CHECK (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid
        OR workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::uuid
    );

CREATE TABLE billing_refunds (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    order_id UUID REFERENCES billing_orders(id) ON DELETE RESTRICT,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    provider TEXT NOT NULL CHECK (provider IN ('paddle', 'paymob', 'manual')),
    external_invoice_id TEXT NOT NULL,
    refund_request_key TEXT NOT NULL,
    provider_refund_id TEXT,
    amount_minor BIGINT NOT NULL CHECK (amount_minor >= 0),
    currency CHAR(3) NOT NULL CHECK (currency IN ('USD', 'EGP')),
    kind TEXT NOT NULL CHECK (kind IN ('refund', 'partial_refund', 'chargeback')),
    status TEXT NOT NULL CHECK (status IN ('requested', 'confirmed', 'failed')),
    mandatory_legal_override BOOLEAN NOT NULL DEFAULT FALSE,
    reason TEXT NOT NULL DEFAULT '',
    requested_by UUID REFERENCES users(id) ON DELETE SET NULL,
    confirmed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT billing_refunds_req_key_not_blank CHECK (length(btrim(refund_request_key)) > 0),
    UNIQUE (workspace_id, refund_request_key)
);

CREATE UNIQUE INDEX billing_refunds_provider_ref_uidx
    ON billing_refunds (provider, provider_refund_id)
    WHERE provider_refund_id IS NOT NULL;

ALTER TABLE billing_refunds ENABLE ROW LEVEL SECURITY;
ALTER TABLE billing_refunds FORCE ROW LEVEL SECURITY;
CREATE POLICY billing_refunds_tenant_isolation ON billing_refunds
    USING (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid
        OR workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::uuid
    )
    WITH CHECK (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid
        OR workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::uuid
    );

CREATE TABLE coupon_reservations (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    coupon_id UUID NOT NULL REFERENCES coupons(id) ON DELETE RESTRICT,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    order_id UUID NOT NULL REFERENCES billing_orders(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'reserved' CHECK (status IN ('reserved', 'committed', 'released', 'expired')),
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX coupon_reservations_active_user_uidx
    ON coupon_reservations (coupon_id, user_id)
    WHERE status IN ('reserved', 'committed');

ALTER TABLE coupon_reservations ENABLE ROW LEVEL SECURITY;
ALTER TABLE coupon_reservations FORCE ROW LEVEL SECURITY;
CREATE POLICY coupon_reservations_tenant_isolation ON coupon_reservations
    USING (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid
        OR workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::uuid
    )
    WITH CHECK (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid
        OR workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::uuid
    );

CREATE TABLE workspace_billing_watermarks (
    workspace_id UUID PRIMARY KEY REFERENCES workspaces(id) ON DELETE CASCADE,
    last_event_occurred_at BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE workspace_billing_watermarks ENABLE ROW LEVEL SECURITY;
ALTER TABLE workspace_billing_watermarks FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_billing_watermarks_tenant_isolation ON workspace_billing_watermarks
    USING (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid
        OR workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::uuid
    )
    WITH CHECK (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid
        OR workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::uuid
    );

CREATE TABLE workspace_entitlement_state (
    workspace_id UUID PRIMARY KEY REFERENCES workspaces(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    plan_tier TEXT NOT NULL DEFAULT 'free' CHECK (
        plan_tier IN (
            'free',
            'trial',
            'individual_monthly',
            'individual_annual',
            'organization_seat',
            'friend_vip',
            'owner'
        )
    ),
    subscription_state TEXT NOT NULL DEFAULT 'free' CHECK (
        subscription_state IN ('active', 'trialing', 'past_due', 'canceled', 'expired', 'free')
    ),
    seats INTEGER NOT NULL DEFAULT 1 CHECK (seats >= 1),
    trial_expires_at BIGINT,
    current_period_end BIGINT,
    past_due_since BIGINT,
    friend_vip_active BOOLEAN NOT NULL DEFAULT FALSE,
    friend_vip_granted_by UUID REFERENCES users(id) ON DELETE SET NULL,
    operation_credits INTEGER NOT NULL DEFAULT 0 CHECK (operation_credits >= 0),
    pay_per_file_tokens INTEGER NOT NULL DEFAULT 0 CHECK (pay_per_file_tokens >= 0),
    trial_ai_used INTEGER NOT NULL DEFAULT 0 CHECK (trial_ai_used >= 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE workspace_entitlement_state ENABLE ROW LEVEL SECURITY;
ALTER TABLE workspace_entitlement_state FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_entitlement_state_tenant_isolation ON workspace_entitlement_state
    USING (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid
        OR workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::uuid
    )
    WITH CHECK (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid
        OR workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::uuid
    );

-- Enhance billing_invoices with order correlation, unique provider transaction deduplication,
-- partial refund tracking, and owner_override RLS access.
ALTER TABLE billing_invoices
    ADD COLUMN IF NOT EXISTS order_id UUID REFERENCES billing_orders(id) ON DELETE RESTRICT,
    ADD COLUMN IF NOT EXISTS provider_transaction_id TEXT,
    ADD COLUMN IF NOT EXISTS refunded_amount_minor BIGINT NOT NULL DEFAULT 0 CHECK (refunded_amount_minor >= 0);

CREATE UNIQUE INDEX IF NOT EXISTS billing_invoices_provider_tx_uidx
    ON billing_invoices (provider, provider_transaction_id)
    WHERE provider_transaction_id IS NOT NULL;

ALTER TABLE billing_invoices DROP CONSTRAINT IF EXISTS billing_invoices_status_check;
ALTER TABLE billing_invoices
    ADD CONSTRAINT billing_invoices_status_check
    CHECK (status IN ('paid', 'pending', 'refund_pending', 'partially_refunded', 'refunded', 'chargeback', 'void'));

DROP POLICY IF EXISTS billing_invoices_tenant_isolation ON billing_invoices;
CREATE POLICY billing_invoices_tenant_isolation ON billing_invoices
    USING (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid
        OR workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::uuid
    )
    WITH CHECK (
        current_setting('app.owner_override', true) = 'true'
        OR workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid
        OR workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::uuid
    );

-- Enhance billing_webhook_events with durable order/transaction correlation and processing audit.
ALTER TABLE billing_webhook_events
    ADD COLUMN IF NOT EXISTS order_id UUID REFERENCES billing_orders(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS provider_transaction_id TEXT,
    ADD COLUMN IF NOT EXISTS event_type TEXT,
    ADD COLUMN IF NOT EXISTS workspace_id UUID REFERENCES workspaces(id) ON DELETE SET NULL,
    ADD COLUMN IF NOT EXISTS occurred_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS outcome TEXT,
    ADD COLUMN IF NOT EXISTS error_detail TEXT;

CREATE INDEX IF NOT EXISTS billing_webhook_events_order_idx
    ON billing_webhook_events (order_id)
    WHERE order_id IS NOT NULL;

COMMIT;
