// Tooltip: the item's name and what choosing it does, in one or two
// sentences. Opens 800 ms after the pointer rests or the element takes
// keyboard focus, closes on leave, blur or Escape. Width is capped so lines
// stay readable. Never the only place a piece of information lives.
import {
  cloneElement,
  isValidElement,
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type ReactElement,
  type ReactNode,
} from "react";
import { Portal, placeNear } from "./layer";

export const TOOLTIP_DELAY_MS = 800;

type TooltipProps = {
  content: ReactNode;
  children: ReactElement<{ "aria-describedby"?: string | undefined }>;
  side?: "top" | "bottom";
  /** Delay before opening; the default is the app-wide 800 ms. */
  delay?: number;
  /** Gallery: always open, drawn in place above the element. */
  staticOpen?: boolean;
};

export function Tooltip({ content, children, side = "top", delay = TOOLTIP_DELAY_MS, staticOpen = false }: TooltipProps) {
  const id = useId();
  const anchorRef = useRef<HTMLSpanElement>(null);
  const tipRef = useRef<HTMLDivElement>(null);
  const timer = useRef<number | undefined>(undefined);
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);

  const clear = () => window.clearTimeout(timer.current);
  const show = useCallback(() => {
    clear();
    timer.current = window.setTimeout(() => setOpen(true), delay);
  }, [delay]);
  const hide = useCallback(() => {
    clear();
    setOpen(false);
  }, []);

  useEffect(() => clear, []);

  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") hide();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, hide]);

  useLayoutEffect(() => {
    if (!open || !anchorRef.current || !tipRef.current) return;
    const anchor = anchorRef.current.getBoundingClientRect();
    const box = tipRef.current.getBoundingClientRect();
    const placed = placeNear(anchor, { width: box.width, height: box.height }, side, 8);
    setPos({ left: placed.left, top: placed.top });
  }, [open, side, content]);

  if (!isValidElement(children)) return children;
  const describedBy = [children.props["aria-describedby"], open || staticOpen ? id : undefined].filter(Boolean).join(" ") || undefined;

  if (staticOpen) {
    return (
      <span className="ui-tooltip-anchor ui-tooltip-anchor--static">
        {cloneElement(children, { "aria-describedby": describedBy })}
        <span id={id} role="tooltip" className="ui-tooltip ui-tooltip--static">
          {content}
        </span>
      </span>
    );
  }

  return (
    <span
      ref={anchorRef}
      className="ui-tooltip-anchor"
      onPointerEnter={show}
      onPointerLeave={hide}
      onPointerDown={hide}
      onFocus={(event) => {
        if ((event.target as HTMLElement).matches?.(":focus-visible")) show();
      }}
      onBlur={hide}
    >
      {cloneElement(children, { "aria-describedby": describedBy })}
      {open && (
        <Portal>
          <div
            ref={tipRef}
            id={id}
            role="tooltip"
            className="ui-tooltip"
            style={pos ? { left: pos.left, top: pos.top } : { left: -9999, top: -9999 }}
          >
            {content}
          </div>
        </Portal>
      )}
    </span>
  );
}
