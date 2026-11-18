import { describe, expect, it } from "vitest";
import { ingestWorkbookStructure } from "./workbook-ingestion";
import { buildSemanticWorkbookModel } from "./semantic-intelligence";

describe("Universal Data Intelligence Engine & Canonical Semantic Model (Phase 2)", () => {
  it("protects Identifier/Phone/Code columns from numeric averaging and computes full statistical distributions for Measures", () => {
    const ingestion = ingestWorkbookStructure([
      {
        name: "Sales_Ledger",
        grid: [
          ["Employee_ID", "Phone", "Branch_Code", "Sale_Date", "Revenue_EGP", "Margin_Pct"],
          ["1001", "+201011112222", "BR-01", "2026-01-05", "1000", "20%"],
          ["1002", "+201011113333", "BR-02", "2026-01-12", "1200", "22%"],
          ["1003", "+201011114444", "BR-01", "2026-01-19", "1100", "21%"],
          ["1004", "+201011115555", "BR-02", "2026-01-26", "1300", "23%"],
          ["1005", "+201011116666", "BR-01", "2026-02-02", "9000", "24%"]
        ]
      },
      {
        name: "Branches_Master",
        grid: [
          ["Branch_Code", "City", "Manager"],
          ["BR-01", "Cairo", "Ahmed"],
          ["BR-02", "Alexandria", "Sara"]
        ]
      }
    ]);

    const model = buildSemanticWorkbookModel(ingestion);
    expect(model.tables).toHaveLength(2);

    const salesTable = model.tables[0]!;
    const empIdCol = salesTable.columns.find((c) => c.headerName === "Employee_ID")!;
    expect(empIdCol.analyticalRole).toBe("Identifier");
    expect(empIdCol.isIdentifier).toBe(true);
    expect(empIdCol.aggregationAllowed).toBe(false);
    expect(empIdCol.numericStats).toBeUndefined();

    const phoneCol = salesTable.columns.find((c) => c.headerName === "Phone")!;
    expect(phoneCol.analyticalRole).toBe("Identifier");
    expect(phoneCol.piiSensitivity).toBe("PersonalContact");
    expect(phoneCol.aggregationAllowed).toBe(false);

    const dateCol = salesTable.columns.find((c) => c.headerName === "Sale_Date")!;
    expect(dateCol.analyticalRole).toBe("Temporal");
    expect(dateCol.dateStats?.minDateIso).toBe("2026-01-05");
    expect(dateCol.dateStats?.maxDateIso).toBe("2026-02-02");
    expect(dateCol.dateStats?.granularity).toBe("Weekly");

    const revCol = salesTable.columns.find((c) => c.headerName === "Revenue_EGP")!;
    expect(revCol.analyticalRole).toBe("Measure");
    expect(revCol.semanticType).toBe("CurrencyAmount");
    expect(revCol.currencyCode).toBe("EGP");
    expect(revCol.numericStats?.sum).toBe(13600);
    expect(revCol.numericStats?.median).toBe(1200);
    expect(revCol.numericStats?.outlierCount).toBe(1);
    expect(revCol.numericStats?.outlierValues).toEqual([9000]);

    // Cross-sheet relationship discovered (Sales_Ledger.Branch_Code -> Branches_Master.Branch_Code = N:1)
    expect(model.relationships).toHaveLength(1);
    expect(model.relationships[0]?.cardinality).toBe("N:1");
    expect(model.relationships[0]?.matchRatePct).toBe(100);
    expect(model.relationships[0]?.orphanSourceKeyCount).toBe(0);

    // Canonical KPIs only include Revenue_EGP and Margin_Pct (never Employee_ID or Phone)
    expect(model.canonicalKpis.map((k) => k.columnName)).toEqual(["Revenue_EGP", "Margin_Pct"]);
    expect(model.canonicalKpis[0]?.rawValue).toBe(13600);
    expect(model.canonicalKpis[1]?.aggregation).toBe("AVERAGE");
  });

  it("classifies data quality issues into ConfirmedIssue, PotentialIssue, Information, and NeedsClarification", () => {
    const ingestion = ingestWorkbookStructure([
      {
        name: "Audit_Sheet",
        grid: [
          ["Ref_Number", "Amount"],
          ["5001", "100"],
          ["5001", "100"],
          ["5002", "#DIV/0!"],
          ["5003", ""],
          ["Total", "200"]
        ]
      }
    ]);

    const model = buildSemanticWorkbookModel(ingestion);
    const categories = new Set(model.qualityFindings.map((f) => f.category));
    expect(categories.has("ConfirmedIssue")).toBe(true);
    expect(categories.has("PotentialIssue")).toBe(true);
    expect(categories.has("Information")).toBe(true);
  });
});
