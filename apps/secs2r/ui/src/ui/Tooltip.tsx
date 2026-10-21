// Context stays available to assistive technology while the visual tooltip is
// closed. Pointer, keyboard and explicit help disclosures share one surface.
import { cloneElement, isValidElement, useCallback, useEffect, useId, useLayoutEffect, useRef, useState, type ReactElement, type ReactNode } from "react";
import { Portal, placeNear } from "./layer";

export const TOOLTIP_DELAY_MS = 400;
export const TOOLTIP_CLOSE_DELAY_MS = 180;
let active: { id: string; close: () => void } | null = null;
let latestIntent = 0;
type Trigger = { "aria-describedby"?: string | undefined; "aria-expanded"?: boolean | undefined; "aria-controls"?: string | undefined };
export type TooltipProps = {
  content: ReactNode; children: ReactElement<Trigger>; side?: "top" | "bottom";
  delay?: number; staticOpen?: boolean; id?: string;
  /** A dedicated help button can pin the tooltip with click, touch or Enter. */
  toggle?: boolean;
};

export function Tooltip({ content, children, side = "top", delay = TOOLTIP_DELAY_MS, staticOpen = false, id: suppliedId, toggle = false }: TooltipProps) {
  const generatedId = useId();
  const id = suppliedId ?? generatedId;
  const anchor = useRef<HTMLSpanElement>(null);
  const tip = useRef<HTMLDivElement>(null);
  const opening = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const closing = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const pinned = useRef(false);
  const focused = useRef(false);
  const hovered = useRef(false);
  const tipHovered = useRef(false);
  const readingPointer = useRef(false);
  const dismissed = useRef(false);
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState<ReturnType<typeof placeNear> | null>(null);
  const clear = useCallback(() => { clearTimeout(opening.current); clearTimeout(closing.current); }, []);
  const hide = useCallback(() => {
    clear(); pinned.current = false; setOpen(false);
    if (active?.id === id) active = null;
  }, [clear, id]);
  const show = useCallback(() => {
    clear();
    if (dismissed.current) return;
    latestIntent++;
    if (active?.id !== id) active?.close();
    active = { id, close: hide };
    setOpen(true);
  }, [clear, hide, id]);
  const leave = () => {
    clearTimeout(opening.current);
    if (pinned.current || focused.current || readingPointer.current) return;
    clearTimeout(closing.current);
    closing.current = setTimeout(() => { if (!hovered.current && !tipHovered.current && !focused.current && !pinned.current) hide(); }, TOOLTIP_CLOSE_DELAY_MS);
  };
  useEffect(() => () => { clear(); if (active?.id === id) active = null; }, [clear, id]);
  useEffect(() => {
    if (!open) return;
    const inside = (target: EventTarget | null) => target instanceof Node && (anchor.current?.contains(target) || tip.current?.contains(target));
    const key = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      // Capture before a containing dialog/menu/input handles Escape.
      event.preventDefault(); event.stopPropagation(); latestIntent++; dismissed.current = true; hide();
    };
    const pointer = (event: Event) => {
      readingPointer.current = event.target instanceof Node && !!tip.current?.contains(event.target);
      if (!inside(event.target)) { latestIntent++; hide(); }
    };
    const release = () => {
      if (!readingPointer.current) return;
      readingPointer.current = false;
      // A text selection or touch scroll may blur the trigger. Restore its
      // keyboard position after reading without cancelling the native gesture.
      anchor.current?.querySelector<HTMLElement>("button, input, select, [tabindex]")?.focus({ preventScroll: true });
    };
    const focus = (event: FocusEvent) => {
      if (readingPointer.current && event.target === document.body) return;
      if (!inside(event.target)) { latestIntent++; hide(); }
    };
    window.addEventListener("keydown", key, true);
    document.addEventListener("pointerdown", pointer, true);
    // Touch generates compatibility mousedown after pointerup. It can move
    // focus again, so that native reading gesture owns the blur as well.
    document.addEventListener("mousedown", pointer, true);
    document.addEventListener("mouseup", release, true);
    document.addEventListener("pointerup", release, true);
    document.addEventListener("pointercancel", release, true);
    document.addEventListener("focusin", focus, true);
    return () => {
      window.removeEventListener("keydown", key, true);
      document.removeEventListener("pointerdown", pointer, true);
      document.removeEventListener("mousedown", pointer, true);
      document.removeEventListener("mouseup", release, true);
      document.removeEventListener("pointerup", release, true);
      document.removeEventListener("pointercancel", release, true);
      document.removeEventListener("focusin", focus, true);
    };
  }, [open, hide]);
  useLayoutEffect(() => {
    if (!open) return;
    const place = () => {
      if (!anchor.current || !tip.current) return;
      const box = tip.current.getBoundingClientRect();
      const next = placeNear(anchor.current.getBoundingClientRect(), { width: box.width, height: box.height }, side, 8);
      setPosition((old) => old?.left === next.left && old.top === next.top && old.side === next.side ? old : next);
    };
    place();
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(place);
    if (anchor.current) observer?.observe(anchor.current);
    if (tip.current) observer?.observe(tip.current);
    const visibility = typeof IntersectionObserver === "undefined" ? null : new IntersectionObserver(([entry]) => { if (entry && !entry.isIntersecting) hide(); });
    if (anchor.current) visibility?.observe(anchor.current);
    return () => { window.removeEventListener("resize", place); window.removeEventListener("scroll", place, true); observer?.disconnect(); visibility?.disconnect(); };
  }, [open, side, content, hide]);
  if (!isValidElement(children)) return children;
  const describedBy = [children.props["aria-describedby"], id].filter(Boolean).join(" ");
  const child = cloneElement(children, { "aria-describedby": describedBy, ...(toggle ? { "aria-expanded": open || staticOpen, "aria-controls": id } : {}) });
  if (staticOpen) return <span className="ui-tooltip-anchor ui-tooltip-anchor--static">{child}<span id={id} role="tooltip" className="ui-tooltip ui-tooltip--static">{content}</span></span>;
  return <span ref={anchor} className="ui-tooltip-anchor"
    onPointerEnter={(event) => {
      if (event.pointerType === "touch") return;
      hovered.current = true; dismissed.current = false; clearTimeout(closing.current);
      const intent = ++latestIntent;
      opening.current = setTimeout(() => { if (intent === latestIntent) show(); }, delay);
    }}
    onPointerLeave={() => { hovered.current = false; dismissed.current = false; leave(); }}
    onFocus={(event) => {
      if ((event.target as HTMLElement).matches?.(":focus-visible")) { focused.current = true; dismissed.current = false; show(); }
    }}
    onBlur={(event) => {
      focused.current = false;
      if (readingPointer.current || (event.relatedTarget instanceof Node && (anchor.current?.contains(event.relatedTarget) || tip.current?.contains(event.relatedTarget)))) return;
      hide();
    }}
    onClick={(event) => {
      // Portal clicks bubble through React's tree, but reading the tooltip is
      // not another click on its trigger.
      if (!(event.target instanceof Node) || !anchor.current?.contains(event.target)) return;
      if (!toggle) { hide(); return; }
      if (pinned.current) { latestIntent++; dismissed.current = true; hide(); }
      else { dismissed.current = false; pinned.current = true; show(); }
    }}
  >
    {child}
    <Portal><div ref={tip} id={id} role={open ? "tooltip" : undefined}
      className={open ? "ui-tooltip" : "visually-hidden"} data-side={position?.side ?? side}
      style={open ? position ? { left: position.left, top: position.top } : { left: -9999, top: -9999 } : undefined}
      onPointerEnter={() => { tipHovered.current = true; clearTimeout(closing.current); }}
      onPointerLeave={() => { tipHovered.current = false; leave(); }}
    >{content}</div></Portal>
  </span>;
}
