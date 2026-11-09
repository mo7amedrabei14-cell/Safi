import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { AccountAccessSheet } from "./AccountAccessSheet";

const noop = (): void => undefined;

describe("account access sheet", () => {
  it("renders English sign-in and account creation by default", () => {
    const html = renderToString(
      createElement(AccountAccessSheet, {
        open: true,
        onClose: noop,
        onAuthenticated: noop,
        onLoggedOut: noop
      })
    );

    expect(html).toContain("Sign in");
    expect(html).toContain("Create account");
    expect(html).toContain("Email address");
    expect(html).toContain("Password");
    expect(html).toContain("Resend verification email");
    expect(html).not.toContain("تسجيل الدخول");
  });

  it("renders the same account flow in Arabic when selected", () => {
    const html = renderToString(
      createElement(AccountAccessSheet, {
        open: true,
        onClose: noop,
        onAuthenticated: noop,
        onLoggedOut: noop,
        lang: "ar"
      })
    );

    expect(html).toContain("تسجيل الدخول");
    expect(html).toContain("البريد الإلكتروني");
    expect(html).toContain("حساب جديد");
    expect(html).toContain('dir="rtl"');
  });

  it("does not render a session token into the interface", () => {
    const html = renderToString(
      createElement(AccountAccessSheet, {
        open: true,
        onClose: noop,
        onAuthenticated: noop,
        onLoggedOut: noop,
        sessionToken: "sfs_test_secret_token"
      })
    );

    expect(html).toContain("You are signed in");
    expect(html).not.toContain("sfs_test_secret_token");
    expect(html).toContain("Sign out");
  });
});
