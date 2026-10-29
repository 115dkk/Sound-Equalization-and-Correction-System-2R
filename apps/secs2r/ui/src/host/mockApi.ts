// A HostShellApi double for the tests and the gallery: the service's state
// changes as the real one would after each command (installing starts it,
// removing it forgets everything, 관리 끄기 lets go of both targets), and a
// command can be made to fail once. Every call is recorded.
import * as fx from "../ipc/fixtures";
import type { HostLogLine } from "../ipc/bindings/HostLogLine";
import type { HostStatus } from "../ipc/bindings/HostStatus";
import type { IpcError } from "../ipc/bindings/IpcError";
import type { ServiceState } from "../ipc/bindings/ServiceState";
import type { HostShellApi } from "./api";

type Command = keyof HostShellApi;

export type MockHostApi = HostShellApi & {
  calls: Command[];
  trayLabelCalls: Parameters<HostShellApi["trayLabels"]>[0][];
  /** The next call of `command` rejects with `error` (as the shell's commands reject). */
  failNext: (command: Command, error: IpcError) => void;
  /** What `status` answers from now on. */
  set: (service: ServiceState, host?: HostStatus | null, log?: HostLogLine[]) => void;
};

export function createMockHostApi({ service = "not_installed", host = null, log = [] }: { service?: ServiceState; host?: HostStatus | null; log?: HostLogLine[] } = {}): MockHostApi {
  let state: { service: ServiceState; host: HostStatus | null; log: HostLogLine[] } = { service, host: service === "running" ? host ?? fx.hostIdle : null, log };
  const failures = new Map<Command, IpcError>();
  const calls: Command[] = [];
  const trayLabelCalls: MockHostApi["trayLabelCalls"] = [];
  const clone = <T,>(value: T): T => structuredClone(value);
  async function call<T>(command: Command, answer: () => T): Promise<T> {
    calls.push(command);
    const failure = failures.get(command);
    if (failure) {
      failures.delete(command);
      throw clone(failure);
    }
    return clone(answer());
  }
  const unreachable: IpcError = { code: "apply_failed", params: {}, retryable: true, detail: "connect secs2r-host pipe: The system cannot find the file specified. (os error 2)" };
  const reached = (): HostStatus => {
    if (state.service !== "running" || !state.host) throw clone(unreachable);
    return state.host;
  };
  return {
    calls,
    trayLabelCalls,
    failNext: (command, error) => failures.set(command, error),
    set: (next, nextHost = null, nextLog = state.log) => {
      state = { service: next, host: next === "running" ? nextHost : null, log: nextLog };
    },
    status: () => call("status", () => ({ service: state.service, host: state.host })),
    log: (lines) => call("log", () => {
      reached();
      return state.log.slice(-lines);
    }),
    install: () => call("install", () => {
      state = { ...state, service: "running", host: state.host ?? fx.hostIdle };
    }),
    uninstall: () => call("uninstall", () => {
      state = { service: "not_installed", host: null, log: [] };
    }),
    stopManaging: () => call("stopManaging", () => {
      state = { ...state, host: { ...reached(), managed: {}, camilla: fx.hostIdle.camilla, eqapo_include: "absent" } };
    }),
    openApp: () => call("openApp", () => undefined),
    trayLabels: (labels) => {
      trayLabelCalls.push(clone(labels));
      return call("trayLabels", () => undefined);
    },
  };
}
