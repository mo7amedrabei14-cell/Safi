import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { AuthModal } from "./AuthModal";

const noop = (): void => undefined;

describe("SafiData motion auth portal", () => {
  it("renders sign in, the seven-day trial tab, and all three OAuth providers", () => {
    const html = renderToString(createElement(AuthModal, { open: true, onClose: noop }));

    expect(html).toContain("Sign In");
    expect(html).toContain("safi-auth-close");
    expect(html).toContain("Create account — free 7-day trial (Sign Up)");
    expect(html).toContain("Continue with Google");
    expect(html).toContain("Continue with Apple");
    expect(html).toContain("Continue with GitHub");
    expect(html).toContain('aria-label="Continue with Google"');
    expect(html).toContain("Email address");
    expect(html).toContain("Password");
    expect(html).toContain("mo7amed.rabei14@gmail.com");
  });

  it("exposes the eight-character minimum with a live meter and an eye toggle", () => {
    const html = renderToString(
      createElement(AuthModal, { open: true, onClose: noop, lang: "ar", initialMode: "signup" })
    );

    expect(html).toContain("8 خانات كحد أدنى");
    expect(html).toContain('aria-label="إظهار كلمة المرور"');
    expect(html).toContain('type="password"');
    expect(html).toContain("المتابعة باستخدام Google");
    expect(html).not.toContain("12 خانات");
    expect(html).not.toContain("12 حرف");
  });

  it("wires the duplicate-account guard to the sign-in tab", () => {
    const source = readFileSync(new URL("./AuthModal.tsx", import.meta.url), "utf8");

    expect(source).toContain("SF-AUTH-008");
    expect(source).toContain("DUPLICATE_ACCOUNT_MESSAGE_AR");
    expect(source).toContain("Go to sign in");
    expect(source).toContain('switchMode("signin")');
    expect(source).toContain("isOwnerEmail(normalizedEmail)");
  });

  it("renders the verified owner badge for an owner session", () => {
    const html = renderToString(
      createElement(AuthModal, {
        open: true,
        onClose: noop,
        sessionToken: "sfs_owner_session",
        sessionEmail: "mo7amed.rabei14@gmail.com"
      })
    );

    expect(html).toContain("Verified Owner — Mohamed Rabea");
    expect(html).not.toContain("sfs_owner_session");
  });
  it("renders a mandatory sign-in gate without any close control", () => {
    const html = renderToString(
      createElement(AuthModal, {
        open: true,
        mandatory: true,
        onClose: noop
      })
    );

    expect(html).toContain("Sign In");
    expect(html).toContain('class="safi-auth-modal"');
    expect(html).not.toContain("safi-auth-close");
  });

});
