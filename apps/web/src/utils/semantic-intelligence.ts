/**
 * Universal Data Intelligence Engine & Canonical Shared Semantic Model (`Phase 2`).
 *
 * Consumes a `WorkbookIngestionReport` (`Phase 1`) and produces a single canonical
 * `SemanticWorkbookModel` used across:
 * - Contextual Question & Confidence System (`Phase 3`)
 * - Conversational Data Assistant & Verified Analysis Planner (`Phases 3 & 4`)
 * - Dashboard Studio (`Phase 5`), Presentation Studio (`Phase 6`), Document Studio (`Phase 7`)
 */

import type {
  DetectedTableRegion,
  IngestedCellDataType,
  WorkbookIngestionReport
} from "./workbook-ingestion";

export type ConfidenceTier = "High" | "Medium" | "Low" | "Unknown";

export type ColumnAnalyticalRole =
  | "Measure"
  | "Dimension"
  | "Identifier"
  | "Temporal"
  | "DescriptiveText";

export type SemanticColumnType =
  | "CurrencyAmount"
  | "Quantity"
  | "PercentageRate"
  | "UnitPrice"
  | "ScoreOrIndex"
  | "IdentifierCode"
  | "PersonName"
  | "OrganizationOrBranch"
  | "GeographicLocation"
  | "CategoryOrStatus"
  | "CalendarDate"
  | "Timestamp"
  | "PhoneOrContact"
  | "EmailAddress"
  | "NationalIdOrTaxId"
  | "FreeTextNotes"
  | "GeneralMetric"
  | "GeneralDimension";

export type PiiSensitivityLevel = "None" | "InternalIdentifier" | "PersonalContact" | "HighRiskPii";

export interface NumericDistributionStats {
  readonly count: number;
  readonly sum: number;
  readonly min: number;
  readonly max: number;
  readonly mean: number;
  readonly median: number;
  readonly stdDev: number;
  readonly q1: number;
  readonly q2: number;
  readonly q3: number;
  readonly iqr: number;
  readonly outlierCount: number;
  readonly outlierValues: readonly number[];
}

export interface DateRangeStats {
  readonly minDateIso: string;
  readonly maxDateIso: string;
  readonly spanDays: number;
  readonly granularity: "Daily" | "Weekly" | "Monthly" | "Quarterly" | "Yearly" | "Irregular";
}

export interface FrequencyValueItem {
  readonly value: string;
  readonly count: number;
  readonly sharePct: number;
}

export interface SemanticColumnProfile {
  readonly columnId: string;
  readonly tableId: string;
  readonly sheetName: string;
  readonly columnIndex: number;
  readonly headerName: string;
  readonly inferredMeaning: string;
  readonly storageType: IngestedCellDataType;
  readonly semanticType: SemanticColumnType;
  readonly analyticalRole: ColumnAnalyticalRole;
  readonly isIdentifier: boolean;
  readonly aggregationAllowed: boolean;
  readonly recommendedAggregation:
    | "SUM"
    | "AVERAGE"
    | "MEDIAN"
    | "COUNT_DISTINCT"
    | "COUNT"
    | "NONE";
  readonly unit?: string | undefined;
  readonly currencyCode?: string | undefined;
  readonly piiSensitivity: PiiSensitivityLevel;
  readonly totalRows: number;
  readonly nonNullCount: number;
  readonly missingCount: number;
  readonly missingPct: number;
  readonly distinctCount: number;
  readonly uniquenessPct: number;
  readonly duplicateCount: number;
  readonly topValues: readonly FrequencyValueItem[];
  readonly numericStats?: NumericDistributionStats | undefined;
  readonly dateStats?: DateRangeStats | undefined;
  readonly qualityScorePct: number;
  readonly confidencePct: number;
  readonly confidenceTier: ConfidenceTier;
}

export type DataQualityIssueCategory =
  | "ConfirmedIssue"
  | "PotentialIssue"
  | "Information"
  | "NeedsClarification";

export type ReversibleCleaningActionType =
  | "TrimAndNormalizeArabic"
  | "DeduplicateExactRows"
  | "StandardizeIsoDates"
  | "ImputeMissingWithMedian"
  | "MaskSensitivePii"
  | "ExcludeFormulaErrorRows"
  | "ConfirmColumnSemanticRole";

export interface DataQualityFinding {
  readonly findingId: string;
  readonly category: DataQualityIssueCategory;
  readonly sheetName: string;
  readonly tableId: string;
  readonly columnName?: string | undefined;
  readonly title: string;
  readonly description: string;
  readonly affectedRowCount: number;
  readonly sampleRowIndices: readonly number[];
  readonly suggestedAction?:
    | {
        readonly actionType: ReversibleCleaningActionType;
        readonly label: string;
        readonly previewSummary: string;
        readonly reversible: true;
      }
    | undefined;
}

