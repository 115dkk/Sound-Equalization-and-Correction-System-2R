// The SECS 2R Host service (ADR 0018) in the states the export screen's and
// the host shell's specimens and tests draw. Times are fixed around the
// gallery's clock (14 October 2026, 05:12 UTC) so screenshots repeat.
import * as fx from "../ipc/fixtures";
import type { HostLogLine } from "../ipc/bindings/HostLogLine";
import type { HostState } from "./model";

/** The specimens' "now": the gallery's T0 (frameSpecimens). */
export const HOST_NOW = Date.UTC(2026, 9, 14, 5, 12, 0);

const at = (seconds: number) => new Date(HOST_NOW + seconds * 1000).toISOString();

/** CamillaDSP stopped and the service waits to start it again: the second restart within five minutes, four seconds from now. */
export const hostRestarting: HostState = {
  ...fx.hostManaging,
  camilla: { state: { kind: "backing_off", until: at(4) }, pid: null, restarts_in_window: 2 },
};

/** CamillaDSP stopped five times within five minutes: the service no longer starts it (`secs2r-host` gives this reason). The line was repaired two minutes ago, as `LOG_GIVEN_UP` says. */
export const hostGivenUp: HostState = {
  ...fx.hostManaging,
  camilla: { state: { kind: "given_up", reason: "five exits within five minutes" }, pid: null, restarts_in_window: 4 },
  last_repair: at(-120),
};

/** Another user is signed in at the console, as the service reports it. */
export const hostOtherUser: HostState = {
  ...fx.hostManaging,
  camilla: { state: { kind: "other_user" }, pid: null, restarts_in_window: 0 },
};

/** Only Equalizer APO is managed, and the service put SECS 2R's line back two minutes ago. */
export const hostEqapoRepaired: HostState = {
  ...fx.hostIdle,
  managed: { eqapo: fx.managedEqapo },
  eqapo_include: "present",
  last_repair: at(-120),
};

/** Both targets managed, the line repaired two minutes ago. */
export const hostManagingRepaired: HostState = { ...fx.hostManaging, last_repair: at(-120) };

const line = (seconds: number, event: string, fields: Record<string, string> = {}): HostLogLine => ({ timestamp: at(seconds), event, fields });

/** A quiet morning, oldest first as the service keeps it: sign-in, CamillaDSP started, one repair. */
export const LOG_MANAGING: HostLogLine[] = [
  line(-840, "session_logon", { session: "1" }),
  line(-835, "launch", { pid: "4812", design_id: fx.MOCK_DESIGN_ID }),
  line(-120, "repair", { path: `${fx.managedEqapo.config_path}/config.txt`, design_id: fx.MOCK_DESIGN_ID, timestamp: at(-120) }),
];

/** CamillaDSP stopping five times within a minute, restarted after 1, 2, 4 and 8 s, before the service stops trying. */
export const LOG_GIVEN_UP: HostLogLine[] = [
  ...LOG_MANAGING,
  line(-62, "exit", { pid: "4812", code: "1" }),
  line(-62, "restart", { after_ms: "1000" }),
  line(-61, "launch", { pid: "4920", design_id: fx.MOCK_DESIGN_ID }),
  line(-59, "exit", { pid: "4920", code: "1" }),
  line(-59, "restart", { after_ms: "2000" }),
  line(-57, "launch", { pid: "5016", design_id: fx.MOCK_DESIGN_ID }),
  line(-55, "exit", { pid: "5016", code: "1" }),
  line(-55, "restart", { after_ms: "4000" }),
  line(-51, "launch", { pid: "5104", design_id: fx.MOCK_DESIGN_ID }),
  line(-49, "exit", { pid: "5104", code: "1" }),
  line(-49, "restart", { after_ms: "8000" }),
  line(-41, "launch", { pid: "5188", design_id: fx.MOCK_DESIGN_ID }),
  line(-39, "exit", { pid: "5188", code: "1" }),
  line(-39, "given_up", { reason: "five exits within five minutes" }),
];
