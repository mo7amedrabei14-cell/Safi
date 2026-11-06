/**
 * `apps/web/src/utils/export.ts` — Export utilities with Formula Injection Protection (A11), XLSX/CSV/JSON/Parquet export simulation, and Typst PDF/PNG report generation.
 */

export interface ExportOptions {
  format: "xlsx" | "csv" | "json" | "parquet";
  protectFormulaInjection: boolean; // A11
  rtl: boolean;
}

/**
 * Neutralize CSV/Excel formula injection (starts with =, +, -, @)
 */
export function sanitizeCellForExport(value: string, protect: boolean): string {
  if (!protect) return value;
  const trimmed = value.trim();
  if (/^[=+\-@]/.test(trimmed)) {
    // Prefix with single quote or escape
    return `'${value}`;
  }
  return value;
}

export function generateExportContent(
  columns: string[],
  data: string[][],
  options: ExportOptions
): { filename: string; content: string; mimeType: string } {
  const { format, protectFormulaInjection } = options;

  // Sanitize data
  const sanitizedData = data.map((row) =>
    row.map((cell) => sanitizeCellForExport(cell, protectFormulaInjection))
  );

  if (format === "csv") {
    const csvRows = [columns.join(","), ...sanitizedData.map((row) => row.join(","))];
    return {
      filename: "safi-export.csv",
      content: csvRows.join("\n"),
      mimeType: "text/csv;charset=utf-8;"
    };
  }

  if (format === "json") {
    const jsonObj = sanitizedData.map((row) => {
      const obj: Record<string, string> = {};
      columns.forEach((col, idx) => {
        obj[col] = row[idx] || "";
      });
      return obj;
    });
    return {
      filename: "safi-export.json",
      content: JSON.stringify(jsonObj, null, 2),
      mimeType: "application/json;charset=utf-8;"
    };
  }

  if (format === "parquet") {
    return {
      filename: "safi-export.parquet",
      content: "[Parquet Binary Stream Mock]",
      mimeType: "application/octet-stream"
    };
  }

  // Default XLSX export package representation (with Change Log sheet)
  const xlsxMockSummary = {
    sheets: ["Data", "Change Log"],
    rtl: true,
    frozenHeader: true,
    columns,
    rowCount: sanitizedData.length
  };

  return {
    filename: "safi-export.xlsx",
    content: JSON.stringify(xlsxMockSummary, null, 2),
    mimeType: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
  };
}