export type RelationshipCardinality = "1:1" | "1:N" | "N:1" | "M:N";

export interface DiscoveredTableRelationship {
  readonly relationshipId: string;
  readonly fromSheet: string;
  readonly fromTableId: string;
  readonly fromColumn: string;
  readonly toSheet: string;
  readonly toTableId: string;
  readonly toColumn: string;
  readonly cardinality: RelationshipCardinality;
  readonly matchRatePct: number;
  readonly matchedDistinctKeys: number;
  readonly orphanSourceKeyCount: number;
  readonly duplicateTargetKeyCount: number;
  readonly confidencePct: number;
  readonly confidenceTier: ConfidenceTier;
  readonly needsConfirmation: boolean;
  readonly explanation: string;
}

export interface CanonicalKpiMetric {
  readonly metricId: string;
  readonly label: string;
  readonly sheetName: string;
  readonly tableId: string;
  readonly columnName: string;
  readonly aggregation: "SUM" | "AVERAGE" | "MEDIAN" | "MIN" | "MAX" | "COUNT_DISTINCT";
  readonly rawValue: number;
  readonly formattedValue: string;
  readonly unit?: string | undefined;
  readonly currencyCode?: string | undefined;
  readonly formulaTrace: string;
  readonly confidencePct: number;
  readonly confidenceTier: ConfidenceTier;
  readonly provisional: boolean;
}

export interface SemanticTableProfile {
  readonly tableId: string;
  readonly title: string;
  readonly sheetName: string;
  readonly a1Range: string;
  readonly rowCount: number;
  readonly columnCount: number;
  readonly exactDuplicateRowCount: number;
  readonly overallQualityScorePct: number;
  readonly columns: readonly SemanticColumnProfile[];
  readonly candidatePrimaryKeyColumns: readonly string[];
}

export interface SemanticWorkbookModel {
  readonly modelVersion: "2.0.0";
  readonly ingestionReport: WorkbookIngestionReport;
  readonly tables: readonly SemanticTableProfile[];
  readonly relationships: readonly DiscoveredTableRelationship[];
  readonly qualityFindings: readonly DataQualityFinding[];
  readonly canonicalKpis: readonly CanonicalKpiMetric[];
  readonly overallQualityScorePct: number;
  readonly overallConfidencePct: number;
  readonly overallConfidenceTier: ConfidenceTier;
}

export function scoreToConfidenceTier(scorePct: number): ConfidenceTier {
  if (scorePct >= 85) return "High";
  if (scorePct >= 65) return "Medium";
  if (scorePct >= 40) return "Low";
  return "Unknown";
}

const ID_HEADER_PATTERN =
  /(^id$|_id$|\bid\b|code|uuid|guid|serial|account_no|invoice_no|order_no|emp_no|national_id|ssn|iban|phone|mobile|tel|zip|postal|كود|رمز|معرف|رقم الهوية|رقم القومي|رقم الهاتف|جوال|موبايل|رقم الفاتورة|رقم الطلب|رقم الحساب)/i;

const CURRENCY_HEADER_PATTERN =
  /(revenue|sales|amount|price|cost|expense|budget|salary|profit|total|balance|fee|tax|discount|إيراد|مبيعات|مبلغ|سعر|تكلفة|مصروف|ميزانية|راتب|ربح|رصيد|ضريبة|خصم|قيمة)/i;

const QUANTITY_HEADER_PATTERN =
  /(qty|quantity|units|volume|count|headcount|items|hours|days|weight|كمية|عدد|وحدات|ساعات|أيام|وزن|حجم)/i;

const PERCENT_HEADER_PATTERN =
  /(rate|ratio|pct|percent|margin|share|growth|utilization|نسبة|معدل|هامش|حصة|نمو)/i;

const DATE_HEADER_PATTERN =
  /(date|time|day|month|year|quarter|period|created|updated|تاريخ|وقت|يوم|شهر|سنة|عام|فترة|الربع)/i;

const LOCATION_HEADER_PATTERN =
  /(country|city|state|region|branch|governorate|district|location|address|دولة|مدينة|محافظة|منطقة|فرع|عنوان|موقع)/i;

const PERSON_HEADER_PATTERN =
  /(name|employee|customer|client|manager|owner|beneficiary|اسم|موظف|عميل|مدير|مستفيد|مسؤول)/i;

const ORG_HEADER_PATTERN =
  /(department|division|company|vendor|supplier|agency|ministry|قسم|إدارة|شركة|مورد|جهة|وزارة|قطاع)/i;

const CATEGORY_HEADER_PATTERN =
  /(status|type|category|segment|tier|grade|priority|channel|gender|حالة|نوع|فئة|تصنيف|درجة|أولوية|قناة|جنس)/i;

