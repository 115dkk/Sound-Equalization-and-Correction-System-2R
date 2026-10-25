// Toasts: short results ("위치 3 측정 지움") with at most one action, most
// often 되돌리기. They stack at the bottom right above the job tray, leave on
// their own after a while (paused while hovered or focused) and never carry
// the only copy of an error: errors also go to the job tray's history.
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useT } from "../i18n/react";
import { Icon, type IconName } from "./Icon";
import { Portal } from "./layer";

type ToastTone = "neutral" | "success" | "warning" | "danger";

type ToastInput = {
  message: ReactNode;
  tone?: ToastTone;
  action?: { label: string; onAction: () => void; id?: string };
  /** Milliseconds on screen; 0 keeps it until closed. */
  duration?: number;
};

type ToastItem = ToastInput & { id: number };

type ToastApi = { show: (toast: ToastInput) => number; dismiss: (id: number) => void };

const ToastContext = createContext<ToastApi | null>(null);

export function useToast(): ToastApi {
  const api = useContext(ToastContext);
  if (!api) throw new Error("useToast() needs a <ToastProvider> above it");
  return api;
}

const TONE_ICON: Record<ToastTone, IconName> = { neutral: "info", success: "check", warning: "warning", danger: "error" };

export function ToastProvider({ children, initial = [] }: { children: ReactNode; initial?: ToastInput[] }) {
  const [items, setItems] = useState<ToastItem[]>(() => initial.map((toast, index) => ({ ...toast, id: index + 1 })));
  const next = useRef(initial.length + 1);
  const dismiss = useCallback((id: number) => setItems((list) => list.filter((item) => item.id !== id)), []);
  const show = useCallback((toast: ToastInput) => {
    const id = next.current++;
    setItems((list) => [...list.slice(-2), { ...toast, id }]);
    return id;
  }, []);
  const api = useMemo(() => ({ show, dismiss }), [show, dismiss]);
  return (
    <ToastContext.Provider value={api}>
      {children}
      <Portal>
        <ToastRegion items={items} dismiss={dismiss} />
      </Portal>
    </ToastContext.Provider>
  );
}

function ToastRegion({ items, dismiss }: { items: ToastItem[]; dismiss: (id: number) => void }) {
  const t = useT();
  return (
    <section className="ui-toasts" aria-label={t("ui.toast.region")}>
      <ol className="ui-toasts__list" aria-live="polite" aria-relevant="additions">
        {items.map((item) => (
          <ToastView key={item.id} item={item} onClose={() => dismiss(item.id)} />
        ))}
      </ol>
    </section>
  );
}

export function ToastView({ item, onClose }: { item: ToastInput; onClose: () => void }) {
  const t = useT();
  const tone = item.tone ?? "neutral";
  const duration = item.duration ?? (item.action ? 8000 : 5000);
  const [paused, setPaused] = useState(false);
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    if (paused || duration === 0) return;
    const timer = window.setTimeout(() => close.current(), duration);
    return () => window.clearTimeout(timer);
  }, [paused, duration]);
  return (
    <li
      className={`ui-toast ui-toast--${tone}`}
      onPointerEnter={() => setPaused(true)}
      onPointerLeave={() => setPaused(false)}
      onFocus={() => setPaused(true)}
      onBlur={() => setPaused(false)}
    >
      <Icon name={TONE_ICON[tone]} size={18} className="ui-toast__icon" />
      <p className="ui-toast__message">{item.message}</p>
      {item.action && (
        <button
          type="button"
          className="ui-toast__action"
          data-ui={item.action.id}
          onClick={() => {
            item.action?.onAction();
            onClose();
          }}
        >
          {item.action.label}
        </button>
      )}
      <button type="button" className="ui-toast__close" aria-label={t("ui.close")} onClick={onClose}>
        <Icon name="close" size={16} />
      </button>
    </li>
  );
}
