/**
 * `apps/web/src/lib/recipe.ts`
 *
 * Deterministic recipe builder for the Recipes studio: the same toggles always
 * produce the same canonical JSON and therefore the same BLAKE3 digest, and the
 * F1 watch-folder contract is validated before a recipe may be exported.
 */
import type { NormOptions } from "./arabic-normalize";
import { blake3Hex } from "./pii";

export type RecipeStepId = keyof NormOptions;

export interface RecipeStepDefinition {
  readonly id: RecipeStepId;
  readonly labelAr: string;
  readonly labelEn: string;
  readonly detailAr: string;
  readonly detailEn: string;
}

/** Canonical order: the digest depends on it, so it is never sorted at runtime. */
export const RECIPE_STEPS: readonly RecipeStepDefinition[] = [
  { id: "unifyAlif", labelAr: "توحيد الألف", labelEn: "Normalize Alif", detailAr: "أ إ آ ٱ ← ا", detailEn: "أ / إ / آ / ٱ → ا" },
  { id: "unifyYa", labelAr: "توحيد الياء", labelEn: "Normalize Ya", detailAr: "ى ← ي", detailEn: "ى → ي" },
  { id: "unifyHa", labelAr: "توحيد التاء المربوطة", labelEn: "Normalize Taa Marbuta", detailAr: "ة ← ه", detailEn: "ة → ه" },
  { id: "removeTatweel", labelAr: "إزالة التطويل", labelEn: "Remove Tatweel", detailAr: "حذف ـ الكاشيدة", detailEn: "Remove the kashida elongation mark" },
  { id: "removeDiacritics", labelAr: "إزالة التشكيل", labelEn: "Remove Diacritics", detailAr: "حذف الفتحات والضمات والتنوين", detailEn: "Remove Arabic vowel marks and tanween" },
  { id: "normalizeDigits", labelAr: "توحيد الأرقام", labelEn: "Normalize Digits", detailAr: "٠-٩ و۰-۹ ← 0-9", detailEn: "٠-٩ and ۰-۹ → 0-9" },
  { id: "joinCompoundNames", labelAr: "وصل الأسماء المركبة", labelEn: "Join Compound Names", detailAr: "عبد الله ← عبدالله", detailEn: "عبد الله → عبدالله" },
  {
    id: "cleanWhitespace",
    labelAr: "تنظيف المسافات",
    labelEn: "Clean Whitespace",
    detailAr: "حذف المحارف غير المرئية والمسافات المكررة",
    detailEn: "Remove invisible characters and repeated whitespace"
  },
  { id: "removeTitles", labelAr: "إزالة الألقاب", labelEn: "Remove Titles", detailAr: "د. م. أ. الأستاذ دكتور", detailEn: "Remove common professional and academic titles" }
];

export type RecipeToggles = Readonly<Record<RecipeStepId, boolean>>;

export const DEFAULT_RECIPE_TOGGLES: RecipeToggles = {
  unifyAlif: true,
  unifyYa: true,
  unifyHa: true,
  removeTatweel: true,
  removeDiacritics: true,
  cleanWhitespace: true,
  normalizeDigits: true,
  joinCompoundNames: true,
  removeTitles: false
};

export const RECIPE_SCHEMA_VERSION = "1.0.0";

export interface CanonicalRecipeStep {
  readonly id: RecipeStepId;
  readonly enabled: boolean;
}

export interface CanonicalRecipe {
  readonly schema: "safi.recipe";
  readonly version: string;
  readonly name: string;
  readonly steps: readonly CanonicalRecipeStep[];
}

/**
 * The recipe name is trimmed and length-capped; steps follow `RECIPE_STEPS`
 * order. Two drafts that differ only in object key order serialize identically.
 */
export function buildCanonicalRecipe(name: string, toggles: RecipeToggles): CanonicalRecipe {
  return {
    schema: "safi.recipe",
    version: RECIPE_SCHEMA_VERSION,
    name: name.trim().slice(0, 80),
    steps: RECIPE_STEPS.map((step) => ({ id: step.id, enabled: toggles[step.id] }))
  };
}

export function canonicalRecipeJson(name: string, toggles: RecipeToggles): string {
  return JSON.stringify(buildCanonicalRecipe(name, toggles));
}

/** 64 hex chars of real BLAKE3 over the canonical JSON. */
export function recipeDigest(name: string, toggles: RecipeToggles): string {
  return blake3Hex(canonicalRecipeJson(name, toggles), 32);
}

