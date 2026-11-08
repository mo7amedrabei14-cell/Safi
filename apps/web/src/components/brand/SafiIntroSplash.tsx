/**
 * `apps/web/src/components/brand/SafiIntroSplash.tsx`
 *
 * Cinematic Motion Graphic Intro Preloader (2.2s @ 120fps via GSAP Timeline)
 * Locked to the 4-Color Palette: `#F9F7F7`, `#DBE2EF`, `#3F72AF`, `#112D4E`.
 * Includes a hard `setTimeout` safety guard AND click-anywhere-to-skip so it
 * can never block the main UI even if the browser tab is backgrounded.
 */
import type { ReactNode } from "react";
import { useCallback, useEffect, useRef } from "react";
import { gsap } from "gsap";
import { SafiPrismLogo } from "./SafiPrismLogo";
import { BlurWordText } from "../ui/BlurWordText";
import { canAnimate } from "../ui/motion-preference";

export interface SafiIntroSplashProps {
  readonly open: boolean;
  readonly onComplete: () => void;
}

interface RawDataShard {
  readonly id: string;
  readonly label: string;
  readonly startX: number;
  readonly startY: number;
  readonly rotation: number;
}

export const INTRO_RAW_SHARDS: readonly RawDataShard[] = [
  { id: "s1", label: "أَحْمَدُ مُحَمَّدْ ـــ", startX: -260, startY: -150, rotation: -12 },
  { id: "s2", label: "٠١٠١٢٣٤٥٦٧٨", startX: 250, startY: -140, rotation: 10 },
  { id: "s3", label: "29801011234567", startX: -290, startY: 20, rotation: -6 },
  { id: "s4", label: "#N/A • NULL • CSV", startX: 280, startY: 30, rotation: 8 },
  { id: "s5", label: "١٢,٥٠٠ ج.م", startX: -220, startY: 155, rotation: 9 },
  { id: "s6", label: "عبد الله / ابوبكر", startX: 230, startY: 150, rotation: -11 },
  { id: "s7", label: "XLSX • UTF-8 BOM", startX: 0, startY: -195, rotation: 4 },
  { id: "s8", label: "PII -> sf_ph_b3a8", startX: 0, startY: 195, rotation: -5 }
];

