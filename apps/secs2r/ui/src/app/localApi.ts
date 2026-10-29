// The local API (ADR 0016) as the settings popover and the 로컬 API dialog
// see it: the view they draw, the actions they call, and the pure checks
// both share (the state line, the port draft, the request reference).
import { useEffect, useMemo, useRef, useState } from "react";
import { IpcFailure } from "../ipc/client";
import { useOptionalSession } from "../state/react";
import { useT } from "../i18n/react";
import { useToast } from "../ui/Toast";
import type { LocalApiSettings } from "../ipc/bindings/LocalApiSettings";
import type { IpcError } from "../ipc/bindings/IpcError";
import type { LocalApiStatus } from "../ipc/bindings/LocalApiStatus";

/** Ports the dialog accepts: above the system range, up to the last TCP port. */
export const PORT_MIN = 1024;
export const PORT_MAX = 65535;
/** `secs_types::local_api::DEFAULT_PORT`. */
export const DEFAULT_PORT = 4712;

/** The call in flight: the switch, a port change or a new token. */
export type LocalApiPending = "toggle" | "port" | "token";

export type LocalApiView = {
  /** The last status read; null before the first read answers or when it failed. */
  status: LocalApiStatus | null;
  pending: LocalApiPending | null;
  /** The last call's failure, shown with its cause, next action and 자세히. */
  error: IpcError | null;
};

export type LocalApiActions = {
  /** Writes `local_api.enabled`; the shell starts or stops the server after the save. */
  setEnabled: (on: boolean) => void;
  /** Writes `local_api.port` (a value `portProblem` accepts); a running server restarts on it. */
  setPort: (port: number) => void;
  /** Replaces the token file; a running server restarts with the new token. */
  regenerateToken: () => void;
  /** Puts the text on the clipboard and says so in a toast. */
  copy: (text: string) => void;
};

export type LocalApi = { view: LocalApiView; actions: LocalApiActions };

/* ── State line ─────────────────────────────────────────────────────────── */

/** What the state line says, from the status and the call in flight. */
export type LocalApiState =
  | { kind: "unknown" }
  | { kind: "unreadable" }
  | { kind: "off" }
  | { kind: "turning_on" }
  | { kind: "turning_off" }
  | { kind: "listening"; address: string }
  | { kind: "port_busy"; port: number }
  | { kind: "failed"; detail: string };

/**
 * The listener's text for a taken port (`secs_local_api::LocalApiError::PortBusy`,
 * "local API port 4712 is busy"). The status carries the failure only as
 * text, so this pattern is the one place that reads it.
 */
const PORT_BUSY = /\bport (\d+) is busy\b/;

export function localApiState(view: LocalApiView): LocalApiState {
  const status = view.status;
  if (!status) return view.error && view.pending === null ? { kind: "unreadable" } : { kind: "unknown" };
  if (view.pending === "toggle") return { kind: status.enabled ? "turning_off" : "turning_on" };
  if (!status.enabled) return { kind: "off" };
  if (status.running) return { kind: "listening", address: hostOf(status.url) };
  const busy = status.error ? PORT_BUSY.exec(status.error) : null;
  if (busy) return { kind: "port_busy", port: Number(busy[1]) };
  return { kind: "failed", detail: status.error ?? "" };
}

