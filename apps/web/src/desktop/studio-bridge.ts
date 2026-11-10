/**
 * `apps/web/src/desktop/studio-bridge.ts`
 *
 * Typed IPC client for the desktop Studio commands implemented in
 * `crates/sf-desktop/src/studio.rs` and registered in the Tauri shell.
 *
 * The web build stays a showcase: the engine executes only inside the desktop app, and this
 * module notices the desktop runtime by the IPC bridge Tauri always injects (even with
 * `withGlobalTauri: false`). In a browser every call fails fast with `SF-SYS-010`, so nothing
 * here can run on the public site.
 */

interface TauriInvoke {
  (command: string, args?: Record<string, unknown>): Promise<unknown>;
}

interface TauriWindow {
  __TAURI_INTERNALS__?: { invoke?: TauriInvoke };
}

export interface DesktopMetric {
  readonly id: string;
  readonly name: string;
  readonly value: string;
  readonly unit: string | null;
  readonly formula: string;
  readonly formulaVersion: number;
  readonly sourceVersion: number;
  readonly fingerprint: string;
  readonly status: string;
  readonly rows: readonly number[];
  readonly columns: readonly string[];
  readonly engineVersion: string;
  readonly computedAtMs: number;
}

export interface DesktopColumn {
  readonly name: string;
  readonly type: string;
}

export interface DesktopAnomaly {
  readonly kind: string;
  readonly messageKey: string;
  readonly methodKey: string;
  readonly suggestionKey: string;
  readonly column: string | null;
  readonly rows: readonly number[];
}

export interface DesktopParameter {
  readonly name: string;
  readonly value: string;
}

export interface DesktopStudioState {
  readonly sourceId: string;
  readonly revision: number;
  readonly datasetVersion: number;
  readonly originFingerprint: string;
  readonly columns: readonly DesktopColumn[];
  readonly rows: readonly (readonly string[])[];
  readonly metrics: readonly DesktopMetric[];
  readonly errors: readonly { readonly id: string; readonly code: string }[];
  readonly anomalies: readonly DesktopAnomaly[];
  readonly calculated: readonly string[];
  readonly lastRecomputed: readonly string[];
  readonly activeScenario: string | null;
  readonly scenarios: readonly string[];
  readonly parameters: readonly DesktopParameter[];
}

export interface DesktopGroup {
  readonly label: string;
  readonly value: string;
}

function bridge(): { invoke?: TauriInvoke } | undefined {
  if (typeof window === "undefined") {
    return undefined;
  }
  return (window as unknown as TauriWindow).__TAURI_INTERNALS__;
}

/** True only inside the packaged desktop app, never in a browser tab. */
export function isDesktopRuntime(): boolean {
  return typeof bridge()?.invoke === "function";
}

async function call(command: string, args: Record<string, unknown> = {}): Promise<string> {
  const invoke = bridge()?.invoke;
  if (invoke === undefined) {
    throw new Error("SF-SYS-010");
  }
  const result = await invoke(command, args);
  return typeof result === "string" ? result : JSON.stringify(result);
}

async function stateCall(command: string, args: Record<string, unknown>): Promise<DesktopStudioState> {
  return JSON.parse(await call(command, args)) as DesktopStudioState;
}

/** Opens the native open dialog; returns the chosen path or `null`. */
export async function pickInputFile(): Promise<string | null> {
  return JSON.parse(await call("pick_input_file")) as string | null;
}

/** Opens the native save dialog; returns the chosen path or `null`. */
export async function pickOutputFile(): Promise<string | null> {
  return JSON.parse(await call("pick_output_file")) as string | null;
}

/** Imports a spreadsheet from disk into a new session. */
export async function openProject(project: string, path: string): Promise<DesktopStudioState> {
  return stateCall("studio_open_file", { project, path });
}

/** Reads the current state of an open project. */
export async function projectState(project: string): Promise<DesktopStudioState> {
  return stateCall("studio_state", { project });
}

/** Replaces one cell and returns the recomputed state. */
export async function setCell(
  project: string,
  row: number,
  column: string,
  value: string
): Promise<DesktopStudioState> {
  return stateCall("studio_set_cell", { project, row, column, value });
}

/** Defines a metric and returns its verified token. */
export async function defineMetric(
  project: string,
  id: string,
  name: string,
  unit: string | null,
  formula: string
): Promise<DesktopStudioState> {
  return stateCall("studio_define_metric", { project, id, name, unit, formula });
}

/** Adds a calculated column. */
export async function addCalculatedColumn(
  project: string,
  name: string,
  formula: string
): Promise<DesktopStudioState> {
  return stateCall("studio_add_calculated_column", { project, name, formula });
}

/** Undoes the last change. */
export async function undo(project: string): Promise<DesktopStudioState> {
  return stateCall("studio_undo", { project });
}

/** Redoes the last undone change. */
export async function redo(project: string): Promise<DesktopStudioState> {
  return stateCall("studio_redo", { project });
}

/** Groups and aggregates inside Rust. */
export async function grouped(
  project: string,
  aggregate: string,
  column: string,
  groupBy: string
): Promise<readonly DesktopGroup[]> {
  return JSON.parse(
    await call("studio_grouped", { project, function: aggregate, column, groupBy })
  ) as readonly DesktopGroup[];
}

/** Serializes the project for local storage. */
export async function snapshot(project: string): Promise<string> {
  return JSON.stringify(JSON.parse(await call("studio_snapshot", { project })));
}

/** Restores a saved project; Rust re-derives every metric first. */
export async function restoreProject(
  project: string,
  saved: string
): Promise<DesktopStudioState> {
  return stateCall("studio_restore", { project, snapshot: saved });
}

/** Writes the capsule to the chosen path and returns it. */
export async function exportCapsule(
  project: string,
  target: string,
  options: string
): Promise<string> {
  return JSON.parse(await call("studio_export_capsule", { project, target, options })) as string;
}
