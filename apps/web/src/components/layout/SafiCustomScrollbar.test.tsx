import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  computeNextArrowScrollTop,
  computeScrollbarMetrics,
  computeScrollStepPx,
  SCROLLBAR_MIN_THUMB_PX
} from "./SafiCustomScrollbar";

describe("SafiData custom scrollbar", () => {
  it("stays hidden when the document does not scroll", () => {
    expect(computeScrollbarMetrics(0, 800, 800, 784).visible).toBe(false);
    expect(computeScrollbarMetrics(0, 400, 800, 784).visible).toBe(false);
  });

  it("derives a proportional thumb and boundary flags at start and end", () => {
    const top = computeScrollbarMetrics(0, 4000, 800, 784);
    expect(top.visible).toBe(true);
    expect(top.thumbHeight).toBeGreaterThanOrEqual(SCROLLBAR_MIN_THUMB_PX);
    expect(top.thumbTop).toBe(0);
    expect(top.progress).toBe(0);
    expect(top.atStart).toBe(true);
    expect(top.atEnd).toBe(false);

    const bottom = computeScrollbarMetrics(3200, 4000, 800, 784);
    expect(bottom.progress).toBe(1);
    expect(bottom.thumbTop).toBe(bottom.trackTravel);
    expect(bottom.atStart).toBe(false);
    expect(bottom.atEnd).toBe(true);

    const middle = computeScrollbarMetrics(1600, 4000, 800, 784);
    expect(middle.progress).toBeCloseTo(0.5, 5);
    expect(middle.thumbTop).toBeLessThanOrEqual(middle.trackTravel);
    expect(middle.atStart).toBe(false);
    expect(middle.atEnd).toBe(false);
  });

  it("clamps out-of-range scroll positions", () => {
    const negative = computeScrollbarMetrics(-500, 4000, 800, 784);
    expect(negative.progress).toBe(0);
    expect(negative.atStart).toBe(true);
    const past = computeScrollbarMetrics(99_999, 4000, 800, 784);
    expect(past.progress).toBe(1);
    expect(past.atEnd).toBe(true);
  });

  it("computes bounded up and down arrow step destinations", () => {
    const step = computeScrollStepPx(900);
    expect(step).toBeGreaterThanOrEqual(96);

    expect(computeNextArrowScrollTop(0, 2000, 900, "down")).toBe(step);
    expect(computeNextArrowScrollTop(50, 2000, 900, "up")).toBe(0);
    expect(computeNextArrowScrollTop(1950, 2000, 900, "down")).toBe(2000);
    expect(computeNextArrowScrollTop(500, 2000, 900, "up")).toBe(500 - step);
  });

  it("exposes functional arrow controls and preserves internal scrollbars", () => {
    const component = readFileSync(new URL("./SafiCustomScrollbar.tsx", import.meta.url), "utf8");
    const css = readFileSync(new URL("../../index.css", import.meta.url), "utf8");
    const app = readFileSync(new URL("../../App.tsx", import.meta.url), "utf8");

    expect(component).toContain('role="scrollbar"');
    expect(component).toContain('data-scrollbar-arrow="up"');
    expect(component).toContain('data-scrollbar-arrow="down"');
    expect(component).toContain("disabled={metrics.atStart}");
    expect(component).toContain("disabled={metrics.atEnd}");
    expect(component).toContain("aria-valuenow");
    expect(component).toContain("data-safi-custom-scrollbar");
    expect(component).toContain("onKeyDown={onThumbKeyDown}");
    expect(app).toContain("active={!introMounted}");
    expect(app).toContain("<SafiCustomScrollbar");

    expect(css).toContain("html::-webkit-scrollbar");
    expect(css).toContain("body::-webkit-scrollbar");
    expect(css).toContain("*:not(html):not(body)::-webkit-scrollbar");
    expect(css).not.toMatch(/^\s*\*::-webkit-scrollbar/m);
    expect(css).toContain(".safi-auth-modal::-webkit-scrollbar");
    expect(css).toContain(".safi-scrollbar-arrow");
    expect(css).toContain(".safi-scrollbar-thumb");
    expect(css).toContain(".safi-scrollbar.is-engaged .safi-scrollbar-track");
    expect(component).toContain("data-safi-own-scrollbar");
  });
});
