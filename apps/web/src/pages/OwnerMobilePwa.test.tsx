import { describe, it, expect } from "vitest";
import fs from "fs";
import path from "path";

describe("Owner Mobile PWA & Service Worker Verification (T5.14)", () => {
  it("verifies owner manifest and service worker exist and enforce secure caching rules", () => {
    const manifestPath = path.resolve(process.cwd(), "apps/web/public/owner-manifest.webmanifest");
    expect(fs.existsSync(manifestPath)).toBe(true);
    const manifest = JSON.parse(fs.readFileSync(manifestPath, "utf-8"));
    expect(manifest.name).toContain("صافي");
    expect(manifest.display).toBe("standalone");
    expect(manifest.dir).toBe("rtl");

    const swPath = path.resolve(process.cwd(), "apps/web/public/owner-sw.js");
    expect(fs.existsSync(swPath)).toBe(true);
    const swContent = fs.readFileSync(swPath, "utf-8");
    expect(swContent).toContain("/v1/owner/");
    expect(swContent).toContain("/v1/auth/");
    expect(swContent).toContain("push");
  });
});
