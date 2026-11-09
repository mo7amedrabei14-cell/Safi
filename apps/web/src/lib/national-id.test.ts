import { describe, expect, it } from "vitest";
import { normalizeDigits } from "./arabic-normalize";
import { validateEgyptianNationalId } from "./national-id";

describe("normalizeDigits", () => {
  it("is exported as a callable function (regression: missing export blanked the whole app)", () => {
    expect(typeof normalizeDigits).toBe("function");
  });

  it("maps Arabic-Indic and Persian digits to ASCII and leaves other characters alone", () => {
    expect(normalizeDigits("٠١٢٣٤٥٦٧٨٩")).toBe("0123456789");
    expect(normalizeDigits("۰۱۲۳۴۵۶۷۸۹")).toBe("0123456789");
    expect(normalizeDigits("رقم ٤٢ - abc")).toBe("رقم 42 - abc");
    expect(normalizeDigits("")).toBe("");
  });
});

describe("validateEgyptianNationalId", () => {
  it("accepts a valid 14-digit ID typed in Arabic-Indic digits", () => {
    const result = validateEgyptianNationalId("٢٩٠٠٥١٥٠١١٢٣٤٧");
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.info.cleanId).toBe("29005150112347");
      expect(result.info.birthDate).toBe("1990-05-15");
      expect(result.info.governorateCode).toBe("01");
      expect(result.info.governorateNameAr).toBe("القاهرة");
      expect(result.info.genderAr).toBe("أنثى");
      expect(result.info.expectedCheckDigit).toBe(7);
      expect(result.info.checksumMatches).toBe(true);
    }
  });

  it("returns coded errors for malformed IDs", () => {
    const short = validateEgyptianNationalId("123");
    expect(short.ok).toBe(false);
    if (!short.ok) {
      expect(short.code).toBe("SF-CLN-014");
    }

    const badCentury = validateEgyptianNationalId("19005150112347");
    expect(badCentury.ok).toBe(false);
    if (!badCentury.ok) {
      expect(badCentury.code).toBe("SF-CLN-015");
    }
  });
});
