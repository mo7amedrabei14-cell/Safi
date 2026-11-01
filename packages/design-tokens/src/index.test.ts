import { describe, expect, it } from "vitest";
import {
  COLOR_CSS_VARS,
  COLOR_TOKENS,
  DARK_COLORS,
  DENSITY_SCALES,
  FONT_FAMILIES,
  FONT_NUMERIC_FEATURES,
  FONT_SIZES_PX,
  FONT_WEIGHTS,
  GLASS_TOKENS,
  INTERACTION_LAYOUT_TOKENS,
  LIGHT_COLORS,
  LINE_HEIGHTS,
  MOTION_CHOREOGRAPHY,
  MOTION_DURATIONS_MS,
  MOTION_EASINGS,
  MOTION_SPRING,
  PACKAGE_ID,
  RADII_PX,
  REDUCED_MOTION_TOKENS,
  SHADOW_TOKENS,
  SPACING_GRID_PX,
  TYPOGRAPHY_TOKENS,
  WCAG_AA_TEXT_MIN_RATIO,
  WCAG_AA_UI_GRAPHIC_MIN_RATIO,
  auditAllThemesContrast,
  auditThemeContrast,
  contrastRatio,
  generateTokensCss,
  parseHexToRgb,
  relativeLuminance,
  runContrastVerification
} from "./index.ts";

