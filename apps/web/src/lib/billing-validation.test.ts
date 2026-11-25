import { describe, expect, it } from "vitest";
import {
  BILLING_MAX_ORGANIZATION_SEATS,
  BILLING_MIN_ORGANIZATION_SEATS,
  validateAndNormalizeEgyptianPhone,
  validateBillingCheckoutForm,
  validateBillingEmail,
  validateBillingPersonName,
  validateOrganizationSeatsInput
} from "./billing-validation";

describe("billing-validation (Paymob EG + Paddle USD field validation & normalization)", () => {
  it("validates email syntax strictly and normalizes whitespace/casing", () => {
    expect(validateBillingEmail("  Buyer@SafiData.APP ", false)).toEqual({
      valid: true,
      normalized: "buyer@safidata.app",
      error: null
    });
    expect(validateBillingEmail("", false).valid).toBe(false);
    expect(validateBillingEmail("not-an-email", false).valid).toBe(false);
    expect(validateBillingEmail("user@domain", true).error).toContain("البريد الإلكتروني");
  });

  it("accepts local Egyptian mobile formats (010/011/012/015) and normalizes to +201...", () => {
    expect(validateAndNormalizeEgyptianPhone("01012345678", false)).toEqual({
      valid: true,
      normalized: "+201012345678",
      error: null
    });
    expect(validateAndNormalizeEgyptianPhone("011-2345-6789", false)).toEqual({
      valid: true,
      normalized: "+201123456789",
      error: null
    });
    expect(validateAndNormalizeEgyptianPhone("012 3456 7890", true)).toEqual({
      valid: true,
      normalized: "+201234567890",
      error: null
    });
    expect(validateAndNormalizeEgyptianPhone("01555551234", false)).toEqual({
      valid: true,
      normalized: "+201555551234",
      error: null
    });
  });

  it("accepts international Egyptian mobile formats (+20, 0020, 20) and normalizes to +201...", () => {
    expect(validateAndNormalizeEgyptianPhone("+201012345678", false)).toEqual({
      valid: true,
      normalized: "+201012345678",
      error: null
    });
    expect(validateAndNormalizeEgyptianPhone("+20 11 2233 4455", false)).toEqual({
      valid: true,
      normalized: "+201122334455",
      error: null
    });
    expect(validateAndNormalizeEgyptianPhone("00201299887766", false)).toEqual({
      valid: true,
      normalized: "+201299887766",
      error: null
    });
    expect(validateAndNormalizeEgyptianPhone("201511223344", false)).toEqual({
      valid: true,
      normalized: "+201511223344",
      error: null
    });
  });

  it("rejects invalid Egyptian phone numbers cleanly in EN and AR", () => {
    for (const bad of [
      "",
      "12345",
      "01912345678",
      "+14155552671",
      "0101234567",
      "010123456789",
      "abc01012345678"
    ]) {
      expect(validateAndNormalizeEgyptianPhone(bad, false).valid).toBe(false);
      expect(validateAndNormalizeEgyptianPhone(bad, true).valid).toBe(false);
    }
  });

  it("validates first/last names for Paymob Egypt", () => {
    expect(validateBillingPersonName("Mohamed", "firstName", false).valid).toBe(true);
    expect(validateBillingPersonName("محمد", "firstName", true).valid).toBe(true);
    expect(validateBillingPersonName("", "firstName", false).valid).toBe(false);
    expect(validateBillingPersonName("A", "lastName", false).valid).toBe(false);
    expect(validateBillingPersonName("12345", "lastName", false).valid).toBe(false);
  });

  it("validates organization seat bounds without silent clamping", () => {
    expect(validateOrganizationSeatsInput("2", false).valid).toBe(false);
    expect(validateOrganizationSeatsInput("0", false).valid).toBe(false);
    expect(validateOrganizationSeatsInput("-5", false).valid).toBe(false);
    expect(validateOrganizationSeatsInput("abc", false).valid).toBe(false);
    expect(
      validateOrganizationSeatsInput(String(BILLING_MAX_ORGANIZATION_SEATS + 1), false).valid
    ).toBe(false);
    expect(validateOrganizationSeatsInput(String(BILLING_MIN_ORGANIZATION_SEATS), false)).toEqual({
      valid: true,
      seats: BILLING_MIN_ORGANIZATION_SEATS,
      error: null
    });
    expect(validateOrganizationSeatsInput("15", true)).toEqual({
      valid: true,
      seats: 15,
      error: null
    });
  });

  it("validates full checkout form and returns deterministic firstInvalidField", () => {
    const badEg = validateBillingCheckoutForm(
      {
        product: "organization_seat_annual",
        country: "EG",
        email: "bad-email",
        firstName: "",
        lastName: "",
        phone: "01900000000",
        seatsInput: "2"
      },
      false
    );
    expect(badEg.valid).toBe(false);
    expect(badEg.firstInvalidField).toBe("seats");
    expect(badEg.errors.seats).toBeTruthy();
    expect(badEg.errors.email).toBeTruthy();
    expect(badEg.errors.firstName).toBeTruthy();
    expect(badEg.errors.lastName).toBeTruthy();
    expect(badEg.errors.phone).toBeTruthy();

    const okEg = validateBillingCheckoutForm(
      {
        product: "organization_seat_annual",
        country: "EG",
        email: "buyer@safidata.app",
        firstName: "Mohamed",
        lastName: "Rabei",
        phone: "010 1234 5678",
        seatsInput: "6"
      },
      false
    );
    expect(okEg.valid).toBe(true);
    expect(okEg.normalizedPhone).toBe("+201012345678");
    expect(okEg.parsedSeats).toBe(6);

    // Paddle USD skips Paymob-only person/phone checks
    const okUs = validateBillingCheckoutForm(
      {
        product: "individual_annual",
        country: "US",
        email: "buyer@safidata.app",
        firstName: "",
        lastName: "",
        phone: "",
        seatsInput: "1"
      },
      false
    );
    expect(okUs.valid).toBe(true);
  });
});
