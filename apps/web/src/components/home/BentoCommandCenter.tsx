/**
 * `apps/web/src/components/home/BentoCommandCenter.tsx`
 *
 * The SafiData Command Center (Spec Sections 6.5, 7.1–7.4, 20.1):
 * - Uses the locked 4-color palette (`#F9F7F7`, `#DBE2EF`, `#3F72AF`, `#112D4E`)
 * - 12-Column Full-Viewport Bento Grid (`LivePipelineVisualizer`, `SmartDropzone`, `CountUpMetric`, Offline Security Shield)
 * - Uses `gsap.fromTo` + `clearProps` on cleanup so React `<StrictMode>` never leaves cards hidden
 */
import type { ReactNode } from "react";
import { useEffect, useRef, useState } from "react";
import { Film, Fingerprint, Lock, ShieldCheck } from "lucide-react";
import { gsap } from "gsap";
import { BlurWordText } from "../ui/BlurWordText";
import { CountUpMetric } from "../ui/CountUpMetric";
import { MagneticButton } from "../ui/MagneticButton";
import { SpotlightCard } from "../ui/SpotlightCard";
import { GraphifyLineageCanvas } from "../ui/GraphifyLineageCanvas";
import { canAnimate } from "../ui/motion-preference";
import { SafiMotionShowcase } from "../brand/SafiMotionShowcase";
import { LivePipelineVisualizer } from "./LivePipelineVisualizer";
import { SmartDropzone } from "./SmartDropzone";

export interface BentoCommandCenterProps {
  readonly onStart?: () => void;
  readonly onOpenLibrary?: () => void;
}

const HERO_TITLE =
  "مرحبًا بك في منصة صافي البيانات — الهندسة الذكية لتنقية وحماية البيانات العربية";

const SECURITY_FACTS: readonly { readonly id: string; readonly label: string }[] = [
  { id: "vault", label: "خزنة SQLCipher مع مفاتيح Argon2id" },
  { id: "updater", label: "محدّث موقّع Ed25519 مع منع الرجوع Anti-Rollback" },
  { id: "guard", label: "حارس Zero Trust بلا أي اتصال شبكي" }
];

