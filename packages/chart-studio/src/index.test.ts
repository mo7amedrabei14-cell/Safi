import { describe, expect, it } from "vitest";
import { PACKAGE_ID } from "./index.ts";

describe("@safi/chart-studio workspace boundary", () => {
  it("exports canonical package identifier", () => {
    expect(PACKAGE_ID).toBe("@safi/chart-studio");
  });
});
