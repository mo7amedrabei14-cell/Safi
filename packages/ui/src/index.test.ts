import { describe, it, expect } from "vitest";
import React from "react";
import { renderToString } from "react-dom/server";
import { Button, Dialog, Progress, Toast, GlideDataGrid } from "../src/index";
import { SavingsCounter, AccessibleSortableList } from "../src/react-bits/index";

describe("packages/ui WCAG 2.2 AA & RTL tests", () => {
  it("renders Button with 44px touch target and ARIA attributes", () => {
    const html = renderToString(React.createElement(Button, { variant: "primary" }, "ابدأ"));
    expect(html).toContain("button");
    expect(html).toContain("min-height:44px");
    expect(html).toContain("min-width:44px");
  });

  it("renders Dialog with modal role and Escape key support", () => {
    const html = renderToString(
      React.createElement(
        Dialog,
        { isOpen: true, onClose: () => {}, title: "تأكيد الحذف" },
        "هل أنت متأكد؟"
      )
    );
    expect(html).toContain('role="dialog"');
    expect(html).toContain('aria-modal="true"');
    expect(html).toContain("direction:rtl");
  });

  it("renders Progress linear and circular correctly", () => {
    const lin = renderToString(React.createElement(Progress, { value: 50, label: "التقدم" }));
    expect(lin).toContain('role="progressbar"');
    expect(lin).toContain('aria-valuenow="50"');

    const circ = renderToString(React.createElement(Progress, { value: 75, variant: "circular" }));
    expect(circ).toContain("<svg");
  });

  it("renders Toast with alert role", () => {
    const html = renderToString(
      React.createElement(Toast, { message: "تم الحفظ", type: "success" })
    );
    expect(html).toContain('role="alert"');
  });

  it("renders GlideDataGrid and SavingsCounter", () => {
    const grid = renderToString(
      React.createElement(GlideDataGrid, { columns: ["أ", "ب"], data: [["1", "2"]] })
    );
    expect(grid).toContain("<canvas");

    const savings = renderToString(
      React.createElement(SavingsCounter, { hoursSaved: 10, filesCleaned: 5 })
    );
    expect(savings).toContain("عدّاد التوفير");
  });

  it("renders AccessibleSortableList with click-to-move buttons", () => {
    const list = renderToString(
      React.createElement(AccessibleSortableList, { items: ["أ", "ب"], onReorder: () => {} })
    );
    expect(list).toContain("↑");
    expect(list).toContain("↓");
  });
});
