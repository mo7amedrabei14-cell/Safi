/**
 * `apps/web/src/components/ChartStudio.tsx` — Chart Studio UI Component (Section 17.2 - 17.4).
 */

import React, { useState } from "react";
import { type ChartSpec, generateDeterministicSvg, validateChartSpec } from "../charts/chart-spec";

export interface ChartStudioProps {
  initialSpec: ChartSpec;
}

export const ChartStudio: React.FC<ChartStudioProps> = ({ initialSpec }) => {
  const [spec, setSpec] = useState<ChartSpec>(initialSpec);
  const validation = validateChartSpec(spec);

  return (
    <div className="p-4 bg-white dark:bg-zinc-900 rounded-lg shadow rtl text-right">
      <h2 className="text-xl font-bold mb-4">استديو الرسوم البيانية (Chart Studio)</h2>
      <div className="mb-4">
        <label className="block text-sm font-medium mb-1">عنوان المخطط:</label>
        <input
          type="text"
          value={spec.titleAr}
          onChange={(e) => setSpec({ ...spec, titleAr: e.target.value })}
          className="border rounded p-2 w-full max-w-md dark:bg-zinc-800 dark:border-zinc-700"
        />
      </div>
      {validation.valid ? (
        <div
          className="border rounded p-4 bg-zinc-50 dark:bg-zinc-800 overflow-x-auto"
          dangerouslySetInnerHTML={{ __html: generateDeterministicSvg(spec) }}
        />
      ) : (
        <div className="text-red-500 font-semibold">
          مواصفات المخطط غير صالحة: {validation.errors.join(", ")}
        </div>
      )}
    </div>
  );
};
