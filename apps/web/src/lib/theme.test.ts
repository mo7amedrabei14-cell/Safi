import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  applySafiTheme,
  getInitialSafiTheme,
  getThemeDisplayLabel,
  parseSafiTheme,
  SAFI_DEFAULT_THEME,
  SAFI_THEME_STORAGE_KEY
} from "./theme";

describe("SafiData theme reorganization and legacy compatibility", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("uses Gemini Dark as the default Dark Mode theme and Vision Glass as Light Mode", () => {
    expect(SAFI_DEFAULT_THEME).toBe("google-gemini");
    expect(getInitialSafiTheme()).toBe("google-gemini");
    expect(getThemeDisplayLabel("vision-glass", "en")).toBe("Light Mode");
    expect(getThemeDisplayLabel("vision-glass", "ar")).toBe("الوضع الفاتح");
    expect(getThemeDisplayLabel("google-gemini", "en")).toBe("Dark Mode");
    expect(getThemeDisplayLabel("google-gemini", "ar")).toBe("الوضع الداكن");
  });

  it("preserves existing saved Light Mode, Dark Mode, and legacy apple-light preferences", () => {
    expect(parseSafiTheme("apple-light")).toBe("apple-light");
    expect(parseSafiTheme("vision-glass")).toBe("vision-glass");
    expect(parseSafiTheme("google-gemini")).toBe("google-gemini");
    expect(parseSafiTheme("light")).toBe("vision-glass");
    expect(parseSafiTheme("dark")).toBe("google-gemini");
    expect(parseSafiTheme("unknown-theme")).toBeNull();
    expect(getThemeDisplayLabel("apple-light", "en")).toBe("Classic Light");
    expect(getThemeDisplayLabel("apple-light", "ar")).toBe("الفاتح الكلاسيكي");

    const store = new Map<string, string>();
    const attrs = new Map<string, string>();
    vi.stubGlobal("window", {
      localStorage: {
        getItem: (k: string) => store.get(k) ?? null,
        setItem: (k: string, v: string) => {
          store.set(k, v);
        }
      }
    });
    vi.stubGlobal("document", {
      documentElement: {
        getAttribute: (k: string) => attrs.get(k) ?? null,
        setAttribute: (k: string, v: string) => {
          attrs.set(k, v);
        }
      }
    });

    // Fresh user with no saved preference starts in Dark Mode
    expect(getInitialSafiTheme()).toBe("google-gemini");

    // Existing saved Light Mode is preserved and never forcibly overwritten
    store.set(SAFI_THEME_STORAGE_KEY, "vision-glass");
    expect(getInitialSafiTheme()).toBe("vision-glass");
    expect(store.get(SAFI_THEME_STORAGE_KEY)).toBe("vision-glass");

    // Existing saved legacy apple-light is preserved
    store.set(SAFI_THEME_STORAGE_KEY, "apple-light");
    expect(getInitialSafiTheme()).toBe("apple-light");
    expect(store.get(SAFI_THEME_STORAGE_KEY)).toBe("apple-light");

    // Explicit switching updates DOM attribute and storage cleanly in both directions
    applySafiTheme("google-gemini");
    expect(attrs.get("data-theme")).toBe("google-gemini");
    expect(store.get(SAFI_THEME_STORAGE_KEY)).toBe("google-gemini");
    applySafiTheme("vision-glass");
    expect(attrs.get("data-theme")).toBe("vision-glass");
    expect(store.get(SAFI_THEME_STORAGE_KEY)).toBe("vision-glass");
  });

  it("configures pre-paint boot initialization with Gemini Dark default and legacy preservation", () => {
    const html = readFileSync(new URL("../../index.html", import.meta.url), "utf8");
    const bootLocale = readFileSync(
      new URL("../../public/boot-locale.js", import.meta.url),
      "utf8"
    );
    const bootCss = readFileSync(new URL("../../public/boot.css", import.meta.url), "utf8");
    const css = readFileSync(new URL("../index.css", import.meta.url), "utf8");

    expect(html).toContain('data-theme="google-gemini"');
    expect(bootLocale).toContain(SAFI_THEME_STORAGE_KEY);
    expect(bootLocale).toContain('let theme = "google-gemini"');
    expect(bootLocale).toContain('"vision-glass"');
    expect(bootLocale).toContain('"apple-light"');
    expect(bootCss).toContain("color-scheme: dark;");
    expect(bootCss).toContain("background: #090b10;");
    expect(css).toContain(':root,\n:root[data-theme="dark"],\n:root[data-theme="google-gemini"]');
    expect(css).toContain(':root[data-theme="light"],\n:root[data-theme="vision-glass"]');
    expect(css).toContain(':root[data-theme="apple-light"]');
  });

  it("does not throw when applying theme in SSR environments", () => {
    expect(() => applySafiTheme("google-gemini")).not.toThrow();
    expect(() => applySafiTheme("vision-glass")).not.toThrow();
  });
});
