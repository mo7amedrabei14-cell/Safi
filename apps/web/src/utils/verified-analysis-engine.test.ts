import { describe, expect, it } from "vitest";
import { ingestWorkbookStructure } from "./workbook-ingestion";
import { buildSemanticWorkbookModel } from "./semantic-intelligence";
import {
  answerAnalyticalQuestion,
  applyQuestionResolution,
  buildVerifiedAnalysisReport,
  generateContextualQuestions
} from "./verified-analysis-engine";

describe("Contextual Question System, Conversational Assistant & Verified Analysis (Phases 3 & 4)", () => {
  it("generates high-impact questions, supports Answer/Skip/Defer/Custom/Correct, and marks skipped dependencies provisional", () => {
    const ingestion = ingestWorkbookStructure([
      {
        name: "Operations",
        grid: [
          ["Batch_Ref", "Branch", "Period", "Revenue_EGP", "Budget_EGP"],
          ["9001", "Cairo", "2026-01-01", "1000", "900"],
          ["9002", "Cairo", "2026-02-01", "1300", "1100"],
          ["9003", "Alexandria", "2026-03-01", "1600", "1400"],
          ["9004", "Cairo", "2026-04-01", "1900", "1700"],
          ["9005", "Alexandria", "2026-05-01", "8500", "2000"]
        ]
      }
    ]);

    const model = buildSemanticWorkbookModel(ingestion);
    const questions = generateContextualQuestions(model);
    expect(questions.length).toBeGreaterThanOrEqual(1);

    const firstQ = questions[0];
    expect(firstQ).toBeDefined();
    if (!firstQ) return;
    expect(firstQ.status).toBe("Pending");

    // 1. Skip -> marks dependent metric provisional
    const afterSkip = applyQuestionResolution(questions, firstQ.questionId, {
      kind: "Skip"
    });
    expect(afterSkip[0]?.status).toBe("Skipped");

    const skippedReport = buildVerifiedAnalysisReport(model, afterSkip);
    expect(
      skippedReport.insightsByEpistemicCategory.UnresolvedQuestion.length
    ).toBeGreaterThanOrEqual(1);

    // 2. Defer -> status becomes Deferred
    const afterDefer = applyQuestionResolution(afterSkip, firstQ.questionId, {
      kind: "Defer"
    });
    expect(afterDefer[0]?.status).toBe("Deferred");

    // 3. Custom Answer -> status becomes CustomAnswered
    const afterCustom = applyQuestionResolution(afterDefer, firstQ.questionId, {
      kind: "Custom",
      customText: "Batch_Ref is an internal batch identifier code"
    });
    expect(afterCustom[0]?.status).toBe("CustomAnswered");
    expect(afterCustom[0]?.customAnswerText).toContain("internal batch identifier");

    // 4. Correct previous answer -> status becomes Answered with full history
    const afterCorrect = applyQuestionResolution(afterCustom, firstQ.questionId, {
      kind: "Correct",
      optionId: "identifier"
    });
    expect(afterCorrect[0]?.status).toBe("Answered");
    expect(afterCorrect[0]?.selectedOptionId).toBe("identifier");
    expect(afterCorrect[0]?.answerHistory).toHaveLength(4);
  });

  it("separates insights into all 5 epistemic categories and answers grounded analytical questions with source citations", () => {
    const ingestion = ingestWorkbookStructure([
      {
        name: "Branch_Financials",
        grid: [
          ["Branch", "Month", "Revenue_EGP", "Target_EGP"],
          ["Cairo", "2026-01-01", "2000", "1800"],
          ["Alexandria", "2026-02-01", "1200", "1300"],
          ["Cairo", "2026-03-01", "2600", "2200"],
          ["Giza", "2026-04-01", "900", "1000"],
          ["Cairo", "2026-05-01", "9500", "3000"]
        ]
      }
    ]);

    const model = buildSemanticWorkbookModel(ingestion);
    const report = buildVerifiedAnalysisReport(model);

    expect(report.insightsByEpistemicCategory.ConfirmedFact.length).toBeGreaterThan(0);
    expect(report.insightsByEpistemicCategory.DataSupportedObservation.length).toBeGreaterThan(0);
    expect(report.insightsByEpistemicCategory.PotentialExplanation.length).toBeGreaterThan(0);
    expect(report.insightsByEpistemicCategory.UnsupportedConclusion.length).toBeGreaterThan(0);
    expect(report.forecasts).toHaveLength(1);
    expect(report.forecasts[0]?.explicitAssumptions.length).toBeGreaterThanOrEqual(2);

    // Grounded Conversational Assistant checks
    const summaryAns = answerAnalyticalQuestion("What does this workbook contain?", model, report);
    expect(summaryAns.intent).toBe("WorkbookSummary");
    expect(summaryAns.answerText).toContain("1 worksheet");
    expect(summaryAns.citations.length).toBeGreaterThan(0);

    const rankAns = answerAnalyticalQuestion(
      "Which branch performed best and worst?",
      model,
      report
    );
    expect(rankAns.intent).toBe("PerformanceRanking");
    expect(rankAns.answerText).toContain("Cairo");
    expect(rankAns.answerText).toContain("Giza");

    const changeAns = answerAnalyticalQuestion(
      "Why did Revenue_EGP change over time?",
      model,
      report
    );
    expect(changeAns.intent).toBe("MetricChangeExplanation");
    expect(changeAns.bulletPoints.some((b) => b.includes("[PotentialExplanation]"))).toBe(true);

    const anomalyAns = answerAnalyticalQuestion(
      "Are there any anomalies or outliers?",
      model,
      report
    );
    expect(anomalyAns.intent).toBe("AnomaliesAndQuality");
    expect(anomalyAns.bulletPoints.some((b) => b.includes("9500"))).toBe(true);

    const boundaryAns = answerAnalyticalQuestion(
      "What cannot be concluded or proven from this data?",
      model,
      report
    );
    expect(boundaryAns.intent).toBe("EpistemicBoundaries");
    expect(boundaryAns.cannotConcludeNotes.length).toBeGreaterThan(0);
  });
});
