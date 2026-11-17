import { translateLang } from "@safi/i18n";
import type { SafiLang } from "./locale";

export type SafiPrimaryThemeId = "vision-glass" | "google-gemini";
export type SafiThemeId = SafiPrimaryThemeId | "apple-light";

export const SAFI_DEFAULT_THEME: SafiThemeId = "vision-glass";
export const SAFI_PRIMARY_THEMES: readonly SafiPrimaryThemeId[] = ["vision-glass", "google-gemini"];
export const SAFI_THEME_STORAGE_KEY = "safi.theme";

/**
 * Parse a persisted or attribute theme ID.
 * Legacy `"apple-light"` preferences remain valid and are never dropped or overwritten silently.
 */
export function parseSafiTheme(value: string | null | undefined): SafiThemeId | null {
  if (value === "vision-glass" || value === "light") return "vision-glass";
  if (value === "google-gemini" || value === "dark") return "google-gemini";
  if (value === "apple-light") return "apple-light";
  return null;
}

export function isDarkSafiTheme(theme: SafiThemeId): boolean {
  return theme === "google-gemini";
}

export function getThemeDisplayLabel(theme: SafiThemeId, lang: SafiLang): string {
  switch (theme) {
    case "google-gemini":
      return translateLang(lang, "theme.darkMode");
    case "apple-light":
      return translateLang(lang, "theme.legacyAppleLight");
    case "vision-glass":
    default:
      return translateLang(lang, "theme.lightMode");
  }
}

export function getInitialSafiTheme(): SafiThemeId {
  if (typeof window !== "undefined") {
    try {
      const fromStorage = parseSafiTheme(window.localStorage.getItem(SAFI_THEME_STORAGE_KEY));
      if (fromStorage !== null) return fromStorage;
    } catch {
      // Fall back safely when storage is restricted.
    }
  }

  if (typeof document !== "undefined") {
    const fromDocument = parseSafiTheme(document.documentElement.getAttribute("data-theme"));
    if (fromDocument !== null) return fromDocument;
  }

  return SAFI_DEFAULT_THEME;
}

export function syncDomTheme(theme: SafiThemeId): void {
  if (typeof document !== "undefined") {
    document.documentElement.setAttribute("data-theme", theme);
  }
}

/**
 * Apply and persist an explicit user theme choice.
 */
export function applySafiTheme(theme: SafiThemeId): void {
  syncDomTheme(theme);

  if (typeof window !== "undefined") {
    try {
      window.localStorage.setItem(SAFI_THEME_STORAGE_KEY, theme);
    } catch {
      // Session theme state still updates when storage is unavailable.
    }
  }
}
