# Product Decisions — قرارات المنتج

Source of truth: `config/business.json` (status `CONFIRMED_V1`). This document records the locked launch decisions that the code must implement.

## Identity and authentication
- Minimum password length is **8 characters** (not 12). Passwords can be shown or hidden with an eye toggle.
- Duplicate accounts are prevented: normalized email and verified phone are unique account keys. Sign-up with an existing email or phone is rejected and the interface offers a direct switch to sign-in.
- OAuth providers: Google, Apple, GitHub.
- Motion animations are enabled in the auth portal, with reduced-motion respected.
- Owner identity is a strict, singular lock: after `trim().to_ascii_lowercase()`, only `mo7amed.rabei14@gmail.com` is the owner. No other account can pass an owner check or grant the Friend role.

## Owner-only permissions
`access_owner_dashboard`, `grant_or_revoke_friend_role`, `cancel_subscription`, `issue_refund`, `extend_trial`, `suspend_user`, `toggle_maintenance_mode`.

## Friend VIP role
- Labels: `صديق المالك (VIP Friend)` / `Owner's Friend (VIP)`.
- Granted and revoked exclusively by the owner from the owner dashboard, never purchased or self-served.
- Price: 0 USD / 0 EGP.
- Full paid plan features: all paid features open, watermark removed, saved recipes, premium recipe packs, 400 AI calls per month.
- Expires never, unless the owner revokes it.

## Free plan and trial
See `BILLING_RULES.md`. The trial requires verified email and phone, uses SMS with a WhatsApp fallback, and caps AI at 25 calls.

## Owner dashboard
- Full-width executive command center (`full_width_executive_command_center`).
- Time horizons: monthly, quarterly, annual. Quarters Q1–Q4.
- Tracked metrics: total users, active trials, free users, monthly subscribers, annual subscribers, organization seats, Friend VIP users, credit pack buyers, pay-per-file buyers, MRR (USD/EGP), ARR (USD/EGP), quarterly revenue growth, churn rate, conversion rate, rows cleaned, PII masked, AI calls used.
- The six-card owner console (release, messaging, users, feature flags, support, audit) plus the Zero Trust / YubiKey guard, emergency maintenance mode, and BLAKE3 audit chain remain intact.

## Languages
Launch languages: ar, en, fr, es, tr, fa, ur. Default: ar. RTL: ar, fa, ur.

## AI
Available in 1.0, requires explicit user consent, masks PII locally on-device before any send. Legal position for Egypt: mask locally and require the consent checkbox.

## Data retention
0 days of user data after account deletion; only statutory tax invoices are retained.

## Open items
None.
