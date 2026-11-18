/**
 * Permanent 35-Workbook Regression Suite, Known-Answer Analytical Verification,
 * and Cross-Studio Numerical Consistency Suite (`Phase 13`).
 */

import { describe, expect, it } from "vitest";
import {
  type RawSheetInput,
  type WorkbookIngestionOptions,
  ingestWorkbookStructure
} from "./workbook-ingestion";
import { buildSemanticWorkbookModel } from "./semantic-intelligence";
import { answerAnalyticalQuestion, buildVerifiedAnalysisReport } from "./verified-analysis-engine";
import {
  generateAutoDashboard,
  generateDocumentReport,
  generatePresentationDeck
} from "./quad-studio-engine";
import { exportStudioDeliverable } from "./export";

interface RegressionFixtureSpec {
  readonly fixtureNumber: number;
  readonly fixtureId: string;
  readonly description: string;
  readonly sheets: readonly RawSheetInput[];
  readonly options?: WorkbookIngestionOptions | undefined;
  readonly expectedErrorCode?: string | undefined;
  readonly assertIngestion: (report: ReturnType<typeof ingestWorkbookStructure>) => void;
}

const PERMANENT_35_FIXTURES: readonly RegressionFixtureSpec[] = [
  {
    fixtureNumber: 1,
    fixtureId: "01_simple_clean_single_sheet",
    description: "Simple clean single-sheet file",
    sheets: [
      {
        name: "CleanData",
        grid: [
          ["Region", "Revenue"],
          ["Cairo", "1000"],
          ["Alexandria", "2000"]
        ]
      }
    ],
    assertIngestion: (r) => {
      expect(r.totalSheets).toBe(1);
      expect(r.totalTablesExtracted).toBe(1);
      expect(r.totalAnalyticalRows).toBe(2);
      expect(r.worksheets[0]?.tables[0]?.headers).toEqual(["Region", "Revenue"]);
    }
  },
  {
    fixtureNumber: 2,
    fixtureId: "02_multi_sheet_workbook",
    description: "Multi-sheet workbook",
    sheets: [
      {
        name: "Sales_2026",
        grid: [
          ["Branch", "Amount"],
          ["B01", "500"]
        ]
      },
      {
        name: "Branches_Ref",
        grid: [
          ["Branch", "City"],
          ["B01", "Cairo"]
        ]
      },
      {
        name: "Summary_KPI",
        grid: [
          ["Metric", "Value"],
          ["Total", "500"]
        ]
      }
    ],
    assertIngestion: (r) => {
      expect(r.totalSheets).toBe(3);
      expect(r.worksheets.map((w) => w.classification)).toEqual([
        "LookupTable",
        "ReferenceData",
        "Summary"
      ]);
    }
  },
  {
    fixtureNumber: 3,
    fixtureId: "03_hidden_and_very_hidden_sheets",
    description: "Workbook with hidden and veryHidden sheets",
    sheets: [
      {
        name: "Public_View",
        visibility: "Visible",
        grid: [
          ["Product", "Sales"],
          ["A", "100"]
        ]
      },
      {
        name: "Hidden_Staging",
        visibility: "Hidden",
        grid: [
          ["Step", "Val"],
          ["1", "100"]
        ],
        formulas: { B2: "=10*10" }
      },
      {
        name: "VeryHidden_Rates",
        visibility: "VeryHidden",
        grid: [
          ["Rate", "Val"],
          ["Tax", "14"]
        ],
        formulas: { B2: "=7*2" }
      }
    ],
    assertIngestion: (r) => {
      expect(r.totalSheets).toBe(3);
      expect(r.visibleSheets).toBe(1);
      expect(r.hiddenSheets).toBe(2);
      expect(r.worksheets[1]?.classification).toBe("SupportingCalculations");
      expect(r.worksheets[2]?.visibility).toBe("VeryHidden");
    }
  },
  {
    fixtureNumber: 4,
    fixtureId: "04_merged_headers",
    description: "Workbook with merged headers",
    sheets: [
      {
        name: "MergedHeaderSheet",
        mergedRanges: ["B1:C1"],
        grid: [
          ["Region", "Q1 Performance", ""],
          ["City", "Revenue", "Cost"],
          ["Cairo", "1200", "800"]
        ]
      }
    ],
    assertIngestion: (r) => {
      expect(r.worksheets[0]?.tables[0]?.headers).toEqual([
        "Region — City",
        "Q1 Performance — Revenue",
        "Q1 Performance — Cost"
      ]);
      expect(r.worksheets[0]?.tables[0]?.dataRows).toHaveLength(1);
    }
  },
  {
    fixtureNumber: 5,
    fixtureId: "05_multi_row_headers",
    description: "Workbook with multi-row hierarchical headers",
    sheets: [
      {
        name: "Hierarchical",
        grid: [
          ["Department", "FY2025", "FY2025", "FY2026", "FY2026"],
          ["Unit", "Budget", "Actual", "Budget", "Actual"],
          ["Engineering", "100", "95", "120", "118"]
        ]
      }
    ],
    assertIngestion: (r) => {
      expect(r.worksheets[0]?.tables[0]?.headerRows).toEqual([0, 1]);
      expect(r.worksheets[0]?.tables[0]?.headers).toEqual([
        "Department — Unit",
        "FY2025 — Budget",
        "FY2025 — Actual",
        "FY2026 — Budget",
        "FY2026 — Actual"
      ]);
    }
  },
  {
    fixtureNumber: 6,
    fixtureId: "06_multiple_tables_single_sheet",
    description: "Sheet with multiple tables (vertical and side-by-side)",
    sheets: [
      {
        name: "MultiTable",
        grid: [
          ["North_Branch", "Rev", "", "South_Branch", "Rev"],
          ["Cairo", "500", "", "Aswan", "300"],
          ["", "", "", "", ""],
          ["East_Branch", "Rev", "", "", ""],
          ["Suez", "250", "", "", ""]
        ]
      }
    ],
    assertIngestion: (r) => {
      expect(r.totalTablesExtracted).toBe(3);
      expect(r.totalAnalyticalRows).toBe(3);
    }
  },
  {
    fixtureNumber: 7,
    fixtureId: "07_titles_notes_blank_rows",
    description: "Sheet with title rows, notes, and blank rows before the table",
    sheets: [
      {
        name: "Banners",
        grid: [
          ["Ministry of Finance — Annual Summary", "", ""],
          ["Prepared for Q4 Audit", "", ""],
          ["", "", ""],
          ["Fund", "Allocated", "Spent"],
          ["Infrastructure", "5000", "4800"],
          ["* Note: Figures audited", "", ""]
        ]
      }
    ],
    assertIngestion: (r) => {
      const tbl = r.worksheets[0]?.tables[0]!;
      expect(tbl.title).toBe("Ministry of Finance — Annual Summary");
      expect(tbl.dataRows).toHaveLength(1);
      expect(tbl.separatedRows.map((s) => s.kind)).toEqual(["Title", "Subtitle", "Footnote"]);
    }
  },
  {
    fixtureNumber: 8,
    fixtureId: "08_subtotals_and_grand_totals",
    description: "Sheet with subtotals and grand totals isolated from analytical rows",
    sheets: [
      {
        name: "TotalsSheet",
        grid: [
          ["Category", "Amount"],
          ["Hardware", "1000"],
          ["Subtotal Hardware", "1000"],
          ["Software", "2000"],
          ["Subtotal Software", "2000"],
          ["Grand Total", "3000"]
        ]
      }
    ],
    assertIngestion: (r) => {
      const tbl = r.worksheets[0]?.tables[0]!;
      expect(tbl.dataRows).toHaveLength(2);
      expect(tbl.separatedRows.filter((s) => s.kind === "Subtotal")).toHaveLength(2);
      expect(tbl.separatedRows.filter((s) => s.kind === "GrandTotal")).toHaveLength(1);
    }
  },
  {
    fixtureNumber: 9,
    fixtureId: "09_mixed_arabic_english_bilingual",
    description: "Mixed Arabic/English headers and data",
    sheets: [
      {
        name: "بيانات_مختلطة",
        grid: [
          ["الفرع / Branch", "الإيراد / Revenue EGP", "الحالة / Status"],
          ["القاهرة - Cairo", "15000", "مكتمل / Active"],
          ["الإسكندرية - Alex", "9500", "قيد المراجعة / Pending"],
          ["الإجمالي العام", "24500", ""]
        ]
      }
    ],
    assertIngestion: (r) => {
      expect(r.totalAnalyticalRows).toBe(2);
      expect(r.worksheets[0]?.tables[0]?.separatedRows[0]?.kind).toBe("GrandTotal");
    }
  },
  {
    fixtureNumber: 10,
    fixtureId: "10_rtl_worksheet_layout",
    description: "RTL worksheet layout with Arabic financial records",
    sheets: [
      {
        name: "ميزان_المراجعة",
        grid: [
          ["كود الحساب", "اسم الحساب", "المبلغ ج.م"],
          ["00101", "النقدية بالصندوق", "45000"],
          ["00102", "البنك التجاري", "125000"]
        ]
      }
    ],
    assertIngestion: (r) => {
      expect(r.worksheets[0]?.cellMetadata[3]?.dataType).toBe("ZeroPreservedText");
      expect(r.totalAnalyticalRows).toBe(2);
    }
  },
  {
    fixtureNumber: 11,
    fixtureId: "11_mixed_date_formats_and_1904",
    description: "Mixed date formats and 1904 date system",
    sheets: [
      {
        name: "Dates",
        grid: [
          ["Order_Date", "Timestamp", "Amount"],
          ["2026-01-15", "2026-01-15 14:30:00", "400"],
          ["15/02/2026", "2026-02-15 09:15:00", "600"]
        ]
      }
    ],
    options: { dateSystem: "Macintosh1904" },
    assertIngestion: (r) => {
      expect(r.dateSystem).toBe("Macintosh1904");
      const a2 = r.worksheets[0]?.cellMetadata.find((c) => c.address === "A2");
      const b2 = r.worksheets[0]?.cellMetadata.find((c) => c.address === "B2");
      expect(a2?.dataType).toBe("Date");
      expect(b2?.dataType).toBe("DateTime");
    }
  },
  {
    fixtureNumber: 12,
    fixtureId: "12_text_numbers_and_currency_strings",
    description: "Text numbers and currency strings",
    sheets: [
      {
        name: "Currencies",
        grid: [
          ["Invoice", "Amount_USD", "Local_EGP", "Share_Pct"],
          ["INV-1", "$1,250.50", "62,525 EGP", "15.5%"],
          ["INV-2", "$749.50", "37,475 ج.م", "84.5%"]
        ]
      }
    ],
    assertIngestion: (r) => {
      const b2 = r.worksheets[0]?.cellMetadata.find((c) => c.address === "B2");
      const c2 = r.worksheets[0]?.cellMetadata.find((c) => c.address === "C2");
      const d2 = r.worksheets[0]?.cellMetadata.find((c) => c.address === "D2");
      expect(b2?.dataType).toBe("Currency");
      expect(c2?.dataType).toBe("Currency");
      expect(d2?.dataType).toBe("Percentage");
    }
  },
  {
    fixtureNumber: 13,
    fixtureId: "13_leading_zero_identifiers",
    description: "Leading zero identifiers (00123, phone numbers, national IDs)",
    sheets: [
      {
        name: "Directory",
        grid: [
          ["Emp_Code", "Phone", "Zip_Code", "Salary"],
          ["00042", "+201099887766", "01152", "18000"],
          ["00043", "01012345678", "02241", "21000"]
        ]
      }
    ],
    assertIngestion: (r) => {
      const a2 = r.worksheets[0]?.cellMetadata.find((c) => c.address === "A2");
      const b2 = r.worksheets[0]?.cellMetadata.find((c) => c.address === "B2");
      const c2 = r.worksheets[0]?.cellMetadata.find((c) => c.address === "C2");
      expect(a2?.dataType).toBe("ZeroPreservedText");
      expect(b2?.dataType).toBe("ZeroPreservedText");
      expect(c2?.dataType).toBe("ZeroPreservedText");
      expect(a2?.rawValue).toBe("00042");
    }
  },
  {
    fixtureNumber: 14,
    fixtureId: "14_formula_cells_with_cached_values",
    description: "Formula cells with cached values verified against local evaluator",
    sheets: [
      {
        name: "CachedFormulas",
        grid: [
          ["Q1", "Q2", "HalfYear"],
          ["400", "600", "1000"]
        ],
        formulas: {
          C2: "=A2+B2"
        }
      }
    ],
    assertIngestion: (r) => {
      const c2 = r.worksheets[0]?.cellMetadata.find((c) => c.address === "C2");
      expect(c2?.formulaStatus).toBe("VerifiedCachedAndRecalculated");
      expect(c2?.cachedValue).toBe("1000");
      expect(c2?.recalculatedValue).toBe("1000");
    }
  },
  {
    fixtureNumber: 15,
    fixtureId: "15_formula_cells_without_cached_values",
    description: "Formula cells without cached values recalculated locally",
    sheets: [
      {
        name: "UncachedFormulas",
        grid: [
          ["Price", "Qty", "LineTotal"],
          ["25", "8", ""]
        ],
        formulas: {
          C2: "=A2*B2"
        }
      }
    ],
    assertIngestion: (r) => {
      const c2 = r.worksheets[0]?.cellMetadata.find((c) => c.address === "C2");
      expect(c2?.formulaStatus).toBe("RecalculatedLocally");
      expect(c2?.rawValue).toBe("200");
      expect(r.worksheets[0]?.tables[0]?.dataRows[0]?.[2]).toBe("200");
    }
  },
  {
    fixtureNumber: 16,
    fixtureId: "16_cross_sheet_formulas",
    description: "Cross-sheet and external workbook formulas",
    sheets: [
      {
        name: "Consolidation",
        grid: [
          ["Source", "Amount"],
          ["BranchA", "5000"],
          ["External", "1200"]
        ],
        formulas: {
          B2: "=Sheet2!B10",
          B3: "='[ExternalLedger.xlsx]Sheet1'!$C$4"
        }
      }
    ],
    assertIngestion: (r) => {
      const b2 = r.worksheets[0]?.cellMetadata.find((c) => c.address === "B2");
      const b3 = r.worksheets[0]?.cellMetadata.find((c) => c.address === "B3");
      expect(b2?.formulaStatus).toBe("CachedResultOnly");
      expect(b3?.formulaStatus).toBe("ExternalWorkbookReference");
    }
  },
  {
    fixtureNumber: 17,
    fixtureId: "17_broken_formula_errors",
    description: "Broken formulas (#REF!, #DIV/0!, #VALUE!, #NAME?, #N/A, #NUM!)",
    sheets: [
      {
        name: "FormulaErrors",
        grid: [
          ["Case", "Result"],
          ["DivZero", "#DIV/0!"],
          ["MissingRef", "#REF!"],
          ["BadName", "#NAME?"],
          ["NotAvail", "#N/A"]
        ],
        formulas: {
          B2: "=100/0",
          B3: "=#REF!+1",
          B4: "=UNKNOWN_FN()",
          B5: "=VLOOKUP(99,A1:B2,2,FALSE)"
        }
      }
    ],
    assertIngestion: (r) => {
      const statuses = r.worksheets[0]?.cellMetadata
        .filter((c) => c.col === 1 && c.row >= 1)
        .map((c) => c.formulaStatus);
      expect(statuses).toEqual(["FormulaError", "FormulaError", "FormulaError", "FormulaError"]);
    }
  },
  {
    fixtureNumber: 18,
    fixtureId: "18_named_ranges",
    description: "Workbook with named ranges and built-in print areas",
    sheets: [
      {
        name: "NamedSheet",
        printArea: "NamedSheet!$A$1:$B$10",
        grid: [
          ["TaxRate", "Value"],
          ["VAT", "0.14"]
        ]
      }
    ],
    options: {
      definedNames: [
        { name: "GlobalVAT", refersTo: "NamedSheet!$B$2", isBuiltIn: false },
        { name: "_xlnm.Print_Area", refersTo: "NamedSheet!$A$1:$B$10", isBuiltIn: true }
      ]
    },
    assertIngestion: (r) => {
      expect(r.definedNames).toHaveLength(2);
      expect(r.worksheets[0]?.printArea).toBe("NamedSheet!$A$1:$B$10");
    }
  },
  {
    fixtureNumber: 19,
    fixtureId: "19_excel_tables_metadata",
    description: "Workbook with formal Excel Table objects",
    sheets: [
      {
        name: "FormalTable",
        grid: [
          ["SKU", "Stock"],
          ["SKU-1", "45"]
        ]
      }
    ],
    options: {
      embeddedObjects: [
        {
          kind: "ExcelTable",
          location: "xl/tables/table1.xml",
          handling: "Extracted",
          summary: "Excel Table tblInventory (A1:B2)"
        }
      ]
    },
    assertIngestion: (r) => {
      expect(r.embeddedObjects.some((o) => o.kind === "ExcelTable")).toBe(true);
    }
  },
  {
    fixtureNumber: 20,
    fixtureId: "20_filtered_ranges_autofilter",
    description: "Workbook with active autoFilter and frozen panes",
    sheets: [
      {
        name: "FilteredSheet",
        hasFrozenPanes: true,
        autoFilterRange: "A1:C20",
        grid: [
          ["Dept", "Headcount", "Cost"],
          ["Sales", "12", "240000"]
        ]
      }
    ],
    assertIngestion: (r) => {
      expect(r.worksheets[0]?.hasFrozenPanes).toBe(true);
      expect(r.worksheets[0]?.autoFilterRange).toBe("A1:C20");
    }
  },
  {
    fixtureNumber: 21,
    fixtureId: "21_pivot_table_sheets",
    description: "Workbook with PivotTable output sheet",
    sheets: [
      {
        name: "Sales_Pivot",
        hasPivotTable: true,
        grid: [
          ["Row Labels", "Sum of Revenue"],
          ["Cairo", "12000"],
          ["Grand Total", "12000"]
        ]
      }
    ],
    assertIngestion: (r) => {
      expect(r.worksheets[0]?.classification).toBe("PivotOutput");
      expect(r.worksheets[0]?.tables[0]?.dataRows).toHaveLength(1);
    }
  },
  {
    fixtureNumber: 22,
    fixtureId: "22_embedded_charts_inventory",
    description: "Workbook with embedded OOXML charts inventoried transparently",
    sheets: [
      {
        name: "ChartSheet",
        grid: [
          ["Month", "Revenue"],
          ["Jan", "100"]
        ]
      }
    ],
    options: {
      embeddedObjects: [
        {
          kind: "Chart",
          location: "xl/charts/chart1.xml",
          handling: "Summarized",
          summary: "Embedded bar chart on ChartSheet"
        }
      ]
    },
    assertIngestion: (r) => {
      expect(r.embeddedObjects).toContainEqual(
        expect.objectContaining({ kind: "Chart", handling: "Summarized" })
      );
    }
  },
  {
    fixtureNumber: 23,
    fixtureId: "23_embedded_images_and_drawings",
    description: "Workbook with embedded media images and vector drawings",
    sheets: [
      {
        name: "BrandedReport",
        grid: [
          ["Item", "Cost"],
          ["LogoBanner", "50"]
        ]
      }
    ],
    options: {
      embeddedObjects: [
        {
          kind: "Image",
          location: "xl/media/image1.png",
          handling: "Summarized",
          summary: "Company header logo"
        },
        {
          kind: "DrawingOrShape",
          location: "xl/drawings/drawing1.xml",
          handling: "Summarized",
          summary: "Callout arrow shape"
        }
      ]
    },
    assertIngestion: (r) => {
      expect(r.embeddedObjects.map((o) => o.kind)).toEqual(["Image", "DrawingOrShape"]);
    }
  },
  {
    fixtureNumber: 24,
    fixtureId: "24_comments_and_notes",
    description: "Worksheet with cell comments and review notes",
    sheets: [
      {
        name: "AuditedLedger",
        grid: [
          ["Account", "Balance"],
          ["PettyCash", "4500"]
        ],
        comments: {
          B2: "Counted in person by internal audit on 2026-03-31"
        }
      }
    ],
    assertIngestion: (r) => {
      const b2 = r.worksheets[0]?.cellMetadata.find((c) => c.address === "B2");
      expect(b2?.comment).toContain("internal audit");
      expect(r.embeddedObjects.some((o) => o.kind === "CommentOrNote")).toBe(true);
    }
  },
  {
    fixtureNumber: 25,
    fixtureId: "25_hyperlinks",
    description: "Worksheet with external and internal cell hyperlinks",
    sheets: [
      {
        name: "LinksSheet",
        grid: [
          ["Vendor", "Portal"],
          ["Acme Corp", "https://acme.example.com"]
        ],
        hyperlinks: {
          B2: "https://acme.example.com"
        }
      }
    ],
    assertIngestion: (r) => {
      const b2 = r.worksheets[0]?.cellMetadata.find((c) => c.address === "B2");
      expect(b2?.hyperlink).toBe("https://acme.example.com");
      expect(r.embeddedObjects.some((o) => o.kind === "Hyperlink")).toBe(true);
    }
  },
  {
    fixtureNumber: 26,
    fixtureId: "26_very_wide_sheet_65_columns",
    description: "Very wide sheet (65 columns)",
    sheets: [
      {
        name: "WideMatrix",
        grid: [
          Array.from({ length: 65 }, (_, i) => `Metric_${i + 1}`),
          Array.from({ length: 65 }, (_, i) => String((i + 1) * 10))
        ]
      }
    ],
    assertIngestion: (r) => {
      expect(r.worksheets[0]?.rawColCount).toBe(65);
      expect(r.worksheets[0]?.tables[0]?.headers).toHaveLength(65);
      expect(r.worksheets[0]?.tables[0]?.a1Range).toBe("A1:BM2");
    }
  },
  {
    fixtureNumber: 27,
    fixtureId: "27_large_row_count_sheet",
    description: "Large row count sheet (1,200 rows)",
    sheets: [
      {
        name: "HighVolume",
        grid: [
          ["RowID", "Branch", "Revenue"],
          ...Array.from({ length: 1200 }, (_, i) => [
            `R-${i + 1}`,
            i % 2 === 0 ? "Cairo" : "Alexandria",
            String(100 + (i % 20))
          ])
        ]
      }
    ],
    assertIngestion: (r) => {
      expect(r.totalAnalyticalRows).toBe(1200);
      expect(r.worksheets[0]?.classification).toBe("MainDataset");
    }
  },
  {
    fixtureNumber: 28,
    fixtureId: "28_sparse_sheet_distant_cells",
    description: "Sparse sheet with offset tables and empty regions",
    sheets: [
      {
        name: "Sparse",
        grid: [
          ["", "", "", "", ""],
          ["", "", "", "", ""],
          ["", "", "Metric", "Value", ""],
          ["", "", "Alpha", "99", ""]
        ]
      }
    ],
    assertIngestion: (r) => {
      expect(r.worksheets[0]?.tables[0]?.a1Range).toBe("C3:D4");
      expect(r.worksheets[0]?.tables[0]?.headers).toEqual(["Metric", "Value"]);
    }
  },
  {
    fixtureNumber: 29,
    fixtureId: "29_duplicate_column_names",
    description: "Sheet with duplicate and blank column names disambiguated deterministically",
    sheets: [
      {
        name: "DupCols",
        grid: [
          ["Amount", "Amount", "", "Amount"],
          ["10", "20", "30", "40"]
        ]
      }
    ],
    assertIngestion: (r) => {
      expect(r.worksheets[0]?.tables[0]?.headers).toEqual([
        "Amount",
        "Amount_2",
        "Column_3",
        "Amount_3"
      ]);
    }
  },
  {
    fixtureNumber: 30,
    fixtureId: "30_empty_columns_and_empty_rows",
    description: "Sheet with empty columns and empty rows separating regions",
    sheets: [
      {
        name: "Gaps",
        grid: [
          ["ColA", "ColB"],
          ["", ""],
          ["1", "2"]
        ]
      }
    ],
    assertIngestion: (r) => {
      expect(r.totalTablesExtracted).toBe(2);
    }
  },
  {
    fixtureNumber: 31,
    fixtureId: "31_inconsistent_data_types_in_one_column",
    description: "Column with mixed numbers and text tokens",
    sheets: [
      {
        name: "MixedCol",
        grid: [
          ["Item", "Score"],
          ["A", "95"],
          ["B", "88"],
          ["C", "91"],
          ["D", "84"],
          ["E", "Pending"]
        ]
      }
    ],
    assertIngestion: (r) => {
      const model = buildSemanticWorkbookModel(r);
      const scoreCol = model.tables[0]?.columns.find((c) => c.headerName === "Score");
      expect(scoreCol?.analyticalRole).toBe("Measure");
      expect(scoreCol?.numericStats?.count).toBe(4);
    }
  },
  {
    fixtureNumber: 32,
    fixtureId: "32_outliers_and_missing_values",
    description: "Dataset with statistical IQR outliers and missing sentinels",
    sheets: [
      {
        name: "OutliersAndNulls",
        grid: [
          ["Branch", "Cost_EGP"],
          ["B1", "100"],
          ["B2", "105"],
          ["B3", "98"],
          ["B4", "102"],
          ["B5", "N/A"],
          ["B6", "5000"]
        ]
      }
    ],
    assertIngestion: (r) => {
      const model = buildSemanticWorkbookModel(r);
      const costCol = model.tables[0]?.columns[1]!;
      expect(costCol.missingCount).toBe(1);
      expect(costCol.numericStats?.outlierCount).toBe(1);
      expect(costCol.numericStats?.outlierValues).toEqual([5000]);
    }
  },
  {
    fixtureNumber: 33,
    fixtureId: "33_cancelled_or_corrupted_stream",
    description: "Aborted/corrupted ingestion stream throws deterministic SF-IMP-002",
    sheets: [{ name: "Aborted", grid: [["A"], ["1"]] }],
    options: (() => {
      const ctrl = new AbortController();
      ctrl.abort();
      return { signal: ctrl.signal };
    })(),
    expectedErrorCode: "SF-IMP-002",
    assertIngestion: () => {}
  },
  {
    fixtureNumber: 34,
    fixtureId: "34_row_limit_exceeded_guard",
    description: "Exceeding platform row limit throws deterministic SF-IMP-003",
    sheets: [
      {
        name: "TooManyRows",
        grid: [["A"], ["1"], ["2"], ["3"], ["4"]]
      }
    ],
    options: { maxRows: 2 },
    expectedErrorCode: "SF-IMP-003",
    assertIngestion: () => {}
  },
  {
    fixtureNumber: 35,
    fixtureId: "35_legacy_xls_xlsm_xlsb_csv_ods_formats",
    description: "Multi-format ingestion across .xls, .xlsm, .xlsb, .csv, and .ods",
    sheets: [
      {
        name: "MultiFormat",
        grid: [
          ["Code", "Amount"],
          ["007", "700"]
        ]
      }
    ],
    options: { filename: "legacy_workbook.xlsb" },
    assertIngestion: (r) => {
      expect(r.detectedFormat).toBe("Xlsb");
      expect(r.supportLevel).toBe("SupportedTabularAndFormulas");
      const csvRep = ingestWorkbookStructure(
        [
          {
            name: "flat",
            grid: [
              ["K", "V"],
              ["1", "2"]
            ]
          }
        ],
        { filename: "data.csv" }
      );
      expect(csvRep.detectedFormat).toBe("Csv");
      const odsRep = ingestWorkbookStructure(
        [
          {
            name: "open",
            grid: [
              ["K", "V"],
              ["1", "2"]
            ]
          }
        ],
        { filename: "data.ods" }
      );
      expect(odsRep.detectedFormat).toBe("Ods");
    }
  }
];