export function enabledStepCount(toggles: RecipeToggles): number {
  return RECIPE_STEPS.filter((step) => toggles[step.id]).length;
}

export type WatchFolderValidation =
  | { readonly ok: true; readonly normalized: string }
  | { readonly ok: false; readonly code: "SF-EXP-001"; readonly messageAr: string };

const ABSOLUTE_PATH = /^(?:[A-Za-z]:[\\/]|\/)/;

/**
 * F1 contract: an absolute path, no `..` traversal, no NUL byte. The desktop
 * shell additionally refuses symlinks before it starts the watcher.
 */
export function validateWatchFolder(raw: string): WatchFolderValidation {
  const trimmed = raw.trim();
  if (trimmed.length === 0) {
    return {
      ok: false,
      code: "SF-EXP-001",
      messageAr: "اختر مجلدًا مطلقًا للمخرجات، مثل C:\\Safi\\Output أو /home/safi/output."
    };
  }
  if (trimmed.includes("\u0000")) {
    return { ok: false, code: "SF-EXP-001", messageAr: "مسار المجلد يحتوي محرفًا غير صالح." };
  }
  if (!ABSOLUTE_PATH.test(trimmed)) {
    return {
      ok: false,
      code: "SF-EXP-001",
      messageAr: "يجب أن يكون مسار المجلد مطلقًا (يبدأ بمحرك قرص أو بشرطة مائلة)."
    };
  }
  const segments = trimmed.split(/[\\/]+/);
  if (segments.includes("..")) {
    return {
      ok: false,
      code: "SF-EXP-001",
      messageAr: "لا يُسمح بالخروج من المجلد الهدف: احذف أي مقطع ‎..‎ من المسار."
    };
  }
  let normalized = trimmed.replace(/[\\/]+/g, "/");
  if (normalized.length > 1 && !/^[A-Za-z]:\/$/.test(normalized)) {
    normalized = normalized.replace(/\/+$/, "");
  }
  return { ok: true, normalized };
}

export interface RecipeTemplate {
  readonly id: string;
  readonly nameAr: string;
  readonly nameEn: string;
  readonly descriptionAr: string;
  readonly descriptionEn: string;
  readonly toggles: RecipeToggles;
}

export const RECIPE_TEMPLATES: readonly RecipeTemplate[] = [
  {
    id: "clients",
    nameAr: "تنقية أسماء العملاء",
    nameEn: "Clean Customer Names",
    descriptionAr: "توحيد الألف والتاء المربوطة، إزالة اللقب، ووصل الأسماء المركبة قبل المطابقة.",
    descriptionEn: "Normalize Arabic letter variants, remove titles, and join compound names before matching.",
    toggles: {
      ...DEFAULT_RECIPE_TOGGLES,
      unifyHa: true,
      joinCompoundNames: true,
      removeTitles: true
    }
  },
  {
    id: "phones",
    nameAr: "توحيد أرقام الهواتف",
    nameEn: "Normalize Phone Numbers",
    descriptionAr: "أرقام لاتينية موحدة ومسافات نظيفة مع إبقاء بادئة الدولة كما هي.",
    descriptionEn: "Standardize digits and whitespace while preserving each country calling code.",
    toggles: {
      ...DEFAULT_RECIPE_TOGGLES,
      unifyAlif: false,
      unifyYa: false,
      unifyHa: false,
      normalizeDigits: true
    }
  },
  {
    id: "schools",
    nameAr: "تقارير المدارس",
    nameEn: "School Reports",
    descriptionAr: "تنظيف أسماء الطلاب مع إبقاء التشكيل عند الحاجة لإظهار الاسم كما ورد.",
    descriptionEn: "Clean student names while preserving diacritics when the original spelling matters.",
    toggles: {
      ...DEFAULT_RECIPE_TOGGLES,
      removeDiacritics: false,
      removeTitles: true
    }
  },
  {
    id: "payroll",
    nameAr: "كشوف الرواتب",
    nameEn: "Payroll Sheets",
    descriptionAr: "توحيد الأرقام والمسافات فقط، دون أي تغيير في الحروف أو الأسماء.",
    descriptionEn: "Normalize digits and whitespace only, without changing letters or names.",
    toggles: {
      unifyAlif: false,
      unifyYa: false,
      unifyHa: false,
      removeTatweel: false,
      removeDiacritics: false,
      cleanWhitespace: true,
      normalizeDigits: true,
      joinCompoundNames: false,
      removeTitles: false
    }
  }
];
