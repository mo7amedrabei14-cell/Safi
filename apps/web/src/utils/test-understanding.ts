/**
 * `apps/web/src/utils/test-understanding.ts` — «اختبر فهمي» engine generating natural sentences from table rows (Section 16.4).
 */

export interface UnderstandingSentence {
  rowIndex: number;
  sentenceAr: string;
}

export function generateUnderstandingSentences(
  headers: string[],
  rows: string[][],
  maxRows: number = 10
): UnderstandingSentence[] {
  if (!rows || rows.length === 0 || !headers || headers.length === 0) {
    return [];
  }

  const indices: number[] = [];
  const total = rows.length;
  for (let i = 0; i < Math.min(total, maxRows); i++) {
    const idx = Math.floor((i * total) / Math.min(total, maxRows));
    indices.push(idx);
  }

  return indices.map((rowIndex) => {
    const row = rows[rowIndex] || [];
    const parts = headers.map((h, colIdx) => {
      const val = row[colIdx] ?? "";
      return `${h}: (${val})`;
    });
    return {
      rowIndex,
      sentenceAr: `في الصف رقم ${rowIndex + 1}: ${parts.join("، ")}.`
    };
  });
}
