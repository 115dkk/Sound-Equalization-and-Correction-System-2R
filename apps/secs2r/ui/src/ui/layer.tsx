// Where floating things (tooltips, popovers, dialogs, toasts) are portalled.
// A `LayerRoot` sits inside each themed subtree so a floating element keeps
// the theme, accent and language of the place that opened it; without one,
// floating elements go to <body>.
import { createContext, useContext, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";

const LayerContext = createContext<HTMLElement | null>(null);

export function LayerRoot({ children }: { children: ReactNode }) {
  const [node, setNode] = useState<HTMLDivElement | null>(null);
  return (
    <LayerContext.Provider value={node}>
      {children}
      <div className="ui-layer" ref={setNode} />
    </LayerContext.Provider>
  );
}

export function Portal({ children }: { children: ReactNode }) {
  const node = useContext(LayerContext);
  const target = node ?? (typeof document !== "undefined" ? document.body : null);
  return target ? createPortal(children, target) : null;
}

/** Positions a floating box next to an anchor rectangle, inside the viewport. */
export function placeNear(
  anchor: DOMRect,
  box: { width: number; height: number },
  side: "top" | "bottom" | "bottom-start" | "bottom-end" | "right-start" = "bottom-start",
  gap = 6,
): { left: number; top: number; side: "top" | "bottom" | "right" } {
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const margin = 8;
  let left: number;
  let top: number;
  let placed: "top" | "bottom" | "right" = side === "top" ? "top" : side === "right-start" ? "right" : "bottom";
  if (side === "right-start") {
    left = anchor.right + gap;
    top = anchor.top;
    if (left + box.width > vw - margin) left = anchor.left - gap - box.width;
  } else {
    left =
      side === "top" || side === "bottom"
        ? anchor.left + anchor.width / 2 - box.width / 2
        : side === "bottom-end"
          ? anchor.right - box.width
          : anchor.left;
    top = side === "top" ? anchor.top - gap - box.height : anchor.bottom + gap;
    if (placed === "top" && top < margin) {
      top = anchor.bottom + gap;
      placed = "bottom";
    } else if (placed === "bottom" && top + box.height > vh - margin && anchor.top - gap - box.height >= margin) {
      top = anchor.top - gap - box.height;
      placed = "top";
    }
  }
  left = Math.min(Math.max(margin, left), Math.max(margin, vw - margin - box.width));
  top = Math.min(Math.max(margin, top), Math.max(margin, vh - margin - box.height));
  return { left, top, side: placed };
}
