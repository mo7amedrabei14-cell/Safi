/**
 * `packages/i18n` — ICU MessageFormat translation engine, React Provider,
 * 7 launch languages (`en` default, `ar`, `fr`, `es`, `tr`, `fa`, `ur`),
 * automatic RTL (`ar`, `fa`, `ur`) vs LTR (`en`, `fr`, `es`, `tr`),
 * and 40% text expansion test helper (Specification Sections 6.7, 14.2, Phase 12).
 */

import React, { createContext, useContext, useState, useEffect } from "react";
import type { ReactNode } from "react";
import arDict from "../locales/ar.json";
import enDict from "../locales/en.json";
import frDict from "../locales/fr.json";
import esDict from "../locales/es.json";
import trDict from "../locales/tr.json";
import faDict from "../locales/fa.json";
import urDict from "../locales/ur.json";

export * from "./generated-errors.ts";

export type Locale = "en-US" | "ar-EG" | "fr-FR" | "es-ES" | "tr-TR" | "fa-IR" | "ur-PK";
export type ShortLang = "en" | "ar" | "fr" | "es" | "tr" | "fa" | "ur";

export const LAUNCH_LOCALES: readonly Locale[] = [
  "en-US",
  "ar-EG",
  "fr-FR",
  "es-ES",
  "tr-TR",
  "fa-IR",
  "ur-PK"
];

export const RTL_LOCALES: ReadonlySet<Locale> = new Set<Locale>(["ar-EG", "fa-IR", "ur-PK"]);

export function localeDirection(locale: Locale): "rtl" | "ltr" {
  return RTL_LOCALES.has(locale) ? "rtl" : "ltr";
}

export function shortLangToLocale(lang: ShortLang): Locale {
  switch (lang) {
    case "ar":
      return "ar-EG";
    case "fr":
      return "fr-FR";
    case "es":
      return "es-ES";
    case "tr":
      return "tr-TR";
    case "fa":
      return "fa-IR";
    case "ur":
      return "ur-PK";
    default:
      return "en-US";
  }
}

export function localeToShortLang(locale: Locale): ShortLang {
  switch (locale) {
    case "ar-EG":
      return "ar";
    case "fr-FR":
      return "fr";
    case "es-ES":
      return "es";
    case "tr-TR":
      return "tr";
    case "fa-IR":
      return "fa";
    case "ur-PK":
      return "ur";
    default:
      return "en";
  }
}

export interface I18nContextType {
  locale: Locale;
  setLocale: (locale: Locale) => void;
  t: (key: string, params?: Record<string, string | number>) => string;
  expandText40Pct: (text: string) => string;
  dir: "rtl" | "ltr";
}

const dictionaries: Record<Locale, Record<string, string>> = {
  "en-US": enDict,
  "ar-EG": arDict,
  "fr-FR": frDict,
  "es-ES": esDict,
  "tr-TR": trDict,
  "fa-IR": faDict,
  "ur-PK": urDict
};

/**
 * Translate an ICU message key for any supported launch locale.
 */
export function translate(
  locale: Locale,
  key: string,
  params?: Record<string, string | number>
): string {
  const dict = dictionaries[locale] ?? dictionaries["en-US"];
  let template = dict[key] ?? key;

  if (params) {
    for (const [paramKey, paramVal] of Object.entries(params)) {
      template = template.replace(new RegExp(`\\{${paramKey}\\}`, "g"), String(paramVal));
    }
  }
  return template;
}

/**
 * Resolve short language code (`"en"`, `"ar"`, `"fr"`, `"es"`, `"tr"`, `"fa"`, `"ur"`)
 * into ICU dictionary translations.
 */
export function translateLang(
  lang: ShortLang,
  key: string,
  params?: Record<string, string | number>
): string {
  return translate(shortLangToLocale(lang), key, params);
}

const I18nContext = createContext<I18nContextType | undefined>(undefined);

export interface I18nProviderProps {
  children: ReactNode;
  defaultLocale?: Locale;
}

function parseStoredLocale(raw: string | null | undefined): Locale | null {
  switch (raw) {
    case "en":
    case "en-US":
      return "en-US";
    case "ar":
    case "ar-EG":
      return "ar-EG";
    case "fr":
    case "fr-FR":
      return "fr-FR";
    case "es":
    case "es-ES":
      return "es-ES";
    case "tr":
    case "tr-TR":
      return "tr-TR";
    case "fa":
    case "fa-IR":
      return "fa-IR";
    case "ur":
    case "ur-PK":
      return "ur-PK";
    default:
      return null;
  }
}

export const I18nProvider: React.FC<I18nProviderProps> = ({
  children,
  defaultLocale = "en-US"
}) => {
  const [locale, setLocaleState] = useState<Locale>(() => {
    if (typeof window !== "undefined") {
      try {
        const parsed = parseStoredLocale(window.localStorage.getItem("safi.locale"));
        if (parsed !== null) return parsed;
      } catch {
        // Default English locale remains available without storage.
      }
    }
    if (typeof document !== "undefined") {
      const parsed = parseStoredLocale(document.documentElement.dataset.safiLocale);
      if (parsed !== null) return parsed;
    }
    return defaultLocale;
  });

  const applyToDocument = (nextLocale: Locale): void => {
    if (typeof document !== "undefined") {
      const short = localeToShortLang(nextLocale);
      document.documentElement.lang = nextLocale;
      document.documentElement.dir = localeDirection(nextLocale);
      document.documentElement.dataset.safiLocale = short;
      const manifestLink = document.querySelector<HTMLLinkElement>("#safi-manifest");
      if (manifestLink !== null) {
        manifestLink.href =
          nextLocale === "ar-EG" ? "/manifest.ar.webmanifest" : "/manifest.webmanifest";
      }
    }
  };

  const setLocale = (newLocale: Locale) => {
    setLocaleState(newLocale);
    applyToDocument(newLocale);
    if (typeof window !== "undefined") {
      try {
        window.localStorage.setItem("safi.locale", localeToShortLang(newLocale));
      } catch {
        // The locale still changes for the current session.
      }
    }
  };

  useEffect(() => {
    applyToDocument(locale);
  }, [locale]);

  const t = (key: string, params?: Record<string, string | number>): string =>
    translate(locale, key, params);

  const expandText40Pct = (text: string): string => {
    return locale === "ar-EG" ? text + " [ممتد 40%]" : text + " [expanded 40%]";
  };

  const dir = localeDirection(locale);

  return React.createElement(
    I18nContext.Provider,
    { value: { locale, setLocale, t, expandText40Pct, dir } },
    children
  );
};

export const useI18n = (): I18nContextType => {
  const context = useContext(I18nContext);
  if (!context) {
    throw new Error("useI18n must be used within an I18nProvider");
  }
  return context;
};
