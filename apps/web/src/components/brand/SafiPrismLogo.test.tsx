import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { SafiPrismLogo } from "./SafiPrismLogo";
import { DynamicStatusIsland, STATUS_METRICS } from "./DynamicStatusIsland";

describe("UI-Phase-2 brand identity", () => {
  it("renders the prism mark with a drawn shield, three waves and three data nodes", () => {
    const html = renderToString(createElement(SafiPrismLogo, null));

    expect(html).toContain("<svg");
    expect(html).toContain('aria-label="شعار صافي البيانات"');
    expect(html).toContain('id="safi-prism-glow"');
    expect(html).toContain('stdDeviation="2.5"');
    expect(html.split('class="safi-node"').length - 1).toBe(3);
    expect(html.split('data-draw="true"').length - 1).toBe(10);
    expect(html).toContain("صافي البيانات");
    expect(html).toContain("SafiData • Gold Master v1.0");
  });

  it("scales the mark for the requested size", () => {
    const small = renderToString(createElement(SafiPrismLogo, { size: "sm" }));
    const large = renderToString(createElement(SafiPrismLogo, { size: "lg" }));

    expect(small).toContain('width="40"');
    expect(large).toContain('width="112"');
  });

  it("renders the status capsule collapsed with its three offline metrics ready", () => {
    const html = renderToString(createElement(DynamicStatusIsland, null));

    expect(html).toContain('aria-expanded="false"');
    expect(html).toContain("نواة Rust نشطة • صفر شبكة • خزنة SQLCipher مؤمّنة");
    expect(html).toContain('aria-controls="safi-status-panel"');
    expect(html).toContain('aria-label="مقاييس الأمان الفورية"');

    expect(STATUS_METRICS).toHaveLength(3);
    for (const metric of STATUS_METRICS) {
      expect(html).toContain(metric.label);
      expect(html).toContain(metric.value);
    }
    expect(html).toContain("Ed25519 Offline Verified");
    expect(html).toContain("BLAKE3 Chain Intact");
    expect(html).toContain("125,000 صف/ث");
  });
});