function detectCurrencyAndUnit(
  header: string,
  sampleValues: readonly string[],
  numberFormat?: string
): { currencyCode?: string | undefined; unit?: string | undefined } {
  const combined = `${header} ${numberFormat ?? ""} ${sampleValues.slice(0, 20).join(" ")}`;
  if (/EGP|ج\.م|جنيه/i.test(combined)) return { currencyCode: "EGP", unit: "EGP" };
  if (/SAR|ر\.س|ريال/i.test(combined)) return { currencyCode: "SAR", unit: "SAR" };
  if (/AED|د\.إ|درهم/i.test(combined)) return { currencyCode: "AED", unit: "AED" };
  if (/\$|USD|دولار/i.test(combined)) return { currencyCode: "USD", unit: "USD" };
  if (/€|EUR|يورو/i.test(combined)) return { currencyCode: "EUR", unit: "EUR" };
  if (/%|percent|نسبة/i.test(combined)) return { unit: "%" };
  if (/\bkg\b|كجم/i.test(combined)) return { unit: "kg" };
  if (/\bhrs?\b|hours|ساعة/i.test(combined)) return { unit: "hours" };
  return {};
}

function detectPiiSensitivity(
  header: string,
  sampleValues: readonly string[]
): PiiSensitivityLevel {
  const lower = header.toLowerCase();
  if (/national_id|ssn|passport|iban|credit_card|رقم قومي|هوية وطنية|جواز سفر/i.test(lower)) {
    return "HighRiskPii";
  }
  const hasEmail = sampleValues.some((v) => /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(v));
  const hasPhone = sampleValues.some((v) => /^\+?\d[\d\s-]{8,14}$/.test(v));
  if (hasEmail || hasPhone || /email|phone|mobile|بريد|هاتف|جوال|موبايل/i.test(lower)) {
    return "PersonalContact";
  }
  if (ID_HEADER_PATTERN.test(header)) {
    return "InternalIdentifier";
  }
  return "None";
}

function computeNumericStats(numbers: readonly number[]): NumericDistributionStats | undefined {
  if (numbers.length === 0) return undefined;
  const sorted = [...numbers].sort((a, b) => a - b);
  const n = sorted.length;
  const sum = sorted.reduce((acc, v) => acc + v, 0);
  const mean = sum / n;
  const min = sorted[0] ?? 0;
  const max = sorted[n - 1] ?? 0;

  const percentile = (p: number): number => {
    if (n === 1) return sorted[0] ?? 0;
    const pos = p * (n - 1);
    const base = Math.floor(pos);
    const rest = pos - base;
    const lower = sorted[base] ?? 0;
    const upper = sorted[Math.min(n - 1, base + 1)] ?? lower;
    return lower + rest * (upper - lower);
  };

  const q1 = percentile(0.25);
  const median = percentile(0.5);
  const q3 = percentile(0.75);
  const iqr = q3 - q1;
  const variance = sorted.reduce((acc, v) => acc + (v - mean) ** 2, 0) / n;
  const stdDev = Math.sqrt(variance);

  const lowerFence = q1 - 1.5 * iqr;
  const upperFence = q3 + 1.5 * iqr;
  const outlierValues =
    iqr > 0
      ? sorted.filter((v) => v < lowerFence || v > upperFence)
      : stdDev > 0
        ? sorted.filter((v) => Math.abs(v - mean) > 3 * stdDev)
        : [];

  return {
    count: n,
    sum: Number(sum.toFixed(4)),
    min: Number(min.toFixed(4)),
    max: Number(max.toFixed(4)),
    mean: Number(mean.toFixed(4)),
    median: Number(median.toFixed(4)),
    stdDev: Number(stdDev.toFixed(4)),
    q1: Number(q1.toFixed(4)),
    q2: Number(median.toFixed(4)),
    q3: Number(q3.toFixed(4)),
    iqr: Number(iqr.toFixed(4)),
    outlierCount: outlierValues.length,
    outlierValues: outlierValues.slice(0, 10)
  };
}

function computeDateRangeStats(rawDates: readonly string[]): DateRangeStats | undefined {
  const timestamps: number[] = [];
  for (const s of rawDates) {
    const parsed = Date.parse(s);
    if (Number.isFinite(parsed)) {
      timestamps.push(parsed);
    }
  }
  if (timestamps.length === 0) return undefined;
  timestamps.sort((a, b) => a - b);
  const minTs = timestamps[0]!;
  const maxTs = timestamps[timestamps.length - 1]!;
  const spanDays = Math.max(0, Math.round((maxTs - minTs) / 86_400_000));

  const uniqueSorted = [...new Set(timestamps)];
  let avgGapDays = spanDays;
  if (uniqueSorted.length > 1) {
    let totalGap = 0;
    for (let i = 1; i < uniqueSorted.length; i += 1) {
      totalGap += (uniqueSorted[i]! - uniqueSorted[i - 1]!) / 86_400_000;
    }
    avgGapDays = totalGap / (uniqueSorted.length - 1);
  }

  const granularity: DateRangeStats["granularity"] =
    avgGapDays <= 2
      ? "Daily"
      : avgGapDays <= 10
        ? "Weekly"
        : avgGapDays <= 40
          ? "Monthly"
          : avgGapDays <= 110
            ? "Quarterly"
            : avgGapDays <= 400
              ? "Yearly"
              : "Irregular";

  return {
    minDateIso: new Date(minTs).toISOString().slice(0, 10),
    maxDateIso: new Date(maxTs).toISOString().slice(0, 10),
    spanDays,
    granularity
  };
}

