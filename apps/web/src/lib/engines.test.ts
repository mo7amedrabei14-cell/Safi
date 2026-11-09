import { describe, expect, it } from "vitest";
import { DEFAULT_NORM_OPTIONS, normalizeLines, normalizeText } from "./arabic-normalize";
import { blake3Hex, countByKind, detectPii, maskText, maskValue, tokenizeValue } from "./pii";
import {
  DEFAULT_RECIPE_TOGGLES,
  RECIPE_STEPS,
  canonicalRecipeJson,
  enabledStepCount,
  recipeDigest,
  validateWatchFolder
} from "./recipe";
import type { RecipeToggles } from "./recipe";
import { buildRecipeReportHtml, escapeHtml } from "./report";
import { runClientSecurityAudit, summarizeAudit } from "./client-audit";
import { downloadTextFile } from "./download";

const PII_SAMPLE = [
  "الاسم: أَحْمَدُ محمد عبد الله",
  "الهاتف: ٠١٠١٢٣٤٥٦٧٨",
  "البريد: ahmed.mohamed@example.com",
  "الرقم القومي: 29801011234567"
].join("\n");

describe("Arabic normalizer mirrors the sf-arabic engine", () => {
  it("normalizes diacritics, tatweel, ta marbuta and Arabic-Indic digits", () => {
    expect(normalizeText("أَحْمَدُ مُحَمَّدْ  الْـعَبْدِي ـــ ١٢٣٤٥")).toBe(
      "احمد محمد العبدي 12345"
    );
  });

  it("joins compound names after alif unification", () => {
    expect(normalizeText("عبد الله و أبو بكر")).toBe("عبدالله و ابوبكر");
  });

  it("is idempotent, so re-running a recipe never changes the value again", () => {
    const once = normalizeLines(PII_SAMPLE);
    expect(normalizeLines(once)).toBe(once);
    expect(once.split("\n")).toHaveLength(4);
  });

  it("removes bidi marks and collapses whitespace", () => {
    expect(normalizeText("صافي\u200F   البيانات\u00A0\u00A0هنا")).toBe("صافي البيانات هنا");
  });

  it("honours disabled options and optional title removal", () => {
    expect(normalizeText("أحمد", { ...DEFAULT_NORM_OPTIONS, unifyAlif: false })).toBe("أحمد");
    expect(normalizeText("د. أحمد", { ...DEFAULT_NORM_OPTIONS, removeTitles: true })).toBe("احمد");
    expect(normalizeText("د. أحمد")).toBe("د. احمد");
  });
});

describe("PII masking and deterministic tokenization", () => {
  it('computes real BLAKE3 (official test vector for "abc")', () => {
    expect(blake3Hex("abc", 32)).toBe(
      "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85"
    );
  });

  it("detects the four personal fields after normalization", () => {
    const matches = detectPii(normalizeLines(PII_SAMPLE));
    expect(matches.map((match) => match.kind)).toEqual(["name", "phone", "email", "national_id"]);

    const counts = countByKind(matches);
    expect(counts).toEqual({ name: 1, phone: 1, email: 1, national_id: 1, iban: 0 });
  });

  it("masks values without leaking the original digits", () => {
    const normalized = normalizeLines(PII_SAMPLE);
    const masked = maskText(normalized, detectPii(normalized));

    expect(masked).toContain("010••••5678");
    expect(masked).not.toContain("01012345678");
    expect(masked).toContain("a•••@example.com");
    expect(masked).not.toContain("ahmed.mohamed@example.com");
    expect(masked).toContain("2980••••••4567");
    expect(masked).toContain("احمد •••");
  });

  it("masks every kind through one function", () => {
    expect(maskValue("phone", "01012345678")).toBe("010••••5678");
    expect(maskValue("email", "safi@example.com")).toBe("s•••@example.com");
    expect(maskValue("national_id", "29801011234567")).toBe("2980••••••4567");
    const iban = "EG380019000500000000263180002";
    expect(maskValue("iban", iban)).toBe(`EG38${"•".repeat(iban.length - 8)}0002`);
    expect(maskValue("name", "أحمد محمد")).toBe("أحمد •••");
  });

  it("keeps tokens stable per salt and unlinkable across salts", () => {
    const first = tokenizeValue("phone", "01012345678", "tenant-a");
    const again = tokenizeValue("phone", "01012345678", "tenant-a");
    const other = tokenizeValue("phone", "01012345678", "tenant-b");

    expect(first).toBe(again);
    expect(other).not.toBe(first);
    expect(first).toMatch(/^sf_ph_[0-9a-f]{32}$/);
  });
});

