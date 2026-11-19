import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { afterEach, describe, expect, it } from "vitest";
import { ApplicationShell } from "./ApplicationShell";

describe("Phase 01 runtime boundary: public browser Showcase vs Desktop app", () => {
  afterEach(() => {
    delete (globalThis as { window?: unknown }).window;
  });

  it("opens the public browser Showcase freely without mandatory login and hides Desktop/Owner controls", () => {
    const html = renderToString(createElement(ApplicationShell));

    expect(html).toContain('class="safi-topbar"');
    expect(html).toContain('id="safi-main"');
    expect(html).toContain('data-runtime="showcase"');
    expect(html).not.toContain("Quad-Studio");
    expect(html).not.toContain("Owner App");
    expect(html).not.toContain('data-tab-id="owner"');
    expect(html).not.toContain('data-tab-id="studio"');
  });

  it("mandates authentication inside the packaged Desktop runtime before mounting workspace", () => {
    (globalThis as { window?: unknown }).window = {
      __TAURI_INTERNALS__: { invoke: async () => ({}) }
    };

    const html = renderToString(createElement(ApplicationShell));

    expect(html).toContain("SAFIDATA IDENTITY");
    expect(html).toContain("Sign In");
    expect(html).not.toContain('class="safi-topbar"');
    expect(html).not.toContain('id="safi-main"');
    expect(html).not.toContain("Owner App");
  });

  it("keeps Desktop locked while an email verification link is handled", () => {
    (globalThis as { window?: unknown }).window = {
      __TAURI_INTERNALS__: { invoke: async () => ({}) }
    };

    const html = renderToString(
      createElement(ApplicationShell, {
        initialVerificationToken: "verification-token-for-test"
      })
    );

    expect(html).toContain("safi-modal safi-account-modal");
    expect(html).not.toContain('class="safi-topbar"');
    expect(html).not.toContain('id="safi-main"');
  });

  it("keeps Desktop locked while a password reset link is handled", () => {
    (globalThis as { window?: unknown }).window = {
      __TAURI_INTERNALS__: { invoke: async () => ({}) }
    };

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
