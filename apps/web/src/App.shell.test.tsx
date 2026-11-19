import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { App } from "./App";
import { ApplicationShell, SHELL_TABS, renderStudio } from "./ApplicationShell";
import { DOCK_TABS } from "./components/layout/FloatingGlassDock";

const noop = (): void => undefined;

describe("SafiData startup and application shell", () => {
  it("renders only the intro during the initial application commit", () => {
    const html = renderToString(createElement(App, null));

    expect(html).toContain('class="safi-intro-splash"');
    expect(html).toContain("SafiData cinematic startup");
    expect(html).toContain('<bdi dir="ltr">SafiData</bdi>');
    expect(html).not.toContain("صافي البيانات");
    expect(html).not.toContain('class="safi-shell"');
    expect(html).not.toContain("data-safi-ambient");
    expect(html).not.toContain('data-active-tab="home"');
    expect(html).not.toContain("data-safi-word");
  });

  it("keeps ApplicationShell empty until introComplete becomes true and opens public Showcase freely in browser", () => {
    const gated = renderToString(
      createElement(ApplicationShell, { onReady: noop, introComplete: false })
    );
    expect(gated).toBe("");
    expect(gated).not.toContain("safi-auth-modal");
    expect(gated).not.toContain("Sign In");
    expect(gated).not.toContain("safi-shell");

    const html = renderToString(createElement(ApplicationShell, { onReady: noop }));

    expect(SHELL_TABS).toEqual(["home", "cleaning", "recipes", "charts", "library", "owner"]);
    expect(DOCK_TABS).toHaveLength(6);
    expect(html).toContain('class="safi-shell"');
    expect(html).toContain('class="safi-topbar"');
    expect(html).toContain('data-runtime="showcase"');
    expect(html).toContain('data-active-tab="home"');
    expect(html).not.toContain("Owner App");
    expect(html).not.toContain("Quad-Studio");
  });

  it("keeps the PWA name and public share card aligned with the English default", () => {
    const englishManifest = JSON.parse(
      readFileSync(new URL("../public/manifest.webmanifest", import.meta.url), "utf8")
    );
    const arabicManifest = JSON.parse(
      readFileSync(new URL("../public/manifest.ar.webmanifest", import.meta.url), "utf8")
    );
    const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
    const bootLocale = readFileSync(new URL("../public/boot-locale.js", import.meta.url), "utf8");
    const shareCard = readFileSync(new URL("../public/og-preview.svg", import.meta.url), "utf8");

    expect(englishManifest.name).toBe("SafiData");
    expect(englishManifest.short_name).toBe("SafiData");
    expect(englishManifest.lang).toBe("en-US");
    expect(englishManifest.dir).toBe("ltr");
    expect(arabicManifest.name).toBe("صافي البيانات");
    expect(arabicManifest.short_name).toBe("صافي");
    expect(html).toContain('id="safi-manifest"');
    expect(bootLocale).toContain("/manifest.ar.webmanifest");
    expect(shareCard).toContain("SafiData");
    const hasArabicShareCopy = [...shareCard].some((character) => {
      const codePoint = character.codePointAt(0) ?? 0;
      return codePoint >= 0x0600 && codePoint <= 0x06ff;
    });
    expect(hasArabicShareCopy).toBe(false);
  });

  it("uses English as the first-run locale for the default studio", () => {
    const html = renderToString(renderStudio("home", noop));
    expect(html).toContain("Welcome to SafiData");
    expect(html).not.toContain("مرحبًا بك في منصة صافي البيانات");
  });

  it("blocks operational studios on public Showcase browser and renders them strictly inside Desktop runtime", () => {
    const expectations: Array<[string, string]> = [
      ["home", "الإصدار الذهبي المعتمد"],
      ["cleaning", "مختبر التنقية والخصوصية"],
      ["recipes", "محرك الوصفات الحتمية"],
      ["charts", "التحقق من تباين WCAG 2.2 AA"],
      ["library", "مكتبة القوالب وتقارير الجودة"]
    ];

    // Public Showcase (desktopRuntime: false) must NEVER expose operational studios.
    for (const [tab, marker] of expectations) {
      const browserHtml = renderToString(
        renderStudio(tab as (typeof SHELL_TABS)[number], noop, "ar", { desktopRuntime: false })
      );
      if (tab === "home") {
        expect(browserHtml).toContain(marker);
      } else {
        expect(browserHtml).not.toContain(marker);
        expect(browserHtml).toContain("data-download-installer-btn");
      }
    }

    // Packaged Desktop runtime renders every operational studio tab.
    for (const [tab, marker] of expectations) {
      const desktopHtml = renderToString(
        renderStudio(tab as (typeof SHELL_TABS)[number], noop, "ar", { desktopRuntime: true })
      );
      expect(desktopHtml).toContain(marker);
    }

    // Browser / non-owner never renders OwnerDashboard inside renderStudio.
    const browserOwner = renderToString(renderStudio("owner", noop, "ar"));
    expect(browserOwner).not.toContain("safi-owner-frame");

    // Verified Desktop Owner renders the real MFA-gated OwnerDashboard frame.
    const desktopOwner = renderToString(
      renderStudio("owner", noop, "ar", {
        desktopRuntime: true,
        isOwner: true,
        sessionToken: "sfs_test_token"
      })
    );
    expect(desktopOwner).toContain("safi-owner-frame");
    expect(desktopOwner).toContain("Google Authenticator");
  });
});
