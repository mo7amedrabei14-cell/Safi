/**
 * `apps/web/src/lib/pii.ts`
 *
 * Local PII detection, masking and deterministic tokenization for the Cleaning &
 * PII studio.
 *
 * Cryptography rule: BLAKE3 comes from the audited `@noble/hashes` implementation
 * (verified against the official `"abc"` test vector in the unit tests). No
 * hand-written hash or cipher exists in this file.
 *
 * A token is `sf_<kind>_<32 hex>` computed as
 * `BLAKE3(salt + ":" + kind + ":" + value, 16 bytes)`. Same salt + same value
 * always yields the same token (joins stay possible), and a different salt yields
 * an unrelated token (unlinkable across tenants).
 */
import { blake3 } from "@noble/hashes/blake3.js";

export type PiiKind = "name" | "phone" | "email" | "national_id" | "iban";

export interface PiiMatch {
  readonly kind: PiiKind;
  readonly start: number;
  readonly end: number;
  readonly value: string;
}

export interface PiiToken {
  readonly kind: PiiKind;
  readonly token: string;
}

export const PII_KIND_LABELS: Readonly<Record<PiiKind, string>> = {
  name: "اسم شخص",
  phone: "رقم هاتف",
  email: "بريد إلكتروني",
  national_id: "رقم قومي",
  iban: "حساب IBAN"
};

const TOKEN_PREFIX: Readonly<Record<PiiKind, string>> = {
  name: "nm",
  phone: "ph",
  email: "em",
  national_id: "id",
  iban: "ib"
};

const BULLET = "•";

function toHex(bytes: Uint8Array): string {
  let output = "";
  for (const byte of bytes) {
    output += byte.toString(16).padStart(2, "0");
  }
  return output;
}

/** Exposed for tests: real BLAKE3 hex digest of a UTF-8 string. */
export function blake3Hex(value: string, length = 32): string {
  return toHex(blake3(new TextEncoder().encode(value), { dkLen: length }));
}

export function tokenizeValue(kind: PiiKind, value: string, salt: string): string {
  return `sf_${TOKEN_PREFIX[kind]}_${blake3Hex(`${salt}:${kind}:${value}`, 16)}`;
}

export function maskValue(kind: PiiKind, value: string): string {
  const trimmed = value.trim();
  switch (kind) {
    case "phone": {
      if (trimmed.length <= 7) {
        return BULLET.repeat(trimmed.length);
      }
      return `${trimmed.slice(0, 3)}${BULLET.repeat(trimmed.length - 7)}${trimmed.slice(-4)}`;
    }
    case "email": {
      const at = trimmed.indexOf("@");
      if (at <= 0) {
        return BULLET.repeat(trimmed.length);
      }
      return `${trimmed.slice(0, 1)}${BULLET.repeat(3)}${trimmed.slice(at)}`;
    }
    case "national_id":
    case "iban": {
      if (trimmed.length <= 8) {
        return BULLET.repeat(trimmed.length);
      }
      return `${trimmed.slice(0, 4)}${BULLET.repeat(trimmed.length - 8)}${trimmed.slice(-4)}`;
    }
    case "name": {
      const [first = ""] = trimmed.split(/\s+/);
      return first.length === 0 ? BULLET.repeat(trimmed.length) : `${first} ${BULLET.repeat(3)}`;
    }
  }
}

interface DetectionRule {
  readonly kind: PiiKind;
  readonly pattern: RegExp;
  /** Capture group holding the value; 0 means the whole match. */
  readonly group: number;
}

const DETECTION_RULES: readonly DetectionRule[] = [
  {
    kind: "email",
    pattern: /[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+/g,
    group: 0
  },
  { kind: "iban", pattern: /\bEG\d{2}[A-Z0-9]{25}\b/gi, group: 0 },
  { kind: "national_id", pattern: /(?<!\d)[23]\d{13}(?!\d)/g, group: 0 },
  { kind: "phone", pattern: /(?:\+20|20|0)1[0125]\d{8}(?!\d)/g, group: 0 },
  { kind: "name", pattern: /(?:الاسم|الاسم الكامل|اسم العميل)\s*[:：]\s*([^\n،,]+)/g, group: 1 }
];

/** Non-overlapping matches, ordered by position in the text. */
export function detectPii(text: string): PiiMatch[] {
  const found: PiiMatch[] = [];
  for (const rule of DETECTION_RULES) {
    const pattern = new RegExp(rule.pattern.source, rule.pattern.flags);
    for (const match of text.matchAll(pattern)) {
      const captured = match[rule.group];
      if (captured === undefined || captured.trim().length === 0) {
        continue;
      }
      const offset = rule.group === 0 ? 0 : match[0].length - captured.length;
      const start = (match.index ?? 0) + offset;
      found.push({ kind: rule.kind, start, end: start + captured.length, value: captured });
    }
  }

  found.sort((left, right) => left.start - right.start || right.end - left.end);

  const accepted: PiiMatch[] = [];
  let cursor = -1;
  for (const candidate of found) {
    if (candidate.start >= cursor) {
      accepted.push(candidate);
      cursor = candidate.end;
    }
  }
  return accepted;
}

/** Replaces every detected value with its masked form, preserving the rest. */
export function maskText(text: string, matches: readonly PiiMatch[] = detectPii(text)): string {
  let output = "";
  let cursor = 0;
  for (const match of matches) {
    output += text.slice(cursor, match.start) + maskValue(match.kind, match.value);
    cursor = match.end;
  }
  return output + text.slice(cursor);
}

export function tokenizeMatches(matches: readonly PiiMatch[], salt: string): readonly PiiToken[] {
  return matches.map((match) => ({
    kind: match.kind,
    token: tokenizeValue(match.kind, match.value, salt)
  }));
}

export function countByKind(matches: readonly PiiMatch[]): Readonly<Record<PiiKind, number>> {
  const counts: Record<PiiKind, number> = {
    name: 0,
    phone: 0,
    email: 0,
    national_id: 0,
    iban: 0
  };
  for (const match of matches) {
    counts[match.kind] += 1;
  }
  return counts;
}
