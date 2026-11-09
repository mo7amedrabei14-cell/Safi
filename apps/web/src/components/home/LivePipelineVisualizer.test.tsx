import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { LivePipelineVisualizer, PIPELINE_STAGES, simulatedDigest } from "./LivePipelineVisualizer";

describe("UI-Phase-4 live pipeline visualizer", () => {
  it("renders the three pipeline stages with their full descriptions", () => {
    const html = renderToString(<LivePipelineVisualizer lang="ar" />);

    expect(PIPELINE_STAGES).toHaveLength(3);
    for (const stage of PIPELINE_STAGES) {
      expect(html).toContain(stage.title);
      expect(html).toContain(stage.detail);
    }

    expect(html).toContain("CSV / XLSX");
    expect(html).toContain("PII");
    expect(html).toContain("BLAKE3");
    expect(html).toContain("fsync");
    expect(html).toContain("تقرير PDF موقّع");
  });

  it("exposes the run control, four data packets and the fingerprint slot", () => {
    const html = renderToString(createElement(LivePipelineVisualizer, null));

    expect(html).toContain("تشغيل محاكاة فورية");
    expect(html.split('data-packet="').length - 1).toBe(4);
    expect(html).toContain('data-safi-decrypted="true"');
    expect(html).toContain("بصمة BLAKE3");
    expect(html).toContain("الصفوف المعالجة");
    expect(html).toContain("عدد مرات التشغيل:");
    expect(html).toContain("الصفوف المعالجة");
    expect(html).toContain('data-metric-value="0"');
  });

  it("defaults to English labels for all pipeline stages", () => {
    const html = renderToString(<LivePipelineVisualizer />);

    expect(html).toContain("Live pipeline simulation");
    expect(html).toContain("Raw input");
    expect(html).toContain("SafiData core engine");
    expect(html).toContain("Verified outputs");
    expect(html).toContain("Run simulation now");
    expect(html).not.toContain("المدخلات الخام");
  });

  it("derives a deterministic 16-hex fingerprint per simulated run", () => {
    const first = simulatedDigest(1);
    expect(first).toMatch(/^[0-9a-f]{16}$/);
    expect(simulatedDigest(1)).toBe(first);
    expect(simulatedDigest(2)).not.toBe(first);
  });
});
