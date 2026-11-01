/**
 * SafiData Design Tokens — Typography System (Section 6.3)
 * Fonts are bundled locally with zero external network requests.
 */

export const FONT_FAMILIES = {
  arabicPrimary: "IBM Plex Sans Arabic",
  arabicFallback: "Noto Sans Arabic",
  latin: "Inter",
  sansArabic: '"IBM Plex Sans Arabic", "Noto Sans Arabic", sans-serif',
  sansLatin: '"Inter", "IBM Plex Sans Arabic", "Noto Sans Arabic", sans-serif',
  sansBilingual: '"IBM Plex Sans Arabic", "Noto Sans Arabic", "Inter", sans-serif'
} as const;

export const FONT_NUMERIC_FEATURES = {
  fontVariantNumeric: "tabular-nums",
  fontFeatureSettings: '"tnum" 1'
} as const;

/**
 * Font size scale in pixels per Section 6.3: 12 / 13 / 14 (base) / 16 / 20 / 24 / 32 / 48.
 */
export const FONT_SIZES_PX = {
  xs: 12,
  sm: 13,
  base: 14,
  md: 16,
  lg: 20,
  xl: 24,
  "2xl": 32,
  "3xl": 48
} as const;

export type FontSizeKey = keyof typeof FONT_SIZES_PX;
export type FontSizePx = (typeof FONT_SIZES_PX)[FontSizeKey];

/**
 * Font size scale in rem (assuming 16px browser root for WCAG 200% zoom compatibility).
 */
export const FONT_SIZES_REM: Readonly<Record<FontSizeKey, `${string}rem`>> = {
  xs: "0.75rem",
  sm: "0.8125rem",
  base: "0.875rem",
  md: "1rem",
  lg: "1.25rem",
  xl: "1.5rem",
  "2xl": "2rem",
  "3xl": "3rem"
} as const;

/**
 * Line heights per Section 6.3: 1.6 for Arabic, 1.45 for Latin.
 */
export const LINE_HEIGHTS = {
  arabic: 1.6,
  latin: 1.45
} as const;

export type ScriptLineHeightKey = keyof typeof LINE_HEIGHTS;

/**
 * Font weights per Section 6.3: strictly 400, 500, and 600 only.
 */
export const FONT_WEIGHTS = {
  regular: 400,
  medium: 500,
  semibold: 600
} as const;

export type FontWeightKey = keyof typeof FONT_WEIGHTS;
export type AllowedFontWeight = (typeof FONT_WEIGHTS)[FontWeightKey];

export const TYPOGRAPHY_TOKENS = {
  families: FONT_FAMILIES,
  numeric: FONT_NUMERIC_FEATURES,
  sizesPx: FONT_SIZES_PX,
  sizesRem: FONT_SIZES_REM,
  baseSizePx: FONT_SIZES_PX.base,
  lineHeights: LINE_HEIGHTS,
  weights: FONT_WEIGHTS,
  bundledLocally: true,
  allowExternalFontCdn: false
} as const;
