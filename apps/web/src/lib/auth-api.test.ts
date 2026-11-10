import { describe, expect, it } from "vitest";

import {
  normalizeAuthApiOrigin,
  postSafiAuth,
  resolveAuthApiBaseUrl,
  SafiAuthConfigurationError
} from "./auth-api";

describe("auth API origin", () => {
  it("accepts and normalizes strict HTTPS origins", () => {
    expect(normalizeAuthApiOrigin("https://API.SafiData.app")).toBe("https://api.safidata.app");
    expect(normalizeAuthApiOrigin("  HTTPS://api.safidata.app:443  ")).toBe(
      "https://api.safidata.app"
    );
    expect(normalizeAuthApiOrigin("https://api.safidata.app:8443")).toBe(
      "https://api.safidata.app:8443"
    );
  });

  it("rejects values that are not a bare HTTPS origin", () => {
    for (const value of [
      "",
      "http://api.safidata.app",
      "https://",
      "https://api.safidata.app/",
      "https://api.safidata.app/v1",
      "https://api.safidata.app?x=1",
      "https://api.safidata.app#fragment",
      "https://user:pass@api.safidata.app",
      "https://*.safidata.app",
      "https://api..safidata.app",
      "https://api_safidata.app",
      "https://safidata"
    ]) {
      expect(normalizeAuthApiOrigin(value), value).toBeNull();
    }
  });

  it("rejects IP addresses, localhost, and invalid ports", () => {
    for (const value of [
      "https://127.0.0.1",
      "https://10.0.0.5:8443",
      "https://localhost",
      "https://api.localhost",
      "https://api.safidata.app:0",
      "https://api.safidata.app:65536",
      "https://api.safidata.app:",
      "https://api.safidata.app:abc"
    ]) {
      expect(normalizeAuthApiOrigin(value), value).toBeNull();
    }
  });

  it("keeps the website on same-origin relative paths", () => {
    expect(resolveAuthApiBaseUrl("", false)).toBe("");
    expect(resolveAuthApiBaseUrl("https://api.safidata.app", false)).toBe(
      "https://api.safidata.app"
    );
  });

  it("fails closed when an installed build has no configured origin", () => {
    expect(() => resolveAuthApiBaseUrl("", true)).toThrow(SafiAuthConfigurationError);
    try {
      resolveAuthApiBaseUrl("   ", true);
    } catch (error) {
      expect(error).toBeInstanceOf(SafiAuthConfigurationError);
      expect((error as SafiAuthConfigurationError).code).toBe("SF-SYS-031");
    }
  });

  it("rejects a malformed configured origin even on the website", () => {
    expect(() => resolveAuthApiBaseUrl("http://api.safidata.app", false)).toThrow(
      SafiAuthConfigurationError
    );
  });

  it("sends account requests to the relative path for the website", async () => {
    const calls: string[] = [];
    const original = globalThis.fetch;
    globalThis.fetch = (async (input: RequestInfo | URL) => {
      calls.push(String(input));
      return new Response(JSON.stringify({ ok: true }), {
        status: 200,
        headers: { "Content-Type": "application/json" }
      });
    }) as typeof fetch;
    try {
      await postSafiAuth("/login", { email: "user@example.test" });
    } finally {
      globalThis.fetch = original;
    }
    expect(calls).toEqual(["/v1/auth/login"]);
  });
});
