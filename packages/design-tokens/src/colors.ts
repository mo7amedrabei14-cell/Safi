/**
 * SafiData Design Tokens — Color System (Sections 6.2 & 18.1 ن5)
 * Supervised by UI/UX Pro Max Skill & I-Have-ADHD Skill.
 * All text pairs satisfy WCAG 2.2 AA (>= 4.5:1) and all UI/chart pairs satisfy (>= 3.0:1).
 */

export type ThemeMode = "light" | "dark";

export type HexColor = `#${string}`;

export interface ThemeColorTokens {
  readonly bg: HexColor;
  readonly surface: HexColor;
  readonly surface2: HexColor;
  readonly border: HexColor;
  readonly textPrimary: HexColor;
  readonly textSecondary: HexColor;
  readonly textMuted: HexColor;
  readonly primary: HexColor;
  readonly primaryHover: HexColor;
  readonly primarySoft: HexColor;
  readonly success: HexColor;
  readonly warning: HexColor;
  readonly danger: HexColor;
  readonly info: HexColor;
}

export type ColorTokenKey = keyof ThemeColorTokens;

/**
 * Light mode palette per Section 6.2 and Section 18.1 (ن5: muted text #5B6B7F for >= 4.5:1 contrast on #F5F7FA).
 */
export const LIGHT_COLORS = {
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
} as const satisfies ThemeColorTokens;

/**
 * Dark mode palette per Section 6.2.
 */
export const DARK_COLORS = {
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
} as const satisfies ThemeColorTokens;

export const COLOR_TOKENS: Readonly<Record<ThemeMode, ThemeColorTokens>> = {
  light: LIGHT_COLORS,
  dark: DARK_COLORS
} as const;

export const COLOR_CSS_VARS: Readonly<Record<ColorTokenKey, `--sf-color-${string}`>> = {
  bg: "--sf-color-bg",
  surface: "--sf-color-surface",
  surface2: "--sf-color-surface-2",
  border: "--sf-color-border",
  textPrimary: "--sf-color-text-primary",
  textSecondary: "--sf-color-text-secondary",
  textMuted: "--sf-color-text-muted",
  primary: "--sf-color-primary",
  primaryHover: "--sf-color-primary-hover",
  primarySoft: "--sf-color-primary-soft",
  success: "--sf-color-success",
  warning: "--sf-color-warning",
  danger: "--sf-color-danger",
  info: "--sf-color-info"
} as const;
