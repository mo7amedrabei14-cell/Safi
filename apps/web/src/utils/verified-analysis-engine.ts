/**
 * Contextual Question Engine, Grounded Conversational Data Assistant (`Phase 3`),
 * and Verified Analysis & Insight Planner (`Phase 4`).
 *
 * Enforces strict epistemic discipline across 5 categories:
 * 1. `ConfirmedFact`
 * 2. `DataSupportedObservation`
 * 3. `PotentialExplanation`
 * 4. `UnresolvedQuestion`
 * 5. `UnsupportedConclusion`
 */

import type { ConfidenceTier, SemanticWorkbookModel } from "./semantic-intelligence";
import { scoreToConfidenceTier } from "./semantic-intelligence";

export type QuestionResolutionStatus =
  | "Pending"
  | "Answered"
  | "Skipped"
  | "Deferred"
  | "CustomAnswered";

export interface QuestionOption {
  readonly optionId: string;
  readonly label: string;
  readonly effectSummary: string;
}

export interface ContextualClarificationQuestion {
  readonly questionId: string;
  readonly priority: "Critical" | "High" | "Medium";
  readonly sheetName: string;
  readonly tableId: string;
  readonly columnName?: string | undefined;
  readonly prompt: string;
  readonly whyItMatters: string;
  readonly affectedMetricsAndViews: readonly string[];
  readonly options: readonly QuestionOption[];
  readonly status: QuestionResolutionStatus;
  readonly selectedOptionId?: string | undefined;
  readonly customAnswerText?: string | undefined;
  readonly answerHistory: readonly {
    readonly action: "Answer" | "Skip" | "Defer" | "Custom" | "Correct";
    readonly value: string;
    readonly timestampIso: string;
  }[];
}

export type EpistemicCategory =
  | "ConfirmedFact"
  | "DataSupportedObservation"
  | "PotentialExplanation"
  | "UnresolvedQuestion"
  | "UnsupportedConclusion";

export type AnalysisInsightDomain =
  | "DatasetOverview"
  | "KeyMetrics"
  | "TimeTrends"
  | "PeriodComparison"
  | "CategoryRanking"
  | "GrowthAndDecline"
  | "DistributionAnalysis"
  | "RatiosAndMargins"
  | "BudgetVariance"
  | "CorrelationAnalysis"
  | "OperationalIndicators"
  | "AnomalyDetection"
  | "ConditionalForecast";

export interface SourceCitation {
  readonly sheetName: string;
  readonly tableId: string;
  readonly columnNames: readonly string[];
  readonly a1Range: string;
  readonly formulaOrMethod: string;
}

export interface VerifiedAnalyticalInsight {
  readonly insightId: string;
  readonly domain: AnalysisInsightDomain;
  readonly epistemicCategory: EpistemicCategory;
  readonly title: string;
  readonly statement: string;
  readonly confidencePct: number;
  readonly confidenceTier: ConfidenceTier;
  readonly provisional: boolean;
  readonly provisionalReason?: string | undefined;
  readonly citation: SourceCitation;
}

export interface ConditionalForecastResult {
  readonly metricName: string;
  readonly timeColumn: string;
  readonly historicalPoints: readonly { readonly period: string; readonly value: number }[];
  readonly projectedNextPeriod: string;
  readonly projectedValue: number;
  readonly lowerBound: number;
  readonly upperBound: number;
  readonly slopePerPeriod: number;
  readonly rSquared: number;
  readonly explicitAssumptions: readonly string[];
}

export interface VerifiedAnalysisReport {
  readonly modelVersion: "2.0.0";
  readonly questions: readonly ContextualClarificationQuestion[];
  readonly insights: readonly VerifiedAnalyticalInsight[];
  readonly insightsByEpistemicCategory: Readonly<
    Record<EpistemicCategory, readonly VerifiedAnalyticalInsight[]>
  >;
  readonly forecasts: readonly ConditionalForecastResult[];
  readonly provisionalMetricIds: readonly string[];
}

export interface AssistantResponse {
  readonly query: string;
  readonly intent:
    | "WorkbookSummary"
    | "MainFindings"
    | "PerformanceRanking"
    | "MetricChangeExplanation"
    | "AnomaliesAndQuality"
    | "EpistemicBoundaries";
  readonly answerText: string;
  readonly bulletPoints: readonly string[];
  readonly epistemicCategory: EpistemicCategory;
  readonly confidenceTier: ConfidenceTier;
  readonly citations: readonly SourceCitation[];
  readonly cannotConcludeNotes: readonly string[];
}

/**
 * Generates prioritized high-impact contextual clarification questions from genuine model uncertainties.
 */
