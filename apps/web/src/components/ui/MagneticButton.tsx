/**
 * `apps/web/src/components/ui/MagneticButton.tsx`
 *
 * React Bits "magnet" + "StarBorder" button locked to the 4-Color Palette:
 * `#F9F7F7`, `#DBE2EF`, `#3F72AF`, `#112D4E`
 */
import type { PointerEvent as ReactPointerEvent, ReactNode } from "react";
import { createElement, useRef } from "react";
import { clsx } from "clsx";
import { gsap } from "gsap";
import { canAnimate } from "./motion-preference";

export type MagneticButtonVariant = "emerald" | "glass" | "primary" | "secondary";

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
    background: "#112D4E",
    color: "#F9F7F7",
    borderColor: "#3F72AF"
  },
  primary: {
    background: "#112D4E",
    color: "#F9F7F7",
    borderColor: "#3F72AF"
  },
  glass: {
    background: "var(--bg-elevated)",
    color: "var(--text-primary)",
    borderColor: "var(--border-strong)"
  },
  secondary: {
    background: "var(--bg-elevated)",
    color: "var(--text-primary)",
    borderColor: "var(--border-strong)"
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

  const isPrimary = variant === "emerald" || variant === "primary";

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
      duration: canAnimate() ? 0.85 : 0,
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
      className: clsx(
        "safi-btn",
        isPrimary ? "safi-btn-primary" : "safi-btn-secondary",
        "safi-star-border",
        className
      ),
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
        padding: "10px 20px",
        borderRadius: "999px",
        border: "1px solid",
        fontFamily: "var(--font-arabic)",
        fontSize: "14px",
        fontWeight: 700,
        cursor: disabled ? "not-allowed" : "pointer",
        opacity: disabled ? 0.5 : 1,
        willChange: "transform",
        ...VARIANT_STYLES[variant]
      }
    },
    children
  );
}
