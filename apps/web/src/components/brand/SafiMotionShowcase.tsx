/**
 * `apps/web/src/components/brand/SafiMotionShowcase.tsx`
 *
 * Interactive Motion Graphics Studio & Social Link Preview (Spec Section 7.1, 7.2, 7.3, 7.4)
 * Strictly locked to the 4-Color Palette: `#F9F7F7`, `#DBE2EF`, `#3F72AF`, `#112D4E`.
 */
import type { ReactNode } from "react";
import { useEffect, useRef, useState } from "react";
import { Check, Copy, Download, Play, Share2 } from "lucide-react";
import { gsap } from "gsap";
import { MOTION_DOTS } from "./SafiPrismLogo";
import { downloadTextFile } from "../../lib/download";

const CHOREOGRAPHY_PHASES: ReadonlyArray<{
  readonly id: string;
  readonly time: string;
  readonly title: string;
  readonly desc: string;
}> = [
  {
    id: "scattered",
    time: "0.00s – 0.35s",
    title: "١. ظهور النقاط التسع المبعثرة",
    desc: "تكبير من 0 إلى 1 بفارق 30ms يمثّل البيانات الخام قبل التنظيم."
  },
  {
    id: "grid",
    time: "0.35s – 1.00s",
    title: "٢. الاصطفاف في شبكة 3 × 3",
    desc: "حركة نابضية (Spring) ترتّب النقاط التسع في مصفوفة هندسية متوازنة."
  },
  {
    id: "gooey",
    time: "1.00s – 1.50s",
    title: "٣. الاندماج الغروي في قطرة صافي",
    desc: "مرشح Gooey يدمج النقاط في قطرة متماثلة بدرجات #3F72AF و #112D4E."
  },
  {
    id: "check",
    time: "1.50s – 2.60s",
    title: "٤. رسم علامة الصح وحلقة التموّج",
    desc: "رسم علامة الصح بلون #F9F7F7 مع حلقة تموّج وكشف اسم «صافي البيانات»."
  }
];

