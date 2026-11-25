/**
 * `apps/web/src/lib/billing-validation.ts`
 *
 * Pure, deterministic client-side validation and provider-format normalization
 * for the SafiData Website Billing Portal (`/billing`).
 *
 * Authoritative validation always remains enforced on the Rust backend (`sf-server`);
 * this module provides immediate accessible field-level UX feedback and prevents
 * invalid checkout network submissions.
 */
import type { CheckoutProduct } from "./billing-checkout";
import { BUSINESS_PRICING } from "./business-pricing";

export type BillingFieldKey = "email" | "firstName" | "lastName" | "phone" | "seats";

export type BillingFieldErrors = Partial<Record<BillingFieldKey, string>>;

export interface BillingValidationInput {
  readonly product: CheckoutProduct;
  readonly country: "US" | "EG";
  readonly email: string;
  readonly firstName: string;
  readonly lastName: string;
  readonly phone: string;
  readonly seatsInput: string;
}

export interface BillingValidationResult {
  readonly valid: boolean;
  readonly errors: BillingFieldErrors;
  readonly firstInvalidField: BillingFieldKey | null;
  readonly normalizedEmail: string;
  readonly normalizedPhone: string | null;
  readonly parsedSeats: number;
}

export const BILLING_MAX_ORGANIZATION_SEATS = 5000;
export const BILLING_MIN_ORGANIZATION_SEATS = BUSINESS_PRICING.organizationSeat.minSeats;

const EMAIL_PATTERN = /^[^\s@]+@[^\s@]+\.[^\s@]{2,}$/;
const EGYPT_LOCAL_MOBILE_PATTERN = /^01[0125]\d{8}$/;
const EGYPT_INTL_MOBILE_PATTERN = /^(?:\+20|0020|20)(1[0125]\d{8})$/;

export function validateBillingEmail(
  raw: string,
  isAr: boolean
): { readonly valid: boolean; readonly normalized: string; readonly error: string | null } {
  const normalized = raw.trim().toLowerCase();
  if (!normalized) {
    return {
      valid: false,
      normalized,
      error: isAr
        ? "البريد الإلكتروني للحساب مطلوب لإتمام الفوترة وإصدار الإيصال."
        : "A verified account email address is required for billing and receipts."
    };
  }
  if (normalized.length > 254 || !EMAIL_PATTERN.test(normalized)) {
    return {
      valid: false,
      normalized,
      error: isAr
        ? "صيغة البريد الإلكتروني غير صالحة. يرجى التحقق من البريد الموثق للحساب."
        : "Invalid email format. Please check your verified account email address."
    };
  }
  return { valid: true, normalized, error: null };
}

export function validateBillingPersonName(
  raw: string,
  field: "firstName" | "lastName",
  isAr: boolean
): { readonly valid: boolean; readonly normalized: string; readonly error: string | null } {
  const normalized = raw.trim().replace(/\s+/g, " ");
  if (!normalized) {
    return {
      valid: false,
      normalized,
      error:
        field === "firstName"
          ? isAr
            ? "الاسم الأول مطلوب لإتمام الدفع عبر Paymob."
            : "First name is required for Paymob Egypt checkout."
          : isAr
            ? "اسم العائلة مطلوب لإتمام الدفع عبر Paymob."
            : "Last name is required for Paymob Egypt checkout."
    };
  }
  if (normalized.length < 2) {
    return {
      valid: false,
      normalized,
      error:
        field === "firstName"
          ? isAr
            ? "يرجى إدخال اسم أول صحيح (حرفين على الأقل)."
            : "Please enter a valid first name (at least 2 characters)."
          : isAr
            ? "يرجى إدخال اسم عائلة صحيح (حرفين على الأقل)."
            : "Please enter a valid last name (at least 2 characters)."
    };
  }
  if (normalized.length > 80) {
    return {
      valid: false,
      normalized,
      error: isAr
        ? "الاسم طويل جدًا (الحد الأقصى 80 حرفًا)."
        : "Name is too long (maximum 80 characters)."
    };
  }
  if (/^\d+$/.test(normalized)) {
    return {
      valid: false,
      normalized,
      error:
        field === "firstName"
          ? isAr
            ? "لا يمكن أن يتكون الاسم الأول من أرقام فقط."
            : "First name cannot contain only numbers."
          : isAr
            ? "لا يمكن أن يتكون اسم العائلة من أرقام فقط."
            : "Last name cannot contain only numbers."
    };
  }
  return { valid: true, normalized, error: null };
}

/**
 * Validates an Egyptian mobile number in either local (`010XXXXXXXX`) or
 * international (`+2010XXXXXXXX`, `002010XXXXXXXX`, `2010XXXXXXXX`) form and
 * normalizes it to canonical E.164 `+201XXXXXXXXX` for Paymob.
 */