export function SafiIntroSplash({ open, onComplete }: SafiIntroSplashProps): ReactNode {
  const overlayRef = useRef<HTMLElement | null>(null);
  const ringRef = useRef<HTMLDivElement | null>(null);
  const logoWrapRef = useRef<HTMLDivElement | null>(null);
  const identityRef = useRef<HTMLDivElement | null>(null);
  const progressFillRef = useRef<HTMLDivElement | null>(null);
  const timelineRef = useRef<gsap.core.Timeline | null>(null);

  const handleSkip = useCallback((): void => {
    timelineRef.current?.kill();
    onComplete();
  }, [onComplete]);

  useEffect(() => {
    if (!open) {
      return;
    }

    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.key === "Escape" || event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        handleSkip();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [open, handleSkip]);

  useEffect(() => {
    if (!open) {
      return;
    }

    // Hard timeout guarantee: even if rAF is paused in a background tab, close after 2.4s
    const safetyTimer = window.setTimeout(() => {
      onComplete();
    }, 2400);

    const overlay = overlayRef.current;
    if (overlay === null || !canAnimate() || typeof requestAnimationFrame !== "function") {
      window.clearTimeout(safetyTimer);
      onComplete();
      return;
    }

    timelineRef.current?.kill();

    const shards = Array.from(overlay.querySelectorAll<HTMLElement>("[data-intro-shard]"));
    const ring = ringRef.current;
    const logoWrap = logoWrapRef.current;
    const identity = identityRef.current;
    const progressFill = progressFillRef.current;

    gsap.set(overlay, { opacity: 1, scale: 1, filter: "blur(0px)" });

    shards.forEach((shard, idx) => {
      const spec = INTRO_RAW_SHARDS[idx];
      if (!spec) return;
      gsap.set(shard, {
        x: spec.startX,
        y: spec.startY,
        rotation: spec.rotation,
        scale: 0.8,
        opacity: 0
      });
    });

    if (ring !== null) {
      gsap.set(ring, { scale: 0.6, opacity: 0.3, rotation: 0 });
    }
    if (logoWrap !== null) {
      gsap.set(logoWrap, { scale: 0.8, opacity: 1 });
    }
    if (identity !== null) {
      gsap.set(identity, { y: 12, opacity: 1 });
    }
    if (progressFill !== null) {
      gsap.set(progressFill, { scaleX: 0.15, transformOrigin: "right center" });
    }

    const tl = gsap.timeline({
      onComplete: () => {
        window.clearTimeout(safetyTimer);
        onComplete();
      }
    });
    timelineRef.current = tl;

    tl.to(
      shards,
      {
        opacity: 0.95,
        scale: 1,
        duration: 0.22,
        stagger: 0.02,
        ease: "power2.out"
      },
      0
    );

    tl.to(
      shards,
      {
        x: 0,
        y: 0,
        rotation: 0,
        scale: 0.35,
        opacity: 0,
        duration: 0.5,
        stagger: 0.02,
        ease: "power3.in"
      },
      0.26
    );

    if (ring !== null) {
      tl.to(
        ring,
        {
          scale: 1.05,
          opacity: 0.85,
          rotation: 220,
          duration: 0.85,
          ease: "expo.out"
        },
        0.45
      );
    }

    if (logoWrap !== null) {
      tl.to(
        logoWrap,
        {
          scale: 1,
          opacity: 1,
          duration: 0.6,
          ease: "back.out(1.4)"
        },
        0.5
      );
    }

    if (identity !== null) {
      tl.to(
        identity,
        {
          y: 0,
          opacity: 1,
          duration: 0.4,
          ease: "power2.out"
        },
        0.95
      );
    }

    if (progressFill !== null) {
      tl.to(
        progressFill,
        {
          scaleX: 1,
          duration: 0.7,
          ease: "power2.inOut"
        },
        0.95
      );
    }

    tl.to(
      overlay,
      {
        scale: 1.05,
        opacity: 0,
        filter: "blur(14px)",
        duration: 0.38,
        ease: "power3.inOut"
      },
      1.78
    );

    return () => {
      window.clearTimeout(safetyTimer);
      tl.kill();
    };
  }, [open, onComplete]);

  if (!open) {
    return null;
  }

  return (
    <aside
      ref={overlayRef}
      dir="rtl"
      onClick={handleSkip}
      className="safi-intro-splash"
      aria-label="العرض الافتتاحي لمنصة صافي البيانات"
      data-safi-intro="true"
    >
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          handleSkip();
        }}
        className="safi-intro-skip"
        aria-label="تخطي العرض الافتتاحي"
      >
        تخطي العرض ⏭
      </button>

      <div
        aria-hidden="true"
        style={{
          position: "relative",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          width: "260px",
          height: "200px"
        }}
      >
        {INTRO_RAW_SHARDS.map((shard) => (
          <span key={shard.id} data-intro-shard={shard.id} className="safi-intro-shard">
            {shard.label}
          </span>
        ))}

        <div ref={ringRef} className="safi-intro-conic-ring" />

        <div
          ref={logoWrapRef}
          style={{
            position: "relative",
            zIndex: 2,
            display: "flex",
            alignItems: "center",
            justifyContent: "center"
          }}
        >
          <SafiPrismLogo size="lg" hideText />
        </div>
      </div>

      <div
        ref={identityRef}
        style={{
          marginTop: "24px",
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          gap: "12px",
          textAlign: "center",
          maxWidth: "560px",
          width: "100%"
        }}
      >
        <BlurWordText
          as="h2"
          text="صافي البيانات | SafiData"
          stagger={0.04}
          style={{
            fontSize: "28px",
            fontWeight: 700,
            color: "#F9F7F7",
            letterSpacing: "-0.02em"
          }}
        />

        <p
          className="safi-mono"
          style={{
            fontSize: "12.5px",
            color: "#DBE2EF",
            fontWeight: 600
          }}
        >
          تهيئة نواة Rust • فحص خزنة SQLCipher • التحقق من بصمة BLAKE3
        </p>

        <div
          aria-hidden="true"
          style={{
            width: "320px",
            maxWidth: "85vw",
            height: "5px",
            borderRadius: "999px",
            background: "rgba(249, 247, 247, 0.2)",
            overflow: "hidden"
          }}
        >
          <div
            ref={progressFillRef}
            style={{
              width: "100%",
              height: "100%",
              borderRadius: "999px",
              background: "linear-gradient(90deg, #F9F7F7 0%, #DBE2EF 50%, #3F72AF 100%)"
            }}
          />
        </div>
      </div>
    </aside>
  );
}
