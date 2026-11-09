export type SafiLang = "ar" | "en";

export const SAFI_LOCALE_STORAGE_KEY = "safi.locale";

export function parseSafiLang(value: string | null | undefined): SafiLang | null {
  if (value === "ar" || value === "ar-EG") return "ar";
  if (value === "en" || value === "en-US") return "en";
  return null;
}

export function getInitialSafiLang(): SafiLang {
  if (typeof window !== "undefined") {
    try {
      const fromStorage = parseSafiLang(window.localStorage.getItem(SAFI_LOCALE_STORAGE_KEY));
      if (fromStorage !== null) return fromStorage;
    } catch {
      // Storage may be unavailable in private or restricted browser contexts.
    }
  }

  if (typeof document !== "undefined") {
    const fromDocument = parseSafiLang(document.documentElement.dataset.safiLocale);
    if (fromDocument !== null) return fromDocument;
  }

  return "en";
}

export function applySafiLang(lang: SafiLang): void {
  if (typeof document !== "undefined") {
    const root = document.documentElement;
    root.dataset.safiLocale = lang;
    root.lang = lang === "ar" ? "ar-EG" : "en";
    root.dir = lang === "ar" ? "rtl" : "ltr";
    const manifestLink = document.querySelector<HTMLLinkElement>("#safi-manifest");
    if (manifestLink !== null) {
      manifestLink.href = lang === "ar" ? "/manifest.ar.webmanifest" : "/manifest.webmanifest";
    }
  }

  if (typeof window !== "undefined") {
    try {
      window.localStorage.setItem(SAFI_LOCALE_STORAGE_KEY, lang);
    } catch {
      // The current session still changes locale when persistence is unavailable.
    }
  }
}
