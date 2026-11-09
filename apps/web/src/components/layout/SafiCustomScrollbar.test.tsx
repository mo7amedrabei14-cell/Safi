import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { computeScrollbarMetrics, SCROLLBAR_MIN_THUMB_PX } from "./SafiCustomScrollbar";

describe("SafiData custom scrollbar", () => {
  it("stays hidden when the document does not scroll", () => {
    expect(computeScrollbarMetrics(0, 800, 800, 784).visible).toBe(false);
    expect(computeScrollbarMetrics(0, 400, 800, 784).visible).toBe(false);
  });

  it("derives a proportional thumb that never leaves the track", () => {
    const metrics = computeScrollbarMetrics(0, 4000, 800, 784);
    expect(metrics.visible).toBe(true);
    expect(metrics.thumbHeight).toBeGreaterThanOrEqual(SCROLLBAR_MIN_THUMB_PX);
    expect(metrics.thumbTop).toBe(0);
    expect(metrics.progress).toBe(0);

    const bottom = computeScrollbarMetrics(3200, 4000, 800, 784);
    expect(bottom.progress).toBe(1);
    expect(bottom.thumbTop).toBe(bottom.trackTravel);

    const middle = computeScrollbarMetrics(1600, 4000, 800, 784);
    expect(middle.progress).toBeCloseTo(0.5, 5);
    expect(middle.thumbTop).toBeLessThanOrEqual(middle.trackTravel);
  });

  it("clamps out-of-range scroll positions", () => {
    const negative = computeScrollbarMetrics(-500, 4000, 800, 784);
    expect(negative.progress).toBe(0);
    const past = computeScrollbarMetrics(99_999, 4000, 800, 784);
    expect(past.progress).toBe(1);
  });

  it("renders an accessible role=scrollbar thumb and hides the native bar", () => {
    const component = readFileSync(new URL("./SafiCustomScrollbar.tsx", import.meta.url), "utf8");
    const css = readFileSync(new URL("../../index.css", import.meta.url), "utf8");
    const app = readFileSync(new URL("../../App.tsx", import.meta.url), "utf8");

    expect(component).toContain('role="scrollbar"');
    expect(component).toContain("aria-valuenow");
    expect(component).toContain("data-safi-custom-scrollbar");
    expect(component).toContain("onKeyDown={onThumbKeyDown}");
    expect(app).toContain("active={!introMounted}");
    expect(app).toContain("<SafiCustomScrollbar");

    expect(css).toContain("scrollbar-width: none");
    expect(css).toContain("*::-webkit-scrollbar");
    expect(css).toContain("display: none");
    expect(css).toContain(".safi-scrollbar-thumb");
    expect(css).toContain(".safi-scrollbar.is-engaged .safi-scrollbar-track");
  });
});
