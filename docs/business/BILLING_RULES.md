# Billing Rules — قواعد الفوترة

Source of truth: `config/business.json`. This document mirrors the constitution; any divergence is a defect and `node tools/validate-config.mjs` must fail.

## Currency
- Egypt: **EGP** (Paymob).
- Rest of the world: **USD** (Paddle).
- Selection rule: billing country, falling back to IP country.

## Trial
- 7 days, no card required.
- Requires verified email **and** verified phone (SMS primary, WhatsApp fallback).
- One trial per verified email, verified phone, and device fingerprint.
- AI cap during trial: **25 calls**.
- After the trial: automatic downgrade to the Free plan.

## Free plan
- 5 operations per month, max 5,000 rows, max 20 columns.
- Cleaning types: normalize_text, dedupe_exact, blanks, dates, numbers.
- Watermark on everything: `Processed by SafiData (Free Plan) — صافي البيانات`.
- No saved recipes (existing recipes become read-only and locked), no premium recipe packs, no AI, no teams.

## Paid plans
| Plan | Billing | USD | EGP | AI calls / month |
| --- | --- | --- | --- | --- |
| `individual_monthly` | monthly | 20 | 1,000 | 400 |
| `individual_annual` | annual | 168 | 8,400 | 400 |
| `organization_seat` | annual per seat (min 3 seats) | 17/month, 204/year per seat | 850/month, 10,200/year per seat | 400 per seat |

The annual individual plan equals 12 monthly payments minus 30% (`12 × 20 = 240`, `240 × 0.7 = 168`). The organization seat equals the individual monthly price minus 15% (`20 × 0.85 = 17`).

## Micro purchases
- **Pay per file**: 2 USD / 50 EGP, removes the watermark, max 100,000 rows. No AI, no premium recipe packs.
- **Credit pack 25**: 5 USD / 250 EGP, 25 operations, never expires, removes the watermark. No AI, no premium recipe packs.

## One-time desktop license
Disabled. Rejected by the owner. The desktop app requires an active subscription, the Friend role, or an active trial.

## Premium recipe packs
- Not sold separately.
- Available only to active subscribers and Friend VIP users: `individual_monthly`, `individual_annual`, `organization_seat`, `friend`.
- Never available to credit-pack or pay-per-file buyers.
- Access is valid only while the subscription or Friend role is active.

## Grace period
Past due for 3 days with a visible warning banner before access is restricted.

## Discounts and price policy
- No launch discounts.
- Prices are a hypothesis until P0; the EGP review happens every 3 months.

## Refunds
- Only by emailing the owner (`mo7amed.rabei14@gmail.com`); the owner activates the refund manually.
- Window: **3 days**. Inside the window: negotiable. After the window: no refund.
- Displayed on the pricing page.
- Owner console actions: `cancel_subscription`, `issue_refund`.
- EU consumer law and Paddle policy: enforce the 3-day owner policy by default and allow only Paddle's mandatory legal webhook path.

## Payment providers
International: Paddle. Egypt: Paymob. No accounts have been created yet.

## Roles
- Owner: exclusively `mo7amed.rabei14@gmail.com` after normalization, with `strict_single_owner_lock`.
- Friend VIP: granted and revoked only by the owner, price 0 USD / 0 EGP, all paid features open, no watermark, saved recipes, premium recipe packs, 400 AI calls/month, never expires unless revoked.

## Open items
None. `open_items` is empty at launch.
