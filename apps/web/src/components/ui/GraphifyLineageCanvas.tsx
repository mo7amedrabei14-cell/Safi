/**
 * `apps/web/src/components/ui/GraphifyLineageCanvas.tsx`
 *
 * Graphify Interactive Node-Graph Canvas (Spec Section 20.1 #6)
 * Locked to `#F9F7F7`, `#DBE2EF`, `#3F72AF`, `#112D4E`.
 */
import type { ReactNode } from "react";
import { useState } from "react";
import { GitBranch } from "lucide-react";

export interface GraphifyNode {
  readonly id: string;
  readonly kind: "source" | "transform" | "output";
  readonly titleAr: string;
  readonly subtitleAr: string;
  readonly x: number;
  readonly y: number;
  readonly detailAr: string;
}

export interface GraphifyEdge {
  readonly from: string;
  readonly to: string;
  readonly labelAr?: string;
}

export interface GraphifyLineageCanvasProps {
  readonly titleAr?: string;
  readonly nodes?: readonly GraphifyNode[];
  readonly edges?: readonly GraphifyEdge[];
}

export const DEFAULT_LINEAGE_NODES: readonly GraphifyNode[] = [
  {
    id: "src-raw",
    kind: "source",
    titleAr: "ملف العملاء الخام (CSV)",
    subtitleAr: "4 أعمدة • نصوص غير مطبّعة",
    x: 690,
    y: 55,
    detailAr: "المصدر الأصلي: لا يُعدَّل أبدًا (قاعدة 13). يُقرأ في ذاكرة معزولة."
  },
  {
    id: "src-nid",
    kind: "source",
    titleAr: "عمود الرقم القومي والهاتف",
    subtitleAr: "14 رقمًا + أرقام هندية ٠-٩",
    x: 690,
    y: 165,
    detailAr: "يحتوي على بيانات شخصية حساسة (PII) وأرقام عربية مشرقية."
  },
  {
    id: "tr-arabic",
    kind: "transform",
    titleAr: "تطبيع عربي قياسي (sf-arabic)",
    subtitleAr: "توحيد الألف والياء وإزالة التطويل",
    x: 390,
    y: 55,
    detailAr: "يحوّل أشكال الألف (أ/إ/آ ← ا) والتاء المربوطة والأرقام المشرقية حتميًا."
  },
  {
    id: "tr-pii",
    kind: "transform",
    titleAr: "إخفاء وترميز BLAKE3 + فحص المحافظة",
    subtitleAr: "استخراج المحافظة وتاريخ الميلاد",
    x: 390,
    y: 165,
    detailAr: "يتحقق من معادلة الرقم القومي المصري (القسم 20.3) ويولّد رمز BLAKE3 مملّح."
  },
  {
    id: "out-clean",
    kind: "output",
    titleAr: "جدول نقي + كتابة ذرية fsync",
    subtitleAr: "حماية حقن الصيغ A11",
    x: 95,
    y: 55,
    detailAr: "يُكتب كملف جديد تمامًا مع سجل التغييرات وحماية CSV/Excel Formula Injection."
  },
  {
    id: "out-chart",
    kind: "output",
    titleAr: "مخطط المحافظات وتقرير Typst PDF",
    subtitleAr: "تباين WCAG 2.2 AA ≥ 3.0:1",
    x: 95,
    y: 165,
    detailAr: "نسب الأرقام (N11): كل عمود في الرسم البياني مرتبط مباشرةً بالخلايا المنقّاة."
  }
];

export const DEFAULT_LINEAGE_EDGES: readonly GraphifyEdge[] = [
  { from: "src-raw", to: "tr-arabic", labelAr: "تنظيف" },
  { from: "src-nid", to: "tr-pii", labelAr: "فحص PII" },
  { from: "tr-arabic", to: "out-clean", labelAr: "حتمي" },
  { from: "tr-pii", to: "out-clean", labelAr: "مُرمّز" },
  { from: "tr-arabic", to: "out-chart" },
  { from: "tr-pii", to: "out-chart", labelAr: "تجميع" }
];

const KIND_COLORS: Readonly<
  Record<GraphifyNode["kind"], { stroke: string; badge: string; label: string }>
> = {
  source: { stroke: "#DAC0A3", badge: "#EADBC8", label: "مصدر" },
  transform: { stroke: "#102C57", badge: "#EADBC8", label: "تحويل" },
  output: { stroke: "#102C57", badge: "#DAC0A3", label: "مخرج" }
};

