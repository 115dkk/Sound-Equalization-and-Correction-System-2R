// Anchored floating panels: a popover holds a small form (axis range,
// settings); a menu holds commands (project menu). Both close on Escape and
// on a click outside, and hand focus back to the button that opened them.
import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent, type ReactNode, type RefObject } from "react";
import { Icon, type IconName } from "./Icon";
import { Portal, placeNear } from "./layer";

type Placement = "bottom-start" | "bottom-end" | "right-start";

function useFloating(open: boolean, anchor: RefObject<HTMLElement | null>, panel: RefObject<HTMLElement | null>, placement: Placement) {
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);
  useLayoutEffect(() => {
    if (!open) return;
    const place = () => {
      if (!anchor.current || !panel.current) return;
      const box = panel.current.getBoundingClientRect();
      const p = placeNear(anchor.current.getBoundingClientRect(), { width: box.width, height: box.height }, placement);
      setPos({ left: p.left, top: p.top });
    };
    place();
    window.addEventListener("resize", place);
    return () => window.removeEventListener("resize", place);
  }, [open, anchor, panel, placement]);
  return pos;
}

function useDismiss(open: boolean, onClose: () => void, anchor: RefObject<HTMLElement | null>, panel: RefObject<HTMLElement | null>) {
  useEffect(() => {
    if (!open) return;
    const onPointer = (event: PointerEvent) => {
      const target = event.target as Node;
      if (panel.current?.contains(target) || anchor.current?.contains(target)) return;
      onClose();
    };
    document.addEventListener("pointerdown", onPointer, true);
    return () => document.removeEventListener("pointerdown", onPointer, true);
  }, [open, onClose, anchor, panel]);
}

export function Popover({
  open,
  onClose,
  anchor,
  label,
  placement = "bottom-start",
  children,
  id,
  inline,
}: {
  open: boolean;
  onClose: () => void;
  anchor: RefObject<HTMLElement | null>;
  /** Accessible name of the panel. */
  label: string;
  placement?: Placement;
  children: ReactNode;
  id?: string;
  /** Render in place (gallery specimens). */
  inline?: boolean;
}) {
  const panel = useRef<HTMLDivElement>(null);
  const pos = useFloating(open && !inline, anchor, panel, placement);
  useDismiss(open && !inline, onClose, anchor, panel);
  useEffect(() => {
    if (!open || inline) return;
    const first = panel.current?.querySelector<HTMLElement>("[data-autofocus], input, select, button");
    first?.focus();
    const back = anchor.current;
    return () => back?.focus();
  }, [open, inline, anchor]);
  if (!open) return null;
  const body = (
    <div
      ref={panel}
      id={id}
      data-ui={id}
      role="dialog"
      aria-label={label}
      className={["ui-popover", inline && "ui-popover--inline"].filter(Boolean).join(" ")}
      style={inline ? undefined : pos ? { left: pos.left, top: pos.top } : { left: -9999, top: -9999 }}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.stopPropagation();
          onClose();
        }
      }}
    >
      {children}
    </div>
  );
  return inline ? body : <Portal>{body}</Portal>;
}

export type MenuItem = {
  id: string;
  label: string;
  icon?: IconName;
  shortcut?: string;
  disabled?: boolean;
  onSelect: () => void;
};

export function Menu({
  open,
  onClose,
  anchor,
  label,
  items,
  placement = "bottom-start",
  inline,
  id,
}: {
  open: boolean;
  onClose: () => void;
  anchor: RefObject<HTMLElement | null>;
  label: string;
  items: readonly MenuItem[];
  placement?: Placement;
  inline?: boolean;
  id?: string;
}) {
  const panel = useRef<HTMLDivElement>(null);
  const pos = useFloating(open && !inline, anchor, panel, placement);
  useDismiss(open && !inline, onClose, anchor, panel);
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  useEffect(() => {
    if (!open || inline) return;
    refs.current.find((node) => node && !node.disabled)?.focus();
    const back = anchor.current;
    return () => back?.focus();
  }, [open, inline, anchor]);
  if (!open) return null;
  const enabled = () => refs.current.filter((node): node is HTMLButtonElement => !!node && !node.disabled);
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const list = enabled();
    const index = list.indexOf(document.activeElement as HTMLButtonElement);
    const go = (i: number) => list[(i + list.length) % list.length]?.focus();
    if (event.key === "ArrowDown") go(index + 1);
    else if (event.key === "ArrowUp") go(index - 1);
    else if (event.key === "Home") go(0);
    else if (event.key === "End") go(list.length - 1);
    else if (event.key === "Escape" || event.key === "Tab") {
      if (event.key === "Escape") event.stopPropagation();
      onClose();
      return;
    } else return;
    event.preventDefault();
  };
  const body = (
    <div
      ref={panel}
      id={id}
      data-ui={id}
      role="menu"
      aria-label={label}
      className={["ui-menu", inline && "ui-popover--inline"].filter(Boolean).join(" ")}
      style={inline ? undefined : pos ? { left: pos.left, top: pos.top } : { left: -9999, top: -9999 }}
      onKeyDown={onKeyDown}
    >
      {items.map((item, index) => (
        <button
          key={item.id}
          ref={(node) => {
            refs.current[index] = node;
          }}
          type="button"
          role="menuitem"
          id={item.id}
          data-ui={item.id}
          tabIndex={-1}
          disabled={item.disabled}
          className="ui-menu__item"
          onClick={() => {
            onClose();
            item.onSelect();
          }}
        >
          <span className="ui-menu__icon">{item.icon && <Icon name={item.icon} size={18} />}</span>
          <span className="ui-menu__label">{item.label}</span>
          {item.shortcut && <kbd className="ui-menu__shortcut">{item.shortcut}</kbd>}
        </button>
      ))}
    </div>
  );
  return inline ? body : <Portal>{body}</Portal>;
}
