// The host shell's only way out of its page (ADR 0018, 관리): the SECS 2R
// Host shell's Tauri commands (P126). The page never imports the app's IPC
// client; the gallery and the tests hand it the mock in `mockApi.ts`.
import { invoke } from "@tauri-apps/api/core";
import type { ErrorCode } from "../ipc/bindings/ErrorCode";
import type { HostLogLine } from "../ipc/bindings/HostLogLine";
import type { HostStatus } from "../ipc/bindings/HostStatus";
import type { IpcError } from "../ipc/bindings/IpcError";
import type { ServiceState } from "../ipc/bindings/ServiceState";

export type HostShellApi = {
  /** The service's state, and its status over the pipe while it runs and answers (else null). */
  status(): Promise<{ service: ServiceState; host: HostStatus | null }>;
  /** The latest `lines` log lines, oldest first, as the service keeps them. */
  log(lines: number): Promise<HostLogLine[]>;
  /** Registers the service (`secs2r-host.exe install` through Windows's administrator approval). */
  install(): Promise<void>;
  /** Stops and removes the service (administrator approval again). */
  uninstall(): Promise<void>;
  /** `HostRequest::StopManaging`: the service lets go of both targets. */
  stopManaging(): Promise<void>;
  /** Opens SECS 2R (or brings its window forward). */
  openApp(): Promise<void>;
  /** Sets the native tray menu's labels after the page's catalog is ready. */
  trayLabels(labels: { open: string; open_app: string; quit: string }): Promise<void>;
};

/** The shell's commands as P126 registers them; each rejects with what the command returned as its error. */
export function createTauriHostApi(): HostShellApi {
  return {
    status: () => invoke<{ service: ServiceState; host: HostStatus | null }>("host_status"),
    log: (lines) => invoke<HostLogLine[]>("host_log", { lines }),
    install: () => invoke<void>("host_install"),
    uninstall: () => invoke<void>("host_uninstall"),
    stopManaging: () => invoke<void>("host_stop_managing"),
    openApp: () => invoke<void>("open_app"),
    trayLabels: (labels) => invoke<void>("tray_labels", labels),
  };
}

/**
 * A rejected command as an `IpcError`: the service's own error when the
 * command passed one through, else the text it rejected with as an internal
 * error. The page writes its own sentence per action and keeps this only
 * behind 자세히, so the code is not checked against `ErrorCode`.
 */
export function toIpcError(error: unknown): IpcError {
  if (error && typeof error === "object" && "code" in error && typeof error.code === "string") {
    const value = error as { code: string; params?: Record<string, string>; retryable?: boolean; detail?: string };
    return { code: value.code as ErrorCode, params: value.params ?? {}, retryable: value.retryable ?? false, detail: value.detail ?? "" };
  }
  const detail = typeof error === "string" ? error : error instanceof Error ? error.message : JSON.stringify(error) ?? String(error);
  return { code: "internal", params: {}, retryable: false, detail };
}
