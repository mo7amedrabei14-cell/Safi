#!/usr/bin/env node
/**
 * Validates `config/business.json` — the single source of truth for business,
 * auth, roles, and billing decisions. Fails loudly on any divergence.
 */
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const configPath = resolve(here, "..", "config", "business.json");

const errors = [];

function fail(message) {
  errors.push(message);
}

function equal(actual, expected, label) {
  if (actual !== expected) {
    fail(`${label}: expected ${JSON.stringify(expected)}, received ${JSON.stringify(actual)}`);
  }
}

function deepEqual(actual, expected, label) {
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    fail(`${label}: expected ${JSON.stringify(expected)}, received ${JSON.stringify(actual)}`);
  }
}

let cfg;
try {
  cfg = JSON.parse(readFileSync(configPath, "utf8"));
} catch (error) {
  console.error(`business.json INVALID: ${error.message}`);
  process.exit(1);
}

const OWNER_EMAIL = "mo7amed.rabei14@gmail.com";

// Brand and owner
equal(cfg.brand?.name_en, "SafiData", "brand.name_en");
equal(cfg.brand?.name_ar, "صافي البيانات", "brand.name_ar");
equal(cfg.brand?.status, "CONFIRMED_V1", "brand.status");
equal(cfg.owner?.email, OWNER_EMAIL, "owner.email");
equal(cfg.owner?.strict_single_owner_lock, true, "owner.strict_single_owner_lock");
deepEqual(
  cfg.owner?.exclusive_permissions,
  [
    "access_owner_dashboard",
    "grant_or_revoke_friend_role",
    "cancel_subscription",
    "issue_refund",
    "extend_trial",
    "suspend_user",
    "toggle_maintenance_mode"
  ],
  "owner.exclusive_permissions"
);
equal(cfg.support?.email, OWNER_EMAIL, "support.email");
equal(cfg.support?.show_publicly, true, "support.show_publicly");

// Authentication
equal(cfg.auth?.min_password_length, 8, "auth.min_password_length");
equal(cfg.auth?.password_visibility_toggle, true, "auth.password_visibility_toggle");
equal(cfg.auth?.prevent_duplicate_accounts, true, "auth.prevent_duplicate_accounts");
deepEqual(
  cfg.auth?.unique_account_keys,
  ["normalized_email", "verified_phone"],
  "auth.unique_account_keys"
);
deepEqual(cfg.auth?.oauth_providers, ["google", "apple", "github"], "auth.oauth_providers");
equal(cfg.auth?.motion_animations_enabled, true, "auth.motion_animations_enabled");

// Roles
equal(cfg.roles?.owner?.email_match, OWNER_EMAIL, "roles.owner.email_match");
equal(cfg.roles?.owner?.full_platform_access, true, "roles.owner.full_platform_access");
equal(cfg.roles?.owner?.owner_console_access, true, "roles.owner.owner_console_access");
const friend = cfg.roles?.friend;
equal(friend?.label_ar, "صديق المالك (VIP Friend)", "roles.friend.label_ar");
equal(friend?.label_en, "Owner's Friend (VIP)", "roles.friend.label_en");
equal(
  friend?.["granted_ exclusively_by"],
  OWNER_EMAIL,
  "roles.friend.granted_ exclusively_by"
);
deepEqual(friend?.price, { USD: 0, EGP: 0 }, "roles.friend.price");
equal(friend?.all_paid_features_open, true, "roles.friend.all_paid_features_open");
equal(friend?.removes_watermark, true, "roles.friend.removes_watermark");
equal(friend?.saved_recipes, true, "roles.friend.saved_recipes");
equal(
  friend?.includes_premium_recipe_packs,
  true,
  "roles.friend.includes_premium_recipe_packs"
);
equal(friend?.ai_calls_per_month, 400, "roles.friend.ai_calls_per_month");
equal(
  friend?.expires,
  "never_unless_revoked_by_owner",
  "roles.friend.expires"
);

