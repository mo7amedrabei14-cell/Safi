/**
 * `apps/web/src/components/brand/useAnimatedFavicon.ts`
 *
 * Animates the browser tab's `<link rel="icon">` in real time using an offscreen
 * 64x64 `<canvas>` showing the 3 ascending data analytics columns & trend pulse
 * in `#F8F0E5`, `#EADBC8`, `#DAC0A3`, `#102C57`.
 */
import { useEffect } from "react";

export function useAnimatedFavicon(): void {
  useEffect(() => {
    if (
      typeof window === "undefined" ||
      typeof document === "undefined" ||
      typeof HTMLCanvasElement === "undefined"
    ) {
      return;
    }

    if (
      typeof window.matchMedia === "function" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches
    ) {
      return;
    }

    const canvas = document.createElement("canvas");
    canvas.width = 64;
    canvas.height = 64;
    let ctx: CanvasRenderingContext2D | null = null;
    try {
      ctx = canvas.getContext("2d");
    } catch {
      ctx = null;
    }
    if (ctx === null) {
      return;
    }

    let link = document.querySelector<HTMLLinkElement>('link[rel="icon"]');
    if (link === null) {
      link = document.createElement("link");
      link.rel = "icon";
      document.head.appendChild(link);
    }

    let frame = 0;
    const renderFrame = (): void => {
      if (document.hidden || ctx === null || link === null) {
        return;
      }
      frame += 1;
      const t = frame * 0.16;

      ctx.clearRect(0, 0, 64, 64);

      // 1. Royal Navy Squircle Tile (#102C57)
      ctx.save();
      ctx.beginPath();
      if (typeof ctx.roundRect === "function") {
        ctx.roundRect(3, 3, 58, 58, 14);
      } else {
        ctx.rect(3, 3, 58, 58);
      }
      ctx.fillStyle = "#102C57";
      ctx.fill();
      ctx.lineWidth = 2.2;
      ctx.strokeStyle = "#DAC0A3";
      ctx.stroke();
      ctx.restore();

      // 2. Animated Data Analytics Equalizer Columns (#DAC0A3, #EADBC8, #F8F0E5)
      const h1 = 15 + Math.sin(t) * 3;
      const h2 = 24 + Math.sin(t + 1.1) * 4;
      const h3 = 33 + Math.sin(t + 2.2) * 4;

      ctx.fillStyle = "#DAC0A3";
      ctx.fillRect(13, 50 - h1, 9, h1);

      ctx.fillStyle = "#EADBC8";
      ctx.fillRect(27, 50 - h2, 9, h2);

      ctx.fillStyle = "#F8F0E5";
      ctx.fillRect(41, 50 - h3, 9, h3);

      // 3. Ascending Trend Line (#F8F0E5)
      ctx.beginPath();
      ctx.moveTo(17, 44 - h1);
      ctx.lineTo(31, 44 - h2);
      ctx.lineTo(46, 44 - h3);
      ctx.strokeStyle = "#F8F0E5";
      ctx.lineWidth = 2.4;
      ctx.lineCap = "round";
      ctx.lineJoin = "round";
      ctx.stroke();

      try {
        link.href = canvas.toDataURL("image/png");
      } catch {
        // Ignore in non-browser runners
      }
    };

    renderFrame();
    const timer = window.setInterval(renderFrame, 140);

    return () => {
      window.clearInterval(timer);
    };
  }, []);
}
