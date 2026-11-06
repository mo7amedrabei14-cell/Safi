/**
 * `apps/web/src/utils/aggregated-questions.ts` — Aggregated questions and impact-ranked review card engine (Section 16.4).
 */

export interface DataIssueItem {
  id: string;
  category: "schema" | "ambiguity" | "outlier" | "type";
  titleAr: string;
  impactScore: number;
  deferred: boolean;
}

export function aggregateAndRankQuestions(issues: DataIssueItem[]): DataIssueItem[] {
  const active = issues.filter((i) => !i.deferred);
  active.sort((a, b) => b.impactScore - a.impactScore);
  return active.slice(0, 3);
}
