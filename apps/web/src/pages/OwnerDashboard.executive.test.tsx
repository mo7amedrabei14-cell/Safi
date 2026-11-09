import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToString } from "react-dom/server";
import { OwnerDashboard } from "./OwnerDashboard";

describe("Executive owner dashboard", () => {
  it("shows the strict owner identity and all three time horizons", () => {
    const html = renderToString(createElement(OwnerDashboard, { lang: "en" }));

    expect(html).toContain("mo7amed.rabei14@gmail.com");
    expect(html).toContain('aria-label="Time horizon filter"');
    expect(html).toContain('data-horizon="monthly"');
    expect(html).toContain("Monthly");
    expect(html).toContain("Quarterly (Q1–Q4)");
    expect(html).toContain("Annual");
    expect(html).toContain("MoM%");
  });

  it("lists every detailed user segment and its price", () => {
    const html = renderToString(createElement(OwnerDashboard, { lang: "en" }));

    for (const label of [
      "Total users",
      "Active 7-day trials",
      "Free plan users",
      "Individual monthly subscribers",
      "Individual annual subscribers",
      "Organization seats",
      "Friend VIP members",
      "Credit pack buyers (25 ops)",
      "Pay-per-file buyers",
      "Monthly recurring revenue (USD)",
      "Monthly recurring revenue (EGP)",
      "Annual recurring revenue (USD)",
      "Annual recurring revenue (EGP)",
      "Trial-to-paid conversion",
      "Churn rate",
      "Rows cleaned",
      "PII fields masked",
      "AI calls used"
    ]) {
      expect(html).toContain(label);
    }

    expect(html).toContain("1,000 EGP / $20");
    expect(html).toContain("8,400 EGP / $168");
    expect(html).toContain("850 EGP / $17");
    expect(html).toContain("400");
  });

  it("keeps the Friend VIP manager and the full subscriber action table", () => {
    const html = renderToString(createElement(OwnerDashboard, { lang: "en" }));

    expect(html).toContain("Owner Friend VIP access manager");
    expect(html).toContain("Grant full Friend access for free");
    expect(html).toContain("Subscriber and operations management");
    expect(html).toContain("Promote to Friend VIP");
    expect(html).toContain("Extend trial");
    expect(html).toContain("Cancel subscription");
    expect(html).toContain("Issue refund (within 3 days)");
    expect(html).toContain("Suspend account");
    expect(html).toContain('type="search"');
  });

  it("keeps the six-card console and refuses every non-owner viewer", () => {
    const ownerHtml = renderToString(createElement(OwnerDashboard, { lang: "en" }));
    expect(ownerHtml).toContain("1. Publish an update");

    const denied = renderToString(
      createElement(OwnerDashboard, { lang: "en", viewerEmail: "friend@example.test" })
    );
    expect(denied).toContain("reserved for the exclusive owner");
    expect(denied).toContain("mo7amed.rabei14@gmail.com");
    expect(denied).not.toContain("1. Publish an update");
  });

  it("renders Arabic executive copy and never leaks Arabic into English", () => {
    const arabic = renderToString(createElement(OwnerDashboard, { lang: "ar" }));
    expect(arabic).toContain("المالك الحصري المعتمد");
    expect(arabic).toContain("ربع سنوي (Q1–Q4)");
    expect(arabic).toContain("إدارة صلاحية أصدقاء المالك");
    expect(arabic).toContain("تمديد التجربة");

    const english = renderToString(createElement(OwnerDashboard, { lang: "en" }));
    expect(english).not.toMatch(/[\u0600-\u06FF]/);
  });
});
