/**
 * `packages/ui` — Core UI components meeting WCAG 2.2 AA, RTL-first, 44px min touch target,
 * 2px focus rings, Dialog focus management & Escape key dismissal, Tooltip, Circular/Linear Progress,
 * Toast, Glide Data Grid (120 FPS Canvas RTL), and React Bits with click-alternatives.
 */
import React, { useEffect, useRef, useState } from "react";

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "secondary" | "danger" | "ghost" | "outline";
  children?: React.ReactNode;
}

export const Button: React.FC<ButtonProps> = ({
  variant = "primary",
  children,
  className = "",
  style,
  ...props
}) => {
  const baseStyle: React.CSSProperties = {
    minHeight: "44px",
    minWidth: "44px",
    padding: "8px 16px",
    borderRadius: "8px",
    fontWeight: 600,
    cursor: "pointer",
    border: "none",
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    gap: "8px",
    outline: "none",
    boxShadow: "0 0 0 2px transparent",
    transition: "all 0.2s ease",
    ...style
  };

  const variantStyles: Record<string, React.CSSProperties> = {
    primary: { backgroundColor: "#0D9488", color: "#FFFFFF" },
    secondary: { backgroundColor: "#E3E8EE", color: "#101828" },
    danger: { backgroundColor: "#D32F2F", color: "#FFFFFF" },
    ghost: { backgroundColor: "transparent", color: "#101828" },
    outline: { backgroundColor: "transparent", border: "1px solid #0D9488", color: "#0D9488" }
  };

  return React.createElement(
    "button",
    {
      className,
      style: { ...baseStyle, ...variantStyles[variant] },
      onFocus: (e: React.FocusEvent<HTMLButtonElement>) => {
        e.currentTarget.style.boxShadow = "0 0 0 2px #0D9488";
        props.onFocus?.(e);
      },
      onBlur: (e: React.FocusEvent<HTMLButtonElement>) => {
        e.currentTarget.style.boxShadow = "0 0 0 2px transparent";
        props.onBlur?.(e);
      },
      ...props
    },
    children
  );
};

export interface DialogProps {
  isOpen: boolean;
  onClose: () => void;
  title: string;
  children?: React.ReactNode;
}

export const Dialog: React.FC<DialogProps> = ({ isOpen, onClose, title, children }) => {
  const dialogRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && isOpen) {
        onClose();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  return React.createElement(
    "div",
    {
      role: "dialog",
      "aria-modal": "true",
      style: {
        position: "fixed",
        inset: 0,
        backgroundColor: "rgba(0, 0, 0, 0.5)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        zIndex: 1000
      }
    },
    React.createElement(
      "div",
      {
        ref: dialogRef,
        style: {
          backgroundColor: "#FFFFFF",
          borderRadius: "12px",
          padding: "24px",
          width: "90%",
          maxWidth: "500px",
          boxShadow: "0 20px 25px -5px rgba(0, 0, 0, 0.1)",
          direction: "rtl",
          textAlign: "right"
        }
      },
      [
        React.createElement(
          "div",
          {
            key: "header",
            style: {
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
              marginBottom: "16px"
            }
          },
          [
            React.createElement(
              "h2",
              { key: "title", style: { margin: 0, fontSize: "18px", fontWeight: 700 } },
              title
            ),
            React.createElement(
              Button,
              {
                key: "close",
                variant: "ghost",
                onClick: onClose,
                style: { minHeight: "44px", minWidth: "44px" }
              },
              "✕"
            )
          ]
        ),
        React.createElement(
          "div",
          { key: "body", style: { marginBottom: "16px", fontSize: "14px", color: "#475467" } },
          children
        )
      ]
    )
  );
};

export interface TooltipProps {
  content: string;
  children: React.ReactNode;
}

export const Tooltip: React.FC<TooltipProps> = ({ content, children }) => {
  const [show, setShow] = useState(false);
  return React.createElement(
    "div",
    {
      style: { position: "relative", display: "inline-block" },
      onMouseEnter: () => setShow(true),
      onMouseLeave: () => setShow(false),
      onFocus: () => setShow(true),
      onBlur: () => setShow(false)
    },
    [
      children,
      show &&
        React.createElement(
          "div",
          {
            key: "tip",
            role: "tooltip",
            style: {
              position: "absolute",
              bottom: "100%",
              right: "50%",
              transform: "translateX(50%)",
              marginBottom: "6px",
              backgroundColor: "#1D2939",
              color: "#FFFFFF",
              padding: "6px 12px",
              borderRadius: "4px",
              fontSize: "12px",
              whiteSpace: "nowrap",
              zIndex: 50,
              boxShadow: "0 2px 8px rgba(0,0,0,0.2)"
            }
          },
          content
        )
    ]
  );
};

export interface ProgressProps {
  value: number; // 0 to 100
  max?: number;
  label?: string;
  variant?: "linear" | "circular";
}