/** "http://127.0.0.1:4712" → "127.0.0.1:4712". */
export function hostOf(url: string): string {
  return url.replace(/^[a-z][a-z0-9+.-]*:\/\//i, "").replace(/\/+$/, "");
}

/* ── Port draft ─────────────────────────────────────────────────────────── */

/** Why a typed port cannot be applied. */
export type PortProblem = "empty" | "not_number" | "out_of_range";

/** Checks a typed port: whole digits only, PORT_MIN to PORT_MAX. */
export function portProblem(text: string): PortProblem | null {
  const cleaned = text.trim();
  if (cleaned === "") return "empty";
  if (!/^\d+$/.test(cleaned)) return "not_number";
  const port = Number(cleaned);
  return port < PORT_MIN || port > PORT_MAX ? "out_of_range" : null;
}

/** The port a typed draft names, or null while `portProblem` objects. */
export function parsePort(text: string): number | null {
  return portProblem(text) === null ? Number(text.trim()) : null;
}

/* ── Request reference ──────────────────────────────────────────────────── */

/** The three entry points (ADR 0016) and the catalog line that says what each does. */
export const ENDPOINTS = [
  { method: "POST", path: "/v1/call", what: "frame.local_api.reference.call" },
  { method: "GET", path: "/v1/status", what: "frame.local_api.reference.status" },
  { method: "GET", path: "/v1/events", what: "frame.local_api.reference.events" },
] as const;

/**
 * The reference block as plain text, the same on screen and on the
 * clipboard: the base address, one aligned line per entry point, and the
 * header every request sends (with a placeholder, never the token).
 */
export function referenceText(url: string, lines: { what: (key: (typeof ENDPOINTS)[number]["what"]) => string; auth: string }): string {
  const heads = ENDPOINTS.map((entry) => `${entry.method.padEnd(4)} ${entry.path}`);
  const width = Math.max(...heads.map((head) => head.length)) + 3;
  return [url, ...ENDPOINTS.map((entry, index) => `${heads[index]!.padEnd(width)}${lines.what(entry.what)}`), lines.auth].join("\n");
}

/* ── Hook ──────────────────────────────────────────────────────────────── */

const STUB_STATUS: LocalApiStatus = {
  enabled: false,
  port: DEFAULT_PORT,
  running: false,
  error: null,
  url: `http://127.0.0.1:${DEFAULT_PORT}`,
  token: "",
  token_path: "",
};

const NO_ACTIONS: LocalApiActions = { setEnabled: () => {}, setPort: () => {}, regenerateToken: () => {}, copy: () => {} };

/**
 * The local API's view and actions for the settings popover and the dialog.
 * `open` is true while either one shows; the status is read when it turns
 * true and after each change. The settings button calls this hook in the
 * gallery and in tests too, where there is no session, so it has to keep
 * working there (a fixed off status is enough).
 */
export function useLocalApi(open: boolean): LocalApi {
  const session = useOptionalSession();
  const toast = useToast();
  const t = useT();
  const [view, setView] = useState<LocalApiView>({ status: session ? null : STUB_STATUS, pending: null, error: null });
  const busy = useRef(false);
  const generation = useRef(0);
  const alive = useRef(false);
  useEffect(() => {
    alive.current = true;
    return () => { alive.current = false; generation.current++; };
  }, [session]);

  useEffect(() => {
    if (!session || !open) return;
    const refresh = async () => {
      if (busy.current) return;
      const ticket = ++generation.current;
      setView((prior) => ({ ...prior, error: null }));
      try {
        const status = await session.call({ method: "local_api_status" });
        if (alive.current && ticket === generation.current) setView({ status, pending: null, error: null });
      } catch (error) {
        if (alive.current && ticket === generation.current) setView({ status: null, pending: null, error: typedError(error) });
      }
    };
    void refresh();
    const off = session.stores.appSettings.subscribe(() => { void refresh(); });
    return off;
  }, [open, session]);

  const actions = useMemo<LocalApiActions>(() => {
    if (!session) return NO_ACTIONS;
    const perform = (pending: LocalApiPending | null, work: () => Promise<LocalApiStatus | void>) => {
      if (busy.current) return;
      busy.current = true;
      const ticket = ++generation.current;
      setView((prior) => ({ ...prior, pending, error: null }));
      void work().then((status) => {
        if (alive.current && ticket === generation.current) setView((prior) => ({ status: status ?? prior.status, pending: null, error: null }));
      }, (error: unknown) => {
        if (alive.current && ticket === generation.current) setView((prior) => ({ ...prior, pending: null, error: typedError(error) }));
      }).finally(() => {
        busy.current = false;
        if (alive.current) setView((prior) => ({ ...prior, pending: null }));
      });
    };
    const save = async (patch: Partial<LocalApiSettings>) => {
      if (!session.stores.appSettings.getSnapshot()) {
        session.stores.appSettings.set(await session.call({ method: "app_settings_get" }));
      }
      const accepted = await session.writers.app.edit((latest) => ({ local_api: { ...latest.local_api, ...patch } }));
      if (!accepted) throw new IpcFailure(session.errors.getSnapshot()?.error ?? typedError("App settings were not saved"));
      return session.call({ method: "local_api_status" });
    };
    return {
      setEnabled: (enabled) => perform("toggle", () => save({ enabled })),
      setPort: (port) => perform("port", () => save({ port })),
      regenerateToken: () => perform("token", () => session.call({ method: "local_api_token_regenerate" })),
      copy: (text) => perform(null, async () => {
        await navigator.clipboard.writeText(text);
        if (alive.current) toast.show({ message: t("frame.local_api.copied"), tone: "neutral" });
      }),
    };
  }, [session, toast, t]);
  return { view, actions };
}

function typedError(error: unknown): IpcError {
  return error instanceof IpcFailure ? error.error : { code: "internal", params: {}, retryable: false, detail: String(error) };
}
