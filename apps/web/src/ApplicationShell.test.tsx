import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ApplicationShell } from "./ApplicationShell";

describe("application authentication gate", () => {
  it("does not mount product navigation or workspace before sign-in", () => {
    const html = renderToString(createElement(ApplicationShell));

    expect(html).toContain("SAFIDATA IDENTITY");
    expect(html).toContain("Sign In");
    expect(html).not.toContain('class="safi-topbar"');
    expect(html).not.toContain('id="safi-main"');
    expect(html).not.toContain("Command palette");
    expect(html).not.toContain("Cleaning App");
  });

  it("keeps the application locked while an email verification link is handled", () => {
    const html = renderToString(
      createElement(ApplicationShell, {
        initialVerificationToken: "verification-token-for-test"
      })
    );

    expect(html).toContain("safi-modal safi-account-modal");
    expect(html).not.toContain('class="safi-topbar"');
    expect(html).not.toContain('id="safi-main"');
  });

  it("keeps the application locked while a password reset link is handled", () => {
    const html = renderToString(
      createElement(ApplicationShell, {
        initialResetToken: "reset-token-for-test"
      })
    );

    expect(html).toContain("Choose a new password");
    expect(html).not.toContain('class="safi-topbar"');
    expect(html).not.toContain('id="safi-main"');
  });
});
