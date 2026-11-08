/**
 * `apps/web/src/lib/arabic-normalize.ts`
 *
 * Browser mirror of `crates/sf-arabic/src/normalizer.rs` (`normalize_text`).
 *
 * The studio previews must show the same result the Rust engine produces, so the
 * steps run in the same order as the crate:
 *   1. optional title removal at the start of the value,
 *   2. diacritics / superscript alef / tatweel removal,
 *   3. character maps (alif, ya, ta marbuta, digits, Arabic punctuation),
 *   4. compound-name joining (`عبد الله` -> `عبدالله`),
 *   5. invisible-character removal and whitespace collapsing.
 *
 * The original value is never mutated: the caller keeps it and displays the
 * normalized form next to it (Specification Section 4.4).
 */

export interface NormOptions {
  readonly unifyAlif: boolean;
  readonly unifyYa: boolean;
  readonly unifyHa: boolean;
  readonly removeTatweel: boolean;
  readonly removeDiacritics: boolean;
  readonly cleanWhitespace: boolean;
  /** Eastern Arabic-Indic and Extended Arabic-Indic digits become 0-9. */
  readonly normalizeDigits: boolean;
  readonly joinCompoundNames: boolean;
  readonly removeTitles: boolean;
}

export const DEFAULT_NORM_OPTIONS: NormOptions = {
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

const TITLES: readonly string[] = ["د.", "م.", "أ.", "الأستاذ", "الاستاذ", "مهندس", "دكتور"];

const COMPOUNDS: readonly string[] = ["عبد ", "أبو ", "ابو ", "أم ", "ام "];

/** ZWJ, ZWNJ, LTR/RTL marks and embeddings, BOM and soft hyphen. */
const INVISIBLE_CHARACTERS: ReadonlySet<string> = new Set([
  "\u200C",
  "\u200D",
  "\u200E",
  "\u200F",
  "\u202A",
  "\u202B",
  "\u202C",
  "\u202D",
  "\u202E",
  "\uFEFF",
  "\u00AD"
]);

function isDiacritic(char: string): boolean {
  const code = char.codePointAt(0) ?? 0;
  return code >= 0x064b && code <= 0x065f;
}

function mapCharacter(char: string, options: NormOptions): string {
  let result = char;

  if (options.unifyAlif && (result === "أ" || result === "إ" || result === "آ" || result === "ٱ")) {
    result = "ا";
  }
  if (options.unifyYa && result === "ى") {
    result = "ي";
  }
  if (options.unifyHa && result === "ة") {
    result = "ه";
  }

  const code = result.codePointAt(0) ?? 0;
  if (options.normalizeDigits) {
    if (code >= 0x0660 && code <= 0x0669) {
      return String.fromCodePoint(code - 0x0660 + 0x30);
    }
    if (code >= 0x06f0 && code <= 0x06f9) {
      return String.fromCodePoint(code - 0x06f0 + 0x30);
    }
  }

  if (result === "٫") {
    return ".";
  }
  if (result === "٬") {
    return ",";
  }
  return result === "؟" ? "?" : result;
}

function joinCompoundNames(value: string): string {
  let result = value;
  for (const compound of COMPOUNDS) {
    const glued = compound.trimEnd();
    let updated = "";
    let searchFrom = 0;
    let index = result.indexOf(compound, searchFrom);
    while (index !== -1) {
      updated += result.slice(searchFrom, index) + glued;
      searchFrom = index + compound.length;
      index = result.indexOf(compound, searchFrom);
    }
    updated += result.slice(searchFrom);
    result = updated;
  }
  return result;
}

function cleanWhitespace(value: string): string {
  let cleaned = "";
  let lastWasSpace = true;
  for (const char of value) {
    if (INVISIBLE_CHARACTERS.has(char)) {
      continue;
    }
    if (/\s/.test(char) || char === "\u00A0") {
      if (!lastWasSpace) {
        cleaned += " ";
        lastWasSpace = true;
      }
      continue;
    }
    cleaned += char;
    lastWasSpace = false;
  }
  return cleaned.endsWith(" ") ? cleaned.slice(0, -1) : cleaned;
}

export function normalizeText(input: string, options: NormOptions = DEFAULT_NORM_OPTIONS): string {
  if (input.length === 0) {
    return "";
  }

  let working = input;
  if (options.removeTitles) {
    for (const title of TITLES) {
      if (working.startsWith(title)) {
        working = working.slice(title.length).trimStart();
      }
    }
  }

  let filtered = "";
  for (const char of working) {
    if (options.removeDiacritics && isDiacritic(char)) {
      continue;
    }
    // The engine drops the superscript alef even when diacritics are kept.
    if (char === "\u0670") {
      continue;
    }
    if (options.removeTatweel && char === "\u0640") {
      continue;
    }
    filtered += char;
  }

  let mapped = "";
  for (const char of filtered) {
    mapped += mapCharacter(char, options);
  }

  const joined = options.joinCompoundNames ? joinCompoundNames(mapped) : mapped;
  return options.cleanWhitespace ? cleanWhitespace(joined) : joined;
}

/**
 * Field values are normalized one per line: the engine never receives a whole
 * document as a single value, and collapsing line breaks would let one field's
 * rule consume the following fields.
 */
export function normalizeLines(text: string, options: NormOptions = DEFAULT_NORM_OPTIONS): string {
  return text
    .split(/\r?\n/)
    .map((line) => normalizeText(line, options))
    .join("\n");
}