describe("Permanent 35-Workbook Regression Matrix & Cross-Studio Consistency (Phase 13)", () => {
  it("executes all 35 permanent workbook regression fixtures with zero silent data loss", () => {
    expect(PERMANENT_35_FIXTURES).toHaveLength(35);

    for (const fixture of PERMANENT_35_FIXTURES) {
      if (fixture.expectedErrorCode) {
        expect(() => ingestWorkbookStructure(fixture.sheets, fixture.options)).toThrow(
          new RegExp(fixture.expectedErrorCode)
        );
      } else {
        const report = ingestWorkbookStructure(fixture.sheets, fixture.options);
        fixture.assertIngestion(report);
      }
    }
  });

  it("verifies analytical accuracy against a Known-Answer ground-truth dataset and enforces Cross-Studio numerical identity", () => {
    // Ground-Truth Dataset:
    // 5 branches, 1 separated Grand Total row (must NOT be double-counted), 1 Identifier column (must NOT be summed/averaged)
    // Exact Revenue sum = 1000 + 2000 + 3000 + 4000 + 5000 = 15,000 EGP
    // Exact Revenue mean = 3,000 EGP, median = 3,000 EGP, min = 1,000, max = 5,000
    // Exact Cost sum = 600 + 1200 + 1800 + 2400 + 3000 = 9,000 EGP
    const ingestion = ingestWorkbookStructure([
      {
        name: "GroundTruth_Financials",
        grid: [
          ["Branch_ID", "Branch_Name", "Period", "Revenue_EGP", "Cost_EGP"],
          ["101", "Cairo", "2026-01-01", "1000", "600"],
          ["102", "Alexandria", "2026-02-01", "2000", "1200"],
          ["103", "Giza", "2026-03-01", "3000", "1800"],
          ["104", "Mansoura", "2026-04-01", "4000", "2400"],
          ["105", "Tanta", "2026-05-01", "5000", "3000"],
          ["Grand Total", "", "", "15000", "9000"]
        ]
      }
    ]);

    const model = buildSemanticWorkbookModel(ingestion);
    const analysis = buildVerifiedAnalysisReport(model);
    const dashboard = generateAutoDashboard(model, analysis);
    const presentation = generatePresentationDeck(model, analysis);
    const document = generateDocumentReport(model, analysis);

    // 1. Verify ground-truth statistical accuracy
    const revCol = model.tables[0]!.columns.find((c) => c.headerName === "Revenue_EGP")!;
    expect(revCol.numericStats).toMatchObject({
      count: 5,
      sum: 15000,
      mean: 3000,
      median: 3000,
      min: 1000,
      max: 5000,
      q1: 2000,
      q2: 3000,
      q3: 4000,
      iqr: 2000,
      outlierCount: 0
    });

    // Branch_ID must be classified as Identifier and NEVER appear in canonicalKpis
    expect(model.canonicalKpis.map((k) => k.columnName)).toEqual(["Revenue_EGP", "Cost_EGP"]);

    // 2. Verify Cross-Studio Numerical Identity across:
    // - Semantic Model (`15,000 EGP`, `9,000 EGP`)
    // - Conversational Assistant
    // - Dashboard Studio KPI widgets & charts
    // - Presentation Studio KPI slide
    // - Document Studio KPI summary section
    // - Exported deliverables (.xlsx, .pptx, .docx, .safi.html)
    const canonicalFormatted = model.canonicalKpis.map((k) => k.formattedValue);
    expect(canonicalFormatted).toEqual(["15,000 EGP", "9,000 EGP"]);

    const assistantReply = answerAnalyticalQuestion("What are the main findings?", model, analysis);
    expect(assistantReply.answerText).toContain("15,000 EGP");

    const dashKpis = dashboard.widgets
      .filter((w) => w.kind === "kpi")
      .map((w) => w.kpiMetric?.formattedValue);
    expect(dashKpis).toEqual(canonicalFormatted);

    const slideKpis = presentation.slides
      .find((s) => s.sectionRole === "KeyMetrics")!
      .kpis.map((k) => k.formattedValue);
    expect(slideKpis).toEqual(canonicalFormatted);

    const docKpis = document.sections
      .find((s) => s.kind === "KpiSummary")!
      .kpis!.map((k) => k.formattedValue);
    expect(docKpis).toEqual(canonicalFormatted);

    const pptxExport = exportStudioDeliverable({ format: "pptx", model, presentation });
    expect(pptxExport.content).toContain("15,000 EGP");

    const docxExport = exportStudioDeliverable({ format: "docx", model, document });
    expect(docxExport.content).toContain("15,000 EGP");

    const htmlExport = exportStudioDeliverable({ format: "safi.html", model, presentation });
    expect(htmlExport.content).toContain("15,000 EGP");
  });
});
