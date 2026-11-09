import { describe, expect, it } from "vitest";
import { BRAND_CONFIG } from "@safi/brand";

describe("@safi/web workspace boundary", () => {
  it("resolves central SafiData brand configuration", () => {
    expect(BRAND_CONFIG.internalName).toBe("safi");
    expect(BRAND_CONFIG.displayName).toBe("صافي البيانات | SafiData");
  });
});