export function BentoCommandCenter({ onStart, onOpenLibrary }: BentoCommandCenterProps): ReactNode {
  const gridRef = useRef<HTMLElement | null>(null);
  const [showMotionStudio, setShowMotionStudio] = useState<boolean>(true);

  useEffect(() => {
    const grid = gridRef.current;
    if (grid === null || !canAnimate()) {
      return;
    }

    const cards = Array.from(grid.querySelectorAll<HTMLElement>("[data-bento-card]"));
    if (cards.length === 0) {
      return;
    }

    const cascade = gsap.fromTo(
      cards,
      { opacity: 0.25, y: 12, scale: 0.99 },
      {
        opacity: 1,
        y: 0,
        scale: 1,
        duration: 0.35,
        stagger: 0.05,
        ease: "power2.out",
        clearProps: "transform,opacity"
      }
    );

    return () => {
      cascade.kill();
      gsap.set(cards, { clearProps: "transform,opacity" });
    };
  }, []);

  return (
    <div
      dir="rtl"
      style={{ display: "flex", flexDirection: "column", gap: "24px", paddingBlock: "4px" }}
    >
      <header className="safi-hero">
        <span className="safi-hero-badge">
          <span className="safi-shiny-text">
            الإصدار الذهبي المعتمد v1.0.0-s5-gold • معالجة محلية 100% بلا إنترنت
          </span>
        </span>

        <BlurWordText
          as="h1"
          className="safi-hero-title"
          text={HERO_TITLE}
          stagger={0.04}
          ariaLabel={HERO_TITLE}
        />

        <p className="safi-hero-subtitle">
          نظّف ووحّد وأخفِ بياناتك العربية والرقم القومي بالكامل على جهازك، بوصفات حتمية ورسوم
          بيانية وتقارير PDF متجهية قابلة للتدقيق.
        </p>

        <div className="safi-hero-actions">
          <MagneticButton variant="primary" onClick={onStart} ariaLabel="ابدأ الآن">
            ابدأ الآن — مساحة التنضيف
          </MagneticButton>
          <MagneticButton variant="secondary" onClick={onOpenLibrary} ariaLabel="استعرض القوالب">
            استعرض القوالب الجاهزة
          </MagneticButton>
          <button
            type="button"
            className="safi-btn safi-btn-secondary"
            onClick={() => setShowMotionStudio((prev) => !prev)}
          >
            <Film size={15} aria-hidden="true" style={{ color: "var(--accent-primary)" }} />
            {showMotionStudio
              ? "إخفاء استوديو الموشن والمعاينة"
              : "عرض الموشن لوجو (2.6s) ومعاينة الروابط"}
          </button>
        </div>
      </header>

      {showMotionStudio ? <SafiMotionShowcase /> : null}

      <section ref={gridRef} className="safi-bento" aria-label="مركز القيادة">
        <div className="safi-bento-wide" data-bento-card="true">
          <SpotlightCard padding={24}>
            <LivePipelineVisualizer />
          </SpotlightCard>
        </div>

        <div className="safi-bento-half" data-bento-card="true">
          <SpotlightCard padding={24}>
            <SmartDropzone onOpenCleaning={onStart} />
          </SpotlightCard>
        </div>

        <div className="safi-bento-third" data-bento-card="true">
          <SpotlightCard padding={24}>
            <div
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                marginBottom: "16px"
              }}
            >
              <h2 style={{ fontSize: "16px", fontWeight: 700, color: "var(--text-primary)" }}>
                عدادات الأداء والتوفير (React Bits I13)
              </h2>
              <span className="safi-chip safi-chip--accent">60 FPS RTL</span>
            </div>
            <div style={{ display: "flex", gap: "24px", flexWrap: "wrap" }}>
              <CountUpMetric value={99.9} decimals={1} suffix="%" label="دقة التطبيع العربي" />
              <CountUpMetric value={185} suffix="اختبارًا" label="اجتياز اختبارات المنظومة" />
              <CountUpMetric value={4} prefix="<" suffix="ms" label="زمن استجابة النواة" />
              <CountUpMetric value={42} suffix="ساعة" label="وقت تم توفيره (عدّاد I13)" />
            </div>
          </SpotlightCard>
        </div>

        <div className="safi-bento-third" data-bento-card="true">
          <SpotlightCard padding={24}>
            <h2
              style={{
                display: "flex",
                alignItems: "center",
                gap: "8px",
                fontSize: "16px",
                fontWeight: 700,
                color: "var(--text-primary)",
                marginBottom: "14px"
              }}
            >
              <ShieldCheck
                size={18}
                aria-hidden="true"
                style={{ color: "var(--accent-primary)" }}
              />
              درع الأمان والأوفلاين
            </h2>
            <ul
              style={{
                margin: 0,
                padding: 0,
                listStyle: "none",
                display: "flex",
                flexDirection: "column",
                gap: "12px"
              }}
            >
              {SECURITY_FACTS.map((fact) => (
                <li
                  key={fact.id}
                  data-security-fact={fact.id}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: "10px",
                    fontSize: "13px",
                    color: "var(--text-primary)",
                    fontWeight: 600,
                    lineHeight: 1.6
                  }}
                >
                  {fact.id === "vault" ? (
                    <Lock size={15} aria-hidden="true" style={{ color: "var(--accent-primary)" }} />
                  ) : fact.id === "updater" ? (
                    <Fingerprint
                      size={15}
                      aria-hidden="true"
                      style={{ color: "var(--accent-primary)" }}
                    />
                  ) : (
                    <ShieldCheck
                      size={15}
                      aria-hidden="true"
                      style={{ color: "var(--accent-primary)" }}
                    />
                  )}
                  {fact.label}
                </li>
              ))}
            </ul>
          </SpotlightCard>
        </div>
      </section>

      <GraphifyLineageCanvas />
    </div>
  );
}
