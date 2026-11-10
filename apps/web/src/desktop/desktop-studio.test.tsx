import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { DesktopStudio } from "./DesktopStudio";
import { exportCapsule, isDesktopRuntime, openProject, setCell } from "./studio-bridge";
import { DOCK_TABS, STUDIO_TAB } from "../components/layout/FloatingGlassDock";

describe("desktop studio surface", () => {
  it("refuses to render the workspace outside the desktop app", () => {
    const html = renderToString(createElement(DesktopStudio, { lang: "ar" }));

    expect(isDesktopRuntime()).toBe(false);
    expect(html).toContain("safi-desktop-studio--locked");
    expect(html).toContain("الاستوديو يعمل داخل التطبيق");
    expect(html).not.toContain("safi-desktop-studio__table");
  });

  it("keeps the engine unreachable from a browser tab", async () => {
    await expect(openProject("main", "/tmp/input.csv")).rejects.toThrow("SF-SYS-010");
    await expect(setCell("main", 0, "amount", "1")).rejects.toThrow("SF-SYS-010");
    await expect(exportCapsule("main", "/tmp/out.safi.html", "{}")).rejects.toThrow("SF-SYS-010");
  });

  it("adds the studio tab only through the desktop tab list", () => {
    expect(DOCK_TABS.some((tab) => tab.id === "studio")).toBe(false);
    expect(STUDIO_TAB.id).toBe("studio");
    expect([...DOCK_TABS, STUDIO_TAB].some((tab) => tab.id === "studio")).toBe(true);
  });
});
