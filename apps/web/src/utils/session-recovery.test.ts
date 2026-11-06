import { describe, it, expect, beforeEach } from "vitest";
import {
  saveSessionSnapshot,
  restoreSessionSnapshot,
  type SessionSnapshot
} from "./session-recovery";

describe("Auto-Save & Session Recovery (N21 / Section 4.10)", () => {
  beforeEach(() => {
    if (typeof localStorage !== "undefined") {
      localStorage.clear();
    }
  });

  it("saves and restores session snapshot successfully", () => {
    const snapshot: SessionSnapshot = {
      recipeId: "rec-123",
      timestamp: Date.now(),
      stepsCount: 3,
      dataHash: "blake3hash"
    };

    saveSessionSnapshot(snapshot);
    const restored = restoreSessionSnapshot();
    expect(restored).toEqual(snapshot);
  });
});