function parseNumericCell(val: string): number | null {
  const cleaned = val
    .trim()
    .replace(/^[$€£]\s*/, "")
    .replace(/\s*(EGP|SAR|AED|USD|EUR|ج\.م|ر\.س|%)\s*$/i, "")
    .replace(/,/g, "");
  if (!cleaned || !/^-?\d+(\.\d+)?$/.test(cleaned)) return null;
  const n = Number(cleaned);
  return Number.isFinite(n) ? n : null;
}

function profileTableRegion(
  region: DetectedTableRegion,
  findingsOut: DataQualityFinding[]
): SemanticTableProfile {
  const totalRows = region.dataRows.length;
  const seenRowSignatures = new Set<string>();
  const duplicateRowIndices: number[] = [];

  region.dataRows.forEach((row, idx) => {
    const sig = row.map((c) => c.trim().toLowerCase()).join("\u001F");
    if (seenRowSignatures.has(sig)) {
      duplicateRowIndices.push(region.sourceRowIndices[idx] ?? idx);
    } else {
      seenRowSignatures.add(sig);
    }
  });

  if (duplicateRowIndices.length > 0) {
    findingsOut.push({
      findingId: `dq_${region.tableId}_dup_rows`,
      category: "ConfirmedIssue",
      sheetName: region.sheetName,
      tableId: region.tableId,
      title: `Exact duplicate rows detected in ${region.title}`,
      description: `${duplicateRowIndices.length} row(s) are exact duplicates of earlier records in ${region.sheetName}.`,
      affectedRowCount: duplicateRowIndices.length,
      sampleRowIndices: duplicateRowIndices.slice(0, 5),
      suggestedAction: {
        actionType: "DeduplicateExactRows",
        label: "Remove exact duplicate rows",
        previewSummary: `Deduplicate ${duplicateRowIndices.length} duplicate record(s) while preserving first occurrence`,
        reversible: true
      }
    });
  }

  for (const sep of region.separatedRows) {
    if (sep.kind === "Subtotal" || sep.kind === "GrandTotal") {
      findingsOut.push({
        findingId: `dq_${region.tableId}_sep_${sep.sheetRow}`,
        category: "Information",
        sheetName: region.sheetName,
        tableId: region.tableId,
        title: `${sep.kind} row isolated from analytical records`,
        description: `Row ${sep.sheetRow + 1} (${sep.values[0] ?? sep.kind}) was separated automatically so KPIs and charts never double-count totals.`,
        affectedRowCount: 1,
        sampleRowIndices: [sep.sheetRow]
      });
    }
  }

  const columns: SemanticColumnProfile[] = region.headers.map((header, colIdx) => {
    const rawCells = region.dataRows.map((r) => (r[colIdx] ?? "").trim());
    const nonNullCells = rawCells.filter(
      (v) => v.length > 0 && !/^(null|nil|n\/a|na|-|--|—|غير متوفر|لا يوجد)$/i.test(v)
    );
    const nonNullCount = nonNullCells.length;
    const missingCount = totalRows - nonNullCount;
    const missingPct = totalRows > 0 ? Number(((missingCount / totalRows) * 100).toFixed(2)) : 0;

    const freqMap = new Map<string, number>();
    for (const v of nonNullCells) {
      freqMap.set(v, (freqMap.get(v) ?? 0) + 1);
    }
    const distinctCount = freqMap.size;
    const uniquenessPct =
      nonNullCount > 0 ? Number(((distinctCount / nonNullCount) * 100).toFixed(2)) : 0;
    const duplicateCount = Math.max(0, nonNullCount - distinctCount);

    const topValues: FrequencyValueItem[] = [...freqMap.entries()]
      .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
      .slice(0, 8)
      .map(([value, count]) => ({
        value,
        count,
        sharePct: nonNullCount > 0 ? Number(((count / nonNullCount) * 100).toFixed(2)) : 0
      }));

    const leadingZeroCount = nonNullCells.filter(
      (v) => (v.length > 1 && v.startsWith("0") && /^\d+$/.test(v)) || /^\+\d{5,15}$/.test(v)
    ).length;
    const formulaErrorCount = nonNullCells.filter((v) => v.startsWith("#")).length;
    const numericVals = nonNullCells.map(parseNumericCell).filter((v): v is number => v !== null);
    const dateVals = nonNullCells.filter(
      (v) => /^\d{4}[-/]\d{1,2}[-/]\d{1,2}/.test(v) || /^\d{1,2}[-/]\d{1,2}[-/]\d{4}/.test(v)
    );

    const isNumericDominant =
      nonNullCount > 0 && numericVals.length / nonNullCount >= 0.8 && leadingZeroCount === 0;
    const isDateDominant = nonNullCount > 0 && dateVals.length / nonNullCount >= 0.8;

    const { currencyCode, unit } = detectCurrencyAndUnit(header, nonNullCells);
    const piiSensitivity = detectPiiSensitivity(header, nonNullCells);

    // Determine if this column is an Identifier (NEVER aggregate IDs/codes/phones!)
    const headerSaysId = ID_HEADER_PATTERN.test(header);
    const headerSaysMeasure =
      CURRENCY_HEADER_PATTERN.test(header) ||
      QUANTITY_HEADER_PATTERN.test(header) ||
      PERCENT_HEADER_PATTERN.test(header);

    const looksLikeSequentialOrCodeId =
      headerSaysId ||
      leadingZeroCount > 0 ||
      piiSensitivity === "HighRiskPii" ||
      piiSensitivity === "PersonalContact" ||
      (!headerSaysMeasure &&
        isNumericDominant &&
        uniquenessPct >= 98 &&
        totalRows >= 5 &&
        numericVals.every((n) => Number.isInteger(n) && n >= 1));

    let analyticalRole: ColumnAnalyticalRole = "Dimension";
    let semanticType: SemanticColumnType = "GeneralDimension";
    let storageType: IngestedCellDataType = "Text";
    let confidencePct = 88;

    if (looksLikeSequentialOrCodeId) {
      analyticalRole = "Identifier";
      semanticType =
        piiSensitivity === "HighRiskPii"
          ? "NationalIdOrTaxId"
          : piiSensitivity === "PersonalContact"
            ? /@/i.test(nonNullCells[0] ?? "")
              ? "EmailAddress"
              : "PhoneOrContact"
            : "IdentifierCode";
      storageType =
        leadingZeroCount > 0 ? "ZeroPreservedText" : isNumericDominant ? "Integer" : "Text";
      confidencePct = headerSaysId || leadingZeroCount > 0 ? 96 : 72;
    } else if (isDateDominant || (DATE_HEADER_PATTERN.test(header) && dateVals.length > 0)) {
      analyticalRole = "Temporal";
      semanticType = nonNullCells.some((v) => v.includes(":")) ? "Timestamp" : "CalendarDate";
      storageType = semanticType === "Timestamp" ? "DateTime" : "Date";
      confidencePct = 94;
    } else if (isNumericDominant) {
      analyticalRole = "Measure";
      if (currencyCode || CURRENCY_HEADER_PATTERN.test(header)) {
        semanticType = /price|سعر/i.test(header) ? "UnitPrice" : "CurrencyAmount";
        storageType = "Currency";
        confidencePct = 95;
      } else if (unit === "%" || PERCENT_HEADER_PATTERN.test(header)) {
        semanticType = "PercentageRate";
        storageType = "Percentage";
        confidencePct = 93;
      } else if (QUANTITY_HEADER_PATTERN.test(header)) {
        semanticType = "Quantity";
        storageType = numericVals.every(Number.isInteger) ? "Integer" : "Decimal";
        confidencePct = 92;
      } else {
        semanticType = "GeneralMetric";
        storageType = numericVals.every(Number.isInteger) ? "Integer" : "Decimal";
        confidencePct = 76;
      }
    } else {
      if (LOCATION_HEADER_PATTERN.test(header)) {
        semanticType = "GeographicLocation";
        confidencePct = 92;
      } else if (PERSON_HEADER_PATTERN.test(header)) {
        semanticType = "PersonName";
        confidencePct = 90;
      } else if (ORG_HEADER_PATTERN.test(header)) {
        semanticType = "OrganizationOrBranch";
        confidencePct = 91;
      } else if (
        CATEGORY_HEADER_PATTERN.test(header) ||
        distinctCount <= Math.max(12, totalRows * 0.3)
      ) {
        semanticType = "CategoryOrStatus";
        confidencePct = 86;
      } else {
        analyticalRole = "DescriptiveText";
        semanticType = "FreeTextNotes";
        confidencePct = 80;
      }
    }

    // Only compute numeric aggregations if the column is a genuine Measure (NEVER for IDs!)
    const aggregationAllowed = analyticalRole === "Measure";
    const numericStats = aggregationAllowed ? computeNumericStats(numericVals) : undefined;
    const dateStats = analyticalRole === "Temporal" ? computeDateRangeStats(dateVals) : undefined;

    const recommendedAggregation: SemanticColumnProfile["recommendedAggregation"] =
      !aggregationAllowed
        ? analyticalRole === "Identifier"
          ? "COUNT_DISTINCT"
          : "NONE"
        : semanticType === "PercentageRate" || semanticType === "UnitPrice"
          ? "AVERAGE"
          : "SUM";

    // Quality findings for this column
    if (formulaErrorCount > 0) {
      findingsOut.push({
        findingId: `dq_${region.tableId}_col_${colIdx}_ferr`,
        category: "ConfirmedIssue",
        sheetName: region.sheetName,
        tableId: region.tableId,
        columnName: header,
        title: `Formula error values in column "${header}"`,
        description: `${formulaErrorCount} cell(s) in "${header}" contain Excel formula error tokens (#DIV/0!, #REF!, etc.).`,
        affectedRowCount: formulaErrorCount,
        sampleRowIndices: [0],
        suggestedAction: {
          actionType: "ExcludeFormulaErrorRows",
          label: "Isolate rows with formula errors",
          previewSummary: `Quarantine ${formulaErrorCount} formula-error cell(s) before KPI aggregation`,
          reversible: true
        }
      });
    }

    if (missingPct > 0) {
      findingsOut.push({
        findingId: `dq_${region.tableId}_col_${colIdx}_missing`,
        category: missingPct >= 20 ? "PotentialIssue" : "Information",
        sheetName: region.sheetName,
        tableId: region.tableId,
        columnName: header,
        title: `Missing values in "${header}" (${missingPct}%)`,
        description: `${missingCount} of ${totalRows} record(s) have empty or sentinel values in "${header}".`,
        affectedRowCount: missingCount,
        sampleRowIndices: [],
        suggestedAction:
          aggregationAllowed && numericStats
            ? {
                actionType: "ImputeMissingWithMedian",
                label: `Fill missing "${header}" with median (${numericStats.median})`,
                previewSummary: `Impute ${missingCount} missing value(s) using column median ${numericStats.median}`,
                reversible: true
              }
            : undefined
      });
    }

    if (numericStats && numericStats.outlierCount > 0) {
      findingsOut.push({
        findingId: `dq_${region.tableId}_col_${colIdx}_outliers`,
        category: "PotentialIssue",
        sheetName: region.sheetName,
        tableId: region.tableId,
        columnName: header,
        title: `Statistical outliers in "${header}"`,
        description: `${numericStats.outlierCount} value(s) in "${header}" fall outside the IQR fence (${numericStats.outlierValues.join(", ")}).`,
        affectedRowCount: numericStats.outlierCount,
        sampleRowIndices: []
      });
    }

    if (!headerSaysId && !headerSaysMeasure && isNumericDominant && uniquenessPct >= 80) {
      confidencePct = Math.min(confidencePct, 62);
      findingsOut.push({
        findingId: `dq_${region.tableId}_col_${colIdx}_ambig`,
        category: "NeedsClarification",
        sheetName: region.sheetName,
        tableId: region.tableId,
        columnName: header,
        title: `Clarify role of numeric column "${header}"`,
        description: `"${header}" contains mostly unique numbers (${uniquenessPct}% unique). Confirm whether it is a reference code/identifier or an aggregatable metric.`,
        affectedRowCount: totalRows,
        sampleRowIndices: [],
        suggestedAction: {
          actionType: "ConfirmColumnSemanticRole",
          label: `Confirm whether "${header}" is an Identifier or Measure`,
          previewSummary: `Prevent accidental summation if "${header}" is an identifier code`,
          reversible: true
        }
      });
    }

    const qualityPenalty =
      missingPct * 0.5 +
      (formulaErrorCount > 0 ? 25 : 0) +
      (numericStats && numericStats.outlierCount > 0 ? 5 : 0);
    const qualityScorePct = Math.max(0, Math.min(100, Math.round(100 - qualityPenalty)));

    const inferredMeaning =
      analyticalRole === "Identifier"
        ? `Unique or reference identifier (${semanticType}) — protected from numeric averaging/summing`
        : analyticalRole === "Measure"
          ? `Quantitative ${semanticType}${currencyCode ? ` (${currencyCode})` : unit ? ` (${unit})` : ""} suitable for ${recommendedAggregation}`
          : analyticalRole === "Temporal"
            ? `Temporal dimension (${dateStats?.granularity ?? "Date"}: ${dateStats?.minDateIso ?? ""} to ${dateStats?.maxDateIso ?? ""})`
            : `Categorical/descriptive ${semanticType} (${distinctCount} distinct values)`;

    return {
      columnId: `${region.tableId}_col_${colIdx + 1}`,
      tableId: region.tableId,
      sheetName: region.sheetName,
      columnIndex: colIdx,
      headerName: header,
      inferredMeaning,
      storageType,
      semanticType,
      analyticalRole,
      isIdentifier: analyticalRole === "Identifier",
      aggregationAllowed,
      recommendedAggregation,
      unit,
      currencyCode,
      piiSensitivity,
      totalRows,
      nonNullCount,
      missingCount,
      missingPct,
      distinctCount,
      uniquenessPct,
      duplicateCount,
      topValues,
      numericStats,
      dateStats,
      qualityScorePct,
      confidencePct,
      confidenceTier: scoreToConfidenceTier(confidencePct)
    };
  });

  const candidatePrimaryKeyColumns = columns
    .filter((c) => c.nonNullCount === totalRows && c.uniquenessPct === 100 && totalRows > 0)
    .map((c) => c.headerName);

  const avgColQuality =
    columns.length > 0
      ? Math.round(columns.reduce((acc, c) => acc + c.qualityScorePct, 0) / columns.length)
      : 100;

  return {
    tableId: region.tableId,
    title: region.title,
    sheetName: region.sheetName,
    a1Range: region.a1Range,
    rowCount: totalRows,
    columnCount: columns.length,
    exactDuplicateRowCount: duplicateRowIndices.length,
    overallQualityScorePct: avgColQuality,
    columns,
    candidatePrimaryKeyColumns
  };
}

