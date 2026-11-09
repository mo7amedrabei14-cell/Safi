/**
 * SafiData Design Tokens — WCAG 2.2 AA Contrast Engine & Automated Gate (Sections 6.2, 6.8, 18.1 ن5)
 * Fails the build automatically if any text pair drops below 4.5:1 or any UI/chart element drops below 3.0:1.
 */

import {
  COLOR_TOKENS,
  type ColorTokenKey,
  type HexColor,
  type ThemeColorTokens,
  type ThemeMode
} from "./colors.ts";

export const WCAG_AA_TEXT_MIN_RATIO = 4.5 as const;
export const WCAG_AA_UI_GRAPHIC_MIN_RATIO = 3.0 as const;

export type ContrastPairCategory = "text" | "ui-graphic";

export interface ContrastCheckItem {
  readonly theme: ThemeMode;
  readonly category: ContrastPairCategory;
  readonly foregroundToken: ColorTokenKey;
  readonly backgroundToken: ColorTokenKey;
  readonly foregroundHex: HexColor;
  readonly backgroundHex: HexColor;
  readonly ratio: number;
  readonly minRequired: number;
  readonly passed: boolean;
}

export interface ContrastAuditReport {
  readonly passed: boolean;
  readonly checks: readonly ContrastCheckItem[];
  readonly failures: readonly ContrastCheckItem[];
}

const HEX_COLOR_RE = /^#([0-9a-fA-F]{6})$/;

/**
 * Parses a 6-digit hex color (#RRGGBB) into an [R, G, B] tuple (0..255).
 */
export function parseHexToRgb(hex: string): readonly [number, number, number] {
  const match = HEX_COLOR_RE.exec(hex.trim());
  const digits = match?.[1];
  if (!digits) {
    throw new Error(`[SF-SYS-001] Invalid 6-digit hex color: "${hex}"`);
  }
  const r = Number.parseInt(digits.slice(0, 2), 16);
  const g = Number.parseInt(digits.slice(2, 4), 16);
  const b = Number.parseInt(digits.slice(4, 6), 16);
  return [r, g, b] as const;
}

function channelToLinear(channel8Bit: number): number {
  const srgb = channel8Bit / 255;
  return srgb <= 0.04045 ? srgb / 12.92 : ((srgb + 0.055) / 1.055) ** 2.4;
}

/**
 * Calculates WCAG 2.2 relative luminance (0.0 darkest to 1.0 brightest).
 */
export function relativeLuminance(hex: string): number {
  const [r, g, b] = parseHexToRgb(hex);
  return 0.2126 * channelToLinear(r) + 0.7152 * channelToLinear(g) + 0.0722 * channelToLinear(b);
}

/**
 * Calculates WCAG 2.2 contrast ratio between two hex colors (1.0 to 21.0).
 */
export function contrastRatio(foregroundHex: string, backgroundHex: string): number {
  const l1 = relativeLuminance(foregroundHex);
  const l2 = relativeLuminance(backgroundHex);
  const lighter = Math.max(l1, l2);
  const darker = Math.min(l1, l2);
  return (lighter + 0.05) / (darker + 0.05);
}

interface PairSpec {
  readonly category: ContrastPairCategory;
  readonly fg: ColorTokenKey;
  readonly bg: ColorTokenKey;
}

const CORE_SURFACES: readonly ColorTokenKey[] = ["bg", "surface", "surface2"] as const;

const TEXT_TOKENS: readonly ColorTokenKey[] = [
  "textPrimary",
  "textSecondary",
  "textMuted"
] as const;

const SEMANTIC_TEXT_ON_MAIN_SURFACES: readonly ColorTokenKey[] = [
  "primary",
  "primaryHover",
  "success",
  "warning",
  "danger",
  "info"
] as const;

const UI_AND_CHART_TOKENS: readonly ColorTokenKey[] = [
  "primary",
  "primaryHover",
  "success",
  "warning",
  "danger",
  "info"
] as const;

function buildPairSpecsForTheme(theme: ThemeMode): readonly PairSpec[] {
  const pairs: PairSpec[] = [];

  for (const fg of TEXT_TOKENS) {
    for (const bg of CORE_SURFACES) {
      pairs.push({ category: "text", fg, bg });
    }
  }

  pairs.push({ category: "text", fg: "textPrimary", bg: "primarySoft" });
  pairs.push({
    category: "text",
    fg: theme === "light" ? "primaryHover" : "primary",
    bg: "primarySoft"
  });

  for (const fg of SEMANTIC_TEXT_ON_MAIN_SURFACES) {
    pairs.push({ category: "text", fg, bg: "bg" });
    pairs.push({ category: "text", fg, bg: "surface" });
  }

  for (const fg of UI_AND_CHART_TOKENS) {
    for (const bg of CORE_SURFACES) {
      pairs.push({ category: "ui-graphic", fg, bg });
    }
  }

  return pairs;
}

/**
 * Audits a single theme palette against WCAG 2.2 AA thresholds:
 * - Normal text pairs >= 4.5:1
 * - UI & chart graphic pairs >= 3.0:1
 */
export function auditThemeContrast(
  theme: ThemeMode,
  palette: ThemeColorTokens = COLOR_TOKENS[theme]
): ContrastAuditReport {
  const specs = buildPairSpecsForTheme(theme);
  const checks: ContrastCheckItem[] = specs.map((spec) => {
    const fgHex = palette[spec.fg];
    const bgHex = palette[spec.bg];
    const ratio = contrastRatio(fgHex, bgHex);
    const minRequired =
      spec.category === "text" ? WCAG_AA_TEXT_MIN_RATIO : WCAG_AA_UI_GRAPHIC_MIN_RATIO;
    const passed = ratio >= minRequired;
    return {
      theme,
      category: spec.category,
      foregroundToken: spec.fg,
      backgroundToken: spec.bg,
      foregroundHex: fgHex,
      backgroundHex: bgHex,
      ratio,
      minRequired,
      passed
    };
  });

  const failures = checks.filter((item) => !item.passed);
  return {
    passed: failures.length === 0,
    checks,
    failures
  };
}

/**
 * Audits both Light and Dark palettes against WCAG 2.2 AA requirements.
 */
export function auditAllThemesContrast(
  palettes: Readonly<Record<ThemeMode, ThemeColorTokens>> = COLOR_TOKENS
): ContrastAuditReport {
  const lightReport = auditThemeContrast("light", palettes.light);
  const darkReport = auditThemeContrast("dark", palettes.dark);
  const checks = [...lightReport.checks, ...darkReport.checks];
  const failures = [...lightReport.failures, ...darkReport.failures];
  return {
    passed: failures.length === 0,
    checks,
    failures
  };
}
