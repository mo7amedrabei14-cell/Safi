-- 0003_accounts_subscriptions_devices.up.sql
-- Account identity, email credentials, one-time auth tokens, device registry,
-- subscription catalog, and payment-provider idempotency.
-- Customer file contents are deliberately not stored in this schema.

BEGIN;

ALTER TABLE users
    ADD COLUMN email_verified_at TIMESTAMPTZ;

CREATE TABLE user_credentials (
    user_id UUID PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    password_hash TEXT NOT NULL,
    password_changed_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT user_credentials_argon2id_hash CHECK (
        password_hash LIKE '$argon2id$%'
    )
);

ALTER TABLE user_credentials ENABLE ROW LEVEL SECURITY;
ALTER TABLE user_credentials FORCE ROW LEVEL SECURITY;
CREATE POLICY user_credentials_user_isolation ON user_credentials
    USING (user_id = NULLIF(current_setting('app.user_id', true), '')::UUID)
    WITH CHECK (user_id = NULLIF(current_setting('app.user_id', true), '')::UUID);

CREATE TABLE email_verification_tokens (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    CONSTRAINT email_verification_token_hash_size CHECK (octet_length(token_hash) = 32),
    CONSTRAINT email_verification_token_expiry CHECK (expires_at > created_at)
);

CREATE UNIQUE INDEX email_verification_tokens_hash_key
    ON email_verification_tokens (token_hash);
CREATE UNIQUE INDEX email_verification_tokens_one_pending_per_user
    ON email_verification_tokens (user_id)
    WHERE consumed_at IS NULL;
CREATE INDEX email_verification_tokens_expiry_idx
    ON email_verification_tokens (expires_at)
    WHERE consumed_at IS NULL;

CREATE TABLE password_reset_tokens (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash BYTEA NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    CONSTRAINT password_reset_token_hash_size CHECK (octet_length(token_hash) = 32),
    CONSTRAINT password_reset_token_expiry CHECK (expires_at > created_at)
);

CREATE UNIQUE INDEX password_reset_tokens_hash_key
    ON password_reset_tokens (token_hash);
CREATE UNIQUE INDEX password_reset_tokens_one_pending_per_user
    ON password_reset_tokens (user_id)
    WHERE consumed_at IS NULL;
CREATE INDEX password_reset_tokens_expiry_idx
    ON password_reset_tokens (expires_at)
    WHERE consumed_at IS NULL;

CREATE TABLE devices (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    workspace_id UUID NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    device_name TEXT NOT NULL,
    platform TEXT NOT NULL,
    installation_key_hash BYTEA NOT NULL,
    registered_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at TIMESTAMPTZ,
    CONSTRAINT devices_name_not_blank CHECK (length(btrim(device_name)) > 0),
    CONSTRAINT devices_platform_valid CHECK (
        platform IN ('windows', 'linux', 'macos', 'web')
    ),
    CONSTRAINT devices_installation_hash_size CHECK (
        octet_length(installation_key_hash) = 32
    )
);

CREATE UNIQUE INDEX devices_installation_key_hash_key
    ON devices (installation_key_hash);
CREATE UNIQUE INDEX devices_one_active_per_user_key
    ON devices (user_id)
    WHERE revoked_at IS NULL;
CREATE INDEX devices_workspace_last_seen_idx
    ON devices (workspace_id, last_seen_at DESC);

ALTER TABLE devices ENABLE ROW LEVEL SECURITY;
ALTER TABLE devices FORCE ROW LEVEL SECURITY;
CREATE POLICY devices_workspace_isolation ON devices
    USING (workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::UUID);

CREATE TABLE plans (
    code TEXT PRIMARY KEY,
    plan_tier TEXT NOT NULL,
    display_name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'draft',
    price_minor INTEGER,
    currency CHAR(3),
    billing_interval TEXT NOT NULL,
    entitlements JSONB NOT NULL DEFAULT '{}'::JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT plans_tier_valid CHECK (
        plan_tier IN ('free', 'personal', 'team', 'enterprise')
    ),
    CONSTRAINT plans_status_valid CHECK (status IN ('active', 'draft', 'retired')),
    CONSTRAINT plans_price_non_negative CHECK (price_minor IS NULL OR price_minor >= 0),
    CONSTRAINT plans_currency_format CHECK (
        currency IS NULL OR currency ~ '^[A-Z]{3}$'
    ),
    CONSTRAINT plans_interval_valid CHECK (
        billing_interval IN ('none', 'monthly', 'yearly', 'custom')
    ),
    CONSTRAINT plans_active_price_defined CHECK (
        status <> 'active' OR price_minor IS NOT NULL
    )
);

