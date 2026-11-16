/**
 * Real AI-to-DSL contract for Safi Quad-Studio (`PHASE 2`).
 *
 * Strictly honest: never disguises keyword heuristics or rule templates as AI.
 * By default the desktop shell runs 100% offline with no bundled weights or remote API
 * keys, so natural-language AI translation reports `SF-AIX-001` cleanly unless the user
 * configures an approved OpenAI-compatible endpoint AND consents to sending schema-only
 * metadata (column names, metric IDs, parameter names — NEVER row cells or filenames).
 *
 * Every candidate DSL command — whether from a configured model or edited manually — is
 * treated as untrusted input, validated against the strict command schema, previewed
 * before state mutation, and re-validated in Rust (`studio_apply_command`).
 */

export type StudioDslCommand =
  | {
      op: "define_metric";
      id: string;
      name: string;
      unit: string | null;
      formula: string;
    }
  | {
      op: "update_metric_formula";
      id: string;
      formula: string;
    }
  | {
      op: "set_cell_by_id";
      row_id: number;
      column: string;
      value: string;
    }
  | {
      op: "add_calculated_column";
      name: string;
      formula: string;
    }
  | {
      op: "define_parameter";
      name: string;
      value: string;
    }
  | {
      op: "activate_scenario";
      id: string | null;
    }
  | {
      op: "sort_rows";
      column: string;
      ascending: boolean;
    }
  | {
      op: "filter_rows";
      column: string;
      query: string;
    }
  | {
      op: "delete_row";
      row_id: number;
    };

export interface AiSchemaContext {
  readonly columns: ReadonlyArray<{ name: string; type: string }>;
  readonly metricIds: ReadonlyArray<string>;
  readonly parameterNames: ReadonlyArray<string>;
}

export interface AiProviderConfig {
  readonly kind: "unconfigured" | "openai_compatible";
  readonly endpointUrl: string;
  readonly modelName: string;
  /** Must be explicitly true before sending schema-only metadata to any endpoint. */
  readonly consentedToSchemaContext: boolean;
  /** Optional user-supplied bearer token in memory only (never persisted or hardcoded). */
  readonly apiKey?: string;
}

export const DEFAULT_AI_PROVIDER: AiProviderConfig = {
  kind: "unconfigured",
  endpointUrl: "",
  modelName: "",
  consentedToSchemaContext: false
};

export type AiInterpretationResult =
  | {
      readonly ok: true;
      readonly model: string;
      readonly suggestedSummary: string;
      readonly command: StudioDslCommand;
      readonly dslJson: string;
    }
  | {
      readonly ok: false;
      readonly code: "SF-AIX-001" | "SF-AIX-002";
      readonly blockerDetail: string;
    };

const ALLOWED_OPS = new Set<StudioDslCommand["op"]>([
  "define_metric",
  "update_metric_formula",
  "set_cell_by_id",
  "add_calculated_column",
  "define_parameter",
  "activate_scenario",
  "sort_rows",
  "filter_rows",
  "delete_row"
]);

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasExactKeys(record: Record<string, unknown>, allowed: string[]): boolean {
  const keys = Object.keys(record);
  if (keys.length !== allowed.length) {
    return false;
  }
  return allowed.every((k) => Object.prototype.hasOwnProperty.call(record, k));
}