function normalizeHeaderToken(header: string): string {
  return header
    .toLowerCase()
    .replace(/[_\-\s]+/g, "")
    .replace(/(id|code|key|no|num|كود|رمز|معرف|رقم)$/g, "");
}

function discoverCrossTableRelationships(
  tables: readonly SemanticTableProfile[],
  rawRegionsById: ReadonlyMap<string, DetectedTableRegion>
): DiscoveredTableRelationship[] {
  const relationships: DiscoveredTableRelationship[] = [];

  for (let i = 0; i < tables.length; i += 1) {
    for (let j = i + 1; j < tables.length; j += 1) {
      const tableA = tables[i]!;
      const tableB = tables[j]!;
      const regionA = rawRegionsById.get(tableA.tableId);
      const regionB = rawRegionsById.get(tableB.tableId);
      if (!regionA || !regionB || tableA.rowCount === 0 || tableB.rowCount === 0) continue;

      for (const colA of tableA.columns) {
        if (colA.analyticalRole === "Measure" && !colA.isIdentifier) continue;
        if (colA.distinctCount < 2) continue;

        const valsA = regionA.dataRows
          .map((r) => (r[colA.columnIndex] ?? "").trim())
          .filter((v) => v.length > 0);
        const setA = new Set(valsA);

        for (const colB of tableB.columns) {
          if (colB.analyticalRole === "Measure" && !colB.isIdentifier) continue;
          if (colB.distinctCount < 2) continue;

          // Ensure compatible storage/semantic types
          const bothIds = colA.isIdentifier && colB.isIdentifier;
          const sameSemantic = colA.semanticType === colB.semanticType;
          const tokenA = normalizeHeaderToken(colA.headerName);
          const tokenB = normalizeHeaderToken(colB.headerName);
          const nameRelated =
            colA.headerName.toLowerCase() === colB.headerName.toLowerCase() ||
            (tokenA.length >= 3 && tokenA === tokenB);

          if (!bothIds && !sameSemantic && !nameRelated) continue;

          const valsB = regionB.dataRows
            .map((r) => (r[colB.columnIndex] ?? "").trim())
            .filter((v) => v.length > 0);
          const setB = new Set(valsB);

          let matchedDistinct = 0;
          for (const k of setA) {
            if (setB.has(k)) matchedDistinct += 1;
          }
          if (matchedDistinct === 0) continue;

          const minDistinct = Math.min(setA.size, setB.size);
          const matchRatePct = Number(((matchedDistinct / minDistinct) * 100).toFixed(2));
          if (matchRatePct < 50) continue;

          const uniqueA = colA.uniquenessPct === 100 && colA.missingCount === 0;
          const uniqueB = colB.uniquenessPct === 100 && colB.missingCount === 0;

          const cardinality: RelationshipCardinality =
            uniqueA && uniqueB ? "1:1" : uniqueB ? "N:1" : uniqueA ? "1:N" : "M:N";

          const orphanSourceKeyCount = [...setA].filter((k) => !setB.has(k)).length;
          const duplicateTargetKeyCount = colB.duplicateCount;

          let confidencePct = Math.round(
            matchRatePct * 0.65 + (uniqueA || uniqueB ? 25 : 5) + (nameRelated ? 10 : 0)
          );
          confidencePct = Math.min(98, Math.max(40, confidencePct));

          const needsConfirmation =
            cardinality === "M:N" || matchRatePct < 80 || orphanSourceKeyCount > 0;

          relationships.push({
            relationshipId: `rel_${tableA.tableId}_${colA.columnIndex}_${tableB.tableId}_${colB.columnIndex}`,
            fromSheet: tableA.sheetName,
            fromTableId: tableA.tableId,
            fromColumn: colA.headerName,
            toSheet: tableB.sheetName,
            toTableId: tableB.tableId,
            toColumn: colB.headerName,
            cardinality,
            matchRatePct,
            matchedDistinctKeys: matchedDistinct,
            orphanSourceKeyCount,
            duplicateTargetKeyCount,
            confidencePct,
            confidenceTier: scoreToConfidenceTier(confidencePct),
            needsConfirmation,
            explanation: `${matchedDistinct} distinct key(s) match (${matchRatePct}% overlap) with ${cardinality} cardinality (${orphanSourceKeyCount} unmatched in source, ${duplicateTargetKeyCount} duplicates in target).`
          });
        }
      }
    }
  }

  return relationships;
}

