// 연결·점검: choose how to measure, then check every channel's level
// (upstream steps 1 and 3). The screen draws `ConnectView` and reports what
// the user did through `ConnectActions`; P33 wires both to the service.
import type { ConfigResult } from "../../ipc/bindings/ConfigResult";
import type { JobEvent } from "../../ipc/bindings/JobEvent";
import type { LevelResult } from "../../ipc/bindings/LevelResult";
import type { MeasureConfigResult } from "../../ipc/bindings/MeasureConfigResult";
import type { MeasureDevice } from "../../ipc/bindings/MeasureDevice";
import type { MeasureMethodsResult } from "../../ipc/bindings/MeasureMethodsResult";

/** The measurement path: the service's three methods, or mdat import only (skips this step). */
export type ConnectPath = "native" | "rew" | "mock" | "mdat";

/** One `Meter` event while level_monitor runs. */
export type MeterReading = Omit<Extract<JobEvent, { event: "meter" }>, "event" | "job">;

/**
 * The level monitor: off (not started), starting, receiving (Meter events
 * arrive), stalled (no new reading for a while; restarting, upstream's
 * stale state) or offline (failed or no answer).
 */
export type MonitorState = "off" | "starting" | "receiving" | "stalled" | "offline";

/** What upstream shows about REW beyond RewStatusResult. Service gaps: undefined = not reported yet. */
export type RewDetails = {
  /** REW's executable path (`find_rew_exe`); null when it was not found. */
  executable?: string | null | undefined;
  /** REW's version (`GET /version`). */
  version?: string | null | undefined;
  /** REW's audio driver (`GET /audio/driver`: ASIO, Java or another name); null = unreadable. */
  driver?: string | null | undefined;
  /** Whether REW's API blocking mode is on. */
  blocking?: boolean | null | undefined;
  /** Mic calibration file REW has loaded (`GET /audio/input-cal`): "" = none loaded, null = unknown. */
  micCalibration?: string | null | undefined;
  /** REW's input and output devices, with channel counts (0 = REW has not opened this Java device, so the count is unknown). */
  inputs?: readonly MeasureDevice[] | undefined;
  outputs?: readonly MeasureDevice[] | undefined;
  /** REW's input channel names, index 0 = channel 1. */
  inputChannelNames?: readonly string[] | undefined;
  /** The devices and input REW has open now, read from its own audio preferences (R7); null = unreadable. */
  current?: { inputDevice: string | null; outputDevice: string | null; input: string | null } | undefined;
  /** REW's measurement settings, read on request (rate, sweep length, level). */
  settings?: { sample_rate: number; sweep_length: string; level_dbfs: number | null } | null | undefined;
};

/** Why the last REW launch did not end with REW answering (service gap: rew_launch does not tell these apart yet). */
export type RewProblem = "exe_not_found" | "cannot_launch" | "timed_out" | "launch_failed";

export type ConnectView = {
  /** The chosen path; null until the user chooses. "mdat" skips the step (service gap: the choice is not stored yet). */
  path: ConnectPath | null;
  /** measure_methods, refreshed by MeasureStatus events while status_watch runs; null before the first answer. */
  methods: MeasureMethodsResult | null;
  /** measure_configure / level_apply (the active measurement settings); null before the first answer. */
  settings: MeasureConfigResult | null;
  /** config_get: the channels to check, in role order (upstream ordered_speakers). */
  config: ConfigResult;
  /** Sample rate in Hz, shown read-only (`extra['devices']['sample_rate']`, 48000 by default; service gap). */
  sampleRate: number;
  rew: {
    /** checking: rew_launch is asking REW; launching: REW was started and is not answering yet. */
    launch: "idle" | "checking" | "launching";
    problem: RewProblem | null;
    /** The raw text of a launch failure (자세히). */
    problemDetail: string | null;
    /** Reading REW's settings into the hint line. */
    readingSettings: boolean;
    /** rew_driver_set is waiting for REW to switch its driver and reopen its devices. */
    switchingDriver: boolean;
    details: RewDetails;
  };
  /** Stored level per role in dBFS (`extra['levels']`), pre-filling the rows (service gap: no method returns them). */
  levels: Readonly<Record<string, number>>;
  level: {
    monitor: MonitorState;
    /** The latest Meter event; null before the first. */
    reading: MeterReading | null;
    /** The role level_play is playing (2.5 s), or null. */
    playing: string | null;
    /** Playing every channel in turn. */
    playingAll: boolean;
    /** Master output for level_play, −40…0 dBFS (host state; −20 by default). */
    masterDbfs: number;
  };
  /** level_check job and its result (U1: the recommended sweep level, applied with level_apply). */
  sweepCheck: { running: boolean; result: LevelResult | null; role: string | null };
};

export type ConnectActions = {
  /** measure_configure { method } for native, rew and mock; "mdat" is a host choice that skips the step. */
  choosePath: (path: ConnectPath) => void;
  /** measure_methods (read the devices again after an enumeration error). */
  refreshMethods: () => void;
  /** rew_launch: checks REW, and starts it in API mode when it does not answer. */
  launchRew: () => void;
  /** Reads REW's sample rate, sweep length and level for the hint line (service gap: no method yet). */
  readRewSettings: () => void;
  /** rew_driver_set { driver }: switches REW's audio driver, then reads REW's devices again. */
  setRewDriver: (driver: string) => void;
  /** measure_configure { input_device } ("" = the default device); on the REW path a REW device name, which REW also opens. */
  setInputDevice: (id: string) => void;
  /** measure_configure { input_channel } (1-based). */
  setInputChannel: (channel: number) => void;
  /** measure_configure { output_device } ("" = the default device; on the REW path a REW device name, which REW also opens); also the measurement output device 시스템 shows. */
  setOutputDevice: (id: string) => void;
  /** measure_configure { sweep_length } in samples (65536 … 4194304). */
  setSweepLength: (samples: number) => void;
  /** measure_configure { share_mode } (native only, in the device details). */
  setShareMode: (mode: "auto" | "exclusive" | "shared") => void;
  /** level_play { role, master_dbfs } (level_monitor_start first when the monitor is off); subs play 50 Hz, 2.5 s. */
  playLevel: (role: string) => void;
  /** Stops the playing channel and the queue (service gap: no early stop for one burst; level_monitor_stop and start again). */
  stopLevel: () => void;
  /** level_play for every channel in turn, in row order (host queue). */
  playAllLevels: () => void;
  /** Host state: the master output for level_play. */
  setMasterLevel: (dbfs: number) => void;
  /** Fills the channels without a level with upstream's defaults (service gap: no method yet). */
  skipLevels: () => void;
  /** level_check { role } job (U1). */
  checkSweepLevel: (role: string) => void;
  /** level_apply { level } (U1). */
  applySweepLevel: (level: LevelResult) => void;
  /** Host: opens 측정·가져오기 (the header's primary action on the mdat path). */
  next: () => void;
};