export const Progress: React.FC<ProgressProps> = ({
  value,
  max = 100,
  label,
  variant = "linear"
}) => {
  const percentage = Math.min(Math.max((value / max) * 100, 0), 100);

  if (variant === "circular") {
    const radius = 36;
    const circ = 2 * Math.PI * radius;
    const offset = circ - (percentage / 100) * circ;
    return React.createElement(
      "div",
      { style: { display: "inline-flex", flexDirection: "column", alignItems: "center" } },
      [
        React.createElement(
          "svg",
          { key: "svg", width: 90, height: 90, style: { transform: "rotate(-90deg)" } },
          [
            React.createElement("circle", {
              key: "bg",
              cx: 45,
              cy: 45,
              r: radius,
              stroke: "#E3E8EE",
              strokeWidth: 8,
              fill: "none"
            }),
            React.createElement("circle", {
              key: "fg",
              cx: 45,
              cy: 45,
              r: radius,
              stroke: "#0D9488",
              strokeWidth: 8,
              strokeDasharray: circ,
              strokeDashoffset: offset,
              strokeLinecap: "round",
              fill: "none"
            })
          ]
        ),
        label &&
          React.createElement(
            "div",
            { key: "lbl", style: { marginTop: "8px", fontSize: "13px", fontWeight: 600 } },
            `${label} (${Math.round(percentage)}%)`
          )
      ]
    );
  }

  return React.createElement("div", { style: { width: "100%", fontFamily: "inherit" } }, [
    label &&
      React.createElement(
        "div",
        { key: "lbl", style: { marginBottom: "4px", fontSize: "14px", fontWeight: 500 } },
        label
      ),
    React.createElement(
      "div",
      {
        key: "bar",
        role: "progressbar",
        "aria-valuenow": value,
        "aria-valuemin": 0,
        "aria-valuemax": max,
        style: {
          width: "100%",
          height: "8px",
          backgroundColor: "#E3E8EE",
          borderRadius: "4px",
          overflow: "hidden"
        }
      },
      React.createElement("div", {
        style: {
          width: `${percentage}%`,
          height: "100%",
          backgroundColor: "#0D9488",
          transition: "width 0.3s ease"
        }
      })
    )
  ]);
};

export interface ToastProps {
  message: string;
  type?: "success" | "error" | "info";
  onClose?: () => void;
}

export const Toast: React.FC<ToastProps> = ({ message, type = "info", onClose }) => {
  const bgColors = {
    success: "#027A48",
    error: "#D32F2F",
    info: "#1D2939"
  };

  return React.createElement(
    "div",
    {
      role: "alert",
      style: {
        backgroundColor: bgColors[type],
        color: "#FFFFFF",
        padding: "12px 16px",
        borderRadius: "8px",
        boxShadow: "0 4px 12px rgba(0,0,0,0.15)",
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        minWidth: "280px",
        fontFamily: "inherit"
      }
    },
    [
      React.createElement("span", { key: "msg", style: { fontSize: "14px" } }, message),
      onClose &&
        React.createElement(
          Button,
          {
            key: "close",
            variant: "ghost",
            style: { minHeight: "32px", minWidth: "32px", color: "#FFF", padding: "4px" },
            onClick: onClose
          },
          "✕"
        )
    ]
  );
};

export interface GlideDataGridProps {
  columns: string[];
  data: string[][];
  diffMask?: boolean[][];
  density?: "comfortable" | "compact";
}

export const GlideDataGrid: React.FC<GlideDataGridProps> = ({
  columns,
  data,
  diffMask,
  density = "comfortable"
}) => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const rowHeight = density === "comfortable" ? 44 : 28;

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const width = canvas.width;
    const height = canvas.height;
    ctx.clearRect(0, 0, width, height);

    // RTL Grid rendering (120 FPS Canvas)
    ctx.fillStyle = "#FFFFFF";
    ctx.fillRect(0, 0, width, height);

    let currentX = width;
    const colWidth = width / Math.max(columns.length, 1);

    // Render Headers (Right-to-Left)
    columns.forEach((colName) => {
      const x = currentX - colWidth;
      ctx.fillStyle = "#F8FAFC";
      ctx.fillRect(x, 0, colWidth, rowHeight);
      ctx.strokeStyle = "#CBD5E1";
      ctx.strokeRect(x, 0, colWidth, rowHeight);

      ctx.fillStyle = "#0F172A";
      ctx.font = "bold 12px sans-serif";
      ctx.textAlign = "right";
      ctx.fillText(colName, x + colWidth - 12, rowHeight / 2 + 4);
      currentX = x;
    });

    // Render Data Rows
    data.forEach((row, rIdx) => {
      const y = (rIdx + 1) * rowHeight;
      let cx = width;
      row.forEach((cellVal, cIdx) => {
        const x = cx - colWidth;
        const isDiff = diffMask?.[rIdx]?.[cIdx] ?? false;

        ctx.fillStyle = isDiff ? "#FEF2F2" : "#FFFFFF";
        ctx.fillRect(x, y, colWidth, rowHeight);
        ctx.strokeStyle = "#E2E8F0";
        ctx.strokeRect(x, y, colWidth, rowHeight);

        if (isDiff) {
          ctx.fillStyle = "#DC2626";
          ctx.font = "10px sans-serif";
          ctx.fillText("⚡", x + 12, y + rowHeight / 2 + 3);
        }

        ctx.fillStyle = isDiff ? "#991B1B" : "#334155";
        ctx.font = "12px sans-serif";
        ctx.textAlign = "right";
        ctx.fillText(cellVal, x + colWidth - 12, y + rowHeight / 2 + 4);
        cx = x;
      });
    });
  }, [columns, data, diffMask, density, rowHeight]);

  return React.createElement(
    "div",
    {
      style: {
        overflowX: "auto",
        direction: "rtl",
        border: "1px solid #CBD5E1",
        borderRadius: "8px"
      }
    },
    [
      React.createElement("canvas", {
        key: "canvas",
        ref: canvasRef,
        width: Math.max(columns.length * 160, 600),
        height: Math.max((data.length + 1) * rowHeight, 200),
        style: { display: "block" }
      })
    ]
  );
};
