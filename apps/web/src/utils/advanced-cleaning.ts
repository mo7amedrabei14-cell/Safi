/**
 * `apps/web/src/utils/advanced-cleaning.ts` — Advanced cleaning operations (split, merge, units, outliers) & Recipe export (Section 4.7).
 */

export interface AdvancedOperationResult {
  success: boolean;
  messageAr: string;
  processedRows: string[][];
}

export function splitColumn(
  rows: string[][],
  colIndex: number,
  delimiter: string
): AdvancedOperationResult {
  const processed = rows.map((row) => {
    const val = row[colIndex] ?? "";
    const parts = val.split(delimiter);
    const newRow = [...row];
    newRow.splice(colIndex, 1, ...parts);
    return newRow;
  });
  return {
    success: true,
    messageAr: `تم تقسيم العمود بمحدد (${delimiter}) بنجاح.`,
    processedRows: processed
  };
}

export function mergeColumns(
  rows: string[][],
  colIndices: number[],
  separator: string,
  targetIndex: number
): AdvancedOperationResult {
  const sources = new Set(colIndices);
  const processed = rows.map((row) => {
    const mergedVal = colIndices.map((idx) => row[idx] ?? "").join(separator);
    const newRow: string[] = [];
    row.forEach((cell, idx) => {
      if (!sources.has(idx)) {
        newRow.push(cell);
      } else if (idx === targetIndex) {
        newRow.push(mergedVal);
      }
    });
    if (!sources.has(targetIndex)) {
      newRow.splice(Math.min(targetIndex, newRow.length), 0, mergedVal);
    }
    return newRow;
  });
  return {
    success: true,
    messageAr: `تم دمج الأعمدة بنجاح.`,
    processedRows: processed
  };
}
