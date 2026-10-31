// The settings popover's app-level choices besides the display settings,
// the local API and updates: 낡은 결과 자동 갱신 (ADR 0029) and the log
// folder (ADR 0027), as the popover sees them. The view reads the app
// settings and the bootstrap info the session already holds; the actions
// are calls the app already makes (the app settings writer, `open_folder`).
// Without a session (gallery, tests) the switch shows its default and the
// actions do nothing; specimens pass a fixed view instead.
import { useMemo, useRef, useState, useSyncExternalStore } from "react";
import { useOptionalSession } from "../state/react";

export type AppOptionsView = {
  /** `AppSettings.auto_refresh` as saved; null before the app settings are read. */
  autoRefresh: boolean | null;
  /** A change of `auto_refresh` is being saved; the switch drops presses until it answers. */
  saving: boolean;
  /** `BootstrapInfo.log_dir`; null when the app keeps no log (the CLI, tests) or before the bootstrap answers. */
  logDir: string | null;
};

export type AppOptionsActions = {
  /** `app_settings_set` with `auto_refresh` changed, through the app settings writer. */
  setAutoRefresh: (on: boolean) => void;
  /** `open_folder { path: logDir }`. */
  openLogFolder: () => void;
};

export type AppOptions = { view: AppOptionsView; actions: AppOptionsActions };

const unsubscribed = () => () => {};
const empty = () => null;

export function useAppOptions(): AppOptions {
  const session = useOptionalSession();
  const settings = useSyncExternalStore(session?.stores.appSettings.subscribe ?? unsubscribed, session?.stores.appSettings.getSnapshot ?? empty);
  const bootstrap = useSyncExternalStore(session?.stores.bootstrap.subscribe ?? unsubscribed, session?.stores.bootstrap.getSnapshot ?? empty);
  const [saving, setSaving] = useState(false);
  const busy = useRef(false);
  const logDir = bootstrap?.log_dir ?? null;
  const actions = useMemo<AppOptionsActions>(() => ({
    setAutoRefresh: (on) => {
      if (!session || busy.current) return;
      busy.current = true;
      setSaving(true);
      void session.writers.app.edit({ auto_refresh: on }).finally(() => {
        busy.current = false;
        setSaving(false);
      });
    },
    openLogFolder: () => {
      if (session && logDir) session.run(() => session.call({ method: "open_folder", params: { path: logDir } }));
    },
  }), [session, logDir]);
  // Without a session (gallery specimens of other parts) the switch shows the default, off.
  return { view: { autoRefresh: session ? (settings?.auto_refresh ?? null) : false, saving, logDir }, actions };
}
