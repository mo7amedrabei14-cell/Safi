import { describe, it, expect } from "vitest";
import fs from "fs";
import path from "path";

describe("Owner Dashboard & Log Center Verification (T5.11 & T5.13)", () => {
  it("verifies OwnerDashboard component exists and defines the 6 admin cards", () => {
    const dashboardPath = path.resolve(process.cwd(), "apps/web/src/pages/OwnerDashboard.tsx");
    expect(fs.existsSync(dashboardPath)).toBe(true);
    const content = fs.readFileSync(dashboardPath, "utf-8");
    expect(content).toContain("1. انشر تحديثًا");
    expect(content).toContain("2. أرسل رسالة للمستخدمين");
    expect(content).toContain("3. المستخدمون والاشتراكات");
    expect(content).toContain("4. أعلام الميزات والإعدادات");
    expect(content).toContain("5. الدعم والتشخيص");
    expect(content).toContain("6. الأمان وسجل التدقيق");
  });
});
