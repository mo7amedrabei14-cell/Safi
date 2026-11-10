import { describe, expect, it } from "vitest";
import { MIN_PASSWORD_LENGTH, OWNER_EMAIL } from "./business-config";
import {
  detectDuplicateAccount,
  isOwnerEmail,
  isValidEmail,
  meetsMinPasswordLength,
  validateSignUp
} from "./auth-rules";
import { canManageFriends, grantFriendRole, revokeFriendRole } from "./owner-access";

const NOW = "2026-11-09 12:00:00 UTC";

describe("authentication constitution rules", () => {
  it("enforces exactly eight characters and rejects seven", () => {
    expect(MIN_PASSWORD_LENGTH).toBe(8);
    expect(meetsMinPasswordLength("1234567")).toBe(false);
    expect(meetsMinPasswordLength("12345678")).toBe(true);
    expect(meetsMinPasswordLength("صافي1234")).toBe(true);
    expect(meetsMinPasswordLength("")).toBe(false);
  });

  it("recognises only the normalized owner address", () => {
    expect(OWNER_EMAIL).toBe("mo7amed.rabei14@gmail.com");
    expect(isOwnerEmail("  MO7amed.Rabei14@Gmail.com ")).toBe(true);
    expect(isOwnerEmail("mo7amed.rabei14@gmail.com.eg")).toBe(false);
    expect(isOwnerEmail("friend@example.test")).toBe(false);
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

  it("lets only the owner grant and revoke Friend VIP with the full paid plan for free", () => {
    expect(canManageFriends(OWNER_EMAIL)).toBe(true);
    expect(canManageFriends("friend@example.test")).toBe(false);

    expect(grantFriendRole("friend@example.test", "other@example.test", "", NOW)).toEqual({
      ok: false,
      reason: "not_owner"
    });

    const granted = grantFriendRole(
      " MO7amed.Rabei14@Gmail.com ",
      " Friend@Example.Test ",
      "Ahmed",
      NOW
    );
    expect(granted.ok).toBe(true);
    if (!granted.ok) return;
    expect(granted.member.email).toBe("friend@example.test");
    expect(granted.member.grantedBy).toBe(OWNER_EMAIL);
    expect(granted.member.entitlesPaidPlan).toBe(true);
    expect(granted.member.aiCallsLimit).toBe(400);
    expect(granted.member.grantedAt).toBe(NOW);

    expect(grantFriendRole(OWNER_EMAIL, OWNER_EMAIL, "", NOW)).toEqual({
      ok: false,
      reason: "owner_address"
    });
    expect(revokeFriendRole("friend@example.test", "other@example.test")).toEqual({
      ok: false,
      reason: "not_owner"
    });
    expect(revokeFriendRole(OWNER_EMAIL, " Friend@Example.Test ")).toEqual({
      ok: true,
      email: "friend@example.test"
    });
  });
});
