import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { OwnerDashboard } from "./OwnerDashboard";

describe("Real Owner Control Plane (Google Authenticator TOTP + sealed identity)", () => {
  it("denies access by default when no server-authenticated Owner session is provided", () => {
    const html = renderToString(createElement(OwnerDashboard, { lang: "en" }));

    expect(html).toContain('data-owner-access="denied"');
    expect(html).toContain("SF-AUTH-013");
    expect(html).toContain("Google Authenticator");
    // Must NEVER expose fake hardcoded metrics or YubiKey stubs.
    expect(html).not.toContain("14280");
    expect(html).not.toContain("509800");
    expect(html).not.toContain("YubiKey");
  });

  it("gates an authenticated Owner behind real Google Authenticator RFC 6238 TOTP MFA", () => {
    const html = renderToString(
      createElement(OwnerDashboard, {
        lang: "en",
        isOwner: true,
        sessionToken: "sfs_test_session_token_for_render"
      })
    );

    expect(html).toContain('data-owner-mfa="required"');
    expect(html).toContain("Google Authenticator");
    expect(html).toContain("RFC 6238");
    expect(html).not.toContain('data-owner-access="granted"');
    expect(html).not.toMatch(/[\u0600-\u06FF]/);
  });

  it("renders clean Arabic MFA and denial copy when lang is ar", () => {
    const deniedAr = renderToString(createElement(OwnerDashboard, { lang: "ar" }));
    expect(deniedAr).toContain("الوصول محصور بمالك المنصة المعتمد");
    expect(deniedAr).toContain("Google Authenticator");

    const mfaAr = renderToString(
      createElement(OwnerDashboard, {
        lang: "ar",
        isOwner: true,
        sessionToken: "sfs_test_session_token_for_render"
      })
    );
    expect(mfaAr).toContain("التحقق الثنائي لمالك المنصة (Google Authenticator)");
    expect(mfaAr).toContain("أدخل الرمز المكوّن من 6 أرقام");
  });
});
