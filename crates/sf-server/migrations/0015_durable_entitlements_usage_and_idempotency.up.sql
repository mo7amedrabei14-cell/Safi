-- Migration 0015: Durable PostgreSQL 18 Entitlement Usage Accounting,
-- Fingerprinted Usage Idempotency, and Persistent Trial Anti-Abuse Claims.

BEGIN;

CREATE TABLE IF NOT EXISTS workspace_monthly_usage (
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    period_ym CHAR(7) NOT NULL CHECK (period_ym ~ '^[0-9]{4}-[0-9]{2}$'),
    operations_used INTEGER NOT NULL DEFAULT 0 CHECK (operations_used >= 0),
    ai_calls_used INTEGER NOT NULL DEFAULT 0 CHECK (ai_calls_used >= 0),
    rows_cleaned BIGINT NOT NULL DEFAULT 0 CHECK (rows_cleaned >= 0),
    pii_masked BIGINT NOT NULL DEFAULT 0 CHECK (pii_masked >= 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, period_ym)
);

ALTER TABLE workspace_monthly_usage ENABLE ROW LEVEL SECURITY;
ALTER TABLE workspace_monthly_usage FORCE ROW LEVEL SECURITY;
CREATE POLICY workspace_monthly_usage_tenant_isolation ON workspace_monthly_usage
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

CREATE TABLE IF NOT EXISTS entitlement_usage_idempotency (
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    idempotency_key TEXT NOT NULL CHECK (length(btrim(idempotency_key)) > 0 AND length(idempotency_key) <= 128),
    request_fingerprint BYTEA NOT NULL CHECK (octet_length(request_fingerprint) = 32),
    usage_kind TEXT NOT NULL CHECK (usage_kind IN ('data_operation', 'ai_call')),
    watermark_required BOOLEAN NOT NULL,
    debited_credit BOOLEAN NOT NULL DEFAULT FALSE,
    debited_pay_per_file BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, idempotency_key)
);

ALTER TABLE entitlement_usage_idempotency ENABLE ROW LEVEL SECURITY;
ALTER TABLE entitlement_usage_idempotency FORCE ROW LEVEL SECURITY;
CREATE POLICY entitlement_usage_idempotency_tenant_isolation ON entitlement_usage_idempotency
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

CREATE TABLE IF NOT EXISTS trial_anti_abuse_claims (
    claim_kind TEXT NOT NULL CHECK (claim_kind IN ('email', 'phone', 'device')),
    claim_hash TEXT NOT NULL CHECK (length(btrim(claim_hash)) > 0),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (claim_kind, claim_hash)
);

COMMIT;
