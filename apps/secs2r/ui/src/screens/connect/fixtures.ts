// Typed 연결·점검 views for every state the specimens and tests draw. The
// channels are the sample 7.1.4 room's (src/ipc/fixtures.ts); devices and
// REW facts are plausible for a Windows measurement PC.
import type { IpcError } from "../../ipc/bindings/IpcError";
import type { MeasureConfigResult } from "../../ipc/bindings/MeasureConfigResult";
import type { MeasureMethodsResult } from "../../ipc/bindings/MeasureMethodsResult";
import type { MicCalibrationFile } from "../../ipc/bindings/MicCalibrationFile";
import { SAMPLE_ROLES } from "../../ipc/fixtures";
import { sampleView } from "../system/fixtures";
import type { ConnectView, RewDetails } from "./types";

const MIC = "wasapi:{0.0.1.00000000}.{8a1c}";
const INTERFACE_IN = "wasapi:{0.0.1.00000000}.{51f2}";
const INTERFACE_OUT = "wasapi:{0.0.0.00000000}.{51f2}";

const nativeDevices: MeasureMethodsResult["native_devices"] = [
  { id: MIC, name: "UMIK-1", input_channels: 2, output_channels: 0 },
  { id: INTERFACE_IN, name: "MOTU 16A", input_channels: 16, output_channels: 0 },
  { id: INTERFACE_OUT, name: "MOTU 16A", input_channels: 0, output_channels: 16 },
  { id: "wasapi:{0.0.0.00000000}.{hdmi}", name: "AVR (HDMI)", input_channels: 0, output_channels: 8 },
];

const methodsWith = (rew: MeasureMethodsResult["rew"], native = true): MeasureMethodsResult => ({
  methods: ["mock", ...(native ? ["native"] : []), ...(rew.reachable && rew.pro !== false ? ["rew"] : [])],
  native_devices: native ? nativeDevices : [],
  native_error: null,
  rew,
  mock_positions: [1, 2, 3, 4, 5, 6, 7, 8, 9],
  mock_roles: [...SAMPLE_ROLES],
});

const rewOff = { running: false, reachable: false, pro: null };

const settingsFor = (method: string, patch: Partial<MeasureConfigResult> = {}): MeasureConfigResult => ({
  method,
  repetitions: 1,
  sample_rate: 48000,
  sweep_lengths: [65536, 131072, 262144, 524288, 1048576, 2097152, 4194304],
  sweep_level_dbfs: -12,
  input_device: method === "native" ? MIC : "",
  output_device: method === "native" ? INTERFACE_OUT : "",
  input_channel: 1,
  roles: [...SAMPLE_ROLES],
  output_channels: SAMPLE_ROLES.map((_, i) => i + 1),
  sweep_length: 262144,
  timing: "acoustic",
  loopback_input_channel: null,
  timing_ref_output: 2,
  share_mode: "auto",
  ...patch,
});

const rewFound: RewDetails = { executable: "C:\\Program Files\\REW\\roomeqwizard.exe" };

const rewConnected: RewDetails = {
  executable: "C:\\Program Files\\REW\\roomeqwizard.exe",
  version: "5.40 beta 92",
  driver: "ASIO",
  blocking: true,
  micCalibration: "C:\\Users\\demo\\Documents\\REW\\7103946_90deg.txt",
  inputs: [{ id: "ASIO: MOTU Pro Audio", name: "ASIO: MOTU Pro Audio", input_channels: 16, output_channels: 0 }],
  outputs: [{ id: "ASIO: MOTU Pro Audio", name: "ASIO: MOTU Pro Audio", input_channels: 0, output_channels: 16 }],
  inputChannelNames: ["Mic/Line 1", "Mic/Line 2", "Line 3", "Line 4", "Line 5", "Line 6", "Line 7", "Line 8"],
  current: { inputDevice: "ASIO: MOTU Pro Audio", outputDevice: "ASIO: MOTU Pro Audio", input: "Mic/Line 1" },
};

