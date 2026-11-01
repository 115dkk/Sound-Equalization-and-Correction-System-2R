// 연결·점검: choose how to measure, then check every channel's level
// (upstream steps 1 and 3). The screen draws `ConnectView` and reports what
// the user did through `ConnectActions`; P33 wires both to the service.
import type { ConfigResult } from "../../ipc/bindings/ConfigResult";
import type { IpcError } from "../../ipc/bindings/IpcError";
import type { JobEvent } from "../../ipc/bindings/JobEvent";
import type { MeasureConfigResult } from "../../ipc/bindings/MeasureConfigResult";
import type { MeasureDevice } from "../../ipc/bindings/MeasureDevice";
import type { MeasureMethodsResult } from "../../ipc/bindings/MeasureMethodsResult";
import type { MicCalibrationStatus } from "../../ipc/bindings/MicCalibrationStatus";

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

/**
 * The project's mic calibration file for native sweeps (ADR 0015). Each sweep
 * measured after the file is chosen carries it, and the calculation removes
 * the curve; measurements keep the curve they carry when the file changes.
 */
export type MicCalibrationView = {
  /** mic_calibration_get; null before the first answer. */
  status: MicCalibrationStatus | null;
  /** What runs now: choosing a file (the dialog, then reading it), clearing it, or attaching it to sweeps without a curve. */
  busy: "pick" | "clear" | "attach" | null;
  /** The last failure of those three until the next attempt; null = none. */
  error: IpcError | null;
};

/** The last rew_launch failure's typed reason, with launch_failed for unclassified errors. */
export type RewProblem = "exe_not_found" | "cannot_launch" | "timed_out" | "launch_failed";

export type ConnectView = {
  /** The stored measurement method; `none` is shown as null and `mdat` skips this step. */
  path: ConnectPath | null;
  /** measure_methods, refreshed by MeasureStatus events while status_watch runs; null before the first answer. */
  methods: MeasureMethodsResult | null;
  /** measure_configure (the active measurement settings); null before the first answer. */
  settings: MeasureConfigResult | null;
  /** config_get: the channels to check, in role order (upstream ordered_speakers). */
  config: ConfigResult;
  /** Sample rate in Hz from MeasureConfigResult.sample_rate, shown read-only. */
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
  micCalibration: MicCalibrationView;
  /** Stored level per role in dBFS from levels_get, pre-filling the rows. */
  levels: Readonly<Record<string, number>>;
  level: {
    monitor: MonitorState;
    /** Why the level monitor job failed (with `offline`); null again once the monitor starts or is switched off. */
    error: IpcError | null;
    /** The latest Meter event; null before the first. */
    reading: MeterReading | null;
    /** The role level_play is playing (2.5 s), or null. */
    playing: string | null;
    /** Playing every channel in turn. */
    playingAll: boolean;
    /** Master output for level_play, −40…0 dBFS (host state; −20 by default). */
    masterDbfs: number;
  };
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
  /** measure_configure { share_mode } (native only, in the device details). */
  setShareMode: (mode: "auto" | "exclusive" | "shared") => void;
  /** dialog_open_file { kind: "mic_calibration" }, then mic_calibration_set { path }; a cancelled dialog changes nothing. */
  pickMicCalibration: () => void;
  /** mic_calibration_clear: the project no longer has a file; measurements keep the curves they carry. */
  clearMicCalibration: () => void;
  /** mic_calibration_attach: the project's file goes to the sweeps measured without a mic curve. */
  attachMicCalibration: () => void;
  /** level_play { role, master_dbfs } (level_monitor_start first when the monitor is off or failed); subs play 50 Hz, 2.5 s. */
  playLevel: (role: string) => void;
  /** Stops the playing channel and the queue (service gap: no early stop for one burst; level_monitor_stop and start again). */
  stopLevel: () => void;
  /** level_play for every channel in turn, in row order (host queue). */
  playAllLevels: () => void;
  /** Host state: the master output for level_play. */
  setMasterLevel: (dbfs: number) => void;
  /** Fills the channels without a level with upstream's defaults (service gap: no method yet). */
  skipLevels: () => void;
  /** Host: opens 측정·가져오기 (the header's primary action on the mdat path). */
  next: () => void;
};
