// Modal dialog. The title names the action, the body is one factual line
// about the consequence, and the buttons repeat the action ("측정 지우기" /
// "취소"), never a bare yes/no. Escape and the close button always leave.
// The dialog never grows past the window: the head and the buttons stay,
// and only the body scrolls (lines mark its edges while it does).
//
// Actions that clear measurements use confirm-and-undo (U5): the dialog
// confirms, the action runs, and a toast offers 되돌리기 with the undo token
// the service returned.
import { useCallback, useEffect, useId, useLayoutEffect, useRef, useState, useSyncExternalStore, type KeyboardEvent, type ReactNode } from "react";
import { useT } from "../i18n/react";
import { Button } from "./Button";
import { Icon } from "./Icon";
import { HelpTip } from "./HelpTip";
import { Portal } from "./layer";
import { useToast } from "./Toast";

const FOCUSABLE = 'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/* ── Open confirm dialogs ───────────────────────────────────────────────── */
// A confirm asks for a decision in the middle of a task (지우기, 관리 끄기).
// Something that opens on its own (the update notice) waits while one is
// open instead of covering it. Inline confirms (gallery) do not count.

const openConfirms = new Set<symbol>();
const confirmListeners = new Set<() => void>();
const announceConfirms = () => {
  for (const listener of confirmListeners) listener();
};
const subscribeConfirms = (listener: () => void) => {
  confirmListeners.add(listener);
  return () => {
    confirmListeners.delete(listener);
  };
};

/** Whether a confirm dialog is open anywhere in the window. */
export function useConfirmOpen(): boolean {
  return useSyncExternalStore(subscribeConfirms, () => openConfirms.size > 0, () => false);
}

function useRegisterConfirm(open: boolean) {
  useEffect(() => {
    if (!open) return;
    const token = Symbol("confirm");
    openConfirms.add(token);
    announceConfirms();
    return () => {
      openConfirms.delete(token);
      announceConfirms();
    };
  }, [open]);
}

export function Dialog({
  open,
  onClose,
  title,
  help,
  children,
  footer,
  id,
  tone,
  inline,
  above,
}: {
  open: boolean;
  onClose: () => void;
  title: ReactNode;
  help?: ReactNode;
  children?: ReactNode;
  footer?: ReactNode;
  id?: string | undefined;
  tone?: "danger" | undefined;
  /** Render in place without a backdrop (gallery specimens). */
  inline?: boolean | undefined;
  /**
   * Stack over the other dialogs whatever order they opened in (the update
   * notice, which opens on its own at start beside 새 프로젝트).
   */
  above?: boolean | undefined;
}) {
  const t = useT();
  const titleId = useId();
  const bodyId = useId();
  const panel = useRef<HTMLDivElement>(null);
  const body = useRef<HTMLDivElement>(null);
  const returnTo = useRef<Element | null>(null);
  const [scrolls, setScrolls] = useState(false);

  useEffect(() => {
    if (!open || inline) return;
    returnTo.current = document.activeElement;
    const node = panel.current;
    const first = node?.querySelector<HTMLElement>("[data-autofocus]") ?? [...(node?.querySelectorAll<HTMLElement>(FOCUSABLE) ?? [])].find((element) => !element.hasAttribute("data-help-trigger"));
    first?.focus();
    return () => {
      if (returnTo.current instanceof HTMLElement) returnTo.current.focus();
    };
  }, [open, inline]);

  // Whether the body overflows its box (a long list, an opened 자세히, a short window).
  useLayoutEffect(() => {
    const node = body.current;
    if (!open || !node || typeof ResizeObserver === "undefined") return;
    const check = () => setScrolls(node.scrollHeight > node.clientHeight + 1);
    const observer = new ResizeObserver(check);
    observer.observe(node);
    for (const child of node.children) observer.observe(child);
    check();
    return () => observer.disconnect();
  }, [open, children]);

  if (!open) return null;

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.stopPropagation();
      onClose();
      return;
    }
    if (event.key !== "Tab" || !panel.current) return;
    const items = [...panel.current.querySelectorAll<HTMLElement>(FOCUSABLE)];
    const first = items[0];
    const last = items[items.length - 1];
    if (!first || !last) return;
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  };

  const box = (
    <div
      ref={panel}
      id={id}
      data-ui={id}
      role="dialog"
      aria-modal={inline ? undefined : true}
      aria-labelledby={titleId}
      aria-describedby={children ? bodyId : undefined}
      className={["ui-dialog", tone === "danger" && "ui-dialog--danger", scrolls && "ui-dialog--scrolls"].filter(Boolean).join(" ")}
      onKeyDown={onKeyDown}
    >
      <header className="ui-dialog__header">
        <div className="ui-heading-help">
          <h2 id={titleId} className="ui-dialog__title">{title}</h2>
          {help && <HelpTip id={`${id ?? titleId}.help`} labelledBy={titleId} content={help} />}
        </div>
        <button type="button" className="ui-dialog__close" aria-label={t("ui.close")} onClick={onClose}>
          <Icon name="close" size={18} />
        </button>
      </header>
      {children && (
        <div ref={body} id={bodyId} className="ui-dialog__body">
          {children}
        </div>
      )}
      {footer && <footer className="ui-dialog__footer">{footer}</footer>}
    </div>
  );

  if (inline) return <div className="ui-dialog-inline">{box}</div>;
  return (
    <Portal>
      <div className={["ui-dialog-backdrop", above && "ui-dialog-backdrop--above"].filter(Boolean).join(" ")} onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
        {box}
      </div>
    </Portal>
  );
}