export function generateContextualQuestions(
  model: SemanticWorkbookModel
): ContextualClarificationQuestion[] {
  const questions: ContextualClarificationQuestion[] = [];

  for (const finding of model.qualityFindings) {
    if (finding.category !== "NeedsClarification") continue;

    if (finding.findingId.includes("_ambig") && finding.columnName) {
      const affectedKpis = model.canonicalKpis
        .filter((k) => k.tableId === finding.tableId && k.columnName === finding.columnName)
        .map((k) => k.label);

      questions.push({
        questionId: `q_${finding.findingId}`,
        priority: "Critical",
        sheetName: finding.sheetName,
        tableId: finding.tableId,
        columnName: finding.columnName,
        prompt: `Should "${finding.columnName}" in sheet "${finding.sheetName}" be treated as an Identifier Code or an Aggregatable Metric?`,
        whyItMatters: finding.description,
        affectedMetricsAndViews:
          affectedKpis.length > 0
            ? affectedKpis
            : [`Dashboard & Report aggregations for ${finding.sheetName}.${finding.columnName}`],
        options: [
          {
            optionId: "identifier",
            label: "Treat as Identifier / Code (Do not sum or average)",
            effectSummary: "Excludes column from sum/average KPIs and uses COUNT_DISTINCT only."
          },
          {
            optionId: "measure_sum",
            label: "Treat as Quantitative Measure (Allow SUM & AVERAGE)",
            effectSummary: "Includes column in total and average KPI calculations."
          }
        ],
        status: "Pending",
        answerHistory: []
      });
    } else if (finding.findingId.startsWith("dq_rel_")) {
      questions.push({
        questionId: `q_${finding.findingId}`,
        priority: "High",
        sheetName: finding.sheetName,
        tableId: finding.tableId,
        columnName: finding.columnName,
        prompt: `${finding.title} — should cross-sheet analysis join these tables on "${finding.columnName ?? ""}"?`,
        whyItMatters: finding.description,
        affectedMetricsAndViews: [
          `Cross-sheet breakdowns between ${finding.sheetName} and related lookup/reference sheets`
        ],
        options: [
          {
            optionId: "confirm_join",
            label: "Confirm relationship and join matching keys",
            effectSummary:
              "Enables cross-sheet dimensional grouping; unmatched keys are grouped as 'Unmatched'."
          },
          {
            optionId: "keep_separate",
            label: "Keep tables independent (Do not join)",
            effectSummary: "Analyzes each worksheet independently without cross-table merging."
          }
        ],
        status: "Pending",
        answerHistory: []
      });
    }
  }

  // Check for high missingness in measure columns
  for (const table of model.tables) {
    for (const col of table.columns) {
      if (col.analyticalRole === "Measure" && col.missingPct >= 15) {
        questions.push({
          questionId: `q_${col.columnId}_missing_policy`,
          priority: "Medium",
          sheetName: table.sheetName,
          tableId: table.tableId,
          columnName: col.headerName,
          prompt: `Column "${col.headerName}" in "${table.sheetName}" is missing ${col.missingPct}% of values (${col.missingCount} rows). How should missing values be handled?`,
          whyItMatters: `High missingness in "${col.headerName}" can skew averages and totals.`,
          affectedMetricsAndViews: [
            `Total ${col.headerName}`,
            `Average ${col.headerName}`,
            `Distribution & Trend charts for ${col.headerName}`
          ],
          options: [
            {
              optionId: "exclude_missing",
              label: "Exclude missing rows from averages (Recommended)",
              effectSummary:
                "Computes averages over non-null records only without fabricating values."
            },
            {
              optionId: "impute_median",
              label: `Impute missing values with median (${col.numericStats?.median ?? 0})`,
              effectSummary: "Reversibly fills missing cells with the column median."
            },
            {
              optionId: "treat_as_zero",
              label: "Treat missing cells as 0",
              effectSummary: "Replaces empty cells with 0 in totals and averages."
            }
          ],
          status: "Pending",
          answerHistory: []
        });
      }
    }
  }

  return questions;
}

/**
 * Applies a user action (`Answer`, `Skip`, `Defer`, `Custom`, `Correct`) to a contextual question.
 */
export function applyQuestionResolution(
  questions: readonly ContextualClarificationQuestion[],
  questionId: string,
  action: {
    readonly kind: "Answer" | "Skip" | "Defer" | "Custom" | "Correct";
    readonly optionId?: string | undefined;
    readonly customText?: string | undefined;
    readonly timestampIso?: string | undefined;
  }
): ContextualClarificationQuestion[] {
  const ts = action.timestampIso ?? new Date().toISOString();
  return questions.map((q) => {
    if (q.questionId !== questionId) return q;

    const historyEntry = {
      action: action.kind,
      value: action.customText ?? action.optionId ?? action.kind,
      timestampIso: ts
    };

    if (action.kind === "Skip") {
      return {
        ...q,
        status: "Skipped",
        selectedOptionId: undefined,
        customAnswerText: undefined,
        answerHistory: [...q.answerHistory, historyEntry]
      };
    }
    if (action.kind === "Defer") {
      return {
        ...q,
        status: "Deferred",
        answerHistory: [...q.answerHistory, historyEntry]
      };
    }
    if (action.kind === "Custom") {
      return {
        ...q,
        status: "CustomAnswered",
        customAnswerText: action.customText ?? "",
        selectedOptionId: undefined,
        answerHistory: [...q.answerHistory, historyEntry]
      };
    }
    return {
      ...q,
      status: "Answered",
      selectedOptionId: action.optionId,
      customAnswerText: action.customText,
      answerHistory: [...q.answerHistory, historyEntry]
    };
  });
}

