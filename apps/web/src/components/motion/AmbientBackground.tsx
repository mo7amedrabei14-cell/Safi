/**
 * `apps/web/src/components/motion/AmbientBackground.tsx`
 *
 * ReactBits-Style Living Aurora + Interactive Data Constellation Canvas (60fps)
 * - Full-bleed 100vw x 100vh animated canvas with floating connected data nodes
 *   and flowing calm aurora waves that react to pointer movement
 * - Preserves the 3 `blur(130px)` halos and `28px 28px` grid for `motion-infrastructure.test.tsx`
 */
import type { ReactNode } from "react";
import { useEffect, useRef } from "react";
import { gsap } from "gsap";
import { canAnimate } from "../ui/motion-preference";

interface HaloSpec {
  readonly id: string;
  readonly width: number;
  readonly height: number;
  readonly top: string;
  readonly left: string;
  readonly gradient: string;
  readonly opacity: number;
}

const HALO_SPECS: readonly HaloSpec[] = [
  {
    id: "halo-calm-sky",
    width: 920,
    height: 520,
    top: "-12%",
    left: "52%",
    gradient: "radial-gradient(circle at 50% 50%, rgba(59, 130, 246, 0.18) 0%, transparent 72%)",
    opacity: 0.85
  },
  {
    id: "halo-calm-indigo",
    width: 820,
    height: 460,
    top: "24%",
    left: "4%",
    gradient: "radial-gradient(circle at 50% 50%, rgba(99, 102, 241, 0.14) 0%, transparent 72%)",
    opacity: 0.75
  },
  {
    id: "halo-calm-emerald",
    width: 740,
    height: 420,
    top: "64%",
    left: "40%",
    gradient: "radial-gradient(circle at 50% 50%, rgba(16, 185, 129, 0.12) 0%, transparent 72%)",
    opacity: 0.65
  }
];

interface Particle {
  x: number;
  y: number;
  vx: number;
  vy: number;
  r: number;
}

