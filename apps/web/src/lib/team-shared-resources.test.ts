import { describe, expect, it } from "vitest";
import {
  buildEditedSharedResourcePayload,
  buildSharedDashboardDraft,
  buildSharedProjectBlueprintDraft,
  buildSharedRecipeDraft,
  buildSharedWorkflowDraft,
  deriveSafeResourceId,
  parseSharedResourcePayload
} from "./team-shared-resources";
import type { DesktopLocalRecipeRecord, SheetWorkflowMemoryRecord } from "../desktop/studio-bridge";

describe("team-shared-resources (real SafiData privacy-safe team sharing codecs)", () => {
  it("builds & parses a privacy-safe shared recipe without raw customer rows", () => {
    const recipe: DesktopLocalRecipeRecord = {
      id: "rec_cairo_vat_01",
      name: "Cairo Q4 VAT Cleaning",
      description: "Normalizes Egyptian invoice headers and trims whitespace",
      category: "Finance",
      recipe_type: "cleaning",
      origin: "my_recipe",
      source_template_id: null,
      required_columns: ["invoice_id", "amount", "vat"],
      recipe: {
        schema_version: 1,
        id: "rec_cairo_vat_01",
        name: "Cairo Q4 VAT Cleaning",
        steps: [
          { id: "s1", op: { type: "trim_whitespace" }, columns: ["invoice_id"], enabled: true },
          { id: "s2", op: { type: "normalize_arabic" }, columns: ["amount"], enabled: true }
        ]
      },
      created_at_ms: 1700000000000,
      updated_at_ms: 1700000000000
    };

    const draft = buildSharedRecipeDraft(recipe, "Use for monthly closing");
    expect(draft.resourceKind).toBe("recipe");
    expect(draft.resourceId).toBe("recipe-rec_cairo_vat_01");
    expect(draft.payload.schema).toBe("safi.shared_recipe.v1");
    expect(draft.payload.privacy_guarantee).toBe("no_raw_data_rows");
    expect(draft.summaryEn).toContain("2 cleaning steps");

    const parsed = parseSharedResourcePayload({
      resource_kind: draft.resourceKind,
      payload_json: JSON.stringify(draft.payload)
    });
    expect(parsed.privacyVerified).toBe(true);
    expect(parsed.typeLabelEn).toBe("Saved Recipe");
    expect(parsed.teamNotes).toBe("Use for monthly closing");
    expect(parsed.typedPayload?.schema).toBe("safi.shared_recipe.v1");

    // Structured edit (Member updates title & adds cleaning step without touching raw JSON!)
    const editedJson = buildEditedSharedResourcePayload(
      {
        workspace_id: "ws_1",
        resource_kind: "recipe",
        resource_id: draft.resourceId,
        title: draft.title,
        payload_json: JSON.stringify(draft.payload),
        version: 1,
        key_epoch: 1,
        deleted: false,
        dirty: false,
        last_author_user_id: null,
        origin_device_id: "dev_test",
        updated_at: 1700000000
      },
      {
        title: "Cairo Q4 VAT Cleaning v2",
        teamNotes: "Added Arabic normalization",
        appendNormalizeStep: true
      }
    );
    const parsedV2 = parseSharedResourcePayload({
      resource_kind: "recipe",
      payload_json: editedJson
    });
    expect(parsedV2.typedPayload?.schema).toBe("safi.shared_recipe.v1");
    if (parsedV2.typedPayload?.schema === "safi.shared_recipe.v1") {
      expect(parsedV2.typedPayload.steps).toHaveLength(3);
      expect(parsedV2.typedPayload.name).toBe("Cairo Q4 VAT Cleaning v2");
    }
  });

  it("builds & parses shared dashboards, workflows, and project blueprints without raw rows", () => {
    const dash = buildSharedDashboardDraft("Executive Revenue Board", [
      {
        id: "ch1",
        title: "Revenue by Region",
        chartType: "bar",
        function: "SUM",
        column: "amount",
        groupBy: "region"
      }
    ]);
    expect(dash.resourceKind).toBe("dashboard");
    expect(dash.resourceId).toBe(deriveSafeResourceId("dashboard", "Executive Revenue Board"));
    expect(
      parseSharedResourcePayload({
        resource_kind: dash.resourceKind,
        payload_json: JSON.stringify(dash.payload)
      }).summaryEn
    ).toContain("1 interactive chart");

    const wf: SheetWorkflowMemoryRecord = {
      workflowId: "wf_sales_01",
      displayName: "Monthly Sales Reconciliation",
      projectId: "main",
      lastSheetName: "Sales_Oct",
      lastTableId: "tbl_1",
      businessDomain: "sales",
      schemaVersion: 1,
      workflowVersion: 2,
      createdAtMs: 1700000000000,
      updatedAtMs: 1700000000000,
      signature: {
        businessDomain: "sales",
        canonicalConcepts: ["order_id", "amount"],
        structuralHash: "abc"
      },
      questionResolutions: [],
      columnDecisions: [],
      duplicatePolicy: "exact_key",
      missingValuePolicy: "fill_zero",
      businessRules: ["Exclude cancelled"],
      cleaningOperations: [{ stepId: "op1", kind: "trim_column", targetColumn: "order_id" }],
      metrics: [{ id: "m1", name: "Total Sales", unit: "EGP", formula: "SUM(amount)" }],
      dashboardPreferences: {},
      versions: [],
      validationStatus: "verified"
    };
    const wfDraft = buildSharedWorkflowDraft(wf);
    expect(wfDraft.resourceKind).toBe("document");
    expect(wfDraft.payload.privacy_guarantee).toBe("no_raw_data_rows");

    const bp = buildSharedProjectBlueprintDraft({
      title: "Q4 Board Pack Blueprint",
      charts: [],
      slides: [
        {
          id: "sl1",
          title: "Summary",
          body: "Highlights",
          metricId: null,
          chartIndex: null,
          layout: "kpi_split"
        }
      ],
      docBlocks: [],
      metrics: [],
      templateConfig: null
    });
    expect(bp.resourceKind).toBe("project");
    expect(bp.includedItemsEn.some((i) => i.includes("EXCLUDED"))).toBe(true);
  });
});
