import { describe, it, expect } from "vitest";
import arDict from "../locales/ar.json";
import enDict from "../locales/en.json";

describe("i18n Dictionaries & ICU Parity", () => {
  it("has identical keys in ar.json and en.json", () => {
    const arKeys = Object.keys(arDict).sort();
    const enKeys = Object.keys(enDict).sort();
    expect(arKeys).toEqual(enKeys);
  });

  it("contains no empty strings", () => {
    for (const val of Object.values(arDict)) {
      expect(val.trim().length).toBeGreaterThan(0);
    }
    for (const val of Object.values(enDict)) {
      expect(val.trim().length).toBeGreaterThan(0);
    }
  });
});
