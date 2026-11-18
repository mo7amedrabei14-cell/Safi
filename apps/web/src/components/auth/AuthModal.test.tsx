import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { AuthModal, buildSanitizedExitGhost, resolveAuthTabKeyboardTarget } from "./AuthModal";

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
  it("isolates its bespoke thin scrollbar and renders animated tabs, form stage, and pointer-only magnet/glare layers", () => {
    const source = readFileSync(new URL("./AuthModal.tsx", import.meta.url), "utf8");
    const css = readFileSync(new URL("../../index.css", import.meta.url), "utf8");
    const html = renderToString(createElement(AuthModal, { open: true, onClose: noop }));

    expect(html).toContain('data-safi-own-scrollbar="true"');
    expect(html).toContain('class="safi-auth-tabs-indicator"');
    expect(html).toContain('class="safi-auth-form-stage"');
    expect(source).toContain('event.pointerType !== "mouse"');
    expect(source).toContain("buildSanitizedExitGhost");
    expect(css).toContain(".safi-auth-modal::-webkit-scrollbar");
    expect(css).toContain("width: 5px");
  });

  it("links tabs to their tabpanel with semantic ARIA attributes and RTL arrow navigation", () => {
    const ltrHtml = renderToString(
      createElement(AuthModal, { open: true, onClose: noop, lang: "en" })
    );
    expect(ltrHtml).toContain('role="tablist"');
    expect(ltrHtml).toContain('aria-orientation="horizontal"');
    expect(ltrHtml).toContain('role="tabpanel"');
    expect(ltrHtml).toContain("aria-controls=");
    expect(ltrHtml).toContain("aria-labelledby=");
    expect(ltrHtml).toContain('tabindex="0"');
    expect(ltrHtml).toContain('tabindex="-1"');

    // LTR arrow direction
    expect(resolveAuthTabKeyboardTarget("signin", "ArrowRight", false)).toBe("signup");
    expect(resolveAuthTabKeyboardTarget("signup", "ArrowRight", false)).toBe("signin");
    expect(resolveAuthTabKeyboardTarget("signup", "ArrowLeft", false)).toBe("signin");
    // RTL mirrored arrow direction
    expect(resolveAuthTabKeyboardTarget("signin", "ArrowLeft", true)).toBe("signup");
    expect(resolveAuthTabKeyboardTarget("signup", "ArrowRight", true)).toBe("signin");
    // Boundaries and vertical mobile stack keys
    expect(resolveAuthTabKeyboardTarget("signup", "Home", false)).toBe("signin");
    expect(resolveAuthTabKeyboardTarget("signin", "End", true)).toBe("signup");
    expect(resolveAuthTabKeyboardTarget("signin", "ArrowDown", false)).toBe("signup");
    expect(resolveAuthTabKeyboardTarget("signup", "ArrowUp", false)).toBe("signin");
  });

  it("never copies password input values or plaintext visibility into exit ghost clones", () => {
    function makeInput(opts: { name: string; type: string; autocomplete: string; value: string }) {
      const attrs = new Map<string, string>([
        ["id", `id-${opts.name}`],
        ["name", opts.name],
        ["type", opts.type],
        ["autocomplete", opts.autocomplete],
        ["value", opts.value]
      ]);
      const node = {
        name: opts.name,
        type: opts.type,
        autocomplete: opts.autocomplete,
        value: opts.value,
        defaultValue: opts.value,
        getAttribute: (key: string) => attrs.get(key) ?? null,
        setAttribute: (key: string, val: string) => {
          attrs.set(key, val);
        },
        removeAttribute: (key: string) => {
          attrs.delete(key);
        },
        cloneInput: () => makeInput(opts)
      };
      return node;
    }

    const emailSrc = makeInput({
      name: "email",
      type: "email",
      autocomplete: "email",
      value: "alex@safidata.dev"
    });
    const secretMaskedSrc = makeInput({
      name: "password",
      type: "password",
      autocomplete: "current-password",
      value: "TopSecret#99"
    });
    const secretVisibleSrc = makeInput({
      name: "password",
      type: "text",
      autocomplete: "new-password",
      value: "VisibleSecret#99"
    });

    const sourceInputs = [emailSrc, secretMaskedSrc, secretVisibleSrc];
    const clonedInputs = sourceInputs.map((item) => item.cloneInput());
    const formAttrs = new Map<string, string>();
    const fakeForm = {
      className: "safi-auth-form",
      setAttribute: (k: string, v: string) => formAttrs.set(k, v),
      querySelectorAll: (selector: string) => {
        if (selector === "input") return sourceInputs as unknown as NodeListOf<HTMLInputElement>;
        return [] as unknown as NodeListOf<Element>;
      },
      cloneNode: () => ({
        className: "",
        setAttribute: (k: string, v: string) => formAttrs.set(k, v),
        querySelectorAll: (selector: string) => {
          if (selector === "input") return clonedInputs as unknown as NodeListOf<HTMLInputElement>;
          if (selector === "button, input") return clonedInputs as unknown as NodeListOf<Element>;
          return clonedInputs as unknown as NodeListOf<Element>;
        }
      })
    };

    buildSanitizedExitGhost(fakeForm as unknown as HTMLElement);

    // Original form values remain intact
    expect(emailSrc.value).toBe("alex@safidata.dev");
    expect(secretMaskedSrc.value).toBe("TopSecret#99");
    expect(secretVisibleSrc.value).toBe("VisibleSecret#99");

    // Email non-secret preserved; both hidden AND visible password clones are completely scrubbed
    expect(clonedInputs[0]?.value).toBe("alex@safidata.dev");
    for (const secretClone of [clonedInputs[1], clonedInputs[2]]) {
      expect(secretClone?.value).toBe("");
      expect(secretClone?.defaultValue).toBe("");
      expect(secretClone?.type).toBe("password");
      expect(secretClone?.getAttribute("value")).toBeNull();
      expect(secretClone?.getAttribute("tabindex")).toBe("-1");
      expect(secretClone?.getAttribute("disabled")).toBe("true");
      expect(secretClone?.getAttribute("name")).toBeNull();
      expect(secretClone?.getAttribute("id")).toBeNull();
    }
    expect(formAttrs.get("aria-hidden")).toBe("true");
  });

  it("runs a single cohesive panel-only entrance motion before first paint without backdrop fade or child stagger", () => {
    const source = readFileSync(new URL("./AuthModal.tsx", import.meta.url), "utf8");

    expect(source).not.toContain("backdropRef");
    expect(source).not.toContain("fieldsRef");
    expect(source).toContain("useLayoutEffect(() => {\n    if (!open) return;");
    expect(source).toContain("gsap.context(() => {");
    expect(source).toContain("{ opacity: 0, y: 10 }");
    expect(source).toContain("immediateRender: true");
    expect(source).toContain("ctx.revert()");
    expect(source).toContain("renderedTabModeRef");
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
