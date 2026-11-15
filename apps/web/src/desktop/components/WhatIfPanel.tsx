/**
 * What-If Scenario & Numeric Assumption Sliders Panel (`Phase 1.3`).
 *
 * Every numeric calculation and scenario comparison runs strictly inside Rust.
 * Changing assumptions never modifies the immutable original dataset.
 */
import type { ReactNode } from "react";
import { useState } from "react";
import { GitCompare, Play, RotateCcw, Sliders } from "lucide-react";
import type { SafiLang } from "../../components/layout/FloatingGlassDock";
import type { DesktopScenarioComparison, DesktopStudioState } from "../studio-bridge";

export interface WhatIfPanelProps {
  readonly state: DesktopStudioState;
  readonly lang?: SafiLang;
  readonly busy: boolean;
  readonly onDefineParameter: (name: string, value: string) => Promise<void>;
  readonly onUpdateParameter: (name: string, value: string) => Promise<void>;
  readonly onAddScenario: (id: string, name: string) => Promise<void>;
  readonly onRenameScenario: (id: string, newName: string) => Promise<void>;
  readonly onActivateScenario: (id: string | null) => Promise<void>;
  readonly onResetScenario: () => Promise<void>;
  readonly onSetScenarioParameter: (
    scenarioId: string,
    parameter: string,
    value: string
  ) => Promise<void>;
  readonly onCompareScenarios: (
    left: string,
    right: string
  ) => Promise<readonly DesktopScenarioComparison[]>;
}

