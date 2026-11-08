/**
 * `apps/web/src/components/ui/SpotlightCard.tsx`
 *
 * Glass card with a cursor-following spotlight. The pointer position is written
 * straight into the `--mouse-x` / `--mouse-y` custom properties on the element, so
 * React never re-renders while the mouse moves (no `useState` per pointer frame).
 *
 * Optional 3D tilt runs through `gsap.quickTo`, which reuses one tween per axis.
 */
import type { PointerEvent as ReactPointerEvent, ReactNode } from "react";
import { useRef } from "react";
import { clsx } from "clsx";
import { gsap } from "gsap";
import { canAnimate } from "./motion-preference";

export interface SpotlightCardProps {
  readonly children: ReactNode;
  readonly className?: string;
  /** Adds a subtle 3D tilt toward the pointer. */
  readonly tilt?: boolean;
  /** Extra inline padding in pixels. */
  readonly padding?: number;
}

interface TiltSetters {
  rotationX: (value: number) => void;
  rotationY: (value: number) => void;
}

export function SpotlightCard({
  children,
  className,
  tilt = false,
  padding = 24
}: SpotlightCardProps): ReactNode {
  const cardRef = useRef<HTMLDivElement | null>(null);
  const tiltRef = useRef<HTMLDivElement | null>(null);
  const settersRef = useRef<TiltSetters | null>(null);

  const handlePointerMove = (event: ReactPointerEvent<HTMLDivElement>): void => {
    const card = cardRef.current;
    if (card === null) {
      return;
    }

    const bounds = card.getBoundingClientRect();
    const x = event.clientX - bounds.left;
    const y = event.clientY - bounds.top;

    card.style.setProperty("--mouse-x", `${x.toFixed(1)}px`);
    card.style.setProperty("--mouse-y", `${y.toFixed(1)}px`);

    if (!tilt || !canAnimate() || tiltRef.current === null) {
      return;
    }

    if (settersRef.current === null) {
      settersRef.current = {
        rotationX: gsap.quickTo(tiltRef.current, "rotationX", {
          duration: 0.45,
          ease: "power2.out"
        }),
        rotationY: gsap.quickTo(tiltRef.current, "rotationY", {
          duration: 0.45,
          ease: "power2.out"
        })
      };
    }

    const offsetX = x / bounds.width - 0.5;
    const offsetY = y / bounds.height - 0.5;
    settersRef.current.rotationY(offsetX * 8);
    settersRef.current.rotationX(offsetY * -8);
  };

  const handlePointerLeave = (): void => {
    const card = cardRef.current;
    if (card !== null) {
      card.style.setProperty("--mouse-x", "50%");
      card.style.setProperty("--mouse-y", "50%");
    }

    const tiltElement = tiltRef.current;
    if (tiltElement === null) {
      return;
    }

    gsap.to(tiltElement, {
      rotationX: 0,
      rotationY: 0,
      duration: canAnimate() ? 0.6 : 0,
      ease: "elastic.out(1, 0.5)"
    });
  };

  return (
    <div
      ref={cardRef}
      className={clsx("safi-spotlight", className)}
      onPointerMove={handlePointerMove}
      onPointerLeave={handlePointerLeave}
      style={{ padding: `${padding}px`, perspective: tilt ? "900px" : undefined }}
    >
      <div
        ref={tiltRef}
        style={{
          transformStyle: tilt ? "preserve-3d" : undefined,
          willChange: tilt ? "transform" : undefined
        }}
      >
        {children}
      </div>
    </div>
  );
}