function parseNum(v: string): number | null {
  const cleaned = v
    .trim()
    .replace(/^[$€£]\s*/, "")
    .replace(/\s*(EGP|SAR|AED|USD|EUR|ج\.م|ر\.س|%)\s*$/i, "")
    .replace(/,/g, "");
  if (!cleaned || !/^-?\d+(\.\d+)?$/.test(cleaned)) return null;
  const n = Number(cleaned);
  return Number.isFinite(n) ? n : null;
}

function computePearsonCorrelation(xs: readonly number[], ys: readonly number[]): number | null {
  const n = Math.min(xs.length, ys.length);
  if (n < 4) return null;
  const meanX = xs.slice(0, n).reduce((a, b) => a + b, 0) / n;
  const meanY = ys.slice(0, n).reduce((a, b) => a + b, 0) / n;
  let num = 0;
  let denX = 0;
  let denY = 0;
  for (let i = 0; i < n; i += 1) {
    const dx = xs[i]! - meanX;
    const dy = ys[i]! - meanY;
    num += dx * dy;
    denX += dx * dx;
    denY += dy * dy;
  }
  const den = Math.sqrt(denX * denY);
  if (den < 1e-12) return null;
  return Number((num / den).toFixed(4));
}

/**
 * Generates the complete `VerifiedAnalysisReport` (`Phase 4`) with strict separation into
 * the 5 epistemic categories (`ConfirmedFact`, `DataSupportedObservation`, `PotentialExplanation`,
 * `UnresolvedQuestion`, `UnsupportedConclusion`).
 */