/** Levels stored for the first six channels of the sample room. */
const halfLevels: Record<string, number> = { L: -22.4, R: -21.8, C: -23.1, SW1: -18.6, SW2: -19.9, Ls: -26.3 };

/** Every channel: one a little high, one a little low, the rest in range. */
const allLevels: Record<string, number> = {
  ...halfLevels,
  Rs: -25.7,
  Lb: -27.9,
  Rb: -28.4,
  Tfl: -31.2,
  Tfr: -29.6,
  Tbl: -24.8,
  Tbr: -13.6,
};

const base: ConnectView = {
  path: null,
  methods: methodsWith(rewOff),
  settings: null,
  config: sampleView.config,
  sampleRate: 48000,
  rew: { launch: "idle", problem: null, problemDetail: null, readingSettings: false, switchingDriver: false, details: {} },
  micCalibration: { status: { file: null, attached: 0, attachable: 0 }, busy: null, error: null },
  levels: {},
  level: { monitor: "off", reading: null, playing: null, playingAll: false, masterDbfs: -20 },
};

/** A UMIK-1 90° file as the service describes it (serial and gains are invented). */
export const umikFile: MicCalibrationFile = {
  name: "7000001_90deg.txt",
  path: "C:\\Users\\demo\\Documents\\UMIK-1\\7000001_90deg.txt",
  serial: "7000001",
  sens_factor_db: -0.7312,
  analog_gain_db: null,
  sensitivity_dbfs: null,
  notes: ["Auto-generated 90-degree calibration file for use with nanoAVR DL"],
  points: 615,
  min_hz: 10.062,
  max_hz: 20011.3,
  has_phase: false,
};

/** Nothing chosen yet: REW is not running, native devices are there. */
export const emptyView: ConnectView = base;

/** REW chosen, not running. */
export const rewOffView: ConnectView = {
  ...base,
  path: "rew",
  settings: settingsFor("rew"),
  rew: { ...base.rew, details: rewFound },
};

/** REW running without the API (started by hand without -api). */
export const rewNoApiView: ConnectView = {
  ...rewOffView,
  methods: methodsWith({ running: true, reachable: false, pro: null }),
};

/** REW answering, licence not known yet (REW has no licence endpoint; the first sweep tells, R6). */
export const rewUntestedView: ConnectView = {
  ...rewOffView,
  methods: methodsWith({ running: true, reachable: true, pro: null }),
  settings: settingsFor("rew", { input_device: "wasapi:{0.0.1.00000000}.{mic}", output_device: "wasapi:{0.0.0.00000000}.{out}" }),
  rew: { ...base.rew, details: { ...rewConnected, settings: { sample_rate: 48000, sweep_length: "256k", level_dbfs: -12 } } },
};

/** REW refused a sweep for want of Pro: devices and mic calibration are read, measuring is manual. */
export const rewNotProView: ConnectView = {
  ...rewOffView,
  methods: methodsWith({ running: true, reachable: true, pro: false }),
  settings: settingsFor("rew", { input_device: "Java: UMIK-1", output_device: "ASIO: MOTU Pro Audio" }),
  rew: { ...base.rew, details: { ...rewConnected, settings: { sample_rate: 48000, sweep_length: "256k", level_dbfs: -12 } } },
};

/**
 * REW on its Java driver: it knows the channel count only of the devices it
 * has open, so the others are listed by name alone; the stored names are REW's.
 */
export const rewJavaView: ConnectView = {
  ...rewOffView,
  methods: methodsWith({ running: true, reachable: true, pro: true }),
  settings: settingsFor("rew", { input_device: "UMIK-1", output_device: "AVR (HDMI)" }),
  rew: {
    ...base.rew,
    details: {
      ...rewConnected,
      driver: "Java",
      inputs: [
        { id: "UMIK-1", name: "UMIK-1", input_channels: 2, output_channels: 0 },
        { id: "Line In (Realtek)", name: "Line In (Realtek)", input_channels: 0, output_channels: 0 },
      ],
      outputs: [
        { id: "AVR (HDMI)", name: "AVR (HDMI)", input_channels: 0, output_channels: 8 },
        { id: "Speakers (Realtek)", name: "Speakers (Realtek)", input_channels: 0, output_channels: 0 },
      ],
      inputChannelNames: ["Left", "Right"],
      current: { inputDevice: "UMIK-1", outputDevice: "AVR (HDMI)", input: "Left" },
    },
  },
};

