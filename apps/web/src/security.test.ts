import { describe, it, expect } from "vitest";
import fs from "fs";
import path from "path";

describe("Security & CSP _headers validation tests (Section 8.2 & 8.5)", () => {
  it("verifies _headers file exists and contains strict CSP and COOP/COEP headers", () => {
    const headersPath = path.resolve(process.cwd(), "apps/web/public/_headers");
    expect(fs.existsSync(headersPath)).toBe(true);
    const content = fs.readFileSync(headersPath, "utf-8");
    expect(content).toContain("Content-Security-Policy: default-src 'self'");
    expect(content).toContain("X-Content-Type-Options: nosniff");
    expect(content).toContain("Cross-Origin-Opener-Policy: same-origin");
    expect(content).toContain("Cross-Origin-Embedder-Policy: require-corp");
    expect(content).not.toContain("unsafe-eval");
  });
});
