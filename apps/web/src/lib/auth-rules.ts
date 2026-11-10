/**
 * Pure authentication rules shared by the auth portal and its tests.
 *
 * Every threshold comes from `config/business.json` through
 * `lib/business-config.ts`; nothing is duplicated in the interface.
 */
import { MIN_PASSWORD_LENGTH, OWNER_EMAIL } from "./business-config";

export interface ExistingAccountIdentity {
  readonly normalizedEmail: string;
  readonly verifiedPhone?: string | null;
}

export interface SignUpInput {
  readonly email: string;
  readonly phone?: string;
  readonly password: string;
}

export type SignUpViolation =
  | { readonly kind: "invalid_email" }
  | { readonly kind: "duplicate_email" }
  | { readonly kind: "duplicate_phone" }
  | { readonly kind: "password_too_short"; readonly minLength: number };

export type SignUpValidation =
  | { readonly ok: true }
  | { readonly ok: false; readonly violation: SignUpViolation };

export const PASSWORD_TOO_SHORT_MESSAGE_AR = `كلمة المرور يجب ألا تقل عن ${MIN_PASSWORD_LENGTH} خانات`;
export const PASSWORD_TOO_SHORT_MESSAGE_EN = `Password must be at least ${MIN_PASSWORD_LENGTH} characters.`;
export const DUPLICATE_ACCOUNT_MESSAGE_AR = "هذا الحساب مسجل بالفعل، يرجى تسجيل الدخول";
export const DUPLICATE_ACCOUNT_MESSAGE_EN =
  "This account is already registered. Please sign in instead.";

export function normalizeEmail(raw: string): string {
  return raw.trim().toLowerCase();
}

export function normalizePhone(raw: string): string {
  const digits = raw.replace(/[^0-9]/g, "");
  if (digits.length === 12 && digits.startsWith("20")) return digits.slice(2);
  if (digits.length === 11 && digits.startsWith("0")) return digits.slice(1);
  return digits;
}

export function isOwnerEmail(raw: string): boolean {
  return normalizeEmail(raw) === OWNER_EMAIL;
}

export function meetsMinPasswordLength(password: string): boolean {
  return Array.from(password).length >= MIN_PASSWORD_LENGTH;
}

export function isValidEmail(email: string): boolean {
  const normalized = normalizeEmail(email);
  return (
    normalized.length > 3 &&
    normalized.length <= 254 &&
    /^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(normalized)
  );
}

/** Duplicate detection across the two constitution unique keys. */
export function detectDuplicateAccount(
  existing: readonly ExistingAccountIdentity[],
  email: string,
  phone?: string
): "email" | "phone" | null {
  const normalizedEmail = normalizeEmail(email);
  const normalizedPhone = phone === undefined ? "" : normalizePhone(phone);

  if (
    normalizedEmail.length > 0 &&
    existing.some((account) => normalizeEmail(account.normalizedEmail) === normalizedEmail)
  ) {
    return "email";
  }

  if (
    normalizedPhone.length > 0 &&
    existing.some(
      (account) =>
        account.verifiedPhone !== undefined &&
        account.verifiedPhone !== null &&
        normalizePhone(account.verifiedPhone) === normalizedPhone
    )
  ) {
    return "phone";
  }

  return null;
}

export function validateSignUp(
  input: SignUpInput,
  existing: readonly ExistingAccountIdentity[] = []
): SignUpValidation {
  if (!isValidEmail(input.email)) {
    return { ok: false, violation: { kind: "invalid_email" } };
  }

  if (!meetsMinPasswordLength(input.password)) {
    return {
      ok: false,
      violation: { kind: "password_too_short", minLength: MIN_PASSWORD_LENGTH }
    };
  }

  const duplicate = detectDuplicateAccount(existing, input.email, input.phone);
  if (duplicate === "email") {
    return { ok: false, violation: { kind: "duplicate_email" } };
  }
  if (duplicate === "phone") {
    return { ok: false, violation: { kind: "duplicate_phone" } };
  }

  return { ok: true };
}
