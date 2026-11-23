-- Rollback Migration 0016: Desktop-to-Website Billing Handoffs & Website Billing Sessions.

DROP TABLE IF EXISTS billing_web_sessions;
DROP TABLE IF EXISTS desktop_billing_handoffs;
