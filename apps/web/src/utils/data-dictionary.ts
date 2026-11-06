/**
 * `apps/web/src/utils/data-dictionary.ts` — Automated Data Dictionary generator (Section 16.4).
 */

export interface ColumnDictionaryEntry {
  columnName: string;
  inferredType: string;
  completenessRate: number;
  meaning: string;
}

export function generateDataDictionaryMarkdown(
  tableName: string,
  entries: ColumnDictionaryEntry[]
): string {
  let md = `# قاموس البيانات التلقائي — ${tableName}\n\n`;
  md += `| اسم العمود | النوع المستنتج | نسبة الاكتمال | المعنى والوصف |\n`;
  md += `| :--- | :--- | :--- | :--- |\n`;

  for (const entry of entries) {
    const pct = Math.round(entry.completenessRate * 100);
    md += `| ${entry.columnName} | ${entry.inferredType} | ${pct}% | ${entry.meaning} |\n`;
  }

  return md;
}