/** REW switching from Java to ASIO: the fields wait until it has reopened its devices. */
export const rewSwitchingView: ConnectView = { ...rewJavaView, rew: { ...rewJavaView.rew, switchingDriver: true } };

/** Native measurement with a USB mic and a 16-output interface; the monitor reads the quiet room. */
export const nativeView: ConnectView = {
  ...base,
  path: "native",
  settings: settingsFor("native"),
  level: { ...base.level, monitor: "receiving", reading: { rms_dbfs: -61.8, peak_dbfs: -52.4, spl_db: 36.5, clipping: false } },
};

/** Native with the mic's file chosen: every sweep so far carries it. */
export const micCalView: ConnectView = {
  ...nativeView,
  micCalibration: { status: { file: umikFile, attached: 26, attachable: 0 }, busy: null, error: null },
};

/** The file was chosen after the first positions were measured: those sweeps can take it now. */
export const micCalAttachView: ConnectView = {
  ...nativeView,
  micCalibration: { status: { file: umikFile, attached: 0, attachable: 26 }, busy: null, error: null },
};

/** A text file that is not a calibration: the line that broke it is named. */
export const micCalErrorView: ConnectView = {
  ...nativeView,
  micCalibration: {
    status: { file: null, attached: 0, attachable: 0 },
    busy: null,
    error: {
      code: "measurement_input",
      params: { reason: "mic_calibration", issue: "row", line: "3", file: "readme.txt" },
      retryable: false,
      detail: "calibration Row at line Some(3)",
    },
  },
};

const enumerationError: IpcError = {
  code: "device_unavailable",
  params: {},
  retryable: true,
  detail: "WASAPI IMMDeviceEnumerator::EnumAudioEndpoints failed: 0x80070005 (E_ACCESSDENIED)",
};

/** Native chosen, but the device list could not be read (REW and the sample room stay available). */
export const nativeErrorView: ConnectView = {
  ...base,
  path: "native",
  methods: { ...methodsWith(rewOff, false), native_error: enumerationError },
  settings: settingsFor("native", { input_device: "", output_device: "" }),
};

/** Half the channels checked; Rs plays now and the meter follows it. */
export const levelHalfView: ConnectView = {
  ...nativeView,
  levels: halfLevels,
  level: { monitor: "receiving", reading: { rms_dbfs: -24.6, peak_dbfs: -13.9, spl_db: 76.8, clipping: false }, playing: "Rs", playingAll: true, masterDbfs: -20 },
};

/** Every channel has a level. */
export const levelDoneView: ConnectView = {
  ...nativeView,
  levels: allLevels,
  level: { ...nativeView.level, reading: { rms_dbfs: -60.9, peak_dbfs: -11.2, spl_db: 37.1, clipping: false } },
};

/** Every channel in the recommended range: the check passed, the header goes on to measuring. */
export const levelPassedView: ConnectView = {
  ...levelDoneView,
  levels: { ...allLevels, Tfl: -27.4, Tbr: -18.2 },
};

/** The sample room: no devices to choose, some channels checked. */
export const mockView: ConnectView = {
  ...base,
  path: "mock",
  settings: settingsFor("mock"),
  levels: { L: -22.1, R: -22.7, C: -23.4 },
  level: { ...base.level, monitor: "receiving", reading: { rms_dbfs: -64.2, peak_dbfs: -55.0, spl_db: null, clipping: false } },
};

/** mdat import only: the step is skipped. */
export const mdatView: ConnectView = { ...base, path: "mdat" };
