import { describe, it, expect } from "vitest";
import React from "react";
import { renderToString } from "react-dom/server";
import { CleaningWorkspace } from "./CleaningWorkspace";

describe("apps/web CleaningWorkspace tests (Stories 1, 2, 3)", () => {
  it("renders cleaning workspace with grid, understanding panel, and steps panel", () => {
    const html = renderToString(React.createElement(CleaningWorkspace, null));
    expect(html).toContain("مساحة التنضيف");
    expect(html).toContain("لوحة الفهم");
    expect(html).toContain("خطوات التنضيف والمعاينة");
    expect(html).toContain("الاسم");
    expect(html).toContain("تاريخ الصرف");
  });
});