export function validateUntrustedDslPayload(
  rawJson: string
): { ok: true; command: StudioDslCommand } | { ok: false; code: "SF-AIX-002"; reason: string } {
  let parsed: unknown;
  try {
    parsed = JSON.parse(rawJson);
  } catch {
    return { ok: false, code: "SF-AIX-002", reason: "Malformed JSON command." };
  }
  if (!isRecord(parsed) || typeof parsed.op !== "string") {
    return { ok: false, code: "SF-AIX-002", reason: "Missing string 'op' field." };
  }
  if (!ALLOWED_OPS.has(parsed.op as StudioDslCommand["op"])) {
    return {
      ok: false,
      code: "SF-AIX-002",
      reason: `Unsupported DSL operation '${parsed.op}'.`
    };
  }
  const op = parsed.op as StudioDslCommand["op"];

  switch (op) {
    case "define_metric": {
      if (
        !hasExactKeys(parsed, ["op", "id", "name", "unit", "formula"]) ||
        typeof parsed.id !== "string" ||
        typeof parsed.name !== "string" ||
        (parsed.unit !== null && typeof parsed.unit !== "string") ||
        typeof parsed.formula !== "string"
      ) {
        return { ok: false, code: "SF-AIX-002", reason: "Invalid define_metric fields." };
      }
      return { ok: true, command: parsed as StudioDslCommand };
    }
    case "update_metric_formula": {
      if (
        !hasExactKeys(parsed, ["op", "id", "formula"]) ||
        typeof parsed.id !== "string" ||
        typeof parsed.formula !== "string"
      ) {
        return { ok: false, code: "SF-AIX-002", reason: "Invalid update_metric_formula fields." };
      }
      return { ok: true, command: parsed as StudioDslCommand };
    }
    case "set_cell_by_id": {
      if (
        !hasExactKeys(parsed, ["op", "row_id", "column", "value"]) ||
        typeof parsed.row_id !== "number" ||
        !Number.isInteger(parsed.row_id) ||
        parsed.row_id < 1 ||
        typeof parsed.column !== "string" ||
        typeof parsed.value !== "string"
      ) {
        return { ok: false, code: "SF-AIX-002", reason: "Invalid set_cell_by_id fields." };
      }
      return { ok: true, command: parsed as StudioDslCommand };
    }
    case "add_calculated_column": {
      if (
        !hasExactKeys(parsed, ["op", "name", "formula"]) ||
        typeof parsed.name !== "string" ||
        typeof parsed.formula !== "string"
      ) {
        return { ok: false, code: "SF-AIX-002", reason: "Invalid add_calculated_column fields." };
      }
      return { ok: true, command: parsed as StudioDslCommand };
    }
    case "define_parameter": {
      if (
        !hasExactKeys(parsed, ["op", "name", "value"]) ||
        typeof parsed.name !== "string" ||
        typeof parsed.value !== "string"
      ) {
        return { ok: false, code: "SF-AIX-002", reason: "Invalid define_parameter fields." };
      }
      return { ok: true, command: parsed as StudioDslCommand };
    }
    case "activate_scenario": {
      if (
        !hasExactKeys(parsed, ["op", "id"]) ||
        (parsed.id !== null && typeof parsed.id !== "string")
      ) {
        return { ok: false, code: "SF-AIX-002", reason: "Invalid activate_scenario fields." };
      }
      return { ok: true, command: parsed as StudioDslCommand };
    }
    case "sort_rows": {
      if (
        !hasExactKeys(parsed, ["op", "column", "ascending"]) ||
        typeof parsed.column !== "string" ||
        typeof parsed.ascending !== "boolean"
      ) {
        return { ok: false, code: "SF-AIX-002", reason: "Invalid sort_rows fields." };
      }
      return { ok: true, command: parsed as StudioDslCommand };
    }
    case "filter_rows": {
      if (
        !hasExactKeys(parsed, ["op", "column", "query"]) ||
        typeof parsed.column !== "string" ||
        typeof parsed.query !== "string"
      ) {
        return { ok: false, code: "SF-AIX-002", reason: "Invalid filter_rows fields." };
      }
      return { ok: true, command: parsed as StudioDslCommand };
    }
    case "delete_row": {
      if (
        !hasExactKeys(parsed, ["op", "row_id"]) ||
        typeof parsed.row_id !== "number" ||
        !Number.isInteger(parsed.row_id) ||
        parsed.row_id < 1
      ) {
        return { ok: false, code: "SF-AIX-002", reason: "Invalid delete_row fields." };
      }
      return { ok: true, command: parsed as StudioDslCommand };
    }
  }
}

