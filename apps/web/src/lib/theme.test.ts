import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  applySafiTheme,
  getInitialSafiTheme,
  getThemeDisplayLabel,
  parseSafiTheme,
  SAFI_DEFAULT_THEME,
  SAFI_THEME_STORAGE_KEY
} from "./theme";

describe("SafiData theme reorganization and legacy compatibility", () => {
  it("uses Vision Glass as the default Light Mode theme and Gemini Dark as Dark Mode", () => {
    expect(SAFI_DEFAULT_THEME).toBe("vision-glass");
    expect(getInitialSafiTheme()).toBe("vision-glass");
    expect(getThemeDisplayLabel("vision-glass", "en")).toBe("Light Mode");
    expect(getThemeDisplayLabel("vision-glass", "ar")).toBe("الوضع الفاتح");
    expect(getThemeDisplayLabel("google-gemini", "en")).toBe("Dark Mode");
    expect(getThemeDisplayLabel("google-gemini", "ar")).toBe("الوضع الداكن");
  });

  it("preserves legacy apple-light preferences without destructive rewriting", () => {
    expect(parseSafiTheme("apple-light")).toBe("apple-light");
    expect(parseSafiTheme("vision-glass")).toBe("vision-glass");
    expect(parseSafiTheme("google-gemini")).toBe("google-gemini");
    expect(parseSafiTheme("light")).toBe("vision-glass");
    expect(parseSafiTheme("dark")).toBe("google-gemini");
    expect(parseSafiTheme("unknown-theme")).toBeNull();
    expect(getThemeDisplayLabel("apple-light", "en")).toBe("Classic Light");
    expect(getThemeDisplayLabel("apple-light", "ar")).toBe("الفاتح الكلاسيكي");
  });

  it("configures pre-paint boot initialization with Vision Glass default and legacy preservation", () => {
    const html = readFileSync(new URL("../../index.html", import.meta.url), "utf8");
    const bootLocale = readFileSync(
      new URL("../../public/boot-locale.js", import.meta.url),
      "utf8"
    );
    const css = readFileSync(new URL("../index.css", import.meta.url), "utf8");

    expect(html).toContain('data-theme="vision-glass"');
    expect(bootLocale).toContain(SAFI_THEME_STORAGE_KEY);
    expect(bootLocale).toContain('let theme = "vision-glass"');
    expect(bootLocale).toContain('"apple-light"');
    expect(css).toContain(':root,\n:root[data-theme="light"],\n:root[data-theme="vision-glass"]');
    expect(css).toContain(':root[data-theme="apple-light"]');
    expect(css).toContain(':root[data-theme="google-gemini"]');
  });

  it("does not throw when applying theme in SSR environments", () => {
    expect(() => applySafiTheme("google-gemini")).not.toThrow();
    expect(() => applySafiTheme("vision-glass")).not.toThrow();
  });
});
