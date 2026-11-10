/**
 * `apps/web/src/studio/engine.ts`
 *
 * Loads the Rust Studio engine (compiled to WebAssembly) and exposes one typed entry point.
 *
 * Nothing in this module computes a metric. It only forwards user actions to Rust and returns the
 * state snapshot that Rust produced, so the browser can never invent or alter a verified number.
 */
import init, { StudioSession, studio_import_csv, studio_import_excel } from "./generated/sf_wasm";
import wasmUrl from "./generated/sf_wasm_bg.wasm?url";

export type StudioMetricStatus = "Verified" | "Stale" | "Unverified";

export interface StudioMetric {
  readonly id: string;
  readonly name: string;
  readonly value: string;
  readonly unit: string | null;
  readonly formula: string;
  readonly formulaVersion: number;
  readonly sourceVersion: number;
  readonly fingerprint: string;
  readonly status: StudioMetricStatus;
  readonly rows: readonly number[];
  readonly columns: readonly string[];
  readonly engineVersion: string;
  readonly computedAtMs: number;
}

export interface StudioColumn {
  readonly name: string;
  readonly type: string;
}

export interface StudioAnomaly {
  readonly kind: string;
  readonly messageKey: string;
  readonly methodKey: string;
  readonly suggestionKey: string;
  readonly column: string | null;
  readonly rows: readonly number[];
}

export interface StudioParameter {
  readonly name: string;
  readonly value: string;
}

export interface StudioState {
  readonly sourceId: string;
  readonly revision: number;
  readonly datasetVersion: number;
  readonly originFingerprint: string;
  readonly columns: readonly StudioColumn[];
  readonly rows: readonly (readonly string[])[];
  readonly metrics: readonly StudioMetric[];
  readonly errors: readonly { readonly id: string; readonly code: string }[];
  readonly anomalies: readonly StudioAnomaly[];
  readonly calculated: readonly string[];
  readonly lastRecomputed: readonly string[];
  readonly activeScenario: string | null;
  readonly scenarios: readonly string[];
  readonly parameters: readonly StudioParameter[];
}

export interface StudioGroup {
  readonly label: string;
  readonly value: string;
}

export interface NarrativeResult {
  readonly text: string;
  readonly missing: readonly string[];
  readonly references: readonly {
    readonly id: string;
    readonly value: string;
    readonly fingerprint: string;
  }[];
}

let engineReady: Promise<void> | null = null;

/** Loads the WASM module once per page and resolves when the engine is usable. */
export function ensureStudioEngine(): Promise<void> {
  engineReady ??= init({ module_or_path: wasmUrl }).then(() => undefined);
  return engineReady;
}

/** Opens a session from a table JSON produced by the import helpers. */
export async function openStudioSession(tableJson: string): Promise<StudioSession> {
  await ensureStudioEngine();
  return new StudioSession(tableJson, Date.now());
}

/** Restores a saved project; Rust re-derives every metric before returning. */
export async function restoreStudioSession(snapshotJson: string): Promise<StudioSession> {
  await ensureStudioEngine();
  return StudioSession.restore(snapshotJson, Date.now());
}

/** Parses CSV bytes in Rust (bounded by the platform limits). */
export async function importCsv(bytes: Uint8Array): Promise<string> {
  await ensureStudioEngine();
  return studio_import_csv(bytes, "Sheet1");
}

/** Parses the first sheet of an XLSX/ODS workbook in Rust. */
export async function importExcel(bytes: Uint8Array): Promise<string> {
  await ensureStudioEngine();
  return studio_import_excel(bytes);
}

export type { StudioSession };