export function previewDslCommand(command: StudioDslCommand, language: "en" | "ar"): string {
  switch (command.op) {
    case "define_metric":
      return language === "ar"
        ? `إنشاء مؤشر جديد "${command.name}" (${command.id}) بالمعادلة: ${command.formula}`
        : `Define verified metric "${command.name}" (${command.id}) = ${command.formula}`;
    case "update_metric_formula":
      return language === "ar"
        ? `تحديث معادلة المؤشر (${command.id}) إلى: ${command.formula}`
        : `Update metric "${command.id}" formula to ${command.formula}`;
    case "set_cell_by_id":
      return language === "ar"
        ? `تعديل الخلية (صف #${command.row_id}، عمود "${command.column}") إلى "${command.value}"`
        : `Set cell Row #${command.row_id}, column "${command.column}" -> "${command.value}"`;
    case "add_calculated_column":
      return language === "ar"
        ? `إضافة عمود محسوب "${command.name}" = ${command.formula}`
        : `Add calculated column "${command.name}" = ${command.formula}`;
    case "define_parameter":
      return language === "ar"
        ? `تعريف معامل "${command.name}" = ${command.value}`
        : `Define parameter "${command.name}" = ${command.value}`;
    case "activate_scenario":
      return language === "ar"
        ? `تفعيل السيناريو: ${command.id ?? "الأساسي"}`
        : `Activate scenario: ${command.id ?? "Baseline"}`;
    case "sort_rows":
      return language === "ar"
        ? `فرز الصفوف حسب "${command.column}" (${command.ascending ? "تصاعديًا" : "تنازليًا"})`
        : `Sort rows by "${command.column}" (${command.ascending ? "ascending" : "descending"})`;
    case "filter_rows":
      return language === "ar"
        ? `تصفية "${command.column}" بالنص "${command.query}"`
        : `Filter "${command.column}" matching "${command.query}"`;
    case "delete_row":
      return language === "ar"
        ? `حذف الصف #${command.row_id} (بدون إعادة استخدام معرّفه)`
        : `Delete Row #${command.row_id} (identity never reused)`;
  }
}

export interface OutboundAiUserPayload {
  readonly request: string;
  readonly schema: {
    readonly columns: ReadonlyArray<{ name: string; type: string }>;
    readonly metrics: ReadonlyArray<string>;
    readonly parameters: ReadonlyArray<string>;
  };
}

/**
 * Constructs the exact outbound user payload sent to a configured AI provider.
 * Contains ONLY the user's prompt text, column names & types, metric IDs, and
 * parameter names — never row values, cell contents, or filenames.
 */
export function buildOutboundAiPayload(
  prompt: string,
  schema: AiSchemaContext
): OutboundAiUserPayload {
  return {
    request: prompt.trim(),
    schema: {
      columns: schema.columns.map((c) => ({ name: c.name, type: c.type })),
      metrics: [...schema.metricIds],
      parameters: [...schema.parameterNames]
    }
  };
}

/**
 * Validates that the configured AI endpoint matches the pinned local Ollama loopback origin
 * allowed by the desktop CSP (`http://127.0.0.1:11434` or `http://localhost:11434`, normalized to
 * `http://127.0.0.1:11434`).
 *
 * Remote HTTPS cloud AI endpoints are intentionally disabled in this phase and deferred to a
 * dedicated cloud-AI phase with explicit CSP & key-management policies.
 */
export function validateAiEndpointUrl(
  rawUrl: string
):
  | { readonly ok: true; readonly normalizedUrl: string }
  | { readonly ok: false; readonly reason: string } {
  const trimmed = rawUrl.trim();
  if (!trimmed) {
    return { ok: false, reason: "AI endpoint URL is empty." };
  }
  let parsed: URL;
  try {
    parsed = new URL(trimmed);
  } catch {
    return { ok: false, reason: "AI endpoint URL is not a valid URL." };
  }
  if (
    parsed.protocol === "http:" &&
    (parsed.origin === "http://127.0.0.1:11434" || parsed.origin === "http://localhost:11434")
  ) {
    const normalized = new URL(
      parsed.pathname + parsed.search + parsed.hash,
      "http://127.0.0.1:11434"
    );
    return { ok: true, normalizedUrl: normalized.toString() };
  }
  return {
    ok: false,
    reason:
      "Desktop CSP in this phase permits ONLY the local Ollama loopback origin (http://127.0.0.1:11434). Optional remote cloud AI is deferred to a separate phase."
  };
}

