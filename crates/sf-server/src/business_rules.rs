//! Single Source of Truth for SafiData business rules, prices, and entitlement limits.
//!
//! Parsed directly at compile-time/startup from `config/business.json` so neither billing prices
//! nor entitlement limits can ever drift from the owner-approved business configuration.

use std::sync::LazyLock;

use serde::Deserialize;

use crate::billing::{BillingCurrency, BillingProvider, CheckoutProduct};

const RAW_BUSINESS_JSON: &str = include_str!("../../../config/business.json");

#[derive(Debug, Clone, Deserialize)]
struct RawCurrencyPair {
    #[serde(rename = "USD")]
    usd: u64,
    #[serde(rename = "EGP")]
    egp: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct RawFriendRole {
    ai_calls_per_month: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct RawRoles {
    friend: RawFriendRole,
}

#[derive(Debug, Clone, Deserialize)]
struct RawTrial {
    days: i64,
    ai_calls_cap: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct RawFreePlan {
    operations_per_month: u32,
    max_rows: u64,
    max_columns: u32,
    watermark_text: String,
    saved_recipes: bool,
    premium_recipe_packs: bool,
    ai: bool,
    teams: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct RawPlan {
    id: String,
    price: Option<RawCurrencyPair>,
    min_seats: Option<u32>,
    price_per_seat_per_month: Option<RawCurrencyPair>,
    price_per_seat_per_year: Option<RawCurrencyPair>,
    ai_calls_per_month: Option<u32>,
    ai_calls_per_seat_per_month: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawPayPerFile {
    price: RawCurrencyPair,
    max_rows: u64,
    removes_watermark: bool,
    includes_premium_recipe_packs: bool,
    includes_ai: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct RawCreditPack25 {
    operations: u32,
    price: RawCurrencyPair,
    removes_watermark: bool,
    includes_premium_recipe_packs: bool,
    includes_ai: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct RawMicroPurchases {
    pay_per_file: RawPayPerFile,
    credit_pack_25: RawCreditPack25,
}

#[derive(Debug, Clone, Deserialize)]
struct RawGracePeriod {
    past_due_days: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct RawRefunds {
    window_days: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct RawBusinessConfig {
    roles: RawRoles,
    trial: RawTrial,
    free_plan: RawFreePlan,
    plans: Vec<RawPlan>,
    micro_purchases: RawMicroPurchases,
    billing_grace_period: RawGracePeriod,
    refunds: RawRefunds,
}

/// Validated, immutable runtime business rules loaded directly from `config/business.json`.
#[derive(Debug, Clone)]
pub struct BusinessRules {
    pub free_ops_per_month: u32,
    pub free_max_rows: u64,
    pub free_max_cols: u32,
    pub free_watermark_text: String,
    pub free_saved_recipes: bool,
    pub free_premium_recipe_packs: bool,
    pub free_ai: bool,
    pub free_teams: bool,

    pub trial_days: i64,
    pub trial_duration_secs: i64,
    pub trial_ai_calls_cap: u32,

    pub paid_ai_calls_per_month: u32,
    pub friend_ai_calls_per_month: u32,
    pub org_ai_calls_per_seat_per_month: u32,
    pub org_min_seats: u32,

    pub billing_grace_period_secs: i64,
    pub refund_window_secs: i64,

    // Approved major-unit prices (converted to minor units * 100 on checkout)
    pub individual_monthly_usd: u64,
    pub individual_monthly_egp: u64,
    pub individual_annual_usd: u64,
    pub individual_annual_egp: u64,
    pub org_seat_monthly_usd: u64,
    pub org_seat_monthly_egp: u64,
    pub org_seat_annual_usd: u64,
    pub org_seat_annual_egp: u64,

    pub pay_per_file_usd: u64,
    pub pay_per_file_egp: u64,
    pub pay_per_file_max_rows: u64,
    pub pay_per_file_removes_watermark: bool,
    pub pay_per_file_includes_ai: bool,
    pub pay_per_file_includes_premium_packs: bool,

    pub credit_pack_25_operations: u32,
    pub credit_pack_25_usd: u64,
    pub credit_pack_25_egp: u64,
    pub credit_pack_25_removes_watermark: bool,
    pub credit_pack_25_includes_ai: bool,
    pub credit_pack_25_includes_premium_packs: bool,
}

impl BusinessRules {
    /// Parse and validate `config/business.json`. Panics at startup if the configuration is invalid
    /// or missing any mandatory approved plan/price/limit field.
    pub fn parse_json(raw_json: &str) -> Result<Self, String> {
        let cfg: RawBusinessConfig = serde_json::from_str(raw_json)
            .map_err(|e| format!("failed to parse config/business.json: {e}"))?;

        let find_plan = |id: &str| -> Result<&RawPlan, String> {
            cfg.plans
                .iter()
                .find(|p| p.id == id)
                .ok_or_else(|| format!("missing plan `{id}` in config/business.json"))
        };

        let ind_m = find_plan("individual_monthly")?;
        let ind_a = find_plan("individual_annual")?;
        let org = find_plan("organization_seat")?;

        let ind_m_price = ind_m
            .price
            .as_ref()
            .ok_or("missing price for individual_monthly")?;
        let ind_a_price = ind_a
            .price
            .as_ref()
            .ok_or("missing price for individual_annual")?;
        let org_m_price = org
            .price_per_seat_per_month
            .as_ref()
            .ok_or("missing price_per_seat_per_month for organization_seat")?;
        let org_a_price = org
            .price_per_seat_per_year
            .as_ref()
            .ok_or("missing price_per_seat_per_year for organization_seat")?;

        if cfg.free_plan.operations_per_month == 0
            || cfg.free_plan.max_rows == 0
            || cfg.free_plan.max_columns == 0
        {
            return Err("free_plan limits must be > 0".to_owned());
        }
        if cfg.free_plan.ai
            || cfg.free_plan.teams
            || cfg.free_plan.saved_recipes
            || cfg.free_plan.premium_recipe_packs
        {
            return Err(
                "free_plan must keep ai, teams, saved_recipes, and premium_recipe_packs disabled"
                    .to_owned(),
            );
        }
        if cfg.micro_purchases.pay_per_file.includes_ai
            || cfg
                .micro_purchases
                .pay_per_file
                .includes_premium_recipe_packs
            || cfg.micro_purchases.credit_pack_25.includes_ai
            || cfg
                .micro_purchases
                .credit_pack_25
                .includes_premium_recipe_packs
        {
            return Err("micro-purchases must not unlock AI or premium recipe packs".to_owned());
        }

        let org_min_seats = org.min_seats.ok_or("missing organization_seat.min_seats")?;
        let paid_ai = ind_m
            .ai_calls_per_month
            .ok_or("missing individual_monthly.ai_calls_per_month")?;
        let org_ai = org
            .ai_calls_per_seat_per_month
            .ok_or("missing organization_seat.ai_calls_per_seat_per_month")?;

        Ok(Self {
            free_ops_per_month: cfg.free_plan.operations_per_month,
            free_max_rows: cfg.free_plan.max_rows,
            free_max_cols: cfg.free_plan.max_columns,
            free_watermark_text: cfg.free_plan.watermark_text,
            free_saved_recipes: cfg.free_plan.saved_recipes,
            free_premium_recipe_packs: cfg.free_plan.premium_recipe_packs,
            free_ai: cfg.free_plan.ai,
            free_teams: cfg.free_plan.teams,

            trial_days: cfg.trial.days,
            trial_duration_secs: cfg.trial.days * 86_400,
            trial_ai_calls_cap: cfg.trial.ai_calls_cap,

            paid_ai_calls_per_month: paid_ai,
            friend_ai_calls_per_month: cfg.roles.friend.ai_calls_per_month,
            org_ai_calls_per_seat_per_month: org_ai,
            org_min_seats,

            billing_grace_period_secs: cfg.billing_grace_period.past_due_days * 86_400,
            refund_window_secs: cfg.refunds.window_days * 86_400,

            individual_monthly_usd: ind_m_price.usd,
            individual_monthly_egp: ind_m_price.egp,
            individual_annual_usd: ind_a_price.usd,
            individual_annual_egp: ind_a_price.egp,
            org_seat_monthly_usd: org_m_price.usd,
            org_seat_monthly_egp: org_m_price.egp,
            org_seat_annual_usd: org_a_price.usd,
            org_seat_annual_egp: org_a_price.egp,

            pay_per_file_usd: cfg.micro_purchases.pay_per_file.price.usd,
            pay_per_file_egp: cfg.micro_purchases.pay_per_file.price.egp,
            pay_per_file_max_rows: cfg.micro_purchases.pay_per_file.max_rows,
            pay_per_file_removes_watermark: cfg.micro_purchases.pay_per_file.removes_watermark,
            pay_per_file_includes_ai: cfg.micro_purchases.pay_per_file.includes_ai,
            pay_per_file_includes_premium_packs: cfg
                .micro_purchases
                .pay_per_file
                .includes_premium_recipe_packs,

            credit_pack_25_operations: cfg.micro_purchases.credit_pack_25.operations,
            credit_pack_25_usd: cfg.micro_purchases.credit_pack_25.price.usd,
            credit_pack_25_egp: cfg.micro_purchases.credit_pack_25.price.egp,
            credit_pack_25_removes_watermark: cfg.micro_purchases.credit_pack_25.removes_watermark,
            credit_pack_25_includes_ai: cfg.micro_purchases.credit_pack_25.includes_ai,
            credit_pack_25_includes_premium_packs: cfg
                .micro_purchases
                .credit_pack_25
                .includes_premium_recipe_packs,
        })
    }

    /// Resolve authoritative provider, currency, and minor-unit price directly from `config/business.json`.
    #[must_use]
    pub fn resolve_provider_and_price(
        &self,
        country_iso2: &str,
        product: CheckoutProduct,
        seats: u32,
    ) -> (BillingProvider, BillingCurrency, u64) {
        let is_egypt = country_iso2.trim().eq_ignore_ascii_case("EG");
        let seat_count = u64::from(seats.max(self.org_min_seats));
        if is_egypt {
            let major = match product {
                CheckoutProduct::IndividualMonthly => self.individual_monthly_egp,
                CheckoutProduct::IndividualAnnual => self.individual_annual_egp,
                CheckoutProduct::OrganizationSeatAnnual => self.org_seat_annual_egp * seat_count,
                CheckoutProduct::PayPerFile => self.pay_per_file_egp,
                CheckoutProduct::CreditPack25 => self.credit_pack_25_egp,
            };
            (BillingProvider::Paymob, BillingCurrency::Egp, major * 100)
        } else {
            let major = match product {
                CheckoutProduct::IndividualMonthly => self.individual_monthly_usd,
                CheckoutProduct::IndividualAnnual => self.individual_annual_usd,
                CheckoutProduct::OrganizationSeatAnnual => self.org_seat_annual_usd * seat_count,
                CheckoutProduct::PayPerFile => self.pay_per_file_usd,
                CheckoutProduct::CreditPack25 => self.credit_pack_25_usd,
            };
            (BillingProvider::Paddle, BillingCurrency::Usd, major * 100)
        }
    }
}

/// Global validated business rules loaded from `config/business.json`.
pub static BUSINESS_RULES: LazyLock<BusinessRules> = LazyLock::new(|| {
    BusinessRules::parse_json(RAW_BUSINESS_JSON)
        .expect("config/business.json must be valid and complete")
});

/// Accessor for the authoritative business rules parsed from `config/business.json`.
#[must_use]
pub fn rules() -> &'static BusinessRules {
    &BUSINESS_RULES
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing::resolve_provider_and_price;
    use crate::entitlements::{
        BILLING_GRACE_PERIOD_SECS, FREE_MAX_COLS, FREE_MAX_ROWS, FREE_OPS_PER_MONTH, ORG_MIN_SEATS,
        PAID_AI_CALLS_PER_MONTH, PAY_PER_FILE_MAX_ROWS, TRIAL_AI_CALLS_CAP, TRIAL_DURATION_SECS,
        WATERMARK_TEXT,
    };

    #[test]
    fn runtime_prices_and_entitlement_limits_match_business_json_exactly() {
        let r = rules();
        // Verify constants & runtime rules never diverge from config/business.json
        assert_eq!(r.free_ops_per_month, FREE_OPS_PER_MONTH);
        assert_eq!(r.free_max_rows, FREE_MAX_ROWS);
        assert_eq!(r.free_max_cols, FREE_MAX_COLS);
        assert_eq!(r.free_watermark_text, WATERMARK_TEXT);
        assert!(!r.free_saved_recipes);
        assert!(!r.free_premium_recipe_packs);
        assert!(!r.free_ai);
        assert!(!r.free_teams);

        assert_eq!(r.trial_duration_secs, TRIAL_DURATION_SECS);
        assert_eq!(r.trial_ai_calls_cap, TRIAL_AI_CALLS_CAP);
        assert_eq!(r.paid_ai_calls_per_month, PAID_AI_CALLS_PER_MONTH);
        assert_eq!(r.friend_ai_calls_per_month, PAID_AI_CALLS_PER_MONTH);
        assert_eq!(r.org_min_seats, ORG_MIN_SEATS);
        assert_eq!(r.billing_grace_period_secs, BILLING_GRACE_PERIOD_SECS);
        assert_eq!(r.pay_per_file_max_rows, PAY_PER_FILE_MAX_ROWS);

        // Verify billing::resolve_provider_and_price delegates directly to config/business.json
        assert_eq!(
            resolve_provider_and_price("US", CheckoutProduct::IndividualMonthly, 1),
            (
                BillingProvider::Paddle,
                BillingCurrency::Usd,
                r.individual_monthly_usd * 100
            )
        );
        assert_eq!(
            resolve_provider_and_price("EG", CheckoutProduct::IndividualMonthly, 1),
            (
                BillingProvider::Paymob,
                BillingCurrency::Egp,
                r.individual_monthly_egp * 100
            )
        );
        assert_eq!(
            resolve_provider_and_price("US", CheckoutProduct::IndividualAnnual, 1),
            (
                BillingProvider::Paddle,
                BillingCurrency::Usd,
                r.individual_annual_usd * 100
            )
        );
        assert_eq!(
            resolve_provider_and_price("EG", CheckoutProduct::IndividualAnnual, 1),
            (
                BillingProvider::Paymob,
                BillingCurrency::Egp,
                r.individual_annual_egp * 100
            )
        );
        assert_eq!(
            resolve_provider_and_price("US", CheckoutProduct::OrganizationSeatAnnual, 4),
            (
                BillingProvider::Paddle,
                BillingCurrency::Usd,
                r.org_seat_annual_usd * 4 * 100
            )
        );
        assert_eq!(
            resolve_provider_and_price("EG", CheckoutProduct::OrganizationSeatAnnual, 4),
            (
                BillingProvider::Paymob,
                BillingCurrency::Egp,
                r.org_seat_annual_egp * 4 * 100
            )
        );
        assert_eq!(
            resolve_provider_and_price("US", CheckoutProduct::PayPerFile, 1),
            (
                BillingProvider::Paddle,
                BillingCurrency::Usd,
                r.pay_per_file_usd * 100
            )
        );
        assert_eq!(
            resolve_provider_and_price("EG", CheckoutProduct::PayPerFile, 1),
            (
                BillingProvider::Paymob,
                BillingCurrency::Egp,
                r.pay_per_file_egp * 100
            )
        );
        assert_eq!(
            resolve_provider_and_price("US", CheckoutProduct::CreditPack25, 1),
            (
                BillingProvider::Paddle,
                BillingCurrency::Usd,
                r.credit_pack_25_usd * 100
            )
        );
        assert_eq!(
            resolve_provider_and_price("EG", CheckoutProduct::CreditPack25, 1),
            (
                BillingProvider::Paymob,
                BillingCurrency::Egp,
                r.credit_pack_25_egp * 100
            )
        );
    }

    #[test]
    fn tampered_business_json_enabling_free_ai_is_rejected() {
        let broken = RAW_BUSINESS_JSON.replace("\"ai\": false", "\"ai\": true");
        assert!(BusinessRules::parse_json(&broken).is_err());
    }
}
