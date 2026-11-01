/**
 * SafiData Design Tokens — Motion System (Sections 6.4 & 18.9)
 * Motion explains spatial origin and destination; respects prefers-reduced-motion (120ms fade).
 */

export const MOTION_DURATIONS_MS = {
  instant: 100,
  fast: 160,
  base: 240,
  slow: 360,
  deliberate: 520
} as const;

export type MotionDurationKey = keyof typeof MOTION_DURATIONS_MS;
export type MotionDurationMs = (typeof MOTION_DURATIONS_MS)[MotionDurationKey];

export const MOTION_DURATIONS_CSS: Readonly<Record<MotionDurationKey, `${number}ms`>> = {
  instant: "100ms",
  fast: "160ms",
  base: "240ms",
  slow: "360ms",
  deliberate: "520ms"
} as const;

/**
 * Easing curves per Section 6.4.
 */
export const MOTION_EASINGS = {
  standard: "cubic-bezier(0.2, 0, 0, 1)",
  enter: "cubic-bezier(0.05, 0.7, 0.1, 1)",
  exit: "cubic-bezier(0.3, 0, 1, 1)"
} as const;

export type MotionEasingKey = keyof typeof MOTION_EASINGS;

/**
 * Drag-and-drop spring physics per Section 6.4: stiffness 260, damping 26.
 */
export const MOTION_SPRING = {
  stiffness: 260,
  damping: 26
} as const;

/**
 * Entrance choreography & performance guardrails per Sections 6.4 and 18.9.
 */
export const MOTION_CHOREOGRAPHY = {
  enterSlideMinPx: 8,
  enterSlideMaxPx: 16,
  listStaggerStepMs: 24,
  listStaggerMaxItems: 8,
  allowedAnimatedProperties: ["transform", "opacity"] as const,
  targetFps: 60,
  maxConcurrentScreenAnimations: 2
} as const;

/**
 * Reduced motion fallback per Section 6.4:
 * When prefers-reduced-motion is active, replace all transitions with a 120ms opacity fade
 * and disable decorative animations and spatial slide offsets.
 */
export const REDUCED_MOTION_TOKENS = {
  fadeMs: 120,
  fadeCss: "120ms",
  slidePx: 0,
  staggerStepMs: 0,
  decorativeMotionEnabled: false
} as const;

export const MOTION_TOKENS = {
  durationsMs: MOTION_DURATIONS_MS,
  durationsCss: MOTION_DURATIONS_CSS,
  easings: MOTION_EASINGS,
  spring: MOTION_SPRING,
  choreography: MOTION_CHOREOGRAPHY,
  reducedMotion: REDUCED_MOTION_TOKENS
} as const;