describe("@safi/design-tokens (T1.01)", () => {
  it("exports canonical package identifier", () => {
    expect(PACKAGE_ID).toBe("@safi/design-tokens");
  });

  describe("Color Tokens (Sections 6.2 & 18.1 ن5)", () => {
    it("defines exact Light and Dark palettes including updated muted text #5B6B7F", () => {
      expect(LIGHT_COLORS).toEqual({
        bg: "#F5F7FA",
        surface: "#FFFFFF",
        surface2: "#EEF2F6",
        border: "#E2E8F0",
        textPrimary: "#0F172A",
        textSecondary: "#475569",
        textMuted: "#5B6B7F",
        primary: "#0B7C8C",
        primaryHover: "#09697A",
        primarySoft: "#E0F4F7",
        success: "#15803D",
        warning: "#B45309",
        danger: "#B91C1C",
        info: "#1D4ED8"
      });

      expect(DARK_COLORS).toEqual({
        bg: "#0B1016",
        surface: "#121A23",
        surface2: "#1A2430",
        border: "#26323F",
        textPrimary: "#E8EEF4",
        textSecondary: "#A8B5C3",
        textMuted: "#8795A5",
        primary: "#3CC3D6",
        primaryHover: "#62D3E2",
        primarySoft: "#0E2B31",
        success: "#4ADE80",
        warning: "#FBBF24",
        danger: "#F87171",
        info: "#60A5FA"
      });

      expect(COLOR_CSS_VARS.textMuted).toBe("--sf-color-text-muted");
      expect(COLOR_CSS_VARS.surface2).toBe("--sf-color-surface-2");
    });

    it("satisfies Section 18.1 (ن5): #5B6B7F achieves >= 4.5:1 on #F5F7FA while legacy #64748B fails", () => {
      const updatedMutedRatio = contrastRatio(LIGHT_COLORS.textMuted, LIGHT_COLORS.bg);
      const legacyMutedRatio = contrastRatio("#64748B", LIGHT_COLORS.bg);

      expect(updatedMutedRatio).toBeGreaterThanOrEqual(WCAG_AA_TEXT_MIN_RATIO);
      expect(updatedMutedRatio).toBeGreaterThan(5.0);
      expect(legacyMutedRatio).toBeLessThan(WCAG_AA_TEXT_MIN_RATIO);
    });
  });

  describe("WCAG 2.2 AA Contrast Gate (Acceptance Criteria)", () => {
    it("verifies all Light and Dark text pairs >= 4.5:1 and UI/chart pairs >= 3.0:1", () => {
      const report = auditAllThemesContrast(COLOR_TOKENS);
      expect(report.passed).toBe(true);
      expect(report.failures).toHaveLength(0);

      const textChecks = report.checks.filter((c) => c.category === "text");
      const graphicChecks = report.checks.filter((c) => c.category === "ui-graphic");

      expect(textChecks.length).toBeGreaterThan(0);
      expect(graphicChecks.length).toBeGreaterThan(0);

      for (const check of textChecks) {
        expect(check.ratio).toBeGreaterThanOrEqual(WCAG_AA_TEXT_MIN_RATIO);
      }
      for (const check of graphicChecks) {
        expect(check.ratio).toBeGreaterThanOrEqual(WCAG_AA_UI_GRAPHIC_MIN_RATIO);
      }
    });

    it("fails the audit automatically when a text token drops below 4.5:1 or a graphic token drops below 3.0:1", () => {
      const brokenTextReport = auditThemeContrast("light", {
        ...LIGHT_COLORS,
        textMuted: "#64748B"
      });
      expect(brokenTextReport.passed).toBe(false);
      expect(
        brokenTextReport.failures.some(
          (f) => f.category === "text" && f.foregroundToken === "textMuted"
        )
      ).toBe(true);

      const brokenGraphicReport = auditThemeContrast("dark", {
        ...DARK_COLORS,
        primary: "#1F2C38"
      });
      expect(brokenGraphicReport.passed).toBe(false);
      expect(
        brokenGraphicReport.failures.some(
          (f) => f.category === "ui-graphic" && f.foregroundToken === "primary"
        )
      ).toBe(true);
    });

    it("runs CLI contrast verification and self-test cleanly", () => {
      expect(runContrastVerification([])).toBe(true);
      expect(runContrastVerification(["--self-test"])).toBe(true);
    });

    it("validates hex parsing and relative luminance boundary values", () => {
      expect(parseHexToRgb("#000000")).toEqual([0, 0, 0]);
      expect(parseHexToRgb("#FFFFFF")).toEqual([255, 255, 255]);
      expect(relativeLuminance("#000000")).toBeCloseTo(0, 5);
      expect(relativeLuminance("#FFFFFF")).toBeCloseTo(1, 5);
      expect(contrastRatio("#FFFFFF", "#000000")).toBeCloseTo(21, 5);
      expect(() => parseHexToRgb("invalid")).toThrow();
    });
  });

  describe("Typography Tokens (Section 6.3)", () => {
    it("configures IBM Plex Sans Arabic, Noto Sans Arabic, Inter, and tabular-nums", () => {
      expect(FONT_FAMILIES.arabicPrimary).toBe("IBM Plex Sans Arabic");
      expect(FONT_FAMILIES.arabicFallback).toBe("Noto Sans Arabic");
      expect(FONT_FAMILIES.latin).toBe("Inter");
      expect(FONT_NUMERIC_FEATURES.fontVariantNumeric).toBe("tabular-nums");
      expect(TYPOGRAPHY_TOKENS.bundledLocally).toBe(true);
      expect(TYPOGRAPHY_TOKENS.allowExternalFontCdn).toBe(false);
    });

    it("enforces the exact font size scale (12/13/14/16/20/24/32/48), line heights (1.6 / 1.45), and weights (400/500/600)", () => {
      expect(Object.values(FONT_SIZES_PX)).toEqual([12, 13, 14, 16, 20, 24, 32, 48]);
      expect(TYPOGRAPHY_TOKENS.baseSizePx).toBe(14);
      expect(LINE_HEIGHTS.arabic).toBe(1.6);
      expect(LINE_HEIGHTS.latin).toBe(1.45);
      expect(FONT_WEIGHTS).toEqual({
        regular: 400,
        medium: 500,
        semibold: 600
      });
    });
  });

  describe("Spacing, Density, Radii, Shadows, Glass & A11y Metrics (Sections 6.3 & 18.9)", () => {
    it("defines the 4px spacing grid (4/8/12/16/24/32/48/64)", () => {
      const values = Object.values(SPACING_GRID_PX);
      expect(values).toEqual([4, 8, 12, 16, 24, 32, 48, 64]);
      for (const v of values) {
        expect(v % 4).toBe(0);
      }
    });

    it("substantively changes spacing between comfortable («مريح») and compact («مدمج») densities", () => {
      const { comfortable, compact } = DENSITY_SCALES;
      expect(comfortable.labelAr).toBe("مريح");
      expect(compact.labelAr).toBe("مدمج");
      expect(compact.cellInlinePx).toBeLessThan(comfortable.cellInlinePx);
      expect(compact.cellBlockPx).toBeLessThan(comfortable.cellBlockPx);
      expect(compact.rowHeightPx).toBeLessThan(comfortable.rowHeightPx);
      expect(compact.cardPaddingPx).toBeLessThan(comfortable.cardPaddingPx);
      expect(compact.panelPaddingPx).toBeLessThan(comfortable.panelPaddingPx);
      expect(compact.sectionGapPx).toBeLessThan(comfortable.sectionGapPx);
    });

    it("defines radii (6/10/16/999), light/dark elevation shadows, glass blur 20px, and 44px touch targets", () => {
      expect(RADII_PX).toEqual({
        field: 6,
        card: 10,
        panel: 16,
        pill: 999
      });
      expect(SHADOW_TOKENS.light.e1).toBe("0 1px 2px rgba(15,23,42,.06)");
      expect(SHADOW_TOKENS.light.e2).toBe("0 4px 12px rgba(15,23,42,.08)");
      expect(SHADOW_TOKENS.light.e3).toBe("0 12px 32px rgba(15,23,42,.14)");
      expect(SHADOW_TOKENS.dark.e1).toContain("1px");
      expect(GLASS_TOKENS.blurPx).toBe(20);
      expect(GLASS_TOKENS.opacity).toBe(0.72);
      expect(GLASS_TOKENS.reducedTransparencyOpacity).toBe(1);
      expect(INTERACTION_LAYOUT_TOKENS.minTouchTargetPx).toBe(44);
      expect(INTERACTION_LAYOUT_TOKENS.minTargetGapPx).toBe(8);
      expect(INTERACTION_LAYOUT_TOKENS.focusRingWidthPx).toBe(2);
      expect(INTERACTION_LAYOUT_TOKENS.scrollPaddingTopPx).toBe(56);
    });
  });

  describe("Motion Tokens (Section 6.4)", () => {
    it("defines exact durations (100/160/240/360/520ms), curves, spring (260/26), and reduced-motion (120ms)", () => {
      expect(MOTION_DURATIONS_MS).toEqual({
        instant: 100,
        fast: 160,
        base: 240,
        slow: 360,
        deliberate: 520
      });
      expect(MOTION_EASINGS).toEqual({
        standard: "cubic-bezier(0.2, 0, 0, 1)",
        enter: "cubic-bezier(0.05, 0.7, 0.1, 1)",
        exit: "cubic-bezier(0.3, 0, 1, 1)"
      });
      expect(MOTION_SPRING).toEqual({
        stiffness: 260,
        damping: 26
      });
      expect(MOTION_CHOREOGRAPHY.enterSlideMinPx).toBe(8);
      expect(MOTION_CHOREOGRAPHY.enterSlideMaxPx).toBe(16);
      expect(MOTION_CHOREOGRAPHY.listStaggerStepMs).toBe(24);
      expect(MOTION_CHOREOGRAPHY.listStaggerMaxItems).toBe(8);
      expect(MOTION_CHOREOGRAPHY.allowedAnimatedProperties).toEqual(["transform", "opacity"]);
      expect(REDUCED_MOTION_TOKENS.fadeMs).toBe(120);
      expect(REDUCED_MOTION_TOKENS.slidePx).toBe(0);
      expect(REDUCED_MOTION_TOKENS.decorativeMotionEnabled).toBe(false);
    });
  });

  describe("Tailwind CSS v4 Stylesheet Generation", () => {
    it("generates deterministic CSS variables for Light, Dark, Densities, Reduced Motion, and @theme", () => {
      const css = generateTokensCss();
      expect(css).toContain("--sf-color-text-muted: #5B6B7F;");
      expect(css).toContain("--sf-color-text-muted: #8795A5;");
      expect(css).toContain('[data-theme="dark"]');
      expect(css).toContain('[data-density="comfortable"]');
      expect(css).toContain('[data-density="compact"]');
      expect(css).toContain("@media (prefers-reduced-motion: reduce)");
      expect(css).toContain("--sf-duration-base: 120ms;");
      expect(css).toContain("@theme {");
      expect(css).toContain("--color-text-muted: var(--sf-color-text-muted);");
    });
  });
});
