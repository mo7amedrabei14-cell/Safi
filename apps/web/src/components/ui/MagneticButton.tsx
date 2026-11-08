/**
 * `apps/web/src/components/ui/MagneticButton.tsx`
 *
 * React Bits "magnet" + "StarBorder" button. The cursor pulls the button by a few
 * pixels through `gsap.quickTo` (one reusable tween per axis), and releasing the
 * pointer returns it with `elastic.out(1, 0.4)`.
 *
 * Accessibility: a real <button>, `min-inline-size`/`min-block-size` of 44px
 * (WCAG 2.2 AA target size), and a visible focus ring from the global stylesheet.
 */
import type { PointerEvent as ReactPointerEvent, ReactNode } from "react";
import { createElement, useRef } from "react";
import { clsx } from "clsx";
import { gsap } from "gsap";
import { canAnimate } from "./motion-preference";

export type MagneticButtonVariant = "emerald" | "glass";

export interface MagneticButtonProps {
  readonly children: ReactNode;
  readonly onClick?: (() => void) | undefined;
  readonly variant?: MagneticButtonVariant;
  readonly type?: "button" | "submit" | "reset";
  readonly className?: string;
  readonly ariaLabel?: string;
  readonly disabled?: boolean;
  /** Maximum pull distance in pixels. */
  readonly strength?: number;
}

const VARIANT_STYLES: Record<MagneticButtonVariant, Record<string, string>> = {
  emerald: {
    backgroundImage: "linear-gradient(135deg, #10b981 0%, #06b6d4 100%)",
    color: "#04110c",
    borderColor: "transparent"
  },
  glass: {
    background: "var(--glass-bg)",
    color: "var(--text-primary)",
    borderColor: "var(--glass-border)"
  }
};

interface MagnetSetters {
  x: (value: number) => void;
  y: (value: number) => void;
  scale: (value: number) => void;
}

export function MagneticButton({
  children,
  onClick,
  variant = "emerald",
  type = "button",
  className,
  ariaLabel,
  disabled = false,
  strength = 6
}: MagneticButtonProps): ReactNode {
  const buttonRef = useRef<HTMLButtonElement | null>(null);
  const settersRef = useRef<MagnetSetters | null>(null);

  const handlePointerMove = (event: ReactPointerEvent<HTMLButtonElement>): void => {
    const button = buttonRef.current;
    if (button === null || !canAnimate()) {
      return;
    }

    if (settersRef.current === null) {
      settersRef.current = {
        x: gsap.quickTo(button, "x", { duration: 0.35, ease: "power2.out" }),
        y: gsap.quickTo(button, "y", { duration: 0.35, ease: "power2.out" }),
        scale: gsap.quickTo(button, "scale", { duration: 0.35, ease: "power2.out" })
      };
    }

    const bounds = button.getBoundingClientRect();
    const offsetX = (event.clientX - bounds.left) / bounds.width - 0.5;
    const offsetY = (event.clientY - bounds.top) / bounds.height - 0.5;

    settersRef.current.x(offsetX * strength * 2);
    settersRef.current.y(offsetY * strength * 2);
    settersRef.current.scale(1.03);
  };

  const handlePointerLeave = (): void => {
    const button = buttonRef.current;
    if (button === null) {
      return;
    }
    gsap.to(button, {
      x: 0,
      y: 0,
      scale: 1,
      duration: canAnimate() ? 0.9 : 0,
      ease: "elastic.out(1, 0.4)"
    });
  };

  return createElement(
    "button",
    {
      ref: buttonRef,
      type,
      onClick,
      onPointerMove: handlePointerMove,
      onPointerLeave: handlePointerLeave,
      onBlur: handlePointerLeave,
      className: clsx("safi-star-border", className),
      "aria-label": ariaLabel,
      disabled,
      "data-safi-magnetic": "true",
      style: {
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        gap: "8px",
        minHeight: "44px",
        minWidth: "44px",
        paddingBlock: "12px",
        paddingInline: "22px",
        borderRadius: "14px",
        border: "1px solid",
        fontFamily: "var(--font-arabic)",
        fontSize: "15px",
        fontWeight: 600,
        cursor: disabled ? "not-allowed" : "pointer",
        opacity: disabled ? 0.55 : 1,
        willChange: "transform",
        ...VARIANT_STYLES[variant]
      }
    },
    children
  );
}
