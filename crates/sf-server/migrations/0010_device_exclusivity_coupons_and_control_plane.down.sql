-- Rollback Migration 0010: drop announcements, support tickets, coupons, device replacement history,
-- and restore prior columns/constraints.

ALTER TABLE billing_invoices DROP CONSTRAINT IF EXISTS billing_invoices_status_check;
ALTER TABLE billing_invoices
    ADD CONSTRAINT billing_invoices_status_check
    CHECK (status IN ('paid', 'pending', 'refunded', 'void'));

DROP TABLE IF EXISTS platform_announcements;
DROP TABLE IF EXISTS support_tickets;
DROP TABLE IF EXISTS coupon_redemptions;
DROP TABLE IF EXISTS coupons;
DROP TABLE IF EXISTS device_replacement_events;

ALTER TABLE sessions
    DROP COLUMN IF EXISTS revocation_reason,
    DROP COLUMN IF EXISTS session_generation;

ALTER TABLE devices
    DROP COLUMN IF EXISTS replaced_by_device_id,
    DROP COLUMN IF EXISTS public_key_hex;

ALTER TABLE users
    DROP COLUMN IF EXISTS session_generation;
