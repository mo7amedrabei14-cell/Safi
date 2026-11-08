import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { AmbientBackground } from "./AmbientBackground";
import { SmoothScrollProvider, shouldEnableSmoothScroll } from "./SmoothScrollProvider";

describe("UI-Phase-1 motion infrastructure", () => {
  it("renders three aurora halos and a masked dot grid as a decorative layer", () => {
    const html = renderToString(createElement(AmbientBackground, null));

    expect(html).toContain('aria-hidden="true"');
    expect(html).toContain("28px 28px");
    expect(html.split("blur(130px)").length - 1).toBe(3);
    expect(html).not.toContain("tabindex");
  });

  it("keeps smooth scrolling disabled outside a real browser (no Lenis in tests)", () => {
    expect(shouldEnableSmoothScroll()).toBe(false);
  });

  it("passes children through untouched when scrolling is native", () => {
    const html = renderToString(
      createElement(SmoothScrollProvider, null, createElement("main", null, "محتوى صافي"))
    );

    expect(html).toBe("<main>محتوى صافي</main>");
  });
});
