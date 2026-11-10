import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { PasswordResetModal } from "./PasswordResetModal";

const noop = (): void => undefined;

describe("SafiData password reset portal", () => {
  it("requests a one-time link without revealing the token", () => {
    const html = renderToString(createElement(PasswordResetModal, { open: true, onClose: noop }));

    expect(html).toContain("Reset your password");
    expect(html).toContain("Send reset link");
    expect(html).toContain("Email address");
    expect(html).not.toContain("New password");
  });

  it("renders the new-password form for an emailed token and never echoes it", () => {
    const token = "a".repeat(64);
    const html = renderToString(
      createElement(PasswordResetModal, {
        open: true,
        onClose: noop,
        lang: "ar",
        initialToken: token
      })
    );

    expect(html).toContain("اختر كلمة مرور جديدة");
    expect(html).toContain("تأكيد كلمة المرور");
    expect(html).toContain("8 خانات على الأقل");
    expect(html).not.toContain(token);
  });

  it("renders nothing while closed", () => {
    const html = renderToString(createElement(PasswordResetModal, { open: false, onClose: noop }));
    expect(html).toBe("");
  });
});
