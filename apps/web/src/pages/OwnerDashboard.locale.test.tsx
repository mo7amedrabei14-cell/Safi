import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { OwnerDashboard } from "./OwnerDashboard";

describe("Owner dashboard locale", () => {
  it("renders English by default without Arabic interface copy", () => {
    const html = renderToString(createElement(OwnerDashboard, null));
    expect(html).toContain("Platform Owner Access Required");
    expect(html).toContain("Google Authenticator");
    expect(html).toContain("SF-AUTH-013");
    expect(html).not.toMatch(/[\u0600-\u06FF]/);
  });

  it("keeps Arabic copy when Arabic is explicitly selected", () => {
    const html = renderToString(createElement(OwnerDashboard, { lang: "ar" }));
    expect(html).toContain("الوصول محصور بمالك المنصة المعتمد");
    expect(html).toContain("Google Authenticator");
  });
});
