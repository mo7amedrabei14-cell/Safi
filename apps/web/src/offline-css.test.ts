import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

describe("Offline CSS Guard", () => {
  it("rejects external URLs in application stylesheets", () => {
    const sourceDirectory = path.resolve(process.cwd(), "apps/web/src");
    const cssFiles = fs
      .readdirSync(sourceDirectory, { recursive: true })
      .filter((entry): entry is string => typeof entry === "string" && entry.endsWith(".css"));

    expect(cssFiles).toContain("index.css");
    for (const cssFile of cssFiles) {
      const content = fs.readFileSync(path.join(sourceDirectory, cssFile), "utf8");
      expect(content, cssFile).not.toMatch(/https?:\/\//i);
    }
  });
});