export function buildVerifiedAnalysisReport(
  model: SemanticWorkbookModel,
  resolvedQuestions?: readonly ContextualClarificationQuestion[]
): VerifiedAnalysisReport {
  const questions = resolvedQuestions ?? generateContextualQuestions(model);
  const skippedOrPendingColumns = new Set<string>();
  const provisionalMetricIds: string[] = [];

  for (const q of questions) {
    if (q.status === "Skipped" || q.status === "Deferred" || q.status === "Pending") {
      if (q.columnName) {
        skippedOrPendingColumns.add(`${q.tableId}:${q.columnName}`);
      }
    }
  }

  for (const kpi of model.canonicalKpis) {
    if (kpi.provisional || skippedOrPendingColumns.has(`${kpi.tableId}:${kpi.columnName}`)) {
      provisionalMetricIds.push(kpi.metricId);
    }
  }

  const insights: VerifiedAnalyticalInsight[] = [];
  const forecasts: ConditionalForecastResult[] = [];

  // 1. Dataset Overview (ConfirmedFact)
  insights.push({
    insightId: "ins_overview_workbook",
    domain: "DatasetOverview",
    epistemicCategory: "ConfirmedFact",
    title: "Workbook Structure & Record Inventory",
    statement: `Workbook (${model.ingestionReport.detectedFormat}) contains ${model.ingestionReport.totalSheets} worksheet(s) (${model.ingestionReport.visibleSheets} visible, ${model.ingestionReport.hiddenSheets} hidden) with ${model.ingestionReport.totalTablesExtracted} structured table(s) and ${model.ingestionReport.totalAnalyticalRows} analytical record(s).`,
    confidencePct: 100,
    confidenceTier: "High",
    provisional: false,
    citation: {
      sheetName: model.tables[0]?.sheetName ?? "Workbook",
      tableId: model.tables[0]?.tableId ?? "tbl_1",
      columnNames: [],
      a1Range: model.tables[0]?.a1Range ?? "A1",
      formulaOrMethod: "COUNT(Worksheets, Tables, AnalyticalRows)"
    }
  });

  // 2. Key Metrics / KPIs (ConfirmedFact)
  for (const kpi of model.canonicalKpis) {
    const isProv = provisionalMetricIds.includes(kpi.metricId);
    insights.push({
      insightId: `ins_kpi_${kpi.metricId}`,
      domain: "KeyMetrics",
      epistemicCategory: "ConfirmedFact",
      title: kpi.label,
      statement: `${kpi.label} in "${kpi.sheetName}" is ${kpi.formattedValue}${isProv ? " [Provisional — pending column clarification]" : ""}.`,
      confidencePct: isProv ? Math.min(60, kpi.confidencePct) : kpi.confidencePct,
      confidenceTier: isProv ? "Low" : kpi.confidenceTier,
      provisional: isProv,
      provisionalReason: isProv
        ? `Clarification question for "${kpi.columnName}" was skipped or remains pending.`
        : undefined,
      citation: {
        sheetName: kpi.sheetName,
        tableId: kpi.tableId,
        columnNames: [kpi.columnName],
        a1Range: model.tables.find((t) => t.tableId === kpi.tableId)?.a1Range ?? "A1",
        formulaOrMethod: kpi.formulaTrace
      }
    });
  }

  // Per-table deep analysis
  for (const table of model.tables) {
    const rawRegion = model.ingestionReport.worksheets
      .flatMap((w) => w.tables)
      .find((r) => r.tableId === table.tableId);
    if (!rawRegion || rawRegion.dataRows.length === 0) continue;

    const dimensionCols = table.columns.filter((c) => c.analyticalRole === "Dimension");
    const measureCols = table.columns.filter(
      (c) => c.analyticalRole === "Measure" && c.aggregationAllowed && c.numericStats
    );
    const temporalCols = table.columns.filter((c) => c.analyticalRole === "Temporal");

    // 3. Category / Branch / Product Rankings (ConfirmedFact + DataSupportedObservation)
    if (dimensionCols.length > 0 && measureCols.length > 0) {
      const dim = dimensionCols[0]!;
      const meas = measureCols[0]!;
      const groupTotals = new Map<string, number>();

      for (const row of rawRegion.dataRows) {
        const key = (row[dim.columnIndex] ?? "").trim() || "(Unspecified)";
        const val = parseNum(row[meas.columnIndex] ?? "");
        if (val !== null) {
          groupTotals.set(key, (groupTotals.get(key) ?? 0) + val);
        }
      }

      const ranked = [...groupTotals.entries()].sort((a, b) => b[1] - a[1]);
      if (ranked.length >= 2) {
        const top = ranked[0]!;
        const bottom = ranked[ranked.length - 1]!;
        const grandSum = ranked.reduce((acc, [, v]) => acc + v, 0);
        const topSharePct = grandSum > 0 ? Number(((top[1] / grandSum) * 100).toFixed(1)) : 0;

        insights.push({
          insightId: `ins_rank_${table.tableId}_${dim.columnIndex}_${meas.columnIndex}`,
          domain: "CategoryRanking",
          epistemicCategory: "ConfirmedFact",
          title: `${dim.headerName} Performance Ranking by ${meas.headerName}`,
          statement: `Top ${dim.headerName} is "${top[0]}" (${top[1].toLocaleString("en-US")}, ${topSharePct}% of total), while lowest is "${bottom[0]}" (${bottom[1].toLocaleString("en-US")}).`,
          confidencePct: 96,
          confidenceTier: "High",
          provisional: false,
          citation: {
            sheetName: table.sheetName,
            tableId: table.tableId,
            columnNames: [dim.headerName, meas.headerName],
            a1Range: table.a1Range,
            formulaOrMethod: `GROUP_BY(${dim.headerName}, SUM(${meas.headerName}))`
          }
        });

        if (topSharePct >= 45) {
          insights.push({
            insightId: `ins_conc_${table.tableId}_${dim.columnIndex}`,
            domain: "OperationalIndicators",
            epistemicCategory: "DataSupportedObservation",
            title: `High Concentration in "${top[0]}"`,
            statement: `"${top[0]}" accounts for ${topSharePct}% of total ${meas.headerName} across ${ranked.length} ${dim.headerName} groups, indicating high concentration risk.`,
            confidencePct: 90,
            confidenceTier: "High",
            provisional: false,
            citation: {
              sheetName: table.sheetName,
              tableId: table.tableId,
              columnNames: [dim.headerName, meas.headerName],
              a1Range: table.a1Range,
              formulaOrMethod: `SHARE(${top[0]}) / SUM(${meas.headerName})`
            }
          });
        }
      }
    }

    // 4. Time Trends, Period-over-Period, Growth/Decline & Conditional Forecast
    if (temporalCols.length > 0 && measureCols.length > 0) {
      const timeCol = temporalCols[0]!;
      const meas = measureCols[0]!;
      const periodPoints: Array<{ period: string; value: number }> = [];

      for (const row of rawRegion.dataRows) {
        const p = (row[timeCol.columnIndex] ?? "").trim();
        const v = parseNum(row[meas.columnIndex] ?? "");
        if (p && v !== null) {
          periodPoints.push({ period: p, value: v });
        }
      }

      if (periodPoints.length >= 2) {
        const first = periodPoints[0]!;
        const last = periodPoints[periodPoints.length - 1]!;
        const delta = last.value - first.value;
        const pctChange =
          Math.abs(first.value) > 1e-9
            ? Number(((delta / Math.abs(first.value)) * 100).toFixed(2))
            : 0;
        const direction = delta > 0 ? "increased" : delta < 0 ? "decreased" : "remained flat";

        insights.push({
          insightId: `ins_trend_${table.tableId}_${meas.columnIndex}`,
          domain: "PeriodComparison",
          epistemicCategory: "DataSupportedObservation",
          title: `${meas.headerName} Period-over-Period Trajectory`,
          statement: `From ${first.period} (${first.value.toLocaleString("en-US")}) to ${last.period} (${last.value.toLocaleString("en-US")}), ${meas.headerName} ${direction} by ${Math.abs(pctChange)}% (${delta >= 0 ? "+" : ""}${delta.toLocaleString("en-US")}).`,
          confidencePct: 92,
          confidenceTier: "High",
          provisional: false,
          citation: {
            sheetName: table.sheetName,
            tableId: table.tableId,
            columnNames: [timeCol.headerName, meas.headerName],
            a1Range: table.a1Range,
            formulaOrMethod: `PCT_CHANGE(${first.period} -> ${last.period})`
          }
        });

        // Potential Explanation (strictly labeled as hypothesis requiring verification!)
        insights.push({
          insightId: `ins_hypoth_${table.tableId}_${meas.columnIndex}`,
          domain: "GrowthAndDecline",
          epistemicCategory: "PotentialExplanation",
          title: `Potential Drivers of ${meas.headerName} Movement`,
          statement: `The ${Math.abs(pctChange)}% shift in ${meas.headerName} between ${first.period} and ${last.period} may reflect seasonal demand, pricing adjustments, or volume concentration in top categories; domain verification is required.`,
          confidencePct: 68,
          confidenceTier: "Medium",
          provisional: true,
          provisionalReason:
            "Hypothesis requires external operational context beyond the workbook.",
          citation: {
            sheetName: table.sheetName,
            tableId: table.tableId,
            columnNames: [timeCol.headerName, meas.headerName],
            a1Range: table.a1Range,
            formulaOrMethod: "HYPOTHESIS_GENERATION(TrendDelta)"
          }
        });

        // Conditional Forecast if >= 3 periods
        if (periodPoints.length >= 3) {
          const n = periodPoints.length;
          const xs = periodPoints.map((_, idx) => idx);
          const ys = periodPoints.map((p) => p.value);
          const meanX = xs.reduce((a, b) => a + b, 0) / n;
          const meanY = ys.reduce((a, b) => a + b, 0) / n;
          let num = 0;
          let den = 0;
          for (let i = 0; i < n; i += 1) {
            num += (xs[i]! - meanX) * (ys[i]! - meanY);
            den += (xs[i]! - meanX) ** 2;
          }
          const slope = den > 0 ? num / den : 0;
          const intercept = meanY - slope * meanX;
          const projected = Number((intercept + slope * n).toFixed(2));
          const r = computePearsonCorrelation(xs, ys) ?? 0;
          const rSquared = Number((r * r).toFixed(3));
          const stdErr = meas.numericStats?.stdDev ?? Math.abs(projected) * 0.1;

          forecasts.push({
            metricName: meas.headerName,
            timeColumn: timeCol.headerName,
            historicalPoints: periodPoints,
            projectedNextPeriod: `Next Period (t+${n + 1})`,
            projectedValue: projected,
            lowerBound: Number((projected - 1.28 * stdErr).toFixed(2)),
            upperBound: Number((projected + 1.28 * stdErr).toFixed(2)),
            slopePerPeriod: Number(slope.toFixed(2)),
            rSquared,
            explicitAssumptions: [
              `Linear continuation of ${n}-period historical trend (R² = ${rSquared})`,
              "Assumes no structural regime change, price shock, or missing period bias"
            ]
          });

          insights.push({
            insightId: `ins_forecast_${table.tableId}_${meas.columnIndex}`,
            domain: "ConditionalForecast",
            epistemicCategory: "DataSupportedObservation",
            title: `Conditional Next-Period Projection for ${meas.headerName}`,
            statement: `Assuming linear continuation of the ${n}-period trend (slope ${slope >= 0 ? "+" : ""}${slope.toFixed(2)}/period, R² = ${rSquared}), next-period ${meas.headerName} is conditionally projected at ${projected.toLocaleString("en-US")} (80% interval: ${(projected - 1.28 * stdErr).toFixed(1)} to ${(projected + 1.28 * stdErr).toFixed(1)}).`,
            confidencePct: Math.max(55, Math.min(88, Math.round(rSquared * 90))),
            confidenceTier: scoreToConfidenceTier(Math.max(55, Math.round(rSquared * 90))),
            provisional: true,
            provisionalReason:
              "Conditional statistical projection subject to explicit linear trend assumptions.",
            citation: {
              sheetName: table.sheetName,
              tableId: table.tableId,
              columnNames: [timeCol.headerName, meas.headerName],
              a1Range: table.a1Range,
              formulaOrMethod: `OLS_LINEAR_TREND(n=${n}, R²=${rSquared})`
            }
          });
        }
      }
    }

    // 5. Distribution & Anomaly Detection
    for (const meas of measureCols) {
      const stats = meas.numericStats!;
      if (stats.outlierCount > 0) {
        insights.push({
          insightId: `ins_anomaly_${meas.columnId}`,
          domain: "AnomalyDetection",
          epistemicCategory: "DataSupportedObservation",
          title: `Outlier Detection in ${meas.headerName}`,
          statement: `${stats.outlierCount} record(s) in "${meas.headerName}" exceed the 1.5×IQR fence (values: ${stats.outlierValues.join(", ")}; median = ${stats.median}, mean = ${stats.mean}).`,
          confidencePct: 94,
          confidenceTier: "High",
          provisional: false,
          citation: {
            sheetName: table.sheetName,
            tableId: table.tableId,
            columnNames: [meas.headerName],
            a1Range: table.a1Range,
            formulaOrMethod: `IQR_OUTLIER_FENCE(Q1=${stats.q1}, Q3=${stats.q3}, IQR=${stats.iqr})`
          }
        });
      }
    }

    // 6. Correlation Candidates & Budget vs Actual Variance
    if (measureCols.length >= 2) {
      const m1 = measureCols[0]!;
      const m2 = measureCols[1]!;
      const pairedX: number[] = [];
      const pairedY: number[] = [];
      for (const row of rawRegion.dataRows) {
        const vx = parseNum(row[m1.columnIndex] ?? "");
        const vy = parseNum(row[m2.columnIndex] ?? "");
        if (vx !== null && vy !== null) {
          pairedX.push(vx);
          pairedY.push(vy);
        }
      }
      const r = computePearsonCorrelation(pairedX, pairedY);
      if (r !== null && Math.abs(r) >= 0.5) {
        insights.push({
          insightId: `ins_corr_${table.tableId}_${m1.columnIndex}_${m2.columnIndex}`,
          domain: "CorrelationAnalysis",
          epistemicCategory: "DataSupportedObservation",
          title: `Co-Movement Between ${m1.headerName} and ${m2.headerName}`,
          statement: `Observed Pearson correlation between "${m1.headerName}" and "${m2.headerName}" is r = ${r} across ${pairedX.length} paired records (statistical association only; does not prove causation).`,
          confidencePct: 86,
          confidenceTier: "High",
          provisional: false,
          citation: {
            sheetName: table.sheetName,
            tableId: table.tableId,
            columnNames: [m1.headerName, m2.headerName],
            a1Range: table.a1Range,
            formulaOrMethod: `PEARSON_R(${m1.headerName}, ${m2.headerName})`
          }
        });
      }

      // Check for Target/Budget vs Actual variance pair
      const budgetCol = measureCols.find((c) =>
        /budget|target|plan|ميزانية|مستهدف|خطة/i.test(c.headerName)
      );
      const actualCol = measureCols.find(
        (c) =>
          c !== budgetCol &&
          /actual|revenue|sales|amount|فعلي|إيراد|مبيعات|منجز/i.test(c.headerName)
      );
      if (budgetCol?.numericStats && actualCol?.numericStats) {
        const variance = actualCol.numericStats.sum - budgetCol.numericStats.sum;
        const varPct =
          Math.abs(budgetCol.numericStats.sum) > 1e-9
            ? Number(((variance / budgetCol.numericStats.sum) * 100).toFixed(2))
            : 0;
        insights.push({
          insightId: `ins_var_${table.tableId}`,
          domain: "BudgetVariance",
          epistemicCategory: "ConfirmedFact",
          title: `${actualCol.headerName} vs ${budgetCol.headerName} Variance`,
          statement: `Total ${actualCol.headerName} (${actualCol.numericStats.sum.toLocaleString("en-US")}) vs ${budgetCol.headerName} (${budgetCol.numericStats.sum.toLocaleString("en-US")}) yields a variance of ${variance >= 0 ? "+" : ""}${variance.toLocaleString("en-US")} (${variance >= 0 ? "+" : ""}${varPct}%).`,
          confidencePct: 97,
          confidenceTier: "High",
          provisional: false,
          citation: {
            sheetName: table.sheetName,
            tableId: table.tableId,
            columnNames: [actualCol.headerName, budgetCol.headerName],
            a1Range: table.a1Range,
            formulaOrMethod: `SUM(${actualCol.headerName}) - SUM(${budgetCol.headerName})`
          }
        });
      }
    }
  }

  // 7. Unresolved Questions (EpistemicCategory: UnresolvedQuestion)
  for (const q of questions) {
    if (q.status !== "Answered") {
      insights.push({
        insightId: `ins_unresolved_${q.questionId}`,
        domain: "DatasetOverview",
        epistemicCategory: "UnresolvedQuestion",
        title: `Unresolved Analytical Question: ${q.columnName ?? q.sheetName}`,
        statement: `${q.prompt} (Status: ${q.status}). Dependent metrics remain flagged as provisional until confirmed.`,
        confidencePct: 50,
        confidenceTier: "Low",
        provisional: true,
        provisionalReason: q.whyItMatters,
        citation: {
          sheetName: q.sheetName,
          tableId: q.tableId,
          columnNames: q.columnName ? [q.columnName] : [],
          a1Range: "A1",
          formulaOrMethod: "CLARIFICATION_REQUIRED"
        }
      });
    }
  }

  // 8. Unsupported Conclusions Guardrail (EpistemicCategory: UnsupportedConclusion)
  insights.push({
    insightId: "ins_guardrail_causality",
    domain: "DatasetOverview",
    epistemicCategory: "UnsupportedConclusion",
    title: "Epistemic Boundary: What Cannot Be Concluded From This Workbook Alone",
    statement:
      "Observational workbook data establishes exact accounting totals, distributions, and correlations, but cannot by itself prove external root causes (e.g., macroeconomic shifts, competitor actions, or unrecorded off-sheet transactions) without external verification.",
    confidencePct: 100,
    confidenceTier: "High",
    provisional: false,
    citation: {
      sheetName: model.tables[0]?.sheetName ?? "Workbook",
      tableId: model.tables[0]?.tableId ?? "tbl_1",
      columnNames: [],
      a1Range: model.tables[0]?.a1Range ?? "A1",
      formulaOrMethod: "EPISTEMIC_CAUSALITY_GUARDRAIL"
    }
  });

  const insightsByEpistemicCategory: Record<EpistemicCategory, VerifiedAnalyticalInsight[]> = {
    ConfirmedFact: insights.filter((i) => i.epistemicCategory === "ConfirmedFact"),
    DataSupportedObservation: insights.filter(
      (i) => i.epistemicCategory === "DataSupportedObservation"
    ),
    PotentialExplanation: insights.filter((i) => i.epistemicCategory === "PotentialExplanation"),
    UnresolvedQuestion: insights.filter((i) => i.epistemicCategory === "UnresolvedQuestion"),
    UnsupportedConclusion: insights.filter((i) => i.epistemicCategory === "UnsupportedConclusion")
  };

  return {
    modelVersion: "2.0.0",
    questions,
    insights,
    insightsByEpistemicCategory,
    forecasts,
    provisionalMetricIds
  };
}