export function SafiMotionShowcase(): ReactNode {
  const [activePhase, setActivePhase] = useState<number>(3);
  const [fillPct, setFillPct] = useState<number>(78);
  const [copiedLink, setCopiedLink] = useState<boolean>(false);
  const stageSvgRef = useRef<SVGSVGElement | null>(null);
  const tlRef = useRef<gsap.core.Timeline | null>(null);

  const playFullChoreography = (): void => {
    const svg = stageSvgRef.current;
    if (svg === null) return;

    tlRef.current?.kill();

    const dots = Array.from(svg.querySelectorAll<SVGCircleElement>("[data-stage-dot]"));
    const gridLines = svg.querySelector<SVGGElement>("[data-stage-grid]");
    const dropBody = svg.querySelector<SVGPathElement>("[data-stage-drop]");
    const checkMark = svg.querySelector<SVGPathElement>("[data-stage-check]");
    const ripple = svg.querySelector<SVGCircleElement>("[data-stage-ripple]");
    const wordmark = svg.querySelector<SVGGElement>("[data-stage-wordmark]");

    const tl = gsap.timeline();
    tlRef.current = tl;

    setActivePhase(0);
    dots.forEach((dot, idx) => {
      const pt = MOTION_DOTS[idx];
      if (!pt) return;
      gsap.set(dot, {
        attr: { cx: pt.startX, cy: pt.startY, r: 5 },
        opacity: 0,
        scale: 0,
        transformOrigin: "50% 50%"
      });
    });

    if (gridLines) gsap.set(gridLines, { opacity: 0 });
    if (dropBody) gsap.set(dropBody, { opacity: 0, scale: 0.65, transformOrigin: "64px 76px" });
    if (checkMark) gsap.set(checkMark, { strokeDasharray: 68, strokeDashoffset: 68, opacity: 0 });
    if (ripple) gsap.set(ripple, { scale: 0.85, opacity: 0, transformOrigin: "64px 78px" });
    if (wordmark) gsap.set(wordmark, { opacity: 0, x: 12 });

    tl.to(
      dots,
      {
        opacity: 1,
        scale: 1,
        duration: 0.35,
        stagger: 0.03,
        ease: "back.out(1.8)"
      },
      0
    );

    tl.call(() => setActivePhase(1), [], 0.35);
    if (gridLines) {
      tl.to(gridLines, { opacity: 0.45, duration: 0.3 }, 0.4);
    }
    dots.forEach((dot, idx) => {
      const pt = MOTION_DOTS[idx];
      if (!pt) return;
      tl.to(
        dot,
        {
          attr: { cx: pt.gridX, cy: pt.gridY },
          duration: 0.5,
          ease: "elastic.out(1, 0.7)"
        },
        0.35
      );
    });

    tl.call(() => setActivePhase(2), [], 1.0);
    if (gridLines) {
      tl.to(gridLines, { opacity: 0, duration: 0.2 }, 1.0);
    }
    tl.to(
      dots,
      {
        attr: { cx: 64, cy: 78, r: 9 },
        opacity: 0,
        duration: 0.48,
        ease: "power3.in"
      },
      1.0
    );
    if (dropBody) {
      tl.to(
        dropBody,
        {
          opacity: 1,
          scale: 1,
          duration: 0.48,
          ease: "back.out(1.4)"
        },
        1.12
      );
    }

    tl.call(() => setActivePhase(3), [], 1.5);
    if (checkMark) {
      tl.set(checkMark, { opacity: 1 }, 1.5);
      tl.to(checkMark, { strokeDashoffset: 0, duration: 0.4, ease: "power2.out" }, 1.5);
    }
    if (ripple) {
      tl.fromTo(
        ripple,
        { scale: 0.95, opacity: 0.5 },
        { scale: 1.6, opacity: 0, duration: 0.55, ease: "power2.out" },
        1.85
      );
    }
    if (wordmark) {
      tl.to(wordmark, { opacity: 1, x: 0, duration: 0.45, ease: "power2.out" }, 1.7);
    }
  };

  useEffect(() => {
    return () => {
      tlRef.current?.kill();
    };
  }, []);

  const handleCopyShareLink = (): void => {
    const url = typeof window !== "undefined" ? window.location.href : "https://safidata.app";
    if (typeof navigator !== "undefined" && navigator.clipboard !== undefined) {
      void navigator.clipboard.writeText(url);
    }
    setCopiedLink(true);
    window.setTimeout(() => setCopiedLink(false), 2200);
  };

  const handleDownloadLogoSvg = (): void => {
    const svgCode = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128" role="img" aria-label="صافي">\n  <defs>\n    <linearGradient id="g" x1="64" y1="10" x2="64" y2="120" gradientUnits="userSpaceOnUse">\n      <stop offset="0" stop-color="#3F72AF" />\n      <stop offset="1" stop-color="#112D4E" />\n    </linearGradient>\n  </defs>\n  <path d="M64 10C64 10 26 54 26 82C26 103 43 120 64 120C85 120 102 103 102 82C102 54 64 10 64 10Z" fill="url(#g)" />\n  <path d="M45 82L58 95L84 66" fill="none" stroke="#F9F7F7" stroke-width="10" stroke-linecap="round" stroke-linejoin="round" />\n</svg>`;
    downloadTextFile("safi-logo-official.svg", svgCode, "image/svg+xml");
  };

  const waterY = 120 - (fillPct / 100) * 110;

  return (
    <div className="safi-grid-2" dir="rtl" style={{ marginTop: "8px" }}>
      {/* Card 1: 2.6s Motion Logo Stage & Droplet Loader */}
      <div className="safi-spotlight" style={{ display: "flex", flexDirection: "column", gap: "16px" }}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "12px", flexWrap: "wrap" }}>
          <div>
            <span className="safi-eyebrow">القسم 7.1 و 7.2 • الموشن جرافيك الرسمي (2.6 ثانية)</span>
            <h3 style={{ fontSize: "18px", fontWeight: 700, marginTop: "2px", color: "var(--text-primary)" }}>
              مبعثر ← شبكة 3×3 ← قطرة صافي النقية
            </h3>
          </div>
          <div style={{ display: "flex", gap: "8px" }}>
            <button
              type="button"
              className="safi-btn safi-btn-primary"
              onClick={playFullChoreography}
              style={{ minHeight: "38px", padding: "6px 14px", fontSize: "13px" }}
            >
              <Play size={14} aria-hidden="true" />
              تشغيل الموشن (2.6s)
            </button>
            <button
              type="button"
              className="safi-btn safi-btn-secondary"
              onClick={handleDownloadLogoSvg}
              style={{ minHeight: "38px", padding: "6px 12px", fontSize: "12px" }}
            >
              <Download size={14} aria-hidden="true" />
              SVG
            </button>
          </div>
        </div>

        <div
          style={{
            display: "grid",
            gridTemplateColumns: "minmax(180px, 220px) 1fr",
            gap: "16px",
            alignItems: "center",
            padding: "16px",
            borderRadius: "12px",
            background: "var(--bg-elevated)",
            border: "1px solid var(--border-subtle)"
          }}
        >
          <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: "8px" }}>
            <svg
              ref={stageSvgRef}
              viewBox="0 0 128 156"
              width="160"
              height="195"
              role="img"
              aria-label="عرض موشن لوجو صافي التفاعلي"
              style={{ overflow: "visible" }}
            >
              <defs>
                <linearGradient id="stage-drop-grad" x1="64" y1="10" x2="64" y2="120" gradientUnits="userSpaceOnUse">
                  <stop offset="0%" stopColor="#3F72AF" />
                  <stop offset="100%" stopColor="#112D4E" />
                </linearGradient>
                <filter id="stage-gooey">
                  <feGaussianBlur in="SourceGraphic" stdDeviation="3" result="blur" />
                  <feColorMatrix
                    in="blur"
                    mode="matrix"
                    values="1 0 0 0 0  0 1 0 0 0  0 0 1 0 0  0 0 0 18 -7"
                    result="goo"
                  />
                  <feBlend in="SourceGraphic" in2="goo" />
                </filter>
              </defs>

              <g data-stage-grid="true" opacity="0" stroke="#3F72AF" strokeWidth="0.75" strokeDasharray="2 3">
                <line x1="44" y1="48" x2="44" y2="108" />
                <line x1="64" y1="48" x2="64" y2="108" />
                <line x1="84" y1="48" x2="84" y2="108" />
                <line x1="34" y1="58" x2="94" y2="58" />
                <line x1="34" y1="78" x2="94" y2="78" />
                <line x1="34" y1="98" x2="94" y2="98" />
              </g>

              <circle
                data-stage-ripple="true"
                cx="64"
                cy="78"
                r="42"
                fill="none"
                stroke="#3F72AF"
                strokeWidth="2.5"
                opacity="0"
              />

              <g filter="url(#stage-gooey)">
                {MOTION_DOTS.map((dot) => (
                  <circle
                    key={dot.id}
                    data-stage-dot={dot.id}
                    cx={dot.gridX}
                    cy={dot.gridY}
                    r="5"
                    fill="#3F72AF"
                    opacity="0"
                  />
                ))}
              </g>

              <path
                data-stage-drop="true"
                d="M64 10C64 10 26 54 26 82C26 103 43 120 64 120C85 120 102 103 102 82C102 54 64 10 64 10Z"
                fill="url(#stage-drop-grad)"
              />

              <path
                data-stage-check="true"
                d="M45 82L58 95L84 66"
                fill="none"
                stroke="#F9F7F7"
                strokeWidth="10"
                strokeLinecap="round"
                strokeLinejoin="round"
              />

              <g data-stage-wordmark="true" textAnchor="middle">
                <text x="64" y="140" fill="var(--text-primary)" fontSize="14" fontWeight="700">
                  صافي البيانات
                </text>
                <text x="64" y="154" fill="var(--text-secondary)" fontSize="10" fontWeight="700">
                  بياناتك... صافية
                </text>
              </g>
            </svg>
          </div>

          <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>
            {CHOREOGRAPHY_PHASES.map((phase, idx) => {
              const isCurrent = idx === activePhase;
              return (
                <div
                  key={phase.id}
                  style={{
                    padding: "8px 12px",
                    borderRadius: "8px",
                    border: `1px solid ${isCurrent ? "var(--border-strong)" : "var(--border-subtle)"}`,
                    background: "var(--bg-surface)"
                  }}
                >
                  <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                    <span style={{ fontSize: "12px", fontWeight: 700, color: "var(--text-primary)" }}>
                      {phase.title}
                    </span>
                    <span className="safi-mono" style={{ fontSize: "11px", color: "var(--text-secondary)", fontWeight: 700 }} dir="ltr">
                      {phase.time}
                    </span>
                  </div>
                  <p style={{ margin: "2px 0 0", fontSize: "11px", color: "var(--text-secondary)" }}>
                    {phase.desc}
                  </p>
                </div>
              );
            })}
          </div>
        </div>

        {/* Section 7.3 Interactive Water Drop Progress Loader */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            gap: "16px",
            padding: "12px 14px",
            borderRadius: "10px",
            background: "var(--bg-elevated)",
            border: "1px solid var(--border-subtle)",
            flexWrap: "wrap"
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: "12px" }}>
            <svg width="44" height="44" viewBox="0 0 128 128" aria-label="مؤشر امتلاء قطرة صافي">
              <defs>
                <clipPath id="safi-fill-clip">
                  <path d="M64 10C64 10 26 54 26 82C26 103 43 120 64 120C85 120 102 103 102 82C102 54 64 10 64 10Z" />
                </clipPath>
              </defs>
              <path
                d="M64 10C64 10 26 54 26 82C26 103 43 120 64 120C85 120 102 103 102 82C102 54 64 10 64 10Z"
                fill="none"
                stroke="#3F72AF"
                strokeWidth="4"
                strokeOpacity="0.5"
              />
              <g clipPath="url(#safi-fill-clip)">
                <rect x="0" y={waterY} width="128" height="128" fill="url(#stage-drop-grad)" />
                <path
                  d={`M0 ${waterY} Q 32 ${waterY - 6}, 64 ${waterY} T 128 ${waterY} V 128 H 0 Z`}
                  fill="#3F72AF"
                  fillOpacity="0.65"
                />
              </g>
              {fillPct >= 100 ? (
                <path
                  d="M45 82L58 95L84 66"
                  fill="none"
                  stroke="#F9F7F7"
                  strokeWidth="10"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
              ) : null}
            </svg>
            <div>
              <span style={{ fontSize: "13px", fontWeight: 700, display: "block", color: "var(--text-primary)" }}>
                حركة التحميل بالقطرة الحية (القسم 7.3): {fillPct}%
              </span>
              <span style={{ fontSize: "11px", color: "var(--text-secondary)" }}>
                تمتلئ القطرة من الأسفل للأعلى مع موجة سطح الماء وتُرسم علامة الصح عند 100%
              </span>
            </div>
          </div>

          <input
            type="range"
            min={0}
            max={100}
            value={fillPct}
            onChange={(e) => setFillPct(Number(e.target.value))}
            aria-label="نسبة امتلاء قطرة التحميل"
            style={{ width: "140px", cursor: "pointer" }}
          />
        </div>
      </div>

      {/* Card 2: Browser Tab Favicon & Messaging Social Preview Card */}
      <div className="safi-spotlight" style={{ display: "flex", flexDirection: "column", gap: "16px" }}>
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: "12px", flexWrap: "wrap" }}>
          <div>
            <span className="safi-eyebrow">القسم 7.4 • أيقونة المتصفح ومعاينة الروابط في الرسائل</span>
            <h3 style={{ fontSize: "18px", fontWeight: 700, marginTop: "2px", color: "var(--text-primary)" }}>
              مظهر التبويب (Favicon) وبطاقة المشاركة (OpenGraph)
            </h3>
          </div>
          <button
            type="button"
            className="safi-btn safi-btn-primary"
            onClick={handleCopyShareLink}
            style={{ minHeight: "38px", padding: "6px 14px", fontSize: "13px" }}
          >
            {copiedLink ? <Check size={14} aria-hidden="true" /> : <Copy size={14} aria-hidden="true" />}
            {copiedLink ? "تم نسخ الرابط!" : "نسخ رابط المنصة"}
          </button>
        </div>

        <div
          style={{
            padding: "10px 14px",
            borderRadius: "10px",
            background: "var(--bg-elevated)",
            border: "1px solid var(--border-subtle)",
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            gap: "12px",
            flexWrap: "wrap"
          }}
        >
          <div
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: "10px",
              padding: "6px 14px",
              borderRadius: "8px 8px 0 0",
              background: "var(--bg-surface)",
              border: "1px solid var(--border-subtle)"
            }}
          >
            <img
              src="/favicon.svg"
              alt="أيقونة تبويب صافي"
              width={20}
              height={20}
              style={{ borderRadius: "4px", display: "block" }}
            />
            <span style={{ fontSize: "12px", fontWeight: 700, color: "var(--text-primary)" }}>
              صافي البيانات | SafiData — بياناتك... صافية
            </span>
          </div>
          <span style={{ fontSize: "11px", color: "var(--text-primary)", fontWeight: 700 }}>
            ✓ مفعّل في تبويب المتصفح (`/favicon.svg`)
          </span>
        </div>

        <div
          style={{
            borderRadius: "12px",
            overflow: "hidden",
            border: "1px solid var(--border-strong)",
            background: "var(--bg-elevated)"
          }}
        >
          <div
            style={{
              padding: "8px 12px",
              background: "var(--bg-surface)",
              borderBottom: "1px solid var(--border-subtle)",
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              fontSize: "11px",
              color: "var(--text-primary)"
            }}
          >
            <span style={{ display: "inline-flex", alignItems: "center", gap: "6px", fontWeight: 700 }}>
              <Share2 size={13} aria-hidden="true" />
              معاينة عند إرسال الرابط في أي رسالة (WhatsApp • Telegram • Slack • X)
            </span>
            <span className="safi-mono" dir="ltr">
              1200 × 630 OG Card
            </span>
          </div>

          <img
            src="/og-preview.svg"
            alt="بطاقة معاينة رابط صافي البيانات في تطبيقات المراسلة"
            style={{ width: "100%", height: "auto", display: "block" }}
          />

          <div style={{ padding: "12px 14px", display: "flex", flexDirection: "column", gap: "4px" }}>
            <span className="safi-mono" style={{ fontSize: "11px", color: "var(--text-secondary)", fontWeight: 700 }} dir="ltr">
              safidata.app • OpenGraph & Twitter Summary Large Image
            </span>
            <strong style={{ fontSize: "14px", color: "var(--text-primary)" }}>
              صافي البيانات | SafiData — بياناتك... صافية
            </strong>
            <span style={{ fontSize: "12px", color: "var(--text-secondary)" }}>
              نظّف ووحّد وأخفِ بياناتك العربية بالكامل على جهازك بدون سحابة — مع محرك الرقم القومي المصري، وصفات BLAKE3 الحتمية، ورسوم WCAG 2.2 AA.
            </span>
          </div>
        </div>
      </div>
    </div>
  );
}