INSERT INTO plans (code, plan_tier, display_name, status, price_minor, currency, billing_interval)
VALUES
    ('free', 'free', 'Free', 'active', 0, 'EGP', 'none'),
    ('pro', 'personal', 'Pro', 'draft', NULL, NULL, 'custom'),
    ('pro_monthly', 'personal', 'Pro Monthly', 'draft', NULL, NULL, 'monthly'),
    ('pro_yearly', 'personal', 'Pro Yearly', 'draft', NULL, NULL, 'yearly'),
    ('team', 'team', 'Team', 'draft', NULL, NULL, 'custom'),
    ('team_monthly', 'team', 'Team Monthly', 'draft', NULL, NULL, 'monthly'),
    ('team_yearly', 'team', 'Team Yearly', 'draft', NULL, NULL, 'yearly'),
    ('enterprise', 'enterprise', 'Enterprise', 'draft', NULL, NULL, 'custom')
ON CONFLICT (code) DO NOTHING;

CREATE TRIGGER plans_touch_updated_at
    BEFORE UPDATE ON plans
    FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

CREATE TABLE subscriptions (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    workspace_id UUID NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    plan_code TEXT NOT NULL REFERENCES plans (code),
    status TEXT NOT NULL,
    provider TEXT NOT NULL DEFAULT 'manual',
    provider_customer_id TEXT,
    provider_subscription_id TEXT,
    started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    current_period_start TIMESTAMPTZ NOT NULL DEFAULT now(),
    current_period_end TIMESTAMPTZ,
    cancel_at_period_end BOOLEAN NOT NULL DEFAULT false,
    canceled_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT subscriptions_status_valid CHECK (
        status IN ('trialing', 'active', 'past_due', 'grace_period', 'canceled', 'expired', 'unpaid')
    ),
    CONSTRAINT subscriptions_provider_valid CHECK (
        provider IN ('manual', 'paymob', 'lemon_squeezy')
    ),
    CONSTRAINT subscriptions_period_valid CHECK (
        current_period_end IS NULL OR current_period_end > current_period_start
    )
);

CREATE UNIQUE INDEX subscriptions_one_current_per_workspace_key
    ON subscriptions (workspace_id)
    WHERE status IN ('trialing', 'active', 'past_due', 'grace_period');
CREATE UNIQUE INDEX subscriptions_provider_reference_key
    ON subscriptions (provider, provider_subscription_id)
    WHERE provider_subscription_id IS NOT NULL;
CREATE INDEX subscriptions_workspace_history_idx
    ON subscriptions (workspace_id, created_at DESC);

ALTER TABLE subscriptions ENABLE ROW LEVEL SECURITY;
ALTER TABLE subscriptions FORCE ROW LEVEL SECURITY;
CREATE POLICY subscriptions_workspace_isolation ON subscriptions
    USING (workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::UUID)
    WITH CHECK (workspace_id = NULLIF(current_setting('app.workspace_id', true), '')::UUID);

CREATE TRIGGER subscriptions_touch_updated_at
    BEFORE UPDATE ON subscriptions
    FOR EACH ROW EXECUTE FUNCTION touch_updated_at();

-- Existing workspaces receive a Free subscription only when they do not already
-- have a current subscription. Unknown legacy plan labels are not copied.
INSERT INTO subscriptions (
    workspace_id,
    plan_code,
    status,
    started_at,
    current_period_start
)
SELECT
    workspace.id,
    CASE
        WHEN EXISTS (SELECT 1 FROM plans WHERE plans.code = workspace.plan_code)
            THEN workspace.plan_code
        ELSE 'free'
    END,
    'active',
    workspace.created_at,
    workspace.created_at
FROM workspaces AS workspace
WHERE NOT EXISTS (
    SELECT 1
    FROM subscriptions
    WHERE subscriptions.workspace_id = workspace.id
      AND subscriptions.status IN ('trialing', 'active', 'past_due', 'grace_period')
);

CREATE TABLE billing_webhook_events (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    provider TEXT NOT NULL,
    provider_event_id TEXT NOT NULL,
    payload_hash BYTEA NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    processed_at TIMESTAMPTZ,
    processing_status TEXT NOT NULL DEFAULT 'received',
    CONSTRAINT billing_webhook_provider_valid CHECK (
        provider IN ('paymob', 'lemon_squeezy')
    ),
    CONSTRAINT billing_webhook_event_not_blank CHECK (
        length(btrim(provider_event_id)) > 0
    ),
    CONSTRAINT billing_webhook_payload_hash_size CHECK (
        octet_length(payload_hash) = 32
    ),
    CONSTRAINT billing_webhook_status_valid CHECK (
        processing_status IN ('received', 'processed', 'ignored', 'failed')
    ),
    UNIQUE (provider, provider_event_id)
);

COMMIT;
