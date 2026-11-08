/**
 * `apps/web/src/components/home/LivePipelineVisualizer.tsx`
 *
 * Animated three-stage data pipeline locked to `#F9F7F7`, `#DBE2EF`, `#3F72AF`, `#112D4E`.
 */
import type { ReactNode } from "react";
import { useEffect, useRef, useState } from "react";
import { gsap } from "gsap";
import { CountUpMetric } from "../ui/CountUpMetric";
import { DecryptedMonoText } from "../ui/DecryptedMonoText";
import { MagneticButton } from "../ui/MagneticButton";
import { canAnimate } from "../ui/motion-preference";

export interface PipelineStage {
  readonly id: "raw" | "core" | "clean";
  readonly title: string;
  readonly detail: string;
}

export const PIPELINE_STAGES: readonly PipelineStage[] = [
  {
    id: "raw",
    title: "المدخلات الخام",
    detail: "CSV / XLSX • نصوص عربية غير مطبّعة • بيانات PII مكشوفة"
  },
  {
    id: "core",
    title: "نواة صافي الكريستالية",
    detail: "تطبيع عربي قياسي + إخفاء/ترميز BLAKE3 + كتابة ذرية fsync"
  },
  {
    id: "clean",
    title: "المخرجات الموثقة",
    detail: "بيانات نقية 100% + تقرير PDF موقّع + سجل تدقيق غير قابل للتلاعب"
  }
];

const PACKET_COUNT = 4;
const ROWS_PER_RUN = 25_000;

export function simulatedDigest(run: number): string {
  let hash = 0x811c9dc5;
  const seed = `safi-simulation-${run}`;
  for (let index = 0; index < seed.length; index += 1) {
    hash ^= seed.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  const first = hash.toString(16).padStart(8, "0");
  const second = (Math.imul(hash, 0x9e3779b1) >>> 0).toString(16).padStart(8, "0");
  return `${first}${second}`;
}

export function LivePipelineVisualizer(): ReactNode {
  const [runs, setRuns] = useState(0);
  const packetsRef = useRef<Array<SVGCircleElement | null>>([]);
  const tweensRef = useRef<gsap.core.Tween[]>([]);

  useEffect(() => {
    if (!canAnimate()) {
      return;
    }

    const tweens: gsap.core.Tween[] = [];
    packetsRef.current.forEach((packet, index) => {
      const node = packet;
      if (node === null) {
        return;
      }
      const progress = { value: 0 };
      const tween = gsap.to(progress, {
        value: 1,
        duration: 4.0,
        repeat: -1,
        ease: "none",
        delay: index * 0.9,
        onUpdate: () => {
          const x = 960 - progress.value * 920;
          node.setAttribute("cx", x.toFixed(1));
          node.setAttribute(
            "opacity",
            progress.value > 0.94 || progress.value < 0.04 ? "0.2" : "0.95"
          );
        }
      });
      tweens.push(tween);
    });
    tweensRef.current = tweens;

    return () => {
      for (const tween of tweens) {
        tween.kill();
      }
      tweensRef.current = [];
    };
  }, []);

  const handleRun = (): void => {
    for (const tween of tweensRef.current) {
      tween.timeScale(2.8);
    }
    setRuns((current) => current + 1);
  };

  const processedRows = runs * ROWS_PER_RUN;
  const digest = runs === 0 ? "0000000000000000" : simulatedDigest(runs);

  return (
    <div dir="rtl" style={{ display: "flex", flexDirection: "column", gap: "16px" }}>
      <header
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: "12px",
          flexWrap: "wrap"
        }}
      >
        <h2 style={{ fontSize: "16px", fontWeight: 700, color: "var(--text-primary)" }}>
          محاكاة خط الأنابيب الحي
        </h2>
        <span
          className="safi-mono"
          style={{ fontSize: "12px", color: "var(--text-secondary)", fontWeight: 700 }}
          role="status"
        >
          عدد مرات التشغيل: {runs}
        </span>
      </header>

      <div style={{ position: "relative", height: "32px" }}>
        <svg
          aria-hidden="true"
          viewBox="0 0 1000 32"
          preserveAspectRatio="none"
          style={{ width: "100%", height: "32px", display: "block" }}
        >
          <defs>
            <linearGradient id="safi-pipeline-bus" x1="1" y1="0" x2="0" y2="0">
              <stop offset="0%" stopColor="#3F72AF" stopOpacity="0.9" />
              <stop offset="100%" stopColor="#112D4E" stopOpacity="0.9" />
            </linearGradient>
          </defs>
          <path
            d="M960 16 H40"
            stroke="url(#safi-pipeline-bus)"
            strokeWidth="2"
            strokeDasharray="6 6"
            fill="none"
          />
          {Array.from({ length: PACKET_COUNT }, (_, index) => (
            <circle
              key={`packet-${index}`}
              ref={(element) => {
                packetsRef.current[index] = element;
              }}
              data-packet={index}
              cx={960}
              cy={16}
              r="5"
              fill="#112D4E"
              opacity="0.95"
            />
          ))}
        </svg>
      </div>

      <ol
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fit, minmax(190px, 1fr))",
          gap: "12px",
          listStyle: "none",
          margin: 0,
          padding: 0
        }}
      >
        {PIPELINE_STAGES.map((stage, index) => (
          <li
            key={stage.id}
            data-pipeline-stage={stage.id}
            style={{
              padding: "12px",
              borderRadius: "var(--radius-card)",
              border: "1px solid var(--border-subtle)",
              background: "var(--bg-elevated)"
            }}
          >
            <div style={{ display: "flex", alignItems: "center", gap: "8px", marginBottom: "4px" }}>
              <span
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  justifyContent: "center",
                  width: "20px",
                  height: "20px",
                  borderRadius: "50%",
                  background: "#112D4E",
                  color: "#F9F7F7",
                  fontSize: "11px",
                  fontWeight: 700
                }}
              >
                {index + 1}
              </span>
              <h3 style={{ fontSize: "13px", fontWeight: 700, margin: 0, color: "var(--text-primary)" }}>
                {stage.title}
              </h3>
            </div>
            <p
              style={{
                margin: 0,
                fontSize: "12px",
                color: "var(--text-secondary)",
                lineHeight: 1.6
              }}
            >
              {stage.detail}
            </p>
          </li>
        ))}
      </ol>

      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: "16px",
          flexWrap: "wrap",
          paddingTop: "4px"
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: "16px", flexWrap: "wrap" }}>
          <CountUpMetric value={processedRows} suffix="صف" label="الصفوف المعالجة" />
          <div style={{ display: "flex", flexDirection: "column", gap: "2px" }}>
            <span style={{ fontSize: "11px", color: "var(--text-secondary)", fontWeight: 700 }}>
              بصمة BLAKE3
            </span>
            <DecryptedMonoText key={runs} value={digest} />
          </div>
        </div>

        <MagneticButton variant="primary" onClick={handleRun} ariaLabel="تشغيل محاكاة فورية">
          تشغيل محاكاة فورية
        </MagneticButton>
      </div>
    </div>
  );
}
