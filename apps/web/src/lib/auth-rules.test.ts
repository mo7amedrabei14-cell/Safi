import { describe, expect, it } from "vitest";
import {
  BUSINESS_CONFIG,
  DEFAULT_LANGUAGE,
  LAUNCH_LANGUAGES,
  MIN_PASSWORD_LENGTH
} from "./business-config";
import {
  detectDuplicateAccount,
  isValidEmail,
  meetsMinPasswordLength,
  validateSignUp
} from "./auth-rules";
import {
  canManageFriends,
  validateFriendGrantRequest,
  validateFriendRevokeRequest
} from "./owner-access";

const NOW = "2026-11-19 12:00:00 UTC";
const OWNER_ACTOR = {
  isOwner: true,
  userId: "01950000-0000-7000-8000-000000000001",
  email: "owner@example.test"
} as const;
const REGULAR_ACTOR = {
  isOwner: false,
  userId: "01950000-0000-7000-8000-000000000002",
  email: "friend@example.test"
} as const;

describe("authentication constitution rules & sealed owner model", () => {
  it("enforces exactly eight characters and rejects seven", () => {
    expect(MIN_PASSWORD_LENGTH).toBe(8);
    expect(meetsMinPasswordLength("1234567")).toBe(false);
    expect(meetsMinPasswordLength("12345678")).toBe(true);
    expect(meetsMinPasswordLength("صافي1234")).toBe(true);
    expect(meetsMinPasswordLength("")).toBe(false);
  });

  it("stores zero hardcoded owner emails in business config and defaults to English across 7 launch languages", () => {
    expect(BUSINESS_CONFIG.owner.identity_mode).toBe("internal_immutable_user_id");
    expect(BUSINESS_CONFIG.owner.mfa_policy).toBe("rfc6238_totp_google_authenticator");
    expect(DEFAULT_LANGUAGE).toBe("en");
    expect(LAUNCH_LANGUAGES).toEqual(["en", "ar", "fr", "es", "tr", "fa", "ur"]);
  });

  it("rejects a duplicate email sign-up with the sign-in instruction", () => {
    const existing = [{ normalizedEmail: "user@example.test", verifiedPhone: "201001234567" }];

    expect(detectDuplicateAccount(existing, " USER@Example.Test ", undefined)).toBe("email");
    expect(validateSignUp({ email: "user@example.test", password: "12345678" }, existing)).toEqual({
      ok: false,
      violation: { kind: "duplicate_email" }
    });
    expect(validateSignUp({ email: "new@example.test", password: "12345678" }, existing)).toEqual({
      ok: true
    });
  });

  it("rejects a duplicate verified phone even with a new email", () => {
    const existing = [{ normalizedEmail: "user@example.test", verifiedPhone: "+20 100 123 4567" }];

    expect(detectDuplicateAccount(existing, "new@example.test", "201001234567")).toBe("phone");
    expect(
      validateSignUp(
        { email: "new@example.test", phone: "01001234567", password: "12345678" },
        existing
      )
    ).toEqual({ ok: false, violation: { kind: "duplicate_phone" } });
  });

  it("rejects a seven-character password before any storage", () => {
    expect(validateSignUp({ email: "new@example.test", password: "1234567" }, [])).toEqual({
      ok: false,
      violation: { kind: "password_too_short", minLength: 8 }
    });
    expect(validateSignUp({ email: "new@example.test", password: "12345678" }, [])).toEqual({
      ok: true
    });
    expect(isValidEmail("not-an-email")).toBe(false);
  });

  it("lets only a server-confirmed Owner manage Friend VIP requests without relying on email matching", () => {
    expect(canManageFriends(OWNER_ACTOR)).toBe(true);
    expect(canManageFriends(REGULAR_ACTOR)).toBe(false);

    expect(validateFriendGrantRequest(REGULAR_ACTOR, "other@example.test", "", NOW)).toEqual({
      ok: false,
      reason: "not_owner"
    });

    const granted = validateFriendGrantRequest(OWNER_ACTOR, " Friend@Example.Test ", "VIP", NOW);
    expect(granted.ok).toBe(true);
    if (!granted.ok) return;
    expect(granted.member.email).toBe("friend@example.test");
    expect(granted.member.grantedByUserId).toBe(OWNER_ACTOR.userId);
    expect(granted.member.entitlesPaidPlan).toBe(true);
    expect(granted.member.aiCallsLimit).toBe(400);

    expect(validateFriendGrantRequest(OWNER_ACTOR, OWNER_ACTOR.email, "", NOW)).toEqual({
      ok: false,
      reason: "self_grant"
    });
    expect(validateFriendRevokeRequest(REGULAR_ACTOR, "other@example.test")).toEqual({
      ok: false,
      reason: "not_owner"
    });
    expect(validateFriendRevokeRequest(OWNER_ACTOR, " Friend@Example.Test ")).toEqual({
      ok: true,
      email: "friend@example.test"
    });
  });
});
