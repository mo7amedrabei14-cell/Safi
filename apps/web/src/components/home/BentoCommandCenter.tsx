/**
 * `apps/web/src/components/home/BentoCommandCenter.tsx`
 *
 * The home screen: a hero (shiny version badge, word-blur headline, magnetic CTAs)
 * over an Apple-style Bento grid that arrives with a GSAP stagger cascade.
 *
 * Cards: live pipeline simulation, smart dropzone, system metrics and the offline
 * security shield. Every card is a `SpotlightCard`, so the pointer highlight is
 * pure CSS-variable work with no React re-render.
 */
import type { ReactNode } from "react";
import { useEffect, useRef } from "react";
import { Fingerprint, Lock, ShieldCheck } from "lucide-react";
import { gsap } from "gsap";
import { BlurWordText } from "../ui/BlurWordText";
import { CountUpMetric } from "../ui/CountUpMetric";
import { MagneticButton } from "../ui/MagneticButton";
import { SpotlightCard } from "../ui/SpotlightCard";
import { canAnimate } from "../ui/motion-preference";
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

  useEffect(() => {
    const grid = gridRef.current;
    if (grid === null || !canAnimate()) {
      return;
    }

    const cards = Array.from(grid.querySelectorAll<HTMLElement>("[data-bento-card]"));
    if (cards.length === 0) {
      return;
    }

    const cascade = gsap.from(cards, {
      opacity: 0,
      y: 24,
      scale: 0.99,
      duration: 0.6,
      stagger: 0.1,
      ease: "power3.out",
      clearProps: "transform"
    });

    return () => {
      cascade.kill();
    };
  }, []);

  return (
    <div
      dir="rtl"
      style={{ display: "flex", flexDirection: "column", gap: "32px", paddingBlock: "8px" }}
    >
      <header style={{ display: "flex", flexDirection: "column", gap: "16px", maxWidth: "820px" }}>
        <span
          className="safi-shiny-text"
          style={{ fontSize: "12px", fontWeight: 600, letterSpacing: "0.02em" }}
        >
          الإصدار الذهبي المعتمد v1.0.0-s5-gold • معالجة محلية 100% بلا إنترنت
        </span>

        <BlurWordText as="h1" text={HERO_TITLE} stagger={0.05} ariaLabel={HERO_TITLE} />

        <p style={{ margin: 0, fontSize: "15px", color: "var(--text-muted)", lineHeight: 1.8 }}>
          نظّف ووحّد وأخفِ بياناتك العربية بالكامل على جهازك، بوصفات حتمية وتقارير قابلة للتدقيق.
        </p>

        <div style={{ display: "flex", gap: "12px", flexWrap: "wrap" }}>
          <MagneticButton onClick={onStart} ariaLabel="ابدأ الآن">
            ابدأ الآن
          </MagneticButton>
          <MagneticButton variant="glass" onClick={onOpenLibrary} ariaLabel="استعرض القوالب">
            استعرض القوالب
          </MagneticButton>
        </div>
      </header>

      <section ref={gridRef} className="safi-bento" aria-label="مركز القيادة">
        <div className="safi-bento-wide" data-bento-card="true">
          <SpotlightCard padding={22}>
            <LivePipelineVisualizer />
          </SpotlightCard>
        </div>

        <div className="safi-bento-half" data-bento-card="true">
          <SpotlightCard padding={22}>
            <SmartDropzone />
          </SpotlightCard>
        </div>

        <div className="safi-bento-third" data-bento-card="true">
          <SpotlightCard padding={22}>
            <h2 style={{ fontSize: "16px", fontWeight: 700, marginBottom: "16px" }}>
              عدادات الأداء الحية
            </h2>
            <div style={{ display: "flex", gap: "24px", flexWrap: "wrap" }}>
              <CountUpMetric value={99.9} decimals={1} suffix="%" label="دقة التطبيع العربي" />
              <CountUpMetric value={185} suffix="اختبارًا" label="اجتياز اختبارات المنظومة" />
              <CountUpMetric value={4} prefix="<" suffix="ms" label="زمن استجابة النواة" />
            </div>
          </SpotlightCard>
        </div>

        <div className="safi-bento-third" data-bento-card="true">
          <SpotlightCard padding={22}>
            <h2
              style={{
                display: "flex",
                alignItems: "center",
                gap: "8px",
                fontSize: "16px",
                fontWeight: 700,
                marginBottom: "14px"
              }}
            >
              <ShieldCheck
                size={18}
                aria-hidden="true"
                style={{ color: "var(--accent-emerald)" }}
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
                gap: "10px"
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
                    fontSize: "12px",
                    color: "var(--text-muted)",
                    lineHeight: 1.6
                  }}
                >
                  {fact.id === "vault" ? (
                    <Lock size={14} aria-hidden="true" />
                  ) : fact.id === "updater" ? (
                    <Fingerprint size={14} aria-hidden="true" />
                  ) : (
                    <ShieldCheck size={14} aria-hidden="true" />
                  )}
                  {fact.label}
                </li>
              ))}
            </ul>
          </SpotlightCard>
        </div>
      </section>
    </div>
  );
}
