-- Migration 0008: Phone OTP verification, OAuth identity links, authoritative entitlements,
-- usage counters, credit balances, trial anti-abuse, Friend VIP grants, and Paddle/Paymob billing.

ALTER TABLE users
    ADD COLUMN IF NOT EXISTS verified_phone TEXT,
    ADD COLUMN IF NOT EXISTS phone_verified_at TIMESTAMPTZ;

CREATE UNIQUE INDEX IF NOT EXISTS idx_users_verified_phone_unique
    ON users (verified_phone)
    WHERE verified_phone IS NOT NULL;

CREATE TABLE phone_verifications (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    phone_e164 TEXT NOT NULL,
    channel TEXT NOT NULL CHECK (channel IN ('sms', 'whatsapp')),
    code_hash TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_phone_verifications_user_active
    ON phone_verifications(user_id, created_at DESC)
    WHERE consumed_at IS NULL;

CREATE TABLE oauth_identities (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider TEXT NOT NULL CHECK (provider IN ('google', 'apple', 'github')),
    provider_subject TEXT NOT NULL,
    email_at_provider TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider, provider_subject)
);

CREATE INDEX idx_oauth_identities_user
    ON oauth_identities(user_id);

-- Extend billing provider checks to support Paddle alongside Paymob and manual overrides.
-- Migration 0003 named these constraints `subscriptions_provider_valid` and `billing_webhook_provider_valid`.
ALTER TABLE subscriptions DROP CONSTRAINT IF EXISTS subscriptions_provider_valid;
ALTER TABLE subscriptions DROP CONSTRAINT IF EXISTS subscriptions_provider_check;
ALTER TABLE subscriptions
    ADD CONSTRAINT subscriptions_provider_valid
    CHECK (provider IN ('paddle', 'paymob', 'manual', 'lemon_squeezy'));

ALTER TABLE billing_webhook_events DROP CONSTRAINT IF EXISTS billing_webhook_provider_valid;
ALTER TABLE billing_webhook_events DROP CONSTRAINT IF EXISTS billing_webhook_events_provider_check;
ALTER TABLE billing_webhook_events
    ADD CONSTRAINT billing_webhook_provider_valid
    CHECK (provider IN ('paddle', 'paymob', 'manual', 'lemon_squeezy'));

CREATE TABLE trial_claims (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    email_hash TEXT NOT NULL,
    phone_hash TEXT,
    device_fingerprint_hash TEXT,
    claimed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_trial_claims_email_hash ON trial_claims(email_hash);
CREATE INDEX idx_trial_claims_phone_hash ON trial_claims(phone_hash) WHERE phone_hash IS NOT NULL;
CREATE INDEX idx_trial_claims_device_hash ON trial_claims(device_fingerprint_hash) WHERE device_fingerprint_hash IS NOT NULL;

CREATE TABLE friend_vip_grants (
    user_id UUID PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    granted_by UUID REFERENCES users(id) ON DELETE SET NULL,
    reason TEXT NOT NULL DEFAULT '',
    granted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at TIMESTAMPTZ
);

CREATE TABLE monthly_usage_counters (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    period_ym TEXT NOT NULL,
    operations_used INTEGER NOT NULL DEFAULT 0 CHECK (operations_used >= 0),
    ai_calls_used INTEGER NOT NULL DEFAULT 0 CHECK (ai_calls_used >= 0),
    rows_cleaned BIGINT NOT NULL DEFAULT 0 CHECK (rows_cleaned >= 0),
    pii_masked BIGINT NOT NULL DEFAULT 0 CHECK (pii_masked >= 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (workspace_id, period_ym)
);

ALTER TABLE monthly_usage_counters ENABLE ROW LEVEL SECURITY;
ALTER TABLE monthly_usage_counters FORCE ROW LEVEL SECURITY;
CREATE POLICY monthly_usage_counters_tenant_isolation ON monthly_usage_counters
    USING (workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid);

CREATE TABLE usage_ledger (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    idempotency_key TEXT NOT NULL,
    event_kind TEXT NOT NULL CHECK (event_kind IN ('operation', 'ai_call', 'pay_per_file', 'credit_purchase', 'credit_debit')),
    quantity INTEGER NOT NULL CHECK (quantity > 0),
    row_count BIGINT NOT NULL DEFAULT 0 CHECK (row_count >= 0),
    column_count INTEGER NOT NULL DEFAULT 0 CHECK (column_count >= 0),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (workspace_id, idempotency_key)
);

ALTER TABLE usage_ledger ENABLE ROW LEVEL SECURITY;
ALTER TABLE usage_ledger FORCE ROW LEVEL SECURITY;
CREATE POLICY usage_ledger_tenant_isolation ON usage_ledger
    USING (workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid);

CREATE TABLE credit_balances (
    workspace_id UUID PRIMARY KEY REFERENCES workspaces(id) ON DELETE CASCADE,
    operation_credits INTEGER NOT NULL DEFAULT 0 CHECK (operation_credits >= 0),
    pay_per_file_tokens INTEGER NOT NULL DEFAULT 0 CHECK (pay_per_file_tokens >= 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

ALTER TABLE credit_balances ENABLE ROW LEVEL SECURITY;
ALTER TABLE credit_balances FORCE ROW LEVEL SECURITY;
CREATE POLICY credit_balances_tenant_isolation ON credit_balances
    USING (workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid);

CREATE TABLE billing_invoices (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    provider TEXT NOT NULL CHECK (provider IN ('paddle', 'paymob', 'manual')),
    external_invoice_id TEXT NOT NULL UNIQUE,
    amount_minor BIGINT NOT NULL CHECK (amount_minor >= 0),
    currency TEXT NOT NULL CHECK (currency IN ('USD', 'EGP')),
    status TEXT NOT NULL CHECK (status IN ('paid', 'pending', 'refunded', 'void')),
    issued_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    refunded_at TIMESTAMPTZ
);

ALTER TABLE billing_invoices ENABLE ROW LEVEL SECURITY;
ALTER TABLE billing_invoices FORCE ROW LEVEL SECURITY;
CREATE POLICY billing_invoices_tenant_isolation ON billing_invoices
    USING (workspace_id = NULLIF(current_setting('app.current_workspace_id', true), '')::uuid);
