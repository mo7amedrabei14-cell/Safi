/**
 * `packages/icons` — RTL-aware directional icon library for SafiData v5.0.
 */
import React from "react";

export interface IconProps extends React.SVGProps<SVGSVGElement> {
  className?: string;
  size?: number;
  flipInRtl?: boolean;
}

export const SafiIcon: React.FC<IconProps> = ({
  className = "",
  size = 20,
  flipInRtl = false,
  style,
  ...props
}) => {
  return React.createElement(
    "svg",
    {
      xmlns: "http://www.w3.org/2000/svg",
      width: size,
      height: size,
      viewBox: "0 0 24 24",
      fill: "none",
      stroke: "currentColor",
      strokeWidth: 2,
      strokeLinecap: "round",
      strokeLinejoin: "round",
      className,
      style: {
        ...style,
        transform: flipInRtl ? "scaleX(-1)" : undefined
      },
      ...props
    },
    React.createElement("path", { d: "M5 12h14M12 5l7 7-7 7" })
  );
};