export function validateAndNormalizeEgyptianPhone(
  raw: string,
  isAr: boolean
): { readonly valid: boolean; readonly normalized: string | null; readonly error: string | null } {
  const trimmed = raw.trim();
  if (!trimmed) {
    return {
      valid: false,
      normalized: null,
      error: isAr
        ? "رقم الهاتف المصري مطلوب (مثال: 01012345678 أو +201012345678)."
        : "Egyptian mobile number is required (e.g. 01012345678 or +201012345678)."
    };
  }

  // Strip common visual separators (spaces, hyphens, parentheses, dots).
  const compact = trimmed.replace(/[\s\-().]/g, "");

  if (EGYPT_LOCAL_MOBILE_PATTERN.test(compact)) {
    return {
      valid: true,
      normalized: `+20${compact.slice(1)}`,
      error: null
    };
  }

  const intlMatch = compact.match(EGYPT_INTL_MOBILE_PATTERN);
  if (intlMatch?.[1]) {
    return {
      valid: true,
      normalized: `+20${intlMatch[1]}`,
      error: null
    };
  }

  return {
    valid: false,
    normalized: null,
    error: isAr
      ? "رقم الهاتف المصري غير صالح. أدخل رقم موبايل مصري صحيح يبدأ بـ 010 أو 011 أو 012 أو 015 (أو بصيغة +201...)."
      : "Invalid Egyptian mobile number. Enter a valid number starting with 010, 011, 012, or 015 (or +201...)."
  };
}

export function validateOrganizationSeatsInput(
  raw: string,
  isAr: boolean
): { readonly valid: boolean; readonly seats: number; readonly error: string | null } {
  const trimmed = raw.trim();
  if (!trimmed || !/^\d+$/.test(trimmed)) {
    return {
      valid: false,
      seats: BILLING_MIN_ORGANIZATION_SEATS,
      error: isAr
        ? `يرجى إدخال عدد صحيح للمقاعد بين ${BILLING_MIN_ORGANIZATION_SEATS} و ${BILLING_MAX_ORGANIZATION_SEATS}.`
        : `Please enter a whole number of seats between ${BILLING_MIN_ORGANIZATION_SEATS} and ${BILLING_MAX_ORGANIZATION_SEATS}.`
    };
  }

  const parsed = Number.parseInt(trimmed, 10);
  if (!Number.isFinite(parsed) || parsed < BILLING_MIN_ORGANIZATION_SEATS) {
    return {
      valid: false,
      seats: parsed,
      error: isAr
        ? `الحد الأدنى لباقة المؤسسات هو ${BILLING_MIN_ORGANIZATION_SEATS} مقاعد.`
        : `Organization plans require a minimum of ${BILLING_MIN_ORGANIZATION_SEATS} seats.`
    };
  }

  if (parsed > BILLING_MAX_ORGANIZATION_SEATS) {
    return {
      valid: false,
      seats: parsed,
      error: isAr
        ? `الحد الأقصى للطلب الواحد هو ${BILLING_MAX_ORGANIZATION_SEATS} مقعد.`
        : `Maximum seats per self-serve checkout is ${BILLING_MAX_ORGANIZATION_SEATS}.`
    };
  }

  return { valid: true, seats: parsed, error: null };
}

export function validateBillingCheckoutForm(
  input: BillingValidationInput,
  isAr: boolean
): BillingValidationResult {
  const errors: BillingFieldErrors = {};
  const fieldOrder: BillingFieldKey[] = [];

  let parsedSeats = BILLING_MIN_ORGANIZATION_SEATS;
  if (input.product === "organization_seat_annual") {
    const seatsCheck = validateOrganizationSeatsInput(input.seatsInput, isAr);
    parsedSeats = seatsCheck.seats;
    if (!seatsCheck.valid && seatsCheck.error) {
      errors.seats = seatsCheck.error;
      fieldOrder.push("seats");
    }
  }

  const emailCheck = validateBillingEmail(input.email, isAr);
  if (!emailCheck.valid && emailCheck.error) {
    errors.email = emailCheck.error;
    fieldOrder.push("email");
  }

  let normalizedPhone: string | null = null;
  if (input.country === "EG") {
    const firstCheck = validateBillingPersonName(input.firstName, "firstName", isAr);
    if (!firstCheck.valid && firstCheck.error) {
      errors.firstName = firstCheck.error;
      fieldOrder.push("firstName");
    }

    const lastCheck = validateBillingPersonName(input.lastName, "lastName", isAr);
    if (!lastCheck.valid && lastCheck.error) {
      errors.lastName = lastCheck.error;
      fieldOrder.push("lastName");
    }

    const phoneCheck = validateAndNormalizeEgyptianPhone(input.phone, isAr);
    normalizedPhone = phoneCheck.normalized;
    if (!phoneCheck.valid && phoneCheck.error) {
      errors.phone = phoneCheck.error;
      fieldOrder.push("phone");
    }
  }

  return {
    valid: fieldOrder.length === 0,
    errors,
    firstInvalidField: fieldOrder[0] ?? null,
    normalizedEmail: emailCheck.normalized,
    normalizedPhone,
    parsedSeats
  };
}
