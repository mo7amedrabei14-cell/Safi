/**
 * SafiData Design Tokens — Spacing Grid, Density Modes, Radii, Shadows, Glass & Layout/A11y Metrics
 * (Sections 6.1, 6.3, 6.5, 6.8, 18.1 ن5, and 18.9)
 */

export type DensityMode = "comfortable" | "compact";

/**
 * 4px spacing grid per Section 6.3: 4 / 8 / 12 / 16 / 24 / 32 / 48 / 64.
 */
export const SPACING_GRID_PX = {
  "1": 4,
  "2": 8,
  "3": 12,
  "4": 16,
  "6": 24,
  "8": 32,
  "12": 48,
  "16": 64
} as const;

export type SpacingStepKey = keyof typeof SPACING_GRID_PX;
export type SpacingGridValuePx = (typeof SPACING_GRID_PX)[SpacingStepKey];

export const SPACING_GRID_CSS: Readonly<Record<SpacingStepKey, `${number}px`>> = {
  "1": "4px",
  "2": "8px",
  "3": "12px",
  "4": "16px",
  "6": "24px",
  "8": "32px",
  "12": "48px",
  "16": "64px"
} as const;

export interface DensitySpacingScale {
  readonly mode: DensityMode;
  readonly labelAr: "مريح" | "مدمج";
  readonly cellInlinePx: SpacingGridValuePx;
  readonly cellBlockPx: SpacingGridValuePx;
  readonly rowHeightPx: number;
  readonly controlPaddingInlinePx: SpacingGridValuePx;
  readonly controlPaddingBlockPx: number;
  readonly cardPaddingPx: SpacingGridValuePx;
  readonly panelPaddingPx: SpacingGridValuePx;
  readonly sectionGapPx: SpacingGridValuePx;
  readonly stackGapPx: SpacingGridValuePx;
  readonly inlineGapPx: SpacingGridValuePx;
}

/**
 * Display density modes («مريح» comfortable & «مدمج» compact) per Sections 6.1 and 18.9.
 * Switching density substantively alters the semantic spacing scale on the 4px grid.
 */
export const DENSITY_SCALES: Readonly<Record<DensityMode, DensitySpacingScale>> = {
  comfortable: {
    mode: "comfortable",
    labelAr: "مريح",
    cellInlinePx: 12,
    cellBlockPx: 8,
    rowHeightPx: 40,
    controlPaddingInlinePx: 16,
    controlPaddingBlockPx: 10,
    cardPaddingPx: 24,
    panelPaddingPx: 24,
    sectionGapPx: 24,
    stackGapPx: 12,
    inlineGapPx: 8
  },
  compact: {
    mode: "compact",
    labelAr: "مدمج",
    cellInlinePx: 8,
    cellBlockPx: 4,
    rowHeightPx: 32,
    controlPaddingInlinePx: 12,
    controlPaddingBlockPx: 8,
    cardPaddingPx: 16,
    panelPaddingPx: 16,
    sectionGapPx: 16,
    stackGapPx: 8,
    inlineGapPx: 4
  }
} as const;

/**
 * Border radii per Section 6.3: 6 for fields, 10 for cards, 16 for panels, 999 for pills.
 */
export const RADII_PX = {
  field: 6,
  card: 10,
  panel: 16,
  pill: 999
} as const;

export type RadiusKey = keyof typeof RADII_PX;

export const RADII_CSS: Readonly<Record<RadiusKey, `${number}px`>> = {
  field: "6px",
  card: "10px",
  panel: "16px",
  pill: "999px"
} as const;

export interface ElevationShadows {
  readonly e1: string;
  readonly e2: string;
  readonly e3: string;
}

/**
 * Elevation shadows per Section 6.3:
 * - Light: e1, e2, e3 soft slate shadows.
 * - Dark: replaced by crisp borders and subtle primary glow.
 */
export const SHADOW_TOKENS = {
  light: {
    e1: "0 1px 2px rgba(15,23,42,.06)",
    e2: "0 4px 12px rgba(15,23,42,.08)",
    e3: "0 12px 32px rgba(15,23,42,.14)"
  },
  dark: {
    e1: "0 0 0 1px rgba(38,50,63,0.9), 0 1px 2px rgba(60,195,214,0.04)",
    e2: "0 0 0 1px rgba(38,50,63,1), 0 4px 12px rgba(60,195,214,0.07)",
    e3: "0 0 0 1px rgba(60,195,214,0.22), 0 12px 32px rgba(60,195,214,0.12)"
  }
} as const satisfies Record<"light" | "dark", ElevationShadows>;

/**
 * Glass surface tokens per Sections 6.1 and 6.3:
 * blur 20px and opacity 0.72 for top bar and menus only; disabled when user enables reduced transparency.
 */
export const GLASS_TOKENS = {
  blurPx: 20,
  blurCss: "blur(20px)",
  opacity: 0.72,
  reducedTransparencyBlurPx: 0,
  reducedTransparencyBlurCss: "none",
  reducedTransparencyOpacity: 1,
  allowedSurfaces: ["top-bar", "menus"] as const
} as const;

/**
 * Interactive accessibility & workspace layout metrics per Sections 6.1, 6.5, 6.8, 18.1 ن5, and 18.9.
 */
export const INTERACTION_LAYOUT_TOKENS = {
  minTouchTargetPx: 44,
  minTargetGapPx: 8,
  focusRingWidthPx: 2,
  topBarHeightPx: 56,
  scrollPaddingTopPx: 56,
  sidePanelWidthPx: 320
} as const;