export function AmbientBackground(): ReactNode {
  const stageRef = useRef<HTMLDivElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    const stage = stageRef.current;
    if (stage === null || !canAnimate()) {
      return;
    }

    const halos = Array.from(stage.querySelectorAll<HTMLDivElement>("[data-safi-halo]"));
    const setters = halos.map((el, index) => ({
      x: gsap.quickTo(el, "x", { duration: 1.1 + index * 0.2, ease: "power3.out" }),
      y: gsap.quickTo(el, "y", { duration: 1.1 + index * 0.2, ease: "power3.out" }),
      factor: (index + 1) * 22
    }));

    let pointerX = window.innerWidth * 0.5;
    let pointerY = window.innerHeight * 0.35;

    const onPointerMove = (event: PointerEvent): void => {
      pointerX = event.clientX;
      pointerY = event.clientY;
      const nx = event.clientX / Math.max(1, window.innerWidth) - 0.5;
      const ny = event.clientY / Math.max(1, window.innerHeight) - 0.5;
      for (const s of setters) {
        s.x(nx * s.factor);
        s.y(ny * s.factor);
      }
    };

    window.addEventListener("pointermove", onPointerMove, { passive: true });

    // ReactBits Interactive Constellation & Aurora Wave Canvas
    const canvas = canvasRef.current;
    let rafId = 0;
    if (canvas !== null && typeof HTMLCanvasElement !== "undefined") {
      let ctx: CanvasRenderingContext2D | null = null;
      try {
        ctx = canvas.getContext("2d");
      } catch {
        ctx = null;
      }

      if (ctx !== null) {
        const resize = (): void => {
          canvas.width = window.innerWidth;
          canvas.height = window.innerHeight;
        };
        resize();
        window.addEventListener("resize", resize);

        const count = 44;
        const particles: Particle[] = Array.from({ length: count }, (_, i) => ({
          x: ((i * 137.5) % 100) * 0.01 * canvas.width,
          y: ((i * 73.1) % 100) * 0.01 * canvas.height,
          vx: (i % 2 === 0 ? 1 : -1) * (0.38 + (i % 5) * 0.08),
          vy: (i % 3 === 0 ? 1 : -1) * (0.3 + (i % 4) * 0.07),
          r: 2.2 + (i % 3) * 0.85
        }));

        const render = (): void => {
          const isDark =
            document.documentElement.getAttribute("data-theme") === "google-gemini" ||
            document.documentElement.getAttribute("data-theme") === "dark";

          ctx.clearRect(0, 0, canvas.width, canvas.height);

          const dotColor = isDark ? "rgba(148, 163, 184, 0.55)" : "rgba(37, 99, 235, 0.42)";
          const lineColor = isDark ? "rgba(96, 165, 250, 0.2)" : "rgba(37, 99, 235, 0.18)";

          for (let i = 0; i < particles.length; i += 1) {
            const p = particles[i];
            if (!p) continue;
            p.x += p.vx;
            p.y += p.vy;

            if (p.x < 0) p.x = canvas.width;
            if (p.x > canvas.width) p.x = 0;
            if (p.y < 0) p.y = canvas.height;
            if (p.y > canvas.height) p.y = 0;

            ctx.beginPath();
            ctx.arc(p.x, p.y, p.r, 0, Math.PI * 2);
            ctx.fillStyle = dotColor;
            ctx.fill();

            // Connect nearby particles
            for (let j = i + 1; j < particles.length; j += 1) {
              const q = particles[j];
              if (!q) continue;
              const dx = p.x - q.x;
              const dy = p.y - q.y;
              const distSq = dx * dx + dy * dy;
              if (distSq < 32000) {
                ctx.beginPath();
                ctx.moveTo(p.x, p.y);
                ctx.lineTo(q.x, q.y);
                ctx.strokeStyle = lineColor;
                ctx.lineWidth = 1.1;
                ctx.stroke();
              }
            }

            // Connect to pointer (ReactBits Magnet/Constellation effect)
            const pdx = p.x - pointerX;
            const pdy = p.y - pointerY;
            if (pdx * pdx + pdy * pdy < 36000) {
              ctx.beginPath();
              ctx.moveTo(p.x, p.y);
              ctx.lineTo(pointerX, pointerY);
              ctx.strokeStyle = isDark ? "rgba(96, 165, 250, 0.28)" : "rgba(37, 99, 235, 0.22)";
              ctx.lineWidth = 1.2;
              ctx.stroke();
            }
          }

          rafId = window.requestAnimationFrame(render);
        };

        rafId = window.requestAnimationFrame(render);

        return () => {
          window.removeEventListener("pointermove", onPointerMove);
          window.removeEventListener("resize", resize);
          window.cancelAnimationFrame(rafId);
        };
      }
    }

    return () => {
      window.removeEventListener("pointermove", onPointerMove);
    };
  }, []);

  return (
    <div
      ref={stageRef}
      aria-hidden="true"
      data-safi-ambient="true"
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 0,
        pointerEvents: "none",
        overflow: "hidden"
      }}
    >
      {HALO_SPECS.map((halo) => (
        <div
          key={halo.id}
          data-safi-halo={halo.id}
          style={{
            position: "absolute",
            top: halo.top,
            left: halo.left,
            width: `${halo.width}px`,
            height: `${halo.height}px`,
            background: halo.gradient,
            opacity: halo.opacity,
            filter: "blur(130px)",
            borderRadius: "999px",
            willChange: "transform"
          }}
        />
      ))}

      <div
        data-safi-grid="true"
        style={{
          position: "absolute",
          inset: 0,
          backgroundImage: "radial-gradient(rgba(148, 163, 184, 0.32) 1px, transparent 1px)",
          backgroundSize: "28px 28px",
          opacity: 0.28
        }}
      />

      <canvas
        ref={canvasRef}
        style={{
          position: "absolute",
          inset: 0,
          width: "100%",
          height: "100%",
          display: "block"
        }}
      />
    </div>
  );
}
