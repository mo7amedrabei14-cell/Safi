import { describe, it, expect } from "vitest";
import arDict from "../locales/ar.json";
import enDict from "../locales/en.json";
import { translateLang } from "./index";

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

  it("translates primary theme modes and scrollbar labels without hardcoded names", () => {
    expect(translateLang("en", "theme.lightMode")).toBe("Light Mode");
    expect(translateLang("en", "theme.darkMode")).toBe("Dark Mode");
    expect(translateLang("ar", "theme.lightMode")).toBe("الوضع الفاتح");
    expect(translateLang("ar", "theme.darkMode")).toBe("الوضع الداكن");
    expect(translateLang("en", "scrollbar.scrollUp")).toBe("Scroll up");
    expect(translateLang("ar", "scrollbar.scrollDown")).toBe("تمرير للأسفل");
    expect(translateLang("en", "scrollbar.percent", { percent: 42 })).toBe("42 percent");
    expect(translateLang("ar", "scrollbar.percent", { percent: 42 })).toBe("42 بالمئة");
  });
});
