/**
 * `apps/web/src/lib/report.ts`
 *
 * Deterministic HTML quality report for an exported recipe. The report is built
 * from the canonical recipe JSON only: no cell values, file names or column names
 * ever enter it (`AGENTS.md` rule 4).
 */
import { SAFI_APP_VERSION, SAFI_REPORT_INTEGRITY_NOTE } from "./app-meta";
import { RECIPE_STEPS } from "./recipe";
import type { CanonicalRecipe } from "./recipe";

export function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

export interface RecipeReportInput {
  readonly recipe: CanonicalRecipe;
  readonly digest: string;
  readonly generatedAt: string;
}

function renderStepRows(recipe: CanonicalRecipe): string {
  return RECIPE_STEPS.map((definition) => {
    const step = recipe.steps.find((candidate) => candidate.id === definition.id);
    const enabled = step?.enabled === true;
    return `<tr><td>${escapeHtml(definition.labelAr)}</td><td>${escapeHtml(
      definition.detailAr
    )}</td><td class="${enabled ? "on" : "off"}">${enabled ? "مفعّلة" : "موقوفة"}</td></tr>`;
  }).join("");
}

export function buildRecipeReportHtml(input: RecipeReportInput): string {
  const { recipe, digest, generatedAt } = input;
  const enabled = recipe.steps.filter((step) => step.enabled).length;

  return `<!doctype html>
<html lang="ar" dir="rtl">
<head>
<meta charset="utf-8" />
<title>${escapeHtml(`تقرير جودة الوصفة — ${recipe.name}`)}</title>
<style>
  body { font-family: "IBM Plex Sans Arabic", "Segoe UI", Tahoma, sans-serif; background: #f5f7fa; color: #0f172a; margin: 0; padding: 32px; }
  main { max-width: 880px; margin: 0 auto; background: #fff; border: 1px solid #e2e8f0; border-radius: 16px; padding: 32px; }
  h1 { font-size: 24px; margin: 0 0 4px; }
  .muted { color: #475569; font-size: 13px; }
  table { width: 100%; border-collapse: collapse; margin-top: 20px; font-size: 13px; }
  th, td { text-align: start; padding: 8px 10px; border-bottom: 1px solid #e2e8f0; }
  th { background: #f8fafc; font-weight: 600; }
  .on { color: #15803d; font-weight: 600; }
  .off { color: #64748b; }
  pre { direction: ltr; text-align: left; background: #0b1016; color: #e8eef4; border-radius: 12px; padding: 14px; font-size: 12px; overflow-x: auto; }
  .digest { font-family: "JetBrains Mono", Consolas, monospace; direction: ltr; text-align: left; word-break: break-all; font-size: 12px; }
  footer { margin-top: 24px; font-size: 12px; color: #475569; line-height: 1.7; }
</style>
</head>
<body>
<main>
  <h1>${escapeHtml(`تقرير جودة الوصفة — ${recipe.name}`)}</h1>
  <p class="muted">${escapeHtml(
    `صافي البيانات ${SAFI_APP_VERSION} • مُولَّد في ${generatedAt} • خطوات مفعّلة: ${enabled} من ${RECIPE_STEPS.length}`
  )}</p>

  <table>
    <thead><tr><th>الخطوة</th><th>القاعدة</th><th>الحالة</th></tr></thead>
    <tbody>${renderStepRows(recipe)}</tbody>
  </table>

  <h2 style="font-size:16px;margin-top:28px;">الوصفة القانونية (Canonical JSON)</h2>
  <pre>${escapeHtml(JSON.stringify(recipe, null, 2))}</pre>

  <h2 style="font-size:16px;margin-top:24px;">بصمة التحقق BLAKE3</h2>
  <p class="digest">${escapeHtml(digest)}</p>

  <footer>${escapeHtml(SAFI_REPORT_INTEGRITY_NOTE)}</footer>
</main>
</body>
</html>`;
}
