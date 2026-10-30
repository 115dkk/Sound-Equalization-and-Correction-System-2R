// The host shell's state: the service read every 2 s while the window is
// visible (not at all while it is hidden, at once when it shows again), its
// log while it runs, and the four actions, each followed by a fresh read.
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useT } from "../i18n/react";
import type { HostLogLine } from "../ipc/bindings/HostLogLine";
import type { IpcError } from "../ipc/bindings/IpcError";
import type { ServiceState } from "../ipc/bindings/ServiceState";
import { toIpcError, type HostShellApi } from "./api";
import type { HostState } from "./model";

/** How many log lines the page asks for and shows. */
export const LOG_LINES = 50;
/** How often the page reads the service while the window is visible. */
export const POLL_MS = 2000;

export type HostShellAction = "install" | "uninstall" | "stop" | "open";

export type HostShellView = {
  /** The service's state; null until the first answer. */
  service: ServiceState | null;
  /** Its status over the pipe while it runs and answers. */
  host: HostState | null;
  /** The latest log lines, oldest first (the page draws them newest first). */
  log: HostLogLine[];
  /** The last read failed; the page keeps reading. */
  readError: IpcError | null;
  /** The action in flight. */
  busy: HostShellAction | null;
  /** The last action that failed, until the next one starts. */
  failure: { action: HostShellAction; error: IpcError } | null;
  /** When the view was drawn: times name their date only when it is not today. */
  now: number;
};

export type HostShellActions = {
  install: () => void;
  uninstall: () => void;
  stopManaging: () => void;
  openApp: () => void;
};

type State = Omit<HostShellView, "now">;

const INITIAL: State = { service: null, host: null, log: [], readError: null, busy: null, failure: null };

export function useHostShell(api: HostShellApi, { intervalMs = POLL_MS }: { intervalMs?: number } = {}): { view: HostShellView; actions: HostShellActions } {
  const [state, setState] = useState<State>(INITIAL);
  const t = useT();
  const trayReady = useRef<HostShellApi | null>(null);
  useEffect(() => {
    // Startup only, even while hidden or when StrictMode repeats setup.
    if (trayReady.current === api) return;
    trayReady.current = api;
    void api.trayLabels({ open: t("host.tray.open"), open_app: t("host.tray.open_app"), quit: t("host.tray.quit") }).catch(() => {
      // Keep the native menu's English defaults if labels could not be set; service polling still runs.
    });
  }, [api, t]);
  const read = useCallback(async () => {
    try {
      const { service, host } = await api.status();
      // The log comes over the same pipe: only a running service has one, and a failed read keeps the lines already shown.
      const log = service === "running" && host ? await api.log(LOG_LINES).catch(() => null) : [];
      setState((latest) => ({ ...latest, service, host, log: log ?? latest.log, readError: null }));
    } catch (error) {
      setState((latest) => ({ ...latest, readError: toIpcError(error) }));
    }
  }, [api]);

  useEffect(() => {
    // One chain of reads at a time: each waits for the previous answer, and showing the window again starts a new chain.
    let generation = 0;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const run = (chain: number) => {
      void read().then(() => {
        if (chain !== generation || document.visibilityState === "hidden") return;
        timer = setTimeout(() => run(chain), intervalMs);
      });
    };
    const restart = () => {
      generation++;
      clearTimeout(timer);
      if (document.visibilityState !== "hidden") run(generation);
    };
    restart();
    document.addEventListener("visibilitychange", restart);
    return () => {
      generation++;
      clearTimeout(timer);
      document.removeEventListener("visibilitychange", restart);
    };
  }, [read, intervalMs]);

  const actions = useMemo<HostShellActions>(() => {
    // The button stays busy until the service has been read again, so it never shows the state from before the action.
    const perform = (action: HostShellAction, call: () => Promise<void>, reread: boolean) => {
      setState((latest) => ({ ...latest, busy: action, failure: null }));
      void (async () => {
        try {
          await call();
        } catch (error) {
          setState((latest) => ({ ...latest, failure: { action, error: toIpcError(error) } }));
        }
        if (reread) await read();
        setState((latest) => ({ ...latest, busy: null }));
      })();
    };
    return {
      install: () => perform("install", () => api.install(), true),
      uninstall: () => perform("uninstall", () => api.uninstall(), true),
      stopManaging: () => perform("stop", () => api.stopManaging(), true),
      openApp: () => perform("open", () => api.openApp(), false),
    };
  }, [api, read]);

  return { view: { ...state, now: Date.now() }, actions };
}
