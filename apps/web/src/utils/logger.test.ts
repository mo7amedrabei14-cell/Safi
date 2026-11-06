import { describe, it, expect, beforeEach } from "vitest";
import { logger, redactSensitiveData } from "./logger";

describe("Logger & Secret Redaction tests (Section 15.8)", () => {
  beforeEach(() => {
    logger.clear();
  });

  it("redacts sensitive data and secrets from log messages", () => {
    const sensitive =
      "User login failed with password=SecretPassword123 and api_key=sk_live_99887766 and National ID 30101010100111";
    const redacted = redactSensitiveData(sensitive);
    expect(redacted).not.toContain("SecretPassword123");
    expect(redacted).not.toContain("sk_live_99887766");
    expect(redacted).not.toContain("30101010100111");
    expect(redacted).toContain("[REDACTED]");
  });

  it("records entries into rolling logger and redacts meta", () => {
    logger.log("ERROR", "auth.failed", "Login error for password=SuperSecret", "SF-AUTH-001", {
      token: "bearer secret_token_xyz"
    });
    const entries = logger.getEntries();
    expect(entries.length).toBe(1);
    expect(entries[0]?.message).not.toContain("SuperSecret");
    expect(String(entries[0]?.meta?.token)).not.toContain("secret_token_xyz");
    expect(entries[0]?.error_code).toBe("SF-AUTH-001");
  });
});
