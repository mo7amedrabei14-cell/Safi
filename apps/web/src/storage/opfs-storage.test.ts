import { describe, it, expect, beforeEach } from "vitest";
import { EncryptedOPFSStorage } from "./opfs-storage";

describe("OPFS Encrypted Local Storage & Passphrase tests (Section 8.3)", () => {
  let storage: EncryptedOPFSStorage;

  beforeEach(() => {
    storage = new EncryptedOPFSStorage();
  });

  it("saves and restores unencrypted recipes successfully", async () => {
    const recipe = { id: "rec-1", name: "وصفة تجريبية", steps: [] };
    await storage.saveRecipe("rec-1", "وصفة تجريبية", recipe);

    const loaded = await storage.loadRecipe("rec-1");
    expect(loaded).toEqual(recipe);
  });

  it("saves and restores encrypted recipes with correct passphrase", async () => {
    const recipe = { id: "rec-2", name: "وصفة سريّة", steps: [] };
    const pass = "secret-pass-123";
    await storage.saveRecipe("rec-2", "وصفة سريّة", recipe, pass);

    // Loading without pass should fail or throw SF-RCP-002
    await expect(storage.loadRecipe("rec-2")).rejects.toThrow();

    // Loading with wrong pass should fail
    await expect(storage.loadRecipe("rec-2", "wrong-pass")).rejects.toThrow();

    // Loading with correct pass should succeed
    const loaded = await storage.loadRecipe("rec-2", pass);
    expect(loaded).toEqual(recipe);
  });
});