function formatKpiNumber(val: number, currencyCode?: string, unit?: string): string {
  const formatted = Number.isInteger(val)
    ? val.toLocaleString("en-US")
    : val.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
  if (currencyCode) return `${formatted} ${currencyCode}`;
  if (unit === "%") return `${formatted}%`;
  if (unit) return `${formatted} ${unit}`;
  return formatted;
}

/**
 * Builds the canonical `SemanticWorkbookModel` from a `WorkbookIngestionReport`.
 */
export function buildSemanticWorkbookModel(
  ingestionReport: WorkbookIngestionReport
): SemanticWorkbookModel {
  const qualityFindings: DataQualityFinding[] = [];
  const rawRegionsById = new Map<string, DetectedTableRegion>();
  const tables: SemanticTableProfile[] = [];

  for (const ws of ingestionReport.worksheets) {
    for (const region of ws.tables) {
      rawRegionsById.set(region.tableId, region);
      tables.push(profileTableRegion(region, qualityFindings));
    }
  }

  const relationships = discoverCrossTableRelationships(tables, rawRegionsById);
  for (const rel of relationships) {
    if (rel.needsConfirmation) {
      qualityFindings.push({
        findingId: `dq_${rel.relationshipId}`,
        category: "NeedsClarification",
        sheetName: rel.fromSheet,
        tableId: rel.fromTableId,
        columnName: rel.fromColumn,
        title: `Confirm join relationship: ${rel.fromSheet}.${rel.fromColumn} -> ${rel.toSheet}.${rel.toColumn}`,
        description: rel.explanation,
        affectedRowCount: rel.orphanSourceKeyCount,
        sampleRowIndices: []
      });
    }
  }

  const canonicalKpis: CanonicalKpiMetric[] = [];
  for (const table of tables) {
    for (const col of table.columns) {
      if (!col.aggregationAllowed || !col.numericStats) continue;
      const isAvg = col.recommendedAggregation === "AVERAGE";
      const rawValue = isAvg ? col.numericStats.mean : col.numericStats.sum;
      const aggLabel = isAvg ? "Average" : "Total";
      const provisional = col.confidenceTier === "Low" || col.confidenceTier === "Unknown";

      canonicalKpis.push({
        metricId: `kpi_${col.columnId}_${col.recommendedAggregation.toLowerCase()}`,
        label: `${aggLabel} ${col.headerName}`,
        sheetName: table.sheetName,
        tableId: table.tableId,
        columnName: col.headerName,
        aggregation: isAvg ? "AVERAGE" : "SUM",
        rawValue,
        formattedValue: formatKpiNumber(rawValue, col.currencyCode, col.unit),
        unit: col.unit,
        currencyCode: col.currencyCode,
        formulaTrace: `${col.recommendedAggregation}(${table.sheetName}!${col.headerName}) [${col.nonNullCount} records]`,
        confidencePct: col.confidencePct,
        confidenceTier: col.confidenceTier,
        provisional
      });
    }
  }

  const overallQualityScorePct =
    tables.length > 0
      ? Math.round(tables.reduce((acc, t) => acc + t.overallQualityScorePct, 0) / tables.length)
      : 100;

  const allCols = tables.flatMap((t) => t.columns);
  const overallConfidencePct =
    allCols.length > 0
      ? Math.round(allCols.reduce((acc, c) => acc + c.confidencePct, 0) / allCols.length)
      : 90;

  return {
    modelVersion: "2.0.0",
    ingestionReport,
    tables,
    relationships,
    qualityFindings,
    canonicalKpis,
    overallQualityScorePct,
    overallConfidencePct,
    overallConfidenceTier: scoreToConfidenceTier(overallConfidencePct)
  };
}
