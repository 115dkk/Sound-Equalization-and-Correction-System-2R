// The window when the app settings could not be read at start (audit #92
// TD-29). There is no frame without them, so this stands outside it: the
// dialog surface the first-run dialog wears, centred on the window's own
// background, with no backdrop and no close button (nothing lies behind it).
// The title names what did not load, the notice gives the service's reason
// in the user's words (`errorText`, raw text behind 자세히), and 다시 시도
// is the one filled action. While it reads again the panel stays and the
// button is busy, so a retry that fails again does not blink the window.
import { useEffect, useId, useRef } from "react";
import { useT } from "../i18n/react";
import type { IpcError } from "../ipc/bindings/IpcError";
import { Button, ErrorNotice } from "../ui";
import { errorText } from "./jobs/jobText";
import "./app.css";

export function SettingsFailed({ error, busy, onRetry }: { error: IpcError; busy: boolean; onRetry: () => void }) {
  const t = useT();
  const titleId = useId();
  const retry = useRef<HTMLButtonElement>(null);
  // The one action has the focus, and gets it back when a retry fails
  // (a busy button is disabled, which drops the focus).
  useEffect(() => {
    if (!busy) retry.current?.focus({ preventScroll: true });
  }, [busy]);
  return (
    <main className="app-start" data-ui="app.settings_failed">
      <section className="ui-dialog app-start__panel" aria-labelledby={titleId}>
        <header className="ui-dialog__header">
          <h1 id={titleId} className="ui-dialog__title">
            {t("app.settings_failed.title")}
          </h1>
        </header>
        <div className="ui-dialog__body">
          <ErrorNotice id="app.settings_failed.error" cause={errorText(t, error).cause} detail={error.detail} code={error.code} />
        </div>
        <footer className="ui-dialog__footer">
          <Button ref={retry} variant="primary" data-ui="app.settings_failed.retry" busy={busy} onClick={onRetry}>
            {t("app.settings_failed.retry")}
          </Button>
        </footer>
      </section>
    </main>
  );
}
