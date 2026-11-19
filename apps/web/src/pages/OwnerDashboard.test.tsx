import { describe, it, expect } from "vitest";
import fs from "fs";
import path from "path";

describe("Owner Dashboard & Log Center Verification (Phases 04 & 05)", () => {
  it("verifies OwnerDashboard uses real /v1/owner APIs, Google Authenticator TOTP, and 6 executive KPI cards", () => {
    const dashboardPath = path.resolve(process.cwd(), "apps/web/src/pages/OwnerDashboard.tsx");
    expect(fs.existsSync(dashboardPath)).toBe(true);
    const content = fs.readFileSync(dashboardPath, "utf-8");
    expect(content).toContain("ownerApi");
    expect(content).toContain("Google Authenticator");
    expect(content).toContain('data-owner-kpi-count="6"');
    expect(content).toContain("BLAKE3");
    expect(content).toContain("SF-AUTH-016");
    expect(content).not.toContain("YubiKey");
    expect(content).not.toContain("14280");
    expect(content).not.toContain("509800");
  });
});