export function GraphifyLineageCanvas({
  titleAr = "مخطط Graphify التفاعلي لنسب الأرقام (Lineage N11) ومراحل الوصفة",
  nodes = DEFAULT_LINEAGE_NODES,
  edges = DEFAULT_LINEAGE_EDGES
}: GraphifyLineageCanvasProps): ReactNode {
  const [selectedId, setSelectedId] = useState<string>(nodes[0]?.id ?? "src-raw");
  const selectedNode = nodes.find((n) => n.id === selectedId) ?? nodes[0];

  const nodeMap = new Map(nodes.map((n) => [n.id, n]));

  return (
    <div
      dir="rtl"
      style={{
        display: "flex",
        flexDirection: "column",
        gap: "12px",
        padding: "16px",
        borderRadius: "12px",
        border: "1px solid var(--border-subtle)",
        background: "var(--bg-elevated)"
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: "12px",
          flexWrap: "wrap"
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
          <GitBranch size={16} aria-hidden="true" style={{ color: "var(--accent-primary)" }} />
          <h3
            style={{ fontSize: "15px", fontWeight: 700, margin: 0, color: "var(--text-primary)" }}
          >
            {titleAr}
          </h3>
        </div>
        <span style={{ fontSize: "12px", color: "var(--text-secondary)", fontWeight: 600 }}>
          انقر على أي عقدة لاستعراض نسب البيانات والعملية الحتمية
        </span>
      </div>

      <div style={{ overflowX: "auto" }}>
        <svg
          viewBox="0 0 880 235"
          style={{ width: "100%", minWidth: "660px", height: "auto", display: "block" }}
          role="img"
          aria-label={titleAr}
        >
          <defs>
            <linearGradient id="graphify-edge-grad" x1="1" y1="0" x2="0" y2="0">
              <stop offset="0%" stopColor="#DAC0A3" stopOpacity="0.85" />
              <stop offset="100%" stopColor="#102C57" stopOpacity="0.85" />
            </linearGradient>
          </defs>

          {edges.map((edge, idx) => {
            const fromNode = nodeMap.get(edge.from);
            const toNode = nodeMap.get(edge.to);
            if (!fromNode || !toNode) return null;

            const startX = fromNode.x;
            const startY = fromNode.y + 28;
            const endX = toNode.x + 170;
            const endY = toNode.y + 28;
            const midX = (startX + endX) / 2;
            const pathD = `M ${startX} ${startY} C ${midX} ${startY}, ${midX} ${endY}, ${endX} ${endY}`;

            const isHighlighted = selectedId === edge.from || selectedId === edge.to;

            return (
              <g key={`${edge.from}-${edge.to}-${idx}`}>
                <path
                  d={pathD}
                  fill="none"
                  stroke={isHighlighted ? "#102C57" : "url(#graphify-edge-grad)"}
                  strokeWidth={isHighlighted ? 2.5 : 1.5}
                  strokeDasharray={isHighlighted ? undefined : "5 5"}
                  opacity={isHighlighted ? 1 : 0.65}
                />
                <circle cx={endX} cy={endY} r="3.5" fill="#102C57" />
              </g>
            );
          })}

          {nodes.map((node) => {
            const isSelected = node.id === selectedId;
            const palette = KIND_COLORS[node.kind];
            return (
              <g
                key={node.id}
                transform={`translate(${node.x}, ${node.y})`}
                onClick={() => setSelectedId(node.id)}
                style={{ cursor: "pointer" }}
              >
                <rect
                  width="170"
                  height="56"
                  rx="10"
                  fill="var(--bg-surface)"
                  stroke={isSelected ? palette.stroke : "var(--border-strong)"}
                  strokeWidth={isSelected ? 2.4 : 1.2}
                />
                <rect x="126" y="8" width="36" height="16" rx="8" fill={palette.badge} />
                <text
                  x="144"
                  y="19"
                  textAnchor="middle"
                  fill="#102C57"
                  fontSize="9"
                  fontWeight="700"
                >
                  {palette.label}
                </text>
                <text
                  x="118"
                  y="23"
                  textAnchor="end"
                  fill="var(--text-primary)"
                  fontSize="11"
                  fontWeight="700"
                >
                  {node.titleAr}
                </text>
                <text x="158" y="42" textAnchor="end" fill="var(--text-secondary)" fontSize="10">
                  {node.subtitleAr}
                </text>
              </g>
            );
          })}
        </svg>
      </div>

      {selectedNode ? (
        <div
          role="status"
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            gap: "12px",
            padding: "10px 14px",
            borderRadius: "8px",
            background: "var(--bg-surface)",
            border: "1px solid var(--border-subtle)",
            fontSize: "13px",
            flexWrap: "wrap"
          }}
        >
          <div>
            <strong style={{ color: "var(--text-primary)" }}>{selectedNode.titleAr}: </strong>
            <span style={{ color: "var(--text-secondary)" }}>{selectedNode.detailAr}</span>
          </div>
          <span
            className="safi-mono"
            style={{ fontSize: "11px", color: "var(--text-secondary)", fontWeight: 700 }}
            dir="ltr"
          >
            node://{selectedNode.id}
          </span>
        </div>
      ) : null}
    </div>
  );
}