// Currency, trial, free plan
equal(cfg.currency?.egypt, "EGP", "currency.egypt");
equal(cfg.currency?.rest_of_world, "USD", "currency.rest_of_world");
equal(
  cfg.currency?.choose_by,
  "billing country (fallback: IP country)",
  "currency.choose_by"
);
equal(cfg.trial?.days, 7, "trial.days");
equal(cfg.trial?.all_features_open, true, "trial.all_features_open");
equal(cfg.trial?.requires_card, false, "trial.requires_card");
equal(cfg.trial?.requires_verified_email, true, "trial.requires_verified_email");
equal(cfg.trial?.requires_verified_phone, true, "trial.requires_verified_phone");
deepEqual(
  cfg.trial?.phone_verification,
  { primary: "sms", fallback: "whatsapp" },
  "trial.phone_verification"
);
deepEqual(
  cfg.trial?.one_trial_per,
  ["verified_email", "verified_phone", "device_fingerprint"],
  "trial.one_trial_per"
);
equal(cfg.trial?.ai_calls_cap, 25, "trial.ai_calls_cap");
equal(cfg.trial?.after_trial, "downgrade_to_free_plan", "trial.after_trial");

equal(cfg.free_plan?.operations_per_month, 5, "free_plan.operations_per_month");
equal(cfg.free_plan?.max_rows, 5000, "free_plan.max_rows");
equal(cfg.free_plan?.max_columns, 20, "free_plan.max_columns");
deepEqual(
  cfg.free_plan?.cleaning_types,
  ["normalize_text", "dedupe_exact", "blanks", "dates", "numbers"],
  "free_plan.cleaning_types"
);
equal(cfg.free_plan?.watermark, "on_everything", "free_plan.watermark");
equal(
  cfg.free_plan?.watermark_text,
  "Processed by SafiData (Free Plan) — صافي البيانات",
  "free_plan.watermark_text"
);
equal(cfg.free_plan?.saved_recipes, false, "free_plan.saved_recipes");
equal(
  cfg.free_plan?.recipes_after_downgrade,
  "read_only_locked",
  "free_plan.recipes_after_downgrade"
);
equal(cfg.free_plan?.premium_recipe_packs, false, "free_plan.premium_recipe_packs");
equal(cfg.free_plan?.ai, false, "free_plan.ai");
equal(cfg.free_plan?.teams, false, "free_plan.teams");

// Plans
const plans = Array.isArray(cfg.plans) ? cfg.plans : [];
equal(plans.length, 3, "plans.length");
const [monthly, annual, organization] = plans;
equal(monthly?.id, "individual_monthly", "plans[0].id");
equal(monthly?.audience, "individual", "plans[0].audience");
equal(monthly?.billing, "monthly", "plans[0].billing");
deepEqual(monthly?.price, { USD: 20, EGP: 1000 }, "plans[0].price");
equal(monthly?.ai_calls_per_month, 400, "plans[0].ai_calls_per_month");
equal(
  monthly?.includes_premium_recipe_packs,
  true,
  "plans[0].includes_premium_recipe_packs"
);
equal(
  monthly?.recipe_packs_duration,
  "active_subscription_only",
  "plans[0].recipe_packs_duration"
);

equal(annual?.id, "individual_annual", "plans[1].id");
equal(annual?.audience, "individual", "plans[1].audience");
equal(annual?.billing, "annual", "plans[1].billing");
deepEqual(annual?.price, { USD: 168, EGP: 8400 }, "plans[1].price");
equal(annual?.ai_calls_per_month, 400, "plans[1].ai_calls_per_month");
equal(
  annual?.includes_premium_recipe_packs,
  true,
  "plans[1].includes_premium_recipe_packs"
);
equal(
  annual?.recipe_packs_duration,
  "active_subscription_only",
  "plans[1].recipe_packs_duration"
);