describe("deterministic recipe builder", () => {
  it("produces the same digest regardless of object key order", () => {
    const shuffled: RecipeToggles = {
      removeTitles: false,
      cleanWhitespace: true,
      removeDiacritics: true,
      joinCompoundNames: true,
      normalizeDigits: true,
      removeTatweel: true,
      unifyHa: true,
      unifyYa: true,
      unifyAlif: true
    };

    expect(canonicalRecipeJson("وصفة الاختبار", shuffled)).toBe(
      canonicalRecipeJson("وصفة الاختبار", DEFAULT_RECIPE_TOGGLES)
    );
    expect(recipeDigest("وصفة الاختبار", shuffled)).toBe(
      recipeDigest("وصفة الاختبار", DEFAULT_RECIPE_TOGGLES)
    );
  });

  it("changes the digest when a step changes and always emits 64 hex chars", () => {
    const base = recipeDigest("وصفة", DEFAULT_RECIPE_TOGGLES);
    const changed = recipeDigest("وصفة", { ...DEFAULT_RECIPE_TOGGLES, removeTitles: true });

    expect(base).toMatch(/^[0-9a-f]{64}$/);
    expect(changed).not.toBe(base);
    expect(RECIPE_STEPS).toHaveLength(9);
    expect(enabledStepCount(DEFAULT_RECIPE_TOGGLES)).toBe(8);
  });

  it("validates the F1 watch folder against traversal and relative paths", () => {
    expect(validateWatchFolder("C:\\Safi\\Output")).toEqual({
      ok: true,
      normalized: "C:/Safi/Output"
    });
    expect(validateWatchFolder("/home/safi/output/")).toEqual({
      ok: true,
      normalized: "/home/safi/output"
    });
    expect(validateWatchFolder("relative/path")).toMatchObject({
      ok: false,
      code: "SF-EXP-001"
    });
    expect(validateWatchFolder("C:\\Safi\\..\\Windows")).toMatchObject({ ok: false });
    expect(validateWatchFolder("   ")).toMatchObject({ ok: false });
    expect(validateWatchFolder("C:\\Safi\\\u0000bad")).toMatchObject({ ok: false });
  });
});

describe("recipe quality report", () => {
  it("escapes HTML and embeds the integrity digest", () => {
    const digest = recipeDigest("وصفة <خطرة>", DEFAULT_RECIPE_TOGGLES);
    const html = buildRecipeReportHtml({
      recipe: {
        schema: "safi.recipe",
        version: "1.0.0",
        name: "<script>alert(1)</script>",
        steps: []
      },
      digest,
      generatedAt: "2026-11-08T00:00:00.000Z"
    });

    expect(html).not.toContain("<script>alert(1)</script>");
    expect(html).toContain("&lt;script&gt;alert(1)&lt;/script&gt;");
    expect(html).toContain(digest);
    expect(html).toContain("BLAKE3");
    expect(escapeHtml("a&b<c>\"d\"'e'")).toBe("a&amp;b&lt;c&gt;&quot;d&quot;&#39;e&#39;");
  });
});

describe("client security audit", () => {
  it("reports every check honestly and counts the passes", () => {
    const allPassing = runClientSecurityAudit({
      secureContext: true,
      subtleCrypto: true,
      opfs: true,
      topFrame: true
    });
    expect(allPassing).toHaveLength(4);
    expect(summarizeAudit(allPassing)).toEqual({ passed: 4, total: 4 });

    const mixed = runClientSecurityAudit({
      secureContext: false,
      subtleCrypto: true,
      opfs: true,
      topFrame: false
    });
    expect(summarizeAudit(mixed)).toEqual({ passed: 2, total: 4 });
    expect(mixed.find((check) => check.id === "secure-context")?.passed).toBe(false);
  });

  it("never creates a file outside a browser", () => {
    expect(downloadTextFile("recipe.json", "{}", "application/json")).toBe(false);
  });
});
