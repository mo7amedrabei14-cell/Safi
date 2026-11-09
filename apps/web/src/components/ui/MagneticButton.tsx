/**
 * `apps/web/src/components/ui/MagneticButton.tsx`
 *
 * Magnetic CTA Button
 * Locked Palette: `#F8F0E5`, `#EADBC8`, `#DAC0A3`, `#102C57`.
 */
import type { CSSProperties, PointerEvent as ReactPointerEvent, ReactNode } from "react";
import { useRef } from "react";
import { clsx } from "clsx";
import { gsap } from "gsap";
import { canAnimate } from "./motion-preference";

export type MagneticButtonVariant = "primary" | "secondary" | "emerald" | "glass";

export interface MagneticButtonProps {
  readonly children: ReactNode;
  readonly onClick?: (() => void) | undefined;
  readonly variant?: MagneticButtonVariant;
  readonly className?: string;
  readonly ariaLabel?: string;
  readonly type?: "button" | "submit";
  readonly disabled?: boolean;
}

const VARIANT_STYLES: Record<MagneticButtonVariant, CSSProperties> = {
  primary: {
    background: "var(--text-primary)",
    color: "var(--bg-surface)",
    borderColor: "var(--border-strong)"
  },
  emerald: {
    background: "var(--text-primary)",
    color: "var(--bg-surface)",
    borderColor: "var(--border-strong)"
  },
  secondary: {
    background: "var(--bg-elevated)",
    color: "var(--text-primary)",
    borderColor: "var(--border-strong)"
  },
  glass: {
    background: "var(--bg-elevated)",
    color: "var(--text-primary)",
    borderColor: "var(--border-strong)"
  }
};

export function MagneticButton({
  children,
  onClick,
  variant = "primary",
  className,
  ariaLabel,
  type = "button",
  disabled = false
}: MagneticButtonProps): ReactNode {
  const buttonRef = useRef<HTMLButtonElement | null>(null);
  const settersRef = useRef<{
    x: (value: number) => void;
    y: (value: number) => void;
  } | null>(null);

  const handlePointerMove = (event: ReactPointerEvent<HTMLButtonElement>): void => {
    const button = buttonRef.current;
    if (button === null || disabled || !canAnimate()) {
      return;
    }

    if (settersRef.current === null) {
      settersRef.current = {
        x: gsap.quickTo(button, "x", { duration: 0.35, ease: "power3.out" }),
        y: gsap.quickTo(button, "y", { duration: 0.35, ease: "power3.out" })
      };
    }

    const bounds = button.getBoundingClientRect();
    const dx = event.clientX - (bounds.left + bounds.width / 2);
    const dy = event.clientY - (bounds.top + bounds.height / 2);
    settersRef.current.x(dx * 0.16);
    settersRef.current.y(dy * 0.16);
  };

  const handlePointerLeave = (): void => {
    const button = buttonRef.current;
    if (button === null || !canAnimate()) {
      return;
    }
    gsap.to(button, {
      x: 0,
      y: 0,
      duration: 0.55,
      ease: "elastic.out(1, 0.4)"
    });
  };

  return (
    <button
      ref={buttonRef}
      type={type}
      disabled={disabled}
      aria-label={ariaLabel}
      data-safi-magnetic="true"
      onClick={onClick}
      onPointerMove={handlePointerMove}
      onPointerLeave={handlePointerLeave}
      className={clsx(
        "safi-btn safi-star-border",
        variant === "secondary" || variant === "glass" ? "safi-btn-secondary" : "safi-btn-primary",
        className
      )}
      style={{
        minHeight: "44px",
        minWidth: "44px",
        padding: "10px 22px",
        borderRadius: "999px",
        borderWidth: "1px",
        borderStyle: "solid",
        fontWeight: 700,
        fontSize: "13.5px",
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        gap: "8px",
        cursor: disabled ? "not-allowed" : "pointer",
        opacity: disabled ? 0.55 : 1,
        willChange: "transform",
        ...VARIANT_STYLES[variant]
      }}
    >
      {children}
    </button>
  );
}
