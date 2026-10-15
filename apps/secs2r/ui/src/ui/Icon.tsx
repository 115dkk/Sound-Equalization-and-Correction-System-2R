// One outline icon set drawn for SECS 2R: 24-unit grid, round caps, one
// stroke weight (1.5 beside regular text, 2 beside semibold via `weight`).
// Icons take `currentColor`; states recolour the same SVG through CSS.
import type { SVGProps } from "react";

const PATHS = {
  check: ["M5 12.5l4.5 4.5L19 7.5"],
  close: ["M6.5 6.5l11 11", "M17.5 6.5l-11 11"],
  chevronDown: ["M6.5 9.5l5.5 5.5 5.5-5.5"],
  chevronUp: ["M6.5 14.5l5.5-5.5 5.5 5.5"],
  chevronRight: ["M9.5 6.5l5.5 5.5-5.5 5.5"],
  chevronLeft: ["M14.5 6.5L9 12l5.5 5.5"],
  arrowLeft: ["M19 12H5.5", "M11 6.5L5.5 12l5.5 5.5"],
  arrowRight: ["M5 12h13.5", "M13 6.5l5.5 5.5-5.5 5.5"],
  warning: ["M12 4L21 19.5H3L12 4z", "M12 10v4", "M12 16.8v.2"],
  error: ["M12 3.5a8.5 8.5 0 1 0 0 17 8.5 8.5 0 0 0 0-17z", "M12 7.5V13", "M12 16.3v.2"],
  info: ["M12 3.5a8.5 8.5 0 1 0 0 17 8.5 8.5 0 0 0 0-17z", "M12 11v5.5", "M12 7.7v.2"],
  lock: ["M6.5 11h11a1 1 0 0 1 1 1v7a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1v-7a1 1 0 0 1 1-1z", "M8.5 11V8a3.5 3.5 0 0 1 7 0v3"],
  plus: ["M12 5.5v13", "M5.5 12h13"],
  minus: ["M5.5 12h13"],
  play: ["M8 5.8v12.4a.6.6 0 0 0 .9.5l9.6-6.2a.6.6 0 0 0 0-1L8.9 5.3a.6.6 0 0 0-.9.5z"],
  stop: ["M7.5 6.5h9a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1h-9a1 1 0 0 1-1-1v-9a1 1 0 0 1 1-1z"],
  folder: ["M4 7.5a1.5 1.5 0 0 1 1.5-1.5h3.8l2 2.2h7.2A1.5 1.5 0 0 1 20 9.7v7.8a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 17.5z"],
  save: ["M12 4.5v10", "M7.5 10.5L12 15l4.5-4.5", "M5 19.5h14"],
  undo: ["M9 14L4.5 9.5 9 5", "M4.5 9.5H15a4.5 4.5 0 0 1 0 9h-3"],
  help: [
    "M12 3.5a8.5 8.5 0 1 0 0 17 8.5 8.5 0 0 0 0-17z",
    "M9.6 9.4a2.5 2.5 0 1 1 3.4 2.4c-.6.2-1 .8-1 1.4v.6",
    "M12 16.6v.2",
  ],
  settings: ["M4 7.5h9", "M17 7.5h3", "M4 16.5h3", "M11 16.5h9", "M15 5.5v4", "M9 14.5v4"],
  more: ["M6.5 12h.01", "M12 12h.01", "M17.5 12h.01"],
  list: ["M9 7h11", "M9 12h11", "M9 17h11", "M4.5 7h.01", "M4.5 12h.01", "M4.5 17h.01"],
  axes: ["M4.5 4v15.5H20", "M8 15.5l3.5-4.5 3 2.5L19 8"],
  file: ["M7 3.5h6.5L18 8v11.5a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1v-15a1 1 0 0 1 1-1z", "M13.5 3.5V8H18"],
  dot: ["M12 12h.01"],
} as const;

export type IconName = keyof typeof PATHS;

type IconProps = Omit<SVGProps<SVGSVGElement>, "name"> & {
  name: IconName;
  size?: number;
  weight?: "regular" | "semibold";
  /** Text alternative. Without it the icon is decorative (aria-hidden). */
  label?: string;
};

export function Icon({ name, size = 18, weight = "regular", label, className, ...rest }: IconProps) {
  const dots = name === "more" || name === "dot";
  return (
    <svg
      viewBox="0 0 24 24"
      width={size}
      height={size}
      fill="none"
      stroke="currentColor"
      strokeWidth={dots ? 3 : weight === "semibold" ? 2 : 1.5}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={["ui-icon", className].filter(Boolean).join(" ")}
      aria-hidden={label ? undefined : true}
      role={label ? "img" : undefined}
      aria-label={label}
      focusable="false"
      {...rest}
    >
      {PATHS[name].map((d) => (
        <path key={d} d={d} />
      ))}
    </svg>
  );
}

/** A rotating arc for indeterminate waits; always paired with words. */
export function Spinner({ size = 16, className }: { size?: number; className?: string }) {
  return (
    <svg
      viewBox="0 0 24 24"
      width={size}
      height={size}
      className={["ui-spinner", className].filter(Boolean).join(" ")}
      aria-hidden="true"
      focusable="false"
    >
      <circle cx="12" cy="12" r="8.5" fill="none" stroke="currentColor" strokeOpacity="0.25" strokeWidth="2" />
      <path d="M12 3.5a8.5 8.5 0 0 1 8.5 8.5" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
    </svg>
  );
}
