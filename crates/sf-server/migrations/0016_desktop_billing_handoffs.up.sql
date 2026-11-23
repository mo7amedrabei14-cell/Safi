-- Migration 0016: Durable Desktop-to-Website Billing Handoffs & Website Billing Sessions.
--
-- Security invariants:
-- 1. Desktop never processes card payments; it initiates a short-lived (180s), single-use
--    handoff bound strictly to `(user_id, workspace_id)` and stored as BLAKE3 hashes only.
-- 2. Website Billing Sessions (`sfb_*`) allow authenticated checkout on `/billing` WITHOUT
--    revoking or replacing the user's single active Desktop installation (`sfs_*`), and are
--    strictly rejected by Desktop Studio, Sync, Org, AI, and Owner endpoints.

CREATE TABLE desktop_billing_handoffs (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    handoff_code_hash BYTEA NOT NULL UNIQUE CHECK (octet_length(handoff_code_hash) = 32),
    state_hash BYTEA NOT NULL CHECK (octet_length(state_hash) = 32),
    product_code TEXT NOT NULL CHECK (
        product_code IN (
            'individual_monthly',
            'individual_annual',
            'organization_seat_annual',
            'pay_per_file',
            'credit_pack_25'
        )
    ),
    seats INTEGER NOT NULL DEFAULT 1 CHECK (seats >= 1),
    billing_country_iso2 TEXT NOT NULL DEFAULT 'US' CHECK (length(btrim(billing_country_iso2)) = 2),
    coupon_code TEXT,
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_desktop_billing_handoffs_ws_created
    ON desktop_billing_handoffs (workspace_id, created_at DESC);

ALTER TABLE desktop_billing_handoffs ENABLE ROW LEVEL SECURITY;
ALTER TABLE desktop_billing_handoffs FORCE ROW LEVEL SECURITY;
CREATE POLICY desktop_billing_handoffs_isolation ON desktop_billing_handoffs
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

CREATE TABLE billing_web_sessions (
    id UUID PRIMARY KEY DEFAULT uuidv7(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    token_hash BYTEA NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    user_agent_hint TEXT NOT NULL DEFAULT 'billing-web',
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_billing_web_sessions_user
    ON billing_web_sessions (user_id, created_at DESC);

ALTER TABLE billing_web_sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE billing_web_sessions FORCE ROW LEVEL SECURITY;
CREATE POLICY billing_web_sessions_isolation ON billing_web_sessions
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