equal(organization?.id, "organization_seat", "plans[2].id");
equal(organization?.audience, "organization", "plans[2].audience");
equal(organization?.billing, "annual_per_seat", "plans[2].billing");
equal(organization?.min_seats, 3, "plans[2].min_seats");
deepEqual(
  organization?.price_per_seat_per_month,
  { USD: 17, EGP: 850 },
  "plans[2].price_per_seat_per_month"
);
deepEqual(
  organization?.price_per_seat_per_year,
  { USD: 204, EGP: 10200 },
  "plans[2].price_per_seat_per_year"
);
equal(
  organization?.ai_calls_per_seat_per_month,
  400,
  "plans[2].ai_calls_per_seat_per_month"
);
equal(
  organization?.includes_premium_recipe_packs,
  true,
  "plans[2].includes_premium_recipe_packs"
);
equal(
  organization?.recipe_packs_duration,
  "active_subscription_only",
  "plans[2].recipe_packs_duration"
);

// Micro purchases
const payPerFile = cfg.micro_purchases?.pay_per_file;
equal(payPerFile?.enabled, true, "micro_purchases.pay_per_file.enabled");
deepEqual(payPerFile?.price, { USD: 2, EGP: 50 }, "micro_purchases.pay_per_file.price");
equal(payPerFile?.removes_watermark, true, "micro_purchases.pay_per_file.removes_watermark");
equal(payPerFile?.max_rows, 100000, "micro_purchases.pay_per_file.max_rows");
equal(
  payPerFile?.includes_premium_recipe_packs,
  false,
  "micro_purchases.pay_per_file.includes_premium_recipe_packs"
);
equal(payPerFile?.includes_ai, false, "micro_purchases.pay_per_file.includes_ai");

const creditPack = cfg.micro_purchases?.credit_pack_25;
equal(creditPack?.enabled, true, "micro_purchases.credit_pack_25.enabled");
equal(creditPack?.operations, 25, "micro_purchases.credit_pack_25.operations");
deepEqual(creditPack?.price, { USD: 5, EGP: 250 }, "micro_purchases.credit_pack_25.price");
equal(creditPack?.expires, false, "micro_purchases.credit_pack_25.expires");
equal(
  creditPack?.removes_watermark,
  true,
  "micro_purchases.credit_pack_25.removes_watermark"
);
equal(
  creditPack?.includes_premium_recipe_packs,
  false,
  "micro_purchases.credit_pack_25.includes_premium_recipe_packs"
);
equal(creditPack?.includes_ai, false, "micro_purchases.credit_pack_25.includes_ai");

equal(
  cfg.OneTimeDesktopLicense?.enabled,
  false,
  "OneTimeDesktopLicense.enabled"
);

// Premium recipe packs policy
const packs = cfg.premium_recipe_packs_policy;
equal(packs?.sold_separately, false, "premium_recipe_packs_policy.sold_separately");
equal(
  packs?.access_rule,
  "ACTIVE_SUBSCRIBERS_AND_FRIENDS_ONLY",
  "premium_recipe_packs_policy.access_rule"
);
deepEqual(
  packs?.eligible_plans,
  ["individual_monthly", "individual_annual", "organization_seat", "friend"],
  "premium_recipe_packs_policy.eligible_plans"
);
equal(
  packs?.available_to_credit_pack_users,
  false,
  "premium_recipe_packs_policy.available_to_credit_pack_users"
);
equal(
  packs?.available_to_pay_per_file_users,
  false,
  "premium_recipe_packs_policy.available_to_pay_per_file_users"
);
equal(
  packs?.access_duration,
  "valid_only_while_subscription_or_friend_role_is_active",
  "premium_recipe_packs_policy.access_duration"
);

// Owner dashboard analytics
const analytics = cfg.owner_dashboard_analytics;
equal(
  analytics?.layout,
  "full_width_executive_command_center",
  "owner_dashboard_analytics.layout"
);
deepEqual(
  analytics?.time_horizons,
  ["monthly", "quarterly", "annual"],
  "owner_dashboard_analytics.time_horizons"
);
deepEqual(analytics?.quarters, ["Q1", "Q2", "Q3", "Q4"], "owner_dashboard_analytics.quarters");
deepEqual(
  analytics?.tracked_metrics,
  [
    "total_users",
    "active_trials",
    "free_users",
    "monthly_subscribers",
    "annual_subscribers",
    "organization_seats",
    "friend_vip_users",
    "credit_pack_buyers",
    "pay_per_file_buyers",
    "mrr_usd_egp",
    "arr_usd_egp",
    "quarterly_revenue_growth",
    "churn_rate",
    "conversion_rate",
    "rows_cleaned_total",
    "pii_masked_total",
    "ai_calls_used"
  ],
  "owner_dashboard_analytics.tracked_metrics"
);

