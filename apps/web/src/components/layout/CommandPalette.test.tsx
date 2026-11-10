import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { CommandPalette, PALETTE_ITEMS, filterCommands, nextIndex } from "./CommandPalette";

const noop = (): void => undefined;

describe("UI-Phase-5 command palette", () => {
  it("renders nothing while closed", () => {
    const html = renderToString(
      createElement(CommandPalette, { open: false, onClose: noop, onSelectTab: noop })
    );
    expect(html).toBe("");
  });

  it("renders a searchable listbox with every studio and the security audit", () => {
    const html = renderToString(
      createElement(CommandPalette, { open: true, onClose: noop, onSelectTab: noop })
    );

    expect(html).toContain('role="dialog"');
    expect(html).toContain('aria-modal="true"');
    expect(html).toContain("Quick command palette");
    expect(html).toContain('role="combobox"');
    expect(html).toContain('aria-controls="safi-palette-list"');
    expect(html).toContain("Search tools and commands…");
    expect(html.split('role="option"').length - 1).toBe(PALETTE_ITEMS.length);
    expect(PALETTE_ITEMS).toHaveLength(7);

    for (const label of [
      "Website",
      "Cleaning App",
      "Recipes App",
      "Charts App",
      "Library App",
      "Owner App",
      "Quick security audit"
    ]) {
      expect(html).toContain(label);
    }

    expect(html.split('aria-selected="true"').length - 1).toBe(1);
    expect(html).toContain("Esc");
    expect(html).toContain("Enter");
  });

  it("searches with Arabic normalization (alif forms do not matter)", () => {
    expect(filterCommands(PALETTE_ITEMS, "تنظيف").map((item) => item.id)).toEqual(["nav-cleaning"]);
    expect(filterCommands(PALETTE_ITEMS, "الوصفات").map((item) => item.id)).toEqual([
      "nav-recipes"
    ]);
    expect(filterCommands(PALETTE_ITEMS, "فحص").map((item) => item.id)).toEqual(["security-audit"]);
    expect(filterCommands(PALETTE_ITEMS, "   ")).toHaveLength(PALETTE_ITEMS.length);
    expect(filterCommands(PALETTE_ITEMS, "zzz")).toHaveLength(0);
  });

  it("wraps the active index in both directions", () => {
    expect(nextIndex(0, -1, 7)).toBe(6);
    expect(nextIndex(6, 1, 7)).toBe(0);
    expect(nextIndex(3, 1, 7)).toBe(4);
    expect(nextIndex(0, 1, 0)).toBe(0);
  });
});
