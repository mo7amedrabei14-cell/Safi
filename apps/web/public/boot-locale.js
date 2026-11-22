(() => {
  "use strict";

  const LOCALE_STORAGE_KEY = "safi.locale";
  const THEME_STORAGE_KEY = "safi.theme";
  let locale = "en";
  let theme = "google-gemini";

  try {
    const storedLocale = window.localStorage.getItem(LOCALE_STORAGE_KEY);
    if (storedLocale === "ar" || storedLocale === "en") locale = storedLocale;
  } catch {
    // English is the safe first-run default when storage is unavailable.
  }

  try {
    const storedTheme = window.localStorage.getItem(THEME_STORAGE_KEY);
    if (storedTheme === "vision-glass" || storedTheme === "google-gemini" || storedTheme === "apple-light") {
      theme = storedTheme;
    } else if (storedTheme === "dark") {
      theme = "google-gemini";
    } else if (storedTheme === "light") {
      theme = "vision-glass";
    }
  } catch {
    // Gemini Dark is the safe first-run Dark Mode default when storage is unavailable.
  }

  const root = document.documentElement;
  root.dataset.safiLocale = locale;
  root.lang = locale === "ar" ? "ar-EG" : "en";
  root.dir = locale === "ar" ? "rtl" : "ltr";
  root.setAttribute("data-theme", theme);

  const manifestLink = document.querySelector("#safi-manifest");
  if (manifestLink) {
    manifestLink.setAttribute(
      "href",
      locale === "ar" ? "/manifest.ar.webmanifest" : "/manifest.webmanifest"
    );
  }

  const applyLocalizedMarkup = () => {
    document.querySelectorAll("[data-locale-en][data-locale-ar]").forEach((element) => {
      const value = locale === "ar" ? element.getAttribute("data-locale-ar") : element.getAttribute("data-locale-en");
      if (value !== null) element.textContent = value;
      if (element.tagName === "BDI") element.setAttribute("dir", locale === "ar" ? "rtl" : "ltr");
    });

    document.querySelectorAll("[data-locale-aria-en][data-locale-aria-ar]").forEach((element) => {
      const value = locale === "ar" ? element.getAttribute("data-locale-aria-ar") : element.getAttribute("data-locale-aria-en");
      if (value !== null) element.setAttribute("aria-label", value);
    });

    document.querySelectorAll("[data-locale-content-en][data-locale-content-ar]").forEach((element) => {
      const value = locale === "ar" ? element.getAttribute("data-locale-content-ar") : element.getAttribute("data-locale-content-en");
      if (value !== null) element.setAttribute("content", value);
    });

    const title = document.querySelector("title[data-locale-en][data-locale-ar]");
    if (title) title.textContent = locale === "ar" ? title.getAttribute("data-locale-ar") : title.getAttribute("data-locale-en");
  };

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", applyLocalizedMarkup, { once: true });
  } else {
    applyLocalizedMarkup();
  }
})();
