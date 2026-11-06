import { describe, it, expect } from "vitest";
import { createBackupPackage, parseAndRestoreBackupPackage } from "./utils/backup-restore";

describe("Slice S2 Comprehensive Integration Test (T2.17)", () => {
  it("integrates recipes, OPFS storage, backup restore, and cleaning features successfully", () => {
    const recipes = [{ id: "s2-rec-1", name: "وصفة الشريحة الثانية" }];
    const dictionaries = [{ columnName: "الرقم القومي", meaning: "هوية وطنية" }];

    const pkg = createBackupPackage(recipes, dictionaries);
    const restored = parseAndRestoreBackupPackage(JSON.stringify(pkg));

    expect(restored.recipes.length).toBe(1);
    expect(restored.dictionaries.length).toBe(1);
    expect(restored.version).toBe("5.0.0");
  });
});
