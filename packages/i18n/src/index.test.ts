import { describe, expect, it } from "vitest";
import { PACKAGE_ID } from "./index.ts";

describe("@safi/i18n workspace boundary", () => {
  it("exports canonical package identifier", () => {
    expect(PACKAGE_ID).toBe("@safi/i18n");
  });
});
