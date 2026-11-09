import { describe, it, expect } from "vitest";
import { createBackupPackage, parseAndRestoreBackupPackage } from "./backup-restore";

describe("Encrypted Backup & One-Step Restore (N22 / Section 18.1)", () => {
  it("creates and restores backup package successfully in one step", () => {
    const recipes = [{ id: "rec-1", name: "وصفة اختبار" }];
    const dictionaries = [{ columnName: "الاسم", meaning: "العميل" }];

    const pkg = createBackupPackage(recipes, dictionaries);
    const jsonStr = JSON.stringify(pkg);

    const restored = parseAndRestoreBackupPackage(jsonStr);
    expect(restored.recipes.length).toBe(1);
    expect(restored.dictionaries.length).toBe(1);
    expect(restored.version).toBe("5.0.0");
  });
});
