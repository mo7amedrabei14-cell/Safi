import { describe, it, expect } from "vitest";
import {
  sanitizeCellForExport,
  generateExportContent,
  exportStudioDeliverable,
  listZipEntryPaths
} from "./export";
import { ingestWorkbookStructure } from "./workbook-ingestion";
import { buildSemanticWorkbookModel } from "./semantic-intelligence";
import { buildVerifiedAnalysisReport } from "./verified-analysis-engine";
import {
  generateAutoDashboard,
  generateDocumentReport,
  generatePresentationDeck
} from "./quad-studio-engine";

describe("Export utilities & Genuine Multi-Format Exporters (Phase 9 & A11)", () => {
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
    expect(res.sizeBytes).toBeGreaterThan(10);
  });

  it("generates genuine OOXML XLSX ZIP export with Change Log and RTL spec", () => {
    const cols = ["Col1", "Amount"];
    const data = [["Val1", "1500"]];
    const res = generateExportContent(cols, data, {
      format: "xlsx",
      protectFormulaInjection: true,
      rtl: true
    });
    expect(res.filename).toBe("safi-export.xlsx");
    // Must be a genuine PK\x03\x04 ZIP archive
    expect(res.binaryBytes[0]).toBe(0x50); // 'P'
    expect(res.binaryBytes[1]).toBe(0x4b); // 'K'
    expect(res.binaryBytes[2]).toBe(0x03);
    expect(res.binaryBytes[3]).toBe(0x04);
    const entries = listZipEntryPaths(res.binaryBytes);
    expect(entries).toContain("[Content_Types].xml");
    expect(entries).toContain("xl/workbook.xml");
    expect(entries).toContain("xl/worksheets/sheet1.xml");
    expect(entries).toContain("xl/worksheets/sheet2.xml");
    expect(res.content).toContain("Change Log");
    expect(res.content).toContain('rightToLeft="1"');
  });

  it("generates genuine PPTX, DOCX, PDF, PNG, SVG, Interactive HTML, and Power BI packages", () => {
    const ingestion = ingestWorkbookStructure([
      {
        name: "Revenue_2026",
        grid: [
          ["Branch", "Date", "Revenue_EGP"],
          ["Cairo", "2026-01-01", "4500"],
          ["Alexandria", "2026-02-01", "3200"]
        ]
      }
    ]);
    const model = buildSemanticWorkbookModel(ingestion);
    const analysis = buildVerifiedAnalysisReport(model);
    const dashboard = generateAutoDashboard(model, analysis);
    const presentation = generatePresentationDeck(model, analysis);
    const document = generateDocumentReport(model, analysis);

    // 1. Genuine PPTX OOXML ZIP
    const pptx = exportStudioDeliverable({ format: "pptx", model, presentation });
    expect(pptx.filename).toBe("safi-presentation.pptx");
    const pptxEntries = listZipEntryPaths(pptx.binaryBytes);
    expect(pptxEntries).toContain("[Content_Types].xml");
    expect(pptxEntries).toContain("ppt/presentation.xml");
    expect(pptxEntries).toContain("ppt/slides/slide1.xml");
    expect(pptxEntries).toContain("ppt/slides/slide10.xml");

    // 2. Genuine DOCX OOXML ZIP (with RTL support)
    const docx = exportStudioDeliverable({ format: "docx", model, document, rtl: true });
    expect(docx.filename).toBe("safi-report.docx");
    const docxEntries = listZipEntryPaths(docx.binaryBytes);
    expect(docxEntries).toContain("[Content_Types].xml");
    expect(docxEntries).toContain("word/document.xml");
    expect(docxEntries).toContain("word/styles.xml");
    expect(docxEntries).toContain("word/header1.xml");
    expect(docxEntries).toContain("word/footer1.xml");
    expect(docx.content).toContain("<w:bidi/>");

    // 3. Genuine multi-page PDF 1.7 with non-zero byte-offset startxref
    const pdf = exportStudioDeliverable({ format: "pdf", model, document, rtl: true });
    expect(pdf.filename).toBe("safi-report.pdf");
    expect(pdf.content.startsWith("%PDF-1.7")).toBe(true);
    expect(pdf.content.trim().endsWith("%%EOF")).toBe(true);
    const startxrefMatch = /startxref\s+(\d+)/.exec(pdf.content);
    expect(Number(startxrefMatch?.[1] ?? 0)).toBeGreaterThan(100);
    expect(pdf.content).toContain("Page 1 of ");

    // 4. Genuine multi-color chart-rendered PNG (\x89PNG\r\n\x1a\n)
    const png = exportStudioDeliverable({ format: "png", model, dashboard });
    expect(png.filename).toBe("safi-chart.png");
    expect(Array.from(png.binaryBytes.slice(0, 8))).toEqual([
      0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a
    ]);
    expect(png.content).toContain("IHDR");
    expect(png.content).toContain("IDAT");
    expect(png.content).toContain("IEND");

    // 5. Genuine SVG (with distinct chart visual kind)
    const svg = exportStudioDeliverable({ format: "svg", model, dashboard });
    expect(svg.filename).toBe("safi-chart.svg");
    expect(svg.content).toContain("<svg");

    // 6. Genuine Interactive HTML (.safi.html)
    const html = exportStudioDeliverable({ format: "safi.html", model, presentation });
    expect(html.filename).toBe("safi-presentation.safi.html");
    expect(html.content).toContain("<!DOCTYPE html>");
    expect(html.content).toContain('data-safi-presentation-player="true"');

    // 7. Genuine Power BI Export Bundle (.pbids + model.bim + multi-table CSVs)
    const pbi = exportStudioDeliverable({ format: "powerbi", model });
    expect(pbi.filename).toBe("safi-powerbi-package.zip");
    const pbiEntries = listZipEntryPaths(pbi.binaryBytes);
    expect(pbiEntries).toContain("safi-datasource.pbids");
    expect(pbiEntries).toContain("model.bim");
    expect(pbiEntries).toContain("README-POWERBI.md");
    expect(pbiEntries.some((p) => p.startsWith("data/") && p.endsWith(".csv"))).toBe(true);
    expect(pbi.content).toContain('"compatibilityLevel": 1550');

    // 8. Multi-sheet & multi-table XLSX never truncates to sheet 0 table 0
    const multiIngest = ingestWorkbookStructure([
      {
        name: "Sales",
        grid: [
          ["Region", "Revenue"],
          ["Cairo", "1000"]
        ]
      },
      {
        name: "Costs",
        grid: [
          ["Region", "Cost"],
          ["Cairo", "400"]
        ]
      },
      {
        name: "Secret",
        visibility: "Hidden",
        grid: [
          ["Param", "Rate"],
          ["Tax", "0.14"]
        ]
      }
    ]);
    const multiModel = buildSemanticWorkbookModel(multiIngest);
    const xlsxVisibleOnly = exportStudioDeliverable({ format: "xlsx", model: multiModel });
    const xlsxVisEntries = listZipEntryPaths(xlsxVisibleOnly.binaryBytes);
    expect(xlsxVisEntries).toContain("xl/worksheets/sheet1.xml");
    expect(xlsxVisEntries).toContain("xl/worksheets/sheet2.xml");
    expect(xlsxVisibleOnly.content).toContain("Sales");
    expect(xlsxVisibleOnly.content).toContain("Costs");
    expect(xlsxVisibleOnly.content).toContain("Secret (Hidden)");

    const xlsxWithHidden = exportStudioDeliverable({
      format: "xlsx",
      model: multiModel,
      includeHiddenSheets: true
    });
    expect(listZipEntryPaths(xlsxWithHidden.binaryBytes).length).toBeGreaterThan(
      xlsxVisEntries.length
    );
  });
});
