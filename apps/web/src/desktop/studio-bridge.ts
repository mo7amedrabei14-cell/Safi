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

export type MetricVerificationStatus = "Verified" | "Stale" | "Unverified" | "Error";

export interface DesktopMetric {
  readonly id: string;
  readonly name: string;
  readonly value: string;
  readonly unit: string | null;
  readonly formula: string;
  readonly formulaVersion: number;
  readonly sourceId: string;
  readonly sourceVersion: number;
  readonly sourceFingerprint: string;
  readonly fingerprint: string;
  readonly status: string;
  readonly rows: readonly number[];
  readonly sourceRowIds: readonly number[];
  readonly columns: readonly string[];
  readonly columnIds: readonly number[];
  readonly transformationTrace: readonly string[];
  readonly engineVersion: string;
  readonly computedAtMs: number;
}

export interface DesktopColumn {
  readonly id: number;
  readonly name: string;
  readonly type: string;
  readonly originColumnIndex: number | null;
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

export interface DesktopScenario {
  readonly id: string;
  readonly name: string;
  readonly parameters: Readonly<Record<string, string>>;
}

export interface DesktopScenarioComparison {
  readonly metricId: string;
  readonly metricName: string;
  readonly left: string;
  readonly right: string;
}

export interface DesktopStudioState {
  readonly sourceId: string;
  readonly revision: number;
  readonly datasetVersion: number;
  readonly originFingerprint: string;
  readonly rowCount: number;
  readonly columns: readonly DesktopColumn[];
  readonly metrics: readonly DesktopMetric[];
  readonly errors: readonly { readonly id: string; readonly code: string }[];
  readonly anomalies: readonly DesktopAnomaly[];
  readonly calculated: readonly string[];
  readonly lastRecomputed: readonly string[];
  readonly activeScenario: string | null;
  readonly scenarios: readonly DesktopScenario[];
  readonly parameters: readonly DesktopParameter[];
}

export interface DesktopRowQuery {
  readonly offset: number;
  readonly limit: number;
  readonly search: string | null;
  readonly sort_column: string | null;
  readonly ascending: boolean;
  readonly filter_column: string | null;
  readonly filter_value: string | null;
}

export interface DesktopRowCellView {
  readonly raw: string;
  readonly normalized: string;
  readonly visible: string;
  readonly changed: boolean;
  readonly validation_error: string | null;
}

export interface DesktopRowPageItem {
  readonly working_row: number;
  readonly row_id: number;
  readonly origin_row_id: number | null;
  readonly origin_row_index: number | null;
  readonly cells: readonly DesktopRowCellView[];
}

export interface DesktopRowPage {
  readonly total_rows: number;
  readonly filtered_rows: number;
  readonly offset: number;
  readonly limit: number;
  readonly revision: number;
  readonly rows: readonly DesktopRowPageItem[];
}

export interface DesktopGroup {
  readonly label: string;
  readonly value: string;
}

export interface DesktopNarrativeResult {
  readonly text: string;
  readonly missing: readonly string[];
  readonly references: readonly {
    readonly id: string;
    readonly value: string;
    readonly fingerprint: string;
  }[];
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

async function stateCall(
  command: string,
  args: Record<string, unknown>
): Promise<DesktopStudioState> {
  return JSON.parse(await call(command, args)) as DesktopStudioState;
}

/** Ensures superseded async revisions never overwrite newer state snapshots. */
export function isNewerOrEqualRevision(
  candidate: DesktopStudioState,
  current: DesktopStudioState | null
): boolean {
  if (current === null) {
    return true;
  }
  return candidate.revision >= current.revision;
}

/** Monotonic client request sequence helper for rejecting stale out-of-order IPC responses. */
export interface RequestTracker {
  next(): number;
  isCurrent(seq: number): boolean;
}

export function createRequestTracker(): RequestTracker {
  let latest = 0;
  return {
    next(): number {
      latest += 1;
      return latest;
    },
    isCurrent(seq: number): boolean {
      return seq === latest;
    }
  };
}

/** Opens the native open dialog; returns the chosen path or `null`. */
export async function pickInputFile(): Promise<string | null> {
  return JSON.parse(await call("pick_input_file")) as string | null;
}

/** Opens the native save dialog for CSV export; returns the chosen path or `null`. */
export async function pickOutputFile(): Promise<string | null> {
  return JSON.parse(await call("pick_output_file")) as string | null;
}

/** Opens the native save dialog for `.safi.html` capsule export; returns the chosen path or `null`. */
export async function pickCapsuleOutputFile(): Promise<string | null> {
  return JSON.parse(await call("pick_capsule_output_file")) as string | null;
}

/** Imports a spreadsheet from disk into a new session. */
export async function openProject(project: string, path: string): Promise<DesktopStudioState> {
  return stateCall("studio_open_file", { project, path });
}

/** Reads the current metadata/provenance state of an open project. */
export async function projectState(project: string): Promise<DesktopStudioState> {
  return stateCall("studio_state", { project });
}

/** Fetches a bounded page of working rows without transferring the full dataset. */
export async function fetchRowsPage(
  project: string,
  query: Partial<DesktopRowQuery> = {}
): Promise<DesktopRowPage> {
  const fullQuery: DesktopRowQuery = {
    offset: query.offset ?? 0,
    limit: query.limit ?? 50,
    search: query.search ?? null,
    sort_column: query.sort_column ?? null,
    ascending: query.ascending ?? true,
    filter_column: query.filter_column ?? null,
    filter_value: query.filter_value ?? null
  };
  return JSON.parse(
    await call("studio_rows_page", { project, query: JSON.stringify(fullQuery) })
  ) as DesktopRowPage;
}

/** Replaces one cell by display index and returns the recomputed state. */
export async function setCell(
  project: string,
  row: number,
  column: string,
  value: string
): Promise<DesktopStudioState> {
  return stateCall("studio_set_cell", { project, row, column, value });
}

/** Replaces one cell by stable `RowId` with an optional optimistic revision check. */
export async function setCellById(
  project: string,
  rowId: number,
  column: string,
  value: string,
  expectedRevision?: number
): Promise<DesktopStudioState> {
  return stateCall("studio_set_cell_by_id", {
    project,
    rowId,
    column,
    value,
    expectedRevision: expectedRevision ?? null
  });
}

/** Sorts working rows cleanly while retaining every row's stable `RowId`. */
export async function sortRows(
  project: string,
  column: string,
  ascending: boolean
): Promise<DesktopStudioState> {
  return stateCall("studio_sort_rows", { project, column, ascending });
}

/** Filters working rows while retaining every remaining row's stable `RowId`. */
export async function filterRows(
  project: string,
  column: string,
  query: string
): Promise<DesktopStudioState> {
  return stateCall("studio_filter_rows", { project, column, query });
}

/** Deletes a working row by stable `RowId`; never reuses its ID. */
export async function deleteRow(project: string, rowId: number): Promise<DesktopStudioState> {
  return stateCall("studio_delete_row", { project, rowId });
}

/** Inserts a working row with a new monotonic `RowId`. */
export async function insertRow(
  project: string,
  afterRowId: number | null,
  values: Readonly<Record<string, string>>
): Promise<DesktopStudioState> {
  return stateCall("studio_insert_row", {
    project,
    afterRowId,
    values: JSON.stringify(values)
  });
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

/** Defines a baseline numeric What-If parameter. */
export async function defineParameter(
  project: string,
  name: string,
  value: string
): Promise<DesktopStudioState> {
  return stateCall("studio_define_parameter", { project, name, value });
}

/** Updates a baseline numeric What-If parameter and recomputes dependent metrics. */
export async function updateParameter(
  project: string,
  name: string,
  value: string
): Promise<DesktopStudioState> {
  return stateCall("studio_update_parameter", { project, name, value });
}

/** Adds a named What-If scenario. */
export async function addScenario(
  project: string,
  id: string,
  name: string,
  parameters: Readonly<Record<string, string>> = {}
): Promise<DesktopStudioState> {
  return stateCall("studio_add_scenario", {
    project,
    id,
    name,
    parameters: JSON.stringify(parameters)
  });
}

/** Renames an existing What-If scenario. */
export async function renameScenario(
  project: string,
  id: string,
  newName: string
): Promise<DesktopStudioState> {
  return stateCall("studio_rename_scenario", { project, id, newName });
}

/** Activates a What-If scenario (or `null` for baseline) and recomputes metrics. */
export async function activateScenario(
  project: string,
  id: string | null
): Promise<DesktopStudioState> {
  return stateCall("studio_activate_scenario", { project, id });
}

/** Resets active What-If scenario back to baseline assumptions. */
export async function resetScenario(project: string): Promise<DesktopStudioState> {
  return stateCall("studio_reset_scenario", { project });
}

/** Updates one parameter inside an existing scenario and recomputes if active. */
export async function setScenarioParameter(
  project: string,
  scenarioId: string,
  parameter: string,
  value: string
): Promise<DesktopStudioState> {
  return stateCall("studio_set_scenario_parameter", {
    project,
    scenarioId,
    parameter,
    value
  });
}

/** Compares two scenarios side-by-side inside Rust without mutating project state. */
export async function compareScenarios(
  project: string,
  left: string,
  right: string
): Promise<readonly DesktopScenarioComparison[]> {
  return JSON.parse(
    await call("studio_compare_scenarios", { project, left, right })
  ) as readonly DesktopScenarioComparison[];
}

/** Executes a validated Studio DSL JSON command inside Rust. */
export async function applyDslCommand(
  project: string,
  commandJson: string
): Promise<DesktopStudioState> {
  return stateCall("studio_apply_command", { project, command: commandJson });
}

/** Resolves `{{metric:...}}` narrative tokens inside Rust using verified metric tokens. */
export async function renderNarrative(
  project: string,
  template: string
): Promise<DesktopNarrativeResult> {
  return JSON.parse(
    await call("studio_narrative", { project, template })
  ) as DesktopNarrativeResult;
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
export async function restoreProject(project: string, saved: string): Promise<DesktopStudioState> {
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