export function WhatIfPanel({
  state,
  lang = "en",
  busy,
  onDefineParameter,
  onUpdateParameter,
  onAddScenario,
  onRenameScenario,
  onActivateScenario,
  onResetScenario,
  onSetScenarioParameter,
  onCompareScenarios
}: WhatIfPanelProps): ReactNode {
  const L = (ar: string, en: string): string => (lang === "ar" ? ar : en);
  const [paramName, setParamName] = useState("growth");
  const [paramValue, setParamValue] = useState("1.0");
  const [scenarioId, setScenarioId] = useState("optimistic");
  const [scenarioName, setScenarioName] = useState("Optimistic Q4");
  const [renameTarget, setRenameTarget] = useState("");
  const [renameText, setRenameText] = useState("");
  const [leftScenario, setLeftScenario] = useState("");
  const [rightScenario, setRightScenario] = useState("");
  const [comparisons, setComparisons] = useState<readonly DesktopScenarioComparison[]>([]);

  const activeScenarioObj =
    state.scenarios.find((entry) => entry.id === state.activeScenario) ?? null;

  const effectiveParamValue = (name: string, baseline: string): string => {
    if (activeScenarioObj !== null && activeScenarioObj.parameters[name] !== undefined) {
      return activeScenarioObj.parameters[name];
    }
    return baseline;
  };

  return (
    <section
      className="safi-desktop-studio__panel"
      aria-label={L("تحليل ماذا لو", "What-If Scenarios")}
    >
      <div className="safi-desktop-studio__panel-head">
        <h2>
          <Sliders size={16} aria-hidden="true" /> {L("ماذا لو والسيناريوهات", "What-If Scenarios")}
        </h2>
        <span className="safi-desktop-studio__note">
          {state.activeScenario !== null
            ? `${L("السيناريو النشط", "Active scenario")}: ${activeScenarioObj?.name ?? state.activeScenario}`
            : L("الافتراضات الأساسية (الأصل ثابت)", "Baseline assumptions (source untouched)")}
        </span>
      </div>

      {/* Define baseline assumption */}
      <form
        className="safi-desktop-studio__form"
        onSubmit={(event) => {
          event.preventDefault();
          void onDefineParameter(paramName, paramValue);
        }}
      >
        <label>
          {L("متغير رقمي", "Parameter")}
          <input
            value={paramName}
            onChange={(event) => setParamName(event.target.value)}
            placeholder="growth"
          />
        </label>
        <label>
          {L("القيمة الأساسية", "Baseline value")}
          <input
            value={paramValue}
            onChange={(event) => setParamValue(event.target.value)}
            placeholder="1.0"
          />
        </label>
        <button type="submit" disabled={busy || paramName.trim().length === 0}>
          {L("أضف المتغير", "Define parameter")}
        </button>
      </form>

      {/* Parameter Sliders */}
      {state.parameters.length > 0 ? (
        <div className="safi-desktop-studio__sliders">
          {state.parameters.map((param) => {
            const current = effectiveParamValue(param.name, param.value);
            const numeric = Number.parseFloat(current);
            const sliderVal = Number.isFinite(numeric) ? numeric : 1;
            const min = Math.min(0, Math.floor(sliderVal - 5));
            const max = Math.max(5, Math.ceil(sliderVal + 5));
            return (
              <div key={param.name} className="safi-desktop-studio__slider-row">
                <label htmlFor={`whatif-slider-${param.name}`}>
                  <strong>{param.name}</strong> ({current})
                </label>
                <input
                  id={`whatif-slider-${param.name}`}
                  type="range"
                  min={min}
                  max={max}
                  step="0.05"
                  value={sliderVal}
                  disabled={busy}
                  onChange={(event) => {
                    const nextVal = event.target.value;
                    if (state.activeScenario !== null) {
                      void onSetScenarioParameter(state.activeScenario, param.name, nextVal);
                    } else {
                      void onUpdateParameter(param.name, nextVal);
                    }
                  }}
                />
                <input
                  type="text"
                  aria-label={`${param.name} value`}
                  value={current}
                  disabled={busy}
                  onChange={(event) => {
                    const nextVal = event.target.value;
                    if (state.activeScenario !== null) {
                      void onSetScenarioParameter(state.activeScenario, param.name, nextVal);
                    } else {
                      void onUpdateParameter(param.name, nextVal);
                    }
                  }}
                />
              </div>
            );
          })}
        </div>
      ) : null}

      {/* Create scenario */}
      <form
        className="safi-desktop-studio__form"
        onSubmit={(event) => {
          event.preventDefault();
          void onAddScenario(scenarioId, scenarioName);
        }}
      >
        <label>
          {L("معرّف السيناريو", "Scenario ID")}
          <input value={scenarioId} onChange={(event) => setScenarioId(event.target.value)} />
        </label>
        <label>
          {L("اسم السيناريو", "Scenario name")}
          <input value={scenarioName} onChange={(event) => setScenarioName(event.target.value)} />
        </label>
        <button type="submit" disabled={busy || scenarioId.trim().length === 0}>
          {L("أنشئ سيناريو", "Create scenario")}
        </button>
        <button
          type="button"
          disabled={busy || state.activeScenario === null}
          onClick={() => void onResetScenario()}
        >
          <RotateCcw size={14} aria-hidden="true" />
          {L("العودة للأساس", "Reset to baseline")}
        </button>
      </form>

      {/* Activate / Rename scenarios */}
      {state.scenarios.length > 0 ? (
        <div className="safi-desktop-studio__scenario-list">
          {state.scenarios.map((scenario) => {
            const active = state.activeScenario === scenario.id;
            return (
              <div key={scenario.id} className="safi-desktop-studio__scenario-pill">
                <button
                  type="button"
                  disabled={busy || active}
                  onClick={() => void onActivateScenario(scenario.id)}
                >
                  <Play size={14} aria-hidden="true" />
                  {scenario.name} ({scenario.id})
                </button>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => {
                    setRenameTarget(scenario.id);
                    setRenameText(scenario.name);
                  }}
                >
                  {L("إعادة تسمية", "Rename")}
                </button>
              </div>
            );
          })}
        </div>
      ) : null}

      {renameTarget.length > 0 ? (
        <form
          className="safi-desktop-studio__form"
          onSubmit={(event) => {
            event.preventDefault();
            void onRenameScenario(renameTarget, renameText).then(() => setRenameTarget(""));
          }}
        >
          <label>
            {L("الاسم الجديد لـ", "New name for")} {renameTarget}
            <input value={renameText} onChange={(event) => setRenameText(event.target.value)} />
          </label>
          <button type="submit" disabled={busy || renameText.trim().length === 0}>
            {L("حفظ الاسم", "Save name")}
          </button>
        </form>
      ) : null}

      {/* Compare scenarios */}
      {state.scenarios.length >= 2 ? (
        <div className="safi-desktop-studio__form">
          <label>
            {L("السيناريو الأول", "Left scenario")}
            <select
              value={leftScenario || state.scenarios[0]?.id || ""}
              onChange={(event) => setLeftScenario(event.target.value)}
            >
              {state.scenarios.map((scenario) => (
                <option key={scenario.id} value={scenario.id}>
                  {scenario.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            {L("السيناريو الثاني", "Right scenario")}
            <select
              value={rightScenario || state.scenarios[1]?.id || ""}
              onChange={(event) => setRightScenario(event.target.value)}
            >
              {state.scenarios.map((scenario) => (
                <option key={scenario.id} value={scenario.id}>
                  {scenario.name}
                </option>
              ))}
            </select>
          </label>
          <button
            type="button"
            disabled={busy}
            onClick={() => {
              const left = leftScenario || state.scenarios[0]?.id || "";
              const right = rightScenario || state.scenarios[1]?.id || "";
              if (left.length > 0 && right.length > 0) {
                void onCompareScenarios(left, right).then(setComparisons);
              }
            }}
          >
            <GitCompare size={14} aria-hidden="true" />
            {L("قارن", "Compare")}
          </button>
        </div>
      ) : null}

      {comparisons.length > 0 ? (
        <table className="safi-desktop-studio__table">
          <thead>
            <tr>
              <th>{L("المؤشر", "Metric")}</th>
              <th>{leftScenario || state.scenarios[0]?.name}</th>
              <th>{rightScenario || state.scenarios[1]?.name}</th>
            </tr>
          </thead>
          <tbody>
            {comparisons.map((entry) => (
              <tr key={entry.metricId}>
                <td>{entry.metricName}</td>
                <td>{entry.left}</td>
                <td>{entry.right}</td>
              </tr>
            ))}
          </tbody>
        </table>
      ) : null}

      {state.lastRecomputed.length > 0 ? (
        <p className="safi-desktop-studio__note">
          {L("المؤشرات المتأثرة وآخر تحديث", "Affected metrics recomputed")}:{" "}
          {state.lastRecomputed.join(", ")}
        </p>
      ) : null}
    </section>
  );
}
