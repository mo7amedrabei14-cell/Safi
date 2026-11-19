import { describe, it, expect } from "vitest";
import arDict from "../locales/ar.json";
import enDict from "../locales/en.json";
import frDict from "../locales/fr.json";
import esDict from "../locales/es.json";
import trDict from "../locales/tr.json";
import faDict from "../locales/fa.json";
import urDict from "../locales/ur.json";
import { LAUNCH_LOCALES, localeDirection, translateLang, type ShortLang } from "./index";

const ALL_DICTS: Record<ShortLang, Record<string, string>> = {
  en: enDict,
  ar: arDict,
  fr: frDict,
  es: esDict,
  tr: trDict,
  fa: faDict,
  ur: urDict
};

describe("i18n Dictionaries & 7-Language ICU Parity", () => {
  it("has identical non-empty keys across all 7 launch languages", () => {
    const enKeys = Object.keys(enDict).sort();
    expect(LAUNCH_LOCALES).toHaveLength(7);

    for (const [lang, dict] of Object.entries(ALL_DICTS)) {
      expect(Object.keys(dict).sort(), `keys for ${lang}`).toEqual(enKeys);
      for (const val of Object.values(dict)) {
        expect(val.trim().length, `non-empty in ${lang}`).toBeGreaterThan(0);
      }
    }
  });

  it("assigns RTL strictly to Arabic, Persian, and Urdu and LTR to English, French, Spanish, and Turkish", () => {
    expect(localeDirection("en-US")).toBe("ltr");
    expect(localeDirection("fr-FR")).toBe("ltr");
    expect(localeDirection("es-ES")).toBe("ltr");
    expect(localeDirection("tr-TR")).toBe("ltr");
    expect(localeDirection("ar-EG")).toBe("rtl");
    expect(localeDirection("fa-IR")).toBe("rtl");
    expect(localeDirection("ur-PK")).toBe("rtl");
  });

  it("translates primary theme modes and ICU placeholders across all 7 launch locales", () => {
    expect(translateLang("en", "theme.lightMode")).toBe("Light Mode");
    expect(translateLang("en", "theme.darkMode")).toBe("Dark Mode");
    expect(translateLang("ar", "theme.lightMode")).toBe("الوضع الفاتح");
    expect(translateLang("ar", "theme.darkMode")).toBe("الوضع الداكن");
    expect(translateLang("fr", "theme.lightMode")).toBe("Mode clair");
    expect(translateLang("es", "theme.darkMode")).toBe("Modo oscuro");
    expect(translateLang("tr", "theme.lightMode")).toBe("Açık Mod");
    expect(translateLang("fa", "theme.darkMode")).toBe("حالت تاریک");
    expect(translateLang("ur", "theme.lightMode")).toBe("ہلکا موڈ");
    expect(translateLang("en", "scrollbar.percent", { percent: 42 })).toBe("42 percent");
    expect(translateLang("ar", "scrollbar.percent", { percent: 42 })).toBe("42 بالمئة");
    expect(translateLang("fr", "scrollbar.percent", { percent: 42 })).toBe("42 pour cent");
  });
});
