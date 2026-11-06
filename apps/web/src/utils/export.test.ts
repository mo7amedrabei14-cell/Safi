import { describe, it, expect } from "vitest";
import { sanitizeCellForExport, generateExportContent } from "./export";

describe("Export utilities & Formula Injection Protection (A11)", () => {
  it("protects against formula injection when enabled", () => {
    const malicious = "=SUM(1+1)";
    const safe = sanitizeCellForExport(malicious, true);
    expect(safe).toBe(`'${malicious}`);
  });

  it("does not protect when disabled", () => {
    const malicious = "=SUM(1+1)";
    const safe = sanitizeCellForExport(malicious, false);
    expect(safe).toBe(malicious);
  });

  it("generates CSV export with sanitized cells", () => {
    const cols = ["Name", "Formula"];
    const data = [["Ahmed", "=cmd|'/c calc'!A1"]];
    const res = generateExportContent(cols, data, {
      format: "csv",
      protectFormulaInjection: true,
      rtl: true
    });
    expect(res.filename).toBe("safi-export.csv");
    expect(res.content).toContain("'=cmd");
  });

  it("generates XLSX export with change log and RTL spec", () => {
    const cols = ["Col1"];
    const data = [["Val1"]];
    const res = generateExportContent(cols, data, {
      format: "xlsx",
      protectFormulaInjection: true,
      rtl: true
    });
    expect(res.filename).toBe("safi-export.xlsx");
    expect(res.content).toContain("Change Log");
  });
});
