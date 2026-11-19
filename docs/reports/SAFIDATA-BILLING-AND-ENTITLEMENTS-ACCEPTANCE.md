# SafiData Desktop 1.0 — Billing & Entitlements Acceptance Report

- **Repository:** `mo7amedrabei14-cell/Safi` | **Branch:** `main` | **Baseline HEAD:** `da5c9c4f997158078aee05c3eee04f24af7f44aa`
- **Approved Payment Providers:** Paddle (International USD) + Paymob (Egypt EGP)
- **Status:** `READY_FOR_MANUAL_SETUP` (server adapters, HMAC verifiers, entitlement engine, migrations, and contract tests complete; live Paddle & Paymob merchant accounts remain `NOT_CONFIGURED`)

---

## 1. Approved Pricing & Entitlement Rules Verified (`config/business.json` + `crates/sf-server/src/entitlements.rs`)

| Tier / Product | Price (USD / EGP) | Enforced Limits & Features | Verified Test |
| :--- | :--- | :--- | :--- |
| Free Plan | $0 / 0 EGP | 5 ops/month, max 5,000 rows, max 20 columns, mandatory watermark (`Processed by SafiData (Free Plan) — صافي البيانات`), read-only locked saved recipes, 0 AI calls, no premium packs | `free_plan_boundaries_credits_pay_per_file_and_monthly_reset` |
| 7-Day Trial | $0 / 0 EGP (no card) | All features open, verified email + verified phone, one trial per email/phone/device hash, 25 AI calls total, automatic downgrade to Free at expiry | `trial_anti_abuse_ai_cap_auto_downgrade_and_friend_vip` |
| Individual Monthly | $20 / 1,000 EGP | Unlimited ops/rows, no watermark, saved recipes + premium packs, 400 AI calls/month | `paddle_and_paymob_full_lifecycle_security_and_idempotency_suite` |
| Individual Annual | $168 / 8,400 EGP | Same as monthly, billed annually | `paddle_and_paymob_full_lifecycle_security_and_idempotency_suite` |
| Owner's Friend (VIP) | $0 / 0 EGP | All paid features + 400 AI calls/month, granted/revoked exclusively by sealed Owner with step-up TOTP | `trial_anti_abuse_ai_cap_auto_downgrade_and_friend_vip` + `zero_trust_guard.rs` |
| Organization Seat | $17 / 850 EGP/seat/mo ($204 / 10,200 EGP/seat/yr) | Minimum 3 seats enforced (`SF-SYN-004`), 400 AI calls/seat/month, teams & RBAC | `organization_enforces_min_three_seats_roles_and_sso_status` |
| Pay-Per-File | $2 / 50 EGP | Up to 100,000 rows, removes watermark, no AI, no premium packs | `free_plan_boundaries_credits_pay_per_file_and_monthly_reset` |
| Credit Pack (25 ops) | $5 / 250 EGP | 25 non-expiring operations, removes watermark, no AI, no premium packs | `free_plan_boundaries_credits_pay_per_file_and_monthly_reset` |

## 2. Billing Security & Webhook Contract Verification (`crates/sf-server/src/billing/mod.rs`)

- **Paddle HMAC-SHA256 (`Paddle-Signature: ts=...;h1=...`):** Verified over `"{ts}:{raw_body}"` using `subtle::ConstantTimeEq` and 5-minute (`300s`) anti-replay window (`SF-PAY-002`, `SF-PAY-003`).
- **Paymob HMAC-SHA512:** Verified over the official 20-key sorted Paymob transaction concatenation string using `subtle::ConstantTimeEq` (`SF-PAY-002`).
- **Idempotency & Out-of-Order Safety:** Duplicate `(provider, external_event_id)` events return `idempotent_duplicate_ignored` without double-granting credits or double-mutating subscriptions; stale older `occurred_at_unix` events are safely ignored (`stale_out_of_order_ignored`).
- **Concurrency Safety:** `concurrent_consumption_never_exceeds_free_five_op_quota` launches 12 concurrent tasks against a 5-operation Free workspace and proves exactly 5 succeed and 7 fail with `SF-LIC-005`.
- **Refund Policy & Grace Period:** 3-day past-due grace period with warning banner (`show_past_due_banner`); 3-day owner refund window enforced by default (`SF-PAY-004`), with explicit statutory legal override flag for mandatory Paddle legal webhooks.
- **Unconfigured Fail-Closed Guarantee:** Calling checkout or webhooks when Paddle/Paymob keys are unset returns `SF-PAY-001` (`503 Service Unavailable`) and `/v1/billing/status` reports `NOT_CONFIGURED`.
