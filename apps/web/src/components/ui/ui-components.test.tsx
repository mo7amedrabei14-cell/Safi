import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { BlurWordText, splitIntoWords } from "./BlurWordText";
import { DecryptedMonoText } from "./DecryptedMonoText";
import { SpotlightCard } from "./SpotlightCard";
import { MagneticButton } from "./MagneticButton";
import { CountUpMetric, formatMetricValue } from "./CountUpMetric";

describe("UI-Phase-3 component kit", () => {
  it("splits blur text into whole words, never into Arabic characters", () => {
    const phrase = "مرحبًا بك في منصة صافي البيانات";
    const html = renderToString(createElement(BlurWordText, { text: phrase }));

    expect(splitIntoWords(phrase)).toHaveLength(6);
    expect(html.split('data-safi-word="true"').length - 1).toBe(6);
    expect(html).toContain(">البيانات<");
    expect(html).toContain("صافي");
    expect(html.startsWith("<p")).toBe(true);

    const heading = renderToString(createElement(BlurWordText, { text: phrase, as: "h2" }));
    expect(heading.startsWith("<h2")).toBe(true);

    expect(splitIntoWords("  صافي   البيانات  ")).toEqual(["صافي", "البيانات"]);
    expect(splitIntoWords("")).toEqual([]);
  });

  it("renders the cipher fingerprint in mono tabular digits with the real value", () => {
    const value = "b3a8c1d94f0e2a7c";
    const html = renderToString(createElement(DecryptedMonoText, { value }));

    expect(html).toContain(value);
    expect(html).toContain("safi-mono");
    expect(html).toContain("tabular-nums");
    expect(html).toContain('data-safi-decrypted="true"');
    expect(html).toContain(`title="${value}"`);
  });

  it("renders the spotlight card with its glass class and padding", () => {
    const html = renderToString(
      createElement(SpotlightCard, {
        padding: 18,
        children: createElement("p", null, "محتوى البطاقة")
      })
    );

    expect(html).toContain("safi-spotlight");
    expect(html).toContain("padding:18px");
    expect(html).toContain("محتوى البطاقة");
    expect(html).not.toContain("--mouse-x"); // variables are written on pointer move only
  });

  it("renders a magnetic button that meets the 44px touch target and star border", () => {
    const html = renderToString(
      createElement(MagneticButton, { ariaLabel: "ابدأ الآن", children: "ابدأ الآن" })
    );

    expect(html).toContain("<button");
    expect(html).toContain('type="button"');
    expect(html).toContain("safi-star-border");
    expect(html).toContain("min-height:44px");
    expect(html).toContain("min-width:44px");
    expect(html).toContain('aria-label="ابدأ الآن"');
    expect(html).toContain('data-safi-magnetic="true"');
  });

  it("formats count-up metrics with grouping, decimals and suffixes", () => {
    expect(formatMetricValue(125000)).toBe("125,000");
    expect(formatMetricValue(99.9, 1)).toBe("99.9");

    const html = renderToString(
      createElement(CountUpMetric, { value: 125000, suffix: "صف/ث", label: "سرعة المعالجة" })
    );

    expect(html).toContain("125,000");
    expect(html).toContain("صف/ث");
    expect(html).toContain("سرعة المعالجة");
    expect(html).toContain("tabular-nums");
    expect(html).toContain('data-metric-value="125000"');
  });
});