// Billing, pricing, refunds, payments, hosting
equal(cfg.billing_grace_period?.past_due_days, 3, "billing_grace_period.past_due_days");
equal(
  cfg.billing_grace_period?.show_warning_banner,
  true,
  "billing_grace_period.show_warning_banner"
);
equal(cfg.discounts, "none_at_launch", "discounts");
equal(cfg.price_policy?.status, "HYPOTHESIS_UNTIL_P0", "price_policy.status");
equal(cfg.price_policy?.egp_review, "every 3 months", "price_policy.egp_review");

const refunds = cfg.refunds;
equal(
  refunds?.how,
  "only by emailing the owner; the owner activates the refund manually",
  "refunds.how"
);
equal(refunds?.window_days, 3, "refunds.window_days");
equal(refunds?.inside_window, "negotiable", "refunds.inside_window");
equal(
  refunds?.after_window,
  "no refund, even if the customer contacts the owner",
  "refunds.after_window"
);
equal(refunds?.show_on_pricing_page, true, "refunds.show_on_pricing_page");
deepEqual(
  refunds?.owner_console_actions,
  ["cancel_subscription", "issue_refund"],
  "refunds.owner_console_actions"
);
equal(
  refunds?.eu_consumer_law_and_paddle_policy_check,
  "enforce_3_day_owner_policy_by_default_allow_paddle_mandatory_legal_webhook_only",
  "refunds.eu_consumer_law_and_paddle_policy_check"
);

equal(cfg.payments?.international, "paddle", "payments.international");
equal(cfg.payments?.egypt, "paymob", "payments.egypt");
equal(cfg.payments?.accounts_created, false, "payments.accounts_created");
equal(cfg.hosting?.region_now, "europe", "hosting.region_now");
equal(cfg.hosting?.egypt_option, "later", "hosting.egypt_option");

// Languages and AI
deepEqual(
  cfg.languages?.launch,
  ["ar", "en", "fr", "es", "tr", "fa", "ur"],
  "languages.launch"
);
equal(cfg.languages?.default, "ar", "languages.default");
deepEqual(cfg.languages?.rtl, ["ar", "fa", "ur"], "languages.rtl");

equal(cfg.ai?.in_1_0, true, "ai.in_1_0");
equal(cfg.ai?.requires_user_consent, true, "ai.requires_user_consent");
equal(cfg.ai?.local_pii_masking_before_send, true, "ai.local_pii_masking_before_send");
deepEqual(
  cfg.ai?.usage_limit,
  { trial_total_calls: 25, paid_monthly_calls: 400, friend_monthly_calls: 400 },
  "ai.usage_limit"
);
equal(
  cfg.ai?.egypt_cross_border_legal_check,
  "mask_pii_locally_on_device_and_require_explicit_user_consent_checkbox",
  "ai.egypt_cross_border_legal_check"
);

equal(
  cfg.data_retention?.user_data_days_after_account_deletion,
  0,
  "data_retention.user_data_days_after_account_deletion"
);
equal(
  cfg.data_retention?.keep_statutory_tax_invoices_only,
  true,
  "data_retention.keep_statutory_tax_invoices_only"
);

// The launch constitution contains zero open questions.
equal(Array.isArray(cfg.open_items) ? cfg.open_items.length : -1, 0, "open_items.length");

if (errors.length > 0) {
  console.error("business.json INVALID");
  for (const message of errors) {
    console.error(` - ${message}`);
  }
  process.exit(1);
}

console.log("business.json OK");
