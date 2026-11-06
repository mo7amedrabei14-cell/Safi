/**
 * `apps/web/src/utils/hard-files-benchmark.ts` — Benchmark framework for hard files understanding and low-confidence alerting (Section 16.6).
 */

export interface BenchmarkResult {
  totalFilesTested: number;
  typeAccuracy: number;
  lowConfidenceAlertRate: number;
  passed: boolean;
}

export function runHardFilesBenchmark(
  sampleFiles: Array<{
    name: string;
    expectedTypes: string[];
    detectedTypes: string[];
    confidenceScores: number[];
  }>
): BenchmarkResult {
  let correctTypes = 0;
  let totalTypes = 0;
  let lowConfidenceAlerts = 0;
  let totalColumns = 0;

  for (const file of sampleFiles) {
    for (let i = 0; i < file.expectedTypes.length; i++) {
      totalTypes++;
      if (file.expectedTypes[i] === file.detectedTypes[i]) {
        correctTypes++;
      }
    }
    for (const score of file.confidenceScores) {
      totalColumns++;
      if (score < 0.75) {
        lowConfidenceAlerts++;
      }
    }
  }

  const typeAccuracy = totalTypes > 0 ? correctTypes / totalTypes : 1.0;
  const lowConfidenceAlertRate = totalColumns > 0 ? lowConfidenceAlerts / totalColumns : 0.0;

  return {
    totalFilesTested: sampleFiles.length,
    typeAccuracy,
    lowConfidenceAlertRate,
    passed: typeAccuracy >= 0.9
  };
}
