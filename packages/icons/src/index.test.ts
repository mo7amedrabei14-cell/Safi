import { describe, it, expect } from "vitest";
import React from "react";
import { renderToString } from "react-dom/server";
import { SafiIcon } from "../src/index";

describe("packages/icons accessibility & RTL tests", () => {
  it("renders icon with SVG elements and flipInRtl support", () => {
    const html = renderToString(React.createElement(SafiIcon, { size: 24, flipInRtl: true }));
    expect(html).toContain("<svg");
    expect(html).toContain('width="24"');
    expect(html).toContain("transform:scaleX(-1)");
  });
});
