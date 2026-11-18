import { describe, expect, it } from "vitest";
import {
  classifyCellType,
  coordsToA1,
  evaluateSimpleFormula,
  getFormatCompatibilityMatrix,
  ingestWorkbookStructure,
  parseA1Range,
  segmentWorksheetTables
} from "./workbook-ingestion";

describe("Universal Workbook Ingestion Engine (Phase 1)", () => {
  it("exposes the complete 6-format compatibility matrix (XLSX, XLS, XLSM, XLSB, CSV, ODS)", () => {
    const matrix = getFormatCompatibilityMatrix();
    expect(matrix.map((m) => m.format)).toEqual(["Xlsx", "Xlsm", "Xls", "Xlsb", "Csv", "Ods"]);
    expect(matrix.find((m) => m.format === "Xlsm")?.supportLevel).toBe(
      "SupportedWithMacroNeutralization"
    );
  });

  it("converts A1 coordinates and ranges accurately", () => {
    expect(coordsToA1(0, 0)).toBe("A1");
    expect(coordsToA1(9, 27)).toBe("AB10");
    expect(parseA1Range("$B$2:$D$15")).toEqual({
      startRow: 1,
      startCol: 1,
      endRow: 14,
      endCol: 3
    });
  });

  it("segments multiple tables, banner titles, multi-row headers, repeated headers, subtotals, and footnotes on a single sheet", () => {
    const grid = [
      ["تقرير المبيعات الإقليمي 2026", "", "", "", "", ""],
      ["", "", "", "", "", ""],
      ["المنطقة", "2026", "2026", "", "الكود", "الفرع"],
      ["المدينة", "الربع الأول", "الربع الثاني", "", "الرمز", "الاسم"],
      ["القاهرة", "1500", "1800", "", "B01", "فرع مدينة نصر"],
      ["الإسكندرية", "900", "1100", "", "B02", "فرع سموحة"],
      ["إجمالي فرعي", "2400", "2900", "", "", ""],
      ["المنطقة — المدينة", "2026 — الربع الأول", "2026 — الربع الثاني", "", "", ""],
      ["الجيزة", "600", "700", "", "", ""],
      ["الإجمالي العام", "3000", "3600", "", "", ""],
      ["* ملاحظة: الأرقام بآلاف الجنيهات", "", "", "", "", ""]
    ];

    const tables = segmentWorksheetTables("المبيعات", grid);
    expect(tables).toHaveLength(2);

    const mainTable = tables[0];
    expect(mainTable?.title).toBe("تقرير المبيعات الإقليمي 2026");
    expect(mainTable?.headerRows).toEqual([2, 3]);
    expect(mainTable?.headers).toEqual([
      "المنطقة — المدينة",
      "2026 — الربع الأول",
      "2026 — الربع الثاني"
    ]);
    // Only the 3 real city rows (Cairo, Alexandria, Giza) are in dataRows!
    expect(mainTable?.dataRows).toHaveLength(3);
    expect(mainTable?.separatedRows.map((s) => s.kind)).toEqual([
      "Title",
      "Subtotal",
      "RepeatedHeader",
      "GrandTotal",
      "Footnote"
    ]);

    const lookupTable = tables[1];
    expect(lookupTable?.headers).toEqual(["الكود — الرمز", "الفرع — الاسم"]);
    expect(lookupTable?.dataRows).toHaveLength(2);
  });

  it("preserves cell metadata, leading zeros, formulas, cached vs recalculated values, and neutralizes XLSM macros", () => {
    const report = ingestWorkbookStructure(
      [
        {
          name: "Transactions",
          visibility: "Visible",
          hasFrozenPanes: true,
          autoFilterRange: "A1:C4",
          mergedRanges: ["A1:C1"],
          grid: [
            ["Q1 Ledger", "", ""],
            ["AccountCode", "Amount", "Tax"],
            ["00192", "1000", "140"],
            ["00193", "2000", "280"],
            ["Total", "3000", "420"]
          ],
          formulas: {
            B5: "=SUM(B3:B4)",
            C5: "=SUM(C3:C4)"
          },
          numberFormats: {
            B3: "$#,##0.00",
            B4: "$#,##0.00"
          },
          comments: {
            A3: "Verified corporate account"
          },
          hyperlinks: {
            A4: "https://example.org/accounts/193"
          }
        },
        {
          name: "Calc_Helper",
          visibility: "VeryHidden",
          grid: [
            ["Factor", "Result"],
            ["1.14", "1140"]
          ],
          formulas: {
            B2: "=A2*1000"
          }
        }
      ],
      {
        filename: "corporate_2026.xlsm",
        dateSystem: "Macintosh1904",
        hasVbaMacros: true
      }
    );

    expect(report.detectedFormat).toBe("Xlsm");
    expect(report.macrosNeutralized).toBe(true);
    expect(report.dateSystem).toBe("Macintosh1904");
    expect(report.totalSheets).toBe(2);
    expect(report.visibleSheets).toBe(1);
    expect(report.hiddenSheets).toBe(1);
    expect(report.totalFormulasDetected).toBe(3);
    expect(report.totalFormulasVerified).toBe(3);
    expect(report.worksheets[0]?.classification).toBe("MainDataset");
    expect(report.worksheets[1]?.classification).toBe("SupportingCalculations");

    const a3Meta = report.worksheets[0]?.cellMetadata.find((c) => c.address === "A3");
    expect(a3Meta?.dataType).toBe("ZeroPreservedText");
    expect(a3Meta?.comment).toBe("Verified corporate account");

    const b3Meta = report.worksheets[0]?.cellMetadata.find((c) => c.address === "B3");
    expect(b3Meta?.dataType).toBe("Currency");

    const b5Meta = report.worksheets[0]?.cellMetadata.find((c) => c.address === "B5");
    expect(b5Meta?.formulaStatus).toBe("VerifiedCachedAndRecalculated");
    expect(b5Meta?.recalculatedValue).toBe("3000");

    expect(classifyCellType("+201012345678")).toBe("ZeroPreservedText");
    expect(
      evaluateSimpleFormula(
        "=A1/B1",
        new Map([
          ["0,0", 50],
          ["0,1", 0]
        ])
      )
    ).toBe("#DIV/0!");
  });
});
