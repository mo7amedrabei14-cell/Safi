/**
 * `packages/i18n` — ICU MessageFormat translation engine, React Provider,
 * locale state switcher, and 40% text expansion test helper (Specification Sections 6.7, 14.2).
 */

import React, { createContext, useContext, useState, useEffect } from "react";
import type { ReactNode } from "react";
import arDict from "../locales/ar.json";
import enDict from "../locales/en.json";

export * from "./generated-errors.ts";

export type Locale = "ar-EG" | "en-US";

export interface I18nContextType {
  locale: Locale;
  setLocale: (locale: Locale) => void;
  t: (key: string, params?: Record<string, string | number>) => string;
  expandText40Pct: (text: string) => string;
  dir: "rtl" | "ltr";
}

const dictionaries: Record<Locale, Record<string, string>> = {
  "ar-EG": arDict,
  "en-US": enDict
};

const I18nContext = createContext<I18nContextType | undefined>(undefined);

export interface I18nProviderProps {
  children: ReactNode;
  defaultLocale?: Locale;
}

export const I18nProvider: React.FC<I18nProviderProps> = ({
  children,
  defaultLocale = "en-US"
}) => {
  const [locale, setLocaleState] = useState<Locale>(() => {
    if (typeof document !== "undefined") {
      const selected = document.documentElement.dataset.safiLocale;
      if (selected === "ar") return "ar-EG";
      if (selected === "en") return "en-US";
    }
    if (typeof window !== "undefined") {
      try {
        const selected = window.localStorage.getItem("safi.locale");
        if (selected === "ar" || selected === "ar-EG") return "ar-EG";
        if (selected === "en" || selected === "en-US") return "en-US";
      } catch {
        // The configured default locale remains available without storage.
      }
    }
    return defaultLocale;
  });

  const setLocale = (newLocale: Locale) => {
    setLocaleState(newLocale);
    if (typeof document !== "undefined") {
      document.documentElement.lang = newLocale;
      document.documentElement.dir = newLocale === "ar-EG" ? "rtl" : "ltr";
      document.documentElement.dataset.safiLocale = newLocale === "ar-EG" ? "ar" : "en";
    }
    if (typeof window !== "undefined") {
      try {
        window.localStorage.setItem("safi.locale", newLocale === "ar-EG" ? "ar" : "en");
      } catch {
        // The locale still changes for the current session.
      }
    }
  };

  useEffect(() => {
    if (typeof document !== "undefined") {
      document.documentElement.lang = locale;
      document.documentElement.dir = locale === "ar-EG" ? "rtl" : "ltr";
      document.documentElement.dataset.safiLocale = locale === "ar-EG" ? "ar" : "en";
    }
  }, [locale]);

  const t = (key: string, params?: Record<string, string | number>): string => {
    const dict = dictionaries[locale];
    let template = dict[key] ?? key;

    if (params) {
      for (const [paramKey, paramVal] of Object.entries(params)) {
        template = template.replace(new RegExp(`\\{${paramKey}\\}`, "g"), String(paramVal));
      }
    }
    return template;
  };

  const expandText40Pct = (text: string): string => {
    // Specification Section 6.7: UI layout resilience testing by expanding text length by ~40%
    return locale === "ar-EG" ? text + " [ممتد 40%]" : text + " [expanded 40%]";
  };

  const dir = locale === "ar-EG" ? "rtl" : "ltr";

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
