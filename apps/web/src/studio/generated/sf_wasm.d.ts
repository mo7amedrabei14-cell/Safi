/* tslint:disable */
/* eslint-disable */
/**
 * Imports CSV bytes into a table JSON the session can open.
 */
export function studio_import_csv(data: Uint8Array, sheet_name: string): string;
/**
 * Imports the first table of the first sheet of an XLSX/ODS workbook.
 */
export function studio_import_excel(data: Uint8Array): string;
/**
 * Returns the display form of a metric value for tests and tooling.
 */
export function studio_display_value(value: string): string;
/**
 * Initializes panic hook to intercept any Rust panics and map them to `SF-UNK-001`
 * without leaking any user cell values, file names, or column names (`AGENTS.md` Rule #4).
 */
export function init_wasm(): void;
/**
 * 1. `analyze`: Analyzes raw file bytes (CSV or spreadsheet) and returns JSON profile & quality.
 */
export function analyze(data: Uint8Array, _filename: string): any;
/**
 * 2. `suggest_steps`: Suggests cleaning operations based on profile JSON.
 */
export function suggest_steps(profile_json: string): any;
/**
 * 3. `apply`: Applies a cleaning recipe to data and returns cleaned table & deltas.
 */
export function apply(data: Uint8Array, recipe_json: string): any;
/**
 * 4. `undo`: Reverts the last cleaning operation from delta history.
 */
export function undo(data: Uint8Array): any;
/**
 * 5. `export_xlsx`: Exports cleaned data to a real Excel `.xlsx` file with RTL layout,
 * frozen header row, second sheet `سجل التغييرات`, and formula injection neutralization (`=`, `+`, `-`, `@`).
 */
export function export_xlsx(data: Uint8Array): Uint8Array;
/**
 * 6. `report_html`: Generates an RTL Arabic HTML report summarizing data quality and cleaning results.
 */
export function report_html(rows_before: number, rows_after: number, cells_corrected: number): string;
/**
 * Generates an instant RTL Arabic vector PDF report as byte array.
 */
export function generate_pdf_report(rows_before: number, rows_after: number, duplicates_removed: number, cells_corrected: number): Uint8Array;
/**
 * One live Studio project.
 */
export class StudioSession {
  free(): void;
  /**
   * Returns the full UI state: table, tokens, errors, anomalies, scenarios.
   */
  state_json(): string;
  /**
   * Adds a scenario from a JSON object of parameter overrides.
   */
  add_scenario(id: string, name: string, parameters_json: string, clock_ms: number): string;
  /**
   * Executes a validated Studio DSL command (the only path an AI interpreter may use).
   */
  apply_command(json: string, clock_ms: number): string;
  /**
   * Defines a metric and computes its verified token.
   */
  define_metric(id: string, name: string, unit: string | undefined, formula: string, clock_ms: number): string;
  /**
   * Returns the persisted project JSON.
   */
  snapshot_json(): string;
  /**
   * Registers a What-If parameter.
   */
  define_parameter(name: string, value: string, clock_ms: number): string;
  /**
   * Activates a scenario, or `None` to return to the defaults.
   */
  activate_scenario(id: string | undefined, clock_ms: number): string;
  /**
   * Appends a calculated column.
   */
  add_calculated_column(name: string, formula: string, clock_ms: number): string;
  /**
   * Creates a session over an imported table (JSON produced by the import functions below).
   */
  constructor(table_json: string, clock_ms: number);
  /**
   * Redoes the last undone change.
   */
  redo(clock_ms: number): string;
  /**
   * Undoes the last change (the original dataset is never touched).
   */
  undo(clock_ms: number): string;
  /**
   * Exports the self-contained `.safi.html` capsule (no JavaScript inside).
   */
  capsule(options_json: string): string;
  /**
   * Groups and aggregates in Rust for the dashboard and the capsule.
   */
  grouped(_function: string, column: string, group_by: string): string;
  /**
   * Restores a saved project and re-derives every metric before returning it.
   */
  static restore(snapshot_json: string, clock_ms: number): StudioSession;
  /**
   * Replaces one cell and recomputes only dependent metrics.
   */
  set_cell(row: number, column: string, raw: string, clock_ms: number): string;
  /**
   * Resolves `{{metric:ID}}` placeholders from verified tokens only.
   */
  narrative(template: string): string;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
  readonly memory: WebAssembly.Memory;
  readonly __wbg_studiosession_free: (a: number, b: number) => void;
  readonly studio_display_value: (a: number, b: number) => [number, number];
  readonly studio_import_csv: (a: number, b: number, c: number, d: number) => [number, number, number, number];
  readonly studio_import_excel: (a: number, b: number) => [number, number, number, number];
  readonly studiosession_activate_scenario: (a: number, b: number, c: number, d: number) => [number, number, number, number];
  readonly studiosession_add_calculated_column: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
  readonly studiosession_add_scenario: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => [number, number, number, number];
  readonly studiosession_apply_command: (a: number, b: number, c: number, d: number) => [number, number, number, number];
  readonly studiosession_capsule: (a: number, b: number, c: number) => [number, number, number, number];
  readonly studiosession_define_metric: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number, i: number, j: number) => [number, number, number, number];
  readonly studiosession_define_parameter: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
  readonly studiosession_grouped: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => [number, number, number, number];
  readonly studiosession_narrative: (a: number, b: number, c: number) => [number, number, number, number];
  readonly studiosession_new: (a: number, b: number, c: number) => [number, number, number];
  readonly studiosession_redo: (a: number, b: number) => [number, number, number, number];
  readonly studiosession_restore: (a: number, b: number, c: number) => [number, number, number];
  readonly studiosession_set_cell: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => [number, number, number, number];
  readonly studiosession_snapshot_json: (a: number) => [number, number, number, number];
  readonly studiosession_state_json: (a: number) => [number, number, number, number];
  readonly studiosession_undo: (a: number, b: number) => [number, number, number, number];
  readonly analyze: (a: number, b: number, c: number, d: number) => [number, number, number];
  readonly apply: (a: number, b: number, c: number, d: number) => [number, number, number];
  readonly export_xlsx: (a: number, b: number) => [number, number, number, number];
  readonly generate_pdf_report: (a: number, b: number, c: number, d: number) => [number, number, number, number];
  readonly report_html: (a: number, b: number, c: number) => [number, number, number, number];
  readonly suggest_steps: (a: number, b: number) => [number, number, number];
  readonly undo: (a: number, b: number) => [number, number, number];
  readonly init_wasm: () => void;
  readonly __wbindgen_export_0: WebAssembly.Table;
  readonly __wbindgen_malloc: (a: number, b: number) => number;
  readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
  readonly __externref_table_dealloc: (a: number) => void;
  readonly __wbindgen_free: (a: number, b: number, c: number) => void;
  readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;
/**
* Instantiates the given `module`, which can either be bytes or
* a precompiled `WebAssembly.Module`.
*
* @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
*
* @returns {InitOutput}
*/
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
* If `module_or_path` is {RequestInfo} or {URL}, makes a request and
* for everything else, calls `WebAssembly.instantiate` directly.
*
* @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
*
* @returns {Promise<InitOutput>}
*/
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