export function ConfirmDialog({
  open,
  onClose,
  onConfirm,
  title,
  body,
  confirmLabel,
  id,
  inline,
}: {
  open: boolean;
  onClose: () => void;
  onConfirm: () => void;
  title: ReactNode;
  body: ReactNode;
  /** Repeats the action named in the title. */
  confirmLabel: string;
  id?: string | undefined;
  inline?: boolean | undefined;
}) {
  const t = useT();
  useRegisterConfirm(open && !inline);
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={title}
      id={id}
      tone="danger"
      inline={inline}
      footer={
        <>
          <Button variant="secondary" onClick={onClose} data-autofocus data-ui={id ? `${id}.cancel` : undefined}>
            {t("ui.dialog.cancel")}
          </Button>
          <Button
            variant="danger"
            data-ui={id ? `${id}.confirm` : undefined}
            onClick={() => {
              onConfirm();
              onClose();
            }}
          >
            {confirmLabel}
          </Button>
        </>
      }
    >
      <p>{body}</p>
    </Dialog>
  );
}

/**
 * Confirm, run, then offer undo (U5). `run` returns the undo token from the
 * service (`position_clear` → `UndoResult.token`); `undo` hands it back.
 */
export function useConfirmAndUndo({
  id,
  title,
  body,
  confirmLabel,
  done,
  run,
  undo,
  onError,
}: {
  id: string;
  title: ReactNode;
  body: ReactNode;
  confirmLabel: string;
  /** Result line for the toast ("위치 3 측정 지움"). */
  done: ReactNode;
  run: () => Promise<string>;
  undo: (token: string) => Promise<void>;
  onError?: (error: unknown) => void;
}) {
  const t = useT();
  const toast = useToast();
  const [open, setOpen] = useState(false);
  const request = useCallback(() => setOpen(true), []);
  const dialog = (
    <ConfirmDialog
      id={id}
      open={open}
      onClose={() => setOpen(false)}
      title={title}
      body={body}
      confirmLabel={confirmLabel}
      onConfirm={() => {
        run()
          .then((token) =>
            toast.show({
              message: done,
              tone: "neutral",
              action: { label: t("ui.toast.undo"), id: `${id}.undo`, onAction: () => void undo(token).catch(onError) },
            }),
          )
          .catch(onError);
      }}
    />
  );
  return { request, dialog };
}
