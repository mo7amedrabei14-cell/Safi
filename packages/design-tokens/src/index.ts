/**
 * `@safi/design-tokens` — Complete Design Token System (`T1.01`)
 * Colors (Light & Dark), Typography, 4px Spacing Grid & Density Modes,
 * Radii, Shadows, Glass Layers, Motion, Tailwind CSS v4 @theme generator,
 * and WCAG 2.2 AA Contrast Verification Engine.
 */

export const PACKAGE_ID = "@safi/design-tokens" as const;

export {
  COLOR_CSS_VARS,
  COLOR_TOKENS,
  DARK_COLORS,
  LIGHT_COLORS,
  type ColorTokenKey,
  type HexColor,
  type ThemeColorTokens,
  type ThemeMode
} from "./colors.ts";

export {
  FONT_FAMILIES,
  FONT_NUMERIC_FEATURES,
  FONT_SIZES_PX,
  FONT_SIZES_REM,
  FONT_WEIGHTS,
  LINE_HEIGHTS,
  TYPOGRAPHY_TOKENS,
  type AllowedFontWeight,
  type FontSizeKey,
  type FontSizePx,
  type FontWeightKey,
  type ScriptLineHeightKey
} from "./typography.ts";

export {
  DENSITY_SCALES,
  GLASS_TOKENS,
  INTERACTION_LAYOUT_TOKENS,
  RADII_CSS,
  RADII_PX,
  SHADOW_TOKENS,
  SPACING_GRID_CSS,
  SPACING_GRID_PX,
  type DensityMode,
  type DensitySpacingScale,
  type ElevationShadows,
  type RadiusKey,
  type SpacingGridValuePx,
  type SpacingStepKey
} from "./spacing.ts";

export {
  MOTION_CHOREOGRAPHY,
  MOTION_DURATIONS_CSS,
  MOTION_DURATIONS_MS,
  MOTION_EASINGS,
  MOTION_SPRING,
  MOTION_TOKENS,
  REDUCED_MOTION_TOKENS,
  type MotionDurationKey,
  type MotionDurationMs,
  type MotionEasingKey
} from "./motion.ts";

export {
  WCAG_AA_TEXT_MIN_RATIO,
  WCAG_AA_UI_GRAPHIC_MIN_RATIO,
  auditAllThemesContrast,
  auditThemeContrast,
  contrastRatio,
  parseHexToRgb,
  relativeLuminance,
  type ContrastAuditReport,
  type ContrastCheckItem,
  type ContrastPairCategory
} from "./contrast.ts";

export { generateTokensCss } from "./css.ts";
export { runContrastVerification } from "./verify-contrast.ts";
