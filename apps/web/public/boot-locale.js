(() => {
  "use strict";

  const STORAGE_KEY = "safi.locale";
  let locale = "en";

  try {
    const stored = window.localStorage.getItem(STORAGE_KEY);
    if (stored === "ar" || stored === "en") locale = stored;
  } catch {
    // English is the safe first-run default when storage is unavailable.
  }

  const root = document.documentElement;
  root.dataset.safiLocale = locale;
  root.lang = locale === "ar" ? "ar-EG" : "en";
  root.dir = locale === "ar" ? "rtl" : "ltr";

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
