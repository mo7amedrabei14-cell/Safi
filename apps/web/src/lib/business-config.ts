/**
 * Typed bridge to `config/business.json`, the single source of truth for
 * business, auth, roles, and billing decisions. Never hard-code these values.
 */
import rawBusinessConfig from "../../../../config/business.json";

export type AuthProvider = "google" | "apple" | "github";

export interface FriendRoleConfig {
  readonly label_ar: string;
  readonly label_en: string;
  readonly "granted_ exclusively_by": string;
  readonly price: { readonly USD: number; readonly EGP: number };
  readonly all_paid_features_open: boolean;
  readonly removes_watermark: boolean;
  readonly saved_recipes: boolean;
  readonly includes_premium_recipe_packs: boolean;
  readonly ai_calls_per_month: number;
  readonly expires: string;
}

export interface BusinessConfig {
  readonly brand: { readonly name_en: string; readonly name_ar: string };
  readonly owner: { readonly email: string; readonly strict_single_owner_lock: boolean };
  readonly support: { readonly email: string; readonly show_publicly: boolean };
  readonly auth: {
    readonly min_password_length: number;
    readonly password_visibility_toggle: boolean;
    readonly prevent_duplicate_accounts: boolean;
    readonly unique_account_keys: readonly string[];
    readonly oauth_providers: readonly AuthProvider[];
    readonly motion_animations_enabled: boolean;
  };
  readonly roles: {
    readonly owner: { readonly email_match: string };
    readonly friend: FriendRoleConfig;
  };
  readonly trial: { readonly days: number; readonly ai_calls_cap: number };
  readonly plans: ReadonlyArray<{
    readonly id: string;
    readonly price?: { readonly USD: number; readonly EGP: number };
    readonly price_per_seat_per_month?: { readonly USD: number; readonly EGP: number };
    readonly price_per_seat_per_year?: { readonly USD: number; readonly EGP: number };
  }>;
  readonly owner_dashboard_analytics: {
    readonly time_horizons: readonly ("monthly" | "quarterly" | "annual")[];
    readonly quarters: readonly string[];
  };
  readonly refunds: { readonly window_days: number };
}

export const BUSINESS_CONFIG = rawBusinessConfig as unknown as BusinessConfig;

export const OWNER_EMAIL = BUSINESS_CONFIG.owner.email;
export const MIN_PASSWORD_LENGTH = BUSINESS_CONFIG.auth.min_password_length;
export const OAUTH_PROVIDERS = BUSINESS_CONFIG.auth.oauth_providers;
export const FRIEND_ROLE = BUSINESS_CONFIG.roles.friend;
export const TRIAL_DAYS = BUSINESS_CONFIG.trial.days;