/**
 * Translates a natural-language request using ONLY a real, explicitly configured & consented
 * AI provider. When running in offline desktop mode with no local/approved model configured,
 * returns `SF-AIX-001` honestly without faking AI responses.
 */
export async function interpretPromptWithRealAi(
  prompt: string,
  schema: AiSchemaContext,
  provider: AiProviderConfig,
  signal?: AbortSignal
): Promise<AiInterpretationResult> {
  const trimmed = prompt.trim();
  if (!trimmed) {
    return {
      ok: false,
      code: "SF-AIX-002",
      blockerDetail: "Prompt is empty."
    };
  }

  if (
    provider.kind === "unconfigured" ||
    !provider.endpointUrl.trim() ||
    !provider.modelName.trim()
  ) {
    return {
      ok: false,
      code: "SF-AIX-001",
      blockerDetail:
        "No local LLM weights or local Ollama daemon (http://127.0.0.1:11434) is configured in this offline desktop environment (optional cloud AI is deferred to a separate phase). Enter or edit a structured DSL command below to execute verified actions safely."
    };
  }

  const endpointCheck = validateAiEndpointUrl(provider.endpointUrl);
  if (!endpointCheck.ok) {
    return {
      ok: false,
      code: "SF-AIX-001",
      blockerDetail: endpointCheck.reason
    };
  }

  if (!provider.consentedToSchemaContext) {
    return {
      ok: false,
      code: "SF-AIX-001",
      blockerDetail:
        "Explicit consent is required before sending your prompt text, column names and types, metric IDs, and parameter names to the configured provider."
    };
  }

  // Send ONLY prompt + schema identifiers — zero raw rows, cell values, or file paths.
  const outboundUserPayload = buildOutboundAiPayload(trimmed, schema);

  try {
    const headers: Record<string, string> = { "Content-Type": "application/json" };
    if (provider.apiKey && provider.apiKey.trim()) {
      headers.Authorization = `Bearer ${provider.apiKey.trim()}`;
    }

    const response = await fetch(endpointCheck.normalizedUrl, {
      method: "POST",
      headers,
      ...(signal !== undefined ? { signal } : {}),
      body: JSON.stringify({
        model: provider.modelName,
        temperature: 0,
        messages: [
          {
            role: "system",
            content:
              "Output strict JSON ONLY matching one Safi StudioCommand (define_metric, update_metric_formula, set_cell_by_id, add_calculated_column, define_parameter, activate_scenario, sort_rows, filter_rows, delete_row). Never compute numeric results or invent values."
          },
          {
            role: "user",
            content: JSON.stringify(outboundUserPayload)
          }
        ]
      })
    });

    if (!response.ok) {
      return {
        ok: false,
        code: "SF-AIX-001",
        blockerDetail: `Configured AI endpoint responded with HTTP ${response.status} (offline/CSP sandbox or endpoint unreachable).`
      };
    }

    const body = (await response.json()) as {
      choices?: Array<{ message?: { content?: string } }>;
    };
    const rawContent = body.choices?.[0]?.message?.content?.trim() ?? "";
    const validated = validateUntrustedDslPayload(rawContent);
    if (!validated.ok) {
      return {
        ok: false,
        code: "SF-AIX-002",
        blockerDetail: validated.reason
      };
    }

    return {
      ok: true,
      model: provider.modelName,
      suggestedSummary: previewDslCommand(validated.command, "en"),
      command: validated.command,
      dslJson: JSON.stringify(validated.command, null, 2)
    };
  } catch (err) {
    return {
      ok: false,
      code: "SF-AIX-001",
      blockerDetail:
        err instanceof Error
          ? `AI request blocked or unreachable in sandbox: ${err.message}`
          : "AI provider unreachable in offline sandbox."
    };
  }
}
