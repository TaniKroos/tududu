import React from "react";

// Brand name, not UI copy: never translated.
const WORDMARK = "tududu";

// Tududu wordmark. Kept under the upstream component name so imports stay
// unchanged and upstream merges don't conflict in every caller.
const HandyTextLogo = ({
  width,
  height,
  className,
}: {
  width?: number;
  height?: number;
  className?: string;
}) => {
  return (
    <svg
      width={width}
      height={height}
      className={className}
      viewBox="0 0 930 328"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      role="img"
      aria-label="Tududu"
    >
      <text
        x="465"
        y="232"
        textAnchor="middle"
        fontFamily="ui-rounded, 'SF Pro Rounded', 'Arial Rounded MT Bold', 'Segoe UI', system-ui, sans-serif"
        fontWeight={800}
        fontSize={230}
        letterSpacing={-4}
        strokeWidth={16}
        strokeLinejoin="round"
        style={{
          fill: "var(--color-logo-primary)",
          stroke: "var(--color-logo-stroke)",
          paintOrder: "stroke",
        }}
      >
        {WORDMARK}
      </text>
    </svg>
  );
};

export default HandyTextLogo;
