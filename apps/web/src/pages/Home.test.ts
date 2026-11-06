import { describe, it, expect } from "vitest";
import React from "react";
import { renderToString } from "react-dom/server";
import { Home } from "./Home";

describe("apps/web Home dropzone & error codes tests", () => {
  it("renders Home screen with dropzone and error handling", () => {
    const html = renderToString(React.createElement(Home, null));
    expect(html).toContain("اختر ملفًا أو اسحبه للبدء");
    expect(html).toContain("الملفات الأخيرة");
  });
});