/**
 * Answers natural-language analytical questions grounded strictly in the `SemanticWorkbookModel`
 * and `VerifiedAnalysisReport` (`Phase 3` Conversational Data Assistant).
 */
export function answerAnalyticalQuestion(
  query: string,
  model: SemanticWorkbookModel,
  analysis?: VerifiedAnalysisReport
): AssistantResponse {
  const report = analysis ?? buildVerifiedAnalysisReport(model);
  const qLower = query.trim().toLowerCase();

  const defaultCitation: SourceCitation = {
    sheetName: model.tables[0]?.sheetName ?? "Workbook",
    tableId: model.tables[0]?.tableId ?? "tbl_1",
    columnNames: model.tables[0]?.columns.map((c) => c.headerName) ?? [],
    a1Range: model.tables[0]?.a1Range ?? "A1",
    formulaOrMethod: "SEMANTIC_WORKBOOK_MODEL"
  };

  const cannotConcludeNotes = report.insightsByEpistemicCategory.UnsupportedConclusion.map(
    (i) => i.statement
  );

  // 1. Epistemic boundaries / What can or cannot be concluded
  if (
    /cannot|can't|conclude|limit|prove|causal|لا يمكن|استنتاج|حدود|سبب/.test(qLower) &&
    /conclude|limit|prove|استنتاج|حدود/.test(qLower)
  ) {
    const facts = report.insightsByEpistemicCategory.ConfirmedFact.map((i) => i.statement);
    const unresolved = report.insightsByEpistemicCategory.UnresolvedQuestion.map(
      (i) => i.statement
    );
    return {
      query,
      intent: "EpistemicBoundaries",
      answerText: `From this workbook, ${facts.length} confirmed fact(s) and ${report.insightsByEpistemicCategory.DataSupportedObservation.length} data-supported observation(s) are verified directly. However, causal explanations for external market/operational drivers and ${unresolved.length} unresolved clarification question(s) cannot be concluded from the spreadsheet alone.`,
      bulletPoints: [...facts.slice(0, 3), ...cannotConcludeNotes, ...unresolved],
      epistemicCategory: "UnsupportedConclusion",
      confidenceTier: "High",
      citations: [defaultCitation],
      cannotConcludeNotes
    };
  }

  // 2. Anomalies or data quality issues
  if (
    /anomal|outlier|quality|missing|duplicate|error|issue|شذوذ|متطرف|جودة|مفقود|مكرر|خطأ|مشاكل/.test(
      qLower
    )
  ) {
    const findingsBullets = model.qualityFindings.map(
      (f) => `[${f.category}] ${f.title}: ${f.description}`
    );
    const anomalyInsights = report.insights.filter((i) => i.domain === "AnomalyDetection");
    return {
      query,
      intent: "AnomaliesAndQuality",
      answerText:
        model.qualityFindings.length > 0
          ? `Detected ${model.qualityFindings.length} data quality & anomaly finding(s) across the workbook (Overall Quality Score: ${model.overallQualityScorePct}%).`
          : `No critical data quality issues or statistical outliers were detected (Overall Quality Score: ${model.overallQualityScorePct}%).`,
      bulletPoints: [...findingsBullets, ...anomalyInsights.map((a) => a.statement)],
      epistemicCategory: "ConfirmedFact",
      confidenceTier: "High",
      citations: anomalyInsights[0] ? [anomalyInsights[0].citation] : [defaultCitation],
      cannotConcludeNotes
    };
  }

  // 3. Best / worst branch, category, product, or ranking
  if (
    /best|worst|top|bottom|highest|lowest|rank|branch|category|product|أفضل|أسوأ|أعلى|أقل|ترتيب|فرع|فئة|منتج/.test(
      qLower
    )
  ) {
    const rankInsights = report.insights.filter(
      (i) => i.domain === "CategoryRanking" || i.domain === "OperationalIndicators"
    );
    if (rankInsights.length > 0) {
      return {
        query,
        intent: "PerformanceRanking",
        answerText: rankInsights.map((i) => i.statement).join(" "),
        bulletPoints: rankInsights.map((i) => i.statement),
        epistemicCategory: "ConfirmedFact",
        confidenceTier: "High",
        citations: rankInsights.map((i) => i.citation),
        cannotConcludeNotes
      };
    }
  }

  // 4. Why a metric changed / trend / period comparison
  if (
    /why|change|trend|grow|declin|increase|decrease|period|forecast|لماذا|تغير|اتجاه|نمو|انخفاض|فترة|توقع/.test(
      qLower
    )
  ) {
    const trendInsights = report.insights.filter(
      (i) =>
        i.domain === "PeriodComparison" ||
        i.domain === "GrowthAndDecline" ||
        i.domain === "ConditionalForecast" ||
        i.domain === "BudgetVariance"
    );
    if (trendInsights.length > 0) {
      return {
        query,
        intent: "MetricChangeExplanation",
        answerText: trendInsights.map((i) => i.statement).join(" "),
        bulletPoints: trendInsights.map((i) => `[${i.epistemicCategory}] ${i.statement}`),
        epistemicCategory: "DataSupportedObservation",
        confidenceTier: "High",
        citations: trendInsights.map((i) => i.citation),
        cannotConcludeNotes
      };
    }
  }

  // 5. Workbook structure / what the workbook contains
  if (
    /contain|sheet|table|structure|column|what is in|ماذا يحتوي|ورقة|جدول|هيكل|أعمدة/.test(qLower)
  ) {
    const sheetSummaries = model.ingestionReport.worksheets.map(
      (w) =>
        `Sheet "${w.name}" (${w.visibility}, classified as ${w.classification}): ${w.tables.length} table(s), ${w.formulaCount} formula(s)`
    );
    return {
      query,
      intent: "WorkbookSummary",
      answerText:
        report.insights.find((i) => i.insightId === "ins_overview_workbook")?.statement ??
        `Workbook contains ${model.ingestionReport.totalSheets} worksheet(s).`,
      bulletPoints: sheetSummaries,
      epistemicCategory: "ConfirmedFact",
      confidenceTier: "High",
      citations: [defaultCitation],
      cannotConcludeNotes
    };
  }

  // 6. Default: Main findings & canonical KPIs
  const mainBullets = [
    ...report.insightsByEpistemicCategory.ConfirmedFact.map((i) => i.statement),
    ...report.insightsByEpistemicCategory.DataSupportedObservation.map((i) => i.statement)
  ];
  return {
    query,
    intent: "MainFindings",
    answerText:
      mainBullets.slice(0, 3).join(" ") ||
      `Analyzed ${model.ingestionReport.totalAnalyticalRows} records across ${model.tables.length} table(s).`,
    bulletPoints: mainBullets,
    epistemicCategory: "ConfirmedFact",
    confidenceTier: model.overallConfidenceTier,
    citations: report.insights.slice(0, 3).map((i) => i.citation),
    cannotConcludeNotes
  };
}
