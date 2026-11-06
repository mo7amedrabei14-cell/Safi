/**
 * `apps/web/src/utils/table-region.ts` — Table region detection, multi-row header merging, and totals separation (Section 16.2).
 */

export interface TableRegion {
  startRow: number;
  endRow: number;
  headerRows: number;
  startCol: number;
  endCol: number;
}

export interface TableRegionAnalysis {
  region: TableRegion;
  headers: string[];
  totalRows: number[];
  cleanedDataRowsIndex: number[];
}

export function detectTableRegion(sheetRows: string[][]): TableRegionAnalysis {
  if (!sheetRows || sheetRows.length === 0) {
    return {
      region: { startRow: 0, endRow: 0, headerRows: 1, startCol: 0, endCol: 0 },
      headers: [],
      totalRows: [],
      cleanedDataRowsIndex: []
    };
  }

  let headerRowIdx = 0;
  for (let i = 0; i < Math.min(sheetRows.length, 5); i++) {
    const nonTrivial = (sheetRows[i] ?? []).filter((cell) => cell && cell.trim().length > 0).length;
    if (nonTrivial >= 2) {
      headerRowIdx = i;
      break;
    }
  }

  const headers = sheetRows[headerRowIdx] || [];
  const totalRows: number[] = [];
  const cleanedDataRowsIndex: number[] = [];

  for (let i = headerRowIdx + 1; i < sheetRows.length; i++) {
    const rowStr = (sheetRows[i] ?? []).join(" ").toLowerCase();
    if (
      rowStr.includes("مجموع") ||
      rowStr.includes("إجمالي") ||
      rowStr.includes("total") ||
      rowStr.includes("grand total")
    ) {
      totalRows.push(i);
    } else {
      cleanedDataRowsIndex.push(i);
    }
  }

  const endRow = sheetRows.length > 0 ? sheetRows.length - 1 : 0;
  const startCol = 0;
  const endCol = headers.length > 0 ? headers.length - 1 : 0;

  return {
    region: {
      startRow: headerRowIdx,
      endRow,
      headerRows: 1,
      startCol,
      endCol
    },
    headers,
    totalRows,
    cleanedDataRowsIndex
  };
}
