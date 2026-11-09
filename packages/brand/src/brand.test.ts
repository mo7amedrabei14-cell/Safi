import { describe, expect, it } from "vitest";
import { BRAND_CONFIG } from "./index.ts";

describe("@safi/brand", () => {
  it("exposes the locked v5.0 internal name and display brand", () => {
    expect(BRAND_CONFIG.internalName).toBe("safi");
    expect(BRAND_CONFIG.displayName).toBe("SafiData");
    expect(BRAND_CONFIG.displayNameAr).toBe("صافي البيانات");
    expect(BRAND_CONFIG.displayNameEn).toBe("SafiData");
  });
});
